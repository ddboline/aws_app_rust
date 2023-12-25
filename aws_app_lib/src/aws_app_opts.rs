use anyhow::{format_err, Error};
use aws_sdk_route53::types::RrType;
use clap::Parser;
use futures::{future, stream::FuturesUnordered, TryStreamExt};
use itertools::Itertools;
use log::debug;
use refinery::embed_migrations;
use stack_string::{format_sstr, StackString};
use std::{net::Ipv4Addr, path::PathBuf, sync::Arc};
use tokio::io::{stdin, AsyncReadExt};

use crate::{
    aws_app_interface::AwsAppInterface,
    config::Config,
    instance_opt::InstanceOpt,
    models::{InstanceFamily, InstanceList},
    novnc_instance::NoVncInstance,
    pgpool::PgPool,
    resource_type::{ResourceType, ALL_RESOURCES},
    spot_request_opt::{get_tags, SpotRequestOpt},
    sysinfo_instance::SysinfoInstance,
    systemd_instance::SystemdInstance,
};

embed_migrations!("../migrations");

#[derive(Parser, Debug, Clone)]
pub enum AwsAppOpts {
    /// Update metadata
    Update,
    /// List information about resources
    List {
        #[clap(short, use_value_delimiter = true, value_delimiter = ',')]
        /// Possible values are:
        /// -r instances,reserved,spot,ami,volume,snapshot,ecr,key,script,user,
        /// group,access-key,route53,systemd
        resources: Vec<ResourceType>,
        #[clap(short, long)]
        /// List all regions
        all_regions: bool,
    },
    /// Terminate a running ec2 instance
    Terminate {
        #[clap(short, long, use_value_delimiter = true, value_delimiter = ',')]
        /// Instance IDs
        instance_ids: Vec<StackString>,
    },
    /// Request a new spot instance
    Request(SpotRequestOpt),
    /// Cancel Spot Request
    CancelRequest {
        #[clap(short, long, use_value_delimiter = true, value_delimiter = ',')]
        instance_ids: Vec<StackString>,
    },
    /// Run a new ec2 instance
    Run(InstanceOpt),
    /// Get On-demand/Reserved and Spot instance pricing
    Price {
        #[clap(short, long, use_value_delimiter = true, value_delimiter = ',')]
        search: Vec<StackString>,
    },
    /// List Instance Families
    ListFamilies,
    /// List Instance Types
    ListInstances {
        #[clap(short, long, use_value_delimiter = true, value_delimiter = ',')]
        search: Vec<StackString>,
    },
    /// Create an ami image from a running ec2 instance
    CreateImage {
        #[clap(short, long)]
        /// Instance ID
        instance_id: StackString,
        #[clap(short, long)]
        /// Name for new AMI image
        name: StackString,
    },
    /// Delete ami image
    DeleteImage {
        #[clap(short, long)]
        /// AMI ID
        ami: StackString,
    },
    /// Create new EBS Volume
    CreateVolume {
        #[clap(short = 's', long)]
        size: Option<i32>,
        #[clap(short, long)]
        zoneid: StackString,
        #[clap(long)]
        snapid: Option<StackString>,
    },
    /// Create new User
    CreateUser {
        #[clap(short, long)]
        user_name: StackString,
    },
    /// Delete EBS Volume
    DeleteVolume {
        volid: StackString,
    },
    /// Attach EBS Volume
    AttachVolume {
        #[clap(long)]
        volid: StackString,
        #[clap(short, long)]
        instance_id: StackString,
        #[clap(short, long)]
        device_id: StackString,
    },
    /// Detach EBS Volume
    DetachVolume {
        #[clap(long)]
        volid: StackString,
    },
    /// Modify EBS Volume
    ModifyVolume {
        #[clap(long)]
        volid: StackString,
        #[clap(short, long)]
        size: i32,
    },
    /// Create EBS Snapshot
    CreateSnapshot {
        #[clap(long)]
        volid: StackString,
        #[clap(
            short,
            long,
            long = "tag",
            use_value_delimiter = true,
            value_delimiter = ','
        )]
        tags: Vec<StackString>,
    },
    /// Delete Snapshot
    DeleteSnapshot {
        #[clap(long)]
        snapid: StackString,
    },
    /// Tag Resource
    Tag {
        #[clap(short, long)]
        id: StackString,
        #[clap(
            short,
            long,
            long = "tag",
            use_value_delimiter = true,
            value_delimiter = ','
        )]
        tags: Vec<StackString>,
    },
    /// Delete ECR Images
    DeleteEcrImages {
        #[clap(short, long)]
        reponame: StackString,
        #[clap(short, long, use_value_delimiter = true, value_delimiter = ',')]
        imageids: Vec<StackString>,
    },
    /// Cleanup ECR Images
    CleanupEcrImages,
    /// Print ssh command to connect to instance
    Connect {
        #[clap(short, long)]
        /// Instance ID
        instance_id: StackString,
    },
    Status {
        #[clap(short, long)]
        /// Instance ID
        instance_id: StackString,
    },
    UpdateDns {
        #[clap(short, long)]
        zone: StackString,
        #[clap(short, long)]
        dnsname: StackString,
        #[clap(short, long)]
        new_ip: Option<Ipv4Addr>,
    },
    UpdatePricing,
    Systemd {
        #[clap(short, long)]
        pattern: Option<StackString>,
    },
    Processes {
        #[clap(short, long)]
        pattern: Option<StackString>,
    },
    NoVnc {
        #[clap(short, long)]
        cert: Option<PathBuf>,
        #[clap(short, long)]
        key: Option<PathBuf>,
    },
    RunMigrations,
}

impl AwsAppOpts {
    /// # Errors
    /// Returns error if api call fails
    pub async fn process_args() -> Result<(), Error> {
        let opts = Self::parse();
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let sdk_config = aws_config::load_from_env().await;
        let app = AwsAppInterface::new(config, &sdk_config, pool);

        let result = match opts {
            Self::Update => {
                for line in app.update().await? {
                    app.stdout.send(line);
                }
                Ok(())
            }
            Self::List {
                resources,
                all_regions,
            } => {
                let resources = if resources.get(0) == Some(&ResourceType::All) {
                    ALL_RESOURCES.to_vec()
                } else {
                    resources
                };
                let resources = Arc::new(resources);
                if all_regions {
                    let futures: FuturesUnordered<_> = app
                        .ec2
                        .get_all_regions()
                        .await?
                        .into_keys()
                        .map(|region| {
                            let mut app_ = app.clone();
                            let resources = resources.clone();
                            async move {
                                app_.set_region(&region).await?;
                                app_.list(resources.iter()).await
                            }
                        })
                        .collect();
                    futures.try_collect().await
                } else {
                    app.list(resources.iter()).await
                }
            }
            Self::Terminate { instance_ids } => app.terminate(&instance_ids).await,
            Self::Request(req) => {
                app.request_spot_instance(&mut req.into_spot_request(&app.config)?)
                    .await
            }
            Self::CancelRequest { instance_ids } => {
                app.ec2.cancel_spot_instance_request(&instance_ids).await
            }
            Self::Run(req) => {
                app.run_ec2_instance(&mut req.into_instance_request(&app.config)?)
                    .await
            }
            Self::Price { search } => app.print_ec2_prices(&search).await,
            Self::ListFamilies => {
                let mut stream = Box::pin(InstanceFamily::get_all(&app.pool, None).await?);
                while let Some(fam) = stream.try_next().await? {
                    app.stdout
                        .send(format_sstr!("{:5} {}", fam.family_name, fam.family_type));
                }
                Ok(())
            }
            Self::ListInstances { search } => {
                let mut instances: Vec<_> = InstanceList::get_all_instances(&app.pool)
                    .await?
                    .try_filter(|inst| {
                        future::ready({
                            if search.is_empty() {
                                true
                            } else {
                                search
                                    .iter()
                                    .any(|s| inst.instance_type.contains(s.as_str()))
                            }
                        })
                    })
                    .try_collect()
                    .await?;
                instances.sort_by_key(|i| i.n_cpu);
                instances.sort_by(|x, y| {
                    let x = x.instance_type.split('.').next().unwrap_or("");
                    let y = y.instance_type.split('.').next().unwrap_or("");
                    x.cmp(y)
                });
                for inst in instances {
                    app.stdout.send(format_sstr!(
                        "{:18} cpu: {:3} mem: {:6.2} {}",
                        inst.instance_type,
                        inst.n_cpu,
                        inst.memory_gib,
                        inst.generation,
                    ));
                }
                Ok(())
            }
            Self::CreateImage { instance_id, name } => {
                if let Some(id) = app.create_image(instance_id, name).await? {
                    app.stdout.send(format_sstr!("New id {id}"));
                }
                Ok(())
            }
            Self::DeleteImage { ami } => app.delete_image(ami.as_ref()).await,
            Self::CreateVolume {
                size,
                zoneid,
                snapid,
            } => {
                app.stdout
                    .send(format_sstr!("{size:?} {zoneid} {snapid:?}"));
                if let Some(id) = app.create_ebs_volume(zoneid, size, snapid).await? {
                    app.stdout.send(format_sstr!("Created Volume {id}"));
                }
                Ok(())
            }
            Self::DeleteVolume { volid } => app.delete_ebs_volume(volid).await,
            Self::AttachVolume {
                volid,
                instance_id,
                device_id,
            } => app.attach_ebs_volume(volid, instance_id, device_id).await,
            Self::DetachVolume { volid } => app.detach_ebs_volume(volid).await,
            Self::ModifyVolume { volid, size } => app.modify_ebs_volume(volid, size).await,
            Self::CreateUser { user_name } => {
                if let Some(user) = app.create_user(user_name.as_str()).await? {
                    app.stdout.send(format_sstr!("{user:?}"));
                }
                Ok(())
            }
            Self::CreateSnapshot { volid, tags } => {
                if let Some(id) = app.create_ebs_snapshot(volid, &get_tags(&tags)).await? {
                    app.stdout.send(format_sstr!("Created snapshot {id}"));
                }
                Ok(())
            }
            Self::DeleteSnapshot { snapid } => app.delete_ebs_snapshot(snapid).await,
            Self::Tag { id, tags } => app.ec2.tag_ec2_instance(id, &get_tags(&tags)).await,
            Self::DeleteEcrImages { reponame, imageids } => {
                app.ecr.delete_ecr_images(reponame, &imageids).await
            }
            Self::CleanupEcrImages => app.ecr.cleanup_ecr_images().await,
            Self::Connect { instance_id } => app.connect(instance_id).await,
            Self::Status { instance_id } => {
                for line in app.get_status(instance_id).await? {
                    app.stdout.send(line);
                }
                Ok(())
            }
            Self::UpdateDns {
                zone,
                dnsname,
                new_ip,
            } => {
                let record_name = format_sstr!("{dnsname}.");
                let old_ip = app
                    .route53
                    .list_record_sets(&zone)
                    .await?
                    .into_iter()
                    .find_map(|record| {
                        if record.r#type == RrType::A && record.name == record_name.as_str() {
                            let ip: Ipv4Addr =
                                record.resource_records?.pop()?.value().parse().ok()?;
                            Some(ip)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| format_err!("No IP"))?;
                let current_ip = app.route53.get_ip_address().await?;
                let new_ip = new_ip.unwrap_or(current_ip);
                app.route53
                    .update_dns_record(&zone, &record_name, old_ip, new_ip)
                    .await
            }
            Self::UpdatePricing => {
                let number_of_updates = app.pricing.update_all_prices(&app.pool).await?;
                app.stdout.send(format_sstr!("{number_of_updates} updates"));
                Ok(())
            }
            Self::Systemd { pattern } => {
                let systemd = SystemdInstance::new(&app.config.systemd_services);
                if let Some(pattern) = &pattern {
                    let stat = systemd.get_service_status(pattern).await?;
                    let log = systemd
                        .get_service_logs(pattern)
                        .await?
                        .into_iter()
                        .join("\n");
                    app.stdout.send(format_sstr!("{stat}"));
                    app.stdout.send(log);
                } else {
                    let services = systemd.list_running_services().await?;
                    for service in &app.config.systemd_services {
                        if let Some(val) = services.get(service) {
                            app.stdout.send(format_sstr!("{service} {val}"));
                        } else {
                            app.stdout.send(format_sstr!("{service} not running"));
                        }
                    }
                }
                Ok(())
            }
            Self::Processes { pattern } => {
                let sysinfo = if let Some(name) = pattern {
                    SysinfoInstance::new(&[name])
                } else {
                    SysinfoInstance::new(&app.config.systemd_services)
                };
                let procs = sysinfo.get_process_info();
                for proc in procs {
                    println!(
                        "{name:15}\t{pid}\t{memory:0.1} MiB",
                        name = proc.name,
                        pid = proc.pid,
                        memory = proc.memory as f64 / 1e6
                    );
                }
                Ok(())
            }
            Self::NoVnc { cert, key } => {
                let novnc_path = app
                    .config
                    .novnc_path
                    .as_ref()
                    .ok_or_else(|| format_err!("novnc_path not set"))?;
                let cert = cert.ok_or_else(|| format_err!("No cert"))?;
                let key = key.ok_or_else(|| format_err!("No key"))?;
                let novnc = NoVncInstance::new();
                novnc.novnc_start(novnc_path, &cert, &key).await?;
                app.stdout.send("Press any key");
                let mut buf = [0u8; 8];
                let written = stdin().read(&mut buf).await?;
                debug!("written {written}");
                let lines = novnc.novnc_stop_request().await?;
                app.stdout.send(lines.join("\n"));
                Ok(())
            }
            Self::RunMigrations => {
                let mut client = app.pool.get().await?;
                migrations::runner().run_async(&mut **client).await?;
                Ok(())
            }
        };
        result?;
        app.stdout.close().await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use crate::aws_app_opts::get_tags;

    #[test]
    fn test_get_tags() -> Result<(), Error> {
        let tags0 = &["spartacus", "Types:Nothing", "LastName:NoWhere"];
        let tags = get_tags(tags0);
        println!("{:?}", tags);
        assert_eq!(tags.get("Name").map(Into::into), Some("spartacus"));
        assert_eq!(tags.get("Types").map(Into::into), Some("Nothing"));
        assert_eq!(tags.get("LastName").map(Into::into), Some("NoWhere"));
        Ok(())
    }
}

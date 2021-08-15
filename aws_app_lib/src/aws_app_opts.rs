use anyhow::{format_err, Error};
use futures::future::try_join_all;
use itertools::Itertools;
use stack_string::StackString;
use std::{net::Ipv4Addr, string::ToString, sync::Arc};
use structopt::StructOpt;

use crate::{
    aws_app_interface::AwsAppInterface,
    config::Config,
    instance_opt::InstanceOpt,
    models::{InstanceFamily, InstanceList},
    pgpool::PgPool,
    resource_type::ResourceType,
    spot_request_opt::{get_tags, SpotRequestOpt},
    systemd_instance::SystemdInstance,
};

#[derive(StructOpt, Debug, Clone)]
pub enum AwsAppOpts {
    /// Update metadata
    Update,
    /// List information about resources
    List {
        #[structopt(short)]
        /// Possible values are: reserved spot ami volume snapshot ecr key
        resources: Vec<ResourceType>,
        #[structopt(short, long)]
        /// List all regions
        all_regions: bool,
    },
    /// Terminate a running ec2 instance
    Terminate {
        #[structopt(short, long)]
        /// Instance IDs
        instance_ids: Vec<StackString>,
    },
    /// Request a new spot instance
    Request(SpotRequestOpt),
    /// Cancel Spot Request
    CancelRequest {
        #[structopt(short, long)]
        instance_ids: Vec<StackString>,
    },
    /// Run a new ec2 instance
    Run(InstanceOpt),
    /// Get On-demand/Reserved and Spot instance pricing
    Price {
        #[structopt(short, long)]
        search: Vec<StackString>,
    },
    /// List Instance Families
    ListFamilies,
    /// List Instance Types
    ListInstances {
        #[structopt(short, long)]
        search: Vec<StackString>,
    },
    /// Create an ami image from a running ec2 instance
    CreateImage {
        #[structopt(short, long)]
        /// Instance ID
        instance_id: StackString,
        #[structopt(short, long)]
        /// Name for new AMI image
        name: StackString,
    },
    /// Delete ami image
    DeleteImage {
        #[structopt(short, long)]
        /// AMI ID
        ami: StackString,
    },
    /// Create new EBS Volume
    CreateVolume {
        #[structopt(short = "s", long)]
        size: Option<i64>,
        #[structopt(short, long)]
        zoneid: StackString,
        #[structopt(long)]
        snapid: Option<StackString>,
    },
    /// Create new User
    CreateUser {
        #[structopt(short, long)]
        user_name: StackString,
    },
    /// Delete EBS Volume
    DeleteVolume {
        volid: StackString,
    },
    /// Attach EBS Volume
    AttachVolume {
        #[structopt(long)]
        volid: StackString,
        #[structopt(short, long)]
        instance_id: StackString,
        #[structopt(short, long)]
        device_id: StackString,
    },
    /// Detach EBS Volume
    DetachVolume {
        #[structopt(long)]
        volid: StackString,
    },
    /// Modify EBS Volume
    ModifyVolume {
        #[structopt(long)]
        volid: StackString,
        #[structopt(short, long)]
        size: i64,
    },
    /// Create EBS Snapshot
    CreateSnapshot {
        #[structopt(long)]
        volid: StackString,
        #[structopt(short, long, long = "tag")]
        tags: Vec<StackString>,
    },
    /// Delete Snapshot
    DeleteSnapshot {
        #[structopt(long)]
        snapid: StackString,
    },
    /// Tag Resource
    Tag {
        #[structopt(short, long)]
        id: StackString,
        #[structopt(short, long, long = "tag")]
        tags: Vec<StackString>,
    },
    /// Delete ECR Images
    DeleteEcrImages {
        #[structopt(short, long)]
        reponame: StackString,
        #[structopt(short, long)]
        imageids: Vec<StackString>,
    },
    /// Cleanup ECR Images
    CleanupEcrImages,
    /// Print ssh command to connect to instance
    Connect {
        #[structopt(short, long)]
        /// Instance ID
        instance_id: StackString,
    },
    Status {
        #[structopt(short, long)]
        /// Instance ID
        instance_id: StackString,
    },
    UpdateDns {
        #[structopt(short, long)]
        zone: StackString,
        #[structopt(short, long)]
        dnsname: StackString,
        #[structopt(short, long)]
        new_ip: Option<Ipv4Addr>,
    },
    UpdatePricing,
    Systemd {
        #[structopt(short, long)]
        pattern: Option<StackString>,
    },
}

impl AwsAppOpts {
    pub async fn process_args() -> Result<(), Error> {
        let opts = Self::from_args();
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let app = AwsAppInterface::new(config, pool);

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
                let resources = Arc::new(resources);
                if all_regions {
                    let futures = app
                    .ec2
                    .get_all_regions()
                    .await?
                    .into_iter().map(|(region, _)| {
                        let mut app_ = app.clone();
                        let resources = resources.clone();
                        async move {
                            app_.set_region(&region)?;
                            app_.list(resources.iter()).await
                        }
                    });
                    try_join_all(futures).await?;
                    Ok(())
                } else {
                    app.list(resources.iter()).await
                }
            }
            Self::Terminate { instance_ids } => app.terminate(&instance_ids).await,
            Self::Request(req) => {
                app.request_spot_instance(&mut req.into_spot_request(&app.config))
                    .await
            }
            Self::CancelRequest { instance_ids } => {
                app.ec2.cancel_spot_instance_request(&instance_ids).await
            }
            Self::Run(req) => {
                app.run_ec2_instance(&mut req.into_instance_request(&app.config))
                    .await
            }
            Self::Price { search } => app.print_ec2_prices(&search).await,
            Self::ListFamilies => {
                for fam in InstanceFamily::get_all(&app.pool).await? {
                    app.stdout
                        .send(format!("{:5} {}", fam.family_name, fam.family_type));
                }
                Ok(())
            }
            Self::ListInstances { search } => {
                let mut instances: Vec<_> = InstanceList::get_all_instances(&app.pool)
                    .await?
                    .into_iter()
                    .filter(|inst| {
                        if search.is_empty() {
                            true
                        } else {
                            search
                                .iter()
                                .any(|s| inst.instance_type.contains(s.as_str()))
                        }
                    })
                    .collect();
                instances.sort_by_key(|i| i.n_cpu);
                instances.sort_by(|x, y| {
                    let x = x.instance_type.split('.').next().unwrap_or("");
                    let y = y.instance_type.split('.').next().unwrap_or("");
                    x.cmp(y)
                });
                for inst in instances {
                    app.stdout.send(format!(
                        "{:18} cpu: {:3} mem: {:6.2} {}",
                        inst.instance_type, inst.n_cpu, inst.memory_gib, inst.generation,
                    ));
                }
                Ok(())
            }
            Self::CreateImage { instance_id, name } => {
                if let Some(id) = app
                    .create_image(instance_id.as_ref(), name.as_ref())
                    .await?
                {
                    app.stdout.send(format!("New id {}", id));
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
                    .send(format!("{:?} {} {:?}", size, zoneid, snapid));
                if let Some(id) = app.create_ebs_volume(zoneid.as_ref(), size, snapid).await? {
                    app.stdout.send(format!("Created Volume {}", id));
                }
                Ok(())
            }
            Self::DeleteVolume { volid } => app.delete_ebs_volume(volid.as_ref()).await,
            Self::AttachVolume {
                volid,
                instance_id,
                device_id,
            } => {
                app.attach_ebs_volume(volid.as_ref(), instance_id.as_ref(), device_id.as_ref())
                    .await
            }
            Self::DetachVolume { volid } => app.detach_ebs_volume(volid.as_ref()).await,
            Self::ModifyVolume { volid, size } => app.modify_ebs_volume(volid.as_ref(), size).await,
            Self::CreateUser { user_name } => {
                if let Some(user) = app.create_user(user_name.as_str()).await? {
                    app.stdout.send(format!("{:?}", user));
                }
                Ok(())
            }
            Self::CreateSnapshot { volid, tags } => {
                if let Some(id) = app
                    .create_ebs_snapshot(volid.as_ref(), &get_tags(&tags))
                    .await?
                {
                    app.stdout.send(format!("Created snapshot {}", id));
                }
                Ok(())
            }
            Self::DeleteSnapshot { snapid } => app.delete_ebs_snapshot(snapid.as_ref()).await,
            Self::Tag { id, tags } => {
                app.ec2
                    .tag_ec2_instance(id.as_ref(), &get_tags(&tags))
                    .await
            }
            Self::DeleteEcrImages { reponame, imageids } => {
                app.ecr
                    .delete_ecr_images(reponame.as_ref(), &imageids)
                    .await
            }
            Self::CleanupEcrImages => app.ecr.cleanup_ecr_images().await,
            Self::Connect { instance_id } => app.connect(instance_id.as_ref()).await,
            Self::Status { instance_id } => {
                for line in app.get_status(instance_id.as_ref()).await? {
                    app.stdout.send(line);
                }
                Ok(())
            }
            Self::UpdateDns {
                zone,
                dnsname,
                new_ip,
            } => {
                let record_name = format!("{}.", dnsname);
                let old_ip = app
                    .route53
                    .list_record_sets(&zone)
                    .await?
                    .into_iter()
                    .find_map(|record| {
                        if record.type_ == "A" && record.name == record_name {
                            let ip: Ipv4Addr =
                                record.resource_records?.pop()?.value.parse().ok()?;
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
                app.stdout.send(format!("{} updates", number_of_updates));
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
                    app.stdout.send(format!("{}", stat));
                    app.stdout.send(log);
                } else {
                    let services = systemd.list_running_services().await?;
                    for service in &app.config.systemd_services {
                        if let Some(val) = services.get(service) {
                            app.stdout.send(format!("{} {}", service, val));
                        } else {
                            app.stdout.send(format!("{} not running", service));
                        }
                    }
                }
                Ok(())
            }
        };
        result?;
        app.stdout.close().await
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

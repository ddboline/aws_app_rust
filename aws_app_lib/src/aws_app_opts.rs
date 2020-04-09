use anyhow::Error;
use futures::future::try_join_all;
use std::{collections::HashMap, string::ToString, sync::Arc};
use structopt::StructOpt;

use crate::{
    aws_app_interface::AwsAppInterface,
    config::Config,
    ec2_instance::{InstanceRequest, SpotRequest},
    models::{InstanceFamily, InstanceList},
    pgpool::PgPool,
    resource_type::ResourceType,
    stack_string::StackString,
};

#[derive(Debug, Clone, StructOpt)]
pub struct SpotRequestOpt {
    #[structopt(short, long)]
    ami: StackString,
    #[structopt(short, long)]
    instance_type: StackString,
    #[structopt(long)]
    security_group: Option<StackString>,
    #[structopt(short, long)]
    script: Option<StackString>,
    #[structopt(long)]
    price: Option<f32>,
    #[structopt(short, long, long = "tag")]
    tags: Vec<StackString>,
    #[structopt(short, long)]
    key_name: Option<StackString>,
}

fn get_tags<T: AsRef<str>>(tags: &[T]) -> HashMap<StackString, StackString> {
    tags.iter()
        .map(|tag| {
            if tag.as_ref().contains(':') {
                let t: Vec<_> = tag.as_ref().split(':').collect();
                if t.len() > 1 {
                    (t[0].into(), t[1].into())
                } else {
                    (t[0].into(), "".into())
                }
            } else {
                ("Name".into(), tag.as_ref().into())
            }
        })
        .collect()
}

impl SpotRequestOpt {
    pub fn into_spot_request(self, config: &Config) -> SpotRequest {
        SpotRequest {
            ami: self.ami,
            instance_type: self.instance_type,
            security_group: self
                .security_group
                .unwrap_or_else(|| config.spot_security_group.clone().into()),
            script: self.script.unwrap_or_else(|| "setup_aws.sh".into()),
            key_name: self
                .key_name
                .unwrap_or_else(|| config.default_key_name.clone().into()),
            price: self.price,
            tags: get_tags(&self.tags),
        }
    }
}

#[derive(StructOpt, Debug, Clone)]
pub struct InstanceOpt {
    #[structopt(short, long)]
    ami: StackString,
    #[structopt(short, long)]
    instance_type: StackString,
    #[structopt(long)]
    security_group: Option<StackString>,
    #[structopt(short, long)]
    script: Option<StackString>,
    #[structopt(short, long, long = "tag")]
    tags: Vec<StackString>,
    #[structopt(short, long)]
    key_name: Option<StackString>,
}

impl InstanceOpt {
    pub fn into_instance_request(self, config: &Config) -> InstanceRequest {
        InstanceRequest {
            ami: self.ami,
            instance_type: self.instance_type,
            security_group: self
                .security_group
                .unwrap_or_else(|| config.default_security_group.clone().into()),
            script: self.script.unwrap_or_else(|| "setup_aws.sh".into()),
            key_name: self
                .key_name
                .unwrap_or_else(|| config.default_key_name.clone().into()),
            tags: get_tags(&self.tags),
        }
    }
}

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
    /// Delete EBS Volume
    DeleteVolume { volid: StackString },
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
}

impl AwsAppOpts {
    pub async fn process_args() -> Result<(), Error> {
        let opts = Self::from_args();
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let app = AwsAppInterface::new(config, pool);

        let task = app.stdout.spawn_stdout_task();

        let result = match opts {
            Self::Update => {
                for line in app.update().await? {
                    app.stdout.send(line)?;
                }
                Ok(())
            }
            Self::List {
                resources,
                all_regions,
            } => {
                let resources = Arc::new(resources);
                if all_regions {
                    let regions: Vec<_> = app
                        .ec2
                        .get_all_regions()
                        .await?
                        .into_iter()
                        .map(|(k, _)| k)
                        .collect();

                    let futures = regions.into_iter().map(|region| {
                        let mut app_ = app.clone();
                        let resources = resources.clone();
                        async move {
                            app_.set_region(&region)?;
                            app_.list(&resources).await
                        }
                    });
                    try_join_all(futures).await?;
                    Ok(())
                } else {
                    app.list(&resources).await
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
                        .send(format!("{:5} {}", fam.family_name, fam.family_type).into())?;
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
                                .any(|s| inst.instance_type.as_ref().contains(s.as_ref()))
                        }
                    })
                    .collect();
                instances.sort_by_key(|i| i.n_cpu);
                instances.sort_by(|x, y| {
                    let x = x.instance_type.as_ref().split('.').next().unwrap_or("");
                    let y = y.instance_type.as_ref().split('.').next().unwrap_or("");
                    x.cmp(&y)
                });
                for inst in instances {
                    app.stdout.send(
                        format!(
                            "{:18} cpu: {:3} mem: {:6.2} {}",
                            inst.instance_type, inst.n_cpu, inst.memory_gib, inst.generation,
                        )
                        .into(),
                    )?;
                }
                Ok(())
            }
            Self::CreateImage { instance_id, name } => {
                if let Some(id) = app.create_image(instance_id.as_ref(), name.as_ref()).await? {
                    app.stdout.send(format!("New id {}", id).into())?;
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
                    .send(format!("{:?} {} {:?}", size, zoneid, snapid).into())?;
                if let Some(id) = app.create_ebs_volume(zoneid.as_ref(), size, snapid).await? {
                    app.stdout.send(format!("Created Volume {}", id).into())?;
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
            Self::CreateSnapshot { volid, tags } => {
                if let Some(id) = app.create_ebs_snapshot(volid.as_ref(), &get_tags(&tags)).await? {
                    app.stdout.send(format!("Created snapshot {}", id).into())?;
                }
                Ok(())
            }
            Self::DeleteSnapshot { snapid } => app.delete_ebs_snapshot(snapid.as_ref()).await,
            Self::Tag { id, tags } => app.ec2.tag_ec2_instance(id.as_ref(), &get_tags(&tags)).await,
            Self::DeleteEcrImages { reponame, imageids } => {
                app.ecr.delete_ecr_images(reponame.as_ref(), &imageids).await
            }
            Self::CleanupEcrImages => app.ecr.cleanup_ecr_images().await,
            Self::Connect { instance_id } => app.connect(instance_id.as_ref()).await,
            Self::Status { instance_id } => {
                for line in app.get_status(instance_id.as_ref()).await? {
                    app.stdout.send(line.into())?;
                }
                Ok(())
            }
        };
        result?;
        app.stdout.close().await?;
        task.await?
    }
}

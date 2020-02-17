use anyhow::Error;
use futures::future::try_join_all;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use std::collections::HashMap;
use std::io::{stdout, Write};
use std::string::ToString;
use structopt::StructOpt;
use std::sync::Arc;

use crate::aws_app_interface::AwsAppInterface;
use crate::config::Config;
use crate::ec2_instance::{InstanceRequest, SpotRequest};
use crate::models::{InstanceFamily, InstanceList};
use crate::pgpool::PgPool;
use crate::resource_type::ResourceType;

#[derive(Debug, Clone, StructOpt)]
pub struct SpotRequestOpt {
    #[structopt(short, long)]
    ami: String,
    #[structopt(short, long)]
    instance_type: String,
    #[structopt(long)]
    security_group: Option<String>,
    #[structopt(short, long)]
    script: Option<String>,
    #[structopt(long)]
    price: Option<f32>,
    #[structopt(short, long, long = "tag")]
    tags: Vec<String>,
    #[structopt(short, long)]
    key_name: Option<String>,
}

fn get_tags(tags: &[String]) -> HashMap<String, String> {
    tags.par_iter()
        .map(|tag| {
            if tag.contains(':') {
                let t: Vec<_> = tag.split(':').collect();
                if t.len() > 1 {
                    (t[0].to_string(), t[1].to_string())
                } else {
                    (t[0].to_string(), "".to_string())
                }
            } else {
                ("Name".to_string(), tag.to_string())
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
                .unwrap_or_else(|| config.spot_security_group.clone()),
            script: self.script.unwrap_or_else(|| "setup_aws.sh".to_string()),
            key_name: self
                .key_name
                .unwrap_or_else(|| config.default_key_name.to_string()),
            price: self.price.unwrap_or(config.max_spot_price),
            tags: get_tags(&self.tags),
        }
    }
}

#[derive(StructOpt, Debug, Clone)]
pub struct InstanceOpt {
    #[structopt(short, long)]
    ami: String,
    #[structopt(short, long)]
    instance_type: String,
    #[structopt(long)]
    security_group: Option<String>,
    #[structopt(short, long)]
    script: Option<String>,
    #[structopt(short, long, long = "tag")]
    tags: Vec<String>,
    #[structopt(short, long)]
    key_name: Option<String>,
}

impl InstanceOpt {
    pub fn into_instance_request(self, config: &Config) -> InstanceRequest {
        InstanceRequest {
            ami: self.ami,
            instance_type: self.instance_type,
            security_group: self
                .security_group
                .unwrap_or_else(|| config.default_security_group.clone()),
            script: self.script.unwrap_or_else(|| "setup_aws.sh".to_string()),
            key_name: self
                .key_name
                .unwrap_or_else(|| config.default_key_name.to_string()),
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
        instance_ids: Vec<String>,
    },
    /// Request a new spot instance
    Request(SpotRequestOpt),
    /// Cancel Spot Request
    CancelRequest {
        #[structopt(short, long)]
        instance_ids: Vec<String>,
    },
    /// Run a new ec2 instance
    Run(InstanceOpt),
    /// Get On-demand/Reserved and Spot instance pricing
    Price {
        #[structopt(short, long)]
        search: Vec<String>,
    },
    /// List Instance Families
    ListFamilies,
    /// List Instance Types
    ListInstances {
        #[structopt(short, long)]
        search: Vec<String>,
    },
    /// Create an ami image from a running ec2 instance
    CreateImage {
        #[structopt(short, long)]
        /// Instance ID
        instance_id: String,
        #[structopt(short, long)]
        /// Name for new AMI image
        name: String,
    },
    /// Delete ami image
    DeleteImage {
        #[structopt(short, long)]
        /// AMI ID
        ami: String,
    },
    /// Create new EBS Volume
    CreateVolume {
        #[structopt(short = "s", long)]
        size: Option<i64>,
        #[structopt(short, long)]
        zoneid: String,
        #[structopt(long)]
        snapid: Option<String>,
    },
    /// Delete EBS Volume
    DeleteVolume { volid: String },
    /// Attach EBS Volume
    AttachVolume {
        #[structopt(long)]
        volid: String,
        #[structopt(short, long)]
        instance_id: String,
        #[structopt(short, long)]
        device_id: String,
    },
    /// Detach EBS Volume
    DetachVolume {
        #[structopt(long)]
        volid: String,
    },
    /// Modify EBS Volume
    ModifyVolume {
        #[structopt(long)]
        volid: String,
        #[structopt(short, long)]
        size: i64,
    },
    /// Create EBS Snapshot
    CreateSnapshot {
        #[structopt(long)]
        volid: String,
        #[structopt(short, long, long = "tag")]
        tags: Vec<String>,
    },
    /// Delete Snapshot
    DeleteSnapshot {
        #[structopt(long)]
        snapid: String,
    },
    /// Tag Resource
    Tag {
        #[structopt(short, long)]
        id: String,
        #[structopt(short, long, long = "tag")]
        tags: Vec<String>,
    },
    /// Delete ECR Images
    DeleteEcrImages {
        #[structopt(short, long)]
        reponame: String,
        #[structopt(short, long)]
        imageids: Vec<String>,
    },
    /// Cleanup ECR Images
    CleanupEcrImages,
    /// Print ssh command to connect to instance
    Connect {
        #[structopt(short, long)]
        /// Instance ID
        instance_id: String,
    },
    Status {
        #[structopt(short, long)]
        /// Instance ID
        instance_id: String,
    },
}

impl AwsAppOpts {
    pub async fn process_args() -> Result<(), Error> {
        let opts = Self::from_args();
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let app = AwsAppInterface::new(config, pool);

        match opts {
            Self::Update => {
                for line in app.update().await? {
                    writeln!(stdout(), "{}", line)?;
                }
                Ok(())
            }
            Self::List {
                resources,
                all_regions,
            } => {
                let resources = Arc::new(resources);
                if all_regions {
                    let regions: Vec<_> =
                        app.ec2.get_all_regions().await?.keys().cloned().collect();

                    let futures: Vec<_> = regions
                        .into_iter()
                        .map(|region| {
                            let mut app_ = app.clone();
                            let region = region.clone();
                            let resources = Arc::clone(&resources);
                            async move {
                                app_.set_region(&region)?;
                                app_.list(&resources).await
                            }
                        })
                        .collect();
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
                    writeln!(stdout(), "{:5} {}", fam.family_name, fam.family_type)?;
                }
                Ok(())
            }
            Self::ListInstances { search } => {
                let mut instances: Vec<_> = InstanceList::get_all_instances(&app.pool)
                    .await?
                    .into_par_iter()
                    .filter(|inst| {
                        if search.is_empty() {
                            true
                        } else {
                            search.par_iter().any(|s| inst.instance_type.contains(s))
                        }
                    })
                    .collect();
                instances.par_sort_by_key(|i| i.n_cpu);
                instances.par_sort_by_key(|i| {
                    i.instance_type
                        .split('.')
                        .nth(0)
                        .unwrap_or_else(|| "")
                        .to_string()
                });
                for inst in instances {
                    writeln!(
                        stdout(),
                        "{:18} cpu: {:3} mem: {:6.2} {}",
                        inst.instance_type,
                        inst.n_cpu,
                        inst.memory_gib,
                        inst.generation,
                    )?;
                }
                Ok(())
            }
            Self::CreateImage { instance_id, name } => {
                if let Some(id) = app.create_image(&instance_id, &name).await? {
                    writeln!(stdout(), "New id {}", id)?;
                }
                Ok(())
            }
            Self::DeleteImage { ami } => app.delete_image(&ami).await,
            Self::CreateVolume {
                size,
                zoneid,
                snapid,
            } => {
                writeln!(stdout(), "{:?} {} {:?}", size, zoneid, snapid)?;
                if let Some(id) = app.create_ebs_volume(&zoneid, size, snapid).await? {
                    writeln!(stdout(), "Created Volume {}", id)?;
                }
                Ok(())
            }
            Self::DeleteVolume { volid } => app.delete_ebs_volume(&volid).await,
            Self::AttachVolume {
                volid,
                instance_id,
                device_id,
            } => {
                app.attach_ebs_volume(&volid, &instance_id, &device_id)
                    .await
            }
            Self::DetachVolume { volid } => app.detach_ebs_volume(&volid).await,
            Self::ModifyVolume { volid, size } => app.modify_ebs_volume(&volid, size).await,
            Self::CreateSnapshot { volid, tags } => {
                if let Some(id) = app.create_ebs_snapshot(&volid, &get_tags(&tags)).await? {
                    writeln!(stdout(), "Created snapshot {}", id)?;
                }
                Ok(())
            }
            Self::DeleteSnapshot { snapid } => app.delete_ebs_snapshot(&snapid).await,
            Self::Tag { id, tags } => app.ec2.tag_ec2_instance(&id, &get_tags(&tags)).await,
            Self::DeleteEcrImages { reponame, imageids } => {
                app.ecr.delete_ecr_images(&reponame, &imageids).await
            }
            Self::CleanupEcrImages => app.ecr.cleanup_ecr_images().await,
            Self::Connect { instance_id } => app.connect(&instance_id).await,
            Self::Status { instance_id } => {
                for line in app.get_status(&instance_id).await? {
                    writeln!(stdout(), "{}", line)?;
                }
                Ok(())
            }
        }
    }
}

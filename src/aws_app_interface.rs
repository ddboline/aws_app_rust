use failure::Error;

use crate::models::{AwsGeneration, PricingType};
use crate::scrape_instance_info::scrape_instance_info;
use crate::scrape_pricing_info::scrape_pricing_info;

use crate::config::Config;
use crate::ec2_instance::Ec2Instance;
use crate::ecr_instance::EcrInstance;
use crate::pgpool::PgPool;
use crate::resource_type::ResourceType;

#[derive(Clone)]
pub struct AwsAppInterface {
    pub config: Config,
    pub pool: PgPool,
    pub ec2: Ec2Instance,
    pub ecr: EcrInstance,
}

impl AwsAppInterface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self {
            ec2: Ec2Instance::new(config.clone()),
            ecr: EcrInstance::new(config.clone()),
            config,
            pool,
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.ec2.set_region(region)?;
        self.ecr.set_region(region)
    }

    pub fn update(&self) -> Result<(), Error> {
        scrape_instance_info(AwsGeneration::HVM, &self.pool)?;
        scrape_instance_info(AwsGeneration::PV, &self.pool)?;

        scrape_pricing_info(PricingType::Reserved, &self.pool)?;
        scrape_pricing_info(PricingType::OnDemand, &self.pool)
    }

    pub fn list(&self, resources: &[ResourceType]) -> Result<(), Error> {
        let instances = self.ec2.get_all_instances()?;

        if !instances.is_empty() {
            println!("instances:");
            for inst in &instances {
                let name = inst
                    .tags
                    .get("Name")
                    .cloned()
                    .unwrap_or_else(|| "".to_string());
                println!(
                    "{} {} {} {} {} {} {}",
                    inst.id,
                    inst.dns_name,
                    inst.state,
                    name,
                    inst.instance_type,
                    inst.launch_time,
                    inst.availability_zone,
                );
            }
        }

        for resource in resources {
            match resource {
                ResourceType::Reserved => {
                    let reserved = self.ec2.get_reserved_instances()?;
                    println!("---\nGet Reserved Instance\n---");
                    for res in &reserved {
                        println!(
                            "{} {} {} {} {}",
                            res.id, res.price, res.instance_type, res.state, res.availability_zone
                        );
                    }
                }
                ResourceType::Spot => {
                    let requests = self.ec2.get_spot_instance_requests()?;
                    println!("---\nSpot Instance Requests:");
                    for req in &requests {
                        println!(
                            "{} {} {} {} {} {}",
                            req.id,
                            req.price,
                            req.imageid,
                            req.instance_type,
                            req.spot_type,
                            req.status
                        );
                    }
                }
                ResourceType::Ami => {
                    let ami_tags = self.ec2.get_ami_tags()?;
                    println!("---\nAMI's:");
                    for ami in &ami_tags {
                        println!(
                            "{} {} {} {}",
                            ami.id,
                            ami.name,
                            ami.state,
                            ami.snapshot_ids.join(" ")
                        );
                    }
                }
                ResourceType::Volume => {
                    let volumes = self.ec2.get_all_volumes()?;
                    println!("---\nVolumes:");
                    for vol in &volumes {
                        println!(
                            "{} {} {} {} {} {:?}",
                            vol.id, vol.availability_zone, vol.size, vol.iops, vol.state, vol.tags
                        );
                    }
                }
                ResourceType::Snapshot => {
                    let snapshots = self.ec2.get_all_snapshots()?;
                    for snap in &snapshots {
                        println!(
                            "{} {} GB {} {} {:?}",
                            snap.id, snap.volume_size, snap.state, snap.progress, snap.tags
                        );
                    }
                }
                ResourceType::Ecr => {
                    let repos = self.ecr.get_all_repositories()?;
                    println!("---\nECR images");
                    for repo in &repos {
                        let images = self.ecr.get_all_images(&repo)?;
                        for image in &images {
                            println!(
                                "{} {} {}",
                                repo,
                                image
                                    .tags
                                    .get(0)
                                    .map(|s| s.as_str())
                                    .unwrap_or_else(|| "None"),
                                image.digest
                            );
                        }
                    }
                }
            };
        }
        Ok(())
    }
}

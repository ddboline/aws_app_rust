use chrono::Local;
use failure::Error;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::ec2_instance::Ec2Instance;
use crate::ecr_instance::EcrInstance;
use crate::models::{AwsGeneration, InstanceFamily, InstanceList, InstancePricing, PricingType};
use crate::pgpool::PgPool;
use crate::resource_type::ResourceType;
use crate::scrape_instance_info::scrape_instance_info;
use crate::scrape_pricing_info::scrape_pricing_info;

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
        let mut instances = self.ec2.get_all_instances()?;

        if !instances.is_empty() {
            instances.sort_by_cached_key(|inst| inst.launch_time);
            instances.sort_by_cached_key(|inst| inst.state != "running");
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
                    inst.launch_time.with_timezone(&Local),
                    inst.availability_zone,
                );
            }
        }

        let mut visited_resources = HashSet::new();
        for resource in resources {
            if !visited_resources.insert(resource) {
                continue;
            }
            match resource {
                ResourceType::Reserved => {
                    let reserved = self.ec2.get_reserved_instances()?;
                    if reserved.is_empty() {
                        continue;
                    }
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
                    if requests.is_empty() {
                        continue;
                    }
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
                    if ami_tags.is_empty() {
                        continue;
                    }
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
                    if volumes.is_empty() {
                        continue;
                    }
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
                    if snapshots.is_empty() {
                        continue;
                    }
                    println!("---\nSnapshots:");
                    for snap in &snapshots {
                        println!(
                            "{} {} GB {} {} {:?}",
                            snap.id, snap.volume_size, snap.state, snap.progress, snap.tags
                        );
                    }
                }
                ResourceType::Ecr => {
                    let repos = self.ecr.get_all_repositories()?;
                    if repos.is_empty() {
                        continue;
                    }
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

    pub fn terminate(&self, instance_ids: &[String]) -> Result<(), Error> {
        let name_map: HashMap<String, String> = self
            .ec2
            .get_all_instances()?
            .into_iter()
            .filter_map(|inst| {
                if inst.state != "running" {
                    return None;
                }
                inst.tags
                    .get("Name")
                    .map(|name| (name.to_string(), inst.id.to_string()))
            })
            .collect();
        let mapped_inst_ids: Vec<String> = instance_ids
            .iter()
            .map(|id| {
                if let Some(id_) = name_map.get(id) {
                    id_.to_string()
                } else {
                    id.to_string()
                }
            })
            .collect();
        self.ec2.terminate_instance(&mapped_inst_ids)
    }

    pub fn get_ec2_prices(&self, search: &[String]) -> Result<(), Error> {
        let instance_families: HashMap<_, _> = InstanceFamily::get_all(&self.pool)?
            .into_iter()
            .map(|f| (f.family_name.to_string(), f))
            .collect();
        let instance_list: HashMap<_, _> = InstanceList::get_all_instances(&self.pool)?
            .into_iter()
            .map(|i| (i.instance_type.to_string(), i))
            .collect();
        let inst_list: Vec<_> = instance_list
            .keys()
            .map(|s| s.to_string())
            .filter(|inst| search.iter().any(|s| inst.contains(s)))
            .collect();

        let spot_prices = self.ec2.get_latest_spot_inst_prices(&inst_list)?;

        let prices: HashMap<_, _> = InstancePricing::get_all(&self.pool)?
            .into_iter()
            .map(|p| ((p.instance_type.to_string(), p.price_type.to_string()), p))
            .collect();

        let mut outstrings: Vec<_> = inst_list
            .into_par_iter()
            .map(|inst| {
                let ond_price = prices
                    .get(&(inst.to_string(), "ondemand".to_string()))
                    .map(|x| x.price);
                let res_price = prices
                    .get(&(inst.to_string(), "reserved".to_string()))
                    .map(|x| x.price);
                let spot_price = spot_prices.get(&inst);
                let instance_metadata = instance_list.get(&inst).unwrap();
                let inst_fam = inst.split(".").nth(0).unwrap();
                let instance_family = instance_families
                    .get(inst_fam)
                    .unwrap()
                    .family_type
                    .to_string();
                let mut outstr = Vec::new();
                outstr.push(format!("{:14} ", inst));
                match ond_price {
                    Some(p) => outstr.push(format!("ond: ${:0.4}/hr   ", p)),
                    None => outstr.push(format!("{:4} {:9}   ", "", "")),
                }
                match spot_price {
                    Some(p) => outstr.push(format!("spot: ${:0.4}/hr   ", p)),
                    None => outstr.push(format!("{:5} {:9}    ", "", "")),
                }
                match res_price {
                    Some(p) => outstr.push(format!("res: ${:0.4}/hr   ", p)),
                    None => outstr.push(format!("{:4} {:9}   ", "", "")),
                }
                outstr.push(format!("cpu: {:2}   ", instance_metadata.n_cpu));
                outstr.push(format!("inst_fam: {}", instance_family));

                (instance_metadata.n_cpu, outstr.join(""))
            })
            .collect();
        outstrings.sort();

        for (_, line) in &outstrings {
            println!("{}", line);
        }
        Ok(())
    }
}

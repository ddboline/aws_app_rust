use chrono::Local;
use failure::Error;
use lazy_static::lazy_static;
use parking_lot::RwLock;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::ec2_instance::{Ec2Instance, Ec2InstanceInfo, InstanceRequest, SpotRequest};
use crate::ecr_instance::EcrInstance;
use crate::models::{AwsGeneration, InstanceFamily, InstanceList, InstancePricing, PricingType};
use crate::pgpool::PgPool;
use crate::resource_type::ResourceType;
use crate::scrape_instance_info::scrape_instance_info;
use crate::scrape_pricing_info::scrape_pricing_info;

lazy_static! {
    static ref INSTANCE_LIST: RwLock<Vec<Ec2InstanceInfo>> = RwLock::new(Vec::new());
}

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

    fn fill_instance_list(&self) -> Result<(), Error> {
        *INSTANCE_LIST.write() = self.ec2.get_all_instances()?;
        Ok(())
    }

    pub fn list(&self, resources: &[ResourceType]) -> Result<(), Error> {
        self.fill_instance_list()?;

        let mut instances = INSTANCE_LIST.write();

        if !instances.is_empty() {
            instances.par_sort_by_key(|inst| inst.launch_time);
            instances.par_sort_by_key(|inst| inst.state != "running");
            println!("instances:");
            let result: Vec<_> = instances
                .par_iter()
                .map(|inst| {
                    let name = inst
                        .tags
                        .get("Name")
                        .cloned()
                        .unwrap_or_else(|| "".to_string());
                    format!(
                        "{} {} {} {} {} {} {}",
                        inst.id,
                        inst.dns_name,
                        inst.state,
                        name,
                        inst.instance_type,
                        inst.launch_time.with_timezone(&Local),
                        inst.availability_zone,
                    )
                })
                .collect();
            for item in result {
                println!("{}", item);
            }
        }

        let mut visited_resources = HashSet::new();
        let resources: Vec<_> = resources
            .iter()
            .filter(|x| visited_resources.insert(*x))
            .collect();

        let result: Result<Vec<_>, Error> = resources
            .into_par_iter()
            .map(|resource| {
                let mut output = Vec::new();
                match resource {
                    ResourceType::Reserved => {
                        let reserved = self.ec2.get_reserved_instances()?;
                        if reserved.is_empty() {
                            return Ok(Vec::new());
                        }
                        output.push("---\nGet Reserved Instance\n---".to_string());
                        let result: Vec<_> = reserved
                            .par_iter()
                            .map(|res| {
                                format!(
                                    "{} {} {} {} {}",
                                    res.id,
                                    res.price,
                                    res.instance_type,
                                    res.state,
                                    res.availability_zone
                                )
                            })
                            .collect();
                        output.extend_from_slice(&result);
                    }
                    ResourceType::Spot => {
                        let requests = self.ec2.get_spot_instance_requests()?;
                        if requests.is_empty() {
                            return Ok(Vec::new());
                        }
                        output.push("---\nSpot Instance Requests:".to_string());
                        let result: Vec<_> = requests
                            .par_iter()
                            .map(|req| {
                                format!(
                                    "{} {} {} {} {} {}",
                                    req.id,
                                    req.price,
                                    req.imageid,
                                    req.instance_type,
                                    req.spot_type,
                                    req.status
                                )
                            })
                            .collect();
                        output.extend_from_slice(&result);
                    }
                    ResourceType::Ami => {
                        let mut ami_tags = self.ec2.get_ami_tags()?;
                        if ami_tags.is_empty() {
                            return Ok(Vec::new());
                        }
                        let mut ubuntu_amis = self
                            .ec2
                            .get_latest_ubuntu_ami(&self.config.ubuntu_release)?;
                        ubuntu_amis.par_sort_by_key(|x| x.name.clone());
                        if !ubuntu_amis.is_empty() {
                            ami_tags.push(ubuntu_amis[ubuntu_amis.len() - 1].clone());
                        }
                        output.push("---\nAMI's:".to_string());
                        let result: Vec<_> = ami_tags
                            .par_iter()
                            .map(|ami| {
                                format!(
                                    "{} {} {} {}",
                                    ami.id,
                                    ami.name,
                                    ami.state,
                                    ami.snapshot_ids.join(" ")
                                )
                            })
                            .collect();
                        output.extend_from_slice(&result);
                    }
                    ResourceType::Key => {
                        let keys = self.ec2.get_all_key_pairs()?;
                        output.push("---\nKeys:".to_string());
                        let result: Vec<_> = keys
                            .into_par_iter()
                            .map(|(key, fingerprint)| format!("{} {}", key, fingerprint))
                            .collect();
                        output.extend_from_slice(&result);
                    }
                    ResourceType::Volume => {
                        let volumes = self.ec2.get_all_volumes()?;
                        if volumes.is_empty() {
                            return Ok(Vec::new());
                        }
                        output.push("---\nVolumes:".to_string());
                        let result: Vec<_> = volumes
                            .par_iter()
                            .map(|vol| {
                                format!(
                                    "{} {} {} {} {} {}",
                                    vol.id,
                                    vol.availability_zone,
                                    vol.size,
                                    vol.iops,
                                    vol.state,
                                    print_tags(&vol.tags)
                                )
                            })
                            .collect();
                        output.extend_from_slice(&result);
                    }
                    ResourceType::Snapshot => {
                        let snapshots = self.ec2.get_all_snapshots()?;
                        if snapshots.is_empty() {
                            return Ok(Vec::new());
                        }
                        output.push("---\nSnapshots:".to_string());
                        let result: Vec<_> = snapshots
                            .par_iter()
                            .map(|snap| {
                                format!(
                                    "{} {} GB {} {} {}",
                                    snap.id,
                                    snap.volume_size,
                                    snap.state,
                                    snap.progress,
                                    print_tags(&snap.tags)
                                )
                            })
                            .collect();
                        output.extend_from_slice(&result);
                    }
                    ResourceType::Ecr => {
                        let repos = self.ecr.get_all_repositories()?;
                        if repos.is_empty() {
                            return Ok(Vec::new());
                        }
                        output.push("---\nECR images".to_string());
                        let result: Result<Vec<_>, Error> = repos
                            .par_iter()
                            .map(|repo| {
                                let images = self.ecr.get_all_images(&repo)?;
                                let lines: Vec<_> = images
                                    .par_iter()
                                    .map(|image| {
                                        format!(
                                            "{} {} {}",
                                            repo,
                                            image
                                                .tags
                                                .get(0)
                                                .map(|s| s.as_str())
                                                .unwrap_or_else(|| "None"),
                                            image.digest
                                        )
                                    })
                                    .collect();
                                Ok(lines)
                            })
                            .collect();
                        let result: Vec<_> = result?.into_par_iter().flatten().collect();
                        output.extend_from_slice(&result);
                    }
                };
                Ok(output)
            })
            .collect();
        let output: Vec<_> = result?.into_par_iter().flatten().collect();

        for line in output {
            println!("{}", line);
        }

        Ok(())
    }

    fn get_name_map(&self) -> Result<HashMap<String, String>, Error> {
        Ok(INSTANCE_LIST
            .read()
            .par_iter()
            .filter_map(|inst| {
                if inst.state != "running" {
                    return None;
                }
                inst.tags
                    .get("Name")
                    .map(|name| (name.to_string(), inst.id.to_string()))
            })
            .collect())
    }

    pub fn terminate(&self, instance_ids: &[String]) -> Result<(), Error> {
        self.fill_instance_list()?;
        let name_map = self.get_name_map()?;
        let mapped_inst_ids: Vec<String> = instance_ids
            .par_iter()
            .map(|id| map_or_val(&name_map, id))
            .collect();
        self.ec2.terminate_instance(&mapped_inst_ids)
    }

    fn get_id_host_map(&self) -> Result<HashMap<String, String>, Error> {
        Ok(INSTANCE_LIST
            .read()
            .par_iter()
            .filter_map(|inst| {
                if inst.state != "running" {
                    return None;
                }
                Some((inst.id.to_string(), inst.dns_name.to_string()))
            })
            .collect())
    }

    pub fn connect(&self, instance_id: &str) -> Result<(), Error> {
        self.fill_instance_list()?;
        let name_map = self.get_name_map()?;
        let id_host_map = self.get_id_host_map()?;
        let inst_id = map_or_val(&name_map, instance_id);
        if let Some(host) = id_host_map.get(&inst_id) {
            println!("ssh ubuntu@{}", host)
        }
        Ok(())
    }

    pub fn get_ec2_prices(&self, search: &[String]) -> Result<(), Error> {
        let instance_families: HashMap<_, _> = InstanceFamily::get_all(&self.pool)?
            .into_par_iter()
            .map(|f| (f.family_name.to_string(), f))
            .collect();
        let instance_list: HashMap<_, _> = InstanceList::get_all_instances(&self.pool)?
            .into_par_iter()
            .map(|i| (i.instance_type.to_string(), i))
            .collect();
        let inst_list: Vec<_> = instance_list
            .par_iter()
            .map(|(k, _)| k.to_string())
            .filter(|inst| search.par_iter().any(|s| inst.contains(s)))
            .collect();

        let spot_prices = self.ec2.get_latest_spot_inst_prices(&inst_list)?;

        let prices: HashMap<_, _> = InstancePricing::get_all(&self.pool)?
            .into_par_iter()
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
                let inst_fam = inst.split('.').nth(0).unwrap();
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
        outstrings.par_sort();

        for (_, line) in &outstrings {
            println!("{}", line);
        }
        Ok(())
    }

    pub fn request_spot_instance(&self, req: &mut SpotRequest) -> Result<(), Error> {
        let ami_map = self.ec2.get_ami_map()?;
        if let Some(a) = ami_map.get(&req.ami) {
            req.ami = a.to_string();
        }

        self.ec2.request_spot_instance(&req)
    }

    pub fn run_ec2_instance(&self, req: &mut InstanceRequest) -> Result<(), Error> {
        let ami_map = self.ec2.get_ami_map()?;
        if let Some(a) = ami_map.get(&req.ami) {
            req.ami = a.to_string();
        }

        self.ec2.run_ec2_instance(&req)
    }

    pub fn create_image(&self, inst_id: &str, name: &str) -> Result<Option<String>, Error> {
        self.fill_instance_list()?;
        let name_map = self.get_name_map()?;
        let inst_id = map_or_val(&name_map, inst_id);
        self.ec2.create_image(&inst_id, name)
    }

    fn get_snapshot_map(&self) -> Result<HashMap<String, String>, Error> {
        Ok(self
            .ec2
            .get_all_snapshots()?
            .into_par_iter()
            .filter_map(|snap| {
                snap.tags
                    .get("Name")
                    .map(|n| (n.to_string(), snap.id.to_string()))
            })
            .collect())
    }

    pub fn create_ebs_volume(
        &self,
        zoneid: &str,
        size: Option<i64>,
        snapid: Option<String>,
    ) -> Result<Option<String>, Error> {
        let snap_map = self.get_snapshot_map()?;
        let snapid = snapid.map(|s| map_or_val(&snap_map, &s));
        self.ec2.create_ebs_volume(zoneid, size, snapid)
    }

    fn get_volume_map(&self) -> Result<HashMap<String, String>, Error> {
        Ok(self
            .ec2
            .get_all_volumes()?
            .into_par_iter()
            .filter_map(|vol| {
                vol.tags
                    .get("Name")
                    .map(|n| (n.to_string(), vol.id.to_string()))
            })
            .collect())
    }

    pub fn delete_ebs_volume(&self, volid: &str) -> Result<(), Error> {
        let vol_map = self.get_volume_map()?;
        let volid = map_or_val(&vol_map, volid);
        self.ec2.delete_ebs_volume(&volid)
    }

    pub fn attach_ebs_volume(&self, volid: &str, instid: &str, device: &str) -> Result<(), Error> {
        self.fill_instance_list()?;
        let vol_map = self.get_volume_map()?;
        let name_map = self.get_name_map()?;
        let volid = map_or_val(&vol_map, volid);
        let instid = map_or_val(&name_map, instid);
        self.ec2.attach_ebs_volume(&volid, &instid, device)
    }

    pub fn detach_ebs_volume(&self, volid: &str) -> Result<(), Error> {
        let vol_map = self.get_volume_map()?;
        let volid = map_or_val(&vol_map, volid);
        self.ec2.detach_ebs_volume(&volid)
    }

    pub fn modify_ebs_volume(&self, volid: &str, size: i64) -> Result<(), Error> {
        let vol_map = self.get_volume_map()?;
        let volid = map_or_val(&vol_map, volid);
        self.ec2.modify_ebs_volume(&volid, size)
    }

    pub fn create_ebs_snapshot(
        &self,
        volid: &str,
        tags: &HashMap<String, String>,
    ) -> Result<Option<String>, Error> {
        let vol_map = self.get_volume_map()?;
        let volid = map_or_val(&vol_map, volid);
        self.ec2.create_ebs_snapshot(&volid, tags)
    }

    pub fn delete_ebs_snapshot(&self, snapid: &str) -> Result<(), Error> {
        let snap_map = self.get_snapshot_map()?;
        let snapid = map_or_val(&snap_map, snapid);
        self.ec2.delete_ebs_snapshot(&snapid)
    }
}

fn print_tags(tags: &HashMap<String, String>) -> String {
    let results: Vec<_> = tags
        .par_iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    results.join(", ")
}

fn map_or_val(name_map: &HashMap<String, String>, id: &str) -> String {
    if let Some(id_) = name_map.get(id) {
        id_.to_string()
    } else {
        id.to_string()
    }
}

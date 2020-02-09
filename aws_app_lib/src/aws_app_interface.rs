use anyhow::Error;
use chrono::Local;
use futures::future::{join, join_all};
use lazy_static::lazy_static;
use parking_lot::RwLock;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use std::collections::{HashMap, HashSet};
use std::io::{stdout, Write};
use std::string::String;
use tokio::task::spawn_blocking;
use walkdir::WalkDir;

use crate::config::Config;
use crate::ec2_instance::{AmiInfo, Ec2Instance, Ec2InstanceInfo, InstanceRequest, SpotRequest};
use crate::ecr_instance::EcrInstance;
use crate::instance_family::InstanceFamilies;
use crate::models::{AwsGeneration, InstanceFamily, InstanceList, InstancePricing, PricingType};
use crate::pgpool::PgPool;
use crate::resource_type::ResourceType;
use crate::scrape_instance_info::scrape_instance_info;
use crate::scrape_pricing_info::scrape_pricing_info;
use crate::ssh_instance::SSHInstance;

lazy_static! {
    pub static ref INSTANCE_LIST: RwLock<Vec<Ec2InstanceInfo>> = RwLock::new(Vec::new());
}

#[derive(Debug)]
pub struct AwsInstancePrice {
    pub instance_type: String,
    pub ondemand_price: Option<f64>,
    pub spot_price: Option<f64>,
    pub reserved_price: Option<f64>,
    pub ncpu: i32,
    pub memory: f64,
    pub instance_family: InstanceFamilies,
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
            ec2: Ec2Instance::new(&config),
            ecr: EcrInstance::new(&config),
            config,
            pool,
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.ec2.set_region(region)?;
        self.ecr.set_region(region)
    }

    pub async fn update(&self) -> Result<Vec<String>, Error> {
        let mut output = Vec::new();
        output.extend_from_slice(&scrape_instance_info(AwsGeneration::HVM, &self.pool).await?);
        output.extend_from_slice(&scrape_instance_info(AwsGeneration::PV, &self.pool).await?);

        output.extend_from_slice(&scrape_pricing_info(PricingType::Reserved, &self.pool).await?);
        output.extend_from_slice(&scrape_pricing_info(PricingType::OnDemand, &self.pool).await?);
        Ok(output)
    }

    pub async fn fill_instance_list(&self) -> Result<(), Error> {
        let mut instances = self.ec2.get_all_instances().await?;
        if !instances.is_empty() {
            instances.par_sort_by_key(|inst| inst.launch_time);
            instances.par_sort_by_key(|inst| inst.state != "running");
        }
        *INSTANCE_LIST.write() = instances;
        Ok(())
    }

    pub async fn process_resource(&self, resource: ResourceType) -> Result<Vec<String>, Error> {
        let mut output = Vec::new();
        match resource {
            ResourceType::Instances => {
                fn process_list() -> Vec<String> {
                    INSTANCE_LIST
                        .read()
                        .par_iter()
                        .map(|inst| {
                            let name = inst
                                .tags
                                .get("Name")
                                .as_ref()
                                .map_or_else(|| "", |x| x.as_str());
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
                        .collect()
                }

                self.fill_instance_list().await?;

                let result = spawn_blocking(process_list).await?;
                if !result.is_empty() {
                    output.push("instances:".to_string());
                    output.extend_from_slice(&result);
                }
            }
            ResourceType::Reserved => {
                let reserved = self.ec2.get_reserved_instances().await?;
                if reserved.is_empty() {
                    return Ok(Vec::new());
                }
                output.push("---\nGet Reserved Instance\n---".to_string());
                let result: Vec<_> = reserved
                    .par_iter()
                    .map(|res| {
                        format!(
                            "{} {} {} {} {}",
                            res.id, res.price, res.instance_type, res.state, res.availability_zone
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
            }
            ResourceType::Spot => {
                let requests = self.ec2.get_spot_instance_requests().await?;
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
                let ubuntu_amis = self.ec2.get_latest_ubuntu_ami(&self.config.ubuntu_release);
                let ami_tags = self.ec2.get_ami_tags();
                let (ubuntu_amis, ami_tags) = join(ubuntu_amis, ami_tags).await;

                let mut ubuntu_amis = ubuntu_amis?;
                let mut ami_tags = ami_tags?;
                if ami_tags.is_empty() {
                    return Ok(Vec::new());
                }
                ubuntu_amis.par_sort_by_key(|x| x.name.clone());
                if let Some(ami) = ubuntu_amis.pop() {
                    ami_tags.push(ami);
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
                let keys = self.ec2.get_all_key_pairs().await?;
                output.push("---\nKeys:".to_string());
                let result: Vec<_> = keys
                    .into_par_iter()
                    .map(|(key, fingerprint)| format!("{} {}", key, fingerprint))
                    .collect();
                output.extend_from_slice(&result);
            }
            ResourceType::Volume => {
                let volumes = self.ec2.get_all_volumes().await?;
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
                let snapshots = self.ec2.get_all_snapshots().await?;
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
                let repos = self.ecr.get_all_repositories().await?;
                if repos.is_empty() {
                    return Ok(Vec::new());
                }
                output.push("---\nECR images".to_string());
                for repo in repos {
                    let lines = self.ecr.get_all_images(&repo).await.and_then(|images| {
                        let lines: Vec<_> = images
                            .par_iter()
                            .map(|image| {
                                format!(
                                    "{} {} {} {} {:0.2} MB",
                                    repo,
                                    image.tags.get(0).map_or_else(|| "None", String::as_str),
                                    image.digest,
                                    image.pushed_at,
                                    image.image_size,
                                )
                            })
                            .collect();
                        Ok(lines)
                    })?;
                    output.extend_from_slice(&lines);
                }
            }
            ResourceType::Script => {
                output.push("---\nScripts:".to_string());
                output.extend_from_slice(&self.get_all_scripts().await?);
            }
        };
        Ok(output)
    }

    pub async fn get_all_scripts(&self) -> Result<Vec<String>, Error> {
        let mut files: Vec<_> = WalkDir::new(&self.config.script_directory)
            .same_file_system(true)
            .into_iter()
            .filter_map(|entry| {
                entry.ok().and_then(|entry| {
                    if entry.file_type().is_dir() {
                        None
                    } else {
                        entry.path().file_name().map(|f| f.to_string_lossy().into())
                    }
                })
            })
            .collect();
        files.sort();
        Ok(files)
    }

    pub async fn list(&self, resources: &[ResourceType]) -> Result<(), Error> {
        let mut visited_resources = HashSet::new();
        let resources: Vec<_> = [ResourceType::Instances]
            .iter()
            .chain(resources.iter())
            .filter(|x| visited_resources.insert(*x))
            .collect();

        let futures: Vec<_> = resources
            .into_iter()
            .map(|resource| self.process_resource(*resource))
            .collect();

        let result: Result<Vec<_>, Error> = join_all(futures).await.into_iter().collect();
        let output: Vec<_> = result?.into_par_iter().flatten().collect();

        for line in output {
            writeln!(stdout(), "{}", line)?;
        }

        Ok(())
    }

    pub async fn terminate(&self, instance_ids: &[String]) -> Result<(), Error> {
        self.fill_instance_list().await?;
        let name_map = get_name_map()?;
        let mapped_inst_ids: Vec<String> = instance_ids
            .par_iter()
            .map(|id| map_or_val(&name_map, id))
            .collect();
        self.ec2.terminate_instance(&mapped_inst_ids).await
    }

    pub async fn connect(&self, instance_id: &str) -> Result<(), Error> {
        self.fill_instance_list().await?;
        let name_map = get_name_map()?;
        let id_host_map = get_id_host_map()?;
        let inst_id = map_or_val(&name_map, instance_id);
        if let Some(host) = id_host_map.get(&inst_id) {
            writeln!(stdout(), "ssh ubuntu@{}", host)?;
        }
        Ok(())
    }

    pub async fn get_status(&self, instance_id: &str) -> Result<Vec<String>, Error> {
        self.run_command(instance_id, "tail /var/log/cloud-init-output.log")
            .await
    }

    pub async fn run_command(
        &self,
        instance_id: &str,
        command: &str,
    ) -> Result<Vec<String>, Error> {
        self.fill_instance_list().await?;
        let name_map = get_name_map()?;
        let id_host_map = get_id_host_map()?;
        let inst_id = map_or_val(&name_map, instance_id);
        if let Some(host) = id_host_map.get(&inst_id) {
            let command = command.to_owned();
            let host = host.to_owned();
            spawn_blocking(move || {
                SSHInstance::new("ubuntu", &host, 22).run_command_stream_stdout(&command)
            })
            .await?
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn get_ec2_prices(&self, search: &[String]) -> Result<Vec<AwsInstancePrice>, Error> {
        let instance_families: HashMap<_, _> = InstanceFamily::get_all(&self.pool)
            .await?
            .into_par_iter()
            .map(|f| (f.family_name.to_string(), f))
            .collect();
        let instance_list: HashMap<_, _> = InstanceList::get_all_instances(&self.pool)
            .await?
            .into_par_iter()
            .map(|i| (i.instance_type.to_string(), i))
            .collect();
        let inst_list: Vec<_> = instance_list
            .keys()
            .filter_map(|inst| {
                if search.par_iter().any(|s| inst.contains(s)) {
                    Some(inst.to_string())
                } else {
                    None
                }
            })
            .collect();

        let spot_prices = self.ec2.get_latest_spot_inst_prices(&inst_list).await?;

        let prices: HashMap<_, _> = InstancePricing::get_all(&self.pool)
            .await?
            .into_par_iter()
            .map(|p| ((p.instance_type.to_string(), p.price_type.to_string()), p))
            .collect();

        let prices: Result<Vec<_>, Error> = inst_list
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
                    .parse()?;

                Ok(AwsInstancePrice {
                    instance_type: inst,
                    ondemand_price: ond_price,
                    spot_price: spot_price.map(|x| f64::from(*x)),
                    reserved_price: res_price,
                    ncpu: instance_metadata.n_cpu,
                    memory: instance_metadata.memory_gib,
                    instance_family,
                })
            })
            .collect();
        let mut prices = prices?;
        prices.sort_by_key(|p| (p.ncpu, p.memory as i64));
        Ok(prices)
    }

    pub async fn print_ec2_prices(&self, search: &[String]) -> Result<(), Error> {
        let prices = self.get_ec2_prices(search).await?;

        let mut outstrings: Vec<_> = prices
            .into_par_iter()
            .map(|price| {
                let mut outstr = Vec::new();
                outstr.push(format!("{:14} ", price.instance_type));
                match price.ondemand_price {
                    Some(p) => outstr.push(format!("ond: ${:0.4}/hr   ", p)),
                    None => outstr.push(format!("{:4} {:9}   ", "", "")),
                }
                match price.spot_price {
                    Some(p) => outstr.push(format!("spot: ${:0.4}/hr   ", p)),
                    None => outstr.push(format!("{:5} {:9}    ", "", "")),
                }
                match price.reserved_price {
                    Some(p) => outstr.push(format!("res: ${:0.4}/hr   ", p)),
                    None => outstr.push(format!("{:4} {:9}   ", "", "")),
                }
                outstr.push(format!("cpu: {:2}   ", price.ncpu));
                outstr.push(format!("mem: {:2}   ", price.memory));
                outstr.push(format!("inst_fam: {}", price.instance_family));

                (price.ncpu, price.memory as i64, outstr.join(""))
            })
            .collect();
        outstrings.par_sort();

        for (_, _, line) in &outstrings {
            writeln!(stdout(), "{}", line)?;
        }
        Ok(())
    }

    pub async fn delete_image(&self, ami: &str) -> Result<(), Error> {
        let ami_map = self.ec2.get_ami_map().await?;
        let ami = if let Some(a) = ami_map.get(ami) {
            a
        } else {
            ami
        };
        self.ec2.delete_image(ami).await
    }

    pub async fn request_spot_instance(&self, req: &mut SpotRequest) -> Result<(), Error> {
        let ami_map = self.ec2.get_ami_map().await?;
        if let Some(a) = ami_map.get(&req.ami) {
            req.ami = a.to_string();
        }

        self.ec2.request_spot_instance(&req).await
    }

    pub async fn run_ec2_instance(&self, req: &mut InstanceRequest) -> Result<(), Error> {
        let ami_map = self.ec2.get_ami_map().await?;
        if let Some(a) = ami_map.get(&req.ami) {
            req.ami = a.to_string();
        }

        self.ec2.run_ec2_instance(&req).await
    }

    pub async fn create_image(&self, inst_id: &str, name: &str) -> Result<Option<String>, Error> {
        self.fill_instance_list().await?;
        let name_map = get_name_map()?;
        let inst_id = map_or_val(&name_map, inst_id);
        self.ec2.create_image(&inst_id, name).await
    }

    async fn get_snapshot_map(&self) -> Result<HashMap<String, String>, Error> {
        Ok(self
            .ec2
            .get_all_snapshots()
            .await?
            .into_par_iter()
            .filter_map(|snap| {
                snap.tags
                    .get("Name")
                    .map(|n| (n.to_string(), snap.id.to_string()))
            })
            .collect())
    }

    pub async fn create_ebs_volume(
        &self,
        zoneid: &str,
        size: Option<i64>,
        snapid: Option<String>,
    ) -> Result<Option<String>, Error> {
        let snap_map = self.get_snapshot_map().await?;
        let snapid = snapid.map(|s| map_or_val(&snap_map, &s));
        self.ec2.create_ebs_volume(zoneid, size, snapid).await
    }

    async fn get_volume_map(&self) -> Result<HashMap<String, String>, Error> {
        Ok(self
            .ec2
            .get_all_volumes()
            .await?
            .into_par_iter()
            .filter_map(|vol| {
                vol.tags
                    .get("Name")
                    .map(|n| (n.to_string(), vol.id.to_string()))
            })
            .collect())
    }

    pub async fn delete_ebs_volume(&self, volid: &str) -> Result<(), Error> {
        let vol_map = self.get_volume_map().await?;
        let volid = map_or_val(&vol_map, volid);
        self.ec2.delete_ebs_volume(&volid).await
    }

    pub async fn attach_ebs_volume(
        &self,
        volid: &str,
        instid: &str,
        device: &str,
    ) -> Result<(), Error> {
        self.fill_instance_list().await?;
        let vol_map = self.get_volume_map().await?;
        let name_map = get_name_map()?;
        let volid = map_or_val(&vol_map, volid);
        let instid = map_or_val(&name_map, instid);
        self.ec2.attach_ebs_volume(&volid, &instid, device).await
    }

    pub async fn detach_ebs_volume(&self, volid: &str) -> Result<(), Error> {
        let vol_map = self.get_volume_map().await?;
        let volid = map_or_val(&vol_map, volid);
        self.ec2.detach_ebs_volume(&volid).await
    }

    pub async fn modify_ebs_volume(&self, volid: &str, size: i64) -> Result<(), Error> {
        let vol_map = self.get_volume_map().await?;
        let volid = map_or_val(&vol_map, volid);
        self.ec2.modify_ebs_volume(&volid, size).await
    }

    pub async fn create_ebs_snapshot(
        &self,
        volid: &str,
        tags: &HashMap<String, String>,
    ) -> Result<Option<String>, Error> {
        let vol_map = self.get_volume_map().await?;
        let volid = map_or_val(&vol_map, volid);
        self.ec2.create_ebs_snapshot(&volid, tags).await
    }

    pub async fn delete_ebs_snapshot(&self, snapid: &str) -> Result<(), Error> {
        let snap_map = self.get_snapshot_map().await?;
        let snapid = map_or_val(&snap_map, snapid);
        self.ec2.delete_ebs_snapshot(&snapid).await
    }

    pub async fn get_all_ami_tags(&self) -> Result<Vec<AmiInfo>, Error> {
        let mut ami_tags = self.ec2.get_ami_tags().await?;
        if ami_tags.is_empty() {
            return Ok(Vec::new());
        }
        let mut ubuntu_amis = self
            .ec2
            .get_latest_ubuntu_ami(&self.config.ubuntu_release)
            .await?;
        ubuntu_amis.par_sort_by_key(|x| x.name.clone());
        if !ubuntu_amis.is_empty() {
            ami_tags.push(ubuntu_amis[ubuntu_amis.len() - 1].clone());
        }
        Ok(ami_tags)
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

fn get_name_map() -> Result<HashMap<String, String>, Error> {
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

fn get_id_host_map() -> Result<HashMap<String, String>, Error> {
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

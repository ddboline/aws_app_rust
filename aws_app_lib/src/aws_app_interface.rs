use anyhow::{format_err, Error};
use chrono::Local;
use futures::future::try_join_all;
use itertools::Itertools;
use lazy_static::lazy_static;
use stack_string::StackString;
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};
use stdout_channel::StdoutChannel;
use tokio::{sync::RwLock, try_join};
use walkdir::WalkDir;

use crate::{
    config::Config,
    ec2_instance::{AmiInfo, Ec2Instance, Ec2InstanceInfo, InstanceRequest, SpotRequest},
    ecr_instance::EcrInstance,
    iam_instance::{IamAccessKey, IamInstance, IamUser},
    instance_family::InstanceFamilies,
    models::{AwsGeneration, InstanceFamily, InstanceList, InstancePricing},
    pgpool::PgPool,
    pricing_instance::PricingInstance,
    resource_type::ResourceType,
    route53_instance::Route53Instance,
    scrape_instance_info::scrape_instance_info,
    ssh_instance::SSHInstance,
    systemd_instance::SystemdInstance,
};

lazy_static! {
    pub static ref INSTANCE_LIST: RwLock<Vec<Ec2InstanceInfo>> = RwLock::new(Vec::new());
}

#[derive(Debug)]
pub struct AwsInstancePrice {
    pub instance_type: StackString,
    pub ondemand_price: Option<f64>,
    pub spot_price: Option<f64>,
    pub reserved_price: Option<f64>,
    pub ncpu: i32,
    pub memory: f64,
    pub instance_family: InstanceFamilies,
    pub data_url: Option<StackString>,
}

#[derive(Clone)]
pub struct AwsAppInterface {
    pub config: Config,
    pub pool: PgPool,
    pub ec2: Ec2Instance,
    pub ecr: EcrInstance,
    pub iam: IamInstance,
    pub route53: Route53Instance,
    pub pricing: PricingInstance,
    pub systemd: SystemdInstance,
    pub stdout: StdoutChannel<StackString>,
}

impl AwsAppInterface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self {
            ec2: Ec2Instance::new(&config),
            ecr: EcrInstance::new(&config),
            iam: IamInstance::new(&config),
            route53: Route53Instance::new(&config),
            pricing: PricingInstance::new(&config),
            systemd: SystemdInstance::new(&config.systemd_services),
            config,
            pool,
            stdout: StdoutChannel::new(),
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.ec2
            .set_region(region)
            .and_then(|_| self.ecr.set_region(region))
            .and_then(|_| self.route53.set_region(region))
    }

    pub async fn update(&self) -> Result<impl Iterator<Item = StackString>, Error> {
        let (hvm, pv) = try_join!(
            scrape_instance_info(AwsGeneration::HVM, &self.pool),
            scrape_instance_info(AwsGeneration::PV, &self.pool),
        )?;
        let iter = hvm.into_iter().chain(pv.into_iter());
        Ok(iter)
    }

    pub async fn fill_instance_list(&self) -> Result<(), Error> {
        let mut instances: Vec<_> = self.ec2.get_all_instances().await?.collect();
        if !instances.is_empty() {
            instances.sort_by_key(|inst| inst.launch_time);
            instances.sort_by_key(|inst| &inst.state != "running");
        }
        *INSTANCE_LIST.write().await = instances;
        Ok(())
    }

    pub async fn process_resource(&self, resource: ResourceType) -> Result<(), Error> {
        match resource {
            ResourceType::Instances => {
                self.fill_instance_list().await?;

                let result = INSTANCE_LIST
                    .read()
                    .await
                    .iter()
                    .map(|inst| {
                        let name = inst.tags.get("Name");
                        let name = name.as_ref().map_or_else(|| "", |x| x.as_ref());
                        format!(
                            "{} {} {} {} {} {} {} {}",
                            inst.id,
                            inst.dns_name,
                            inst.state,
                            name,
                            inst.instance_type,
                            inst.launch_time.with_timezone(&Local),
                            inst.availability_zone,
                            inst.volumes.join(" "),
                        )
                    })
                    .join("\n");

                if !result.is_empty() {
                    self.stdout.send(format!("instances:\n{}", result));
                }
            }
            ResourceType::Reserved => {
                let reserved = self
                    .ec2
                    .get_reserved_instances()
                    .await?
                    .map(|res| {
                        format!(
                            "{} {} {} {} {}",
                            res.id,
                            res.price,
                            res.instance_type,
                            res.state,
                            res.availability_zone
                                .as_ref()
                                .map_or_else(|| "", |s| s.as_ref())
                        )
                    })
                    .join("\n");
                if reserved.is_empty() {
                    return Ok(());
                }
                self.stdout
                    .send(format!("---\nGet Reserved Instance\n---\n{}", reserved));
            }
            ResourceType::Spot => {
                let requests = self
                    .ec2
                    .get_spot_instance_requests()
                    .await?
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
                    .join("\n");
                if requests.is_empty() {
                    return Ok(());
                }
                self.stdout
                    .send(format!("---\nSpot Instance Requests:\n{}", requests));
            }
            ResourceType::Ami => {
                let ubuntu_ami = self
                    .ec2
                    .get_latest_ubuntu_ami(&self.config.ubuntu_release, "amd64");
                let ubuntu_ami_arm64 = self
                    .ec2
                    .get_latest_ubuntu_ami(&self.config.ubuntu_release, "arm64");
                let ami_tags = self.ec2.get_ami_tags();
                let (ubuntu_ami, ubuntu_ami_arm64, ami_tags) =
                    try_join!(ubuntu_ami, ubuntu_ami_arm64, ami_tags)?;
                let mut ami_tags: Vec<_> = ami_tags.collect();

                if ami_tags.is_empty() {
                    return Ok(());
                }
                if let Some(ami) = ubuntu_ami {
                    ami_tags.push(ami);
                }
                if let Some(ami) = ubuntu_ami_arm64 {
                    ami_tags.push(ami);
                }
                let ami_tags = ami_tags
                    .into_iter()
                    .map(|ami| {
                        format!(
                            "{} {} {} {}",
                            ami.id,
                            ami.name,
                            ami.state,
                            ami.snapshot_ids.join(" ")
                        )
                    })
                    .join("\n");
                self.stdout.send(format!("---\nAMI's:\n{}", ami_tags));
            }
            ResourceType::Key => {
                let keys = self
                    .ec2
                    .get_all_key_pairs()
                    .await?
                    .map(|(key, fingerprint)| format!("{} {}", key, fingerprint))
                    .join("\n");
                self.stdout.send(format!("---\nKeys:\n{}", keys));
            }
            ResourceType::Volume => {
                let volumes = self
                    .ec2
                    .get_all_volumes()
                    .await?
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
                    .join("\n");
                if volumes.is_empty() {
                    return Ok(());
                }
                self.stdout.send(format!("---\nVolumes:\n{}", volumes));
            }
            ResourceType::Snapshot => {
                let snapshots = self
                    .ec2
                    .get_all_snapshots()
                    .await?
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
                    .join("\n");
                if snapshots.is_empty() {
                    return Ok(());
                }
                self.stdout.send(format!("---\nSnapshots:\n{}", snapshots));
            }
            ResourceType::Ecr => {
                let futures = self
                    .ecr
                    .get_all_repositories()
                    .await?
                    .map(|repo| async move {
                        let lines = self
                            .ecr
                            .get_all_images(repo.as_ref())
                            .await?
                            .map(|image| {
                                format!(
                                    "{} {} {} {} {:0.2} MB",
                                    repo,
                                    image.tags.get(0).map_or_else(|| "None", |s| s.as_ref()),
                                    image.digest,
                                    image.pushed_at,
                                    image.image_size,
                                )
                            })
                            .join("\n");
                        Ok(lines)
                    });
                let results: Result<Vec<_>, Error> = try_join_all(futures).await;
                let results = results?.join("\n");
                if results.is_empty() {
                    return Ok(());
                }
                self.stdout.send(format!("---\nECR images:\n{}", results));
            }
            ResourceType::Script => {
                self.stdout.send(format!(
                    "---\nScripts:\n{}",
                    self.get_all_scripts()?.join("\n")
                ));
            }
            ResourceType::User => {
                let users = self
                    .iam
                    .list_users()
                    .await?
                    .map(|u| {
                        format!(
                            "{} {} {:30} {:60}",
                            u.user_id, u.create_date, u.user_name, u.arn,
                        )
                    })
                    .join("\n");
                self.stdout.send(format!("---\nUsers:\n{}", users));
            }
            ResourceType::Group => {
                let groups = self
                    .iam
                    .list_groups()
                    .await?
                    .map(|g| {
                        format!(
                            "{} {} {:30} {:60}",
                            g.group_id, g.create_date, g.group_name, g.arn,
                        )
                    })
                    .join("\n");
                self.stdout.send(format!("---\nGroups:\n{}", groups));
            }
            ResourceType::AccessKey => {
                let futures =
                    self.iam.list_users().await?.map(|user| async move {
                        self.iam.list_access_keys(&user.user_name).await
                    });
                let results: Result<Vec<Vec<_>>, Error> = try_join_all(futures).await;
                let keys = results?
                    .into_iter()
                    .map(|keys| {
                        keys.into_iter()
                            .filter_map(|key| {
                                Some(format!(
                                    "{} {:30} {} {}",
                                    key.access_key_id?,
                                    key.user_name?,
                                    key.create_date?,
                                    key.status?
                                ))
                            })
                            .join("\n")
                    })
                    .join("\n");
                self.stdout.send(format!("---\nAccess Keys:\n{}", keys));
            }
            ResourceType::Route53 => {
                let current_ip = self.route53.get_ip_address().await?;
                let dns_records = self
                    .route53
                    .list_all_dns_records()
                    .await?
                    .into_iter()
                    .map(|(zone, name, ip)| format!("{} {} {} {}", zone, name, ip, current_ip))
                    .join("\n");
                self.stdout.send(format!("---\nDNS:\n{}", dns_records));
            }
            ResourceType::SystemD => {
                let services = self.systemd.list_running_services().await?;
                for service in &self.config.systemd_services {
                    if let Some(val) = services.get(service) {
                        self.stdout.send(format!("{} {}", service, val));
                    } else {
                        self.stdout.send(format!("{} not running", service));
                    }
                }
            }
        };
        Ok(())
    }

    pub fn get_all_scripts(&self) -> Result<Vec<StackString>, Error> {
        let mut files: Vec<_> = WalkDir::new(&self.config.script_directory)
            .same_file_system(true)
            .into_iter()
            .filter_map(|entry| {
                entry.ok().and_then(|entry| {
                    if entry.file_type().is_dir() {
                        None
                    } else {
                        entry
                            .path()
                            .file_name()
                            .map(|f| f.to_string_lossy().as_ref().into())
                    }
                })
            })
            .collect();
        files.sort();
        Ok(files)
    }

    pub async fn list(
        &self,
        resources: impl IntoIterator<Item = &ResourceType>,
    ) -> Result<(), Error> {
        let mut visited_resources = HashSet::new();

        let futures = [ResourceType::Instances]
            .iter()
            .chain(resources.into_iter())
            .map(|resource| {
                let resource = *resource;
                let visit_resource = visited_resources.insert(resource);
                async move {
                    if visit_resource {
                        self.process_resource(resource).await?;
                    }
                    Ok(())
                }
            });

        let result: Result<Vec<_>, Error> = try_join_all(futures).await;
        result?;

        Ok(())
    }

    pub async fn terminate(
        &self,
        instance_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), Error> {
        self.fill_instance_list().await?;
        let name_map = get_name_map().await?;
        let mapped_inst_ids: Vec<_> = instance_ids
            .into_iter()
            .map(|id| map_or_val(&name_map, id.as_ref()).to_string())
            .collect();
        self.ec2.terminate_instance(&mapped_inst_ids).await
    }

    pub async fn connect(&self, instance_id: &str) -> Result<(), Error> {
        self.fill_instance_list().await?;
        let name_map = get_name_map().await?;
        let id_host_map = get_id_host_map().await?;
        let inst_id = map_or_val(&name_map, instance_id);
        if let Some(host) = id_host_map.get(inst_id) {
            self.stdout.send(format!("ssh ubuntu@{}", host));
        }
        Ok(())
    }

    pub async fn get_status(&self, instance_id: &str) -> Result<Vec<StackString>, Error> {
        self.run_command(instance_id, "tail /var/log/cloud-init-output.log")
            .await
    }

    pub async fn run_command(
        &self,
        instance_id: &str,
        command: &str,
    ) -> Result<Vec<StackString>, Error> {
        self.fill_instance_list().await?;
        let name_map = get_name_map().await?;
        let id_host_map = get_id_host_map().await?;
        let inst_id = map_or_val(&name_map, instance_id);
        if let Some(host) = id_host_map.get(inst_id) {
            SSHInstance::new("ubuntu", host.as_ref(), 22)
                .await
                .run_command_stream_stdout(&command)
                .await
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn get_ec2_prices(
        &self,
        search: &[impl AsRef<str>],
    ) -> Result<Vec<AwsInstancePrice>, Error> {
        let instance_families: HashMap<_, _> = InstanceFamily::get_all(&self.pool)
            .await?
            .into_iter()
            .map(|f| (f.family_name.to_string(), f))
            .collect();
        let instance_list: HashMap<_, _> = InstanceList::get_all_instances(&self.pool)
            .await?
            .into_iter()
            .map(|i| (i.instance_type.to_string(), i))
            .collect();
        let inst_list: Vec<_> = instance_list
            .keys()
            .filter_map(|inst| {
                if search.iter().any(|s| inst.starts_with(s.as_ref())) {
                    Some(inst.to_string())
                } else {
                    None
                }
            })
            .collect();

        let spot_prices = self.ec2.get_latest_spot_inst_prices(&inst_list).await?;

        let prices: HashMap<_, _> = InstancePricing::get_all(&self.pool)
            .await?
            .into_iter()
            .map(|p| ((p.instance_type.to_string(), p.price_type.to_string()), p))
            .collect();

        let prices: Result<Vec<_>, Error> = inst_list
            .into_iter()
            .map(|inst| {
                let ond_price = prices
                    .get(&(inst.to_string(), "ondemand".to_string()))
                    .map(|x| x.price);
                let res_price = prices
                    .get(&(inst.to_string(), "reserved".to_string()))
                    .map(|x| x.price);
                let spot_price = spot_prices.get(inst.as_str());
                let instance_metadata = instance_list
                    .get(&inst)
                    .ok_or_else(|| format_err!("this should be impossible {}", inst))?;
                let inst_fam = inst
                    .split('.')
                    .next()
                    .ok_or_else(|| format_err!("invalid instance name {}", inst))?;
                let inst_fam = instance_families
                    .get(inst_fam)
                    .ok_or_else(|| format_err!("inst_fam {} does not exist", inst_fam))?;
                let instance_family = inst_fam.family_type.parse()?;

                Ok(AwsInstancePrice {
                    instance_type: inst.into(),
                    ondemand_price: ond_price,
                    spot_price: spot_price.map(|x| f64::from(*x)),
                    reserved_price: res_price,
                    ncpu: instance_metadata.n_cpu,
                    memory: instance_metadata.memory_gib,
                    instance_family,
                    data_url: inst_fam.data_url.clone(),
                })
            })
            .collect();
        let mut prices = prices?;
        prices.sort_by_key(|p| (p.ncpu, p.memory as i64));
        Ok(prices)
    }

    pub async fn print_ec2_prices(&self, search: &[impl AsRef<str>]) -> Result<(), Error> {
        let mut prices: Vec<_> = self
            .get_ec2_prices(search)
            .await?
            .into_iter()
            .map(|price| {
                let mut outstr = vec![format!("{:14} ", price.instance_type)];
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
        prices.sort();

        let outstrings: Vec<_> = prices.into_iter().map(|(_, _, line)| line).collect();
        self.stdout.send(outstrings.join("\n"));
        Ok(())
    }

    pub async fn delete_image(&self, ami: &str) -> Result<(), Error> {
        let ami_map = self.ec2.get_ami_map().await?;
        let ami = ami_map.get(ami).map_or(ami, |a| a.as_ref());
        self.ec2.delete_image(ami).await
    }

    pub async fn request_spot_instance(&self, req: &mut SpotRequest) -> Result<(), Error> {
        let ami_map = self.ec2.get_ami_map().await?;
        if let Some(a) = ami_map.get(&req.ami) {
            req.ami = a.clone();
        }
        if let Some(spot_id) = self.ec2.request_spot_instance(&req).await?.pop() {
            self.ec2.tag_spot_instance(&spot_id, &req.tags, 20).await?;
        }
        Ok(())
    }

    pub async fn run_ec2_instance(&self, req: &mut InstanceRequest) -> Result<(), Error> {
        let ami_map = self.ec2.get_ami_map().await?;
        if let Some(a) = ami_map.get(&req.ami) {
            req.ami = a.clone();
        }

        self.ec2.run_ec2_instance(&req).await
    }

    pub async fn create_image(
        &self,
        inst_id: &str,
        name: &str,
    ) -> Result<Option<StackString>, Error> {
        self.fill_instance_list().await?;
        let name_map = get_name_map().await?;
        let inst_id = map_or_val(&name_map, inst_id);
        self.ec2.create_image(&inst_id, name).await
    }

    async fn get_snapshot_map(&self) -> Result<HashMap<StackString, StackString>, Error> {
        let snapshot_map = self
            .ec2
            .get_all_snapshots()
            .await?
            .filter_map(|snap| snap.tags.get("Name").map(|n| (n.clone(), snap.id.clone())))
            .collect();
        Ok(snapshot_map)
    }

    pub async fn create_ebs_volume(
        &self,
        zoneid: &str,
        size: Option<i64>,
        snapid: Option<impl AsRef<str>>,
    ) -> Result<Option<StackString>, Error> {
        let snap_map = self.get_snapshot_map().await?;
        let snapid = snapid.map(|s| map_or_val(&snap_map, s.as_ref()).to_string());
        self.ec2.create_ebs_volume(zoneid, size, snapid).await
    }

    async fn get_volume_map(&self) -> Result<HashMap<StackString, StackString>, Error> {
        let volume_map = self
            .ec2
            .get_all_volumes()
            .await?
            .filter_map(|vol| vol.tags.get("Name").map(|n| (n.clone(), vol.id.clone())))
            .collect();
        Ok(volume_map)
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
        let name_map = get_name_map().await?;
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
        tags: &HashMap<StackString, StackString>,
    ) -> Result<Option<StackString>, Error> {
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
        let ami_tag_task = self.ec2.get_ami_tags();
        let ubuntu_ami_task = self
            .ec2
            .get_latest_ubuntu_ami(&self.config.ubuntu_release, "amd64");
        let ubuntu_ami_arm64_task = self
            .ec2
            .get_latest_ubuntu_ami(&self.config.ubuntu_release, "arm64");

        let (ami_tags, ubuntu_ami, ubuntu_ami_arm64) =
            try_join!(ami_tag_task, ubuntu_ami_task, ubuntu_ami_arm64_task)?;
        let mut ami_tags: Vec<_> = ami_tags.collect();
        if let Some(ami) = ubuntu_ami {
            ami_tags.push(ami);
        }
        if let Some(ami) = ubuntu_ami_arm64 {
            ami_tags.push(ami);
        }
        Ok(ami_tags)
    }

    pub async fn create_user(&self, user_name: &str) -> Result<Option<IamUser>, Error> {
        self.iam.create_user(user_name).await
    }

    pub async fn delete_user(&self, user_name: &str) -> Result<(), Error> {
        self.iam.delete_user(user_name).await
    }

    pub async fn add_user_to_group(&self, user_name: &str, group_name: &str) -> Result<(), Error> {
        self.iam.add_user_to_group(user_name, group_name).await
    }

    pub async fn remove_user_from_group(
        &self,
        user_name: &str,
        group_name: &str,
    ) -> Result<(), Error> {
        self.iam.remove_user_from_group(user_name, group_name).await
    }

    pub async fn create_access_key(&self, user_name: &str) -> Result<IamAccessKey, Error> {
        self.iam.create_access_key(user_name).await
    }

    pub async fn delete_access_key(
        &self,
        user_name: &str,
        access_key_id: &str,
    ) -> Result<(), Error> {
        self.iam.delete_access_key(user_name, access_key_id).await
    }
}

fn print_tags(tags: &HashMap<impl Display, impl Display>) -> StackString {
    let results: Vec<_> = tags.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    results.join(", ").into()
}

fn map_or_val<'a>(name_map: &'a HashMap<StackString, StackString>, id: &'a str) -> &'a str {
    name_map.get(id).map_or(id, |id_| id_.as_ref())
}

async fn get_name_map() -> Result<HashMap<StackString, StackString>, Error> {
    let name_map = INSTANCE_LIST
        .read()
        .await
        .iter()
        .filter_map(|inst| {
            if &inst.state != "running" {
                return None;
            }
            inst.tags
                .get("Name")
                .map(|name| (name.clone(), inst.id.clone()))
        })
        .collect();
    Ok(name_map)
}

async fn get_id_host_map() -> Result<HashMap<StackString, StackString>, Error> {
    let id_host_map = INSTANCE_LIST
        .read()
        .await
        .iter()
        .filter_map(|inst| {
            if &inst.state != "running" {
                return None;
            }
            Some((inst.id.clone(), inst.dns_name.clone()))
        })
        .collect();
    Ok(id_host_map)
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use stack_string::StackString;

    use crate::{
        aws_app_interface::{get_id_host_map, get_name_map, INSTANCE_LIST},
        ec2_instance::Ec2InstanceInfo,
    };

    #[tokio::test]
    async fn test_get_id_host_map() -> Result<(), Error> {
        let js = include_str!("../../tests/data/ec2_instances.json");
        let instances: Vec<Ec2InstanceInfo> = serde_json::from_str(&js)?;
        *INSTANCE_LIST.write().await = instances;
        let host_map = get_id_host_map().await?;
        assert!(host_map.len() == 1);
        assert_eq!(
            host_map.get("i-05c99b55b3acf8606").map(StackString::as_str),
            Some("ec2-3-231-142-210.compute-1.amazonaws.com")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_name_map() -> Result<(), Error> {
        let js = include_str!("../../tests/data/ec2_instances.json");
        let instances: Vec<Ec2InstanceInfo> = serde_json::from_str(&js)?;
        *INSTANCE_LIST.write().await = instances;
        let name_map = get_name_map().await?;
        assert!(name_map.len() == 1);
        assert_eq!(
            name_map.get("ddbolineinthecloud").map(StackString::as_str),
            Some("i-05c99b55b3acf8606")
        );
        Ok(())
    }
}

use anyhow::{format_err, Error};
use aws_config::SdkConfig;
use aws_sdk_ec2::{
    primitives::DateTime,
    types::{
        Filter, InstanceType, RequestSpotLaunchSpecification, ResourceType, Tag, TagSpecification,
        VolumeType,
    },
    Client as Ec2Client,
};
use aws_types::region::Region;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use itertools::Itertools;
use log::debug;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use stack_string::{format_sstr, StackString};
use std::{
    collections::HashMap,
    fmt,
    fs::read_to_string,
    path::{Path, PathBuf},
};
use time::{Duration, OffsetDateTime, UtcOffset};
use tokio::{task::spawn, time::sleep};

use crate::{config::Config, date_time_wrapper::DateTimeWrapper};

static UBUNTU_OWNER: &str = "099720109477";

#[derive(Clone)]
pub struct Ec2Instance {
    ec2_client: Ec2Client,
    my_owner_id: Option<StackString>,
    script_dir: PathBuf,
    region: Region,
}

impl fmt::Debug for Ec2Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ec2Instance")
    }
}

impl Ec2Instance {
    #[must_use]
    pub fn new(config: &Config, sdk_config: &SdkConfig) -> Self {
        let region: String = config.aws_region_name.as_str().into();
        Self {
            ec2_client: Ec2Client::from_conf(sdk_config.into()),
            my_owner_id: config.my_owner_id.clone(),
            script_dir: config.script_directory.clone(),
            region: Region::new(region),
        }
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn set_region(&mut self, region: impl AsRef<str>) -> Result<(), Error> {
        let region: String = region.as_ref().into();
        let region = Region::new(region);
        self.region = region.clone();
        let sdk_config = aws_config::from_env().region(region).load().await;
        self.ec2_client = Ec2Client::from_conf((&sdk_config).into());
        Ok(())
    }

    pub fn set_owner_id(&mut self, owner_id: impl Into<StackString>) -> Option<StackString> {
        self.my_owner_id.replace(owner_id.into())
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_ami_tags(&self) -> Result<impl Iterator<Item = AmiInfo>, Error> {
        let owner_id = self
            .my_owner_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| format_err!("No owner id"))?;
        let filter = Filter::builder().name("owner-id").values(owner_id).build();
        self.ec2_client
            .describe_images()
            .filters(filter)
            .send()
            .await
            .map(|l| {
                l.images
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|image| {
                        Some(AmiInfo {
                            id: image.image_id?.into(),
                            name: image.name?.into(),
                            state: image.state?.as_str().into(),
                            snapshot_ids: image
                                .block_device_mappings?
                                .into_iter()
                                .filter_map(|block| {
                                    block.ebs.and_then(|b| b.snapshot_id.map(Into::into))
                                })
                                .collect(),
                        })
                    })
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_latest_ubuntu_ami(
        &self,
        ubuntu_release: impl fmt::Display,
        arch: impl fmt::Display,
    ) -> Result<Option<AmiInfo>, Error> {
        let owner_filter = Filter::builder()
            .name("owner-id")
            .values(UBUNTU_OWNER)
            .build();
        let name_filter = Filter::builder()
            .name("name")
            .values(format_sstr!(
                "ubuntu/images/hvm-ssd/ubuntu-{ubuntu_release}-{arch}-server*"
            ))
            .build();
        let resp = self
            .ec2_client
            .describe_images()
            .filters(owner_filter)
            .filters(name_filter)
            .send()
            .await?;

        let image = resp
            .images
            .unwrap_or_default()
            .into_iter()
            .filter_map(|image| {
                Some(AmiInfo {
                    id: image.image_id?.into(),
                    name: image.name?.into(),
                    state: image.state?.as_str().into(),
                    snapshot_ids: image
                        .block_device_mappings?
                        .into_iter()
                        .filter_map(|block| block.ebs.and_then(|b| b.snapshot_id.map(Into::into)))
                        .collect(),
                })
            })
            .minmax_by(|x, y| x.name.cmp(&y.name))
            .into_option()
            .map(|(_, x)| x);

        Ok(image)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_ami_map(&self) -> Result<HashMap<StackString, StackString>, Error> {
        let req = self.get_ami_tags().await?;
        let mut latest_ami_name = None;
        let mut latest_arm64_name = None;
        let mut ami_map: HashMap<_, _> = req
            .map(|ami| {
                if ami.name.contains("_tmpfs_")
                    && (latest_ami_name.is_none() || Some(&ami.name) > latest_ami_name.as_ref())
                {
                    latest_ami_name = Some(ami.name.clone());
                } else if ami.name.contains("_arm64_")
                    && (latest_arm64_name.is_none() || Some(&ami.name) > latest_arm64_name.as_ref())
                {
                    latest_arm64_name = Some(ami.name.clone());
                }
                (ami.name, ami.id)
            })
            .collect();
        if let Some(latest_ami_name) = &latest_ami_name {
            let latest_ami_id = ami_map
                .get(latest_ami_name)
                .ok_or_else(|| format_err!("No latest ami"))?
                .clone();
            ami_map.insert("latest".into(), latest_ami_id);
        }
        if let Some(latest_arm64_name) = &latest_arm64_name {
            let latest_arm64_id = ami_map
                .get(latest_arm64_name)
                .ok_or_else(|| format_err!("No latest ami"))?
                .clone();
            ami_map.insert("arm64".into(), latest_arm64_id);
        }
        Ok(ami_map)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_all_regions(&self) -> Result<HashMap<StackString, StackString>, Error> {
        self.ec2_client
            .describe_regions()
            .send()
            .await
            .map(|r| {
                r.regions
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|region| {
                        Some((region.region_name?.into(), region.opt_in_status?.into()))
                    })
                    .collect()
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_all_instances(&self) -> Result<impl Iterator<Item = Ec2InstanceInfo>, Error> {
        self.ec2_client
            .describe_instances()
            .send()
            .await
            .map(|instances| {
                instances
                    .reservations
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|res| {
                        res.instances.map(|instances| {
                            instances.into_iter().filter_map(|inst| {
                                let tags: HashMap<_, _> = inst
                                    .tags
                                    .unwrap_or_default()
                                    .into_iter()
                                    .filter_map(|tag| Some((tag.key?.into(), tag.value?.into())))
                                    .collect();
                                let volumes = inst
                                    .block_device_mappings
                                    .unwrap_or_default()
                                    .into_iter()
                                    .filter_map(|bm| {
                                        let ebs = bm.ebs?.volume_id?;
                                        Some(ebs.into())
                                    })
                                    .collect();
                                Some(Ec2InstanceInfo {
                                    id: inst.instance_id?.into(),
                                    dns_name: inst.public_dns_name?.into(),
                                    state: inst.state?.name?.as_str().into(),
                                    instance_type: inst.instance_type?.as_str().into(),
                                    availability_zone: inst.placement?.availability_zone?.into(),
                                    launch_time: inst
                                        .launch_time
                                        .and_then(|t| {
                                            OffsetDateTime::from_unix_timestamp(
                                                t.as_secs_f64() as i64
                                            )
                                            .ok()
                                        })
                                        .map(|t| t.to_offset(UtcOffset::UTC).into())?,
                                    tags,
                                    volumes,
                                })
                            })
                        })
                    })
                    .flatten()
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_reserved_instances(
        &self,
    ) -> Result<impl Iterator<Item = ReservedInstanceInfo>, Error> {
        self.ec2_client
            .describe_reserved_instances()
            .send()
            .await
            .map(|res| {
                res.reserved_instances
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|inst| {
                        let state = inst.state.as_ref()?;
                        if state.as_str() == "retired" {
                            return None;
                        }
                        Some(ReservedInstanceInfo {
                            id: inst.reserved_instances_id?.into(),
                            price: inst.fixed_price?,
                            instance_type: inst.instance_type?.as_str().into(),
                            state: inst.state?.as_str().into(),
                            availability_zone: inst.availability_zone.map(Into::into),
                        })
                    })
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_availability_zones(&self) -> Result<Vec<String>, Error> {
        let filter = Filter::builder()
            .name("region-name")
            .values(self.region.as_ref())
            .build();
        let zones = self
            .ec2_client
            .describe_availability_zones()
            .filters(filter)
            .send()
            .await?
            .availability_zones
            .unwrap_or_default()
            .into_iter()
            .filter_map(|zone| zone.zone_name)
            .collect();
        Ok(zones)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_latest_spot_inst_prices(
        &self,
        inst_list: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<HashMap<StackString, f32>, Error> {
        let inst_list: Vec<_> = inst_list.into_iter().map(|x| x.as_ref().into()).collect();
        let zones = self.get_availability_zones().await?;
        let filters = vec![
            Filter::builder()
                .name("product-description")
                .values("Linux/UNIX")
                .build(),
            Filter::builder()
                .name("availability-zone")
                .set_values(Some(zones))
                .build(),
        ];
        let start_time =
            DateTime::from_secs((OffsetDateTime::now_utc() - Duration::hours(4)).unix_timestamp());
        let mut builder = self
            .ec2_client
            .describe_spot_price_history()
            .start_time(start_time)
            .set_filters(Some(filters));
        if !inst_list.is_empty() {
            builder = builder.set_instance_types(Some(inst_list));
        }
        builder
            .send()
            .await
            .map(|spot_hist| {
                spot_hist
                    .spot_price_history
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|spot_price| {
                        Some((
                            spot_price.instance_type?.as_str().into(),
                            spot_price.spot_price.and_then(|s| s.parse().ok())?,
                        ))
                    })
                    .collect()
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_spot_instance_requests(
        &self,
    ) -> Result<impl Iterator<Item = SpotInstanceRequestInfo>, Error> {
        self.ec2_client
            .describe_spot_instance_requests()
            .send()
            .await
            .map(|s| {
                s.spot_instance_requests
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|inst| {
                        let launch_spec = inst.launch_specification?;
                        Some(SpotInstanceRequestInfo {
                            id: inst.spot_instance_request_id?.into(),
                            price: inst
                                .spot_price
                                .and_then(|s| s.parse::<f32>().ok())
                                .unwrap_or(0.0),
                            instance_type: launch_spec.instance_type?.as_str().into(),
                            spot_type: inst.r#type?.as_str().into(),
                            status: inst.status?.code?.into(),
                            imageid: launch_spec.image_id?.into(),
                            instance_id: inst.instance_id.map(Into::into),
                        })
                    })
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_all_volumes(&self) -> Result<impl Iterator<Item = VolumeInfo>, Error> {
        self.ec2_client
            .describe_volumes()
            .send()
            .await
            .map(|v| {
                v.volumes.unwrap_or_default().into_iter().filter_map(|v| {
                    Some(VolumeInfo {
                        id: v.volume_id?.into(),
                        availability_zone: v.availability_zone?.into(),
                        size: v.size?.into(),
                        iops: v.iops.unwrap_or(0).into(),
                        state: v.state?.as_str().into(),
                        tags: v
                            .tags
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|t| Some((t.key?.into(), t.value?.into())))
                            .collect(),
                    })
                })
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_all_snapshots(&self) -> Result<impl Iterator<Item = SnapshotInfo>, Error> {
        let owner_id = self
            .my_owner_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| format_err!("No owner id"))?;
        let filter = Filter::builder().name("owner-id").values(owner_id).build();
        self.ec2_client
            .describe_snapshots()
            .filters(filter)
            .send()
            .await
            .map(|s| {
                s.snapshots
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|snap| {
                        Some(SnapshotInfo {
                            id: snap.snapshot_id?.into(),
                            volume_size: snap.volume_size?.into(),
                            state: snap.state?.as_str().into(),
                            progress: snap.progress?.into(),
                            tags: snap
                                .tags
                                .unwrap_or_default()
                                .into_iter()
                                .filter_map(|t| Some((t.key?.into(), t.value?.into())))
                                .collect(),
                        })
                    })
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn terminate_instance(
        &self,
        instance_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), Error> {
        let instance_ids = instance_ids
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        self.ec2_client
            .terminate_instances()
            .set_instance_ids(Some(instance_ids))
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn request_spot_instance(&self, spot: &SpotRequest) -> Result<Vec<String>, Error> {
        let user_data = get_user_data_from_script(&self.script_dir, &spot.script)?;
        let instance_type: InstanceType = spot.instance_type.parse()?;
        let launch_specification = RequestSpotLaunchSpecification::builder()
            .image_id(&spot.ami)
            .instance_type(instance_type)
            .security_group_ids(&spot.security_group)
            .user_data(STANDARD_NO_PAD.encode(&user_data))
            .key_name(&spot.key_name)
            .build();
        let mut builder = self
            .ec2_client
            .request_spot_instances()
            .instance_count(1)
            .launch_specification(launch_specification);
        if let Some(spot_price) = spot.price {
            builder = builder.spot_price(format_sstr!("{spot_price}"));
        }
        let req = builder.send().await?;
        let spot_ids = req
            .spot_instance_requests
            .unwrap_or_default()
            .into_iter()
            .filter_map(|result| result.spot_instance_request_id)
            .collect();
        Ok(spot_ids)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn tag_ami_snapshot(
        &self,
        inst_id: impl AsRef<str>,
        name: impl Into<StackString>,
        iterations: usize,
    ) -> Result<(), Error> {
        sleep(std::time::Duration::from_secs(2)).await;
        let inst_id = inst_id.as_ref();
        for i in 0..iterations {
            if let Some(ami) = self
                .get_ami_tags()
                .await?
                .find(|ami| ami.id == inst_id || ami.name == inst_id)
            {
                if let Some(snapshot_id) = ami.snapshot_ids.first() {
                    let tags = hashmap! {"Name".into() => name.into()};
                    self.tag_ec2_instance(snapshot_id, &tags).await?;
                    break;
                }
            }
            let secs = if i < 20 {
                2
            } else if i < 40 {
                20
            } else {
                40
            };
            sleep(std::time::Duration::from_secs(secs)).await;
        }
        Ok(())
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn tag_spot_instance(
        &self,
        spot_instance_request_id: &str,
        tags: &HashMap<StackString, StackString>,
        iterations: usize,
    ) -> Result<(), Error> {
        sleep(std::time::Duration::from_secs(2)).await;
        for i in 0..iterations {
            let reqs: HashMap<_, _> = self
                .get_spot_instance_requests()
                .await?
                .map(|r| (r.id, r.instance_id))
                .collect();
            if !reqs.contains_key(spot_instance_request_id) && i > 10 {
                return Ok(());
            }
            let instances: HashMap<_, _> = self
                .get_all_instances()
                .await?
                .map(|inst| (inst.id.clone(), inst))
                .collect();
            if let Some(Some(instance_id)) = reqs.get(spot_instance_request_id) {
                debug!("tag {} with {:?}", instance_id, tags);
                self.tag_ec2_instance(instance_id, tags).await?;
                if let Some(inst) = instances.get(instance_id) {
                    if !inst.volumes.is_empty() {
                        for vol in &inst.volumes {
                            self.tag_ec2_instance(vol.as_str(), tags).await?;
                        }
                        return Ok(());
                    }
                }
            }
            let secs = if i < 20 {
                2
            } else if i < 40 {
                20
            } else {
                40
            };
            sleep(std::time::Duration::from_secs(secs)).await;
        }
        Ok(())
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn cancel_spot_instance_request(
        &self,
        inst_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), Error> {
        let inst_ids = inst_ids
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        self.ec2_client
            .cancel_spot_instance_requests()
            .set_spot_instance_request_ids(Some(inst_ids))
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn tag_ec2_instance(
        &self,
        inst_id: impl Into<String>,
        tags: &HashMap<StackString, StackString>,
    ) -> Result<(), Error> {
        self.ec2_client
            .create_tags()
            .resources(inst_id)
            .set_tags(Some(
                tags.iter()
                    .map(|(k, v)| Tag::builder().key(k).value(v).build())
                    .collect(),
            ))
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn run_ec2_instance(&self, request: &InstanceRequest) -> Result<(), Error> {
        let user_data = get_user_data_from_script(&self.script_dir, &request.script)?;
        let instance_type: InstanceType = request.instance_type.parse()?;
        let req = self
            .ec2_client
            .run_instances()
            .image_id(&request.ami)
            .instance_type(instance_type)
            .min_count(1)
            .max_count(1)
            .key_name(&request.key_name)
            .security_group_ids(&request.security_group)
            .user_data(STANDARD_NO_PAD.encode(&user_data))
            .send()
            .await?;
        for inst in req.instances.unwrap_or_default() {
            if let Some(inst) = inst.instance_id {
                self.tag_ec2_instance(&inst, &request.tags).await?;
            }
        }
        Ok(())
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn create_image(
        &self,
        inst_id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Option<StackString>, Error> {
        let name = name.into();
        self.ec2_client
            .create_image()
            .instance_id(inst_id)
            .name(&name)
            .send()
            .await
            .map_err(Into::into)
            .map(|r| {
                r.image_id.map(|image_id| {
                    {
                        let image_id = image_id.clone();
                        let ec2 = self.clone();
                        spawn(async move { ec2.tag_ami_snapshot(&image_id, &name, 100).await });
                    }
                    image_id.into()
                })
            })
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn delete_image(&self, ami: impl Into<String>) -> Result<(), Error> {
        self.ec2_client
            .deregister_image()
            .image_id(ami)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn create_ebs_volume(
        &self,
        zoneid: impl Into<String>,
        size: Option<i32>,
        snapid: Option<impl AsRef<str>>,
    ) -> Result<Option<StackString>, Error> {
        let snapid = snapid.as_ref().map(|s| s.as_ref().to_string());
        self.ec2_client
            .create_volume()
            .availability_zone(zoneid)
            .set_size(size)
            .set_snapshot_id(snapid)
            .volume_type(VolumeType::Standard)
            .send()
            .await
            .map(|v| v.volume_id.map(Into::into))
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn delete_ebs_volume(&self, volid: impl Into<String>) -> Result<(), Error> {
        self.ec2_client
            .delete_volume()
            .volume_id(volid)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn attach_ebs_volume(
        &self,
        volid: impl Into<String>,
        instid: impl Into<String>,
        device: impl Into<String>,
    ) -> Result<(), Error> {
        self.ec2_client
            .attach_volume()
            .device(device)
            .instance_id(instid)
            .volume_id(volid)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn detach_ebs_volume(&self, volid: impl Into<String>) -> Result<(), Error> {
        self.ec2_client
            .detach_volume()
            .volume_id(volid)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn modify_ebs_volume(
        &self,
        volid: impl Into<String>,
        size: i32,
    ) -> Result<(), Error> {
        self.ec2_client
            .modify_volume()
            .volume_id(volid)
            .size(size)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn create_ebs_snapshot(
        &self,
        volid: impl Into<String>,
        tags: &HashMap<StackString, StackString>,
    ) -> Result<Option<StackString>, Error> {
        let mut builder = self.ec2_client.create_snapshot().volume_id(volid);
        if !tags.is_empty() {
            let tags: Vec<_> = tags
                .iter()
                .map(|(k, v)| Tag::builder().key(k).value(v).build())
                .collect();
            builder = builder.tag_specifications(
                TagSpecification::builder()
                    .resource_type(ResourceType::Snapshot)
                    .set_tags(Some(tags))
                    .build(),
            );
        }
        builder
            .send()
            .await
            .map(|s| {
                s.snapshot_id.map(|snapshot_id| {
                    {
                        let ec2 = self.clone();
                        let tags = tags.clone();
                        let snapshot_id = snapshot_id.clone();
                        spawn(async move { ec2.tag_ec2_instance(&snapshot_id, &tags).await });
                    }
                    snapshot_id.into()
                })
            })
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn delete_ebs_snapshot(&self, snapid: impl Into<String>) -> Result<(), Error> {
        self.ec2_client
            .delete_snapshot()
            .snapshot_id(snapid)
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns error if aws api call fails
    pub async fn get_all_key_pairs(
        &self,
    ) -> Result<impl Iterator<Item = (StackString, StackString)>, Error> {
        self.ec2_client
            .describe_key_pairs()
            .send()
            .await
            .map(|x| {
                x.key_pairs
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|key| {
                        let fingerprint = key.key_fingerprint.unwrap_or_default();
                        key.key_name.map(|x| (x.into(), fingerprint.into()))
                    })
            })
            .map_err(Into::into)
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct InstanceRequest {
    pub ami: StackString,
    pub instance_type: StackString,
    pub key_name: StackString,
    pub security_group: StackString,
    pub script: PathBuf,
    pub tags: HashMap<StackString, StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SpotRequest {
    pub ami: StackString,
    pub instance_type: StackString,
    pub security_group: StackString,
    pub script: PathBuf,
    pub key_name: StackString,
    pub price: Option<f32>,
    pub tags: HashMap<StackString, StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AmiInfo {
    pub id: StackString,
    pub name: StackString,
    pub state: StackString,
    pub snapshot_ids: Vec<StackString>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Ec2InstanceInfo {
    pub id: StackString,
    pub dns_name: StackString,
    pub state: StackString,
    pub instance_type: StackString,
    pub availability_zone: StackString,
    pub launch_time: DateTimeWrapper,
    pub tags: HashMap<StackString, StackString>,
    pub volumes: Vec<StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReservedInstanceInfo {
    pub id: StackString,
    pub price: f32,
    pub instance_type: StackString,
    pub state: StackString,
    pub availability_zone: Option<StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct SpotInstanceRequestInfo {
    pub id: StackString,
    pub price: f32,
    pub instance_type: StackString,
    pub spot_type: StackString,
    pub status: StackString,
    pub imageid: StackString,
    pub instance_id: Option<StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    pub id: StackString,
    pub availability_zone: StackString,
    pub size: i64,
    pub iops: i64,
    pub state: StackString,
    pub tags: HashMap<StackString, StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub id: StackString,
    pub volume_size: i64,
    pub state: StackString,
    pub progress: StackString,
    pub tags: HashMap<StackString, StackString>,
}

/// # Errors
/// Return error if
pub fn get_user_data_from_script(
    default_dir: impl AsRef<Path>,
    script: impl AsRef<Path>,
) -> Result<StackString, Error> {
    let fname = if script.as_ref().exists() {
        script.as_ref().to_path_buf()
    } else {
        let fname = default_dir.as_ref().join(script);
        if !fname.exists() {
            return Ok(include_str!("../../templates/setup_aws.sh").into());
        }
        fname
    };
    read_to_string(fname).map(Into::into).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use log::debug;
    use std::path::Path;

    use crate::{
        config::Config,
        ec2_instance::{get_user_data_from_script, Ec2Instance},
    };

    #[test]
    fn test_get_user_data_from_script() -> Result<(), Error> {
        let user_data = get_user_data_from_script(
            &Path::new("/home/ddboline/.config/aws_app_rust/scripts"),
            "build_rust_repo.sh",
        )?;
        debug!("{}", user_data);
        assert!(user_data.len() > 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_instances() -> Result<(), Error> {
        let config = Config::init_config()?;
        let sdk_config = aws_config::load_from_env().await;
        let ec2 = Ec2Instance::new(&config, &sdk_config);
        let instances: Vec<_> = ec2.get_all_instances().await?.collect();

        assert!(instances.len() > 0);

        let result = ec2.get_availability_zones().await?;
        assert_eq!(result[0].as_str(), "us-east-1a");
        Ok(())
    }
}

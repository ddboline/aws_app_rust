use anyhow::{format_err, Error};
use chrono::{DateTime, Duration, Utc};
use itertools::Itertools;
use log::debug;
use maplit::hashmap;
use rusoto_core::Region;
use rusoto_ec2::{
    AttachVolumeRequest, CancelSpotInstanceRequestsRequest, CreateImageRequest,
    CreateSnapshotRequest, CreateTagsRequest, CreateVolumeRequest, DeleteSnapshotRequest,
    DeleteVolumeRequest, DeregisterImageRequest, DescribeAvailabilityZonesRequest,
    DescribeImagesRequest, DescribeInstancesRequest, DescribeKeyPairsRequest,
    DescribeRegionsRequest, DescribeReservedInstancesRequest, DescribeSnapshotsRequest,
    DescribeSpotInstanceRequestsRequest, DescribeSpotPriceHistoryRequest, DescribeVolumesRequest,
    DetachVolumeRequest, Ec2, Ec2Client, Filter, ModifyVolumeRequest, RequestSpotInstancesRequest,
    RequestSpotLaunchSpecification, RunInstancesRequest, Tag, TagSpecification,
    TerminateInstancesRequest,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{
    collections::HashMap,
    fmt,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time,
};
use sts_profile_auth::get_client_sts;
use tokio::{task::spawn, time::sleep};

use crate::config::Config;

static UBUNTU_OWNER: &str = "099720109477";

#[derive(Clone)]
pub struct Ec2Instance {
    ec2_client: Ec2Client,
    my_owner_id: Option<StackString>,
    region: Region,
    script_dir: PathBuf,
}

impl fmt::Debug for Ec2Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ec2Instance")
    }
}

impl Default for Ec2Instance {
    fn default() -> Self {
        let config = Config::new();
        Self {
            ec2_client: get_client_sts!(Ec2Client, Region::UsEast1).expect("StsProfile failed"),
            my_owner_id: config.my_owner_id.clone(),
            region: Region::UsEast1,
            script_dir: config.script_directory.clone(),
        }
    }
}

impl Ec2Instance {
    pub fn new(config: &Config) -> Self {
        let region: Region = config
            .aws_region_name
            .parse()
            .ok()
            .unwrap_or(Region::UsEast1);
        Self {
            ec2_client: get_client_sts!(Ec2Client, region.clone()).expect("StsProfile failed"),
            my_owner_id: config.my_owner_id.clone(),
            region,
            script_dir: config.script_directory.clone(),
        }
    }

    pub fn set_region(&mut self, region: impl AsRef<str>) -> Result<(), Error> {
        self.region = region.as_ref().parse()?;
        self.ec2_client = get_client_sts!(Ec2Client, self.region.clone())?;
        Ok(())
    }

    pub fn set_owner_id(&mut self, owner_id: impl Into<StackString>) -> Option<StackString> {
        self.my_owner_id.replace(owner_id.into())
    }

    pub async fn get_ami_tags(&self) -> Result<impl Iterator<Item = AmiInfo>, Error> {
        let owner_id = self
            .my_owner_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| format_err!("No owner id"))?;
        let req = DescribeImagesRequest {
            filters: Some(vec![Filter {
                name: Some("owner-id".to_string()),
                values: Some(vec![owner_id]),
            }]),
            ..DescribeImagesRequest::default()
        };
        self.ec2_client
            .describe_images(req)
            .await
            .map(|l| {
                l.images
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|image| {
                        Some(AmiInfo {
                            id: image.image_id?.into(),
                            name: image.name?.into(),
                            state: image.state?.into(),
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

    pub async fn get_latest_ubuntu_ami(
        &self,
        ubuntu_release: impl fmt::Display,
        arch: impl fmt::Display,
    ) -> Result<Option<AmiInfo>, Error> {
        let request = DescribeImagesRequest {
            filters: Some(vec![
                Filter {
                    name: Some("owner-id".to_string()),
                    values: Some(vec![UBUNTU_OWNER.to_string()]),
                },
                Filter {
                    name: Some("name".to_string()),
                    values: Some(vec![format!(
                        "ubuntu/images/hvm-ssd/ubuntu-{}-{}-server*",
                        ubuntu_release, arch,
                    )]),
                },
            ]),
            ..DescribeImagesRequest::default()
        };
        let req = self.ec2_client.describe_images(request).await?;

        let image = req
            .images
            .unwrap_or_else(Vec::new)
            .into_iter()
            .filter_map(|image| {
                Some(AmiInfo {
                    id: image.image_id?.into(),
                    name: image.name?.into(),
                    state: image.state?.into(),
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

    pub async fn get_all_regions(&self) -> Result<HashMap<StackString, StackString>, Error> {
        self.ec2_client
            .describe_regions(DescribeRegionsRequest::default())
            .await
            .map(|r| {
                r.regions
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|region| {
                        Some((region.region_name?.into(), region.opt_in_status?.into()))
                    })
                    .collect()
            })
            .map_err(Into::into)
    }

    pub async fn get_all_instances(&self) -> Result<impl Iterator<Item = Ec2InstanceInfo>, Error> {
        self.ec2_client
            .describe_instances(DescribeInstancesRequest::default())
            .await
            .map(|instances| {
                instances
                    .reservations
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|res| {
                        res.instances.map(|instances| {
                            instances.into_iter().filter_map(|inst| {
                                let tags: HashMap<_, _> = inst
                                    .tags
                                    .unwrap_or_else(Vec::new)
                                    .into_iter()
                                    .filter_map(|tag| Some((tag.key?.into(), tag.value?.into())))
                                    .collect();
                                let volumes = inst
                                    .block_device_mappings
                                    .unwrap_or_else(Vec::new)
                                    .into_iter()
                                    .filter_map(|bm| {
                                        let ebs = bm.ebs?.volume_id?;
                                        Some(ebs.into())
                                    })
                                    .collect();
                                Some(Ec2InstanceInfo {
                                    id: inst.instance_id?.into(),
                                    dns_name: inst.public_dns_name?.into(),
                                    state: inst.state?.name?.into(),
                                    instance_type: inst.instance_type?.into(),
                                    availability_zone: inst.placement?.availability_zone?.into(),
                                    launch_time: inst
                                        .launch_time
                                        .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
                                        .map(|t| t.with_timezone(&Utc))?,
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

    pub async fn get_reserved_instances(
        &self,
    ) -> Result<impl Iterator<Item = ReservedInstanceInfo>, Error> {
        self.ec2_client
            .describe_reserved_instances(DescribeReservedInstancesRequest::default())
            .await
            .map(|res| {
                res.reserved_instances
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|inst| {
                        let state = inst.state.as_ref()?;
                        if state == "retired" {
                            return None;
                        }
                        Some(ReservedInstanceInfo {
                            id: inst.reserved_instances_id?.into(),
                            price: inst.fixed_price?,
                            instance_type: inst.instance_type?.into(),
                            state: inst.state?.into(),
                            availability_zone: inst.availability_zone.map(Into::into),
                        })
                    })
            })
            .map_err(Into::into)
    }

    pub async fn get_availability_zones(&self) -> Result<Vec<String>, Error> {
        let req = DescribeAvailabilityZonesRequest {
            filters: Some(vec![Filter {
                name: Some("region-name".into()),
                values: Some(vec![self.region.name().into()]),
            }]),
            ..DescribeAvailabilityZonesRequest::default()
        };
        let zones = self
            .ec2_client
            .describe_availability_zones(req)
            .await?
            .availability_zones
            .unwrap_or_else(Vec::new)
            .into_iter()
            .filter_map(|zone| zone.zone_name)
            .collect();
        Ok(zones)
    }

    pub async fn get_latest_spot_inst_prices(
        &self,
        inst_list: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<HashMap<StackString, f32>, Error> {
        let inst_list: Vec<_> = inst_list.into_iter().map(|x| x.as_ref().into()).collect();
        let zones = self.get_availability_zones().await?;
        let filters = vec![
            Filter {
                name: Some("product-description".to_string()),
                values: Some(vec!["Linux/UNIX".to_string()]),
            },
            Filter {
                name: Some("availability-zone".to_string()),
                values: Some(zones),
            },
        ];
        let start_time = Utc::now() - Duration::hours(4);
        self.ec2_client
            .describe_spot_price_history(DescribeSpotPriceHistoryRequest {
                start_time: Some(start_time.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                filters: Some(filters),
                instance_types: if inst_list.is_empty() {
                    None
                } else {
                    Some(inst_list)
                },
                ..DescribeSpotPriceHistoryRequest::default()
            })
            .await
            .map(|spot_hist| {
                spot_hist
                    .spot_price_history
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|spot_price| {
                        Some((
                            spot_price.instance_type?.into(),
                            spot_price.spot_price.and_then(|s| s.parse().ok())?,
                        ))
                    })
                    .collect()
            })
            .map_err(Into::into)
    }

    pub async fn get_spot_instance_requests(
        &self,
    ) -> Result<impl Iterator<Item = SpotInstanceRequestInfo>, Error> {
        self.ec2_client
            .describe_spot_instance_requests(DescribeSpotInstanceRequestsRequest::default())
            .await
            .map(|s| {
                s.spot_instance_requests
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|inst| {
                        let launch_spec = inst.launch_specification?;
                        Some(SpotInstanceRequestInfo {
                            id: inst.spot_instance_request_id?.into(),
                            price: inst
                                .spot_price
                                .and_then(|s| s.parse::<f32>().ok())
                                .unwrap_or(0.0),
                            instance_type: launch_spec.instance_type?.into(),
                            spot_type: inst.type_?.into(),
                            status: inst.status?.code?.into(),
                            imageid: launch_spec.image_id?.into(),
                            instance_id: inst.instance_id.map(Into::into),
                        })
                    })
            })
            .map_err(Into::into)
    }

    pub async fn get_all_volumes(&self) -> Result<impl Iterator<Item = VolumeInfo>, Error> {
        self.ec2_client
            .describe_volumes(DescribeVolumesRequest::default())
            .await
            .map(|v| {
                v.volumes
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|v| {
                        Some(VolumeInfo {
                            id: v.volume_id?.into(),
                            availability_zone: v.availability_zone?.into(),
                            size: v.size?,
                            iops: v.iops.unwrap_or(0),
                            state: v.state?.into(),
                            tags: v
                                .tags
                                .unwrap_or_else(Vec::new)
                                .into_iter()
                                .filter_map(|t| Some((t.key?.into(), t.value?.into())))
                                .collect(),
                        })
                    })
            })
            .map_err(Into::into)
    }

    pub async fn get_all_snapshots(&self) -> Result<impl Iterator<Item = SnapshotInfo>, Error> {
        let owner_id = self
            .my_owner_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| format_err!("No owner id"))?;

        self.ec2_client
            .describe_snapshots(DescribeSnapshotsRequest {
                filters: Some(vec![Filter {
                    name: Some("owner-id".to_string()),
                    values: Some(vec![owner_id]),
                }]),
                ..DescribeSnapshotsRequest::default()
            })
            .await
            .map(|s| {
                s.snapshots
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|snap| {
                        Some(SnapshotInfo {
                            id: snap.snapshot_id?.into(),
                            volume_size: snap.volume_size?,
                            state: snap.state?.into(),
                            progress: snap.progress?.into(),
                            tags: snap
                                .tags
                                .unwrap_or_else(Vec::new)
                                .into_iter()
                                .filter_map(|t| Some((t.key?.into(), t.value?.into())))
                                .collect(),
                        })
                    })
            })
            .map_err(Into::into)
    }

    pub async fn terminate_instance(
        &self,
        instance_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), Error> {
        let instance_ids = instance_ids
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        self.ec2_client
            .terminate_instances(TerminateInstancesRequest {
                instance_ids,
                ..TerminateInstancesRequest::default()
            })
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn request_spot_instance(&self, spot: &SpotRequest) -> Result<Vec<String>, Error> {
        let user_data = get_user_data_from_script(&self.script_dir, &spot.script)?;

        let req = self
            .ec2_client
            .request_spot_instances(RequestSpotInstancesRequest {
                spot_price: spot.price.map(|x| x.to_string()),
                instance_count: Some(1),
                launch_specification: Some(RequestSpotLaunchSpecification {
                    image_id: Some(spot.ami.to_string()),
                    instance_type: Some(spot.instance_type.to_string()),
                    security_group_ids: Some(vec![spot.security_group.to_string()]),
                    user_data: Some(base64::encode(&user_data)),
                    key_name: Some(spot.key_name.to_string()),
                    ..RequestSpotLaunchSpecification::default()
                }),
                ..RequestSpotInstancesRequest::default()
            })
            .await?;
        let spot_ids = req
            .spot_instance_requests
            .unwrap_or_else(Vec::new)
            .into_iter()
            .filter_map(|result| result.spot_instance_request_id)
            .collect();
        Ok(spot_ids)
    }

    pub async fn tag_ami_snapshot(
        &self,
        inst_id: impl AsRef<str>,
        name: impl Into<StackString>,
        iterations: usize,
    ) -> Result<(), Error> {
        sleep(time::Duration::from_secs(2)).await;
        let inst_id = inst_id.as_ref();
        for i in 0..iterations {
            if let Some(ami) = self
                .get_ami_tags()
                .await?
                .find(|ami| ami.id == inst_id || ami.name == inst_id)
            {
                if let Some(snapshot_id) = ami.snapshot_ids.get(0) {
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
            sleep(time::Duration::from_secs(secs)).await;
        }
        Ok(())
    }

    pub async fn tag_spot_instance(
        &self,
        spot_instance_request_id: &str,
        tags: &HashMap<StackString, StackString>,
        iterations: usize,
    ) -> Result<(), Error> {
        sleep(time::Duration::from_secs(2)).await;
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
            sleep(time::Duration::from_secs(secs)).await;
        }
        Ok(())
    }

    pub async fn cancel_spot_instance_request(
        &self,
        inst_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), Error> {
        let inst_ids = inst_ids
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        self.ec2_client
            .cancel_spot_instance_requests(CancelSpotInstanceRequestsRequest {
                spot_instance_request_ids: inst_ids,
                ..CancelSpotInstanceRequestsRequest::default()
            })
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn tag_ec2_instance(
        &self,
        inst_id: impl Into<String>,
        tags: &HashMap<StackString, StackString>,
    ) -> Result<(), Error> {
        self.ec2_client
            .create_tags(CreateTagsRequest {
                resources: vec![inst_id.into()],
                tags: tags
                    .iter()
                    .map(|(k, v)| Tag {
                        key: Some(k.to_string()),
                        value: Some(v.to_string()),
                    })
                    .collect(),
                ..CreateTagsRequest::default()
            })
            .await
            .map_err(Into::into)
    }

    pub async fn run_ec2_instance(&self, request: &InstanceRequest) -> Result<(), Error> {
        let user_data = get_user_data_from_script(&self.script_dir, &request.script)?;

        let req = self
            .ec2_client
            .run_instances(RunInstancesRequest {
                image_id: Some(request.ami.to_string()),
                instance_type: Some(request.instance_type.to_string()),
                min_count: 1,
                max_count: 1,
                key_name: Some(request.key_name.to_string()),
                security_group_ids: Some(vec![request.security_group.to_string()]),
                user_data: Some(base64::encode(&user_data)),
                ..RunInstancesRequest::default()
            })
            .await?;

        for inst in req.instances.unwrap_or_else(Vec::new) {
            if let Some(inst) = inst.instance_id {
                self.tag_ec2_instance(&inst, &request.tags).await?;
            }
        }
        Ok(())
    }

    pub async fn create_image(
        &self,
        inst_id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Option<StackString>, Error> {
        let name = name.into();
        self.ec2_client
            .create_image(CreateImageRequest {
                instance_id: inst_id.into(),
                name: name.clone(),
                ..CreateImageRequest::default()
            })
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

    pub async fn delete_image(&self, ami: impl Into<String>) -> Result<(), Error> {
        self.ec2_client
            .deregister_image(DeregisterImageRequest {
                image_id: ami.into(),
                ..DeregisterImageRequest::default()
            })
            .await
            .map_err(Into::into)
    }

    pub async fn create_ebs_volume(
        &self,
        zoneid: impl Into<String>,
        size: Option<i64>,
        snapid: Option<impl AsRef<str>>,
    ) -> Result<Option<StackString>, Error> {
        let snapid = snapid.as_ref().map(|s| s.as_ref().to_string());
        self.ec2_client
            .create_volume(CreateVolumeRequest {
                availability_zone: zoneid.into(),
                size,
                snapshot_id: snapid,
                volume_type: Some("standard".to_string()),
                ..CreateVolumeRequest::default()
            })
            .await
            .map(|v| v.volume_id.map(Into::into))
            .map_err(Into::into)
    }

    pub async fn delete_ebs_volume(&self, volid: impl Into<String>) -> Result<(), Error> {
        self.ec2_client
            .delete_volume(DeleteVolumeRequest {
                volume_id: volid.into(),
                ..DeleteVolumeRequest::default()
            })
            .await
            .map_err(Into::into)
    }

    pub async fn attach_ebs_volume(
        &self,
        volid: impl Into<String>,
        instid: impl Into<String>,
        device: impl Into<String>,
    ) -> Result<(), Error> {
        self.ec2_client
            .attach_volume(AttachVolumeRequest {
                device: device.into(),
                instance_id: instid.into(),
                volume_id: volid.into(),
                ..AttachVolumeRequest::default()
            })
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn detach_ebs_volume(&self, volid: impl Into<String>) -> Result<(), Error> {
        self.ec2_client
            .detach_volume(DetachVolumeRequest {
                volume_id: volid.into(),
                ..DetachVolumeRequest::default()
            })
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn modify_ebs_volume(
        &self,
        volid: impl Into<String>,
        size: i64,
    ) -> Result<(), Error> {
        self.ec2_client
            .modify_volume(ModifyVolumeRequest {
                volume_id: volid.into(),
                size: Some(size),
                ..ModifyVolumeRequest::default()
            })
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn create_ebs_snapshot(
        &self,
        volid: impl Into<String>,
        tags: &HashMap<StackString, StackString>,
    ) -> Result<Option<StackString>, Error> {
        self.ec2_client
            .create_snapshot(CreateSnapshotRequest {
                volume_id: volid.into(),
                tag_specifications: if tags.is_empty() {
                    None
                } else {
                    Some(vec![TagSpecification {
                        resource_type: Some("snapshot".to_string()),
                        tags: Some(
                            tags.iter()
                                .map(|(k, v)| Tag {
                                    key: Some(k.to_string()),
                                    value: Some(v.to_string()),
                                })
                                .collect(),
                        ),
                    }])
                },
                ..CreateSnapshotRequest::default()
            })
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

    pub async fn delete_ebs_snapshot(&self, snapid: impl Into<String>) -> Result<(), Error> {
        self.ec2_client
            .delete_snapshot(DeleteSnapshotRequest {
                snapshot_id: snapid.into(),
                ..DeleteSnapshotRequest::default()
            })
            .await
            .map_err(Into::into)
    }

    pub async fn get_all_key_pairs(
        &self,
    ) -> Result<impl Iterator<Item = (StackString, StackString)>, Error> {
        self.ec2_client
            .describe_key_pairs(DescribeKeyPairsRequest::default())
            .await
            .map(|x| {
                x.key_pairs
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|key| {
                        let fingerprint = key.key_fingerprint.unwrap_or_else(|| "".to_string());
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
    pub script: StackString,
    pub tags: HashMap<StackString, StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SpotRequest {
    pub ami: StackString,
    pub instance_type: StackString,
    pub security_group: StackString,
    pub script: StackString,
    pub key_name: StackString,
    pub price: Option<f32>,
    pub tags: HashMap<StackString, StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AmiInfo {
    pub id: StackString,
    pub name: StackString,
    pub state: StackString,
    pub snapshot_ids: Vec<StackString>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Ec2InstanceInfo {
    pub id: StackString,
    pub dns_name: StackString,
    pub state: StackString,
    pub instance_type: StackString,
    pub availability_zone: StackString,
    pub launch_time: DateTime<Utc>,
    pub tags: HashMap<StackString, StackString>,
    pub volumes: Vec<StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ReservedInstanceInfo {
    pub id: StackString,
    pub price: f32,
    pub instance_type: StackString,
    pub state: StackString,
    pub availability_zone: Option<StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SpotInstanceRequestInfo {
    pub id: StackString,
    pub price: f32,
    pub instance_type: StackString,
    pub spot_type: StackString,
    pub status: StackString,
    pub imageid: StackString,
    pub instance_id: Option<StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct VolumeInfo {
    pub id: StackString,
    pub availability_zone: StackString,
    pub size: i64,
    pub iops: i64,
    pub state: StackString,
    pub tags: HashMap<StackString, StackString>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SnapshotInfo {
    pub id: StackString,
    pub volume_size: i64,
    pub state: StackString,
    pub progress: StackString,
    pub tags: HashMap<StackString, StackString>,
}

pub fn get_user_data_from_script(
    default_dir: impl AsRef<Path>,
    script: impl AsRef<Path>,
) -> Result<StackString, Error> {
    let fname = if Path::new(script.as_ref()).exists() {
        Path::new(script.as_ref()).to_path_buf()
    } else {
        let fname = default_dir.as_ref().join(script);
        if !fname.exists() {
            return Ok(include_str!("../../templates/setup_aws.sh").into());
        }
        fname
    };
    let mut user_data = String::new();
    File::open(fname)?.read_to_string(&mut user_data)?;
    Ok(user_data.into())
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
        let ec2 = Ec2Instance::new(&config);
        let instances: Vec<_> = ec2.get_all_instances().await?.collect();

        assert!(instances.len() > 0);

        let result = ec2.get_availability_zones().await?;
        assert_eq!(result[0].as_str(), "us-east-1a");
        Ok(())
    }
}

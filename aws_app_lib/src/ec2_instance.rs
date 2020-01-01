use chrono::{DateTime, Duration, Utc};
use failure::{err_msg, Error};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rusoto_core::Region;
use rusoto_ec2::{
    AttachVolumeRequest, CancelSpotInstanceRequestsRequest, CreateImageRequest,
    CreateSnapshotRequest, CreateTagsRequest, CreateVolumeRequest, DeleteSnapshotRequest,
    DeleteVolumeRequest, DeregisterImageRequest, DescribeImagesRequest, DescribeInstancesRequest,
    DescribeKeyPairsRequest, DescribeRegionsRequest, DescribeReservedInstancesRequest,
    DescribeSnapshotsRequest, DescribeSpotInstanceRequestsRequest, DescribeSpotPriceHistoryRequest,
    DescribeVolumesRequest, DetachVolumeRequest, Ec2, Ec2Client, Filter, ModifyVolumeRequest,
    RequestSpotInstancesRequest, RequestSpotLaunchSpecification, RunInstancesRequest, Tag,
    TagSpecification, TerminateInstancesRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::thread::sleep;
use std::time;

use crate::config::Config;
use crate::sts_instance::StsInstance;

macro_rules! some {
    ($expr : expr) => {
        if let Some(v) = $expr {
            v
        } else {
            return None;
        }
    };
}

#[derive(Clone)]
pub struct Ec2Instance {
    sts: StsInstance,
    ec2_client: Ec2Client,
    my_owner_id: Option<String>,
    region: Region,
    script_dir: String,
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
            sts: StsInstance::default(),
            ec2_client: Ec2Client::new(Region::UsEast1),
            my_owner_id: config.my_owner_id.clone(),
            region: Region::UsEast1,
            script_dir: config.script_directory.clone(),
        }
    }
}

impl Ec2Instance {
    pub fn new(config: Config) -> Self {
        let region: Region = config
            .aws_region_name
            .parse()
            .ok()
            .unwrap_or(Region::UsEast1);
        let sts = StsInstance::new(None).unwrap();
        Self {
            ec2_client: sts.get_ec2_client(region.clone()).unwrap(),
            my_owner_id: config.my_owner_id.clone(),
            region,
            script_dir: config.script_directory.clone(),
            sts,
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.region = region.parse()?;
        self.ec2_client = self.sts.get_ec2_client(self.region.clone())?;
        Ok(())
    }

    pub fn set_owner_id(&mut self, owner_id: &str) -> Option<String> {
        self.my_owner_id.replace(owner_id.to_string())
    }

    pub fn get_ami_tags(&self) -> Result<Vec<AmiInfo>, Error> {
        let owner_id = match self.my_owner_id.as_ref() {
            Some(x) => x.to_string(),
            None => return Ok(Vec::new()),
        };
        self.ec2_client
            .describe_images(DescribeImagesRequest {
                filters: Some(vec![Filter {
                    name: Some("owner-id".to_string()),
                    values: Some(vec![owner_id]),
                }]),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|l| {
                l.images
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|image| {
                        Some(AmiInfo {
                            id: some!(image.image_id),
                            name: some!(image.name),
                            state: some!(image.state),
                            snapshot_ids: some!(image.block_device_mappings)
                                .into_par_iter()
                                .filter_map(|block| block.ebs.and_then(|b| b.snapshot_id))
                                .collect(),
                        })
                    })
                    .collect()
            })
    }

    pub fn get_latest_ubuntu_ami(&self, ubuntu_release: &str) -> Result<Vec<AmiInfo>, Error> {
        let ubuntu_owner = "099720109477".to_string();
        self.ec2_client
            .describe_images(DescribeImagesRequest {
                filters: Some(vec![
                    Filter {
                        name: Some("owner-id".to_string()),
                        values: Some(vec![ubuntu_owner]),
                    },
                    Filter {
                        name: Some("name".to_string()),
                        values: Some(vec![format!(
                            "ubuntu/images/hvm-ssd/ubuntu-{}-amd64-server*",
                            ubuntu_release
                        )]),
                    },
                ]),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|l| {
                l.images
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|image| {
                        Some(AmiInfo {
                            id: some!(image.image_id),
                            name: some!(image.name),
                            state: some!(image.state),
                            snapshot_ids: some!(image.block_device_mappings)
                                .into_par_iter()
                                .filter_map(|block| block.ebs.and_then(|b| b.snapshot_id))
                                .collect(),
                        })
                    })
                    .collect()
            })
    }

    pub fn get_ami_map(&self) -> Result<HashMap<String, String>, Error> {
        Ok(self
            .get_ami_tags()?
            .into_par_iter()
            .map(|ami| (ami.name, ami.id))
            .collect())
    }

    pub fn get_all_regions(&self) -> Result<HashMap<String, String>, Error> {
        self.ec2_client
            .describe_regions(DescribeRegionsRequest::default())
            .sync()
            .map_err(err_msg)
            .map(|r| {
                r.regions
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|region| {
                        Some((some!(region.region_name), some!(region.opt_in_status)))
                    })
                    .collect()
            })
    }

    pub fn get_all_instances(&self) -> Result<Vec<Ec2InstanceInfo>, Error> {
        self.ec2_client
            .describe_instances(DescribeInstancesRequest::default())
            .sync()
            .map_err(err_msg)
            .map(|instances| {
                instances
                    .reservations
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|res| {
                        res.instances.map(|instances| {
                            instances
                                .into_par_iter()
                                .filter_map(|inst| {
                                    let tags: HashMap<String, String> = inst
                                        .tags
                                        .unwrap_or_else(Vec::new)
                                        .into_par_iter()
                                        .filter_map(|tag| Some((some!(tag.key), some!(tag.value))))
                                        .collect();
                                    Some(Ec2InstanceInfo {
                                        id: some!(inst.instance_id),
                                        dns_name: some!(inst.public_dns_name),
                                        state: some!(some!(inst.state).name),
                                        instance_type: some!(inst.instance_type),
                                        availability_zone: some!(
                                            some!(inst.placement).availability_zone
                                        ),
                                        launch_time: some!(inst
                                            .launch_time
                                            .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
                                            .map(|t| t.with_timezone(&Utc))),
                                        tags,
                                    })
                                })
                                .collect::<Vec<Ec2InstanceInfo>>()
                        })
                    })
                    .flatten()
                    .collect::<Vec<Ec2InstanceInfo>>()
            })
    }

    pub fn get_reserved_instances(&self) -> Result<Vec<ReservedInstanceInfo>, Error> {
        self.ec2_client
            .describe_reserved_instances(DescribeReservedInstancesRequest::default())
            .sync()
            .map_err(err_msg)
            .map(|res| {
                res.reserved_instances
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|inst| {
                        let state = some!(inst.state.as_ref());
                        if state == "retired" {
                            return None;
                        }
                        Some(ReservedInstanceInfo {
                            id: some!(inst.reserved_instances_id),
                            price: some!(inst.fixed_price),
                            instance_type: some!(inst.instance_type),
                            state: some!(inst.state),
                            availability_zone: inst
                                .availability_zone
                                .unwrap_or_else(|| "".to_string()),
                        })
                    })
                    .collect()
            })
    }

    pub fn get_latest_spot_inst_prices(
        &self,
        inst_list: &[String],
    ) -> Result<HashMap<String, f32>, Error> {
        let filters = vec![
            Filter {
                name: Some("product-description".to_string()),
                values: Some(vec!["Linux/UNIX".to_string()]),
            },
            Filter {
                name: Some("availability-zone".to_string()),
                values: Some(vec![
                    "us-east-1a".to_string(),
                    "us-east-1b".to_string(),
                    "us-east-1c".to_string(),
                    "us-east-1d".to_string(),
                    "us-east-1e".to_string(),
                    "us-east-1f".to_string(),
                ]),
            },
        ];
        let start_time = Utc::now() - Duration::hours(4);
        self.ec2_client
            .describe_spot_price_history(DescribeSpotPriceHistoryRequest {
                start_time: Some(start_time.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                filters: Some(filters),
                instance_types: if !inst_list.is_empty() {
                    Some(inst_list.to_vec())
                } else {
                    None
                },
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|spot_hist| {
                spot_hist
                    .spot_price_history
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|spot_price| {
                        Some((
                            some!(spot_price.instance_type),
                            some!(spot_price.spot_price.and_then(|s| { s.parse().ok() })),
                        ))
                    })
                    .collect()
            })
    }

    pub fn get_spot_instance_requests(&self) -> Result<Vec<SpotInstanceRequestInfo>, Error> {
        self.ec2_client
            .describe_spot_instance_requests(DescribeSpotInstanceRequestsRequest::default())
            .sync()
            .map_err(err_msg)
            .map(|s| {
                s.spot_instance_requests
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|inst| {
                        let launch_spec = some!(inst.launch_specification);
                        Some(SpotInstanceRequestInfo {
                            id: some!(inst.spot_instance_request_id),
                            price: inst
                                .spot_price
                                .and_then(|s| s.parse::<f32>().ok())
                                .unwrap_or(0.0),
                            instance_type: launch_spec
                                .instance_type
                                .unwrap_or_else(|| "".to_string()),
                            spot_type: inst.type_.unwrap_or_else(|| "".to_string()),
                            status: match inst.status {
                                Some(s) => s.code.unwrap_or_else(|| "".to_string()),
                                None => "".to_string(),
                            },
                            imageid: launch_spec.image_id.unwrap_or_else(|| "".to_string()),
                            instance_id: inst.instance_id,
                        })
                    })
                    .collect()
            })
    }

    pub fn get_all_volumes(&self) -> Result<Vec<VolumeInfo>, Error> {
        self.ec2_client
            .describe_volumes(DescribeVolumesRequest::default())
            .sync()
            .map_err(err_msg)
            .map(|v| {
                v.volumes
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|v| {
                        Some(VolumeInfo {
                            id: some!(v.volume_id),
                            availability_zone: some!(v.availability_zone),
                            size: some!(v.size),
                            iops: some!(v.iops),
                            state: some!(v.state),
                            tags: v
                                .tags
                                .unwrap_or_else(Vec::new)
                                .into_par_iter()
                                .filter_map(|t| Some((some!(t.key), some!(t.value))))
                                .collect(),
                        })
                    })
                    .collect()
            })
    }

    pub fn get_all_snapshots(&self) -> Result<Vec<SnapshotInfo>, Error> {
        let owner_id = match self.my_owner_id.as_ref() {
            Some(x) => x.to_string(),
            None => return Ok(Vec::new()),
        };

        self.ec2_client
            .describe_snapshots(DescribeSnapshotsRequest {
                filters: Some(vec![Filter {
                    name: Some("owner-id".to_string()),
                    values: Some(vec![owner_id]),
                }]),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|s| {
                s.snapshots
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|snap| {
                        Some(SnapshotInfo {
                            id: some!(snap.snapshot_id),
                            volume_size: some!(snap.volume_size),
                            state: some!(snap.state),
                            progress: some!(snap.progress),
                            tags: snap
                                .tags
                                .unwrap_or_else(Vec::new)
                                .into_par_iter()
                                .filter_map(|t| Some((some!(t.key), some!(t.value))))
                                .collect(),
                        })
                    })
                    .collect()
            })
    }

    pub fn terminate_instance(&self, instance_ids: &[String]) -> Result<(), Error> {
        self.ec2_client
            .terminate_instances(TerminateInstancesRequest {
                instance_ids: instance_ids.to_vec(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn request_spot_instance(&self, spot: &SpotRequest) -> Result<(), Error> {
        let user_data = get_user_data_from_script(&self.script_dir, &spot.script)?;

        self.ec2_client
            .request_spot_instances(RequestSpotInstancesRequest {
                spot_price: Some(spot.price.to_string()),
                instance_count: Some(1),
                launch_specification: Some(RequestSpotLaunchSpecification {
                    image_id: Some(spot.ami.to_string()),
                    instance_type: Some(spot.instance_type.to_string()),
                    security_group_ids: Some(vec![spot.security_group.to_string()]),
                    user_data: Some(base64::encode(&user_data)),
                    key_name: Some(spot.key_name.to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .and_then(|req| {
                if spot.tags.is_empty() {
                    return Ok(());
                }
                req.spot_instance_requests
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .map(|result| {
                        if let Some(spot_instance_request_id) = result.spot_instance_request_id {
                            for _ in 0..20 {
                                let reqs: HashMap<_, _> = self
                                    .get_spot_instance_requests()?
                                    .into_par_iter()
                                    .map(|r| (r.id, r.instance_id))
                                    .collect();
                                if let Some(Some(instance_id)) = reqs.get(&spot_instance_request_id)
                                {
                                    println!("tag {} with {:?}", instance_id, spot.tags);
                                    self.tag_ec2_instance(&instance_id, &spot.tags)?;
                                    break;
                                }
                                sleep(time::Duration::from_secs(5));
                            }
                            Ok(())
                        } else {
                            Ok(())
                        }
                    })
                    .collect()
            })
    }

    pub fn cancel_spot_instance_request(&self, inst_ids: &[String]) -> Result<(), Error> {
        self.ec2_client
            .cancel_spot_instance_requests(CancelSpotInstanceRequestsRequest {
                spot_instance_request_ids: inst_ids.to_vec(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn tag_ec2_instance(
        &self,
        inst_id: &str,
        tags: &HashMap<String, String>,
    ) -> Result<(), Error> {
        self.ec2_client
            .create_tags(CreateTagsRequest {
                resources: vec![inst_id.to_string()],
                tags: tags
                    .par_iter()
                    .map(|(k, v)| Tag {
                        key: Some(k.to_string()),
                        value: Some(v.to_string()),
                    })
                    .collect(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
    }

    pub fn run_ec2_instance(&self, request: &InstanceRequest) -> Result<(), Error> {
        let user_data = get_user_data_from_script(&self.script_dir, &request.script)?;

        self.ec2_client
            .run_instances(RunInstancesRequest {
                image_id: Some(request.ami.to_string()),
                instance_type: Some(request.instance_type.to_string()),
                min_count: 1,
                max_count: 1,
                key_name: Some(request.key_name.to_string()),
                security_group_ids: Some(vec![request.security_group.to_string()]),
                user_data: Some(base64::encode(&user_data)),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .and_then(|req| {
                req.instances
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|inst| inst.instance_id)
                    .map(|inst| self.tag_ec2_instance(&inst, &request.tags))
                    .collect()
            })
    }

    pub fn create_image(&self, inst_id: &str, name: &str) -> Result<Option<String>, Error> {
        self.ec2_client
            .create_image(CreateImageRequest {
                instance_id: inst_id.to_string(),
                name: name.to_string(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|r| r.image_id)
    }

    pub fn delete_image(&self, ami: &str) -> Result<(), Error> {
        self.ec2_client
            .deregister_image(DeregisterImageRequest {
                image_id: ami.to_string(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
    }

    pub fn create_ebs_volume(
        &self,
        zoneid: &str,
        size: Option<i64>,
        snapid: Option<String>,
    ) -> Result<Option<String>, Error> {
        self.ec2_client
            .create_volume(CreateVolumeRequest {
                availability_zone: zoneid.to_string(),
                size,
                snapshot_id: snapid,
                volume_type: Some("standard".to_string()),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|v| v.volume_id)
    }

    pub fn delete_ebs_volume(&self, volid: &str) -> Result<(), Error> {
        self.ec2_client
            .delete_volume(DeleteVolumeRequest {
                volume_id: volid.to_string(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
    }

    pub fn attach_ebs_volume(&self, volid: &str, instid: &str, device: &str) -> Result<(), Error> {
        self.ec2_client
            .attach_volume(AttachVolumeRequest {
                device: device.to_string(),
                instance_id: instid.to_string(),
                volume_id: volid.to_string(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn detach_ebs_volume(&self, volid: &str) -> Result<(), Error> {
        self.ec2_client
            .detach_volume(DetachVolumeRequest {
                volume_id: volid.to_string(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn modify_ebs_volume(&self, volid: &str, size: i64) -> Result<(), Error> {
        self.ec2_client
            .modify_volume(ModifyVolumeRequest {
                volume_id: volid.to_string(),
                size: Some(size),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn create_ebs_snapshot(
        &self,
        volid: &str,
        tags: &HashMap<String, String>,
    ) -> Result<Option<String>, Error> {
        self.ec2_client
            .create_snapshot(CreateSnapshotRequest {
                volume_id: volid.to_string(),
                tag_specifications: if tags.is_empty() {
                    None
                } else {
                    Some(vec![TagSpecification {
                        resource_type: Some("snapshot".to_string()),
                        tags: Some(
                            tags.par_iter()
                                .map(|(k, v)| Tag {
                                    key: Some(k.to_string()),
                                    value: Some(v.to_string()),
                                })
                                .collect(),
                        ),
                    }])
                },
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|s| s.snapshot_id)
    }

    pub fn delete_ebs_snapshot(&self, snapid: &str) -> Result<(), Error> {
        self.ec2_client
            .delete_snapshot(DeleteSnapshotRequest {
                snapshot_id: snapid.to_string(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
    }

    pub fn get_all_key_pairs(&self) -> Result<Vec<(String, String)>, Error> {
        self.ec2_client
            .describe_key_pairs(DescribeKeyPairsRequest::default())
            .sync()
            .map_err(err_msg)
            .map(|x| {
                x.key_pairs
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|key| {
                        let fingerprint = key.key_fingerprint.unwrap_or_else(|| "".to_string());
                        key.key_name.map(|x| (x, fingerprint))
                    })
                    .collect()
            })
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct InstanceRequest {
    pub ami: String,
    pub instance_type: String,
    pub key_name: String,
    pub security_group: String,
    pub script: String,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SpotRequest {
    pub ami: String,
    pub instance_type: String,
    pub security_group: String,
    pub script: String,
    pub key_name: String,
    pub price: f32,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AmiInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub snapshot_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Ec2InstanceInfo {
    pub id: String,
    pub dns_name: String,
    pub state: String,
    pub instance_type: String,
    pub availability_zone: String,
    pub launch_time: DateTime<Utc>,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ReservedInstanceInfo {
    pub id: String,
    pub price: f32,
    pub instance_type: String,
    pub state: String,
    pub availability_zone: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SpotInstanceRequestInfo {
    pub id: String,
    pub price: f32,
    pub instance_type: String,
    pub spot_type: String,
    pub status: String,
    pub imageid: String,
    pub instance_id: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct VolumeInfo {
    pub id: String,
    pub availability_zone: String,
    pub size: i64,
    pub iops: i64,
    pub state: String,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SnapshotInfo {
    pub id: String,
    pub volume_size: i64,
    pub state: String,
    pub progress: String,
    pub tags: HashMap<String, String>,
}

pub fn get_user_data_from_script(default_dir: &str, script: &str) -> Result<String, Error> {
    let fname = if !Path::new(script).exists() {
        let fname = format!("{}/{}", default_dir, script);
        if !Path::new(&fname).exists() {
            return Ok(include_str!("../../templates/setup_aws.sh").to_string());
        }
        fname
    } else {
        script.to_string()
    };
    let mut user_data = String::new();
    File::open(fname)?.read_to_string(&mut user_data)?;
    Ok(user_data)
}

#[cfg(test)]
mod tests {
    use crate::ec2_instance::get_user_data_from_script;

    #[test]
    fn test_get_user_data_from_script() {
        let user_data = get_user_data_from_script(
            "/home/ddboline/.config/aws_app_rust/scripts",
            "build_rust_repo.sh",
        )
        .unwrap();
        println!("{}", user_data);
        assert!(user_data.len() > 0);
    }
}
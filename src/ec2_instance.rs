use chrono::{Duration, Utc};
use failure::{err_msg, Error};
use rusoto_core::Region;
use rusoto_ec2::{
    DescribeImagesRequest, DescribeInstancesRequest, DescribeRegionsRequest,
    DescribeReservedInstancesRequest, DescribeSnapshotsRequest,
    DescribeSpotInstanceRequestsRequest, DescribeSpotPriceHistoryRequest, DescribeVolumesRequest,
    Ec2, Ec2Client, Filter,
};
use std::collections::HashMap;
use std::fmt;

use crate::config::Config;

macro_rules! some {
    ($expr : expr) => {
        if let Some(v) = $expr {
            v
        } else {
            return None;
        }
    };
}

pub struct Ec2Instance {
    ec2_client: Ec2Client,
    config: Config,
}

impl fmt::Debug for Ec2Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ec2Instance")
    }
}

impl Default for Ec2Instance {
    fn default() -> Self {
        Self {
            ec2_client: Ec2Client::new(Region::UsEast1),
            config: Config::new(),
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
        Self {
            ec2_client: Ec2Client::new(region),
            config,
        }
    }

    pub fn get_ami_tags(&self) -> Result<Vec<AmiInfo>, Error> {
        let owner_id = match self.config.my_owner_id.as_ref() {
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
                    .unwrap_or_else(|| Vec::new())
                    .into_iter()
                    .filter_map(|image| {
                        Some(AmiInfo {
                            id: some!(image.image_id),
                            name: some!(image.name),
                            state: some!(image.state),
                            snapshot_ids: some!(image.block_device_mappings)
                                .into_iter()
                                .filter_map(|block| block.ebs.and_then(|b| b.snapshot_id))
                                .collect(),
                        })
                    })
                    .collect()
            })
    }

    pub fn get_all_regions(&self) -> Result<HashMap<String, String>, Error> {
        self.ec2_client
            .describe_regions(DescribeRegionsRequest::default())
            .sync()
            .map_err(err_msg)
            .map(|r| {
                r.regions
                    .unwrap_or_else(|| Vec::new())
                    .into_iter()
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
                    .unwrap_or_else(|| Vec::new())
                    .into_iter()
                    .filter_map(|res| {
                        res.instances.map(|instances| {
                            instances
                                .into_iter()
                                .filter_map(|inst| {
                                    let tags: HashMap<String, String> = inst
                                        .tags
                                        .unwrap_or_else(|| Vec::new())
                                        .into_iter()
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
                    .unwrap_or_else(|| Vec::new())
                    .into_iter()
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
    ) -> Result<HashMap<String, String>, Error> {
        let mut filters = vec![
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
        if !inst_list.is_empty() {
            filters.push(Filter {
                name: Some("instance-type".to_string()),
                values: Some(inst_list.to_vec()),
            })
        }
        let start_time = Utc::now() - Duration::hours(4);
        self.ec2_client
            .describe_spot_price_history(DescribeSpotPriceHistoryRequest {
                start_time: Some(start_time.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                filters: Some(filters),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|spot_hist| {
                println!("{:?}", spot_hist.next_token);
                spot_hist
                    .spot_price_history
                    .unwrap_or_else(|| Vec::new())
                    .into_iter()
                    .filter_map(|spot_price| {
                        Some((
                            some!(spot_price.instance_type),
                            some!(spot_price.spot_price),
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
                    .unwrap_or_else(|| Vec::new())
                    .into_iter()
                    .filter_map(|inst| {
                        Some(SpotInstanceRequestInfo {
                            id: some!(inst.spot_instance_request_id),
                            price: some!(inst.spot_price.and_then(|s| s.parse::<f32>().ok())),
                            instance_type: some!(some!(inst.launch_specification).instance_type),
                            spot_type: some!(inst.type_),
                            status: some!(some!(inst.status).code),
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
                    .unwrap_or_else(|| Vec::new())
                    .into_iter()
                    .filter_map(|v| {
                        Some(VolumeInfo {
                            id: some!(v.volume_id),
                            availability_zone: some!(v.availability_zone),
                            size: some!(v.size),
                            iops: some!(v.iops),
                            state: some!(v.state),
                            tags: some!(v.tags)
                                .into_iter()
                                .filter_map(|t| Some((some!(t.key), some!(t.value))))
                                .collect(),
                        })
                    })
                    .collect()
            })
    }

    pub fn get_all_snapshots(&self) -> Result<Vec<SnapshotInfo>, Error> {
        let owner_id = match self.config.my_owner_id.as_ref() {
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
                    .unwrap_or_else(|| Vec::new())
                    .into_iter()
                    .filter_map(|snap| {
                        Some(SnapshotInfo {
                            id: some!(snap.snapshot_id),
                            volume_size: some!(snap.volume_size),
                            state: some!(snap.state),
                            progress: some!(snap.progress),
                            tags: some!(snap.tags)
                                .into_iter()
                                .filter_map(|t| Some((some!(t.key), some!(t.value))))
                                .collect(),
                        })
                    })
                    .collect()
            })
    }
}

#[derive(Debug)]
pub struct AmiInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub snapshot_ids: Vec<String>,
}

#[derive(Debug)]
pub struct Ec2InstanceInfo {
    pub id: String,
    pub dns_name: String,
    pub state: String,
    pub instance_type: String,
    pub availability_zone: String,
    pub tags: HashMap<String, String>,
}

#[derive(Debug)]
pub struct ReservedInstanceInfo {
    pub id: String,
    pub price: f32,
    pub instance_type: String,
    pub state: String,
    pub availability_zone: String,
}

#[derive(Debug)]
pub struct SpotInstanceRequestInfo {
    pub id: String,
    pub price: f32,
    pub instance_type: String,
    pub spot_type: String,
    pub status: String,
}

#[derive(Debug)]
pub struct VolumeInfo {
    pub id: String,
    pub availability_zone: String,
    pub size: i64,
    pub iops: i64,
    pub state: String,
    pub tags: HashMap<String, String>,
}

#[derive(Debug)]
pub struct SnapshotInfo {
    pub id: String,
    pub volume_size: i64,
    pub state: String,
    pub progress: String,
    pub tags: HashMap<String, String>,
}

use chrono::{Duration, Utc};
use failure::{err_msg, Error};
use rusoto_core::Region;
use rusoto_ec2::{
    DescribeImagesRequest, DescribeInstancesRequest, DescribeRegionsRequest,
    DescribeReservedInstancesRequest, DescribeSpotPriceHistoryRequest, Ec2, Ec2Client, Filter,
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
        }
    }
}

impl Ec2Instance {
    pub fn new(aws_region_name: &str) -> Self {
        let region: Region = aws_region_name.parse().ok().unwrap_or(Region::UsEast1);
        Self {
            ec2_client: Ec2Client::new(region),
        }
    }

    pub fn get_ami_tags(&self, config: &Config) -> Result<HashMap<String, String>, Error> {
        let owner_id = match config.my_owner_id.as_ref() {
            Some(x) => x.to_string(),
            None => return Ok(HashMap::new()),
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
                    .filter_map(|image| Some((some!(image.name), some!(image.image_id))))
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
                            state: some!(inst.state),
                            availability_zone: inst
                                .availability_zone
                                .unwrap_or_else(|| "".to_string()),
                        })
                    })
                    .collect()
            })
    }

    pub fn get_latest_spot_inst_prices(&self) -> Result<HashMap<String, String>, Error> {
        let start_time = Utc::now() - Duration::hours(4);
        self.ec2_client
            .describe_spot_price_history(DescribeSpotPriceHistoryRequest {
                start_time: Some(start_time.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                filters: Some(vec![
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
                ]),
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
    pub state: String,
    pub availability_zone: String,
}

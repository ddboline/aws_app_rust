use failure::{err_msg, Error};
use rusoto_core::Region;
use rusoto_ec2::{DescribeImagesRequest, DescribeRegionsRequest, Ec2, Ec2Client, Filter};
use std::collections::HashMap;
use std::fmt;

use crate::config::Config;

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
                    .map(|image| {
                        (
                            image.name.unwrap_or_else(|| "".to_string()),
                            image.image_id.unwrap_or_else(|| "".to_string()),
                        )
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
                        region.region_name.as_ref().and_then(|region_name| {
                            region.opt_in_status.as_ref().map(|opt_in_status| {
                                (region_name.to_string(), opt_in_status.to_string())
                            })
                        })
                    })
                    .collect()
            })
    }
}

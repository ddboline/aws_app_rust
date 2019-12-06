use failure::{err_msg, Error};
use rusoto_core::Region;
use rusoto_ecr::{DescribeImagesRequest, DescribeRepositoriesRequest, Ecr, EcrClient};
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

#[derive(Clone)]
pub struct EcrInstance {
    ecr_client: EcrClient,
    region: Region,
}

impl fmt::Debug for EcrInstance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "EcrInstance")
    }
}

impl Default for EcrInstance {
    fn default() -> Self {
        Self {
            ecr_client: EcrClient::new(Region::UsEast1),
            region: Region::UsEast1,
        }
    }
}

impl EcrInstance {
    pub fn new(config: Config) -> Self {
        let region: Region = config
            .aws_region_name
            .parse()
            .ok()
            .unwrap_or(Region::UsEast1);
        Self {
            ecr_client: EcrClient::new(region.clone()),
            region,
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.region = region.parse()?;
        self.ecr_client = EcrClient::new(self.region.clone());
        Ok(())
    }

    pub fn get_all_repositories(&self) -> Result<Vec<String>, Error> {
        self.ecr_client
            .describe_repositories(DescribeRepositoriesRequest::default())
            .sync()
            .map_err(err_msg)
            .map(|r| {
                r.repositories
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|repo| repo.repository_name)
                    .collect()
            })
    }

    pub fn get_all_images(&self, reponame: &str) -> Result<Vec<ImageInfo>, Error> {
        self.ecr_client
            .describe_images(DescribeImagesRequest {
                repository_name: reponame.to_string(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|i| {
                i.image_details
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|image| {
                        Some(ImageInfo {
                            digest: some!(image.image_digest),
                            tags: image.image_tags.unwrap_or_else(|| Vec::new()),
                        })
                    })
                    .collect()
            })
    }
}

#[derive(Debug)]
pub struct ImageInfo {
    pub digest: String,
    pub tags: Vec<String>,
}

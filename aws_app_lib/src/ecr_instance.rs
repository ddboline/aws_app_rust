use failure::{err_msg, Error};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rusoto_core::Region;
use rusoto_ecr::{
    BatchDeleteImageRequest, DescribeImagesRequest, DescribeRepositoriesRequest, Ecr, EcrClient,
    ImageIdentifier,
};
use std::fmt;

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
pub struct EcrInstance {
    sts: StsInstance,
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
            sts: StsInstance::default(),
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
        let sts = StsInstance::new(None).unwrap();
        Self {
            ecr_client: sts.get_ecr_client(region.clone()).unwrap(),
            region,
            sts,
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.region = region.parse()?;
        self.ecr_client = self.sts.get_ecr_client(self.region.clone())?;
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
                    .into_par_iter()
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
                    .into_par_iter()
                    .filter_map(|image| {
                        Some(ImageInfo {
                            digest: some!(image.image_digest),
                            tags: image.image_tags.unwrap_or_else(Vec::new),
                        })
                    })
                    .collect()
            })
    }

    pub fn delete_ecr_images(&self, reponame: &str, imageids: &[String]) -> Result<(), Error> {
        self.ecr_client
            .batch_delete_image(BatchDeleteImageRequest {
                repository_name: reponame.to_string(),
                image_ids: imageids
                    .par_iter()
                    .map(|i| ImageIdentifier {
                        image_digest: Some(i.to_string()),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })
            .sync()
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn cleanup_ecr_images(&self) -> Result<(), Error> {
        self.get_all_repositories()?
            .into_par_iter()
            .map(|repo| {
                let imageids: Vec<_> = self
                    .get_all_images(&repo)?
                    .into_par_iter()
                    .filter_map(|i| {
                        if i.tags.is_empty() {
                            Some(i.digest)
                        } else {
                            None
                        }
                    })
                    .collect();
                if !imageids.is_empty() {
                    self.delete_ecr_images(&repo, &imageids)
                } else {
                    Ok(())
                }
            })
            .collect()
    }
}

#[derive(Debug)]
pub struct ImageInfo {
    pub digest: String,
    pub tags: Vec<String>,
}

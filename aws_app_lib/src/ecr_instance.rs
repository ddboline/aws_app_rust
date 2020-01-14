use anyhow::Error;
use chrono::{DateTime, Utc, TimeZone};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rusoto_core::Region;
use rusoto_ecr::{
    BatchDeleteImageRequest, DescribeImagesRequest, DescribeRepositoriesRequest, Ecr, EcrClient,
    ImageIdentifier,
};
use std::fmt;
use sts_profile_auth::get_client_sts;

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
            ecr_client: get_client_sts!(EcrClient, Region::UsEast1).expect("StsProfile failed"),
            region: Region::UsEast1,
        }
    }
}

impl EcrInstance {
    pub fn new(config: &Config) -> Self {
        let region: Region = config
            .aws_region_name
            .parse()
            .ok()
            .unwrap_or(Region::UsEast1);
        Self {
            ecr_client: get_client_sts!(EcrClient, region.clone()).expect("StsProfile failed"),
            region,
        }
    }

    pub fn set_region(&mut self, region: &str) -> Result<(), Error> {
        self.region = region.parse()?;
        self.ecr_client = get_client_sts!(EcrClient, self.region.clone())?;
        Ok(())
    }

    pub fn get_all_repositories(&self) -> Result<Vec<String>, Error> {
        self.ecr_client
            .describe_repositories(DescribeRepositoriesRequest::default())
            .sync()
            .map_err(Into::into)
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
                ..DescribeImagesRequest::default()
            })
            .sync()
            .map_err(Into::into)
            .map(|i| {
                i.image_details
                    .unwrap_or_else(Vec::new)
                    .into_par_iter()
                    .filter_map(|image| {
                        let pushed_at = image.image_pushed_at.map_or_else(
                            Utc::now,
                            |p| {
                                let s = p as i64;
                                let ns = p.fract() * 1.0e9;
                                Utc.timestamp(s, ns as u32)
                            },
                        );
                        let image_size = image.image_size_in_bytes.map_or(0.0, |s| {
                            s as f64 / 1e6
                        });
                        Some(ImageInfo {
                            digest: some!(image.image_digest),
                            tags: image.image_tags.unwrap_or_else(Vec::new),
                            pushed_at,
                            image_size,
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
                        ..ImageIdentifier::default()
                    })
                    .collect(),
                ..BatchDeleteImageRequest::default()
            })
            .sync()
            .map_err(Into::into)
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
                if imageids.is_empty() {
                    Ok(())
                } else {
                    self.delete_ecr_images(&repo, &imageids)
                }
            })
            .collect()
    }
}

#[derive(Debug)]
pub struct ImageInfo {
    pub digest: String,
    pub tags: Vec<String>,
    pub pushed_at: DateTime<Utc>,
    pub image_size: f64,
}

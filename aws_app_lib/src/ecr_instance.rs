use anyhow::Error;
use chrono::{DateTime, TimeZone, Utc};
use futures::future::try_join_all;
use rusoto_core::Region;
use rusoto_ecr::{
    BatchDeleteImageRequest, DescribeImagesRequest, DescribeRepositoriesRequest, Ecr, EcrClient,
    ImageIdentifier,
};
use stack_string::StackString;
use std::fmt;
use sts_profile_auth::get_client_sts;

use crate::config::Config;

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

    pub async fn get_all_repositories(&self) -> Result<Vec<StackString>, Error> {
        self.ecr_client
            .describe_repositories(DescribeRepositoriesRequest::default())
            .await
            .map_err(Into::into)
            .map(|r| {
                r.repositories
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|repo| repo.repository_name.map(Into::into))
                    .collect()
            })
    }

    pub async fn get_all_images(&self, reponame: &str) -> Result<Vec<ImageInfo>, Error> {
        self.ecr_client
            .describe_images(DescribeImagesRequest {
                repository_name: reponame.to_string(),
                ..DescribeImagesRequest::default()
            })
            .await
            .map_err(Into::into)
            .map(|i| {
                i.image_details
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|image| {
                        let pushed_at = image.image_pushed_at.map_or_else(Utc::now, |p| {
                            let s = p as i64;
                            let ns = p.fract() * 1.0e9;
                            Utc.timestamp(s, ns as u32)
                        });
                        let image_size = image.image_size_in_bytes.map_or(0.0, |s| s as f64 / 1e6);
                        let tags: Vec<_> = image
                            .image_tags
                            .unwrap_or_else(Vec::new)
                            .into_iter()
                            .map(|s| s.into())
                            .collect();
                        Some(ImageInfo {
                            digest: image.image_digest?.into(),
                            tags,
                            pushed_at,
                            image_size,
                        })
                    })
                    .collect()
            })
    }

    pub async fn delete_ecr_images<T: AsRef<str>>(
        &self,
        reponame: &str,
        imageids: &[T],
    ) -> Result<(), Error> {
        self.ecr_client
            .batch_delete_image(BatchDeleteImageRequest {
                repository_name: reponame.to_string(),
                image_ids: imageids
                    .iter()
                    .map(|i| ImageIdentifier {
                        image_digest: Some(i.as_ref().into()),
                        ..ImageIdentifier::default()
                    })
                    .collect(),
                ..BatchDeleteImageRequest::default()
            })
            .await
            .map_err(Into::into)
            .map(|_| ())
    }

    pub async fn cleanup_ecr_images(&self) -> Result<(), Error> {
        let futures = self
            .get_all_repositories()
            .await?
            .into_iter()
            .map(|repo| async move {
                let imageids: Vec<_> = self
                    .get_all_images(repo.as_ref())
                    .await?
                    .into_iter()
                    .filter_map(|i| {
                        if i.tags.is_empty() {
                            Some(i.digest.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                if !imageids.is_empty() {
                    self.delete_ecr_images(repo.as_ref(), &imageids).await?;
                }
                Ok(())
            });
        let results: Result<Vec<_>, Error> = try_join_all(futures).await;
        results?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ImageInfo {
    pub digest: StackString,
    pub tags: Vec<StackString>,
    pub pushed_at: DateTime<Utc>,
    pub image_size: f64,
}

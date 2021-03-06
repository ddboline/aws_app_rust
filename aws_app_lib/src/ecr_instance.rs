use anyhow::Error;
use chrono::{DateTime, TimeZone, Utc};
use futures::future::try_join_all;
use rusoto_core::Region;
use rusoto_ecr::{
    BatchDeleteImageRequest, DescribeImagesRequest, DescribeRepositoriesRequest, Ecr, EcrClient,
    ImageIdentifier,
};
use stack_string::StackString;
use std::{fmt, sync::Arc};
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

    pub async fn get_all_repositories(&self) -> Result<impl Iterator<Item = StackString>, Error> {
        self.ecr_client
            .describe_repositories(DescribeRepositoriesRequest::default())
            .await
            .map_err(Into::into)
            .map(|r| {
                r.repositories
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(|repo| repo.repository_name.map(Into::into))
            })
    }

    pub async fn get_all_images(
        &self,
        reponame: &str,
    ) -> Result<impl Iterator<Item = ImageInfo>, Error> {
        let reponame: Arc<StackString> = Arc::new(reponame.into());
        self.ecr_client
            .describe_images(DescribeImagesRequest {
                repository_name: reponame.to_string(),
                ..DescribeImagesRequest::default()
            })
            .await
            .map_err(Into::into)
            .map(move |i| {
                i.image_details
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .filter_map(move |image| {
                        let pushed_at = image.image_pushed_at.map_or_else(Utc::now, |p| {
                            let s = p as i64;
                            let ns = p.fract() * 1.0e9;
                            Utc.timestamp(s, ns as u32)
                        });
                        let image_size = image.image_size_in_bytes.map_or(0.0, |s| s as f64 / 1e6);
                        let tags = image
                            .image_tags
                            .unwrap_or_else(Vec::new)
                            .into_iter()
                            .map(|s| s.into())
                            .collect();
                        Some(ImageInfo {
                            repo: (*reponame).clone(),
                            digest: image.image_digest?.into(),
                            tags,
                            pushed_at,
                            image_size,
                        })
                    })
            })
    }

    pub async fn delete_ecr_images(
        &self,
        reponame: &str,
        image_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), Error> {
        let image_ids: Vec<_> = image_ids
            .into_iter()
            .map(|i| ImageIdentifier {
                image_digest: Some(i.as_ref().into()),
                ..ImageIdentifier::default()
            })
            .collect();
        if image_ids.is_empty() {
            return Ok(());
        }
        self.ecr_client
            .batch_delete_image(BatchDeleteImageRequest {
                repository_name: reponame.to_string(),
                image_ids,
                ..BatchDeleteImageRequest::default()
            })
            .await
            .map_err(Into::into)
            .map(|_| ())
    }

    pub async fn cleanup_ecr_images(&self) -> Result<(), Error> {
        let futures = self.get_all_repositories().await?.map(|repo| async move {
            let imageids = self.get_all_images(repo.as_ref()).await?.filter_map(|i| {
                if i.tags.is_empty() {
                    Some(i.digest.to_string())
                } else {
                    None
                }
            });
            self.delete_ecr_images(repo.as_ref(), imageids).await?;
            Ok(())
        });
        let results: Result<Vec<_>, Error> = try_join_all(futures).await;
        results?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ImageInfo {
    pub repo: StackString,
    pub digest: StackString,
    pub tags: Vec<StackString>,
    pub pushed_at: DateTime<Utc>,
    pub image_size: f64,
}

impl ImageInfo {
    pub fn get_html_string(&self) -> StackString {
        format!(
            r#"<tr style="text-align: center;">
            <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:0.2} MB</td>
            </tr>"#,
            format!(
                r#"<input type="button" name="DeleteEcrImage" value="DeleteEcrImage"
                    onclick="deleteEcrImage('{}', '{}')">"#,
                self.repo, self.digest,
            ),
            self.repo,
            self.tags.get(0).map_or_else(|| "None", StackString::as_str),
            self.digest,
            self.pushed_at,
            self.image_size,
        )
        .into()
    }
}

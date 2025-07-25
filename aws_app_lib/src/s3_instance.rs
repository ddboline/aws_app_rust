use aws_config::SdkConfig;
use aws_sdk_s3::{
    Client as S3Client,
    operation::list_objects::ListObjectsOutput,
    primitives::ByteStream,
    types::{Bucket, Object},
};
use parking_lot::{Mutex, MutexGuard};
use stack_string::{StackString, format_sstr};
use std::{fmt, path::Path, sync::LazyLock};
use tokio::io::AsyncReadExt;
use url::Url;

use crate::{errors::AwslibError as Error, exponential_retry};

static S3INSTANCE_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone)]
pub struct S3Instance {
    s3_client: S3Client,
    max_keys: Option<i32>,
}

impl fmt::Debug for S3Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("S3Instance")
    }
}

impl S3Instance {
    #[must_use]
    pub fn new(sdk_config: &SdkConfig) -> Self {
        Self {
            s3_client: S3Client::from_conf(sdk_config.into()),
            max_keys: None,
        }
    }

    pub fn get_instance_lock() -> MutexGuard<'static, ()> {
        S3INSTANCE_TEST_MUTEX.lock()
    }

    #[must_use]
    pub fn max_keys(mut self, max_keys: i32) -> Self {
        self.max_keys = Some(max_keys);
        self
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_list_of_buckets(&self) -> Result<Vec<Bucket>, Error> {
        exponential_retry(|| async move {
            self.s3_client
                .list_buckets()
                .send()
                .await
                .map(|l| l.buckets.unwrap_or_default())
                .map_err(Into::into)
        })
        .await
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn create_bucket(&self, bucket_name: &str) -> Result<String, Error> {
        exponential_retry(|| async move {
            let location = self
                .s3_client
                .create_bucket()
                .bucket(bucket_name)
                .send()
                .await?
                .location
                .ok_or_else(|| Error::StaticCustomError("Failed to create bucket"))?;
            Ok(location)
        })
        .await
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn delete_bucket(&self, bucket_name: &str) -> Result<(), Error> {
        exponential_retry(|| async move {
            self.s3_client
                .delete_bucket()
                .bucket(bucket_name)
                .send()
                .await
                .map(|_| ())
                .map_err(Into::into)
        })
        .await
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn delete_key(&self, bucket_name: &str, key_name: &str) -> Result<(), Error> {
        exponential_retry(|| async move {
            self.s3_client
                .delete_object()
                .bucket(bucket_name)
                .key(key_name)
                .send()
                .await
                .map(|_| ())
                .map_err(Into::into)
        })
        .await
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn copy_key(
        &self,
        source: &Url,
        bucket_to: &str,
        key_to: &str,
    ) -> Result<Option<String>, Error> {
        exponential_retry(|| {
            let copy_source = source.to_string();
            async move {
                self.s3_client
                    .copy_object()
                    .copy_source(copy_source)
                    .bucket(bucket_to)
                    .key(key_to)
                    .send()
                    .await
                    .map_err(Into::into)
            }
        })
        .await
        .map(|x| x.copy_object_result.and_then(|s| s.e_tag))
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn upload(
        &self,
        fname: &Path,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<(), Error> {
        if !fname.exists() {
            return Err(Error::CustomError(format_sstr!(
                "File doesn't exist {fname:?}"
            )));
        }
        exponential_retry(|| async move {
            let body = ByteStream::read_from().path(fname).build().await?;
            self.s3_client
                .put_object()
                .bucket(bucket_name)
                .key(key_name)
                .body(body)
                .send()
                .await
                .map(|_| ())
                .map_err(Into::into)
        })
        .await
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn download(
        &self,
        bucket_name: &str,
        key_name: &str,
        fname: &Path,
    ) -> Result<StackString, Error> {
        exponential_retry(|| async move {
            let resp = self
                .s3_client
                .get_object()
                .bucket(bucket_name)
                .key(key_name)
                .send()
                .await?;
            let etag = resp
                .e_tag
                .ok_or_else(|| Error::StaticCustomError("No etag"))?
                .trim_matches('"')
                .into();
            tokio::io::copy(
                &mut resp.body.into_async_read(),
                &mut tokio::fs::File::create(fname).await?,
            )
            .await?;
            Ok(etag)
        })
        .await
    }

    async fn list_keys(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        marker: Option<impl AsRef<str>>,
        max_keys: Option<i32>,
    ) -> Result<ListObjectsOutput, Error> {
        let mut builder = self.s3_client.list_objects().bucket(bucket);
        if let Some(prefix) = prefix {
            builder = builder.prefix(prefix);
        }
        if let Some(marker) = marker {
            builder = builder.marker(marker.as_ref());
        }
        if let Some(max_keys) = max_keys {
            builder = builder.max_keys(max_keys);
        }
        builder.send().await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_list_of_keys(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<Object>, Error> {
        exponential_retry(|| async move {
            let mut marker: Option<String> = None;
            let mut list_of_keys = Vec::new();
            let mut max_keys = self.max_keys;
            loop {
                let mut output = self
                    .list_keys(bucket, prefix, marker.as_ref(), max_keys)
                    .await?;
                if let Some(contents) = output.contents.take() {
                    if let Some(last) = contents.last() {
                        if let Some(key) = &last.key {
                            marker.replace(key.into());
                        }
                    }
                    if let Some(n) = max_keys {
                        if contents.len() <= n as usize {
                            max_keys.replace(n - contents.len() as i32);
                        }
                    }
                    list_of_keys.extend_from_slice(&contents);
                }
                if output.is_truncated == Some(false) || output.is_truncated.is_none() {
                    break;
                }
            }
            Ok(list_of_keys)
        })
        .await
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn process_list_of_keys<T>(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        callback: T,
    ) -> Result<(), Error>
    where
        T: Fn(&Object) -> Result<(), Error> + Send + Sync,
    {
        let mut marker: Option<String> = None;
        let mut max_keys = self.max_keys;
        loop {
            let mut output = self
                .list_keys(bucket, prefix, marker.as_ref(), max_keys)
                .await?;
            if let Some(contents) = output.contents.take() {
                if let Some(last) = contents.last() {
                    if let Some(key) = &last.key {
                        marker.replace(key.into());
                    }
                }
                if let Some(n) = max_keys {
                    if contents.len() <= n as usize {
                        max_keys.replace(n - contents.len() as i32);
                    }
                }
                for object in &contents {
                    callback(object)?;
                }
            }
            if output.is_truncated == Some(false) || output.is_truncated.is_none() {
                break;
            }
        }
        Ok(())
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn download_to_string(
        &self,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<String, Error> {
        exponential_retry(|| async move {
            let resp = self
                .s3_client
                .get_object()
                .bucket(bucket_name)
                .key(key_name)
                .send()
                .await?;
            let mut buf = String::new();
            resp.body.into_async_read().read_to_string(&mut buf).await?;
            Ok(buf)
        })
        .await
    }
}

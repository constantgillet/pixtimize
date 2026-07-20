//! S3-compatible object storage adapter.

use aws_sdk_s3::{
    config::{BehaviorVersion, Credentials, Region},
    primitives::ByteStream,
    types::{Delete, ObjectCannedAcl, ObjectIdentifier},
};
use bytes::Bytes;

use crate::{config::Config, error::AppError};

/// Object metadata returned without downloading its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
    pub content_length: u64,
    pub content_type: Option<String>,
}

/// S3-compatible source and transformed-image storage.
pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Storage {
    /// Constructs the adapter from application configuration.
    pub fn new(config: &Config) -> Self {
        let credentials = Credentials::new(
            config.s3.access_key.clone(),
            config.s3.secret_key.clone(),
            None,
            None,
            "pixtimize",
        );
        let s3_config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.s3.region.clone()))
            .endpoint_url(config.s3.endpoint.clone())
            .credentials_provider(credentials)
            .force_path_style(false)
            .build();

        Self {
            client: aws_sdk_s3::Client::from_conf(s3_config),
            bucket: config.s3.bucket.clone(),
        }
    }

    /// Fetches the raw bytes stored at `key`.
    pub async fn get(&self, key: &str) -> Result<Bytes, AppError> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| {
                let service_error = error.into_service_error();
                if service_error.is_no_such_key() {
                    AppError::NotFound
                } else {
                    AppError::Storage(service_error.to_string())
                }
            })?;

        output
            .body
            .collect()
            .await
            .map(aws_sdk_s3::primitives::AggregatedBytes::into_bytes)
            .map_err(|error| AppError::Storage(error.to_string()))
    }

    /// Fetches object metadata without downloading its body.
    pub async fn head(&self, key: &str) -> Result<ObjectMeta, AppError> {
        let output = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| {
                let service_error = error.into_service_error();
                if service_error.is_not_found() {
                    AppError::NotFound
                } else {
                    AppError::Storage(service_error.to_string())
                }
            })?;

        Ok(ObjectMeta {
            content_length: output.content_length().unwrap_or(0).max(0) as u64,
            content_type: output.content_type,
        })
    }

    /// Uploads `data` to a publicly readable object.
    pub async fn upload(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(), AppError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data))
            .acl(ObjectCannedAcl::PublicRead)
            .content_type(content_type)
            .send()
            .await
            .map_err(|error| AppError::Storage(error.into_service_error().to_string()))?;
        Ok(())
    }

    /// Deletes every object named in `keys` in one request.
    pub async fn delete_multiple(&self, keys: Vec<String>) -> Result<(), AppError> {
        if keys.is_empty() {
            return Ok(());
        }

        let objects = keys
            .into_iter()
            .map(|key| {
                ObjectIdentifier::builder()
                    .key(key)
                    .build()
                    .map_err(|error| AppError::Storage(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let delete = Delete::builder()
            .set_objects(Some(objects))
            .quiet(false)
            .build()
            .map_err(|error| AppError::Storage(error.to_string()))?;

        self.client
            .delete_objects()
            .bucket(&self.bucket)
            .delete(delete)
            .send()
            .await
            .map_err(|error| AppError::Storage(error.into_service_error().to_string()))?;
        Ok(())
    }
}

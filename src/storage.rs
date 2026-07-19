//! Thin wrappers around the S3 client used by the transform pipeline.

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{Delete, ObjectCannedAcl, ObjectIdentifier};
use bytes::Bytes;

use crate::{error::AppError, state::AppState};

impl AppState {
    /// Fetches the raw bytes of the object stored at `key`.
    ///
    /// Returns [`AppError::NotFound`] when the object does not exist.
    pub async fn get_file(&self, key: &str) -> Result<Bytes, AppError> {
        let output = self
            .s3()
            .get_object()
            .bucket(&self.config().s3.bucket)
            .key(key)
            .send()
            .await
            .map_err(|err| {
                let service_err = err.into_service_error();
                if service_err.is_no_such_key() {
                    AppError::NotFound
                } else {
                    AppError::Storage(service_err.to_string())
                }
            })?;

        output
            .body
            .collect()
            .await
            .map(aws_sdk_s3::primitives::AggregatedBytes::into_bytes)
            .map_err(|err| AppError::Storage(err.to_string()))
    }

    /// Uploads `data` to `key` as a publicly readable object.
    pub async fn upload(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(), AppError> {
        self.s3()
            .put_object()
            .bucket(&self.config().s3.bucket)
            .key(key)
            .body(ByteStream::from(data))
            .acl(ObjectCannedAcl::PublicRead)
            .content_type(content_type)
            .send()
            .await
            .map_err(|err| AppError::Storage(err.into_service_error().to_string()))?;

        Ok(())
    }

    /// Deletes every object named in `keys` in a single request.
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
                    .map_err(|err| AppError::Storage(err.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let delete = Delete::builder()
            .set_objects(Some(objects))
            .quiet(false)
            .build()
            .map_err(|err| AppError::Storage(err.to_string()))?;

        self.s3()
            .delete_objects()
            .bucket(&self.config().s3.bucket)
            .delete(delete)
            .send()
            .await
            .map_err(|err| AppError::Storage(err.into_service_error().to_string()))?;

        Ok(())
    }
}

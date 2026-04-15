//! S3-compatible object storage client (SeaweedFS via the S3 API).

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client as SdkClient;

use crate::error::{Error, Result};

/// Object store client.
#[derive(Clone)]
pub struct Client {
    inner: SdkClient,
    bucket: String,
}

/// Minimal `HEAD` result — we only need `content_length` at the call sites
/// and parse it as a structured number, never by substring-matching raw
/// header text.
#[derive(Debug, Clone)]
pub struct HeadObject {
    /// Object size in bytes.
    pub content_length: u64,
}

/// Default bucket used by sunbeam-meet. Overridden via `S3_BUCKET` env var at
/// connect time so the dev compose stack (`sunbeam-meet-it`) and prod
/// (`sunbeam-meet-recordings`) share one connect path.
pub const DEFAULT_BUCKET: &str = "sunbeam-meet";

/// Connect to an S3-compatible endpoint.
pub async fn connect(endpoint: &str, access_key: &str, secret_key: &str) -> Result<Client> {
    let creds = Credentials::new(access_key, secret_key, None, None, "sunbeam-meet");
    let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(endpoint)
        .credentials_provider(creds)
        .load()
        .await;
    let s3_cfg = aws_sdk_s3::config::Builder::from(&cfg)
        .force_path_style(true)
        .build();
    let bucket = std::env::var("S3_BUCKET").unwrap_or_else(|_| DEFAULT_BUCKET.to_owned());
    Ok(Client {
        inner: SdkClient::from_conf(s3_cfg),
        bucket,
    })
}

impl Client {
    /// Create the bucket if it does not already exist. Idempotent — a
    /// `BucketAlreadyOwnedByYou` / 409 response is treated as success.
    pub async fn ensure_bucket(&self) -> Result<()> {
        match self.inner.create_bucket().bucket(&self.bucket).send().await {
            Ok(_) => Ok(()),
            Err(e) => {
                // Parse the structured error — never string-match on the raw
                // message. SDK returns a typed service error we can classify.
                use aws_sdk_s3::operation::create_bucket::CreateBucketError;
                if let Some(svc) = e.as_service_error() {
                    if matches!(
                        svc,
                        CreateBucketError::BucketAlreadyExists(_)
                            | CreateBucketError::BucketAlreadyOwnedByYou(_)
                    ) {
                        return Ok(());
                    }
                }
                Err(Error::Internal(anyhow::anyhow!("s3 create_bucket: {e}")))
            }
        }
    }
}

impl Client {
    /// HEAD an object. Returns `Ok(None)` on 404, `Ok(Some(_))` on hit.
    pub async fn head_object(&self, key: &str) -> Result<Option<HeadObject>> {
        match self
            .inner
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(out) => Ok(Some(HeadObject {
                content_length: u64::try_from(out.content_length().unwrap_or(0)).unwrap_or(0),
            })),
            Err(e) => {
                // Classify via the SDK's structured error, never by string
                // matching. HeadObject's typed 404 variant is `NotFound`.
                use aws_sdk_s3::operation::head_object::HeadObjectError;
                if let Some(HeadObjectError::NotFound(_)) = e.as_service_error() {
                    return Ok(None);
                }
                Err(Error::Internal(anyhow::anyhow!("s3 head: {e}")))
            }
        }
    }

    /// Delete an object.
    pub async fn delete_object(&self, key: &str) -> Result<()> {
        self.inner
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("s3 delete: {e}")))?;
        Ok(())
    }
}

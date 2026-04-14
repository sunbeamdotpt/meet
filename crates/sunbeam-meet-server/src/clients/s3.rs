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

/// Default bucket used by sunbeam-meet. Overridden via config when wired up.
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
    Ok(Client {
        inner: SdkClient::from_conf(s3_cfg),
        bucket: DEFAULT_BUCKET.to_owned(),
    })
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
                // Fail-closed on errors that aren't "not found".
                let msg = format!("{e}");
                if msg.contains("NotFound") || msg.contains("404") {
                    Ok(None)
                } else {
                    Err(Error::Internal(anyhow::anyhow!("s3 head: {e}")))
                }
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

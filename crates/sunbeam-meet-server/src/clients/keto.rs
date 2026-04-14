//! Ory Keto client — relation-tuple authorization checks.

use serde::Deserialize;

use crate::config::KetoConfig;
use crate::error::{Error, Result};

/// Keto client.
#[derive(Clone)]
pub struct KetoClient {
    http: reqwest::Client,
    read_url: String,
    write_url: String,
}

#[derive(Debug, Deserialize)]
struct CheckResponse {
    allowed: bool,
}

impl KetoClient {
    /// New client from config.
    pub fn new(cfg: &KetoConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            read_url: cfg.read_url.clone(),
            write_url: cfg.write_url.clone(),
        }
    }

    /// Check whether `(subject) -- relation -- (namespace, object)` is allowed.
    pub async fn check(
        &self,
        namespace: &str,
        object: &str,
        relation: &str,
        subject_id: &str,
    ) -> Result<bool> {
        let url = format!("{}/relation-tuples/check", self.read_url);
        let res = self
            .http
            .get(url)
            .query(&[
                ("namespace", namespace),
                ("object", object),
                ("relation", relation),
                ("subject_id", subject_id),
            ])
            .send()
            .await?;
        if res.status() == reqwest::StatusCode::FORBIDDEN {
            return Ok(false);
        }
        if !res.status().is_success() {
            return Err(Error::Internal(anyhow::anyhow!(
                "keto check: {}",
                res.status()
            )));
        }
        let c: CheckResponse = res.json().await?;
        Ok(c.allowed)
    }

    /// Write a relation tuple (grant a relation).
    pub async fn grant(
        &self,
        namespace: &str,
        object: &str,
        relation: &str,
        subject_id: &str,
    ) -> Result<()> {
        let url = format!("{}/admin/relation-tuples", self.write_url);
        let body = serde_json::json!({
            "namespace": namespace,
            "object": object,
            "relation": relation,
            "subject_id": subject_id,
        });
        let res = self.http.put(url).json(&body).send().await?;
        if !res.status().is_success() {
            return Err(Error::Internal(anyhow::anyhow!(
                "keto grant: {}",
                res.status()
            )));
        }
        Ok(())
    }
}

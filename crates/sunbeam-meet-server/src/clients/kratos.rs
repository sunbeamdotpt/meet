//! Ory Kratos client — resolves session cookies / bearer tokens to an identity.

use serde::Deserialize;

use crate::config::KratosConfig;
use crate::error::{Error, Result};

/// Kratos client.
#[derive(Clone)]
pub struct KratosClient {
    http: reqwest::Client,
    public_url: String,
    #[allow(dead_code)]
    admin_url: String,
}

/// Resolved Kratos identity.
#[derive(Debug, Clone)]
pub struct Identity {
    /// Kratos identity ID.
    pub id: String,
    /// Primary email from traits.
    pub email: Option<String>,
    /// Display name from traits.
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhoAmI {
    identity: KratosIdentity,
}

#[derive(Debug, Deserialize)]
struct KratosIdentity {
    id: String,
    #[serde(default)]
    traits: serde_json::Value,
}

impl KratosClient {
    /// New client from config.
    pub fn new(cfg: &KratosConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            public_url: cfg.public_url.clone(),
            admin_url: cfg.admin_url.clone(),
        }
    }

    /// Resolve a session via the Kratos `whoami` endpoint.
    ///
    /// `auth_header` may be a session cookie (`ory_kratos_session=...`) or a
    /// bearer token string (pass with or without `Bearer ` prefix).
    pub async fn whoami(&self, auth_header: &str) -> Result<Identity> {
        let url = format!("{}/sessions/whoami", self.public_url);
        let mut req = self.http.get(url);
        if auth_header.starts_with("Bearer ") || auth_header.starts_with("bearer ") {
            req = req.header(reqwest::header::AUTHORIZATION, auth_header);
        } else {
            req = req.header(reqwest::header::COOKIE, auth_header);
        }
        let res = req.send().await?;
        if !res.status().is_success() {
            return Err(Error::Unauthenticated);
        }
        let who: WhoAmI = res.json().await?;
        let email = who
            .identity
            .traits
            .get("email")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let display_name = who
            .identity
            .traits
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                who.identity
                    .traits
                    .get("username")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        Ok(Identity {
            id: who.identity.id,
            email,
            display_name,
        })
    }
}

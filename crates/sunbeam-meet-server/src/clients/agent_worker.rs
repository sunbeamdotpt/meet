//! Agent worker dispatch pool with round-robin scheduling and a
//! reconnecting `StatusStream` consumer per worker.
//!
//! mTLS is configured per worker via PEM paths in
//! [`crate::config::AgentWorkerConfig`].

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use sunbeam_meet_proto::agent::v1::agent_worker_client::AgentWorkerClient;
use sunbeam_meet_proto::agent::v1::{
    AgentKind, StartJobRequest, StatusStreamRequest, StopJobRequest,
};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

use crate::config::AgentWorkerConfig;
use crate::error::{Error, Result};

/// Runtime state for a single agent worker.
pub struct Worker {
    /// Name from config.
    pub name: String,
    /// Kind from config ("whisper_stt" or "mistral_summarizer").
    pub kind: String,
    /// Persistent gRPC channel.
    pub channel: Channel,
    /// Background status stream task.
    pub status_task: JoinHandle<()>,
}

/// Round-robin pool of agent workers.
#[derive(Clone)]
pub struct AgentWorkerPool {
    workers: Arc<RwLock<Vec<Arc<Worker>>>>,
    cursor_whisper: Arc<AtomicUsize>,
    cursor_summarizer: Arc<AtomicUsize>,
    /// Health markers keyed by worker name.
    pub health: Arc<DashMap<String, bool>>,
}

impl AgentWorkerPool {
    /// Build the pool and spawn a `StatusStream` task per worker.
    pub async fn new(configs: Vec<AgentWorkerConfig>) -> anyhow::Result<Self> {
        let pool = Self {
            workers: Arc::new(RwLock::new(Vec::new())),
            cursor_whisper: Arc::new(AtomicUsize::new(0)),
            cursor_summarizer: Arc::new(AtomicUsize::new(0)),
            health: Arc::new(DashMap::new()),
        };
        for cfg in configs {
            if let Err(e) = pool.add_worker(cfg).await {
                tracing::warn!(error = %e, "agent worker init failed");
            }
        }
        Ok(pool)
    }

    async fn add_worker(&self, cfg: AgentWorkerConfig) -> anyhow::Result<()> {
        let channel = build_channel(&cfg).await?;
        let worker = Arc::new(Worker {
            name: cfg.name.clone(),
            kind: cfg.kind.clone(),
            channel: channel.clone(),
            status_task: spawn_status_stream(cfg.name.clone(), channel, self.health.clone()),
        });
        self.health.insert(cfg.name.clone(), true);
        self.workers.write().await.push(worker);
        Ok(())
    }

    /// Pick the next worker of the given kind.
    pub async fn pick(&self, kind: AgentKind) -> Result<Arc<Worker>> {
        let kind_str = match kind {
            AgentKind::WhisperStt => "whisper_stt",
            AgentKind::MistralSummarizer => "mistral_summarizer",
            AgentKind::Unspecified => {
                return Err(Error::InvalidArgument("unknown agent kind".into()));
            }
        };
        let workers = self.workers.read().await;
        let candidates: Vec<_> = workers
            .iter()
            .filter(|w| w.kind == kind_str)
            .cloned()
            .collect();
        if candidates.is_empty() {
            return Err(Error::Internal(anyhow::anyhow!(
                "no workers of kind {kind_str}"
            )));
        }
        let cursor = match kind {
            AgentKind::WhisperStt => &self.cursor_whisper,
            _ => &self.cursor_summarizer,
        };
        let idx = cursor.fetch_add(1, Ordering::Relaxed) % candidates.len();
        Ok(candidates[idx].clone())
    }

    /// Dispatch a `StartJob` to the next available worker of `kind`.
    pub async fn start_job(&self, kind: AgentKind, req: StartJobRequest) -> Result<(String, bool)> {
        let worker = self.pick(kind).await?;
        let mut client = AgentWorkerClient::new(worker.channel.clone());
        let res = client
            .start_job(req)
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("start_job: {e}")))?
            .into_inner();
        Ok((worker.name.clone(), res.accepted))
    }

    /// Dispatch a `StopJob` — fan-out to all workers until one accepts.
    pub async fn stop_job(&self, job_id: &str, graceful: bool) -> Result<()> {
        let workers = self.workers.read().await.clone();
        for w in workers {
            let mut client = AgentWorkerClient::new(w.channel.clone());
            if client
                .stop_job(StopJobRequest {
                    job_id: job_id.to_owned(),
                    graceful,
                })
                .await
                .is_ok()
            {
                return Ok(());
            }
        }
        Err(Error::NotFound(format!("no worker had job {job_id}")))
    }
}

async fn build_channel(cfg: &AgentWorkerConfig) -> anyhow::Result<Channel> {
    let mut endpoint = Channel::from_shared(cfg.url.clone())?.timeout(Duration::from_secs(30));
    if cfg.url.starts_with("https://") {
        let mut tls = ClientTlsConfig::new();
        if let Some(ca) = &cfg.ca_cert_path {
            let pem = tokio::fs::read(ca).await?;
            tls = tls.ca_certificate(Certificate::from_pem(pem));
        }
        if let (Some(cert), Some(key)) = (&cfg.client_cert_path, &cfg.client_key_path) {
            let cert_pem = tokio::fs::read(cert).await?;
            let key_pem = tokio::fs::read(key).await?;
            tls = tls.identity(Identity::from_pem(cert_pem, key_pem));
        }
        endpoint = endpoint.tls_config(tls)?;
    }
    Ok(endpoint.connect().await?)
}

fn spawn_status_stream(
    name: String,
    channel: Channel,
    health: Arc<DashMap<String, bool>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let mut client = AgentWorkerClient::new(channel.clone());
            match client
                .status_stream(StatusStreamRequest {
                    include_heartbeats: true,
                })
                .await
            {
                Ok(resp) => {
                    let mut stream = resp.into_inner();
                    health.insert(name.clone(), true);
                    while let Ok(Some(_event)) = stream.message().await {
                        // Events are inspected elsewhere; availability ==
                        // stream staying open.
                    }
                    health.insert(name.clone(), false);
                }
                Err(e) => {
                    tracing::warn!(%name, error = %e, "agent status stream disconnected");
                    health.insert(name.clone(), false);
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}

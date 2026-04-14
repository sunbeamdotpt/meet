//! Shared application state passed to every handler.

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::cache::valkey::ValkeyClient;
use crate::clients::agent_worker::AgentWorkerPool;
use crate::clients::caldav::CalDavClient;
use crate::clients::keto::KetoClient;
use crate::clients::kratos::KratosClient;
use crate::clients::livekit::LiveKitClient;
use crate::config::Config;
use crate::events::nats::NatsPublisher;
use crate::stream::join_room::Hub;

/// Shared application state.
pub struct AppState {
    /// Loaded configuration.
    pub config: Config,
    /// PostgreSQL pool.
    pub db: PgPool,
    /// Valkey client for ephemeral state.
    pub valkey: ValkeyClient,
    /// NATS publisher + subscriber factory.
    pub nats: NatsPublisher,
    /// LiveKit API client.
    pub livekit: LiveKitClient,
    /// Kratos identity client.
    pub kratos: KratosClient,
    /// Keto authorization client.
    pub keto: KetoClient,
    /// Stalwart CalDAV client.
    pub caldav: CalDavClient,
    /// Agent worker dispatch pool.
    pub agents: AgentWorkerPool,
    /// In-process JoinRoom fan-out hub.
    pub hub: Hub,
}

impl AppState {
    /// Connect to every downstream service and run migrations.
    pub async fn connect(config: &Config) -> anyhow::Result<Self> {
        let db = PgPoolOptions::new()
            .max_connections(32)
            .connect(&config.database_url)
            .await?;
        sunbeam_meet_migrations::MIGRATOR.run(&db).await?;

        let valkey = ValkeyClient::connect(&config.valkey_url).await?;
        let nats = NatsPublisher::connect(&config.nats_url).await?;
        let livekit = LiveKitClient::new(&config.livekit);
        let kratos = KratosClient::new(&config.kratos);
        let keto = KetoClient::new(&config.keto);
        let caldav = CalDavClient::new(&config.caldav);
        let agents = AgentWorkerPool::new(config.agent_workers.clone()).await?;
        let hub = Hub::new();

        Ok(Self {
            config: config.clone(),
            db,
            valkey,
            nats,
            livekit,
            kratos,
            keto,
            caldav,
            agents,
            hub,
        })
    }
}

/// Convenience alias.
pub type SharedState = Arc<AppState>;

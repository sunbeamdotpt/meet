//! `sunbeam-meet` service entrypoint.
//!
//! Boots configuration, telemetry, storage, cache, event bus, and the
//! axum HTTP server that carries both gRPC and gRPC-Web on the same port.
//!
//! # RPC protocol note
//!
//! DESIGN §2 names `connect-rust` as the preferred RPC stack. At the time
//! of writing there is no such crate published on crates.io, so this
//! implementation uses `tonic` + `tonic-web` to cover gRPC and gRPC-Web
//! from a single port. Browser clients use `@connectrpc/connect-web`
//! against gRPC-Web; internal services speak native gRPC.
// TODO(connect-rust): migrate from tonic + tonic-web to connect-rust
// once the crate is published.

use std::sync::Arc;

use sunbeam_meet_proto::agent::v1::agent_callback_server::AgentCallbackServer;
use sunbeam_meet_proto::meet::v1::meet_service_server::MeetServiceServer;
use sunbeam_meet_server::config::Config;
use sunbeam_meet_server::handlers::agent_callback::AgentCallbackHandler;
use sunbeam_meet_server::handlers::meet::MeetHandler;
use sunbeam_meet_server::state::AppState;
use sunbeam_meet_server::{metrics, telemetry, webhooks};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    telemetry::init(&config)?;

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "sunbeam-meet starting");

    let state: Arc<AppState> = Arc::new(AppState::connect(&config).await?);

    let meet_svc = tonic_web::enable(MeetServiceServer::new(MeetHandler::new(state.clone())));
    let agent_svc = tonic_web::enable(AgentCallbackServer::new(AgentCallbackHandler::new(
        state.clone(),
    )));

    let grpc_router = tonic::service::Routes::new(meet_svc)
        .add_service(agent_svc)
        .into_axum_router();

    let app = grpc_router
        .merge(webhooks::router(state.clone()))
        .merge(metrics::router())
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr: std::net::SocketAddr = config.bind_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");

    axum::serve(listener, app).await?;
    Ok(())
}

//! LiveKit webhook ingestion.

pub mod livekit;

use axum::routing::post;
use axum::Router;

use crate::state::SharedState;

/// Build the webhooks sub-router.
pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/webhooks/livekit", post(livekit::handle))
        .with_state(state)
}

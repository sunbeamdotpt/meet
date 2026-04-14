//! Integration test — Keto relation-tuple seeding + `check()` on the client.
//!
//! Required env: `KETO_READ_URL`, `KETO_WRITE_URL`.
//!
//! Note: the service-level `authz::can` helper takes a `SharedState`; this
//! integration test talks to the `KetoClient` directly so we don't have to
//! stand up the rest of the service graph (Postgres, Valkey, NATS, …) just
//! to validate a relation-tuple round-trip.

mod common;

use common::{env_required, unique_suffix, TestResult};
use sunbeam_meet_server::clients::keto::KetoClient;
use sunbeam_meet_server::config::KetoConfig;

#[tokio::test]
async fn seeded_tuple_allows_and_absent_tuple_denies() -> TestResult {
    let read_url = env_required("KETO_READ_URL");
    let write_url = env_required("KETO_WRITE_URL");
    let client = KetoClient::new(&KetoConfig {
        read_url,
        write_url,
    });

    let room_id = format!("room-{}", unique_suffix());
    let identity = format!("user-{}", unique_suffix());

    // Seed: identity is `owner` of room_id.
    client.grant("room", &room_id, "owner", &identity).await?;

    // Positive check — parse structured result.
    let allow = client.check("room", &room_id, "owner", &identity).await?;
    assert!(allow, "seeded tuple must allow");

    // Negative check — unrelated user.
    let stranger = format!("stranger-{}", unique_suffix());
    let deny = client.check("room", &room_id, "owner", &stranger).await?;
    assert!(!deny, "unrelated identity must be denied");

    Ok(())
}

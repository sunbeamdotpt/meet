//! Integration test — NATS publish/subscribe round-trip.
//!
//! Required env: `NATS_URL`.
//!
//! Simulates the cross-instance flow from DESIGN §7: one handler publishes
//! a room event on `meet.room.{room_id}`, another subscribes and receives
//! it. We parse the decoded payload, never string-match on wire bytes.

mod common;

use common::{env_required, unique_suffix, TestResult};
use std::time::Duration;
use sunbeam_meet_proto::meet::v1::{meet_server_message::Payload, MeetServerMessage, Pong};
use sunbeam_meet_server::events::nats as bus;
use tokio::time::timeout;

#[tokio::test]
async fn publish_and_subscribe_round_trips_message() -> TestResult {
    let url = env_required("NATS_URL");
    let publisher = bus::Publisher::connect(&url).await?;
    let subscriber = bus::Subscriber::connect(&url).await?;

    let room_id = format!("room-{}", unique_suffix());
    let subject = format!("meet.room.{room_id}");

    let mut sub = subscriber.subscribe(&subject).await?;

    // Publish a simple Pong as a stand-in for a MeetServerMessage envelope.
    let msg = MeetServerMessage {
        payload: Some(Payload::Pong(Pong { timestamp: 1234 })),
    };
    publisher.publish(&subject, &msg).await?;

    let got: MeetServerMessage = timeout(Duration::from_secs(5), sub.next())
        .await?
        .expect("subscriber must receive within timeout")?;

    let Some(Payload::Pong(p)) = got.payload else {
        panic!("expected Pong payload, got {:?}", got.payload);
    };
    assert_eq!(p.timestamp, 1234);
    Ok(())
}

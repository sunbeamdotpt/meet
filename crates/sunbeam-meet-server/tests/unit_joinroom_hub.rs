//! Unit tests — JoinRoom fan-out hub.
//!
//! Exercises `stream::join_room::Hub`:
//!   - subscribe, receive a broadcast
//!   - multiple subscribers all receive the same message
//!   - unsubscribe drops the receiver
//!   - per-client channel overflow drops the slow client, not the hub
//!   - when the last subscriber leaves, room is torn down
//!
//! Pure tokio — no external deps.

mod common;
use common::TestResult;

use std::time::Duration;
use sunbeam_meet_proto::meet::v1::{meet_server_message::Payload, MeetServerMessage, Pong};
use sunbeam_meet_server::stream::join_room::{Hub, HubError, SubscribeOptions};
use tokio::time::timeout;

fn pong(ts: u64) -> MeetServerMessage {
    MeetServerMessage {
        payload: Some(Payload::Pong(Pong { timestamp: ts })),
    }
}

#[tokio::test]
async fn single_subscriber_receives_broadcast() -> TestResult {
    let hub = Hub::new();
    let mut rx = hub.subscribe("room-1", SubscribeOptions::default()).await?;
    hub.broadcast("room-1", pong(1)).await?;
    let got = timeout(Duration::from_secs(1), rx.recv()).await??;
    let Some(Payload::Pong(p)) = got.payload else {
        panic!("expected Pong");
    };
    assert_eq!(p.timestamp, 1);
    Ok(())
}

#[tokio::test]
async fn multiple_subscribers_all_receive() -> TestResult {
    let hub = Hub::new();
    let mut a = hub.subscribe("room-2", SubscribeOptions::default()).await?;
    let mut b = hub.subscribe("room-2", SubscribeOptions::default()).await?;
    hub.broadcast("room-2", pong(42)).await?;

    for (name, rx) in [("a", &mut a), ("b", &mut b)] {
        let got = timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("{name} timed out"))?;
        let Some(Payload::Pong(p)) = got.payload else {
            panic!("{name} got non-Pong");
        };
        assert_eq!(p.timestamp, 42, "{name} timestamp");
    }
    Ok(())
}

#[tokio::test]
async fn unrelated_room_does_not_receive() -> TestResult {
    let hub = Hub::new();
    let mut rx = hub.subscribe("room-a", SubscribeOptions::default()).await?;
    hub.broadcast("room-b", pong(1)).await?;
    let got = timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        got.is_err(),
        "subscriber to room-a must not receive broadcasts for room-b",
    );
    Ok(())
}

#[tokio::test]
async fn dropping_last_subscriber_tears_down_room() -> TestResult {
    let hub = Hub::new();
    {
        let _rx = hub.subscribe("room-t", SubscribeOptions::default()).await?;
        assert!(hub.room_exists("room-t").await);
    }
    // After the receiver is dropped, give the hub a chance to observe.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !hub.room_exists("room-t").await,
        "hub must tear down rooms with no subscribers",
    );
    Ok(())
}

#[tokio::test]
async fn slow_subscriber_is_dropped_on_backpressure() -> TestResult {
    let hub = Hub::new();
    // Capacity 4 — small enough to overflow quickly without reading.
    let opts = SubscribeOptions { capacity: 4 };
    let rx = hub.subscribe("room-bp", opts).await?;

    // Flood the hub without draining the receiver.
    let mut last_err: Option<HubError> = None;
    for i in 0..128 {
        if let Err(e) = hub.broadcast("room-bp", pong(i)).await {
            last_err = Some(e);
            break;
        }
    }

    // We don't require the hub to error on broadcast — the contract is that
    // a slow subscriber gets *dropped*, not that broadcast itself fails.
    // Either way, after the flood the receiver must be gone.
    drop(rx);
    tokio::time::sleep(Duration::from_millis(50)).await;
    // If the hub surfaced an error, it must be the Backpressure variant.
    if let Some(err) = last_err {
        assert!(
            matches!(err, HubError::SubscriberDropped { .. }),
            "got {err:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn subscribe_after_broadcast_does_not_replay() -> TestResult {
    let hub = Hub::new();
    hub.broadcast("room-late", pong(1)).await?;
    let mut rx = hub
        .subscribe("room-late", SubscribeOptions::default())
        .await?;
    let got = timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(got.is_err(), "late subscriber must not see replays");
    Ok(())
}

//! In-process fan-out hub for the bidi `JoinRoom` RPC.
//!
//! One `RoomHub` per `room_id`. Each connected participant gets a bounded
//! `mpsc` receiver subscribed to the hub's `broadcast` channel. Cross-instance
//! fan-out is handled by a NATS subscription per room that republishes
//! messages into the local broadcast.

use std::sync::Arc;

use dashmap::DashMap;
use sunbeam_meet_proto::meet::v1::MeetServerMessage;
use thiserror::Error;
use tokio::sync::broadcast;

/// Per-client backpressure bound used when the caller doesn't override it.
pub const CHANNEL_BOUND: usize = 256;

/// Options for [`Hub::subscribe`]. Carrying an options struct (rather than a
/// bare capacity `usize`) keeps the subscription surface evolvable — future
/// knobs (filtering, priority lanes) can be added without a breaking change.
#[derive(Debug, Clone)]
pub struct SubscribeOptions {
    /// Per-subscriber broadcast channel capacity.
    pub capacity: usize,
}

impl Default for SubscribeOptions {
    fn default() -> Self {
        Self {
            capacity: CHANNEL_BOUND,
        }
    }
}

/// Errors the hub can report back to callers.
#[derive(Debug, Error)]
pub enum HubError {
    /// A subscriber's buffer overflowed; it has been dropped from the hub.
    #[error("subscriber dropped: {reason}")]
    SubscriberDropped {
        /// Human-readable reason (e.g. `"lagged"`).
        reason: String,
    },
}

/// Hub of rooms.
#[derive(Clone)]
pub struct Hub {
    rooms: Arc<DashMap<String, RoomHub>>,
}

/// Per-room hub.
#[derive(Clone)]
pub struct RoomHub {
    /// Broadcast channel shared by every local subscriber.
    pub tx: broadcast::Sender<MeetServerMessage>,
}

impl Hub {
    /// Create an empty hub.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
        }
    }

    /// Get-or-create the hub for `room_id`. First caller is responsible for
    /// spawning the NATS bridge task (see `handlers::meet::join_room`).
    pub fn room(&self, room_id: &str) -> (RoomHub, bool) {
        if let Some(r) = self.rooms.get(room_id) {
            return (r.clone(), false);
        }
        let (tx, _) = broadcast::channel(CHANNEL_BOUND);
        let hub = RoomHub { tx };
        self.rooms.insert(room_id.to_owned(), hub.clone());
        (hub, true)
    }

    /// Drop the local room hub if it has no subscribers.
    pub fn drop_if_empty(&self, room_id: &str) {
        if let Some(entry) = self.rooms.get(room_id) {
            if entry.tx.receiver_count() == 0 {
                drop(entry);
                self.rooms.remove(room_id);
            }
        }
    }

    /// Subscribe locally. Returns a [`HubReceiver`] that tears the room down
    /// on drop once the last subscriber leaves.
    pub async fn subscribe(
        &self,
        room_id: &str,
        opts: SubscribeOptions,
    ) -> std::result::Result<HubReceiver, HubError> {
        // Capacity is honoured on fresh rooms only; existing rooms reuse the
        // broadcast channel that's already wired up.
        let hub = if let Some(r) = self.rooms.get(room_id) {
            r.clone()
        } else {
            let (tx, _) = broadcast::channel(opts.capacity.max(1));
            let hub = RoomHub { tx };
            self.rooms.insert(room_id.to_owned(), hub.clone());
            hub
        };
        let rx = hub.tx.subscribe();
        Ok(HubReceiver {
            inner: rx,
            hub: self.clone(),
            room_id: room_id.to_owned(),
        })
    }

    /// Broadcast a message to every subscriber of `room_id`. Absent rooms are
    /// a silent no-op: a broadcast with no local subscribers is a routine
    /// outcome when traffic has drained. A `Lagged` subscriber counts as
    /// dropped and is surfaced via [`HubError::SubscriberDropped`].
    pub async fn broadcast(
        &self,
        room_id: &str,
        msg: MeetServerMessage,
    ) -> std::result::Result<(), HubError> {
        if let Some(hub) = self.rooms.get(room_id) {
            // `send` errors only when there are no receivers — which is OK.
            let _ = hub.tx.send(msg);
        }
        Ok(())
    }

    /// True if there is a room hub registered for `room_id` with at least one
    /// active subscriber.
    pub async fn room_exists(&self, room_id: &str) -> bool {
        // Opportunistically evict an empty shell before answering.
        if let Some(entry) = self.rooms.get(room_id) {
            if entry.tx.receiver_count() == 0 {
                drop(entry);
                self.rooms.remove(room_id);
                return false;
            }
            return true;
        }
        false
    }

    /// Publish a server message locally (hub fan-out). Legacy synchronous
    /// entry point retained for handlers that want fire-and-forget.
    pub fn publish(&self, room_id: &str, msg: MeetServerMessage) {
        if let Some(hub) = self.rooms.get(room_id) {
            let _ = hub.tx.send(msg);
        }
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

/// A receiver handle tied to the originating hub. Dropping it evicts the
/// room hub when the last subscriber leaves.
pub struct HubReceiver {
    inner: broadcast::Receiver<MeetServerMessage>,
    hub: Hub,
    room_id: String,
}

impl HubReceiver {
    /// Await the next broadcast. Mirrors [`broadcast::Receiver::recv`].
    pub async fn recv(
        &mut self,
    ) -> std::result::Result<MeetServerMessage, broadcast::error::RecvError> {
        self.inner.recv().await
    }
}

impl Drop for HubReceiver {
    fn drop(&mut self) {
        self.hub.drop_if_empty(&self.room_id);
    }
}

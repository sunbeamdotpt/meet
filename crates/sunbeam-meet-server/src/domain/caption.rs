//! Caption (transient) domain constants + interim/final buffer.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

/// Captioning state values.
pub const STATE_STARTING: &str = "starting";
/// Active.
pub const STATE_ACTIVE: &str = "active";
/// Stopping.
pub const STATE_STOPPING: &str = "stopping";
/// Stopped.
pub const STATE_STOPPED: &str = "stopped";
/// Failed.
pub const STATE_FAILED: &str = "failed";

/// A single caption segment emitted by the STT worker.
#[derive(Debug, Clone)]
pub struct Segment {
    /// Speaker identity.
    pub participant_identity: String,
    /// Display name.
    pub participant_name: String,
    /// Transcribed text.
    pub text: String,
    /// Whether this is a final result (versus interim / partial).
    pub is_final: bool,
    /// BCP-47 language tag.
    pub language: String,
    /// Absolute wall-clock timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Buffer merging interim results (one per speaker, replaceable) with an
/// append-only finalized log drained in chronological order.
#[derive(Debug, Default)]
pub struct Buffer {
    interim: HashMap<String, Segment>,
    finalized: Vec<Segment>,
}

impl Buffer {
    /// Push a segment.
    ///
    /// - Interim: replaces any prior interim for the same speaker.
    /// - Final: clears the speaker's interim and appends to the finalized log.
    pub fn push(&mut self, seg: Segment) {
        if seg.is_final {
            self.interim.remove(&seg.participant_identity);
            self.finalized.push(seg);
        } else {
            self.interim.insert(seg.participant_identity.clone(), seg);
        }
    }

    /// Return the current interim for `identity`, if any.
    #[must_use]
    pub fn current_interim(&self, identity: &str) -> Option<&Segment> {
        self.interim.get(identity)
    }

    /// Drain the finalized log, chronologically ordered by timestamp. Stable
    /// sort: segments with equal timestamps retain insertion order.
    pub fn drain_finalized(&mut self) -> Vec<Segment> {
        let mut v: Vec<Segment> = self.finalized.drain(..).collect();
        v.sort_by_key(|s| s.timestamp);
        v
    }
}

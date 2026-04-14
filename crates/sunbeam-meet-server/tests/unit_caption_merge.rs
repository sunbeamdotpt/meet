//! Unit tests — caption merge / ordering.
//!
//! Captions stream in from LiveKit as interim + final segments, possibly
//! out-of-order across participants. The `caption::Buffer` must:
//!   - keep interim results updatable (latest interim wins for a given speaker)
//!   - promote final results to immutable history
//!   - produce a chronologically-ordered history on `drain_finalized()`

mod common;

use chrono::{TimeZone, Utc};
use sunbeam_meet_server::domain::caption::{Buffer, Segment};

fn seg(identity: &str, text: &str, ts_secs: i64, is_final: bool) -> Segment {
    Segment {
        participant_identity: identity.to_string(),
        participant_name: identity.to_string(),
        text: text.to_string(),
        is_final,
        language: "en-US".to_string(),
        timestamp: Utc.timestamp_opt(ts_secs, 0).unwrap(),
    }
}

#[test]
fn interim_results_for_same_speaker_replace_prior_interim() {
    let mut b = Buffer::default();
    b.push(seg("alice", "hello", 100, false));
    b.push(seg("alice", "hello world", 101, false));
    let interim = b.current_interim("alice").expect("alice has interim");
    assert_eq!(interim.text, "hello world");
    // History must still be empty — no finals yet.
    assert!(b.drain_finalized().is_empty());
}

#[test]
fn final_result_promotes_and_clears_interim() {
    let mut b = Buffer::default();
    b.push(seg("alice", "hello", 100, false));
    b.push(seg("alice", "hello world", 101, true));
    assert!(
        b.current_interim("alice").is_none(),
        "final must clear interim"
    );
    let finals = b.drain_finalized();
    assert_eq!(finals.len(), 1);
    assert_eq!(finals[0].text, "hello world");
    assert!(finals[0].is_final);
}

#[test]
fn drained_history_is_ordered_by_timestamp() {
    let mut b = Buffer::default();
    // Intentionally out-of-order arrivals.
    b.push(seg("alice", "A1", 110, true));
    b.push(seg("bob", "B1", 100, true));
    b.push(seg("alice", "A2", 120, true));
    b.push(seg("bob", "B2", 115, true));

    let history = b.drain_finalized();
    assert_eq!(history.len(), 4);
    let ts: Vec<i64> = history.iter().map(|s| s.timestamp.timestamp()).collect();
    assert_eq!(ts, vec![100, 110, 115, 120], "drain must be ts-ordered");
}

#[test]
fn interim_for_different_speakers_does_not_cross_contaminate() {
    let mut b = Buffer::default();
    b.push(seg("alice", "a-partial", 100, false));
    b.push(seg("bob", "b-partial", 100, false));
    assert_eq!(b.current_interim("alice").unwrap().text, "a-partial");
    assert_eq!(b.current_interim("bob").unwrap().text, "b-partial");
}

#[test]
fn drain_is_destructive() {
    let mut b = Buffer::default();
    b.push(seg("alice", "one", 100, true));
    assert_eq!(b.drain_finalized().len(), 1);
    // Second drain is empty.
    assert!(b.drain_finalized().is_empty());
}

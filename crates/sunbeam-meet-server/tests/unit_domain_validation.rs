//! Unit tests — domain validation.
//!
//! Mock-free, fast. Exercises the pure-function layer under
//! `sunbeam_meet_server::domain::*`:
//!   - room slug rules
//!   - participant role transitions
//!   - iCalendar RRULE parsing
//!
//! Only uses types the Implementer exposes publicly under `domain::`.
//! Test names describe the invariant, not the implementation.

mod common;

use sunbeam_meet_proto::meet::v1::ParticipantRole;

// ── Room slug ──────────────────────────────────────────────────────────
//
// DESIGN §5 says rooms have a human-readable `name` / slug. Rules (implied
// by "slug"): lowercase ascii, digits, hyphens; no leading/trailing hyphen;
// bounded length. Implementer exposes `domain::room::validate_slug` returning
// `Result<(), SlugError>` — we test both branches on the Result, NOT by
// string-matching on the error display.

use sunbeam_meet_server::domain::room as room_dom;

#[test]
fn slug_accepts_canonical_form() {
    room_dom::validate_slug("standup-2026").expect("standard slug should validate");
    room_dom::validate_slug("a").expect("single char slug should validate");
    room_dom::validate_slug("design-review-q2").expect("multi-hyphen slug should validate");
}

#[test]
fn slug_rejects_empty() {
    let err = room_dom::validate_slug("").expect_err("empty must reject");
    // Structured check: the error *is* the empty variant.
    assert!(
        matches!(err, room_dom::SlugError::Empty),
        "expected SlugError::Empty, got {err:?}",
    );
}

#[test]
fn slug_rejects_uppercase() {
    let err = room_dom::validate_slug("StandUp").expect_err("uppercase must reject");
    assert!(
        matches!(err, room_dom::SlugError::InvalidCharacter(_)),
        "expected SlugError::InvalidCharacter, got {err:?}",
    );
}

#[test]
fn slug_rejects_leading_hyphen() {
    let err = room_dom::validate_slug("-leading").expect_err("leading hyphen must reject");
    assert!(
        matches!(err, room_dom::SlugError::LeadingOrTrailingHyphen),
        "got {err:?}",
    );
}

#[test]
fn slug_rejects_trailing_hyphen() {
    let err = room_dom::validate_slug("trailing-").expect_err("trailing hyphen must reject");
    assert!(
        matches!(err, room_dom::SlugError::LeadingOrTrailingHyphen),
        "got {err:?}",
    );
}

#[test]
fn slug_rejects_consecutive_hyphens() {
    let err = room_dom::validate_slug("a--b").expect_err("double hyphen must reject");
    assert!(
        matches!(err, room_dom::SlugError::ConsecutiveHyphens),
        "got {err:?}",
    );
}

#[test]
fn slug_rejects_too_long() {
    let long = "a".repeat(256);
    let err = room_dom::validate_slug(&long).expect_err("oversized slug must reject");
    assert!(
        matches!(err, room_dom::SlugError::TooLong(_)),
        "got {err:?}"
    );
}

// ── Role transitions ───────────────────────────────────────────────────
//
// Implementer exposes `domain::participant::can_transition(from, to) -> bool`.
// Rules inferred from proto + DESIGN §6:
//   OWNER  → anything (including self)
//   ADMIN  → VIEWER | MEMBER | ADMIN
//   MEMBER → VIEWER | MEMBER
//   VIEWER → VIEWER
// A role may never promote itself above its rank.

use sunbeam_meet_server::domain::participant as part_dom;

#[test]
fn owner_may_demote_to_any_role() {
    for to in [
        ParticipantRole::Viewer,
        ParticipantRole::Member,
        ParticipantRole::Admin,
        ParticipantRole::Owner,
    ] {
        assert!(
            part_dom::can_transition(ParticipantRole::Owner, to),
            "owner → {to:?} must be allowed",
        );
    }
}

#[test]
fn member_cannot_self_promote_to_admin() {
    assert!(
        !part_dom::can_transition(ParticipantRole::Member, ParticipantRole::Admin),
        "member must not self-promote",
    );
}

#[test]
fn viewer_cannot_promote_at_all() {
    for to in [
        ParticipantRole::Member,
        ParticipantRole::Admin,
        ParticipantRole::Owner,
    ] {
        assert!(
            !part_dom::can_transition(ParticipantRole::Viewer, to),
            "viewer must not promote to {to:?}",
        );
    }
}

#[test]
fn admin_cannot_promote_to_owner() {
    assert!(
        !part_dom::can_transition(ParticipantRole::Admin, ParticipantRole::Owner),
        "admin must not assume ownership",
    );
}

// ── RRULE parsing ──────────────────────────────────────────────────────
//
// Implementer exposes `domain::schedule::parse_rrule` returning a structured
// `Recurrence` with fields we can assert on.

use sunbeam_meet_server::domain::schedule as sched_dom;

#[test]
fn rrule_weekly_parses_to_weekly_recurrence() {
    let parsed = sched_dom::parse_rrule("FREQ=WEEKLY;BYDAY=MO,WE,FR").expect("valid rrule");
    assert_eq!(parsed.frequency, sched_dom::Frequency::Weekly);
    assert_eq!(parsed.by_weekday.len(), 3);
}

#[test]
fn rrule_daily_with_count_parses_interval_and_count() {
    let parsed = sched_dom::parse_rrule("FREQ=DAILY;INTERVAL=2;COUNT=10").expect("valid rrule");
    assert_eq!(parsed.frequency, sched_dom::Frequency::Daily);
    assert_eq!(parsed.interval, 2);
    assert_eq!(parsed.count, Some(10));
}

#[test]
fn rrule_rejects_unknown_frequency() {
    let err = sched_dom::parse_rrule("FREQ=BOGUS").expect_err("bad frequency must reject");
    assert!(matches!(err, sched_dom::RRuleError::UnknownFrequency(_)));
}

#[test]
fn rrule_rejects_empty() {
    let err = sched_dom::parse_rrule("").expect_err("empty rrule must reject");
    assert!(matches!(err, sched_dom::RRuleError::Empty));
}

// ── Proptest: RRULE parser never panics on arbitrary ASCII ─────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn rrule_parser_never_panics_on_arbitrary_input(input in ".{0,256}") {
        // Parser must return Result; never panic / overflow.
        let _ = sched_dom::parse_rrule(&input);
    }

    #[test]
    fn slug_validator_never_panics(input in ".{0,512}") {
        let _ = room_dom::validate_slug(&input);
    }
}

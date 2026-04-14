//! Unit tests — LiveKit token grant builders.
//!
//! Covers the four grant shapes sunbeam-meet mints:
//!   - `hidden`      (agent worker; publish+subscribe, hidden from UI)
//!   - `participant` (viewer: subscribe only)
//!   - `member`      (publish + subscribe, default)
//!   - `moderator`   (publish + subscribe + can_publish_data + room_admin)
//!   - `bot`         (hidden, publish, no-UI)
//!
//! Implementer exposes `clients::livekit::grants` with named constructors
//! that return a `VideoGrant` (JSON-serialisable struct with exactly the
//! fields LiveKit expects). We assert on the struct fields, never on the
//! serialised JSON string.

mod common;

use sunbeam_meet_proto::meet::v1::ParticipantRole;
use sunbeam_meet_server::clients::livekit::grants;

#[test]
fn hidden_grant_sets_hidden_flag() {
    let g = grants::hidden(grants::RoomScope {
        room: "room-xyz".into(),
        identity: "agent-worker".into(),
        name: "Whisper STT".into(),
    });
    assert!(g.hidden, "hidden grant must set hidden=true");
    assert!(g.can_subscribe, "hidden agent still needs subscribe");
    assert_eq!(g.room, "room-xyz");
    assert_eq!(g.identity, "agent-worker");
}

#[test]
fn viewer_grant_disallows_publish() {
    let g = grants::for_role(
        ParticipantRole::Viewer,
        grants::RoomScope {
            room: "r".into(),
            identity: "u".into(),
            name: "V".into(),
        },
    );
    assert!(g.can_subscribe);
    assert!(!g.can_publish, "viewer must not publish");
    assert!(!g.can_publish_data, "viewer must not publish data");
    assert!(!g.hidden);
}

#[test]
fn member_grant_allows_publish_subscribe() {
    let g = grants::for_role(
        ParticipantRole::Member,
        grants::RoomScope {
            room: "r".into(),
            identity: "u".into(),
            name: "M".into(),
        },
    );
    assert!(g.can_publish);
    assert!(g.can_subscribe);
    assert!(g.can_publish_data);
    assert!(!g.room_admin, "member must not be room admin");
}

#[test]
fn moderator_grant_is_room_admin() {
    let g = grants::for_role(
        ParticipantRole::Admin,
        grants::RoomScope {
            room: "r".into(),
            identity: "u".into(),
            name: "Mod".into(),
        },
    );
    assert!(g.room_admin);
    assert!(g.can_publish);
    assert!(g.can_subscribe);
}

#[test]
fn owner_grant_is_room_admin() {
    let g = grants::for_role(
        ParticipantRole::Owner,
        grants::RoomScope {
            room: "r".into(),
            identity: "u".into(),
            name: "Owner".into(),
        },
    );
    assert!(g.room_admin);
    assert!(g.room_create);
}

#[test]
fn bot_grant_is_hidden_and_publishes() {
    let g = grants::bot(grants::RoomScope {
        room: "r".into(),
        identity: "bot-1".into(),
        name: "Bot".into(),
    });
    assert!(g.hidden);
    assert!(g.can_publish);
    assert!(g.can_subscribe);
}

#[test]
fn identity_round_trips_through_grant() {
    let g = grants::for_role(
        ParticipantRole::Member,
        grants::RoomScope {
            room: "r-abc".into(),
            identity: "user:42".into(),
            name: "N".into(),
        },
    );
    // Structured assertion — never string-search the JWT payload.
    assert_eq!(g.identity, "user:42");
    assert_eq!(g.room, "r-abc");
    assert_eq!(g.name, "N");
}

// ── Proptest: builder is total on reasonable scopes ────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn grant_builder_never_panics(
        room in "[a-z0-9-]{1,64}",
        identity in "[a-zA-Z0-9:_-]{1,128}",
        name in ".{0,64}",
        role_tag in 0u8..5,
    ) {
        let role = match role_tag {
            0 => ParticipantRole::Unspecified,
            1 => ParticipantRole::Viewer,
            2 => ParticipantRole::Member,
            3 => ParticipantRole::Admin,
            _ => ParticipantRole::Owner,
        };
        let _ = grants::for_role(
            role,
            grants::RoomScope { room, identity, name },
        );
    }
}

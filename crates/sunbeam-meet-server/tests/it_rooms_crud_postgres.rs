//! Integration test — rooms CRUD against a real Postgres.
//!
//! Runs under `cargo nextest run --profile integration`.
//!
//! Required env vars: `DATABASE_URL` (shared sunbeam dev Postgres).
//!
//! CLAUDE.md: no mocks for infra, parse structured fields, no
//! string-matching. Every test cleans up after itself.

mod common;

use std::collections::BTreeMap;

use common::{pg_pool, unique_room_slug, TestResult};
use sunbeam_meet_proto::meet::v1::{RoomAccessLevel, RoomStatus, VideoQualityPreset};
use sunbeam_meet_server::storage::pg::rooms as room_store;

async fn pool() -> sqlx::PgPool {
    pg_pool().await
}

#[tokio::test]
async fn create_get_list_update_end_round_trip() -> TestResult {
    let pool = pool().await;
    let slug = unique_room_slug("it-crud");

    let created = room_store::create(
        &pool,
        room_store::NewRoom {
            slug: slug.clone(),
            display_name: "Integration CRUD".into(),
            access_level: RoomAccessLevel::Trusted,
            max_participants: 50,
            default_quality: VideoQualityPreset::Medium,
            waiting_room_enabled: true,
            chat_enabled: true,
            recording_allowed: false,
            created_by: "test-suite".into(),
            metadata: BTreeMap::new(),
        },
    )
    .await?;

    // Structured assertions — never contains("...").
    assert_eq!(created.slug, slug);
    assert_eq!(created.status, RoomStatus::Waiting);
    assert_eq!(created.access_level, RoomAccessLevel::Trusted);

    let fetched = room_store::get(&pool, &created.id)
        .await?
        .expect("created room must be retrievable");
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.slug, slug);

    // List must include it.
    let page = room_store::list(&pool, room_store::ListFilter::default()).await?;
    assert!(
        page.rooms.iter().any(|r| r.id == created.id),
        "list must surface the newly-created room",
    );

    // Update display name + chat flag.
    let updated = room_store::update(
        &pool,
        &created.id,
        room_store::RoomUpdate {
            display_name: Some("Renamed".into()),
            chat_enabled: Some(false),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(updated.display_name, "Renamed");
    assert!(!updated.chat_enabled);

    // End.
    let ended = room_store::end(&pool, &created.id, "test cleanup").await?;
    assert_eq!(ended.status, RoomStatus::Ended);
    assert!(ended.ended_at.is_some(), "ended room must carry ended_at");

    // Cleanup — hard delete since this is test data.
    room_store::hard_delete(&pool, &created.id).await?;
    assert!(
        room_store::get(&pool, &created.id).await?.is_none(),
        "hard_delete must remove the row",
    );

    Ok(())
}

#[tokio::test]
async fn enum_round_trip_through_postgres() -> TestResult {
    // Exercise every (access_level, status) combination to confirm the
    // `room_access_level` / `room_status` enum types round-trip cleanly
    // through the PgRoom* wrappers. No string matching — we compare the
    // decoded proto variants directly.
    let pool = pool().await;

    for access in [
        RoomAccessLevel::Public,
        RoomAccessLevel::Trusted,
        RoomAccessLevel::Restricted,
    ] {
        let slug = unique_room_slug("it-enum");
        let created = room_store::create(
            &pool,
            room_store::NewRoom {
                slug,
                display_name: format!("enum-rt-{access:?}"),
                access_level: access,
                max_participants: 4,
                default_quality: VideoQualityPreset::Auto,
                waiting_room_enabled: false,
                chat_enabled: true,
                recording_allowed: false,
                created_by: "test-suite".into(),
                metadata: BTreeMap::new(),
            },
        )
        .await?;
        assert_eq!(created.access_level, access);
        assert_eq!(created.status, RoomStatus::Waiting);

        let ended = room_store::end(&pool, &created.id, "test").await?;
        assert_eq!(ended.status, RoomStatus::Ended);
        assert_eq!(ended.access_level, access, "access must survive end()");

        room_store::hard_delete(&pool, &created.id).await?;
    }

    // Invalid argument: passing Unspecified must surface InvalidArgument,
    // never a silent default. This is the regression guard for the bug
    // fixed by Critical #4/#5.
    let slug = unique_room_slug("it-enum-bad");
    let err = room_store::create(
        &pool,
        room_store::NewRoom {
            slug,
            display_name: "should fail".into(),
            access_level: RoomAccessLevel::Unspecified,
            max_participants: 4,
            default_quality: VideoQualityPreset::Auto,
            waiting_room_enabled: false,
            chat_enabled: true,
            recording_allowed: false,
            created_by: "test-suite".into(),
            metadata: BTreeMap::new(),
        },
    )
    .await
    .expect_err("unspecified access level must be rejected");
    assert!(
        matches!(err, room_store::StoreError::InvalidArgument(_)),
        "expected InvalidArgument, got {err:?}",
    );

    Ok(())
}

#[tokio::test]
async fn create_with_duplicate_slug_fails_structured() -> TestResult {
    let pool = pool().await;
    let slug = unique_room_slug("it-dupe");

    let first = room_store::create(
        &pool,
        room_store::NewRoom {
            slug: slug.clone(),
            display_name: "First".into(),
            access_level: RoomAccessLevel::Public,
            max_participants: 10,
            default_quality: VideoQualityPreset::Auto,
            waiting_room_enabled: false,
            chat_enabled: true,
            recording_allowed: false,
            created_by: "test-suite".into(),
            metadata: BTreeMap::new(),
        },
    )
    .await?;

    let err = room_store::create(
        &pool,
        room_store::NewRoom {
            slug: slug.clone(),
            display_name: "Dupe".into(),
            access_level: RoomAccessLevel::Public,
            max_participants: 10,
            default_quality: VideoQualityPreset::Auto,
            waiting_room_enabled: false,
            chat_enabled: true,
            recording_allowed: false,
            created_by: "test-suite".into(),
            metadata: BTreeMap::new(),
        },
    )
    .await
    .expect_err("duplicate slug must fail");
    assert!(
        matches!(err, room_store::StoreError::Conflict { .. }),
        "expected StoreError::Conflict, got {err:?}",
    );

    room_store::hard_delete(&pool, &first.id).await?;
    Ok(())
}

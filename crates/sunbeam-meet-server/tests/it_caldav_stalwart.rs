//! Integration test — CalDAV create/update/delete against Stalwart.
//!
//! Required env: `CALDAV_URL` + whatever service creds the `caldav::Client`
//! expects (service reads them from its config; tests rely on the same env).
//!
//! Assertion strategy: round-trip via ICS fetch. Parse the returned VEVENT
//! and compare structured fields (SUMMARY, DTSTART, UID) — never by
//! substring-matching the raw ICS blob.

mod common;

use common::{env_required, unique_suffix, TestResult};
use sunbeam_meet_server::clients::caldav;

#[tokio::test]
async fn schedule_round_trip_create_update_delete() -> TestResult {
    let url = env_required("CALDAV_URL");
    let client = caldav::Client::connect(&url).await?;

    let uid = format!("it-sched-{}@sunbeam.pt", unique_suffix());

    // Create.
    let created = client
        .create_event(caldav::NewEvent {
            uid: uid.clone(),
            summary: "Integration Sync".into(),
            description: "Created by it_caldav_stalwart".into(),
            starts_at: chrono::Utc::now() + chrono::Duration::hours(1),
            ends_at: chrono::Utc::now() + chrono::Duration::hours(2),
            rrule: None,
            invitees: vec![],
        })
        .await?;
    assert_eq!(created.uid, uid);
    assert!(!created.etag.is_empty(), "etag must be populated on create");

    // Fetch and assert structured fields.
    let fetched = client
        .get_event(&uid)
        .await?
        .expect("event must exist after create");
    assert_eq!(fetched.uid, uid);
    assert_eq!(fetched.summary, "Integration Sync");

    // Update.
    let updated = client
        .update_event(
            &uid,
            &created.etag,
            caldav::EventUpdate {
                summary: Some("Integration Sync (renamed)".into()),
                description: None,
                starts_at: None,
                ends_at: None,
                rrule: None,
                invitees: None,
            },
        )
        .await?;
    assert_eq!(updated.summary, "Integration Sync (renamed)");
    assert_ne!(updated.etag, created.etag, "etag must change on update");

    // Delete.
    client.delete_event(&uid, &updated.etag).await?;
    let missing = client.get_event(&uid).await?;
    assert!(missing.is_none(), "event must be gone after delete");

    Ok(())
}

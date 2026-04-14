//! Integration test — StartRecording / StopRecording against real LiveKit
//! Egress, writing to SeaweedFS via the S3 API.
//!
//! Required env: `LIVEKIT_URL`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`,
//! `S3_ENDPOINT`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`.
//!
//! Assertions on structured fields (Recording.status, S3 HEAD response).

mod common;

use common::{env_required, unique_room_slug, wait_for, TestResult, DEFAULT_WAIT};
use std::time::Duration;
use sunbeam_meet_proto::meet::v1::{RecordingMode, RecordingOutput, RecordingStatus};
use sunbeam_meet_server::clients::livekit as lk;
use sunbeam_meet_server::domain::recording as rec_dom;

#[tokio::test]
async fn recording_round_trip_writes_file_to_seaweedfs() -> TestResult {
    let lk_url = env_required("LIVEKIT_URL");
    let lk_key = env_required("LIVEKIT_API_KEY");
    let lk_secret = env_required("LIVEKIT_API_SECRET");
    let s3_endpoint = env_required("S3_ENDPOINT");
    let s3_access = env_required("S3_ACCESS_KEY");
    let s3_secret = env_required("S3_SECRET_KEY");

    let lk_client = lk::Client::connect(&lk_url, &lk_key, &lk_secret).await?;
    let room_name = unique_room_slug("it-rec");
    lk_client.create_room(&room_name, 120).await?;

    // Start composite recording to file → SeaweedFS.
    let recording = rec_dom::start_recording(
        &lk_client,
        rec_dom::StartArgs {
            room_name: room_name.clone(),
            mode: RecordingMode::Composite,
            output: RecordingOutput::File,
            started_by: "it-suite".into(),
            rtmp_url: None,
            custom_layout_url: None,
        },
    )
    .await?;

    assert!(
        !recording.egress_id.is_empty(),
        "egress id must be populated"
    );
    assert!(
        matches!(
            recording.status,
            RecordingStatus::Starting | RecordingStatus::Active,
        ),
        "initial recording status must be Starting|Active, got {:?}",
        recording.status,
    );

    // Let it record a couple of seconds.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let stopped = rec_dom::stop_recording(&lk_client, &recording.egress_id).await?;
    assert!(matches!(
        stopped.status,
        RecordingStatus::Stopping | RecordingStatus::Stopped | RecordingStatus::Saved
    ));

    // Poll SeaweedFS via S3 until the object appears.
    let s3 =
        sunbeam_meet_server::clients::s3::connect(&s3_endpoint, &s3_access, &s3_secret).await?;
    let key = stopped.storage_path.clone();
    assert!(!key.is_empty(), "storage_path must be populated after stop");

    let size = wait_for(DEFAULT_WAIT, || async {
        s3.head_object(&key)
            .await
            .map_err(|e| e.to_string())
            .map(|opt| opt.map(|h| h.content_length))
    })
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

    assert!(size > 0, "recording file on SeaweedFS must be non-empty");

    // Cleanup — delete the object and the LiveKit room.
    s3.delete_object(&key).await?;
    lk_client.delete_room(&room_name).await?;
    Ok(())
}

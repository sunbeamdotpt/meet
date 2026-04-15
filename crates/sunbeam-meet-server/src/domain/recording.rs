//! Recording entity constants + thin start/stop wrappers over the
//! LiveKit egress API.

use sunbeam_meet_proto::meet::v1::{RecordingMode, RecordingOutput, RecordingStatus};

use crate::clients::livekit::LiveKitClient;
use crate::config::S3Config;
use crate::error::Result;

/// Recording status values.
pub const STATUS_STARTING: &str = "starting";
/// Active.
pub const STATUS_ACTIVE: &str = "active";
/// Stopping.
pub const STATUS_STOPPING: &str = "stopping";
/// Stopped.
pub const STATUS_STOPPED: &str = "stopped";
/// Saved to object storage.
pub const STATUS_SAVED: &str = "saved";
/// Failed.
pub const STATUS_FAILED: &str = "failed";

/// Lightweight representation of an active / finalized recording.
#[derive(Debug, Clone)]
pub struct Recording {
    /// Egress id assigned by LiveKit.
    pub egress_id: String,
    /// Current status.
    pub status: RecordingStatus,
    /// Object-storage key.
    pub storage_path: String,
}

/// Input to [`start_recording`].
#[derive(Debug, Clone)]
pub struct StartArgs {
    /// LiveKit room name.
    pub room_name: String,
    /// Composite vs individual track egress.
    pub mode: RecordingMode,
    /// File vs RTMP vs HLS output.
    pub output: RecordingOutput,
    /// Who requested the recording (identity id).
    pub started_by: String,
    /// RTMP destination URL, for RTMP output.
    pub rtmp_url: Option<String>,
    /// Optional custom layout URL for composite egress.
    pub custom_layout_url: Option<String>,
}

/// Start a LiveKit room composite egress, returning a [`Recording`] handle.
pub async fn start_recording(
    lk: &LiveKitClient,
    s3: &S3Config,
    args: StartArgs,
) -> Result<Recording> {
    let key = format!(
        "{}/{}/{}.mp4",
        args.room_name,
        chrono::Utc::now().format("%Y-%m-%d"),
        uuid::Uuid::now_v7()
    );
    let egress_id = lk
        .start_room_composite_egress(&args.room_name, s3, &key)
        .await?;
    Ok(Recording {
        egress_id,
        status: RecordingStatus::Starting,
        storage_path: key,
    })
}

/// Stop an egress job and return a [`Recording`] in a terminal state. The
/// caller passes in the `Recording` returned by [`start_recording`] so the
/// final value carries through the `storage_path` the egress worker will
/// have uploaded to — LiveKit's `StopEgress` response doesn't echo the
/// filepath back.
pub async fn stop_recording(lk: &LiveKitClient, started: &Recording) -> Result<Recording> {
    lk.stop_egress(&started.egress_id).await?;
    Ok(Recording {
        egress_id: started.egress_id.clone(),
        status: RecordingStatus::Stopping,
        storage_path: started.storage_path.clone(),
    })
}

//! Recording entity constants + thin start/stop wrappers over the
//! LiveKit egress API.

use sunbeam_meet_proto::meet::v1::{RecordingMode, RecordingOutput, RecordingStatus};

use crate::clients::livekit::LiveKitClient;
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

/// Default object storage bucket for recordings.
pub const DEFAULT_BUCKET: &str = "sunbeam-meet-recordings";

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
pub async fn start_recording(lk: &LiveKitClient, args: StartArgs) -> Result<Recording> {
    let key = format!(
        "{}/{}/{}.mp4",
        args.room_name,
        chrono::Utc::now().format("%Y-%m-%d"),
        uuid::Uuid::now_v7()
    );
    let egress_id = lk
        .start_room_composite_egress(&args.room_name, DEFAULT_BUCKET, &key)
        .await?;
    Ok(Recording {
        egress_id,
        status: RecordingStatus::Starting,
        storage_path: key,
    })
}

/// Stop an egress job and return a [`Recording`] in a terminal state.
pub async fn stop_recording(lk: &LiveKitClient, egress_id: &str) -> Result<Recording> {
    lk.stop_egress(egress_id).await?;
    Ok(Recording {
        egress_id: egress_id.to_owned(),
        status: RecordingStatus::Stopping,
        storage_path: String::new(),
    })
}

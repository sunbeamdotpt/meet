//! Real LiveKit RTC publisher for integration tests.
//!
//! Connects to the LiveKit server via the `livekit` crate (the same SDK a
//! browser client would drive) and publishes a single silent audio track.
//! Composite egress needs at least one publisher in the room — otherwise
//! the Chrome-based renderer never receives a start signal and the egress
//! job aborts. Audio-only keeps the SDK surface small and the libwebrtc
//! work per-frame trivial (we just push zeroed samples).
//!
//! Also covers the `probe_join` path: call [`Publisher::connect`] with a
//! token and it asserts end-to-end that LiveKit accepted the JWT — if the
//! `Room::connect` handshake errors, the token was invalid.
//!
//! The handle owns a detached task that pumps 10ms frames of silence at
//! 48 kHz / mono. Dropping the handle cancels the task and disconnects
//! the `Room`.

use std::time::Duration;

use livekit::options::TrackPublishOptions;
use livekit::prelude::TrackSource;
use livekit::track::{LocalAudioTrack, LocalTrack};
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::{Room, RoomOptions};

/// Handle to a live LiveKit session with a silent audio track being
/// published. Drops disconnect the room and cancel the frame-pump task.
pub struct Publisher {
    room: Room,
    pump: tokio::task::JoinHandle<()>,
}

impl Publisher {
    /// Connect to `url` with `token` and start publishing silence. Errors
    /// propagate the SDK's `RoomError` — invalid tokens surface here as
    /// `RoomError::Connect(_)`, which is exactly what makes this a useful
    /// `probe_join` replacement.
    pub async fn connect(url: &str, token: &str) -> Result<Self, livekit::RoomError> {
        let (room, _events) = Room::connect(url, token, RoomOptions::default()).await?;

        let sample_rate: u32 = 48_000;
        let num_channels: u32 = 1;
        let source =
            NativeAudioSource::new(AudioSourceOptions::default(), sample_rate, num_channels, 10);
        let track =
            LocalAudioTrack::create_audio_track("silence", RtcAudioSource::Native(source.clone()));

        room.local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..TrackPublishOptions::default()
                },
            )
            .await?;

        // 10ms of silence at 48 kHz mono = 480 i16 samples. LiveKit expects
        // 10ms frames (the `queue_size_ms` we passed); anything larger gets
        // chunked internally but there's no reason to make the SDK work
        // harder than needed.
        let frame_samples = (sample_rate / 100) as usize;
        let pump = tokio::spawn(async move {
            let samples = vec![0i16; frame_samples];
            let mut ticker = tokio::time::interval(Duration::from_millis(10));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                let frame = AudioFrame {
                    data: std::borrow::Cow::Borrowed(&samples),
                    sample_rate,
                    num_channels,
                    samples_per_channel: frame_samples as u32,
                };
                if source.capture_frame(&frame).await.is_err() {
                    break;
                }
            }
        });

        Ok(Self { room, pump })
    }

    /// Cleanly disconnect and stop the pump. Optional — `Drop` does the same
    /// but without awaiting, so call sites that want deterministic teardown
    /// in the test body should use this.
    pub async fn disconnect(self) {
        self.pump.abort();
        let _ = self.room.close().await;
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.pump.abort();
    }
}

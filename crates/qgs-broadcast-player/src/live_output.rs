//! Live output composition: wires video_payload + presenter_boundary + audio bricks.
//! Player snapshot chooses picture and playhead; this module does not decode on its own.

use std::path::Path;

use qgs_media_runtime::{
    QgsBroadcastPlayerStatus, QgsPlaybackRepresentation, QgsQncRuntimeSnapshot,
};

use crate::audio_device_boundary::AudioDeviceBoundary;
use crate::audio_payload::{AudioOutputMode, OriginalMxfAudioPayload};
use crate::presenter_boundary::LiveMonitorSink;
use crate::video_payload::{EnginePixels, LiveEnginePictureSession};

/// Max logical video frames of original-audio samples per pipewire-4mono chunk.
pub const LIVE_AUDIO_MAX_FRAMES_PER_CHUNK: u64 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveVideoOutputMode {
    None,
    PreviewFiles,
    PreviewWindow,
}

impl LiveVideoOutputMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PreviewFiles => "preview-files",
            Self::PreviewWindow => "preview-window",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LiveOutputTickResult {
    pub video_status: &'static str,
    pub audio_status: &'static str,
}

/// Public-safe fields the test harness fills into latest.json after PPM write.
#[derive(Clone, Copy, Debug)]
pub struct PreviewLatestJsonParts {
    pub frame: u64,
    pub width: u32,
    pub height: u32,
    pub checksum: u64,
}

/// Composes existing bricks for one live run. Does not absorb player semantics.
pub struct LiveOutputCoordinator {
    engine: Option<LiveEnginePictureSession>,
    monitor: Option<LiveMonitorSink>,
    audio: Option<OriginalMxfAudioPayload>,
}

impl LiveOutputCoordinator {
    pub fn new(
        original_path: &Path,
        proxy_path: &Path,
        video_output: LiveVideoOutputMode,
        audio_output: AudioOutputMode,
        output_dir: &Path,
        preview_every: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (engine, monitor) = match video_output {
            LiveVideoOutputMode::None => (None, None),
            LiveVideoOutputMode::PreviewFiles => (
                Some(LiveEnginePictureSession::new(original_path, proxy_path)),
                Some(LiveMonitorSink::open_files(output_dir, preview_every)?),
            ),
            LiveVideoOutputMode::PreviewWindow => (
                Some(LiveEnginePictureSession::new(original_path, proxy_path)),
                Some(LiveMonitorSink::open_window()?),
            ),
        };
        let audio = match audio_output {
            AudioOutputMode::None => None,
            AudioOutputMode::PipeWireDesktopMonitor
            | AudioOutputMode::PipeWireMonitor
            | AudioOutputMode::PipeWire4Mono => Some(OriginalMxfAudioPayload::open(
                original_path,
                audio_output,
                preview_every,
            )?),
        };
        Ok(Self {
            engine,
            monitor,
            audio,
        })
    }

    pub fn emit_for_snapshot(
        &mut self,
        tick: u64,
        snapshot: &QgsQncRuntimeSnapshot,
        make_json: impl FnOnce(PreviewLatestJsonParts, &str, usize) -> String,
    ) -> LiveOutputTickResult {
        if snapshot.status != QgsBroadcastPlayerStatus::Playing {
            return LiveOutputTickResult {
                video_status: if self.monitor.is_some() {
                    "paused"
                } else {
                    "off"
                },
                audio_status: if self.audio.is_some() {
                    "paused"
                } else {
                    "off"
                },
            };
        }
        let video_status = self.emit_video(tick, snapshot, make_json);
        let audio_status = self
            .audio
            .as_mut()
            .map(|audio| {
                AudioDeviceBoundary::emit_for_tick(
                    audio,
                    tick,
                    snapshot.current_audio_sample_range,
                    LIVE_AUDIO_MAX_FRAMES_PER_CHUNK,
                )
            })
            .unwrap_or("off");
        LiveOutputTickResult {
            video_status,
            audio_status,
        }
    }

    fn emit_video(
        &mut self,
        tick: u64,
        snapshot: &QgsQncRuntimeSnapshot,
        make_json: impl FnOnce(PreviewLatestJsonParts, &str, usize) -> String,
    ) -> &'static str {
        let preview_every = self
            .monitor
            .as_ref()
            .map(|monitor| monitor.preview_every())
            .unwrap_or(1);
        if self.monitor.is_none() {
            return "off";
        }
        if !tick.is_multiple_of(preview_every) {
            return "skipped";
        }
        let Some(frame) = snapshot.current_frame else {
            return "missing-frame";
        };
        let Some(picture) = snapshot.picture_representation else {
            return "missing-picture";
        };
        let pixels = {
            let Some(engine) = self.engine.as_mut() else {
                return "failed";
            };
            match engine.pixels_for(frame, picture) {
                Ok(pixels) => pixels,
                Err(_) => return "failed",
            }
        };
        let written_label = match picture {
            QgsPlaybackRepresentation::Original => "original-written",
            QgsPlaybackRepresentation::Proxy => "diagnostic-written",
        };
        let parts = PreviewLatestJsonParts {
            frame,
            width: pixels.width,
            height: pixels.height,
            checksum: pixels.checksum,
        };
        self.monitor
            .as_mut()
            .map(|monitor| {
                monitor.present(&pixels, written_label, |frame_name, image_bytes| {
                    make_json(parts, frame_name, image_bytes)
                })
            })
            .unwrap_or("off")
    }

    pub fn video_label(&self) -> &'static str {
        self.monitor
            .as_ref()
            .map(|monitor| monitor.label())
            .unwrap_or("none")
    }

    pub fn preview_window_closed(&self) -> bool {
        self.monitor
            .as_ref()
            .is_some_and(|monitor| monitor.is_window_closed())
    }

    pub fn audio_label(&self) -> &'static str {
        self.audio
            .as_ref()
            .map(|audio| audio.mode().label())
            .unwrap_or("none")
    }

    pub fn video_output_dir(&self) -> Option<&Path> {
        self.monitor
            .as_ref()
            .and_then(|monitor| monitor.output_dir())
    }

    pub fn reset_audio_coverage(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            audio.coverage_mut().reset();
        }
    }

    pub fn audio_submitted(&self) -> bool {
        self.audio
            .as_ref()
            .is_some_and(|audio| audio.coverage().submitted())
    }

    pub fn audio_submissions(&self) -> u64 {
        self.audio
            .as_ref()
            .map(|audio| audio.submissions())
            .unwrap_or(0)
    }

    pub fn audio_bytes_copied(&self) -> usize {
        self.audio
            .as_ref()
            .map(|audio| audio.bytes_copied())
            .unwrap_or(0)
    }
}

/// Diagnostic helper: expose engine pixels type at the assembly edge.
pub type LiveEnginePixels = EnginePixels;

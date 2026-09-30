//! Passive view facts projected from a public bridge snapshot.
//!
//! Field rules follow the QNC timeline adapter: the playhead comes from a
//! confirmed carrier frame, and a ready window does not invent one. Monitor
//! picture requires visual verification. Wave lanes stay discrete and original.
//! This module does not import QNC crates or paint UI.

use std::time::Duration;

use qnc_qgs_contract::PreparedInput;

use crate::{BridgeEvent, BridgeSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassiveAudioLane {
    pub lane_index: u16,
    pub label: String,
    pub source_track_index: u32,
    pub source_channel_index: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MonitorView {
    Empty,
    Status { transport: &'static str },
    Picture { frame: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineProjection {
    pub range_start_frame: u64,
    pub duration_frames: u64,
    pub playhead_frame: Option<u64>,
    pub cue_enabled: bool,
}

impl Default for TimelineProjection {
    fn default() -> Self {
        Self {
            range_start_frame: 0,
            duration_frames: 1,
            playhead_frame: None,
            cue_enabled: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaveProjection {
    pub source_kind: &'static str,
    pub lanes: Vec<PassiveAudioLane>,
    pub sample_cursor: Option<(u64, u64)>,
    pub peaks_included: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusProjection {
    pub transport: &'static str,
    pub public_source_uri: Option<String>,
    pub picture_label: &'static str,
    pub audio_label: &'static str,
    pub play_enabled: bool,
    pub pause_enabled: bool,
    pub source_loaded: bool,
    pub prepared_frame_count: usize,
    pub video_payload_ready: bool,
    pub audio_payload_ready: bool,
    pub presented: bool,
    pub last_rejection: Option<&'static str>,
    pub realtime_verified: bool,
    pub visual_verified: bool,
    pub audio_device_verified: bool,
    pub av_sync_verified: bool,
}

/// Predicates `qnc-player-client::View` exposes to the UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerClientView {
    pub preparing: bool,
    pub video_visible: bool,
    pub error: Option<&'static str>,
    pub playing: bool,
    pub ready: bool,
    pub confirmed_position: bool,
    pub can_start_playback: bool,
    pub frame_interval: Option<Duration>,
}

impl PlayerClientView {
    pub const fn playing(&self) -> bool {
        self.playing
    }

    pub const fn ready(&self) -> bool {
        self.ready
    }

    pub const fn has_confirmed_position(&self) -> bool {
        self.confirmed_position
    }

    pub const fn can_start_playback(&self) -> bool {
        self.can_start_playback
    }

    pub const fn source_frame_interval(&self) -> Option<Duration> {
        self.frame_interval
    }

    pub(crate) fn from_snapshot(
        snapshot: &BridgeSnapshot,
        source: &SourceFacts,
        last_rejection: Option<&'static str>,
    ) -> Self {
        let confirmed = snapshot.source_loaded
            && snapshot.current_frame.is_some()
            && snapshot
                .active_range
                .is_some_and(|(start, end)| end > start)
            && source.frame_rate_numerator > 0
            && source.frame_rate_denominator > 0
            && !snapshot.private_path_exposed
            && !snapshot.proxy_aac_authoritative;
        let ready = confirmed && snapshot.transport_ready;
        Self {
            preparing: false,
            video_visible: false,
            error: last_rejection,
            playing: snapshot.status == "Playing",
            ready,
            confirmed_position: confirmed,
            can_start_playback: ready,
            frame_interval: confirmed
                .then(|| frame_interval(source.frame_rate_numerator, source.frame_rate_denominator))
                .flatten(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassiveProjection {
    pub monitor: MonitorView,
    pub timeline: TimelineProjection,
    pub wave: WaveProjection,
    pub status: StatusProjection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceFacts {
    pub frame_rate_numerator: u64,
    pub frame_rate_denominator: u64,
    pub lanes: Vec<PassiveAudioLane>,
}

impl SourceFacts {
    pub(crate) fn from_contract(input: &PreparedInput) -> Self {
        Self {
            frame_rate_numerator: input.stream_layout.original_video.frame_rate.numerator,
            frame_rate_denominator: input.stream_layout.original_video.frame_rate.denominator,
            lanes: input
                .stream_layout
                .audio_channels
                .iter()
                .map(|lane| PassiveAudioLane {
                    lane_index: lane.lane_index,
                    label: lane_label(lane.lane_index),
                    source_track_index: lane.source_track_index,
                    source_channel_index: lane.source_channel_index,
                })
                .collect(),
        }
    }
}

impl PassiveProjection {
    pub fn empty() -> Self {
        Self {
            monitor: MonitorView::Empty,
            timeline: TimelineProjection::default(),
            wave: WaveProjection {
                source_kind: "none",
                lanes: Vec::new(),
                sample_cursor: None,
                peaks_included: false,
            },
            status: StatusProjection {
                transport: "Empty",
                public_source_uri: None,
                picture_label: "none",
                audio_label: "none",
                play_enabled: false,
                pause_enabled: false,
                source_loaded: false,
                prepared_frame_count: 0,
                video_payload_ready: false,
                audio_payload_ready: false,
                presented: false,
                last_rejection: None,
                realtime_verified: false,
                visual_verified: false,
                audio_device_verified: false,
                av_sync_verified: false,
            },
        }
    }
}

pub(crate) fn project(
    snapshot: &BridgeSnapshot,
    source: &SourceFacts,
    last_rejection: Option<&'static str>,
    events: &[BridgeEvent],
) -> PassiveProjection {
    if snapshot.private_path_exposed || snapshot.proxy_aac_authoritative {
        return PassiveProjection::empty();
    }
    // Event labels, including FramePresented, are not presentation evidence.
    let _ = events;
    let presented = snapshot.visual_verified && snapshot.current_frame.is_some();
    let confirmed =
        snapshot.source_loaded && snapshot.current_frame.is_some() && timebase_ok(source);
    let timeline = if snapshot.source_loaded {
        let (range_start_frame, duration_frames) = snapshot
            .active_range
            .filter(|(start, end)| *end > *start)
            .map(|(start, end)| (start, end - start))
            .unwrap_or((0, 1));
        TimelineProjection {
            range_start_frame,
            duration_frames,
            playhead_frame: snapshot.current_frame.filter(|_| snapshot.source_loaded),
            cue_enabled: confirmed,
        }
    } else {
        TimelineProjection::default()
    };
    let picture_label = match snapshot.source_mode {
        Some("OriginalMedia") => "original MXF",
        Some("ProxyPreview") => "proxy picture",
        _ => "none",
    };
    let monitor = match (presented, snapshot.current_frame, snapshot.source_loaded) {
        (true, Some(frame), _) => MonitorView::Picture { frame },
        (_, _, true) => MonitorView::Status {
            transport: snapshot.status,
        },
        _ => MonitorView::Empty,
    };
    let wave = if snapshot.source_loaded && snapshot.authoritative_audio == "original MXF" {
        WaveProjection {
            source_kind: "original",
            lanes: source.lanes.clone(),
            sample_cursor: snapshot.current_audio_sample_range,
            peaks_included: false,
        }
    } else {
        WaveProjection {
            source_kind: "none",
            lanes: Vec::new(),
            sample_cursor: None,
            peaks_included: false,
        }
    };
    PassiveProjection {
        monitor,
        timeline,
        wave,
        status: StatusProjection {
            transport: snapshot.status,
            public_source_uri: snapshot
                .source_loaded
                .then(|| snapshot.public_source_uri.clone())
                .flatten(),
            picture_label: if snapshot.source_loaded {
                picture_label
            } else {
                "none"
            },
            audio_label: if snapshot.source_loaded {
                snapshot.authoritative_audio
            } else {
                "none"
            },
            play_enabled: snapshot.transport_ready && confirmed,
            pause_enabled: snapshot.status == "Playing",
            source_loaded: snapshot.source_loaded,
            prepared_frame_count: snapshot.prepared_frame_count,
            video_payload_ready: snapshot.video_payload_ready,
            audio_payload_ready: snapshot.audio_payload_ready,
            presented,
            last_rejection,
            realtime_verified: snapshot.realtime_verified,
            visual_verified: snapshot.visual_verified,
            audio_device_verified: snapshot.audio_device_verified,
            av_sync_verified: snapshot.av_sync_verified,
        },
    }
}

fn frame_interval(numerator: u64, denominator: u64) -> Option<Duration> {
    if numerator == 0 || denominator == 0 {
        return None;
    }
    let nanos = 1_000_000_000u128
        .checked_mul(u128::from(denominator))?
        .div_ceil(u128::from(numerator));
    u64::try_from(nanos).ok().map(Duration::from_nanos)
}

fn timebase_ok(source: &SourceFacts) -> bool {
    source.frame_rate_numerator > 0 && source.frame_rate_denominator > 0
}

fn lane_label(index: u16) -> String {
    match index {
        0 => "A1".to_string(),
        1 => "A2".to_string(),
        2 => "A3".to_string(),
        3 => "A4".to_string(),
        other => format!("A{}", u32::from(other) + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> BridgeSnapshot {
        BridgeSnapshot {
            session_id: "qnc-controller".to_string(),
            generation: 2,
            status: "Ready",
            source_loaded: true,
            public_source_uri: Some("qnc://fixture/media/original/clip".to_string()),
            source_mode: Some("OriginalMedia"),
            picture: Some("Original"),
            authoritative_audio: "original MXF",
            proxy_aac_authoritative: false,
            active_range: Some((10, 90)),
            current_frame: None,
            current_audio_sample_range: None,
            prepared_frame_count: 6,
            video_payload_ready: true,
            audio_payload_ready: true,
            transport_ready: true,
            private_path_exposed: false,
            realtime_verified: false,
            visual_verified: false,
            audio_device_verified: false,
            av_sync_verified: false,
            real_display: "not-implemented",
        }
    }

    fn source() -> SourceFacts {
        SourceFacts {
            frame_rate_numerator: 25,
            frame_rate_denominator: 1,
            lanes: (0..4)
                .map(|lane| PassiveAudioLane {
                    lane_index: lane,
                    label: lane_label(lane),
                    source_track_index: u32::from(lane) + 1,
                    source_channel_index: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn readiness_without_carrier_does_not_create_a_playhead_or_picture() {
        let mut facts = snapshot();
        facts.current_frame = None;
        let projected = project(
            &facts,
            &source(),
            None,
            &[BridgeEvent {
                kind: "FramePresented",
                generation: 2,
            }],
        );
        assert!(projected.timeline.playhead_frame.is_none());
        assert!(!projected.timeline.cue_enabled);
        assert_eq!(projected.timeline.range_start_frame, 10);
        assert_eq!(projected.timeline.duration_frames, 80);
        assert!(matches!(
            projected.monitor,
            MonitorView::Status { transport: "Ready" }
        ));
        assert!(!projected.status.presented);
        assert!(!projected.status.play_enabled);
        assert!(!projected.wave.peaks_included);
        assert_eq!(projected.wave.lanes.len(), 4);
        assert_eq!(projected.wave.lanes[0].label, "A1");
        assert_eq!(projected.wave.source_kind, "original");
    }

    #[test]
    fn confirmed_carrier_enables_cue_without_claiming_presentation() {
        let mut facts = snapshot();
        facts.current_frame = Some(12);
        facts.current_audio_sample_range = Some((23_040, 24_960));
        let projected = project(&facts, &source(), None, &[]);
        assert_eq!(projected.timeline.playhead_frame, Some(12));
        assert!(projected.timeline.cue_enabled);
        assert!(projected.status.play_enabled);
        assert_eq!(projected.wave.sample_cursor, Some((23_040, 24_960)));
        assert!(matches!(projected.monitor, MonitorView::Status { .. }));
        assert!(!projected.status.visual_verified);
    }
}

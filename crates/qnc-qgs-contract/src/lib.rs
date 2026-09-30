#![forbid(unsafe_code)]

//! Draft field contract between QNC and QGS.
//!
//! This crate has no Vulkan, PipeWire, UI, database, or serde dependency.
//! QGS maps these values into its runtime. QNC may later depend on the same
//! crate. Nothing here executes playback.

use std::time::Duration;

pub const CONTRACT_NAME: &str = "qnc-qgs-contract";
pub const CONTRACT_VERSION: &str = "m2-v1-draft";
pub const CONTRACT_STABILITY: &str = "draft-internal";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameRate {
    pub numerator: u64,
    pub denominator: u64,
}

impl FrameRate {
    pub const fn new(numerator: u64, denominator: u64) -> Result<Self, ContractError> {
        if denominator == 0 {
            Err(ContractError::InvalidFrameRate)
        } else {
            Ok(Self {
                numerator,
                denominator,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameRange {
    pub start_frame: u64,
    pub end_frame_exclusive: u64,
}

impl FrameRange {
    pub const fn validate(self) -> Result<(), ContractError> {
        if self.end_frame_exclusive <= self.start_frame {
            Err(ContractError::InvalidActiveRange)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoTiming {
    pub frame_rate: FrameRate,
    pub duration_frames: u64,
    pub duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceMode {
    ProxyPreview,
    OriginalMedia,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PictureRepresentation {
    Original,
    Proxy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackInput {
    Original,
    Proxy,
    ProxyIfAvailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioRepresentation {
    Original,
    ProxyAac,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelKind {
    Mono,
    StereoCollapsed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceIdentity {
    pub public_source_uri: String,
    pub source_id: String,
    pub workspace_db_uri: String,
    pub original_media_id: Option<String>,
    pub proxy_media_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateBinding {
    pub original_binding_ref: String,
    pub proxy_binding_ref: Option<String>,
    pub original_bound: bool,
    pub proxy_bound: bool,
    pub private_path_exposed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioLane {
    pub lane_index: u16,
    pub source_track_index: u32,
    pub source_channel_index: u16,
    pub channel_kind: ChannelKind,
    pub authoritative: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamLayout {
    pub original_video: VideoTiming,
    pub proxy_video: Option<VideoTiming>,
    pub audio_sample_rate: u32,
    pub audio_bit_depth: u16,
    pub audio_channels: Vec<AudioLane>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectAudio {
    pub channels: u16,
    pub sample_rate_hz: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedInput {
    pub contract_version: String,
    pub descriptor_revision: u64,
    pub source_identity: SourceIdentity,
    pub private_binding: PrivateBinding,
    pub playback_input: PlaybackInput,
    pub selected_picture_representation: PictureRepresentation,
    pub authoritative_audio_representation: AudioRepresentation,
    pub source_mode: SourceMode,
    pub active_range: Option<FrameRange>,
    pub project_audio: ProjectAudio,
    pub stream_layout: StreamLayout,
    pub original_proxy_timing_compatible: Option<bool>,
    pub proxy_aac_authoritative: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandKind {
    LoadPreparedInput,
    Prepare,
    Cue,
    Preroll,
    Play,
    Pause,
    Seek,
    Stop,
    Unload,
    Snapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandPayload {
    Empty,
    Frame { frame: u64 },
    Play { frame_count: Option<u64> },
    Preroll { target_frame: Option<u64> },
    PreparedInput(PreparedInput),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandRequest {
    pub command_id: String,
    pub session_id: Option<String>,
    pub expected_generation: Option<u64>,
    pub command: CommandKind,
    pub payload: CommandPayload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractError {
    InvalidFrameRate,
    MissingPublicSourceUri,
    MissingOriginalAudio,
    InvalidSourceMode,
    SelectedPictureMismatch,
    PlaybackInputMismatch,
    ProxyAudioCannotBeAuthoritative,
    NoAudioLanes,
    StereoCollapseRejected,
    PrivatePathExposureRejected,
    InvalidActiveRange,
    TimingCompatibilityUnknown,
    InvalidProjectAudio,
}

impl ContractError {
    pub const fn label(self) -> &'static str {
        match self {
            Self::InvalidFrameRate => "invalid frame rate",
            Self::MissingPublicSourceUri => "missing public source URI",
            Self::MissingOriginalAudio => "missing authoritative original audio",
            Self::InvalidSourceMode => "invalid source mode",
            Self::SelectedPictureMismatch => {
                "selected picture representation does not match source mode"
            }
            Self::PlaybackInputMismatch => {
                "playback input does not match source mode and selected picture"
            }
            Self::ProxyAudioCannotBeAuthoritative => "proxy AAC cannot be authoritative",
            Self::NoAudioLanes => "no original audio lanes",
            Self::StereoCollapseRejected => "stereo collapse is not a valid runtime audio model",
            Self::PrivatePathExposureRejected => "private path exposure rejected",
            Self::InvalidActiveRange => "invalid active range",
            Self::TimingCompatibilityUnknown => "original/proxy timing compatibility unknown",
            Self::InvalidProjectAudio => "invalid project audio",
        }
    }
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl std::error::Error for ContractError {}

fn is_public_uri(value: &str) -> bool {
    value.starts_with("qnc://") && !value.contains('\\') && !value.contains("file:")
}

fn text_exposes_private_path(value: &str) -> bool {
    value.starts_with('/') || value.starts_with("file:") || value.contains("file:")
}

impl PreparedInput {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.contract_version.trim().is_empty()
            || !is_public_uri(&self.source_identity.public_source_uri)
            || !is_public_uri(&self.source_identity.workspace_db_uri)
            || self
                .source_identity
                .original_media_id
                .as_deref()
                .is_some_and(|uri| !is_public_uri(uri))
            || self
                .source_identity
                .proxy_media_id
                .as_deref()
                .is_some_and(|uri| !is_public_uri(uri))
        {
            return Err(ContractError::MissingPublicSourceUri);
        }
        if self.exposes_private_path() {
            return Err(ContractError::PrivatePathExposureRejected);
        }
        if self.authoritative_audio_representation != AudioRepresentation::Original
            || self.proxy_aac_authoritative
        {
            return Err(if self.proxy_aac_authoritative {
                ContractError::ProxyAudioCannotBeAuthoritative
            } else {
                ContractError::MissingOriginalAudio
            });
        }
        match (self.source_mode, self.selected_picture_representation) {
            (SourceMode::ProxyPreview, PictureRepresentation::Proxy)
            | (SourceMode::OriginalMedia, PictureRepresentation::Original) => {}
            _ => return Err(ContractError::SelectedPictureMismatch),
        }
        if !self.playback_input_agrees() {
            return Err(ContractError::PlaybackInputMismatch);
        }
        if self.stream_layout.audio_channels.is_empty() {
            return Err(ContractError::NoAudioLanes);
        }
        for (expected, lane) in self.stream_layout.audio_channels.iter().enumerate() {
            if lane.channel_kind != ChannelKind::Mono
                || !lane.authoritative
                || usize::from(lane.lane_index) != expected
            {
                return Err(ContractError::StereoCollapseRejected);
            }
        }
        if self.stream_layout.audio_sample_rate == 0
            || self.stream_layout.audio_bit_depth == 0
            || self.project_audio.channels == 0
            || self.project_audio.sample_rate_hz == 0
            || self.project_audio.sample_rate_hz != self.stream_layout.audio_sample_rate
            || usize::from(self.project_audio.channels) > self.stream_layout.audio_channels.len()
            || self.stream_layout.original_video.frame_rate.denominator == 0
            || self.stream_layout.original_video.duration_frames == 0
        {
            return Err(ContractError::InvalidProjectAudio);
        }
        if let Some(range) = self.active_range {
            range.validate()?;
        }
        match self.source_mode {
            SourceMode::ProxyPreview => {
                if !self.private_binding.proxy_bound
                    || self.private_binding.proxy_binding_ref.is_none()
                    || self.stream_layout.proxy_video.is_none()
                    || self.original_proxy_timing_compatible != Some(true)
                {
                    return Err(if self.original_proxy_timing_compatible != Some(true) {
                        ContractError::TimingCompatibilityUnknown
                    } else {
                        ContractError::InvalidSourceMode
                    });
                }
            }
            SourceMode::OriginalMedia => {}
        }
        Ok(())
    }

    pub fn exposes_private_path(&self) -> bool {
        self.private_binding.private_path_exposed
            || text_exposes_private_path(&self.private_binding.original_binding_ref)
            || self
                .private_binding
                .proxy_binding_ref
                .as_deref()
                .is_some_and(text_exposes_private_path)
            || text_exposes_private_path(&self.source_identity.public_source_uri)
            || text_exposes_private_path(&self.source_identity.source_id)
            || text_exposes_private_path(&self.source_identity.workspace_db_uri)
            || self
                .source_identity
                .original_media_id
                .as_deref()
                .is_some_and(text_exposes_private_path)
            || self
                .source_identity
                .proxy_media_id
                .as_deref()
                .is_some_and(text_exposes_private_path)
    }

    fn playback_input_agrees(&self) -> bool {
        let proxy_available = self.stream_layout.proxy_video.is_some();
        match self.playback_input {
            PlaybackInput::Original => {
                self.source_mode == SourceMode::OriginalMedia
                    && self.selected_picture_representation == PictureRepresentation::Original
            }
            PlaybackInput::Proxy => {
                proxy_available
                    && self.source_mode == SourceMode::ProxyPreview
                    && self.selected_picture_representation == PictureRepresentation::Proxy
            }
            PlaybackInput::ProxyIfAvailable => {
                if proxy_available {
                    self.source_mode == SourceMode::ProxyPreview
                        && self.selected_picture_representation == PictureRepresentation::Proxy
                } else {
                    self.source_mode == SourceMode::OriginalMedia
                        && self.selected_picture_representation == PictureRepresentation::Original
                }
            }
        }
    }

    pub fn frozen_proxy_preview() -> Self {
        frozen_input(
            "block-x-proxy-preview",
            PlaybackInput::ProxyIfAvailable,
            PictureRepresentation::Proxy,
            SourceMode::ProxyPreview,
            FrameRange {
                start_frame: 0,
                end_frame_exclusive: 100,
            },
        )
    }

    pub fn frozen_original_media() -> Self {
        frozen_input(
            "block-x-original-media",
            PlaybackInput::Original,
            PictureRepresentation::Original,
            SourceMode::OriginalMedia,
            FrameRange {
                start_frame: 10,
                end_frame_exclusive: 90,
            },
        )
    }
}

fn frozen_input(
    clip_id: &str,
    playback_input: PlaybackInput,
    selected_picture: PictureRepresentation,
    source_mode: SourceMode,
    active_range: FrameRange,
) -> PreparedInput {
    let frame_rate = FrameRate::new(25, 1).expect("25 fps");
    let timing = VideoTiming {
        frame_rate,
        duration_frames: 100,
        duration: Duration::from_secs(4),
    };
    PreparedInput {
        contract_version: CONTRACT_VERSION.to_string(),
        descriptor_revision: 1,
        source_identity: SourceIdentity {
            public_source_uri: format!("qnc://fixture/source/{clip_id}"),
            source_id: clip_id.to_string(),
            workspace_db_uri: "qnc://fixture/workspace/block-x".to_string(),
            original_media_id: Some(format!("qnc://fixture/media/original/{clip_id}")),
            proxy_media_id: Some(format!("qnc://fixture/media/proxy/{clip_id}")),
        },
        private_binding: PrivateBinding {
            original_binding_ref: format!("opaque-original-{clip_id}"),
            proxy_binding_ref: Some(format!("opaque-proxy-{clip_id}")),
            original_bound: true,
            proxy_bound: true,
            private_path_exposed: false,
        },
        playback_input,
        selected_picture_representation: selected_picture,
        authoritative_audio_representation: AudioRepresentation::Original,
        source_mode,
        active_range: Some(active_range),
        project_audio: ProjectAudio {
            channels: 4,
            sample_rate_hz: 48_000,
        },
        stream_layout: StreamLayout {
            original_video: timing,
            proxy_video: Some(timing),
            audio_sample_rate: 48_000,
            audio_bit_depth: 24,
            audio_channels: (0..4)
                .map(|lane| AudioLane {
                    lane_index: lane,
                    source_track_index: u32::from(lane) + 1,
                    source_channel_index: 0,
                    channel_kind: ChannelKind::Mono,
                    authoritative: true,
                })
                .collect(),
        },
        original_proxy_timing_compatible: Some(true),
        proxy_aac_authoritative: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_fixtures_validate_and_keep_paths_private() {
        let proxy = PreparedInput::frozen_proxy_preview();
        let original = PreparedInput::frozen_original_media();
        proxy.validate().unwrap();
        original.validate().unwrap();
        assert_eq!(proxy.contract_version, CONTRACT_VERSION);
        assert_eq!(proxy.source_mode, SourceMode::ProxyPreview);
        assert_eq!(original.source_mode, SourceMode::OriginalMedia);
        assert_eq!(original.active_range.unwrap().start_frame, 10);
        assert!(!proxy.exposes_private_path());
        assert!(!proxy.proxy_aac_authoritative);
        assert_eq!(proxy.stream_layout.audio_channels.len(), 4);
    }

    #[test]
    fn playback_input_mismatch_is_rejected() {
        let mut input = PreparedInput::frozen_proxy_preview();
        input.playback_input = PlaybackInput::Original;
        assert_eq!(
            input.validate().unwrap_err(),
            ContractError::PlaybackInputMismatch
        );
    }

    #[test]
    fn private_path_binding_is_rejected() {
        let mut input = PreparedInput::frozen_original_media();
        input.private_binding.original_binding_ref = "/tmp/original.mxf".to_string();
        assert_eq!(
            input.validate().unwrap_err(),
            ContractError::PrivatePathExposureRejected
        );
    }
}

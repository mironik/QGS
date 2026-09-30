//! Portable extract of `qnc_player_input::PreparedInput`.
//!
//! Field names follow the QNC audit of `PreparedInput`, `PlaybackInput`,
//! `ProjectAudio`, and `AudioChannel`. The QNC `Snapshot` stays in QNC.

use std::time::Duration;

use qnc_qgs_contract::{
    AudioLane, AudioRepresentation, ChannelKind, FrameRange, FrameRate, PictureRepresentation,
    PlaybackInput, PreparedInput, PrivateBinding, ProjectAudio, SourceIdentity, SourceMode,
    StreamLayout, VideoTiming, CONTRACT_VERSION,
};

use crate::BridgeError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QncHostAudioChannel {
    pub stream_index: u32,
    pub channel_index: u16,
}

/// Facts a QNC controller copies out of a prepared clip before calling QGS.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QncHostPreparedRecord {
    pub workspace_db_uri: String,
    pub clip_id: String,
    pub public_source_uri: String,
    pub original_media_uri: String,
    pub proxy_media_uri: Option<String>,
    pub playback_input: PlaybackInput,
    pub proxy_available: bool,
    pub project_audio_channels: u16,
    pub project_audio_sample_rate_hz: u32,
    pub original_frame_rate_numerator: u64,
    pub original_frame_rate_denominator: u64,
    pub original_duration_frames: u64,
    pub original_duration: Duration,
    pub proxy_frame_rate_numerator: Option<u64>,
    pub proxy_frame_rate_denominator: Option<u64>,
    pub proxy_duration_frames: Option<u64>,
    pub proxy_duration: Option<Duration>,
    pub audio_sample_rate: u32,
    pub audio_bit_depth: u16,
    pub audio_channels: Vec<QncHostAudioChannel>,
    pub original_binding_ref: String,
    pub proxy_binding_ref: Option<String>,
    pub active_range: Option<FrameRange>,
}

impl QncHostPreparedRecord {
    pub fn into_prepared_input(self) -> Result<PreparedInput, BridgeError> {
        let picture = match (self.playback_input, self.proxy_available) {
            (PlaybackInput::Original, _) | (PlaybackInput::ProxyIfAvailable, false) => {
                PictureRepresentation::Original
            }
            (PlaybackInput::Proxy, false) => return Err(BridgeError::MissingProxy),
            (PlaybackInput::Proxy | PlaybackInput::ProxyIfAvailable, true) => {
                PictureRepresentation::Proxy
            }
        };
        let source_mode = match picture {
            PictureRepresentation::Proxy => SourceMode::ProxyPreview,
            PictureRepresentation::Original => SourceMode::OriginalMedia,
        };
        let original_video = video_timing(
            self.original_frame_rate_numerator,
            self.original_frame_rate_denominator,
            self.original_duration_frames,
            self.original_duration,
        )?;
        let proxy_video = if self.proxy_available {
            match (
                self.proxy_frame_rate_numerator,
                self.proxy_frame_rate_denominator,
                self.proxy_duration_frames,
                self.proxy_duration,
            ) {
                (Some(numerator), Some(denominator), Some(frames), Some(duration)) => {
                    Some(video_timing(numerator, denominator, frames, duration)?)
                }
                _ => return Err(BridgeError::MissingProxy),
            }
        } else {
            None
        };
        let timing_compatible = proxy_video.as_ref().map(|proxy| {
            proxy.frame_rate == original_video.frame_rate
                && proxy.duration_frames == original_video.duration_frames
        });
        if picture == PictureRepresentation::Proxy {
            if self.proxy_binding_ref.is_none() || timing_compatible != Some(true) {
                return Err(if timing_compatible == Some(false) {
                    BridgeError::Rejected(
                        "proxy picture and original audio require matching video timing",
                    )
                } else {
                    BridgeError::MissingProxy
                });
            }
        }
        if self.audio_channels.is_empty() {
            return Err(BridgeError::Rejected("no original audio lanes"));
        }
        let audio_channels = self
            .audio_channels
            .into_iter()
            .enumerate()
            .map(|(lane, channel)| {
                Ok(AudioLane {
                    lane_index: u16::try_from(lane)
                        .map_err(|_| BridgeError::Rejected("too many audio lanes"))?,
                    source_track_index: channel.stream_index,
                    source_channel_index: channel.channel_index,
                    channel_kind: ChannelKind::Mono,
                    authoritative: true,
                })
            })
            .collect::<Result<Vec<_>, BridgeError>>()?;
        Ok(PreparedInput {
            contract_version: CONTRACT_VERSION.to_string(),
            descriptor_revision: 1,
            source_identity: SourceIdentity {
                public_source_uri: self.public_source_uri,
                source_id: self.clip_id,
                workspace_db_uri: self.workspace_db_uri,
                original_media_id: Some(self.original_media_uri),
                proxy_media_id: self.proxy_media_uri,
            },
            private_binding: PrivateBinding {
                original_binding_ref: self.original_binding_ref,
                proxy_binding_ref: self.proxy_binding_ref,
                original_bound: true,
                proxy_bound: self.proxy_available,
                private_path_exposed: false,
            },
            playback_input: self.playback_input,
            selected_picture_representation: picture,
            authoritative_audio_representation: AudioRepresentation::Original,
            source_mode,
            active_range: self.active_range,
            project_audio: ProjectAudio {
                channels: self.project_audio_channels,
                sample_rate_hz: self.project_audio_sample_rate_hz,
            },
            stream_layout: StreamLayout {
                original_video,
                proxy_video,
                audio_sample_rate: self.audio_sample_rate,
                audio_bit_depth: self.audio_bit_depth,
                audio_channels,
            },
            original_proxy_timing_compatible: timing_compatible,
            proxy_aac_authoritative: false,
        })
    }
}

fn video_timing(
    numerator: u64,
    denominator: u64,
    duration_frames: u64,
    duration: Duration,
) -> Result<VideoTiming, BridgeError> {
    Ok(VideoTiming {
        frame_rate: FrameRate::new(numerator, denominator)
            .map_err(|_| BridgeError::Rejected("invalid frame rate"))?,
        duration_frames,
        duration,
    })
}

#[cfg(test)]
pub fn original_media_host_record() -> QncHostPreparedRecord {
    QncHostPreparedRecord {
        workspace_db_uri: "qnc://fixture/workspace/block-x".to_string(),
        clip_id: "block-x-original-media".to_string(),
        public_source_uri: "qnc://fixture/source/block-x-original-media".to_string(),
        original_media_uri: "qnc://fixture/media/original/block-x-original-media".to_string(),
        proxy_media_uri: Some("qnc://fixture/media/proxy/block-x-original-media".to_string()),
        playback_input: PlaybackInput::Original,
        proxy_available: true,
        project_audio_channels: 4,
        project_audio_sample_rate_hz: 48_000,
        original_frame_rate_numerator: 25,
        original_frame_rate_denominator: 1,
        original_duration_frames: 100,
        original_duration: Duration::from_secs(4),
        proxy_frame_rate_numerator: Some(25),
        proxy_frame_rate_denominator: Some(1),
        proxy_duration_frames: Some(100),
        proxy_duration: Some(Duration::from_secs(4)),
        audio_sample_rate: 48_000,
        audio_bit_depth: 24,
        audio_channels: (0..4)
            .map(|stream| QncHostAudioChannel {
                stream_index: stream + 1,
                channel_index: 0,
            })
            .collect(),
        original_binding_ref: "opaque-original-block-x-original-media".to_string(),
        proxy_binding_ref: Some("opaque-proxy-block-x-original-media".to_string()),
        active_range: Some(FrameRange {
            start_frame: 10,
            end_frame_exclusive: 90,
        }),
    }
}

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

const NANOS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RationalRate {
    numerator: u64,
    denominator: u64,
}

impl RationalRate {
    pub fn new(numerator: u64, denominator: u64) -> Result<Self, PlaybackError> {
        if numerator == 0 || denominator == 0 {
            return Err(PlaybackError::InvalidRate);
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    pub fn numerator(self) -> u64 {
        self.numerator
    }

    pub fn denominator(self) -> u64 {
        self.denominator
    }

    pub fn frame_offset(self, presentation_position: u64) -> Result<Duration, PlaybackError> {
        let nanos = u128::from(presentation_position)
            .checked_mul(u128::from(self.denominator))
            .and_then(|value| value.checked_mul(NANOS_PER_SECOND))
            .ok_or(PlaybackError::TimestampOverflow)?
            / u128::from(self.numerator);
        duration_from_nanos(nanos)
    }

    pub fn frame_duration(self) -> Result<Duration, PlaybackError> {
        self.frame_offset(1)
    }

    pub fn duration_for_frames(self, frame_count: u64) -> Result<Duration, PlaybackError> {
        self.frame_offset(frame_count)
    }
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}

fn duration_from_nanos(nanos: u128) -> Result<Duration, PlaybackError> {
    let secs = nanos / NANOS_PER_SECOND;
    let subsec = nanos % NANOS_PER_SECOND;
    Ok(Duration::new(
        u64::try_from(secs).map_err(|_| PlaybackError::TimestampOverflow)?,
        u32::try_from(subsec).map_err(|_| PlaybackError::TimestampOverflow)?,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MediaTimestamp {
    units: u64,
}

impl MediaTimestamp {
    pub fn new(units: u64) -> Self {
        Self { units }
    }

    pub fn units(self) -> u64 {
        self.units
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameIdentity {
    pub presentation_position: u64,
    pub pts: MediaTimestamp,
}

impl FrameIdentity {
    pub fn from_position(presentation_position: u64) -> Self {
        Self {
            presentation_position,
            pts: MediaTimestamp::new(presentation_position),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackState {
    Idle,
    Prerolling,
    Playing,
    Draining,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationStatus {
    Presented,
    Late,
    Dropped,
    Duplicated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationDecision {
    pub identity: FrameIdentity,
    pub expected: Duration,
    pub actual: Duration,
    pub lateness: Duration,
    pub status: PresentationStatus,
}

#[derive(Clone, Debug)]
pub struct TestPresentationSink {
    decisions: Vec<PresentationDecision>,
}

impl TestPresentationSink {
    pub fn new() -> Self {
        Self {
            decisions: Vec::new(),
        }
    }

    pub fn record(&mut self, decision: PresentationDecision) {
        self.decisions.push(decision);
    }

    pub fn decisions(&self) -> &[PresentationDecision] {
        &self.decisions
    }

    pub fn counts(&self) -> PresentationCounts {
        let mut counts = PresentationCounts::default();
        for decision in &self.decisions {
            match decision.status {
                PresentationStatus::Presented => counts.presented += 1,
                PresentationStatus::Late => counts.late += 1,
                PresentationStatus::Dropped => counts.dropped += 1,
                PresentationStatus::Duplicated => counts.duplicated += 1,
            }
        }
        counts
    }
}

impl Default for TestPresentationSink {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PresentationCounts {
    pub presented: usize,
    pub late: usize,
    pub dropped: usize,
    pub duplicated: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackConfig {
    pub compressed_capacity: usize,
    pub decoded_capacity: usize,
    pub gpu_capacity: usize,
    pub presentation_capacity: usize,
    pub preroll_frames: usize,
    pub on_time_tolerance: Duration,
    pub drop_threshold: Duration,
}

impl PlaybackConfig {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        if self.compressed_capacity == 0
            || self.decoded_capacity == 0
            || self.gpu_capacity == 0
            || self.presentation_capacity == 0
            || self.preroll_frames == 0
        {
            return Err(PlaybackError::InvalidCapacity);
        }
        if self.preroll_frames > self.presentation_capacity {
            return Err(PlaybackError::InvalidCapacity);
        }
        if self.on_time_tolerance > self.drop_threshold {
            return Err(PlaybackError::InvalidLatePolicy);
        }
        Ok(self)
    }
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            compressed_capacity: 8,
            decoded_capacity: 6,
            gpu_capacity: 3,
            presentation_capacity: 4,
            preroll_frames: 3,
            on_time_tolerance: Duration::from_millis(5),
            drop_threshold: Duration::from_millis(40),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueStats {
    pub capacity: usize,
    pub peak_depth: usize,
    pub backpressure_events: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioSampleFormat {
    PcmSignedInt { bits_per_sample: u8 },
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmEndian {
    Little,
    Big,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmSampleFormat {
    SignedInteger {
        bits_per_sample: u8,
        endian: PcmEndian,
    },
}

impl PcmSampleFormat {
    pub fn bytes_per_sample(self) -> Result<usize, PlaybackError> {
        match self {
            Self::SignedInteger {
                bits_per_sample, ..
            } if bits_per_sample != 0 && bits_per_sample % 8 == 0 => {
                Ok(usize::from(bits_per_sample / 8))
            }
            Self::SignedInteger { .. } => Err(PlaybackError::InvalidAudioFormat),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: AudioSampleFormat,
}

impl AudioFormat {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        if self.sample_rate == 0 || self.channels == 0 {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        if let AudioSampleFormat::PcmSignedInt { bits_per_sample } = self.sample_format {
            if bits_per_sample == 0 {
                return Err(PlaybackError::InvalidAudioFormat);
            }
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalAudioTrack {
    pub track_id: u32,
    pub channel_index: u16,
    pub format: AudioFormat,
    pub sample_count: Option<u64>,
    pub duration: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioTimeline {
    pub tracks: Vec<OriginalAudioTrack>,
    pub duration: Duration,
}

impl AudioTimeline {
    pub fn new(tracks: Vec<OriginalAudioTrack>) -> Result<Self, PlaybackError> {
        if tracks.is_empty() {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        let duration = tracks
            .iter()
            .map(|track| track.duration)
            .max()
            .ok_or(PlaybackError::InvalidAudioFormat)?;
        Ok(Self { tracks, duration })
    }

    pub fn sample_rate(&self) -> Option<u32> {
        self.tracks.first().map(|track| track.format.sample_rate)
    }

    pub fn bit_depth(&self) -> Option<u8> {
        self.tracks
            .first()
            .and_then(|track| match track.format.sample_format {
                AudioSampleFormat::PcmSignedInt { bits_per_sample } => Some(bits_per_sample),
                AudioSampleFormat::Unknown => None,
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioTimingPacket {
    pub track_id: u32,
    pub start: Duration,
    pub duration: Duration,
    pub sample_count: u32,
    pub has_payload: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcmAudioPacket {
    pub track_id: u32,
    pub channel_index: u16,
    pub start: Duration,
    pub duration: Duration,
    pub sample_count: u32,
    pub format: PcmSampleFormat,
    pub payload: Vec<u8>,
}

impl PcmAudioPacket {
    pub fn new(
        track_id: u32,
        channel_index: u16,
        start: Duration,
        duration: Duration,
        sample_count: u32,
        format: PcmSampleFormat,
        payload: Vec<u8>,
    ) -> Result<Self, PlaybackError> {
        let expected = pcm_payload_byte_len(sample_count, 1, format)?;
        if payload.len() != expected {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        Ok(Self {
            track_id,
            channel_index,
            start,
            duration,
            sample_count,
            format,
            payload,
        })
    }

    pub fn payload_bytes(&self) -> usize {
        self.payload.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmAudioBlockLayout {
    MonoTrack { track_id: u32, channel_index: u16 },
    InterleavedChannels { channel_count: u16 },
}

impl PcmAudioBlockLayout {
    pub fn channel_count(self) -> Result<u16, PlaybackError> {
        match self {
            Self::MonoTrack { .. } => Ok(1),
            Self::InterleavedChannels { channel_count } if channel_count != 0 => Ok(channel_count),
            Self::InterleavedChannels { .. } => Err(PlaybackError::InvalidAudioFormat),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcmAudioBlock {
    pub start_time: Duration,
    pub duration: Duration,
    pub sample_rate: u32,
    pub sample_count: u32,
    pub format: PcmSampleFormat,
    pub layout: PcmAudioBlockLayout,
    pub payload: Vec<u8>,
}

impl PcmAudioBlock {
    pub fn new(
        start_time: Duration,
        duration: Duration,
        sample_rate: u32,
        sample_count: u32,
        format: PcmSampleFormat,
        layout: PcmAudioBlockLayout,
        payload: Vec<u8>,
    ) -> Result<Self, PlaybackError> {
        if sample_rate == 0 {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        let expected = pcm_payload_byte_len(sample_count, layout.channel_count()?, format)?;
        if payload.len() != expected {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        Ok(Self {
            start_time,
            duration,
            sample_rate,
            sample_count,
            format,
            layout,
            payload,
        })
    }

    pub fn from_mono_packet(
        packet: PcmAudioPacket,
        sample_rate: u32,
    ) -> Result<Self, PlaybackError> {
        Self::new(
            packet.start,
            packet.duration,
            sample_rate,
            packet.sample_count,
            packet.format,
            PcmAudioBlockLayout::MonoTrack {
                track_id: packet.track_id,
                channel_index: packet.channel_index,
            },
            packet.payload,
        )
    }

    pub fn payload_bytes(&self) -> usize {
        self.payload.len()
    }

    pub fn track_id(&self) -> Option<u32> {
        match self.layout {
            PcmAudioBlockLayout::MonoTrack { track_id, .. } => Some(track_id),
            PcmAudioBlockLayout::InterleavedChannels { .. } => None,
        }
    }

    pub fn end_time(&self) -> Result<Duration, PlaybackError> {
        self.start_time
            .checked_add(self.duration)
            .ok_or(PlaybackError::TimestampOverflow)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioRangeCoverage {
    pub track_id: u32,
    pub channel_index: u16,
    pub blocks_used: u32,
    pub bytes_covered: u64,
    pub first_block_start: Option<Duration>,
    pub last_block_end: Option<Duration>,
    pub gaps: u32,
    pub overlaps: u32,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastMediaSourceRole {
    OriginalAuthoritativeAudio,
    ProxyPreviewVideo,
    OriginalFinishingMedia,
    ProxyAudioDiagnosticOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPreviewProfile {
    Journalist50iPreview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastVideoSourceMode {
    ProxyPreview,
    OriginalMedia,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsPlaybackRepresentation {
    Original,
    Proxy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsOriginalProxyAssociationStatus {
    TimingCompatible,
    TimingMismatch,
    MissingProxy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsAudioRepresentation {
    Original,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPreparedSourceIdentity {
    pub clip_id: String,
    pub workspace_db_uri: String,
    pub source_record_uri: String,
}

impl QgsPreparedSourceIdentity {
    pub fn public_uris_are_valid(&self) -> bool {
        is_qgs_public_uri(&self.workspace_db_uri) && is_qgs_public_uri(&self.source_record_uri)
    }
}

fn is_qgs_public_uri(value: &str) -> bool {
    value.starts_with("qnc://") && !value.contains('\\') && !value.starts_with("file:")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPreparedMediaBinding {
    pub original_media_uri: String,
    pub proxy_media_uri: Option<String>,
    pub private_original_path_bound: bool,
    pub private_proxy_path_bound: bool,
    pub association_status: QgsOriginalProxyAssociationStatus,
}

impl QgsPreparedMediaBinding {
    pub fn public_uris_are_valid(&self) -> bool {
        is_qgs_public_uri(&self.original_media_uri)
            && self
                .proxy_media_uri
                .as_deref()
                .is_none_or(is_qgs_public_uri)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsPreparedVideoTiming {
    pub timebase: RationalRate,
    pub duration_frames: u64,
    pub duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsPreparedAudioChannel {
    pub track_id: u32,
    pub lane_index: u16,
    pub channel_index: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPreparedAudioLayout {
    pub representation: QgsAudioRepresentation,
    pub sample_rate: u32,
    pub bit_depth: u8,
    pub channels: Vec<QgsPreparedAudioChannel>,
    pub proxy_aac_authoritative: bool,
}

impl QgsPreparedAudioLayout {
    pub fn validate(&self) -> Result<(), PlaybackError> {
        if self.representation != QgsAudioRepresentation::Original
            || self.sample_rate == 0
            || self.bit_depth == 0
            || self.channels.is_empty()
            || self.proxy_aac_authoritative
        {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        for (expected, channel) in self.channels.iter().enumerate() {
            if channel.lane_index as usize != expected {
                return Err(PlaybackError::InvalidAudioFormat);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsPreparedStreamLayout {
    pub original_video: QgsPreparedVideoTiming,
    pub proxy_video: Option<QgsPreparedVideoTiming>,
    pub audio_sample_rate: u32,
}

impl QgsPreparedStreamLayout {
    pub fn selected_video(
        self,
        representation: QgsPlaybackRepresentation,
    ) -> Option<QgsPreparedVideoTiming> {
        match representation {
            QgsPlaybackRepresentation::Original => Some(self.original_video),
            QgsPlaybackRepresentation::Proxy => self.proxy_video,
        }
    }

    pub fn proxy_original_timing_compatible(self) -> bool {
        self.proxy_video.is_some_and(|proxy| {
            proxy.timebase == self.original_video.timebase
                && proxy.duration_frames == self.original_video.duration_frames
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPreparedInputDescriptor {
    pub contract_version: String,
    pub identity: QgsPreparedSourceIdentity,
    pub binding: QgsPreparedMediaBinding,
    pub selected_picture: QgsPlaybackRepresentation,
    pub authoritative_audio: QgsAudioRepresentation,
    pub project_audio_channels: u16,
    pub project_audio_sample_rate: u32,
    pub layout: QgsPreparedStreamLayout,
    pub audio_layout: QgsPreparedAudioLayout,
}

impl QgsPreparedInputDescriptor {
    pub fn validate(&self) -> Result<(), PlaybackError> {
        if self.contract_version.trim().is_empty()
            || !self.identity.public_uris_are_valid()
            || !self.binding.public_uris_are_valid()
            || !self.binding.private_original_path_bound
            || self.authoritative_audio != QgsAudioRepresentation::Original
            || self.project_audio_channels == 0
            || self.project_audio_sample_rate == 0
            || self.project_audio_sample_rate != self.audio_layout.sample_rate
            || usize::from(self.project_audio_channels) > self.audio_layout.channels.len()
            || self.layout.audio_sample_rate != self.audio_layout.sample_rate
            || self.layout.original_video.duration_frames == 0
            || self.layout.original_video.duration.is_zero()
        {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        self.audio_layout.validate()?;
        match self.selected_picture {
            QgsPlaybackRepresentation::Original => {}
            QgsPlaybackRepresentation::Proxy => {
                if self.binding.proxy_media_uri.is_none()
                    || !self.binding.private_proxy_path_bound
                    || self.binding.association_status
                        != QgsOriginalProxyAssociationStatus::TimingCompatible
                    || !self.layout.proxy_original_timing_compatible()
                {
                    return Err(PlaybackError::InvalidRuntimeTransition);
                }
            }
        }
        self.layout
            .selected_video(self.selected_picture)
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncRepresentationLike {
    Original,
    Proxy,
}

impl From<QgsQncRepresentationLike> for QgsPlaybackRepresentation {
    fn from(value: QgsQncRepresentationLike) -> Self {
        match value {
            QgsQncRepresentationLike::Original => Self::Original,
            QgsQncRepresentationLike::Proxy => Self::Proxy,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncPlaybackInputLike {
    Original,
    Proxy,
    ProxyIfAvailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncAudioRepresentationLike {
    Original,
    ProxyAac,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncSourceIdentityLike {
    pub public_source_uri: String,
    pub source_id: String,
    pub workspace_db_uri: String,
    pub original_media_id: Option<String>,
    pub proxy_media_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncPrivateBindingLike {
    pub original_binding_ref: String,
    pub proxy_binding_ref: Option<String>,
    pub original_bound: bool,
    pub proxy_bound: bool,
    pub private_path_exposed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsQncVideoInputLike {
    pub timebase: RationalRate,
    pub duration_frames: u64,
    pub duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncAudioChannelKindLike {
    Mono,
    StereoCollapsed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsQncAudioChannelLike {
    pub lane_index: u16,
    pub source_track_index: u32,
    pub source_channel_index: u16,
    pub channel_kind: QgsQncAudioChannelKindLike,
    pub authoritative: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncStreamLayoutLike {
    pub original_video: QgsQncVideoInputLike,
    pub proxy_video: Option<QgsQncVideoInputLike>,
    pub audio_sample_rate: u32,
    pub audio_bit_depth: u8,
    pub audio_channels: Vec<QgsQncAudioChannelLike>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsQncProjectAudioLike {
    pub channels: u16,
    pub sample_rate_hz: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsQncFrameRangeLike {
    pub start_frame: u64,
    pub end_frame_exclusive: u64,
}

impl QgsQncFrameRangeLike {
    pub fn validate(self) -> Result<(), QgsQncPreparedInputMappingError> {
        if self.end_frame_exclusive <= self.start_frame {
            return Err(QgsQncPreparedInputMappingError::InvalidActiveRange);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncPreparedInputLike {
    pub contract_version: String,
    pub descriptor_revision: u64,
    pub source_identity: QgsQncSourceIdentityLike,
    pub private_binding: QgsQncPrivateBindingLike,
    pub playback_input: QgsQncPlaybackInputLike,
    pub selected_picture_representation: QgsQncRepresentationLike,
    pub authoritative_audio_representation: QgsQncAudioRepresentationLike,
    pub source_mode: QgsInputPlanSourceMode,
    pub active_range: Option<QgsQncFrameRangeLike>,
    pub project_audio: QgsQncProjectAudioLike,
    pub stream_layout: QgsQncStreamLayoutLike,
    pub original_proxy_timing_compatible: Option<bool>,
    pub proxy_aac_authoritative: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncPreparedInputMappingError {
    MissingPublicSourceUri,
    MissingOriginalAudio,
    InvalidSourceMode,
    SelectedPictureMismatch,
    ProxyAudioCannotBeAuthoritative,
    NoAudioLanes,
    StereoCollapseRejected,
    PrivatePathExposureRejected,
    InvalidActiveRange,
    TimingCompatibilityUnknown,
    InvalidProjectAudio,
    InvalidDescriptor,
}

impl std::fmt::Display for QgsQncPreparedInputMappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::MissingPublicSourceUri => "missing public source URI",
            Self::MissingOriginalAudio => "missing authoritative original audio",
            Self::InvalidSourceMode => "invalid source mode",
            Self::SelectedPictureMismatch => {
                "selected picture representation does not match source mode"
            }
            Self::ProxyAudioCannotBeAuthoritative => "proxy AAC cannot be authoritative",
            Self::NoAudioLanes => "no original audio lanes",
            Self::StereoCollapseRejected => "stereo collapse is not a valid runtime audio model",
            Self::PrivatePathExposureRejected => "private path exposure rejected",
            Self::InvalidActiveRange => "invalid active range",
            Self::TimingCompatibilityUnknown => "original/proxy timing compatibility unknown",
            Self::InvalidProjectAudio => "invalid project audio",
            Self::InvalidDescriptor => "mapped QGS descriptor did not validate",
        };
        f.write_str(message)
    }
}

impl std::error::Error for QgsQncPreparedInputMappingError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncPreparedInputMapping {
    pub descriptor: QgsPreparedInputDescriptor,
    pub descriptor_revision: u64,
    pub active_range: Option<QgsQncFrameRangeLike>,
    pub private_path_exposed: bool,
}

impl QgsQncPreparedInputLike {
    pub fn validate_for_qgs_mapping(&self) -> Result<(), QgsQncPreparedInputMappingError> {
        if self.source_identity.public_source_uri.trim().is_empty()
            || !is_qgs_public_uri(&self.source_identity.public_source_uri)
            || !is_qgs_public_uri(&self.source_identity.workspace_db_uri)
        {
            return Err(QgsQncPreparedInputMappingError::MissingPublicSourceUri);
        }
        if self.private_binding.private_path_exposed {
            return Err(QgsQncPreparedInputMappingError::PrivatePathExposureRejected);
        }
        if self.authoritative_audio_representation != QgsQncAudioRepresentationLike::Original {
            return Err(QgsQncPreparedInputMappingError::MissingOriginalAudio);
        }
        if self.proxy_aac_authoritative {
            return Err(QgsQncPreparedInputMappingError::ProxyAudioCannotBeAuthoritative);
        }
        match (self.source_mode, self.selected_picture_representation) {
            (QgsInputPlanSourceMode::ProxyPreview, QgsQncRepresentationLike::Proxy)
            | (QgsInputPlanSourceMode::OriginalMedia, QgsQncRepresentationLike::Original) => {}
            _ => return Err(QgsQncPreparedInputMappingError::SelectedPictureMismatch),
        }
        if self.stream_layout.audio_channels.is_empty() {
            return Err(QgsQncPreparedInputMappingError::NoAudioLanes);
        }
        for (expected, channel) in self.stream_layout.audio_channels.iter().enumerate() {
            if channel.channel_kind != QgsQncAudioChannelKindLike::Mono
                || !channel.authoritative
                || channel.lane_index as usize != expected
            {
                return Err(QgsQncPreparedInputMappingError::StereoCollapseRejected);
            }
        }
        if self.stream_layout.audio_sample_rate == 0
            || self.stream_layout.audio_bit_depth == 0
            || self.project_audio.channels == 0
            || self.project_audio.sample_rate_hz == 0
            || self.project_audio.sample_rate_hz != self.stream_layout.audio_sample_rate
            || usize::from(self.project_audio.channels) > self.stream_layout.audio_channels.len()
        {
            return Err(QgsQncPreparedInputMappingError::InvalidProjectAudio);
        }
        if let Some(range) = self.active_range {
            range.validate()?;
        }
        match self.source_mode {
            QgsInputPlanSourceMode::ProxyPreview => {
                if !self.private_binding.proxy_bound
                    || self.private_binding.proxy_binding_ref.is_none()
                    || self.stream_layout.proxy_video.is_none()
                {
                    return Err(QgsQncPreparedInputMappingError::InvalidSourceMode);
                }
                if self.original_proxy_timing_compatible != Some(true) {
                    return Err(QgsQncPreparedInputMappingError::TimingCompatibilityUnknown);
                }
            }
            QgsInputPlanSourceMode::OriginalMedia => {}
        }
        Ok(())
    }

    pub fn map_to_qgs_descriptor(
        &self,
    ) -> Result<QgsQncPreparedInputMapping, QgsQncPreparedInputMappingError> {
        self.validate_for_qgs_mapping()?;
        let clip_id = if self.source_identity.source_id.trim().is_empty() {
            self.source_identity.public_source_uri.clone()
        } else {
            self.source_identity.source_id.clone()
        };
        let original_media_uri = self
            .source_identity
            .original_media_id
            .clone()
            .unwrap_or_else(|| self.source_identity.public_source_uri.clone());
        let proxy_media_uri = self.source_identity.proxy_media_id.clone();
        let selected_picture =
            QgsPlaybackRepresentation::from(self.selected_picture_representation);
        let proxy_timing = self
            .stream_layout
            .proxy_video
            .map(|video| QgsPreparedVideoTiming {
                timebase: video.timebase,
                duration_frames: video.duration_frames,
                duration: video.duration,
            });
        let association_status = match self.original_proxy_timing_compatible {
            Some(true) => QgsOriginalProxyAssociationStatus::TimingCompatible,
            Some(false) => QgsOriginalProxyAssociationStatus::TimingMismatch,
            None => QgsOriginalProxyAssociationStatus::MissingProxy,
        };
        let descriptor = QgsPreparedInputDescriptor {
            contract_version: self.contract_version.clone(),
            identity: QgsPreparedSourceIdentity {
                clip_id,
                workspace_db_uri: self.source_identity.workspace_db_uri.clone(),
                source_record_uri: self.source_identity.public_source_uri.clone(),
            },
            binding: QgsPreparedMediaBinding {
                original_media_uri,
                proxy_media_uri,
                private_original_path_bound: self.private_binding.original_bound,
                private_proxy_path_bound: self.private_binding.proxy_bound,
                association_status,
            },
            selected_picture,
            authoritative_audio: QgsAudioRepresentation::Original,
            project_audio_channels: self.project_audio.channels,
            project_audio_sample_rate: self.project_audio.sample_rate_hz,
            layout: QgsPreparedStreamLayout {
                original_video: QgsPreparedVideoTiming {
                    timebase: self.stream_layout.original_video.timebase,
                    duration_frames: self.stream_layout.original_video.duration_frames,
                    duration: self.stream_layout.original_video.duration,
                },
                proxy_video: proxy_timing,
                audio_sample_rate: self.stream_layout.audio_sample_rate,
            },
            audio_layout: QgsPreparedAudioLayout {
                representation: QgsAudioRepresentation::Original,
                sample_rate: self.stream_layout.audio_sample_rate,
                bit_depth: self.stream_layout.audio_bit_depth,
                channels: self
                    .stream_layout
                    .audio_channels
                    .iter()
                    .map(|channel| QgsPreparedAudioChannel {
                        track_id: channel.source_track_index,
                        lane_index: channel.lane_index,
                        channel_index: channel.source_channel_index,
                    })
                    .collect(),
                proxy_aac_authoritative: false,
            },
        };
        descriptor
            .validate()
            .map_err(|_| QgsQncPreparedInputMappingError::InvalidDescriptor)?;
        Ok(QgsQncPreparedInputMapping {
            descriptor,
            descriptor_revision: self.descriptor_revision,
            active_range: self.active_range,
            private_path_exposed: self.private_binding.private_path_exposed,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsInputPlanSourceMode {
    ProxyPreview,
    OriginalMedia,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsInputPlanVideoSource {
    pub mode: QgsInputPlanSourceMode,
    pub representation: QgsPlaybackRepresentation,
    pub media_uri: String,
    pub timebase: RationalRate,
    pub duration_frames: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsInputPlanAudioSource {
    pub representation: QgsAudioRepresentation,
    pub media_uri: String,
    pub sample_rate: u32,
    pub bit_depth: u8,
    pub lanes: Vec<QgsPreparedAudioChannel>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsInputPlanQueueRequirements {
    pub min_video_frames: usize,
    pub min_audio_ranges: usize,
    pub max_video_queue: usize,
    pub max_audio_queue: usize,
}

impl QgsInputPlanQueueRequirements {
    pub fn validate(self) -> Result<(), PlaybackError> {
        if self.min_video_frames == 0
            || self.min_audio_ranges == 0
            || self.max_video_queue == 0
            || self.max_audio_queue == 0
            || self.min_video_frames > self.max_video_queue
            || self.min_audio_ranges > self.max_audio_queue
        {
            return Err(PlaybackError::InvalidCapacity);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsInputPlanCapabilityRequirements {
    pub requires_original_audio: bool,
    pub requires_discrete_mono_lanes: bool,
    pub requires_proxy_aac_diagnostic_only: bool,
    pub requires_uri_identity: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsInputPlan {
    pub source_mode: QgsInputPlanSourceMode,
    pub video_source: QgsInputPlanVideoSource,
    pub audio_source: QgsInputPlanAudioSource,
    pub queue_requirements: QgsInputPlanQueueRequirements,
    pub capability_requirements: QgsInputPlanCapabilityRequirements,
    pub frame_sample_mapping_rate: u32,
}

impl QgsInputPlan {
    pub fn from_descriptor(
        descriptor: &QgsPreparedInputDescriptor,
        queue_requirements: QgsInputPlanQueueRequirements,
    ) -> Result<Self, PlaybackError> {
        descriptor.validate()?;
        queue_requirements.validate()?;
        let selected_video = descriptor
            .layout
            .selected_video(descriptor.selected_picture)
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        let source_mode = match descriptor.selected_picture {
            QgsPlaybackRepresentation::Original => QgsInputPlanSourceMode::OriginalMedia,
            QgsPlaybackRepresentation::Proxy => QgsInputPlanSourceMode::ProxyPreview,
        };
        let video_uri = match descriptor.selected_picture {
            QgsPlaybackRepresentation::Original => descriptor.binding.original_media_uri.clone(),
            QgsPlaybackRepresentation::Proxy => descriptor
                .binding
                .proxy_media_uri
                .clone()
                .ok_or(PlaybackError::InvalidRuntimeTransition)?,
        };
        let plan = Self {
            source_mode,
            video_source: QgsInputPlanVideoSource {
                mode: source_mode,
                representation: descriptor.selected_picture,
                media_uri: video_uri,
                timebase: selected_video.timebase,
                duration_frames: selected_video.duration_frames,
            },
            audio_source: QgsInputPlanAudioSource {
                representation: QgsAudioRepresentation::Original,
                media_uri: descriptor.binding.original_media_uri.clone(),
                sample_rate: descriptor.audio_layout.sample_rate,
                bit_depth: descriptor.audio_layout.bit_depth,
                lanes: descriptor.audio_layout.channels.clone(),
            },
            queue_requirements,
            capability_requirements: QgsInputPlanCapabilityRequirements {
                requires_original_audio: true,
                requires_discrete_mono_lanes: true,
                requires_proxy_aac_diagnostic_only: true,
                requires_uri_identity: true,
            },
            frame_sample_mapping_rate: descriptor.audio_layout.sample_rate,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<(), PlaybackError> {
        self.queue_requirements.validate()?;
        if self.audio_source.representation != QgsAudioRepresentation::Original
            || self.audio_source.sample_rate == 0
            || self.audio_source.bit_depth == 0
            || self.audio_source.lanes.is_empty()
            || !self.audio_source.media_uri.starts_with("qnc://")
            || !self.video_source.media_uri.starts_with("qnc://")
            || !self.capability_requirements.requires_original_audio
            || !self.capability_requirements.requires_discrete_mono_lanes
            || !self
                .capability_requirements
                .requires_proxy_aac_diagnostic_only
            || !self.capability_requirements.requires_uri_identity
            || self.frame_sample_mapping_rate != self.audio_source.sample_rate
        {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        Ok(())
    }

    pub fn samples_for_duration(&self, duration: Duration) -> Result<u64, PlaybackError> {
        audio_samples_for_duration(duration, self.frame_sample_mapping_rate)
    }
}

pub fn qgs_frames_for_duration(
    duration: Duration,
    rate: RationalRate,
) -> Result<u64, PlaybackError> {
    let nanos = duration.as_nanos();
    let numerator = nanos
        .checked_mul(u128::from(rate.numerator()))
        .ok_or(PlaybackError::TimestampOverflow)?;
    let denominator = NANOS_PER_SECOND
        .checked_mul(u128::from(rate.denominator()))
        .ok_or(PlaybackError::TimestampOverflow)?;
    if denominator == 0 {
        return Err(PlaybackError::InvalidRate);
    }
    u64::try_from(numerator / denominator).map_err(|_| PlaybackError::TimestampOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsFrameClockRate {
    pub multiplier: u32,
}

impl QgsFrameClockRate {
    pub const fn zero() -> Self {
        Self { multiplier: 0 }
    }

    pub const fn one() -> Self {
        Self { multiplier: 1 }
    }

    pub const fn two() -> Self {
        Self { multiplier: 2 }
    }

    pub fn validate(self) -> Result<(), PlaybackError> {
        if self.multiplier > 2 {
            return Err(PlaybackError::InvalidRate);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsFrameClockMode {
    Forward,
    Reverse,
    Still,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsFrameAudioSampleRange {
    pub frame: u64,
    pub start_sample: u64,
    pub end_sample: u64,
    pub start_time: Duration,
    pub duration: Duration,
}

impl QgsFrameAudioSampleRange {
    pub fn sample_count(self) -> u64 {
        self.end_sample.saturating_sub(self.start_sample)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsCueValidation {
    pub frame: u64,
    pub sample: u64,
    pub valid: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsActiveRangeTiming {
    pub start_frame: u64,
    pub end_frame: u64,
    pub start_sample: u64,
    pub end_sample: u64,
    pub source_duration_frames: u64,
    pub frame_rate: RationalRate,
    pub audio_sample_rate: u32,
}

impl QgsActiveRangeTiming {
    pub fn new(
        source_duration_frames: u64,
        frame_rate: RationalRate,
        audio_sample_rate: u32,
        start_frame: u64,
        end_frame: u64,
    ) -> Result<Self, PlaybackError> {
        if audio_sample_rate == 0
            || source_duration_frames == 0
            || start_frame >= end_frame
            || end_frame > source_duration_frames
        {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        let start_sample = frame_to_sample_boundary(start_frame, frame_rate, audio_sample_rate)?;
        let end_sample = frame_to_sample_boundary(end_frame, frame_rate, audio_sample_rate)?;
        Ok(Self {
            start_frame,
            end_frame,
            start_sample,
            end_sample,
            source_duration_frames,
            frame_rate,
            audio_sample_rate,
        })
    }

    pub fn contains_frame(self, frame: u64) -> bool {
        frame >= self.start_frame && frame < self.end_frame
    }

    pub fn validate_cue(self, frame: u64) -> Result<QgsCueValidation, PlaybackError> {
        if !self.contains_frame(frame) {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        Ok(QgsCueValidation {
            frame,
            sample: frame_to_sample_boundary(frame, self.frame_rate, self.audio_sample_rate)?,
            valid: true,
        })
    }

    pub fn frame_audio_sample_range(
        self,
        frame: u64,
    ) -> Result<QgsFrameAudioSampleRange, PlaybackError> {
        self.validate_cue(frame)?;
        let start_sample =
            frame_to_sample_boundary(frame, self.frame_rate, self.audio_sample_rate)?;
        let end_sample = frame_to_sample_boundary(
            frame
                .checked_add(1)
                .ok_or(PlaybackError::TimestampOverflow)?,
            self.frame_rate,
            self.audio_sample_rate,
        )?;
        Ok(QgsFrameAudioSampleRange {
            frame,
            start_sample,
            end_sample,
            start_time: self.frame_rate.frame_offset(frame)?,
            duration: self.frame_rate.duration_for_frames(1)?,
        })
    }

    pub fn duration(self) -> Result<Duration, PlaybackError> {
        self.frame_rate
            .duration_for_frames(self.end_frame - self.start_frame)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsDueFrameDrain {
    pub frames: Vec<u64>,
    pub latest_due_frame: Option<u64>,
    pub bounded_by_start: bool,
    pub bounded_by_end: bool,
    pub truncated_by_limit: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsFrameClock {
    pub active_range: QgsActiveRangeTiming,
    pub mode: QgsFrameClockMode,
    pub rate: QgsFrameClockRate,
    pub anchor_frame: u64,
}

impl QgsFrameClock {
    pub fn new(
        active_range: QgsActiveRangeTiming,
        mode: QgsFrameClockMode,
        rate: QgsFrameClockRate,
        anchor_frame: u64,
    ) -> Result<Self, PlaybackError> {
        rate.validate()?;
        if !active_range.contains_frame(anchor_frame) {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        Ok(Self {
            active_range,
            mode,
            rate,
            anchor_frame,
        })
    }

    pub fn forward(active_range: QgsActiveRangeTiming) -> Result<Self, PlaybackError> {
        Self::new(
            active_range,
            QgsFrameClockMode::Forward,
            QgsFrameClockRate::one(),
            active_range.start_frame,
        )
    }

    pub fn reverse(active_range: QgsActiveRangeTiming) -> Result<Self, PlaybackError> {
        Self::new(
            active_range,
            QgsFrameClockMode::Reverse,
            QgsFrameClockRate::one(),
            active_range.end_frame - 1,
        )
    }

    pub fn still(active_range: QgsActiveRangeTiming, frame: u64) -> Result<Self, PlaybackError> {
        Self::new(
            active_range,
            QgsFrameClockMode::Still,
            QgsFrameClockRate::zero(),
            frame,
        )
    }

    pub fn latest_due_frame(self, elapsed: Duration) -> Result<Option<u64>, PlaybackError> {
        if self.mode == QgsFrameClockMode::Still || self.rate.multiplier == 0 {
            return Ok(Some(self.anchor_frame));
        }
        let elapsed_frames = qgs_frames_for_duration(elapsed, self.active_range.frame_rate)?;
        let offset = elapsed_frames
            .checked_mul(u64::from(self.rate.multiplier))
            .ok_or(PlaybackError::TimestampOverflow)?;
        match self.mode {
            QgsFrameClockMode::Forward => Ok(Some(
                self.anchor_frame
                    .saturating_add(offset)
                    .min(self.active_range.end_frame - 1),
            )),
            QgsFrameClockMode::Reverse => Ok(Some(
                self.anchor_frame
                    .saturating_sub(offset)
                    .max(self.active_range.start_frame),
            )),
            QgsFrameClockMode::Still => Ok(Some(self.anchor_frame)),
        }
    }

    pub fn drain_due_frames(
        self,
        last_emitted_frame: Option<u64>,
        elapsed: Duration,
        max_frames: usize,
    ) -> Result<QgsDueFrameDrain, PlaybackError> {
        if max_frames == 0 {
            return Err(PlaybackError::InvalidCapacity);
        }
        let latest_due_frame = self.latest_due_frame(elapsed)?;
        let Some(latest) = latest_due_frame else {
            return Ok(QgsDueFrameDrain {
                frames: Vec::new(),
                latest_due_frame,
                bounded_by_start: false,
                bounded_by_end: false,
                truncated_by_limit: false,
            });
        };
        if self.mode == QgsFrameClockMode::Still || self.rate.multiplier == 0 {
            return Ok(QgsDueFrameDrain {
                frames: Vec::new(),
                latest_due_frame,
                bounded_by_start: false,
                bounded_by_end: false,
                truncated_by_limit: false,
            });
        }

        let mut frames = Vec::new();
        let mut truncated_by_limit = false;
        match self.mode {
            QgsFrameClockMode::Forward => {
                let mut next = last_emitted_frame
                    .and_then(|frame| frame.checked_add(1))
                    .unwrap_or(self.anchor_frame)
                    .max(self.active_range.start_frame);
                while next <= latest && next < self.active_range.end_frame {
                    if frames.len() == max_frames {
                        truncated_by_limit = true;
                        break;
                    }
                    frames.push(next);
                    next = next
                        .checked_add(1)
                        .ok_or(PlaybackError::TimestampOverflow)?;
                }
            }
            QgsFrameClockMode::Reverse => {
                let mut next = last_emitted_frame
                    .and_then(|frame| frame.checked_sub(1))
                    .unwrap_or(self.anchor_frame)
                    .min(self.active_range.end_frame - 1);
                loop {
                    if next < latest || next < self.active_range.start_frame {
                        break;
                    }
                    if frames.len() == max_frames {
                        truncated_by_limit = true;
                        break;
                    }
                    frames.push(next);
                    if next == 0 {
                        break;
                    }
                    next -= 1;
                }
            }
            QgsFrameClockMode::Still => {}
        }

        Ok(QgsDueFrameDrain {
            frames,
            latest_due_frame,
            bounded_by_start: self.mode == QgsFrameClockMode::Reverse
                && latest == self.active_range.start_frame,
            bounded_by_end: self.mode == QgsFrameClockMode::Forward
                && latest == self.active_range.end_frame - 1,
            truncated_by_limit,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsPreparedFrameKey {
    pub frame: u64,
    pub revision: QgsTransportSourceRevision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsPreparedFrameStatus {
    NotPrepared,
    Preparing,
    Prepared,
    Accounted,
    Discarded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsPreparedPayloadKind {
    OriginalAudioRange,
    ProxyVideoReference,
    OriginalVideoReference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsBufferDiscardReason {
    BehindBackwardWindow,
    LimitPressure,
    SourceClosed,
    SourceUnloaded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsPlayoutBufferLimits {
    pub backward_keep_frames: u64,
    pub forward_prepare_frames: u64,
    pub max_prepared_frames: usize,
}

impl QgsPlayoutBufferLimits {
    pub const fn default_transport_window() -> Self {
        Self {
            backward_keep_frames: 2,
            forward_prepare_frames: 5,
            max_prepared_frames: 8,
        }
    }

    pub fn validate(self) -> Result<(), PlaybackError> {
        if self.max_prepared_frames == 0
            || self.forward_prepare_frames == 0
            || u64::try_from(self.max_prepared_frames)
                .map_err(|_| PlaybackError::InvalidCapacity)?
                == 0
        {
            return Err(PlaybackError::InvalidCapacity);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsPlayoutBufferWindow {
    pub carrier_frame: u64,
    pub start_frame: u64,
    pub end_frame: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPreparedAudioRangeSlot {
    pub frame: u64,
    pub sample_range: QgsFrameAudioSampleRange,
    pub lanes: Vec<QgsPreparedAudioChannel>,
    pub payload_kind: QgsPreparedPayloadKind,
    pub status: QgsPreparedFrameStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPreparedVideoSlot {
    pub frame: u64,
    pub source_mode: QgsInputPlanSourceMode,
    pub payload_kind: QgsPreparedPayloadKind,
    pub status: QgsPreparedFrameStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPreparedFrameSlot {
    pub key: QgsPreparedFrameKey,
    pub audio: QgsPreparedAudioRangeSlot,
    pub video: QgsPreparedVideoSlot,
    pub status: QgsPreparedFrameStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QgsTickPreparationEvent {
    PlayoutBufferCreated {
        max_prepared_frames: usize,
    },
    TickPreparationStarted {
        carrier_frame: u64,
    },
    DueFramesDrained {
        frames: Vec<u64>,
    },
    FramePrepared {
        frame: u64,
    },
    AudioRangePrepared {
        frame: u64,
        start_sample: u64,
        end_sample: u64,
        lanes: usize,
    },
    VideoPayloadMarkedReady {
        frame: u64,
        payload_kind: QgsPreparedPayloadKind,
    },
    PreparedWindowAdvanced {
        start_frame: u64,
        end_frame: u64,
    },
    OldFrameDiscarded {
        frame: u64,
        reason: QgsBufferDiscardReason,
    },
    PreparedStateDiscarded {
        frames: Vec<u64>,
        reason: QgsBufferDiscardReason,
    },
    BufferLimitReached {
        max_prepared_frames: usize,
    },
    TickPreparationCompleted {
        prepared_frames: usize,
    },
    TickValidationFailed {
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsTickPreparationInput {
    pub carrier_frame: u64,
    pub elapsed: Duration,
    pub max_due_frames: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsTickPreparationResult {
    pub carrier_frame: u64,
    pub window: QgsPlayoutBufferWindow,
    pub due_frames: Vec<u64>,
    pub prepared_frames: Vec<u64>,
    pub discarded_frames: Vec<u64>,
    pub slots: Vec<QgsPreparedFrameSlot>,
    pub events: Vec<QgsTickPreparationEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPlayoutBufferState {
    pub active_range: QgsActiveRangeTiming,
    pub revision: QgsTransportSourceRevision,
    pub source_mode: QgsInputPlanSourceMode,
    pub lanes: Vec<QgsPreparedAudioChannel>,
    pub limits: QgsPlayoutBufferLimits,
    slots: BTreeMap<u64, QgsPreparedFrameSlot>,
    last_accounted_frame: Option<u64>,
    events: Vec<QgsTickPreparationEvent>,
}

impl QgsPlayoutBufferState {
    pub fn new(
        active_range: QgsActiveRangeTiming,
        revision: QgsTransportSourceRevision,
        source_mode: QgsInputPlanSourceMode,
        lanes: Vec<QgsPreparedAudioChannel>,
        limits: QgsPlayoutBufferLimits,
    ) -> Result<Self, PlaybackError> {
        limits.validate()?;
        if lanes.is_empty() {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        Ok(Self {
            active_range,
            revision,
            source_mode,
            lanes,
            limits,
            slots: BTreeMap::new(),
            last_accounted_frame: None,
            events: vec![QgsTickPreparationEvent::PlayoutBufferCreated {
                max_prepared_frames: limits.max_prepared_frames,
            }],
        })
    }

    pub fn slots(&self) -> Vec<QgsPreparedFrameSlot> {
        self.slots.values().cloned().collect()
    }

    pub fn prepared_frame_count(&self) -> usize {
        self.slots.len()
    }

    pub fn events(&self) -> &[QgsTickPreparationEvent] {
        &self.events
    }

    pub fn discard_all(&mut self, reason: QgsBufferDiscardReason) -> Vec<u64> {
        let frames = self.slots.keys().copied().collect::<Vec<_>>();
        self.slots.clear();
        self.last_accounted_frame = None;
        self.events
            .push(QgsTickPreparationEvent::PreparedStateDiscarded {
                frames: frames.clone(),
                reason,
            });
        frames
    }

    pub fn tick_prepare(
        &mut self,
        clock: QgsFrameClock,
        input: QgsTickPreparationInput,
    ) -> Result<QgsTickPreparationResult, PlaybackError> {
        if clock.active_range != self.active_range
            || !self.active_range.contains_frame(input.carrier_frame)
        {
            self.events
                .push(QgsTickPreparationEvent::TickValidationFailed {
                    reason: "carrier outside active range",
                });
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        let mut tick_events = Vec::new();
        tick_events.push(QgsTickPreparationEvent::TickPreparationStarted {
            carrier_frame: input.carrier_frame,
        });
        let drain = clock.drain_due_frames(
            self.last_accounted_frame,
            input.elapsed,
            input.max_due_frames,
        )?;
        self.mark_frames_accounted(&drain.frames);
        if let Some(last) = drain.frames.last().copied() {
            self.last_accounted_frame = Some(last);
        }
        tick_events.push(QgsTickPreparationEvent::DueFramesDrained {
            frames: drain.frames.clone(),
        });

        let discarded_frames = self.discard_old_frames(input.carrier_frame, &mut tick_events)?;
        let window = self.window_for_carrier(input.carrier_frame)?;
        if window.end_frame < self.natural_window_end(input.carrier_frame)? {
            tick_events.push(QgsTickPreparationEvent::BufferLimitReached {
                max_prepared_frames: self.limits.max_prepared_frames,
            });
        }
        let mut prepared_frames = Vec::new();
        let mut frame = window.start_frame;
        while frame < window.end_frame {
            if self.slots.len() >= self.limits.max_prepared_frames
                && !self.slots.contains_key(&frame)
            {
                tick_events.push(QgsTickPreparationEvent::BufferLimitReached {
                    max_prepared_frames: self.limits.max_prepared_frames,
                });
                break;
            }
            if !self.slots.contains_key(&frame) {
                let slot = self.prepare_slot(frame)?;
                tick_events.push(QgsTickPreparationEvent::FramePrepared { frame });
                tick_events.push(QgsTickPreparationEvent::AudioRangePrepared {
                    frame,
                    start_sample: slot.audio.sample_range.start_sample,
                    end_sample: slot.audio.sample_range.end_sample,
                    lanes: slot.audio.lanes.len(),
                });
                tick_events.push(QgsTickPreparationEvent::VideoPayloadMarkedReady {
                    frame,
                    payload_kind: slot.video.payload_kind,
                });
                self.slots.insert(frame, slot);
                prepared_frames.push(frame);
            }
            frame = frame
                .checked_add(1)
                .ok_or(PlaybackError::TimestampOverflow)?;
        }
        self.mark_frames_accounted(&drain.frames);

        tick_events.push(QgsTickPreparationEvent::PreparedWindowAdvanced {
            start_frame: window.start_frame,
            end_frame: window.end_frame,
        });
        tick_events.push(QgsTickPreparationEvent::TickPreparationCompleted {
            prepared_frames: self.slots.len(),
        });
        self.events.extend(tick_events.clone());
        Ok(QgsTickPreparationResult {
            carrier_frame: input.carrier_frame,
            window,
            due_frames: drain.frames,
            prepared_frames,
            discarded_frames,
            slots: self.slots(),
            events: tick_events,
        })
    }

    fn prepare_slot(&self, frame: u64) -> Result<QgsPreparedFrameSlot, PlaybackError> {
        let sample_range = self.active_range.frame_audio_sample_range(frame)?;
        let video_payload_kind = match self.source_mode {
            QgsInputPlanSourceMode::ProxyPreview => QgsPreparedPayloadKind::ProxyVideoReference,
            QgsInputPlanSourceMode::OriginalMedia => QgsPreparedPayloadKind::OriginalVideoReference,
        };
        Ok(QgsPreparedFrameSlot {
            key: QgsPreparedFrameKey {
                frame,
                revision: self.revision,
            },
            audio: QgsPreparedAudioRangeSlot {
                frame,
                sample_range,
                lanes: self.lanes.clone(),
                payload_kind: QgsPreparedPayloadKind::OriginalAudioRange,
                status: QgsPreparedFrameStatus::Prepared,
            },
            video: QgsPreparedVideoSlot {
                frame,
                source_mode: self.source_mode,
                payload_kind: video_payload_kind,
                status: QgsPreparedFrameStatus::Prepared,
            },
            status: QgsPreparedFrameStatus::Prepared,
        })
    }

    fn mark_frames_accounted(&mut self, frames: &[u64]) {
        for frame in frames {
            if let Some(slot) = self.slots.get_mut(frame) {
                slot.status = QgsPreparedFrameStatus::Accounted;
                slot.audio.status = QgsPreparedFrameStatus::Accounted;
                slot.video.status = QgsPreparedFrameStatus::Accounted;
            }
        }
    }

    fn window_for_carrier(
        &self,
        carrier_frame: u64,
    ) -> Result<QgsPlayoutBufferWindow, PlaybackError> {
        let start_frame = carrier_frame
            .saturating_sub(self.limits.backward_keep_frames)
            .max(self.active_range.start_frame);
        let mut end_frame = self.natural_window_end(carrier_frame)?;
        let max_end = start_frame
            .checked_add(
                u64::try_from(self.limits.max_prepared_frames)
                    .map_err(|_| PlaybackError::InvalidCapacity)?,
            )
            .ok_or(PlaybackError::TimestampOverflow)?;
        end_frame = end_frame.min(max_end);
        Ok(QgsPlayoutBufferWindow {
            carrier_frame,
            start_frame,
            end_frame,
        })
    }

    fn natural_window_end(&self, carrier_frame: u64) -> Result<u64, PlaybackError> {
        Ok(carrier_frame
            .checked_add(self.limits.forward_prepare_frames)
            .and_then(|value| value.checked_add(1))
            .ok_or(PlaybackError::TimestampOverflow)?
            .min(self.active_range.end_frame))
    }

    fn discard_old_frames(
        &mut self,
        carrier_frame: u64,
        tick_events: &mut Vec<QgsTickPreparationEvent>,
    ) -> Result<Vec<u64>, PlaybackError> {
        let keep_start = carrier_frame
            .saturating_sub(self.limits.backward_keep_frames)
            .max(self.active_range.start_frame);
        let old_frames = self
            .slots
            .keys()
            .copied()
            .filter(|frame| *frame < keep_start)
            .collect::<Vec<_>>();
        for frame in &old_frames {
            self.slots.remove(frame);
            tick_events.push(QgsTickPreparationEvent::OldFrameDiscarded {
                frame: *frame,
                reason: QgsBufferDiscardReason::BehindBackwardWindow,
            });
        }
        Ok(old_frames)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsTransportSourceRevision(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsTransportSourceHandle {
    pub source_id: String,
    pub revision: QgsTransportSourceRevision,
    pub video_uri: String,
    pub audio_uri: String,
    pub source_mode: QgsInputPlanSourceMode,
    pub duration_frames: u64,
    pub timebase: RationalRate,
    pub audio_sample_rate: u32,
}

impl QgsTransportSourceHandle {
    fn from_plan(plan: &QgsInputPlan, revision: QgsTransportSourceRevision) -> Self {
        Self {
            source_id: plan.video_source.media_uri.clone(),
            revision,
            video_uri: plan.video_source.media_uri.clone(),
            audio_uri: plan.audio_source.media_uri.clone(),
            source_mode: plan.source_mode,
            duration_frames: plan.video_source.duration_frames,
            timebase: plan.video_source.timebase,
            audio_sample_rate: plan.audio_source.sample_rate,
        }
    }

    pub fn exposes_private_path(&self) -> bool {
        self.source_id.starts_with('/')
            || self.video_uri.starts_with('/')
            || self.audio_uri.starts_with('/')
            || self.source_id.starts_with("file:")
            || self.video_uri.starts_with("file:")
            || self.audio_uri.starts_with("file:")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsTransportStatus {
    Empty,
    Loaded,
    Ready,
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsTransportActiveRange {
    pub start_frame: u64,
    pub end_frame: u64,
    pub start_sample: u64,
    pub end_sample: u64,
}

impl QgsTransportActiveRange {
    pub fn from_frames(
        handle: &QgsTransportSourceHandle,
        start_frame: u64,
        end_frame: u64,
    ) -> Result<Self, PlaybackError> {
        let timing = QgsActiveRangeTiming::new(
            handle.duration_frames,
            handle.timebase,
            handle.audio_sample_rate,
            start_frame,
            end_frame,
        )?;
        Ok(Self {
            start_frame: timing.start_frame,
            end_frame: timing.end_frame,
            start_sample: timing.start_sample,
            end_sample: timing.end_sample,
        })
    }

    pub fn contains_frame(self, frame: u64) -> bool {
        frame >= self.start_frame && frame < self.end_frame
    }
}

fn frame_to_sample_boundary(
    frame: u64,
    timebase: RationalRate,
    sample_rate: u32,
) -> Result<u64, PlaybackError> {
    let numerator = u128::from(frame)
        .checked_mul(u128::from(timebase.denominator()))
        .and_then(|value| value.checked_mul(u128::from(sample_rate)))
        .ok_or(PlaybackError::TimestampOverflow)?;
    let denominator = u128::from(timebase.numerator());
    if denominator == 0 {
        return Err(PlaybackError::InvalidRate);
    }
    u64::try_from(numerator / denominator).map_err(|_| PlaybackError::TimestampOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsTransportCuePoint {
    pub frame: u64,
    pub sample: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsTransportPreparedAnchor {
    pub frame: u64,
    pub sample: u64,
    pub revision: QgsTransportSourceRevision,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QgsTransportNoWorkOnPlayCounters {
    pub source_open_on_play: u32,
    pub decode_on_play: u32,
    pub queue_fill_on_play: u32,
    pub preroll_on_play: u32,
    pub anchor_prepare_on_play: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QgsTransportEvent {
    TransportEngineCreated,
    SourceLoaded {
        source_id: String,
        revision: QgsTransportSourceRevision,
    },
    SourcePreloaded {
        source_id: String,
        revision: QgsTransportSourceRevision,
    },
    ActiveSourceChanged {
        source_id: Option<String>,
        revision: Option<QgsTransportSourceRevision>,
    },
    ActiveRangeSet {
        start_frame: u64,
        end_frame: u64,
        start_sample: u64,
        end_sample: u64,
    },
    CueCompleted {
        frame: u64,
        sample: u64,
    },
    PreparedAnchorReady {
        frame: u64,
        revision: QgsTransportSourceRevision,
    },
    PlayReadinessChanged {
        ready: bool,
    },
    TransportPlayRejected {
        reason: &'static str,
    },
    TransportStarted {
        frame: u64,
    },
    TransportPaused,
    TransportStopped,
    ActiveSourceCleared {
        source_id: String,
        revision: QgsTransportSourceRevision,
    },
    SourceRevisionInvalidated {
        source_id: String,
        revision: QgsTransportSourceRevision,
    },
    SourceUnloaded {
        source_id: String,
        revision: QgsTransportSourceRevision,
    },
    PreparedStateDiscarded {
        frames: usize,
    },
    TransportValidationFailed {
        reason: &'static str,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsRuntimeEventSequence(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsRuntimeEventGeneration(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsRuntimeEventSource {
    pub source_id: String,
    pub revision: QgsTransportSourceRevision,
}

impl QgsRuntimeEventSource {
    pub fn exposes_private_path(&self) -> bool {
        self.source_id.starts_with('/') || self.source_id.starts_with("file:")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsRuntimeEventKind {
    RuntimeEngineCreated,
    SourceLoaded,
    SourcePreloaded,
    ActiveSourceChanged,
    ActiveRangeSet,
    CueCompleted,
    PreparedAnchorReady,
    PlayReadinessChanged,
    TransportStarted,
    TransportPaused,
    TransportStopped,
    TickPreparationCompleted,
    PreparedStateDiscarded,
    ActiveSourceCleared,
    SourceRevisionInvalidated,
    SourceUnloaded,
    TransportCommandRejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsRuntimeEventPayload {
    pub summary: String,
    pub evidence_level: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsRuntimeEventEnvelope {
    pub sequence: QgsRuntimeEventSequence,
    pub generation: QgsRuntimeEventGeneration,
    pub source: Option<QgsRuntimeEventSource>,
    pub kind: QgsRuntimeEventKind,
    pub payload: QgsRuntimeEventPayload,
}

impl QgsRuntimeEventEnvelope {
    pub fn exposes_private_path(&self) -> bool {
        self.source
            .as_ref()
            .is_some_and(QgsRuntimeEventSource::exposes_private_path)
            || self.payload.summary.starts_with('/')
            || self.payload.summary.contains("file:")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsRuntimeEventLog {
    next_sequence: u64,
    pub generation: QgsRuntimeEventGeneration,
    envelopes: Vec<QgsRuntimeEventEnvelope>,
}

impl QgsRuntimeEventLog {
    pub fn new() -> Self {
        Self {
            next_sequence: 0,
            generation: QgsRuntimeEventGeneration(0),
            envelopes: Vec::new(),
        }
    }

    pub fn increment_generation(&mut self) -> QgsRuntimeEventGeneration {
        self.generation.0 = self.generation.0.saturating_add(1);
        self.generation
    }

    pub fn push(
        &mut self,
        kind: QgsRuntimeEventKind,
        source: Option<QgsRuntimeEventSource>,
        summary: impl Into<String>,
    ) -> QgsRuntimeEventEnvelope {
        let envelope = QgsRuntimeEventEnvelope {
            sequence: QgsRuntimeEventSequence(self.next_sequence),
            generation: self.generation,
            source,
            kind,
            payload: QgsRuntimeEventPayload {
                summary: summary.into(),
                evidence_level: None,
            },
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.envelopes.push(envelope.clone());
        envelope
    }

    pub fn push_transport_event(&mut self, event: &QgsTransportEvent) -> QgsRuntimeEventEnvelope {
        let (kind, source, summary) = runtime_payload_from_transport_event(event);
        self.push(kind, source, summary)
    }

    pub fn push_tick_event(&mut self, event: &QgsTickPreparationEvent) -> QgsRuntimeEventEnvelope {
        let (kind, summary) = runtime_payload_from_tick_event(event);
        self.push(kind, None, summary)
    }

    pub fn envelopes(&self) -> &[QgsRuntimeEventEnvelope] {
        &self.envelopes
    }

    pub fn sequence_is_monotonic(&self) -> bool {
        self.envelopes
            .iter()
            .enumerate()
            .all(|(index, event)| event.sequence.0 == u64::try_from(index).unwrap_or(u64::MAX))
    }
}

impl Default for QgsRuntimeEventLog {
    fn default() -> Self {
        Self::new()
    }
}

fn runtime_payload_from_transport_event(
    event: &QgsTransportEvent,
) -> (QgsRuntimeEventKind, Option<QgsRuntimeEventSource>, String) {
    match event {
        QgsTransportEvent::TransportEngineCreated => (
            QgsRuntimeEventKind::RuntimeEngineCreated,
            None,
            "transport engine created".to_string(),
        ),
        QgsTransportEvent::SourceLoaded {
            source_id,
            revision,
        } => (
            QgsRuntimeEventKind::SourceLoaded,
            Some(QgsRuntimeEventSource {
                source_id: source_id.clone(),
                revision: *revision,
            }),
            format!("source loaded revision={}", revision.0),
        ),
        QgsTransportEvent::SourcePreloaded {
            source_id,
            revision,
        } => (
            QgsRuntimeEventKind::SourcePreloaded,
            Some(QgsRuntimeEventSource {
                source_id: source_id.clone(),
                revision: *revision,
            }),
            format!("source preloaded revision={}", revision.0),
        ),
        QgsTransportEvent::ActiveSourceChanged {
            source_id,
            revision,
        } => (
            QgsRuntimeEventKind::ActiveSourceChanged,
            source_id
                .as_ref()
                .zip(revision.as_ref())
                .map(|(source_id, revision)| QgsRuntimeEventSource {
                    source_id: source_id.clone(),
                    revision: *revision,
                }),
            "active source changed".to_string(),
        ),
        QgsTransportEvent::ActiveRangeSet {
            start_frame,
            end_frame,
            start_sample,
            end_sample,
        } => (
            QgsRuntimeEventKind::ActiveRangeSet,
            None,
            format!(
                "active range frames=[{start_frame}..{end_frame}) samples=[{start_sample}..{end_sample})"
            ),
        ),
        QgsTransportEvent::CueCompleted { frame, sample } => (
            QgsRuntimeEventKind::CueCompleted,
            None,
            format!("cue completed frame={frame} sample={sample}"),
        ),
        QgsTransportEvent::PreparedAnchorReady { frame, revision } => (
            QgsRuntimeEventKind::PreparedAnchorReady,
            None,
            format!("prepared anchor frame={frame} revision={}", revision.0),
        ),
        QgsTransportEvent::PlayReadinessChanged { ready } => (
            QgsRuntimeEventKind::PlayReadinessChanged,
            None,
            format!("play_ready={ready}"),
        ),
        QgsTransportEvent::TransportStarted { frame } => (
            QgsRuntimeEventKind::TransportStarted,
            None,
            format!("transport started frame={frame}"),
        ),
        QgsTransportEvent::ActiveSourceCleared {
            source_id,
            revision,
        } => (
            QgsRuntimeEventKind::ActiveSourceCleared,
            Some(QgsRuntimeEventSource {
                source_id: source_id.clone(),
                revision: *revision,
            }),
            format!("active source cleared revision={}", revision.0),
        ),
        QgsTransportEvent::SourceRevisionInvalidated {
            source_id,
            revision,
        } => (
            QgsRuntimeEventKind::SourceRevisionInvalidated,
            Some(QgsRuntimeEventSource {
                source_id: source_id.clone(),
                revision: *revision,
            }),
            format!("source revision invalidated revision={}", revision.0),
        ),
        QgsTransportEvent::SourceUnloaded {
            source_id,
            revision,
        } => (
            QgsRuntimeEventKind::SourceUnloaded,
            Some(QgsRuntimeEventSource {
                source_id: source_id.clone(),
                revision: *revision,
            }),
            format!("source unloaded revision={}", revision.0),
        ),
        QgsTransportEvent::PreparedStateDiscarded { frames } => (
            QgsRuntimeEventKind::PreparedStateDiscarded,
            None,
            format!("prepared state discarded frames={frames}"),
        ),
        QgsTransportEvent::TransportPlayRejected { reason }
        | QgsTransportEvent::TransportValidationFailed { reason } => (
            QgsRuntimeEventKind::TransportCommandRejected,
            None,
            format!("command rejected reason={reason}"),
        ),
        QgsTransportEvent::TransportPaused => (
            QgsRuntimeEventKind::TransportPaused,
            None,
            "transport paused".to_string(),
        ),
        QgsTransportEvent::TransportStopped => (
            QgsRuntimeEventKind::TransportStopped,
            None,
            "transport stopped".to_string(),
        ),
    }
}

fn runtime_payload_from_tick_event(
    event: &QgsTickPreparationEvent,
) -> (QgsRuntimeEventKind, String) {
    match event {
        QgsTickPreparationEvent::TickPreparationCompleted { prepared_frames } => (
            QgsRuntimeEventKind::TickPreparationCompleted,
            format!("tick preparation completed prepared_frames={prepared_frames}"),
        ),
        QgsTickPreparationEvent::PreparedStateDiscarded { frames, reason } => (
            QgsRuntimeEventKind::PreparedStateDiscarded,
            format!("prepared state discarded frames={frames:?} reason={reason:?}"),
        ),
        QgsTickPreparationEvent::TickValidationFailed { reason } => (
            QgsRuntimeEventKind::TransportCommandRejected,
            format!("tick validation failed reason={reason}"),
        ),
        other => (
            QgsRuntimeEventKind::TickPreparationCompleted,
            format!("tick event {other:?}"),
        ),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSourceUnloadResult {
    pub source_id: String,
    pub revision: QgsTransportSourceRevision,
    pub unloaded: bool,
    pub was_active: bool,
    pub revision_invalidated: bool,
    pub already_missing_or_invalid: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSourceCloseResult {
    pub source_id: String,
    pub revision: QgsTransportSourceRevision,
    pub active_source_cleared: bool,
    pub source_preserved_loaded: bool,
    pub play_ready: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsSourceRevisionInvalidation {
    pub revision: QgsTransportSourceRevision,
    pub invalidated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsRuntimeLifecycleSnapshot {
    pub active_source: Option<QgsTransportSourceHandle>,
    pub play_ready: bool,
    pub prepared_buffer_frames: usize,
    pub generation: QgsRuntimeEventGeneration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsRuntimeCommandOutcome {
    pub accepted: bool,
    pub reason: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QgsQncPlayerCommand {
    LoadPreparedInput,
    PreloadSource,
    SetActiveSource,
    SetActiveRange { start_frame: u64, end_frame: u64 },
    Cue { frame: u64 },
    PrepareAnchor,
    Play,
    Pause,
    Stop,
    TickPrepare { carrier_frame: u64 },
    CloseActiveSource,
    UnloadSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncCommandEnvelope {
    pub command_id: u64,
    pub expected_generation: Option<QgsRuntimeEventGeneration>,
    pub public_source_uri: Option<String>,
    pub command: QgsQncPlayerCommand,
    pub payload_summary: String,
}

impl QgsQncCommandEnvelope {
    pub fn new(
        command_id: u64,
        expected_generation: Option<QgsRuntimeEventGeneration>,
        public_source_uri: Option<String>,
        command: QgsQncPlayerCommand,
        payload_summary: impl Into<String>,
    ) -> Self {
        Self {
            command_id,
            expected_generation,
            public_source_uri,
            command,
            payload_summary: payload_summary.into(),
        }
    }

    pub fn exposes_private_path(&self) -> bool {
        self.public_source_uri
            .as_deref()
            .is_some_and(qgs_projection_text_exposes_private_path)
            || qgs_projection_text_exposes_private_path(&self.payload_summary)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncCommandOutcome {
    pub command_id: u64,
    pub accepted: bool,
    pub reason: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncProjectedEventKind {
    SourceLoaded,
    SourcePreloaded,
    ActiveSourceChanged,
    ActiveRangeChanged,
    CueChanged,
    PreparedAnchorChanged,
    PlaybackReadinessChanged,
    TransportStateChanged,
    TickPrepared,
    PreparedBufferChanged,
    SourceClosed,
    SourceUnloaded,
    CommandRejected,
    RuntimeGenerationChanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncProjectedEvent {
    pub kind: QgsQncProjectedEventKind,
    pub public_payload_summary: String,
    pub evidence_status: QgsQncEvidenceStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncEventEnvelope {
    pub sequence: u64,
    pub generation: u64,
    pub public_source_uri: Option<String>,
    pub event: QgsQncProjectedEvent,
}

impl QgsQncEventEnvelope {
    pub fn from_runtime_event(event: &QgsRuntimeEventEnvelope) -> Self {
        let kind = match event.kind {
            QgsRuntimeEventKind::RuntimeEngineCreated => {
                QgsQncProjectedEventKind::TransportStateChanged
            }
            QgsRuntimeEventKind::SourceLoaded => QgsQncProjectedEventKind::SourceLoaded,
            QgsRuntimeEventKind::SourcePreloaded => QgsQncProjectedEventKind::SourcePreloaded,
            QgsRuntimeEventKind::ActiveSourceChanged => {
                QgsQncProjectedEventKind::ActiveSourceChanged
            }
            QgsRuntimeEventKind::ActiveRangeSet => QgsQncProjectedEventKind::ActiveRangeChanged,
            QgsRuntimeEventKind::CueCompleted => QgsQncProjectedEventKind::CueChanged,
            QgsRuntimeEventKind::PreparedAnchorReady => {
                QgsQncProjectedEventKind::PreparedAnchorChanged
            }
            QgsRuntimeEventKind::PlayReadinessChanged => {
                QgsQncProjectedEventKind::PlaybackReadinessChanged
            }
            QgsRuntimeEventKind::TransportStarted
            | QgsRuntimeEventKind::TransportPaused
            | QgsRuntimeEventKind::TransportStopped => {
                QgsQncProjectedEventKind::TransportStateChanged
            }
            QgsRuntimeEventKind::TickPreparationCompleted => QgsQncProjectedEventKind::TickPrepared,
            QgsRuntimeEventKind::PreparedStateDiscarded => {
                QgsQncProjectedEventKind::PreparedBufferChanged
            }
            QgsRuntimeEventKind::ActiveSourceCleared => QgsQncProjectedEventKind::SourceClosed,
            QgsRuntimeEventKind::SourceRevisionInvalidated => {
                QgsQncProjectedEventKind::RuntimeGenerationChanged
            }
            QgsRuntimeEventKind::SourceUnloaded => QgsQncProjectedEventKind::SourceUnloaded,
            QgsRuntimeEventKind::TransportCommandRejected => {
                QgsQncProjectedEventKind::CommandRejected
            }
        };
        let evidence_status = match event.kind {
            QgsRuntimeEventKind::TickPreparationCompleted
            | QgsRuntimeEventKind::PreparedStateDiscarded => QgsQncEvidenceStatus::Prepared,
            _ => QgsQncEvidenceStatus::NotImplemented,
        };
        Self {
            sequence: event.sequence.0,
            generation: event.generation.0,
            public_source_uri: event.source.as_ref().map(|source| source.source_id.clone()),
            event: QgsQncProjectedEvent {
                kind,
                public_payload_summary: event.payload.summary.clone(),
                evidence_status,
            },
        }
    }

    pub fn exposes_private_path(&self) -> bool {
        self.public_source_uri
            .as_deref()
            .is_some_and(qgs_projection_text_exposes_private_path)
            || qgs_projection_text_exposes_private_path(&self.event.public_payload_summary)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncEvidenceStatus {
    NotImplemented,
    Prepared,
    SubmittedToDevice,
    Presented,
    Verified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncEvidenceView {
    pub prepared: QgsQncEvidenceStatus,
    pub submitted_to_device: QgsQncEvidenceStatus,
    pub presented: QgsQncEvidenceStatus,
    pub verified: QgsQncEvidenceStatus,
    pub realtime_verified: bool,
    pub audio_device_verified: bool,
    pub frame_presented: bool,
}

impl QgsQncEvidenceView {
    pub fn prepared_only() -> Self {
        Self {
            prepared: QgsQncEvidenceStatus::Prepared,
            submitted_to_device: QgsQncEvidenceStatus::NotImplemented,
            presented: QgsQncEvidenceStatus::NotImplemented,
            verified: QgsQncEvidenceStatus::NotImplemented,
            realtime_verified: false,
            audio_device_verified: false,
            frame_presented: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncSourceView {
    pub public_source_uri: Option<String>,
    pub public_video_uri: Option<String>,
    pub public_audio_uri: Option<String>,
    pub source_mode: Option<QgsInputPlanSourceMode>,
    pub revision: Option<QgsTransportSourceRevision>,
}

impl QgsQncSourceView {
    pub fn exposes_private_path(&self) -> bool {
        self.public_source_uri
            .as_deref()
            .is_some_and(qgs_projection_text_exposes_private_path)
            || self
                .public_video_uri
                .as_deref()
                .is_some_and(qgs_projection_text_exposes_private_path)
            || self
                .public_audio_uri
                .as_deref()
                .is_some_and(qgs_projection_text_exposes_private_path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncReadinessView {
    pub play_ready: bool,
    pub prepared_anchor_ready: bool,
    pub active_source_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncTransportView {
    pub state: QgsTransportStatus,
    pub active_range: Option<QgsTransportActiveRange>,
    pub cue: Option<QgsTransportCuePoint>,
    pub prepared_anchor: Option<QgsTransportPreparedAnchor>,
    pub carrier_frame: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncPreparedBufferView {
    pub prepared_frame_count: usize,
    pub prepared_start_frame: Option<u64>,
    pub prepared_end_frame_exclusive: Option<u64>,
    pub latest_discarded_frame_count: usize,
    pub payload_evidence: QgsQncEvidenceStatus,
}

impl QgsQncPreparedBufferView {
    pub fn from_slots(slots: &[QgsPreparedFrameSlot], latest_discarded_frame_count: usize) -> Self {
        let prepared_start_frame = slots.iter().map(|slot| slot.key.frame).min();
        let prepared_end_frame_exclusive = slots
            .iter()
            .filter_map(|slot| slot.key.frame.checked_add(1))
            .max();
        Self {
            prepared_frame_count: slots.len(),
            prepared_start_frame,
            prepared_end_frame_exclusive,
            latest_discarded_frame_count,
            payload_evidence: if slots.is_empty() {
                QgsQncEvidenceStatus::NotImplemented
            } else {
                QgsQncEvidenceStatus::Prepared
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncPassiveView {
    pub generation: QgsRuntimeEventGeneration,
    pub source: QgsQncSourceView,
    pub transport: QgsQncTransportView,
    pub readiness: QgsQncReadinessView,
    pub loaded_source_count: usize,
    pub prepared_buffer: QgsQncPreparedBufferView,
    pub evidence: QgsQncEvidenceView,
    pub private_path_exposed: bool,
}

impl QgsQncPassiveView {
    pub fn from_parts(
        generation: QgsRuntimeEventGeneration,
        snapshot: &QgsTransportSnapshot,
        slots: &[QgsPreparedFrameSlot],
        latest_discarded_frame_count: usize,
    ) -> Self {
        let source = snapshot.active_source.as_ref().map_or(
            QgsQncSourceView {
                public_source_uri: None,
                public_video_uri: None,
                public_audio_uri: None,
                source_mode: None,
                revision: None,
            },
            |handle| QgsQncSourceView {
                public_source_uri: Some(handle.source_id.clone()),
                public_video_uri: Some(handle.video_uri.clone()),
                public_audio_uri: Some(handle.audio_uri.clone()),
                source_mode: Some(handle.source_mode),
                revision: Some(handle.revision),
            },
        );
        let transport = QgsQncTransportView {
            state: snapshot.status,
            active_range: snapshot.active_range,
            cue: snapshot.cue,
            prepared_anchor: snapshot.prepared_anchor,
            carrier_frame: snapshot.cue.map(|cue| cue.frame),
        };
        let prepared_buffer =
            QgsQncPreparedBufferView::from_slots(slots, latest_discarded_frame_count);
        let private_path_exposed = source.exposes_private_path();
        Self {
            generation,
            source,
            transport,
            readiness: QgsQncReadinessView {
                play_ready: snapshot.play_ready,
                prepared_anchor_ready: snapshot.prepared_anchor.is_some(),
                active_source_ready: snapshot.active_source.is_some(),
            },
            loaded_source_count: snapshot.loaded_source_count,
            prepared_buffer,
            evidence: QgsQncEvidenceView::prepared_only(),
            private_path_exposed,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncTimelineProjection {
    pub active_range: Option<QgsTransportActiveRange>,
    pub carrier_frame: Option<u64>,
    pub cue_frame: Option<u64>,
    pub prepared_start_frame: Option<u64>,
    pub prepared_end_frame_exclusive: Option<u64>,
    pub frame_rate: Option<RationalRate>,
    pub audio_sample_rate: Option<u32>,
    pub presented_frame_claimed: bool,
}

impl QgsQncTimelineProjection {
    pub fn from_parts(snapshot: &QgsTransportSnapshot, slots: &[QgsPreparedFrameSlot]) -> Self {
        let handle = snapshot.active_source.as_ref();
        Self {
            active_range: snapshot.active_range,
            carrier_frame: snapshot.cue.map(|cue| cue.frame),
            cue_frame: snapshot.cue.map(|cue| cue.frame),
            prepared_start_frame: slots.iter().map(|slot| slot.key.frame).min(),
            prepared_end_frame_exclusive: slots
                .iter()
                .filter_map(|slot| slot.key.frame.checked_add(1))
                .max(),
            frame_rate: handle.map(|handle| handle.timebase),
            audio_sample_rate: handle.map(|handle| handle.audio_sample_rate),
            presented_frame_claimed: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncMonitorProjection {
    pub source_public_uri: Option<String>,
    pub prepared_descriptor_present: bool,
    pub source_frame: Option<u64>,
    pub payload_status: QgsQncEvidenceStatus,
    pub submitted_to_presenter: bool,
    pub presenter_evidence_kind: Option<QgsPresenterEvidenceKind>,
    pub presented: bool,
    pub frame_presented_test_boundary: bool,
    pub frame_presented_real_backend: bool,
    pub real_display_evidence: Option<&'static str>,
    pub real_display_evidence_present: bool,
    pub visual_verified: bool,
    pub private_path_exposed: bool,
}

impl QgsQncMonitorProjection {
    pub fn from_slots(slots: &[QgsPreparedFrameSlot]) -> Self {
        Self {
            source_public_uri: None,
            prepared_descriptor_present: !slots.is_empty(),
            source_frame: slots.first().map(|slot| slot.key.frame),
            payload_status: if slots.is_empty() {
                QgsQncEvidenceStatus::NotImplemented
            } else {
                QgsQncEvidenceStatus::Prepared
            },
            submitted_to_presenter: false,
            presenter_evidence_kind: None,
            presented: false,
            frame_presented_test_boundary: false,
            frame_presented_real_backend: false,
            real_display_evidence: None,
            real_display_evidence_present: false,
            visual_verified: false,
            private_path_exposed: false,
        }
    }

    pub fn from_presenter_update(update: &QgsMonitorProjectionUpdate) -> Self {
        Self {
            source_public_uri: Some(update.frame.source_public_uri.clone()),
            prepared_descriptor_present: update.frame.prepared_payload_present,
            source_frame: Some(update.frame.source_frame),
            payload_status: update.payload_status,
            submitted_to_presenter: update.submitted_to_presenter,
            presenter_evidence_kind: update.presenter_evidence_kind,
            presented: update.frame_presented_test_boundary || update.frame_presented_real_backend,
            frame_presented_test_boundary: update.frame_presented_test_boundary,
            frame_presented_real_backend: update.frame_presented_real_backend,
            real_display_evidence: if update.real_display_evidence_present {
                Some("real display evidence present")
            } else {
                None
            },
            real_display_evidence_present: update.real_display_evidence_present,
            visual_verified: update.visual_verified,
            private_path_exposed: update.private_path_exposed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsPresenterDeviceKind {
    TestPresenter,
    FilePresenter,
    RealDisplayPresenter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsPresenterCapability {
    AcceptsProcessedGpuFrame,
    AcceptsRgbaU16,
    ProvidesTestBoundaryEvidence,
    WritesDescriptorManifest,
    WritesImageFile,
    ProvidesRealDisplayEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsPresenterBoundaryStatus {
    NotConfigured,
    Ready,
    CapabilityMissing,
    SubmittedToPresenter,
    EvidenceReceived,
    SubmissionRejected,
    NotImplemented,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsPresenterEvidenceKind {
    TestPresenterAccepted,
    FilePresenterManifestWritten,
    FilePresenterImageWritten,
    RealPresenterEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum QgsVideoPresentationEvidenceLevel {
    PreparedPayload,
    SubmittedToPresenter,
    FilePresenterManifestWritten,
    FilePresenterImageWritten,
    PresentationEvidenceReceived,
    FramePresentedTestBoundary,
    FramePresentedRealBackend,
    VisualVerified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPresenterPayloadDescriptor {
    pub public_source_uri: String,
    pub source_mode: BroadcastVideoSourceMode,
    pub video_source_role: BroadcastMediaSourceRole,
    pub source_frame: u64,
    pub selected_preview_frame: Option<u64>,
    pub payload_id: u64,
    pub payload_kind: BroadcastVideoPayloadKind,
    pub payload_format: BroadcastVideoPayloadFormat,
    pub backend_path: BroadcastVideoPayloadBackendPath,
    pub presentation_time: Duration,
    pub duration: Duration,
    pub coded_width: u32,
    pub coded_height: u32,
    pub visible_width: u32,
    pub visible_height: u32,
    pub private_path_exposed: bool,
}

impl QgsPresenterPayloadDescriptor {
    pub fn from_video_binding(
        public_source_uri: impl Into<String>,
        binding: &BroadcastVideoPayloadBinding,
    ) -> Result<Self, PlaybackError> {
        let public_source_uri = public_source_uri.into();
        let payload = binding
            .payload
            .as_ref()
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        if binding.status != BroadcastVideoPayloadBindingStatus::PayloadReady {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        Ok(Self {
            private_path_exposed: qgs_projection_text_exposes_private_path(&public_source_uri),
            public_source_uri,
            source_mode: binding.source_mode,
            video_source_role: binding.video_source_role,
            source_frame: payload.source_frame_index,
            selected_preview_frame: payload.selected_preview_frame_index,
            payload_id: payload.payload_id,
            payload_kind: payload.kind,
            payload_format: payload.format,
            backend_path: payload.backend_path,
            presentation_time: payload.presentation_time,
            duration: payload.duration,
            coded_width: payload.coded_width,
            coded_height: payload.coded_height,
            visible_width: payload.visible_width,
            visible_height: payload.visible_height,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPresenterSubmission {
    pub presenter_kind: QgsPresenterDeviceKind,
    pub payload: QgsPresenterPayloadDescriptor,
    pub status: QgsPresenterBoundaryStatus,
    pub submitted_to_presenter: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPresenterEvidence {
    pub presentation_slot_index: usize,
    pub video_binding_index: usize,
    pub media_time: Duration,
    pub payload_id: u64,
    pub evidence_kind: QgsPresenterEvidenceKind,
    pub presenter_kind: QgsPresenterDeviceKind,
    pub evidence_level: QgsVideoPresentationEvidenceLevel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsPresenterSubmissionResult {
    pub submission: QgsPresenterSubmission,
    pub evidence: Option<QgsPresenterEvidence>,
    pub evidence_level: QgsVideoPresentationEvidenceLevel,
    pub frame_presented_test_boundary: bool,
    pub frame_presented_real_backend: bool,
    pub real_display_evidence_present: bool,
    pub visual_verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsMonitorFrameDescriptor {
    pub source_public_uri: String,
    pub source_frame: u64,
    pub selected_preview_frame: Option<u64>,
    pub prepared_payload_present: bool,
    pub payload_kind: BroadcastVideoPayloadKind,
    pub payload_format: BroadcastVideoPayloadFormat,
    pub private_path_exposed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsMonitorProjectionUpdate {
    pub frame: QgsMonitorFrameDescriptor,
    pub submitted_to_presenter: bool,
    pub presenter_evidence_kind: Option<QgsPresenterEvidenceKind>,
    pub payload_status: QgsQncEvidenceStatus,
    pub frame_presented_test_boundary: bool,
    pub frame_presented_real_backend: bool,
    pub real_display_evidence_present: bool,
    pub visual_verified: bool,
    pub evidence_level: QgsVideoPresentationEvidenceLevel,
    pub private_path_exposed: bool,
}

pub fn submit_qgs_presenter_payload_to_test_boundary(
    public_source_uri: impl Into<String>,
    presentation_slot: &BroadcastPresentationPayloadBinding,
    video_binding: &BroadcastVideoPayloadBinding,
    presenter: &mut BroadcastTestVideoPresenter,
) -> Result<QgsPresenterSubmissionResult, PlaybackError> {
    let descriptor =
        QgsPresenterPayloadDescriptor::from_video_binding(public_source_uri, video_binding)?;
    let submission = QgsPresenterSubmission {
        presenter_kind: QgsPresenterDeviceKind::TestPresenter,
        payload: descriptor,
        status: QgsPresenterBoundaryStatus::SubmittedToPresenter,
        submitted_to_presenter: true,
    };
    let evidence = presenter.submit(presentation_slot, video_binding)?;
    let evidence = QgsPresenterEvidence {
        presentation_slot_index: evidence.presentation_slot_index,
        video_binding_index: evidence
            .video_binding_index
            .ok_or(PlaybackError::InvalidRuntimeTransition)?,
        media_time: evidence.media_time,
        payload_id: evidence
            .payload_id
            .ok_or(PlaybackError::InvalidRuntimeTransition)?,
        evidence_kind: QgsPresenterEvidenceKind::TestPresenterAccepted,
        presenter_kind: QgsPresenterDeviceKind::TestPresenter,
        evidence_level: QgsVideoPresentationEvidenceLevel::FramePresentedTestBoundary,
    };
    Ok(QgsPresenterSubmissionResult {
        submission,
        evidence: Some(evidence),
        evidence_level: QgsVideoPresentationEvidenceLevel::FramePresentedTestBoundary,
        frame_presented_test_boundary: true,
        frame_presented_real_backend: false,
        real_display_evidence_present: false,
        visual_verified: false,
    })
}

pub fn project_qgs_monitor_update_from_presenter_result(
    result: &QgsPresenterSubmissionResult,
) -> QgsMonitorProjectionUpdate {
    let frame = QgsMonitorFrameDescriptor {
        source_public_uri: result.submission.payload.public_source_uri.clone(),
        source_frame: result.submission.payload.source_frame,
        selected_preview_frame: result.submission.payload.selected_preview_frame,
        prepared_payload_present: true,
        payload_kind: result.submission.payload.payload_kind,
        payload_format: result.submission.payload.payload_format,
        private_path_exposed: result.submission.payload.private_path_exposed,
    };
    let private_path_exposed = frame.private_path_exposed;
    QgsMonitorProjectionUpdate {
        frame,
        submitted_to_presenter: result.submission.submitted_to_presenter,
        presenter_evidence_kind: result
            .evidence
            .as_ref()
            .map(|evidence| evidence.evidence_kind),
        payload_status: if result.submission.submitted_to_presenter {
            QgsQncEvidenceStatus::SubmittedToDevice
        } else {
            QgsQncEvidenceStatus::Prepared
        },
        frame_presented_test_boundary: result.frame_presented_test_boundary,
        frame_presented_real_backend: result.frame_presented_real_backend,
        real_display_evidence_present: result.real_display_evidence_present,
        visual_verified: result.visual_verified,
        evidence_level: result.evidence_level,
        private_path_exposed,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsFilePresenterArtifactKind {
    ImageFile,
    DescriptorManifest,
    PlaceholderManifest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsVisualDiagnosticStatus {
    ImageFileWritten,
    ManifestWritten,
    PlaceholderWritten,
    VisualVerified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsFilePresenterArtifact {
    pub kind: QgsFilePresenterArtifactKind,
    pub public_artifact_path: String,
    pub image_written: bool,
    pub manifest_written: bool,
    pub reason: Option<&'static str>,
}

impl QgsFilePresenterArtifact {
    pub fn exposes_private_path(&self) -> bool {
        qgs_projection_text_exposes_private_path(&self.public_artifact_path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsFilePresenterSubmissionResult {
    pub submission: QgsPresenterSubmission,
    pub artifact: QgsFilePresenterArtifact,
    pub evidence: QgsPresenterEvidence,
    pub visual_status: QgsVisualDiagnosticStatus,
    pub real_display_evidence_present: bool,
    pub visual_verified: bool,
}

pub fn submit_qgs_file_presenter_manifest(
    public_source_uri: impl Into<String>,
    video_binding: &BroadcastVideoPayloadBinding,
    public_artifact_path: impl Into<String>,
) -> Result<QgsFilePresenterSubmissionResult, PlaybackError> {
    let descriptor =
        QgsPresenterPayloadDescriptor::from_video_binding(public_source_uri, video_binding)?;
    let artifact = QgsFilePresenterArtifact {
        kind: QgsFilePresenterArtifactKind::DescriptorManifest,
        public_artifact_path: public_artifact_path.into(),
        image_written: false,
        manifest_written: true,
        reason: Some("prepared payload descriptor does not expose CPU-readable frame pixels"),
    };
    if artifact.exposes_private_path() {
        return Err(PlaybackError::InvalidRuntimeTransition);
    }
    let submission = QgsPresenterSubmission {
        presenter_kind: QgsPresenterDeviceKind::FilePresenter,
        payload: descriptor,
        status: QgsPresenterBoundaryStatus::SubmittedToPresenter,
        submitted_to_presenter: true,
    };
    let evidence = QgsPresenterEvidence {
        presentation_slot_index: 0,
        video_binding_index: video_binding.video_slot_index,
        media_time: submission.payload.presentation_time,
        payload_id: submission.payload.payload_id,
        evidence_kind: QgsPresenterEvidenceKind::FilePresenterManifestWritten,
        presenter_kind: QgsPresenterDeviceKind::FilePresenter,
        evidence_level: QgsVideoPresentationEvidenceLevel::FilePresenterManifestWritten,
    };
    Ok(QgsFilePresenterSubmissionResult {
        submission,
        artifact,
        evidence,
        visual_status: QgsVisualDiagnosticStatus::ManifestWritten,
        real_display_evidence_present: false,
        visual_verified: false,
    })
}

pub fn submit_qgs_file_presenter_image(
    public_source_uri: impl Into<String>,
    video_binding: &BroadcastVideoPayloadBinding,
    public_artifact_path: impl Into<String>,
) -> Result<QgsFilePresenterSubmissionResult, PlaybackError> {
    let descriptor =
        QgsPresenterPayloadDescriptor::from_video_binding(public_source_uri, video_binding)?;
    let artifact = QgsFilePresenterArtifact {
        kind: QgsFilePresenterArtifactKind::ImageFile,
        public_artifact_path: public_artifact_path.into(),
        image_written: true,
        manifest_written: false,
        reason: None,
    };
    if artifact.exposes_private_path() {
        return Err(PlaybackError::InvalidRuntimeTransition);
    }
    let submission = QgsPresenterSubmission {
        presenter_kind: QgsPresenterDeviceKind::FilePresenter,
        payload: descriptor,
        status: QgsPresenterBoundaryStatus::SubmittedToPresenter,
        submitted_to_presenter: true,
    };
    let evidence = QgsPresenterEvidence {
        presentation_slot_index: 0,
        video_binding_index: video_binding.video_slot_index,
        media_time: submission.payload.presentation_time,
        payload_id: submission.payload.payload_id,
        evidence_kind: QgsPresenterEvidenceKind::FilePresenterImageWritten,
        presenter_kind: QgsPresenterDeviceKind::FilePresenter,
        evidence_level: QgsVideoPresentationEvidenceLevel::FilePresenterImageWritten,
    };
    Ok(QgsFilePresenterSubmissionResult {
        submission,
        artifact,
        evidence,
        visual_status: QgsVisualDiagnosticStatus::ImageFileWritten,
        real_display_evidence_present: false,
        visual_verified: false,
    })
}

pub fn project_qgs_monitor_update_from_file_presenter_result(
    result: &QgsFilePresenterSubmissionResult,
) -> QgsMonitorProjectionUpdate {
    let frame = QgsMonitorFrameDescriptor {
        source_public_uri: result.submission.payload.public_source_uri.clone(),
        source_frame: result.submission.payload.source_frame,
        selected_preview_frame: result.submission.payload.selected_preview_frame,
        prepared_payload_present: true,
        payload_kind: result.submission.payload.payload_kind,
        payload_format: result.submission.payload.payload_format,
        private_path_exposed: result.submission.payload.private_path_exposed
            || result.artifact.exposes_private_path(),
    };
    let private_path_exposed = frame.private_path_exposed;
    QgsMonitorProjectionUpdate {
        frame,
        submitted_to_presenter: result.submission.submitted_to_presenter,
        presenter_evidence_kind: Some(result.evidence.evidence_kind),
        payload_status: QgsQncEvidenceStatus::SubmittedToDevice,
        frame_presented_test_boundary: false,
        frame_presented_real_backend: false,
        real_display_evidence_present: result.real_display_evidence_present,
        visual_verified: result.visual_verified,
        evidence_level: result.evidence.evidence_level,
        private_path_exposed,
    }
}

pub fn project_qnc_events(events: &[QgsRuntimeEventEnvelope]) -> Vec<QgsQncEventEnvelope> {
    events
        .iter()
        .map(QgsQncEventEnvelope::from_runtime_event)
        .collect()
}

pub fn qgs_qnc_projected_sequence_is_monotonic(events: &[QgsQncEventEnvelope]) -> bool {
    events
        .iter()
        .enumerate()
        .all(|(index, event)| event.sequence == u64::try_from(index).unwrap_or(u64::MAX))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncSessionCommandResult {
    pub outcome: QgsQncCommandOutcome,
    pub projected_events: Vec<QgsQncEventEnvelope>,
    pub passive_view: QgsQncPassiveView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncSessionCommandExecutor {
    plan: QgsInputPlan,
    engine: QgsTransportEngine,
    event_log: QgsRuntimeEventLog,
    active_handle: Option<QgsTransportSourceHandle>,
    buffer: Option<QgsPlayoutBufferState>,
    last_seen_transport_events: usize,
    last_seen_buffer_events: usize,
    latest_discarded_frame_count: usize,
}

impl QgsQncSessionCommandExecutor {
    pub fn new(plan: QgsInputPlan) -> Result<Self, PlaybackError> {
        plan.validate()?;
        Ok(Self {
            plan,
            engine: QgsTransportEngine::new(),
            event_log: QgsRuntimeEventLog::new(),
            active_handle: None,
            buffer: None,
            last_seen_transport_events: 0,
            last_seen_buffer_events: 0,
            latest_discarded_frame_count: 0,
        })
    }

    pub fn current_generation(&self) -> QgsRuntimeEventGeneration {
        self.event_log.generation
    }

    pub fn passive_view(&self) -> QgsQncPassiveView {
        let slots = self
            .buffer
            .as_ref()
            .map_or_else(Vec::new, |buffer| buffer.slots());
        QgsQncPassiveView::from_parts(
            self.event_log.generation,
            &self.engine.snapshot(),
            &slots,
            self.latest_discarded_frame_count,
        )
    }

    pub fn execute(&mut self, command: &QgsQncCommandEnvelope) -> QgsQncSessionCommandResult {
        if command.exposes_private_path() {
            return self.rejected_command(command.command_id, "command exposes private path");
        }
        if command
            .expected_generation
            .is_some_and(|expected| expected != self.event_log.generation)
        {
            return self.rejected_command(command.command_id, "runtime generation mismatch");
        }

        let before_log_len = self.event_log.envelopes().len();
        let result = match command.command {
            QgsQncPlayerCommand::LoadPreparedInput => self.command_load_prepared_input(),
            QgsQncPlayerCommand::PreloadSource => self.command_preload_source(),
            QgsQncPlayerCommand::SetActiveSource => self.command_set_active_source(),
            QgsQncPlayerCommand::SetActiveRange {
                start_frame,
                end_frame,
            } => self.command_set_active_range(start_frame, end_frame),
            QgsQncPlayerCommand::Cue { frame } => self.command_cue(frame),
            QgsQncPlayerCommand::PrepareAnchor => self.command_prepare_anchor(),
            QgsQncPlayerCommand::Play => self.command_play(),
            QgsQncPlayerCommand::Pause => self.command_pause(),
            QgsQncPlayerCommand::Stop => self.command_stop(),
            QgsQncPlayerCommand::TickPrepare { carrier_frame } => {
                self.command_tick_prepare(carrier_frame)
            }
            QgsQncPlayerCommand::CloseActiveSource => self.command_close_active_source(),
            QgsQncPlayerCommand::UnloadSource => self.command_unload_source(),
        };

        let accepted = result.is_ok();
        let reason = result.err();
        self.push_new_runtime_events();
        let projected_events = project_qnc_events(&self.event_log.envelopes()[before_log_len..]);
        QgsQncSessionCommandResult {
            outcome: QgsQncCommandOutcome {
                command_id: command.command_id,
                accepted,
                reason,
            },
            projected_events,
            passive_view: self.passive_view(),
        }
    }

    fn rejected_command(
        &self,
        command_id: u64,
        reason: &'static str,
    ) -> QgsQncSessionCommandResult {
        QgsQncSessionCommandResult {
            outcome: QgsQncCommandOutcome {
                command_id,
                accepted: false,
                reason: Some(reason),
            },
            projected_events: Vec::new(),
            passive_view: self.passive_view(),
        }
    }

    fn command_load_prepared_input(&mut self) -> Result<(), &'static str> {
        if self.active_handle.is_some() || self.engine.snapshot().loaded_source_count != 0 {
            return Err("source already loaded");
        }
        let handle = self
            .engine
            .load_source(&self.plan)
            .map_err(|_| "load source failed")?;
        self.active_handle = Some(handle);
        Ok(())
    }

    fn command_preload_source(&mut self) -> Result<(), &'static str> {
        let handle = self.active_handle.clone().ok_or("no source loaded")?;
        self.engine
            .preload_source(&handle)
            .map_err(|_| "preload source failed")
    }

    fn command_set_active_source(&mut self) -> Result<(), &'static str> {
        let handle = self.active_handle.clone().ok_or("no source loaded")?;
        self.engine
            .set_active_source(&handle)
            .map_err(|_| "set active source failed")
    }

    fn command_set_active_range(
        &mut self,
        start_frame: u64,
        end_frame: u64,
    ) -> Result<(), &'static str> {
        self.engine
            .set_active_range_frames(start_frame, end_frame)
            .map(|_| ())
            .map_err(|_| "set active range failed")
    }

    fn command_cue(&mut self, frame: u64) -> Result<(), &'static str> {
        self.engine
            .cue_frame(frame)
            .map(|_| ())
            .map_err(|_| "cue failed")
    }

    fn command_prepare_anchor(&mut self) -> Result<(), &'static str> {
        self.engine
            .prepare_anchor()
            .map(|_| ())
            .map_err(|_| "prepare anchor failed")
    }

    fn command_play(&mut self) -> Result<(), &'static str> {
        if !self.engine.snapshot().play_ready {
            return Err("not ready");
        }
        self.engine.play().map_err(|_| "play failed")
    }

    fn command_pause(&mut self) -> Result<(), &'static str> {
        self.engine.pause().map_err(|_| "pause failed")
    }

    fn command_stop(&mut self) -> Result<(), &'static str> {
        self.engine.stop().map_err(|_| "stop failed")
    }

    fn command_tick_prepare(&mut self, carrier_frame: u64) -> Result<(), &'static str> {
        let snapshot = self.engine.snapshot();
        let handle = snapshot.active_source.ok_or("no active source")?;
        let range = snapshot.active_range.ok_or("no active range")?;
        let active_range = QgsActiveRangeTiming::new(
            handle.duration_frames,
            handle.timebase,
            handle.audio_sample_rate,
            range.start_frame,
            range.end_frame,
        )
        .map_err(|_| "active range timing failed")?;
        let limits = QgsPlayoutBufferLimits::default_transport_window();
        if self.buffer.is_none() {
            self.buffer = Some(
                QgsPlayoutBufferState::new(
                    active_range,
                    handle.revision,
                    handle.source_mode,
                    self.plan.audio_source.lanes.clone(),
                    limits,
                )
                .map_err(|_| "playout buffer create failed")?,
            );
            self.last_seen_buffer_events = 0;
        }
        let buffer = self.buffer.as_mut().ok_or("playout buffer missing")?;
        buffer
            .tick_prepare(
                QgsFrameClock::forward(active_range).map_err(|_| "frame clock create failed")?,
                QgsTickPreparationInput {
                    carrier_frame,
                    elapsed: Duration::ZERO,
                    max_due_frames: limits.max_prepared_frames,
                },
            )
            .map(|_| ())
            .map_err(|_| "tick prepare failed")
    }

    fn command_close_active_source(&mut self) -> Result<(), &'static str> {
        self.event_log.increment_generation();
        self.discard_buffer(QgsBufferDiscardReason::SourceClosed);
        self.engine
            .close_active_source()
            .map(|_| ())
            .map_err(|_| "close active source failed")
    }

    fn command_unload_source(&mut self) -> Result<(), &'static str> {
        let handle = self.active_handle.clone().ok_or("no source loaded")?;
        self.event_log.increment_generation();
        self.discard_buffer(QgsBufferDiscardReason::SourceUnloaded);
        let result = self.engine.unload_source(&handle);
        if result.unloaded {
            self.active_handle = None;
            Ok(())
        } else {
            Err("unload source failed")
        }
    }

    fn discard_buffer(&mut self, reason: QgsBufferDiscardReason) {
        if let Some(buffer) = self.buffer.as_mut() {
            let frames = buffer.discard_all(reason);
            self.latest_discarded_frame_count = frames.len();
            self.engine.record_prepared_state_discarded(frames.len());
            self.push_new_buffer_events();
            self.buffer = None;
            self.last_seen_buffer_events = 0;
        }
    }

    fn push_new_runtime_events(&mut self) {
        let snapshot = self.engine.snapshot();
        for event in snapshot.events.iter().skip(self.last_seen_transport_events) {
            self.event_log.push_transport_event(event);
        }
        self.last_seen_transport_events = snapshot.events.len();
        self.push_new_buffer_events();
    }

    fn push_new_buffer_events(&mut self) {
        if let Some(buffer) = &self.buffer {
            for event in buffer.events().iter().skip(self.last_seen_buffer_events) {
                self.event_log.push_tick_event(event);
            }
            self.last_seen_buffer_events = buffer.events().len();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct QgsSessionCommandId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsSessionCommandQueueLimits {
    pub max_queued_commands: usize,
}

impl QgsSessionCommandQueueLimits {
    pub const fn default_control_surface() -> Self {
        Self {
            max_queued_commands: 16,
        }
    }

    pub fn validate(self) -> Result<Self, PlaybackError> {
        if self.max_queued_commands == 0 {
            Err(PlaybackError::InvalidCapacity)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsSessionDuplicateCommandPolicy {
    RejectStrict,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSessionRuntimeConfig {
    pub queue_limits: QgsSessionCommandQueueLimits,
    pub duplicate_policy: QgsSessionDuplicateCommandPolicy,
}

impl QgsSessionRuntimeConfig {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        self.queue_limits.validate()?;
        Ok(self)
    }
}

impl Default for QgsSessionRuntimeConfig {
    fn default() -> Self {
        Self {
            queue_limits: QgsSessionCommandQueueLimits::default_control_surface(),
            duplicate_policy: QgsSessionDuplicateCommandPolicy::RejectStrict,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsSessionQueueRejection {
    QueueFull,
    DuplicateCommandId,
    PrivatePathExposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsSessionCommandStatus {
    Enqueued,
    ExecutedAccepted,
    ExecutedRejected,
    EnqueueRejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSessionQueuedCommand {
    pub id: QgsSessionCommandId,
    pub envelope: QgsQncCommandEnvelope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSessionCommandTranscriptEntry {
    pub command_id: QgsSessionCommandId,
    pub command: QgsQncPlayerCommand,
    pub status: QgsSessionCommandStatus,
    pub rejection: Option<&'static str>,
    pub generation_before: QgsRuntimeEventGeneration,
    pub generation_after: QgsRuntimeEventGeneration,
    pub projected_event_count: usize,
    pub passive_view: QgsQncPassiveView,
}

impl QgsSessionCommandTranscriptEntry {
    pub fn exposes_private_path(&self) -> bool {
        self.passive_view.private_path_exposed
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QgsSessionCommandTranscript {
    entries: Vec<QgsSessionCommandTranscriptEntry>,
}

impl QgsSessionCommandTranscript {
    pub fn entries(&self) -> &[QgsSessionCommandTranscriptEntry] {
        &self.entries
    }

    pub fn command_ids(&self) -> Vec<u64> {
        self.entries
            .iter()
            .map(|entry| entry.command_id.0)
            .collect()
    }

    pub fn exposes_private_path(&self) -> bool {
        self.entries
            .iter()
            .any(QgsSessionCommandTranscriptEntry::exposes_private_path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSessionCommandResult {
    pub command_id: QgsSessionCommandId,
    pub status: QgsSessionCommandStatus,
    pub outcome: Option<QgsQncCommandOutcome>,
    pub queue_rejection: Option<QgsSessionQueueRejection>,
    pub projected_events: Vec<QgsQncEventEnvelope>,
    pub passive_view: QgsQncPassiveView,
    pub generation_before: QgsRuntimeEventGeneration,
    pub generation_after: QgsRuntimeEventGeneration,
}

impl QgsSessionCommandResult {
    pub fn exposes_private_path(&self) -> bool {
        self.projected_events
            .iter()
            .any(QgsQncEventEnvelope::exposes_private_path)
            || self.passive_view.private_path_exposed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSessionRuntimeSnapshot {
    pub generation: QgsRuntimeEventGeneration,
    pub queued_commands: usize,
    pub executed_commands: usize,
    pub rejected_enqueue_commands: usize,
    pub passive_view: QgsQncPassiveView,
    pub private_path_exposed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSessionCommandExecutionReport {
    pub executed: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub projected_event_count: usize,
    pub transcript: QgsSessionCommandTranscript,
    pub final_snapshot: QgsSessionRuntimeSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsSessionRuntime {
    config: QgsSessionRuntimeConfig,
    executor: QgsQncSessionCommandExecutor,
    queue: VecDeque<QgsSessionQueuedCommand>,
    queued_command_ids: BTreeSet<QgsSessionCommandId>,
    executed_command_ids: BTreeSet<QgsSessionCommandId>,
    duplicate_rejected_command_ids: BTreeSet<QgsSessionCommandId>,
    rejected_enqueue_commands: usize,
    transcript: QgsSessionCommandTranscript,
}

impl QgsSessionRuntime {
    pub fn new(plan: QgsInputPlan, config: QgsSessionRuntimeConfig) -> Result<Self, PlaybackError> {
        Ok(Self {
            config: config.validate()?,
            executor: QgsQncSessionCommandExecutor::new(plan)?,
            queue: VecDeque::new(),
            queued_command_ids: BTreeSet::new(),
            executed_command_ids: BTreeSet::new(),
            duplicate_rejected_command_ids: BTreeSet::new(),
            rejected_enqueue_commands: 0,
            transcript: QgsSessionCommandTranscript::default(),
        })
    }

    pub fn config(&self) -> &QgsSessionRuntimeConfig {
        &self.config
    }

    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }

    pub fn transcript(&self) -> &QgsSessionCommandTranscript {
        &self.transcript
    }

    pub fn enqueue(
        &mut self,
        envelope: QgsQncCommandEnvelope,
    ) -> Result<QgsSessionCommandStatus, QgsSessionQueueRejection> {
        let id = QgsSessionCommandId(envelope.command_id);
        if envelope.exposes_private_path() {
            self.rejected_enqueue_commands = self.rejected_enqueue_commands.saturating_add(1);
            return Err(QgsSessionQueueRejection::PrivatePathExposed);
        }
        if self.queued_command_ids.contains(&id)
            || self.executed_command_ids.contains(&id)
            || self.duplicate_rejected_command_ids.contains(&id)
        {
            self.duplicate_rejected_command_ids.insert(id);
            self.rejected_enqueue_commands = self.rejected_enqueue_commands.saturating_add(1);
            return Err(QgsSessionQueueRejection::DuplicateCommandId);
        }
        if self.queue.len() >= self.config.queue_limits.max_queued_commands {
            self.rejected_enqueue_commands = self.rejected_enqueue_commands.saturating_add(1);
            return Err(QgsSessionQueueRejection::QueueFull);
        }
        self.queued_command_ids.insert(id);
        self.queue
            .push_back(QgsSessionQueuedCommand { id, envelope });
        Ok(QgsSessionCommandStatus::Enqueued)
    }

    pub fn execute_next(&mut self) -> Option<QgsSessionCommandResult> {
        let queued = self.queue.pop_front()?;
        self.queued_command_ids.remove(&queued.id);
        let generation_before = self.executor.current_generation();
        let result = self.executor.execute(&queued.envelope);
        let generation_after = self.executor.current_generation();
        let status = if result.outcome.accepted {
            QgsSessionCommandStatus::ExecutedAccepted
        } else {
            QgsSessionCommandStatus::ExecutedRejected
        };
        self.executed_command_ids.insert(queued.id);
        let command_result = QgsSessionCommandResult {
            command_id: queued.id,
            status,
            outcome: Some(result.outcome.clone()),
            queue_rejection: None,
            projected_events: result.projected_events.clone(),
            passive_view: result.passive_view.clone(),
            generation_before,
            generation_after,
        };
        self.transcript
            .entries
            .push(QgsSessionCommandTranscriptEntry {
                command_id: queued.id,
                command: queued.envelope.command,
                status,
                rejection: result.outcome.reason,
                generation_before,
                generation_after,
                projected_event_count: result.projected_events.len(),
                passive_view: result.passive_view,
            });
        Some(command_result)
    }

    pub fn drain_queue(&mut self) -> QgsSessionCommandExecutionReport {
        let mut executed = 0usize;
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        let mut projected_event_count = 0usize;
        while let Some(result) = self.execute_next() {
            executed = executed.saturating_add(1);
            projected_event_count =
                projected_event_count.saturating_add(result.projected_events.len());
            match result.status {
                QgsSessionCommandStatus::ExecutedAccepted => {
                    accepted = accepted.saturating_add(1);
                }
                QgsSessionCommandStatus::ExecutedRejected => {
                    rejected = rejected.saturating_add(1);
                }
                QgsSessionCommandStatus::Enqueued | QgsSessionCommandStatus::EnqueueRejected => {}
            }
        }
        QgsSessionCommandExecutionReport {
            executed,
            accepted,
            rejected,
            projected_event_count,
            transcript: self.transcript.clone(),
            final_snapshot: self.snapshot(),
        }
    }

    pub fn snapshot(&self) -> QgsSessionRuntimeSnapshot {
        let passive_view = self.executor.passive_view();
        let private_path_exposed = passive_view.private_path_exposed
            || self.transcript.exposes_private_path()
            || self
                .queue
                .iter()
                .any(|queued| queued.envelope.exposes_private_path());
        QgsSessionRuntimeSnapshot {
            generation: self.executor.current_generation(),
            queued_commands: self.queue.len(),
            executed_commands: self.executed_command_ids.len(),
            rejected_enqueue_commands: self.rejected_enqueue_commands,
            passive_view,
            private_path_exposed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsBroadcastPlayerStatus {
    Empty,
    Loaded,
    Preparing,
    Ready,
    Playing,
    Paused,
    Stopped,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QgsBroadcastPlayerCommand {
    LoadPreparedInput,
    Prepare { active_range: Option<(u64, u64)> },
    Cue { frame: u64 },
    Play,
    Pause,
    Seek { frame: u64 },
    Stop,
    Unload,
    Tick { carrier_frame: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsBroadcastPlayerEventKind {
    PlayerLoaded,
    PlayerPreparing,
    PlayerReady,
    PlayerCued,
    PlayerStarted,
    PlayerPaused,
    PlayerSeeked,
    PlayerStopped,
    PlayerUnloaded,
    PlayerFailed,
    PlayerTicked,
    PlayerCommandRejected,
    PlayerFaultRecorded,
    PlayerRecoverySuggested,
    PlayerEnteredFailed,
    PlayerRecovered,
    PlayerWarningRecorded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerEvent {
    pub sequence: u64,
    pub kind: QgsBroadcastPlayerEventKind,
    pub status: QgsBroadcastPlayerStatus,
    pub public_source_uri: Option<String>,
    pub summary: String,
}

impl QgsBroadcastPlayerEvent {
    pub fn exposes_private_path(&self) -> bool {
        self.public_source_uri
            .as_deref()
            .is_some_and(qgs_projection_text_exposes_private_path)
            || qgs_projection_text_exposes_private_path(&self.summary)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerReadiness {
    pub input_valid: bool,
    pub source_loaded: bool,
    pub transport_ready: bool,
    pub cue_ready: bool,
    pub prepared_window_ready: bool,
    pub video_payload_ready: bool,
    pub audio_payload_ready: bool,
    pub presenter_backend_ready: bool,
    pub audio_backend_ready: bool,
    pub real_display_ready: bool,
    pub audio_device_verified: bool,
    pub visual_verified: bool,
    pub realtime_verified: bool,
    pub av_sync_verified: bool,
    pub proxy_aac_authoritative: bool,
    pub discrete_mono_audio: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerPosition {
    pub current_frame: Option<u64>,
    pub cue_frame: Option<u64>,
    pub active_range: Option<QgsTransportActiveRange>,
    pub current_audio_sample_range: Option<(u64, u64)>,
    pub frame_rate: RationalRate,
    pub audio_sample_rate: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerPreparedWindow {
    pub prepared_start_frame: Option<u64>,
    pub prepared_end_frame_exclusive: Option<u64>,
    pub selected_prepared_frame: Option<u64>,
    pub prepared_frame_count: usize,
    pub latest_discarded_frame_count: usize,
    pub window_policy: QgsPlayoutBufferLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsDeviceSelectionPolicy {
    DiagnosticOnly,
    PreviewOnQncOs,
    OriginalMediaOnQncOs,
    HeadlessCi,
    FutureApplianceDirect,
}

impl QgsDeviceSelectionPolicy {
    pub const fn label(self) -> &'static str {
        match self {
            Self::DiagnosticOnly => "diagnostic-only",
            Self::PreviewOnQncOs => "preview-qnc-os",
            Self::OriginalMediaOnQncOs => "original-qnc-os",
            Self::HeadlessCi => "headless-ci",
            Self::FutureApplianceDirect => "future-appliance-direct",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsVideoPresenterBackendKind {
    TestPresenter,
    FilePresenterDiagnostic,
    GpuReadbackDiagnostic,
    WaylandVulkanPresenter,
    DrmKmsVulkanPresenter,
    X11LegacyNonTarget,
}

impl QgsVideoPresenterBackendKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::TestPresenter => "TestPresenter",
            Self::FilePresenterDiagnostic => "FilePresenterDiagnostic",
            Self::GpuReadbackDiagnostic => "GpuReadbackDiagnostic",
            Self::WaylandVulkanPresenter => "WaylandVulkanPresenter",
            Self::DrmKmsVulkanPresenter => "DrmKmsVulkanPresenter",
            Self::X11LegacyNonTarget => "X11LegacyNonTarget",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsAudioOutputBackendKind {
    NoAudioOutput,
    TestAudioSink,
    PipeWireCommandPrototype,
    PipeWireNativePrototype,
    PipeWireProductionFuture,
}

impl QgsAudioOutputBackendKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::NoAudioOutput => "NoAudioOutput",
            Self::TestAudioSink => "TestAudioSink",
            Self::PipeWireCommandPrototype => "PipeWireCommandPrototype",
            Self::PipeWireNativePrototype => "PipeWireNativePrototype",
            Self::PipeWireProductionFuture => "PipeWireProductionFuture",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsDeviceBackendAvailability {
    Available,
    AvailableDiagnosticOnly,
    NotImplemented,
    UnsupportedForQncOs,
    MissingRuntimeDependency,
    HardwareUnavailable,
    NotProductionVerified,
    DisabledByPolicy,
}

impl QgsDeviceBackendAvailability {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::AvailableDiagnosticOnly => "AvailableDiagnosticOnly",
            Self::NotImplemented => "NotImplemented",
            Self::UnsupportedForQncOs => "UnsupportedForQncOs",
            Self::MissingRuntimeDependency => "MissingRuntimeDependency",
            Self::HardwareUnavailable => "HardwareUnavailable",
            Self::NotProductionVerified => "NotProductionVerified",
            Self::DisabledByPolicy => "DisabledByPolicy",
        }
    }

    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Available)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsVideoBackendCandidate {
    pub kind: QgsVideoPresenterBackendKind,
    pub availability: QgsDeviceBackendAvailability,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsAudioBackendCandidate {
    pub kind: QgsAudioOutputBackendKind,
    pub availability: QgsDeviceBackendAvailability,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsDeviceBackendSelection {
    pub policy: QgsDeviceSelectionPolicy,
    pub selected_video_backend: QgsVideoPresenterBackendKind,
    pub selected_video_availability: QgsDeviceBackendAvailability,
    pub selected_audio_backend: QgsAudioOutputBackendKind,
    pub selected_audio_availability: QgsDeviceBackendAvailability,
    pub diagnostic_video_fallback: Option<QgsVideoPresenterBackendKind>,
    pub diagnostic_audio_fallback: Option<QgsAudioOutputBackendKind>,
    pub video_candidates: Vec<QgsVideoBackendCandidate>,
    pub audio_candidates: Vec<QgsAudioBackendCandidate>,
    pub unavailable_reasons: Vec<&'static str>,
    pub qnc_os_display_target: &'static str,
    pub x11_target: &'static str,
    pub real_display_ready: bool,
    pub real_audio_backend_ready: bool,
    pub visual_verified: bool,
    pub realtime_verified: bool,
    pub audio_device_production_verified: bool,
    pub av_sync_verified: bool,
}

impl QgsDeviceBackendSelection {
    pub fn exposes_private_path(&self) -> bool {
        self.video_candidates
            .iter()
            .any(|candidate| qgs_projection_text_exposes_private_path(candidate.reason))
            || self
                .audio_candidates
                .iter()
                .any(|candidate| qgs_projection_text_exposes_private_path(candidate.reason))
            || self
                .unavailable_reasons
                .iter()
                .any(|reason| qgs_projection_text_exposes_private_path(reason))
    }
}

pub struct QgsDeviceBackendSelector;

impl QgsDeviceBackendSelector {
    pub fn select(policy: QgsDeviceSelectionPolicy) -> QgsDeviceBackendSelection {
        let video_candidates = Self::video_candidates();
        let audio_candidates = Self::audio_candidates();
        let (
            selected_video_backend,
            diagnostic_video_fallback,
            selected_audio_backend,
            diagnostic_audio_fallback,
            unavailable_reasons,
        ) = match policy {
            QgsDeviceSelectionPolicy::DiagnosticOnly => (
                QgsVideoPresenterBackendKind::GpuReadbackDiagnostic,
                Some(QgsVideoPresenterBackendKind::FilePresenterDiagnostic),
                QgsAudioOutputBackendKind::TestAudioSink,
                Some(QgsAudioOutputBackendKind::NoAudioOutput),
                vec!["diagnostic-only policy does not select a real display or production audio backend"],
            ),
            QgsDeviceSelectionPolicy::PreviewOnQncOs => (
                QgsVideoPresenterBackendKind::WaylandVulkanPresenter,
                Some(QgsVideoPresenterBackendKind::GpuReadbackDiagnostic),
                QgsAudioOutputBackendKind::PipeWireProductionFuture,
                Some(QgsAudioOutputBackendKind::TestAudioSink),
                vec![
                    "Wayland/Vulkan presenter is the QNC OS target but is not implemented yet",
                    "PipeWire production audio output is not implemented or production verified yet",
                    "X11 is legacy/non-target for QNC OS and is not selected",
                ],
            ),
            QgsDeviceSelectionPolicy::OriginalMediaOnQncOs => (
                QgsVideoPresenterBackendKind::WaylandVulkanPresenter,
                Some(QgsVideoPresenterBackendKind::GpuReadbackDiagnostic),
                QgsAudioOutputBackendKind::PipeWireProductionFuture,
                Some(QgsAudioOutputBackendKind::TestAudioSink),
                vec![
                    "OriginalMedia still targets Wayland/Vulkan for QNC OS display but the real presenter is not implemented",
                    "OriginalMedia production audio output is not implemented or production verified yet",
                    "X11 is legacy/non-target for QNC OS and is not selected",
                ],
            ),
            QgsDeviceSelectionPolicy::HeadlessCi => (
                QgsVideoPresenterBackendKind::TestPresenter,
                Some(QgsVideoPresenterBackendKind::FilePresenterDiagnostic),
                QgsAudioOutputBackendKind::NoAudioOutput,
                Some(QgsAudioOutputBackendKind::TestAudioSink),
                vec!["headless-ci policy selects deterministic non-real-display and non-production-audio boundaries"],
            ),
            QgsDeviceSelectionPolicy::FutureApplianceDirect => (
                QgsVideoPresenterBackendKind::DrmKmsVulkanPresenter,
                Some(QgsVideoPresenterBackendKind::GpuReadbackDiagnostic),
                QgsAudioOutputBackendKind::PipeWireProductionFuture,
                Some(QgsAudioOutputBackendKind::TestAudioSink),
                vec![
                    "DRM/KMS Vulkan direct-output presenter is a future appliance path and is not implemented",
                    "PipeWire production audio output is not implemented or production verified yet",
                ],
            ),
        };
        let selected_video_availability =
            Self::video_availability(&video_candidates, selected_video_backend);
        let selected_audio_availability =
            Self::audio_availability(&audio_candidates, selected_audio_backend);
        QgsDeviceBackendSelection {
            policy,
            selected_video_backend,
            selected_video_availability,
            selected_audio_backend,
            selected_audio_availability,
            diagnostic_video_fallback,
            diagnostic_audio_fallback,
            video_candidates,
            audio_candidates,
            unavailable_reasons,
            qnc_os_display_target: "Wayland + Vulkan",
            x11_target: "no / legacy non-target",
            real_display_ready: selected_video_availability.is_ready(),
            real_audio_backend_ready: selected_audio_availability.is_ready(),
            visual_verified: false,
            realtime_verified: false,
            audio_device_production_verified: false,
            av_sync_verified: false,
        }
    }

    fn video_candidates() -> Vec<QgsVideoBackendCandidate> {
        vec![
            QgsVideoBackendCandidate {
                kind: QgsVideoPresenterBackendKind::TestPresenter,
                availability: QgsDeviceBackendAvailability::AvailableDiagnosticOnly,
                reason: "test boundary only; not real display output",
            },
            QgsVideoBackendCandidate {
                kind: QgsVideoPresenterBackendKind::FilePresenterDiagnostic,
                availability: QgsDeviceBackendAvailability::AvailableDiagnosticOnly,
                reason: "deterministic file diagnostic only; not real display output",
            },
            QgsVideoBackendCandidate {
                kind: QgsVideoPresenterBackendKind::GpuReadbackDiagnostic,
                availability: QgsDeviceBackendAvailability::AvailableDiagnosticOnly,
                reason: "GPU payload readback diagnostic only; not real display output or VisualVerified",
            },
            QgsVideoBackendCandidate {
                kind: QgsVideoPresenterBackendKind::WaylandVulkanPresenter,
                availability: QgsDeviceBackendAvailability::NotImplemented,
                reason: "first real QNC OS display target; presenter boundary is not implemented yet",
            },
            QgsVideoBackendCandidate {
                kind: QgsVideoPresenterBackendKind::DrmKmsVulkanPresenter,
                availability: QgsDeviceBackendAvailability::NotImplemented,
                reason: "future direct/appliance display path; not implemented",
            },
            QgsVideoBackendCandidate {
                kind: QgsVideoPresenterBackendKind::X11LegacyNonTarget,
                availability: QgsDeviceBackendAvailability::UnsupportedForQncOs,
                reason: "legacy Linux compatibility only; not a QNC OS target and never selected by QNC OS policies",
            },
        ]
    }

    fn audio_candidates() -> Vec<QgsAudioBackendCandidate> {
        vec![
            QgsAudioBackendCandidate {
                kind: QgsAudioOutputBackendKind::NoAudioOutput,
                availability: QgsDeviceBackendAvailability::AvailableDiagnosticOnly,
                reason: "explicit no-output boundary for deterministic non-production runs",
            },
            QgsAudioBackendCandidate {
                kind: QgsAudioOutputBackendKind::TestAudioSink,
                availability: QgsDeviceBackendAvailability::AvailableDiagnosticOnly,
                reason: "test sink evidence only; not real speaker output",
            },
            QgsAudioBackendCandidate {
                kind: QgsAudioOutputBackendKind::PipeWireCommandPrototype,
                availability: QgsDeviceBackendAvailability::NotProductionVerified,
                reason: "command-backed PipeWire prototype evidence only; not production verified",
            },
            QgsAudioBackendCandidate {
                kind: QgsAudioOutputBackendKind::PipeWireNativePrototype,
                availability: QgsDeviceBackendAvailability::NotProductionVerified,
                reason: "native PipeWire prototype evidence only; not production audio output",
            },
            QgsAudioBackendCandidate {
                kind: QgsAudioOutputBackendKind::PipeWireProductionFuture,
                availability: QgsDeviceBackendAvailability::NotImplemented,
                reason: "future production audio backend; device policy and verification are not implemented",
            },
        ]
    }

    fn video_availability(
        candidates: &[QgsVideoBackendCandidate],
        kind: QgsVideoPresenterBackendKind,
    ) -> QgsDeviceBackendAvailability {
        candidates
            .iter()
            .find(|candidate| candidate.kind == kind)
            .map(|candidate| candidate.availability)
            .unwrap_or(QgsDeviceBackendAvailability::NotImplemented)
    }

    fn audio_availability(
        candidates: &[QgsAudioBackendCandidate],
        kind: QgsAudioOutputBackendKind,
    ) -> QgsDeviceBackendAvailability {
        candidates
            .iter()
            .find(|candidate| candidate.kind == kind)
            .map(|candidate| candidate.availability)
            .unwrap_or(QgsDeviceBackendAvailability::NotImplemented)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerDeviceStatus {
    pub selection: QgsDeviceBackendSelection,
    pub test_presenter_boundary_available: bool,
    pub file_presenter_diagnostic_available: bool,
    pub gpu_readback_diagnostic_available: bool,
    pub real_display_backend: &'static str,
    pub real_audio_backend: &'static str,
    pub qnc_os_display_target: &'static str,
    pub x11_target: &'static str,
    pub visual_verified: bool,
    pub realtime_verified: bool,
    pub audio_device_production_verified: bool,
    pub av_sync_verified: bool,
}

impl QgsBroadcastPlayerDeviceStatus {
    pub fn for_policy(policy: QgsDeviceSelectionPolicy) -> Self {
        let selection = QgsDeviceBackendSelector::select(policy);
        Self {
            real_display_backend: selection.selected_video_availability.label(),
            real_audio_backend: selection.selected_audio_availability.label(),
            qnc_os_display_target: selection.qnc_os_display_target,
            x11_target: selection.x11_target,
            visual_verified: selection.visual_verified,
            realtime_verified: selection.realtime_verified,
            audio_device_production_verified: selection.audio_device_production_verified,
            av_sync_verified: selection.av_sync_verified,
            selection,
            test_presenter_boundary_available: true,
            file_presenter_diagnostic_available: true,
            gpu_readback_diagnostic_available: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerEventCounters {
    pub product_events: usize,
    pub lower_level_events: usize,
    pub rejected_commands: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerSnapshot {
    pub status: QgsBroadcastPlayerStatus,
    pub public_source_uri: Option<String>,
    pub source_mode: QgsInputPlanSourceMode,
    pub selected_representation: QgsPlaybackRepresentation,
    pub position: QgsBroadcastPlayerPosition,
    pub readiness: QgsBroadcastPlayerReadiness,
    pub prepared_window: QgsBroadcastPlayerPreparedWindow,
    pub device_status: QgsBroadcastPlayerDeviceStatus,
    pub event_counters: QgsBroadcastPlayerEventCounters,
    pub passive_view: QgsQncPassiveView,
    pub private_path_exposed: bool,
}

impl QgsBroadcastPlayerSnapshot {
    pub fn exposes_private_path(&self) -> bool {
        self.private_path_exposed
            || self
                .public_source_uri
                .as_deref()
                .is_some_and(qgs_projection_text_exposes_private_path)
            || self.passive_view.private_path_exposed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerCommandResult {
    pub command: QgsBroadcastPlayerCommand,
    pub accepted: bool,
    pub reason: Option<&'static str>,
    pub lower_level_results: Vec<QgsSessionCommandResult>,
    pub product_events: Vec<QgsBroadcastPlayerEvent>,
    pub snapshot: QgsBroadcastPlayerSnapshot,
}

pub struct QgsBroadcastPlayerCore {
    plan: QgsInputPlan,
    runtime: QgsSessionRuntime,
    next_command_id: u64,
    product_events: Vec<QgsBroadcastPlayerEvent>,
    lower_level_event_count: usize,
    rejected_command_count: usize,
    failed: bool,
}

impl QgsBroadcastPlayerCore {
    pub fn new(plan: QgsInputPlan) -> Result<Self, PlaybackError> {
        plan.validate()?;
        Ok(Self {
            runtime: QgsSessionRuntime::new(plan.clone(), QgsSessionRuntimeConfig::default())?,
            plan,
            next_command_id: 1,
            product_events: Vec::new(),
            lower_level_event_count: 0,
            rejected_command_count: 0,
            failed: false,
        })
    }

    pub fn execute(
        &mut self,
        command: QgsBroadcastPlayerCommand,
    ) -> QgsBroadcastPlayerCommandResult {
        let mut lower_level_results = Vec::new();
        let mut command_accepted = true;
        let mut reason = None;
        let operations = self.lower_level_operations_for_command(&command);
        if operations.is_empty() {
            command_accepted = false;
            reason = Some("command has no lower-level mapping");
        }
        for operation in operations {
            let result = self.execute_lower_level(operation);
            if result.status == QgsSessionCommandStatus::ExecutedRejected {
                command_accepted = false;
                reason = result
                    .outcome
                    .as_ref()
                    .and_then(|outcome| outcome.reason)
                    .or(Some("lower-level command rejected"));
            }
            self.lower_level_event_count = self
                .lower_level_event_count
                .saturating_add(result.projected_events.len());
            lower_level_results.push(result);
            if !command_accepted {
                break;
            }
        }
        if !command_accepted {
            self.rejected_command_count = self.rejected_command_count.saturating_add(1);
            self.failed = matches!(command, QgsBroadcastPlayerCommand::LoadPreparedInput)
                && reason == Some("command has no lower-level mapping");
        }
        let before_events = self.product_events.len();
        self.push_product_event_for_command(&command, command_accepted, reason);
        let product_events = self.product_events[before_events..].to_vec();
        let snapshot = self.snapshot();
        QgsBroadcastPlayerCommandResult {
            command,
            accepted: command_accepted,
            reason,
            lower_level_results,
            product_events,
            snapshot,
        }
    }

    pub fn snapshot(&self) -> QgsBroadcastPlayerSnapshot {
        let runtime_snapshot = self.runtime.snapshot();
        self.snapshot_from_runtime(runtime_snapshot)
    }

    pub fn events(&self) -> &[QgsBroadcastPlayerEvent] {
        &self.product_events
    }

    fn execute_lower_level(&mut self, command: QgsQncPlayerCommand) -> QgsSessionCommandResult {
        let command_id = self.next_command_id;
        self.next_command_id = self.next_command_id.saturating_add(1);
        let source_uri = self
            .runtime
            .snapshot()
            .passive_view
            .source
            .public_source_uri;
        let envelope = QgsQncCommandEnvelope::new(
            command_id,
            Some(self.runtime.snapshot().generation),
            source_uri,
            command,
            "broadcast player control core",
        );
        if self.runtime.enqueue(envelope).is_err() {
            return QgsSessionCommandResult {
                command_id: QgsSessionCommandId(command_id),
                status: QgsSessionCommandStatus::EnqueueRejected,
                outcome: None,
                queue_rejection: Some(QgsSessionQueueRejection::QueueFull),
                projected_events: Vec::new(),
                passive_view: self.runtime.snapshot().passive_view,
                generation_before: self.runtime.snapshot().generation,
                generation_after: self.runtime.snapshot().generation,
            };
        }
        self.runtime
            .execute_next()
            .expect("just-enqueued command must execute")
    }

    fn lower_level_operations_for_command(
        &self,
        command: &QgsBroadcastPlayerCommand,
    ) -> Vec<QgsQncPlayerCommand> {
        match *command {
            QgsBroadcastPlayerCommand::LoadPreparedInput => {
                vec![QgsQncPlayerCommand::LoadPreparedInput]
            }
            QgsBroadcastPlayerCommand::Prepare { active_range } => {
                let snapshot = self.runtime.snapshot();
                let mut operations = Vec::new();
                if snapshot.passive_view.loaded_source_count > 0 {
                    operations.push(QgsQncPlayerCommand::PreloadSource);
                    if snapshot.passive_view.source.public_source_uri.is_none() {
                        operations.push(QgsQncPlayerCommand::SetActiveSource);
                    }
                    let (start_frame, end_frame) =
                        active_range.unwrap_or((0, self.plan.video_source.duration_frames));
                    let range_already_active = snapshot
                        .passive_view
                        .transport
                        .active_range
                        .is_some_and(|range| {
                            range.start_frame == start_frame && range.end_frame == end_frame
                        });
                    if !range_already_active {
                        operations.push(QgsQncPlayerCommand::SetActiveRange {
                            start_frame,
                            end_frame,
                        });
                    }
                    if snapshot.passive_view.transport.cue.is_some() {
                        operations.push(QgsQncPlayerCommand::PrepareAnchor);
                    }
                }
                operations
            }
            QgsBroadcastPlayerCommand::Cue { frame } => {
                vec![
                    QgsQncPlayerCommand::Cue { frame },
                    QgsQncPlayerCommand::PrepareAnchor,
                ]
            }
            QgsBroadcastPlayerCommand::Seek { frame } => vec![QgsQncPlayerCommand::Cue { frame }],
            QgsBroadcastPlayerCommand::Play => vec![QgsQncPlayerCommand::Play],
            QgsBroadcastPlayerCommand::Pause => vec![QgsQncPlayerCommand::Pause],
            QgsBroadcastPlayerCommand::Stop => vec![QgsQncPlayerCommand::Stop],
            QgsBroadcastPlayerCommand::Unload => {
                let snapshot = self.runtime.snapshot();
                if snapshot.passive_view.source.public_source_uri.is_some() {
                    let mut operations = Vec::new();
                    if snapshot.passive_view.transport.state != QgsTransportStatus::Stopped {
                        operations.push(QgsQncPlayerCommand::Stop);
                    }
                    operations.push(QgsQncPlayerCommand::CloseActiveSource);
                    operations.push(QgsQncPlayerCommand::UnloadSource);
                    operations
                } else {
                    Vec::new()
                }
            }
            QgsBroadcastPlayerCommand::Tick { carrier_frame } => {
                vec![QgsQncPlayerCommand::TickPrepare { carrier_frame }]
            }
        }
    }

    fn push_product_event_for_command(
        &mut self,
        command: &QgsBroadcastPlayerCommand,
        accepted: bool,
        reason: Option<&'static str>,
    ) {
        let kind = if !accepted {
            QgsBroadcastPlayerEventKind::PlayerFailed
        } else {
            match *command {
                QgsBroadcastPlayerCommand::LoadPreparedInput => {
                    QgsBroadcastPlayerEventKind::PlayerLoaded
                }
                QgsBroadcastPlayerCommand::Prepare { .. } => {
                    if self.runtime.snapshot().passive_view.readiness.play_ready {
                        QgsBroadcastPlayerEventKind::PlayerReady
                    } else {
                        QgsBroadcastPlayerEventKind::PlayerPreparing
                    }
                }
                QgsBroadcastPlayerCommand::Cue { .. } => QgsBroadcastPlayerEventKind::PlayerCued,
                QgsBroadcastPlayerCommand::Play => QgsBroadcastPlayerEventKind::PlayerStarted,
                QgsBroadcastPlayerCommand::Pause => QgsBroadcastPlayerEventKind::PlayerPaused,
                QgsBroadcastPlayerCommand::Seek { .. } => QgsBroadcastPlayerEventKind::PlayerSeeked,
                QgsBroadcastPlayerCommand::Stop => QgsBroadcastPlayerEventKind::PlayerStopped,
                QgsBroadcastPlayerCommand::Unload => QgsBroadcastPlayerEventKind::PlayerUnloaded,
                QgsBroadcastPlayerCommand::Tick { .. } => {
                    if self.runtime.snapshot().passive_view.readiness.play_ready {
                        self.push_event(
                            QgsBroadcastPlayerEventKind::PlayerReady,
                            "prepared window ready",
                        );
                    }
                    QgsBroadcastPlayerEventKind::PlayerTicked
                }
            }
        };
        self.push_event(kind, reason.unwrap_or("accepted"));
    }

    fn push_event(&mut self, kind: QgsBroadcastPlayerEventKind, summary: impl Into<String>) {
        let snapshot = self.runtime.snapshot();
        let status = self.status_from_passive(&snapshot.passive_view);
        self.product_events.push(QgsBroadcastPlayerEvent {
            sequence: u64::try_from(self.product_events.len() + 1).unwrap_or(u64::MAX),
            kind,
            status,
            public_source_uri: snapshot.passive_view.source.public_source_uri.or_else(|| {
                (snapshot.passive_view.loaded_source_count > 0)
                    .then(|| self.plan.video_source.media_uri.clone())
            }),
            summary: summary.into(),
        });
    }

    fn snapshot_from_runtime(
        &self,
        runtime_snapshot: QgsSessionRuntimeSnapshot,
    ) -> QgsBroadcastPlayerSnapshot {
        let passive = runtime_snapshot.passive_view.clone();
        let position = self.position_from_passive(&passive);
        let prepared_window = QgsBroadcastPlayerPreparedWindow {
            prepared_start_frame: passive.prepared_buffer.prepared_start_frame,
            prepared_end_frame_exclusive: passive.prepared_buffer.prepared_end_frame_exclusive,
            selected_prepared_frame: passive.transport.carrier_frame,
            prepared_frame_count: passive.prepared_buffer.prepared_frame_count,
            latest_discarded_frame_count: passive.prepared_buffer.latest_discarded_frame_count,
            window_policy: QgsPlayoutBufferLimits::default_transport_window(),
        };
        let public_source_uri = passive.source.public_source_uri.clone().or_else(|| {
            (passive.loaded_source_count > 0).then(|| self.plan.video_source.media_uri.clone())
        });
        let device_status =
            QgsBroadcastPlayerDeviceStatus::for_policy(QgsDeviceSelectionPolicy::PreviewOnQncOs);
        let device_selection_private_path = device_status.selection.exposes_private_path();
        let source_loaded = public_source_uri.is_some();
        let prepared_window_ready = passive.prepared_buffer.prepared_frame_count > 0;
        let readiness = QgsBroadcastPlayerReadiness {
            input_valid: true,
            source_loaded,
            transport_ready: passive.readiness.play_ready,
            cue_ready: passive.readiness.prepared_anchor_ready,
            prepared_window_ready,
            video_payload_ready: prepared_window_ready,
            audio_payload_ready: prepared_window_ready,
            presenter_backend_ready: device_status.selection.real_display_ready,
            audio_backend_ready: device_status.selection.real_audio_backend_ready,
            real_display_ready: device_status.selection.real_display_ready,
            audio_device_verified: device_status.audio_device_production_verified,
            visual_verified: device_status.visual_verified,
            realtime_verified: device_status.realtime_verified,
            av_sync_verified: device_status.av_sync_verified,
            proxy_aac_authoritative: false,
            discrete_mono_audio: !self.plan.audio_source.lanes.is_empty()
                && self
                    .plan
                    .audio_source
                    .lanes
                    .iter()
                    .all(|lane| lane.channel_index == 0),
        };
        QgsBroadcastPlayerSnapshot {
            status: self.status_from_passive(&passive),
            public_source_uri,
            source_mode: self.plan.source_mode,
            selected_representation: self.plan.video_source.representation,
            position,
            readiness,
            prepared_window,
            device_status,
            event_counters: QgsBroadcastPlayerEventCounters {
                product_events: self.product_events.len(),
                lower_level_events: self.lower_level_event_count,
                rejected_commands: self.rejected_command_count
                    + runtime_snapshot.rejected_enqueue_commands,
            },
            private_path_exposed: runtime_snapshot.private_path_exposed
                || self
                    .product_events
                    .iter()
                    .any(QgsBroadcastPlayerEvent::exposes_private_path)
                || device_selection_private_path,
            passive_view: passive,
        }
    }

    fn position_from_passive(&self, passive: &QgsQncPassiveView) -> QgsBroadcastPlayerPosition {
        let cue_frame = passive.transport.cue.map(|cue| cue.frame);
        let current_frame = passive.transport.carrier_frame.or(cue_frame);
        let current_audio_sample_range = passive.transport.cue.and_then(|cue| {
            self.plan
                .video_source
                .timebase
                .frame_duration()
                .ok()
                .and_then(|duration| {
                    audio_samples_for_duration(duration, self.plan.audio_source.sample_rate).ok()
                })
                .map(|samples| (cue.sample, cue.sample.saturating_add(samples)))
        });
        QgsBroadcastPlayerPosition {
            current_frame,
            cue_frame,
            active_range: passive.transport.active_range,
            current_audio_sample_range,
            frame_rate: self.plan.video_source.timebase,
            audio_sample_rate: self.plan.audio_source.sample_rate,
        }
    }

    fn status_from_passive(&self, passive: &QgsQncPassiveView) -> QgsBroadcastPlayerStatus {
        if self.failed {
            return QgsBroadcastPlayerStatus::Failed;
        }
        match passive.transport.state {
            QgsTransportStatus::Empty => QgsBroadcastPlayerStatus::Empty,
            QgsTransportStatus::Loaded => {
                if passive.readiness.play_ready {
                    QgsBroadcastPlayerStatus::Ready
                } else if passive.loaded_source_count > 0 {
                    QgsBroadcastPlayerStatus::Loaded
                } else {
                    QgsBroadcastPlayerStatus::Empty
                }
            }
            QgsTransportStatus::Ready => QgsBroadcastPlayerStatus::Ready,
            QgsTransportStatus::Playing => QgsBroadcastPlayerStatus::Playing,
            QgsTransportStatus::Paused => QgsBroadcastPlayerStatus::Paused,
            QgsTransportStatus::Stopped => {
                if passive.source.public_source_uri.is_none() {
                    QgsBroadcastPlayerStatus::Empty
                } else {
                    QgsBroadcastPlayerStatus::Stopped
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncCommandId(pub String);

impl QgsQncCommandId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncSessionId(pub String);

impl QgsQncSessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsQncGeneration(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncCommandKind {
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

impl QgsQncCommandKind {
    pub const fn is_mutating(self) -> bool {
        !matches!(self, Self::Snapshot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QgsQncCommandPayload {
    Empty,
    Frame { frame: u64 },
    Play { frame_count: Option<u64> },
    Preroll { target_frame: Option<u64> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncCommandRequestEnvelope {
    pub command_id: QgsQncCommandId,
    pub session_id: Option<QgsQncSessionId>,
    pub expected_generation: Option<QgsQncGeneration>,
    pub command: QgsQncCommandKind,
    pub payload: QgsQncCommandPayload,
    pub client_tag: Option<String>,
}

impl QgsQncCommandRequestEnvelope {
    pub fn new(command_id: impl Into<String>, command: QgsQncCommandKind) -> Self {
        Self {
            command_id: QgsQncCommandId::new(command_id),
            session_id: None,
            expected_generation: None,
            command,
            payload: QgsQncCommandPayload::Empty,
            client_tag: None,
        }
    }

    pub fn with_expected_generation(mut self, generation: QgsQncGeneration) -> Self {
        self.expected_generation = Some(generation);
        self
    }

    pub fn with_payload(mut self, payload: QgsQncCommandPayload) -> Self {
        self.payload = payload;
        self
    }

    pub fn exposes_private_path(&self) -> bool {
        self.client_tag
            .as_deref()
            .is_some_and(qgs_projection_text_exposes_private_path)
            || self
                .session_id
                .as_ref()
                .is_some_and(|session| qgs_projection_text_exposes_private_path(&session.0))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncControlEvidenceLevel {
    NotImplemented,
    Prepared,
    PayloadReady,
    TestBoundaryEvidence,
    NativePostSubmitEvidence,
    ControlSurfaceEvidence,
}

impl QgsQncControlEvidenceLevel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotImplemented => "NotImplemented",
            Self::Prepared => "Prepared",
            Self::PayloadReady => "PayloadReady",
            Self::TestBoundaryEvidence => "TestBoundaryEvidence",
            Self::NativePostSubmitEvidence => "NativePostSubmitEvidence",
            Self::ControlSurfaceEvidence => "QncControlSurfaceShapeEvidence",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncControlDeviceStatus {
    pub device_policy: &'static str,
    pub selected_video_backend: &'static str,
    pub selected_audio_backend: &'static str,
    pub qnc_os_display_target: &'static str,
    pub x11_target_status: &'static str,
    pub real_display_status: &'static str,
    pub presenter_evidence_level: QgsQncControlEvidenceLevel,
    pub audio_device_status: &'static str,
    pub visual_verified: bool,
    pub realtime_verified: bool,
    pub audio_device_verified: bool,
    pub av_sync_verified: bool,
}

impl QgsQncControlDeviceStatus {
    pub fn from_player(status: &QgsBroadcastPlayerDeviceStatus) -> Self {
        Self {
            device_policy: status.selection.policy.label(),
            selected_video_backend: status.selection.selected_video_backend.label(),
            selected_audio_backend: status.selection.selected_audio_backend.label(),
            qnc_os_display_target: status.qnc_os_display_target,
            x11_target_status: status.x11_target,
            real_display_status: status.real_display_backend,
            presenter_evidence_level: QgsQncControlEvidenceLevel::NotImplemented,
            audio_device_status: status.real_audio_backend,
            visual_verified: status.visual_verified,
            realtime_verified: status.realtime_verified,
            audio_device_verified: status.audio_device_production_verified,
            av_sync_verified: status.av_sync_verified,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncPreparedWindowView {
    pub start_frame: Option<u64>,
    pub end_frame_exclusive: Option<u64>,
    pub selected_frame: Option<u64>,
    pub prepared_frame_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncFaultRecord {
    pub fault_kind: &'static str,
    pub severity: &'static str,
    pub scope: &'static str,
    pub recoverable: bool,
    pub recovery_action: &'static str,
    pub command_id: Option<QgsQncCommandId>,
    pub state_mutated: bool,
    pub qnc_safe_code: &'static str,
    pub qnc_safe_message: Option<&'static str>,
    pub private_path_exposed: bool,
}

impl QgsQncFaultRecord {
    pub fn from_operational_fault(
        fault: &QgsOperationalFault,
        command_id: Option<QgsQncCommandId>,
        state_mutated: bool,
    ) -> Self {
        Self {
            fault_kind: fault.kind.label(),
            severity: fault.severity.label(),
            scope: fault.scope.label(),
            recoverable: fault.recoverable,
            recovery_action: fault.recovery_action.label(),
            command_id,
            state_mutated,
            qnc_safe_code: fault.qnc_safe_code,
            qnc_safe_message: Some(fault.reason),
            private_path_exposed: fault.exposes_private_path(),
        }
    }

    pub fn exposes_private_path(&self) -> bool {
        self.private_path_exposed
            || self
                .qnc_safe_message
                .is_some_and(qgs_projection_text_exposes_private_path)
            || self
                .command_id
                .as_ref()
                .is_some_and(|command_id| qgs_projection_text_exposes_private_path(&command_id.0))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncRuntimeSnapshot {
    pub generation: QgsQncGeneration,
    pub status: QgsBroadcastPlayerStatus,
    pub source_loaded: bool,
    pub public_source_uri: Option<String>,
    pub source_mode: Option<QgsInputPlanSourceMode>,
    pub picture_representation: Option<QgsPlaybackRepresentation>,
    pub authoritative_audio_source: &'static str,
    pub proxy_aac_authoritative: bool,
    pub broadcast_audio_model: &'static str,
    pub active_range: Option<(u64, u64)>,
    pub current_frame: Option<u64>,
    pub current_audio_sample_range: Option<(u64, u64)>,
    pub media_time: Option<Duration>,
    pub prepared_window: Option<QgsQncPreparedWindowView>,
    pub video_payload_ready: bool,
    pub audio_payload_ready: bool,
    pub buffer_status: &'static str,
    pub device_status: QgsQncControlDeviceStatus,
    pub active_faults: Vec<QgsQncFaultRecord>,
    pub warnings: Vec<&'static str>,
    pub private_path_exposed: bool,
}

impl QgsQncRuntimeSnapshot {
    pub fn exposes_private_path(&self) -> bool {
        self.private_path_exposed
            || self
                .public_source_uri
                .as_deref()
                .is_some_and(qgs_projection_text_exposes_private_path)
            || self
                .active_faults
                .iter()
                .any(QgsQncFaultRecord::exposes_private_path)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsQncControlEventKind {
    SessionCreated,
    SourceLoaded,
    PreparedInputAccepted,
    Cued,
    PrerollReady,
    Started,
    Ticked,
    Paused,
    Seeked,
    Stopped,
    Unloaded,
    CommandRejected,
    FaultRecorded,
    RecoverySuggested,
    SnapshotReported,
}

impl QgsQncControlEventKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::SessionCreated => "SessionCreated",
            Self::SourceLoaded => "SourceLoaded",
            Self::PreparedInputAccepted => "PreparedInputAccepted",
            Self::Cued => "Cued",
            Self::PrerollReady => "PrerollReady",
            Self::Started => "Started",
            Self::Ticked => "Ticked",
            Self::Paused => "Paused",
            Self::Seeked => "Seeked",
            Self::Stopped => "Stopped",
            Self::Unloaded => "Unloaded",
            Self::CommandRejected => "CommandRejected",
            Self::FaultRecorded => "FaultRecorded",
            Self::RecoverySuggested => "RecoverySuggested",
            Self::SnapshotReported => "SnapshotReported",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncControlEventEnvelope {
    pub sequence: u64,
    pub generation: QgsQncGeneration,
    pub event_kind: QgsQncControlEventKind,
    pub command_id: Option<QgsQncCommandId>,
    pub status: Option<QgsBroadcastPlayerStatus>,
    pub frame: Option<u64>,
    pub audio_sample_range: Option<(u64, u64)>,
    pub prepared_window: Option<QgsQncPreparedWindowView>,
    pub fault: Option<QgsQncFaultRecord>,
    pub recovery_action: Option<&'static str>,
    pub evidence_level: Option<QgsQncControlEvidenceLevel>,
    pub private_path_exposed: bool,
}

impl QgsQncControlEventEnvelope {
    pub fn exposes_private_path(&self) -> bool {
        self.private_path_exposed
            || self
                .command_id
                .as_ref()
                .is_some_and(|command_id| qgs_projection_text_exposes_private_path(&command_id.0))
            || self
                .fault
                .as_ref()
                .is_some_and(QgsQncFaultRecord::exposes_private_path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsQncCommandReplyEnvelope {
    pub command_id: QgsQncCommandId,
    pub accepted: bool,
    pub generation_before: QgsQncGeneration,
    pub generation_after: QgsQncGeneration,
    pub status_before: QgsBroadcastPlayerStatus,
    pub status_after: QgsBroadcastPlayerStatus,
    pub state_mutated: bool,
    pub rejection_reason: Option<&'static str>,
    pub fault: Option<QgsQncFaultRecord>,
    pub recovery_action: Option<&'static str>,
    pub snapshot: QgsQncRuntimeSnapshot,
    pub events: Vec<QgsQncControlEventEnvelope>,
    pub private_path_exposed: bool,
}

impl QgsQncCommandReplyEnvelope {
    pub fn exposes_private_path(&self) -> bool {
        self.private_path_exposed
            || qgs_projection_text_exposes_private_path(&self.command_id.0)
            || self.snapshot.exposes_private_path()
            || self
                .events
                .iter()
                .any(QgsQncControlEventEnvelope::exposes_private_path)
            || self
                .fault
                .as_ref()
                .is_some_and(QgsQncFaultRecord::exposes_private_path)
    }
}

pub struct QgsQncControlSurface {
    runtime: QgsBroadcastPlayerOperationalRuntime,
    session_id: QgsQncSessionId,
    generation: QgsQncGeneration,
    next_event_sequence: u64,
    active_frame_count: u64,
}

impl QgsQncControlSurface {
    pub fn from_assembly(
        assembly: &QgsBroadcastPlayerAssembly,
        session_id: impl Into<String>,
    ) -> Result<Self, PlaybackError> {
        Ok(Self {
            runtime: assembly.new_operational_runtime()?,
            session_id: QgsQncSessionId::new(session_id),
            generation: QgsQncGeneration(0),
            next_event_sequence: 1,
            active_frame_count: assembly.active_frame_count(),
        })
    }

    pub const fn generation(&self) -> QgsQncGeneration {
        self.generation
    }

    pub fn session_id(&self) -> &QgsQncSessionId {
        &self.session_id
    }

    pub fn snapshot(&self) -> QgsQncRuntimeSnapshot {
        self.snapshot_from_operational(&self.runtime.snapshot(), Vec::new())
    }

    pub fn handle_command(
        &mut self,
        request: QgsQncCommandRequestEnvelope,
    ) -> QgsQncCommandReplyEnvelope {
        let generation_before = self.generation;
        let status_before = self.runtime.snapshot().operational_status;
        if let Some(expected) = request.expected_generation {
            if expected != self.generation {
                let fault = QgsOperationalFault::new(
                    QgsOperationalFaultKind::StaleGeneration,
                    "expected generation mismatch",
                );
                let projected_fault = QgsQncFaultRecord::from_operational_fault(
                    &fault,
                    Some(request.command_id.clone()),
                    false,
                );
                let snapshot = self.snapshot_from_operational(
                    &self.runtime.snapshot(),
                    vec![projected_fault.clone()],
                );
                let event = self.event_from_snapshot(
                    QgsQncControlEventKind::CommandRejected,
                    Some(request.command_id.clone()),
                    &snapshot,
                    Some(projected_fault.clone()),
                    Some(QgsQncControlEvidenceLevel::ControlSurfaceEvidence),
                );
                let private_path_exposed = snapshot.exposes_private_path()
                    || event.exposes_private_path()
                    || request.exposes_private_path();
                return QgsQncCommandReplyEnvelope {
                    command_id: request.command_id,
                    accepted: false,
                    generation_before,
                    generation_after: generation_before,
                    status_before,
                    status_after: snapshot.status,
                    state_mutated: false,
                    rejection_reason: Some("StaleGeneration"),
                    recovery_action: Some(fault.recovery_action.label()),
                    fault: Some(projected_fault),
                    private_path_exposed,
                    snapshot,
                    events: vec![event],
                };
            }
        }

        if matches!(request.command, QgsQncCommandKind::Snapshot) {
            let snapshot = self.snapshot();
            let event = self.event_from_snapshot(
                QgsQncControlEventKind::SnapshotReported,
                Some(request.command_id.clone()),
                &snapshot,
                None,
                Some(QgsQncControlEvidenceLevel::ControlSurfaceEvidence),
            );
            let private_path_exposed = snapshot.exposes_private_path()
                || event.exposes_private_path()
                || request.exposes_private_path();
            return QgsQncCommandReplyEnvelope {
                command_id: request.command_id,
                accepted: true,
                generation_before,
                generation_after: generation_before,
                status_before,
                status_after: snapshot.status,
                state_mutated: false,
                rejection_reason: None,
                fault: None,
                recovery_action: None,
                private_path_exposed,
                snapshot,
                events: vec![event],
            };
        }

        let before_public = self.runtime.snapshot();
        let execution = self.execute_control_command(&request);
        let accepted = execution.accepted;
        if accepted && request.command.is_mutating() {
            self.generation = QgsQncGeneration(self.generation.0.saturating_add(1));
        }
        let generation_after = self.generation;
        let mut faults = Vec::new();
        let mut projected_fault = None;
        let mut recovery_action = None;
        if !accepted {
            let fault = QgsOperationalFault::new(
                self.fault_kind_for_rejection(request.command),
                execution.reason.unwrap_or("command rejected"),
            );
            let record = QgsQncFaultRecord::from_operational_fault(
                &fault,
                Some(request.command_id.clone()),
                false,
            );
            recovery_action = Some(fault.recovery_action.label());
            projected_fault = Some(record.clone());
            faults.push(record);
        }
        let snapshot = self.snapshot_from_operational(&self.runtime.snapshot(), faults);
        let state_mutated = accepted && before_public != self.runtime.snapshot();
        let mut events = Vec::new();
        for kind in execution.event_kinds {
            events.push(self.event_from_snapshot(
                kind,
                Some(request.command_id.clone()),
                &snapshot,
                None,
                Some(QgsQncControlEvidenceLevel::ControlSurfaceEvidence),
            ));
        }
        for tick_snapshot in execution.tick_snapshots {
            events.push(self.event_from_snapshot(
                QgsQncControlEventKind::Ticked,
                Some(request.command_id.clone()),
                &self.snapshot_from_operational(&tick_snapshot, Vec::new()),
                None,
                Some(QgsQncControlEvidenceLevel::Prepared),
            ));
        }
        if let Some(fault) = &projected_fault {
            events.push(self.event_from_snapshot(
                QgsQncControlEventKind::CommandRejected,
                Some(request.command_id.clone()),
                &snapshot,
                Some(fault.clone()),
                Some(QgsQncControlEvidenceLevel::ControlSurfaceEvidence),
            ));
            events.push(self.event_from_snapshot(
                QgsQncControlEventKind::FaultRecorded,
                Some(request.command_id.clone()),
                &snapshot,
                Some(fault.clone()),
                Some(QgsQncControlEvidenceLevel::ControlSurfaceEvidence),
            ));
            events.push(self.event_from_snapshot(
                QgsQncControlEventKind::RecoverySuggested,
                Some(request.command_id.clone()),
                &snapshot,
                Some(fault.clone()),
                Some(QgsQncControlEvidenceLevel::ControlSurfaceEvidence),
            ));
        }
        let private_path_exposed = request.exposes_private_path()
            || snapshot.exposes_private_path()
            || events
                .iter()
                .any(QgsQncControlEventEnvelope::exposes_private_path)
            || projected_fault
                .as_ref()
                .is_some_and(QgsQncFaultRecord::exposes_private_path);
        QgsQncCommandReplyEnvelope {
            command_id: request.command_id,
            accepted,
            generation_before,
            generation_after,
            status_before,
            status_after: snapshot.status,
            state_mutated,
            rejection_reason: execution.reason,
            fault: projected_fault,
            recovery_action,
            snapshot,
            events,
            private_path_exposed,
        }
    }

    fn execute_control_command(
        &mut self,
        request: &QgsQncCommandRequestEnvelope,
    ) -> QgsQncControlExecution {
        match request.command {
            QgsQncCommandKind::LoadPreparedInput => self.execute_single(
                QgsBroadcastPlayerCommand::LoadPreparedInput,
                QgsQncControlEventKind::SourceLoaded,
            ),
            QgsQncCommandKind::Prepare => self.execute_single(
                QgsBroadcastPlayerCommand::Prepare {
                    active_range: Some((0, self.active_frame_count)),
                },
                QgsQncControlEventKind::PreparedInputAccepted,
            ),
            QgsQncCommandKind::Cue => match request.payload {
                QgsQncCommandPayload::Frame { frame } => self.execute_single(
                    QgsBroadcastPlayerCommand::Cue { frame },
                    QgsQncControlEventKind::Cued,
                ),
                _ => QgsQncControlExecution::rejected("cue requires frame payload"),
            },
            QgsQncCommandKind::Preroll => {
                let mut execution = QgsQncControlExecution::default();
                if self.runtime.snapshot().operational_status == QgsBroadcastPlayerStatus::Paused {
                    let prepare = self.runtime.execute(QgsBroadcastPlayerCommand::Prepare {
                        active_range: Some((0, self.active_frame_count)),
                    });
                    if !prepare.accepted {
                        return QgsQncControlExecution::from_result(
                            prepare,
                            QgsQncControlEventKind::CommandRejected,
                        );
                    }
                }
                let target = match request.payload {
                    QgsQncCommandPayload::Preroll { target_frame } => target_frame.unwrap_or(0),
                    _ => 0,
                };
                let tick = self.runtime.execute(QgsBroadcastPlayerCommand::Tick {
                    carrier_frame: target,
                });
                execution.accepted = tick.accepted;
                execution.reason = tick.reason;
                execution.event_kinds.push(if tick.accepted {
                    QgsQncControlEventKind::PrerollReady
                } else {
                    QgsQncControlEventKind::CommandRejected
                });
                execution
            }
            QgsQncCommandKind::Play => {
                let mut execution = QgsQncControlExecution::default();
                if self.runtime.snapshot().operational_status != QgsBroadcastPlayerStatus::Playing {
                    let play = self.runtime.execute(QgsBroadcastPlayerCommand::Play);
                    if !play.accepted {
                        return QgsQncControlExecution::from_result(
                            play,
                            QgsQncControlEventKind::CommandRejected,
                        );
                    }
                    execution.event_kinds.push(QgsQncControlEventKind::Started);
                }
                execution.accepted = true;
                let frame_count = match request.payload {
                    QgsQncCommandPayload::Play { frame_count } => frame_count.unwrap_or(0),
                    _ => 0,
                };
                for _ in 0..frame_count {
                    execution.tick_snapshots.push(self.runtime.snapshot());
                    let tick = self
                        .runtime
                        .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 });
                    if !tick.accepted {
                        execution.accepted = false;
                        execution.reason = tick.reason;
                        return execution;
                    }
                }
                execution
            }
            QgsQncCommandKind::Pause => self.execute_single(
                QgsBroadcastPlayerCommand::Pause,
                QgsQncControlEventKind::Paused,
            ),
            QgsQncCommandKind::Seek => match request.payload {
                QgsQncCommandPayload::Frame { frame } => self.execute_single(
                    QgsBroadcastPlayerCommand::Seek { frame },
                    QgsQncControlEventKind::Seeked,
                ),
                _ => QgsQncControlExecution::rejected("seek requires frame payload"),
            },
            QgsQncCommandKind::Stop => self.execute_single(
                QgsBroadcastPlayerCommand::Stop,
                QgsQncControlEventKind::Stopped,
            ),
            QgsQncCommandKind::Unload => self.execute_single(
                QgsBroadcastPlayerCommand::Unload,
                QgsQncControlEventKind::Unloaded,
            ),
            QgsQncCommandKind::Snapshot => unreachable!("snapshot handled before execution"),
        }
    }

    fn execute_single(
        &mut self,
        command: QgsBroadcastPlayerCommand,
        kind: QgsQncControlEventKind,
    ) -> QgsQncControlExecution {
        let result = self.runtime.execute(command);
        QgsQncControlExecution::from_result(result, kind)
    }

    const fn fault_kind_for_rejection(
        &self,
        command: QgsQncCommandKind,
    ) -> QgsOperationalFaultKind {
        match command {
            QgsQncCommandKind::Play => QgsOperationalFaultKind::PrepareRequired,
            QgsQncCommandKind::Cue => QgsOperationalFaultKind::CueOutsideActiveRange,
            QgsQncCommandKind::Seek => QgsOperationalFaultKind::SeekOutsideActiveRange,
            QgsQncCommandKind::LoadPreparedInput
            | QgsQncCommandKind::Prepare
            | QgsQncCommandKind::Preroll
            | QgsQncCommandKind::Pause
            | QgsQncCommandKind::Stop
            | QgsQncCommandKind::Unload
            | QgsQncCommandKind::Snapshot => QgsOperationalFaultKind::IllegalCommandForState,
        }
    }

    fn event_from_snapshot(
        &mut self,
        event_kind: QgsQncControlEventKind,
        command_id: Option<QgsQncCommandId>,
        snapshot: &QgsQncRuntimeSnapshot,
        fault: Option<QgsQncFaultRecord>,
        evidence_level: Option<QgsQncControlEvidenceLevel>,
    ) -> QgsQncControlEventEnvelope {
        let sequence = self.next_event_sequence;
        self.next_event_sequence = self.next_event_sequence.saturating_add(1);
        QgsQncControlEventEnvelope {
            sequence,
            generation: self.generation,
            event_kind,
            command_id,
            status: Some(snapshot.status),
            frame: snapshot.current_frame,
            audio_sample_range: snapshot.current_audio_sample_range,
            prepared_window: snapshot.prepared_window.clone(),
            recovery_action: fault.as_ref().map(|fault| fault.recovery_action),
            private_path_exposed: snapshot.private_path_exposed
                || fault
                    .as_ref()
                    .is_some_and(QgsQncFaultRecord::exposes_private_path),
            fault,
            evidence_level,
        }
    }

    fn snapshot_from_operational(
        &self,
        runtime: &QgsOperationalRuntimeSnapshot,
        active_faults: Vec<QgsQncFaultRecord>,
    ) -> QgsQncRuntimeSnapshot {
        let player = &runtime.player;
        let prepared_window = player
            .prepared_window
            .prepared_start_frame
            .or(player.prepared_window.prepared_end_frame_exclusive)
            .or(player.prepared_window.selected_prepared_frame)
            .map(|_| QgsQncPreparedWindowView {
                start_frame: player.prepared_window.prepared_start_frame,
                end_frame_exclusive: player.prepared_window.prepared_end_frame_exclusive,
                selected_frame: player.prepared_window.selected_prepared_frame,
                prepared_frame_count: player.prepared_window.prepared_frame_count,
            });
        let active_range = player
            .position
            .active_range
            .map(|range| (range.start_frame, range.end_frame));
        let buffer_status = if player.prepared_window.prepared_frame_count > 0 {
            "ready"
        } else {
            "empty"
        };
        let warnings = [
            (!player.device_status.selection.real_display_ready)
                .then_some("real display backend not implemented"),
            (!player.device_status.selection.real_audio_backend_ready)
                .then_some("production audio backend not verified"),
            (!player.device_status.visual_verified).then_some("visual verification not claimed"),
            (!player.device_status.realtime_verified)
                .then_some("realtime verification not claimed"),
            (!player.device_status.av_sync_verified).then_some("A/V sync verification not claimed"),
        ]
        .into_iter()
        .flatten()
        .collect();
        let private_path_exposed = player.exposes_private_path()
            || active_faults
                .iter()
                .any(QgsQncFaultRecord::exposes_private_path);
        QgsQncRuntimeSnapshot {
            generation: self.generation,
            status: runtime.operational_status,
            source_loaded: player.readiness.source_loaded,
            public_source_uri: player.public_source_uri.clone(),
            source_mode: Some(player.source_mode),
            picture_representation: Some(player.selected_representation),
            authoritative_audio_source: "original MXF",
            proxy_aac_authoritative: player.readiness.proxy_aac_authoritative,
            broadcast_audio_model: if player.readiness.discrete_mono_audio {
                "discrete original MXF mono lanes"
            } else {
                "unknown"
            },
            active_range,
            current_frame: runtime.current_frame,
            current_audio_sample_range: runtime.current_audio_sample_range,
            media_time: None,
            prepared_window,
            video_payload_ready: player.readiness.video_payload_ready,
            audio_payload_ready: player.readiness.audio_payload_ready,
            buffer_status,
            device_status: QgsQncControlDeviceStatus::from_player(&player.device_status),
            active_faults,
            warnings,
            private_path_exposed,
        }
    }
}

#[derive(Default)]
struct QgsQncControlExecution {
    accepted: bool,
    reason: Option<&'static str>,
    event_kinds: Vec<QgsQncControlEventKind>,
    tick_snapshots: Vec<QgsOperationalRuntimeSnapshot>,
}

impl QgsQncControlExecution {
    fn rejected(reason: &'static str) -> Self {
        Self {
            accepted: false,
            reason: Some(reason),
            event_kinds: vec![QgsQncControlEventKind::CommandRejected],
            tick_snapshots: Vec::new(),
        }
    }

    fn from_result(result: QgsOperationalTickResult, success_kind: QgsQncControlEventKind) -> Self {
        Self {
            accepted: result.accepted,
            reason: result.reason,
            event_kinds: vec![if result.accepted {
                success_kind
            } else {
                QgsQncControlEventKind::CommandRejected
            }],
            tick_snapshots: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsOperationalRuntimeConfig {
    pub tick_frame_step: u64,
    pub low_water_prepared_frames: usize,
}

impl Default for QgsOperationalRuntimeConfig {
    fn default() -> Self {
        Self {
            tick_frame_step: 1,
            low_water_prepared_frames: 3,
        }
    }
}

impl QgsOperationalRuntimeConfig {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        if self.tick_frame_step == 0 {
            Err(PlaybackError::InvalidCapacity)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum QgsOperationalFaultKind {
    BackendNotImplemented,
    RealDisplayUnavailable,
    AudioOutputNotProductionVerified,
    VisualVerificationUnavailable,
    RealtimeVerificationUnavailable,
    AvSyncNotVerified,
    PreparedWindowUnderrun,
    PreparedWindowEmpty,
    IllegalCommandForState,
    SourceNotLoaded,
    SourceAlreadyLoaded,
    StaleSourceHandle,
    StaleGeneration,
    CueOutsideActiveRange,
    SeekOutsideActiveRange,
    PrepareRequired,
    PrepareFailed,
    EndOfRangeReached,
    PrivatePathExposureBlocked,
    UnsupportedQncOsBackend,
    ProxyAudioNotAuthoritative,
}

impl QgsOperationalFaultKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::BackendNotImplemented => "BackendNotImplemented",
            Self::RealDisplayUnavailable => "RealDisplayUnavailable",
            Self::AudioOutputNotProductionVerified => "AudioOutputNotProductionVerified",
            Self::VisualVerificationUnavailable => "VisualVerificationUnavailable",
            Self::RealtimeVerificationUnavailable => "RealtimeVerificationUnavailable",
            Self::AvSyncNotVerified => "AvSyncNotVerified",
            Self::PreparedWindowUnderrun => "PreparedWindowUnderrun",
            Self::PreparedWindowEmpty => "PreparedWindowEmpty",
            Self::IllegalCommandForState => "IllegalCommandForState",
            Self::SourceNotLoaded => "SourceNotLoaded",
            Self::SourceAlreadyLoaded => "SourceAlreadyLoaded",
            Self::StaleSourceHandle => "StaleSourceHandle",
            Self::StaleGeneration => "StaleGeneration",
            Self::CueOutsideActiveRange => "CueOutsideActiveRange",
            Self::SeekOutsideActiveRange => "SeekOutsideActiveRange",
            Self::PrepareRequired => "PrepareRequired",
            Self::PrepareFailed => "PrepareFailed",
            Self::EndOfRangeReached => "EndOfRangeReached",
            Self::PrivatePathExposureBlocked => "PrivatePathExposureBlocked",
            Self::UnsupportedQncOsBackend => "UnsupportedQncOsBackend",
            Self::ProxyAudioNotAuthoritative => "ProxyAudioNotAuthoritative",
        }
    }

    pub const fn policy(self) -> QgsBroadcastPlayerRecoveryPolicy {
        use QgsBroadcastPlayerFaultScope as Scope;
        use QgsBroadcastPlayerFaultSeverity as Severity;
        use QgsBroadcastPlayerRecoveryAction as Action;
        match self {
            Self::IllegalCommandForState => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Warning,
                Scope::Command,
                Action::RetryCommandAfterPrepare,
                true,
                false,
                true,
                true,
                false,
                false,
            ),
            Self::SourceNotLoaded => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Recoverable,
                Scope::Source,
                Action::UnloadAndReload,
                true,
                false,
                true,
                false,
                true,
                false,
            ),
            Self::SourceAlreadyLoaded => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Warning,
                Scope::Source,
                Action::NoActionRequired,
                true,
                false,
                false,
                true,
                false,
                false,
            ),
            Self::StaleSourceHandle | Self::StaleGeneration => {
                QgsBroadcastPlayerRecoveryPolicy::new(
                    Severity::Recoverable,
                    Scope::Session,
                    Action::UnloadAndReload,
                    true,
                    false,
                    true,
                    true,
                    true,
                    false,
                )
            }
            Self::CueOutsideActiveRange => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Recoverable,
                Scope::Timing,
                Action::ReCue,
                true,
                false,
                true,
                true,
                false,
                false,
            ),
            Self::SeekOutsideActiveRange => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Recoverable,
                Scope::Timing,
                Action::SeekToValidRange,
                true,
                false,
                true,
                true,
                false,
                false,
            ),
            Self::PrepareRequired => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Recoverable,
                Scope::Prepare,
                Action::PrepareAgain,
                true,
                false,
                true,
                true,
                false,
                false,
            ),
            Self::PrepareFailed => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Recoverable,
                Scope::Prepare,
                Action::PrepareAgain,
                true,
                false,
                true,
                true,
                true,
                false,
            ),
            Self::PreparedWindowUnderrun | Self::PreparedWindowEmpty => {
                QgsBroadcastPlayerRecoveryPolicy::new(
                    Severity::Recoverable,
                    Scope::Buffer,
                    Action::PrepareAgain,
                    true,
                    false,
                    true,
                    true,
                    true,
                    false,
                )
            }
            Self::EndOfRangeReached => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Info,
                Scope::Playback,
                Action::NoActionRequired,
                true,
                true,
                false,
                true,
                false,
                false,
            ),
            Self::BackendNotImplemented => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Warning,
                Scope::Device,
                Action::WaitForBackendImplementation,
                true,
                false,
                false,
                true,
                false,
                false,
            ),
            Self::RealDisplayUnavailable
            | Self::VisualVerificationUnavailable
            | Self::RealtimeVerificationUnavailable
            | Self::AvSyncNotVerified => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Warning,
                Scope::Device,
                Action::UseDiagnosticBackendOnly,
                true,
                false,
                false,
                true,
                false,
                false,
            ),
            Self::AudioOutputNotProductionVerified => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Warning,
                Scope::Device,
                Action::UseDiagnosticBackendOnly,
                true,
                false,
                false,
                true,
                false,
                false,
            ),
            Self::PrivatePathExposureBlocked => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Recoverable,
                Scope::Session,
                Action::NoActionRequired,
                true,
                false,
                true,
                true,
                false,
                false,
            ),
            Self::UnsupportedQncOsBackend => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Warning,
                Scope::Device,
                Action::SelectDifferentDevicePolicy,
                true,
                false,
                true,
                true,
                false,
                false,
            ),
            Self::ProxyAudioNotAuthoritative => QgsBroadcastPlayerRecoveryPolicy::new(
                Severity::Fatal,
                Scope::Source,
                Action::FatalRequiresNewSession,
                false,
                false,
                false,
                false,
                true,
                true,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsBroadcastPlayerFaultSeverity {
    Info,
    Warning,
    Recoverable,
    Fatal,
}

impl QgsBroadcastPlayerFaultSeverity {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Warning => "Warning",
            Self::Recoverable => "Recoverable",
            Self::Fatal => "Fatal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsBroadcastPlayerFaultScope {
    Command,
    Source,
    Prepare,
    Playback,
    Buffer,
    Device,
    Timing,
    Session,
}

impl QgsBroadcastPlayerFaultScope {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Source => "Source",
            Self::Prepare => "Prepare",
            Self::Playback => "Playback",
            Self::Buffer => "Buffer",
            Self::Device => "Device",
            Self::Timing => "Timing",
            Self::Session => "Session",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsBroadcastPlayerRecoveryAction {
    NoActionRequired,
    RetryCommandAfterPrepare,
    PrepareAgain,
    ReCue,
    SeekToValidRange,
    StopThenUnload,
    UnloadAndReload,
    SelectDifferentDevicePolicy,
    UseDiagnosticBackendOnly,
    WaitForBackendImplementation,
    FatalRequiresNewSession,
}

impl QgsBroadcastPlayerRecoveryAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::NoActionRequired => "NoActionRequired",
            Self::RetryCommandAfterPrepare => "RetryCommandAfterPrepare",
            Self::PrepareAgain => "PrepareAgain",
            Self::ReCue => "ReCue",
            Self::SeekToValidRange => "SeekToValidRange",
            Self::StopThenUnload => "StopThenUnload",
            Self::UnloadAndReload => "UnloadAndReload",
            Self::SelectDifferentDevicePolicy => "SelectDifferentDevicePolicy",
            Self::UseDiagnosticBackendOnly => "UseDiagnosticBackendOnly",
            Self::WaitForBackendImplementation => "WaitForBackendImplementation",
            Self::FatalRequiresNewSession => "FatalRequiresNewSession",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsBroadcastPlayerRecoveryStatus {
    Suggested,
    NotRequired,
    NotAttempted,
    FailedRequiresNewSession,
}

impl QgsBroadcastPlayerRecoveryStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Suggested => "Suggested",
            Self::NotRequired => "NotRequired",
            Self::NotAttempted => "NotAttempted",
            Self::FailedRequiresNewSession => "FailedRequiresNewSession",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerRecoveryPolicy {
    pub severity: QgsBroadcastPlayerFaultSeverity,
    pub scope: QgsBroadcastPlayerFaultScope,
    pub action: QgsBroadcastPlayerRecoveryAction,
    pub recoverable: bool,
    pub state_mutates: bool,
    pub qnc_may_retry: bool,
    pub source_remains_loaded: bool,
    pub prepared_window_invalidated: bool,
    pub moves_to_failed: bool,
}

impl QgsBroadcastPlayerRecoveryPolicy {
    pub const fn new(
        severity: QgsBroadcastPlayerFaultSeverity,
        scope: QgsBroadcastPlayerFaultScope,
        action: QgsBroadcastPlayerRecoveryAction,
        recoverable: bool,
        state_mutates: bool,
        qnc_may_retry: bool,
        source_remains_loaded: bool,
        prepared_window_invalidated: bool,
        moves_to_failed: bool,
    ) -> Self {
        Self {
            severity,
            scope,
            action,
            recoverable,
            state_mutates,
            qnc_may_retry,
            source_remains_loaded,
            prepared_window_invalidated,
            moves_to_failed,
        }
    }

    pub const fn status(self) -> QgsBroadcastPlayerRecoveryStatus {
        if matches!(
            self.action,
            QgsBroadcastPlayerRecoveryAction::FatalRequiresNewSession
        ) {
            QgsBroadcastPlayerRecoveryStatus::FailedRequiresNewSession
        } else if matches!(
            self.action,
            QgsBroadcastPlayerRecoveryAction::NoActionRequired
        ) {
            QgsBroadcastPlayerRecoveryStatus::NotRequired
        } else {
            QgsBroadcastPlayerRecoveryStatus::Suggested
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsOperationalFault {
    pub kind: QgsOperationalFaultKind,
    pub severity: QgsBroadcastPlayerFaultSeverity,
    pub scope: QgsBroadcastPlayerFaultScope,
    pub recoverable: bool,
    pub recovery_action: QgsBroadcastPlayerRecoveryAction,
    pub recovery_status: QgsBroadcastPlayerRecoveryStatus,
    pub qnc_safe_code: &'static str,
    pub reason: &'static str,
}

impl QgsOperationalFault {
    pub fn new(kind: QgsOperationalFaultKind, reason: &'static str) -> Self {
        let policy = kind.policy();
        Self {
            kind,
            severity: policy.severity,
            scope: policy.scope,
            recoverable: policy.recoverable,
            recovery_action: policy.action,
            recovery_status: policy.status(),
            qnc_safe_code: kind.label(),
            reason,
        }
    }

    pub fn exposes_private_path(&self) -> bool {
        qgs_projection_text_exposes_private_path(self.reason)
    }
}

pub type QgsBroadcastPlayerFault = QgsOperationalFault;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerFaultCounter {
    pub kind: QgsOperationalFaultKind,
    pub count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerFaultSnapshot {
    pub active_faults: Vec<QgsOperationalFault>,
    pub last_fault: Option<QgsOperationalFault>,
    pub fault_counters: Vec<QgsBroadcastPlayerFaultCounter>,
    pub recoverable_fault_count: usize,
    pub fatal_fault_count: usize,
    pub recommended_recovery_action: Option<QgsBroadcastPlayerRecoveryAction>,
    pub backend_device_warnings: Vec<QgsOperationalFault>,
    pub private_path_exposed: bool,
}

impl QgsBroadcastPlayerFaultSnapshot {
    pub fn from_faults(faults: Vec<QgsOperationalFault>) -> Self {
        let mut counters = Vec::<QgsBroadcastPlayerFaultCounter>::new();
        for fault in &faults {
            if let Some(counter) = counters
                .iter_mut()
                .find(|counter| counter.kind == fault.kind)
            {
                counter.count = counter.count.saturating_add(1);
            } else {
                counters.push(QgsBroadcastPlayerFaultCounter {
                    kind: fault.kind,
                    count: 1,
                });
            }
        }
        counters.sort_by_key(|counter| counter.kind);
        let recoverable_fault_count = faults.iter().filter(|fault| fault.recoverable).count();
        let fatal_fault_count = faults
            .iter()
            .filter(|fault| fault.severity == QgsBroadcastPlayerFaultSeverity::Fatal)
            .count();
        let last_fault = faults.last().cloned();
        let recommended_recovery_action = last_fault.as_ref().map(|fault| fault.recovery_action);
        let backend_device_warnings = faults
            .iter()
            .filter(|fault| fault.scope == QgsBroadcastPlayerFaultScope::Device)
            .cloned()
            .collect();
        let private_path_exposed = faults.iter().any(QgsOperationalFault::exposes_private_path);
        Self {
            active_faults: faults,
            last_fault,
            fault_counters: counters,
            recoverable_fault_count,
            fatal_fault_count,
            recommended_recovery_action,
            backend_device_warnings,
            private_path_exposed,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsOperationalBufferHealth {
    pub prepared_start_frame: Option<u64>,
    pub prepared_end_frame_exclusive: Option<u64>,
    pub selected_prepared_frame: Option<u64>,
    pub prepared_frame_count: usize,
    pub low_water: bool,
    pub underrun: bool,
    pub discarded_count: usize,
    pub last_prepared_frame: Option<u64>,
    pub preparation_pending: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsOperationalRuntimeSnapshot {
    pub player: QgsBroadcastPlayerSnapshot,
    pub operational_status: QgsBroadcastPlayerStatus,
    pub current_frame: Option<u64>,
    pub current_audio_sample_range: Option<(u64, u64)>,
    pub buffer_health: QgsOperationalBufferHealth,
    pub faults: Vec<QgsOperationalFault>,
    pub fault_snapshot: QgsBroadcastPlayerFaultSnapshot,
    pub accepted_commands: usize,
    pub rejected_commands: usize,
    pub completed: bool,
    pub private_path_exposed: bool,
}

impl QgsOperationalRuntimeSnapshot {
    pub fn exposes_private_path(&self) -> bool {
        self.private_path_exposed
            || self.player.exposes_private_path()
            || self
                .faults
                .iter()
                .any(QgsOperationalFault::exposes_private_path)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsOperationalRuntimeEventKind {
    CommandAccepted,
    CommandRejected,
    PositionAdvanced,
    Seeked,
    Stopped,
    Completed,
    Unloaded,
    WarningRaised,
    PlayerCommandRejected,
    PlayerFaultRecorded,
    PlayerRecoverySuggested,
    PlayerEnteredFailed,
    PlayerRecovered,
    PlayerWarningRecorded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsOperationalRuntimeEvent {
    pub sequence: u64,
    pub kind: QgsOperationalRuntimeEventKind,
    pub status: QgsBroadcastPlayerStatus,
    pub frame: Option<u64>,
    pub summary: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsOperationalTickResult {
    pub command: QgsBroadcastPlayerCommand,
    pub accepted: bool,
    pub reason: Option<&'static str>,
    pub command_result: Option<QgsBroadcastPlayerCommandResult>,
    pub events: Vec<QgsOperationalRuntimeEvent>,
    pub snapshot: QgsOperationalRuntimeSnapshot,
}

pub struct QgsBroadcastPlayerOperationalRuntime {
    core: QgsBroadcastPlayerCore,
    config: QgsOperationalRuntimeConfig,
    current_frame: Option<u64>,
    completed: bool,
    faults: Vec<QgsOperationalFault>,
    events: Vec<QgsOperationalRuntimeEvent>,
    accepted_commands: usize,
    rejected_commands: usize,
}

impl QgsBroadcastPlayerOperationalRuntime {
    pub fn new(
        plan: QgsInputPlan,
        config: QgsOperationalRuntimeConfig,
    ) -> Result<Self, PlaybackError> {
        Ok(Self {
            core: QgsBroadcastPlayerCore::new(plan)?,
            config: config.validate()?,
            current_frame: None,
            completed: false,
            faults: Vec::new(),
            events: Vec::new(),
            accepted_commands: 0,
            rejected_commands: 0,
        })
    }

    pub fn execute(&mut self, command: QgsBroadcastPlayerCommand) -> QgsOperationalTickResult {
        let before_events = self.events.len();
        let legality = self.command_legality(&command);
        if let Err(reason) = legality {
            self.rejected_commands = self.rejected_commands.saturating_add(1);
            self.push_fault(self.rejected_fault_kind(&command, Some(reason)), reason);
            self.push_event(
                QgsOperationalRuntimeEventKind::CommandRejected,
                self.operational_status(),
                self.current_frame,
                reason,
            );
            return QgsOperationalTickResult {
                command,
                accepted: false,
                reason: Some(reason),
                command_result: None,
                events: self.events[before_events..].to_vec(),
                snapshot: self.snapshot(),
            };
        }

        let operational_command = self.operational_command(command.clone());
        let result = self.core.execute(operational_command.clone());
        if result.accepted {
            self.accepted_commands = self.accepted_commands.saturating_add(1);
            self.apply_accepted_command(&operational_command);
            self.push_event_for_accepted_command(&operational_command);
        } else {
            self.rejected_commands = self.rejected_commands.saturating_add(1);
            self.push_fault(
                self.rejected_fault_kind(&operational_command, result.reason),
                result.reason.unwrap_or("command rejected"),
            );
            self.push_event(
                QgsOperationalRuntimeEventKind::CommandRejected,
                self.operational_status(),
                self.current_frame,
                result.reason.unwrap_or("command rejected"),
            );
        }
        QgsOperationalTickResult {
            command,
            accepted: result.accepted,
            reason: result.reason,
            command_result: Some(result),
            events: self.events[before_events..].to_vec(),
            snapshot: self.snapshot(),
        }
    }

    pub fn snapshot(&self) -> QgsOperationalRuntimeSnapshot {
        let player = self.core.snapshot();
        let current_frame = self.current_frame.or(player.position.current_frame);
        let current_audio_sample_range =
            current_frame.and_then(|frame| self.frame_audio_sample_range(frame, &player));
        let buffer_health = self.buffer_health(&player, current_frame);
        let mut faults = self.state_faults(&player, &buffer_health);
        faults.extend(self.faults.clone());
        let fault_snapshot = QgsBroadcastPlayerFaultSnapshot::from_faults(faults.clone());
        let private_path_exposed =
            player.exposes_private_path() || fault_snapshot.private_path_exposed;
        QgsOperationalRuntimeSnapshot {
            player,
            operational_status: self.operational_status(),
            current_frame,
            current_audio_sample_range,
            buffer_health,
            faults,
            fault_snapshot,
            accepted_commands: self.accepted_commands,
            rejected_commands: self.rejected_commands,
            completed: self.completed,
            private_path_exposed,
        }
    }

    pub fn events(&self) -> &[QgsOperationalRuntimeEvent] {
        &self.events
    }

    fn command_legality(&self, command: &QgsBroadcastPlayerCommand) -> Result<(), &'static str> {
        let status = self.operational_status();
        match status {
            QgsBroadcastPlayerStatus::Empty => match command {
                QgsBroadcastPlayerCommand::LoadPreparedInput => Ok(()),
                _ => Err("command is illegal while player is Empty"),
            },
            QgsBroadcastPlayerStatus::Loaded | QgsBroadcastPlayerStatus::Preparing => match command
            {
                QgsBroadcastPlayerCommand::Prepare { .. }
                | QgsBroadcastPlayerCommand::Cue { .. }
                | QgsBroadcastPlayerCommand::Stop
                | QgsBroadcastPlayerCommand::Unload
                | QgsBroadcastPlayerCommand::Tick { .. } => Ok(()),
                QgsBroadcastPlayerCommand::Play => Err("play requires Ready"),
                QgsBroadcastPlayerCommand::LoadPreparedInput => Err("source already loaded"),
                QgsBroadcastPlayerCommand::Pause => Err("pause requires Playing"),
                QgsBroadcastPlayerCommand::Seek { .. } => Err("seek requires active range"),
            },
            QgsBroadcastPlayerStatus::Ready => match command {
                QgsBroadcastPlayerCommand::Play
                | QgsBroadcastPlayerCommand::Seek { .. }
                | QgsBroadcastPlayerCommand::Stop
                | QgsBroadcastPlayerCommand::Unload
                | QgsBroadcastPlayerCommand::Tick { .. }
                | QgsBroadcastPlayerCommand::Prepare { .. }
                | QgsBroadcastPlayerCommand::Cue { .. } => Ok(()),
                QgsBroadcastPlayerCommand::LoadPreparedInput => Err("source already loaded"),
                QgsBroadcastPlayerCommand::Pause => Err("pause requires Playing"),
            },
            QgsBroadcastPlayerStatus::Playing => match command {
                QgsBroadcastPlayerCommand::Tick { .. }
                | QgsBroadcastPlayerCommand::Pause
                | QgsBroadcastPlayerCommand::Stop
                | QgsBroadcastPlayerCommand::Seek { .. } => Ok(()),
                QgsBroadcastPlayerCommand::LoadPreparedInput => Err("source already loaded"),
                QgsBroadcastPlayerCommand::Play => Err("already playing"),
                QgsBroadcastPlayerCommand::Prepare { .. }
                | QgsBroadcastPlayerCommand::Cue { .. } => {
                    Err("prepare/cue requires non-playing state")
                }
                QgsBroadcastPlayerCommand::Unload => Err("unload requires stopped or paused state"),
            },
            QgsBroadcastPlayerStatus::Paused => match command {
                QgsBroadcastPlayerCommand::Play
                | QgsBroadcastPlayerCommand::Seek { .. }
                | QgsBroadcastPlayerCommand::Stop
                | QgsBroadcastPlayerCommand::Unload
                | QgsBroadcastPlayerCommand::Tick { .. }
                | QgsBroadcastPlayerCommand::Prepare { .. }
                | QgsBroadcastPlayerCommand::Cue { .. } => Ok(()),
                QgsBroadcastPlayerCommand::Pause => Err("already paused"),
                QgsBroadcastPlayerCommand::LoadPreparedInput => Err("source already loaded"),
            },
            QgsBroadcastPlayerStatus::Stopped => match command {
                QgsBroadcastPlayerCommand::Cue { .. }
                | QgsBroadcastPlayerCommand::Seek { .. }
                | QgsBroadcastPlayerCommand::Prepare { .. }
                | QgsBroadcastPlayerCommand::Unload
                | QgsBroadcastPlayerCommand::Tick { .. } => Ok(()),
                QgsBroadcastPlayerCommand::Pause => Err("pause requires Playing"),
                QgsBroadcastPlayerCommand::Play => Err("play requires Ready"),
                QgsBroadcastPlayerCommand::LoadPreparedInput => Err("source already loaded"),
                QgsBroadcastPlayerCommand::Stop => Err("already stopped"),
            },
            QgsBroadcastPlayerStatus::Completed => match command {
                QgsBroadcastPlayerCommand::Seek { .. }
                | QgsBroadcastPlayerCommand::Cue { .. }
                | QgsBroadcastPlayerCommand::Prepare { .. }
                | QgsBroadcastPlayerCommand::Unload => Ok(()),
                QgsBroadcastPlayerCommand::Play => {
                    Err("completed range requires seek/prepare before play")
                }
                QgsBroadcastPlayerCommand::Tick { .. } => Err("completed range cannot tick"),
                QgsBroadcastPlayerCommand::LoadPreparedInput => Err("source already loaded"),
                QgsBroadcastPlayerCommand::Pause => Err("pause requires Playing"),
                QgsBroadcastPlayerCommand::Stop => Err("already completed"),
            },
            QgsBroadcastPlayerStatus::Failed => match command {
                QgsBroadcastPlayerCommand::Unload => Ok(()),
                _ => Err("failed runtime requires unload"),
            },
        }
    }

    fn rejected_fault_kind(
        &self,
        command: &QgsBroadcastPlayerCommand,
        reason: Option<&'static str>,
    ) -> QgsOperationalFaultKind {
        match command {
            QgsBroadcastPlayerCommand::Play
                if self.operational_status() == QgsBroadcastPlayerStatus::Empty =>
            {
                QgsOperationalFaultKind::SourceNotLoaded
            }
            QgsBroadcastPlayerCommand::Play if reason == Some("play requires Ready") => {
                QgsOperationalFaultKind::PrepareRequired
            }
            QgsBroadcastPlayerCommand::LoadPreparedInput
                if reason == Some("source already loaded") =>
            {
                QgsOperationalFaultKind::SourceAlreadyLoaded
            }
            QgsBroadcastPlayerCommand::Cue { .. } => QgsOperationalFaultKind::CueOutsideActiveRange,
            QgsBroadcastPlayerCommand::Seek { .. } => {
                QgsOperationalFaultKind::SeekOutsideActiveRange
            }
            QgsBroadcastPlayerCommand::Prepare { .. } => QgsOperationalFaultKind::PrepareFailed,
            _ => QgsOperationalFaultKind::IllegalCommandForState,
        }
    }

    fn operational_command(&self, command: QgsBroadcastPlayerCommand) -> QgsBroadcastPlayerCommand {
        match command {
            QgsBroadcastPlayerCommand::Tick { .. } => QgsBroadcastPlayerCommand::Tick {
                carrier_frame: self.current_frame.unwrap_or(0),
            },
            other => other,
        }
    }

    fn apply_accepted_command(&mut self, command: &QgsBroadcastPlayerCommand) {
        match *command {
            QgsBroadcastPlayerCommand::LoadPreparedInput => {
                self.completed = false;
            }
            QgsBroadcastPlayerCommand::Prepare { .. } => {
                self.completed = false;
            }
            QgsBroadcastPlayerCommand::Cue { frame }
            | QgsBroadcastPlayerCommand::Seek { frame } => {
                self.completed = false;
                self.current_frame = Some(frame);
            }
            QgsBroadcastPlayerCommand::Play => {
                self.completed = false;
            }
            QgsBroadcastPlayerCommand::Tick { .. } => {
                if self.core.snapshot().status == QgsBroadcastPlayerStatus::Playing {
                    self.advance_playing_position();
                }
            }
            QgsBroadcastPlayerCommand::Stop => {}
            QgsBroadcastPlayerCommand::Pause => {}
            QgsBroadcastPlayerCommand::Unload => {
                self.completed = false;
                self.current_frame = None;
            }
        }
    }

    fn advance_playing_position(&mut self) {
        let snapshot = self.core.snapshot();
        let Some(range) = snapshot.position.active_range else {
            return;
        };
        let current = self
            .current_frame
            .or(snapshot.position.current_frame)
            .unwrap_or(range.start_frame);
        let next = current.saturating_add(self.config.tick_frame_step);
        if next >= range.end_frame {
            self.current_frame = Some(range.end_frame.saturating_sub(1));
            self.completed = true;
            self.push_fault(
                QgsOperationalFaultKind::EndOfRangeReached,
                "active range end reached",
            );
            self.push_event(
                QgsOperationalRuntimeEventKind::Completed,
                QgsBroadcastPlayerStatus::Completed,
                self.current_frame,
                "active range end reached",
            );
        } else {
            self.current_frame = Some(next);
            self.push_event(
                QgsOperationalRuntimeEventKind::PositionAdvanced,
                QgsBroadcastPlayerStatus::Playing,
                self.current_frame,
                "logical frame advanced",
            );
        }
    }

    fn push_event_for_accepted_command(&mut self, command: &QgsBroadcastPlayerCommand) {
        let (kind, summary) = match command {
            QgsBroadcastPlayerCommand::Seek { .. } => {
                (QgsOperationalRuntimeEventKind::Seeked, "seek accepted")
            }
            QgsBroadcastPlayerCommand::Stop => {
                (QgsOperationalRuntimeEventKind::Stopped, "stop accepted")
            }
            QgsBroadcastPlayerCommand::Unload => {
                (QgsOperationalRuntimeEventKind::Unloaded, "unload accepted")
            }
            _ => (
                QgsOperationalRuntimeEventKind::CommandAccepted,
                "command accepted",
            ),
        };
        self.push_event(kind, self.operational_status(), self.current_frame, summary);
    }

    fn operational_status(&self) -> QgsBroadcastPlayerStatus {
        if self.completed {
            QgsBroadcastPlayerStatus::Completed
        } else {
            self.core.snapshot().status
        }
    }

    fn frame_audio_sample_range(
        &self,
        frame: u64,
        player: &QgsBroadcastPlayerSnapshot,
    ) -> Option<(u64, u64)> {
        player
            .position
            .frame_rate
            .frame_duration()
            .ok()
            .and_then(|duration| {
                audio_samples_for_duration(duration, player.position.audio_sample_rate).ok()
            })
            .map(|samples_per_frame| {
                let start = frame.saturating_mul(samples_per_frame);
                (start, start.saturating_add(samples_per_frame))
            })
    }

    fn buffer_health(
        &self,
        player: &QgsBroadcastPlayerSnapshot,
        current_frame: Option<u64>,
    ) -> QgsOperationalBufferHealth {
        let prepared_start = player.prepared_window.prepared_start_frame;
        let prepared_end = player.prepared_window.prepared_end_frame_exclusive;
        let prepared_count = player.prepared_window.prepared_frame_count;
        let last_prepared_frame = prepared_end.and_then(|end| end.checked_sub(1));
        let current_prepared = match (current_frame, prepared_start, prepared_end) {
            (Some(frame), Some(start), Some(end)) => frame >= start && frame < end,
            _ => false,
        };
        let source_loaded = player.readiness.source_loaded;
        QgsOperationalBufferHealth {
            prepared_start_frame: prepared_start,
            prepared_end_frame_exclusive: prepared_end,
            selected_prepared_frame: player.prepared_window.selected_prepared_frame,
            prepared_frame_count: prepared_count,
            low_water: prepared_count > 0
                && prepared_count <= self.config.low_water_prepared_frames,
            underrun: source_loaded
                && current_frame.is_some()
                && prepared_count > 0
                && !current_prepared,
            discarded_count: player.prepared_window.latest_discarded_frame_count,
            last_prepared_frame,
            preparation_pending: source_loaded && prepared_count == 0,
        }
    }

    fn state_faults(
        &self,
        player: &QgsBroadcastPlayerSnapshot,
        buffer_health: &QgsOperationalBufferHealth,
    ) -> Vec<QgsOperationalFault> {
        let mut faults = Vec::new();
        if player.device_status.real_display_backend
            == QgsDeviceBackendAvailability::NotImplemented.label()
        {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::BackendNotImplemented,
                "real display backend is NotImplemented",
            ));
        }
        if !player.readiness.real_display_ready {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::RealDisplayUnavailable,
                "real display is unavailable",
            ));
        }
        if !player.readiness.visual_verified {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::VisualVerificationUnavailable,
                "visual verification is unavailable",
            ));
        }
        if !player.readiness.realtime_verified {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::RealtimeVerificationUnavailable,
                "realtime verification is unavailable",
            ));
        }
        if !player.readiness.av_sync_verified {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::AvSyncNotVerified,
                "A/V sync is not verified",
            ));
        }
        if !player.device_status.audio_device_production_verified {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::AudioOutputNotProductionVerified,
                "audio output is not production verified",
            ));
        }
        if player.device_status.selection.selected_video_backend
            == QgsVideoPresenterBackendKind::X11LegacyNonTarget
        {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::UnsupportedQncOsBackend,
                "X11 is legacy/non-target for QNC OS",
            ));
        }
        if player.readiness.proxy_aac_authoritative {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::ProxyAudioNotAuthoritative,
                "proxy AAC cannot be authoritative runtime audio",
            ));
        }
        if buffer_health.preparation_pending {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::PreparedWindowEmpty,
                "prepared window is empty while source is loaded",
            ));
        }
        if buffer_health.underrun {
            faults.push(QgsOperationalFault::new(
                QgsOperationalFaultKind::PreparedWindowUnderrun,
                "current frame is outside prepared window",
            ));
        }
        faults
    }

    fn push_fault(&mut self, kind: QgsOperationalFaultKind, reason: &'static str) {
        let fault = QgsOperationalFault::new(kind, reason);
        if !self.faults.contains(&fault) {
            self.faults.push(fault.clone());
        }
        self.push_event(
            QgsOperationalRuntimeEventKind::PlayerFaultRecorded,
            self.operational_status(),
            self.current_frame,
            reason,
        );
        if fault.recoverable {
            self.push_event(
                QgsOperationalRuntimeEventKind::PlayerRecoverySuggested,
                self.operational_status(),
                self.current_frame,
                fault.recovery_action.label(),
            );
        }
        if fault.severity == QgsBroadcastPlayerFaultSeverity::Fatal {
            self.push_event(
                QgsOperationalRuntimeEventKind::PlayerEnteredFailed,
                QgsBroadcastPlayerStatus::Failed,
                self.current_frame,
                reason,
            );
        }
        self.push_event(
            if fault.scope == QgsBroadcastPlayerFaultScope::Device {
                QgsOperationalRuntimeEventKind::PlayerWarningRecorded
            } else {
                QgsOperationalRuntimeEventKind::WarningRaised
            },
            self.operational_status(),
            self.current_frame,
            reason,
        );
    }

    fn push_event(
        &mut self,
        kind: QgsOperationalRuntimeEventKind,
        status: QgsBroadcastPlayerStatus,
        frame: Option<u64>,
        summary: &'static str,
    ) {
        self.events.push(QgsOperationalRuntimeEvent {
            sequence: u64::try_from(self.events.len() + 1).unwrap_or(u64::MAX),
            kind,
            status,
            frame,
            summary,
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QgsBroadcastPlayerAssemblyModuleKind {
    InputPlan,
    TransportCore,
    FrameClock,
    PrerollManager,
    VideoPayloadProvider,
    AudioPayloadProvider,
    AudioDeviceBoundary,
    PresenterBoundary,
    DeviceBackendSelection,
    FaultRecovery,
    EventSurface,
    SnapshotProjection,
    SessionFacade,
}

impl QgsBroadcastPlayerAssemblyModuleKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::InputPlan => "input_plan",
            Self::TransportCore => "transport",
            Self::FrameClock => "frame_clock",
            Self::PrerollManager => "preroll",
            Self::VideoPayloadProvider => "video_payload",
            Self::AudioPayloadProvider => "audio_payload",
            Self::AudioDeviceBoundary => "audio_device_boundary",
            Self::PresenterBoundary => "presenter_boundary",
            Self::DeviceBackendSelection => "device_selection",
            Self::FaultRecovery => "fault_recovery",
            Self::EventSurface => "event_surface",
            Self::SnapshotProjection => "snapshot_projection",
            Self::SessionFacade => "session_facade",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerAssemblyModule {
    pub kind: QgsBroadcastPlayerAssemblyModuleKind,
    pub owns: &'static str,
    pub boundary: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsBroadcastPlayerAssemblySnapshot {
    pub source_mode: QgsInputPlanSourceMode,
    pub video_source_role: QgsPlaybackRepresentation,
    pub audio_source_role: QgsAudioRepresentation,
    pub public_video_uri: String,
    pub public_audio_uri: String,
    pub original_audio_authoritative: bool,
    pub proxy_aac_authoritative: bool,
    pub discrete_mono_lanes: bool,
    pub private_path_exposed: bool,
    pub device_selection: QgsDeviceBackendSelection,
    pub modules: Vec<QgsBroadcastPlayerAssemblyModule>,
}

impl QgsBroadcastPlayerAssemblySnapshot {
    pub fn module_labels(&self) -> Vec<&'static str> {
        self.modules
            .iter()
            .map(|module| module.kind.label())
            .collect()
    }

    pub fn exposes_private_path(&self) -> bool {
        self.private_path_exposed
            || qgs_projection_text_exposes_private_path(&self.public_video_uri)
            || qgs_projection_text_exposes_private_path(&self.public_audio_uri)
            || self.device_selection.exposes_private_path()
    }
}

/// Composition root for the QGS Broadcast Player Runtime LEGO modules.
///
/// This type wires existing backend-neutral modules together. It deliberately
/// does not implement transport state, frame-clock math, preroll policy,
/// decode, presenter internals, audio-device internals, or QNC UI behavior.
pub struct QgsBroadcastPlayerAssembly {
    plan: QgsInputPlan,
    operational_config: QgsOperationalRuntimeConfig,
    device_policy: QgsDeviceSelectionPolicy,
    modules: Vec<QgsBroadcastPlayerAssemblyModule>,
}

impl QgsBroadcastPlayerAssembly {
    pub fn from_prepared_descriptor(
        descriptor: &QgsPreparedInputDescriptor,
        queue_requirements: QgsInputPlanQueueRequirements,
    ) -> Result<Self, PlaybackError> {
        let plan = QgsInputPlan::from_descriptor(descriptor, queue_requirements)?;
        Self::from_input_plan(plan)
    }

    pub fn from_input_plan(plan: QgsInputPlan) -> Result<Self, PlaybackError> {
        Self::with_config(
            plan,
            QgsOperationalRuntimeConfig::default(),
            QgsDeviceSelectionPolicy::PreviewOnQncOs,
        )
    }

    pub fn with_config(
        plan: QgsInputPlan,
        operational_config: QgsOperationalRuntimeConfig,
        device_policy: QgsDeviceSelectionPolicy,
    ) -> Result<Self, PlaybackError> {
        plan.validate()?;
        operational_config.validate()?;
        let assembly = Self {
            plan,
            operational_config,
            device_policy,
            modules: Self::default_modules(),
        };
        assembly.validate_media_policy()?;
        Ok(assembly)
    }

    pub fn input_plan(&self) -> &QgsInputPlan {
        &self.plan
    }

    pub fn active_frame_count(&self) -> u64 {
        self.plan.video_source.duration_frames
    }

    pub fn module_labels(&self) -> Vec<&'static str> {
        self.modules
            .iter()
            .map(|module| module.kind.label())
            .collect()
    }

    pub fn device_selection(&self) -> QgsDeviceBackendSelection {
        QgsDeviceBackendSelector::select(self.device_policy)
    }

    pub fn new_control_core(&self) -> Result<QgsBroadcastPlayerCore, PlaybackError> {
        QgsBroadcastPlayerCore::new(self.plan.clone())
    }

    pub fn new_operational_runtime(
        &self,
    ) -> Result<QgsBroadcastPlayerOperationalRuntime, PlaybackError> {
        QgsBroadcastPlayerOperationalRuntime::new(self.plan.clone(), self.operational_config)
    }

    pub fn snapshot(&self) -> QgsBroadcastPlayerAssemblySnapshot {
        QgsBroadcastPlayerAssemblySnapshot {
            source_mode: self.plan.source_mode,
            video_source_role: self.plan.video_source.representation,
            audio_source_role: self.plan.audio_source.representation,
            public_video_uri: self.plan.video_source.media_uri.clone(),
            public_audio_uri: self.plan.audio_source.media_uri.clone(),
            original_audio_authoritative: self.plan.audio_source.representation
                == QgsAudioRepresentation::Original,
            proxy_aac_authoritative: false,
            discrete_mono_lanes: self.discrete_mono_lanes(),
            private_path_exposed: false,
            device_selection: self.device_selection(),
            modules: self.modules.clone(),
        }
    }

    fn validate_media_policy(&self) -> Result<(), PlaybackError> {
        if self.plan.audio_source.representation != QgsAudioRepresentation::Original
            || !self.discrete_mono_lanes()
            || self.device_selection().selected_video_backend
                == QgsVideoPresenterBackendKind::X11LegacyNonTarget
            || self.snapshot().exposes_private_path()
        {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        Ok(())
    }

    fn discrete_mono_lanes(&self) -> bool {
        !self.plan.audio_source.lanes.is_empty()
            && self
                .plan
                .audio_source
                .lanes
                .iter()
                .all(|lane| lane.channel_index == 0)
    }

    fn default_modules() -> Vec<QgsBroadcastPlayerAssemblyModule> {
        use QgsBroadcastPlayerAssemblyModuleKind as Kind;
        vec![
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::InputPlan,
                owns: "original/proxy media identity, source mode, public URI identity",
                boundary: "does not own transport state or device output",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::TransportCore,
                owns: "load, prepare, cue, play, pause, seek, stop, unload",
                boundary: "does not open media or build readiness on Play",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::FrameClock,
                owns: "logical frame progression and frame/sample mapping facts",
                boundary: "does not claim realtime scheduling",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::PrerollManager,
                owns: "bounded prepared window and no-play-before-ready facts",
                boundary: "does not own decode or device queues",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::VideoPayloadProvider,
                owns: "proxy preview payload status now and original-media payload status later",
                boundary: "does not present frames to a real display",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::AudioPayloadProvider,
                owns: "authoritative original MXF mono-lane payload facts",
                boundary: "does not use proxy AAC as runtime truth",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::AudioDeviceBoundary,
                owns: "PipeWire prototype evidence and replaceable audio boundary facts",
                boundary: "does not claim production AudioDeviceVerified",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::PresenterBoundary,
                owns: "test/file/readback presenter diagnostics and future presenter boundary",
                boundary: "does not claim real display output",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::DeviceBackendSelection,
                owns: "preview-qnc-os, diagnostic, headless, and future backend policy",
                boundary: "does not implement device backends",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::FaultRecovery,
                owns: "structured runtime faults and recovery suggestions",
                boundary: "does not silently recover",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::EventSurface,
                owns: "QNC-shaped public-safe event facts",
                boundary: "does not expose private paths",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::SnapshotProjection,
                owns: "operator/QNC-readable passive runtime state",
                boundary: "does not mutate runtime behavior",
            },
            QgsBroadcastPlayerAssemblyModule {
                kind: Kind::SessionFacade,
                owns: "clean control surface for qgs-test and future QNC apps",
                boundary: "does not own module internals",
            },
        ]
    }
}

fn qgs_projection_text_exposes_private_path(value: &str) -> bool {
    value.starts_with('/') || value.starts_with("file:") || value.contains("file:")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsTransportSnapshot {
    pub status: QgsTransportStatus,
    pub active_source: Option<QgsTransportSourceHandle>,
    pub active_range: Option<QgsTransportActiveRange>,
    pub cue: Option<QgsTransportCuePoint>,
    pub prepared_anchor: Option<QgsTransportPreparedAnchor>,
    pub play_ready: bool,
    pub loaded_source_count: usize,
    pub no_work_on_play: QgsTransportNoWorkOnPlayCounters,
    pub events: Vec<QgsTransportEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QgsTransportSourceState {
    handle: QgsTransportSourceHandle,
    preloaded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QgsTransportEngine {
    sources: BTreeMap<String, QgsTransportSourceState>,
    active_source_id: Option<String>,
    active_range: Option<QgsTransportActiveRange>,
    cue: Option<QgsTransportCuePoint>,
    prepared_anchor: Option<QgsTransportPreparedAnchor>,
    play_ready: bool,
    next_revision: u64,
    status: QgsTransportStatus,
    no_work_on_play: QgsTransportNoWorkOnPlayCounters,
    events: Vec<QgsTransportEvent>,
}

impl QgsTransportEngine {
    pub fn new() -> Self {
        Self {
            sources: BTreeMap::new(),
            active_source_id: None,
            active_range: None,
            cue: None,
            prepared_anchor: None,
            play_ready: false,
            next_revision: 1,
            status: QgsTransportStatus::Empty,
            no_work_on_play: QgsTransportNoWorkOnPlayCounters::default(),
            events: vec![QgsTransportEvent::TransportEngineCreated],
        }
    }

    pub fn load_source(
        &mut self,
        plan: &QgsInputPlan,
    ) -> Result<QgsTransportSourceHandle, PlaybackError> {
        plan.validate()?;
        let revision = QgsTransportSourceRevision(self.next_revision);
        self.next_revision = self.next_revision.saturating_add(1);
        let handle = QgsTransportSourceHandle::from_plan(plan, revision);
        if handle.exposes_private_path() {
            self.events
                .push(QgsTransportEvent::TransportValidationFailed {
                    reason: "source identity exposes private path",
                });
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        self.sources.insert(
            handle.source_id.clone(),
            QgsTransportSourceState {
                handle: handle.clone(),
                preloaded: false,
            },
        );
        self.status = QgsTransportStatus::Loaded;
        self.play_ready = false;
        self.events.push(QgsTransportEvent::SourceLoaded {
            source_id: handle.source_id.clone(),
            revision,
        });
        Ok(handle)
    }

    pub fn preload_source(
        &mut self,
        handle: &QgsTransportSourceHandle,
    ) -> Result<(), PlaybackError> {
        let state = self
            .sources
            .get_mut(&handle.source_id)
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        if state.handle.revision != handle.revision {
            self.events
                .push(QgsTransportEvent::TransportValidationFailed {
                    reason: "source revision mismatch",
                });
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        state.preloaded = true;
        self.events.push(QgsTransportEvent::SourcePreloaded {
            source_id: handle.source_id.clone(),
            revision: handle.revision,
        });
        self.evaluate_play_ready();
        Ok(())
    }

    pub fn set_active_source(
        &mut self,
        handle: &QgsTransportSourceHandle,
    ) -> Result<(), PlaybackError> {
        let state = self
            .sources
            .get(&handle.source_id)
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        if state.handle.revision != handle.revision {
            self.events
                .push(QgsTransportEvent::TransportValidationFailed {
                    reason: "source revision mismatch",
                });
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        self.active_source_id = Some(handle.source_id.clone());
        self.active_range = None;
        self.cue = None;
        self.prepared_anchor = None;
        self.play_ready = false;
        self.events.push(QgsTransportEvent::ActiveSourceChanged {
            source_id: Some(handle.source_id.clone()),
            revision: Some(handle.revision),
        });
        Ok(())
    }

    pub fn set_active_range_frames(
        &mut self,
        start_frame: u64,
        end_frame: u64,
    ) -> Result<QgsTransportActiveRange, PlaybackError> {
        let handle = self
            .active_source()
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        let range = QgsTransportActiveRange::from_frames(handle, start_frame, end_frame)?;
        self.active_range = Some(range);
        self.cue = None;
        self.prepared_anchor = None;
        self.play_ready = false;
        self.events.push(QgsTransportEvent::ActiveRangeSet {
            start_frame: range.start_frame,
            end_frame: range.end_frame,
            start_sample: range.start_sample,
            end_sample: range.end_sample,
        });
        Ok(range)
    }

    pub fn cue_frame(&mut self, frame: u64) -> Result<QgsTransportCuePoint, PlaybackError> {
        let handle = self
            .active_source()
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        let range = self
            .active_range
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        let timing = QgsActiveRangeTiming::new(
            handle.duration_frames,
            handle.timebase,
            handle.audio_sample_rate,
            range.start_frame,
            range.end_frame,
        )?;
        let cue_validation = match timing.validate_cue(frame) {
            Ok(cue) => cue,
            Err(err) => {
                self.events
                    .push(QgsTransportEvent::TransportValidationFailed {
                        reason: "cue outside active range",
                    });
                return Err(err);
            }
        };
        let cue = QgsTransportCuePoint {
            frame,
            sample: cue_validation.sample,
        };
        self.cue = Some(cue);
        self.prepared_anchor = None;
        self.play_ready = false;
        self.events.push(QgsTransportEvent::CueCompleted {
            frame,
            sample: cue.sample,
        });
        Ok(cue)
    }

    pub fn prepare_anchor(&mut self) -> Result<QgsTransportPreparedAnchor, PlaybackError> {
        let handle = self
            .active_source()
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        let state = self
            .sources
            .get(&handle.source_id)
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        if !state.preloaded {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        let cue = self.cue.ok_or(PlaybackError::InvalidRuntimeTransition)?;
        let anchor = QgsTransportPreparedAnchor {
            frame: cue.frame,
            sample: cue.sample,
            revision: handle.revision,
        };
        self.prepared_anchor = Some(anchor);
        self.events.push(QgsTransportEvent::PreparedAnchorReady {
            frame: anchor.frame,
            revision: anchor.revision,
        });
        self.evaluate_play_ready();
        Ok(anchor)
    }

    pub fn evaluate_play_ready(&mut self) -> bool {
        let ready = self
            .active_source()
            .and_then(|handle| {
                self.sources.get(&handle.source_id).map(|state| {
                    state.preloaded
                        && self.active_range.is_some()
                        && self.cue.is_some()
                        && self.prepared_anchor.is_some_and(|anchor| {
                            anchor.revision == handle.revision
                                && self.cue.is_some_and(|cue| cue.frame == anchor.frame)
                        })
                })
            })
            .unwrap_or(false);
        if self.play_ready != ready {
            self.events
                .push(QgsTransportEvent::PlayReadinessChanged { ready });
        }
        self.play_ready = ready;
        if ready && self.status != QgsTransportStatus::Playing {
            self.status = QgsTransportStatus::Ready;
        }
        ready
    }

    pub fn play(&mut self) -> Result<(), PlaybackError> {
        if !self.play_ready {
            self.events.push(QgsTransportEvent::TransportPlayRejected {
                reason: "not ready",
            });
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        let anchor = self
            .prepared_anchor
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        self.status = QgsTransportStatus::Playing;
        self.play_ready = false;
        self.events
            .push(QgsTransportEvent::PlayReadinessChanged { ready: false });
        self.events.push(QgsTransportEvent::TransportStarted {
            frame: anchor.frame,
        });
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), PlaybackError> {
        if self.status != QgsTransportStatus::Playing {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        self.status = QgsTransportStatus::Paused;
        self.events.push(QgsTransportEvent::TransportPaused);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), PlaybackError> {
        if self.active_source_id.is_none() {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        self.status = QgsTransportStatus::Stopped;
        self.play_ready = false;
        self.events.push(QgsTransportEvent::TransportStopped);
        Ok(())
    }

    pub fn close_active_source(&mut self) -> Result<QgsSourceCloseResult, PlaybackError> {
        let handle = self
            .active_source()
            .cloned()
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        self.clear_active_state_for_handle(&handle);
        self.status = if self.sources.is_empty() {
            QgsTransportStatus::Empty
        } else {
            QgsTransportStatus::Loaded
        };
        Ok(QgsSourceCloseResult {
            source_id: handle.source_id,
            revision: handle.revision,
            active_source_cleared: true,
            source_preserved_loaded: true,
            play_ready: self.play_ready,
        })
    }

    pub fn clear_active_source(&mut self) -> Result<QgsSourceCloseResult, PlaybackError> {
        self.close_active_source()
    }

    pub fn unload_source(&mut self, handle: &QgsTransportSourceHandle) -> QgsSourceUnloadResult {
        let Some(state) = self.sources.get(&handle.source_id).cloned() else {
            self.events
                .push(QgsTransportEvent::TransportValidationFailed {
                    reason: "source already unloaded or invalid",
                });
            return QgsSourceUnloadResult {
                source_id: handle.source_id.clone(),
                revision: handle.revision,
                unloaded: false,
                was_active: false,
                revision_invalidated: false,
                already_missing_or_invalid: true,
            };
        };
        if state.handle.revision != handle.revision {
            self.events
                .push(QgsTransportEvent::TransportValidationFailed {
                    reason: "source revision mismatch",
                });
            return QgsSourceUnloadResult {
                source_id: handle.source_id.clone(),
                revision: handle.revision,
                unloaded: false,
                was_active: false,
                revision_invalidated: false,
                already_missing_or_invalid: true,
            };
        }
        let was_active = self
            .active_source_id
            .as_ref()
            .is_some_and(|source_id| source_id == &handle.source_id);
        if was_active {
            self.clear_active_state_for_handle(handle);
        }
        self.sources.remove(&handle.source_id);
        self.events
            .push(QgsTransportEvent::SourceRevisionInvalidated {
                source_id: handle.source_id.clone(),
                revision: handle.revision,
            });
        self.events.push(QgsTransportEvent::SourceUnloaded {
            source_id: handle.source_id.clone(),
            revision: handle.revision,
        });
        self.status = if self.sources.is_empty() {
            QgsTransportStatus::Empty
        } else {
            QgsTransportStatus::Loaded
        };
        QgsSourceUnloadResult {
            source_id: handle.source_id.clone(),
            revision: handle.revision,
            unloaded: true,
            was_active,
            revision_invalidated: true,
            already_missing_or_invalid: false,
        }
    }

    pub fn invalidate_source_revision(
        &mut self,
        handle: &QgsTransportSourceHandle,
    ) -> QgsSourceRevisionInvalidation {
        let result = self.unload_source(handle);
        QgsSourceRevisionInvalidation {
            revision: handle.revision,
            invalidated: result.revision_invalidated,
        }
    }

    pub fn record_prepared_state_discarded(&mut self, frames: usize) {
        self.events
            .push(QgsTransportEvent::PreparedStateDiscarded { frames });
    }

    fn clear_active_state_for_handle(&mut self, handle: &QgsTransportSourceHandle) {
        self.active_source_id = None;
        self.active_range = None;
        self.cue = None;
        self.prepared_anchor = None;
        self.play_ready = false;
        self.events.push(QgsTransportEvent::ActiveSourceCleared {
            source_id: handle.source_id.clone(),
            revision: handle.revision,
        });
        self.events
            .push(QgsTransportEvent::PlayReadinessChanged { ready: false });
    }

    pub fn active_source(&self) -> Option<&QgsTransportSourceHandle> {
        self.active_source_id
            .as_ref()
            .and_then(|source_id| self.sources.get(source_id))
            .map(|state| &state.handle)
    }

    pub fn snapshot(&self) -> QgsTransportSnapshot {
        QgsTransportSnapshot {
            status: self.status,
            active_source: self.active_source().cloned(),
            active_range: self.active_range,
            cue: self.cue,
            prepared_anchor: self.prepared_anchor,
            play_ready: self.play_ready,
            loaded_source_count: self.sources.len(),
            no_work_on_play: self.no_work_on_play,
            events: self.events.clone(),
        }
    }
}

impl Default for QgsTransportEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastRuntimeState {
    Idle,
    Preparing,
    Ready,
    Playing,
    Paused,
    Draining,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastRuntimeCommand {
    Prepare,
    Play,
    Pause,
    Seek { target_time: Duration },
    Stop,
    Drain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastRuntimeEvent {
    SessionCreated,
    PreparingStarted,
    Prepared,
    PlaybackStarted,
    PlaybackPaused,
    SeekCompleted { target_time: Duration },
    DrainingStarted,
    Completed,
    Failed,
    ContractPrepared,
    QueueBackpressure { queue: BroadcastRuntimeQueueKind },
    IncompleteAudioCoverage { frame_index: u64 },
    IntentionalProfileSkip { source_frame_index: u64 },
    LatenessDrop { frame_index: u64 },
    FrameAccounted { frame_index: u64 },
    AudioRangeAccounted { frame_index: u64 },
    SimulatedPresentationDecision { frame_index: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPlayerPreparedSlotKind {
    Video,
    Audio,
    Presentation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPlayerCapability {
    OriginalVideoRuntime,
    PreparedVideoSlot,
    PrerollReadiness,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPlayerRuntimeFailureReason {
    InvalidTransition,
    IncompleteAudioCoverage,
    CapabilityMissing,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BroadcastRuntimeVerificationLevel {
    NotImplemented,
    CompileChecked,
    UnitTested,
    ControlSurfaceEvidence,
    SelectionPolicyEvidence,
    OperationalStateEvidence,
    FaultRecoveryPolicyEvidence,
    RuntimeBehaviorEvidence,
    MediaInspected,
    PayloadExtracted,
    PayloadBound,
    TestBoundaryEvidence,
    FilePresenterManifestWritten,
    FilePresenterImageWritten,
    NativeBufferSubmissionVerified,
    NativePostSubmitEvidence,
    RuntimeAudioPayloadDrainCompleted,
    RuntimeAudioPayloadAudibleConfirmed,
    ManualAudibleSignalDetectedContentUnverified,
    ManualContentAudibilityPartiallyObserved,
    ManualMonitorPairPreferenceObserved,
    DesktopMonoListeningHelperDrainCompleted,
    ManualDesktopMonoListeningHelperHeard,
    Discrete4MonoOutputDrainCompleted,
    VisualVerified,
    AudioDeviceVerified,
    RealtimeVerified,
    HardwareValidated,
}

impl BroadcastRuntimeVerificationLevel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotImplemented => "NotImplemented",
            Self::CompileChecked => "CompileChecked",
            Self::UnitTested => "UnitTested",
            Self::ControlSurfaceEvidence => "ControlSurfaceEvidence",
            Self::SelectionPolicyEvidence => "SelectionPolicyEvidence",
            Self::OperationalStateEvidence => "OperationalStateEvidence",
            Self::FaultRecoveryPolicyEvidence => "FaultRecoveryPolicyEvidence",
            Self::RuntimeBehaviorEvidence => "RuntimeBehaviorEvidence",
            Self::MediaInspected => "MediaInspected",
            Self::PayloadExtracted => "PayloadExtracted",
            Self::PayloadBound => "PayloadBound",
            Self::TestBoundaryEvidence => "TestBoundaryEvidence",
            Self::FilePresenterManifestWritten => "FilePresenterManifestWritten",
            Self::FilePresenterImageWritten => "FilePresenterImageWritten",
            Self::NativeBufferSubmissionVerified => "NativeBufferSubmissionVerified",
            Self::NativePostSubmitEvidence => "NativePostSubmitEvidence",
            Self::RuntimeAudioPayloadDrainCompleted => "RuntimeAudioPayloadDrainCompleted",
            Self::RuntimeAudioPayloadAudibleConfirmed => "RuntimeAudioPayloadAudibleConfirmed",
            Self::ManualAudibleSignalDetectedContentUnverified => {
                "ManualAudibleSignalDetectedContentUnverified"
            }
            Self::ManualContentAudibilityPartiallyObserved => {
                "ManualContentAudibilityPartiallyObserved"
            }
            Self::ManualMonitorPairPreferenceObserved => "ManualMonitorPairPreferenceObserved",
            Self::DesktopMonoListeningHelperDrainCompleted => {
                "DesktopMonoListeningHelperDrainCompleted"
            }
            Self::ManualDesktopMonoListeningHelperHeard => "ManualDesktopMonoListeningHelperHeard",
            Self::Discrete4MonoOutputDrainCompleted => "Discrete4MonoOutputDrainCompleted",
            Self::VisualVerified => "VisualVerified",
            Self::AudioDeviceVerified => "AudioDeviceVerified",
            Self::RealtimeVerified => "RealtimeVerified",
            Self::HardwareValidated => "HardwareValidated",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastRuntimeVerifiedSubsystem {
    OriginalProxyAssociation,
    ProxyMp4Inspection,
    OriginalMxfInspection,
    ProxyH264HardwareDecode,
    OriginalH264SoftwareDecode,
    OriginalMxfPcmMetadata,
    OriginalMxfPcmExtraction,
    PcmRuntimeBlocks,
    ProxyVideoPayloadBinding,
    OriginalVideoPayloadBinding,
    StateMachine,
    PrerollPlan,
    PreparedSlots,
    EventSurface,
    DeviceBoundaryContract,
    TestVideoPresenterEvidence,
    PresenterMonitorBoundary,
    FilePresenterVisualDiagnostic,
    GpuPayloadReadbackVisualDiagnostic,
    TestAudioSinkEvidence,
    NativePipeWireBufferSubmission,
    NativePipeWireAudibleSmokeTest,
    NativePipeWireOriginalAudioSegmentPlayback,
    PipeWireAudioContentSanityAudit,
    Mironik2002MonitorDiagnostic,
    NativePipeWireDesktopMonoListeningHelper,
    NativePipeWireDiscrete4MonoOutputBoundary,
    BroadcastRuntimeAudioPayloadPipeWire,
    RuntimeSurfaceE2eAcceptance,
    BroadcastPlayerControlCore,
    DeviceBackendSelection,
    BroadcastPlayerOperationalRuntime,
    BroadcastPlayerFaultRecoveryRules,
    BroadcastPlayerRunningRuntimeDemo,
    SimulatedPlaybackLoop,
    RealSpeakerOutput,
    RealDisplayOutput,
    RealtimePlayback,
    ModernHardwareZeroCopy,
}

impl BroadcastRuntimeVerifiedSubsystem {
    pub const fn label(self) -> &'static str {
        match self {
            Self::OriginalProxyAssociation => "original/proxy association",
            Self::ProxyMp4Inspection => "proxy MP4 inspection",
            Self::OriginalMxfInspection => "original MXF inspection",
            Self::ProxyH264HardwareDecode => "proxy H.264 hardware decode",
            Self::OriginalH264SoftwareDecode => "original H.264 10-bit 4:2:2 software decode",
            Self::OriginalMxfPcmMetadata => "original MXF PCM metadata",
            Self::OriginalMxfPcmExtraction => "original MXF PCM extraction",
            Self::PcmRuntimeBlocks => "PCM runtime blocks",
            Self::ProxyVideoPayloadBinding => "proxy video payload binding",
            Self::OriginalVideoPayloadBinding => "original video payload binding",
            Self::StateMachine => "Broadcast Player Runtime state machine",
            Self::PrerollPlan => "preroll plan",
            Self::PreparedSlots => "prepared slots",
            Self::EventSurface => "event surface",
            Self::DeviceBoundaryContract => "device boundary contract",
            Self::TestVideoPresenterEvidence => "test video presenter evidence",
            Self::PresenterMonitorBoundary => "presenter/monitor boundary",
            Self::FilePresenterVisualDiagnostic => "file presenter visual diagnostic",
            Self::GpuPayloadReadbackVisualDiagnostic => "GPU payload readback visual diagnostic",
            Self::TestAudioSinkEvidence => "test audio sink evidence",
            Self::NativePipeWireBufferSubmission => "native PipeWire buffer submission",
            Self::NativePipeWireAudibleSmokeTest => "native PipeWire audible smoke test",
            Self::NativePipeWireOriginalAudioSegmentPlayback => {
                "native PipeWire original-audio segment playback"
            }
            Self::PipeWireAudioContentSanityAudit => "PipeWire audio content sanity audit",
            Self::Mironik2002MonitorDiagnostic => "Mironik 2002 monitor diagnostic",
            Self::NativePipeWireDesktopMonoListeningHelper => {
                "native PipeWire desktop mono listening helper"
            }
            Self::NativePipeWireDiscrete4MonoOutputBoundary => {
                "native PipeWire discrete 4-mono output boundary"
            }
            Self::BroadcastRuntimeAudioPayloadPipeWire => {
                "broadcast runtime audio payload to PipeWire"
            }
            Self::RuntimeSurfaceE2eAcceptance => "runtime surface end-to-end acceptance",
            Self::BroadcastPlayerControlCore => "broadcast player control core",
            Self::DeviceBackendSelection => "device backend selection",
            Self::BroadcastPlayerOperationalRuntime => "broadcast player operational runtime",
            Self::BroadcastPlayerFaultRecoveryRules => "broadcast player fault and recovery rules",
            Self::BroadcastPlayerRunningRuntimeDemo => "broadcast player running runtime demo",
            Self::SimulatedPlaybackLoop => "simulated playback loop",
            Self::RealSpeakerOutput => "real speaker output",
            Self::RealDisplayOutput => "real display output",
            Self::RealtimePlayback => "realtime playback",
            Self::ModernHardwareZeroCopy => "modern-hardware zero-copy",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeVerificationEntry {
    pub subsystem: BroadcastRuntimeVerifiedSubsystem,
    pub level: BroadcastRuntimeVerificationLevel,
    pub summary: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeVerificationMatrix {
    pub entries: Vec<BroadcastRuntimeVerificationEntry>,
}

impl BroadcastRuntimeVerificationMatrix {
    pub fn sony_fx6_sample_002_current() -> Self {
        use BroadcastRuntimeVerificationLevel as Level;
        use BroadcastRuntimeVerifiedSubsystem as Subsystem;
        Self {
            entries: vec![
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::OriginalProxyAssociation,
                    level: Level::MediaInspected,
                    summary: "original/proxy timing and metadata association proven for sample 002",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::ProxyMp4Inspection,
                    level: Level::MediaInspected,
                    summary: "proxy MP4 container, H.264 video, timing, and tracks inspected",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::OriginalMxfInspection,
                    level: Level::MediaInspected,
                    summary: "original MXF structure, essence descriptors, video, audio, and timing inspected",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::ProxyH264HardwareDecode,
                    level: Level::HardwareValidated,
                    summary: "proxy H.264 VA decode proved for 106/106 frames after Step 15",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::OriginalH264SoftwareDecode,
                    level: Level::PayloadExtracted,
                    summary: "original 10-bit 4:2:2 H.264 access units decode to QGS software surfaces",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::OriginalMxfPcmMetadata,
                    level: Level::MediaInspected,
                    summary: "original MXF authoritative LPCM metadata modeled",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::OriginalMxfPcmExtraction,
                    level: Level::PayloadExtracted,
                    summary: "original MXF LPCM payload packets extracted without synthesis",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::PcmRuntimeBlocks,
                    level: Level::PayloadBound,
                    summary: "PCM packets converted to runtime mono-track blocks with timing",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::ProxyVideoPayloadBinding,
                    level: Level::TestBoundaryEvidence,
                    summary: "proxy processed GPU frame payloads bind and pass test presenter evidence",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::OriginalVideoPayloadBinding,
                    level: Level::PayloadBound,
                    summary: "bounded original MXF processed GPU frame payloads bind; realtime not claimed",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::StateMachine,
                    level: Level::UnitTested,
                    summary: "state transitions and invalid transitions covered by unit tests",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::PrerollPlan,
                    level: Level::UnitTested,
                    summary: "bounded preroll readiness and not-ready cases covered",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::PreparedSlots,
                    level: Level::UnitTested,
                    summary: "finite prepared audio/video/presentation slots modeled",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::EventSurface,
                    level: Level::UnitTested,
                    summary: "deterministic backend-neutral event accounting covered",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::DeviceBoundaryContract,
                    level: Level::UnitTested,
                    summary: "PayloadReady and DevicePayloadReady distinction covered",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::TestVideoPresenterEvidence,
                    level: Level::TestBoundaryEvidence,
                    summary: "test presenter evidence gates FramePresented; not real display output",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::PresenterMonitorBoundary,
                    level: Level::TestBoundaryEvidence,
                    summary: "prepared video payload descriptor submits through the test presenter boundary and updates monitor projection facts; real display and visual verification remain unimplemented",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::FilePresenterVisualDiagnostic,
                    level: Level::FilePresenterManifestWritten,
                    summary: "file presenter writes a deterministic public-safe descriptor manifest for a prepared video payload; no frame pixels, real display, or visual verification are claimed",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::GpuPayloadReadbackVisualDiagnostic,
                    level: Level::FilePresenterImageWritten,
                    summary: "diagnostic validation readback writes a bounded PPM image from real prepared proxy GPU payload pixels; this is not real display output or visual verification",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::TestAudioSinkEvidence,
                    level: Level::TestBoundaryEvidence,
                    summary: "test audio sink evidence accepts original PCM; not real speaker output",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::NativePipeWireBufferSubmission,
                    level: Level::NativePostSubmitEvidence,
                    summary: "tiny original-audio-derived buffer queued to native PipeWire stream and drain callback observed; not audible/full playback verification",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::NativePipeWireAudibleSmokeTest,
                    level: Level::ManualContentAudibilityPartiallyObserved,
                    summary: "bounded original-audio-derived smoke-test buffers submit/drain; later listening found voice-like content but routing/gain and production output remain unverified",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::NativePipeWireOriginalAudioSegmentPlayback,
                    level: Level::ManualContentAudibilityPartiallyObserved,
                    summary: "bounded sequential original-audio segment submits/drains; later listening found voice-like content in both tested versions, with routing still unverified",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::PipeWireAudioContentSanityAudit,
                    level: Level::MediaInspected,
                    summary: "original MXF PCM statistics, endian/sign interpretation, f32 conversion, segment/runtime path equality, and PipeWire buffer geometry audited; manual listening partially supports content audibility but routing remains unverified",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::Mironik2002MonitorDiagnostic,
                    level: Level::ManualMonitorPairPreferenceObserved,
                    summary: "Mironik 2002 track 4 / track 1 diagnostic monitor pair preferred for the 0-1000 ms range; not channel certification or production routing",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::NativePipeWireDesktopMonoListeningHelper,
                    level: Level::DesktopMonoListeningHelperDrainCompleted,
                    summary: "single original mono track can be duplicated to L/R as an ad-hoc desktop listening helper and drained; not discrete mono output, channel certification, or production routing",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::NativePipeWireDiscrete4MonoOutputBoundary,
                    level: Level::Discrete4MonoOutputDrainCompleted,
                    summary: "original MXF track 1/2/3/4 can be submitted as output channel 1/2/3/4 to a native PipeWire 4-channel f32 boundary and drained; physical channel mapping and production routing are not certified",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::BroadcastRuntimeAudioPayloadPipeWire,
                    level: Level::RuntimeAudioPayloadDrainCompleted,
                    summary: "first prepared ProxyPreview Broadcast Player Runtime audio payload binding submits to native PipeWire and drains; not full playback, realtime, A/V sync, or full audio-device verification",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::RuntimeSurfaceE2eAcceptance,
                    level: Level::TestBoundaryEvidence,
                    summary: "Integration Block A session runtime and Block B test presenter/monitor boundary run as one end-to-end backend scenario; real display, realtime, and visual verification are not claimed",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::BroadcastPlayerControlCore,
                    level: Level::ControlSurfaceEvidence,
                    summary: "production-shaped Broadcast Player control facade exposes commands, snapshots, readiness, position, prepared-window, device status, and product events over existing Lego modules without claiming real display, realtime, A/V sync, or audio-device verification",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::DeviceBackendSelection,
                    level: Level::SelectionPolicyEvidence,
                    summary: "backend-neutral device selection policies choose diagnostic/future display and audio candidates truthfully; Wayland/Vulkan remains the QNC OS target but NotImplemented, X11 is unsupported for QNC OS, and PipeWire prototypes are not production verified",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::BroadcastPlayerOperationalRuntime,
                    level: Level::OperationalStateEvidence,
                    summary: "deterministic Broadcast Player operational runtime enforces command legality, advances logical position on playing ticks, models seek/stop/completion, reports buffer health and warnings, and keeps real display, realtime, A/V sync, and production audio-device claims false",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::BroadcastPlayerFaultRecoveryRules,
                    level: Level::FaultRecoveryPolicyEvidence,
                    summary: "production-shaped fault severity, scope, recovery action, snapshot counters, backend warnings, and rejected-command no-mutation rules are modeled and tested; real display, realtime, A/V sync, and production audio-device claims remain false",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::BroadcastPlayerRunningRuntimeDemo,
                    level: Level::RuntimeBehaviorEvidence,
                    summary: "running Broadcast Player operational demo streams step-by-step state, frame/audio progression, pause freeze, seek/reprepare/replay, stop, unload, buffer health, and non-claims using the operational runtime",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::SimulatedPlaybackLoop,
                    level: Level::TestBoundaryEvidence,
                    summary: "deterministic simulation consumes prepared slots through test boundaries",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::RealSpeakerOutput,
                    level: Level::NotImplemented,
                    summary: "no audible speaker output, audio-device clock, or full audio playback path exists",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::RealDisplayOutput,
                    level: Level::NotImplemented,
                    summary: "no swapchain/Wayland/X11/DRM/KMS display presenter exists",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::RealtimePlayback,
                    level: Level::NotImplemented,
                    summary: "Broadcast Player realtime scheduler has not been implemented or accepted",
                },
                BroadcastRuntimeVerificationEntry {
                    subsystem: Subsystem::ModernHardwareZeroCopy,
                    level: Level::NotImplemented,
                    summary: "VA/Vulkan zero-copy remains frozen and not verified on modern hardware",
                },
            ],
        }
    }

    pub fn entry(
        &self,
        subsystem: BroadcastRuntimeVerifiedSubsystem,
    ) -> Option<&BroadcastRuntimeVerificationEntry> {
        self.entries
            .iter()
            .find(|entry| entry.subsystem == subsystem)
    }

    pub fn validate_truth_rules(&self) -> Result<(), PlaybackError> {
        use BroadcastRuntimeVerificationLevel as Level;
        use BroadcastRuntimeVerifiedSubsystem as Subsystem;
        let level = |subsystem| {
            self.entry(subsystem)
                .map(|entry| entry.level)
                .ok_or(PlaybackError::InvalidRuntimeTransition)
        };
        if level(Subsystem::RealDisplayOutput)? != Level::NotImplemented {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        if level(Subsystem::RealSpeakerOutput)? != Level::NotImplemented {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        if level(Subsystem::RealtimePlayback)? == Level::RealtimeVerified {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        if level(Subsystem::ModernHardwareZeroCopy)? != Level::NotImplemented {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        if level(Subsystem::TestVideoPresenterEvidence)? > Level::TestBoundaryEvidence {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        if level(Subsystem::TestAudioSinkEvidence)? > Level::TestBoundaryEvidence {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPlayerRuntimeEvent {
    SessionCreated {
        session_index: u64,
        source_mode: BroadcastVideoSourceMode,
        preview_profile: BroadcastPreviewProfile,
        audio_source_role: BroadcastMediaSourceRole,
        video_source_role: BroadcastMediaSourceRole,
    },
    PrepareStarted {
        source_mode: BroadcastVideoSourceMode,
        queue_limits: BroadcastRuntimeQueueLimits,
        preroll_config: BroadcastPrerollConfig,
    },
    PrerollPlanned {
        source_mode: BroadcastVideoSourceMode,
        selected_video_frames: usize,
        audio_ranges: usize,
        intentional_skips: usize,
    },
    PrerollReady {
        prepared_video_slots: usize,
        prepared_audio_slots: usize,
        prepared_presentation_slots: usize,
    },
    PreparedSlotAvailable {
        kind: BroadcastPlayerPreparedSlotKind,
        slot_index: usize,
        source_mode: BroadcastVideoSourceMode,
        ready: bool,
        video_status: Option<BroadcastPreparedVideoSlotStatus>,
    },
    RuntimeReady {
        state: BroadcastRuntimeState,
        source_mode: BroadcastVideoSourceMode,
    },
    TransportStarted {
        state: BroadcastRuntimeState,
        media_time: Duration,
    },
    TransportPaused {
        state: BroadcastRuntimeState,
        media_time: Duration,
    },
    SeekCompleted {
        target_time: Duration,
    },
    IntentionalProfileSkip {
        source_frame_index: u64,
        profile: BroadcastPreviewProfile,
    },
    FrameAccounted {
        selected_preview_frame_index: u64,
        source_frame_index: Option<u64>,
        presentation_time: Duration,
        duration: Duration,
        video_source_role: BroadcastMediaSourceRole,
    },
    AudioRangeAccounted {
        presentation_index: u64,
        start_sample: u64,
        sample_count: u64,
        tracks_covered: usize,
        complete: bool,
    },
    PayloadSubmittedToDevice {
        device_kind: BroadcastDeviceKind,
        presentation_slot_index: usize,
        binding_index: usize,
        status: BroadcastDevicePayloadStatus,
    },
    PresentationEvidenceReceived {
        presentation_slot_index: usize,
        media_time: Duration,
        evidence_kind: BroadcastPresentationEvidenceKind,
        source_device_kind: BroadcastDeviceKind,
        payload_id: Option<u64>,
    },
    FramePresented {
        presentation_slot_index: usize,
        media_time: Duration,
        evidence_kind: BroadcastPresentationEvidenceKind,
        payload_id: Option<u64>,
    },
    CapabilityMissing {
        source_mode: BroadcastVideoSourceMode,
        capability: BroadcastPlayerCapability,
        reason: BroadcastPrerollNotReadyReason,
    },
    RuntimeCompleted {
        selected_frames_accounted: usize,
        audio_ranges_accounted: usize,
        lateness_drops: usize,
        intentional_skips: usize,
        final_state: BroadcastRuntimeState,
    },
    RuntimeFailed {
        reason: BroadcastPlayerRuntimeFailureReason,
    },
}

pub type BroadcastPlayerEvent = BroadcastPlayerRuntimeEvent;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastRuntimeQueueKind {
    AudioBlocks,
    VideoFrames,
    ProcessedFrames,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeQueueLimits {
    pub audio_block_capacity: usize,
    pub video_frame_capacity: usize,
    pub processed_frame_capacity: usize,
}

impl BroadcastRuntimeQueueLimits {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        if self.audio_block_capacity == 0
            || self.video_frame_capacity == 0
            || self.processed_frame_capacity == 0
        {
            return Err(PlaybackError::InvalidCapacity);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeCapabilities {
    pub sample_clock_aware: bool,
    pub preserves_original_pcm_format: bool,
    pub preserves_track_channel_identity: bool,
    pub proxy_video_preview: bool,
    pub original_media_video_source: bool,
    pub original_media_realtime_supported: bool,
    pub proxy_audio_primary: bool,
    pub ui_dependent: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeSessionDescription {
    pub audio_source: BroadcastMediaSourceRole,
    pub video_source: BroadcastMediaSourceRole,
    pub video_source_mode: BroadcastVideoSourceMode,
    pub preview_profile: BroadcastPreviewProfile,
    pub audio_sample_rate: u32,
    pub audio_track_count: usize,
    pub queue_limits: BroadcastRuntimeQueueLimits,
    pub capabilities: BroadcastRuntimeCapabilities,
}

impl BroadcastRuntimeSessionDescription {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        self.queue_limits.validate()?;
        let video_source_valid = match self.video_source_mode {
            BroadcastVideoSourceMode::ProxyPreview => {
                self.video_source == BroadcastMediaSourceRole::ProxyPreviewVideo
                    && self.capabilities.proxy_video_preview
            }
            BroadcastVideoSourceMode::OriginalMedia => {
                self.video_source == BroadcastMediaSourceRole::OriginalFinishingMedia
                    && self.capabilities.original_media_video_source
            }
        };
        if self.audio_source != BroadcastMediaSourceRole::OriginalAuthoritativeAudio
            || !video_source_valid
            || self.audio_sample_rate == 0
            || self.audio_track_count == 0
            || !self.capabilities.sample_clock_aware
            || !self.capabilities.preserves_original_pcm_format
            || !self.capabilities.preserves_track_channel_identity
            || self.capabilities.proxy_audio_primary
            || self.capabilities.ui_dependent
        {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimePrepareFacts {
    pub selected_frame_count: usize,
    pub intentional_profile_skips: usize,
    pub audio_ranges_complete: bool,
    pub frames_outside_audio_range: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPrerollNotReadyReason {
    MissingVideoFrames,
    MissingAudioRanges,
    InvalidQueueLimits,
    CapabilityMissing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastPrerollConfig {
    pub video_source_mode: BroadcastVideoSourceMode,
    pub video_frames_required: usize,
    pub audio_ranges_required: usize,
    pub max_video_queue: usize,
    pub max_audio_queue: usize,
    pub max_presentation_queue: usize,
}

impl BroadcastPrerollConfig {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        if self.video_frames_required == 0
            || self.audio_ranges_required == 0
            || self.max_video_queue == 0
            || self.max_audio_queue == 0
            || self.max_presentation_queue == 0
            || self.video_frames_required > self.max_video_queue
            || self.audio_ranges_required > self.max_audio_queue
        {
            return Err(PlaybackError::InvalidCapacity);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastPrerollPlan {
    pub video_source_mode: BroadcastVideoSourceMode,
    pub selected_video_frames_planned: usize,
    pub audio_ranges_planned: usize,
    pub intentional_skips_planned: usize,
    pub duration_covered: Duration,
    pub finite_queue_limits: BroadcastRuntimeQueueLimits,
    pub video_source_available: bool,
    pub video_runtime_supported: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastPrerollStatus {
    pub ready: bool,
    pub prepared_video_frames: usize,
    pub prepared_audio_ranges: usize,
    pub missing_video_frames: usize,
    pub missing_audio_ranges: usize,
    pub queue_limits_ok: bool,
    pub reason: Option<BroadcastPrerollNotReadyReason>,
}

pub fn evaluate_broadcast_preroll(
    config: BroadcastPrerollConfig,
    plan: BroadcastPrerollPlan,
    prepared_video_frames: usize,
    prepared_audio_ranges: usize,
) -> BroadcastPrerollStatus {
    let queue_limits_ok = config.validate().is_ok()
        && plan.finite_queue_limits.validate().is_ok()
        && config.max_video_queue <= plan.finite_queue_limits.video_frame_capacity
        && config.max_audio_queue <= plan.finite_queue_limits.audio_block_capacity
        && config.max_presentation_queue <= plan.finite_queue_limits.processed_frame_capacity;
    let missing_video_frames = config
        .video_frames_required
        .saturating_sub(prepared_video_frames);
    let missing_audio_ranges = config
        .audio_ranges_required
        .saturating_sub(prepared_audio_ranges);
    let capability_ok = config.video_source_mode == plan.video_source_mode
        && plan.video_source_available
        && match plan.video_source_mode {
            BroadcastVideoSourceMode::ProxyPreview => plan.video_runtime_supported,
            BroadcastVideoSourceMode::OriginalMedia => plan.video_runtime_supported,
        };
    let reason = if !queue_limits_ok {
        Some(BroadcastPrerollNotReadyReason::InvalidQueueLimits)
    } else if !capability_ok {
        Some(BroadcastPrerollNotReadyReason::CapabilityMissing)
    } else if missing_video_frames != 0 {
        Some(BroadcastPrerollNotReadyReason::MissingVideoFrames)
    } else if missing_audio_ranges != 0 {
        Some(BroadcastPrerollNotReadyReason::MissingAudioRanges)
    } else {
        None
    };
    BroadcastPrerollStatus {
        ready: reason.is_none(),
        prepared_video_frames,
        prepared_audio_ranges,
        missing_video_frames,
        missing_audio_ranges,
        queue_limits_ok,
        reason,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPreparedVideoSlotStatus {
    Prepared,
    IntentionalSkip,
    CapabilityMissing,
    NotSupported,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastPreparedAudioSlot {
    pub slot_index: usize,
    pub source_mode: BroadcastVideoSourceMode,
    pub audio_source_role: BroadcastMediaSourceRole,
    pub start_time: Duration,
    pub duration: Duration,
    pub start_sample: u64,
    pub sample_count: u64,
    pub track_coverage: Vec<AudioRangeCoverage>,
    pub complete: bool,
}

impl BroadcastPreparedAudioSlot {
    pub fn from_audio_range(
        slot_index: usize,
        source_mode: BroadcastVideoSourceMode,
        range: &AvFrameAudioRange,
    ) -> Self {
        Self {
            slot_index,
            source_mode,
            audio_source_role: BroadcastMediaSourceRole::OriginalAuthoritativeAudio,
            start_time: range.audio_start_time,
            duration: range.audio_duration,
            start_sample: range.audio_start_sample,
            sample_count: range.audio_sample_count,
            track_coverage: range.covered_tracks.clone(),
            complete: range.complete,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastPreparedVideoSlot {
    pub slot_index: usize,
    pub source_mode: BroadcastVideoSourceMode,
    pub video_source_role: BroadcastMediaSourceRole,
    pub source_frame_index: Option<u64>,
    pub selected_preview_frame_index: Option<u64>,
    pub presentation_time: Duration,
    pub duration: Duration,
    pub status: BroadcastPreparedVideoSlotStatus,
}

impl BroadcastPreparedVideoSlot {
    pub fn is_ready(&self) -> bool {
        self.status == BroadcastPreparedVideoSlotStatus::Prepared
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastPreparedPresentationSlot {
    pub presentation_index: usize,
    pub source_mode: BroadcastVideoSourceMode,
    pub selected_source_frame: Option<u64>,
    pub video_slot_index: usize,
    pub audio_slot_index: usize,
    pub presentation_time: Duration,
    pub duration: Duration,
    pub ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastPreparedSlotSummary {
    pub source_mode: BroadcastVideoSourceMode,
    pub video_slot_capacity: usize,
    pub video_slot_count: usize,
    pub video_slots_prepared: usize,
    pub audio_slot_capacity: usize,
    pub audio_slot_count: usize,
    pub audio_slots_complete: usize,
    pub presentation_slot_capacity: usize,
    pub presentation_slot_count: usize,
    pub presentation_slots_ready: usize,
    pub tracks_covered: usize,
    pub preroll_status: BroadcastPrerollStatus,
}

pub fn summarize_broadcast_prepared_slots(
    config: BroadcastPrerollConfig,
    plan: BroadcastPrerollPlan,
    video_slots: &[BroadcastPreparedVideoSlot],
    audio_slots: &[BroadcastPreparedAudioSlot],
    presentation_slots: &[BroadcastPreparedPresentationSlot],
) -> BroadcastPreparedSlotSummary {
    let video_slots_prepared = video_slots.iter().filter(|slot| slot.is_ready()).count();
    let audio_slots_complete = audio_slots.iter().filter(|slot| slot.complete).count();
    let presentation_slots_ready = presentation_slots.iter().filter(|slot| slot.ready).count();
    let preroll_status =
        evaluate_broadcast_preroll(config, plan, video_slots_prepared, audio_slots_complete);
    let mut covered_tracks = Vec::new();
    for slot in audio_slots.iter().filter(|slot| slot.complete) {
        for coverage in &slot.track_coverage {
            if coverage.complete && !covered_tracks.contains(&coverage.track_id) {
                covered_tracks.push(coverage.track_id);
            }
        }
    }

    BroadcastPreparedSlotSummary {
        source_mode: plan.video_source_mode,
        video_slot_capacity: config.max_video_queue,
        video_slot_count: video_slots.len(),
        video_slots_prepared,
        audio_slot_capacity: config.max_audio_queue,
        audio_slot_count: audio_slots.len(),
        audio_slots_complete,
        presentation_slot_capacity: config.max_presentation_queue,
        presentation_slot_count: presentation_slots.len(),
        presentation_slots_ready,
        tracks_covered: covered_tracks.len(),
        preroll_status: BroadcastPrerollStatus {
            ready: preroll_status.ready && presentation_slots_ready >= config.video_frames_required,
            ..preroll_status
        },
    }
}

pub fn build_broadcast_player_event_surface(
    session_index: u64,
    session: BroadcastRuntimeSessionDescription,
    preroll_config: BroadcastPrerollConfig,
    preroll_plan: BroadcastPrerollPlan,
    runtime_state: BroadcastRuntimeState,
    runtime_accounting: BroadcastRuntimeAccounting,
    runtime_events: &[BroadcastRuntimeEvent],
    prepared_summary: &BroadcastPreparedSlotSummary,
    video_slots: &[BroadcastPreparedVideoSlot],
    audio_slots: &[BroadcastPreparedAudioSlot],
    presentation_slots: &[BroadcastPreparedPresentationSlot],
) -> Vec<BroadcastPlayerRuntimeEvent> {
    let mut events = Vec::new();
    for event in runtime_events {
        match *event {
            BroadcastRuntimeEvent::SessionCreated => {
                events.push(BroadcastPlayerRuntimeEvent::SessionCreated {
                    session_index,
                    source_mode: session.video_source_mode,
                    preview_profile: session.preview_profile,
                    audio_source_role: session.audio_source,
                    video_source_role: session.video_source,
                })
            }
            BroadcastRuntimeEvent::PreparingStarted => {
                events.push(BroadcastPlayerRuntimeEvent::PrepareStarted {
                    source_mode: session.video_source_mode,
                    queue_limits: session.queue_limits,
                    preroll_config,
                });
                events.push(BroadcastPlayerRuntimeEvent::PrerollPlanned {
                    source_mode: preroll_plan.video_source_mode,
                    selected_video_frames: preroll_plan.selected_video_frames_planned,
                    audio_ranges: preroll_plan.audio_ranges_planned,
                    intentional_skips: preroll_plan.intentional_skips_planned,
                });
            }
            BroadcastRuntimeEvent::PlaybackStarted => {
                events.push(BroadcastPlayerRuntimeEvent::TransportStarted {
                    state: BroadcastRuntimeState::Playing,
                    media_time: Duration::ZERO,
                })
            }
            BroadcastRuntimeEvent::PlaybackPaused => {
                events.push(BroadcastPlayerRuntimeEvent::TransportPaused {
                    state: BroadcastRuntimeState::Paused,
                    media_time: Duration::ZERO,
                })
            }
            BroadcastRuntimeEvent::SeekCompleted { target_time } => {
                events.push(BroadcastPlayerRuntimeEvent::SeekCompleted { target_time })
            }
            BroadcastRuntimeEvent::IntentionalProfileSkip { source_frame_index } => {
                events.push(BroadcastPlayerRuntimeEvent::IntentionalProfileSkip {
                    source_frame_index,
                    profile: session.preview_profile,
                })
            }
            BroadcastRuntimeEvent::FrameAccounted { frame_index } => {
                let slot = presentation_slots.iter().find(|slot| {
                    slot.presentation_index
                        == usize::try_from(frame_index).ok().unwrap_or(usize::MAX)
                });
                events.push(BroadcastPlayerRuntimeEvent::FrameAccounted {
                    selected_preview_frame_index: frame_index,
                    source_frame_index: slot.and_then(|slot| slot.selected_source_frame),
                    presentation_time: slot
                        .map(|slot| slot.presentation_time)
                        .unwrap_or(Duration::ZERO),
                    duration: slot.map(|slot| slot.duration).unwrap_or(Duration::ZERO),
                    video_source_role: session.video_source,
                })
            }
            BroadcastRuntimeEvent::AudioRangeAccounted { frame_index } => {
                let slot = audio_slots.iter().find(|slot| {
                    slot.slot_index == usize::try_from(frame_index).ok().unwrap_or(usize::MAX)
                });
                events.push(BroadcastPlayerRuntimeEvent::AudioRangeAccounted {
                    presentation_index: frame_index,
                    start_sample: slot.map(|slot| slot.start_sample).unwrap_or(0),
                    sample_count: slot.map(|slot| slot.sample_count).unwrap_or(0),
                    tracks_covered: slot.map(|slot| slot.track_coverage.len()).unwrap_or(0),
                    complete: slot.map(|slot| slot.complete).unwrap_or(false),
                })
            }
            BroadcastRuntimeEvent::Completed => {
                events.push(BroadcastPlayerRuntimeEvent::RuntimeCompleted {
                    selected_frames_accounted: runtime_accounting.selected_frames_accounted,
                    audio_ranges_accounted: runtime_accounting.audio_ranges_accounted,
                    lateness_drops: runtime_accounting.lateness_drops,
                    intentional_skips: runtime_accounting.intentional_profile_skips,
                    final_state: runtime_state,
                })
            }
            BroadcastRuntimeEvent::Failed => {
                events.push(BroadcastPlayerRuntimeEvent::RuntimeFailed {
                    reason: BroadcastPlayerRuntimeFailureReason::InvalidTransition,
                })
            }
            BroadcastRuntimeEvent::Prepared
            | BroadcastRuntimeEvent::ContractPrepared
            | BroadcastRuntimeEvent::QueueBackpressure { .. }
            | BroadcastRuntimeEvent::IncompleteAudioCoverage { .. }
            | BroadcastRuntimeEvent::LatenessDrop { .. }
            | BroadcastRuntimeEvent::SimulatedPresentationDecision { .. }
            | BroadcastRuntimeEvent::DrainingStarted => {}
        }
    }

    if prepared_summary.preroll_status.ready {
        events.push(BroadcastPlayerRuntimeEvent::PrerollReady {
            prepared_video_slots: prepared_summary.video_slots_prepared,
            prepared_audio_slots: prepared_summary.audio_slots_complete,
            prepared_presentation_slots: prepared_summary.presentation_slots_ready,
        });
        for slot in video_slots.iter().filter(|slot| slot.is_ready()) {
            events.push(BroadcastPlayerRuntimeEvent::PreparedSlotAvailable {
                kind: BroadcastPlayerPreparedSlotKind::Video,
                slot_index: slot.slot_index,
                source_mode: slot.source_mode,
                ready: true,
                video_status: Some(slot.status),
            });
        }
        for slot in audio_slots.iter().filter(|slot| slot.complete) {
            events.push(BroadcastPlayerRuntimeEvent::PreparedSlotAvailable {
                kind: BroadcastPlayerPreparedSlotKind::Audio,
                slot_index: slot.slot_index,
                source_mode: slot.source_mode,
                ready: true,
                video_status: None,
            });
        }
        for slot in presentation_slots.iter().filter(|slot| slot.ready) {
            events.push(BroadcastPlayerRuntimeEvent::PreparedSlotAvailable {
                kind: BroadcastPlayerPreparedSlotKind::Presentation,
                slot_index: slot.presentation_index,
                source_mode: slot.source_mode,
                ready: slot.ready,
                video_status: None,
            });
        }
        events.push(BroadcastPlayerRuntimeEvent::RuntimeReady {
            state: BroadcastRuntimeState::Ready,
            source_mode: prepared_summary.source_mode,
        });
    } else if prepared_summary.preroll_status.reason
        == Some(BroadcastPrerollNotReadyReason::CapabilityMissing)
    {
        events.push(BroadcastPlayerRuntimeEvent::CapabilityMissing {
            source_mode: prepared_summary.source_mode,
            capability: BroadcastPlayerCapability::OriginalVideoRuntime,
            reason: BroadcastPrerollNotReadyReason::CapabilityMissing,
        });
    }

    events
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastPlayerRuntimeEventSummary {
    pub event_count: usize,
    pub prepared_slot_events: usize,
    pub frame_accounted_events: usize,
    pub audio_range_accounted_events: usize,
    pub intentional_skip_events: usize,
    pub lateness_drops: usize,
    pub runtime_ready: bool,
    pub runtime_completed: bool,
    pub capability_missing: bool,
    pub frame_presented_events: usize,
}

pub fn summarize_broadcast_player_runtime_events(
    events: &[BroadcastPlayerRuntimeEvent],
    runtime_accounting: BroadcastRuntimeAccounting,
) -> BroadcastPlayerRuntimeEventSummary {
    BroadcastPlayerRuntimeEventSummary {
        event_count: events.len(),
        prepared_slot_events: events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    BroadcastPlayerRuntimeEvent::PreparedSlotAvailable { .. }
                )
            })
            .count(),
        frame_accounted_events: events
            .iter()
            .filter(|event| matches!(event, BroadcastPlayerRuntimeEvent::FrameAccounted { .. }))
            .count(),
        audio_range_accounted_events: events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    BroadcastPlayerRuntimeEvent::AudioRangeAccounted { .. }
                )
            })
            .count(),
        intentional_skip_events: events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    BroadcastPlayerRuntimeEvent::IntentionalProfileSkip { .. }
                )
            })
            .count(),
        lateness_drops: runtime_accounting.lateness_drops,
        runtime_ready: events
            .iter()
            .any(|event| matches!(event, BroadcastPlayerRuntimeEvent::RuntimeReady { .. })),
        runtime_completed: events
            .iter()
            .any(|event| matches!(event, BroadcastPlayerRuntimeEvent::RuntimeCompleted { .. })),
        capability_missing: events
            .iter()
            .any(|event| matches!(event, BroadcastPlayerRuntimeEvent::CapabilityMissing { .. })),
        frame_presented_events: events
            .iter()
            .filter(|event| matches!(event, BroadcastPlayerRuntimeEvent::FramePresented { .. }))
            .count(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastAudioBlockCoverage {
    pub track_id: u32,
    pub channel_index: u16,
    pub block_index: usize,
    pub block_start_sample: u64,
    pub block_sample_count: u64,
    pub byte_offset_within_block: usize,
    pub byte_count: usize,
    pub full_block: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastAudioPayloadBinding {
    pub audio_slot_index: usize,
    pub source_mode: BroadcastVideoSourceMode,
    pub start_sample: u64,
    pub sample_count: u64,
    pub sample_rate: u32,
    pub track_count: usize,
    pub block_coverage: Vec<BroadcastAudioBlockCoverage>,
    pub total_referenced_payload_bytes: u64,
    pub complete: bool,
}

pub fn bind_broadcast_audio_payload(
    slot: &BroadcastPreparedAudioSlot,
    blocks: &[PcmAudioBlock],
) -> Result<BroadcastAudioPayloadBinding, PlaybackError> {
    if slot.audio_source_role != BroadcastMediaSourceRole::OriginalAuthoritativeAudio {
        return Err(PlaybackError::InvalidAudioFormat);
    }
    let first_block = blocks.first().ok_or(PlaybackError::InvalidAudioFormat)?;
    let sample_rate = first_block.sample_rate;
    let range_start = slot.start_sample;
    let range_end = slot
        .start_sample
        .checked_add(slot.sample_count)
        .ok_or(PlaybackError::TimestampOverflow)?;
    let mut block_coverage = Vec::new();
    let mut coverage_ranges: BTreeMap<(u32, u16), Vec<(u64, u64)>> = BTreeMap::new();
    let mut total_referenced_payload_bytes = 0_u64;

    for (block_index, block) in blocks.iter().enumerate() {
        if block.sample_rate != sample_rate {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        let (track_id, channel_index) = match block.layout {
            PcmAudioBlockLayout::MonoTrack {
                track_id,
                channel_index,
            } => (track_id, channel_index),
            PcmAudioBlockLayout::InterleavedChannels { .. } => {
                return Err(PlaybackError::InvalidAudioFormat)
            }
        };
        if !slot.track_coverage.iter().any(|coverage| {
            coverage.track_id == track_id && coverage.channel_index == channel_index
        }) {
            continue;
        }
        let block_start = audio_samples_for_duration(block.start_time, sample_rate)?;
        let block_sample_count = u64::from(block.sample_count);
        let block_end = block_start
            .checked_add(block_sample_count)
            .ok_or(PlaybackError::TimestampOverflow)?;
        let overlap_start = range_start.max(block_start);
        let overlap_end = range_end.min(block_end);
        if overlap_start >= overlap_end {
            continue;
        }
        let overlap_samples = overlap_end - overlap_start;
        let sample_offset = overlap_start - block_start;
        let channel_count = block.layout.channel_count()?;
        let overlap_samples_u32 =
            u32::try_from(overlap_samples).map_err(|_| PlaybackError::TimestampOverflow)?;
        let sample_offset_u32 =
            u32::try_from(sample_offset).map_err(|_| PlaybackError::TimestampOverflow)?;
        let byte_count = pcm_payload_byte_len(overlap_samples_u32, channel_count, block.format)?;
        let byte_offset_within_block =
            pcm_payload_byte_len(sample_offset_u32, channel_count, block.format)?;
        total_referenced_payload_bytes = total_referenced_payload_bytes
            .checked_add(u64::try_from(byte_count).map_err(|_| PlaybackError::TimestampOverflow)?)
            .ok_or(PlaybackError::TimestampOverflow)?;
        coverage_ranges
            .entry((track_id, channel_index))
            .or_default()
            .push((overlap_start, overlap_end));
        block_coverage.push(BroadcastAudioBlockCoverage {
            track_id,
            channel_index,
            block_index,
            block_start_sample: block_start,
            block_sample_count,
            byte_offset_within_block,
            byte_count,
            full_block: overlap_start == block_start && overlap_end == block_end,
        });
    }

    let complete = slot.complete
        && slot.track_coverage.iter().all(|coverage| {
            coverage.complete
                && coverage_ranges
                    .get(&(coverage.track_id, coverage.channel_index))
                    .map(|ranges| sample_range_is_covered(ranges, range_start, range_end))
                    .unwrap_or(false)
        });

    Ok(BroadcastAudioPayloadBinding {
        audio_slot_index: slot.slot_index,
        source_mode: slot.source_mode,
        start_sample: slot.start_sample,
        sample_count: slot.sample_count,
        sample_rate,
        track_count: slot.track_coverage.len(),
        block_coverage,
        total_referenced_payload_bytes,
        complete,
    })
}

fn sample_range_is_covered(ranges: &[(u64, u64)], range_start: u64, range_end: u64) -> bool {
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable_by_key(|(start, end)| (*start, *end));
    let mut cursor = range_start;
    for (start, end) in sorted {
        if start > cursor {
            return false;
        }
        if end > cursor {
            cursor = end;
            if cursor >= range_end {
                return true;
            }
        }
    }
    cursor >= range_end
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastVideoPayloadBindingStatus {
    AccountedOnly,
    PayloadReady,
    CapabilityMissing,
    NotSupported,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastVideoPayloadKind {
    DecodedVaSurface,
    CpuNv12Surface,
    ProcessedGpuFrame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastVideoPayloadFormat {
    Nv12,
    RgbaU16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastVideoPayloadBackendPath {
    VaapiCpuNv12Vulkan,
    SoftwareH264Yuv422P10Vulkan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastVideoPayloadReference {
    pub payload_id: u64,
    pub kind: BroadcastVideoPayloadKind,
    pub format: BroadcastVideoPayloadFormat,
    pub backend_path: BroadcastVideoPayloadBackendPath,
    pub source_frame_index: u64,
    pub selected_preview_frame_index: Option<u64>,
    pub presentation_time: Duration,
    pub duration: Duration,
    pub coded_width: u32,
    pub coded_height: u32,
    pub visible_width: u32,
    pub visible_height: u32,
    pub bounded_slot_index: usize,
    pub session_index: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastVideoPayloadBinding {
    pub video_slot_index: usize,
    pub source_mode: BroadcastVideoSourceMode,
    pub video_source_role: BroadcastMediaSourceRole,
    pub source_frame_index: Option<u64>,
    pub selected_preview_frame_index: Option<u64>,
    pub status: BroadcastVideoPayloadBindingStatus,
    pub payload_id: Option<u64>,
    pub payload: Option<BroadcastVideoPayloadReference>,
}

pub fn bind_broadcast_video_payload_accounting(
    slot: &BroadcastPreparedVideoSlot,
) -> BroadcastVideoPayloadBinding {
    let status = match slot.status {
        BroadcastPreparedVideoSlotStatus::Prepared => {
            if slot.source_mode == BroadcastVideoSourceMode::ProxyPreview {
                BroadcastVideoPayloadBindingStatus::AccountedOnly
            } else {
                BroadcastVideoPayloadBindingStatus::PayloadReady
            }
        }
        BroadcastPreparedVideoSlotStatus::CapabilityMissing => {
            BroadcastVideoPayloadBindingStatus::CapabilityMissing
        }
        BroadcastPreparedVideoSlotStatus::NotSupported => {
            BroadcastVideoPayloadBindingStatus::NotSupported
        }
        BroadcastPreparedVideoSlotStatus::Missing
        | BroadcastPreparedVideoSlotStatus::IntentionalSkip => {
            BroadcastVideoPayloadBindingStatus::Missing
        }
    };
    BroadcastVideoPayloadBinding {
        video_slot_index: slot.slot_index,
        source_mode: slot.source_mode,
        video_source_role: slot.video_source_role,
        source_frame_index: slot.source_frame_index,
        selected_preview_frame_index: slot.selected_preview_frame_index,
        status,
        payload_id: None,
        payload: None,
    }
}

pub fn bind_broadcast_video_payload_ready(
    slot: &BroadcastPreparedVideoSlot,
    payload: BroadcastVideoPayloadReference,
) -> Result<BroadcastVideoPayloadBinding, PlaybackError> {
    if slot.status != BroadcastPreparedVideoSlotStatus::Prepared {
        return Err(PlaybackError::InvalidRuntimeTransition);
    }
    let expected_role = match slot.source_mode {
        BroadcastVideoSourceMode::ProxyPreview => BroadcastMediaSourceRole::ProxyPreviewVideo,
        BroadcastVideoSourceMode::OriginalMedia => BroadcastMediaSourceRole::OriginalFinishingMedia,
    };
    if slot.video_source_role != expected_role {
        return Err(PlaybackError::InvalidRuntimeTransition);
    }
    let expected_backend = match slot.source_mode {
        BroadcastVideoSourceMode::ProxyPreview => {
            BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan
        }
        BroadcastVideoSourceMode::OriginalMedia => {
            BroadcastVideoPayloadBackendPath::SoftwareH264Yuv422P10Vulkan
        }
    };
    if slot.source_frame_index != Some(payload.source_frame_index)
        || slot.selected_preview_frame_index != payload.selected_preview_frame_index
        || slot.presentation_time != payload.presentation_time
        || slot.duration != payload.duration
        || payload.backend_path != expected_backend
    {
        return Err(PlaybackError::InvalidRuntimeTransition);
    }
    Ok(BroadcastVideoPayloadBinding {
        video_slot_index: slot.slot_index,
        source_mode: slot.source_mode,
        video_source_role: slot.video_source_role,
        source_frame_index: slot.source_frame_index,
        selected_preview_frame_index: slot.selected_preview_frame_index,
        status: BroadcastVideoPayloadBindingStatus::PayloadReady,
        payload_id: Some(payload.payload_id),
        payload: Some(payload),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPresentationPayloadReadiness {
    RuntimeAccountingReady,
    PayloadReady,
    DevicePayloadReady,
    CapabilityMissing,
    NotReady,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastPresentationPayloadBinding {
    pub presentation_slot_index: usize,
    pub video_binding_index: usize,
    pub audio_binding_index: usize,
    pub presentation_time: Duration,
    pub duration: Duration,
    pub ready: bool,
    pub readiness: BroadcastPresentationPayloadReadiness,
}

pub fn bind_broadcast_presentation_payload(
    slot: &BroadcastPreparedPresentationSlot,
    video_binding: &BroadcastVideoPayloadBinding,
    audio_binding: &BroadcastAudioPayloadBinding,
) -> BroadcastPresentationPayloadBinding {
    let readiness = match (video_binding.status, audio_binding.complete) {
        (BroadcastVideoPayloadBindingStatus::PayloadReady, true) => {
            BroadcastPresentationPayloadReadiness::PayloadReady
        }
        (BroadcastVideoPayloadBindingStatus::AccountedOnly, true) => {
            BroadcastPresentationPayloadReadiness::RuntimeAccountingReady
        }
        (BroadcastVideoPayloadBindingStatus::CapabilityMissing, _) => {
            BroadcastPresentationPayloadReadiness::CapabilityMissing
        }
        _ => BroadcastPresentationPayloadReadiness::NotReady,
    };
    BroadcastPresentationPayloadBinding {
        presentation_slot_index: slot.presentation_index,
        video_binding_index: video_binding.video_slot_index,
        audio_binding_index: audio_binding.audio_slot_index,
        presentation_time: slot.presentation_time,
        duration: slot.duration,
        ready: matches!(
            readiness,
            BroadcastPresentationPayloadReadiness::RuntimeAccountingReady
                | BroadcastPresentationPayloadReadiness::PayloadReady
                | BroadcastPresentationPayloadReadiness::DevicePayloadReady
        ),
        readiness,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastPayloadBindingSummary {
    pub source_mode: BroadcastVideoSourceMode,
    pub audio_binding_capacity: usize,
    pub audio_binding_count: usize,
    pub video_binding_capacity: usize,
    pub video_binding_count: usize,
    pub presentation_binding_capacity: usize,
    pub presentation_binding_count: usize,
    pub complete_audio_bindings: usize,
    pub runtime_accounting_ready_presentations: usize,
    pub payload_ready_presentations: usize,
    pub device_payload_ready_presentations: usize,
    pub capability_missing_presentations: usize,
    pub total_referenced_audio_bytes: u64,
}

pub fn summarize_broadcast_payload_bindings(
    source_mode: BroadcastVideoSourceMode,
    config: BroadcastPrerollConfig,
    audio_bindings: &[BroadcastAudioPayloadBinding],
    video_bindings: &[BroadcastVideoPayloadBinding],
    presentation_bindings: &[BroadcastPresentationPayloadBinding],
) -> BroadcastPayloadBindingSummary {
    BroadcastPayloadBindingSummary {
        source_mode,
        audio_binding_capacity: config.max_audio_queue,
        audio_binding_count: audio_bindings.len(),
        video_binding_capacity: config.max_video_queue,
        video_binding_count: video_bindings.len(),
        presentation_binding_capacity: config.max_presentation_queue,
        presentation_binding_count: presentation_bindings.len(),
        complete_audio_bindings: audio_bindings
            .iter()
            .filter(|binding| binding.complete)
            .count(),
        runtime_accounting_ready_presentations: presentation_bindings
            .iter()
            .filter(|binding| {
                binding.readiness == BroadcastPresentationPayloadReadiness::RuntimeAccountingReady
            })
            .count(),
        payload_ready_presentations: presentation_bindings
            .iter()
            .filter(|binding| {
                binding.readiness == BroadcastPresentationPayloadReadiness::PayloadReady
            })
            .count(),
        device_payload_ready_presentations: presentation_bindings
            .iter()
            .filter(|binding| {
                binding.readiness == BroadcastPresentationPayloadReadiness::DevicePayloadReady
            })
            .count(),
        capability_missing_presentations: presentation_bindings
            .iter()
            .filter(|binding| {
                binding.readiness == BroadcastPresentationPayloadReadiness::CapabilityMissing
            })
            .count(),
        total_referenced_audio_bytes: audio_bindings
            .iter()
            .map(|binding| binding.total_referenced_payload_bytes)
            .sum(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastDeviceKind {
    AudioSink,
    VideoPresenter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastDeviceStatus {
    NotConfigured,
    CapabilityMissing,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastDeviceCapability {
    AcceptsOriginalPcm,
    AcceptsF32Pcm,
    AcceptsSignedIntegerPcm,
    Accepts24BitPcm,
    Accepts48000Hz,
    AcceptsMonoTrackBlocks,
    AcceptsProcessedGpuFrame,
    AcceptsCpuImage,
    ProvidesPresentationEvidence,
    ProvidesAudioSinkEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastDevicePayloadStatus {
    PayloadReady,
    DevicePayloadReady,
    DeviceNotConfigured,
    DeviceCapabilityMissing,
    SubmittedToDevice,
    PresentationEvidenceReceived,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastAudioDeviceSubmission {
    pub audio_binding_index: usize,
    pub start_sample: u64,
    pub sample_count: u64,
    pub sample_rate: u32,
    pub track_count: usize,
    pub format: PcmSampleFormat,
    pub status: BroadcastDevicePayloadStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastVideoPresenterSubmission {
    pub video_binding_index: usize,
    pub payload_kind: Option<BroadcastVideoPayloadKind>,
    pub coded_width: Option<u32>,
    pub coded_height: Option<u32>,
    pub visible_width: Option<u32>,
    pub visible_height: Option<u32>,
    pub format: Option<BroadcastVideoPayloadFormat>,
    pub status: BroadcastDevicePayloadStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastPresentationEvidenceKind {
    TestAudioSinkAccepted,
    TestPresenterAccepted,
    VideoFramePresented,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastPresentationEvidence {
    pub presentation_slot_index: usize,
    pub audio_binding_index: Option<usize>,
    pub video_binding_index: Option<usize>,
    pub media_time: Duration,
    pub evidence_kind: BroadcastPresentationEvidenceKind,
    pub source_device_kind: BroadcastDeviceKind,
    pub payload_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastAudioSinkEvidence {
    pub presentation_slot_index: usize,
    pub audio_binding_index: usize,
    pub start_sample: u64,
    pub sample_count: u64,
    pub sample_rate: u32,
    pub track_count: usize,
    pub payload_bytes_accepted: u64,
    pub evidence_kind: BroadcastPresentationEvidenceKind,
    pub source_device_kind: BroadcastDeviceKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadcastRuntimeSimulationFailureReason {
    MissingAudioBinding,
    MissingVideoBinding,
    AudioSinkRejected,
    VideoPresenterRejected,
    PresentationNotPayloadReady,
    CapabilityMissing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeSimulationFailure {
    pub presentation_slot_index: usize,
    pub reason: BroadcastRuntimeSimulationFailureReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeSimulationSummary {
    pub source_mode: BroadcastVideoSourceMode,
    pub presentation_slots_attempted: usize,
    pub audio_submissions: usize,
    pub audio_accepted: usize,
    pub video_submissions: usize,
    pub video_accepted: usize,
    pub audio_evidence_count: usize,
    pub video_evidence_count: usize,
    pub frame_presented_count: usize,
    pub test_evidence_count: usize,
    pub lateness_drops: usize,
    pub failed_slots: usize,
    pub final_state: BroadcastRuntimeState,
    pub completed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeSimulationResult {
    pub summary: BroadcastRuntimeSimulationSummary,
    pub events: Vec<BroadcastPlayerRuntimeEvent>,
    pub audio_evidence: Vec<BroadcastAudioSinkEvidence>,
    pub video_evidence: Vec<BroadcastPresentationEvidence>,
    pub failures: Vec<BroadcastRuntimeSimulationFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastDeviceBoundarySummary {
    pub audio_payload_ready: bool,
    pub video_payload_ready: bool,
    pub audio_device_status: BroadcastDeviceStatus,
    pub video_presenter_status: BroadcastDeviceStatus,
    pub audio_submission_status: BroadcastDevicePayloadStatus,
    pub video_submission_status: BroadcastDevicePayloadStatus,
    pub device_payload_ready: bool,
    pub frame_presented_count: usize,
    pub reason: Option<&'static str>,
}

pub fn build_broadcast_audio_device_submission(
    binding_index: usize,
    binding: &BroadcastAudioPayloadBinding,
    format: PcmSampleFormat,
    device_status: BroadcastDeviceStatus,
    capabilities: &[BroadcastDeviceCapability],
) -> BroadcastAudioDeviceSubmission {
    let status = device_payload_status_for_audio(binding.complete, device_status, capabilities);
    BroadcastAudioDeviceSubmission {
        audio_binding_index: binding_index,
        start_sample: binding.start_sample,
        sample_count: binding.sample_count,
        sample_rate: binding.sample_rate,
        track_count: binding.track_count,
        format,
        status,
    }
}

pub fn build_broadcast_video_presenter_submission(
    binding_index: usize,
    binding: &BroadcastVideoPayloadBinding,
    device_status: BroadcastDeviceStatus,
    capabilities: &[BroadcastDeviceCapability],
) -> BroadcastVideoPresenterSubmission {
    let payload = binding.payload.as_ref();
    let status = device_payload_status_for_video(binding.status, device_status, capabilities);
    BroadcastVideoPresenterSubmission {
        video_binding_index: binding_index,
        payload_kind: payload.map(|payload| payload.kind),
        coded_width: payload.map(|payload| payload.coded_width),
        coded_height: payload.map(|payload| payload.coded_height),
        visible_width: payload.map(|payload| payload.visible_width),
        visible_height: payload.map(|payload| payload.visible_height),
        format: payload.map(|payload| payload.format),
        status,
    }
}

pub fn summarize_broadcast_device_boundary(
    audio_submission: &BroadcastAudioDeviceSubmission,
    video_submission: &BroadcastVideoPresenterSubmission,
    evidence: &[BroadcastPresentationEvidence],
    audio_device_status: BroadcastDeviceStatus,
    video_presenter_status: BroadcastDeviceStatus,
) -> BroadcastDeviceBoundarySummary {
    let audio_payload_ready = audio_submission.status != BroadcastDevicePayloadStatus::Failed
        && !matches!(
            audio_submission.status,
            BroadcastDevicePayloadStatus::DeviceCapabilityMissing
        );
    let video_payload_ready = video_submission.payload_kind.is_some()
        && !matches!(
            video_submission.status,
            BroadcastDevicePayloadStatus::DeviceCapabilityMissing
                | BroadcastDevicePayloadStatus::Failed
        );
    let device_payload_ready = audio_submission.status
        == BroadcastDevicePayloadStatus::DevicePayloadReady
        && video_submission.status == BroadcastDevicePayloadStatus::DevicePayloadReady;
    let frame_presented_count = evidence
        .iter()
        .filter(|event| {
            matches!(
                event.evidence_kind,
                BroadcastPresentationEvidenceKind::TestPresenterAccepted
                    | BroadcastPresentationEvidenceKind::VideoFramePresented
            ) && event.source_device_kind == BroadcastDeviceKind::VideoPresenter
        })
        .count();
    let reason = if audio_device_status == BroadcastDeviceStatus::NotConfigured
        || video_presenter_status == BroadcastDeviceStatus::NotConfigured
    {
        Some("device boundary not configured")
    } else if audio_submission.status == BroadcastDevicePayloadStatus::DeviceCapabilityMissing
        || video_submission.status == BroadcastDevicePayloadStatus::DeviceCapabilityMissing
    {
        Some("device capability missing")
    } else if !device_payload_ready {
        Some("payload not accepted by device boundary")
    } else {
        None
    };
    BroadcastDeviceBoundarySummary {
        audio_payload_ready,
        video_payload_ready,
        audio_device_status,
        video_presenter_status,
        audio_submission_status: audio_submission.status,
        video_submission_status: video_submission.status,
        device_payload_ready,
        frame_presented_count,
        reason,
    }
}

fn device_payload_status_for_audio(
    payload_ready: bool,
    device_status: BroadcastDeviceStatus,
    capabilities: &[BroadcastDeviceCapability],
) -> BroadcastDevicePayloadStatus {
    if !payload_ready {
        return BroadcastDevicePayloadStatus::Failed;
    }
    match device_status {
        BroadcastDeviceStatus::NotConfigured => BroadcastDevicePayloadStatus::DeviceNotConfigured,
        BroadcastDeviceStatus::CapabilityMissing => {
            BroadcastDevicePayloadStatus::DeviceCapabilityMissing
        }
        BroadcastDeviceStatus::Failed => BroadcastDevicePayloadStatus::Failed,
        BroadcastDeviceStatus::Ready => {
            if capabilities.contains(&BroadcastDeviceCapability::AcceptsOriginalPcm) {
                BroadcastDevicePayloadStatus::DevicePayloadReady
            } else {
                BroadcastDevicePayloadStatus::DeviceCapabilityMissing
            }
        }
    }
}

fn device_payload_status_for_video(
    payload_status: BroadcastVideoPayloadBindingStatus,
    device_status: BroadcastDeviceStatus,
    capabilities: &[BroadcastDeviceCapability],
) -> BroadcastDevicePayloadStatus {
    if payload_status != BroadcastVideoPayloadBindingStatus::PayloadReady {
        return match payload_status {
            BroadcastVideoPayloadBindingStatus::CapabilityMissing => {
                BroadcastDevicePayloadStatus::DeviceCapabilityMissing
            }
            _ => BroadcastDevicePayloadStatus::Failed,
        };
    }
    match device_status {
        BroadcastDeviceStatus::NotConfigured => BroadcastDevicePayloadStatus::DeviceNotConfigured,
        BroadcastDeviceStatus::CapabilityMissing => {
            BroadcastDevicePayloadStatus::DeviceCapabilityMissing
        }
        BroadcastDeviceStatus::Failed => BroadcastDevicePayloadStatus::Failed,
        BroadcastDeviceStatus::Ready => {
            if capabilities.contains(&BroadcastDeviceCapability::AcceptsProcessedGpuFrame) {
                BroadcastDevicePayloadStatus::DevicePayloadReady
            } else {
                BroadcastDevicePayloadStatus::DeviceCapabilityMissing
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastTestAudioSinkConfig {
    pub sample_rate: u32,
    pub bits_per_sample: u8,
    pub track_count: usize,
}

impl BroadcastTestAudioSinkConfig {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        if self.sample_rate == 0 || self.bits_per_sample == 0 || self.track_count == 0 {
            return Err(PlaybackError::InvalidAudioFormat);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastTestAudioSink {
    config: BroadcastTestAudioSinkConfig,
    accepted: usize,
    rejected: usize,
    samples_accepted: u64,
    bytes_accepted: u64,
}

impl BroadcastTestAudioSink {
    pub fn new(config: BroadcastTestAudioSinkConfig) -> Result<Self, PlaybackError> {
        Ok(Self {
            config: config.validate()?,
            accepted: 0,
            rejected: 0,
            samples_accepted: 0,
            bytes_accepted: 0,
        })
    }

    pub fn capabilities(&self) -> [BroadcastDeviceCapability; 6] {
        [
            BroadcastDeviceCapability::AcceptsOriginalPcm,
            BroadcastDeviceCapability::AcceptsSignedIntegerPcm,
            BroadcastDeviceCapability::Accepts24BitPcm,
            BroadcastDeviceCapability::Accepts48000Hz,
            BroadcastDeviceCapability::AcceptsMonoTrackBlocks,
            BroadcastDeviceCapability::ProvidesAudioSinkEvidence,
        ]
    }

    pub fn accepted_count(&self) -> usize {
        self.accepted
    }

    pub fn rejected_count(&self) -> usize {
        self.rejected
    }

    pub fn samples_accepted(&self) -> u64 {
        self.samples_accepted
    }

    pub fn bytes_accepted(&self) -> u64 {
        self.bytes_accepted
    }

    pub fn submit(
        &mut self,
        presentation_slot: &BroadcastPresentationPayloadBinding,
        binding: &BroadcastAudioPayloadBinding,
        format: PcmSampleFormat,
    ) -> Result<BroadcastAudioSinkEvidence, PlaybackError> {
        let PcmSampleFormat::SignedInteger {
            bits_per_sample, ..
        } = format;
        let expected_bytes = u64::try_from(pcm_payload_byte_len(
            u32::try_from(binding.sample_count).map_err(|_| PlaybackError::TimestampOverflow)?,
            u16::try_from(binding.track_count).map_err(|_| PlaybackError::TimestampOverflow)?,
            format,
        )?)
        .map_err(|_| PlaybackError::TimestampOverflow)?;
        if !binding.complete
            || binding.sample_rate != self.config.sample_rate
            || bits_per_sample != self.config.bits_per_sample
            || binding.track_count != self.config.track_count
            || binding.total_referenced_payload_bytes != expected_bytes
        {
            self.rejected = self.rejected.saturating_add(1);
            return Err(PlaybackError::InvalidAudioFormat);
        }
        self.accepted = self.accepted.saturating_add(1);
        self.samples_accepted = self.samples_accepted.saturating_add(binding.sample_count);
        self.bytes_accepted = self
            .bytes_accepted
            .saturating_add(binding.total_referenced_payload_bytes);
        Ok(BroadcastAudioSinkEvidence {
            presentation_slot_index: presentation_slot.presentation_slot_index,
            audio_binding_index: binding.audio_slot_index,
            start_sample: binding.start_sample,
            sample_count: binding.sample_count,
            sample_rate: binding.sample_rate,
            track_count: binding.track_count,
            payload_bytes_accepted: binding.total_referenced_payload_bytes,
            evidence_kind: BroadcastPresentationEvidenceKind::TestAudioSinkAccepted,
            source_device_kind: BroadcastDeviceKind::AudioSink,
        })
    }
}

pub fn broadcast_player_events_from_audio_sink_evidence(
    evidence: &[BroadcastAudioSinkEvidence],
) -> Vec<BroadcastPlayerRuntimeEvent> {
    let mut events = Vec::with_capacity(evidence.len());
    for item in evidence {
        events.push(BroadcastPlayerRuntimeEvent::PresentationEvidenceReceived {
            presentation_slot_index: item.presentation_slot_index,
            media_time: duration_from_audio_samples(item.start_sample, item.sample_rate)
                .unwrap_or(Duration::ZERO),
            evidence_kind: item.evidence_kind,
            source_device_kind: item.source_device_kind,
            payload_id: None,
        });
    }
    events
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastTestVideoPresenterConfig {
    pub accepted_kind: BroadcastVideoPayloadKind,
    pub accepted_format: BroadcastVideoPayloadFormat,
    pub visible_width: u32,
    pub visible_height: u32,
    pub coded_width: u32,
    pub coded_height: u32,
}

impl BroadcastTestVideoPresenterConfig {
    pub fn validate(self) -> Result<Self, PlaybackError> {
        if self.visible_width == 0
            || self.visible_height == 0
            || self.coded_width < self.visible_width
            || self.coded_height < self.visible_height
        {
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastTestVideoPresenter {
    config: BroadcastTestVideoPresenterConfig,
    accepted: usize,
    rejected: usize,
}

impl BroadcastTestVideoPresenter {
    pub fn new(config: BroadcastTestVideoPresenterConfig) -> Result<Self, PlaybackError> {
        Ok(Self {
            config: config.validate()?,
            accepted: 0,
            rejected: 0,
        })
    }

    pub fn capabilities(&self) -> [BroadcastDeviceCapability; 2] {
        [
            BroadcastDeviceCapability::AcceptsProcessedGpuFrame,
            BroadcastDeviceCapability::ProvidesPresentationEvidence,
        ]
    }

    pub fn accepted_count(&self) -> usize {
        self.accepted
    }

    pub fn rejected_count(&self) -> usize {
        self.rejected
    }

    pub fn submit(
        &mut self,
        presentation_slot: &BroadcastPresentationPayloadBinding,
        video_binding: &BroadcastVideoPayloadBinding,
    ) -> Result<BroadcastPresentationEvidence, PlaybackError> {
        let Some(payload) = video_binding.payload.as_ref() else {
            self.rejected = self.rejected.saturating_add(1);
            return Err(PlaybackError::InvalidRuntimeTransition);
        };
        if video_binding.status != BroadcastVideoPayloadBindingStatus::PayloadReady
            || payload.kind != self.config.accepted_kind
            || payload.format != self.config.accepted_format
            || payload.visible_width != self.config.visible_width
            || payload.visible_height != self.config.visible_height
            || payload.coded_width != self.config.coded_width
            || payload.coded_height != self.config.coded_height
        {
            self.rejected = self.rejected.saturating_add(1);
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        self.accepted = self.accepted.saturating_add(1);
        Ok(BroadcastPresentationEvidence {
            presentation_slot_index: presentation_slot.presentation_slot_index,
            audio_binding_index: None,
            video_binding_index: Some(video_binding.video_slot_index),
            media_time: presentation_slot.presentation_time,
            evidence_kind: BroadcastPresentationEvidenceKind::TestPresenterAccepted,
            source_device_kind: BroadcastDeviceKind::VideoPresenter,
            payload_id: video_binding.payload_id,
        })
    }
}

pub fn broadcast_player_events_from_presentation_evidence(
    evidence: &[BroadcastPresentationEvidence],
) -> Vec<BroadcastPlayerRuntimeEvent> {
    let mut events = Vec::with_capacity(evidence.len() * 2);
    for item in evidence {
        events.push(BroadcastPlayerRuntimeEvent::PresentationEvidenceReceived {
            presentation_slot_index: item.presentation_slot_index,
            media_time: item.media_time,
            evidence_kind: item.evidence_kind,
            source_device_kind: item.source_device_kind,
            payload_id: item.payload_id,
        });
        if item.source_device_kind == BroadcastDeviceKind::VideoPresenter {
            events.push(BroadcastPlayerRuntimeEvent::FramePresented {
                presentation_slot_index: item.presentation_slot_index,
                media_time: item.media_time,
                evidence_kind: item.evidence_kind,
                payload_id: item.payload_id,
            });
        }
    }
    events
}

pub fn simulate_broadcast_player_runtime_loop(
    source_mode: BroadcastVideoSourceMode,
    presentation_bindings: &[BroadcastPresentationPayloadBinding],
    audio_bindings: &[BroadcastAudioPayloadBinding],
    video_bindings: &[BroadcastVideoPayloadBinding],
    audio_format: PcmSampleFormat,
    audio_sink: &mut BroadcastTestAudioSink,
    video_presenter: &mut BroadcastTestVideoPresenter,
) -> BroadcastRuntimeSimulationResult {
    let mut events = Vec::new();
    let mut audio_evidence = Vec::new();
    let mut video_evidence = Vec::new();
    let mut failures = Vec::new();
    let mut presentation_slots_attempted = 0_usize;
    let mut audio_submissions = 0_usize;
    let mut audio_accepted = 0_usize;
    let mut video_submissions = 0_usize;
    let mut video_accepted = 0_usize;

    events.push(BroadcastPlayerRuntimeEvent::TransportStarted {
        state: BroadcastRuntimeState::Playing,
        media_time: Duration::ZERO,
    });

    for presentation in presentation_bindings {
        presentation_slots_attempted = presentation_slots_attempted.saturating_add(1);
        if presentation.readiness != BroadcastPresentationPayloadReadiness::PayloadReady {
            let reason = if presentation.readiness
                == BroadcastPresentationPayloadReadiness::CapabilityMissing
            {
                BroadcastRuntimeSimulationFailureReason::CapabilityMissing
            } else {
                BroadcastRuntimeSimulationFailureReason::PresentationNotPayloadReady
            };
            failures.push(BroadcastRuntimeSimulationFailure {
                presentation_slot_index: presentation.presentation_slot_index,
                reason,
            });
            break;
        }

        let Some(audio_binding) = audio_bindings
            .iter()
            .find(|binding| binding.audio_slot_index == presentation.audio_binding_index)
        else {
            failures.push(BroadcastRuntimeSimulationFailure {
                presentation_slot_index: presentation.presentation_slot_index,
                reason: BroadcastRuntimeSimulationFailureReason::MissingAudioBinding,
            });
            break;
        };
        let Some(video_binding) = video_bindings
            .iter()
            .find(|binding| binding.video_slot_index == presentation.video_binding_index)
        else {
            failures.push(BroadcastRuntimeSimulationFailure {
                presentation_slot_index: presentation.presentation_slot_index,
                reason: BroadcastRuntimeSimulationFailureReason::MissingVideoBinding,
            });
            break;
        };

        audio_submissions = audio_submissions.saturating_add(1);
        events.push(BroadcastPlayerRuntimeEvent::PayloadSubmittedToDevice {
            device_kind: BroadcastDeviceKind::AudioSink,
            presentation_slot_index: presentation.presentation_slot_index,
            binding_index: audio_binding.audio_slot_index,
            status: BroadcastDevicePayloadStatus::SubmittedToDevice,
        });
        match audio_sink.submit(presentation, audio_binding, audio_format) {
            Ok(evidence) => {
                audio_accepted = audio_accepted.saturating_add(1);
                events.extend(broadcast_player_events_from_audio_sink_evidence(&[
                    evidence.clone(),
                ]));
                events.push(BroadcastPlayerRuntimeEvent::AudioRangeAccounted {
                    presentation_index: u64::try_from(presentation.presentation_slot_index)
                        .unwrap_or(u64::MAX),
                    start_sample: evidence.start_sample,
                    sample_count: evidence.sample_count,
                    tracks_covered: evidence.track_count,
                    complete: true,
                });
                audio_evidence.push(evidence);
            }
            Err(_) => {
                failures.push(BroadcastRuntimeSimulationFailure {
                    presentation_slot_index: presentation.presentation_slot_index,
                    reason: BroadcastRuntimeSimulationFailureReason::AudioSinkRejected,
                });
                break;
            }
        }

        video_submissions = video_submissions.saturating_add(1);
        events.push(BroadcastPlayerRuntimeEvent::PayloadSubmittedToDevice {
            device_kind: BroadcastDeviceKind::VideoPresenter,
            presentation_slot_index: presentation.presentation_slot_index,
            binding_index: video_binding.video_slot_index,
            status: BroadcastDevicePayloadStatus::SubmittedToDevice,
        });
        match video_presenter.submit(presentation, video_binding) {
            Ok(evidence) => {
                video_accepted = video_accepted.saturating_add(1);
                events.extend(broadcast_player_events_from_presentation_evidence(&[
                    evidence.clone(),
                ]));
                events.push(BroadcastPlayerRuntimeEvent::FrameAccounted {
                    selected_preview_frame_index: video_binding
                        .selected_preview_frame_index
                        .unwrap_or(u64::MAX),
                    source_frame_index: video_binding.source_frame_index,
                    presentation_time: presentation.presentation_time,
                    duration: presentation.duration,
                    video_source_role: video_binding.video_source_role,
                });
                video_evidence.push(evidence);
            }
            Err(_) => {
                failures.push(BroadcastRuntimeSimulationFailure {
                    presentation_slot_index: presentation.presentation_slot_index,
                    reason: BroadcastRuntimeSimulationFailureReason::VideoPresenterRejected,
                });
                break;
            }
        }
    }

    let completed = failures.is_empty()
        && presentation_slots_attempted == presentation_bindings.len()
        && audio_accepted == presentation_bindings.len()
        && video_accepted == presentation_bindings.len();
    let final_state = if completed {
        BroadcastRuntimeState::Completed
    } else {
        BroadcastRuntimeState::Failed
    };
    let frame_presented_count = events
        .iter()
        .filter(|event| matches!(event, BroadcastPlayerRuntimeEvent::FramePresented { .. }))
        .count();
    if completed {
        events.push(BroadcastPlayerRuntimeEvent::RuntimeCompleted {
            selected_frames_accounted: video_accepted,
            audio_ranges_accounted: audio_accepted,
            lateness_drops: 0,
            intentional_skips: 0,
            final_state,
        });
    } else {
        events.push(BroadcastPlayerRuntimeEvent::RuntimeFailed {
            reason: failures
                .first()
                .map(|failure| match failure.reason {
                    BroadcastRuntimeSimulationFailureReason::CapabilityMissing => {
                        BroadcastPlayerRuntimeFailureReason::CapabilityMissing
                    }
                    BroadcastRuntimeSimulationFailureReason::AudioSinkRejected
                    | BroadcastRuntimeSimulationFailureReason::MissingAudioBinding => {
                        BroadcastPlayerRuntimeFailureReason::IncompleteAudioCoverage
                    }
                    BroadcastRuntimeSimulationFailureReason::MissingVideoBinding
                    | BroadcastRuntimeSimulationFailureReason::VideoPresenterRejected
                    | BroadcastRuntimeSimulationFailureReason::PresentationNotPayloadReady => {
                        BroadcastPlayerRuntimeFailureReason::InvalidTransition
                    }
                })
                .unwrap_or(BroadcastPlayerRuntimeFailureReason::InvalidTransition),
        });
    }

    BroadcastRuntimeSimulationResult {
        summary: BroadcastRuntimeSimulationSummary {
            source_mode,
            presentation_slots_attempted,
            audio_submissions,
            audio_accepted,
            video_submissions,
            video_accepted,
            audio_evidence_count: audio_evidence.len(),
            video_evidence_count: video_evidence.len(),
            frame_presented_count,
            test_evidence_count: audio_evidence.len() + video_evidence.len(),
            lateness_drops: 0,
            failed_slots: failures.len(),
            final_state,
            completed,
        },
        events,
        audio_evidence,
        video_evidence,
        failures,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeAccounting {
    pub selected_frames_accounted: usize,
    pub audio_ranges_accounted: usize,
    pub intentional_profile_skips: usize,
    pub lateness_drops: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeStateMachine {
    state: BroadcastRuntimeState,
    session: BroadcastRuntimeSessionDescription,
    prepare_facts: Option<BroadcastRuntimePrepareFacts>,
    accounting: BroadcastRuntimeAccounting,
    events: Vec<BroadcastRuntimeEvent>,
}

impl BroadcastRuntimeStateMachine {
    pub fn create(session: BroadcastRuntimeSessionDescription) -> Result<Self, PlaybackError> {
        let session = session.validate()?;
        Ok(Self {
            state: BroadcastRuntimeState::Idle,
            session,
            prepare_facts: None,
            accounting: BroadcastRuntimeAccounting {
                selected_frames_accounted: 0,
                audio_ranges_accounted: 0,
                intentional_profile_skips: 0,
                lateness_drops: 0,
            },
            events: vec![BroadcastRuntimeEvent::SessionCreated],
        })
    }

    pub fn state(&self) -> BroadcastRuntimeState {
        self.state
    }

    pub fn session(&self) -> BroadcastRuntimeSessionDescription {
        self.session
    }

    pub fn accounting(&self) -> BroadcastRuntimeAccounting {
        self.accounting
    }

    pub fn events(&self) -> &[BroadcastRuntimeEvent] {
        &self.events
    }

    pub fn prepare(&mut self, facts: BroadcastRuntimePrepareFacts) -> Result<(), PlaybackError> {
        self.ensure_state(BroadcastRuntimeState::Idle)?;
        self.events.push(BroadcastRuntimeEvent::PreparingStarted);
        self.state = BroadcastRuntimeState::Preparing;
        if facts.selected_frame_count == 0
            || !facts.audio_ranges_complete
            || facts.frames_outside_audio_range != 0
        {
            self.fail();
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        self.prepare_facts = Some(facts);
        self.events.push(BroadcastRuntimeEvent::ContractPrepared);
        self.events.push(BroadcastRuntimeEvent::Prepared);
        self.state = BroadcastRuntimeState::Ready;
        Ok(())
    }

    pub fn prepare_with_preroll(
        &mut self,
        facts: BroadcastRuntimePrepareFacts,
        preroll_status: BroadcastPrerollStatus,
    ) -> Result<(), PlaybackError> {
        self.ensure_state(BroadcastRuntimeState::Idle)?;
        self.events.push(BroadcastRuntimeEvent::PreparingStarted);
        self.state = BroadcastRuntimeState::Preparing;
        if facts.selected_frame_count == 0
            || !facts.audio_ranges_complete
            || facts.frames_outside_audio_range != 0
        {
            self.fail();
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        if !preroll_status.ready {
            return Ok(());
        }
        self.prepare_facts = Some(facts);
        self.events.push(BroadcastRuntimeEvent::ContractPrepared);
        self.events.push(BroadcastRuntimeEvent::Prepared);
        self.state = BroadcastRuntimeState::Ready;
        Ok(())
    }

    pub fn play(&mut self) -> Result<(), PlaybackError> {
        match self.state {
            BroadcastRuntimeState::Ready | BroadcastRuntimeState::Paused => {
                self.state = BroadcastRuntimeState::Playing;
                self.events.push(BroadcastRuntimeEvent::PlaybackStarted);
                Ok(())
            }
            _ => Err(PlaybackError::InvalidRuntimeTransition),
        }
    }

    pub fn pause(&mut self) -> Result<(), PlaybackError> {
        self.ensure_state(BroadcastRuntimeState::Playing)?;
        self.state = BroadcastRuntimeState::Paused;
        self.events.push(BroadcastRuntimeEvent::PlaybackPaused);
        Ok(())
    }

    pub fn seek(&mut self, target_time: Duration) -> Result<(), PlaybackError> {
        match self.state {
            BroadcastRuntimeState::Ready
            | BroadcastRuntimeState::Playing
            | BroadcastRuntimeState::Paused => {
                self.events
                    .push(BroadcastRuntimeEvent::SeekCompleted { target_time });
                Ok(())
            }
            _ => Err(PlaybackError::InvalidRuntimeTransition),
        }
    }

    pub fn stop(&mut self) -> Result<(), PlaybackError> {
        match self.state {
            BroadcastRuntimeState::Ready
            | BroadcastRuntimeState::Playing
            | BroadcastRuntimeState::Paused => {
                self.state = BroadcastRuntimeState::Completed;
                self.events.push(BroadcastRuntimeEvent::Completed);
                Ok(())
            }
            _ => Err(PlaybackError::InvalidRuntimeTransition),
        }
    }

    pub fn account_happy_path(&mut self) -> Result<(), PlaybackError> {
        self.ensure_state(BroadcastRuntimeState::Playing)?;
        let facts = self
            .prepare_facts
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        for frame_index in 0..facts.selected_frame_count {
            let frame_index =
                u64::try_from(frame_index).map_err(|_| PlaybackError::TimestampOverflow)?;
            self.events
                .push(BroadcastRuntimeEvent::FrameAccounted { frame_index });
            self.events
                .push(BroadcastRuntimeEvent::AudioRangeAccounted { frame_index });
            self.events
                .push(BroadcastRuntimeEvent::SimulatedPresentationDecision { frame_index });
        }
        for source_frame_index in 0..facts.intentional_profile_skips {
            self.events
                .push(BroadcastRuntimeEvent::IntentionalProfileSkip {
                    source_frame_index: u64::try_from(source_frame_index)
                        .map_err(|_| PlaybackError::TimestampOverflow)?,
                });
        }
        self.accounting.selected_frames_accounted = facts.selected_frame_count;
        self.accounting.audio_ranges_accounted = facts.selected_frame_count;
        self.accounting.intentional_profile_skips = facts.intentional_profile_skips;
        self.accounting.lateness_drops = 0;
        Ok(())
    }

    pub fn drain(&mut self) -> Result<(), PlaybackError> {
        self.ensure_state(BroadcastRuntimeState::Playing)?;
        self.state = BroadcastRuntimeState::Draining;
        self.events.push(BroadcastRuntimeEvent::DrainingStarted);
        Ok(())
    }

    pub fn complete(&mut self) -> Result<(), PlaybackError> {
        self.ensure_state(BroadcastRuntimeState::Draining)?;
        let facts = self
            .prepare_facts
            .ok_or(PlaybackError::InvalidRuntimeTransition)?;
        if self.accounting.selected_frames_accounted != facts.selected_frame_count
            || self.accounting.audio_ranges_accounted != facts.selected_frame_count
            || self.accounting.lateness_drops != 0
        {
            self.fail();
            return Err(PlaybackError::InvalidRuntimeTransition);
        }
        self.state = BroadcastRuntimeState::Completed;
        self.events.push(BroadcastRuntimeEvent::Completed);
        Ok(())
    }

    pub fn fail(&mut self) {
        self.state = BroadcastRuntimeState::Failed;
        self.events.push(BroadcastRuntimeEvent::Failed);
    }

    fn ensure_state(&self, state: BroadcastRuntimeState) -> Result<(), PlaybackError> {
        if self.state == state {
            Ok(())
        } else {
            Err(PlaybackError::InvalidRuntimeTransition)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvFrameAudioRange {
    pub frame_index: u64,
    pub frame_start_time: Duration,
    pub frame_duration: Duration,
    pub audio_start_sample: u64,
    pub audio_sample_count: u64,
    pub audio_start_time: Duration,
    pub audio_duration: Duration,
    pub covered_tracks: Vec<AudioRangeCoverage>,
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadcastRuntimeContractSummary {
    pub frames_checked: usize,
    pub complete_frames: usize,
    pub incomplete_frames: usize,
    pub max_audio_video_delta: Duration,
    pub frames_outside_audio_range: usize,
    pub suitable_for_broadcast_runtime_contract: bool,
}

pub fn av_frame_audio_range(
    frame_index: u64,
    frame_start_time: Duration,
    frame_duration: Duration,
    sample_rate: u32,
    blocks: &[PcmAudioBlock],
    expected_track_count: usize,
) -> Result<AvFrameAudioRange, PlaybackError> {
    let audio_start_sample = audio_samples_for_duration(frame_start_time, sample_rate)?;
    let audio_sample_count = audio_samples_for_duration(frame_duration, sample_rate)?;
    let audio_start_time = duration_from_audio_samples(audio_start_sample, sample_rate)?;
    let audio_duration = duration_from_audio_samples(audio_sample_count, sample_rate)?;
    let covered_tracks =
        audio_range_coverage(blocks, audio_start_sample, audio_sample_count, sample_rate)?;
    let complete = expected_track_count != 0
        && covered_tracks.len() == expected_track_count
        && covered_tracks.iter().all(|coverage| coverage.complete);

    Ok(AvFrameAudioRange {
        frame_index,
        frame_start_time,
        frame_duration,
        audio_start_sample,
        audio_sample_count,
        audio_start_time,
        audio_duration,
        covered_tracks,
        complete,
    })
}

pub fn audio_range_coverage(
    blocks: &[PcmAudioBlock],
    audio_start_sample: u64,
    audio_sample_count: u64,
    sample_rate: u32,
) -> Result<Vec<AudioRangeCoverage>, PlaybackError> {
    if sample_rate == 0 {
        return Err(PlaybackError::InvalidAudioFormat);
    }
    let range_end = audio_start_sample
        .checked_add(audio_sample_count)
        .ok_or(PlaybackError::TimestampOverflow)?;
    let mut groups = Vec::<((u32, u16), Vec<&PcmAudioBlock>)>::new();
    for block in blocks {
        let PcmAudioBlockLayout::MonoTrack {
            track_id,
            channel_index,
        } = block.layout
        else {
            continue;
        };
        if let Some((_, group)) =
            groups
                .iter_mut()
                .find(|((existing_track, existing_channel), _)| {
                    *existing_track == track_id && *existing_channel == channel_index
                })
        {
            group.push(block);
        } else {
            groups.push(((track_id, channel_index), vec![block]));
        }
    }

    let mut coverage = Vec::with_capacity(groups.len());
    for ((track_id, channel_index), mut group) in groups {
        group.sort_by_key(|block| block.start_time);
        let mut cursor = audio_start_sample;
        let mut blocks_used = 0_u32;
        let mut bytes_covered = 0_u64;
        let mut first_block_start = None;
        let mut last_block_end = None;
        let mut gaps = 0_u32;
        let mut overlaps = 0_u32;

        for block in group {
            let block_start = audio_samples_for_duration(block.start_time, sample_rate)?;
            let block_end = block_start
                .checked_add(u64::from(block.sample_count))
                .ok_or(PlaybackError::TimestampOverflow)?;
            if block_end <= audio_start_sample || block_start >= range_end {
                continue;
            }
            if block_start > cursor {
                gaps = gaps.saturating_add(1);
            } else if block_start < cursor && blocks_used != 0 {
                overlaps = overlaps.saturating_add(1);
            }
            let overlap_start = block_start.max(audio_start_sample);
            let overlap_end = block_end.min(range_end);
            let overlap_samples = overlap_end
                .checked_sub(overlap_start)
                .ok_or(PlaybackError::TimestampOverflow)?;
            let bytes_per_sample = u64::try_from(block.format.bytes_per_sample()?)
                .map_err(|_| PlaybackError::TimestampOverflow)?;
            bytes_covered = bytes_covered
                .checked_add(
                    overlap_samples
                        .checked_mul(bytes_per_sample)
                        .ok_or(PlaybackError::TimestampOverflow)?,
                )
                .ok_or(PlaybackError::TimestampOverflow)?;
            blocks_used = blocks_used.saturating_add(1);
            first_block_start.get_or_insert(block.start_time);
            last_block_end = Some(block.end_time()?);
            cursor = cursor.max(overlap_end);
        }

        let complete = cursor >= range_end && gaps == 0 && blocks_used != 0;
        coverage.push(AudioRangeCoverage {
            track_id,
            channel_index,
            blocks_used,
            bytes_covered,
            first_block_start,
            last_block_end,
            gaps,
            overlaps,
            complete,
        });
    }
    coverage.sort_by_key(|coverage| (coverage.track_id, coverage.channel_index));
    Ok(coverage)
}

pub fn summarize_broadcast_runtime_contract(
    ranges: &[AvFrameAudioRange],
    audio_duration: Duration,
) -> BroadcastRuntimeContractSummary {
    let frames_checked = ranges.len();
    let complete_frames = ranges.iter().filter(|range| range.complete).count();
    let incomplete_frames = frames_checked.saturating_sub(complete_frames);
    let mut max_audio_video_delta = Duration::ZERO;
    let mut frames_outside_audio_range = 0_usize;

    for range in ranges {
        let start_delta = duration_abs_delta(range.frame_start_time, range.audio_start_time);
        let duration_delta = duration_abs_delta(range.frame_duration, range.audio_duration);
        max_audio_video_delta = max_audio_video_delta.max(start_delta).max(duration_delta);
        let frame_end = range
            .frame_start_time
            .checked_add(range.frame_duration)
            .unwrap_or(Duration::MAX);
        if frame_end > audio_duration {
            frames_outside_audio_range = frames_outside_audio_range.saturating_add(1);
        }
    }

    BroadcastRuntimeContractSummary {
        frames_checked,
        complete_frames,
        incomplete_frames,
        max_audio_video_delta,
        frames_outside_audio_range,
        suitable_for_broadcast_runtime_contract: frames_checked != 0
            && incomplete_frames == 0
            && frames_outside_audio_range == 0,
    }
}

pub fn pcm_payload_byte_len(
    sample_count: u32,
    channels: u16,
    format: PcmSampleFormat,
) -> Result<usize, PlaybackError> {
    if channels == 0 {
        return Err(PlaybackError::InvalidAudioFormat);
    }
    usize::try_from(sample_count)
        .ok()
        .and_then(|samples| samples.checked_mul(usize::from(channels)))
        .and_then(|values| values.checked_mul(format.bytes_per_sample().ok()?))
        .ok_or(PlaybackError::TimestampOverflow)
}

#[derive(Clone, Debug, Default)]
pub struct TestAudioSink {
    packets: Vec<AudioTimingPacket>,
}

impl TestAudioSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, packet: AudioTimingPacket) {
        self.packets.push(packet);
    }

    pub fn packets(&self) -> &[AudioTimingPacket] {
        &self.packets
    }

    pub fn monotonic(&self) -> bool {
        self.packets
            .windows(2)
            .all(|pair| pair[0].start <= pair[1].start)
    }
}

pub fn duration_from_audio_samples(
    sample_count: u64,
    sample_rate: u32,
) -> Result<Duration, PlaybackError> {
    if sample_rate == 0 {
        return Err(PlaybackError::InvalidAudioFormat);
    }
    let nanos = u128::from(sample_count)
        .checked_mul(NANOS_PER_SECOND)
        .ok_or(PlaybackError::TimestampOverflow)?
        / u128::from(sample_rate);
    duration_from_nanos(nanos)
}

pub fn audio_samples_for_duration(
    duration: Duration,
    sample_rate: u32,
) -> Result<u64, PlaybackError> {
    if sample_rate == 0 {
        return Err(PlaybackError::InvalidAudioFormat);
    }
    let nanos = duration.as_nanos();
    let samples = nanos
        .checked_mul(u128::from(sample_rate))
        .ok_or(PlaybackError::TimestampOverflow)?
        / NANOS_PER_SECOND;
    u64::try_from(samples).map_err(|_| PlaybackError::TimestampOverflow)
}

pub fn duration_abs_delta(a: Duration, b: Duration) -> Duration {
    if a >= b {
        a - b
    } else {
        b - a
    }
}

pub fn max_video_timestamp_outside_audio_range(
    video_timestamps: &[Duration],
    audio_duration: Duration,
) -> Duration {
    video_timestamps
        .iter()
        .map(|timestamp| timestamp.saturating_sub(audio_duration))
        .max()
        .unwrap_or(Duration::ZERO)
}

pub struct BoundedQueue<T> {
    capacity: usize,
    items: VecDeque<T>,
    peak_depth: usize,
    backpressure_events: u64,
}

impl<T> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Result<Self, PlaybackError> {
        if capacity == 0 {
            return Err(PlaybackError::InvalidCapacity);
        }
        Ok(Self {
            capacity,
            items: VecDeque::with_capacity(capacity),
            peak_depth: 0,
            backpressure_events: 0,
        })
    }

    pub fn try_push(&mut self, item: T) -> Result<(), T> {
        if self.items.len() >= self.capacity {
            self.backpressure_events = self.backpressure_events.saturating_add(1);
            return Err(item);
        }
        self.items.push_back(item);
        self.peak_depth = self.peak_depth.max(self.items.len());
        Ok(())
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.items.pop_front()
    }

    pub fn front(&self) -> Option<&T> {
        self.items.front()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.items.len() >= self.capacity
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn stats(&self) -> QueueStats {
        QueueStats {
            capacity: self.capacity,
            peak_depth: self.peak_depth,
            backpressure_events: self.backpressure_events,
        }
    }
}

pub trait PlaybackClock {
    fn now(&self) -> Duration;

    fn sleep_until(&mut self, target: Duration);
}

pub struct RealTimeClock {
    start: Instant,
}

impl RealTimeClock {
    pub fn start_now() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl PlaybackClock for RealTimeClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn sleep_until(&mut self, target: Duration) {
        let now = self.now();
        if target > now {
            std::thread::sleep(target - now);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TestClock {
    now: Duration,
}

impl TestClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn advance(&mut self, delta: Duration) {
        self.now = self.now.saturating_add(delta);
    }

    pub fn set(&mut self, now: Duration) {
        self.now = now;
    }
}

impl PlaybackClock for TestClock {
    fn now(&self) -> Duration {
        self.now
    }

    fn sleep_until(&mut self, target: Duration) {
        if target > self.now {
            self.now = target;
        }
    }
}

pub fn classify_presentation(lateness: Duration, config: PlaybackConfig) -> PresentationStatus {
    if lateness > config.drop_threshold {
        PresentationStatus::Dropped
    } else if lateness > config.on_time_tolerance {
        PresentationStatus::Late
    } else {
        PresentationStatus::Presented
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PlaybackError {
    InvalidRate,
    InvalidCapacity,
    InvalidLatePolicy,
    InvalidAudioFormat,
    InvalidRuntimeTransition,
    TimestampOverflow,
}

impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRate => write!(f, "invalid playback rate"),
            Self::InvalidCapacity => write!(f, "invalid playback queue capacity"),
            Self::InvalidLatePolicy => write!(f, "invalid playback late/drop policy"),
            Self::InvalidAudioFormat => write!(f, "invalid audio format"),
            Self::InvalidRuntimeTransition => {
                write!(f, "invalid broadcast player runtime transition")
            }
            Self::TimestampOverflow => write!(f, "playback timestamp overflow"),
        }
    }
}

impl std::error::Error for PlaybackError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_qgs_descriptor() -> QgsPreparedInputDescriptor {
        let rate = RationalRate::new(50, 1).expect("rate");
        QgsPreparedInputDescriptor {
            contract_version: "test".into(),
            identity: QgsPreparedSourceIdentity {
                clip_id: "clip".into(),
                workspace_db_uri: "qnc://local/db/project_workspace/clip".into(),
                source_record_uri: "qnc://local/source/clip".into(),
            },
            binding: QgsPreparedMediaBinding {
                original_media_uri: "qnc://local/media/original/clip".into(),
                proxy_media_uri: Some("qnc://local/media/proxy/clip".into()),
                private_original_path_bound: true,
                private_proxy_path_bound: true,
                association_status: QgsOriginalProxyAssociationStatus::TimingCompatible,
            },
            selected_picture: QgsPlaybackRepresentation::Proxy,
            authoritative_audio: QgsAudioRepresentation::Original,
            project_audio_channels: 4,
            project_audio_sample_rate: 48_000,
            layout: QgsPreparedStreamLayout {
                original_video: QgsPreparedVideoTiming {
                    timebase: rate,
                    duration_frames: 100,
                    duration: Duration::from_secs(2),
                },
                proxy_video: Some(QgsPreparedVideoTiming {
                    timebase: rate,
                    duration_frames: 100,
                    duration: Duration::from_secs(2),
                }),
                audio_sample_rate: 48_000,
            },
            audio_layout: QgsPreparedAudioLayout {
                representation: QgsAudioRepresentation::Original,
                sample_rate: 48_000,
                bit_depth: 24,
                channels: (0..4)
                    .map(|lane| QgsPreparedAudioChannel {
                        track_id: lane + 1,
                        lane_index: lane as u16,
                        channel_index: 0,
                    })
                    .collect(),
                proxy_aac_authoritative: false,
            },
        }
    }

    fn sample_qgs_queue_requirements() -> QgsInputPlanQueueRequirements {
        QgsInputPlanQueueRequirements {
            min_video_frames: 3,
            min_audio_ranges: 3,
            max_video_queue: 8,
            max_audio_queue: 8,
        }
    }

    fn sample_qgs_input_plan() -> QgsInputPlan {
        QgsInputPlan::from_descriptor(&sample_qgs_descriptor(), sample_qgs_queue_requirements())
            .unwrap()
    }

    fn sample_qnc_like_input() -> QgsQncPreparedInputLike {
        let descriptor = sample_qgs_descriptor();
        QgsQncPreparedInputLike {
            contract_version: "qgs.phase22.qnc-prepared-input.v1".into(),
            descriptor_revision: 1,
            source_identity: QgsQncSourceIdentityLike {
                public_source_uri: descriptor.identity.source_record_uri.clone(),
                source_id: descriptor.identity.clip_id.clone(),
                workspace_db_uri: descriptor.identity.workspace_db_uri.clone(),
                original_media_id: Some(descriptor.binding.original_media_uri.clone()),
                proxy_media_id: descriptor.binding.proxy_media_uri.clone(),
            },
            private_binding: QgsQncPrivateBindingLike {
                original_binding_ref: "binding:original".into(),
                proxy_binding_ref: Some("binding:proxy".into()),
                original_bound: true,
                proxy_bound: true,
                private_path_exposed: false,
            },
            playback_input: QgsQncPlaybackInputLike::ProxyIfAvailable,
            selected_picture_representation: QgsQncRepresentationLike::Proxy,
            authoritative_audio_representation: QgsQncAudioRepresentationLike::Original,
            source_mode: QgsInputPlanSourceMode::ProxyPreview,
            active_range: Some(QgsQncFrameRangeLike {
                start_frame: 0,
                end_frame_exclusive: 50,
            }),
            project_audio: QgsQncProjectAudioLike {
                channels: descriptor.project_audio_channels,
                sample_rate_hz: descriptor.project_audio_sample_rate,
            },
            stream_layout: QgsQncStreamLayoutLike {
                original_video: QgsQncVideoInputLike {
                    timebase: descriptor.layout.original_video.timebase,
                    duration_frames: descriptor.layout.original_video.duration_frames,
                    duration: descriptor.layout.original_video.duration,
                },
                proxy_video: descriptor
                    .layout
                    .proxy_video
                    .map(|video| QgsQncVideoInputLike {
                        timebase: video.timebase,
                        duration_frames: video.duration_frames,
                        duration: video.duration,
                    }),
                audio_sample_rate: descriptor.audio_layout.sample_rate,
                audio_bit_depth: descriptor.audio_layout.bit_depth,
                audio_channels: descriptor
                    .audio_layout
                    .channels
                    .iter()
                    .map(|lane| QgsQncAudioChannelLike {
                        lane_index: lane.lane_index,
                        source_track_index: lane.track_id,
                        source_channel_index: lane.channel_index,
                        channel_kind: QgsQncAudioChannelKindLike::Mono,
                        authoritative: true,
                    })
                    .collect(),
            },
            original_proxy_timing_compatible: Some(true),
            proxy_aac_authoritative: false,
        }
    }

    #[test]
    fn qnc_prepared_descriptor_validates_proxy_picture_original_audio() {
        let descriptor = sample_qgs_descriptor();
        assert!(descriptor.validate().is_ok());
        assert_eq!(
            descriptor.selected_picture,
            QgsPlaybackRepresentation::Proxy
        );
        assert_eq!(
            descriptor.authoritative_audio,
            QgsAudioRepresentation::Original
        );
        assert!(!descriptor.audio_layout.proxy_aac_authoritative);
        assert!(descriptor.binding.public_uris_are_valid());
        assert!(descriptor.identity.public_uris_are_valid());
    }

    #[test]
    fn qnc_like_proxy_preview_maps_to_proxy_picture_and_original_audio() {
        let mapping = sample_qnc_like_input()
            .map_to_qgs_descriptor()
            .expect("mapping");
        let descriptor = mapping.descriptor;
        assert_eq!(
            descriptor.selected_picture,
            QgsPlaybackRepresentation::Proxy
        );
        assert_eq!(
            descriptor.authoritative_audio,
            QgsAudioRepresentation::Original
        );
        assert!(!descriptor.audio_layout.proxy_aac_authoritative);
        assert_eq!(descriptor.audio_layout.channels.len(), 4);
        assert!(!mapping.private_path_exposed);
        let plan = QgsInputPlan::from_descriptor(&descriptor, sample_qgs_queue_requirements())
            .expect("plan");
        assert_eq!(plan.source_mode, QgsInputPlanSourceMode::ProxyPreview);
        assert_eq!(
            plan.video_source.representation,
            QgsPlaybackRepresentation::Proxy
        );
        assert_eq!(
            plan.audio_source.representation,
            QgsAudioRepresentation::Original
        );
    }

    #[test]
    fn qnc_like_original_media_maps_to_original_picture_and_original_audio() {
        let mut input = sample_qnc_like_input();
        input.source_mode = QgsInputPlanSourceMode::OriginalMedia;
        input.selected_picture_representation = QgsQncRepresentationLike::Original;
        input.playback_input = QgsQncPlaybackInputLike::Original;
        let descriptor = input.map_to_qgs_descriptor().unwrap().descriptor;
        assert_eq!(
            descriptor.selected_picture,
            QgsPlaybackRepresentation::Original
        );
        let plan = QgsInputPlan::from_descriptor(&descriptor, sample_qgs_queue_requirements())
            .expect("plan");
        assert_eq!(plan.source_mode, QgsInputPlanSourceMode::OriginalMedia);
        assert_eq!(
            plan.video_source.representation,
            QgsPlaybackRepresentation::Original
        );
        assert_eq!(plan.audio_source.media_uri, plan.video_source.media_uri);
    }

    #[test]
    fn qnc_like_mapping_rejects_proxy_aac_as_authoritative() {
        let mut input = sample_qnc_like_input();
        input.authoritative_audio_representation = QgsQncAudioRepresentationLike::ProxyAac;
        assert_eq!(
            input.map_to_qgs_descriptor().unwrap_err(),
            QgsQncPreparedInputMappingError::MissingOriginalAudio
        );
        let mut input = sample_qnc_like_input();
        input.proxy_aac_authoritative = true;
        assert_eq!(
            input.map_to_qgs_descriptor().unwrap_err(),
            QgsQncPreparedInputMappingError::ProxyAudioCannotBeAuthoritative
        );
    }

    #[test]
    fn qnc_like_mapping_preserves_discrete_mono_and_rejects_stereo_collapse() {
        let mapping = sample_qnc_like_input().map_to_qgs_descriptor().unwrap();
        for (expected, lane) in mapping.descriptor.audio_layout.channels.iter().enumerate() {
            assert_eq!(lane.lane_index as usize, expected);
            assert_eq!(lane.channel_index, 0);
        }
        let mut input = sample_qnc_like_input();
        input.stream_layout.audio_channels[0].channel_kind =
            QgsQncAudioChannelKindLike::StereoCollapsed;
        assert_eq!(
            input.map_to_qgs_descriptor().unwrap_err(),
            QgsQncPreparedInputMappingError::StereoCollapseRejected
        );
    }

    #[test]
    fn qnc_like_mapping_rejects_private_path_exposure_and_missing_public_uri() {
        let mut input = sample_qnc_like_input();
        input.private_binding.private_path_exposed = true;
        assert_eq!(
            input.map_to_qgs_descriptor().unwrap_err(),
            QgsQncPreparedInputMappingError::PrivatePathExposureRejected
        );
        let mut input = sample_qnc_like_input();
        input.source_identity.public_source_uri.clear();
        assert_eq!(
            input.map_to_qgs_descriptor().unwrap_err(),
            QgsQncPreparedInputMappingError::MissingPublicSourceUri
        );
    }

    #[test]
    fn qnc_like_mapping_rejects_invalid_range_and_source_mode_mismatch() {
        let mut input = sample_qnc_like_input();
        input.active_range = Some(QgsQncFrameRangeLike {
            start_frame: 10,
            end_frame_exclusive: 10,
        });
        assert_eq!(
            input.map_to_qgs_descriptor().unwrap_err(),
            QgsQncPreparedInputMappingError::InvalidActiveRange
        );
        let mut input = sample_qnc_like_input();
        input.source_mode = QgsInputPlanSourceMode::OriginalMedia;
        assert_eq!(
            input.map_to_qgs_descriptor().unwrap_err(),
            QgsQncPreparedInputMappingError::SelectedPictureMismatch
        );
    }

    #[test]
    fn qnc_prepared_descriptor_rejects_missing_proxy_for_proxy_picture() {
        let mut descriptor = sample_qgs_descriptor();
        descriptor.binding.proxy_media_uri = None;
        assert_eq!(
            descriptor.validate(),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    #[test]
    fn qnc_prepared_descriptor_rejects_proxy_aac_as_authoritative() {
        let mut descriptor = sample_qgs_descriptor();
        descriptor.audio_layout.proxy_aac_authoritative = true;
        assert_eq!(
            descriptor.validate(),
            Err(PlaybackError::InvalidAudioFormat)
        );
    }

    #[test]
    fn broadcast_player_assembly_preserves_media_policy() {
        let assembly =
            QgsBroadcastPlayerAssembly::from_input_plan(sample_qgs_input_plan()).unwrap();
        let snapshot = assembly.snapshot();

        assert_eq!(snapshot.source_mode, QgsInputPlanSourceMode::ProxyPreview);
        assert_eq!(snapshot.video_source_role, QgsPlaybackRepresentation::Proxy);
        assert_eq!(snapshot.audio_source_role, QgsAudioRepresentation::Original);
        assert!(snapshot.original_audio_authoritative);
        assert!(!snapshot.proxy_aac_authoritative);
        assert!(snapshot.discrete_mono_lanes);
        assert!(!snapshot.exposes_private_path());
        assert_ne!(
            snapshot.device_selection.selected_video_backend,
            QgsVideoPresenterBackendKind::X11LegacyNonTarget
        );
    }

    #[test]
    fn broadcast_player_assembly_creates_operational_runtime_without_private_paths() {
        let assembly =
            QgsBroadcastPlayerAssembly::from_input_plan(sample_qgs_input_plan()).unwrap();
        let runtime = assembly.new_operational_runtime().unwrap();

        assert!(!runtime.snapshot().exposes_private_path());
        assert!(assembly.module_labels().contains(&"input_plan"));
        assert!(assembly.module_labels().contains(&"session_facade"));
    }

    #[test]
    fn qnc_prepared_descriptor_preserves_four_mono_lanes() {
        let descriptor = sample_qgs_descriptor();
        assert_eq!(descriptor.audio_layout.channels.len(), 4);
        for (index, channel) in descriptor.audio_layout.channels.iter().enumerate() {
            assert_eq!(channel.lane_index as usize, index);
            assert_eq!(channel.channel_index, 0);
        }
    }

    #[test]
    fn qnc_prepared_descriptor_rejects_raw_path_public_identity() {
        let mut descriptor = sample_qgs_descriptor();
        descriptor.binding.original_media_uri = "/tmp/original.mxf".into();
        assert_eq!(
            descriptor.validate(),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    #[test]
    fn qgs_input_plan_builds_from_valid_descriptor() {
        let descriptor = sample_qgs_descriptor();
        let plan =
            QgsInputPlan::from_descriptor(&descriptor, sample_qgs_queue_requirements()).unwrap();
        assert_eq!(plan.source_mode, QgsInputPlanSourceMode::ProxyPreview);
        assert_eq!(
            plan.video_source.representation,
            QgsPlaybackRepresentation::Proxy
        );
        assert_eq!(
            plan.audio_source.representation,
            QgsAudioRepresentation::Original
        );
        assert_eq!(plan.audio_source.lanes.len(), 4);
        assert_eq!(
            plan.samples_for_duration(Duration::from_millis(1_000))
                .unwrap(),
            48_000
        );
    }

    #[test]
    fn qgs_input_plan_rejects_invalid_descriptor() {
        let mut descriptor = sample_qgs_descriptor();
        descriptor.binding.association_status = QgsOriginalProxyAssociationStatus::TimingMismatch;
        assert_eq!(
            QgsInputPlan::from_descriptor(&descriptor, sample_qgs_queue_requirements()),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    #[test]
    fn qgs_transport_empty_engine_is_not_play_ready() {
        let engine = QgsTransportEngine::new();
        let snapshot = engine.snapshot();
        assert_eq!(snapshot.status, QgsTransportStatus::Empty);
        assert!(!snapshot.play_ready);
        assert_eq!(
            snapshot.events,
            vec![QgsTransportEvent::TransportEngineCreated]
        );
    }

    #[test]
    fn qgs_transport_load_source_creates_public_handle() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        assert_eq!(handle.revision, QgsTransportSourceRevision(1));
        assert!(handle.source_id.starts_with("qnc://"));
        assert!(!handle.exposes_private_path());
        assert_eq!(handle.duration_frames, 100);
    }

    #[test]
    fn qgs_transport_rejects_invalid_preload_and_active_handles() {
        let mut engine = QgsTransportEngine::new();
        let mut handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        handle.revision = QgsTransportSourceRevision(99);
        assert_eq!(
            engine.preload_source(&handle),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(
            engine.set_active_source(&handle),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    #[test]
    fn qgs_transport_validates_active_range() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        engine.set_active_source(&handle).unwrap();
        assert_eq!(
            engine.set_active_range_frames(10, 10),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(
            engine.set_active_range_frames(0, 101),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        let range = engine.set_active_range_frames(0, 50).unwrap();
        assert_eq!(range.start_sample, 0);
        assert_eq!(range.end_sample, 48_000);
    }

    #[test]
    fn qgs_transport_rejects_cue_outside_half_open_range() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        engine.set_active_source(&handle).unwrap();
        engine.set_active_range_frames(0, 50).unwrap();
        assert_eq!(
            engine.cue_frame(50),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(
            engine.cue_frame(99),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(engine.cue_frame(49).unwrap().sample, 47_040);
    }

    #[test]
    fn qgs_transport_prepared_anchor_is_required_for_play_ready() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        engine.preload_source(&handle).unwrap();
        engine.set_active_source(&handle).unwrap();
        engine.set_active_range_frames(0, 50).unwrap();
        engine.cue_frame(0).unwrap();
        assert!(!engine.evaluate_play_ready());
        engine.prepare_anchor().unwrap();
        assert!(engine.evaluate_play_ready());
    }

    #[test]
    fn qgs_transport_play_rejects_before_ready_and_accepts_after_anchor() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        engine.preload_source(&handle).unwrap();
        engine.set_active_source(&handle).unwrap();
        engine.set_active_range_frames(0, 50).unwrap();
        assert_eq!(engine.play(), Err(PlaybackError::InvalidRuntimeTransition));
        engine.cue_frame(0).unwrap();
        engine.prepare_anchor().unwrap();
        assert!(engine.evaluate_play_ready());
        engine.play().unwrap();
        let snapshot = engine.snapshot();
        assert_eq!(snapshot.status, QgsTransportStatus::Playing);
        assert_eq!(
            snapshot.no_work_on_play,
            QgsTransportNoWorkOnPlayCounters::default()
        );
    }

    #[test]
    fn qgs_transport_events_are_deterministic() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        engine.preload_source(&handle).unwrap();
        engine.set_active_source(&handle).unwrap();
        engine.set_active_range_frames(0, 50).unwrap();
        engine.cue_frame(0).unwrap();
        engine.prepare_anchor().unwrap();
        engine.play().unwrap();
        let kinds = engine
            .snapshot()
            .events
            .into_iter()
            .map(|event| match event {
                QgsTransportEvent::TransportEngineCreated => "created",
                QgsTransportEvent::SourceLoaded { .. } => "loaded",
                QgsTransportEvent::SourcePreloaded { .. } => "preloaded",
                QgsTransportEvent::ActiveSourceChanged { .. } => "active",
                QgsTransportEvent::ActiveRangeSet { .. } => "range",
                QgsTransportEvent::CueCompleted { .. } => "cue",
                QgsTransportEvent::PreparedAnchorReady { .. } => "anchor",
                QgsTransportEvent::PlayReadinessChanged { ready: true } => "ready",
                QgsTransportEvent::PlayReadinessChanged { ready: false } => "not-ready",
                QgsTransportEvent::TransportStarted { .. } => "started",
                _ => "other",
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                "created",
                "loaded",
                "preloaded",
                "active",
                "range",
                "cue",
                "anchor",
                "ready",
                "not-ready",
                "started"
            ]
        );
    }

    #[test]
    fn qgs_transport_sample_mapping_uses_source_facts() {
        let plan = sample_qgs_input_plan();
        assert_eq!(
            qgs_frames_for_duration(Duration::from_millis(1_000), plan.video_source.timebase)
                .unwrap(),
            50
        );
        assert_eq!(
            plan.samples_for_duration(Duration::from_millis(1_000))
                .unwrap(),
            48_000
        );
    }

    fn sample_qgs_active_range(rate: RationalRate) -> QgsActiveRangeTiming {
        QgsActiveRangeTiming::new(120, rate, 48_000, 0, 50).unwrap()
    }

    #[test]
    fn qgs_frame_clock_counts_integer_rates_exactly() {
        assert_eq!(
            qgs_frames_for_duration(Duration::from_secs(1), RationalRate::new(25, 1).unwrap())
                .unwrap(),
            25
        );
        assert_eq!(
            qgs_frames_for_duration(Duration::from_secs(1), RationalRate::new(50, 1).unwrap())
                .unwrap(),
            50
        );
    }

    #[test]
    fn qgs_frame_clock_preserves_fractional_rates() {
        let ntsc_30 = RationalRate::new(30_000, 1_001).unwrap();
        let ntsc_60 = RationalRate::new(60_000, 1_001).unwrap();
        assert_eq!(
            qgs_frames_for_duration(Duration::from_millis(1_000), ntsc_30).unwrap(),
            29
        );
        assert_eq!(
            qgs_frames_for_duration(Duration::from_millis(1_001), ntsc_30).unwrap(),
            30
        );
        assert_eq!(
            qgs_frames_for_duration(Duration::from_millis(1_000), ntsc_60).unwrap(),
            59
        );
        assert_eq!(
            qgs_frames_for_duration(Duration::from_millis(1_001), ntsc_60).unwrap(),
            60
        );
    }

    #[test]
    fn qgs_active_range_timing_validates_half_open_edges() {
        let rate = RationalRate::new(50, 1).unwrap();
        assert!(QgsActiveRangeTiming::new(106, rate, 48_000, 0, 50).is_ok());
        assert_eq!(
            QgsActiveRangeTiming::new(106, rate, 48_000, 10, 10),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(
            QgsActiveRangeTiming::new(106, rate, 48_000, 50, 49),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(
            QgsActiveRangeTiming::new(106, rate, 48_000, 0, 107),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        let timing = QgsActiveRangeTiming::new(106, rate, 48_000, 0, 50).unwrap();
        assert!(timing.validate_cue(0).is_ok());
        assert!(timing.validate_cue(49).is_ok());
        assert_eq!(
            timing.validate_cue(50),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    #[test]
    fn qgs_frame_audio_sample_mapping_uses_original_audio_rate() {
        let timing_50 =
            QgsActiveRangeTiming::new(106, RationalRate::new(50, 1).unwrap(), 48_000, 0, 50)
                .unwrap();
        assert_eq!(
            timing_50.frame_audio_sample_range(0).unwrap(),
            QgsFrameAudioSampleRange {
                frame: 0,
                start_sample: 0,
                end_sample: 960,
                start_time: Duration::ZERO,
                duration: Duration::from_millis(20),
            }
        );
        assert_eq!(
            timing_50.frame_audio_sample_range(1).unwrap().start_sample,
            960
        );
        assert_eq!(
            timing_50.frame_audio_sample_range(1).unwrap().end_sample,
            1_920
        );
        assert_eq!(timing_50.start_sample, 0);
        assert_eq!(timing_50.end_sample, 48_000);

        let timing_25 =
            QgsActiveRangeTiming::new(53, RationalRate::new(25, 1).unwrap(), 48_000, 0, 25)
                .unwrap();
        assert_eq!(
            timing_25.frame_audio_sample_range(0).unwrap().end_sample,
            1_920
        );
        assert_eq!(
            timing_25.frame_audio_sample_range(1).unwrap().start_sample,
            1_920
        );
        assert_eq!(
            timing_25.frame_audio_sample_range(1).unwrap().end_sample,
            3_840
        );
    }

    #[test]
    fn qgs_fractional_frame_audio_mapping_uses_floor_boundaries_without_overlap() {
        let timing = QgsActiveRangeTiming::new(
            120,
            RationalRate::new(30_000, 1_001).unwrap(),
            48_000,
            0,
            10,
        )
        .unwrap();
        let first = timing.frame_audio_sample_range(0).unwrap();
        let second = timing.frame_audio_sample_range(1).unwrap();
        assert_eq!(first.start_sample, 0);
        assert_eq!(first.end_sample, 1_601);
        assert_eq!(second.start_sample, first.end_sample);
        assert_eq!(second.end_sample, 3_203);
    }

    #[test]
    fn qgs_frame_clock_latest_due_frame_is_range_bounded() {
        let timing = sample_qgs_active_range(RationalRate::new(50, 1).unwrap());
        let clock = QgsFrameClock::forward(timing).unwrap();
        assert_eq!(clock.latest_due_frame(Duration::ZERO).unwrap(), Some(0));
        assert_eq!(
            clock.latest_due_frame(Duration::from_millis(60)).unwrap(),
            Some(3)
        );
        assert_eq!(
            clock.latest_due_frame(Duration::from_secs(2)).unwrap(),
            Some(49)
        );
    }

    #[test]
    fn qgs_frame_clock_drains_forward_without_duplicates_or_skips() {
        let timing = sample_qgs_active_range(RationalRate::new(50, 1).unwrap());
        let clock = QgsFrameClock::forward(timing).unwrap();
        let first = clock
            .drain_due_frames(None, Duration::from_millis(60), 16)
            .unwrap();
        assert_eq!(first.frames, vec![0, 1, 2, 3]);
        let second = clock
            .drain_due_frames(Some(3), Duration::from_millis(120), 16)
            .unwrap();
        assert_eq!(second.frames, vec![4, 5, 6]);
    }

    #[test]
    fn qgs_frame_clock_reverse_is_bounded_by_range_start() {
        let timing = sample_qgs_active_range(RationalRate::new(50, 1).unwrap());
        let clock = QgsFrameClock::reverse(timing).unwrap();
        let drain = clock
            .drain_due_frames(None, Duration::from_secs(2), 128)
            .unwrap();
        assert_eq!(drain.frames.first().copied(), Some(49));
        assert_eq!(drain.frames.last().copied(), Some(0));
        assert!(drain.bounded_by_start);
    }

    #[test]
    fn qgs_frame_clock_still_does_not_advance_due_frames() {
        let timing = sample_qgs_active_range(RationalRate::new(50, 1).unwrap());
        let clock = QgsFrameClock::still(timing, 12).unwrap();
        assert_eq!(
            clock.latest_due_frame(Duration::from_secs(10)).unwrap(),
            Some(12)
        );
        assert!(clock
            .drain_due_frames(None, Duration::from_secs(10), 16)
            .unwrap()
            .frames
            .is_empty());
    }

    #[test]
    fn qgs_frame_clock_supports_simple_double_rate() {
        let timing = sample_qgs_active_range(RationalRate::new(50, 1).unwrap());
        let clock = QgsFrameClock::new(
            timing,
            QgsFrameClockMode::Forward,
            QgsFrameClockRate::two(),
            0,
        )
        .unwrap();
        assert_eq!(
            clock.latest_due_frame(Duration::from_millis(40)).unwrap(),
            Some(4)
        );
    }

    #[test]
    fn qgs_transport_cue_uses_timing_layer() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        engine.set_active_source(&handle).unwrap();
        engine.set_active_range_frames(0, 50).unwrap();
        assert_eq!(engine.cue_frame(1).unwrap().sample, 960);
        assert_eq!(
            engine.cue_frame(50),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    fn sample_qgs_playout_buffer() -> (QgsPlayoutBufferState, QgsFrameClock) {
        let plan = sample_qgs_input_plan();
        let active_range = QgsActiveRangeTiming::new(
            plan.video_source.duration_frames,
            plan.video_source.timebase,
            plan.audio_source.sample_rate,
            0,
            50,
        )
        .unwrap();
        let buffer = QgsPlayoutBufferState::new(
            active_range,
            QgsTransportSourceRevision(1),
            plan.source_mode,
            plan.audio_source.lanes,
            QgsPlayoutBufferLimits::default_transport_window(),
        )
        .unwrap();
        let clock = QgsFrameClock::forward(active_range).unwrap();
        (buffer, clock)
    }

    #[test]
    fn qgs_playout_buffer_rejects_invalid_limits() {
        let plan = sample_qgs_input_plan();
        let active_range = QgsActiveRangeTiming::new(
            plan.video_source.duration_frames,
            plan.video_source.timebase,
            plan.audio_source.sample_rate,
            0,
            50,
        )
        .unwrap();
        assert_eq!(
            QgsPlayoutBufferState::new(
                active_range,
                QgsTransportSourceRevision(1),
                plan.source_mode,
                plan.audio_source.lanes.clone(),
                QgsPlayoutBufferLimits {
                    backward_keep_frames: 2,
                    forward_prepare_frames: 5,
                    max_prepared_frames: 0,
                },
            ),
            Err(PlaybackError::InvalidCapacity)
        );
        assert_eq!(
            QgsPlayoutBufferState::new(
                active_range,
                QgsTransportSourceRevision(1),
                plan.source_mode,
                Vec::new(),
                QgsPlayoutBufferLimits::default_transport_window(),
            ),
            Err(PlaybackError::InvalidAudioFormat)
        );
    }

    #[test]
    fn qgs_playout_buffer_prepares_initial_forward_window() {
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        let result = buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        assert_eq!(result.window.start_frame, 0);
        assert_eq!(result.window.end_frame, 6);
        assert_eq!(result.prepared_frames, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(result.slots.len(), 6);
        assert_eq!(
            result.slots[0].audio.sample_range,
            QgsFrameAudioSampleRange {
                frame: 0,
                start_sample: 0,
                end_sample: 960,
                start_time: Duration::ZERO,
                duration: Duration::from_millis(20),
            }
        );
        assert_eq!(
            result.slots[0].audio.payload_kind,
            QgsPreparedPayloadKind::OriginalAudioRange
        );
        assert_eq!(result.slots[0].audio.lanes.len(), 4);
        assert_eq!(
            result.slots[0].video.payload_kind,
            QgsPreparedPayloadKind::ProxyVideoReference
        );
    }

    #[test]
    fn qgs_playout_buffer_discards_old_frames_when_carrier_advances() {
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        let result = buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 6,
                    elapsed: Duration::from_millis(120),
                    max_due_frames: 8,
                },
            )
            .unwrap();
        assert_eq!(result.discarded_frames, vec![0, 1, 2, 3]);
        assert!(result.slots.iter().all(|slot| slot.key.frame >= 4));
        assert!(result.slots.iter().all(|slot| slot.key.frame < 12));
        assert_eq!(result.slots.len(), 8);
    }

    #[test]
    fn qgs_playout_buffer_respects_active_range_end() {
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        let result = buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 48,
                    elapsed: Duration::from_millis(960),
                    max_due_frames: 64,
                },
            )
            .unwrap();
        assert_eq!(result.window.start_frame, 46);
        assert_eq!(result.window.end_frame, 50);
        assert_eq!(result.prepared_frames, vec![46, 47, 48, 49]);
        assert_eq!(
            result
                .slots
                .iter()
                .find(|slot| slot.key.frame == 49)
                .unwrap()
                .audio
                .sample_range
                .start_sample,
            47_040
        );
        assert_eq!(
            result
                .slots
                .iter()
                .find(|slot| slot.key.frame == 49)
                .unwrap()
                .audio
                .sample_range
                .end_sample,
            48_000
        );
    }

    #[test]
    fn qgs_playout_buffer_enforces_max_prepared_frames() {
        let plan = sample_qgs_input_plan();
        let active_range = QgsActiveRangeTiming::new(
            plan.video_source.duration_frames,
            plan.video_source.timebase,
            plan.audio_source.sample_rate,
            0,
            50,
        )
        .unwrap();
        let mut buffer = QgsPlayoutBufferState::new(
            active_range,
            QgsTransportSourceRevision(1),
            plan.source_mode,
            plan.audio_source.lanes,
            QgsPlayoutBufferLimits {
                backward_keep_frames: 2,
                forward_prepare_frames: 8,
                max_prepared_frames: 3,
            },
        )
        .unwrap();
        let result = buffer
            .tick_prepare(
                QgsFrameClock::forward(active_range).unwrap(),
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        assert_eq!(result.slots.len(), 3);
        assert!(result
            .events
            .contains(&QgsTickPreparationEvent::BufferLimitReached {
                max_prepared_frames: 3
            }));
    }

    #[test]
    fn qgs_playout_buffer_events_are_deterministic_and_non_presenting() {
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        let result = buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        let labels = result
            .events
            .iter()
            .map(|event| match event {
                QgsTickPreparationEvent::TickPreparationStarted { .. } => "started",
                QgsTickPreparationEvent::DueFramesDrained { .. } => "drained",
                QgsTickPreparationEvent::FramePrepared { .. } => "frame",
                QgsTickPreparationEvent::AudioRangePrepared { .. } => "audio",
                QgsTickPreparationEvent::VideoPayloadMarkedReady { .. } => "video",
                QgsTickPreparationEvent::PreparedWindowAdvanced { .. } => "window",
                QgsTickPreparationEvent::TickPreparationCompleted { .. } => "completed",
                _ => "other",
            })
            .collect::<Vec<_>>();
        assert_eq!(
            &labels[..8],
            &["started", "drained", "frame", "audio", "video", "frame", "audio", "video"]
        );
        assert!(!labels.contains(&"presented"));
        assert!(!labels.contains(&"realtime"));
    }

    #[test]
    fn qgs_transport_play_does_not_fill_playout_buffer() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        engine.preload_source(&handle).unwrap();
        engine.set_active_source(&handle).unwrap();
        engine.set_active_range_frames(0, 50).unwrap();
        engine.cue_frame(0).unwrap();
        engine.prepare_anchor().unwrap();
        engine.play().unwrap();
        let snapshot = engine.snapshot();
        assert_eq!(
            snapshot.no_work_on_play,
            QgsTransportNoWorkOnPlayCounters::default()
        );
    }

    fn sample_ready_transport_engine() -> (QgsTransportEngine, QgsTransportSourceHandle) {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        engine.preload_source(&handle).unwrap();
        engine.set_active_source(&handle).unwrap();
        engine.set_active_range_frames(0, 50).unwrap();
        engine.cue_frame(0).unwrap();
        engine.prepare_anchor().unwrap();
        assert!(engine.evaluate_play_ready());
        (engine, handle)
    }

    #[test]
    fn qgs_transport_unloads_inactive_source_and_invalidates_revision() {
        let mut engine = QgsTransportEngine::new();
        let handle = engine.load_source(&sample_qgs_input_plan()).unwrap();
        let result = engine.unload_source(&handle);
        assert!(result.unloaded);
        assert!(!result.was_active);
        assert!(result.revision_invalidated);
        assert_eq!(
            engine.preload_source(&handle),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert!(engine.snapshot().active_source.is_none());
    }

    #[test]
    fn qgs_transport_unloads_active_source_and_clears_state() {
        let (mut engine, handle) = sample_ready_transport_engine();
        assert!(engine.snapshot().play_ready);
        let result = engine.unload_source(&handle);
        assert!(result.unloaded);
        assert!(result.was_active);
        let snapshot = engine.snapshot();
        assert!(snapshot.active_source.is_none());
        assert!(snapshot.active_range.is_none());
        assert!(snapshot.cue.is_none());
        assert!(snapshot.prepared_anchor.is_none());
        assert!(!snapshot.play_ready);
        assert_eq!(snapshot.status, QgsTransportStatus::Empty);
        assert!(snapshot
            .events
            .contains(&QgsTransportEvent::ActiveSourceCleared {
                source_id: handle.source_id.clone(),
                revision: handle.revision,
            }));
        assert!(snapshot
            .events
            .contains(&QgsTransportEvent::PlayReadinessChanged { ready: false }));
        assert!(snapshot
            .events
            .contains(&QgsTransportEvent::SourceRevisionInvalidated {
                source_id: handle.source_id.clone(),
                revision: handle.revision,
            }));
    }

    #[test]
    fn qgs_transport_close_active_source_preserves_loaded_source() {
        let (mut engine, handle) = sample_ready_transport_engine();
        let result = engine.close_active_source().unwrap();
        assert!(result.active_source_cleared);
        assert!(result.source_preserved_loaded);
        assert!(!result.play_ready);
        assert!(engine.snapshot().active_source.is_none());
        engine.set_active_source(&handle).unwrap();
        assert!(engine.snapshot().active_source.is_some());
    }

    #[test]
    fn qgs_transport_rejects_stale_handle_after_unload() {
        let (mut engine, handle) = sample_ready_transport_engine();
        engine.unload_source(&handle);
        assert_eq!(
            engine.set_active_source(&handle),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(
            engine.preload_source(&handle),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(
            engine.cue_frame(0),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(
            engine.prepare_anchor(),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        let stale_unload = engine.unload_source(&handle);
        assert!(!stale_unload.unloaded);
        assert!(stale_unload.already_missing_or_invalid);
    }

    #[test]
    fn qgs_playout_buffer_discards_all_on_source_unload() {
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        assert_eq!(buffer.prepared_frame_count(), 6);
        let discarded = buffer.discard_all(QgsBufferDiscardReason::SourceUnloaded);
        assert_eq!(discarded, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(buffer.prepared_frame_count(), 0);
        assert!(buffer
            .events()
            .contains(&QgsTickPreparationEvent::PreparedStateDiscarded {
                frames: vec![0, 1, 2, 3, 4, 5],
                reason: QgsBufferDiscardReason::SourceUnloaded,
            }));
    }

    #[test]
    fn qgs_runtime_event_envelope_sequence_is_monotonic_and_public() {
        let (mut engine, handle) = sample_ready_transport_engine();
        let mut log = QgsRuntimeEventLog::new();
        for event in &engine.snapshot().events {
            log.push_transport_event(event);
        }
        let previous_count = log.envelopes().len();
        log.increment_generation();
        engine.unload_source(&handle);
        for event in engine.snapshot().events.iter().skip(previous_count) {
            log.push_transport_event(event);
        }
        assert!(log.sequence_is_monotonic());
        assert!(log
            .envelopes()
            .iter()
            .all(|event| !event.exposes_private_path()));
        assert!(log
            .envelopes()
            .iter()
            .any(|event| event.kind == QgsRuntimeEventKind::SourceUnloaded));
        assert!(log
            .envelopes()
            .iter()
            .any(|event| event.generation == QgsRuntimeEventGeneration(1)));
    }

    #[test]
    fn qgs_runtime_event_envelope_wraps_tick_discard_without_verified_claims() {
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        buffer.discard_all(QgsBufferDiscardReason::SourceClosed);
        let mut log = QgsRuntimeEventLog::new();
        for event in buffer.events() {
            log.push_tick_event(event);
        }
        let transcript = format!("{:?}", log.envelopes());
        assert!(log.sequence_is_monotonic());
        assert!(transcript.contains("PreparedStateDiscarded"));
        assert!(!transcript.contains("FramePresented"));
        assert!(!transcript.contains("RealtimeVerified"));
        assert!(!transcript.contains("AudioDeviceVerified"));
    }

    #[test]
    fn qgs_qnc_command_envelope_contains_no_private_path() {
        let command = QgsQncCommandEnvelope::new(
            7,
            Some(QgsRuntimeEventGeneration(1)),
            Some("qnc://local/media/proxy/Mironik-1560".to_string()),
            QgsQncPlayerCommand::SetActiveSource,
            "set active source by public uri",
        );
        assert!(!command.exposes_private_path());
        let private = QgsQncCommandEnvelope::new(
            8,
            None,
            Some("/home/miro/private.MXF".to_string()),
            QgsQncPlayerCommand::LoadPreparedInput,
            "bad private source",
        );
        assert!(private.exposes_private_path());
    }

    #[test]
    fn qgs_qnc_event_projection_preserves_generation_and_monotonic_sequence() {
        let (mut engine, handle) = sample_ready_transport_engine();
        let mut log = QgsRuntimeEventLog::new();
        let ready_snapshot = engine.snapshot();
        for event in &ready_snapshot.events {
            log.push_transport_event(event);
        }
        log.increment_generation();
        engine.unload_source(&handle);
        for event in engine
            .snapshot()
            .events
            .iter()
            .skip(ready_snapshot.events.len())
        {
            log.push_transport_event(event);
        }
        let projected = project_qnc_events(log.envelopes());
        assert!(qgs_qnc_projected_sequence_is_monotonic(&projected));
        assert!(projected.iter().all(|event| !event.exposes_private_path()));
        assert!(projected
            .iter()
            .any(|event| event.event.kind == QgsQncProjectedEventKind::SourceUnloaded));
        assert!(projected.iter().any(|event| event.generation == 1));
    }

    #[test]
    fn qgs_qnc_passive_view_reflects_ready_transport_state() {
        let (engine, _handle) = sample_ready_transport_engine();
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        let view = QgsQncPassiveView::from_parts(
            QgsRuntimeEventGeneration(0),
            &engine.snapshot(),
            &buffer.slots(),
            0,
        );
        assert_eq!(view.loaded_source_count, 1);
        assert!(view.readiness.play_ready);
        assert!(view.readiness.prepared_anchor_ready);
        assert_eq!(view.prepared_buffer.prepared_frame_count, 6);
        assert_eq!(
            view.prepared_buffer.payload_evidence,
            QgsQncEvidenceStatus::Prepared
        );
        assert!(!view.private_path_exposed);
        assert!(!view.evidence.frame_presented);
        assert!(!view.evidence.audio_device_verified);
        assert!(!view.evidence.realtime_verified);
    }

    #[test]
    fn qgs_qnc_passive_view_after_unload_reflects_cleared_source() {
        let (mut engine, handle) = sample_ready_transport_engine();
        engine.unload_source(&handle);
        let view =
            QgsQncPassiveView::from_parts(QgsRuntimeEventGeneration(1), &engine.snapshot(), &[], 6);
        assert_eq!(view.loaded_source_count, 0);
        assert!(view.source.public_source_uri.is_none());
        assert!(!view.readiness.play_ready);
        assert!(!view.readiness.active_source_ready);
        assert_eq!(view.prepared_buffer.latest_discarded_frame_count, 6);
        assert!(!view.private_path_exposed);
    }

    #[test]
    fn qgs_qnc_timeline_projection_uses_active_range_without_presented_claim() {
        let (engine, _handle) = sample_ready_transport_engine();
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        let timeline = QgsQncTimelineProjection::from_parts(&engine.snapshot(), &buffer.slots());
        assert_eq!(timeline.active_range.unwrap().start_frame, 0);
        assert_eq!(timeline.active_range.unwrap().end_frame, 50);
        assert_eq!(timeline.prepared_start_frame, Some(0));
        assert_eq!(timeline.prepared_end_frame_exclusive, Some(6));
        assert_eq!(timeline.audio_sample_rate, Some(48_000));
        assert!(!timeline.presented_frame_claimed);
    }

    #[test]
    fn qgs_qnc_monitor_projection_does_not_claim_real_display_evidence() {
        let (mut buffer, clock) = sample_qgs_playout_buffer();
        buffer
            .tick_prepare(
                clock,
                QgsTickPreparationInput {
                    carrier_frame: 0,
                    elapsed: Duration::ZERO,
                    max_due_frames: 8,
                },
            )
            .unwrap();
        let monitor = QgsQncMonitorProjection::from_slots(&buffer.slots());
        assert!(monitor.prepared_descriptor_present);
        assert_eq!(monitor.source_frame, Some(0));
        assert_eq!(monitor.payload_status, QgsQncEvidenceStatus::Prepared);
        assert!(!monitor.presented);
        assert!(monitor.real_display_evidence.is_none());
    }

    #[test]
    fn qgs_qnc_evidence_view_keeps_prepared_distinct_from_verified() {
        let evidence = QgsQncEvidenceView::prepared_only();
        assert_eq!(evidence.prepared, QgsQncEvidenceStatus::Prepared);
        assert_eq!(
            evidence.submitted_to_device,
            QgsQncEvidenceStatus::NotImplemented
        );
        assert_eq!(evidence.presented, QgsQncEvidenceStatus::NotImplemented);
        assert_eq!(evidence.verified, QgsQncEvidenceStatus::NotImplemented);
        assert!(!evidence.frame_presented);
        assert!(!evidence.audio_device_verified);
        assert!(!evidence.realtime_verified);
    }

    fn qgs_test_command(
        command_id: u64,
        generation: u64,
        command: QgsQncPlayerCommand,
    ) -> QgsQncCommandEnvelope {
        QgsQncCommandEnvelope::new(
            command_id,
            Some(QgsRuntimeEventGeneration(generation)),
            Some("qnc://local/media/proxy/sample".to_string()),
            command,
            "test command",
        )
    }

    #[test]
    fn qgs_session_command_accepts_correct_generation() {
        let mut session = QgsQncSessionCommandExecutor::new(sample_qgs_input_plan()).unwrap();
        let result = session.execute(&qgs_test_command(
            1,
            0,
            QgsQncPlayerCommand::LoadPreparedInput,
        ));
        assert!(result.outcome.accepted);
        assert_eq!(result.passive_view.loaded_source_count, 1);
        assert!(result
            .projected_events
            .iter()
            .any(|event| event.event.kind == QgsQncProjectedEventKind::SourceLoaded));
    }

    #[test]
    fn qgs_session_command_rejects_wrong_generation_without_mutating() {
        let mut session = QgsQncSessionCommandExecutor::new(sample_qgs_input_plan()).unwrap();
        let before = session.passive_view();
        let result = session.execute(&qgs_test_command(
            1,
            9,
            QgsQncPlayerCommand::LoadPreparedInput,
        ));
        assert!(!result.outcome.accepted);
        assert_eq!(result.outcome.reason, Some("runtime generation mismatch"));
        assert!(result.projected_events.is_empty());
        assert_eq!(session.passive_view(), before);
    }

    #[test]
    fn qgs_session_rejected_play_before_ready_does_not_mutate_state() {
        let mut session = QgsQncSessionCommandExecutor::new(sample_qgs_input_plan()).unwrap();
        session.execute(&qgs_test_command(
            1,
            0,
            QgsQncPlayerCommand::LoadPreparedInput,
        ));
        let before = session.passive_view();
        let result = session.execute(&qgs_test_command(2, 0, QgsQncPlayerCommand::Play));
        assert!(!result.outcome.accepted);
        assert_eq!(result.outcome.reason, Some("not ready"));
        assert!(result.projected_events.is_empty());
        assert_eq!(session.passive_view(), before);
    }

    #[test]
    fn qgs_session_projected_events_and_passive_view_follow_commands() {
        let mut session = QgsQncSessionCommandExecutor::new(sample_qgs_input_plan()).unwrap();
        let commands = [
            QgsQncPlayerCommand::LoadPreparedInput,
            QgsQncPlayerCommand::PreloadSource,
            QgsQncPlayerCommand::SetActiveSource,
            QgsQncPlayerCommand::SetActiveRange {
                start_frame: 0,
                end_frame: 50,
            },
            QgsQncPlayerCommand::Cue { frame: 0 },
            QgsQncPlayerCommand::PrepareAnchor,
        ];
        let mut projected_count = 0;
        for (index, command) in commands.into_iter().enumerate() {
            let result = session.execute(&qgs_test_command(index as u64, 0, command));
            assert!(result.outcome.accepted);
            projected_count += result.projected_events.len();
        }
        let view = session.passive_view();
        assert!(view.readiness.play_ready);
        assert_eq!(view.loaded_source_count, 1);
        assert!(projected_count > 0);
        assert!(!view.private_path_exposed);
    }

    #[test]
    fn qgs_session_tick_prepare_is_prepared_not_realtime_or_presented() {
        let mut session = QgsQncSessionCommandExecutor::new(sample_qgs_input_plan()).unwrap();
        for (index, command) in [
            QgsQncPlayerCommand::LoadPreparedInput,
            QgsQncPlayerCommand::PreloadSource,
            QgsQncPlayerCommand::SetActiveSource,
            QgsQncPlayerCommand::SetActiveRange {
                start_frame: 0,
                end_frame: 50,
            },
            QgsQncPlayerCommand::Cue { frame: 0 },
            QgsQncPlayerCommand::PrepareAnchor,
            QgsQncPlayerCommand::Play,
            QgsQncPlayerCommand::TickPrepare { carrier_frame: 0 },
        ]
        .into_iter()
        .enumerate()
        {
            let result = session.execute(&qgs_test_command(index as u64, 0, command));
            assert!(result.outcome.accepted, "{:?}", result.outcome);
        }
        let view = session.passive_view();
        assert_eq!(view.prepared_buffer.prepared_frame_count, 6);
        assert_eq!(
            view.prepared_buffer.payload_evidence,
            QgsQncEvidenceStatus::Prepared
        );
        assert!(!view.evidence.realtime_verified);
        assert!(!view.evidence.frame_presented);
        assert!(!view.evidence.audio_device_verified);
    }

    #[test]
    fn qgs_session_private_path_command_is_rejected_without_exposure() {
        let mut session = QgsQncSessionCommandExecutor::new(sample_qgs_input_plan()).unwrap();
        let command = QgsQncCommandEnvelope::new(
            1,
            Some(QgsRuntimeEventGeneration(0)),
            Some("file:///tmp/private.MXF".to_string()),
            QgsQncPlayerCommand::LoadPreparedInput,
            "private path attempt",
        );
        let result = session.execute(&command);
        assert!(!result.outcome.accepted);
        assert_eq!(result.outcome.reason, Some("command exposes private path"));
        assert!(result.projected_events.is_empty());
        assert!(!result.passive_view.private_path_exposed);
    }

    fn qgs_test_session_runtime(max_queued_commands: usize) -> QgsSessionRuntime {
        QgsSessionRuntime::new(
            sample_qgs_input_plan(),
            QgsSessionRuntimeConfig {
                queue_limits: QgsSessionCommandQueueLimits {
                    max_queued_commands,
                },
                duplicate_policy: QgsSessionDuplicateCommandPolicy::RejectStrict,
            },
        )
        .unwrap()
    }

    #[test]
    fn qgs_session_runtime_enqueue_within_limit_is_accepted() {
        let mut runtime = qgs_test_session_runtime(2);
        assert_eq!(
            runtime.enqueue(qgs_test_command(
                1,
                0,
                QgsQncPlayerCommand::LoadPreparedInput
            )),
            Ok(QgsSessionCommandStatus::Enqueued)
        );
        assert_eq!(runtime.queued_len(), 1);
        assert_eq!(runtime.snapshot().queued_commands, 1);
    }

    #[test]
    fn qgs_session_runtime_enqueue_over_limit_is_rejected_without_mutation() {
        let mut runtime = qgs_test_session_runtime(1);
        runtime
            .enqueue(qgs_test_command(
                1,
                0,
                QgsQncPlayerCommand::LoadPreparedInput,
            ))
            .unwrap();
        let before = runtime.snapshot();
        assert_eq!(
            runtime.enqueue(qgs_test_command(2, 0, QgsQncPlayerCommand::PreloadSource)),
            Err(QgsSessionQueueRejection::QueueFull)
        );
        let after = runtime.snapshot();
        assert_eq!(after.queued_commands, before.queued_commands);
        assert_eq!(after.executed_commands, before.executed_commands);
        assert_eq!(after.passive_view, before.passive_view);
    }

    #[test]
    fn qgs_session_runtime_executes_fifo_and_records_deterministic_transcript() {
        let mut runtime = qgs_test_session_runtime(4);
        for (id, command) in [
            QgsQncPlayerCommand::LoadPreparedInput,
            QgsQncPlayerCommand::PreloadSource,
            QgsQncPlayerCommand::SetActiveSource,
        ]
        .into_iter()
        .enumerate()
        {
            runtime
                .enqueue(qgs_test_command(id as u64 + 1, 0, command))
                .unwrap();
        }
        let report = runtime.drain_queue();
        assert_eq!(report.executed, 3);
        assert_eq!(report.accepted, 3);
        assert_eq!(report.transcript.command_ids(), vec![1, 2, 3]);
        assert_eq!(report.final_snapshot.executed_commands, 3);
        assert!(!report.final_snapshot.private_path_exposed);
    }

    #[test]
    fn qgs_session_runtime_rejects_duplicate_queued_command_id() {
        let mut runtime = qgs_test_session_runtime(4);
        runtime
            .enqueue(qgs_test_command(
                1,
                0,
                QgsQncPlayerCommand::LoadPreparedInput,
            ))
            .unwrap();
        assert_eq!(
            runtime.enqueue(qgs_test_command(1, 0, QgsQncPlayerCommand::PreloadSource)),
            Err(QgsSessionQueueRejection::DuplicateCommandId)
        );
        assert_eq!(
            runtime.enqueue(qgs_test_command(1, 0, QgsQncPlayerCommand::SetActiveSource)),
            Err(QgsSessionQueueRejection::DuplicateCommandId)
        );
    }

    #[test]
    fn qgs_session_runtime_rejects_duplicate_executed_command_id() {
        let mut runtime = qgs_test_session_runtime(4);
        runtime
            .enqueue(qgs_test_command(
                1,
                0,
                QgsQncPlayerCommand::LoadPreparedInput,
            ))
            .unwrap();
        runtime.execute_next().unwrap();
        assert_eq!(
            runtime.enqueue(qgs_test_command(1, 0, QgsQncPlayerCommand::PreloadSource)),
            Err(QgsSessionQueueRejection::DuplicateCommandId)
        );
    }

    #[test]
    fn qgs_session_runtime_rejects_stale_generation_at_execution_without_mutation() {
        let mut runtime = qgs_test_session_runtime(8);
        for (id, command) in [
            QgsQncPlayerCommand::LoadPreparedInput,
            QgsQncPlayerCommand::PreloadSource,
            QgsQncPlayerCommand::SetActiveSource,
            QgsQncPlayerCommand::SetActiveRange {
                start_frame: 0,
                end_frame: 50,
            },
            QgsQncPlayerCommand::Cue { frame: 0 },
            QgsQncPlayerCommand::PrepareAnchor,
            QgsQncPlayerCommand::CloseActiveSource,
        ]
        .into_iter()
        .enumerate()
        {
            runtime
                .enqueue(qgs_test_command(id as u64 + 1, 0, command))
                .unwrap();
        }
        runtime.drain_queue();
        let before = runtime.snapshot();
        runtime
            .enqueue(qgs_test_command(99, 0, QgsQncPlayerCommand::Stop))
            .unwrap();
        let result = runtime.execute_next().unwrap();
        assert_eq!(result.status, QgsSessionCommandStatus::ExecutedRejected);
        assert_eq!(
            result.outcome.unwrap().reason,
            Some("runtime generation mismatch")
        );
        let after = runtime.snapshot();
        assert_eq!(after.generation, before.generation);
        assert_eq!(after.passive_view, before.passive_view);
    }

    #[test]
    fn qgs_session_runtime_rejected_command_does_not_emit_false_evidence() {
        let mut runtime = qgs_test_session_runtime(2);
        runtime
            .enqueue(qgs_test_command(1, 0, QgsQncPlayerCommand::Play))
            .unwrap();
        let report = runtime.drain_queue();
        assert_eq!(report.accepted, 0);
        assert_eq!(report.rejected, 1);
        let transcript = format!("{:?}", report.transcript.entries());
        assert!(!transcript.contains("FramePresented"));
        assert!(!transcript.contains("AudioDeviceVerified"));
        assert!(!transcript.contains("RealtimeVerified"));
        assert!(!report.final_snapshot.private_path_exposed);
    }

    #[test]
    fn qgs_session_runtime_final_close_clears_active_source_and_readiness() {
        let mut runtime = qgs_test_session_runtime(12);
        for (id, command) in [
            QgsQncPlayerCommand::LoadPreparedInput,
            QgsQncPlayerCommand::PreloadSource,
            QgsQncPlayerCommand::SetActiveSource,
            QgsQncPlayerCommand::SetActiveRange {
                start_frame: 0,
                end_frame: 50,
            },
            QgsQncPlayerCommand::Cue { frame: 0 },
            QgsQncPlayerCommand::PrepareAnchor,
            QgsQncPlayerCommand::Play,
            QgsQncPlayerCommand::TickPrepare { carrier_frame: 0 },
            QgsQncPlayerCommand::Pause,
            QgsQncPlayerCommand::CloseActiveSource,
        ]
        .into_iter()
        .enumerate()
        {
            runtime
                .enqueue(qgs_test_command(id as u64 + 1, 0, command))
                .unwrap();
        }
        let report = runtime.drain_queue();
        assert_eq!(report.accepted, 10);
        let view = report.final_snapshot.passive_view;
        assert!(view.source.public_source_uri.is_none());
        assert!(!view.readiness.play_ready);
        assert!(view.transport.active_range.is_none());
        assert_eq!(view.prepared_buffer.prepared_frame_count, 0);
        assert_eq!(view.prepared_buffer.latest_discarded_frame_count, 6);
    }

    #[test]
    fn qgs_broadcast_player_core_runs_product_command_sequence() {
        let mut core = QgsBroadcastPlayerCore::new(sample_qgs_input_plan()).unwrap();
        for command in [
            QgsBroadcastPlayerCommand::LoadPreparedInput,
            QgsBroadcastPlayerCommand::Prepare {
                active_range: Some((0, 50)),
            },
            QgsBroadcastPlayerCommand::Cue { frame: 0 },
            QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 },
            QgsBroadcastPlayerCommand::Play,
            QgsBroadcastPlayerCommand::Tick { carrier_frame: 1 },
            QgsBroadcastPlayerCommand::Pause,
            QgsBroadcastPlayerCommand::Seek { frame: 2 },
            QgsBroadcastPlayerCommand::Prepare {
                active_range: Some((0, 50)),
            },
            QgsBroadcastPlayerCommand::Play,
            QgsBroadcastPlayerCommand::Stop,
            QgsBroadcastPlayerCommand::Unload,
        ] {
            let result = core.execute(command);
            assert!(result.accepted, "{:?}", result);
        }

        let snapshot = core.snapshot();
        assert_eq!(snapshot.status, QgsBroadcastPlayerStatus::Empty);
        assert_eq!(snapshot.source_mode, QgsInputPlanSourceMode::ProxyPreview);
        assert_eq!(
            snapshot.selected_representation,
            QgsPlaybackRepresentation::Proxy
        );
        assert!(core
            .events()
            .iter()
            .any(|event| event.kind == QgsBroadcastPlayerEventKind::PlayerReady));
        assert!(core
            .events()
            .iter()
            .any(|event| event.kind == QgsBroadcastPlayerEventKind::PlayerStarted));
        assert!(core
            .events()
            .iter()
            .any(|event| event.kind == QgsBroadcastPlayerEventKind::PlayerUnloaded));
        assert!(!snapshot.exposes_private_path());
    }

    #[test]
    fn qgs_broadcast_player_core_rejects_play_before_ready() {
        let mut core = QgsBroadcastPlayerCore::new(sample_qgs_input_plan()).unwrap();
        let result = core.execute(QgsBroadcastPlayerCommand::Play);
        assert!(!result.accepted);
        assert_eq!(result.reason, Some("not ready"));
        assert_eq!(result.snapshot.status, QgsBroadcastPlayerStatus::Empty);
        assert_eq!(result.snapshot.event_counters.rejected_commands, 1);
        assert!(result
            .product_events
            .iter()
            .any(|event| event.kind == QgsBroadcastPlayerEventKind::PlayerFailed));
    }

    #[test]
    fn qgs_broadcast_player_core_readiness_requires_tick_preparation() {
        let mut core = QgsBroadcastPlayerCore::new(sample_qgs_input_plan()).unwrap();
        assert!(
            core.execute(QgsBroadcastPlayerCommand::LoadPreparedInput)
                .accepted
        );
        assert!(
            core.execute(QgsBroadcastPlayerCommand::Prepare {
                active_range: Some((0, 50)),
            })
            .accepted
        );
        assert!(
            core.execute(QgsBroadcastPlayerCommand::Cue { frame: 0 })
                .accepted
        );

        let before_tick = core.snapshot();
        assert_eq!(before_tick.status, QgsBroadcastPlayerStatus::Ready);
        assert!(before_tick.readiness.transport_ready);
        assert!(!before_tick.readiness.prepared_window_ready);
        assert!(!before_tick.readiness.video_payload_ready);
        assert!(!before_tick.readiness.audio_payload_ready);

        assert!(
            core.execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        let after_tick = core.snapshot();
        assert!(after_tick.readiness.prepared_window_ready);
        assert!(after_tick.readiness.video_payload_ready);
        assert!(after_tick.readiness.audio_payload_ready);
        assert_eq!(after_tick.prepared_window.prepared_frame_count, 6);
    }

    #[test]
    fn qgs_broadcast_player_core_seek_updates_cue_without_real_output_claims() {
        let mut core = QgsBroadcastPlayerCore::new(sample_qgs_input_plan()).unwrap();
        for command in [
            QgsBroadcastPlayerCommand::LoadPreparedInput,
            QgsBroadcastPlayerCommand::Prepare {
                active_range: Some((0, 50)),
            },
            QgsBroadcastPlayerCommand::Cue { frame: 0 },
            QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 },
            QgsBroadcastPlayerCommand::Play,
            QgsBroadcastPlayerCommand::Pause,
            QgsBroadcastPlayerCommand::Seek { frame: 2 },
        ] {
            assert!(core.execute(command).accepted);
        }
        let snapshot = core.snapshot();
        assert_eq!(snapshot.status, QgsBroadcastPlayerStatus::Paused);
        assert_eq!(snapshot.position.cue_frame, Some(2));
        assert_eq!(snapshot.position.current_frame, Some(2));
        assert_eq!(
            snapshot.position.current_audio_sample_range,
            Some((1_920, 2_880))
        );
        assert!(!snapshot.device_status.visual_verified);
        assert!(!snapshot.device_status.realtime_verified);
        assert!(!snapshot.device_status.audio_device_production_verified);
        assert!(!snapshot.device_status.av_sync_verified);
    }

    #[test]
    fn qgs_broadcast_player_core_snapshot_preserves_media_and_device_policy() {
        let mut core = QgsBroadcastPlayerCore::new(sample_qgs_input_plan()).unwrap();
        for command in [
            QgsBroadcastPlayerCommand::LoadPreparedInput,
            QgsBroadcastPlayerCommand::Prepare {
                active_range: Some((0, 50)),
            },
            QgsBroadcastPlayerCommand::Cue { frame: 0 },
            QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 },
        ] {
            assert!(core.execute(command).accepted);
        }
        let snapshot = core.snapshot();
        assert_eq!(snapshot.source_mode, QgsInputPlanSourceMode::ProxyPreview);
        assert_eq!(
            snapshot.selected_representation,
            QgsPlaybackRepresentation::Proxy
        );
        assert!(snapshot.readiness.discrete_mono_audio);
        assert!(!snapshot.readiness.proxy_aac_authoritative);
        assert_eq!(
            snapshot.device_status.real_display_backend,
            "NotImplemented"
        );
        assert_eq!(
            snapshot.device_status.qnc_os_display_target,
            "Wayland + Vulkan"
        );
        assert_eq!(snapshot.device_status.x11_target, "no / legacy non-target");
        assert_eq!(
            snapshot.device_status.selection.policy,
            QgsDeviceSelectionPolicy::PreviewOnQncOs
        );
        assert_eq!(
            snapshot.device_status.selection.selected_video_backend,
            QgsVideoPresenterBackendKind::WaylandVulkanPresenter
        );
        assert_eq!(
            snapshot.device_status.selection.selected_audio_backend,
            QgsAudioOutputBackendKind::PipeWireProductionFuture
        );
        assert_eq!(snapshot.device_status.real_audio_backend, "NotImplemented");
        assert!(!snapshot.readiness.real_display_ready);
        assert!(!snapshot.readiness.audio_device_verified);
        assert!(!snapshot.readiness.visual_verified);
        assert!(!snapshot.readiness.realtime_verified);
        assert!(!snapshot.readiness.av_sync_verified);
        assert!(!snapshot.exposes_private_path());
        assert!(core
            .events()
            .iter()
            .all(|event| !event.exposes_private_path()));
    }

    fn qgs_test_control_surface() -> QgsQncControlSurface {
        let assembly =
            QgsBroadcastPlayerAssembly::from_input_plan(sample_qgs_input_plan()).unwrap();
        QgsQncControlSurface::from_assembly(&assembly, "test-session").unwrap()
    }

    fn qgs_control_request(
        id: &str,
        surface: &QgsQncControlSurface,
        command: QgsQncCommandKind,
        payload: QgsQncCommandPayload,
    ) -> QgsQncCommandRequestEnvelope {
        QgsQncCommandRequestEnvelope::new(id, command)
            .with_expected_generation(surface.generation())
            .with_payload(payload)
    }

    #[test]
    fn qgs_qnc_control_surface_echoes_command_id_and_increments_generation() {
        let mut surface = qgs_test_control_surface();
        let request = qgs_control_request(
            "cmd-0001",
            &surface,
            QgsQncCommandKind::LoadPreparedInput,
            QgsQncCommandPayload::Empty,
        );
        let reply = surface.handle_command(request);
        assert!(reply.accepted);
        assert_eq!(reply.command_id.0, "cmd-0001");
        assert_eq!(reply.generation_before, QgsQncGeneration(0));
        assert_eq!(reply.generation_after, QgsQncGeneration(1));
        assert_eq!(surface.generation(), QgsQncGeneration(1));
        assert!(reply.state_mutated);
        assert!(reply
            .events
            .iter()
            .any(|event| event.event_kind == QgsQncControlEventKind::SourceLoaded));
        assert!(!reply.exposes_private_path());
    }

    #[test]
    fn qgs_qnc_control_surface_snapshot_does_not_increment_generation() {
        let mut surface = qgs_test_control_surface();
        let request = qgs_control_request(
            "cmd-snapshot",
            &surface,
            QgsQncCommandKind::Snapshot,
            QgsQncCommandPayload::Empty,
        );
        let reply = surface.handle_command(request);
        assert!(reply.accepted);
        assert_eq!(reply.generation_before, QgsQncGeneration(0));
        assert_eq!(reply.generation_after, QgsQncGeneration(0));
        assert!(!reply.state_mutated);
        assert!(reply
            .events
            .iter()
            .any(|event| event.event_kind == QgsQncControlEventKind::SnapshotReported));
    }

    #[test]
    fn qgs_qnc_control_surface_rejects_stale_generation_without_mutation() {
        let mut surface = qgs_test_control_surface();
        let before = surface.snapshot();
        let reply = surface.handle_command(
            QgsQncCommandRequestEnvelope::new("cmd-stale", QgsQncCommandKind::LoadPreparedInput)
                .with_expected_generation(QgsQncGeneration(99)),
        );
        assert!(!reply.accepted);
        assert_eq!(reply.rejection_reason, Some("StaleGeneration"));
        assert_eq!(reply.generation_before, QgsQncGeneration(0));
        assert_eq!(reply.generation_after, QgsQncGeneration(0));
        assert!(!reply.state_mutated);
        assert_eq!(reply.snapshot.status, before.status);
        assert_eq!(reply.snapshot.source_loaded, before.source_loaded);
        assert_eq!(reply.snapshot.current_frame, before.current_frame);
        assert!(reply.fault.is_some());
        assert!(reply
            .events
            .iter()
            .any(|event| event.event_kind == QgsQncControlEventKind::CommandRejected));
        assert!(!reply.exposes_private_path());
    }

    #[test]
    fn qgs_qnc_control_surface_snapshot_preserves_source_and_device_truth() {
        let mut surface = qgs_test_control_surface();
        for (id, command, payload) in [
            (
                "cmd-load",
                QgsQncCommandKind::LoadPreparedInput,
                QgsQncCommandPayload::Empty,
            ),
            (
                "cmd-prepare",
                QgsQncCommandKind::Prepare,
                QgsQncCommandPayload::Empty,
            ),
            (
                "cmd-cue",
                QgsQncCommandKind::Cue,
                QgsQncCommandPayload::Frame { frame: 0 },
            ),
            (
                "cmd-preroll",
                QgsQncCommandKind::Preroll,
                QgsQncCommandPayload::Preroll { target_frame: None },
            ),
        ] {
            let request = qgs_control_request(id, &surface, command, payload);
            assert!(surface.handle_command(request).accepted);
        }
        let snapshot = surface.snapshot();
        assert_eq!(
            snapshot.source_mode,
            Some(QgsInputPlanSourceMode::ProxyPreview)
        );
        assert_eq!(
            snapshot.picture_representation,
            Some(QgsPlaybackRepresentation::Proxy)
        );
        assert_eq!(snapshot.authoritative_audio_source, "original MXF");
        assert!(!snapshot.proxy_aac_authoritative);
        assert_eq!(
            snapshot.broadcast_audio_model,
            "discrete original MXF mono lanes"
        );
        assert_eq!(
            snapshot.device_status.qnc_os_display_target,
            "Wayland + Vulkan"
        );
        assert_eq!(
            snapshot.device_status.x11_target_status,
            "no / legacy non-target"
        );
        assert!(!snapshot.device_status.visual_verified);
        assert!(!snapshot.device_status.realtime_verified);
        assert!(!snapshot.device_status.audio_device_verified);
        assert!(!snapshot.device_status.av_sync_verified);
        assert!(!snapshot.exposes_private_path());
    }

    #[test]
    fn qgs_qnc_control_surface_rejected_command_returns_snapshot_and_fault() {
        let mut surface = qgs_test_control_surface();
        let request = qgs_control_request(
            "cmd-play",
            &surface,
            QgsQncCommandKind::Play,
            QgsQncCommandPayload::Play {
                frame_count: Some(1),
            },
        );
        let reply = surface.handle_command(request);
        assert!(!reply.accepted);
        assert_eq!(reply.generation_before, reply.generation_after);
        assert_eq!(reply.snapshot.status, QgsBroadcastPlayerStatus::Empty);
        let fault = reply.fault.as_ref().expect("fault record");
        assert_eq!(fault.fault_kind, "PrepareRequired");
        assert_eq!(fault.command_id.as_ref().unwrap().0, "cmd-play");
        assert!(!fault.exposes_private_path());
        assert!(reply
            .events
            .iter()
            .all(|event| event.event_kind != QgsQncControlEventKind::Started));
        let debug = format!("{reply:?}");
        assert!(!debug.contains("FramePresented"));
        assert!(!debug.contains("AudioDeviceVerified"));
    }

    #[test]
    fn qgs_qnc_control_surface_default_sequence_projects_events() {
        let mut surface = qgs_test_control_surface();
        let sequence = [
            (
                QgsQncCommandKind::LoadPreparedInput,
                QgsQncCommandPayload::Empty,
            ),
            (QgsQncCommandKind::Prepare, QgsQncCommandPayload::Empty),
            (
                QgsQncCommandKind::Cue,
                QgsQncCommandPayload::Frame { frame: 0 },
            ),
            (
                QgsQncCommandKind::Preroll,
                QgsQncCommandPayload::Preroll { target_frame: None },
            ),
            (
                QgsQncCommandKind::Play,
                QgsQncCommandPayload::Play {
                    frame_count: Some(3),
                },
            ),
            (QgsQncCommandKind::Pause, QgsQncCommandPayload::Empty),
            (
                QgsQncCommandKind::Seek,
                QgsQncCommandPayload::Frame { frame: 4 },
            ),
            (
                QgsQncCommandKind::Preroll,
                QgsQncCommandPayload::Preroll { target_frame: None },
            ),
            (
                QgsQncCommandKind::Play,
                QgsQncCommandPayload::Play {
                    frame_count: Some(2),
                },
            ),
            (QgsQncCommandKind::Stop, QgsQncCommandPayload::Empty),
            (QgsQncCommandKind::Unload, QgsQncCommandPayload::Empty),
        ];
        let mut event_sequences = Vec::new();
        for (index, (command, payload)) in sequence.into_iter().enumerate() {
            let request =
                qgs_control_request(&format!("cmd-{index:04}"), &surface, command, payload);
            let reply = surface.handle_command(request);
            assert!(reply.accepted, "{reply:?}");
            event_sequences.extend(reply.events.iter().map(|event| event.sequence));
            assert!(!reply.private_path_exposed);
        }
        assert!(event_sequences
            .windows(2)
            .all(|window| window[0] < window[1]));
        let snapshot = surface.snapshot();
        assert_eq!(snapshot.status, QgsBroadcastPlayerStatus::Empty);
        assert!(!snapshot.source_loaded);
        assert!(!snapshot.private_path_exposed);
    }

    #[test]
    fn qgs_device_selection_diagnostic_only_selects_diagnostic_backends() {
        let selection = QgsDeviceBackendSelector::select(QgsDeviceSelectionPolicy::DiagnosticOnly);
        assert_eq!(
            selection.selected_video_backend,
            QgsVideoPresenterBackendKind::GpuReadbackDiagnostic
        );
        assert_eq!(
            selection.selected_video_availability,
            QgsDeviceBackendAvailability::AvailableDiagnosticOnly
        );
        assert_eq!(
            selection.selected_audio_backend,
            QgsAudioOutputBackendKind::TestAudioSink
        );
        assert!(!selection.real_display_ready);
        assert!(!selection.visual_verified);
        assert!(!selection.realtime_verified);
        assert!(!selection.audio_device_production_verified);
        assert!(!selection.av_sync_verified);
        assert!(!selection.exposes_private_path());
    }

    #[test]
    fn qgs_device_selection_preview_qnc_os_targets_wayland_not_x11() {
        let selection = QgsDeviceBackendSelector::select(QgsDeviceSelectionPolicy::PreviewOnQncOs);
        assert_eq!(
            selection.selected_video_backend,
            QgsVideoPresenterBackendKind::WaylandVulkanPresenter
        );
        assert_eq!(
            selection.selected_video_availability,
            QgsDeviceBackendAvailability::NotImplemented
        );
        assert_ne!(
            selection.selected_video_backend,
            QgsVideoPresenterBackendKind::X11LegacyNonTarget
        );
        let x11 = selection
            .video_candidates
            .iter()
            .find(|candidate| candidate.kind == QgsVideoPresenterBackendKind::X11LegacyNonTarget)
            .expect("x11 candidate");
        assert_eq!(
            x11.availability,
            QgsDeviceBackendAvailability::UnsupportedForQncOs
        );
        assert_eq!(selection.qnc_os_display_target, "Wayland + Vulkan");
        assert_eq!(selection.x11_target, "no / legacy non-target");
        assert!(!selection.real_display_ready);
    }

    #[test]
    fn qgs_device_selection_headless_ci_uses_non_real_display_boundary() {
        let selection = QgsDeviceBackendSelector::select(QgsDeviceSelectionPolicy::HeadlessCi);
        assert_eq!(
            selection.selected_video_backend,
            QgsVideoPresenterBackendKind::TestPresenter
        );
        assert_eq!(
            selection.selected_video_availability,
            QgsDeviceBackendAvailability::AvailableDiagnosticOnly
        );
        assert_eq!(
            selection.selected_audio_backend,
            QgsAudioOutputBackendKind::NoAudioOutput
        );
        assert!(!selection.real_display_ready);
        assert!(!selection.real_audio_backend_ready);
    }

    #[test]
    fn qgs_device_selection_pipewire_prototypes_are_not_production_verified() {
        let selection = QgsDeviceBackendSelector::select(QgsDeviceSelectionPolicy::PreviewOnQncOs);
        for kind in [
            QgsAudioOutputBackendKind::PipeWireCommandPrototype,
            QgsAudioOutputBackendKind::PipeWireNativePrototype,
        ] {
            let candidate = selection
                .audio_candidates
                .iter()
                .find(|candidate| candidate.kind == kind)
                .expect("pipewire prototype candidate");
            assert_eq!(
                candidate.availability,
                QgsDeviceBackendAvailability::NotProductionVerified
            );
        }
        assert_eq!(
            selection.selected_audio_backend,
            QgsAudioOutputBackendKind::PipeWireProductionFuture
        );
        assert_eq!(
            selection.selected_audio_availability,
            QgsDeviceBackendAvailability::NotImplemented
        );
        assert!(!selection.audio_device_production_verified);
    }

    fn qgs_operational_runtime_ready(range: (u64, u64)) -> QgsBroadcastPlayerOperationalRuntime {
        let mut runtime = QgsBroadcastPlayerOperationalRuntime::new(
            sample_qgs_input_plan(),
            QgsOperationalRuntimeConfig::default(),
        )
        .unwrap();
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::LoadPreparedInput)
                .accepted
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Prepare {
                    active_range: Some(range),
                })
                .accepted
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Cue { frame: range.0 })
                .accepted
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        runtime
    }

    #[test]
    fn qgs_operational_runtime_rejects_illegal_play_while_empty() {
        let mut runtime = QgsBroadcastPlayerOperationalRuntime::new(
            sample_qgs_input_plan(),
            QgsOperationalRuntimeConfig::default(),
        )
        .unwrap();
        let result = runtime.execute(QgsBroadcastPlayerCommand::Play);
        assert!(!result.accepted);
        assert_eq!(
            result.reason,
            Some("command is illegal while player is Empty")
        );
        assert_eq!(result.snapshot.rejected_commands, 1);
        assert!(result
            .snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == QgsOperationalFaultKind::SourceNotLoaded));
        let last_fault = result
            .snapshot
            .fault_snapshot
            .last_fault
            .as_ref()
            .expect("last fault");
        assert_eq!(last_fault.kind, QgsOperationalFaultKind::SourceNotLoaded);
        assert_eq!(
            last_fault.severity,
            QgsBroadcastPlayerFaultSeverity::Recoverable
        );
        assert_eq!(
            last_fault.recovery_action,
            QgsBroadcastPlayerRecoveryAction::UnloadAndReload
        );
        assert!(!result.snapshot.exposes_private_path());
    }

    #[test]
    fn qgs_operational_runtime_rejects_play_before_ready() {
        let mut runtime = QgsBroadcastPlayerOperationalRuntime::new(
            sample_qgs_input_plan(),
            QgsOperationalRuntimeConfig::default(),
        )
        .unwrap();
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::LoadPreparedInput)
                .accepted
        );
        let result = runtime.execute(QgsBroadcastPlayerCommand::Play);
        assert!(!result.accepted);
        assert_eq!(result.reason, Some("play requires Ready"));
        assert_eq!(
            result.snapshot.operational_status,
            QgsBroadcastPlayerStatus::Loaded
        );
        let last_fault = result
            .snapshot
            .fault_snapshot
            .last_fault
            .as_ref()
            .expect("last fault");
        assert_eq!(last_fault.kind, QgsOperationalFaultKind::PrepareRequired);
        assert_eq!(
            last_fault.recovery_action,
            QgsBroadcastPlayerRecoveryAction::PrepareAgain
        );
    }

    #[test]
    fn qgs_operational_runtime_tick_advances_only_while_playing() {
        let mut runtime = qgs_operational_runtime_ready((0, 50));
        assert_eq!(runtime.snapshot().current_frame, Some(0));
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Play).accepted);
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        assert_eq!(runtime.snapshot().current_frame, Some(1));
        assert_eq!(
            runtime.snapshot().current_audio_sample_range,
            Some((960, 1_920))
        );
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Pause).accepted);
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        assert_eq!(runtime.snapshot().current_frame, Some(1));
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Stop).accepted);
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        assert_eq!(runtime.snapshot().current_frame, Some(1));
    }

    #[test]
    fn qgs_operational_runtime_seek_stop_unload_sequence_preserves_source_policy() {
        let mut runtime = qgs_operational_runtime_ready((0, 50));
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Play).accepted);
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Pause).accepted);
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Seek { frame: 4 })
                .accepted
        );
        assert_eq!(runtime.snapshot().current_frame, Some(4));
        assert_eq!(
            runtime.snapshot().current_audio_sample_range,
            Some((3_840, 4_800))
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Prepare {
                    active_range: Some((0, 50)),
                })
                .accepted
        );
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Stop).accepted);
        assert_eq!(
            runtime.snapshot().operational_status,
            QgsBroadcastPlayerStatus::Stopped
        );
        assert!(runtime.snapshot().player.readiness.source_loaded);
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Unload).accepted);
        assert_eq!(
            runtime.snapshot().operational_status,
            QgsBroadcastPlayerStatus::Empty
        );
        assert!(!runtime.snapshot().player.readiness.source_loaded);
    }

    #[test]
    fn qgs_operational_runtime_completion_occurs_at_active_range_end() {
        let mut runtime = qgs_operational_runtime_ready((0, 2));
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Play).accepted);
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        assert_eq!(
            runtime.snapshot().operational_status,
            QgsBroadcastPlayerStatus::Playing
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        let snapshot = runtime.snapshot();
        assert_eq!(
            snapshot.operational_status,
            QgsBroadcastPlayerStatus::Completed
        );
        assert!(snapshot.completed);
        assert_eq!(snapshot.current_frame, Some(1));
        assert!(snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == QgsOperationalFaultKind::EndOfRangeReached));
        assert!(snapshot.player.readiness.source_loaded);
    }

    #[test]
    fn qgs_operational_runtime_seek_outside_active_range_is_rejected() {
        let mut runtime = qgs_operational_runtime_ready((0, 10));
        let result = runtime.execute(QgsBroadcastPlayerCommand::Seek { frame: 99 });
        assert!(!result.accepted);
        assert_eq!(result.reason, Some("cue failed"));
        assert_eq!(result.snapshot.current_frame, Some(0));
        assert!(result
            .snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == QgsOperationalFaultKind::SeekOutsideActiveRange));
        assert_eq!(
            result.snapshot.fault_snapshot.recommended_recovery_action,
            Some(QgsBroadcastPlayerRecoveryAction::SeekToValidRange)
        );
    }

    #[test]
    fn qgs_operational_runtime_snapshot_preserves_non_claims_and_media_policy() {
        let runtime = qgs_operational_runtime_ready((0, 50));
        let snapshot = runtime.snapshot();
        assert_eq!(
            snapshot.player.source_mode,
            QgsInputPlanSourceMode::ProxyPreview
        );
        assert!(!snapshot.player.readiness.proxy_aac_authoritative);
        assert!(snapshot.player.readiness.discrete_mono_audio);
        assert_eq!(
            snapshot
                .player
                .device_status
                .selection
                .selected_video_backend,
            QgsVideoPresenterBackendKind::WaylandVulkanPresenter
        );
        assert_ne!(
            snapshot
                .player
                .device_status
                .selection
                .selected_video_backend,
            QgsVideoPresenterBackendKind::X11LegacyNonTarget
        );
        assert!(!snapshot.player.readiness.real_display_ready);
        assert!(!snapshot.player.readiness.audio_device_verified);
        assert!(!snapshot.player.readiness.visual_verified);
        assert!(!snapshot.player.readiness.realtime_verified);
        assert!(!snapshot.player.readiness.av_sync_verified);
        assert!(!snapshot.buffer_health.underrun);
        assert!(snapshot.buffer_health.prepared_frame_count > 0);
        assert!(snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == QgsOperationalFaultKind::BackendNotImplemented));
        assert!(snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == QgsOperationalFaultKind::AudioOutputNotProductionVerified));
        assert!(snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == QgsOperationalFaultKind::VisualVerificationUnavailable));
        assert!(snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == QgsOperationalFaultKind::RealtimeVerificationUnavailable));
        assert!(snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == QgsOperationalFaultKind::AvSyncNotVerified));
        assert_eq!(snapshot.fault_snapshot.fatal_fault_count, 0);
        assert!(snapshot.fault_snapshot.recoverable_fault_count > 0);
        assert!(!snapshot.exposes_private_path());
    }

    #[test]
    fn qgs_fault_snapshot_counts_last_fault_and_recovery_action() {
        let mut runtime = qgs_operational_runtime_ready((0, 10));
        let before = runtime.snapshot();
        let result = runtime.execute(QgsBroadcastPlayerCommand::Seek { frame: 99 });
        assert!(!result.accepted);
        assert_eq!(
            result.snapshot.operational_status,
            before.operational_status
        );
        assert_eq!(result.snapshot.current_frame, before.current_frame);
        assert_eq!(
            result.snapshot.player.readiness.source_loaded,
            before.player.readiness.source_loaded
        );

        let fault_snapshot = &result.snapshot.fault_snapshot;
        assert!(fault_snapshot
            .fault_counters
            .iter()
            .any(
                |counter| counter.kind == QgsOperationalFaultKind::SeekOutsideActiveRange
                    && counter.count == 1
            ));
        assert_eq!(fault_snapshot.fatal_fault_count, 0);
        assert!(fault_snapshot.recoverable_fault_count > 0);
        assert_eq!(
            fault_snapshot.recommended_recovery_action,
            Some(QgsBroadcastPlayerRecoveryAction::SeekToValidRange)
        );
        assert!(!fault_snapshot.private_path_exposed);
    }

    #[test]
    fn qgs_fault_policy_keeps_backend_warnings_non_fatal() {
        let runtime = qgs_operational_runtime_ready((0, 50));
        let snapshot = runtime.snapshot();
        assert!(snapshot
            .fault_snapshot
            .backend_device_warnings
            .iter()
            .any(
                |fault| fault.kind == QgsOperationalFaultKind::BackendNotImplemented
                    && fault.severity == QgsBroadcastPlayerFaultSeverity::Warning
                    && fault.recovery_action
                        == QgsBroadcastPlayerRecoveryAction::WaitForBackendImplementation
            ));
        assert!(snapshot
            .fault_snapshot
            .backend_device_warnings
            .iter()
            .any(
                |fault| fault.kind == QgsOperationalFaultKind::AudioOutputNotProductionVerified
                    && fault.recovery_action
                        == QgsBroadcastPlayerRecoveryAction::UseDiagnosticBackendOnly
            ));
        assert_eq!(snapshot.fault_snapshot.fatal_fault_count, 0);
        assert_ne!(
            snapshot
                .player
                .device_status
                .selection
                .selected_video_backend,
            QgsVideoPresenterBackendKind::X11LegacyNonTarget
        );
        assert!(!snapshot.player.readiness.proxy_aac_authoritative);
        assert!(snapshot.player.readiness.discrete_mono_audio);
        assert!(!snapshot.player.readiness.visual_verified);
        assert!(!snapshot.player.readiness.realtime_verified);
        assert!(!snapshot.player.readiness.audio_device_verified);
        assert!(!snapshot.player.readiness.av_sync_verified);
    }

    #[test]
    fn qgs_running_runtime_demo_sequence_advances_pauses_seeks_stops_and_unloads() {
        let plan = sample_qgs_input_plan();
        let active_end = plan.video_source.duration_frames;
        let mut runtime =
            QgsBroadcastPlayerOperationalRuntime::new(plan, QgsOperationalRuntimeConfig::default())
                .unwrap();

        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::LoadPreparedInput)
                .accepted
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Prepare {
                    active_range: Some((0, active_end))
                })
                .accepted
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Cue { frame: 0 })
                .accepted
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        assert_eq!(
            runtime.snapshot().operational_status,
            QgsBroadcastPlayerStatus::Ready
        );
        assert!(runtime.snapshot().buffer_health.prepared_frame_count > 0);
        assert!(runtime.execute(QgsBroadcastPlayerCommand::Play).accepted);

        for _ in 0..25 {
            runtime.execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 });
        }
        assert_eq!(runtime.snapshot().current_frame, Some(25));
        assert_eq!(
            runtime.snapshot().current_audio_sample_range,
            Some((24_000, 24_960))
        );

        assert!(runtime.execute(QgsBroadcastPlayerCommand::Pause).accepted);
        let paused_frame = runtime.snapshot().current_frame;
        runtime.execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 });
        assert_eq!(runtime.snapshot().current_frame, paused_frame);

        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Seek { frame: 50 })
                .accepted
        );
        assert_eq!(runtime.snapshot().current_frame, Some(50));
        assert_eq!(
            runtime.snapshot().current_audio_sample_range,
            Some((48_000, 48_960))
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Prepare {
                    active_range: Some((0, active_end))
                })
                .accepted
        );
        assert!(
            runtime
                .execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 })
                .accepted
        );
        let prepared = runtime.snapshot();
        assert!(
            prepared
                .buffer_health
                .prepared_start_frame
                .unwrap_or(u64::MAX)
                <= 50
        );
        assert!(
            prepared
                .buffer_health
                .prepared_end_frame_exclusive
                .unwrap_or_default()
                > 50
        );
        assert!(!prepared.buffer_health.underrun);

        assert!(runtime.execute(QgsBroadcastPlayerCommand::Play).accepted);
        for _ in 0..25 {
            runtime.execute(QgsBroadcastPlayerCommand::Tick { carrier_frame: 0 });
        }
        assert_eq!(runtime.snapshot().current_frame, Some(75));

        assert!(runtime.execute(QgsBroadcastPlayerCommand::Stop).accepted);
        assert_eq!(
            runtime.snapshot().operational_status,
            QgsBroadcastPlayerStatus::Stopped
        );
        assert!(runtime.snapshot().player.readiness.source_loaded);

        assert!(runtime.execute(QgsBroadcastPlayerCommand::Unload).accepted);
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.operational_status, QgsBroadcastPlayerStatus::Empty);
        assert!(!snapshot.player.readiness.source_loaded);
        assert!(!snapshot.exposes_private_path());
        assert_eq!(
            snapshot.player.device_status.real_display_backend,
            QgsDeviceBackendAvailability::NotImplemented.label()
        );
        assert!(!snapshot.player.readiness.visual_verified);
        assert!(!snapshot.player.readiness.realtime_verified);
        assert!(!snapshot.player.readiness.audio_device_verified);
        assert!(!snapshot.player.readiness.av_sync_verified);
        assert_eq!(
            snapshot.player.device_status.x11_target,
            "no / legacy non-target"
        );
        assert!(!snapshot.player.readiness.proxy_aac_authoritative);
        assert!(snapshot.player.readiness.discrete_mono_audio);
    }

    #[test]
    fn exact_integer_rates_use_integer_timing() {
        let rate = RationalRate::new(50, 1).expect("rate");
        assert_eq!(rate.frame_offset(1).unwrap(), Duration::from_millis(20));
        assert_eq!(rate.frame_offset(106).unwrap(), Duration::from_millis(2120));

        let rate = RationalRate::new(25, 1).expect("rate");
        assert_eq!(rate.frame_offset(1).unwrap(), Duration::from_millis(40));
    }

    #[test]
    fn rates_are_reduced_for_reporting_without_losing_timing() {
        let rate = RationalRate::new(5_300_000, 106_000).expect("rate");
        assert_eq!(rate.numerator(), 50);
        assert_eq!(rate.denominator(), 1);
        assert_eq!(rate.frame_offset(106).unwrap(), Duration::from_millis(2120));
    }

    #[test]
    fn fractional_rate_uses_rational_math() {
        let rate = RationalRate::new(30_000, 1001).expect("rate");
        assert_eq!(rate.frame_offset(30).unwrap(), Duration::from_millis(1001));
    }

    #[test]
    fn bounded_queue_reports_backpressure_without_growth() {
        let mut queue = BoundedQueue::new(2).expect("queue");
        assert!(queue.try_push(1).is_ok());
        assert!(queue.try_push(2).is_ok());
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.try_push(3), Err(3));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.stats().peak_depth, 2);
        assert_eq!(queue.stats().backpressure_events, 1);
    }

    #[test]
    fn audio_format_rejects_invalid_values() {
        assert!(AudioFormat {
            sample_rate: 48_000,
            channels: 2,
            sample_format: AudioSampleFormat::PcmSignedInt {
                bits_per_sample: 24
            },
        }
        .validate()
        .is_ok());
        assert!(AudioFormat {
            sample_rate: 0,
            channels: 2,
            sample_format: AudioSampleFormat::PcmSignedInt {
                bits_per_sample: 24
            },
        }
        .validate()
        .is_err());
        assert!(AudioFormat {
            sample_rate: 48_000,
            channels: 1,
            sample_format: AudioSampleFormat::PcmSignedInt { bits_per_sample: 0 },
        }
        .validate()
        .is_err());
    }

    #[test]
    fn audio_duration_uses_exact_sample_rate_math() {
        assert_eq!(
            duration_from_audio_samples(48_000, 48_000).unwrap(),
            Duration::from_secs(1)
        );
        assert_eq!(
            audio_samples_for_duration(Duration::from_millis(2120), 48_000).unwrap(),
            101_760
        );
    }

    #[test]
    fn pcm_packet_byte_size_preserves_24_bit_payloads() {
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };

        assert_eq!(pcm_payload_byte_len(960, 1, format).unwrap(), 2880);
        assert_eq!(pcm_payload_byte_len(960, 2, format).unwrap(), 5760);

        let packet = PcmAudioPacket::new(
            3,
            0,
            Duration::ZERO,
            Duration::from_millis(20),
            960,
            format,
            vec![0_u8; 2880],
        )
        .unwrap();
        assert_eq!(packet.payload_bytes(), 2880);
        assert!(PcmAudioPacket::new(
            3,
            0,
            Duration::ZERO,
            Duration::from_millis(20),
            960,
            format,
            vec![0_u8; 2879],
        )
        .is_err());
    }

    #[test]
    fn pcm_packet_converts_to_mono_runtime_block_without_mutating_payload() {
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let payload = (0..2880)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        let packet = PcmAudioPacket::new(
            3,
            0,
            Duration::from_millis(40),
            Duration::from_millis(20),
            960,
            format,
            payload.clone(),
        )
        .unwrap();

        let block = PcmAudioBlock::from_mono_packet(packet, 48_000).unwrap();

        assert_eq!(block.start_time, Duration::from_millis(40));
        assert_eq!(block.duration, Duration::from_millis(20));
        assert_eq!(block.sample_rate, 48_000);
        assert_eq!(block.sample_count, 960);
        assert_eq!(
            block.layout,
            PcmAudioBlockLayout::MonoTrack {
                track_id: 3,
                channel_index: 0
            }
        );
        assert_eq!(block.payload, payload);
        assert_eq!(block.payload_bytes(), 2880);
        assert_eq!(block.end_time().unwrap(), Duration::from_millis(60));
    }

    #[test]
    fn pcm_block_rejects_invalid_payload_and_layout() {
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };

        assert!(PcmAudioBlock::new(
            Duration::ZERO,
            Duration::from_millis(20),
            48_000,
            960,
            format,
            PcmAudioBlockLayout::MonoTrack {
                track_id: 3,
                channel_index: 0
            },
            vec![0_u8; 2879],
        )
        .is_err());
        assert!(PcmAudioBlock::new(
            Duration::ZERO,
            Duration::from_millis(20),
            48_000,
            960,
            format,
            PcmAudioBlockLayout::InterleavedChannels { channel_count: 0 },
            vec![0_u8; 2880],
        )
        .is_err());
    }

    #[test]
    fn pcm_block_timing_detects_gap_and_overlap_cases() {
        fn classify_pair(first: &PcmAudioBlock, second: &PcmAudioBlock) -> (bool, bool) {
            let first_end = first.end_time().unwrap();
            (second.start_time > first_end, second.start_time < first_end)
        }

        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let make_block = |start_ms| {
            PcmAudioBlock::new(
                Duration::from_millis(start_ms),
                Duration::from_millis(20),
                48_000,
                960,
                format,
                PcmAudioBlockLayout::MonoTrack {
                    track_id: 3,
                    channel_index: 0,
                },
                vec![0_u8; 2880],
            )
            .unwrap()
        };

        assert_eq!(
            classify_pair(&make_block(0), &make_block(20)),
            (false, false)
        );
        assert_eq!(
            classify_pair(&make_block(0), &make_block(40)),
            (true, false)
        );
        assert_eq!(
            classify_pair(&make_block(0), &make_block(10)),
            (false, true)
        );
    }

    #[test]
    fn bounded_pcm_block_queue_reports_backpressure() {
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let block = |position: u64| {
            PcmAudioBlock::new(
                Duration::from_millis(position * 20),
                Duration::from_millis(20),
                48_000,
                960,
                format,
                PcmAudioBlockLayout::MonoTrack {
                    track_id: 3,
                    channel_index: 0,
                },
                vec![0_u8; 2880],
            )
            .unwrap()
        };
        let mut queue = BoundedQueue::new(2).unwrap();
        queue.try_push(block(0)).unwrap();
        queue.try_push(block(1)).unwrap();
        assert!(queue.try_push(block(2)).is_err());
        assert_eq!(queue.stats().peak_depth, 2);
        assert_eq!(queue.stats().backpressure_events, 1);
    }

    #[test]
    fn frame_to_sample_mapping_uses_rational_audio_time() {
        let start = Duration::from_millis(40);
        let duration = Duration::from_millis(40);
        let range = av_frame_audio_range(1, start, duration, 48_000, &clock_ready_blocks(), 2)
            .expect("range");

        assert_eq!(range.audio_start_sample, 1920);
        assert_eq!(range.audio_sample_count, 1920);
        assert_eq!(range.audio_start_time, start);
        assert_eq!(range.audio_duration, duration);
        assert!(range.complete);
        assert_eq!(range.covered_tracks.len(), 2);
    }

    #[test]
    fn audio_range_coverage_detects_gaps_and_overlaps() {
        let gap_blocks = vec![
            test_pcm_block(3, 0, 0, 960),
            test_pcm_block(3, 0, 1920, 960),
        ];
        let gap = audio_range_coverage(&gap_blocks, 0, 2880, 48_000).expect("coverage");
        assert_eq!(gap[0].gaps, 1);
        assert!(!gap[0].complete);

        let overlap_blocks = vec![test_pcm_block(3, 0, 0, 960), test_pcm_block(3, 0, 480, 960)];
        let overlap = audio_range_coverage(&overlap_blocks, 0, 1440, 48_000).expect("coverage");
        assert_eq!(overlap[0].overlaps, 1);
        assert!(overlap[0].complete);
    }

    #[test]
    fn av_sync_summary_rejects_outside_audio_range() {
        let inside = av_frame_audio_range(
            0,
            Duration::ZERO,
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let outside = av_frame_audio_range(
            1,
            Duration::from_millis(120),
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let summary =
            summarize_broadcast_runtime_contract(&[inside, outside], Duration::from_millis(120));

        assert_eq!(summary.frames_checked, 2);
        assert_eq!(summary.frames_outside_audio_range, 1);
        assert!(!summary.suitable_for_broadcast_runtime_contract);
    }

    #[test]
    fn av_sync_summary_accepts_complete_ranges() {
        let ranges = vec![
            av_frame_audio_range(
                0,
                Duration::ZERO,
                Duration::from_millis(40),
                48_000,
                &clock_ready_blocks(),
                2,
            )
            .unwrap(),
            av_frame_audio_range(
                1,
                Duration::from_millis(40),
                Duration::from_millis(40),
                48_000,
                &clock_ready_blocks(),
                2,
            )
            .unwrap(),
        ];
        let summary = summarize_broadcast_runtime_contract(&ranges, Duration::from_millis(120));

        assert_eq!(summary.complete_frames, 2);
        assert_eq!(summary.incomplete_frames, 0);
        assert_eq!(summary.frames_outside_audio_range, 0);
        assert!(summary.suitable_for_broadcast_runtime_contract);
    }

    #[test]
    fn broadcast_runtime_session_requires_authoritative_original_audio() {
        let session = test_broadcast_session();
        assert_eq!(session.validate().unwrap(), session);

        assert!(BroadcastRuntimeSessionDescription {
            audio_source: BroadcastMediaSourceRole::ProxyAudioDiagnosticOnly,
            ..session
        }
        .validate()
        .is_err());
        assert!(BroadcastRuntimeSessionDescription {
            capabilities: BroadcastRuntimeCapabilities {
                proxy_audio_primary: true,
                ..session.capabilities
            },
            ..session
        }
        .validate()
        .is_err());
    }

    #[test]
    fn broadcast_runtime_validates_proxy_preview_source_mode() {
        let session = test_broadcast_session();

        assert_eq!(
            session.video_source_mode,
            BroadcastVideoSourceMode::ProxyPreview
        );
        assert_eq!(
            session.video_source,
            BroadcastMediaSourceRole::ProxyPreviewVideo
        );
        assert!(session.validate().is_ok());
    }

    #[test]
    fn broadcast_runtime_validates_original_media_source_mode_without_realtime_claim() {
        let session = BroadcastRuntimeSessionDescription {
            video_source: BroadcastMediaSourceRole::OriginalFinishingMedia,
            video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
            capabilities: BroadcastRuntimeCapabilities {
                original_media_video_source: true,
                original_media_realtime_supported: false,
                ..test_broadcast_session().capabilities
            },
            ..test_broadcast_session()
        };

        assert!(session.validate().is_ok());
    }

    #[test]
    fn broadcast_runtime_rejects_mismatched_source_mode() {
        assert!(BroadcastRuntimeSessionDescription {
            video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
            ..test_broadcast_session()
        }
        .validate()
        .is_err());
        assert!(BroadcastRuntimeSessionDescription {
            video_source: BroadcastMediaSourceRole::OriginalFinishingMedia,
            ..test_broadcast_session()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn broadcast_runtime_rejects_play_without_prepare() {
        let mut machine = BroadcastRuntimeStateMachine::create(test_broadcast_session()).unwrap();

        assert_eq!(machine.state(), BroadcastRuntimeState::Idle);
        assert!(matches!(
            machine.play(),
            Err(PlaybackError::InvalidRuntimeTransition)
        ));
        assert!(matches!(
            machine.pause(),
            Err(PlaybackError::InvalidRuntimeTransition)
        ));
    }

    #[test]
    fn broadcast_runtime_happy_path_accounts_frames_and_completes() {
        let mut machine = BroadcastRuntimeStateMachine::create(test_broadcast_session()).unwrap();

        machine.prepare(test_prepare_facts()).unwrap();
        assert_eq!(machine.state(), BroadcastRuntimeState::Ready);
        machine.play().unwrap();
        machine.pause().unwrap();
        assert_eq!(machine.state(), BroadcastRuntimeState::Paused);
        machine.play().unwrap();
        machine.seek(Duration::from_millis(400)).unwrap();
        machine.account_happy_path().unwrap();
        machine.drain().unwrap();
        machine.complete().unwrap();

        assert_eq!(machine.state(), BroadcastRuntimeState::Completed);
        assert_eq!(
            machine.accounting(),
            BroadcastRuntimeAccounting {
                selected_frames_accounted: 53,
                audio_ranges_accounted: 53,
                intentional_profile_skips: 53,
                lateness_drops: 0,
            }
        );
        assert!(machine
            .events()
            .contains(&BroadcastRuntimeEvent::SessionCreated));
        assert!(machine
            .events()
            .contains(&BroadcastRuntimeEvent::PreparingStarted));
        assert!(machine.events().contains(&BroadcastRuntimeEvent::Prepared));
        assert!(machine.events().contains(&BroadcastRuntimeEvent::Completed));
        assert!(!machine
            .events()
            .iter()
            .any(|event| matches!(event, BroadcastRuntimeEvent::LatenessDrop { .. })));
    }

    #[test]
    fn broadcast_runtime_invalid_after_completed_and_failed_stays_terminal() {
        let mut completed = BroadcastRuntimeStateMachine::create(test_broadcast_session()).unwrap();
        completed.prepare(test_prepare_facts()).unwrap();
        completed.play().unwrap();
        completed.account_happy_path().unwrap();
        completed.drain().unwrap();
        completed.complete().unwrap();
        assert!(matches!(
            completed.seek(Duration::ZERO),
            Err(PlaybackError::InvalidRuntimeTransition)
        ));

        let mut failed = BroadcastRuntimeStateMachine::create(test_broadcast_session()).unwrap();
        assert!(failed
            .prepare(BroadcastRuntimePrepareFacts {
                audio_ranges_complete: false,
                ..test_prepare_facts()
            })
            .is_err());
        assert_eq!(failed.state(), BroadcastRuntimeState::Failed);
        assert!(matches!(
            failed.prepare(test_prepare_facts()),
            Err(PlaybackError::InvalidRuntimeTransition)
        ));
    }

    #[test]
    fn broadcast_preroll_reports_ready_with_enough_audio_and_video() {
        let status = evaluate_broadcast_preroll(test_preroll_config(), test_preroll_plan(), 3, 3);

        assert!(status.ready);
        assert_eq!(status.missing_video_frames, 0);
        assert_eq!(status.missing_audio_ranges, 0);
        assert!(status.queue_limits_ok);
        assert_eq!(status.reason, None);
    }

    #[test]
    fn broadcast_preroll_reports_original_media_capability_missing() {
        let status = evaluate_broadcast_preroll(
            BroadcastPrerollConfig {
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                ..test_preroll_config()
            },
            BroadcastPrerollPlan {
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                video_source_available: true,
                video_runtime_supported: false,
                selected_video_frames_planned: 106,
                intentional_skips_planned: 0,
                ..test_preroll_plan()
            },
            3,
            3,
        );

        assert!(!status.ready);
        assert_eq!(
            status.reason,
            Some(BroadcastPrerollNotReadyReason::CapabilityMissing)
        );
    }

    #[test]
    fn broadcast_preroll_does_not_count_intentional_skips_as_missing_frames() {
        let status = evaluate_broadcast_preroll(
            test_preroll_config(),
            BroadcastPrerollPlan {
                selected_video_frames_planned: 53,
                intentional_skips_planned: 53,
                ..test_preroll_plan()
            },
            3,
            3,
        );

        assert!(status.ready);
        assert_eq!(status.missing_video_frames, 0);
    }

    #[test]
    fn broadcast_preroll_reports_missing_video_or_audio() {
        let missing_video =
            evaluate_broadcast_preroll(test_preroll_config(), test_preroll_plan(), 2, 3);
        assert!(!missing_video.ready);
        assert_eq!(
            missing_video.reason,
            Some(BroadcastPrerollNotReadyReason::MissingVideoFrames)
        );
        assert_eq!(missing_video.missing_video_frames, 1);

        let missing_audio =
            evaluate_broadcast_preroll(test_preroll_config(), test_preroll_plan(), 3, 2);
        assert!(!missing_audio.ready);
        assert_eq!(
            missing_audio.reason,
            Some(BroadcastPrerollNotReadyReason::MissingAudioRanges)
        );
        assert_eq!(missing_audio.missing_audio_ranges, 1);
    }

    #[test]
    fn broadcast_preroll_rejects_invalid_queue_capacity() {
        assert!(BroadcastPrerollConfig {
            max_video_queue: 0,
            ..test_preroll_config()
        }
        .validate()
        .is_err());

        let status = evaluate_broadcast_preroll(
            BroadcastPrerollConfig {
                max_video_queue: 12,
                ..test_preroll_config()
            },
            test_preroll_plan(),
            3,
            3,
        );
        assert!(!status.ready);
        assert_eq!(
            status.reason,
            Some(BroadcastPrerollNotReadyReason::InvalidQueueLimits)
        );
    }

    #[test]
    fn broadcast_runtime_remains_preparing_when_preroll_not_ready() {
        let mut machine = BroadcastRuntimeStateMachine::create(test_broadcast_session()).unwrap();
        let not_ready =
            evaluate_broadcast_preroll(test_preroll_config(), test_preroll_plan(), 2, 3);

        machine
            .prepare_with_preroll(test_prepare_facts(), not_ready)
            .unwrap();
        assert_eq!(machine.state(), BroadcastRuntimeState::Preparing);
        assert!(matches!(
            machine.play(),
            Err(PlaybackError::InvalidRuntimeTransition)
        ));
    }

    #[test]
    fn broadcast_runtime_enters_ready_when_preroll_ready() {
        let mut machine = BroadcastRuntimeStateMachine::create(test_broadcast_session()).unwrap();
        let ready = evaluate_broadcast_preroll(test_preroll_config(), test_preroll_plan(), 3, 3);

        machine
            .prepare_with_preroll(test_prepare_facts(), ready)
            .unwrap();
        assert_eq!(machine.state(), BroadcastRuntimeState::Ready);
        assert!(machine.play().is_ok());
    }

    #[test]
    fn prepared_audio_slot_preserves_original_audio_coverage() {
        let range = av_frame_audio_range(
            0,
            Duration::ZERO,
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let slot = BroadcastPreparedAudioSlot::from_audio_range(
            0,
            BroadcastVideoSourceMode::ProxyPreview,
            &range,
        );

        assert_eq!(
            slot.audio_source_role,
            BroadcastMediaSourceRole::OriginalAuthoritativeAudio
        );
        assert_eq!(slot.start_sample, 0);
        assert_eq!(slot.sample_count, 1920);
        assert_eq!(slot.track_coverage.len(), 2);
        assert!(slot.complete);
    }

    #[test]
    fn prepared_proxy_preview_slots_satisfy_preroll() {
        let video_slots = test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview);
        let audio_slots = test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview);
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true);
        let summary = summarize_broadcast_prepared_slots(
            test_preroll_config(),
            test_preroll_plan(),
            &video_slots,
            &audio_slots,
            &presentation_slots,
        );

        assert!(summary.preroll_status.ready);
        assert_eq!(summary.video_slot_count, 3);
        assert_eq!(summary.video_slots_prepared, 3);
        assert_eq!(summary.audio_slot_count, 3);
        assert_eq!(summary.audio_slots_complete, 3);
        assert_eq!(summary.presentation_slots_ready, 3);
        assert_eq!(summary.tracks_covered, 2);
    }

    #[test]
    fn prepared_original_media_slots_report_capability_missing() {
        let video_slots = (0..3)
            .map(|slot_index| BroadcastPreparedVideoSlot {
                slot_index,
                source_mode: BroadcastVideoSourceMode::OriginalMedia,
                video_source_role: BroadcastMediaSourceRole::OriginalFinishingMedia,
                source_frame_index: Some(u64::try_from(slot_index).unwrap()),
                selected_preview_frame_index: None,
                presentation_time: Duration::from_millis(u64::try_from(slot_index * 40).unwrap()),
                duration: Duration::from_millis(40),
                status: BroadcastPreparedVideoSlotStatus::CapabilityMissing,
            })
            .collect::<Vec<_>>();
        let audio_slots = test_prepared_audio_slots(BroadcastVideoSourceMode::OriginalMedia);
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::OriginalMedia, false);
        let summary = summarize_broadcast_prepared_slots(
            BroadcastPrerollConfig {
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                ..test_preroll_config()
            },
            BroadcastPrerollPlan {
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                video_source_available: true,
                video_runtime_supported: false,
                selected_video_frames_planned: 106,
                intentional_skips_planned: 0,
                ..test_preroll_plan()
            },
            &video_slots,
            &audio_slots,
            &presentation_slots,
        );

        assert!(!summary.preroll_status.ready);
        assert_eq!(
            summary.preroll_status.reason,
            Some(BroadcastPrerollNotReadyReason::CapabilityMissing)
        );
        assert_eq!(summary.video_slots_prepared, 0);
        assert_eq!(summary.audio_slots_complete, 3);
        assert_eq!(summary.presentation_slots_ready, 0);
    }

    #[test]
    fn prepared_presentation_slots_link_selected_frames_not_intentional_skips() {
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true);

        assert_eq!(presentation_slots.len(), 3);
        assert_eq!(presentation_slots[0].selected_source_frame, Some(0));
        assert_eq!(presentation_slots[1].selected_source_frame, Some(2));
        assert_eq!(presentation_slots[2].selected_source_frame, Some(4));
        assert!(presentation_slots
            .iter()
            .all(|slot| slot.selected_source_frame.unwrap().is_multiple_of(2)));
    }

    #[test]
    fn audio_payload_binding_references_exact_blocks_without_copying_payload() {
        let range = av_frame_audio_range(
            0,
            Duration::ZERO,
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let slot = BroadcastPreparedAudioSlot::from_audio_range(
            0,
            BroadcastVideoSourceMode::ProxyPreview,
            &range,
        );
        let binding = bind_broadcast_audio_payload(&slot, &clock_ready_blocks()).unwrap();

        assert!(binding.complete);
        assert_eq!(binding.track_count, 2);
        assert_eq!(binding.block_coverage.len(), 4);
        assert!(binding
            .block_coverage
            .iter()
            .all(|coverage| coverage.full_block));
        assert_eq!(binding.total_referenced_payload_bytes, 11_520);
    }

    #[test]
    fn audio_payload_binding_handles_partial_leading_and_trailing_blocks() {
        let range = av_frame_audio_range(
            0,
            Duration::from_millis(10),
            Duration::from_millis(20),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let slot = BroadcastPreparedAudioSlot::from_audio_range(
            0,
            BroadcastVideoSourceMode::ProxyPreview,
            &range,
        );
        let binding = bind_broadcast_audio_payload(&slot, &clock_ready_blocks()).unwrap();

        assert!(binding.complete);
        assert_eq!(binding.sample_count, 960);
        assert_eq!(binding.block_coverage.len(), 4);
        assert!(binding
            .block_coverage
            .iter()
            .any(|coverage| coverage.byte_offset_within_block == 1_440));
        assert!(binding
            .block_coverage
            .iter()
            .all(|coverage| !coverage.full_block));
        assert_eq!(binding.total_referenced_payload_bytes, 5_760);
    }

    #[test]
    fn audio_payload_binding_detects_missing_track_payload() {
        let range = av_frame_audio_range(
            0,
            Duration::ZERO,
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let slot = BroadcastPreparedAudioSlot::from_audio_range(
            0,
            BroadcastVideoSourceMode::ProxyPreview,
            &range,
        );
        let missing_track_blocks = clock_ready_blocks()
            .into_iter()
            .filter(|block| block.track_id() == Some(3))
            .collect::<Vec<_>>();
        let binding = bind_broadcast_audio_payload(&slot, &missing_track_blocks).unwrap();

        assert!(!binding.complete);
        assert_eq!(binding.block_coverage.len(), 2);
    }

    #[test]
    fn video_payload_binding_reports_accounted_only_and_capability_missing() {
        let proxy_video = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let proxy_binding = bind_broadcast_video_payload_accounting(proxy_video);
        assert_eq!(
            proxy_binding.status,
            BroadcastVideoPayloadBindingStatus::AccountedOnly
        );
        assert_eq!(proxy_binding.payload_id, None);
        assert_eq!(proxy_binding.payload, None);

        let original_video = BroadcastPreparedVideoSlot {
            status: BroadcastPreparedVideoSlotStatus::CapabilityMissing,
            ..test_prepared_video_slots(BroadcastVideoSourceMode::OriginalMedia)[0].clone()
        };
        let original_binding = bind_broadcast_video_payload_accounting(&original_video);
        assert_eq!(
            original_binding.status,
            BroadcastVideoPayloadBindingStatus::CapabilityMissing
        );
    }

    #[test]
    fn video_payload_binding_can_become_payload_ready_with_real_reference() {
        let proxy_video = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let payload = BroadcastVideoPayloadReference {
            payload_id: 42,
            kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            format: BroadcastVideoPayloadFormat::RgbaU16,
            backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
            source_frame_index: proxy_video.source_frame_index.unwrap(),
            selected_preview_frame_index: proxy_video.selected_preview_frame_index,
            presentation_time: proxy_video.presentation_time,
            duration: proxy_video.duration,
            coded_width: 1920,
            coded_height: 1088,
            visible_width: 1920,
            visible_height: 1080,
            bounded_slot_index: proxy_video.slot_index,
            session_index: 0,
        };
        let binding = bind_broadcast_video_payload_ready(proxy_video, payload).unwrap();

        assert_eq!(
            binding.status,
            BroadcastVideoPayloadBindingStatus::PayloadReady
        );
        assert_eq!(binding.payload_id, Some(42));
        assert_eq!(
            binding.payload.as_ref().map(|payload| payload.kind),
            Some(BroadcastVideoPayloadKind::ProcessedGpuFrame)
        );
    }

    #[test]
    fn original_media_video_payload_can_become_payload_ready_with_original_backend() {
        let original_video = &test_prepared_video_slots(BroadcastVideoSourceMode::OriginalMedia)[0];
        let payload = BroadcastVideoPayloadReference {
            payload_id: 142,
            kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            format: BroadcastVideoPayloadFormat::RgbaU16,
            backend_path: BroadcastVideoPayloadBackendPath::SoftwareH264Yuv422P10Vulkan,
            source_frame_index: original_video.source_frame_index.unwrap(),
            selected_preview_frame_index: original_video.selected_preview_frame_index,
            presentation_time: original_video.presentation_time,
            duration: original_video.duration,
            coded_width: 1920,
            coded_height: 1088,
            visible_width: 1920,
            visible_height: 1080,
            bounded_slot_index: original_video.slot_index,
            session_index: 1,
        };
        let binding = bind_broadcast_video_payload_ready(original_video, payload).unwrap();
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::OriginalMedia)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::OriginalMedia, true)[0];
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &binding, &audio_binding);

        assert_eq!(
            binding.status,
            BroadcastVideoPayloadBindingStatus::PayloadReady
        );
        assert_eq!(
            presentation_binding.readiness,
            BroadcastPresentationPayloadReadiness::PayloadReady
        );
    }

    #[test]
    fn original_media_video_payload_rejects_proxy_backend() {
        let original_video = &test_prepared_video_slots(BroadcastVideoSourceMode::OriginalMedia)[0];
        let payload = BroadcastVideoPayloadReference {
            payload_id: 143,
            kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            format: BroadcastVideoPayloadFormat::RgbaU16,
            backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
            source_frame_index: original_video.source_frame_index.unwrap(),
            selected_preview_frame_index: original_video.selected_preview_frame_index,
            presentation_time: original_video.presentation_time,
            duration: original_video.duration,
            coded_width: 1920,
            coded_height: 1088,
            visible_width: 1920,
            visible_height: 1080,
            bounded_slot_index: original_video.slot_index,
            session_index: 1,
        };

        assert_eq!(
            bind_broadcast_video_payload_ready(original_video, payload),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    #[test]
    fn presenter_payload_descriptor_uses_public_identity_and_payload_shape() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 501,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let descriptor = QgsPresenterPayloadDescriptor::from_video_binding(
            "qnc://local/media/proxy/test",
            &binding,
        )
        .unwrap();

        assert_eq!(descriptor.public_source_uri, "qnc://local/media/proxy/test");
        assert_eq!(descriptor.source_frame, 0);
        assert_eq!(
            descriptor.payload_kind,
            BroadcastVideoPayloadKind::ProcessedGpuFrame
        );
        assert_eq!(
            descriptor.payload_format,
            BroadcastVideoPayloadFormat::RgbaU16
        );
        assert!(!descriptor.private_path_exposed);
    }

    #[test]
    fn presenter_payload_descriptor_rejects_accounting_only_payload() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let binding = bind_broadcast_video_payload_accounting(video_slot);

        assert_eq!(
            QgsPresenterPayloadDescriptor::from_video_binding(
                "qnc://local/media/proxy/test",
                &binding
            ),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    #[test]
    fn test_presenter_boundary_updates_monitor_without_real_display_claim() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 502,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let mut presenter = BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
            accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
            visible_width: 1920,
            visible_height: 1080,
            coded_width: 1920,
            coded_height: 1088,
        })
        .unwrap();

        let result = submit_qgs_presenter_payload_to_test_boundary(
            "qnc://local/media/proxy/test",
            &presentation_binding,
            &video_binding,
            &mut presenter,
        )
        .unwrap();
        let update = project_qgs_monitor_update_from_presenter_result(&result);
        let projection = QgsQncMonitorProjection::from_presenter_update(&update);

        assert_eq!(
            result.evidence_level,
            QgsVideoPresentationEvidenceLevel::FramePresentedTestBoundary
        );
        assert_eq!(
            update.presenter_evidence_kind,
            Some(QgsPresenterEvidenceKind::TestPresenterAccepted)
        );
        assert!(projection.prepared_descriptor_present);
        assert!(projection.submitted_to_presenter);
        assert!(projection.frame_presented_test_boundary);
        assert!(!projection.frame_presented_real_backend);
        assert!(!projection.real_display_evidence_present);
        assert!(!projection.visual_verified);
        assert!(!projection.private_path_exposed);
    }

    #[test]
    fn runtime_surface_e2e_acceptance_keeps_monitor_evidence_scoped() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 504,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let presentation_binding = BroadcastPresentationPayloadBinding {
            presentation_slot_index: 0,
            video_binding_index: video_binding.video_slot_index,
            audio_binding_index: 0,
            presentation_time: video_slot.presentation_time,
            duration: video_slot.duration,
            ready: true,
            readiness: BroadcastPresentationPayloadReadiness::PayloadReady,
        };
        let mut presenter = BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
            accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
            visible_width: 1920,
            visible_height: 1080,
            coded_width: 1920,
            coded_height: 1088,
        })
        .unwrap();

        let result = submit_qgs_presenter_payload_to_test_boundary(
            "qnc://local/media/proxy/e2e",
            &presentation_binding,
            &video_binding,
            &mut presenter,
        )
        .unwrap();
        let update = project_qgs_monitor_update_from_presenter_result(&result);

        assert!(update.frame.prepared_payload_present);
        assert!(update.submitted_to_presenter);
        assert_eq!(
            update.presenter_evidence_kind,
            Some(QgsPresenterEvidenceKind::TestPresenterAccepted)
        );
        assert!(update.frame_presented_test_boundary);
        assert!(!update.frame_presented_real_backend);
        assert!(!update.real_display_evidence_present);
        assert!(!update.visual_verified);
        assert!(!update.private_path_exposed);
        assert_eq!(presenter.accepted_count(), 1);
        assert_eq!(presenter.rejected_count(), 0);
    }

    #[test]
    fn file_presenter_manifest_uses_public_identity_without_real_display_claim() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 505,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();

        let result = submit_qgs_file_presenter_manifest(
            "qnc://local/media/proxy/test",
            &video_binding,
            "target/qgs-visual-diagnostics/test-frame000000-manifest.json",
        )
        .unwrap();
        let update = project_qgs_monitor_update_from_file_presenter_result(&result);
        let projection = QgsQncMonitorProjection::from_presenter_update(&update);

        assert_eq!(
            result.submission.presenter_kind,
            QgsPresenterDeviceKind::FilePresenter
        );
        assert_eq!(
            result.artifact.kind,
            QgsFilePresenterArtifactKind::DescriptorManifest
        );
        assert!(result.artifact.manifest_written);
        assert!(!result.artifact.image_written);
        assert_eq!(
            result.evidence.evidence_kind,
            QgsPresenterEvidenceKind::FilePresenterManifestWritten
        );
        assert_eq!(
            result.evidence.evidence_level,
            QgsVideoPresentationEvidenceLevel::FilePresenterManifestWritten
        );
        assert_eq!(
            result.visual_status,
            QgsVisualDiagnosticStatus::ManifestWritten
        );
        assert!(!result.real_display_evidence_present);
        assert!(!result.visual_verified);
        assert!(projection.prepared_descriptor_present);
        assert!(projection.submitted_to_presenter);
        assert!(!projection.frame_presented_test_boundary);
        assert!(!projection.frame_presented_real_backend);
        assert!(!projection.real_display_evidence_present);
        assert!(!projection.visual_verified);
        assert!(!projection.private_path_exposed);
    }

    #[test]
    fn file_presenter_manifest_rejects_private_artifact_paths() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 506,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();

        assert_eq!(
            submit_qgs_file_presenter_manifest(
                "qnc://local/media/proxy/test",
                &video_binding,
                "/tmp/private-frame-manifest.json",
            ),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
    }

    #[test]
    fn file_presenter_image_evidence_stays_below_real_display_and_visual_verified() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 507,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();

        let result = submit_qgs_file_presenter_image(
            "qnc://local/media/proxy/test",
            &video_binding,
            "qgs-visual-diagnostics/test-frame000000-readback.ppm",
        )
        .unwrap();
        let update = project_qgs_monitor_update_from_file_presenter_result(&result);
        let projection = QgsQncMonitorProjection::from_presenter_update(&update);

        assert_eq!(
            result.artifact.kind,
            QgsFilePresenterArtifactKind::ImageFile
        );
        assert!(result.artifact.image_written);
        assert_eq!(
            result.evidence.evidence_kind,
            QgsPresenterEvidenceKind::FilePresenterImageWritten
        );
        assert_eq!(
            result.evidence.evidence_level,
            QgsVideoPresentationEvidenceLevel::FilePresenterImageWritten
        );
        assert_eq!(
            result.visual_status,
            QgsVisualDiagnosticStatus::ImageFileWritten
        );
        assert!(projection.submitted_to_presenter);
        assert!(!projection.frame_presented_real_backend);
        assert!(!projection.real_display_evidence_present);
        assert!(!projection.visual_verified);
        assert!(!projection.private_path_exposed);
    }

    #[test]
    fn monitor_projection_flags_private_public_identity() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 503,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let descriptor =
            QgsPresenterPayloadDescriptor::from_video_binding("/private/media/file.mp4", &binding)
                .unwrap();

        assert!(descriptor.private_path_exposed);
    }

    #[test]
    fn verification_matrix_orders_evidence_levels() {
        assert!(
            BroadcastRuntimeVerificationLevel::PayloadBound
                > BroadcastRuntimeVerificationLevel::PayloadExtracted
        );
        assert!(
            BroadcastRuntimeVerificationLevel::TestBoundaryEvidence
                < BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::SelectionPolicyEvidence
                < BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::SelectionPolicyEvidence
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::SelectionPolicyEvidence
                < BroadcastRuntimeVerificationLevel::RealtimeVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::OperationalStateEvidence
                < BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::OperationalStateEvidence
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::OperationalStateEvidence
                < BroadcastRuntimeVerificationLevel::RealtimeVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::FaultRecoveryPolicyEvidence
                < BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::FaultRecoveryPolicyEvidence
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::FaultRecoveryPolicyEvidence
                < BroadcastRuntimeVerificationLevel::RealtimeVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::RuntimeBehaviorEvidence
                < BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::RuntimeBehaviorEvidence
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::RuntimeBehaviorEvidence
                < BroadcastRuntimeVerificationLevel::RealtimeVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::FilePresenterManifestWritten
                < BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::FilePresenterImageWritten
                < BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::NativeBufferSubmissionVerified
                < BroadcastRuntimeVerificationLevel::NativePostSubmitEvidence
        );
        assert!(
            BroadcastRuntimeVerificationLevel::NativePostSubmitEvidence
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::RuntimeAudioPayloadDrainCompleted
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::RuntimeAudioPayloadAudibleConfirmed
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::ManualAudibleSignalDetectedContentUnverified
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::ManualContentAudibilityPartiallyObserved
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::ManualMonitorPairPreferenceObserved
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::DesktopMonoListeningHelperDrainCompleted
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::ManualDesktopMonoListeningHelperHeard
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert!(
            BroadcastRuntimeVerificationLevel::Discrete4MonoOutputDrainCompleted
                < BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
    }

    #[test]
    fn verification_matrix_keeps_real_outputs_not_implemented() {
        let matrix = BroadcastRuntimeVerificationMatrix::sony_fx6_sample_002_current();
        matrix.validate_truth_rules().unwrap();

        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::RealDisplayOutput)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::NotImplemented
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::RealSpeakerOutput)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::NotImplemented
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::RealtimePlayback)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::NotImplemented
        );
    }

    #[test]
    fn verification_matrix_does_not_treat_test_evidence_as_real_output() {
        let matrix = BroadcastRuntimeVerificationMatrix::sony_fx6_sample_002_current();

        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::TestVideoPresenterEvidence)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::TestBoundaryEvidence
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::PresenterMonitorBoundary)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::TestBoundaryEvidence
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::RuntimeSurfaceE2eAcceptance)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::TestBoundaryEvidence
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::TestAudioSinkEvidence)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::TestBoundaryEvidence
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::TestVideoPresenterEvidence)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::PresenterMonitorBoundary)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::RuntimeSurfaceE2eAcceptance)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::TestAudioSinkEvidence)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::NativePipeWireBufferSubmission)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::NativePostSubmitEvidence
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::NativePipeWireBufferSubmission)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::NativePipeWireAudibleSmokeTest)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::NativePipeWireAudibleSmokeTest)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::ManualContentAudibilityPartiallyObserved
        );
        assert_ne!(
            matrix
                .entry(
                    BroadcastRuntimeVerifiedSubsystem::NativePipeWireOriginalAudioSegmentPlayback
                )
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert_eq!(
            matrix
                .entry(
                    BroadcastRuntimeVerifiedSubsystem::NativePipeWireOriginalAudioSegmentPlayback
                )
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::ManualContentAudibilityPartiallyObserved
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::PipeWireAudioContentSanityAudit)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::MediaInspected
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::Mironik2002MonitorDiagnostic)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::ManualMonitorPairPreferenceObserved
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::NativePipeWireDesktopMonoListeningHelper)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::DesktopMonoListeningHelperDrainCompleted
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::NativePipeWireDiscrete4MonoOutputBoundary)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::Discrete4MonoOutputDrainCompleted
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::BroadcastRuntimeAudioPayloadPipeWire)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
        assert_eq!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::BroadcastPlayerRunningRuntimeDemo)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::RuntimeBehaviorEvidence
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::BroadcastPlayerRunningRuntimeDemo)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::VisualVerified
        );
        assert_ne!(
            matrix
                .entry(BroadcastRuntimeVerifiedSubsystem::BroadcastPlayerRunningRuntimeDemo)
                .unwrap()
                .level,
            BroadcastRuntimeVerificationLevel::AudioDeviceVerified
        );
    }

    #[test]
    fn presentation_payload_binding_classifies_readiness() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_accounting(video_slot);
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);

        assert!(presentation_binding.ready);
        assert_eq!(
            presentation_binding.readiness,
            BroadcastPresentationPayloadReadiness::RuntimeAccountingReady
        );

        let payload_ready_video = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 7,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let payload_ready_binding = bind_broadcast_presentation_payload(
            presentation_slot,
            &payload_ready_video,
            &audio_binding,
        );
        assert!(payload_ready_binding.ready);
        assert_eq!(
            payload_ready_binding.readiness,
            BroadcastPresentationPayloadReadiness::PayloadReady
        );

        let original_video = BroadcastPreparedVideoSlot {
            status: BroadcastPreparedVideoSlotStatus::CapabilityMissing,
            ..test_prepared_video_slots(BroadcastVideoSourceMode::OriginalMedia)[0].clone()
        };
        let original_video_binding = bind_broadcast_video_payload_accounting(&original_video);
        let original_presentation =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::OriginalMedia, false)[0];
        let original_binding = bind_broadcast_presentation_payload(
            original_presentation,
            &original_video_binding,
            &audio_binding,
        );
        assert!(!original_binding.ready);
        assert_eq!(
            original_binding.readiness,
            BroadcastPresentationPayloadReadiness::CapabilityMissing
        );
    }

    #[test]
    fn device_boundary_keeps_payload_ready_distinct_from_device_ready() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 11,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let audio_submission = build_broadcast_audio_device_submission(
            0,
            &audio_binding,
            format,
            BroadcastDeviceStatus::NotConfigured,
            &[],
        );
        let video_submission = build_broadcast_video_presenter_submission(
            0,
            &video_binding,
            BroadcastDeviceStatus::NotConfigured,
            &[],
        );
        let summary = summarize_broadcast_device_boundary(
            &audio_submission,
            &video_submission,
            &[],
            BroadcastDeviceStatus::NotConfigured,
            BroadcastDeviceStatus::NotConfigured,
        );

        assert!(summary.audio_payload_ready);
        assert!(summary.video_payload_ready);
        assert!(!summary.device_payload_ready);
        assert_eq!(summary.frame_presented_count, 0);
        assert_eq!(
            summary.audio_submission_status,
            BroadcastDevicePayloadStatus::DeviceNotConfigured
        );
        assert_eq!(
            summary.video_submission_status,
            BroadcastDevicePayloadStatus::DeviceNotConfigured
        );
        assert_eq!(summary.reason, Some("device boundary not configured"));
    }

    #[test]
    fn device_boundary_requires_matching_capabilities() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 12,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let video_submission = build_broadcast_video_presenter_submission(
            0,
            &video_binding,
            BroadcastDeviceStatus::Ready,
            &[BroadcastDeviceCapability::AcceptsCpuImage],
        );

        assert_eq!(
            video_submission.status,
            BroadcastDevicePayloadStatus::DeviceCapabilityMissing
        );

        let ready_submission = build_broadcast_video_presenter_submission(
            0,
            &video_binding,
            BroadcastDeviceStatus::Ready,
            &[BroadcastDeviceCapability::AcceptsProcessedGpuFrame],
        );
        assert_eq!(
            ready_submission.status,
            BroadcastDevicePayloadStatus::DevicePayloadReady
        );
    }

    #[test]
    fn frame_presented_requires_presentation_evidence() {
        let audio_submission = BroadcastAudioDeviceSubmission {
            audio_binding_index: 0,
            start_sample: 0,
            sample_count: 1920,
            sample_rate: 48_000,
            track_count: 4,
            format: PcmSampleFormat::SignedInteger {
                bits_per_sample: 24,
                endian: PcmEndian::Little,
            },
            status: BroadcastDevicePayloadStatus::DevicePayloadReady,
        };
        let video_submission = BroadcastVideoPresenterSubmission {
            video_binding_index: 0,
            payload_kind: Some(BroadcastVideoPayloadKind::ProcessedGpuFrame),
            coded_width: Some(1920),
            coded_height: Some(1088),
            visible_width: Some(1920),
            visible_height: Some(1080),
            format: Some(BroadcastVideoPayloadFormat::RgbaU16),
            status: BroadcastDevicePayloadStatus::DevicePayloadReady,
        };

        let no_evidence = summarize_broadcast_device_boundary(
            &audio_submission,
            &video_submission,
            &[],
            BroadcastDeviceStatus::Ready,
            BroadcastDeviceStatus::Ready,
        );
        assert!(no_evidence.device_payload_ready);
        assert_eq!(no_evidence.frame_presented_count, 0);

        let with_evidence = summarize_broadcast_device_boundary(
            &audio_submission,
            &video_submission,
            &[BroadcastPresentationEvidence {
                presentation_slot_index: 0,
                audio_binding_index: None,
                video_binding_index: Some(0),
                media_time: Duration::ZERO,
                evidence_kind: BroadcastPresentationEvidenceKind::VideoFramePresented,
                source_device_kind: BroadcastDeviceKind::VideoPresenter,
                payload_id: Some(1),
            }],
            BroadcastDeviceStatus::Ready,
            BroadcastDeviceStatus::Ready,
        );
        assert_eq!(with_evidence.frame_presented_count, 1);
    }

    #[test]
    fn test_video_presenter_accepts_supported_processed_gpu_payload() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 21,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let mut presenter = BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
            accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
            visible_width: 1920,
            visible_height: 1080,
            coded_width: 1920,
            coded_height: 1088,
        })
        .unwrap();

        let evidence = presenter
            .submit(&presentation_binding, &video_binding)
            .unwrap();
        let events = broadcast_player_events_from_presentation_evidence(&[evidence.clone()]);
        let summary = summarize_broadcast_player_runtime_events(
            &events,
            BroadcastRuntimeAccounting {
                selected_frames_accounted: 0,
                audio_ranges_accounted: 0,
                intentional_profile_skips: 0,
                lateness_drops: 0,
            },
        );

        assert_eq!(presenter.accepted_count(), 1);
        assert_eq!(presenter.rejected_count(), 0);
        assert_eq!(
            evidence.evidence_kind,
            BroadcastPresentationEvidenceKind::TestPresenterAccepted
        );
        assert_eq!(evidence.payload_id, Some(21));
        assert_eq!(summary.frame_presented_events, 1);
        assert!(events.iter().any(|event| {
            matches!(
                event,
                BroadcastPlayerRuntimeEvent::PresentationEvidenceReceived {
                    evidence_kind: BroadcastPresentationEvidenceKind::TestPresenterAccepted,
                    ..
                }
            )
        }));
    }

    #[test]
    fn test_video_presenter_rejects_unsupported_payload_shape() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 22,
                kind: BroadcastVideoPayloadKind::CpuNv12Surface,
                format: BroadcastVideoPayloadFormat::Nv12,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let mut presenter = BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
            accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
            visible_width: 1920,
            visible_height: 1080,
            coded_width: 1920,
            coded_height: 1088,
        })
        .unwrap();

        assert_eq!(
            presenter.submit(&presentation_binding, &video_binding),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(presenter.accepted_count(), 0);
        assert_eq!(presenter.rejected_count(), 1);
    }

    #[test]
    fn original_media_capability_missing_cannot_be_test_presented() {
        let original_video = BroadcastPreparedVideoSlot {
            status: BroadcastPreparedVideoSlotStatus::CapabilityMissing,
            ..test_prepared_video_slots(BroadcastVideoSourceMode::OriginalMedia)[0].clone()
        };
        let video_binding = bind_broadcast_video_payload_accounting(&original_video);
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::OriginalMedia)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::OriginalMedia, false)[0];
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let mut presenter = BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
            accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
            visible_width: 1920,
            visible_height: 1080,
            coded_width: 1920,
            coded_height: 1088,
        })
        .unwrap();

        assert_eq!(
            presenter.submit(&presentation_binding, &video_binding),
            Err(PlaybackError::InvalidRuntimeTransition)
        );
        assert_eq!(presenter.accepted_count(), 0);
        assert_eq!(presenter.rejected_count(), 1);
    }

    #[test]
    fn test_audio_sink_accepts_supported_original_pcm_payload() {
        let range = av_frame_audio_range(
            0,
            Duration::ZERO,
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let audio_slot = BroadcastPreparedAudioSlot::from_audio_range(
            0,
            BroadcastVideoSourceMode::ProxyPreview,
            &range,
        );
        let audio_binding =
            bind_broadcast_audio_payload(&audio_slot, &clock_ready_blocks()).unwrap();
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_accounting(video_slot);
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();

        let evidence = sink
            .submit(&presentation_binding, &audio_binding, format)
            .unwrap();
        let events = broadcast_player_events_from_audio_sink_evidence(&[evidence.clone()]);
        let summary = summarize_broadcast_player_runtime_events(
            &events,
            BroadcastRuntimeAccounting {
                selected_frames_accounted: 0,
                audio_ranges_accounted: 0,
                intentional_profile_skips: 0,
                lateness_drops: 0,
            },
        );

        assert_eq!(sink.accepted_count(), 1);
        assert_eq!(sink.rejected_count(), 0);
        assert_eq!(sink.samples_accepted(), 1920);
        assert_eq!(sink.bytes_accepted(), 11_520);
        assert_eq!(
            evidence.evidence_kind,
            BroadcastPresentationEvidenceKind::TestAudioSinkAccepted
        );
        assert_eq!(evidence.payload_bytes_accepted, 11_520);
        assert_eq!(summary.frame_presented_events, 0);
    }

    #[test]
    fn test_audio_sink_rejects_wrong_sample_rate_or_bit_depth() {
        let range = av_frame_audio_range(
            0,
            Duration::ZERO,
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let audio_slot = BroadcastPreparedAudioSlot::from_audio_range(
            0,
            BroadcastVideoSourceMode::ProxyPreview,
            &range,
        );
        let audio_binding =
            bind_broadcast_audio_payload(&audio_slot, &clock_ready_blocks()).unwrap();
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_accounting(video_slot);
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };

        let mut wrong_rate = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 44_100,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();
        assert_eq!(
            wrong_rate.submit(&presentation_binding, &audio_binding, format),
            Err(PlaybackError::InvalidAudioFormat)
        );

        let mut wrong_depth = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 16,
            track_count: 2,
        })
        .unwrap();
        assert_eq!(
            wrong_depth.submit(&presentation_binding, &audio_binding, format),
            Err(PlaybackError::InvalidAudioFormat)
        );
    }

    #[test]
    fn test_audio_sink_rejects_incomplete_tracks_and_bad_byte_count() {
        let range = av_frame_audio_range(
            0,
            Duration::ZERO,
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let audio_slot = BroadcastPreparedAudioSlot::from_audio_range(
            0,
            BroadcastVideoSourceMode::ProxyPreview,
            &range,
        );
        let mut audio_binding =
            bind_broadcast_audio_payload(&audio_slot, &clock_ready_blocks()).unwrap();
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let video_binding = bind_broadcast_video_payload_accounting(video_slot);
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();

        audio_binding.complete = false;
        assert_eq!(
            sink.submit(&presentation_binding, &audio_binding, format),
            Err(PlaybackError::InvalidAudioFormat)
        );

        audio_binding.complete = true;
        audio_binding.total_referenced_payload_bytes = audio_binding
            .total_referenced_payload_bytes
            .saturating_sub(1);
        assert_eq!(
            sink.submit(&presentation_binding, &audio_binding, format),
            Err(PlaybackError::InvalidAudioFormat)
        );
    }

    #[test]
    fn original_media_audio_sink_evidence_does_not_imply_video_readiness() {
        let range = av_frame_audio_range(
            0,
            Duration::ZERO,
            Duration::from_millis(40),
            48_000,
            &clock_ready_blocks(),
            2,
        )
        .unwrap();
        let audio_slot = BroadcastPreparedAudioSlot::from_audio_range(
            0,
            BroadcastVideoSourceMode::OriginalMedia,
            &range,
        );
        let audio_binding =
            bind_broadcast_audio_payload(&audio_slot, &clock_ready_blocks()).unwrap();
        let video_slot = BroadcastPreparedVideoSlot {
            status: BroadcastPreparedVideoSlotStatus::CapabilityMissing,
            ..test_prepared_video_slots(BroadcastVideoSourceMode::OriginalMedia)[0].clone()
        };
        let video_binding = bind_broadcast_video_payload_accounting(&video_slot);
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::OriginalMedia, false)[0];
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();

        assert!(sink
            .submit(&presentation_binding, &audio_binding, format)
            .is_ok());
        assert_eq!(
            presentation_binding.readiness,
            BroadcastPresentationPayloadReadiness::CapabilityMissing
        );
    }

    #[test]
    fn simulated_loop_completes_over_payload_ready_slots() {
        let video_slots = test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview);
        let audio_slots = test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview);
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true);
        let audio_bindings = audio_slots
            .iter()
            .map(|slot| bind_broadcast_audio_payload(slot, &clock_ready_blocks_120ms()).unwrap())
            .collect::<Vec<_>>();
        let video_bindings = video_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                bind_broadcast_video_payload_ready(
                    slot,
                    BroadcastVideoPayloadReference {
                        payload_id: u64::try_from(index + 1).unwrap(),
                        kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                        format: BroadcastVideoPayloadFormat::RgbaU16,
                        backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                        source_frame_index: slot.source_frame_index.unwrap(),
                        selected_preview_frame_index: slot.selected_preview_frame_index,
                        presentation_time: slot.presentation_time,
                        duration: slot.duration,
                        coded_width: 1920,
                        coded_height: 1088,
                        visible_width: 1920,
                        visible_height: 1080,
                        bounded_slot_index: slot.slot_index,
                        session_index: 0,
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let presentation_bindings = presentation_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                bind_broadcast_presentation_payload(
                    slot,
                    &video_bindings[index],
                    &audio_bindings[index],
                )
            })
            .collect::<Vec<_>>();
        let audio_format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut audio_sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();
        let mut video_presenter =
            BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
                visible_width: 1920,
                visible_height: 1080,
                coded_width: 1920,
                coded_height: 1088,
            })
            .unwrap();

        let result = simulate_broadcast_player_runtime_loop(
            BroadcastVideoSourceMode::ProxyPreview,
            &presentation_bindings,
            &audio_bindings,
            &video_bindings,
            audio_format,
            &mut audio_sink,
            &mut video_presenter,
        );

        assert!(result.summary.completed);
        assert_eq!(result.summary.presentation_slots_attempted, 3);
        assert_eq!(result.summary.audio_submissions, 3);
        assert_eq!(result.summary.audio_accepted, 3);
        assert_eq!(result.summary.video_submissions, 3);
        assert_eq!(result.summary.video_accepted, 3);
        assert_eq!(result.summary.audio_evidence_count, 3);
        assert_eq!(result.summary.video_evidence_count, 3);
        assert_eq!(result.summary.frame_presented_count, 3);
        assert_eq!(result.summary.lateness_drops, 0);
        assert_eq!(result.summary.final_state, BroadcastRuntimeState::Completed);
        assert_eq!(result.failures, Vec::new());
        assert_eq!(audio_sink.accepted_count(), 3);
        assert_eq!(video_presenter.accepted_count(), 3);
    }

    #[test]
    fn simulated_loop_requires_audio_evidence_before_accounting() {
        let video_slots = test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview);
        let audio_slots = test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview);
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true);
        let audio_bindings = audio_slots
            .iter()
            .map(|slot| bind_broadcast_audio_payload(slot, &clock_ready_blocks_120ms()).unwrap())
            .collect::<Vec<_>>();
        let video_bindings = video_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                bind_broadcast_video_payload_ready(
                    slot,
                    BroadcastVideoPayloadReference {
                        payload_id: u64::try_from(index + 10).unwrap(),
                        kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                        format: BroadcastVideoPayloadFormat::RgbaU16,
                        backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                        source_frame_index: slot.source_frame_index.unwrap(),
                        selected_preview_frame_index: slot.selected_preview_frame_index,
                        presentation_time: slot.presentation_time,
                        duration: slot.duration,
                        coded_width: 1920,
                        coded_height: 1088,
                        visible_width: 1920,
                        visible_height: 1080,
                        bounded_slot_index: slot.slot_index,
                        session_index: 0,
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let presentation_bindings = presentation_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                bind_broadcast_presentation_payload(
                    slot,
                    &video_bindings[index],
                    &audio_bindings[index],
                )
            })
            .collect::<Vec<_>>();
        let audio_format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut audio_sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();
        let mut video_presenter =
            BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
                visible_width: 1920,
                visible_height: 1080,
                coded_width: 1920,
                coded_height: 1088,
            })
            .unwrap();

        let result = simulate_broadcast_player_runtime_loop(
            BroadcastVideoSourceMode::ProxyPreview,
            &presentation_bindings,
            &audio_bindings,
            &video_bindings,
            audio_format,
            &mut audio_sink,
            &mut video_presenter,
        );
        let first_audio_evidence = result
            .events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    BroadcastPlayerRuntimeEvent::PresentationEvidenceReceived {
                        source_device_kind: BroadcastDeviceKind::AudioSink,
                        ..
                    }
                )
            })
            .unwrap();
        let first_audio_accounted = result
            .events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    BroadcastPlayerRuntimeEvent::AudioRangeAccounted { .. }
                )
            })
            .unwrap();

        assert!(first_audio_evidence < first_audio_accounted);
    }

    #[test]
    fn simulated_loop_requires_video_evidence_before_frame_presented() {
        let video_slots = test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview);
        let audio_slots = test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview);
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true);
        let audio_bindings = audio_slots
            .iter()
            .map(|slot| bind_broadcast_audio_payload(slot, &clock_ready_blocks_120ms()).unwrap())
            .collect::<Vec<_>>();
        let video_bindings = video_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                bind_broadcast_video_payload_ready(
                    slot,
                    BroadcastVideoPayloadReference {
                        payload_id: u64::try_from(index + 20).unwrap(),
                        kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                        format: BroadcastVideoPayloadFormat::RgbaU16,
                        backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                        source_frame_index: slot.source_frame_index.unwrap(),
                        selected_preview_frame_index: slot.selected_preview_frame_index,
                        presentation_time: slot.presentation_time,
                        duration: slot.duration,
                        coded_width: 1920,
                        coded_height: 1088,
                        visible_width: 1920,
                        visible_height: 1080,
                        bounded_slot_index: slot.slot_index,
                        session_index: 0,
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let presentation_bindings = presentation_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                bind_broadcast_presentation_payload(
                    slot,
                    &video_bindings[index],
                    &audio_bindings[index],
                )
            })
            .collect::<Vec<_>>();
        let audio_format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut audio_sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();
        let mut video_presenter =
            BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
                visible_width: 1920,
                visible_height: 1080,
                coded_width: 1920,
                coded_height: 1088,
            })
            .unwrap();

        let result = simulate_broadcast_player_runtime_loop(
            BroadcastVideoSourceMode::ProxyPreview,
            &presentation_bindings,
            &audio_bindings,
            &video_bindings,
            audio_format,
            &mut audio_sink,
            &mut video_presenter,
        );
        let first_video_evidence = result
            .events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    BroadcastPlayerRuntimeEvent::PresentationEvidenceReceived {
                        source_device_kind: BroadcastDeviceKind::VideoPresenter,
                        ..
                    }
                )
            })
            .unwrap();
        let first_frame_presented = result
            .events
            .iter()
            .position(|event| matches!(event, BroadcastPlayerRuntimeEvent::FramePresented { .. }))
            .unwrap();

        assert!(first_video_evidence < first_frame_presented);
        assert!(!format!("{:?}", result.events).contains('/'));
    }

    #[test]
    fn simulated_loop_blocks_original_media_capability_missing() {
        let video_slots = test_prepared_video_slots(BroadcastVideoSourceMode::OriginalMedia)
            .into_iter()
            .map(|slot| BroadcastPreparedVideoSlot {
                status: BroadcastPreparedVideoSlotStatus::CapabilityMissing,
                ..slot
            })
            .collect::<Vec<_>>();
        let audio_slots = test_prepared_audio_slots(BroadcastVideoSourceMode::OriginalMedia);
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::OriginalMedia, false);
        let audio_bindings = audio_slots
            .iter()
            .map(|slot| bind_broadcast_audio_payload(slot, &clock_ready_blocks_120ms()).unwrap())
            .collect::<Vec<_>>();
        let video_bindings = video_slots
            .iter()
            .map(bind_broadcast_video_payload_accounting)
            .collect::<Vec<_>>();
        let presentation_bindings = presentation_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                bind_broadcast_presentation_payload(
                    slot,
                    &video_bindings[index],
                    &audio_bindings[index],
                )
            })
            .collect::<Vec<_>>();
        let audio_format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut audio_sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();
        let mut video_presenter =
            BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
                visible_width: 1920,
                visible_height: 1080,
                coded_width: 1920,
                coded_height: 1088,
            })
            .unwrap();

        let result = simulate_broadcast_player_runtime_loop(
            BroadcastVideoSourceMode::OriginalMedia,
            &presentation_bindings,
            &audio_bindings,
            &video_bindings,
            audio_format,
            &mut audio_sink,
            &mut video_presenter,
        );

        assert!(!result.summary.completed);
        assert_eq!(result.summary.final_state, BroadcastRuntimeState::Failed);
        assert_eq!(result.summary.frame_presented_count, 0);
        assert_eq!(result.summary.audio_submissions, 0);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(
            result.failures[0].reason,
            BroadcastRuntimeSimulationFailureReason::CapabilityMissing
        );
    }

    #[test]
    fn failed_audio_sink_evidence_prevents_simulation_completion() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 77,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let audio_format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut audio_sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 16,
            track_count: 2,
        })
        .unwrap();
        let mut video_presenter =
            BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
                visible_width: 1920,
                visible_height: 1080,
                coded_width: 1920,
                coded_height: 1088,
            })
            .unwrap();

        let result = simulate_broadcast_player_runtime_loop(
            BroadcastVideoSourceMode::ProxyPreview,
            &[presentation_binding],
            &[audio_binding],
            &[video_binding],
            audio_format,
            &mut audio_sink,
            &mut video_presenter,
        );

        assert!(!result.summary.completed);
        assert_eq!(result.summary.audio_submissions, 1);
        assert_eq!(result.summary.audio_accepted, 0);
        assert_eq!(result.summary.video_submissions, 0);
        assert_eq!(result.summary.frame_presented_count, 0);
        assert_eq!(
            result.failures[0].reason,
            BroadcastRuntimeSimulationFailureReason::AudioSinkRejected
        );
    }

    #[test]
    fn failed_video_evidence_prevents_frame_presented() {
        let video_slot = &test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let audio_slot = &test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview)[0];
        let presentation_slot =
            &test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true)[0];
        let audio_binding =
            bind_broadcast_audio_payload(audio_slot, &clock_ready_blocks_120ms()).unwrap();
        let video_binding = bind_broadcast_video_payload_ready(
            video_slot,
            BroadcastVideoPayloadReference {
                payload_id: 78,
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: video_slot.source_frame_index.unwrap(),
                selected_preview_frame_index: video_slot.selected_preview_frame_index,
                presentation_time: video_slot.presentation_time,
                duration: video_slot.duration,
                coded_width: 1920,
                coded_height: 1088,
                visible_width: 1920,
                visible_height: 1080,
                bounded_slot_index: video_slot.slot_index,
                session_index: 0,
            },
        )
        .unwrap();
        let presentation_binding =
            bind_broadcast_presentation_payload(presentation_slot, &video_binding, &audio_binding);
        let audio_format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        let mut audio_sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 2,
        })
        .unwrap();
        let mut video_presenter =
            BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                accepted_format: BroadcastVideoPayloadFormat::Nv12,
                visible_width: 1920,
                visible_height: 1080,
                coded_width: 1920,
                coded_height: 1088,
            })
            .unwrap();

        let result = simulate_broadcast_player_runtime_loop(
            BroadcastVideoSourceMode::ProxyPreview,
            &[presentation_binding],
            &[audio_binding],
            &[video_binding],
            audio_format,
            &mut audio_sink,
            &mut video_presenter,
        );

        assert!(!result.summary.completed);
        assert_eq!(result.summary.audio_accepted, 1);
        assert_eq!(result.summary.video_submissions, 1);
        assert_eq!(result.summary.video_accepted, 0);
        assert_eq!(result.summary.frame_presented_count, 0);
        assert_eq!(
            result.failures[0].reason,
            BroadcastRuntimeSimulationFailureReason::VideoPresenterRejected
        );
    }

    #[test]
    fn broadcast_player_event_surface_reports_ready_prepared_slots() {
        let mut machine = BroadcastRuntimeStateMachine::create(test_broadcast_session()).unwrap();
        let video_slots = test_prepared_video_slots(BroadcastVideoSourceMode::ProxyPreview);
        let audio_slots = test_prepared_audio_slots(BroadcastVideoSourceMode::ProxyPreview);
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::ProxyPreview, true);
        let summary = summarize_broadcast_prepared_slots(
            test_preroll_config(),
            test_preroll_plan(),
            &video_slots,
            &audio_slots,
            &presentation_slots,
        );
        machine
            .prepare_with_preroll(test_prepare_facts(), summary.preroll_status)
            .unwrap();
        machine.play().unwrap();
        machine.account_happy_path().unwrap();
        machine.drain().unwrap();
        machine.complete().unwrap();
        let events = build_broadcast_player_event_surface(
            0,
            test_broadcast_session(),
            test_preroll_config(),
            test_preroll_plan(),
            machine.state(),
            machine.accounting(),
            machine.events(),
            &summary,
            &video_slots,
            &audio_slots,
            &presentation_slots,
        );
        let event_summary =
            summarize_broadcast_player_runtime_events(&events, machine.accounting());

        assert!(
            events.contains(&BroadcastPlayerRuntimeEvent::SessionCreated {
                session_index: 0,
                source_mode: BroadcastVideoSourceMode::ProxyPreview,
                preview_profile: BroadcastPreviewProfile::Journalist50iPreview,
                audio_source_role: BroadcastMediaSourceRole::OriginalAuthoritativeAudio,
                video_source_role: BroadcastMediaSourceRole::ProxyPreviewVideo,
            })
        );
        assert!(events.iter().any(|event| {
            matches!(
                event,
                BroadcastPlayerRuntimeEvent::PrepareStarted {
                    source_mode: BroadcastVideoSourceMode::ProxyPreview,
                    ..
                }
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                BroadcastPlayerRuntimeEvent::PrerollReady {
                    prepared_video_slots: 3,
                    prepared_audio_slots: 3,
                    prepared_presentation_slots: 3,
                }
            )
        }));
        assert!(events.contains(&BroadcastPlayerRuntimeEvent::RuntimeReady {
            state: BroadcastRuntimeState::Ready,
            source_mode: BroadcastVideoSourceMode::ProxyPreview,
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                BroadcastPlayerRuntimeEvent::TransportStarted {
                    state: BroadcastRuntimeState::Playing,
                    ..
                }
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                BroadcastPlayerRuntimeEvent::PreparedSlotAvailable {
                    kind: BroadcastPlayerPreparedSlotKind::Presentation,
                    slot_index: 0,
                    source_mode: BroadcastVideoSourceMode::ProxyPreview,
                    ready: true,
                    ..
                }
            )
        }));
        assert_eq!(event_summary.frame_accounted_events, 53);
        assert_eq!(event_summary.audio_range_accounted_events, 53);
        assert_eq!(event_summary.intentional_skip_events, 53);
        assert_eq!(event_summary.frame_presented_events, 0);
        assert!(event_summary.runtime_ready);
        assert!(event_summary.runtime_completed);
        assert!(
            !format!("{events:?}").contains("FramePresented"),
            "event surface must not claim display presentation"
        );
    }

    #[test]
    fn broadcast_player_event_surface_reports_original_media_capability_missing() {
        let video_slots = (0..3)
            .map(|slot_index| BroadcastPreparedVideoSlot {
                slot_index,
                source_mode: BroadcastVideoSourceMode::OriginalMedia,
                video_source_role: BroadcastMediaSourceRole::OriginalFinishingMedia,
                source_frame_index: Some(u64::try_from(slot_index).unwrap()),
                selected_preview_frame_index: None,
                presentation_time: Duration::from_millis(u64::try_from(slot_index * 40).unwrap()),
                duration: Duration::from_millis(40),
                status: BroadcastPreparedVideoSlotStatus::CapabilityMissing,
            })
            .collect::<Vec<_>>();
        let audio_slots = test_prepared_audio_slots(BroadcastVideoSourceMode::OriginalMedia);
        let presentation_slots =
            test_prepared_presentation_slots(BroadcastVideoSourceMode::OriginalMedia, false);
        let summary = summarize_broadcast_prepared_slots(
            BroadcastPrerollConfig {
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                ..test_preroll_config()
            },
            BroadcastPrerollPlan {
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                video_source_available: true,
                video_runtime_supported: false,
                selected_video_frames_planned: 106,
                intentional_skips_planned: 0,
                ..test_preroll_plan()
            },
            &video_slots,
            &audio_slots,
            &presentation_slots,
        );
        let events = build_broadcast_player_event_surface(
            0,
            BroadcastRuntimeSessionDescription {
                video_source: BroadcastMediaSourceRole::OriginalFinishingMedia,
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                ..test_broadcast_session()
            },
            BroadcastPrerollConfig {
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                ..test_preroll_config()
            },
            BroadcastPrerollPlan {
                video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
                video_source_available: true,
                video_runtime_supported: false,
                selected_video_frames_planned: 106,
                intentional_skips_planned: 0,
                ..test_preroll_plan()
            },
            BroadcastRuntimeState::Preparing,
            BroadcastRuntimeAccounting {
                selected_frames_accounted: 0,
                audio_ranges_accounted: 0,
                intentional_profile_skips: 0,
                lateness_drops: 0,
            },
            &[
                BroadcastRuntimeEvent::SessionCreated,
                BroadcastRuntimeEvent::PreparingStarted,
            ],
            &summary,
            &video_slots,
            &audio_slots,
            &presentation_slots,
        );
        let event_summary = summarize_broadcast_player_runtime_events(
            &events,
            BroadcastRuntimeAccounting {
                selected_frames_accounted: 0,
                audio_ranges_accounted: 0,
                intentional_profile_skips: 0,
                lateness_drops: 0,
            },
        );

        assert!(
            events.contains(&BroadcastPlayerRuntimeEvent::CapabilityMissing {
                source_mode: BroadcastVideoSourceMode::OriginalMedia,
                capability: BroadcastPlayerCapability::OriginalVideoRuntime,
                reason: BroadcastPrerollNotReadyReason::CapabilityMissing,
            })
        );
        assert!(!event_summary.runtime_ready);
        assert!(event_summary.capability_missing);
    }

    fn test_broadcast_session() -> BroadcastRuntimeSessionDescription {
        BroadcastRuntimeSessionDescription {
            audio_source: BroadcastMediaSourceRole::OriginalAuthoritativeAudio,
            video_source: BroadcastMediaSourceRole::ProxyPreviewVideo,
            video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
            preview_profile: BroadcastPreviewProfile::Journalist50iPreview,
            audio_sample_rate: 48_000,
            audio_track_count: 4,
            queue_limits: BroadcastRuntimeQueueLimits {
                audio_block_capacity: 8,
                video_frame_capacity: 6,
                processed_frame_capacity: 3,
            },
            capabilities: BroadcastRuntimeCapabilities {
                sample_clock_aware: true,
                preserves_original_pcm_format: true,
                preserves_track_channel_identity: true,
                proxy_video_preview: true,
                original_media_video_source: true,
                original_media_realtime_supported: false,
                proxy_audio_primary: false,
                ui_dependent: false,
            },
        }
    }

    fn test_prepare_facts() -> BroadcastRuntimePrepareFacts {
        BroadcastRuntimePrepareFacts {
            selected_frame_count: 53,
            intentional_profile_skips: 53,
            audio_ranges_complete: true,
            frames_outside_audio_range: 0,
        }
    }

    fn test_preroll_config() -> BroadcastPrerollConfig {
        BroadcastPrerollConfig {
            video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
            video_frames_required: 3,
            audio_ranges_required: 3,
            max_video_queue: 6,
            max_audio_queue: 8,
            max_presentation_queue: 3,
        }
    }

    fn test_preroll_plan() -> BroadcastPrerollPlan {
        BroadcastPrerollPlan {
            video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
            selected_video_frames_planned: 53,
            audio_ranges_planned: 53,
            intentional_skips_planned: 53,
            duration_covered: Duration::from_millis(2120),
            finite_queue_limits: BroadcastRuntimeQueueLimits {
                audio_block_capacity: 8,
                video_frame_capacity: 6,
                processed_frame_capacity: 3,
            },
            video_source_available: true,
            video_runtime_supported: true,
        }
    }

    fn test_prepared_video_slots(
        source_mode: BroadcastVideoSourceMode,
    ) -> Vec<BroadcastPreparedVideoSlot> {
        (0..3)
            .map(|slot_index| BroadcastPreparedVideoSlot {
                slot_index,
                source_mode,
                video_source_role: match source_mode {
                    BroadcastVideoSourceMode::ProxyPreview => {
                        BroadcastMediaSourceRole::ProxyPreviewVideo
                    }
                    BroadcastVideoSourceMode::OriginalMedia => {
                        BroadcastMediaSourceRole::OriginalFinishingMedia
                    }
                },
                source_frame_index: Some(u64::try_from(slot_index * 2).unwrap()),
                selected_preview_frame_index: match source_mode {
                    BroadcastVideoSourceMode::ProxyPreview => {
                        Some(u64::try_from(slot_index).unwrap())
                    }
                    BroadcastVideoSourceMode::OriginalMedia => None,
                },
                presentation_time: Duration::from_millis(u64::try_from(slot_index * 40).unwrap()),
                duration: Duration::from_millis(40),
                status: BroadcastPreparedVideoSlotStatus::Prepared,
            })
            .collect()
    }

    fn test_prepared_audio_slots(
        source_mode: BroadcastVideoSourceMode,
    ) -> Vec<BroadcastPreparedAudioSlot> {
        (0..3)
            .map(|index| {
                let range = av_frame_audio_range(
                    u64::try_from(index).unwrap(),
                    Duration::from_millis(u64::try_from(index * 40).unwrap()),
                    Duration::from_millis(40),
                    48_000,
                    &clock_ready_blocks_120ms(),
                    2,
                )
                .unwrap();
                BroadcastPreparedAudioSlot::from_audio_range(index, source_mode, &range)
            })
            .collect()
    }

    fn test_prepared_presentation_slots(
        source_mode: BroadcastVideoSourceMode,
        ready: bool,
    ) -> Vec<BroadcastPreparedPresentationSlot> {
        (0..3)
            .map(|index| BroadcastPreparedPresentationSlot {
                presentation_index: index,
                source_mode,
                selected_source_frame: Some(u64::try_from(index * 2).unwrap()),
                video_slot_index: index,
                audio_slot_index: index,
                presentation_time: Duration::from_millis(u64::try_from(index * 40).unwrap()),
                duration: Duration::from_millis(40),
                ready,
            })
            .collect()
    }

    fn clock_ready_blocks() -> Vec<PcmAudioBlock> {
        vec![
            test_pcm_block(3, 0, 0, 960),
            test_pcm_block(3, 0, 960, 960),
            test_pcm_block(3, 0, 1920, 960),
            test_pcm_block(3, 0, 2880, 960),
            test_pcm_block(4, 1, 0, 960),
            test_pcm_block(4, 1, 960, 960),
            test_pcm_block(4, 1, 1920, 960),
            test_pcm_block(4, 1, 2880, 960),
        ]
    }

    fn clock_ready_blocks_120ms() -> Vec<PcmAudioBlock> {
        vec![
            test_pcm_block(3, 0, 0, 960),
            test_pcm_block(3, 0, 960, 960),
            test_pcm_block(3, 0, 1920, 960),
            test_pcm_block(3, 0, 2880, 960),
            test_pcm_block(3, 0, 3840, 960),
            test_pcm_block(3, 0, 4800, 960),
            test_pcm_block(4, 1, 0, 960),
            test_pcm_block(4, 1, 960, 960),
            test_pcm_block(4, 1, 1920, 960),
            test_pcm_block(4, 1, 2880, 960),
            test_pcm_block(4, 1, 3840, 960),
            test_pcm_block(4, 1, 4800, 960),
        ]
    }

    fn test_pcm_block(
        track_id: u32,
        channel_index: u16,
        start_sample: u64,
        sample_count: u32,
    ) -> PcmAudioBlock {
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample: 24,
            endian: PcmEndian::Little,
        };
        PcmAudioBlock::new(
            duration_from_audio_samples(start_sample, 48_000).unwrap(),
            duration_from_audio_samples(u64::from(sample_count), 48_000).unwrap(),
            48_000,
            sample_count,
            format,
            PcmAudioBlockLayout::MonoTrack {
                track_id,
                channel_index,
            },
            vec![0_u8; pcm_payload_byte_len(sample_count, 1, format).unwrap()],
        )
        .unwrap()
    }

    #[test]
    fn audio_timeline_models_multiple_original_tracks() {
        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 1,
            sample_format: AudioSampleFormat::PcmSignedInt {
                bits_per_sample: 24,
            },
        };
        let timeline = AudioTimeline::new(vec![
            OriginalAudioTrack {
                track_id: 3,
                channel_index: 0,
                format,
                sample_count: Some(101_760),
                duration: Duration::from_millis(2120),
            },
            OriginalAudioTrack {
                track_id: 4,
                channel_index: 1,
                format,
                sample_count: Some(101_760),
                duration: Duration::from_millis(2120),
            },
        ])
        .unwrap();

        assert_eq!(timeline.tracks.len(), 2);
        assert_eq!(timeline.sample_rate(), Some(48_000));
        assert_eq!(timeline.bit_depth(), Some(24));
        assert_eq!(timeline.duration, Duration::from_millis(2120));
    }

    #[test]
    fn bounded_audio_queue_and_sink_preserve_monotonic_timing() {
        let mut queue = BoundedQueue::new(2).unwrap();
        let first = AudioTimingPacket {
            track_id: 3,
            start: Duration::ZERO,
            duration: Duration::from_millis(20),
            sample_count: 960,
            has_payload: false,
        };
        let second = AudioTimingPacket {
            start: Duration::from_millis(20),
            ..first
        };
        let third = AudioTimingPacket {
            start: Duration::from_millis(40),
            ..first
        };
        queue.try_push(first).unwrap();
        queue.try_push(second).unwrap();
        assert_eq!(queue.try_push(third), Err(third));

        let mut sink = TestAudioSink::new();
        sink.record(queue.pop_front().unwrap());
        sink.record(queue.pop_front().unwrap());
        assert!(sink.monotonic());
        assert_eq!(queue.stats().peak_depth, 2);
        assert_eq!(queue.stats().backpressure_events, 1);
    }

    #[test]
    fn video_timestamp_range_compares_against_audio_duration() {
        let rate = RationalRate::new(25, 1).unwrap();
        let timestamps = (0..53)
            .map(|position| rate.frame_offset(position).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            max_video_timestamp_outside_audio_range(&timestamps, Duration::from_millis(2120)),
            Duration::ZERO
        );
        assert_eq!(
            duration_abs_delta(Duration::from_millis(2120), Duration::from_millis(2100)),
            Duration::from_millis(20)
        );
    }

    #[test]
    fn test_clock_never_sleeps_real_time() {
        let mut clock = TestClock::new();
        clock.sleep_until(Duration::from_millis(80));
        assert_eq!(clock.now(), Duration::from_millis(80));
        clock.sleep_until(Duration::from_millis(40));
        assert_eq!(clock.now(), Duration::from_millis(80));
    }

    #[test]
    fn late_policy_separates_present_late_and_drop() {
        let config = PlaybackConfig::default().validate().unwrap();
        assert_eq!(
            classify_presentation(Duration::from_millis(1), config),
            PresentationStatus::Presented
        );
        assert_eq!(
            classify_presentation(Duration::from_millis(10), config),
            PresentationStatus::Late
        );
        assert_eq!(
            classify_presentation(Duration::from_millis(41), config),
            PresentationStatus::Dropped
        );
    }

    #[test]
    fn presentation_sink_preserves_identity_and_counts() {
        let mut sink = TestPresentationSink::new();
        sink.record(PresentationDecision {
            identity: FrameIdentity::from_position(53),
            expected: Duration::from_millis(1060),
            actual: Duration::from_millis(1061),
            lateness: Duration::from_millis(1),
            status: PresentationStatus::Presented,
        });
        assert_eq!(sink.decisions()[0].identity.presentation_position, 53);
        assert_eq!(
            sink.counts(),
            PresentationCounts {
                presented: 1,
                late: 0,
                dropped: 0,
                duplicated: 0
            }
        );
    }

    #[test]
    fn config_rejects_unbounded_or_inconsistent_values() {
        assert!(PlaybackConfig {
            compressed_capacity: 0,
            ..PlaybackConfig::default()
        }
        .validate()
        .is_err());
        assert!(PlaybackConfig {
            preroll_frames: 5,
            presentation_capacity: 4,
            ..PlaybackConfig::default()
        }
        .validate()
        .is_err());
        assert!(PlaybackConfig {
            on_time_tolerance: Duration::from_millis(50),
            drop_threshold: Duration::from_millis(40),
            ..PlaybackConfig::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn preroll_frames_must_fit_the_presentation_queue() {
        let config = PlaybackConfig {
            preroll_frames: 3,
            presentation_capacity: 3,
            ..PlaybackConfig::default()
        }
        .validate()
        .unwrap();
        assert_eq!(config.preroll_frames, config.presentation_capacity);
    }

    #[test]
    fn faster_decoder_hits_bounded_backpressure() {
        let mut decoded = BoundedQueue::new(3).expect("queue");
        for frame in 0..3 {
            decoded
                .try_push(FrameIdentity::from_position(frame))
                .unwrap();
        }
        assert_eq!(
            decoded.try_push(FrameIdentity::from_position(3)),
            Err(FrameIdentity::from_position(3))
        );
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded.stats().backpressure_events, 1);
    }

    #[test]
    fn temporary_gpu_backpressure_preserves_input_identity() {
        let mut gpu = BoundedQueue::new(1).expect("queue");
        let first = FrameIdentity::from_position(10);
        let second = FrameIdentity::from_position(11);
        gpu.try_push(first).unwrap();
        assert_eq!(gpu.try_push(second), Err(second));
        assert_eq!(gpu.pop_front(), Some(first));
        gpu.try_push(second).unwrap();
        assert_eq!(gpu.pop_front(), Some(second));
    }

    #[test]
    fn eos_drain_records_remaining_frames_in_presentation_order() {
        let rate = RationalRate::new(50, 1).unwrap();
        let mut clock = TestClock::new();
        let mut sink = TestPresentationSink::new();
        for position in 0..3 {
            let expected = rate.frame_offset(position).unwrap();
            clock.sleep_until(expected);
            sink.record(PresentationDecision {
                identity: FrameIdentity::from_position(position),
                expected,
                actual: clock.now(),
                lateness: Duration::ZERO,
                status: PresentationStatus::Presented,
            });
        }
        let positions = sink
            .decisions()
            .iter()
            .map(|decision| decision.identity.presentation_position)
            .collect::<Vec<_>>();
        assert_eq!(positions, vec![0, 1, 2]);
    }

    #[test]
    fn slow_stage_injection_drops_after_threshold_without_queue_growth() {
        let config = PlaybackConfig::default().validate().unwrap();
        let mut presentation = BoundedQueue::new(2).unwrap();
        presentation
            .try_push(FrameIdentity::from_position(0))
            .unwrap();
        presentation
            .try_push(FrameIdentity::from_position(1))
            .unwrap();
        assert_eq!(
            presentation.try_push(FrameIdentity::from_position(2)),
            Err(FrameIdentity::from_position(2))
        );
        assert_eq!(
            classify_presentation(config.drop_threshold + Duration::from_millis(1), config),
            PresentationStatus::Dropped
        );
        assert_eq!(presentation.stats().peak_depth, 2);
    }
}

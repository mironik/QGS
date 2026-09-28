#![forbid(unsafe_code)]

use std::collections::VecDeque;
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
            Self::InvalidRuntimeTransition => write!(f, "invalid broadcast runtime transition"),
            Self::TimestampOverflow => write!(f, "playback timestamp overflow"),
        }
    }
}

impl std::error::Error for PlaybackError {}

#[cfg(test)]
mod tests {
    use super::*;

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

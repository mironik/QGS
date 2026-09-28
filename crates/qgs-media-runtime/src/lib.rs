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
    TimestampOverflow,
}

impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRate => write!(f, "invalid playback rate"),
            Self::InvalidCapacity => write!(f, "invalid playback queue capacity"),
            Self::InvalidLatePolicy => write!(f, "invalid playback late/drop policy"),
            Self::InvalidAudioFormat => write!(f, "invalid audio format"),
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

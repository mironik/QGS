//! Original MXF audio payload brick.
//! Four discrete mono PCM lanes at 48 kHz 24-bit. Proxy AAC is never authoritative.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use qgs_linux::LinuxOriginalPcmAudioFormat;
use qgs_media_runtime::{
    audio_samples_for_duration, duration_from_audio_samples, PcmAudioBlock, PcmAudioBlockLayout,
    PcmAudioPacket, PcmEndian, PcmSampleFormat,
};
use qgs_mxf::{open_pcm_audio_index, read_pcm_audio_packet_at, MxfTrack, PcmAudioIndex, TrackKind};

/// Live audio output mode at the payload / device edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioOutputMode {
    None,
    PipeWireDesktopMonitor,
    PipeWireMonitor,
    PipeWire4Mono,
}

impl AudioOutputMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PipeWireDesktopMonitor | Self::PipeWireMonitor => "pipewire-desktop-monitor",
            Self::PipeWire4Mono => "pipewire-4mono",
        }
    }

    pub const fn output_channels(self) -> &'static [u16] {
        match self {
            Self::None => &[],
            Self::PipeWireDesktopMonitor | Self::PipeWireMonitor => &[3, 0],
            Self::PipeWire4Mono => &[0, 1, 2, 3],
        }
    }

    pub const fn submitted_label(self) -> &'static str {
        match self {
            Self::None => "off",
            Self::PipeWireDesktopMonitor | Self::PipeWireMonitor => "submitted-monitor",
            Self::PipeWire4Mono => "submitted-4mono",
        }
    }
}

/// Cursor for live PipeWire chunks. A submitted range stays covered until seek,
/// stop, or a new source resets it. Skipping a covered range is not playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioCoverage {
    pub covered_until_sample: u64,
    pub submitted: bool,
}

impl AudioCoverage {
    pub const fn new() -> Self {
        Self {
            covered_until_sample: 0,
            submitted: false,
        }
    }

    pub fn reset(&mut self) {
        self.covered_until_sample = 0;
    }

    pub fn skip_label(self, start_sample: u64) -> Option<&'static str> {
        if start_sample < self.covered_until_sample {
            Some(if self.submitted { "covered" } else { "pending" })
        } else {
            None
        }
    }

    pub fn next_submit_start(self, start_sample: u64) -> Option<u64> {
        if start_sample < self.covered_until_sample {
            None
        } else {
            Some(start_sample)
        }
    }

    pub fn mark_submitted(&mut self, start_sample: u64, sample_count: u64) {
        self.submitted = true;
        self.covered_until_sample = start_sample.saturating_add(sample_count);
    }

    pub fn submitted(self) -> bool {
        self.submitted
    }

    pub fn covered_until_sample(self) -> u64 {
        self.covered_until_sample
    }
}

/// Authoritative original-MXF PCM payload for live output.
pub struct OriginalMxfAudioPayload {
    original_path: PathBuf,
    index: PcmAudioIndex,
    sample_rate: u32,
    source_total_samples: u64,
    mode: AudioOutputMode,
    preview_every: u64,
    coverage: AudioCoverage,
    submissions: u64,
    bytes_copied: usize,
}

impl OriginalMxfAudioPayload {
    pub fn open(
        original_path: &Path,
        mode: AudioOutputMode,
        preview_every: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let index = open_pcm_audio_index(original_path)?;
        let audio_tracks = index
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .collect::<Vec<_>>();
        let source_format = original_linux_pcm_audio_format(&audio_tracks)?;
        let source_total_samples = audio_total_samples_from_index(&index)?;
        Ok(Self {
            original_path: original_path.to_path_buf(),
            index,
            sample_rate: source_format.sample_rate,
            source_total_samples,
            mode,
            preview_every: preview_every.max(1),
            coverage: AudioCoverage::new(),
            submissions: 0,
            bytes_copied: 0,
        })
    }

    pub fn mode(&self) -> AudioOutputMode {
        self.mode
    }

    pub fn preview_every(&self) -> u64 {
        self.preview_every
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn source_total_samples(&self) -> u64 {
        self.source_total_samples
    }

    pub fn coverage(&self) -> AudioCoverage {
        self.coverage
    }

    pub fn coverage_mut(&mut self) -> &mut AudioCoverage {
        &mut self.coverage
    }

    pub fn submissions(&self) -> u64 {
        self.submissions
    }

    pub fn bytes_copied(&self) -> usize {
        self.bytes_copied
    }

    pub fn record_submission(&mut self, bytes_copied: usize) {
        self.submissions = self.submissions.saturating_add(1);
        self.bytes_copied = self.bytes_copied.saturating_add(bytes_copied);
    }

    pub fn build_f32_buffers(
        &self,
        start_sample: u64,
        sample_count: u64,
        samples_per_buffer: u32,
    ) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        let end_sample = start_sample
            .checked_add(sample_count)
            .ok_or("live audio sample range overflow")?;
        if samples_per_buffer == 0 || sample_count % u64::from(samples_per_buffer) != 0 {
            return Err("live audio output range must align to PCM block size".into());
        }
        let blocks = build_original_pcm_blocks_from_index_range(
            &self.original_path,
            &self.index,
            start_sample,
            end_sample,
        )?;
        let track_groups = original_pcm_blocks_by_channel(&blocks, self.sample_rate)?;
        let mut buffers = Vec::new();
        let buffer_count = sample_count / u64::from(samples_per_buffer);
        for buffer_index in 0..buffer_count {
            let buffer_start = start_sample
                .checked_add(
                    buffer_index
                        .checked_mul(u64::from(samples_per_buffer))
                        .ok_or("live audio buffer start overflow")?,
                )
                .ok_or("live audio buffer start overflow")?;
            buffers.push(build_f32_interleaved_range(
                &track_groups,
                self.mode.output_channels(),
                self.sample_rate,
                buffer_start,
                samples_per_buffer,
            )?);
        }
        Ok(buffers)
    }
}

fn original_linux_pcm_audio_format(
    audio_tracks: &[&MxfTrack],
) -> Result<LinuxOriginalPcmAudioFormat, Box<dyn std::error::Error>> {
    let mut sample_rate = None;
    let mut bits_per_sample = None;
    let mut channels_per_track = None;
    for track in audio_tracks {
        let audio = track.audio.as_ref().ok_or("audio descriptor missing")?;
        let track_rate = rational_to_u32(audio.sample_rate.ok_or("audio sample rate missing")?)?;
        let track_depth = audio.bit_depth.ok_or("audio bit depth missing")?;
        let track_channels = audio.channels.ok_or("audio channel count missing")?;
        if let Some(sample_rate) = sample_rate {
            if sample_rate != track_rate {
                return Err("mixed original audio sample rates are not supported yet".into());
            }
        } else {
            sample_rate = Some(track_rate);
        }
        if let Some(bits_per_sample) = bits_per_sample {
            if bits_per_sample != track_depth {
                return Err("mixed original audio bit depths are not supported yet".into());
            }
        } else {
            bits_per_sample = Some(track_depth);
        }
        if let Some(channels_per_track) = channels_per_track {
            if channels_per_track != track_channels {
                return Err("mixed original audio channel layouts are not supported yet".into());
            }
        } else {
            channels_per_track = Some(track_channels);
        }
    }

    Ok(LinuxOriginalPcmAudioFormat {
        sample_rate: sample_rate.ok_or("original audio sample rate unavailable")?,
        bits_per_sample: bits_per_sample.ok_or("original audio bit depth unavailable")?,
        track_count: audio_tracks.len(),
        channels_per_track: channels_per_track.ok_or("original audio channel count unavailable")?,
    })
}

fn audio_total_samples_from_index(
    index: &PcmAudioIndex,
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut totals: BTreeMap<u16, u64> = BTreeMap::new();
    for entry in &index.audio {
        let end = entry
            .start_sample
            .checked_add(u64::from(entry.sample_count))
            .ok_or("audio index sample range overflow")?;
        totals
            .entry(entry.channel_index)
            .and_modify(|current| *current = (*current).max(end))
            .or_insert(end);
    }
    totals
        .values()
        .copied()
        .min()
        .ok_or_else(|| "audio index has no PCM entries".into())
}

fn original_pcm_blocks_by_channel(
    blocks: &[PcmAudioBlock],
    sample_rate: u32,
) -> Result<BTreeMap<u16, Vec<&PcmAudioBlock>>, Box<dyn std::error::Error>> {
    let mut by_channel: BTreeMap<u16, Vec<&PcmAudioBlock>> = BTreeMap::new();
    for block in blocks {
        if block.sample_rate != sample_rate {
            return Err("audio content audit found mixed sample rates".into());
        }
        let PcmAudioBlockLayout::MonoTrack { channel_index, .. } = block.layout else {
            return Err("audio content audit expects mono-track PCM blocks".into());
        };
        by_channel.entry(channel_index).or_default().push(block);
    }
    for channel_blocks in by_channel.values_mut() {
        channel_blocks.sort_by_key(|block| block.start_time);
    }
    Ok(by_channel)
}

fn build_original_pcm_blocks_from_index_range(
    path: &Path,
    index: &PcmAudioIndex,
    start_sample: u64,
    end_sample: u64,
) -> Result<Vec<PcmAudioBlock>, Box<dyn std::error::Error>> {
    let mut blocks = Vec::new();
    for entry in &index.audio {
        let entry_end = entry
            .start_sample
            .checked_add(u64::from(entry.sample_count))
            .ok_or("audio index entry sample range overflow")?;
        if entry_end <= start_sample || entry.start_sample >= end_sample {
            continue;
        }
        let extracted = read_pcm_audio_packet_at(path, entry)?;
        let track = index
            .tracks
            .iter()
            .find(|track| track.id == extracted.entry.track_id)
            .ok_or("PCM packet references missing audio track")?;
        let audio = track
            .audio
            .as_ref()
            .ok_or("PCM packet track has no audio descriptor")?;
        let sample_rate = rational_to_u32(audio.sample_rate.ok_or("audio sample rate missing")?)?;
        let bits_per_sample = audio.bit_depth.ok_or("audio bit depth missing")?;
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample,
            endian: PcmEndian::Little,
        };
        let packet = PcmAudioPacket::new(
            extracted.entry.track_id.0,
            extracted.entry.channel_index,
            duration_from_audio_samples(extracted.entry.start_sample, sample_rate)?,
            duration_from_audio_samples(u64::from(extracted.entry.sample_count), sample_rate)?,
            extracted.entry.sample_count,
            format,
            extracted.payload,
        )?;
        blocks.push(PcmAudioBlock::from_mono_packet(packet, sample_rate)?);
    }
    if blocks.is_empty() {
        return Err("bounded audio audit range extracted no PCM blocks".into());
    }
    Ok(blocks)
}

fn build_f32_interleaved_range(
    track_groups: &BTreeMap<u16, Vec<&PcmAudioBlock>>,
    channels: &[u16],
    sample_rate: u32,
    start_sample: u64,
    sample_count: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if channels.is_empty() {
        return Err("audio audit output requires at least one channel".into());
    }
    let mut output = Vec::with_capacity(
        usize::try_from(sample_count)?
            .checked_mul(channels.len())
            .and_then(|samples| samples.checked_mul(4))
            .ok_or("audio audit f32 output size overflow")?,
    );
    let end_sample = start_sample
        .checked_add(u64::from(sample_count))
        .ok_or("audio audit output range overflow")?;
    for absolute_sample in start_sample..end_sample {
        for channel in channels {
            let blocks = track_groups
                .get(channel)
                .ok_or("requested audio audit channel is missing")?;
            let sample = sample_at_absolute(blocks, absolute_sample)?;
            let value = (sample as f32 / 8_388_608.0).clamp(-1.0, 1.0);
            output.extend_from_slice(&value.to_le_bytes());
        }
    }
    let expected = usize::try_from(sample_count)?
        .checked_mul(channels.len())
        .and_then(|samples| samples.checked_mul(4))
        .ok_or("audio audit expected output size overflow")?;
    if output.len() != expected {
        return Err("audio audit f32 output size mismatch".into());
    }
    let _ = sample_rate;
    Ok(output)
}

fn sample_at_absolute(
    blocks: &[&PcmAudioBlock],
    absolute_sample: u64,
) -> Result<i32, Box<dyn std::error::Error>> {
    let first = blocks.first().ok_or("no blocks for sample lookup")?;
    let first_start = audio_samples_for_duration(first.start_time, first.sample_rate)?;
    if absolute_sample < first_start {
        return Err("audio sample is before first block".into());
    }
    let samples_per_block = first.sample_count;
    if samples_per_block == 0 {
        return Err("audio block has zero samples".into());
    }
    let relative_sample = absolute_sample - first_start;
    let block_index = usize::try_from(relative_sample / u64::from(samples_per_block))?;
    let local_sample = usize::try_from(relative_sample % u64::from(samples_per_block))?;
    let block = blocks
        .get(block_index)
        .ok_or("audio sample range is not covered by original PCM blocks")?;
    let PcmSampleFormat::SignedInteger {
        bits_per_sample,
        endian,
    } = block.format;
    if bits_per_sample != 24 || endian != PcmEndian::Little {
        return Err("audio content audit expects 24-bit little-endian PCM blocks".into());
    }
    if block.sample_count != samples_per_block {
        return Err("audio content audit expects stable sample counts per block".into());
    }
    let block_start = audio_samples_for_duration(block.start_time, block.sample_rate)?;
    if block_start != first_start + u64::try_from(block_index)? * u64::from(samples_per_block) {
        return Err("audio content audit sample lookup found a gap or overlap".into());
    }
    Ok(decode_s24le_i32(&block.payload, local_sample))
}

fn decode_s24le_i32(payload: &[u8], sample_index: usize) -> i32 {
    let offset = sample_index * 3;
    let bytes = &payload[offset..offset + 3];
    let mut value = i32::from(bytes[0]) | (i32::from(bytes[1]) << 8) | (i32::from(bytes[2]) << 16);
    if value & 0x0080_0000 != 0 {
        value |= !0x00ff_ffff;
    }
    value
}

fn rational_to_u32(value: qgs_mxf::Rational) -> Result<u32, Box<dyn std::error::Error>> {
    if value.denominator == 0 || value.numerator % value.denominator != 0 {
        return Err("non-integer audio sample rate is not supported in Step 20A".into());
    }
    Ok(value.numerator / value.denominator)
}

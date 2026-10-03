//! Audio device boundary brick: PipeWire submission of original-MXF PCM buffers.
//! Does not own authoritative lane identity; that stays in `audio_payload`.

use std::time::Duration;

use qgs_audio_pipewire::{
    submit_native_pipewire_buffers, PipeWireAudioSampleFormat, PipeWireStreamFormat,
};

use crate::audio_payload::{AudioOutputMode, OriginalMxfAudioPayload};

pub struct AudioDeviceBoundary;

impl AudioDeviceBoundary {
    /// Submit f32 interleaved buffers already built from original MXF payload.
    pub fn submit_payload_buffers(
        payload: &mut OriginalMxfAudioPayload,
        start_sample: u64,
        sample_count: u64,
        samples_per_buffer: u32,
        buffers: Vec<Vec<u8>>,
    ) -> &'static str {
        let stream_format = PipeWireStreamFormat {
            sample_rate: payload.sample_rate(),
            channels: u32::try_from(payload.mode().output_channels().len()).unwrap_or(0),
            sample_format: PipeWireAudioSampleFormat::F32Interleaved,
        };
        match submit_native_pipewire_buffers(
            stream_format,
            buffers,
            samples_per_buffer,
            Duration::from_secs(5),
        ) {
            Ok(report) if report.buffer_submitted => {
                payload.record_submission(report.bytes_copied);
                payload
                    .coverage_mut()
                    .mark_submitted(start_sample, sample_count);
                payload.mode().submitted_label()
            }
            Ok(_) => "not-submitted",
            Err(_) => "failed",
        }
    }

    pub fn emit_for_tick(
        payload: &mut OriginalMxfAudioPayload,
        tick: u64,
        audio_sample_range: Option<(u64, u64)>,
        max_frames_per_chunk: u64,
    ) -> &'static str {
        let emit_every = match payload.mode() {
            AudioOutputMode::PipeWireDesktopMonitor | AudioOutputMode::PipeWireMonitor => 25,
            AudioOutputMode::PipeWire4Mono => payload.preview_every(),
            AudioOutputMode::None => 1,
        };
        if !tick.is_multiple_of(emit_every.max(1)) {
            return "skipped";
        }
        let Some((start_sample, end_sample)) = audio_sample_range else {
            return "missing-range";
        };
        let per_frame_samples = end_sample.saturating_sub(start_sample).max(1);
        if payload.coverage().next_submit_start(start_sample).is_none() {
            return payload
                .coverage()
                .skip_label(start_sample)
                .unwrap_or("pending");
        }
        let audio_frames_per_chunk = match payload.mode() {
            AudioOutputMode::PipeWireDesktopMonitor | AudioOutputMode::PipeWireMonitor => 25,
            AudioOutputMode::PipeWire4Mono => {
                payload.preview_every().min(max_frames_per_chunk).max(1)
            }
            AudioOutputMode::None => 0,
        };
        let mut sample_count = match per_frame_samples.checked_mul(audio_frames_per_chunk) {
            Some(value) => value,
            None => return "failed",
        };
        if sample_count == 0 {
            return "off";
        }
        if let Some(remaining) = payload.source_total_samples().checked_sub(start_sample) {
            sample_count = sample_count.min(remaining);
        } else {
            return "missing-range";
        }
        let samples_per_buffer = match u32::try_from(per_frame_samples) {
            Ok(value) => value,
            Err(_) => return "failed",
        };
        let buffers =
            match payload.build_f32_buffers(start_sample, sample_count, samples_per_buffer) {
                Ok(buffer) => buffer,
                Err(_) => return "failed",
            };
        Self::submit_payload_buffers(
            payload,
            start_sample,
            sample_count,
            samples_per_buffer,
            buffers,
        )
    }
}

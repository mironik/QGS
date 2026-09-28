#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::spa;
use pw::{properties::properties, spa::pod::Pod};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeWireAudioSampleFormat {
    F32Interleaved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeWireStreamFormat {
    pub sample_rate: u32,
    pub channels: u32,
    pub sample_format: PipeWireAudioSampleFormat,
}

impl PipeWireStreamFormat {
    pub fn validate(self) -> Result<Self, PipeWireStreamError> {
        if self.sample_rate == 0 {
            return Err(PipeWireStreamError::InvalidFormat("sample rate is zero"));
        }
        if self.channels == 0 || self.channels > spa::param::audio::MAX_CHANNELS as u32 {
            return Err(PipeWireStreamError::InvalidFormat(
                "channel count is outside PipeWire SPA limits",
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeWireNativeStreamEvidenceLevel {
    ConnectFailed,
    StreamCreateFailed,
    NativeStreamCreated,
    NativeStreamConfigured,
    BufferDequeued,
    BufferSubmitted,
    StreamError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeWireObservedStreamState {
    Unconnected,
    Connecting,
    Paused,
    Streaming,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipeWireStreamCreationReport {
    pub connect_attempted: bool,
    pub connected: bool,
    pub stream_create_attempted: bool,
    pub stream_created: bool,
    pub stream_configured: bool,
    pub selected_format: PipeWireStreamFormat,
    pub observed_states: Vec<PipeWireObservedStreamState>,
    pub final_state: Option<PipeWireObservedStreamState>,
    pub evidence_level: PipeWireNativeStreamEvidenceLevel,
    pub audio_device_verified: bool,
    pub buffer_submitted: bool,
    pub status_message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipeWireBufferSubmissionReport {
    pub stream_report: PipeWireStreamCreationReport,
    pub process_callback_reached: bool,
    pub buffer_dequeued: bool,
    pub buffer_capacity: usize,
    pub samples_converted: u32,
    pub output_channels: u32,
    pub f32_samples_written: usize,
    pub bytes_copied: usize,
    pub buffer_submitted: bool,
    pub evidence_level: PipeWireNativeStreamEvidenceLevel,
    pub audio_device_verified: bool,
    pub status_message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PipeWireStreamError {
    InvalidFormat(&'static str),
    Connect(String),
    StreamCreate(String),
    StreamConnect(String),
    Timeout,
}

impl fmt::Display for PipeWireStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(message) => write!(f, "invalid PipeWire stream format: {message}"),
            Self::Connect(message) => write!(f, "PipeWire connect failed: {message}"),
            Self::StreamCreate(message) => write!(f, "PipeWire stream creation failed: {message}"),
            Self::StreamConnect(message) => {
                write!(f, "PipeWire stream configuration failed: {message}")
            }
            Self::Timeout => write!(f, "PipeWire stream did not configure before timeout"),
        }
    }
}

impl std::error::Error for PipeWireStreamError {}

#[derive(Default)]
struct StreamObservation {
    states: Vec<PipeWireObservedStreamState>,
}

#[derive(Default)]
struct SubmissionObservation {
    states: Vec<PipeWireObservedStreamState>,
    process_callback_reached: bool,
    buffer_dequeued: bool,
    buffer_capacity: usize,
    bytes_copied: usize,
    buffer_submitted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonoS24LeTrack<'a> {
    pub channel_index: u16,
    pub payload: &'a [u8],
}

pub fn create_native_pipewire_stream(
    format: PipeWireStreamFormat,
    timeout: Duration,
) -> Result<PipeWireStreamCreationReport, PipeWireStreamError> {
    let format = format.validate()?;
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|err| PipeWireStreamError::Connect(err.to_string()))?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|err| PipeWireStreamError::Connect(err.to_string()))?;
    let core = context
        .connect_rc(None)
        .map_err(|err| PipeWireStreamError::Connect(err.to_string()))?;

    let stream = pw::stream::StreamBox::new(
        &core,
        "qgs-native-audio-stream-create",
        properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::AUDIO_CHANNELS => format.channels.to_string(),
        },
    )
    .map_err(|err| PipeWireStreamError::StreamCreate(err.to_string()))?;

    let observation = Rc::new(RefCell::new(StreamObservation::default()));
    let listener_observation = Rc::clone(&observation);
    let listener_mainloop = mainloop.clone();
    let _listener = stream
        .add_local_listener::<()>()
        .state_changed(move |_stream, _user_data, _old, new| {
            let observed = observed_stream_state(&new);
            listener_observation.borrow_mut().states.push(observed);
            if matches!(
                observed,
                PipeWireObservedStreamState::Paused
                    | PipeWireObservedStreamState::Streaming
                    | PipeWireObservedStreamState::Error
            ) {
                listener_mainloop.quit();
            }
        })
        .register()
        .map_err(|err| PipeWireStreamError::StreamCreate(err.to_string()))?;

    let values = format_pod_values(format)?;
    let mut params = [Pod::from_bytes(&values).ok_or_else(|| {
        PipeWireStreamError::StreamConnect("serialized PipeWire format pod was invalid".to_string())
    })?];

    stream
        .connect(
            spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|err| PipeWireStreamError::StreamConnect(err.to_string()))?;

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(50)));
        if observation.borrow().states.iter().any(|state| {
            matches!(
                state,
                PipeWireObservedStreamState::Paused
                    | PipeWireObservedStreamState::Streaming
                    | PipeWireObservedStreamState::Error
            )
        }) {
            break;
        }
    }

    let states = observation.borrow().states.clone();
    if states.is_empty() {
        return Err(PipeWireStreamError::Timeout);
    }
    let final_state = states.last().copied();
    let stream_configured = states.iter().any(|state| {
        matches!(
            state,
            PipeWireObservedStreamState::Paused | PipeWireObservedStreamState::Streaming
        )
    });

    Ok(PipeWireStreamCreationReport {
        connect_attempted: true,
        connected: true,
        stream_create_attempted: true,
        stream_created: true,
        stream_configured,
        selected_format: format,
        observed_states: states,
        final_state,
        evidence_level: if stream_configured {
            PipeWireNativeStreamEvidenceLevel::NativeStreamConfigured
        } else {
            PipeWireNativeStreamEvidenceLevel::NativeStreamCreated
        },
        audio_device_verified: false,
        buffer_submitted: false,
        status_message: if stream_configured {
            "native PipeWire stream reached configured state; no buffer was submitted".to_string()
        } else {
            "native PipeWire stream was created but did not reach configured state".to_string()
        },
    })
}

pub fn submit_native_pipewire_buffer(
    format: PipeWireStreamFormat,
    f32_interleaved_le: Vec<u8>,
    frame_count: u32,
    timeout: Duration,
) -> Result<PipeWireBufferSubmissionReport, PipeWireStreamError> {
    let format = format.validate()?;
    validate_f32_interleaved_buffer(&f32_interleaved_le, frame_count, format.channels)?;
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|err| PipeWireStreamError::Connect(err.to_string()))?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|err| PipeWireStreamError::Connect(err.to_string()))?;
    let core = context
        .connect_rc(None)
        .map_err(|err| PipeWireStreamError::Connect(err.to_string()))?;

    let stream = pw::stream::StreamBox::new(
        &core,
        "qgs-native-audio-buffer-submit",
        properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::AUDIO_CHANNELS => format.channels.to_string(),
        },
    )
    .map_err(|err| PipeWireStreamError::StreamCreate(err.to_string()))?;

    let observation = Rc::new(RefCell::new(SubmissionObservation::default()));
    let listener_observation = Rc::clone(&observation);
    let state_observation = Rc::clone(&observation);
    let payload = Rc::new(f32_interleaved_le);
    let process_payload = Rc::clone(&payload);
    let listener_mainloop = mainloop.clone();
    let process_mainloop = mainloop.clone();
    let stride = usize::try_from(format.channels)
        .ok()
        .and_then(|channels| channels.checked_mul(4))
        .ok_or(PipeWireStreamError::InvalidFormat("stream stride overflow"))?;

    let _listener = stream
        .add_local_listener::<()>()
        .state_changed(move |_stream, _user_data, _old, new| {
            let observed = observed_stream_state(&new);
            state_observation.borrow_mut().states.push(observed);
            if matches!(observed, PipeWireObservedStreamState::Error) {
                listener_mainloop.quit();
            }
        })
        .process(move |stream, _user_data| {
            let mut observation = listener_observation.borrow_mut();
            observation.process_callback_reached = true;
            if observation.buffer_submitted {
                process_mainloop.quit();
                return;
            }
            let Some(mut buffer) = stream.dequeue_buffer() else {
                process_mainloop.quit();
                return;
            };
            observation.buffer_dequeued = true;
            let Some(data) = buffer.datas_mut().get_mut(0) else {
                process_mainloop.quit();
                return;
            };
            let Some(slice) = data.data() else {
                process_mainloop.quit();
                return;
            };
            observation.buffer_capacity = slice.len();
            if slice.len() < process_payload.len() {
                process_mainloop.quit();
                return;
            }
            slice[..process_payload.len()].copy_from_slice(&process_payload);
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = i32::try_from(stride).unwrap_or(i32::MAX);
            *chunk.size_mut() = u32::try_from(process_payload.len()).unwrap_or(u32::MAX);
            observation.bytes_copied = process_payload.len();
            observation.buffer_submitted = true;
            drop(buffer);
            process_mainloop.quit();
        })
        .register()
        .map_err(|err| PipeWireStreamError::StreamCreate(err.to_string()))?;

    let values = format_pod_values(format)?;
    let mut params = [Pod::from_bytes(&values).ok_or_else(|| {
        PipeWireStreamError::StreamConnect("serialized PipeWire format pod was invalid".to_string())
    })?];

    stream
        .connect(
            spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|err| PipeWireStreamError::StreamConnect(err.to_string()))?;

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(50)));
        if observation.borrow().buffer_submitted {
            break;
        }
    }

    let observation = observation.borrow();
    let states = observation.states.clone();
    if states.is_empty() && !observation.process_callback_reached {
        return Err(PipeWireStreamError::Timeout);
    }
    let final_state = states.last().copied();
    let stream_configured = states.iter().any(|state| {
        matches!(
            state,
            PipeWireObservedStreamState::Paused | PipeWireObservedStreamState::Streaming
        )
    });
    let stream_report = PipeWireStreamCreationReport {
        connect_attempted: true,
        connected: true,
        stream_create_attempted: true,
        stream_created: true,
        stream_configured,
        selected_format: format,
        observed_states: states,
        final_state,
        evidence_level: if observation.buffer_submitted {
            PipeWireNativeStreamEvidenceLevel::BufferSubmitted
        } else if observation.buffer_dequeued {
            PipeWireNativeStreamEvidenceLevel::BufferDequeued
        } else if stream_configured {
            PipeWireNativeStreamEvidenceLevel::NativeStreamConfigured
        } else {
            PipeWireNativeStreamEvidenceLevel::NativeStreamCreated
        },
        audio_device_verified: false,
        buffer_submitted: observation.buffer_submitted,
        status_message: if observation.buffer_submitted {
            "native PipeWire buffer was queued; audible playback is not verified".to_string()
        } else {
            "native PipeWire stream did not accept the prototype buffer".to_string()
        },
    };

    Ok(PipeWireBufferSubmissionReport {
        stream_report,
        process_callback_reached: observation.process_callback_reached,
        buffer_dequeued: observation.buffer_dequeued,
        buffer_capacity: observation.buffer_capacity,
        samples_converted: frame_count,
        output_channels: format.channels,
        f32_samples_written: usize::try_from(frame_count)
            .unwrap_or(0)
            .saturating_mul(usize::try_from(format.channels).unwrap_or(0)),
        bytes_copied: observation.bytes_copied,
        buffer_submitted: observation.buffer_submitted,
        evidence_level: if observation.buffer_submitted {
            PipeWireNativeStreamEvidenceLevel::BufferSubmitted
        } else if observation.buffer_dequeued {
            PipeWireNativeStreamEvidenceLevel::BufferDequeued
        } else {
            PipeWireNativeStreamEvidenceLevel::NativeStreamConfigured
        },
        audio_device_verified: false,
        status_message: if observation.buffer_submitted {
            "bounded prototype buffer queued to native PipeWire stream; no audible verification claimed"
                .to_string()
        } else {
            "bounded prototype buffer was not queued".to_string()
        },
    })
}

pub fn f32_interleaved_from_s24le_mono_tracks(
    tracks: &[MonoS24LeTrack<'_>],
    frame_count: u32,
) -> Result<Vec<u8>, PipeWireStreamError> {
    if tracks.is_empty() {
        return Err(PipeWireStreamError::InvalidFormat("no source tracks"));
    }
    for track in tracks {
        let needed = usize::try_from(frame_count)
            .ok()
            .and_then(|frames| frames.checked_mul(3))
            .ok_or(PipeWireStreamError::InvalidFormat(
                "24-bit source payload size overflow",
            ))?;
        if track.payload.len() < needed {
            return Err(PipeWireStreamError::InvalidFormat(
                "source track is shorter than requested frame count",
            ));
        }
    }
    let mut sorted = tracks.to_vec();
    sorted.sort_by_key(|track| track.channel_index);
    let capacity = usize::try_from(frame_count)
        .ok()
        .and_then(|frames| frames.checked_mul(sorted.len()))
        .and_then(|samples| samples.checked_mul(4))
        .ok_or(PipeWireStreamError::InvalidFormat(
            "converted output size overflow",
        ))?;
    let mut output = Vec::with_capacity(capacity);
    for frame in 0..usize::try_from(frame_count).unwrap_or(0) {
        for track in &sorted {
            let sample = s24le_sample_to_f32(track.payload, frame)?;
            output.extend_from_slice(&sample.to_le_bytes());
        }
    }
    Ok(output)
}

pub fn s24le_sample_to_f32(
    payload: &[u8],
    sample_index: usize,
) -> Result<f32, PipeWireStreamError> {
    let offset = sample_index
        .checked_mul(3)
        .ok_or(PipeWireStreamError::InvalidFormat(
            "24-bit sample offset overflow",
        ))?;
    let bytes = payload
        .get(offset..offset + 3)
        .ok_or(PipeWireStreamError::InvalidFormat(
            "24-bit sample index out of range",
        ))?;
    let mut value = i32::from(bytes[0]) | (i32::from(bytes[1]) << 8) | (i32::from(bytes[2]) << 16);
    if value & 0x0080_0000 != 0 {
        value |= !0x00ff_ffff;
    }
    Ok((value as f32 / 8_388_608.0).clamp(-1.0, 1.0))
}

fn validate_f32_interleaved_buffer(
    bytes: &[u8],
    frame_count: u32,
    channels: u32,
) -> Result<(), PipeWireStreamError> {
    let expected = usize::try_from(frame_count)
        .ok()
        .and_then(|frames| frames.checked_mul(usize::try_from(channels).ok()?))
        .and_then(|samples| samples.checked_mul(4))
        .ok_or(PipeWireStreamError::InvalidFormat(
            "f32 interleaved buffer size overflow",
        ))?;
    if bytes.len() != expected {
        return Err(PipeWireStreamError::InvalidFormat(
            "f32 interleaved buffer size does not match frame count and channel count",
        ));
    }
    Ok(())
}

fn format_pod_values(format: PipeWireStreamFormat) -> Result<Vec<u8>, PipeWireStreamError> {
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    match format.sample_format {
        PipeWireAudioSampleFormat::F32Interleaved => {
            audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
        }
    }
    audio_info.set_rate(format.sample_rate);
    audio_info.set_channels(format.channels);
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    for (index, channel) in default_channel_positions(format.channels)
        .iter()
        .enumerate()
    {
        position[index] = *channel;
    }
    audio_info.set_position(position);

    pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(pw::spa::pod::Object {
            type_: spa_sys::SPA_TYPE_OBJECT_Format,
            id: spa_sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .map(|serialized| serialized.0.into_inner())
    .map_err(|err| PipeWireStreamError::StreamConnect(format!("{err:?}")))
}

pub fn default_channel_positions(channels: u32) -> Vec<u32> {
    match channels {
        1 => vec![spa_sys::SPA_AUDIO_CHANNEL_MONO],
        2 => vec![spa_sys::SPA_AUDIO_CHANNEL_FL, spa_sys::SPA_AUDIO_CHANNEL_FR],
        4 => vec![
            spa_sys::SPA_AUDIO_CHANNEL_FL,
            spa_sys::SPA_AUDIO_CHANNEL_FR,
            spa_sys::SPA_AUDIO_CHANNEL_RL,
            spa_sys::SPA_AUDIO_CHANNEL_RR,
        ],
        count => (0..count)
            .map(|index| {
                if index < spa::param::audio::MAX_CHANNELS as u32 {
                    spa_sys::SPA_AUDIO_CHANNEL_UNKNOWN
                } else {
                    0
                }
            })
            .collect(),
    }
}

fn observed_stream_state(state: &pw::stream::StreamState) -> PipeWireObservedStreamState {
    match state {
        pw::stream::StreamState::Error(_) => PipeWireObservedStreamState::Error,
        pw::stream::StreamState::Unconnected => PipeWireObservedStreamState::Unconnected,
        pw::stream::StreamState::Connecting => PipeWireObservedStreamState::Connecting,
        pw::stream::StreamState::Paused => PipeWireObservedStreamState::Paused,
        pw::stream::StreamState::Streaming => PipeWireObservedStreamState::Streaming,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_format_rejects_invalid_values() {
        assert!(PipeWireStreamFormat {
            sample_rate: 0,
            channels: 4,
            sample_format: PipeWireAudioSampleFormat::F32Interleaved,
        }
        .validate()
        .is_err());
        assert!(PipeWireStreamFormat {
            sample_rate: 48_000,
            channels: 0,
            sample_format: PipeWireAudioSampleFormat::F32Interleaved,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn stream_format_accepts_48k_four_channel_f32_boundary_format() {
        let format = PipeWireStreamFormat {
            sample_rate: 48_000,
            channels: 4,
            sample_format: PipeWireAudioSampleFormat::F32Interleaved,
        }
        .validate()
        .unwrap();

        assert_eq!(format.sample_rate, 48_000);
        assert_eq!(format.channels, 4);
    }

    #[test]
    fn channel_positions_are_stable_for_four_channel_prototype() {
        assert_eq!(
            default_channel_positions(4),
            vec![
                spa_sys::SPA_AUDIO_CHANNEL_FL,
                spa_sys::SPA_AUDIO_CHANNEL_FR,
                spa_sys::SPA_AUDIO_CHANNEL_RL,
                spa_sys::SPA_AUDIO_CHANNEL_RR,
            ]
        );
    }

    #[test]
    fn s24le_to_f32_conversion_handles_key_values() {
        assert_eq!(s24le_sample_to_f32(&[0x00, 0x00, 0x00], 0).unwrap(), 0.0);
        assert!(s24le_sample_to_f32(&[0xff, 0xff, 0x7f], 0).unwrap() > 0.999_999);
        assert_eq!(s24le_sample_to_f32(&[0x00, 0x00, 0x80], 0).unwrap(), -1.0);
        assert_eq!(
            s24le_sample_to_f32(&[0x01, 0x00, 0x00], 0).unwrap(),
            1.0 / 8_388_608.0
        );
        assert_eq!(
            s24le_sample_to_f32(&[0xff, 0xff, 0xff], 0).unwrap(),
            -1.0 / 8_388_608.0
        );
    }

    #[test]
    fn four_mono_tracks_are_interleaved_by_channel_index() {
        let tracks = [
            MonoS24LeTrack {
                channel_index: 2,
                payload: &[0x00, 0x00, 0x40],
            },
            MonoS24LeTrack {
                channel_index: 0,
                payload: &[0x00, 0x00, 0x00],
            },
            MonoS24LeTrack {
                channel_index: 3,
                payload: &[0x00, 0x00, 0x80],
            },
            MonoS24LeTrack {
                channel_index: 1,
                payload: &[0xff, 0xff, 0x7f],
            },
        ];

        let bytes = f32_interleaved_from_s24le_mono_tracks(&tracks, 1).unwrap();
        assert_eq!(bytes.len(), 4 * 4);
        let values = bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(values[0], 0.0);
        assert!(values[1] > 0.999_999);
        assert_eq!(values[2], 0.5);
        assert_eq!(values[3], -1.0);
    }

    #[test]
    fn f32_interleaved_buffer_size_validation_is_exact() {
        assert!(validate_f32_interleaved_buffer(&vec![0_u8; 960 * 4 * 4], 960, 4).is_ok());
        assert!(validate_f32_interleaved_buffer(&vec![0_u8; 7], 960, 4).is_err());
    }
}

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
}

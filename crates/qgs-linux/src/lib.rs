#![forbid(unsafe_code)]

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::{AsFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;

use qgs_protocol::{
    decode_wire_header, decode_wire_message_parts, encode_wire_message, ProtocolError, WireMessage,
    MAX_PAYLOAD_LEN, WIRE_HEADER_LEN,
};
use unix_ancillary::UnixStreamExt;

pub const MAX_ATTACHMENT_COUNT: usize = 1;

pub fn default_socket_path() -> PathBuf {
    PathBuf::from("/tmp/qgsd.sock")
}

pub fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    cleanup_stale_socket(path)?;
    UnixListener::bind(path)
}

pub fn connect_socket(path: &Path) -> io::Result<UnixStream> {
    UnixStream::connect(path)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinuxOriginalPcmAudioFormat {
    pub sample_rate: u32,
    pub bits_per_sample: u8,
    pub track_count: usize,
    pub channels_per_track: u16,
}

impl LinuxOriginalPcmAudioFormat {
    pub fn validate(self) -> Result<Self, LinuxAudioProbeError> {
        if self.sample_rate == 0 || self.bits_per_sample == 0 {
            return Err(LinuxAudioProbeError::InvalidSourceFormat(
                "sample rate and bit depth must be non-zero",
            ));
        }
        if self.track_count == 0 || self.channels_per_track == 0 {
            return Err(LinuxAudioProbeError::InvalidSourceFormat(
                "track and channel counts must be non-zero",
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinuxAudioProbeOutcome {
    CapabilityProbeOnly,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinuxAudioConversionNeed {
    No,
    Yes,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxAudioDeviceProbeReport {
    pub source_format: LinuxOriginalPcmAudioFormat,
    pub pipewire_runtime_socket_available: bool,
    pub pipewire_cli_available: bool,
    pub pipewire_server_reachable: Option<bool>,
    pub wireplumber_cli_available: bool,
    pub default_output_device_available: Option<bool>,
    pub direct_original_pcm_acceptance_known: Option<bool>,
    pub conversion_needed: LinuxAudioConversionNeed,
    pub timing_evidence_available: Option<bool>,
    pub audio_device_verified: bool,
    pub outcome: LinuxAudioProbeOutcome,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinuxAudioProbeError {
    InvalidSourceFormat(&'static str),
}

impl std::fmt::Display for LinuxAudioProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSourceFormat(message) => write!(f, "invalid source format: {message}"),
        }
    }
}

impl std::error::Error for LinuxAudioProbeError {}

pub fn probe_linux_audio_device_boundary(
    source_format: LinuxOriginalPcmAudioFormat,
) -> Result<LinuxAudioDeviceProbeReport, LinuxAudioProbeError> {
    let source_format = source_format.validate()?;
    let pipewire_runtime_socket_available = pipewire_runtime_socket_available();
    let pipewire_cli_available = command_available("pw-cli");
    let pipewire_server_reachable = pipewire_cli_available.then(pipewire_server_reachable);
    let wireplumber_cli_available = command_available("wpctl");
    let default_output_device_available =
        wireplumber_cli_available.then(default_output_device_available);

    let mut notes = Vec::new();
    if source_format.sample_rate == 48_000 {
        notes.push("source sample rate is the normal professional audio rate");
    }
    if source_format.bits_per_sample == 24 {
        notes.push("source keeps 24-bit PCM as engine truth");
    }
    if source_format.track_count > 2 || source_format.channels_per_track == 1 {
        notes.push("device boundary will need explicit routing, interleaving, or mixing policy");
    }
    notes.push("no PipeWire stream was opened by this safe capability probe");
    notes.push("AudioDeviceVerified is not claimed without real stream/evidence");

    let server_visible =
        pipewire_runtime_socket_available || pipewire_server_reachable.unwrap_or(false);
    let conversion_needed = if !server_visible {
        LinuxAudioConversionNeed::Unknown
    } else if source_format.bits_per_sample == 24 && source_format.track_count == 2 {
        LinuxAudioConversionNeed::Unknown
    } else {
        LinuxAudioConversionNeed::Yes
    };

    Ok(LinuxAudioDeviceProbeReport {
        source_format,
        pipewire_runtime_socket_available,
        pipewire_cli_available,
        pipewire_server_reachable,
        wireplumber_cli_available,
        default_output_device_available,
        direct_original_pcm_acceptance_known: None,
        conversion_needed,
        timing_evidence_available: None,
        audio_device_verified: false,
        outcome: if server_visible {
            LinuxAudioProbeOutcome::CapabilityProbeOnly
        } else {
            LinuxAudioProbeOutcome::Unavailable
        },
        notes,
    })
}

fn pipewire_runtime_socket_available() -> bool {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|runtime_dir| runtime_dir.join("pipewire-0").exists())
        .unwrap_or(false)
}

fn pipewire_server_reachable() -> bool {
    Command::new("pw-cli")
        .arg("info")
        .arg("0")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn default_output_device_available() -> bool {
    Command::new("wpctl")
        .arg("status")
        .output()
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.contains("Sinks:") || line.contains("Audio/Sink"))
        })
        .unwrap_or(false)
}

fn command_available(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join(command).is_file()))
        .unwrap_or(false)
}

pub fn send_message(stream: &mut UnixStream, message: &WireMessage) -> io::Result<()> {
    stream.write_all(&encode_wire_message(message))
}

pub fn send_message_with_attachments(
    stream: &mut UnixStream,
    message: &WireMessage,
    attachments: &[impl AsFd],
) -> io::Result<()> {
    if attachments.len() > MAX_ATTACHMENT_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "too many transport attachments",
        ));
    }

    stream.send_fds_all(&encode_wire_message(message), attachments)
}

pub fn receive_message_with_attachments<const N: usize>(
    stream: &mut UnixStream,
) -> Result<ReceivedMessage, TransportError> {
    if N > MAX_ATTACHMENT_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "too many expected transport attachments",
        )
        .into());
    }

    let mut bytes = vec![0_u8; WIRE_HEADER_LEN + MAX_PAYLOAD_LEN as usize];
    let (len, attachments) = stream.recv_fds_exact_into::<N>(&mut bytes)?;
    bytes.truncate(len);
    let message = qgs_protocol::decode_wire_message(&bytes)?;

    Ok(ReceivedMessage {
        message,
        attachments,
    })
}

pub fn receive_message(stream: &mut UnixStream) -> Result<WireMessage, TransportError> {
    let mut header_bytes = [0_u8; WIRE_HEADER_LEN];
    stream.read_exact(&mut header_bytes)?;
    let header = decode_wire_header(&header_bytes)?;

    let payload_len = header.payload_len as usize;
    if header.payload_len > MAX_PAYLOAD_LEN {
        return Err(ProtocolError::PayloadTooLarge {
            len: header.payload_len,
            max: MAX_PAYLOAD_LEN,
        }
        .into());
    }

    let mut payload = vec![0_u8; payload_len];
    stream.read_exact(&mut payload)?;

    Ok(decode_wire_message_parts(header, &payload)?)
}

#[derive(Debug)]
pub struct ReceivedMessage {
    pub message: WireMessage,
    pub attachments: Vec<OwnedFd>,
}

pub fn remove_socket_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => fs::remove_file(path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "socket path exists and is not a Unix socket",
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn cleanup_stale_socket(path: &Path) -> io::Result<()> {
    match UnixStream::connect(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "socket path is already accepting connections",
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => remove_socket_file(path),
    }
}

#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    Protocol(ProtocolError),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "transport I/O error: {err}"),
            Self::Protocol(err) => write!(f, "transport protocol error: {err}"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ProtocolError> for TransportError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::unix::net::UnixStream;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    use qgs_core::SessionManager;
    use qgs_protocol::{HelloRequest, WireMessage, CURRENT_PROTOCOL_VERSION};

    use super::*;

    #[test]
    fn exchanges_hello_welcome_over_unix_socket() {
        let path = unique_socket_path();
        let listener = bind_socket(&path).expect("bind socket");

        let server = thread::spawn({
            let path = path.clone();
            move || {
                let (mut stream, _) = listener.accept().expect("accept client");
                let sessions = SessionManager::new();
                let message = receive_message(&mut stream).expect("receive hello");
                let WireMessage::Hello {
                    request_id,
                    request,
                } = message
                else {
                    panic!("expected HELLO");
                };

                let response = sessions.handle_hello(&request).expect("handle hello");
                send_message(
                    &mut stream,
                    &WireMessage::Welcome {
                        request_id,
                        response,
                    },
                )
                .expect("send welcome");
                remove_socket_file(&path).expect("remove socket");
            }
        });

        let mut client = connect_socket(&path).expect("connect socket");
        send_message(
            &mut client,
            &WireMessage::Hello {
                request_id: 42,
                request: HelloRequest::current(),
            },
        )
        .expect("send hello");

        let response = receive_message(&mut client).expect("receive welcome");
        let WireMessage::Welcome {
            request_id,
            response,
        } = response
        else {
            panic!("expected WELCOME");
        };

        assert_eq!(request_id, 42);
        assert_eq!(response.version, CURRENT_PROTOCOL_VERSION);
        assert_eq!(response.session_id.get(), 1);

        server.join().expect("server thread");
    }

    #[test]
    fn transfers_owned_fd_attachment_over_unix_socket() {
        let (mut sender, mut receiver) = UnixStream::pair().expect("socket pair");
        let file = File::open("/dev/null").expect("open /dev/null");
        let message = WireMessage::EnumerateDevices { request_id: 77 };

        let send_thread = thread::spawn(move || {
            send_message_with_attachments(&mut sender, &message, &[&file]).expect("send fd");
        });

        let received =
            receive_message_with_attachments::<1>(&mut receiver).expect("receive attached fd");

        send_thread.join().expect("send thread");
        assert_eq!(
            received.message,
            WireMessage::EnumerateDevices { request_id: 77 }
        );
        assert_eq!(received.attachments.len(), 1);

        let mut received_file = File::from(received.attachments.into_iter().next().expect("fd"));
        let mut sink = [0_u8; 1];
        assert_eq!(received_file.read(&mut sink).expect("read fd"), 0);
    }

    #[test]
    fn rejects_too_many_outgoing_attachments() {
        let (mut sender, _receiver) = UnixStream::pair().expect("socket pair");
        let first = File::open("/dev/null").expect("open /dev/null");
        let second = File::open("/dev/null").expect("open /dev/null");

        let err = send_message_with_attachments(
            &mut sender,
            &WireMessage::EnumerateDevices { request_id: 78 },
            &[&first, &second],
        )
        .expect_err("too many attachments are rejected");

        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn rejects_missing_expected_attachment() {
        let (mut sender, mut receiver) = UnixStream::pair().expect("socket pair");
        let message = WireMessage::EnumerateDevices { request_id: 79 };

        let send_thread = thread::spawn(move || {
            send_message(&mut sender, &message).expect("send message without fd");
        });

        let err = receive_message_with_attachments::<1>(&mut receiver)
            .expect_err("missing attachment is rejected");

        send_thread.join().expect("send thread");
        assert!(matches!(err, TransportError::Io(_)));
    }

    #[test]
    fn audio_probe_rejects_invalid_source_format() {
        let err = LinuxOriginalPcmAudioFormat {
            sample_rate: 0,
            bits_per_sample: 24,
            track_count: 4,
            channels_per_track: 1,
        }
        .validate()
        .expect_err("zero sample rate rejected");

        assert!(matches!(err, LinuxAudioProbeError::InvalidSourceFormat(_)));
    }

    #[test]
    fn audio_probe_preserves_original_pcm_as_authoritative_source() {
        let report = probe_linux_audio_device_boundary(LinuxOriginalPcmAudioFormat {
            sample_rate: 48_000,
            bits_per_sample: 24,
            track_count: 4,
            channels_per_track: 1,
        })
        .expect("probe classification");

        assert_eq!(report.source_format.sample_rate, 48_000);
        assert_eq!(report.source_format.bits_per_sample, 24);
        assert_eq!(report.source_format.track_count, 4);
        assert_eq!(report.source_format.channels_per_track, 1);
        assert!(!report.audio_device_verified);
        assert_eq!(
            report.direct_original_pcm_acceptance_known, None,
            "capability probe must not claim direct device format acceptance"
        );
        assert!(
            matches!(
                report.outcome,
                LinuxAudioProbeOutcome::CapabilityProbeOnly | LinuxAudioProbeOutcome::Unavailable
            ),
            "probe is not allowed to claim real playback"
        );
    }

    fn unique_socket_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("qgs-test-{}-{nanos}.sock", std::process::id()))
    }
}

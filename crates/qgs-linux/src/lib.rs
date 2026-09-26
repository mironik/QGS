#![forbid(unsafe_code)]

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use qgs_protocol::{
    decode_wire_header, decode_wire_message_parts, encode_wire_message, ProtocolError, WireMessage,
    MAX_PAYLOAD_LEN, WIRE_HEADER_LEN,
};

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

pub fn send_message(stream: &mut UnixStream, message: &WireMessage) -> io::Result<()> {
    stream.write_all(&encode_wire_message(message))
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

    fn unique_socket_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("qgs-test-{}-{nanos}.sock", std::process::id()))
    }
}

#![forbid(unsafe_code)]

use std::path::PathBuf;

use qgs_core::{DeviceDiscovery, SessionManager};
use qgs_linux::{
    bind_socket, default_socket_path, receive_message, remove_socket_file, send_message,
    TransportError,
};
use qgs_protocol::{
    DeviceListResponse, ErrorResponse, ProtocolError, ProtocolErrorCode, WireMessage,
};
use qgs_vulkan::VulkanDeviceDiscovery;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path_from_args();
    let listener = bind_socket(&socket_path)?;
    let sessions = SessionManager::new();
    let discovery = VulkanDeviceDiscovery::new();

    println!("qgsd listening on {}", socket_path.display());

    for stream in listener.incoming() {
        let mut stream = stream?;
        handle_client(&mut stream, &sessions, &discovery)?;
    }

    remove_socket_file(&socket_path)?;

    Ok(())
}

fn handle_client(
    stream: &mut std::os::unix::net::UnixStream,
    sessions: &SessionManager,
    discovery: &impl DeviceDiscovery,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut session_established = false;

    loop {
        let message = match receive_message(stream) {
            Ok(message) => message,
            Err(TransportError::Io(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(());
            }
            Err(TransportError::Protocol(err)) => {
                send_message(
                    stream,
                    &WireMessage::Error {
                        request_id: 0,
                        response: ErrorResponse {
                            code: ProtocolErrorCode::from(&err),
                        },
                    },
                )?;
                continue;
            }
            Err(TransportError::Io(err)) => return Err(err.into()),
        };
        let request_id = message.request_id();

        let response = match message {
            WireMessage::Hello { request, .. } => match sessions.handle_hello(&request) {
                Ok(response) => WireMessage::Welcome {
                    request_id,
                    response,
                },
                Err(err) => WireMessage::Error {
                    request_id,
                    response: ErrorResponse {
                        code: ProtocolErrorCode::from(&err),
                    },
                },
            },
            WireMessage::EnumerateDevices { .. } => {
                if !session_established {
                    WireMessage::Error {
                        request_id,
                        response: ErrorResponse {
                            code: ProtocolErrorCode::SessionRequired,
                        },
                    }
                } else {
                    match discovery.enumerate_devices() {
                        Ok(devices) => WireMessage::DeviceList {
                            request_id,
                            response: DeviceListResponse { devices },
                        },
                        Err(err) => {
                            eprintln!("device discovery failed: {err}");
                            WireMessage::Error {
                                request_id,
                                response: ErrorResponse {
                                    code: ProtocolErrorCode::DiscoveryFailed,
                                },
                            }
                        }
                    }
                }
            }
            WireMessage::Welcome { .. }
            | WireMessage::Error { .. }
            | WireMessage::DeviceList { .. } => WireMessage::Error {
                request_id,
                response: ErrorResponse {
                    code: ProtocolErrorCode::from(&ProtocolError::MalformedPayload),
                },
            },
        };

        if matches!(response, WireMessage::Welcome { .. }) {
            session_established = true;
        }
        send_message(stream, &response)?;
    }
}

fn socket_path_from_args() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_socket_path)
}

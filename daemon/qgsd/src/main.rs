#![forbid(unsafe_code)]

use std::path::PathBuf;

use qgs_core::SessionManager;
use qgs_linux::{
    bind_socket, default_socket_path, receive_message, remove_socket_file, send_message,
    TransportError,
};
use qgs_protocol::{ErrorResponse, ProtocolErrorCode, WireMessage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path_from_args();
    let listener = bind_socket(&socket_path)?;
    let sessions = SessionManager::new();

    println!("qgsd listening on {}", socket_path.display());

    for stream in listener.incoming() {
        let mut stream = stream?;
        let message = match receive_message(&mut stream) {
            Ok(message) => message,
            Err(TransportError::Protocol(err)) => {
                send_message(
                    &mut stream,
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
            WireMessage::Welcome { .. } | WireMessage::Error { .. } => WireMessage::Error {
                request_id,
                response: ErrorResponse {
                    code: ProtocolErrorCode::MalformedPayload,
                },
            },
        };

        send_message(&mut stream, &response)?;
    }

    remove_socket_file(&socket_path)?;

    Ok(())
}

fn socket_path_from_args() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_socket_path)
}

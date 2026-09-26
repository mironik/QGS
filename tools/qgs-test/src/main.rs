#![forbid(unsafe_code)]

use std::path::PathBuf;

use qgs_linux::{connect_socket, default_socket_path, receive_message, send_message};
use qgs_protocol::{HelloRequest, WireMessage, CURRENT_PROTOCOL_VERSION};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path_from_args();
    let request_id = 1;
    let mut stream = connect_socket(&socket_path)?;

    send_message(
        &mut stream,
        &WireMessage::Hello {
            request_id,
            request: HelloRequest::current(),
        },
    )?;

    let response = receive_message(&mut stream)?;
    let WireMessage::Welcome {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected WELCOME response".into());
    };

    if response_request_id != request_id {
        return Err("response request_id did not match request".into());
    }

    if response.version != CURRENT_PROTOCOL_VERSION {
        return Err("negotiated unexpected protocol version".into());
    }

    println!(
        "verified HELLO -> WELCOME version={} session_id={}",
        response.version,
        response.session_id.get()
    );

    Ok(())
}

fn socket_path_from_args() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_socket_path)
}

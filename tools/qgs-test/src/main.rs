#![forbid(unsafe_code)]

use std::path::PathBuf;

use qgs_linux::{connect_socket, default_socket_path, receive_message, send_message};
use qgs_protocol::{DeviceDesc, HelloRequest, WireMessage, CURRENT_PROTOCOL_VERSION};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path_from_args();
    let mut stream = connect_socket(&socket_path)?;
    let mut request_id = 1;

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
        "QGS session established: version={} session_id={}",
        response.version,
        response.session_id.get()
    );

    request_id += 1;
    send_message(&mut stream, &WireMessage::EnumerateDevices { request_id })?;

    let response = receive_message(&mut stream)?;
    let WireMessage::DeviceList {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected DEVICE_LIST response".into());
    };

    if response_request_id != request_id {
        return Err("device list request_id did not match request".into());
    }

    println!();
    println!("Devices:");
    for device in response.devices {
        print_device(&device);
    }

    Ok(())
}

fn print_device(device: &DeviceDesc) {
    println!("- [{:?}] {}", device.class, device.name);
    println!(
        "  vendor=0x{:04x} device=0x{:04x} backend={} api={}",
        device.vendor_id, device.device_id, device.backend, device.api_version
    );
}

fn socket_path_from_args() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_socket_path)
}

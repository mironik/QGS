#![forbid(unsafe_code)]

use std::path::PathBuf;

use qgs_linux::{connect_socket, default_socket_path, receive_message, send_message};
use qgs_protocol::{
    BufferDesc, BufferUsageFlags, CreateBufferRequest, DestroyResourceRequest, DeviceCapabilities,
    DeviceClass, DeviceDesc, HelloRequest, MemoryPreference, QueryDeviceCapabilitiesRequest,
    ResourceId, SelectedMemoryProperties, WireMessage, CURRENT_PROTOCOL_VERSION,
};

const DEMO_BUFFER_SIZE: u64 = 1024 * 1024;

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
    for device in &response.devices {
        print_device(device);
        request_id += 1;
        send_message(
            &mut stream,
            &WireMessage::QueryDeviceCapabilities {
                request_id,
                request: QueryDeviceCapabilitiesRequest {
                    device_id: device.id,
                },
            },
        )?;

        let capability_response = receive_message(&mut stream)?;
        let WireMessage::DeviceCapabilities {
            request_id: response_request_id,
            response,
        } = capability_response
        else {
            return Err("expected DEVICE_CAPABILITIES response".into());
        };

        if response_request_id != request_id {
            return Err("capability response request_id did not match request".into());
        }

        if response.capabilities.device_id != device.id {
            return Err("capability response device_id did not match request".into());
        }

        print_capabilities(&response.capabilities);
    }

    println!();
    println!("Buffers:");
    let physical_devices = response
        .devices
        .iter()
        .filter(|device| {
            matches!(
                device.class,
                DeviceClass::IntegratedGpu | DeviceClass::DiscreteGpu
            )
        })
        .collect::<Vec<_>>();

    for device in &physical_devices {
        request_id += 1;
        let resource_id = create_buffer(&mut stream, request_id, device, DEMO_BUFFER_SIZE)?;

        request_id += 1;
        destroy_resource(&mut stream, request_id, resource_id)?;
        println!("Resource destroyed successfully.");
        println!();
    }

    if let Some(device) = physical_devices.first().copied() {
        println!("Creating transient buffers for disconnect cleanup on:");
        println!("[{:?}] {}", device.class, device.name);
        for _ in 0..3 {
            request_id += 1;
            let _ = create_buffer(&mut stream, request_id, device, 64 * 1024)?;
        }
        println!("Leaving 3 transient buffers alive and disconnecting.");
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

fn print_capabilities(capabilities: &DeviceCapabilities) {
    println!("  Compute:");
    println!("    supported: {}", yes_no(capabilities.compute.supported));
    println!(
        "    max workgroup count: {} x {} x {}",
        capabilities.compute.max_workgroup_count[0],
        capabilities.compute.max_workgroup_count[1],
        capabilities.compute.max_workgroup_count[2]
    );
    println!(
        "    max workgroup size: {} x {} x {}",
        capabilities.compute.max_workgroup_size[0],
        capabilities.compute.max_workgroup_size[1],
        capabilities.compute.max_workgroup_size[2]
    );
    println!(
        "    max invocations: {}",
        capabilities.compute.max_workgroup_invocations
    );

    println!("  Memory:");
    for (index, heap) in capabilities.memory.heaps.iter().enumerate() {
        println!(
            "    heap {index}: {} MiB{}",
            heap.size_bytes / (1024 * 1024),
            if heap.device_local {
                ", device-local"
            } else {
                ""
            }
        );
    }
    println!(
        "    memory types: {}",
        capabilities.memory.memory_type_count
    );
    println!(
        "    host-visible: {}",
        yes_no(capabilities.memory.host_visible)
    );
    println!(
        "    host-coherent: {}",
        yes_no(capabilities.memory.host_coherent)
    );
    println!(
        "    device-local: {}",
        yes_no(capabilities.memory.device_local)
    );

    println!("  Interop:");
    println!(
        "    external-memory-fd: {}",
        yes_no(capabilities.interop.external_memory_fd)
    );
    println!(
        "    dma-buf mechanism: {}",
        yes_no(capabilities.interop.dma_buf)
    );
    println!(
        "    semaphore-fd: {}",
        yes_no(capabilities.interop.external_semaphore_fd)
    );
    println!(
        "    fence-fd: {}",
        yes_no(capabilities.interop.external_fence_fd)
    );
}

fn create_buffer(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: u64,
    device: &DeviceDesc,
    size_bytes: u64,
) -> Result<ResourceId, Box<dyn std::error::Error>> {
    println!("Creating {} buffer on:", format_size(size_bytes));
    println!("[{:?}] {}", device.class, device.name);
    send_message(
        stream,
        &WireMessage::CreateBuffer {
            request_id,
            request: CreateBufferRequest {
                desc: BufferDesc {
                    device_id: device.id,
                    size_bytes,
                    usage: BufferUsageFlags::TRANSFER_SRC
                        | BufferUsageFlags::TRANSFER_DST
                        | BufferUsageFlags::STORAGE,
                    memory_preference: MemoryPreference {
                        device_preferred: true,
                        host_visible_required: false,
                        host_coherent_preferred: true,
                    },
                },
            },
        },
    )?;

    let response = receive_message(stream)?;
    let WireMessage::BufferCreated {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected BUFFER_CREATED response".into());
    };

    if response_request_id != request_id {
        return Err("buffer created request_id did not match request".into());
    }

    println!("Resource created:");
    println!("  id: {}", response.resource_id.get());
    println!("  size: {}", response.size_bytes);
    print_selected_memory(response.selected_memory);

    Ok(response.resource_id)
}

fn destroy_resource(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: u64,
    resource_id: ResourceId,
) -> Result<(), Box<dyn std::error::Error>> {
    send_message(
        stream,
        &WireMessage::DestroyResource {
            request_id,
            request: DestroyResourceRequest { resource_id },
        },
    )?;

    let response = receive_message(stream)?;
    let WireMessage::ResourceDestroyed {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected RESOURCE_DESTROYED response".into());
    };

    if response_request_id != request_id {
        return Err("resource destroyed request_id did not match request".into());
    }
    if response.resource_id != resource_id {
        return Err("resource destroyed id did not match request".into());
    }

    Ok(())
}

fn print_selected_memory(memory: SelectedMemoryProperties) {
    println!("  selected memory:");
    println!("    device-local: {}", yes_no(memory.device_local));
    println!("    host-visible: {}", yes_no(memory.host_visible));
    println!("    host-coherent: {}", yes_no(memory.host_coherent));
}

fn format_size(size_bytes: u64) -> String {
    if size_bytes.is_multiple_of(1024 * 1024) {
        format!("{} MiB", size_bytes / (1024 * 1024))
    } else if size_bytes.is_multiple_of(1024) {
        format!("{} KiB", size_bytes / 1024)
    } else {
        format!("{size_bytes} bytes")
    }
}

const fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn socket_path_from_args() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_socket_path)
}

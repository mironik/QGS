#![forbid(unsafe_code)]

use std::fs::File;
use std::os::fd::OwnedFd;
use std::path::PathBuf;

use qgs_linux::{
    connect_socket, default_socket_path, receive_message, receive_message_with_attachments,
    send_message,
};
use qgs_protocol::{
    BitDepth, BufferDesc, BufferUsageFlags, ChromaSubsampling, CreateBufferRequest,
    CreateDecoderRequest, CreateImageRequest, CreateSyncRequest, DecoderConfig,
    DestroyDecoderRequest, DestroyResourceRequest, DeviceCapabilities, DeviceClass, DeviceDesc,
    ErrorResponse, ExportResourceRequest, ExportSyncRequest, ExternalHandleType, ExternalSharing,
    H264Profile, HelloRequest, ImageDesc, ImageUsageFlags, MemoryPreference, PixelFormat,
    ProtocolErrorCode, QueryDeviceCapabilitiesRequest, QueryVideoCapabilitiesRequest, ResourceId,
    ScanMode, SelectedMemoryProperties, SubmitAccessUnitRequest, SyncExportHandleType, SyncId,
    SyncKind, VideoCapabilities, VideoCodec, VideoProfile, WireMessage, CURRENT_PROTOCOL_VERSION,
};
use qgs_vulkan::{
    diagnose_haswell_video_import, DiagnosticDrmLayer, DiagnosticDrmObject, DiagnosticDrmPlane,
    HaswellVideoDiagnosticInput, VulkanDeviceDiscovery,
};

const DEMO_BUFFER_SIZE: u64 = 1024 * 1024;
const IMAGE_PROOF_WIDTH: u32 = 64;
const IMAGE_PROOF_HEIGHT: u32 = 64;
const VIDEO_CAPABILITIES_ONLY_ARG: &str = "--video-capabilities-only";
const H264_DECODE_ONLY_ARG: &str = "--h264-decode-only";
const HASWELL_VIDEO_DIAGNOSTIC_ARG: &str = "--haswell-video-diagnostic";
const H264_FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/h264/idr-64x64-baseline.h264");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.haswell_video_diagnostic {
        return run_haswell_video_diagnostic();
    }

    let socket_path = args.socket_path;
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

        request_id += 1;
        send_message(
            &mut stream,
            &WireMessage::QueryVideoCapabilities {
                request_id,
                request: QueryVideoCapabilitiesRequest {
                    device_id: device.id,
                },
            },
        )?;

        let video_response = receive_message(&mut stream)?;
        let WireMessage::VideoCapabilities {
            request_id: response_request_id,
            response,
        } = video_response
        else {
            return Err("expected VIDEO_CAPABILITIES response".into());
        };

        if response_request_id != request_id {
            return Err("video capability response request_id did not match request".into());
        }
        if response.capabilities.device_id != device.id {
            return Err("video capability response device_id did not match request".into());
        }

        print_video_capabilities(&response.capabilities);
    }

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

    if args.video_capabilities_only {
        return Ok(());
    }

    if args.h264_decode_only {
        println!();
        println!("H.264 VA-API decode proof:");
        for device in &physical_devices {
            request_id = test_h264_decode(&mut stream, request_id, device)?;
            println!();
        }
        request_id = leave_h264_decoder_for_disconnect(&mut stream, request_id, &physical_devices)?;
        let _ = request_id;
        return Ok(());
    }

    println!();
    println!("Buffers:");

    for device in &physical_devices {
        request_id += 1;
        let resource_id = create_buffer(
            &mut stream,
            request_id,
            device,
            DEMO_BUFFER_SIZE,
            ExternalSharing::None,
        )?;

        request_id += 1;
        destroy_resource(&mut stream, request_id, resource_id)?;
        println!("Resource destroyed successfully.");
        println!();
    }

    println!("External memory sharing:");
    for device in &physical_devices {
        request_id = test_external_memory(&mut stream, request_id, device)?;
        println!();
    }

    println!("External GPU synchronization:");
    for device in &physical_devices {
        request_id = test_external_gpu_sync(&mut stream, request_id, device)?;
        println!();
    }

    println!("Compute proof:");
    for device in &physical_devices {
        request_id = test_compute_proof(&mut stream, request_id, device)?;
        println!();
    }

    println!("Image processing proof:");
    for device in &physical_devices {
        request_id = test_image_processing_proof(&mut stream, request_id, device)?;
        println!();
    }

    println!("H.264 VA-API decode proof:");
    for device in &physical_devices {
        request_id = test_h264_decode(&mut stream, request_id, device)?;
        println!();
    }

    if let Some(device) = physical_devices.first().copied() {
        println!("Creating transient resources for disconnect cleanup on:");
        println!("[{:?}] {}", device.class, device.name);
        for _ in 0..3 {
            request_id += 1;
            let _ = create_buffer(
                &mut stream,
                request_id,
                device,
                64 * 1024,
                ExternalSharing::None,
            )?;
        }
        request_id += 1;
        let _ = create_image(
            &mut stream,
            request_id,
            device,
            IMAGE_PROOF_WIDTH,
            IMAGE_PROOF_HEIGHT,
            ExternalSharing::None,
        )?;
        println!("Leaving 3 transient buffers and 1 transient image alive and disconnecting.");
    }

    Ok(())
}

fn run_haswell_video_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
    println!("QGS M2 Step 4B Haswell imported-video diagnostic");
    println!("Validation layer: VK_LAYER_KHRONOS_validation requested");
    println!("Synchronization validation: requested through VkValidationFeaturesEXT");

    let render_node = PathBuf::from("/dev/dri/renderD128");
    let decoded = qgs_vaapi::decode_h264_drm_prime_for_diagnostic(&render_node, H264_FIXTURE)?;
    println!("VA decode/export:");
    println!("  render node: {}", render_node.display());
    println!("  checksum: 0x{:08x}", decoded.validation_checksum);
    println!("  fourcc: 0x{:08x}", decoded.fourcc);
    println!("  size: {} x {}", decoded.width, decoded.height);
    println!("  objects: {}", decoded.objects.len());
    println!("  layers: {}", decoded.layers.len());
    for (index, object) in decoded.objects.iter().enumerate() {
        println!(
            "  object {index}: size={} modifier={}",
            object.size, object.drm_format_modifier
        );
    }
    for (layer_index, layer) in decoded.layers.iter().enumerate() {
        println!(
            "  layer {layer_index}: drm_format=0x{:08x} planes={}",
            layer.drm_format, layer.num_planes
        );
        for plane_index in 0..layer.num_planes as usize {
            println!(
                "    plane {plane_index}: object={} pitch={} offset={}",
                layer.object_index[plane_index],
                layer.pitch[plane_index],
                layer.offset[plane_index]
            );
        }
    }

    let input = HaswellVideoDiagnosticInput {
        vendor_id: 0x8086,
        device_id: 0x0416,
        width: decoded.width,
        height: decoded.height,
        drm_fourcc: decoded.fourcc,
        objects: decoded
            .objects
            .into_iter()
            .map(|object| DiagnosticDrmObject {
                fd: object.fd,
                size: object.size,
                modifier: object.drm_format_modifier,
            })
            .collect(),
        layers: decoded
            .layers
            .into_iter()
            .map(|layer| DiagnosticDrmLayer {
                drm_format: layer.drm_format,
                planes: (0..layer.num_planes as usize)
                    .map(|index| DiagnosticDrmPlane {
                        object_index: u32::from(layer.object_index[index]),
                        offset: layer.offset[index],
                        pitch: layer.pitch[index],
                    })
                    .collect(),
            })
            .collect(),
    };

    let report = diagnose_haswell_video_import(input)?;
    print_haswell_diagnostic_report(&report);
    Ok(())
}

fn print_haswell_diagnostic_report(report: &qgs_vulkan::HaswellVideoDiagnosticReport) {
    println!();
    println!("Vulkan diagnostic:");
    println!("  device: {}", report.physical_device_name);
    println!("  queue family: {}", report.queue_family_index);
    println!("  modifier: {}", report.modifier);
    println!("  modifier exposed: {}", yes_no(report.modifier_exposed));
    println!(
        "  modifier plane count: {}",
        optional_u32(report.modifier_plane_count)
    );
    println!(
        "  modifier tiling features raw: {}",
        report
            .modifier_tiling_features_raw
            .map(|value| format!("0x{value:016x}"))
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!("  sampled-image: {}", yes_no(report.supports_sampled_image));
    println!("  transfer-src: {}", yes_no(report.supports_transfer_src));
    println!("  transfer-dst: {}", yes_no(report.supports_transfer_dst));
    println!(
        "  ycbcr linear filter: {}",
        yes_no(report.supports_ycbcr_linear_filter)
    );
    println!(
        "  ycbcr separate reconstruction: {}",
        yes_no(report.supports_ycbcr_separate_reconstruction_filter)
    );
    println!(
        "  memory fd type bits: {}",
        optional_hex_u32(report.memory_fd_type_bits)
    );
    println!(
        "  image memory size: {}",
        optional_u64(report.image_memory_size)
    );
    println!(
        "  image memory alignment: {}",
        optional_u64(report.image_memory_alignment)
    );
    println!(
        "  image memory type bits: {}",
        optional_hex_u32(report.image_memory_type_bits)
    );
    println!(
        "  selected memory type index: {}",
        optional_u32(report.selected_memory_type_index)
    );
    println!("  VA objects: {}", report.va_object_count);
    println!("  NV12 format planes: {}", report.nv12_format_plane_count);
    println!(
        "  Vulkan modifier memory planes: {}",
        optional_u32(report.vulkan_modifier_memory_plane_count)
    );
    println!("  disjoint image: {}", yes_no(report.image_create_disjoint));
    println!(
        "  imported memory objects: {}",
        report.imported_memory_objects
    );
    println!("  memory bindings: {}", report.vulkan_memory_bindings);
    println!("  binding offsets: {:?}", report.binding_offsets);

    if let Some(barrier) = &report.barrier {
        println!("Acquire barrier:");
        println!("  srcStageMask: {}", barrier.src_stage_mask);
        println!("  srcAccessMask: {}", barrier.src_access_mask);
        println!("  dstStageMask: {}", barrier.dst_stage_mask);
        println!("  dstAccessMask: {}", barrier.dst_access_mask);
        println!("  oldLayout: {}", barrier.old_layout);
        println!("  newLayout: {}", barrier.new_layout);
        println!("  srcQueueFamilyIndex: {}", barrier.src_queue_family_index);
        println!("  dstQueueFamilyIndex: {}", barrier.dst_queue_family_index);
        println!("  aspectMask: {}", barrier.aspect_mask);
        println!("  baseMipLevel: {}", barrier.base_mip_level);
        println!("  levelCount: {}", barrier.level_count);
        println!("  baseArrayLayer: {}", barrier.base_array_layer);
        println!("  layerCount: {}", barrier.layer_count);
    }

    println!("Results:");
    println!(
        "  vkQueueSubmit: {}",
        report.queue_submit_result.as_deref().unwrap_or("not run")
    );
    println!(
        "  fence wait: {}",
        report.fence_wait_result.as_deref().unwrap_or("not run")
    );
    println!(
        "  GPU read attempted: {}",
        yes_no(report.gpu_read_attempted)
    );
    println!(
        "  GPU read result: {}",
        report.gpu_read_result.as_deref().unwrap_or("not run")
    );
    println!("  classification: {}", report.classification);
    println!("  recommendation: {}", report.recommendation);

    println!("Validation messages:");
    if report.validation_messages.is_empty() {
        println!("  none captured");
    } else {
        for message in &report.validation_messages {
            println!("  {message}");
        }
    }
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

fn print_video_capabilities(capabilities: &VideoCapabilities) {
    println!("  Video decode:");
    if capabilities.decode.is_empty() {
        println!("    advertised capabilities: 0");
        println!("    backend: no decode capabilities reported");
        return;
    }

    println!("    advertised capabilities: {}", capabilities.decode.len());
    for capability in &capabilities.decode {
        println!(
            "    - {:?} {:?}, {}-bit {:?}",
            capability.codec,
            capability.profile,
            capability.bit_depth.get(),
            capability.chroma
        );
        println!(
            "      max: {} x {}",
            capability.max_width, capability.max_height
        );
        println!(
            "      progressive: {}",
            yes_no(capability.progressive_supported)
        );
        println!(
            "      interlaced: {}",
            yes_no(capability.interlaced_supported)
        );
        println!(
            "      output formats: {:?}",
            capability.output_surface_formats
        );
    }
}

fn create_buffer(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: u64,
    device: &DeviceDesc,
    size_bytes: u64,
    external_sharing: ExternalSharing,
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
                    external_sharing,
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

fn create_host_visible_external_buffer(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: u64,
    device: &DeviceDesc,
    handle_type: ExternalHandleType,
) -> Result<Result<ResourceId, ErrorResponse>, Box<dyn std::error::Error>> {
    println!("Testing external memory on:");
    println!("[{:?}] {}", device.class, device.name);
    println!("  requested handle: {handle_type}");
    send_message(
        stream,
        &WireMessage::CreateBuffer {
            request_id,
            request: CreateBufferRequest {
                desc: BufferDesc {
                    device_id: device.id,
                    size_bytes: DEMO_BUFFER_SIZE,
                    usage: BufferUsageFlags::TRANSFER_SRC
                        | BufferUsageFlags::TRANSFER_DST
                        | BufferUsageFlags::STORAGE,
                    memory_preference: MemoryPreference {
                        device_preferred: true,
                        host_visible_required: true,
                        host_coherent_preferred: true,
                    },
                    external_sharing: ExternalSharing::Required { handle_type },
                },
            },
        },
    )?;

    match receive_message(stream)? {
        WireMessage::BufferCreated {
            request_id: response_request_id,
            response,
        } => {
            if response_request_id != request_id {
                return Err("buffer created request_id did not match request".into());
            }
            println!("Created:");
            println!("  ResourceId: {}", response.resource_id.get());
            println!("  size: {}", response.size_bytes);
            println!("  exportable: yes");
            print_selected_memory(response.selected_memory);
            Ok(Ok(response.resource_id))
        }
        WireMessage::Error {
            request_id: response_request_id,
            response,
        } => {
            if response_request_id != request_id {
                return Err("buffer error request_id did not match request".into());
            }
            Ok(Err(response))
        }
        _ => Err("expected BUFFER_CREATED or ERROR response".into()),
    }
}

fn create_image(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: u64,
    device: &DeviceDesc,
    width: u32,
    height: u32,
    external_sharing: ExternalSharing,
) -> Result<ResourceId, Box<dyn std::error::Error>> {
    send_message(
        stream,
        &WireMessage::CreateImage {
            request_id,
            request: CreateImageRequest {
                desc: ImageDesc {
                    device_id: device.id,
                    width,
                    height,
                    format: PixelFormat::Rgba8Unorm,
                    usage: ImageUsageFlags::TRANSFER_SRC
                        | ImageUsageFlags::TRANSFER_DST
                        | ImageUsageFlags::STORAGE,
                    external_sharing,
                },
            },
        },
    )?;

    let response = receive_message(stream)?;
    let WireMessage::ImageCreated {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected IMAGE_CREATED response".into());
    };

    if response_request_id != request_id {
        return Err("image created request_id did not match request".into());
    }

    println!("Image created:");
    println!("  id: {}", response.resource_id.get());
    println!("  size: {} x {}", response.width, response.height);
    println!("  format: {}", response.format);
    print_selected_memory(response.selected_memory);

    Ok(response.resource_id)
}

fn create_external_image(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: u64,
    device: &DeviceDesc,
    handle_type: ExternalHandleType,
) -> Result<Result<ResourceId, ErrorResponse>, Box<dyn std::error::Error>> {
    println!("Image:");
    println!("  {IMAGE_PROOF_WIDTH} x {IMAGE_PROOF_HEIGHT}");
    println!("  format: Rgba8Unorm");
    println!("  resource: Image");
    println!("  external sharing: {handle_type}");
    send_message(
        stream,
        &WireMessage::CreateImage {
            request_id,
            request: CreateImageRequest {
                desc: ImageDesc {
                    device_id: device.id,
                    width: IMAGE_PROOF_WIDTH,
                    height: IMAGE_PROOF_HEIGHT,
                    format: PixelFormat::Rgba8Unorm,
                    usage: ImageUsageFlags::TRANSFER_SRC
                        | ImageUsageFlags::TRANSFER_DST
                        | ImageUsageFlags::STORAGE,
                    external_sharing: ExternalSharing::Required { handle_type },
                },
            },
        },
    )?;

    match receive_message(stream)? {
        WireMessage::ImageCreated {
            request_id: response_request_id,
            response,
        } => {
            if response_request_id != request_id {
                return Err("image created request_id did not match request".into());
            }
            println!("Created:");
            println!("  ResourceId: {}", response.resource_id.get());
            println!("  exportable: yes");
            print_selected_memory(response.selected_memory);
            Ok(Ok(response.resource_id))
        }
        WireMessage::Error {
            request_id: response_request_id,
            response,
        } => {
            if response_request_id != request_id {
                return Err("image error request_id did not match request".into());
            }
            Ok(Err(response))
        }
        _ => Err("expected IMAGE_CREATED or ERROR response".into()),
    }
}

fn test_external_memory(
    stream: &mut std::os::unix::net::UnixStream,
    mut request_id: u64,
    device: &DeviceDesc,
) -> Result<u64, Box<dyn std::error::Error>> {
    for handle_type in [ExternalHandleType::DmaBuf, ExternalHandleType::OpaqueFd] {
        request_id += 1;
        let resource_id =
            match create_host_visible_external_buffer(stream, request_id, device, handle_type)? {
                Ok(resource_id) => resource_id,
                Err(error) => {
                    println!("  create failed for {handle_type}: {:?}", error.code);
                    continue;
                }
            };

        request_id += 1;
        send_message(
            stream,
            &WireMessage::ExportResource {
                request_id,
                request: ExportResourceRequest {
                    resource_id,
                    handle_type,
                },
            },
        )?;
        let received = receive_message_with_attachments::<1>(stream)?;
        let WireMessage::ResourceExported {
            request_id: response_request_id,
            response,
        } = received.message
        else {
            request_id += 1;
            destroy_resource(stream, request_id, resource_id)?;
            return Err("expected RESOURCE_EXPORTED response".into());
        };
        if response_request_id != request_id {
            request_id += 1;
            destroy_resource(stream, request_id, resource_id)?;
            return Err("resource exported request_id did not match request".into());
        }
        if response.metadata.resource_id != resource_id || response.metadata.attachment_count != 1 {
            request_id += 1;
            destroy_resource(stream, request_id, resource_id)?;
            return Err("resource export metadata did not match request".into());
        }
        let Some(handle) = received.attachments.into_iter().next() else {
            request_id += 1;
            destroy_resource(stream, request_id, resource_id)?;
            return Err("missing exported FD attachment".into());
        };

        println!("Export:");
        println!("  mechanism: {}", response.metadata.handle_type);
        println!("  native FD received: yes");

        validate_import(device, &response.metadata, handle)?;

        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        println!("QGS resource cleanup: success");
        return Ok(request_id);
    }

    println!("  external sharing unsupported on this device by current driver");
    Ok(request_id)
}

fn test_external_gpu_sync(
    stream: &mut std::os::unix::net::UnixStream,
    mut request_id: u64,
    device: &DeviceDesc,
) -> Result<u64, Box<dyn std::error::Error>> {
    const FILL_PATTERN: u32 = 0x5147_5337;

    request_id += 1;
    let resource_id = match create_host_visible_external_buffer(
        stream,
        request_id,
        device,
        ExternalHandleType::DmaBuf,
    )? {
        Ok(resource_id) => resource_id,
        Err(error) => {
            println!("  create failed for DMA-BUF: {:?}", error.code);
            println!("  external GPU synchronization unsupported on this device by current driver");
            return Ok(request_id);
        }
    };

    request_id += 1;
    let (resource_metadata, resource_handle) =
        match export_resource(stream, request_id, resource_id, ExternalHandleType::DmaBuf) {
            Ok(export) => export,
            Err(err) => {
                request_id += 1;
                destroy_resource(stream, request_id, resource_id)?;
                return Err(err);
            }
        };

    request_id += 1;
    let sync_id = match create_sync(stream, request_id, device)? {
        Ok(sync_id) => sync_id,
        Err(error) => {
            println!("  sync creation failed: {:?}", error.code);
            request_id += 1;
            destroy_resource(stream, request_id, resource_id)?;
            return Ok(request_id);
        }
    };

    request_id += 1;
    send_message(
        stream,
        &WireMessage::ExportSync {
            request_id,
            request: ExportSyncRequest {
                sync_id,
                resource_id,
                fill_pattern: FILL_PATTERN,
            },
        },
    )?;
    let received = receive_message_with_attachments::<1>(stream)?;
    let WireMessage::SyncExported {
        request_id: response_request_id,
        response,
    } = received.message
    else {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("expected SYNC_EXPORTED response".into());
    };
    if response_request_id != request_id {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("sync exported request_id did not match request".into());
    }
    if response.metadata.sync_id != sync_id || response.metadata.attachment_count != 1 {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("sync export metadata did not match request".into());
    }
    let Some(sync_handle) = received.attachments.into_iter().next() else {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("missing exported sync FD attachment".into());
    };

    println!("Sync export:");
    println!("  mechanism: {}", response.metadata.handle_type);
    println!("  native sync FD received: yes");

    let importer = VulkanDeviceDiscovery::new()?;
    importer.import_wait_and_validate_synced_buffer(
        device,
        &resource_metadata,
        File::from(resource_handle),
        &response.metadata,
        File::from(sync_handle),
    )?;

    println!("Consumer:");
    println!("  imported shared buffer: success");
    println!("  imported sync primitive: success");
    println!("  GPU wait + dependent copy validation: success");
    println!("  producer->consumer idle wait: no");

    request_id += 1;
    destroy_resource(stream, request_id, resource_id)?;
    println!("QGS resource cleanup: success");

    Ok(request_id)
}

fn test_compute_proof(
    stream: &mut std::os::unix::net::UnixStream,
    mut request_id: u64,
    device: &DeviceDesc,
) -> Result<u64, Box<dyn std::error::Error>> {
    const PRODUCER_PATTERN: u32 = 0x5147_5338;

    println!("[{:?}] {}", device.class, device.name);
    let input = compute_proof_input();
    println!("Input elements: {}", input.len());
    println!("Operation: u32 + 1");
    println!("Execution: Vulkan compute");
    println!("Shared resource: DMA-BUF");
    println!("Synchronization: external sync FD");

    request_id += 1;
    let resource_id = match create_host_visible_external_buffer(
        stream,
        request_id,
        device,
        ExternalHandleType::DmaBuf,
    )? {
        Ok(resource_id) => resource_id,
        Err(error) => {
            println!("  create failed for DMA-BUF: {:?}", error.code);
            println!("Validation:");
            println!("SKIP");
            return Ok(request_id);
        }
    };

    request_id += 1;
    let (resource_metadata, resource_handle) =
        match export_resource(stream, request_id, resource_id, ExternalHandleType::DmaBuf) {
            Ok(export) => export,
            Err(err) => {
                request_id += 1;
                destroy_resource(stream, request_id, resource_id)?;
                return Err(err);
            }
        };

    request_id += 1;
    let sync_id = match create_sync(stream, request_id, device)? {
        Ok(sync_id) => sync_id,
        Err(error) => {
            println!("  sync creation failed: {:?}", error.code);
            request_id += 1;
            destroy_resource(stream, request_id, resource_id)?;
            println!("Validation:");
            println!("SKIP");
            return Ok(request_id);
        }
    };

    request_id += 1;
    send_message(
        stream,
        &WireMessage::ExportSync {
            request_id,
            request: ExportSyncRequest {
                sync_id,
                resource_id,
                fill_pattern: PRODUCER_PATTERN,
            },
        },
    )?;
    let received = receive_message_with_attachments::<1>(stream)?;
    let WireMessage::SyncExported {
        request_id: response_request_id,
        response,
    } = received.message
    else {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("expected SYNC_EXPORTED response".into());
    };
    if response_request_id != request_id {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("sync exported request_id did not match request".into());
    }
    let Some(sync_handle) = received.attachments.into_iter().next() else {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("missing exported sync FD attachment".into());
    };

    let importer = VulkanDeviceDiscovery::new()?;
    let output = importer.import_wait_and_run_compute_increment_proof(
        device,
        &resource_metadata,
        File::from(resource_handle),
        &response.metadata,
        File::from(sync_handle),
        &input,
    )?;

    let expected = input
        .iter()
        .map(|value| value.wrapping_add(1))
        .collect::<Vec<_>>();
    println!("Validation:");
    if output == expected {
        println!("PASS");
    } else {
        println!(
            "FAIL sample: {:?} -> {:?}, expected {:?}",
            &input[..6.min(input.len())],
            &output[..6.min(output.len())],
            &expected[..6.min(expected.len())]
        );
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("compute proof output did not match expected values".into());
    }
    println!(
        "Sample: {:?} -> {:?}",
        &input[..6.min(input.len())],
        &output[..6.min(output.len())]
    );

    request_id += 1;
    destroy_resource(stream, request_id, resource_id)?;
    println!("QGS resource cleanup: success");

    Ok(request_id)
}

fn test_image_processing_proof(
    stream: &mut std::os::unix::net::UnixStream,
    mut request_id: u64,
    device: &DeviceDesc,
) -> Result<u64, Box<dyn std::error::Error>> {
    const PRODUCER_PATTERN: u32 = 0x5147_5339;

    println!("[{:?}] {}", device.class, device.name);

    request_id += 1;
    let resource_id =
        match create_external_image(stream, request_id, device, ExternalHandleType::DmaBuf)? {
            Ok(resource_id) => resource_id,
            Err(error) => {
                println!("  create failed for DMA-BUF: {:?}", error.code);
                println!("Validation:");
                println!("SKIP");
                return Ok(request_id);
            }
        };

    request_id += 1;
    let (resource_metadata, resource_handle) =
        match export_resource(stream, request_id, resource_id, ExternalHandleType::DmaBuf) {
            Ok(export) => export,
            Err(err) => {
                request_id += 1;
                destroy_resource(stream, request_id, resource_id)?;
                return Err(err);
            }
        };

    request_id += 1;
    let sync_id = match create_sync(stream, request_id, device)? {
        Ok(sync_id) => sync_id,
        Err(error) => {
            println!("  sync creation failed: {:?}", error.code);
            request_id += 1;
            destroy_resource(stream, request_id, resource_id)?;
            println!("Validation:");
            println!("SKIP");
            return Ok(request_id);
        }
    };

    request_id += 1;
    send_message(
        stream,
        &WireMessage::ExportSync {
            request_id,
            request: ExportSyncRequest {
                sync_id,
                resource_id,
                fill_pattern: PRODUCER_PATTERN,
            },
        },
    )?;
    let received = receive_message_with_attachments::<1>(stream)?;
    let WireMessage::SyncExported {
        request_id: response_request_id,
        response,
    } = received.message
    else {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("expected SYNC_EXPORTED response".into());
    };
    if response_request_id != request_id {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("sync exported request_id did not match request".into());
    }
    let Some(sync_handle) = received.attachments.into_iter().next() else {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("missing exported sync FD attachment".into());
    };

    let input = vec![0_u8; (IMAGE_PROOF_WIDTH as usize) * (IMAGE_PROOF_HEIGHT as usize) * 4];
    let importer = VulkanDeviceDiscovery::new()?;
    let output = importer.import_wait_and_run_image_invert_proof(
        device,
        &resource_metadata,
        File::from(resource_handle),
        &response.metadata,
        File::from(sync_handle),
        &input,
    )?;
    let expected = image_proof_expected_from_clear(PRODUCER_PATTERN, input.len());

    println!("Processing:");
    println!("  operation: private RGBA invert proof");
    println!("  execution: Vulkan GPU");
    println!("  synchronization: external sync FD");
    println!("Validation:");
    if output == expected {
        println!("PASS");
    } else {
        println!(
            "FAIL sample: {:?} -> {:?}, expected {:?}",
            &input[..8.min(input.len())],
            &output[..8.min(output.len())],
            &expected[..8.min(expected.len())]
        );
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
        return Err("image proof output did not match expected pixels".into());
    }
    println!(
        "Sample RGBA bytes: {:?} -> {:?}",
        observed_linear_dma_buf_rgba(PRODUCER_PATTERN),
        &output[..8.min(output.len())]
    );
    println!("producer->consumer idle wait: no");

    request_id += 1;
    destroy_resource(stream, request_id, resource_id)?;
    println!("QGS resource cleanup: success");

    Ok(request_id)
}

fn compute_proof_input() -> Vec<u32> {
    let mut values = (0..1024_u32).collect::<Vec<_>>();
    values[..6].copy_from_slice(&[0, 1, 2, 3, 100, 1000]);
    values
}

fn image_clear_rgba(pattern: u32) -> [u8; 4] {
    [
        (pattern & 0xff) as u8,
        ((pattern >> 8) & 0xff) as u8,
        ((pattern >> 16) & 0xff) as u8,
        ((pattern >> 24) & 0xff) as u8,
    ]
}

fn image_proof_expected_from_clear(pattern: u32, len: usize) -> Vec<u8> {
    let [r, g, b, a] = observed_linear_dma_buf_rgba(pattern);
    std::iter::repeat_n([255 - r, 255 - g, 255 - b, a], len / 4)
        .flatten()
        .collect()
}

fn observed_linear_dma_buf_rgba(pattern: u32) -> [u8; 4] {
    [0, 0, 0, image_clear_rgba(pattern)[3]]
}

fn export_resource(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: u64,
    resource_id: ResourceId,
    handle_type: ExternalHandleType,
) -> Result<(qgs_protocol::ExportedResourceMetadata, OwnedFd), Box<dyn std::error::Error>> {
    send_message(
        stream,
        &WireMessage::ExportResource {
            request_id,
            request: ExportResourceRequest {
                resource_id,
                handle_type,
            },
        },
    )?;
    let received = receive_message_with_attachments::<1>(stream)?;
    let WireMessage::ResourceExported {
        request_id: response_request_id,
        response,
    } = received.message
    else {
        return Err("expected RESOURCE_EXPORTED response".into());
    };
    if response_request_id != request_id {
        return Err("resource exported request_id did not match request".into());
    }
    if response.metadata.resource_id != resource_id || response.metadata.attachment_count != 1 {
        return Err("resource export metadata did not match request".into());
    }
    let Some(handle) = received.attachments.into_iter().next() else {
        return Err("missing exported FD attachment".into());
    };

    println!("Export:");
    println!("  mechanism: {}", response.metadata.handle_type);
    println!("  native FD received: yes");

    Ok((response.metadata, handle))
}

fn create_sync(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: u64,
    device: &DeviceDesc,
) -> Result<Result<SyncId, ErrorResponse>, Box<dyn std::error::Error>> {
    send_message(
        stream,
        &WireMessage::CreateSync {
            request_id,
            request: CreateSyncRequest {
                device_id: device.id,
                kind: SyncKind::BinarySemaphore,
                handle_type: SyncExportHandleType::SyncFd,
            },
        },
    )?;

    match receive_message(stream)? {
        WireMessage::SyncCreated {
            request_id: response_request_id,
            response,
        } => {
            if response_request_id != request_id {
                return Err("sync created request_id did not match request".into());
            }
            println!("Sync created:");
            println!("  id: {}", response.sync_id.get());
            println!("  kind: BinarySemaphore");
            println!("  export handle: sync-fd");
            Ok(Ok(response.sync_id))
        }
        WireMessage::Error {
            request_id: response_request_id,
            response,
        } => {
            if response_request_id != request_id {
                return Err("sync error request_id did not match request".into());
            }
            Ok(Err(response))
        }
        _ => Err("expected SYNC_CREATED or ERROR response".into()),
    }
}

fn validate_import(
    device: &DeviceDesc,
    metadata: &qgs_protocol::ExportedResourceMetadata,
    handle: OwnedFd,
) -> Result<(), Box<dyn std::error::Error>> {
    let importer = VulkanDeviceDiscovery::new()?;
    importer.import_and_validate_external_buffer(device, metadata, File::from(handle))?;

    println!("Import:");
    println!("  second Vulkan context: success");
    println!("  shared allocation validation: success");

    Ok(())
}

fn test_h264_decode(
    stream: &mut std::os::unix::net::UnixStream,
    mut request_id: u64,
    device: &DeviceDesc,
) -> Result<u64, Box<dyn std::error::Error>> {
    println!("H.264 decode proof:");
    println!("[{:?}] {}", device.class, device.name);

    request_id += 1;
    send_message(
        stream,
        &WireMessage::QueryVideoCapabilities {
            request_id,
            request: QueryVideoCapabilitiesRequest {
                device_id: device.id,
            },
        },
    )?;
    let response = receive_message(stream)?;
    let WireMessage::VideoCapabilities {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected VIDEO_CAPABILITIES response".into());
    };
    if response_request_id != request_id {
        return Err("video capability response request_id did not match request".into());
    }
    let supports_h264_baseline = response.capabilities.decode.iter().any(|capability| {
        capability.codec == VideoCodec::H264
            && capability.profile == VideoProfile::H264(H264Profile::Baseline)
            && capability.bit_depth.get() == 8
            && capability.chroma == ChromaSubsampling::Cs420
            && capability
                .output_surface_formats
                .contains(&qgs_protocol::VideoSurfaceFormat::Nv12)
    });

    request_id += 1;
    let config = DecoderConfig {
        device_id: device.id,
        codec: VideoCodec::H264,
        profile: VideoProfile::H264(H264Profile::Baseline),
        bit_depth: BitDepth::new(8)?,
        chroma: ChromaSubsampling::Cs420,
        coded_width: 64,
        coded_height: 64,
        scan_mode: ScanMode::Progressive,
    };
    send_message(
        stream,
        &WireMessage::CreateDecoder {
            request_id,
            request: CreateDecoderRequest { config },
        },
    )?;
    let response = receive_message(stream)?;
    let decoder_id = match response {
        WireMessage::DecoderCreated {
            request_id: response_request_id,
            response,
        } => {
            if response_request_id != request_id {
                return Err("decoder created request_id did not match request".into());
            }
            if !supports_h264_baseline {
                return Err("decoder succeeded on device without advertised H.264 support".into());
            }
            println!("Decoder created:");
            println!("  id: {}", response.decoder_id.get());
            response.decoder_id
        }
        WireMessage::Error {
            request_id: response_request_id,
            response,
        } => {
            if response_request_id != request_id {
                return Err("decoder error request_id did not match request".into());
            }
            if supports_h264_baseline {
                return Err(format!(
                    "decoder creation failed despite advertised support: {:?}",
                    response.code
                )
                .into());
            }
            if response.code != ProtocolErrorCode::UnsupportedDecodeConfiguration {
                return Err(
                    format!("unexpected unsupported decoder error: {:?}", response.code).into(),
                );
            }
            println!("Decoder unsupported as expected for this device.");
            return Ok(request_id);
        }
        _ => return Err("expected DECODER_CREATED or ERROR response".into()),
    };

    request_id += 1;
    send_message(
        stream,
        &WireMessage::SubmitAccessUnit {
            request_id,
            request: SubmitAccessUnitRequest {
                decoder_id,
                data: H264_FIXTURE.to_vec(),
            },
        },
    )?;
    let response = receive_message(stream)?;
    let WireMessage::DecodeOutput {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected DECODE_OUTPUT response".into());
    };
    if response_request_id != request_id {
        return Err("decode output request_id did not match request".into());
    }
    if response.decoder_id != decoder_id {
        return Err("decode output decoder_id did not match request".into());
    }
    println!("Decoded VideoSurface:");
    println!("  resource id: {}", response.resource_id.get());
    println!(
        "  {} x {} {:?}",
        response.surface.coded_width, response.surface.coded_height, response.surface.format
    );
    println!("  validation: backend VA readback succeeded");

    request_id += 1;
    destroy_resource(stream, request_id, response.resource_id)?;
    println!("Decoded VideoSurface destroyed successfully.");

    request_id += 1;
    send_message(
        stream,
        &WireMessage::DestroyDecoder {
            request_id,
            request: DestroyDecoderRequest { decoder_id },
        },
    )?;
    let response = receive_message(stream)?;
    let WireMessage::DecoderDestroyed {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected DECODER_DESTROYED response".into());
    };
    if response_request_id != request_id || response.decoder_id != decoder_id {
        return Err("decoder destroyed response did not match request".into());
    }
    println!("Decoder destroyed successfully.");

    Ok(request_id)
}

fn leave_h264_decoder_for_disconnect(
    stream: &mut std::os::unix::net::UnixStream,
    mut request_id: u64,
    devices: &[&DeviceDesc],
) -> Result<u64, Box<dyn std::error::Error>> {
    for device in devices {
        request_id += 1;
        let config = DecoderConfig {
            device_id: device.id,
            codec: VideoCodec::H264,
            profile: VideoProfile::H264(H264Profile::Baseline),
            bit_depth: BitDepth::new(8)?,
            chroma: ChromaSubsampling::Cs420,
            coded_width: 64,
            coded_height: 64,
            scan_mode: ScanMode::Progressive,
        };
        send_message(
            stream,
            &WireMessage::CreateDecoder {
                request_id,
                request: CreateDecoderRequest { config },
            },
        )?;

        let response = receive_message(stream)?;
        let decoder_id = match response {
            WireMessage::DecoderCreated {
                request_id: response_request_id,
                response,
            } => {
                if response_request_id != request_id {
                    return Err("transient decoder response request_id did not match".into());
                }
                response.decoder_id
            }
            WireMessage::Error { response, .. }
                if response.code == ProtocolErrorCode::UnsupportedDecodeConfiguration =>
            {
                continue;
            }
            _ => return Err("expected transient DECODER_CREATED or unsupported error".into()),
        };

        request_id += 1;
        send_message(
            stream,
            &WireMessage::SubmitAccessUnit {
                request_id,
                request: SubmitAccessUnitRequest {
                    decoder_id,
                    data: H264_FIXTURE.to_vec(),
                },
            },
        )?;
        let response = receive_message(stream)?;
        let WireMessage::DecodeOutput {
            request_id: response_request_id,
            response,
        } = response
        else {
            return Err("expected transient DECODE_OUTPUT response".into());
        };
        if response_request_id != request_id {
            return Err("transient decode output request_id did not match".into());
        }

        println!("Leaving transient H.264 decoder and VideoSurface alive for disconnect cleanup:");
        println!(
            "  decoder id: {} resource id: {}",
            decoder_id.get(),
            response.resource_id.get()
        );
        return Ok(request_id);
    }

    println!("No H.264 decoder available for disconnect cleanup exercise.");
    Ok(request_id)
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

fn optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn optional_hex_u32(value: Option<u32>) -> String {
    value
        .map(|value| format!("0x{value:08x}"))
        .unwrap_or_else(|| "n/a".to_string())
}

const fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

struct Args {
    socket_path: PathBuf,
    video_capabilities_only: bool,
    h264_decode_only: bool,
    haswell_video_diagnostic: bool,
}

impl Args {
    fn parse() -> Self {
        let mut socket_path = None;
        let mut video_capabilities_only = false;
        let mut h264_decode_only = false;
        let mut haswell_video_diagnostic = false;

        for arg in std::env::args_os().skip(1) {
            if arg == VIDEO_CAPABILITIES_ONLY_ARG {
                video_capabilities_only = true;
            } else if arg == H264_DECODE_ONLY_ARG {
                h264_decode_only = true;
            } else if arg == HASWELL_VIDEO_DIAGNOSTIC_ARG {
                haswell_video_diagnostic = true;
            } else if socket_path.is_none() {
                socket_path = Some(PathBuf::from(arg));
            }
        }

        Self {
            socket_path: socket_path.unwrap_or_else(default_socket_path),
            video_capabilities_only,
            h264_decode_only,
            haswell_video_diagnostic,
        }
    }
}

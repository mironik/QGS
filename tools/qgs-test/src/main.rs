#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::time::Instant;

use qgs_core::DeviceDiscovery;
use qgs_linux::{
    connect_socket, default_socket_path, receive_message, receive_message_with_attachments,
    send_message,
};
use qgs_mp4::{
    classify_video_track, nearest_random_access_before, MediaHealth, Mp4Source, Mp4TrackKind,
};
use qgs_mxf::{IndexSource, MediaSource, RandomAccess, TrackKind};
use qgs_protocol::{
    BitDepth, BufferDesc, BufferUsageFlags, ChromaSubsampling, CreateBufferRequest,
    CreateDecoderRequest, CreateImageRequest, CreateSyncRequest, DecoderConfig,
    DestroyDecoderRequest, DestroyResourceRequest, DeviceCapabilities, DeviceClass, DeviceDesc,
    ErrorResponse, ExportResourceRequest, ExportSyncRequest, ExternalHandleType, ExternalSharing,
    FlushDecoderRequest, H264Profile, HelloRequest, ImageDesc, ImageUsageFlags, MemoryPreference,
    PixelFormat, ProtocolErrorCode, QueryDeviceCapabilitiesRequest, QueryVideoCapabilitiesRequest,
    ResourceId, ScanMode, SelectedMemoryProperties, SubmitAccessUnitRequest, SyncExportHandleType,
    SyncId, SyncKind, VideoCapabilities, VideoCodec, VideoProfile, WireMessage,
    CURRENT_PROTOCOL_VERSION,
};
use qgs_software_video::{
    decoder_config_for_surface, DecodeRunStats, SoftwareH264Decoder, SoftwarePixelFormat,
    SoftwareVideoBackend,
};
use qgs_vulkan::{
    diagnose_haswell_video_import, yuv422p10_reference_rgba_u16, yuv422p10_rgba_u16_checksum,
    DiagnosticDrmLayer, DiagnosticDrmObject, DiagnosticDrmPlane, FrameIdentity,
    FrameProcessorError, GpuFrameProcessor, GpuFrameProcessorConfig, HaswellVideoDiagnosticInput,
    VulkanDeviceDiscovery, YcbcrConversion, Yuv422P10Plane, Yuv422P10Upload,
};
use sha2::{Digest, Sha256};

const DEMO_BUFFER_SIZE: u64 = 1024 * 1024;
const IMAGE_PROOF_WIDTH: u32 = 64;
const IMAGE_PROOF_HEIGHT: u32 = 64;
const VIDEO_CAPABILITIES_ONLY_ARG: &str = "--video-capabilities-only";
const H264_DECODE_ONLY_ARG: &str = "--h264-decode-only";
const HASWELL_VIDEO_DIAGNOSTIC_ARG: &str = "--haswell-video-diagnostic";
const MXF_INSPECT_ARG: &str = "--mxf-inspect";
const SOFTWARE_DECODE_MXF_ARG: &str = "--software-decode-mxf";
const SOFTWARE_GPU_MXF_ARG: &str = "--software-gpu-mxf";
const PROXY_PROOF_ARG: &str = "--proxy-proof";
const EXPECTED_FX6_SAMPLE001_MXF_SHA256: &str =
    "6bb8d23f91be8812f0bf9c09b6ee680dce0560757b9d778333d3e996b5f69653";
const EXPECTED_FX6_SAMPLE001_PROXY_SHA256: &str =
    "9d0c64bed89303e6b8e1e32ef04984b62a174508528de42be87f82a985f039c8";
const EXPECTED_FX6_SAMPLE002_MXF_SHA256: &str =
    "52a82d3527717096892a78bfd62f62f864e891fc4321e09ad7ed0856a7b9f24e";
const EXPECTED_FX6_SAMPLE002_PROXY_SHA256: &str =
    "fa7b646f7dc84bee84744dbaee89924b7f94982405c2fc3ffea2936618f73007";
const H264_FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/h264/idr-64x64-baseline.h264");
const H264_LONG_GOP_FIXTURE: &[u8] =
    include_bytes!("../../../tests/fixtures/h264/long-gop-128x72-main.h264");
const DEFAULT_MXF_FIXTURE: &str = "tests/fixtures/mxf/h264-8bit-420-long-gop-128x72.mxf";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.haswell_video_diagnostic {
        return run_haswell_video_diagnostic();
    }
    if let Some(path) = args.mxf_inspect_path {
        return inspect_mxf(&path);
    }
    if let Some(path) = args.software_decode_mxf_path {
        return software_decode_mxf(&path);
    }
    if let Some(path) = args.software_gpu_mxf_path {
        return software_gpu_mxf(&path);
    }
    if let Some((original, proxy)) = args.proxy_proof_paths {
        return proxy_proof(&args.socket_path, &original, &proxy);
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
            request_id = test_h264_long_gop_decode(&mut stream, request_id, device)?;
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

    println!("Software YUV422P10 GPU upload proof:");
    for device in &physical_devices {
        test_synthetic_yuv422p10_gpu_proof(device)?;
        test_reusable_yuv422p10_gpu_processor(device)?;
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

fn inspect_mxf(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let source = MediaSource::parse(&bytes)?;

    println!("MXF:");
    println!("  file: {}", mxf_file_label(path));
    if let Some(duration) = source.duration {
        println!("  duration edit units: {duration}");
    }
    if let Some(rate) = source.edit_rate {
        println!("  edit rate: {}/{}", rate.numerator, rate.denominator);
    }
    println!("  KLV triplets: {}", source.klv_count);
    println!("  metadata sets: {}", source.metadata_set_count);
    if let Some(pattern) = source.operational_pattern {
        println!("  operational pattern: {pattern}");
    }
    println!("Partitions:");
    for partition in &source.partitions {
        println!(
            "  {:?}: offset={} body_sid={} index_sid={} header_bytes={} index_bytes={}",
            partition.kind,
            partition.offset,
            partition.body_sid,
            partition.index_sid,
            partition.header_byte_count,
            partition.index_byte_count
        );
    }
    println!("Packages:");
    for package in &source.packages {
        println!(
            "  {:?}: uid=<redacted> tracks={}",
            package.kind,
            package.track_refs.len()
        );
    }
    if let Some(timecode) = &source.timecode {
        println!("Timecode:");
        println!("  start frame: {}", timecode.start_frame);
        println!(
            "  rate: {}/{}",
            timecode.edit_rate.numerator, timecode.edit_rate.denominator
        );
        println!("  drop-frame: {}", yes_no(timecode.drop_frame));
    }

    println!("Tracks:");
    for track in &source.tracks {
        println!("  {:?} track {}", track.kind, track.id.0);
        if let Some(rate) = track.edit_rate {
            println!("    edit rate: {}/{}", rate.numerator, rate.denominator);
        }
        match track.kind {
            TrackKind::Video => {
                if let Some(video) = &track.video {
                    println!("    codec: {:?}", video.codec);
                    println!("    descriptor source: {:?}", video.source);
                    println!(
                        "    dimensions: {} x {}",
                        video.coded_width, video.coded_height
                    );
                    println!(
                        "    display: {} x {}",
                        video.display_width, video.display_height
                    );
                    println!("    bit depth: {}", video.bit_depth);
                    println!("    chroma: {:?}", video.chroma);
                    if let Some(aspect) = video.aspect_ratio {
                        println!(
                            "    aspect ratio: {}/{}",
                            aspect.numerator, aspect.denominator
                        );
                    }
                }
            }
            TrackKind::Audio => {
                if let Some(audio) = &track.audio {
                    println!("    descriptor source: {:?}", audio.source);
                    println!("    channels: {:?}", audio.channels);
                    if let Some(rate) = audio.sample_rate {
                        println!("    sample rate: {}/{}", rate.numerator, rate.denominator);
                    }
                    println!("    bit depth: {:?}", audio.bit_depth);
                }
            }
            TrackKind::Data => {
                if let Some(data) = &track.data {
                    println!("    descriptor source: {:?}", data.source);
                    if let Some(rate) = data.sample_rate {
                        println!("    sample rate: {}/{}", rate.numerator, rate.denominator);
                    }
                    if let Some(essence) = data.essence {
                        println!("    essence: {essence}");
                    }
                }
            }
            TrackKind::Timecode | TrackKind::Other => {}
        }
    }

    let random_access_points = source
        .index
        .video
        .iter()
        .filter(|entry| entry.random_access == RandomAccess::Yes)
        .count();
    let random_access_positions = source
        .index
        .video
        .iter()
        .filter(|entry| entry.random_access == RandomAccess::Yes)
        .map(|entry| entry.edit_unit)
        .collect::<Vec<_>>();
    println!("Index:");
    let index_source = if source
        .index
        .video
        .iter()
        .any(|entry| entry.source == IndexSource::MxfProvided)
    {
        "MXF provided"
    } else {
        "QGS derived"
    };
    println!("  source: {index_source}");
    println!("  video entries: {}", source.index.video.len());
    println!("  random access points: {random_access_points}");
    println!("  random access edit units: {:?}", random_access_positions);
    println!("  index segments: {}", source.index_segments.len());
    for segment in &source.index_segments {
        println!(
            "  segment: body_sid={} index_sid={} entries={} delta_entries={}",
            segment.body_sid,
            segment.index_sid,
            segment.entries.len(),
            segment.delta_entries.len()
        );
    }
    println!("  RIP entries: {}", source.rip.len());

    if !source.diagnostics.is_empty() {
        println!("Warnings:");
        for diagnostic in &source.diagnostics {
            println!("  {:?}: {}", diagnostic.kind, diagnostic.message);
        }
    }

    let target = source.index.video.len().saturating_div(2);
    let start = source
        .index
        .nearest_random_access_before(target as u64)
        .map(|entry| entry.edit_unit as usize)
        .unwrap_or(target);
    println!("Random access proof:");
    println!("  target edit unit: {target}");
    println!("  nearest prior random access: {start}");

    let mut state = qgs_codec_h264::H264DecoderState::new();
    let mut parsed_target = None;
    for index in start..=target {
        let access_unit = source.extract_video_access_unit(&bytes, index)?;
        let parsed = state.parse_access_unit(&access_unit)?;
        state.finish_picture(&parsed)?;
        if index == target {
            parsed_target = Some(parsed);
        }
    }
    let parsed = parsed_target.ok_or("no target access unit parsed")?;
    println!("H.264 classification:");
    println!("  profile: {:?}", parsed.profile);
    println!(
        "  dimensions: {} x {}",
        parsed.desc.coded_width, parsed.desc.coded_height
    );
    println!("  bit depth: {}", parsed.desc.bit_depth.get());
    println!("  chroma: {:?}", parsed.desc.chroma);
    println!(
        "  picture kind: {:?}",
        parsed.slices.first().map(|slice| &slice.kind)
    );
    print_h264_gop_summary(&source, &bytes)?;
    if let Some(sidecar) = SonyXmlSummary::from_mxf_path(path)? {
        print_sony_xml_comparison(&source, &parsed, &sidecar);
    }

    Ok(())
}

fn software_decode_mxf(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let source = MediaSource::parse(&bytes)?;
    let first_access_unit = source.extract_video_access_unit(&bytes, 0)?;
    let parsed = qgs_codec_h264::parse_annex_b_access_unit(&first_access_unit)?;
    let config = decoder_config_for_surface(
        qgs_protocol::DeviceId::new(1)?,
        parsed.profile,
        parsed.desc.bit_depth,
        parsed.desc.chroma,
        parsed.desc.coded_width,
        parsed.desc.coded_height,
    );
    if !SoftwareVideoBackend::supports_config(&config) {
        return Err("software backend does not support parsed stream".into());
    }

    let positioned = source
        .index
        .video
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            Ok((
                entry.edit_unit,
                source.extract_video_access_unit(&bytes, index)?,
            ))
        })
        .collect::<Result<Vec<_>, qgs_mxf::MxfError>>()?;
    let sequential = decode_positioned_access_units_with_context(config.clone(), &positioned)?;

    let target = 53_u64.min(source.index.video.len().saturating_sub(1) as u64);
    let start = source
        .index
        .nearest_random_access_before(target)
        .map(|entry| entry.edit_unit)
        .unwrap_or(target);
    let random_positioned = (start..=target)
        .map(|edit_unit| {
            let index = usize::try_from(edit_unit)?;
            Ok((edit_unit, source.extract_video_access_unit(&bytes, index)?))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let random = decode_positioned_access_units_with_context(config, &random_positioned)?;

    let sequential_target = sequential
        .frames
        .iter()
        .find(|frame| frame.presentation_index == target)
        .ok_or("sequential target frame missing")?;
    let random_target = random
        .frames
        .iter()
        .find(|frame| frame.presentation_index == target)
        .ok_or("random-access target frame missing")?;
    if sequential_target.checksum != random_target.checksum {
        return Err("random-access target checksum did not match sequential decode".into());
    }

    let first = sequential.frames.first().ok_or("no decoded frames")?;
    let middle = sequential_target;
    let final_frame = sequential.frames.last().ok_or("no final decoded frame")?;
    let plane_count = first.planes.len();
    let owned_bytes = first.owned_bytes();
    let peak_owned = owned_bytes
        .checked_mul(sequential.max_live_surfaces.max(1))
        .ok_or("software surface memory overflow")?;
    let fps = if sequential.elapsed.as_secs_f64() > 0.0 {
        sequential.frames.len() as f64 / sequential.elapsed.as_secs_f64()
    } else {
        0.0
    };

    println!("Software H.264 decode proof:");
    println!("  file: {}", mxf_file_label(path));
    println!("  backend: rsmpeg/libavcodec");
    println!("  parsed profile: {:?}", parsed.profile);
    println!(
        "  parsed format: {} x {}, {}-bit {:?}",
        parsed.desc.coded_width,
        parsed.desc.coded_height,
        parsed.desc.bit_depth.get(),
        parsed.desc.chroma
    );
    println!("  software fallback selected: yes");
    println!("  decoded frames: {}", sequential.frames.len());
    println!("  decoder pixel format: {}", first.decoder_pixel_format);
    println!("  storage format: {:?}", first.storage_format);
    println!("  plane count: {plane_count}");
    for (index, plane) in first.planes.iter().enumerate() {
        println!(
            "  plane {index}: width_samples={} height={} stride={} source_stride={} bytes={}",
            plane.width_samples,
            plane.height,
            plane.stride_bytes,
            plane.source_stride_bytes,
            plane.data.len()
        );
    }
    println!("  first checksum: 0x{:016x}", first.checksum);
    println!(
        "  sequential target {} checksum: 0x{:016x}",
        target, middle.checksum
    );
    println!(
        "  random target {} checksum: 0x{:016x}",
        target, random_target.checksum
    );
    println!("  final checksum: 0x{:016x}", final_frame.checksum);
    println!("  random-access target: {target}");
    println!("  nearest prior random access: {start}");
    println!("  random-access checksum match: yes");
    println!("  bytes per software surface: {owned_bytes}");
    println!(
        "  max simultaneously live QGS software surfaces: {}",
        sequential.max_live_surfaces
    );
    println!("  approximate peak QGS-owned frame memory: {peak_owned} bytes");
    println!(
        "  DEVELOPMENT OBSERVATION - NOT A BENCHMARK: {:.3}s, {:.2} fps",
        sequential.elapsed.as_secs_f64(),
        fps
    );

    Ok(())
}

fn software_gpu_mxf(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let source = MediaSource::parse(&bytes)?;
    let first_access_unit = source.extract_video_access_unit(&bytes, 0)?;
    let parsed = qgs_codec_h264::parse_annex_b_access_unit(&first_access_unit)?;
    let config = decoder_config_for_surface(
        qgs_protocol::DeviceId::new(1)?,
        parsed.profile,
        parsed.desc.bit_depth,
        parsed.desc.chroma,
        parsed.desc.coded_width,
        parsed.desc.coded_height,
    );
    if !SoftwareVideoBackend::supports_config(&config) {
        return Err("software backend does not support parsed stream".into());
    }

    let positioned = source
        .index
        .video
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            Ok((
                entry.edit_unit,
                source.extract_video_access_unit(&bytes, index)?,
            ))
        })
        .collect::<Result<Vec<_>, qgs_mxf::MxfError>>()?;
    let sequential_start = Instant::now();
    let sequential = decode_positioned_access_units_with_context(config.clone(), &positioned)?;
    let sequential_decode_elapsed = sequential_start.elapsed();

    let target = 53_u64.min(source.index.video.len().saturating_sub(1) as u64);
    let start = source
        .index
        .nearest_random_access_before(target)
        .map(|entry| entry.edit_unit)
        .unwrap_or(target);
    let random_positioned = (start..=target)
        .map(|edit_unit| {
            let index = usize::try_from(edit_unit)?;
            Ok((edit_unit, source.extract_video_access_unit(&bytes, index)?))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let random = decode_positioned_access_units_with_context(config, &random_positioned)?;

    let first = sequential.frames.first().ok_or("no decoded frames")?;
    let middle = sequential
        .frames
        .iter()
        .find(|frame| frame.presentation_index == target)
        .ok_or("sequential target frame missing")?;
    let random_target = random
        .frames
        .iter()
        .find(|frame| frame.presentation_index == target)
        .ok_or("random-access target frame missing")?;
    let final_frame = sequential.frames.last().ok_or("no final decoded frame")?;

    let discovery = VulkanDeviceDiscovery::new()?;
    let devices = discovery.enumerate_devices()?;
    let physical_devices = devices
        .iter()
        .filter(|device| {
            matches!(
                device.class,
                DeviceClass::IntegratedGpu | DeviceClass::DiscreteGpu
            )
        })
        .collect::<Vec<_>>();
    if physical_devices.is_empty() {
        return Err("no physical Vulkan devices for software GPU proof".into());
    }

    println!("Software YUV422P10 -> GPU processing proof:");
    println!("  file: {}", mxf_file_label(path));
    println!("  parsed profile: {:?}", parsed.profile);
    println!(
        "  parsed format: {} x {}, {}-bit {:?}",
        parsed.desc.coded_width,
        parsed.desc.coded_height,
        parsed.desc.bit_depth.get(),
        parsed.desc.chroma
    );
    println!(
        "  software decoder pixel format: {}",
        first.decoder_pixel_format
    );
    println!("  sequential decoded frames: {}", sequential.frames.len());
    println!("  random-access target: {target}");
    println!("  nearest prior random access: {start}");
    println!(
        "  software decode observation: {:.3}s for {} frames",
        sequential_decode_elapsed.as_secs_f64(),
        sequential.frames.len()
    );

    for device in physical_devices {
        println!();
        println!("[{:?}] {}", device.class, device.name);
        let first_six = sequential
            .frames
            .iter()
            .take(6)
            .enumerate()
            .map(|(index, frame)| (format!("seq-{index}"), frame))
            .collect::<Vec<_>>();
        run_software_gpu_sequence_proof(
            &discovery,
            device,
            "first-six",
            &first_six
                .iter()
                .map(|(label, frame)| (label.as_str(), *frame))
                .collect::<Vec<_>>(),
        )?;
        let middle_outputs = run_software_gpu_sequence_proof(
            &discovery,
            device,
            "frame-53-sequential-random",
            &[("sequential-53", middle), ("random-53", random_target)],
        )?;
        let sequential_middle = *middle_outputs
            .get("sequential-53")
            .ok_or("missing sequential frame 53 GPU output")?;
        let random_middle = *middle_outputs
            .get("random-53")
            .ok_or("missing random frame 53 GPU output")?;
        if sequential_middle != random_middle {
            return Err("sequential/random frame 53 GPU checksum mismatch".into());
        }
        run_software_gpu_sequence_proof(&discovery, device, "final", &[("final", final_frame)])?;
        println!("  sequential/random frame 53 GPU match: yes");
    }

    Ok(())
}

fn proxy_proof(
    socket_path: &Path,
    original_path: &Path,
    proxy_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_sha256 = sha256_hex(original_path)?;
    let proxy_sha256 = sha256_hex(proxy_path)?;
    let case = identify_camera_case(&original_sha256, &proxy_sha256)?;
    if case.damaged_proxy {
        return diagnose_damaged_proxy_case(case, proxy_path);
    }

    let original_bytes = std::fs::read(original_path)?;
    let original = MediaSource::parse(&original_bytes)?;
    let proxy = Mp4Source::open(proxy_path)?;
    let proxy_video = proxy
        .video
        .as_ref()
        .ok_or("proxy has no H.264 video track")?;
    let proxy_h264 = classify_video_track(proxy_video)?;
    let presentation_ordinals = proxy_presentation_ordinals(proxy_video)?;

    println!("Camera original/proxy proof:");
    println!("  original: {}", case.original_label);
    println!("  proxy: {}", case.proxy_label);
    println!("  hashes: verified");
    println!("  association: filename ignored for proof metadata");
    println!("Proxy MP4:");
    println!("  major brand: {}", proxy.major_brand);
    println!("  compatible brands: {:?}", proxy.compatible_brands);
    println!("  movie timescale: {}", proxy.movie_timescale);
    println!("Tracks:");
    for track in &proxy.tracks {
        println!(
            "  track {}: kind={:?} handler={} codec={} timescale={} duration={} samples={}",
            track.track_id,
            track.kind,
            track.handler,
            track.codec,
            track.timescale,
            track.duration_units,
            track.sample_count
        );
        if track.kind == Mp4TrackKind::Video {
            println!("    dimensions: {:?} x {:?}", track.width, track.height);
        }
        if track.kind == Mp4TrackKind::Audio {
            println!(
                "    sample_rate={:?} channels={:?}",
                track.sample_rate, track.channels
            );
        }
    }
    println!("Proxy video:");
    println!(
        "  H.264 {:?}, {}-bit {:?}, {} x {}",
        proxy_h264.profile,
        proxy_h264.bit_depth,
        proxy_h264.chroma,
        proxy_h264.width,
        proxy_h264.height
    );
    println!(
        "  rate: {}/{}",
        proxy_video.frame_rate.numerator, proxy_video.frame_rate.denominator
    );
    println!("  samples: {}", proxy_video.samples.len());
    println!("  duration units: {}", proxy_video.duration_units);
    println!("  timescale: {}", proxy_video.timescale);
    println!("  nal length size: {}", proxy_video.nal_length_size);
    println!(
        "  avcC parameter sets: SPS={} PPS={}",
        proxy_video.sps_count, proxy_video.pps_count
    );
    println!("  picture counts: {:?}", proxy_h264.picture_counts);
    println!("  random access positions: {:?}", proxy_h264.idr_positions);

    let original_video_entries = original.index.video.len();
    println!("Original/proxy timing:");
    println!("  original edit units: {original_video_entries}");
    println!(
        "  proxy presentation samples: {}",
        proxy_video.samples.len()
    );
    println!("  original duration: {:?}", original.duration);
    println!(
        "  original edit rate: {:?}",
        original
            .edit_rate
            .map(|rate| format!("{}/{}", rate.numerator, rate.denominator))
    );
    println!(
        "  proxy duration/rate: {}/{} units at {}/{} fps",
        proxy_video.duration_units,
        proxy_video.timescale,
        proxy_video.frame_rate.numerator,
        proxy_video.frame_rate.denominator
    );
    if original_video_entries == proxy_video.samples.len() {
        println!("  one proxy presentation frame per original edit unit: yes");
    } else {
        println!("  one proxy presentation frame per original edit unit: no");
    }

    let sidecar = SonyXmlSummary::from_mxf_path(original_path)?;
    if let Some(sidecar) = &sidecar {
        println!("Sony XML sidecar diagnostic:");
        println!("  XML duration edit units: {:?}", sidecar.duration);
        println!(
            "  XML reports proxy/substream metadata: {}",
            yes_no(sidecar.has_proxy_metadata)
        );
        println!("  XML video codec: {:?}", sidecar.video_codec);
        println!("  XML fps: {:?}", sidecar.format_fps);
        println!("  XML layout: {:?} x {:?}", sidecar.width, sidecar.height);
    }
    let strong_metadata = sidecar
        .as_ref()
        .map(|xml| {
            xml.has_proxy_metadata
                && xml.duration == Some(proxy_video.samples.len() as u64)
                && xml.width == Some(proxy_h264.width)
                && xml.height == Some(proxy_h264.height)
        })
        .unwrap_or(false);
    println!(
        "  association confidence: {}",
        if strong_metadata {
            "strong metadata + timing evidence"
        } else {
            "supporting timing evidence only"
        }
    );

    let proxy_config = decoder_config_for_surface(
        qgs_protocol::DeviceId::new(1)?,
        proxy_h264.profile,
        BitDepth::new(proxy_h264.bit_depth)?,
        proxy_h264.chroma,
        proxy_h264.width,
        proxy_h264.height,
    );
    if !SoftwareVideoBackend::supports_config(&proxy_config) {
        return Err("software backend does not support proxy H.264 stream".into());
    }
    let proxy_positioned = proxy_video
        .samples
        .iter()
        .map(|sample| {
            Ok((
                *presentation_ordinals
                    .get(&sample.sample_index)
                    .ok_or("missing proxy presentation ordinal")?,
                sample.annex_b.clone(),
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let proxy_decode_start = Instant::now();
    let proxy_sequential =
        decode_positioned_access_units_with_context(proxy_config.clone(), &proxy_positioned)?;
    let proxy_decode_elapsed = proxy_decode_start.elapsed();
    if proxy_sequential.frames.len() != proxy_video.samples.len() {
        return Err(format!(
            "proxy software reference expected {} frames, got {}",
            proxy_video.samples.len(),
            proxy_sequential.frames.len()
        )
        .into());
    }

    let target = 53_u64.min(proxy_video.samples.len().saturating_sub(1) as u64);
    let target_decode_index = presentation_ordinals
        .iter()
        .find_map(|(sample_index, ordinal)| (*ordinal == target).then_some(*sample_index))
        .ok_or("target proxy presentation frame not found")?;
    let start_decode_index = nearest_random_access_before(proxy_video, target_decode_index)
        .ok_or("proxy random-access point not found")?;
    let proxy_random_positioned = proxy_video
        .samples
        .iter()
        .filter(|sample| {
            sample.sample_index >= start_decode_index && sample.sample_index <= target_decode_index
        })
        .map(|sample| {
            Ok((
                *presentation_ordinals
                    .get(&sample.sample_index)
                    .ok_or("missing random proxy presentation ordinal")?,
                sample.annex_b.clone(),
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let proxy_random = decode_positioned_access_units_with_context(
        proxy_config.clone(),
        &proxy_random_positioned,
    )?;
    let proxy_seq_53 = proxy_sequential
        .frames
        .iter()
        .find(|frame| frame.presentation_index == target)
        .ok_or("proxy sequential target frame missing")?;
    let proxy_rand_53 = proxy_random
        .frames
        .iter()
        .find(|frame| frame.presentation_index == target)
        .ok_or("proxy random target frame missing")?;
    if proxy_seq_53.checksum != proxy_rand_53.checksum {
        return Err("proxy sequential/random frame 53 software checksum mismatch".into());
    }

    println!("Proxy software reference decode:");
    println!("  decoded frames: {}", proxy_sequential.frames.len());
    println!(
        "  decoder pixel format: {}",
        proxy_sequential
            .frames
            .first()
            .map(|frame| frame.decoder_pixel_format.as_str())
            .unwrap_or("n/a")
    );
    println!(
        "  random access target: {} start decode sample: {}",
        target, start_decode_index
    );
    println!("  frame 53 sequential/random checksum match: yes");
    println!(
        "  DEVELOPMENT OBSERVATION - NOT A BENCHMARK: {:.3}s, {:.2} fps",
        proxy_decode_elapsed.as_secs_f64(),
        proxy_sequential.frames.len() as f64 / proxy_decode_elapsed.as_secs_f64().max(0.000_001)
    );

    let original_positioned = original
        .index
        .video
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            Ok((
                entry.edit_unit,
                original.extract_video_access_unit(&original_bytes, index)?,
            ))
        })
        .collect::<Result<Vec<_>, qgs_mxf::MxfError>>()?;
    let first_original_au = original.extract_video_access_unit(&original_bytes, 0)?;
    let original_parsed = qgs_codec_h264::parse_annex_b_access_unit(&first_original_au)?;
    let original_config = decoder_config_for_surface(
        qgs_protocol::DeviceId::new(1)?,
        original_parsed.profile,
        original_parsed.desc.bit_depth,
        original_parsed.desc.chroma,
        original_parsed.desc.coded_width,
        original_parsed.desc.coded_height,
    );
    let original_decode_start = Instant::now();
    let original_decoded =
        decode_positioned_access_units_with_context(original_config, &original_positioned)?;
    let original_decode_elapsed = original_decode_start.elapsed();
    let original_53 = original_decoded
        .frames
        .iter()
        .find(|frame| frame.presentation_index == target)
        .ok_or("original target frame missing")?;
    let correspondence = luma_signature_delta(original_53, proxy_seq_53)?;
    println!("Original/proxy frame correspondence:");
    println!("  compared frame: {target}");
    println!(
        "  original luma signature: 0x{:016x}",
        correspondence.original_hash
    );
    println!(
        "  proxy luma signature: 0x{:016x}",
        correspondence.proxy_hash
    );
    println!(
        "  mean absolute luma signature delta: {:.2}",
        correspondence.mean_abs_delta
    );
    println!(
        "  original software decode observation: {:.3}s, {:.2} fps",
        original_decode_elapsed.as_secs_f64(),
        original_decoded.frames.len() as f64 / original_decode_elapsed.as_secs_f64().max(0.000_001)
    );

    run_proxy_hardware_decode(socket_path, &proxy_config, proxy_video)?;

    let discovery = VulkanDeviceDiscovery::new()?;
    let devices = discovery.enumerate_devices()?;
    let physical_devices = devices
        .iter()
        .filter(|device| {
            matches!(
                device.class,
                DeviceClass::IntegratedGpu | DeviceClass::DiscreteGpu
            )
        })
        .collect::<Vec<_>>();
    println!("Proxy -> GPU proof:");
    for device in physical_devices {
        println!("  [{:?}] {}", device.class, device.name);
        let first = proxy_sequential
            .frames
            .first()
            .ok_or("missing proxy first frame")?;
        let final_frame = proxy_sequential
            .frames
            .last()
            .ok_or("missing proxy final frame")?;
        run_proxy_gpu_frame(&discovery, device, "proxy-first", first)?;
        run_proxy_gpu_frame(&discovery, device, "proxy-53-sequential", proxy_seq_53)?;
        run_proxy_gpu_frame(&discovery, device, "proxy-53-random", proxy_rand_53)?;
        run_proxy_gpu_frame(&discovery, device, "proxy-final", final_frame)?;
    }
    println!("VA->Vulkan zero-copy path: frozen / not used");

    Ok(())
}

#[derive(Clone, Copy)]
struct CameraCase {
    original_label: &'static str,
    proxy_label: &'static str,
    damaged_proxy: bool,
}

fn identify_camera_case(
    original_sha256: &str,
    proxy_sha256: &str,
) -> Result<CameraCase, Box<dyn std::error::Error>> {
    match (original_sha256, proxy_sha256) {
        (EXPECTED_FX6_SAMPLE001_MXF_SHA256, EXPECTED_FX6_SAMPLE001_PROXY_SHA256) => {
            Ok(CameraCase {
                original_label: "Sony FX6 sample 001",
                proxy_label: "Sony FX6 sample 001 proxy",
                damaged_proxy: true,
            })
        }
        (EXPECTED_FX6_SAMPLE002_MXF_SHA256, EXPECTED_FX6_SAMPLE002_PROXY_SHA256) => {
            Ok(CameraCase {
                original_label: "Sony FX6 sample 002",
                proxy_label: "Sony FX6 sample 002 proxy",
                damaged_proxy: false,
            })
        }
        _ => Err("external camera case hashes do not match known Step 12 corpus".into()),
    }
}

fn diagnose_damaged_proxy_case(
    case: CameraCase,
    proxy_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Camera original/proxy proof:");
    println!("  original: {}", case.original_label);
    println!("  proxy: {}", case.proxy_label);
    println!("  hashes: verified");
    match Mp4Source::open(proxy_path) {
        Ok(_) => Err("damaged proxy unexpectedly parsed as valid MP4/H.264".into()),
        Err(error) => {
            let health = MediaHealth::from_open_error(&error);
            println!("Damaged proxy diagnostic:");
            println!("  health: {:?}", health);
            println!("  strict parser result: {error}");
            if health != MediaHealth::DamagedUnrecoverable {
                return Err(format!("expected damaged/unrecoverable proxy, got {health:?}").into());
            }
            println!("  complete proxy playback acceptance: not attempted");
            Ok(())
        }
    }
}

fn run_software_gpu_sequence_proof(
    discovery: &VulkanDeviceDiscovery,
    device: &DeviceDesc,
    label: &str,
    frames: &[(&str, &qgs_software_video::SoftwareVideoSurface)],
) -> Result<BTreeMap<String, u64>, Box<dyn std::error::Error>> {
    if frames.is_empty() {
        return Ok(BTreeMap::new());
    }
    let first_upload = yuv422p10_upload_for_frame(device.id, frames[0].1)?;
    let mut processor = GpuFrameProcessor::new(
        discovery,
        GpuFrameProcessorConfig {
            device_id: device.id,
            width: first_upload.width,
            height: first_upload.height,
            slot_count: 3,
            conversion: first_upload.conversion,
        },
    )?;
    let started = Instant::now();
    let mut pending = VecDeque::new();
    let mut checksums = BTreeMap::new();
    for (name, frame) in frames {
        let upload = yuv422p10_upload_for_frame(device.id, frame)?;
        let reference_start = Instant::now();
        let reference = yuv422p10_reference_rgba_u16(&upload)?;
        let reference_elapsed = reference_start.elapsed();
        let reference_checksum = yuv422p10_rgba_u16_checksum(&reference);
        let identity = FrameIdentity {
            presentation_position: frame.presentation_index,
        };
        match processor.submit_frame(&upload, identity) {
            Ok(token) => pending.push_back((
                token,
                (*name).to_string(),
                frame.presentation_index,
                reference,
                reference_checksum,
                reference_elapsed,
            )),
            Err(FrameProcessorError::NoFrameSlotAvailable) => {
                while let Some((
                    token,
                    pending_name,
                    presentation,
                    reference,
                    reference_checksum,
                    ref_elapsed,
                )) = pending.pop_front()
                {
                    let output = processor.wait_for_frame(token)?;
                    validate_software_gpu_output(
                        &output,
                        &pending_name,
                        presentation,
                        &reference,
                        reference_checksum,
                        ref_elapsed,
                        &mut checksums,
                    )?;
                }
                let token = processor.submit_frame(&upload, identity)?;
                pending.push_back((
                    token,
                    (*name).to_string(),
                    frame.presentation_index,
                    reference,
                    reference_checksum,
                    reference_elapsed,
                ));
            }
            Err(err) => return Err(Box::new(err)),
        }
    }
    while let Some((token, name, presentation, reference, reference_checksum, ref_elapsed)) =
        pending.pop_front()
    {
        let output = processor.wait_for_frame(token)?;
        validate_software_gpu_output(
            &output,
            &name,
            presentation,
            &reference,
            reference_checksum,
            ref_elapsed,
            &mut checksums,
        )?;
    }
    let elapsed = started.elapsed();
    let counters = processor.counters();
    println!(
        "  {label}: frames={} submissions={} slot_reuses={} pipelines={} command_buffers={} elapsed={:.3}ms",
        frames.len(),
        counters.frame_submissions,
        counters.slot_reuses,
        counters.pipeline_creations,
        counters.command_buffer_count,
        elapsed.as_secs_f64() * 1000.0
    );
    Ok(checksums)
}

fn validate_software_gpu_output(
    output: &qgs_vulkan::ProcessedFrameOutput,
    label: &str,
    presentation: u64,
    reference: &[u16],
    reference_checksum: u64,
    reference_elapsed: std::time::Duration,
    checksums: &mut BTreeMap<String, u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.presentation_position != presentation {
        return Err(format!("{label} GPU presentation identity mismatch").into());
    }
    let max_delta = max_u16_delta(reference, &output.rgba_u16)?;
    if max_delta > 1 {
        return Err(format!("{label} GPU output exceeded tolerance: max delta {max_delta}").into());
    }
    println!(
        "  {label}: presentation={} slot={} seq={} gpu_checksum=0x{:016x} cpu_ref=0x{:016x} max_delta={} ref_time={:.3}ms",
        presentation,
        output.slot_index,
        output.submission_sequence,
        output.checksum,
        reference_checksum,
        max_delta,
        reference_elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "    memory: cpu_surface={} staging={} gpu_planes={} output={}",
        output.cpu_surface_bytes, output.staging_bytes, output.gpu_plane_bytes, output.output_bytes
    );
    checksums.insert(label.to_string(), output.checksum);
    Ok(())
}

fn yuv422p10_upload_for_frame<'a>(
    device_id: qgs_protocol::DeviceId,
    frame: &'a qgs_software_video::SoftwareVideoSurface,
) -> Result<Yuv422P10Upload<'a>, Box<dyn std::error::Error>> {
    if frame.storage_format != qgs_software_video::SoftwarePixelFormat::Yuv422P10Le
        || frame.planes.len() != 3
    {
        return Err("expected YUV422P10LE software surface".into());
    }
    let plane = |index: usize| -> Yuv422P10Plane<'a> {
        let plane = &frame.planes[index];
        Yuv422P10Plane {
            width_samples: plane.width_samples,
            height: plane.height,
            stride_bytes: plane.stride_bytes,
            data: &plane.data,
        }
    };
    Ok(Yuv422P10Upload {
        device_id,
        width: frame.desc.coded_width,
        height: frame.desc.coded_height,
        y: plane(0),
        cb: plane(1),
        cr: plane(2),
        conversion: YcbcrConversion::Rec709Limited,
    })
}

fn max_u16_delta(left: &[u16], right: &[u16]) -> Result<u16, Box<dyn std::error::Error>> {
    if left.len() != right.len() {
        return Err("CPU/GPU output lengths differ".into());
    }
    Ok(left
        .iter()
        .zip(right)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0))
}

fn decode_positioned_access_units_with_context(
    config: DecoderConfig,
    access_units: &[(u64, Vec<u8>)],
) -> Result<DecodeRunStats, Box<dyn std::error::Error>> {
    let mut decoder = SoftwareH264Decoder::new(config)?;
    let mut frames = Vec::new();
    for (position, access_unit) in access_units {
        let decoded = decoder
            .decode_access_unit_at(access_unit, *position)
            .map_err(|err| format!("software decode failed at edit unit {position}: {err}"))?;
        frames.extend(decoded);
    }
    frames.extend(decoder.flush_surfaces()?);
    Ok(DecodeRunStats {
        frames,
        max_live_surfaces: decoder.max_live_surfaces(),
        elapsed: decoder.elapsed(),
    })
}

fn print_h264_gop_summary(
    source: &MediaSource,
    bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut state = qgs_codec_h264::H264DecoderState::new();
    let mut kinds = BTreeMap::new();
    let mut random_access_positions = Vec::new();
    let targets = [
        0_usize,
        source.index.video.len() / 2,
        source.index.video.len().saturating_sub(1),
    ];
    let mut target_summaries = Vec::new();
    for index in 0..source.index.video.len() {
        let access_unit = source.extract_video_access_unit(bytes, index)?;
        let parsed = state.parse_access_unit(&access_unit)?;
        if parsed.slices.iter().any(|slice| slice.idr) {
            random_access_positions.push(index);
        }
        if let Some(kind) = parsed
            .slices
            .first()
            .map(|slice| format!("{:?}", slice.kind))
        {
            *kinds.entry(kind.clone()).or_insert(0_usize) += 1;
            if targets.contains(&index) {
                target_summaries.push((
                    index,
                    kind,
                    parsed.desc.bit_depth.get(),
                    parsed.desc.chroma,
                ));
            }
        }
        state.finish_picture(&parsed)?;
    }
    println!("GOP structure:");
    println!("  picture counts: {:?}", kinds);
    println!("  IDR positions: {:?}", random_access_positions);
    for (index, kind, bit_depth, chroma) in target_summaries {
        println!("  sample edit unit {index}: kind={kind} bit_depth={bit_depth} chroma={chroma:?}");
    }
    Ok(())
}

#[derive(Debug)]
struct SonyXmlSummary {
    duration: Option<u64>,
    ltc_tc_fps: Option<String>,
    ltc_half_step: Option<bool>,
    video_codec: Option<String>,
    capture_fps: Option<String>,
    format_fps: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    aspect_ratio: Option<String>,
    audio_channels: Option<u16>,
    audio_codecs: Vec<String>,
    color_values: BTreeMap<String, String>,
    camera_model: Option<String>,
    has_proxy_metadata: bool,
}

impl SonyXmlSummary {
    fn from_mxf_path(path: &Path) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            return Ok(None);
        };
        let sidecar = path.with_file_name(format!("{stem}M01.XML"));
        if !sidecar.exists() {
            return Ok(None);
        }
        let xml = std::fs::read_to_string(sidecar)?;
        Ok(Some(Self::parse(&xml)))
    }

    fn parse(xml: &str) -> Self {
        let duration = find_attr(xml, "<Duration", "value").and_then(|value| value.parse().ok());
        let ltc_tc_fps = find_attr(xml, "<LtcChangeTable", "tcFps");
        let ltc_half_step =
            find_attr(xml, "<LtcChangeTable", "halfStep").map(|value| value == "true");
        let video_codec = find_attr(xml, "<VideoFrame", "videoCodec");
        let capture_fps = find_attr(xml, "<VideoFrame", "captureFps");
        let format_fps = find_attr(xml, "<VideoFrame", "formatFps");
        let width = find_attr(xml, "<VideoLayout", "pixel").and_then(|value| value.parse().ok());
        let height = find_attr(xml, "<VideoLayout", "numOfVerticalLine")
            .and_then(|value| value.parse().ok());
        let aspect_ratio = find_attr(xml, "<VideoLayout", "aspectRatio");
        let audio_channels =
            find_attr(xml, "<AudioFormat", "numOfChannel").and_then(|value| value.parse().ok());
        let audio_codecs = find_all_attrs(xml, "<AudioRecPort", "audioCodec");
        let camera_model = find_attr(xml, "<Device", "modelName");
        let has_proxy_metadata = xml.contains("SubStream")
            || xml.contains("Proxy")
            || xml.contains("proxy")
            || xml.contains("substream");
        let mut color_values = BTreeMap::new();
        for name in [
            "CaptureGammaEquation",
            "CaptureColorPrimaries",
            "CodingEquations",
        ] {
            if let Some(value) = find_item_value(xml, name) {
                color_values.insert(name.to_string(), value);
            }
        }
        Self {
            duration,
            ltc_tc_fps,
            ltc_half_step,
            video_codec,
            capture_fps,
            format_fps,
            width,
            height,
            aspect_ratio,
            audio_channels,
            audio_codecs,
            color_values,
            camera_model,
            has_proxy_metadata,
        }
    }
}

fn sha256_hex(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn proxy_presentation_ordinals(
    video: &qgs_mp4::Mp4VideoTrack,
) -> Result<BTreeMap<u32, u64>, Box<dyn std::error::Error>> {
    let mut ordered = video
        .samples
        .iter()
        .map(|sample| (sample.pts, sample.sample_index))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(pts, sample_index)| (*pts, *sample_index));
    let mut ordinals = BTreeMap::new();
    for (ordinal, (_, sample_index)) in ordered.into_iter().enumerate() {
        ordinals.insert(sample_index, u64::try_from(ordinal)?);
    }
    Ok(ordinals)
}

struct LumaSignatureComparison {
    original_hash: u64,
    proxy_hash: u64,
    mean_abs_delta: f64,
}

fn luma_signature_delta(
    original: &qgs_software_video::SoftwareVideoSurface,
    proxy: &qgs_software_video::SoftwareVideoSurface,
) -> Result<LumaSignatureComparison, Box<dyn std::error::Error>> {
    let original_signature = luma_signature(original)?;
    let proxy_signature = luma_signature(proxy)?;
    if original_signature.len() != proxy_signature.len() {
        return Err("luma signature lengths differ".into());
    }
    let total_delta = original_signature
        .iter()
        .zip(&proxy_signature)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum::<u64>();
    let mean_abs_delta = total_delta as f64 / original_signature.len().max(1) as f64;
    Ok(LumaSignatureComparison {
        original_hash: bytes_checksum(&original_signature),
        proxy_hash: bytes_checksum(&proxy_signature),
        mean_abs_delta,
    })
}

fn luma_signature(
    frame: &qgs_software_video::SoftwareVideoSurface,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let y = frame.planes.first().ok_or("missing luma plane")?;
    let grid_w = 16_u32.min(frame.desc.coded_width);
    let grid_h = 9_u32.min(frame.desc.coded_height);
    let mut values = Vec::new();
    for gy in 0..grid_h {
        let y_pos = gy
            .checked_mul(frame.desc.coded_height.saturating_sub(1))
            .ok_or("luma signature y overflow")?
            / grid_h.saturating_sub(1).max(1);
        for gx in 0..grid_w {
            let x_pos = gx
                .checked_mul(frame.desc.coded_width.saturating_sub(1))
                .ok_or("luma signature x overflow")?
                / grid_w.saturating_sub(1).max(1);
            let sample = match frame.storage_format {
                SoftwarePixelFormat::Yuv420P8 => {
                    let offset = usize::try_from(y_pos)?
                        .checked_mul(y.stride_bytes)
                        .and_then(|row| row.checked_add(usize::try_from(x_pos).ok()?))
                        .ok_or("luma signature offset overflow")?;
                    *y.data.get(offset).ok_or("luma signature out of bounds")?
                }
                SoftwarePixelFormat::Yuv422P10Le => {
                    let offset = usize::try_from(y_pos)?
                        .checked_mul(y.stride_bytes)
                        .and_then(|row| {
                            row.checked_add(usize::try_from(x_pos).ok()?.checked_mul(2)?)
                        })
                        .ok_or("luma signature offset overflow")?;
                    let bytes = y
                        .data
                        .get(offset..offset + 2)
                        .ok_or("luma signature out of bounds")?;
                    (u16::from_le_bytes([bytes[0], bytes[1]]) >> 2) as u8
                }
            };
            values.push(sample);
        }
    }
    Ok(values)
}

fn bytes_checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in bytes {
        hash ^= u64::from(*value);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn run_proxy_hardware_decode(
    socket_path: &Path,
    config: &DecoderConfig,
    video: &qgs_mp4::Mp4VideoTrack,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = connect_socket(socket_path)?;
    let mut request_id = 1_u64;
    send_message(
        &mut stream,
        &WireMessage::Hello {
            request_id,
            request: HelloRequest::current(),
        },
    )?;
    let response = receive_message(&mut stream)?;
    let WireMessage::Welcome { .. } = response else {
        return Err("expected WELCOME for proxy hardware proof".into());
    };
    request_id += 1;
    send_message(&mut stream, &WireMessage::EnumerateDevices { request_id })?;
    let response = receive_message(&mut stream)?;
    let WireMessage::DeviceList { response, .. } = response else {
        return Err("expected DEVICE_LIST for proxy hardware proof".into());
    };
    let Some(device) = response
        .devices
        .iter()
        .find(|device| {
            device.vendor_id == 0x8086 && matches!(device.class, DeviceClass::IntegratedGpu)
        })
        .or_else(|| {
            response
                .devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::IntegratedGpu))
        })
    else {
        println!("Intel proxy hardware decode: no integrated GPU advertised");
        return Ok(());
    };
    let mut intel_config = config.clone();
    intel_config.device_id = device.id;
    let supports_proxy = query_h264_decode_support(
        &mut stream,
        &mut request_id,
        device,
        H264Profile::High,
        ChromaSubsampling::Cs420,
        8,
    )?;
    println!("Intel proxy capability:");
    println!("  device: {}", device.name);
    println!(
        "  H.264 High 8-bit 4:2:0 support: {}",
        yes_no(supports_proxy)
    );
    if !supports_proxy {
        return Ok(());
    }

    request_id += 1;
    send_message(
        &mut stream,
        &WireMessage::CreateDecoder {
            request_id,
            request: CreateDecoderRequest {
                config: intel_config,
            },
        },
    )?;
    let response = receive_message(&mut stream)?;
    let decoder_id = match response {
        WireMessage::DecoderCreated { response, .. } => response.decoder_id,
        WireMessage::Error { response, .. } => {
            return Err(format!(
                "proxy hardware decoder creation failed: {:?}",
                response.code
            )
            .into());
        }
        _ => return Err("expected proxy DECODER_CREATED".into()),
    };
    let started = Instant::now();
    let mut output_count = 0_usize;
    let mut parser_state = qgs_codec_h264::H264DecoderState::new();
    for sample in &video.samples {
        let parsed_for_diagnostic = parser_state.parse_access_unit(&sample.annex_b)?;
        let diagnostic = format!(
            "kind={:?} idr={} frame_num={} poc={} reference={} refs={} l0={} l1={}",
            parsed_for_diagnostic
                .slices
                .first()
                .map(|slice| &slice.kind),
            parsed_for_diagnostic.picture.idr_pic_flag,
            parsed_for_diagnostic.picture.frame_num,
            parsed_for_diagnostic.picture.top_field_order_cnt,
            parsed_for_diagnostic.picture.reference_pic_flag,
            parsed_for_diagnostic.reference_frames.len(),
            parsed_for_diagnostic
                .slices
                .first()
                .map(|slice| slice.ref_pic_list0.len())
                .unwrap_or(0),
            parsed_for_diagnostic
                .slices
                .first()
                .map(|slice| slice.ref_pic_list1.len())
                .unwrap_or(0)
        );
        parser_state.finish_picture(&parsed_for_diagnostic)?;
        request_id += 1;
        send_message(
            &mut stream,
            &WireMessage::SubmitAccessUnit {
                request_id,
                request: SubmitAccessUnitRequest {
                    decoder_id,
                    data: sample.annex_b.clone(),
                },
            },
        )?;
        let response = receive_message(&mut stream)?;
        let response = match response {
            WireMessage::DecodeOutput { response, .. } => response,
            WireMessage::Error { response, .. } => {
                return Err(format!(
                    "proxy hardware decode failed at sample {}: {:?} ({diagnostic})",
                    sample.sample_index, response.code
                )
                .into());
            }
            other => {
                return Err(format!(
                    "expected proxy DECODE_OUTPUT at sample {}, got {:?}",
                    sample.sample_index, other
                )
                .into());
            }
        };
        output_count += response.outputs.len();
        for output in response.outputs {
            request_id += 1;
            destroy_resource(&mut stream, request_id, output.resource_id)?;
        }
    }
    request_id += 1;
    send_message(
        &mut stream,
        &WireMessage::FlushDecoder {
            request_id,
            request: FlushDecoderRequest { decoder_id },
        },
    )?;
    let response = receive_message(&mut stream)?;
    let WireMessage::DecodeOutput { response, .. } = response else {
        return Err("expected proxy flush DECODE_OUTPUT".into());
    };
    output_count += response.outputs.len();
    for output in response.outputs {
        request_id += 1;
        destroy_resource(&mut stream, request_id, output.resource_id)?;
    }
    let elapsed = started.elapsed();
    if output_count != video.samples.len() {
        return Err(format!(
            "proxy hardware decode expected {} frames, got {output_count}",
            video.samples.len()
        )
        .into());
    }
    request_id += 1;
    send_message(
        &mut stream,
        &WireMessage::DestroyDecoder {
            request_id,
            request: DestroyDecoderRequest { decoder_id },
        },
    )?;
    let response = receive_message(&mut stream)?;
    let WireMessage::DecoderDestroyed { .. } = response else {
        return Err("expected proxy DECODER_DESTROYED".into());
    };
    println!("Intel proxy hardware decode:");
    println!("  decoded frames: {output_count}");
    println!(
        "  DEVELOPMENT OBSERVATION - NOT A BENCHMARK: {:.3}s, {:.2} fps, {:.2}x realtime @ 50fps",
        elapsed.as_secs_f64(),
        output_count as f64 / elapsed.as_secs_f64().max(0.000_001),
        (output_count as f64 / elapsed.as_secs_f64().max(0.000_001)) / 50.0
    );
    Ok(())
}

struct ProxyGpuUpload {
    y: Vec<u8>,
    cb: Vec<u8>,
    cr: Vec<u8>,
    width: u32,
    height: u32,
}

impl ProxyGpuUpload {
    fn upload(&self, device_id: qgs_protocol::DeviceId) -> Yuv422P10Upload<'_> {
        let y_stride = usize::try_from(self.width).expect("width") * 2;
        let c_stride = usize::try_from(self.width / 2).expect("width") * 2;
        Yuv422P10Upload {
            device_id,
            width: self.width,
            height: self.height,
            y: Yuv422P10Plane {
                width_samples: self.width,
                height: self.height,
                stride_bytes: y_stride,
                data: &self.y,
            },
            cb: Yuv422P10Plane {
                width_samples: self.width / 2,
                height: self.height,
                stride_bytes: c_stride,
                data: &self.cb,
            },
            cr: Yuv422P10Plane {
                width_samples: self.width / 2,
                height: self.height,
                stride_bytes: c_stride,
                data: &self.cr,
            },
            conversion: YcbcrConversion::Rec709Limited,
        }
    }
}

fn proxy_yuv420p8_to_yuv422p10(
    frame: &qgs_software_video::SoftwareVideoSurface,
) -> Result<ProxyGpuUpload, Box<dyn std::error::Error>> {
    if frame.storage_format != SoftwarePixelFormat::Yuv420P8 || frame.planes.len() != 3 {
        return Err("expected YUV420P8 proxy frame".into());
    }
    let width = frame.desc.coded_width;
    let height = frame.desc.coded_height;
    let y_stride = usize::try_from(width)?
        .checked_mul(2)
        .ok_or("stride overflow")?;
    let c_stride = usize::try_from(width / 2)?
        .checked_mul(2)
        .ok_or("stride overflow")?;
    let mut y = vec![
        0_u8;
        y_stride
            .checked_mul(usize::try_from(height)?)
            .ok_or("Y len overflow")?
    ];
    let mut cb = vec![
        0_u8;
        c_stride
            .checked_mul(usize::try_from(height)?)
            .ok_or("Cb len overflow")?
    ];
    let mut cr = vec![
        0_u8;
        c_stride
            .checked_mul(usize::try_from(height)?)
            .ok_or("Cr len overflow")?
    ];
    let src_y = &frame.planes[0];
    let src_cb = &frame.planes[1];
    let src_cr = &frame.planes[2];
    for row in 0..usize::try_from(height)? {
        for x in 0..usize::try_from(width)? {
            let src_offset = row
                .checked_mul(src_y.stride_bytes)
                .and_then(|base| base.checked_add(x))
                .ok_or("Y source offset overflow")?;
            let value =
                u16::from(*src_y.data.get(src_offset).ok_or("Y source out of bounds")?) << 2;
            y[row * y_stride + x * 2..row * y_stride + x * 2 + 2]
                .copy_from_slice(&value.to_le_bytes());
        }
        let chroma_row = row / 2;
        for x in 0..usize::try_from(width / 2)? {
            let src_offset = chroma_row
                .checked_mul(src_cb.stride_bytes)
                .and_then(|base| base.checked_add(x))
                .ok_or("C source offset overflow")?;
            let cb_value = u16::from(
                *src_cb
                    .data
                    .get(src_offset)
                    .ok_or("Cb source out of bounds")?,
            ) << 2;
            let cr_value = u16::from(
                *src_cr
                    .data
                    .get(src_offset)
                    .ok_or("Cr source out of bounds")?,
            ) << 2;
            cb[row * c_stride + x * 2..row * c_stride + x * 2 + 2]
                .copy_from_slice(&cb_value.to_le_bytes());
            cr[row * c_stride + x * 2..row * c_stride + x * 2 + 2]
                .copy_from_slice(&cr_value.to_le_bytes());
        }
    }
    Ok(ProxyGpuUpload {
        y,
        cb,
        cr,
        width,
        height,
    })
}

fn run_proxy_gpu_frame(
    discovery: &VulkanDeviceDiscovery,
    device: &DeviceDesc,
    label: &str,
    frame: &qgs_software_video::SoftwareVideoSurface,
) -> Result<(), Box<dyn std::error::Error>> {
    let converted = proxy_yuv420p8_to_yuv422p10(frame)?;
    let upload = converted.upload(device.id);
    let reference = yuv422p10_reference_rgba_u16(&upload)?;
    let reference_checksum = yuv422p10_rgba_u16_checksum(&reference);
    let mut processor = GpuFrameProcessor::new(
        discovery,
        GpuFrameProcessorConfig {
            device_id: device.id,
            width: upload.width,
            height: upload.height,
            slot_count: 3,
            conversion: upload.conversion,
        },
    )?;
    let token = processor.submit_frame(
        &upload,
        FrameIdentity {
            presentation_position: frame.presentation_index,
        },
    )?;
    let output = processor.wait_for_frame(token)?;
    let max_delta = max_u16_delta(&reference, &output.rgba_u16)?;
    if max_delta > 1 {
        return Err(format!("proxy GPU output exceeded tolerance: {max_delta}").into());
    }
    println!(
        "    {label}: presentation={} checksum=0x{:016x} cpu=0x{reference_checksum:016x} max_delta={max_delta}",
        output.presentation_position, output.checksum
    );
    Ok(())
}

fn print_sony_xml_comparison(
    source: &MediaSource,
    parsed: &qgs_codec_h264::ParsedH264AccessUnit,
    sidecar: &SonyXmlSummary,
) {
    println!("Sony XML sidecar comparison:");
    if let Some(model) = &sidecar.camera_model {
        println!("  camera model: {model}");
    }
    println!("  XML duration edit units: {:?}", sidecar.duration);
    println!("  MXF duration edit units: {:?}", source.duration);
    println!("  XML video codec: {:?}", sidecar.video_codec);
    println!(
        "  XML fps: capture={:?} format={:?}",
        sidecar.capture_fps, sidecar.format_fps
    );
    println!(
        "  XML layout: {:?} x {:?} aspect={:?}",
        sidecar.width, sidecar.height, sidecar.aspect_ratio
    );
    println!(
        "  H.264 SPS: {} x {} bit_depth={} chroma={:?}",
        parsed.desc.coded_width,
        parsed.desc.coded_height,
        parsed.desc.bit_depth.get(),
        parsed.desc.chroma
    );
    println!("  XML audio channels: {:?}", sidecar.audio_channels);
    println!("  XML audio codecs: {:?}", sidecar.audio_codecs);
    println!(
        "  XML LTC: tcFps={:?} halfStep={:?}",
        sidecar.ltc_tc_fps, sidecar.ltc_half_step
    );
    if !sidecar.color_values.is_empty() {
        println!("  XML color/acquisition: {:?}", sidecar.color_values);
    }
}

fn find_attr(xml: &str, tag_prefix: &str, attr: &str) -> Option<String> {
    let start = xml.find(tag_prefix)?;
    let rest = &xml[start..];
    let end = rest.find('>')?;
    attr_in_tag(&rest[..end], attr)
}

fn find_all_attrs(xml: &str, tag_prefix: &str, attr: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(tag_prefix) {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        if let Some(value) = attr_in_tag(&rest[..end], attr) {
            values.push(value);
        }
        rest = &rest[end..];
    }
    values
}

fn find_item_value(xml: &str, name: &str) -> Option<String> {
    let needle = format!("<Item name=\"{name}\"");
    let start = xml.find(&needle)?;
    let rest = &xml[start..];
    let end = rest.find('>')?;
    attr_in_tag(&rest[..end], "value")
}

fn attr_in_tag(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

fn mxf_file_label(path: &Path) -> String {
    if path.is_absolute() {
        if let Ok(current_dir) = std::env::current_dir() {
            if let Ok(relative) = path.strip_prefix(current_dir) {
                return relative.display().to_string();
            }
        }
        return "<external>".to_string();
    }
    path.display().to_string()
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

fn test_synthetic_yuv422p10_gpu_proof(
    device: &DeviceDesc,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{:?}] {}", device.class, device.name);
    let width = 8_u32;
    let height = 4_u32;
    let y_stride = 20_usize;
    let c_stride = 12_usize;
    let mut y = vec![0_u8; y_stride * height as usize];
    let mut cb = vec![0_u8; c_stride * height as usize];
    let mut cr = vec![0_u8; c_stride * height as usize];
    for row in 0..height as usize {
        for x in 0..width as usize {
            let sample = 64_u16 + u16::try_from((row * width as usize + x) * 7 % 877)?;
            y[row * y_stride + x * 2..row * y_stride + x * 2 + 2]
                .copy_from_slice(&sample.to_le_bytes());
        }
        for x in 0..(width as usize / 2) {
            let cb_sample = 512_u16 + u16::try_from((x * 11 + row * 3) % 96)?;
            let cr_sample = 512_u16.saturating_sub(u16::try_from((x * 5 + row * 7) % 96)?);
            cb[row * c_stride + x * 2..row * c_stride + x * 2 + 2]
                .copy_from_slice(&cb_sample.to_le_bytes());
            cr[row * c_stride + x * 2..row * c_stride + x * 2 + 2]
                .copy_from_slice(&cr_sample.to_le_bytes());
        }
    }
    let upload = Yuv422P10Upload {
        device_id: device.id,
        width,
        height,
        y: Yuv422P10Plane {
            width_samples: width,
            height,
            stride_bytes: y_stride,
            data: &y,
        },
        cb: Yuv422P10Plane {
            width_samples: width / 2,
            height,
            stride_bytes: c_stride,
            data: &cb,
        },
        cr: Yuv422P10Plane {
            width_samples: width / 2,
            height,
            stride_bytes: c_stride,
            data: &cr,
        },
        conversion: YcbcrConversion::Rec709Limited,
    };
    let reference = yuv422p10_reference_rgba_u16(&upload)?;
    let reference_checksum = yuv422p10_rgba_u16_checksum(&reference);
    let discovery = VulkanDeviceDiscovery::new()?;
    let started = Instant::now();
    let output = discovery.process_yuv422p10_surface(&upload)?;
    let elapsed = started.elapsed();
    let max_delta = max_u16_delta(&reference, &output.rgba_u16)?;
    println!("  input: 8x4 YUV422P10LE with padded CPU strides");
    println!("  representation: three u32 storage-buffer GPU planes");
    println!("  operation: fixed Rec.709 limited YCbCr -> RGBA u16 compute proof");
    println!("  gpu checksum: 0x{:016x}", output.checksum);
    println!("  cpu checksum: 0x{reference_checksum:016x}");
    println!("  max CPU/GPU delta: {max_delta}");
    println!(
        "  memory: cpu_surface={} staging={} gpu_planes={} output={}",
        output.cpu_surface_bytes, output.staging_bytes, output.gpu_plane_bytes, output.output_bytes
    );
    println!(
        "  DEVELOPMENT OBSERVATION - NOT A BENCHMARK: {:.3}ms upload/process/readback",
        elapsed.as_secs_f64() * 1000.0
    );
    if max_delta > 1 {
        return Err("synthetic YUV422P10 GPU proof exceeded tolerance".into());
    }
    println!("Validation:");
    println!("PASS");
    Ok(())
}

struct SyntheticYuv422P10Frame {
    width: u32,
    height: u32,
    y_stride: usize,
    c_stride: usize,
    y: Vec<u8>,
    cb: Vec<u8>,
    cr: Vec<u8>,
}

impl SyntheticYuv422P10Frame {
    fn upload(&self, device_id: qgs_protocol::DeviceId) -> Yuv422P10Upload<'_> {
        Yuv422P10Upload {
            device_id,
            width: self.width,
            height: self.height,
            y: Yuv422P10Plane {
                width_samples: self.width,
                height: self.height,
                stride_bytes: self.y_stride,
                data: &self.y,
            },
            cb: Yuv422P10Plane {
                width_samples: self.width / 2,
                height: self.height,
                stride_bytes: self.c_stride,
                data: &self.cb,
            },
            cr: Yuv422P10Plane {
                width_samples: self.width / 2,
                height: self.height,
                stride_bytes: self.c_stride,
                data: &self.cr,
            },
            conversion: YcbcrConversion::Rec709Limited,
        }
    }
}

fn make_synthetic_yuv422p10_frame(
    frame_index: usize,
) -> Result<SyntheticYuv422P10Frame, Box<dyn std::error::Error>> {
    let width = 8_u32;
    let height = 4_u32;
    let y_stride = 20_usize;
    let c_stride = 12_usize;
    let mut y = vec![0_u8; y_stride * height as usize];
    let mut cb = vec![0_u8; c_stride * height as usize];
    let mut cr = vec![0_u8; c_stride * height as usize];
    for row in 0..height as usize {
        for x in 0..width as usize {
            let base = row * width as usize + x + frame_index * 13;
            let sample = 64_u16 + u16::try_from((base * 7) % 877)?;
            y[row * y_stride + x * 2..row * y_stride + x * 2 + 2]
                .copy_from_slice(&sample.to_le_bytes());
        }
        for x in 0..(width as usize / 2) {
            let cb_sample = 512_u16 + u16::try_from((x * 11 + row * 3 + frame_index * 5) % 96)?;
            let cr_sample =
                512_u16.saturating_sub(u16::try_from((x * 5 + row * 7 + frame_index * 9) % 96)?);
            cb[row * c_stride + x * 2..row * c_stride + x * 2 + 2]
                .copy_from_slice(&cb_sample.to_le_bytes());
            cr[row * c_stride + x * 2..row * c_stride + x * 2 + 2]
                .copy_from_slice(&cr_sample.to_le_bytes());
        }
    }
    Ok(SyntheticYuv422P10Frame {
        width,
        height,
        y_stride,
        c_stride,
        y,
        cb,
        cr,
    })
}

fn test_reusable_yuv422p10_gpu_processor(
    device: &DeviceDesc,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("  reusable processor:");
    let discovery = VulkanDeviceDiscovery::new()?;
    let frames = (0..6)
        .map(make_synthetic_yuv422p10_frame)
        .collect::<Result<Vec<_>, _>>()?;
    let mut processor = GpuFrameProcessor::new(
        &discovery,
        GpuFrameProcessorConfig {
            device_id: device.id,
            width: frames[0].width,
            height: frames[0].height,
            slot_count: 3,
            conversion: YcbcrConversion::Rec709Limited,
        },
    )?;
    let started = Instant::now();
    let mut pending = VecDeque::new();
    let mut checksums = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let upload = frame.upload(device.id);
        let reference = yuv422p10_reference_rgba_u16(&upload)?;
        let reference_checksum = yuv422p10_rgba_u16_checksum(&reference);
        match processor.submit_frame(
            &upload,
            FrameIdentity {
                presentation_position: index as u64,
            },
        ) {
            Ok(token) => pending.push_back((token, index, reference, reference_checksum)),
            Err(FrameProcessorError::NoFrameSlotAvailable) => {
                while let Some((token, submitted_index, reference, reference_checksum)) =
                    pending.pop_front()
                {
                    let output = processor.wait_for_frame(token)?;
                    validate_reusable_output(
                        &output,
                        submitted_index,
                        &reference,
                        reference_checksum,
                    )?;
                    checksums.push(output.checksum);
                }
                let token = processor.submit_frame(
                    &upload,
                    FrameIdentity {
                        presentation_position: index as u64,
                    },
                )?;
                pending.push_back((token, index, reference, reference_checksum));
            }
            Err(err) => return Err(Box::new(err)),
        }
        if index == 2 {
            let fourth = frames[3].upload(device.id);
            match processor.submit_frame(
                &fourth,
                FrameIdentity {
                    presentation_position: 3,
                },
            ) {
                Err(FrameProcessorError::NoFrameSlotAvailable) => {
                    println!("    bounded backpressure after 3 in-flight frames: yes");
                }
                Ok(_) => return Err("processor accepted an unbounded fourth slot".into()),
                Err(err) => return Err(Box::new(err)),
            }
        }
    }
    while let Some((token, index, reference, reference_checksum)) = pending.pop_front() {
        let output = processor.wait_for_frame(token)?;
        validate_reusable_output(&output, index, &reference, reference_checksum)?;
        checksums.push(output.checksum);
    }
    let elapsed = started.elapsed();
    let counters = processor.counters();
    println!("    frames submitted: {}", counters.frame_submissions);
    println!("    slot reuses: {}", counters.slot_reuses);
    println!(
        "    resources: pipelines={} shaders={} staging={} gpu_planes={} outputs={} readbacks={} command_buffers={}",
        counters.pipeline_creations,
        counters.shader_module_creations,
        counters.staging_allocations,
        counters.gpu_plane_allocations,
        counters.output_allocations,
        counters.readback_allocations,
        counters.command_buffer_count
    );
    println!(
        "    checksums: {:?}",
        checksums
            .iter()
            .map(|checksum| format!("0x{checksum:016x}"))
            .collect::<Vec<_>>()
    );
    println!(
        "    DEVELOPMENT OBSERVATION - NOT A BENCHMARK: {:.3}ms for 6 reusable submissions",
        elapsed.as_secs_f64() * 1000.0
    );
    if counters.pipeline_creations != 1
        || counters.shader_module_creations != 1
        || counters.staging_allocations != 9
        || counters.gpu_plane_allocations != 9
        || counters.output_allocations != 3
        || counters.command_buffer_count != 3
        || counters.frame_submissions != 6
        || counters.slot_reuses < 3
    {
        return Err("reusable processor counters did not show bounded reuse".into());
    }
    println!("    validation: PASS");
    Ok(())
}

fn validate_reusable_output(
    output: &qgs_vulkan::ProcessedFrameOutput,
    expected_index: usize,
    reference: &[u16],
    reference_checksum: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.presentation_position != expected_index as u64 {
        return Err("processed output presentation identity mismatch".into());
    }
    let max_delta = max_u16_delta(reference, &output.rgba_u16)?;
    if max_delta > 1 {
        return Err(format!("reusable GPU output exceeded tolerance: {max_delta}").into());
    }
    println!(
        "    frame {expected_index}: slot={} seq={} gpu=0x{:016x} cpu=0x{reference_checksum:016x} max_delta={max_delta}",
        output.slot_index, output.submission_sequence, output.checksum
    );
    Ok(())
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

fn query_h264_decode_support(
    stream: &mut std::os::unix::net::UnixStream,
    request_id: &mut u64,
    device: &DeviceDesc,
    profile: H264Profile,
    chroma: ChromaSubsampling,
    bit_depth: u8,
) -> Result<bool, Box<dyn std::error::Error>> {
    *request_id += 1;
    send_message(
        stream,
        &WireMessage::QueryVideoCapabilities {
            request_id: *request_id,
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
    if response_request_id != *request_id {
        return Err("video capability response request_id did not match request".into());
    }
    Ok(response.capabilities.decode.iter().any(|capability| {
        capability.codec == VideoCodec::H264
            && capability.profile == VideoProfile::H264(profile)
            && capability.bit_depth.get() == bit_depth
            && capability.chroma == chroma
            && capability
                .output_surface_formats
                .contains(&qgs_protocol::VideoSurfaceFormat::Nv12)
    }))
}

fn test_h264_decode(
    stream: &mut std::os::unix::net::UnixStream,
    mut request_id: u64,
    device: &DeviceDesc,
) -> Result<u64, Box<dyn std::error::Error>> {
    println!("H.264 decode proof:");
    println!("[{:?}] {}", device.class, device.name);

    let supports_h264_baseline = query_h264_decode_support(
        stream,
        &mut request_id,
        device,
        H264Profile::Baseline,
        ChromaSubsampling::Cs420,
        8,
    )?;

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
                println!("Decoder created through software fallback:");
            } else {
                println!("Decoder created:");
            }
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
    let mut outputs = response.outputs;
    if outputs.is_empty() && !supports_h264_baseline {
        request_id += 1;
        send_message(
            stream,
            &WireMessage::FlushDecoder {
                request_id,
                request: FlushDecoderRequest { decoder_id },
            },
        )?;
        let response = receive_message(stream)?;
        let WireMessage::DecodeOutput {
            request_id: response_request_id,
            response,
        } = response
        else {
            return Err("expected software fallback flush DECODE_OUTPUT response".into());
        };
        if response_request_id != request_id || response.decoder_id != decoder_id {
            return Err("software fallback flush response correlation failed".into());
        }
        outputs = response.outputs;
    }
    if outputs.len() != 1 {
        return Err(format!(
            "IDR decode expected one output surface, got {}",
            outputs.len()
        )
        .into());
    }
    let output = &outputs[0];
    println!("Decoded VideoSurface:");
    println!("  resource id: {}", output.resource_id.get());
    println!(
        "  {} x {} {:?}",
        output.surface.coded_width, output.surface.coded_height, output.surface.format
    );
    if supports_h264_baseline {
        println!("  validation: backend VA readback succeeded");
    } else {
        println!("  validation: software fallback output succeeded");
    }

    request_id += 1;
    destroy_resource(stream, request_id, output.resource_id)?;
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

fn test_h264_long_gop_decode(
    stream: &mut std::os::unix::net::UnixStream,
    mut request_id: u64,
    device: &DeviceDesc,
) -> Result<u64, Box<dyn std::error::Error>> {
    println!("H.264 Long-GOP decode proof:");
    println!("[{:?}] {}", device.class, device.name);

    let access_units = split_h264_annex_b_access_units(H264_LONG_GOP_FIXTURE)?;
    println!("  access units: {}", access_units.len());
    let supports_h264_main = query_h264_decode_support(
        stream,
        &mut request_id,
        device,
        H264Profile::Main,
        ChromaSubsampling::Cs420,
        8,
    )?;

    request_id += 1;
    let config = DecoderConfig {
        device_id: device.id,
        codec: VideoCodec::H264,
        profile: VideoProfile::H264(H264Profile::Main),
        bit_depth: BitDepth::new(8)?,
        chroma: ChromaSubsampling::Cs420,
        coded_width: 128,
        coded_height: 72,
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
                return Err("Long-GOP decoder request_id did not match".into());
            }
            if !supports_h264_main {
                println!("  Long-GOP decoder created through software fallback.");
            }
            response.decoder_id
        }
        WireMessage::Error { response, .. }
            if response.code == ProtocolErrorCode::UnsupportedDecodeConfiguration =>
        {
            println!("  Long-GOP decoder unsupported as expected for this device.");
            return Ok(request_id);
        }
        other => {
            return Err(format!("unexpected Long-GOP decoder response: {other:?}").into());
        }
    };

    let mut output_count = 0_usize;
    let mut output_resources = Vec::new();
    for (index, access_unit) in access_units.iter().enumerate() {
        request_id += 1;
        send_message(
            stream,
            &WireMessage::SubmitAccessUnit {
                request_id,
                request: SubmitAccessUnitRequest {
                    decoder_id,
                    data: access_unit.clone(),
                },
            },
        )?;
        let response = receive_message(stream)?;
        let WireMessage::DecodeOutput {
            request_id: response_request_id,
            response,
        } = response
        else {
            return Err("expected Long-GOP DECODE_OUTPUT response".into());
        };
        if response_request_id != request_id || response.decoder_id != decoder_id {
            return Err("Long-GOP decode response correlation failed".into());
        }
        println!(
            "  submitted AU {:02}: {} output surface(s)",
            index,
            response.outputs.len()
        );
        output_count += response.outputs.len();
        output_resources.extend(
            response
                .outputs
                .into_iter()
                .map(|output| output.resource_id),
        );
    }

    request_id += 1;
    send_message(
        stream,
        &WireMessage::FlushDecoder {
            request_id,
            request: FlushDecoderRequest { decoder_id },
        },
    )?;
    let response = receive_message(stream)?;
    let WireMessage::DecodeOutput {
        request_id: response_request_id,
        response,
    } = response
    else {
        return Err("expected Long-GOP flush DECODE_OUTPUT response".into());
    };
    if response_request_id != request_id || response.decoder_id != decoder_id {
        return Err("Long-GOP flush response correlation failed".into());
    }
    println!("  flush output surface(s): {}", response.outputs.len());
    output_count += response.outputs.len();
    output_resources.extend(
        response
            .outputs
            .into_iter()
            .map(|output| output.resource_id),
    );

    if output_count != 12 {
        return Err(format!("Long-GOP expected 12 output frames, got {output_count}").into());
    }
    if supports_h264_main {
        println!("  validation: backend VA readback succeeded for every decoded frame");
    } else {
        println!("  validation: software fallback output succeeded for every decoded frame");
    }

    for resource_id in output_resources {
        request_id += 1;
        destroy_resource(stream, request_id, resource_id)?;
    }

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
        return Err("expected Long-GOP DECODER_DESTROYED response".into());
    };
    if response_request_id != request_id || response.decoder_id != decoder_id {
        return Err("Long-GOP decoder destroyed response did not match".into());
    }
    println!("  decoded frames: {output_count}");
    println!("  decoder destroyed successfully");

    Ok(request_id)
}

fn split_h264_annex_b_access_units(
    data: &[u8],
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i..].starts_with(&[0, 0, 1]) {
            starts.push((i, 3));
            i += 3;
        } else if i + 4 <= data.len() && data[i..].starts_with(&[0, 0, 0, 1]) {
            starts.push((i, 4));
            i += 4;
        } else {
            i += 1;
        }
    }
    if starts.is_empty() {
        return Err("Long-GOP fixture is not Annex B".into());
    }

    let mut access_units = Vec::new();
    let mut current = Vec::new();
    let mut seen_vcl = false;
    for (index, (start, prefix_len)) in starts.iter().copied().enumerate() {
        let nal_start = start + prefix_len;
        let nal_end = starts
            .get(index + 1)
            .map(|(next, _)| *next)
            .unwrap_or(data.len());
        if nal_start >= nal_end {
            return Err("malformed Annex B NAL in Long-GOP fixture".into());
        }
        let nal_type = data[nal_start] & 0x1f;
        let is_vcl = nal_type == 1 || nal_type == 5;
        if is_vcl && seen_vcl && !current.is_empty() {
            access_units.push(std::mem::take(&mut current));
        }
        current.extend_from_slice(&data[start..nal_end]);
        if is_vcl {
            seen_vcl = true;
        }
    }
    if !current.is_empty() {
        access_units.push(current);
    }
    Ok(access_units)
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
            response
                .outputs
                .first()
                .map(|output| output.resource_id.get())
                .unwrap_or(0)
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
    mxf_inspect_path: Option<PathBuf>,
    software_decode_mxf_path: Option<PathBuf>,
    software_gpu_mxf_path: Option<PathBuf>,
    proxy_proof_paths: Option<(PathBuf, PathBuf)>,
}

impl Args {
    fn parse() -> Self {
        let mut socket_path = None;
        let mut video_capabilities_only = false;
        let mut h264_decode_only = false;
        let mut haswell_video_diagnostic = false;
        let mut mxf_inspect_path = None;
        let mut next_arg_is_mxf_path = false;
        let mut software_decode_mxf_path = None;
        let mut next_arg_is_software_decode_mxf_path = false;
        let mut software_gpu_mxf_path = None;
        let mut next_arg_is_software_gpu_mxf_path = false;
        let mut proxy_proof_original = None;
        let mut proxy_proof_paths = None;
        let mut next_arg_is_proxy_original = false;
        let mut next_arg_is_proxy_proxy = false;

        for arg in std::env::args_os().skip(1) {
            if next_arg_is_proxy_proxy {
                let proxy = PathBuf::from(arg);
                let original = proxy_proof_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                proxy_proof_paths = Some((original, proxy));
                next_arg_is_proxy_proxy = false;
            } else if next_arg_is_proxy_original {
                proxy_proof_original = Some(PathBuf::from(arg));
                next_arg_is_proxy_original = false;
                next_arg_is_proxy_proxy = true;
            } else if next_arg_is_software_gpu_mxf_path {
                software_gpu_mxf_path = Some(PathBuf::from(arg));
                next_arg_is_software_gpu_mxf_path = false;
            } else if next_arg_is_software_decode_mxf_path {
                software_decode_mxf_path = Some(PathBuf::from(arg));
                next_arg_is_software_decode_mxf_path = false;
            } else if next_arg_is_mxf_path {
                mxf_inspect_path = Some(PathBuf::from(arg));
                next_arg_is_mxf_path = false;
            } else if arg == VIDEO_CAPABILITIES_ONLY_ARG {
                video_capabilities_only = true;
            } else if arg == H264_DECODE_ONLY_ARG {
                h264_decode_only = true;
            } else if arg == HASWELL_VIDEO_DIAGNOSTIC_ARG {
                haswell_video_diagnostic = true;
            } else if arg == MXF_INSPECT_ARG {
                mxf_inspect_path = Some(PathBuf::from(DEFAULT_MXF_FIXTURE));
                next_arg_is_mxf_path = true;
            } else if arg == SOFTWARE_DECODE_MXF_ARG {
                next_arg_is_software_decode_mxf_path = true;
            } else if arg == SOFTWARE_GPU_MXF_ARG {
                next_arg_is_software_gpu_mxf_path = true;
            } else if arg == PROXY_PROOF_ARG {
                next_arg_is_proxy_original = true;
            } else if socket_path.is_none() {
                socket_path = Some(PathBuf::from(arg));
            }
        }

        Self {
            socket_path: socket_path.unwrap_or_else(default_socket_path),
            video_capabilities_only,
            h264_decode_only,
            haswell_video_diagnostic,
            mxf_inspect_path,
            software_decode_mxf_path,
            software_gpu_mxf_path,
            proxy_proof_paths,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{mxf_file_label, proxy_presentation_ordinals, SonyXmlSummary};
    use qgs_mp4::{Mp4VideoSample, Mp4VideoTrack, Rational};
    use std::path::PathBuf;

    #[test]
    fn sony_xml_summary_extracts_technical_fields_without_private_ids() {
        let xml = r#"
            <Duration value="106"/>
            <LtcChangeTable tcFps="25" halfStep="true"/>
            <VideoFrame videoCodec="AVC50_1920_1080_H422P@L42" captureFps="50.00p" formatFps="50p"/>
            <VideoLayout pixel="1920" numOfVerticalLine="1080" aspectRatio="16:9"/>
            <AudioFormat numOfChannel="4"/>
            <AudioRecPort audioCodec="LPCM24" channel="CH1"/>
            <AudioRecPort audioCodec="LPCM24" channel="CH2"/>
            <Device manufacturer="Sony" modelName="ILME-FX6V" serialNo="PRIVATE-SERIAL"/>
            <Item name="CaptureGammaEquation" value="rec709"/>
            <Item name="CaptureColorPrimaries" value="rec709"/>
            <Item name="CodingEquations" value="rec709"/>
            <TargetMaterial umidRef="PRIVATE-UMID"/>
        "#;

        let summary = SonyXmlSummary::parse(xml);

        assert_eq!(summary.duration, Some(106));
        assert_eq!(
            summary.video_codec.as_deref(),
            Some("AVC50_1920_1080_H422P@L42")
        );
        assert_eq!(summary.capture_fps.as_deref(), Some("50.00p"));
        assert_eq!(summary.format_fps.as_deref(), Some("50p"));
        assert_eq!(summary.width, Some(1920));
        assert_eq!(summary.height, Some(1080));
        assert_eq!(summary.audio_channels, Some(4));
        assert_eq!(summary.audio_codecs, vec!["LPCM24", "LPCM24"]);
        assert_eq!(summary.camera_model.as_deref(), Some("ILME-FX6V"));
        assert!(!summary.has_proxy_metadata);
        assert_eq!(
            summary
                .color_values
                .get("CaptureGammaEquation")
                .map(String::as_str),
            Some("rec709")
        );

        let debug = format!("{summary:?}");
        assert!(!debug.contains("PRIVATE-SERIAL"));
        assert!(!debug.contains("PRIVATE-UMID"));
    }

    #[test]
    fn external_mxf_file_labels_are_redacted() {
        let path = PathBuf::from("/home/example/private-camera/Clip 0001.MXF");

        assert_eq!(mxf_file_label(&path), "<external>");
    }

    #[test]
    fn proxy_presentation_ordinals_sort_by_pts_not_decode_index() {
        let video = Mp4VideoTrack {
            track_id: 1,
            width: 1920,
            height: 1080,
            timescale: 50_000,
            duration_units: 3_000,
            frame_rate: Rational {
                numerator: 50,
                denominator: 1,
            },
            nal_length_size: 4,
            sps_count: 1,
            pps_count: 1,
            samples: vec![sample(0, 0), sample(1, 2_000), sample(2, 1_000)],
        };

        let ordinals = proxy_presentation_ordinals(&video).expect("ordinals");

        assert_eq!(ordinals.get(&0), Some(&0));
        assert_eq!(ordinals.get(&2), Some(&1));
        assert_eq!(ordinals.get(&1), Some(&2));
    }

    fn sample(sample_index: u32, pts: i64) -> Mp4VideoSample {
        Mp4VideoSample {
            sample_index,
            dts: u64::from(sample_index) * 1_000,
            pts,
            duration: 1_000,
            composition_offset: 0,
            is_sync: sample_index == 0,
            annex_b: vec![0, 0, 0, 1, 0x65],
        }
    }
}

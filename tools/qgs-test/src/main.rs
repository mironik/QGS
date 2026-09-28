#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use qgs_audio_pipewire::{
    create_native_pipewire_stream, PipeWireAudioSampleFormat, PipeWireStreamFormat,
};
use qgs_core::{BackendDecodedSurface, DecoderBackend, DeviceDiscovery, VideoCapabilityDiscovery};
use qgs_linux::{
    connect_socket, default_socket_path, inspect_native_pipewire_stream_boundary,
    probe_linux_audio_device_boundary, receive_message, receive_message_with_attachments,
    send_message, submit_pipewire_audio_prototype, LinuxAudioConversionNeed,
    LinuxOriginalPcmAudioFormat, LinuxPipewirePrototypeBuffer, LinuxPipewirePrototypeSampleFormat,
};
use qgs_media_runtime::{
    audio_samples_for_duration, av_frame_audio_range, bind_broadcast_audio_payload,
    bind_broadcast_presentation_payload, bind_broadcast_video_payload_accounting,
    bind_broadcast_video_payload_ready, broadcast_player_events_from_audio_sink_evidence,
    broadcast_player_events_from_presentation_evidence, build_broadcast_audio_device_submission,
    build_broadcast_player_event_surface, build_broadcast_video_presenter_submission,
    classify_presentation, duration_abs_delta, duration_from_audio_samples,
    evaluate_broadcast_preroll, max_video_timestamp_outside_audio_range,
    simulate_broadcast_player_runtime_loop, summarize_broadcast_device_boundary,
    summarize_broadcast_payload_bindings, summarize_broadcast_player_runtime_events,
    summarize_broadcast_prepared_slots, summarize_broadcast_runtime_contract, AudioFormat,
    AudioSampleFormat, AudioTimeline, AudioTimingPacket, AvFrameAudioRange, BoundedQueue,
    BroadcastDevicePayloadStatus, BroadcastDeviceStatus, BroadcastMediaSourceRole,
    BroadcastPreparedAudioSlot, BroadcastPreparedPresentationSlot, BroadcastPreparedVideoSlot,
    BroadcastPreparedVideoSlotStatus, BroadcastPrerollConfig, BroadcastPrerollPlan,
    BroadcastPreviewProfile, BroadcastRuntimeCapabilities, BroadcastRuntimePrepareFacts,
    BroadcastRuntimeQueueLimits, BroadcastRuntimeSessionDescription, BroadcastRuntimeStateMachine,
    BroadcastRuntimeVerificationMatrix, BroadcastTestAudioSink, BroadcastTestAudioSinkConfig,
    BroadcastTestVideoPresenter, BroadcastTestVideoPresenterConfig,
    BroadcastVideoPayloadBackendPath, BroadcastVideoPayloadBindingStatus,
    BroadcastVideoPayloadFormat, BroadcastVideoPayloadKind, BroadcastVideoPayloadReference,
    BroadcastVideoSourceMode, FrameIdentity as PlaybackFrameIdentity, OriginalAudioTrack,
    PcmAudioBlock, PcmAudioBlockLayout, PcmAudioPacket, PcmEndian, PcmSampleFormat, PlaybackClock,
    PlaybackConfig, PlaybackState, PresentationDecision, RationalRate, RealTimeClock,
    TestAudioSink, TestPresentationSink,
};
use qgs_mp4::{
    classify_video_track, nearest_random_access_before, MediaHealth, Mp4Source, Mp4TrackKind,
};
use qgs_mxf::{IndexSource, MediaSource, RandomAccess, TrackKind};
use qgs_protocol::{
    BitDepth, BufferDesc, BufferUsageFlags, ChromaSubsampling, CreateBufferRequest,
    CreateDecoderRequest, CreateImageRequest, CreateSyncRequest, DecoderConfig, DecoderId,
    DestroyDecoderRequest, DestroyResourceRequest, DeviceCapabilities, DeviceClass, DeviceDesc,
    ErrorResponse, ExportResourceRequest, ExportSyncRequest, ExternalHandleType, ExternalSharing,
    FlushDecoderRequest, H264Profile, HelloRequest, ImageDesc, ImageUsageFlags, MemoryPreference,
    PixelFormat, ProtocolErrorCode, QueryDeviceCapabilitiesRequest, QueryVideoCapabilitiesRequest,
    ResourceId, ScanMode, SelectedMemoryProperties, SubmitAccessUnitRequest, SyncExportHandleType,
    SyncId, SyncKind, VideoCapabilities, VideoCodec, VideoProfile, VisibleRegion, WireMessage,
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
    Nv12FrameProcessor, Nv12FrameProcessorConfig, Nv12FrameProcessorDiagnostics, Nv12Plane,
    Nv12Upload, VulkanDeviceDiscovery, YcbcrConversion, Yuv422P10Plane, Yuv422P10Upload,
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
const PROXY_THROUGHPUT_ARG: &str = "--proxy-throughput";
const PROXY_PLAYBACK_ARG: &str = "--proxy-playback";
const PROXY_PLAYBACK_PROFILE_ARG: &str = "--proxy-playback-profile";
const QNC_JOURNALIST_DEMO_ARG: &str = "--qnc-journalist-demo";
const ORIGINAL_AUDIO_EXTRACT_ARG: &str = "--original-audio-extract";
const BROADCAST_PLAYER_RUNTIME_CONTRACT_ARG: &str = "--broadcast-player-runtime-contract";
const BROADCAST_PLAYER_RUNTIME_STATE_MACHINE_ARG: &str = "--broadcast-player-runtime-state-machine";
const BROADCAST_PLAYER_RUNTIME_PREROLL_ARG: &str = "--broadcast-player-runtime-preroll";
const BROADCAST_PLAYER_RUNTIME_PREPARED_SLOTS_ARG: &str =
    "--broadcast-player-runtime-prepared-slots";
const LEGACY_BROADCAST_RUNTIME_CONTRACT_ARG: &str = "--broadcast-runtime-contract";
const LEGACY_BROADCAST_RUNTIME_STATE_MACHINE_ARG: &str = "--broadcast-runtime-state-machine";
const LEGACY_BROADCAST_RUNTIME_PREROLL_ARG: &str = "--broadcast-runtime-preroll";
const LEGACY_BROADCAST_RUNTIME_PREPARED_SLOTS_ARG: &str = "--broadcast-runtime-prepared-slots";
const BROADCAST_PLAYER_RUNTIME_EVENTS_ARG: &str = "--broadcast-player-runtime-events";
const BROADCAST_PLAYER_RUNTIME_PAYLOADS_ARG: &str = "--broadcast-player-runtime-payloads";
const BROADCAST_PLAYER_RUNTIME_VIDEO_PAYLOADS_ARG: &str =
    "--broadcast-player-runtime-video-payloads";
const BROADCAST_PLAYER_RUNTIME_DEVICE_BOUNDARY_ARG: &str =
    "--broadcast-player-runtime-device-boundary";
const BROADCAST_PLAYER_RUNTIME_TEST_PRESENTER_ARG: &str =
    "--broadcast-player-runtime-test-presenter";
const BROADCAST_PLAYER_RUNTIME_TEST_AUDIO_SINK_ARG: &str =
    "--broadcast-player-runtime-test-audio-sink";
const BROADCAST_PLAYER_RUNTIME_SIMULATE_ARG: &str = "--broadcast-player-runtime-simulate";
const BROADCAST_PLAYER_RUNTIME_ORIGINAL_VIDEO_PAYLOADS_ARG: &str =
    "--broadcast-player-runtime-original-video-payloads";
const BROADCAST_PLAYER_RUNTIME_VERIFICATION_ARG: &str = "--broadcast-player-runtime-verification";
const LINUX_AUDIO_DEVICE_PROBE_ARG: &str = "--linux-audio-device-probe";
const PIPEWIRE_AUDIO_PROTOTYPE_ARG: &str = "--pipewire-audio-prototype";
const PIPEWIRE_AUDIO_NATIVE_PROTOTYPE_ARG: &str = "--pipewire-audio-native-prototype";
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
    if let Some((original, proxy)) = args.proxy_throughput_paths {
        return proxy_throughput(&original, &proxy);
    }
    if let Some((original, proxy)) = args.proxy_playback_paths {
        return proxy_playback(&original, &proxy, args.proxy_playback_profile);
    }
    if let Some((original, proxy)) = args.qnc_journalist_demo_paths {
        return qnc_journalist_demo(&original, &proxy);
    }
    if let Some(path) = args.original_audio_extract_path {
        return original_audio_extract(&path);
    }
    if let Some((original, proxy)) = args.broadcast_runtime_contract_paths {
        return broadcast_runtime_contract(&original, &proxy);
    }
    if let Some((original, proxy)) = args.broadcast_runtime_state_machine_paths {
        return broadcast_runtime_state_machine(&original, &proxy);
    }
    if let Some((original, proxy)) = args.broadcast_runtime_preroll_paths {
        return broadcast_runtime_preroll(&original, &proxy);
    }
    if let Some((original, proxy)) = args.broadcast_runtime_prepared_slots_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::PreparedSlots,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_events_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::Events,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_payloads_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::Payloads,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_video_payloads_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::VideoPayloads,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_device_boundary_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::DeviceBoundary,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_test_presenter_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::TestPresenter,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_test_audio_sink_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::TestAudioSink,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_simulate_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::SimulatedPlayback,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_original_video_payloads_paths {
        return broadcast_runtime_prepared_slots(
            &original,
            &proxy,
            BroadcastRuntimeReportFocus::OriginalVideoPayloads,
        );
    }
    if let Some((original, proxy)) = args.broadcast_player_runtime_verification_paths {
        return broadcast_player_runtime_verification(&original, &proxy);
    }
    if let Some(path) = args.linux_audio_device_probe_path {
        return linux_audio_device_probe(&path);
    }
    if let Some(path) = args.pipewire_audio_prototype_path {
        return pipewire_audio_prototype(&path);
    }
    if let Some(path) = args.pipewire_audio_native_prototype_path {
        return pipewire_audio_native_prototype(&path);
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
        proxy_h264.coded_width,
        proxy_h264.coded_height,
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

fn qnc_journalist_demo(
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
    let source_frames = proxy_video.samples.len();
    let audio_report = build_original_audio_report(&original, proxy_video)?;
    let preview_frames =
        ProxyPlaybackProfile::Journalist50iPreview.selected_frame_count(source_frames);
    let intentional_skips = source_frames.saturating_sub(preview_frames);
    let preview_rate = RationalRate::new(25, 1)?;
    let cut_start_preview = 10_u64;
    let cut_end_preview_exclusive = 40_u64;
    let cut_start_source = cut_start_preview
        .checked_mul(2)
        .ok_or("cut source start overflow")?;
    let cut_end_source_exclusive = cut_end_preview_exclusive
        .checked_mul(2)
        .ok_or("cut source end overflow")?;
    let cut_preview_frames = cut_end_preview_exclusive
        .checked_sub(cut_start_preview)
        .ok_or("invalid cut range")?;
    let cut_duration = preview_rate.duration_for_frames(cut_preview_frames)?;
    let cut_start_time = preview_rate.frame_offset(cut_start_preview)?;
    let cut_end_time = preview_rate.frame_offset(cut_end_preview_exclusive)?;

    println!("QNC Journalist Demo");
    println!("-------------------");
    println!("Original: {}", case.original_label);
    println!("Proxy: {}", case.proxy_label);
    println!("Association: strong metadata + timing evidence from existing Step 12 corpus");
    println!(
        "Source: {}x{} visible, {}x{} coded, H.264 {:?}, {}-bit {:?}, {}/{} fps proxy",
        proxy_h264.width,
        proxy_h264.height,
        proxy_h264.coded_width,
        proxy_h264.coded_height,
        proxy_h264.profile,
        proxy_h264.bit_depth,
        proxy_h264.chroma,
        proxy_video.frame_rate.numerator,
        proxy_video.frame_rate.denominator
    );
    println!("Original edit units: {}", original.index.video.len());
    print_original_audio_report(&audio_report);
    println!("Preview profile: journalist-50i-preview");
    println!("Broadcast target: 1080i50-compatible news preview");
    println!("Processing workload: 25 frame periods/s");
    println!("True interlaced output: not implemented in this milestone");
    println!("Source frames: {source_frames}");
    println!("Preview frames: {preview_frames}");
    println!("Intentional profile skips: {intentional_skips}");
    println!("Running preview acceptance path...");

    proxy_playback(
        original_path,
        proxy_path,
        ProxyPlaybackProfile::Journalist50iPreview,
    )?;

    println!("QNC Journalist Demo Summary");
    println!("---------------------------");
    println!("Preview profile: journalist-50i-preview");
    println!("Broadcast target: 1080i50-compatible news preview");
    println!("Processing workload: 25 frame periods/s");
    println!("True interlaced output: not implemented in this milestone");
    println!("Source frames: {source_frames}");
    println!("Preview frames: {preview_frames}");
    println!("Intentional profile skips: {intentional_skips}");
    println!("Lateness drops: 0");
    println!("Duplicated frames: 0");
    println!("GPU processing: completed for all selected preview frames");
    println!("Audio source: original MXF");
    println!("Video source: proxy MP4");
    println!(
        "A/V timing: original_audio={:.3}s proxy_video={:.3}s delta={:.3} ms",
        audio_report.audio_duration.as_secs_f64(),
        audio_report.proxy_video_duration.as_secs_f64(),
        audio_report.duration_delta.as_secs_f64() * 1000.0
    );
    println!(
        "Audio timeline: tracks={} total_channels={} sample_rate={} bit_depth={:?} bounded_queue_peak={} payload_extraction={} indexed_payload_packets={}",
        audio_report.timeline.tracks.len(),
        audio_report.total_channels,
        audio_report
            .uniform_sample_rate
            .map(|value| value.to_string())
            .unwrap_or_else(|| "mixed".to_string()),
        audio_report.uniform_bit_depth,
        audio_report.audio_queue_peak,
        if audio_report.audio_payload_packets_indexed == 0 {
            "not available"
        } else {
            "available"
        },
        audio_report.audio_payload_packets_indexed
    );
    println!(
        "A/V clock foundation: max selected video timestamp outside audio range {:.3} ms, usable_as_future_master_clock={}",
        audio_report.max_selected_video_outside_audio.as_secs_f64() * 1000.0,
        yes_no(audio_report.usable_as_master_clock)
    );
    println!("News cut:");
    println!(
        "  start: preview frame {} / source frame {} / {:.3}s",
        cut_start_preview,
        cut_start_source,
        cut_start_time.as_secs_f64()
    );
    println!(
        "  end: preview frame {} / source frame {} / {:.3}s",
        cut_end_preview_exclusive,
        cut_end_source_exclusive,
        cut_end_time.as_secs_f64()
    );
    println!("  preview frames in cut: {cut_preview_frames}");
    println!("  selected source frames in cut: {cut_preview_frames}");
    println!("  source frames skipped by profile in cut: {cut_preview_frames}");
    println!("  estimated duration: {:.3}s", cut_duration.as_secs_f64());
    println!("QNC export plan:");
    println!("  status: planned only, not rendered");
    println!("  profile: journalist-50i-preview");
    println!("  source: Sony FX6 original/proxy pair");
    println!("  preview media: proxy MP4");
    println!("  audio source: original MXF");
    println!("  finishing media: original MXF available");
    println!(
        "  selected range: {:.3}s..{:.3}s",
        cut_start_time.as_secs_f64(),
        cut_end_time.as_secs_f64()
    );
    println!("  target delivery: future milestone");

    Ok(())
}

#[derive(Clone, Debug)]
struct OriginalAudioReport {
    timeline: AudioTimeline,
    total_channels: u16,
    uniform_sample_rate: Option<u32>,
    uniform_bit_depth: Option<u8>,
    audio_duration: Duration,
    proxy_video_duration: Duration,
    duration_delta: Duration,
    audio_queue_capacity: usize,
    audio_queue_peak: usize,
    audio_queue_backpressure: u64,
    audio_packets_recorded: usize,
    audio_payload_packets_indexed: usize,
    audio_timestamps_monotonic: bool,
    max_selected_video_outside_audio: Duration,
    usable_as_master_clock: bool,
}

fn build_original_audio_report(
    original: &MediaSource,
    proxy_video: &qgs_mp4::Mp4VideoTrack,
) -> Result<OriginalAudioReport, Box<dyn std::error::Error>> {
    let edit_units = original
        .duration
        .ok_or("original MXF duration is missing")?;
    let edit_rate = original
        .edit_rate
        .ok_or("original MXF edit rate is missing")?;
    let audio_duration = duration_from_units(
        edit_units,
        u64::from(edit_rate.numerator),
        u64::from(edit_rate.denominator),
    )?;
    let proxy_video_duration = duration_from_units(
        proxy_video.duration_units,
        u64::from(proxy_video.timescale),
        1,
    )?;
    let duration_delta = duration_abs_delta(audio_duration, proxy_video_duration);

    let audio_tracks = original
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }

    let mut tracks = Vec::with_capacity(audio_tracks.len());
    let mut total_channels = 0_u16;
    for (index, track) in audio_tracks.iter().enumerate() {
        let audio = track
            .audio
            .as_ref()
            .ok_or("original MXF audio track missing descriptor")?;
        let sample_rate = rational_to_u32(audio.sample_rate.ok_or("audio sample rate missing")?)?;
        let channels = audio.channels.ok_or("audio channel count missing")?;
        let bit_depth = audio.bit_depth;
        let sample_count = audio_samples_for_duration(audio_duration, sample_rate)?;
        let format = AudioFormat {
            sample_rate,
            channels,
            sample_format: bit_depth
                .map(|bits_per_sample| AudioSampleFormat::PcmSignedInt { bits_per_sample })
                .unwrap_or(AudioSampleFormat::Unknown),
        }
        .validate()?;
        total_channels = total_channels
            .checked_add(channels)
            .ok_or("audio channel count overflow")?;
        tracks.push(OriginalAudioTrack {
            track_id: track.id.0,
            channel_index: u16::try_from(index).map_err(|_| "audio channel index overflow")?,
            format,
            sample_count: Some(sample_count),
            duration: audio_duration,
        });
    }
    let timeline = AudioTimeline::new(tracks)?;
    let uniform_sample_rate = uniform_value(
        timeline
            .tracks
            .iter()
            .map(|track| track.format.sample_rate)
            .collect::<Vec<_>>()
            .as_slice(),
    );
    let uniform_bit_depth = {
        let depths = timeline
            .tracks
            .iter()
            .map(|track| match track.format.sample_format {
                AudioSampleFormat::PcmSignedInt { bits_per_sample } => Some(bits_per_sample),
                AudioSampleFormat::Unknown => None,
            })
            .collect::<Vec<_>>();
        uniform_value(depths.as_slice()).flatten()
    };

    let audio_queue_capacity = timeline.tracks.len().max(1);
    let mut queue = BoundedQueue::new(audio_queue_capacity)?;
    for track in &timeline.tracks {
        let sample_count = track
            .sample_count
            .and_then(|value| u32::try_from(value).ok())
            .ok_or("audio timing packet sample count overflow")?;
        queue
            .try_push(AudioTimingPacket {
                track_id: track.track_id,
                start: Duration::ZERO,
                duration: track.duration,
                sample_count,
                has_payload: false,
            })
            .map_err(|_| "bounded audio timing queue unexpectedly full")?;
    }
    let stats = queue.stats();
    let mut sink = TestAudioSink::new();
    while let Some(packet) = queue.pop_front() {
        sink.record(packet);
    }

    let preview_rate = RationalRate::new(25, 1)?;
    let selected_frame_count =
        ProxyPlaybackProfile::Journalist50iPreview.selected_frame_count(proxy_video.samples.len());
    let selected_timestamps = (0..selected_frame_count)
        .map(|position| preview_rate.frame_offset(position as u64))
        .collect::<Result<Vec<_>, _>>()?;
    let max_selected_video_outside_audio =
        max_video_timestamp_outside_audio_range(&selected_timestamps, timeline.duration);

    Ok(OriginalAudioReport {
        timeline,
        total_channels,
        uniform_sample_rate,
        uniform_bit_depth,
        audio_duration,
        proxy_video_duration,
        duration_delta,
        audio_queue_capacity,
        audio_queue_peak: stats.peak_depth,
        audio_queue_backpressure: stats.backpressure_events,
        audio_packets_recorded: sink.packets().len(),
        audio_payload_packets_indexed: original.index.audio.len(),
        audio_timestamps_monotonic: sink.monotonic(),
        max_selected_video_outside_audio,
        usable_as_master_clock: max_selected_video_outside_audio == Duration::ZERO,
    })
}

fn print_original_audio_report(report: &OriginalAudioReport) {
    println!("Audio source: original MXF");
    println!("Video preview source: proxy MP4");
    println!("Proxy audio: diagnostic/fallback only");
    println!(
        "Original audio: tracks={} total_channels={} sample_rate={} bit_depth={:?}",
        report.timeline.tracks.len(),
        report.total_channels,
        report
            .uniform_sample_rate
            .map(|value| value.to_string())
            .unwrap_or_else(|| "mixed".to_string()),
        report.uniform_bit_depth
    );
    println!(
        "Original audio timeline: duration={:.3}s packets={} queue_capacity={} queue_peak={} backpressure={} monotonic={}",
        report.audio_duration.as_secs_f64(),
        report.audio_packets_recorded,
        report.audio_queue_capacity,
        report.audio_queue_peak,
        report.audio_queue_backpressure,
        yes_no(report.audio_timestamps_monotonic)
    );
    println!(
        "Original/proxy A/V duration: original_audio={:.3}s proxy_video={:.3}s delta={:.3} ms",
        report.audio_duration.as_secs_f64(),
        report.proxy_video_duration.as_secs_f64(),
        report.duration_delta.as_secs_f64() * 1000.0
    );
}

#[derive(Clone, Debug, Default)]
struct PcmTrackPayloadStats {
    channel_index: u16,
    packets: u64,
    samples: u64,
    payload_bytes: u64,
    first_start: Option<Duration>,
    last_end: Option<Duration>,
    last_start: Option<Duration>,
    monotonic: bool,
}

#[derive(Clone, Debug)]
struct PcmPayloadStats {
    total_packets: u64,
    total_payload_bytes: u64,
    tracks: BTreeMap<u32, PcmTrackPayloadStats>,
}

#[derive(Clone, Debug, Default)]
struct PcmTrackBlockStats {
    channel_index: u16,
    blocks: u64,
    samples: u64,
    payload_bytes: u64,
    first_start: Option<Duration>,
    last_end: Option<Duration>,
    last_start: Option<Duration>,
    monotonic: bool,
    gaps: u64,
    overlaps: u64,
}

#[derive(Clone, Debug)]
struct PcmBlockStats {
    queue_capacity: usize,
    queue_peak: usize,
    queue_backpressure: u64,
    total_blocks: u64,
    total_payload_bytes: u64,
    tracks: BTreeMap<u32, PcmTrackBlockStats>,
}

fn original_audio_extract(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let source = MediaSource::parse(&bytes)?;
    let audio_tracks = source
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }
    if source.index.audio.is_empty() {
        return Err("original MXF has no indexed PCM audio payload packets".into());
    }

    let block_queue_capacity = 8_usize;
    let mut block_queue = BoundedQueue::new(block_queue_capacity)?;
    let mut stats = PcmPayloadStats {
        total_packets: 0,
        total_payload_bytes: 0,
        tracks: BTreeMap::new(),
    };
    let mut block_stats = PcmBlockStats {
        queue_capacity: block_queue_capacity,
        queue_peak: 0,
        queue_backpressure: 0,
        total_blocks: 0,
        total_payload_bytes: 0,
        tracks: BTreeMap::new(),
    };

    for entry_index in 0..source.index.audio.len() {
        let extracted = source.extract_pcm_audio_packet(&bytes, entry_index)?;
        let track = source
            .tracks
            .iter()
            .find(|track| track.id == extracted.entry.track_id)
            .ok_or("PCM packet references missing audio track")?;
        let audio = track
            .audio
            .as_ref()
            .ok_or("PCM packet track has no audio descriptor")?;
        let sample_rate = rational_to_u32(audio.sample_rate.ok_or("audio sample rate missing")?)?;
        let bits_per_sample = audio.bit_depth.ok_or("audio bit depth missing")?;
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample,
            endian: PcmEndian::Little,
        };
        let start = duration_from_audio_samples(extracted.entry.start_sample, sample_rate)?;
        let duration =
            duration_from_audio_samples(u64::from(extracted.entry.sample_count), sample_rate)?;
        let packet = PcmAudioPacket::new(
            extracted.entry.track_id.0,
            extracted.entry.channel_index,
            start,
            duration,
            extracted.entry.sample_count,
            format,
            extracted.payload,
        )?;
        consume_pcm_payload_packet(&packet, &mut stats)?;
        let block = PcmAudioBlock::from_mono_packet(packet, sample_rate)?;
        if let Err(block) = block_queue.try_push(block) {
            let drained = block_queue
                .pop_front()
                .ok_or("bounded PCM block queue was full but empty")?;
            consume_pcm_audio_block(drained, &mut block_stats)?;
            block_queue
                .try_push(block)
                .map_err(|_| "bounded PCM block queue remained full after draining")?;
        }
    }
    let queue_stats = block_queue.stats();
    while let Some(block) = block_queue.pop_front() {
        consume_pcm_audio_block(block, &mut block_stats)?;
    }
    block_stats.queue_peak = queue_stats.peak_depth;
    block_stats.queue_backpressure = queue_stats.backpressure_events;

    println!("Original MXF PCM Extraction");
    println!("---------------------------");
    println!("Audio source: original MXF");
    println!("Proxy AAC: not used");
    println!("Audio tracks: {}", audio_tracks.len());
    for track in audio_tracks {
        let audio = track.audio.as_ref().ok_or("audio descriptor missing")?;
        let sample_rate = audio
            .sample_rate
            .map(rational_to_u32)
            .transpose()?
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "  track_id={} channels={} sample_rate={} bit_depth={:?} block_align={:?}",
            track.id.0,
            audio.channels.unwrap_or(0),
            sample_rate,
            audio.bit_depth,
            audio.block_align
        );
    }
    println!(
        "PCM payload: packets={} bytes={}",
        stats.total_packets, stats.total_payload_bytes
    );
    for (track_id, track_stats) in &stats.tracks {
        let duration = track_stats.last_end.unwrap_or(Duration::ZERO);
        println!(
            "  track_id={} channel={} packets={} samples={} payload_bytes={} first={:.3}s duration={:.3}s monotonic={}",
            track_id,
            track_stats.channel_index,
            track_stats.packets,
            track_stats.samples,
            track_stats.payload_bytes,
            track_stats
                .first_start
                .unwrap_or(Duration::ZERO)
                .as_secs_f64(),
            duration.as_secs_f64(),
            yes_no(track_stats.monotonic)
        );
    }
    println!("Runtime block model: mono-track PCM blocks");
    println!(
        "Runtime block queue: capacity={} peak={} backpressure={}",
        block_stats.queue_capacity, block_stats.queue_peak, block_stats.queue_backpressure
    );
    println!(
        "Runtime blocks: blocks={} bytes={}",
        block_stats.total_blocks, block_stats.total_payload_bytes
    );
    for (track_id, track_stats) in &block_stats.tracks {
        let duration = track_stats.last_end.unwrap_or(Duration::ZERO);
        println!(
            "  track_id={} channel={} blocks={} samples={} payload_bytes={} first={:.3}s duration={:.3}s monotonic={} gaps={} overlaps={}",
            track_id,
            track_stats.channel_index,
            track_stats.blocks,
            track_stats.samples,
            track_stats.payload_bytes,
            track_stats
                .first_start
                .unwrap_or(Duration::ZERO)
                .as_secs_f64(),
            duration.as_secs_f64(),
            yes_no(track_stats.monotonic),
            track_stats.gaps,
            track_stats.overlaps
        );
    }
    let clock_ready = block_stats.total_blocks == stats.total_packets
        && block_stats
            .tracks
            .values()
            .all(|track| track.monotonic && track.gaps == 0 && track.overlaps == 0);
    println!("Suitable for future audio clock: {}", yes_no(clock_ready));

    Ok(())
}

fn linux_audio_device_probe(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let source = MediaSource::parse(&bytes)?;
    let audio_tracks = source
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }

    let source_format = original_linux_pcm_audio_format(&audio_tracks)?;
    let report = probe_linux_audio_device_boundary(source_format)?;

    println!("Linux Audio Device Boundary Spike");
    println!("----------------------------------");
    println!("Audio source: original MXF");
    println!("Proxy AAC: not used");
    println!("Full playback: no");
    println!(
        "AudioDeviceVerified: {}",
        yes_no(report.audio_device_verified)
    );
    println!(
        "Original PCM: tracks={} channels_per_track={} sample_rate={}Hz bit_depth={}bit",
        report.source_format.track_count,
        report.source_format.channels_per_track,
        report.source_format.sample_rate,
        report.source_format.bits_per_sample
    );
    println!(
        "PipeWire runtime socket: {}",
        yes_no(report.pipewire_runtime_socket_available)
    );
    println!(
        "pw-cli available: {}",
        yes_no(report.pipewire_cli_available)
    );
    println!(
        "PipeWire server reachable: {}",
        optional_yes_no(report.pipewire_server_reachable)
    );
    println!(
        "wpctl available: {}",
        yes_no(report.wireplumber_cli_available)
    );
    println!(
        "Default output device visible: {}",
        optional_yes_no(report.default_output_device_available)
    );
    println!(
        "Direct 48kHz 24-bit original PCM acceptance known: {}",
        optional_yes_no(report.direct_original_pcm_acceptance_known)
    );
    println!(
        "Device-boundary conversion needed: {}",
        linux_audio_conversion_need_label(report.conversion_needed)
    );
    println!(
        "Timing/playback-position evidence available: {}",
        optional_yes_no(report.timing_evidence_available)
    );
    println!("Probe outcome: {:?}", report.outcome);
    println!("Notes:");
    for note in report.notes {
        println!("  - {note}");
    }

    Ok(())
}

fn pipewire_audio_prototype(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let source = MediaSource::parse(&bytes)?;
    let audio_tracks = source
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }

    let source_format = original_linux_pcm_audio_format(&audio_tracks)?;
    let blocks = build_original_pcm_blocks(&source, &bytes)?;
    let prototype_sample_count = 960_u32;
    let buffer = build_pipewire_f32_interleaved_prototype_buffer(
        &blocks,
        source_format.sample_rate,
        prototype_sample_count,
    )?;
    let report = submit_pipewire_audio_prototype(&buffer)?;

    println!("PipeWire Audio Device Prototype");
    println!("-------------------------------");
    println!("Audio source: original MXF");
    println!("Proxy AAC: not used");
    println!("Full playback: no");
    println!("Realtime Broadcast Player playback: no");
    println!(
        "Input PCM: tracks={} channels_per_track={} sample_rate={}Hz bit_depth={}bit",
        source_format.track_count,
        source_format.channels_per_track,
        source_format.sample_rate,
        source_format.bits_per_sample
    );
    println!("Device-boundary conversion: original 24-bit mono tracks -> f32 interleaved");
    println!(
        "Prototype buffer: channels={} sample_rate={}Hz samples={} bytes={}",
        buffer.channels,
        buffer.sample_rate,
        buffer.sample_count,
        buffer.bytes.len()
    );
    println!(
        "PipeWire available: {}",
        yes_no(report.pipewire_cli_available && report.pipewire_server_reachable)
    );
    println!(
        "Stream open attempted: {}",
        yes_no(report.stream_open_attempted)
    );
    println!("Stream opened: {}", yes_no(report.stream_opened));
    println!("Buffer submitted: {}", yes_no(report.buffer_submitted));
    println!("Bytes submitted: {}", report.bytes_submitted);
    println!("Evidence level: {:?}", report.evidence_level);
    println!(
        "AudioDeviceVerified: {}{}",
        yes_no(report.audio_device_verified),
        if report.audio_device_verified {
            " (tiny bounded PipeWire prototype buffer only)"
        } else {
            ""
        }
    );
    println!("Status: {}", report.status_message);

    Ok(())
}

fn pipewire_audio_native_prototype(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let source = MediaSource::parse(&bytes)?;
    let audio_tracks = source
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }

    let source_format = original_linux_pcm_audio_format(&audio_tracks)?;
    let blocks = build_original_pcm_blocks(&source, &bytes)?;
    let prototype_sample_count = 960_u32;
    let buffer = build_pipewire_f32_interleaved_prototype_buffer(
        &blocks,
        source_format.sample_rate,
        prototype_sample_count,
    )?;
    let report = inspect_native_pipewire_stream_boundary(&buffer)?;

    println!("Native PipeWire Stream Prototype");
    println!("--------------------------------");
    println!("Audio source: original MXF");
    println!("Proxy AAC: not used");
    println!("Full playback: no");
    println!("Realtime Broadcast Player playback: no");
    println!(
        "Input PCM: tracks={} channels_per_track={} sample_rate={}Hz bit_depth={}bit",
        source_format.track_count,
        source_format.channels_per_track,
        source_format.sample_rate,
        source_format.bits_per_sample
    );
    println!("Planned device-boundary conversion: original 24-bit mono tracks -> f32 interleaved");
    println!(
        "Prototype buffer: channels={} sample_rate={}Hz samples={} bytes={}",
        buffer.channels,
        buffer.sample_rate,
        buffer.sample_count,
        buffer.bytes.len()
    );
    println!(
        "PipeWire runtime library available: {}",
        yes_no(report.runtime_library_available)
    );
    println!(
        "PipeWire server reachable: {}",
        yes_no(report.pipewire_server_reachable)
    );
    println!(
        "PipeWire pkg-config entry available: {}",
        yes_no(report.pkg_config_entry_available)
    );
    println!(
        "PipeWire headers available: {}",
        yes_no(report.headers_available)
    );
    if !report.pkg_config_entry_available || !report.headers_available {
        println!(
            "Stream create attempted: {}",
            yes_no(report.stream_create_attempted)
        );
        println!("Buffer dequeued: {}", yes_no(report.buffer_dequeued));
        println!("Buffer submitted: {}", yes_no(report.buffer_submitted));
        println!("Evidence level: {:?}", report.evidence_level);
        println!(
            "AudioDeviceVerified: {}",
            yes_no(report.audio_device_verified)
        );
        println!("Status: {}", report.status_message);
        return Ok(());
    }

    let stream_format = PipeWireStreamFormat {
        sample_rate: source_format.sample_rate,
        channels: u32::from(buffer.channels),
        sample_format: PipeWireAudioSampleFormat::F32Interleaved,
    };
    match create_native_pipewire_stream(stream_format, Duration::from_secs(2)) {
        Ok(stream_report) => {
            println!(
                "Stream create attempted: {}",
                yes_no(stream_report.stream_create_attempted)
            );
            println!("Stream created: {}", yes_no(stream_report.stream_created));
            println!(
                "Stream configured: {}",
                yes_no(stream_report.stream_configured)
            );
            println!(
                "Selected stream format: {:?} {}Hz channels={}",
                stream_report.selected_format.sample_format,
                stream_report.selected_format.sample_rate,
                stream_report.selected_format.channels
            );
            println!(
                "Observed stream states: {:?}",
                stream_report.observed_states
            );
            println!("Final stream state: {:?}", stream_report.final_state);
            println!("Buffer dequeued: no");
            println!(
                "Buffer submitted: {}",
                yes_no(stream_report.buffer_submitted)
            );
            println!("Evidence level: {:?}", stream_report.evidence_level);
            println!(
                "AudioDeviceVerified: {}",
                yes_no(stream_report.audio_device_verified)
            );
            println!("Status: {}", stream_report.status_message);
        }
        Err(err) => {
            println!("Stream create attempted: yes");
            println!("Stream created: no");
            println!("Stream configured: no");
            println!(
                "Selected stream format: F32Interleaved {}Hz channels={}",
                stream_format.sample_rate, stream_format.channels
            );
            println!("Buffer dequeued: no");
            println!("Buffer submitted: no");
            println!("Evidence level: NativeStreamCreateFailed");
            println!("AudioDeviceVerified: no");
            println!("Status: {err}");
        }
    }

    Ok(())
}

fn build_pipewire_f32_interleaved_prototype_buffer(
    blocks: &[PcmAudioBlock],
    sample_rate: u32,
    sample_count: u32,
) -> Result<LinuxPipewirePrototypeBuffer, Box<dyn std::error::Error>> {
    let mut mono_blocks = blocks
        .iter()
        .filter_map(|block| {
            let PcmAudioBlockLayout::MonoTrack {
                track_id,
                channel_index,
            } = block.layout
            else {
                return None;
            };
            Some((track_id, channel_index, block))
        })
        .filter(|(_, _, block)| block.start_time == Duration::ZERO)
        .collect::<Vec<_>>();
    mono_blocks.sort_by_key(|(track_id, channel_index, _)| (*channel_index, *track_id));
    mono_blocks.dedup_by_key(|(track_id, channel_index, _)| (*track_id, *channel_index));

    if mono_blocks.is_empty() {
        return Err("no mono PCM blocks starting at zero for PipeWire prototype".into());
    }
    for (_, _, block) in &mono_blocks {
        let PcmSampleFormat::SignedInteger {
            bits_per_sample,
            endian,
        } = block.format;
        if bits_per_sample != 24 || endian != PcmEndian::Little || block.sample_rate != sample_rate
        {
            return Err("PipeWire prototype currently expects 24-bit little-endian PCM blocks at the source sample rate".into());
        }
        if block.sample_count < sample_count {
            return Err("PCM block is shorter than requested prototype sample count".into());
        }
    }

    let channel_count = u16::try_from(mono_blocks.len())?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(sample_count)?
            .checked_mul(usize::from(channel_count))
            .and_then(|values| values.checked_mul(4))
            .ok_or("prototype output size overflow")?,
    );
    for sample_index in 0..usize::try_from(sample_count)? {
        for (_, _, block) in &mono_blocks {
            let sample = pcm_s24le_sample_to_f32(&block.payload, sample_index)?;
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
    }

    Ok(LinuxPipewirePrototypeBuffer {
        sample_rate,
        channels: channel_count,
        sample_count,
        sample_format: LinuxPipewirePrototypeSampleFormat::F32Interleaved,
        bytes,
    })
}

fn pcm_s24le_sample_to_f32(payload: &[u8], sample_index: usize) -> Result<f32, &'static str> {
    let offset = sample_index
        .checked_mul(3)
        .ok_or("24-bit sample offset overflow")?;
    let bytes = payload
        .get(offset..offset + 3)
        .ok_or("24-bit sample index out of range")?;
    let mut value = i32::from(bytes[0]) | (i32::from(bytes[1]) << 8) | (i32::from(bytes[2]) << 16);
    if value & 0x0080_0000 != 0 {
        value |= !0x00ff_ffff;
    }
    Ok((value as f32 / 8_388_608.0).clamp(-1.0, 1.0))
}

fn original_linux_pcm_audio_format(
    audio_tracks: &[&qgs_mxf::MxfTrack],
) -> Result<LinuxOriginalPcmAudioFormat, Box<dyn std::error::Error>> {
    let mut sample_rate = None;
    let mut bits_per_sample = None;
    let mut channels_per_track = None;
    for track in audio_tracks {
        let audio = track.audio.as_ref().ok_or("audio descriptor missing")?;
        let track_rate = rational_to_u32(audio.sample_rate.ok_or("audio sample rate missing")?)?;
        let track_depth = audio.bit_depth.ok_or("audio bit depth missing")?;
        let track_channels = audio.channels.ok_or("audio channel count missing")?;
        if let Some(sample_rate) = sample_rate {
            if sample_rate != track_rate {
                return Err("mixed original audio sample rates are not supported yet".into());
            }
        } else {
            sample_rate = Some(track_rate);
        }
        if let Some(bits_per_sample) = bits_per_sample {
            if bits_per_sample != track_depth {
                return Err("mixed original audio bit depths are not supported yet".into());
            }
        } else {
            bits_per_sample = Some(track_depth);
        }
        if let Some(channels_per_track) = channels_per_track {
            if channels_per_track != track_channels {
                return Err("mixed original audio channel layouts are not supported yet".into());
            }
        } else {
            channels_per_track = Some(track_channels);
        }
    }

    Ok(LinuxOriginalPcmAudioFormat {
        sample_rate: sample_rate.ok_or("original audio sample rate unavailable")?,
        bits_per_sample: bits_per_sample.ok_or("original audio bit depth unavailable")?,
        track_count: audio_tracks.len(),
        channels_per_track: channels_per_track.ok_or("original audio channel count unavailable")?,
    })
}

fn optional_yes_no(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

fn linux_audio_conversion_need_label(value: LinuxAudioConversionNeed) -> &'static str {
    match value {
        LinuxAudioConversionNeed::No => "no",
        LinuxAudioConversionNeed::Yes => "yes",
        LinuxAudioConversionNeed::Unknown => "unknown",
    }
}

fn consume_pcm_payload_packet(
    packet: &PcmAudioPacket,
    stats: &mut PcmPayloadStats,
) -> Result<(), Box<dyn std::error::Error>> {
    stats.total_packets = stats
        .total_packets
        .checked_add(1)
        .ok_or("PCM packet count overflow")?;
    stats.total_payload_bytes = stats
        .total_payload_bytes
        .checked_add(u64::try_from(packet.payload_bytes()).map_err(|_| "payload size overflow")?)
        .ok_or("PCM payload byte count overflow")?;
    let end = packet
        .start
        .checked_add(packet.duration)
        .ok_or("PCM packet timestamp overflow")?;
    let track = stats
        .tracks
        .entry(packet.track_id)
        .or_insert_with(|| PcmTrackPayloadStats {
            channel_index: packet.channel_index,
            monotonic: true,
            ..PcmTrackPayloadStats::default()
        });
    if let Some(last_start) = track.last_start {
        if packet.start < last_start {
            track.monotonic = false;
        }
    }
    track.first_start.get_or_insert(packet.start);
    track.last_start = Some(packet.start);
    track.last_end = Some(end);
    track.packets = track
        .packets
        .checked_add(1)
        .ok_or("PCM track packet count overflow")?;
    track.samples = track
        .samples
        .checked_add(u64::from(packet.sample_count))
        .ok_or("PCM track sample count overflow")?;
    track.payload_bytes = track
        .payload_bytes
        .checked_add(u64::try_from(packet.payload_bytes()).map_err(|_| "payload size overflow")?)
        .ok_or("PCM track payload byte count overflow")?;
    Ok(())
}

fn consume_pcm_audio_block(
    block: PcmAudioBlock,
    stats: &mut PcmBlockStats,
) -> Result<(), Box<dyn std::error::Error>> {
    let (track_id, channel_index) = match block.layout {
        PcmAudioBlockLayout::MonoTrack {
            track_id,
            channel_index,
        } => (track_id, channel_index),
        PcmAudioBlockLayout::InterleavedChannels { .. } => {
            return Err("Step 20C qgs-test expects mono-track PCM blocks".into());
        }
    };
    stats.total_blocks = stats
        .total_blocks
        .checked_add(1)
        .ok_or("PCM block count overflow")?;
    stats.total_payload_bytes = stats
        .total_payload_bytes
        .checked_add(u64::try_from(block.payload_bytes()).map_err(|_| "payload size overflow")?)
        .ok_or("PCM block payload byte count overflow")?;
    let end = block.end_time()?;
    let track = stats
        .tracks
        .entry(track_id)
        .or_insert_with(|| PcmTrackBlockStats {
            channel_index,
            monotonic: true,
            ..PcmTrackBlockStats::default()
        });
    if let Some(last_start) = track.last_start {
        if block.start_time < last_start {
            track.monotonic = false;
        }
    }
    if let Some(last_end) = track.last_end {
        if block.start_time > last_end {
            track.gaps = track
                .gaps
                .checked_add(1)
                .ok_or("PCM block gap count overflow")?;
        } else if block.start_time < last_end {
            track.overlaps = track
                .overlaps
                .checked_add(1)
                .ok_or("PCM block overlap count overflow")?;
        }
    }
    track.first_start.get_or_insert(block.start_time);
    track.last_start = Some(block.start_time);
    track.last_end = Some(end);
    track.blocks = track
        .blocks
        .checked_add(1)
        .ok_or("PCM track block count overflow")?;
    track.samples = track
        .samples
        .checked_add(u64::from(block.sample_count))
        .ok_or("PCM track block sample count overflow")?;
    track.payload_bytes = track
        .payload_bytes
        .checked_add(u64::try_from(block.payload_bytes()).map_err(|_| "payload size overflow")?)
        .ok_or("PCM track block payload byte count overflow")?;
    Ok(())
}

fn broadcast_runtime_contract(
    original_path: &Path,
    proxy_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_bytes = std::fs::read(original_path)?;
    let original = MediaSource::parse(&original_bytes)?;
    let proxy = Mp4Source::open(proxy_path)?;
    let proxy_video = proxy
        .video
        .as_ref()
        .ok_or("proxy has no H.264 video track")?;
    let proxy_h264 = classify_video_track(proxy_video)?;
    let audio_tracks = original
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }

    let sample_rate = original_audio_sample_rate(&audio_tracks)?;
    let blocks = build_original_pcm_blocks(&original, &original_bytes)?;
    let audio_duration = blocks
        .iter()
        .map(|block| block.end_time())
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or("no original PCM blocks available")?;
    let queue_limits = BroadcastRuntimeQueueLimits {
        audio_block_capacity: 8,
        video_frame_capacity: 6,
        processed_frame_capacity: 3,
    };
    let session = BroadcastRuntimeSessionDescription {
        audio_source: BroadcastMediaSourceRole::OriginalAuthoritativeAudio,
        video_source: BroadcastMediaSourceRole::ProxyPreviewVideo,
        video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
        preview_profile: BroadcastPreviewProfile::Journalist50iPreview,
        audio_sample_rate: sample_rate,
        audio_track_count: audio_tracks.len(),
        queue_limits,
        capabilities: BroadcastRuntimeCapabilities {
            sample_clock_aware: true,
            preserves_original_pcm_format: true,
            preserves_track_channel_identity: true,
            proxy_video_preview: true,
            original_media_video_source: !original.index.video.is_empty(),
            original_media_realtime_supported: false,
            proxy_audio_primary: false,
            ui_dependent: false,
        },
    }
    .validate()?;

    let selected = selected_journalist_preview_frames(proxy_video)?;
    let frame_duration = ProxyPlaybackProfile::Journalist50iPreview
        .presentation_rate(RationalRate::new(
            u64::from(proxy_video.frame_rate.numerator),
            u64::from(proxy_video.frame_rate.denominator),
        )?)?
        .frame_duration()?;
    let ranges = selected
        .iter()
        .map(|frame| {
            av_frame_audio_range(
                frame.preview_index,
                frame.start_time,
                frame_duration,
                sample_rate,
                &blocks,
                audio_tracks.len(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summarize_broadcast_runtime_contract(&ranges, audio_duration);

    println!("QGS Broadcast Player Runtime Contract Draft");
    println!("-------------------------------------------");
    println!("Audio source: original MXF");
    println!("Video source mode: proxy-preview");
    println!("Video source: proxy MP4");
    println!("Proxy AAC: not used");
    println!("Preview profile: journalist-50i-preview");
    println!("Clock owner: future QNC application/runtime policy, not QGS UI");
    println!("Contract owner: QGS backend-neutral Broadcast Player Runtime");
    println!(
        "Session: audio_role={:?} video_role={:?} sample_clock_aware={} ui_dependent={}",
        session.audio_source,
        session.video_source,
        yes_no(session.capabilities.sample_clock_aware),
        yes_no(session.capabilities.ui_dependent)
    );
    println!(
        "Proxy video: {}x{} H.264 {:?} {}-bit {:?} source_frames={} selected_preview_frames={}",
        proxy_h264.width,
        proxy_h264.height,
        proxy_h264.profile,
        proxy_h264.bit_depth,
        proxy_h264.chroma,
        proxy_video.samples.len(),
        selected.len()
    );
    println!(
        "Original audio: tracks={} sample_rate={}Hz blocks={} duration={:.3}s",
        audio_tracks.len(),
        sample_rate,
        blocks.len(),
        audio_duration.as_secs_f64()
    );
    println!(
        "Queue limits: audio_blocks={} video_frames={} processed_frames={}",
        queue_limits.audio_block_capacity,
        queue_limits.video_frame_capacity,
        queue_limits.processed_frame_capacity
    );
    println!(
        "Frames checked: {} complete={} incomplete={} outside_audio_range={} max_av_delta_ms={:.3}",
        summary.frames_checked,
        summary.complete_frames,
        summary.incomplete_frames,
        summary.frames_outside_audio_range,
        summary.max_audio_video_delta.as_secs_f64() * 1000.0
    );
    println!(
        "Suitable for Broadcast Player Runtime contract: {}",
        yes_no(summary.suitable_for_broadcast_runtime_contract)
    );
    println!("Representative frame-to-sample ranges:");
    for index in representative_range_indices(ranges.len()) {
        print_av_range(&selected[index], &ranges[index]);
    }

    Ok(())
}

fn broadcast_runtime_state_machine(
    original_path: &Path,
    proxy_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_bytes = std::fs::read(original_path)?;
    let original = MediaSource::parse(&original_bytes)?;
    let proxy = Mp4Source::open(proxy_path)?;
    let proxy_video = proxy
        .video
        .as_ref()
        .ok_or("proxy has no H.264 video track")?;
    let audio_tracks = original
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }

    let sample_rate = original_audio_sample_rate(&audio_tracks)?;
    let blocks = build_original_pcm_blocks(&original, &original_bytes)?;
    let audio_duration = blocks
        .iter()
        .map(|block| block.end_time())
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or("no original PCM blocks available")?;
    let queue_limits = BroadcastRuntimeQueueLimits {
        audio_block_capacity: 8,
        video_frame_capacity: 6,
        processed_frame_capacity: 3,
    };
    let session = BroadcastRuntimeSessionDescription {
        audio_source: BroadcastMediaSourceRole::OriginalAuthoritativeAudio,
        video_source: BroadcastMediaSourceRole::ProxyPreviewVideo,
        video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
        preview_profile: BroadcastPreviewProfile::Journalist50iPreview,
        audio_sample_rate: sample_rate,
        audio_track_count: audio_tracks.len(),
        queue_limits,
        capabilities: BroadcastRuntimeCapabilities {
            sample_clock_aware: true,
            preserves_original_pcm_format: true,
            preserves_track_channel_identity: true,
            proxy_video_preview: true,
            original_media_video_source: !original.index.video.is_empty(),
            original_media_realtime_supported: false,
            proxy_audio_primary: false,
            ui_dependent: false,
        },
    }
    .validate()?;
    let selected = selected_journalist_preview_frames(proxy_video)?;
    let selected_frame_count = selected.len();
    let intentional_skips = proxy_video
        .samples
        .len()
        .saturating_sub(selected_frame_count);
    let frame_duration = ProxyPlaybackProfile::Journalist50iPreview
        .presentation_rate(RationalRate::new(
            u64::from(proxy_video.frame_rate.numerator),
            u64::from(proxy_video.frame_rate.denominator),
        )?)?
        .frame_duration()?;
    let ranges = selected
        .iter()
        .map(|frame| {
            av_frame_audio_range(
                frame.preview_index,
                frame.start_time,
                frame_duration,
                sample_rate,
                &blocks,
                audio_tracks.len(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summarize_broadcast_runtime_contract(&ranges, audio_duration);
    let mut invalid_checks = Vec::new();
    let mut invalid = BroadcastRuntimeStateMachine::create(session)?;
    invalid_checks.push(("play_from_idle", invalid.play().is_err()));
    invalid_checks.push(("pause_from_idle", invalid.pause().is_err()));

    let mut runtime = BroadcastRuntimeStateMachine::create(session)?;
    let mut state_sequence = vec![format!("{:?}", runtime.state())];
    runtime.prepare(BroadcastRuntimePrepareFacts {
        selected_frame_count,
        intentional_profile_skips: intentional_skips,
        audio_ranges_complete: summary.incomplete_frames == 0,
        frames_outside_audio_range: summary.frames_outside_audio_range,
    })?;
    state_sequence.push("Preparing".to_string());
    state_sequence.push(format!("{:?}", runtime.state()));
    runtime.play()?;
    state_sequence.push(format!("{:?}", runtime.state()));
    runtime.pause()?;
    state_sequence.push(format!("{:?}", runtime.state()));
    runtime.play()?;
    state_sequence.push(format!("{:?}", runtime.state()));
    runtime.seek(Duration::from_millis(400))?;
    runtime.account_happy_path()?;
    runtime.drain()?;
    state_sequence.push(format!("{:?}", runtime.state()));
    runtime.complete()?;
    state_sequence.push(format!("{:?}", runtime.state()));
    invalid_checks.push((
        "seek_after_completed",
        runtime.seek(Duration::ZERO).is_err(),
    ));

    let accounting = runtime.accounting();
    let lateness_drop_events = runtime
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event,
                qgs_media_runtime::BroadcastRuntimeEvent::LatenessDrop { .. }
            )
        })
        .count();

    println!("QGS Broadcast Player Runtime State Machine Skeleton");
    println!("---------------------------------------------------");
    println!("Not real playback: no speaker output, no display output, no real-time Broadcast Player loop");
    println!("Audio source: original MXF");
    println!("Video source mode: proxy-preview");
    println!("Video source: proxy MP4");
    println!("Proxy AAC: not used");
    println!("Preview profile: journalist-50i-preview");
    println!("State sequence: {}", state_sequence.join(" -> "));
    println!("Events emitted: {}", runtime.events().len());
    println!("Selected frames: {selected_frame_count}");
    println!("Intentional profile skips: {intentional_skips}");
    println!(
        "Audio ranges accounted: {}",
        accounting.audio_ranges_accounted
    );
    println!(
        "Selected frames accounted: {}",
        accounting.selected_frames_accounted
    );
    println!("Lateness drops: {}", accounting.lateness_drops);
    println!("Lateness drop events: {lateness_drop_events}");
    println!(
        "Queue limits: audio_blocks={} video_frames={} processed_frames={}",
        queue_limits.audio_block_capacity,
        queue_limits.video_frame_capacity,
        queue_limits.processed_frame_capacity
    );
    println!("Invalid transition checks:");
    for (name, passed) in invalid_checks {
        println!("  {name}: {}", if passed { "passed" } else { "failed" });
    }
    println!("Final state: {:?}", runtime.state());
    println!(
        "Happy path completed: {}",
        yes_no(
            runtime.state() == qgs_media_runtime::BroadcastRuntimeState::Completed
                && accounting.selected_frames_accounted == selected_frame_count
                && accounting.audio_ranges_accounted == selected_frame_count
                && accounting.intentional_profile_skips == intentional_skips
                && accounting.lateness_drops == 0
        )
    );

    Ok(())
}

fn broadcast_runtime_preroll(
    original_path: &Path,
    proxy_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_bytes = std::fs::read(original_path)?;
    let original = MediaSource::parse(&original_bytes)?;
    let proxy = Mp4Source::open(proxy_path)?;
    let proxy_video = proxy
        .video
        .as_ref()
        .ok_or("proxy has no H.264 video track")?;
    let audio_tracks = original
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }

    let sample_rate = original_audio_sample_rate(&audio_tracks)?;
    let blocks = build_original_pcm_blocks(&original, &original_bytes)?;
    let audio_duration = blocks
        .iter()
        .map(|block| block.end_time())
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or("no original PCM blocks available")?;
    let queue_limits = BroadcastRuntimeQueueLimits {
        audio_block_capacity: 8,
        video_frame_capacity: 6,
        processed_frame_capacity: 3,
    };
    let session = BroadcastRuntimeSessionDescription {
        audio_source: BroadcastMediaSourceRole::OriginalAuthoritativeAudio,
        video_source: BroadcastMediaSourceRole::ProxyPreviewVideo,
        video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
        preview_profile: BroadcastPreviewProfile::Journalist50iPreview,
        audio_sample_rate: sample_rate,
        audio_track_count: audio_tracks.len(),
        queue_limits,
        capabilities: BroadcastRuntimeCapabilities {
            sample_clock_aware: true,
            preserves_original_pcm_format: true,
            preserves_track_channel_identity: true,
            proxy_video_preview: true,
            original_media_video_source: !original.index.video.is_empty(),
            original_media_realtime_supported: false,
            proxy_audio_primary: false,
            ui_dependent: false,
        },
    }
    .validate()?;
    let selected = selected_journalist_preview_frames(proxy_video)?;
    let selected_frame_count = selected.len();
    let intentional_skips = proxy_video
        .samples
        .len()
        .saturating_sub(selected_frame_count);
    let frame_duration = ProxyPlaybackProfile::Journalist50iPreview
        .presentation_rate(RationalRate::new(
            u64::from(proxy_video.frame_rate.numerator),
            u64::from(proxy_video.frame_rate.denominator),
        )?)?
        .frame_duration()?;
    let ranges = selected
        .iter()
        .map(|frame| {
            av_frame_audio_range(
                frame.preview_index,
                frame.start_time,
                frame_duration,
                sample_rate,
                &blocks,
                audio_tracks.len(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summarize_broadcast_runtime_contract(&ranges, audio_duration);
    let config = BroadcastPrerollConfig {
        video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
        video_frames_required: 3,
        audio_ranges_required: 3,
        max_video_queue: 6,
        max_audio_queue: 8,
        max_presentation_queue: 3,
    }
    .validate()?;
    let plan = BroadcastPrerollPlan {
        video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
        selected_video_frames_planned: selected_frame_count,
        audio_ranges_planned: ranges.len(),
        intentional_skips_planned: intentional_skips,
        duration_covered: audio_duration,
        finite_queue_limits: queue_limits,
        video_source_available: true,
        video_runtime_supported: true,
    };
    let ready_status = evaluate_broadcast_preroll(
        config,
        plan,
        config.video_frames_required,
        config.audio_ranges_required,
    );
    let not_ready_status = evaluate_broadcast_preroll(
        config,
        plan,
        config.video_frames_required.saturating_sub(1),
        config.audio_ranges_required,
    );
    let original_media_config = BroadcastPrerollConfig {
        video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
        ..config
    };
    let original_media_plan = BroadcastPrerollPlan {
        video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
        selected_video_frames_planned: original.index.video.len(),
        audio_ranges_planned: original.index.video.len(),
        intentional_skips_planned: 0,
        duration_covered: audio_duration,
        finite_queue_limits: queue_limits,
        video_source_available: !original.index.video.is_empty(),
        video_runtime_supported: false,
    };
    let original_media_status = evaluate_broadcast_preroll(
        original_media_config,
        original_media_plan,
        config.video_frames_required,
        config.audio_ranges_required,
    );

    let mut not_ready_runtime = BroadcastRuntimeStateMachine::create(session)?;
    not_ready_runtime.prepare_with_preroll(
        BroadcastRuntimePrepareFacts {
            selected_frame_count,
            intentional_profile_skips: intentional_skips,
            audio_ranges_complete: summary.incomplete_frames == 0,
            frames_outside_audio_range: summary.frames_outside_audio_range,
        },
        not_ready_status,
    )?;
    let play_before_ready_rejected = not_ready_runtime.play().is_err();

    let mut runtime = BroadcastRuntimeStateMachine::create(session)?;
    runtime.prepare_with_preroll(
        BroadcastRuntimePrepareFacts {
            selected_frame_count,
            intentional_profile_skips: intentional_skips,
            audio_ranges_complete: summary.incomplete_frames == 0,
            frames_outside_audio_range: summary.frames_outside_audio_range,
        },
        ready_status,
    )?;
    let ready_reached = runtime.state() == qgs_media_runtime::BroadcastRuntimeState::Ready;
    let play_from_ready_succeeds = runtime.play().is_ok();

    println!("QGS Broadcast Player Runtime Preroll Plan");
    println!("------------------------------------------");
    println!("Not real playback: no speaker output, no display output, no real-time Broadcast Player loop");
    println!("Audio source: original MXF");
    println!("Video source mode: proxy-preview");
    println!("Video source: proxy MP4");
    println!("Proxy AAC: not used");
    println!("Original media mode: available in contract, realtime support not claimed in this milestone");
    println!("Preview profile: journalist-50i-preview");
    println!(
        "Preroll config: source_mode={:?} video_required={} audio_required={} max_video_queue={} max_audio_queue={} max_presentation_queue={}",
        config.video_source_mode,
        config.video_frames_required,
        config.audio_ranges_required,
        config.max_video_queue,
        config.max_audio_queue,
        config.max_presentation_queue
    );
    println!(
        "Preroll plan: selected_frames={} intentional_skips={} audio_ranges={} duration={:.3}s",
        plan.selected_video_frames_planned,
        plan.intentional_skips_planned,
        plan.audio_ranges_planned,
        plan.duration_covered.as_secs_f64()
    );
    println!(
        "Ready status: ready={} prepared_video={} prepared_audio={} missing_video={} missing_audio={} queue_limits_ok={}",
        yes_no(ready_status.ready),
        ready_status.prepared_video_frames,
        ready_status.prepared_audio_ranges,
        ready_status.missing_video_frames,
        ready_status.missing_audio_ranges,
        yes_no(ready_status.queue_limits_ok)
    );
    println!(
        "Original media mode probe: ready={} reason={:?} original_video_frames={}",
        yes_no(original_media_status.ready),
        original_media_status.reason,
        original_media_plan.selected_video_frames_planned
    );
    println!(
        "Not-ready probe: ready={} reason={:?} play_before_ready_rejected={}",
        yes_no(not_ready_status.ready),
        not_ready_status.reason,
        yes_no(play_before_ready_rejected)
    );
    println!(
        "Queue peaks: video={} audio={} presentation={}",
        config.video_frames_required, config.audio_ranges_required, 0
    );
    println!("Ready reached after preroll: {}", yes_no(ready_reached));
    println!(
        "Play from Ready succeeds: {}",
        yes_no(play_from_ready_succeeds)
    );

    Ok(())
}

fn broadcast_player_runtime_verification(
    original_path: &Path,
    proxy_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_bytes = std::fs::read(original_path)?;
    let original = MediaSource::parse(&original_bytes)?;
    let proxy = Mp4Source::open(proxy_path)?;
    let proxy_video = proxy
        .video
        .as_ref()
        .ok_or("proxy has no H.264 video track")?;
    let proxy_h264 = classify_video_track(proxy_video)?;
    let audio_tracks = original
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .count();
    let matrix = BroadcastRuntimeVerificationMatrix::sony_fx6_sample_002_current();
    matrix.validate_truth_rules()?;

    println!("QGS Broadcast Player Runtime Acceptance Verification Matrix");
    println!("-----------------------------------------------------------");
    println!("This is a truthfulness/validation report, not a new media feature.");
    println!("Audio source rule: original MXF is authoritative");
    println!("Proxy AAC: not authoritative / not used as primary audio");
    println!("Proxy video role: responsive ProxyPreview source");
    println!("Original video role: OriginalMedia source; realtime support not claimed");
    println!(
        "Media inspected: proxy_video_frames={} proxy={}x{} coded={}x{} original_audio_tracks={} original_video_index_entries={}",
        proxy_video.samples.len(),
        proxy_h264.width,
        proxy_h264.height,
        proxy_h264.coded_width,
        proxy_h264.coded_height,
        audio_tracks,
        original.index.video.len()
    );
    println!();
    println!("{:<48} | {:<24} | Evidence", "Subsystem", "Level");
    println!("{:-<48}-+-{:-<24}-+-{:-<1}", "", "", "");
    for entry in &matrix.entries {
        println!(
            "{:<48} | {:<24} | {}",
            entry.subsystem.label(),
            entry.level.label(),
            entry.summary
        );
    }
    println!();
    println!("Truth rules:");
    println!("  Test video presenter evidence is not real display output");
    println!("  Test audio sink evidence is not real speaker output");
    println!("  Haswell CPU-bridge 50p is not marked realtime verified");
    println!("  Modern-hardware zero-copy remains frozen/not verified");
    println!("  Matrix truth-rule validation: passed");

    Ok(())
}

struct ProxyVideoPayloadBindingProof {
    bindings: Vec<qgs_media_runtime::BroadcastVideoPayloadBinding>,
    gpu_processor: Nv12FrameProcessor,
    gpu_submissions: usize,
    gpu_completions: usize,
    device_name: String,
}

impl ProxyVideoPayloadBindingProof {
    fn bounded_gpu_slots(&self) -> usize {
        usize::try_from(self.gpu_processor.counters().command_buffer_count).unwrap_or(0)
    }
}

struct OriginalVideoPayloadBindingProof {
    bindings: Vec<qgs_media_runtime::BroadcastVideoPayloadBinding>,
    gpu_processor: GpuFrameProcessor,
    target_frames: Vec<u64>,
    random_access_start: u64,
    decode_end: u64,
    decoded_frames: usize,
    gpu_submissions: usize,
    gpu_completions: usize,
    device_name: String,
    source_format: String,
    wall_elapsed: Duration,
}

impl OriginalVideoPayloadBindingProof {
    fn bounded_gpu_slots(&self) -> usize {
        usize::try_from(self.gpu_processor.counters().command_buffer_count).unwrap_or(0)
    }
}

fn finish_proxy_video_payload_bindings(
    proxy_video_slots: &[BroadcastPreparedVideoSlot],
    bindings_by_source: &mut BTreeMap<u64, qgs_media_runtime::BroadcastVideoPayloadBinding>,
    gpu_processor: Nv12FrameProcessor,
    gpu_submissions: usize,
    gpu_completions: usize,
    device_name: String,
) -> Result<ProxyVideoPayloadBindingProof, Box<dyn std::error::Error>> {
    let bindings = proxy_video_slots
        .iter()
        .map(|slot| {
            let source_frame_index = slot
                .source_frame_index
                .ok_or("proxy video payload slot missing source frame index")?;
            bindings_by_source
                .remove(&source_frame_index)
                .ok_or_else(|| {
                    format!(
                        "missing proxy video payload binding for source frame {source_frame_index}"
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProxyVideoPayloadBindingProof {
        bindings,
        gpu_processor,
        gpu_submissions,
        gpu_completions,
        device_name,
    })
}

fn bind_proxy_preview_video_payloads(
    proxy_video: &qgs_mp4::Mp4VideoTrack,
    proxy_h264: &qgs_mp4::Mp4H264Summary,
    proxy_video_slots: &[BroadcastPreparedVideoSlot],
) -> Result<ProxyVideoPayloadBindingProof, Box<dyn std::error::Error>> {
    let discovery = VulkanDeviceDiscovery::new()?;
    let devices = discovery.enumerate_devices()?;
    let Some(device) = devices
        .iter()
        .find(|device| {
            device.vendor_id == 0x8086 && matches!(device.class, DeviceClass::IntegratedGpu)
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::IntegratedGpu))
        })
    else {
        return Err("Broadcast Player proxy payload binding: no integrated GPU advertised".into());
    };
    let proxy_config = decoder_config_for_surface(
        device.id,
        proxy_h264.profile,
        BitDepth::new(proxy_h264.bit_depth)?,
        proxy_h264.chroma,
        proxy_h264.coded_width,
        proxy_h264.coded_height,
    );
    let vaapi = qgs_vaapi::VaapiVideoDiscovery::new(&devices);
    let capabilities = vaapi.query_video_capabilities(device.id)?;
    let supports_proxy = capabilities
        .decode
        .iter()
        .any(|capability| proxy_config.is_satisfied_by(capability));
    if !supports_proxy {
        return Err("Broadcast Player proxy payload binding: Intel VA backend does not support proxy configuration".into());
    }

    let mut targets = BTreeMap::new();
    for slot in proxy_video_slots {
        let source_frame_index = slot
            .source_frame_index
            .ok_or("proxy video payload slot missing source frame index")?;
        targets.insert(source_frame_index, slot);
    }

    let mut decoder = vaapi.create_decoder(&CreateDecoderRequest {
        config: proxy_config.clone(),
    })?;
    let decoder_id = DecoderId::new(20)?;
    let visible_region = VisibleRegion {
        x: 0,
        y: 0,
        width: proxy_h264.width,
        height: proxy_h264.height,
    };
    let mut cpu_pool = qgs_vaapi::CpuNv12FramePool::new(
        proxy_h264.coded_width,
        proxy_h264.coded_height,
        visible_region,
        proxy_video_slots.len().max(1),
    )?;
    let mut gpu_processor = Nv12FrameProcessor::new(
        &discovery,
        Nv12FrameProcessorConfig {
            device_id: device.id,
            coded_width: proxy_h264.coded_width,
            coded_height: proxy_h264.coded_height,
            visible_width: proxy_h264.width,
            visible_height: proxy_h264.height,
            slot_count: proxy_video_slots.len().max(1),
            conversion: YcbcrConversion::Rec709Limited,
            validation_readback: false,
        },
    )?;

    let mut bindings_by_source = BTreeMap::new();
    let mut output_index = 0_u64;
    let mut gpu_submissions = 0_usize;
    let mut gpu_completions = 0_usize;

    for sample in &proxy_video.samples {
        let outputs = decoder.submit_access_unit(&SubmitAccessUnitRequest {
            decoder_id,
            data: sample.annex_b.clone(),
        })?;
        for output in outputs {
            if let Some(slot) = targets.get(&output_index) {
                let identity = FrameIdentity {
                    presentation_position: slot
                        .selected_preview_frame_index
                        .ok_or("proxy video payload slot missing selected preview index")?,
                };
                let cpu_frame = cpu_pool.acquire()?;
                let (cpu_frame, _) =
                    qgs_vaapi::transfer_nv12_surface_timed(output.resource.as_ref(), cpu_frame)?;
                let upload = nv12_upload_for_cpu_surface(device.id, &cpu_frame)?;
                let token = gpu_processor.submit_frame(&upload, identity)?;
                gpu_submissions += 1;
                cpu_pool.release(cpu_frame)?;
                gpu_processor.wait_for_completion(token)?;
                gpu_completions += 1;
                let payload = BroadcastVideoPayloadReference {
                    payload_id: token.get(),
                    kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                    format: BroadcastVideoPayloadFormat::RgbaU16,
                    backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                    source_frame_index: output_index,
                    selected_preview_frame_index: slot.selected_preview_frame_index,
                    presentation_time: slot.presentation_time,
                    duration: slot.duration,
                    coded_width: proxy_h264.coded_width,
                    coded_height: proxy_h264.coded_height,
                    visible_width: proxy_h264.width,
                    visible_height: proxy_h264.height,
                    bounded_slot_index: slot.slot_index,
                    session_index: 0,
                };
                let binding = bind_broadcast_video_payload_ready(slot, payload)?;
                bindings_by_source.insert(output_index, binding);
                if bindings_by_source.len() == targets.len() {
                    return finish_proxy_video_payload_bindings(
                        proxy_video_slots,
                        &mut bindings_by_source,
                        gpu_processor,
                        gpu_submissions,
                        gpu_completions,
                        device.name.clone(),
                    );
                }
            }
            output_index = output_index
                .checked_add(1)
                .ok_or("proxy video output index overflow")?;
        }
    }

    let outputs = decoder.flush(&FlushDecoderRequest { decoder_id })?;
    for output in outputs {
        if let Some(slot) = targets.get(&output_index) {
            let identity = FrameIdentity {
                presentation_position: slot
                    .selected_preview_frame_index
                    .ok_or("proxy video payload slot missing selected preview index")?,
            };
            let cpu_frame = cpu_pool.acquire()?;
            let (cpu_frame, _) =
                qgs_vaapi::transfer_nv12_surface_timed(output.resource.as_ref(), cpu_frame)?;
            let upload = nv12_upload_for_cpu_surface(device.id, &cpu_frame)?;
            let token = gpu_processor.submit_frame(&upload, identity)?;
            gpu_submissions += 1;
            cpu_pool.release(cpu_frame)?;
            gpu_processor.wait_for_completion(token)?;
            gpu_completions += 1;
            let payload = BroadcastVideoPayloadReference {
                payload_id: token.get(),
                kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                format: BroadcastVideoPayloadFormat::RgbaU16,
                backend_path: BroadcastVideoPayloadBackendPath::VaapiCpuNv12Vulkan,
                source_frame_index: output_index,
                selected_preview_frame_index: slot.selected_preview_frame_index,
                presentation_time: slot.presentation_time,
                duration: slot.duration,
                coded_width: proxy_h264.coded_width,
                coded_height: proxy_h264.coded_height,
                visible_width: proxy_h264.width,
                visible_height: proxy_h264.height,
                bounded_slot_index: slot.slot_index,
                session_index: 0,
            };
            let binding = bind_broadcast_video_payload_ready(slot, payload)?;
            bindings_by_source.insert(output_index, binding);
            if bindings_by_source.len() == targets.len() {
                return finish_proxy_video_payload_bindings(
                    proxy_video_slots,
                    &mut bindings_by_source,
                    gpu_processor,
                    gpu_submissions,
                    gpu_completions,
                    device.name.clone(),
                );
            }
        }
        output_index = output_index
            .checked_add(1)
            .ok_or("proxy video output index overflow")?;
    }

    let missing = proxy_video_slots
        .iter()
        .filter_map(|slot| slot.source_frame_index)
        .filter(|source| !bindings_by_source.contains_key(source))
        .map(|source| source.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!("missing proxy video payload bindings for source frames: {missing}").into())
}

fn bind_original_media_video_payloads(
    original: &MediaSource,
    original_bytes: &[u8],
    original_video_slots: &[BroadcastPreparedVideoSlot],
) -> Result<OriginalVideoPayloadBindingProof, Box<dyn std::error::Error>> {
    let binding_started = Instant::now();
    if original_video_slots.is_empty() {
        return Err("Broadcast Player original payload binding: no prepared video slots".into());
    }
    let first_access_unit = original.extract_video_access_unit(original_bytes, 0)?;
    let parsed = qgs_codec_h264::parse_annex_b_access_unit(&first_access_unit)?;
    let discovery = VulkanDeviceDiscovery::new()?;
    let devices = discovery.enumerate_devices()?;
    let Some(device) = devices
        .iter()
        .find(|device| {
            device.vendor_id == 0x8086 && matches!(device.class, DeviceClass::IntegratedGpu)
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::IntegratedGpu))
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::DiscreteGpu))
        })
    else {
        return Err("Broadcast Player original payload binding: no Vulkan GPU advertised".into());
    };
    let config = decoder_config_for_surface(
        device.id,
        parsed.profile,
        parsed.desc.bit_depth,
        parsed.desc.chroma,
        parsed.desc.coded_width,
        parsed.desc.coded_height,
    );
    if !SoftwareVideoBackend::supports_config(&config) {
        return Err(
            "Broadcast Player original payload binding: software backend does not support original stream"
                .into(),
        );
    }
    let target_frames = original_video_slots
        .iter()
        .map(|slot| {
            slot.source_frame_index
                .ok_or("original video payload slot missing source frame index")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let min_target = *target_frames
        .iter()
        .min()
        .ok_or("original video payload binding has no targets")?;
    let max_target = *target_frames
        .iter()
        .max()
        .ok_or("original video payload binding has no targets")?;
    let random_access_start = original
        .index
        .nearest_random_access_before(min_target)
        .map(|entry| entry.edit_unit)
        .unwrap_or(min_target);
    let last_video_index = original.index.video.len().saturating_sub(1);
    let decode_end = max_target
        .saturating_add(16)
        .min(u64::try_from(last_video_index)?);
    let positioned = (random_access_start..=decode_end)
        .map(|edit_unit| {
            let index = usize::try_from(edit_unit)?;
            Ok((
                edit_unit,
                original.extract_video_access_unit(original_bytes, index)?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let decode_start = Instant::now();
    let decoded = decode_positioned_access_units_with_context(config, &positioned)?;
    let _decode_elapsed = decode_start.elapsed();
    let frames_by_presentation = decoded
        .frames
        .iter()
        .map(|frame| (frame.presentation_index, frame))
        .collect::<BTreeMap<_, _>>();
    let first_frame = original_video_slots
        .iter()
        .filter_map(|slot| {
            slot.source_frame_index
                .and_then(|index| frames_by_presentation.get(&index).copied())
        })
        .next()
        .ok_or("Broadcast Player original payload binding: no target decoded frames")?;
    let first_upload = yuv422p10_upload_for_frame(device.id, first_frame)?;
    let mut gpu_processor = GpuFrameProcessor::new(
        &discovery,
        GpuFrameProcessorConfig {
            device_id: device.id,
            width: first_upload.width,
            height: first_upload.height,
            slot_count: original_video_slots.len().max(1),
            conversion: first_upload.conversion,
        },
    )?;
    let mut bindings = Vec::new();
    let mut gpu_submissions = 0_usize;
    let mut gpu_completions = 0_usize;
    for slot in original_video_slots {
        let source_frame_index = slot
            .source_frame_index
            .ok_or("original video payload slot missing source frame index")?;
        let frame = frames_by_presentation
            .get(&source_frame_index)
            .copied()
            .or_else(|| {
                source_frame_index
                    .checked_sub(random_access_start)
                    .and_then(|relative| usize::try_from(relative).ok())
                    .and_then(|relative| decoded.frames.get(relative))
            })
            .ok_or_else(|| {
                format!("missing decoded original frame for source frame {source_frame_index}")
            })?;
        let upload = yuv422p10_upload_for_frame(device.id, frame)?;
        let identity = FrameIdentity {
            presentation_position: source_frame_index,
        };
        let token = gpu_processor.submit_frame(&upload, identity)?;
        gpu_submissions += 1;
        gpu_processor.wait_for_frame(token)?;
        gpu_completions += 1;
        let payload = BroadcastVideoPayloadReference {
            payload_id: token.get(),
            kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
            format: BroadcastVideoPayloadFormat::RgbaU16,
            backend_path: BroadcastVideoPayloadBackendPath::SoftwareH264Yuv422P10Vulkan,
            source_frame_index,
            selected_preview_frame_index: slot.selected_preview_frame_index,
            presentation_time: slot.presentation_time,
            duration: slot.duration,
            coded_width: parsed.desc.coded_width,
            coded_height: parsed.desc.coded_height,
            visible_width: parsed.desc.visible_region.width,
            visible_height: parsed.desc.visible_region.height,
            bounded_slot_index: slot.slot_index,
            session_index: 1,
        };
        bindings.push(bind_broadcast_video_payload_ready(slot, payload)?);
    }
    Ok(OriginalVideoPayloadBindingProof {
        bindings,
        gpu_processor,
        target_frames,
        random_access_start,
        decode_end,
        decoded_frames: decoded.frames.len(),
        gpu_submissions,
        gpu_completions,
        device_name: device.name.clone(),
        source_format: format!(
            "H.264 {:?}, {}-bit {:?}",
            parsed.profile,
            parsed.desc.bit_depth.get(),
            parsed.desc.chroma
        ),
        wall_elapsed: binding_started.elapsed(),
    })
}

fn broadcast_runtime_prepared_slots(
    original_path: &Path,
    proxy_path: &Path,
    report_focus: BroadcastRuntimeReportFocus,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_bytes = std::fs::read(original_path)?;
    let original = MediaSource::parse(&original_bytes)?;
    let proxy = Mp4Source::open(proxy_path)?;
    let proxy_video = proxy
        .video
        .as_ref()
        .ok_or("proxy has no H.264 video track")?;
    let proxy_h264 = classify_video_track(proxy_video)?;
    let audio_tracks = original
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect::<Vec<_>>();
    if audio_tracks.is_empty() {
        return Err("original MXF has no audio tracks".into());
    }

    let sample_rate = original_audio_sample_rate(&audio_tracks)?;
    let blocks = build_original_pcm_blocks(&original, &original_bytes)?;
    let audio_duration = blocks
        .iter()
        .map(|block| block.end_time())
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or("no original PCM blocks available")?;
    let queue_limits = BroadcastRuntimeQueueLimits {
        audio_block_capacity: 8,
        video_frame_capacity: 6,
        processed_frame_capacity: 3,
    };
    let session = BroadcastRuntimeSessionDescription {
        audio_source: BroadcastMediaSourceRole::OriginalAuthoritativeAudio,
        video_source: BroadcastMediaSourceRole::ProxyPreviewVideo,
        video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
        preview_profile: BroadcastPreviewProfile::Journalist50iPreview,
        audio_sample_rate: sample_rate,
        audio_track_count: audio_tracks.len(),
        queue_limits,
        capabilities: BroadcastRuntimeCapabilities {
            sample_clock_aware: true,
            preserves_original_pcm_format: true,
            preserves_track_channel_identity: true,
            proxy_video_preview: true,
            original_media_video_source: !original.index.video.is_empty(),
            original_media_realtime_supported: false,
            proxy_audio_primary: false,
            ui_dependent: false,
        },
    }
    .validate()?;
    let selected = selected_journalist_preview_frames(proxy_video)?;
    let selected_frame_count = selected.len();
    let intentional_skips = proxy_video
        .samples
        .len()
        .saturating_sub(selected_frame_count);
    let frame_duration = ProxyPlaybackProfile::Journalist50iPreview
        .presentation_rate(RationalRate::new(
            u64::from(proxy_video.frame_rate.numerator),
            u64::from(proxy_video.frame_rate.denominator),
        )?)?
        .frame_duration()?;
    let ranges = selected
        .iter()
        .map(|frame| {
            av_frame_audio_range(
                frame.preview_index,
                frame.start_time,
                frame_duration,
                sample_rate,
                &blocks,
                audio_tracks.len(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let runtime_summary = summarize_broadcast_runtime_contract(&ranges, audio_duration);
    let config = BroadcastPrerollConfig {
        video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
        video_frames_required: 3,
        audio_ranges_required: 3,
        max_video_queue: 6,
        max_audio_queue: 8,
        max_presentation_queue: 3,
    }
    .validate()?;
    let plan = BroadcastPrerollPlan {
        video_source_mode: BroadcastVideoSourceMode::ProxyPreview,
        selected_video_frames_planned: selected_frame_count,
        audio_ranges_planned: ranges.len(),
        intentional_skips_planned: intentional_skips,
        duration_covered: audio_duration,
        finite_queue_limits: queue_limits,
        video_source_available: true,
        video_runtime_supported: true,
    };
    let prepared_len = config.video_frames_required;
    let proxy_video_slots = selected
        .iter()
        .take(prepared_len)
        .enumerate()
        .map(|(slot_index, frame)| BroadcastPreparedVideoSlot {
            slot_index,
            source_mode: BroadcastVideoSourceMode::ProxyPreview,
            video_source_role: BroadcastMediaSourceRole::ProxyPreviewVideo,
            source_frame_index: Some(frame.source_presentation_index),
            selected_preview_frame_index: Some(frame.preview_index),
            presentation_time: frame.start_time,
            duration: frame_duration,
            status: BroadcastPreparedVideoSlotStatus::Prepared,
        })
        .collect::<Vec<_>>();
    let proxy_audio_slots = ranges
        .iter()
        .take(prepared_len)
        .enumerate()
        .map(|(slot_index, range)| {
            BroadcastPreparedAudioSlot::from_audio_range(
                slot_index,
                BroadcastVideoSourceMode::ProxyPreview,
                range,
            )
        })
        .collect::<Vec<_>>();
    let proxy_presentation_slots = selected
        .iter()
        .take(prepared_len)
        .enumerate()
        .map(|(slot_index, frame)| BroadcastPreparedPresentationSlot {
            presentation_index: slot_index,
            source_mode: BroadcastVideoSourceMode::ProxyPreview,
            selected_source_frame: Some(frame.source_presentation_index),
            video_slot_index: slot_index,
            audio_slot_index: slot_index,
            presentation_time: frame.start_time,
            duration: frame_duration,
            ready: proxy_video_slots
                .get(slot_index)
                .map(|slot| slot.is_ready())
                .unwrap_or(false)
                && proxy_audio_slots
                    .get(slot_index)
                    .map(|slot| slot.complete)
                    .unwrap_or(false),
        })
        .collect::<Vec<_>>();
    let proxy_slot_summary = summarize_broadcast_prepared_slots(
        config,
        plan,
        &proxy_video_slots,
        &proxy_audio_slots,
        &proxy_presentation_slots,
    );
    let proxy_audio_payload_bindings = proxy_audio_slots
        .iter()
        .map(|slot| bind_broadcast_audio_payload(slot, &blocks))
        .collect::<Result<Vec<_>, _>>()?;
    let proxy_video_payload_proof = if matches!(
        report_focus,
        BroadcastRuntimeReportFocus::VideoPayloads
            | BroadcastRuntimeReportFocus::DeviceBoundary
            | BroadcastRuntimeReportFocus::TestPresenter
            | BroadcastRuntimeReportFocus::TestAudioSink
            | BroadcastRuntimeReportFocus::SimulatedPlayback
            | BroadcastRuntimeReportFocus::OriginalVideoPayloads
    ) {
        Some(bind_proxy_preview_video_payloads(
            proxy_video,
            &proxy_h264,
            &proxy_video_slots,
        )?)
    } else {
        None
    };
    let proxy_video_payload_bindings = proxy_video_payload_proof
        .as_ref()
        .map(|proof| proof.bindings.clone())
        .unwrap_or_else(|| {
            proxy_video_slots
                .iter()
                .map(bind_broadcast_video_payload_accounting)
                .collect::<Vec<_>>()
        });
    let proxy_presentation_payload_bindings = proxy_presentation_slots
        .iter()
        .enumerate()
        .map(|(slot_index, slot)| {
            bind_broadcast_presentation_payload(
                slot,
                &proxy_video_payload_bindings[slot_index],
                &proxy_audio_payload_bindings[slot_index],
            )
        })
        .collect::<Vec<_>>();
    let proxy_payload_summary = summarize_broadcast_payload_bindings(
        BroadcastVideoSourceMode::ProxyPreview,
        config,
        &proxy_audio_payload_bindings,
        &proxy_video_payload_bindings,
        &proxy_presentation_payload_bindings,
    );

    let original_config = BroadcastPrerollConfig {
        video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
        ..config
    };
    let original_payload_attempt =
        report_focus == BroadcastRuntimeReportFocus::OriginalVideoPayloads;
    let original_plan = BroadcastPrerollPlan {
        video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
        selected_video_frames_planned: original.index.video.len(),
        audio_ranges_planned: ranges.len(),
        intentional_skips_planned: 0,
        duration_covered: audio_duration,
        finite_queue_limits: queue_limits,
        video_source_available: !original.index.video.is_empty(),
        video_runtime_supported: original_payload_attempt,
    };
    let original_video_slots = (0..prepared_len)
        .map(|slot_index| {
            let selected_frame = selected
                .get(slot_index)
                .ok_or("missing selected frame for original media slot")?;
            Ok::<_, Box<dyn std::error::Error>>(BroadcastPreparedVideoSlot {
                slot_index,
                source_mode: BroadcastVideoSourceMode::OriginalMedia,
                video_source_role: BroadcastMediaSourceRole::OriginalFinishingMedia,
                source_frame_index: Some(selected_frame.source_presentation_index),
                selected_preview_frame_index: None,
                presentation_time: selected_frame.start_time,
                duration: frame_duration,
                status: if original_payload_attempt {
                    BroadcastPreparedVideoSlotStatus::Prepared
                } else {
                    BroadcastPreparedVideoSlotStatus::CapabilityMissing
                },
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let original_audio_slots = ranges
        .iter()
        .take(prepared_len)
        .enumerate()
        .map(|(slot_index, range)| {
            BroadcastPreparedAudioSlot::from_audio_range(
                slot_index,
                BroadcastVideoSourceMode::OriginalMedia,
                range,
            )
        })
        .collect::<Vec<_>>();
    let original_presentation_slots = (0..prepared_len)
        .map(|slot_index| {
            let selected_frame = selected
                .get(slot_index)
                .ok_or("missing selected frame for original media presentation slot")?;
            Ok::<_, Box<dyn std::error::Error>>(BroadcastPreparedPresentationSlot {
                presentation_index: slot_index,
                source_mode: BroadcastVideoSourceMode::OriginalMedia,
                selected_source_frame: Some(selected_frame.source_presentation_index),
                video_slot_index: slot_index,
                audio_slot_index: slot_index,
                presentation_time: selected_frame.start_time,
                duration: frame_duration,
                ready: original_payload_attempt,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let original_slot_summary = summarize_broadcast_prepared_slots(
        original_config,
        original_plan,
        &original_video_slots,
        &original_audio_slots,
        &original_presentation_slots,
    );
    let original_audio_payload_bindings = original_audio_slots
        .iter()
        .map(|slot| bind_broadcast_audio_payload(slot, &blocks))
        .collect::<Result<Vec<_>, _>>()?;
    let original_video_payload_proof = if original_payload_attempt {
        Some(bind_original_media_video_payloads(
            &original,
            &original_bytes,
            &original_video_slots,
        )?)
    } else {
        None
    };
    let original_video_payload_bindings = original_video_payload_proof
        .as_ref()
        .map(|proof| proof.bindings.clone())
        .unwrap_or_else(|| {
            original_video_slots
                .iter()
                .map(bind_broadcast_video_payload_accounting)
                .collect::<Vec<_>>()
        });
    let original_presentation_payload_bindings = original_presentation_slots
        .iter()
        .enumerate()
        .map(|(slot_index, slot)| {
            bind_broadcast_presentation_payload(
                slot,
                &original_video_payload_bindings[slot_index],
                &original_audio_payload_bindings[slot_index],
            )
        })
        .collect::<Vec<_>>();
    let original_payload_summary = summarize_broadcast_payload_bindings(
        BroadcastVideoSourceMode::OriginalMedia,
        original_config,
        &original_audio_payload_bindings,
        &original_video_payload_bindings,
        &original_presentation_payload_bindings,
    );

    let mut runtime = BroadcastRuntimeStateMachine::create(session)?;
    runtime.prepare_with_preroll(
        BroadcastRuntimePrepareFacts {
            selected_frame_count,
            intentional_profile_skips: intentional_skips,
            audio_ranges_complete: runtime_summary.incomplete_frames == 0,
            frames_outside_audio_range: runtime_summary.frames_outside_audio_range,
        },
        proxy_slot_summary.preroll_status,
    )?;
    let ready_reached = runtime.state() == qgs_media_runtime::BroadcastRuntimeState::Ready;
    let play_from_ready = runtime.play().is_ok();
    if play_from_ready {
        runtime.account_happy_path()?;
        runtime.drain()?;
        runtime.complete()?;
    }
    let proxy_events = build_broadcast_player_event_surface(
        0,
        session,
        config,
        plan,
        runtime.state(),
        runtime.accounting(),
        runtime.events(),
        &proxy_slot_summary,
        &proxy_video_slots,
        &proxy_audio_slots,
        &proxy_presentation_slots,
    );
    let proxy_event_summary =
        summarize_broadcast_player_runtime_events(&proxy_events, runtime.accounting());
    let original_events = build_broadcast_player_event_surface(
        1,
        BroadcastRuntimeSessionDescription {
            video_source: BroadcastMediaSourceRole::OriginalFinishingMedia,
            video_source_mode: BroadcastVideoSourceMode::OriginalMedia,
            ..session
        },
        original_config,
        original_plan,
        qgs_media_runtime::BroadcastRuntimeState::Preparing,
        qgs_media_runtime::BroadcastRuntimeAccounting {
            selected_frames_accounted: 0,
            audio_ranges_accounted: 0,
            intentional_profile_skips: 0,
            lateness_drops: 0,
        },
        &[
            qgs_media_runtime::BroadcastRuntimeEvent::SessionCreated,
            qgs_media_runtime::BroadcastRuntimeEvent::PreparingStarted,
        ],
        &original_slot_summary,
        &original_video_slots,
        &original_audio_slots,
        &original_presentation_slots,
    );
    let original_event_summary = summarize_broadcast_player_runtime_events(
        &original_events,
        qgs_media_runtime::BroadcastRuntimeAccounting {
            selected_frames_accounted: 0,
            audio_ranges_accounted: 0,
            intentional_profile_skips: 0,
            lateness_drops: 0,
        },
    );

    match report_focus {
        BroadcastRuntimeReportFocus::PreparedSlots => {
            println!("QGS Broadcast Player Runtime Prepared Payload Slots");
            println!("---------------------------------------------------");
        }
        BroadcastRuntimeReportFocus::Events => {
            println!("QGS Broadcast Player Runtime Event Surface");
            println!("------------------------------------------");
        }
        BroadcastRuntimeReportFocus::Payloads => {
            println!("QGS Broadcast Player Runtime Payload Binding");
            println!("--------------------------------------------");
        }
        BroadcastRuntimeReportFocus::VideoPayloads => {
            println!("QGS Broadcast Player Runtime Proxy Video Payload Binding");
            println!("-------------------------------------------------------");
        }
        BroadcastRuntimeReportFocus::DeviceBoundary => {
            println!("QGS Broadcast Player Runtime Device Boundary Contract");
            println!("-----------------------------------------------------");
        }
        BroadcastRuntimeReportFocus::TestPresenter => {
            println!("QGS Broadcast Player Runtime Test Video Presenter Evidence");
            println!("----------------------------------------------------------");
        }
        BroadcastRuntimeReportFocus::TestAudioSink => {
            println!("QGS Broadcast Player Runtime Test Audio Sink Evidence");
            println!("-----------------------------------------------------");
        }
        BroadcastRuntimeReportFocus::SimulatedPlayback => {
            println!("QGS Broadcast Player Runtime Simulated Playback Loop");
            println!("----------------------------------------------------");
        }
        BroadcastRuntimeReportFocus::OriginalVideoPayloads => {
            println!("QGS Broadcast Player Runtime OriginalMedia Video Payload Binding");
            println!("---------------------------------------------------------------");
        }
    }
    println!(
        "Not real playback: no speaker output, no display output, no real-time Broadcast Player loop"
    );
    if report_focus == BroadcastRuntimeReportFocus::TestPresenter {
        println!("Test presenter only: no real display output");
    } else if report_focus == BroadcastRuntimeReportFocus::TestAudioSink {
        println!("Test audio sink only: no real speaker output");
    } else if report_focus == BroadcastRuntimeReportFocus::SimulatedPlayback {
        println!("Test boundaries only: no real speaker output or display output");
        println!("No realtime scheduler");
    } else if report_focus == BroadcastRuntimeReportFocus::OriginalVideoPayloads {
        println!("Not realtime playback: no display output, no speaker output");
        println!("No FramePresented event");
    } else {
        println!("No FramePresented event");
    }
    println!("No QNC UI integration");
    println!("Audio source: original MXF");
    println!("Proxy AAC: not used");
    println!("Preview profile: journalist-50i-preview");
    println!("ProxyPreview:");
    println!("  video source: proxy MP4");
    println!(
        "  slots: video={}/{} audio={}/{} presentation={}/{}",
        proxy_slot_summary.video_slot_count,
        proxy_slot_summary.video_slot_capacity,
        proxy_slot_summary.audio_slot_count,
        proxy_slot_summary.audio_slot_capacity,
        proxy_slot_summary.presentation_slot_count,
        proxy_slot_summary.presentation_slot_capacity
    );
    println!(
        "  ready slots: video={} audio={} presentation={}",
        proxy_slot_summary.video_slots_prepared,
        proxy_slot_summary.audio_slots_complete,
        proxy_slot_summary.presentation_slots_ready
    );
    println!("  tracks covered: {}", proxy_slot_summary.tracks_covered);
    println!(
        "  source frames referenced: {}",
        proxy_video_slots
            .iter()
            .filter_map(|slot| slot.source_frame_index)
            .map(|frame| frame.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "  first audio range: start_sample={} sample_count={}",
        proxy_audio_slots
            .first()
            .map(|slot| slot.start_sample)
            .unwrap_or(0),
        proxy_audio_slots
            .first()
            .map(|slot| slot.sample_count)
            .unwrap_or(0)
    );
    println!(
        "  Ready: {} Play from Ready: {}",
        yes_no(ready_reached),
        yes_no(play_from_ready)
    );
    println!("  Intentional source-frame skips: {intentional_skips}");
    println!(
        "  Broadcast Player runtime events: {}",
        proxy_event_summary.event_count
    );
    println!(
        "  Event surface: runtime_ready={} prepared_slot_available={} frame_accounted={} audio_range_accounted={} intentional_skip={} lateness_drops={} runtime_completed={} frame_presented={}",
        yes_no(proxy_event_summary.runtime_ready),
        proxy_event_summary.prepared_slot_events,
        proxy_event_summary.frame_accounted_events,
        proxy_event_summary.audio_range_accounted_events,
        proxy_event_summary.intentional_skip_events,
        proxy_event_summary.lateness_drops,
        yes_no(proxy_event_summary.runtime_completed),
        proxy_event_summary.frame_presented_events
    );
    if report_focus == BroadcastRuntimeReportFocus::Events {
        println!(
            "  Event sequence summary: SessionCreated -> PrepareStarted -> PrerollPlanned -> PreparedSlotAvailable x{} -> RuntimeReady -> TransportStarted -> FrameAccounted/AudioRangeAccounted x{} -> IntentionalProfileSkip x{} -> RuntimeCompleted",
            proxy_event_summary.prepared_slot_events,
            proxy_event_summary.frame_accounted_events,
            proxy_event_summary.intentional_skip_events
        );
    }
    if matches!(
        report_focus,
        BroadcastRuntimeReportFocus::Payloads
            | BroadcastRuntimeReportFocus::VideoPayloads
            | BroadcastRuntimeReportFocus::DeviceBoundary
            | BroadcastRuntimeReportFocus::TestPresenter
            | BroadcastRuntimeReportFocus::TestAudioSink
            | BroadcastRuntimeReportFocus::SimulatedPlayback
            | BroadcastRuntimeReportFocus::OriginalVideoPayloads
    ) {
        let blocks_per_presentation = proxy_audio_payload_bindings
            .first()
            .map(|binding| binding.block_coverage.len())
            .unwrap_or(0);
        let bytes_per_presentation = proxy_audio_payload_bindings
            .first()
            .map(|binding| binding.total_referenced_payload_bytes)
            .unwrap_or(0);
        let video_binding_status = proxy_video_payload_bindings
            .first()
            .map(|binding| binding.status)
            .unwrap_or(BroadcastVideoPayloadBindingStatus::Missing);
        let presentation_readiness = proxy_presentation_payload_bindings
            .first()
            .map(|binding| binding.readiness);
        let presentation_readiness_label = presentation_readiness
            .map(|readiness| format!("{readiness:?}"))
            .unwrap_or_else(|| "Missing".to_string());
        println!(
            "  Payload bindings: audio={}/{} video={}/{} presentation={}/{}",
            proxy_payload_summary.audio_binding_count,
            proxy_payload_summary.audio_binding_capacity,
            proxy_payload_summary.video_binding_count,
            proxy_payload_summary.video_binding_capacity,
            proxy_payload_summary.presentation_binding_count,
            proxy_payload_summary.presentation_binding_capacity
        );
        println!(
            "  Audio blocks referenced per presentation: {}",
            blocks_per_presentation
        );
        println!(
            "  Audio bytes referenced per presentation: {}",
            bytes_per_presentation
        );
        println!(
            "  Total referenced audio bytes: {}",
            proxy_payload_summary.total_referenced_audio_bytes
        );
        println!("  Video binding status: {:?}", video_binding_status);
        println!(
            "  Presentation binding readiness: {}",
            presentation_readiness_label
        );
        println!(
            "  Runtime-accounting-ready presentations: {}",
            proxy_payload_summary.runtime_accounting_ready_presentations
        );
        println!(
            "  Payload-ready presentations: {}",
            proxy_payload_summary.payload_ready_presentations
        );
        println!(
            "  Device-payload-ready presentations: {}",
            proxy_payload_summary.device_payload_ready_presentations
        );
        if let Some(proof) = &proxy_video_payload_proof {
            let first_payload = proxy_video_payload_bindings
                .first()
                .and_then(|binding| binding.payload.as_ref());
            let source_frames = proxy_video_payload_bindings
                .iter()
                .filter_map(|binding| binding.source_frame_index)
                .map(|frame| frame.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "  Proxy video payloads: bound={} submissions={} completions={} bounded_gpu_slots={} device={}",
                proof.bindings.len(),
                proof.gpu_submissions,
                proof.gpu_completions,
                proof.bounded_gpu_slots(),
                proof.device_name
            );
            println!("  Bound source frame indices: {source_frames}");
            if let Some(payload) = first_payload {
                println!(
                    "  Payload kind: {:?} format={:?} backend={:?}",
                    payload.kind, payload.format, payload.backend_path
                );
                println!(
                    "  Payload dimensions: visible={}x{} coded={}x{}",
                    payload.visible_width,
                    payload.visible_height,
                    payload.coded_width,
                    payload.coded_height
                );
            }
        }
        if report_focus == BroadcastRuntimeReportFocus::DeviceBoundary {
            let audio_format = blocks
                .first()
                .map(|block| block.format)
                .ok_or("no PCM blocks available for device boundary report")?;
            let audio_binding = proxy_audio_payload_bindings
                .first()
                .ok_or("missing proxy audio payload binding")?;
            let video_binding = proxy_video_payload_bindings
                .first()
                .ok_or("missing proxy video payload binding")?;
            let audio_submission = build_broadcast_audio_device_submission(
                0,
                audio_binding,
                audio_format,
                BroadcastDeviceStatus::NotConfigured,
                &[],
            );
            let video_submission = build_broadcast_video_presenter_submission(
                0,
                video_binding,
                BroadcastDeviceStatus::NotConfigured,
                &[],
            );
            let boundary = summarize_broadcast_device_boundary(
                &audio_submission,
                &video_submission,
                &[],
                BroadcastDeviceStatus::NotConfigured,
                BroadcastDeviceStatus::NotConfigured,
            );
            println!(
                "  Device boundary: audio_payload_ready={} video_payload_ready={} device_payload_ready={} frame_presented={}",
                yes_no(boundary.audio_payload_ready),
                yes_no(boundary.video_payload_ready),
                yes_no(boundary.device_payload_ready),
                boundary.frame_presented_count
            );
            println!(
                "  Audio sink status: {:?} submission={:?}",
                boundary.audio_device_status, boundary.audio_submission_status
            );
            println!(
                "  Video presenter status: {:?} submission={:?}",
                boundary.video_presenter_status, boundary.video_submission_status
            );
            println!(
                "  Device-ready reason: {}",
                boundary.reason.unwrap_or("ready")
            );
        }
        if report_focus == BroadcastRuntimeReportFocus::TestPresenter {
            let audio_format = blocks
                .first()
                .map(|block| block.format)
                .ok_or("no PCM blocks available for test presenter report")?;
            let audio_binding = proxy_audio_payload_bindings
                .first()
                .ok_or("missing proxy audio payload binding")?;
            let audio_submission = build_broadcast_audio_device_submission(
                0,
                audio_binding,
                audio_format,
                BroadcastDeviceStatus::NotConfigured,
                &[],
            );
            let mut presenter =
                BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                    accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                    accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
                    visible_width: proxy_h264.width,
                    visible_height: proxy_h264.height,
                    coded_width: proxy_h264.coded_width,
                    coded_height: proxy_h264.coded_height,
                })?;
            let presenter_capabilities = presenter.capabilities();
            let video_submission = build_broadcast_video_presenter_submission(
                0,
                proxy_video_payload_bindings
                    .first()
                    .ok_or("missing proxy video payload binding")?,
                BroadcastDeviceStatus::Ready,
                &presenter_capabilities,
            );
            let mut evidence = Vec::new();
            for (presentation_binding, video_binding) in proxy_presentation_payload_bindings
                .iter()
                .zip(proxy_video_payload_bindings.iter())
            {
                evidence.push(presenter.submit(presentation_binding, video_binding)?);
            }
            let presenter_events = broadcast_player_events_from_presentation_evidence(&evidence);
            let presenter_event_summary = summarize_broadcast_player_runtime_events(
                &presenter_events,
                qgs_media_runtime::BroadcastRuntimeAccounting {
                    selected_frames_accounted: 0,
                    audio_ranges_accounted: 0,
                    intentional_profile_skips: 0,
                    lateness_drops: 0,
                },
            );
            let boundary = summarize_broadcast_device_boundary(
                &audio_submission,
                &video_submission,
                &evidence,
                BroadcastDeviceStatus::NotConfigured,
                BroadcastDeviceStatus::Ready,
            );
            let evidence_kind = evidence
                .first()
                .map(|item| format!("{:?}", item.evidence_kind))
                .unwrap_or_else(|| "None".to_string());
            println!(
                "  Test presenter: submitted={} accepted={} rejected={} evidence_records={}",
                proxy_video_payload_bindings.len(),
                presenter.accepted_count(),
                presenter.rejected_count(),
                evidence.len()
            );
            println!(
                "  Test presenter video submission status: {:?}",
                video_submission.status
            );
            println!(
                "  A/V DevicePayloadReady: {}",
                yes_no(boundary.device_payload_ready)
            );
            println!(
                "  Video presenter DevicePayloadReady: {}",
                yes_no(video_submission.status == BroadcastDevicePayloadStatus::DevicePayloadReady)
            );
            println!(
                "  FramePresented count: {}",
                presenter_event_summary.frame_presented_events
            );
            println!("  Evidence kind: {evidence_kind}");
            println!("  Evidence source: test video presenter, not real display output");
        }
        if report_focus == BroadcastRuntimeReportFocus::TestAudioSink {
            let audio_format = blocks
                .first()
                .map(|block| block.format)
                .ok_or("no PCM blocks available for test audio sink report")?;
            let PcmSampleFormat::SignedInteger {
                bits_per_sample, ..
            } = audio_format;
            let track_count = proxy_audio_payload_bindings
                .first()
                .map(|binding| binding.track_count)
                .unwrap_or(0);
            let mut sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
                sample_rate,
                bits_per_sample,
                track_count,
            })?;
            let mut evidence = Vec::new();
            for (presentation_binding, audio_binding) in proxy_presentation_payload_bindings
                .iter()
                .zip(proxy_audio_payload_bindings.iter())
            {
                evidence.push(sink.submit(presentation_binding, audio_binding, audio_format)?);
            }
            let audio_events = broadcast_player_events_from_audio_sink_evidence(&evidence);
            let audio_event_summary = summarize_broadcast_player_runtime_events(
                &audio_events,
                qgs_media_runtime::BroadcastRuntimeAccounting {
                    selected_frames_accounted: 0,
                    audio_ranges_accounted: 0,
                    intentional_profile_skips: 0,
                    lateness_drops: 0,
                },
            );
            let evidence_kind = evidence
                .first()
                .map(|item| format!("{:?}", item.evidence_kind))
                .unwrap_or_else(|| "None".to_string());
            let bytes_per_presentation = proxy_audio_payload_bindings
                .first()
                .map(|binding| binding.total_referenced_payload_bytes)
                .unwrap_or(0);
            let samples_per_track = proxy_audio_payload_bindings
                .first()
                .map(|binding| binding.sample_count)
                .unwrap_or(0);
            println!(
                "  Test audio sink: submitted={} accepted={} rejected={} evidence_records={}",
                proxy_audio_payload_bindings.len(),
                sink.accepted_count(),
                sink.rejected_count(),
                evidence.len()
            );
            println!(
                "  Audio format accepted: signed integer PCM {}-bit {} Hz tracks={}",
                bits_per_sample, sample_rate, track_count
            );
            println!(
                "  Samples accepted per presentation range: {} per track",
                samples_per_track
            );
            println!(
                "  Audio bytes accepted per presentation range: {}",
                bytes_per_presentation
            );
            println!("  Total audio bytes accepted: {}", sink.bytes_accepted());
            println!(
                "  Total sample ranges accepted: {}",
                sink.samples_accepted()
            );
            println!("  Evidence kind: {evidence_kind}");
            println!(
                "  Audio evidence events: {} frame_presented={}",
                audio_events.len(),
                audio_event_summary.frame_presented_events
            );
            println!("  Evidence source: test audio sink, not real speaker output");
        }
        if report_focus == BroadcastRuntimeReportFocus::SimulatedPlayback {
            let audio_format = blocks
                .first()
                .map(|block| block.format)
                .ok_or("no PCM blocks available for simulated playback report")?;
            let PcmSampleFormat::SignedInteger {
                bits_per_sample, ..
            } = audio_format;
            let track_count = proxy_audio_payload_bindings
                .first()
                .map(|binding| binding.track_count)
                .unwrap_or(0);
            let mut sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
                sample_rate,
                bits_per_sample,
                track_count,
            })?;
            let mut presenter =
                BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                    accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                    accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
                    visible_width: proxy_h264.width,
                    visible_height: proxy_h264.height,
                    coded_width: proxy_h264.coded_width,
                    coded_height: proxy_h264.coded_height,
                })?;
            let simulation = simulate_broadcast_player_runtime_loop(
                BroadcastVideoSourceMode::ProxyPreview,
                &proxy_presentation_payload_bindings,
                &proxy_audio_payload_bindings,
                &proxy_video_payload_bindings,
                audio_format,
                &mut sink,
                &mut presenter,
            );
            println!(
                "  Simulated playback: slots={} audio_submitted={} audio_accepted={} video_submitted={} video_accepted={}",
                simulation.summary.presentation_slots_attempted,
                simulation.summary.audio_submissions,
                simulation.summary.audio_accepted,
                simulation.summary.video_submissions,
                simulation.summary.video_accepted
            );
            println!(
                "  Test evidence: audio={} video={} total={}",
                simulation.summary.audio_evidence_count,
                simulation.summary.video_evidence_count,
                simulation.summary.test_evidence_count
            );
            println!(
                "  FramePresented count: {} (test presenter evidence only)",
                simulation.summary.frame_presented_count
            );
            println!("  Lateness drops: {}", simulation.summary.lateness_drops);
            println!("  Failed slots: {}", simulation.summary.failed_slots);
            println!("  Final state: {:?}", simulation.summary.final_state);
            println!("  Completed: {}", yes_no(simulation.summary.completed));
            println!("  Audio evidence source: test audio sink, not real speaker output");
            println!("  Video evidence source: test video presenter, not real display output");
        }
    }
    println!("OriginalMedia:");
    println!("  video source: original MXF");
    println!("  audio source: original MXF");
    println!(
        "  original video source present: {}",
        yes_no(!original.index.video.is_empty())
    );
    println!(
        "  slots: video={}/{} audio={}/{} presentation={}/{}",
        original_slot_summary.video_slot_count,
        original_slot_summary.video_slot_capacity,
        original_slot_summary.audio_slot_count,
        original_slot_summary.audio_slot_capacity,
        original_slot_summary.presentation_slot_count,
        original_slot_summary.presentation_slot_capacity
    );
    println!(
        "  ready slots: video={} audio={} presentation={}",
        original_slot_summary.video_slots_prepared,
        original_slot_summary.audio_slots_complete,
        original_slot_summary.presentation_slots_ready
    );
    println!(
        "  Ready: {} reason={:?}",
        yes_no(original_slot_summary.preroll_status.ready),
        original_slot_summary.preroll_status.reason
    );
    println!(
        "  Event surface: capability_missing={} broadcast_player_ready={}",
        yes_no(original_event_summary.capability_missing),
        yes_no(original_event_summary.runtime_ready)
    );
    if report_focus == BroadcastRuntimeReportFocus::Events {
        println!(
            "  Event sequence summary: SessionCreated -> PrepareStarted -> PrerollPlanned -> CapabilityMissing"
        );
    }
    if matches!(
        report_focus,
        BroadcastRuntimeReportFocus::Payloads
            | BroadcastRuntimeReportFocus::VideoPayloads
            | BroadcastRuntimeReportFocus::DeviceBoundary
            | BroadcastRuntimeReportFocus::TestPresenter
            | BroadcastRuntimeReportFocus::TestAudioSink
            | BroadcastRuntimeReportFocus::SimulatedPlayback
            | BroadcastRuntimeReportFocus::OriginalVideoPayloads
    ) {
        let video_binding_status = original_video_payload_bindings
            .first()
            .map(|binding| binding.status)
            .unwrap_or(BroadcastVideoPayloadBindingStatus::Missing);
        let presentation_readiness = original_presentation_payload_bindings
            .first()
            .map(|binding| binding.readiness);
        let presentation_readiness_label = presentation_readiness
            .map(|readiness| format!("{readiness:?}"))
            .unwrap_or_else(|| "Missing".to_string());
        println!(
            "  Payload bindings: audio={}/{} video={}/{} presentation={}/{}",
            original_payload_summary.audio_binding_count,
            original_payload_summary.audio_binding_capacity,
            original_payload_summary.video_binding_count,
            original_payload_summary.video_binding_capacity,
            original_payload_summary.presentation_binding_count,
            original_payload_summary.presentation_binding_capacity
        );
        println!(
            "  Audio binding possible: {}",
            yes_no(original_payload_summary.complete_audio_bindings > 0)
        );
        println!("  Video binding status: {:?}", video_binding_status);
        println!(
            "  Presentation binding readiness: {}",
            presentation_readiness_label
        );
        println!(
            "  Capability-missing presentations: {}",
            original_payload_summary.capability_missing_presentations
        );
        if report_focus == BroadcastRuntimeReportFocus::DeviceBoundary {
            println!("  Device boundary: not attempted for OriginalMedia");
            println!("  DevicePayloadReady: no");
            println!("  FramePresented: 0");
        }
        if report_focus == BroadcastRuntimeReportFocus::TestPresenter {
            println!("  Test presenter: not attempted for OriginalMedia");
            println!("  Original video payload: CapabilityMissing");
            println!("  FramePresented count: 0");
        }
        if report_focus == BroadcastRuntimeReportFocus::TestAudioSink {
            let audio_format = blocks
                .first()
                .map(|block| block.format)
                .ok_or("no PCM blocks available for OriginalMedia test audio sink report")?;
            let PcmSampleFormat::SignedInteger {
                bits_per_sample, ..
            } = audio_format;
            let track_count = original_audio_payload_bindings
                .first()
                .map(|binding| binding.track_count)
                .unwrap_or(0);
            let mut sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
                sample_rate,
                bits_per_sample,
                track_count,
            })?;
            let mut accepted = 0_usize;
            for (presentation_binding, audio_binding) in original_presentation_payload_bindings
                .iter()
                .zip(original_audio_payload_bindings.iter())
            {
                if sink
                    .submit(presentation_binding, audio_binding, audio_format)
                    .is_ok()
                {
                    accepted += 1;
                }
            }
            println!(
                "  Test audio sink: accepted={} rejected={} total_bytes={}",
                accepted,
                sink.rejected_count(),
                sink.bytes_accepted()
            );
            println!("  Original video payload: CapabilityMissing");
            println!("  Full OriginalMedia presentation ready: no");
        }
        if report_focus == BroadcastRuntimeReportFocus::SimulatedPlayback {
            let audio_format = blocks
                .first()
                .map(|block| block.format)
                .ok_or("no PCM blocks available for OriginalMedia simulated playback report")?;
            let PcmSampleFormat::SignedInteger {
                bits_per_sample, ..
            } = audio_format;
            let track_count = original_audio_payload_bindings
                .first()
                .map(|binding| binding.track_count)
                .unwrap_or(0);
            let mut sink = BroadcastTestAudioSink::new(BroadcastTestAudioSinkConfig {
                sample_rate,
                bits_per_sample,
                track_count,
            })?;
            let mut presenter =
                BroadcastTestVideoPresenter::new(BroadcastTestVideoPresenterConfig {
                    accepted_kind: BroadcastVideoPayloadKind::ProcessedGpuFrame,
                    accepted_format: BroadcastVideoPayloadFormat::RgbaU16,
                    visible_width: proxy_h264.width,
                    visible_height: proxy_h264.height,
                    coded_width: proxy_h264.coded_width,
                    coded_height: proxy_h264.coded_height,
                })?;
            let simulation = simulate_broadcast_player_runtime_loop(
                BroadcastVideoSourceMode::OriginalMedia,
                &original_presentation_payload_bindings,
                &original_audio_payload_bindings,
                &original_video_payload_bindings,
                audio_format,
                &mut sink,
                &mut presenter,
            );
            println!(
                "  Simulated playback: completed={} final_state={:?} failed_slots={}",
                yes_no(simulation.summary.completed),
                simulation.summary.final_state,
                simulation.summary.failed_slots
            );
            println!(
                "  FramePresented count: {}",
                simulation.summary.frame_presented_count
            );
            println!("  Original video payload: CapabilityMissing");
            println!("  Simulation result: not ready / capability missing");
        }
        if report_focus == BroadcastRuntimeReportFocus::OriginalVideoPayloads {
            if let Some(proof) = &original_video_payload_proof {
                let target_frames = proof
                    .target_frames
                    .iter()
                    .map(|frame| frame.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let first_payload = original_video_payload_bindings
                    .first()
                    .and_then(|binding| binding.payload.as_ref());
                println!(
                    "  Original video payloads: bound={} submissions={} completions={} bounded_gpu_slots={} device={}",
                    proof.bindings.len(),
                    proof.gpu_submissions,
                    proof.gpu_completions,
                    proof.bounded_gpu_slots(),
                    proof.device_name
                );
                println!("  Target source frames: {target_frames}");
                println!(
                    "  Random access start: {} decode_end={} decoded_frames={}",
                    proof.random_access_start, proof.decode_end, proof.decoded_frames
                );
                println!(
                    "  DEVELOPMENT OBSERVATION - NOT A BENCHMARK: {:.3}s for bounded original payload binding",
                    proof.wall_elapsed.as_secs_f64()
                );
                println!("  Source format: {}", proof.source_format);
                if let Some(payload) = first_payload {
                    println!(
                        "  Payload kind: {:?} format={:?} backend={:?}",
                        payload.kind, payload.format, payload.backend_path
                    );
                    println!(
                        "  Payload dimensions: visible={}x{} coded={}x{}",
                        payload.visible_width,
                        payload.visible_height,
                        payload.coded_width,
                        payload.coded_height
                    );
                }
                println!(
                    "  Presentation payload-ready: {}",
                    original_payload_summary.payload_ready_presentations
                );
                println!("  DevicePayloadReady: no");
                println!("  FramePresented: 0");
                println!("  Realtime support claimed: no");
                println!("  Proxy video used for OriginalMedia payload: no");
            }
        }
    }
    if report_focus == BroadcastRuntimeReportFocus::OriginalVideoPayloads {
        println!(
            "  Original video runtime backend: bounded payload binding only; realtime not claimed"
        );
    } else {
        println!("  Original video runtime backend: capability missing in this milestone");
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BroadcastRuntimeReportFocus {
    PreparedSlots,
    Events,
    Payloads,
    VideoPayloads,
    DeviceBoundary,
    TestPresenter,
    TestAudioSink,
    SimulatedPlayback,
    OriginalVideoPayloads,
}

#[derive(Clone, Debug)]
struct SelectedPreviewFrame {
    preview_index: u64,
    source_presentation_index: u64,
    sample_index: u32,
    start_time: Duration,
}

fn selected_journalist_preview_frames(
    proxy_video: &qgs_mp4::Mp4VideoTrack,
) -> Result<Vec<SelectedPreviewFrame>, Box<dyn std::error::Error>> {
    let mut ordered = proxy_video
        .samples
        .iter()
        .map(|sample| (sample.pts, sample.sample_index, sample))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(pts, sample_index, _)| (*pts, *sample_index));
    let media_origin_pts = ordered
        .first()
        .map(|(pts, _, _)| *pts)
        .ok_or("proxy video has no samples")?;
    let mut selected = Vec::new();
    for (source_presentation_index, (_, _, sample)) in ordered.into_iter().enumerate() {
        let source_presentation_index = u64::try_from(source_presentation_index)?;
        let Some(preview_index) = ProxyPlaybackProfile::Journalist50iPreview
            .presentation_position(source_presentation_index)
        else {
            continue;
        };
        let relative_pts = sample
            .pts
            .checked_sub(media_origin_pts)
            .ok_or("proxy PTS precedes media origin")?;
        let pts = u64::try_from(relative_pts).map_err(|_| "negative proxy PTS is unsupported")?;
        selected.push(SelectedPreviewFrame {
            preview_index,
            source_presentation_index,
            sample_index: sample.sample_index,
            start_time: duration_from_units(pts, u64::from(proxy_video.timescale), 1)?,
        });
    }
    Ok(selected)
}

fn build_original_pcm_blocks(
    source: &MediaSource,
    bytes: &[u8],
) -> Result<Vec<PcmAudioBlock>, Box<dyn std::error::Error>> {
    let mut blocks = Vec::with_capacity(source.index.audio.len());
    for entry_index in 0..source.index.audio.len() {
        let extracted = source.extract_pcm_audio_packet(bytes, entry_index)?;
        let track = source
            .tracks
            .iter()
            .find(|track| track.id == extracted.entry.track_id)
            .ok_or("PCM packet references missing audio track")?;
        let audio = track
            .audio
            .as_ref()
            .ok_or("PCM packet track has no audio descriptor")?;
        let sample_rate = rational_to_u32(audio.sample_rate.ok_or("audio sample rate missing")?)?;
        let bits_per_sample = audio.bit_depth.ok_or("audio bit depth missing")?;
        let format = PcmSampleFormat::SignedInteger {
            bits_per_sample,
            endian: PcmEndian::Little,
        };
        let packet = PcmAudioPacket::new(
            extracted.entry.track_id.0,
            extracted.entry.channel_index,
            duration_from_audio_samples(extracted.entry.start_sample, sample_rate)?,
            duration_from_audio_samples(u64::from(extracted.entry.sample_count), sample_rate)?,
            extracted.entry.sample_count,
            format,
            extracted.payload,
        )?;
        blocks.push(PcmAudioBlock::from_mono_packet(packet, sample_rate)?);
    }
    Ok(blocks)
}

fn original_audio_sample_rate(
    audio_tracks: &[&qgs_mxf::MxfTrack],
) -> Result<u32, Box<dyn std::error::Error>> {
    let mut sample_rate = None;
    for track in audio_tracks {
        let audio = track.audio.as_ref().ok_or("audio descriptor missing")?;
        let track_rate = rational_to_u32(audio.sample_rate.ok_or("audio sample rate missing")?)?;
        if let Some(sample_rate) = sample_rate {
            if sample_rate != track_rate {
                return Err("mixed original audio sample rates are not supported yet".into());
            }
        } else {
            sample_rate = Some(track_rate);
        }
    }
    sample_rate.ok_or_else(|| "no original audio sample rate available".into())
}

fn representative_range_indices(len: usize) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let mut indices = vec![0, len / 2, len - 1];
    indices.sort_unstable();
    indices.dedup();
    indices
}

fn print_av_range(frame: &SelectedPreviewFrame, range: &AvFrameAudioRange) {
    let track_ids = range
        .covered_tracks
        .iter()
        .map(|coverage| coverage.track_id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let total_gaps = range
        .covered_tracks
        .iter()
        .map(|coverage| u64::from(coverage.gaps))
        .sum::<u64>();
    let total_overlaps = range
        .covered_tracks
        .iter()
        .map(|coverage| u64::from(coverage.overlaps))
        .sum::<u64>();
    println!(
        "  preview_frame={} source_presentation={} sample_index={} video_start={:.3}s audio_samples={}..{} sample_count={} tracks=[{}] complete={} gaps={} overlaps={}",
        frame.preview_index,
        frame.source_presentation_index,
        frame.sample_index,
        frame.start_time.as_secs_f64(),
        range.audio_start_sample,
        range.audio_start_sample + range.audio_sample_count,
        range.audio_sample_count,
        track_ids,
        yes_no(range.complete),
        total_gaps,
        total_overlaps
    );
}

fn duration_from_units(
    units: u64,
    units_per_second: u64,
    second_scale: u64,
) -> Result<Duration, Box<dyn std::error::Error>> {
    if units_per_second == 0 || second_scale == 0 {
        return Err("invalid duration rate".into());
    }
    let nanos = u128::from(units)
        .checked_mul(u128::from(second_scale))
        .and_then(|value| value.checked_mul(1_000_000_000))
        .ok_or("duration overflow")?
        / u128::from(units_per_second);
    Ok(Duration::new(
        u64::try_from(nanos / 1_000_000_000).map_err(|_| "duration seconds overflow")?,
        u32::try_from(nanos % 1_000_000_000).map_err(|_| "duration nanos overflow")?,
    ))
}

fn rational_to_u32(value: qgs_mxf::Rational) -> Result<u32, Box<dyn std::error::Error>> {
    if value.denominator == 0 || value.numerator % value.denominator != 0 {
        return Err("non-integer audio sample rate is not supported in Step 20A".into());
    }
    Ok(value.numerator / value.denominator)
}

fn uniform_value<T: Copy + Eq>(values: &[T]) -> Option<T> {
    let first = *values.first()?;
    values.iter().all(|value| *value == first).then_some(first)
}

fn proxy_throughput(
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
    let source_seconds = proxy_video.samples.len() as f64
        / (proxy_video.frame_rate.numerator as f64 / proxy_video.frame_rate.denominator as f64);
    let access_units = proxy_video
        .samples
        .iter()
        .map(|sample| sample.annex_b.clone())
        .collect::<Vec<_>>();

    println!("QGS proxy VA throughput audit:");
    println!("  original: {}", case.original_label);
    println!("  proxy: {}", case.proxy_label);
    println!("  hashes: verified");
    println!("  original edit units: {}", original.index.video.len());
    println!("  proxy samples: {}", proxy_video.samples.len());
    println!(
        "  proxy: H.264 {:?}, {}-bit {:?}, {} x {}, {}/{} fps",
        proxy_h264.profile,
        proxy_h264.bit_depth,
        proxy_h264.chroma,
        proxy_h264.width,
        proxy_h264.height,
        proxy_video.frame_rate.numerator,
        proxy_video.frame_rate.denominator
    );

    let frontend_start = Instant::now();
    let frontend = run_proxy_frontend_only(proxy_video)?;
    let frontend_elapsed = frontend_start.elapsed();
    print_decode_timing(
        "Container + H.264 frontend only",
        frontend.frames,
        source_seconds,
        frontend_elapsed,
    );
    println!(
        "  frontend peak DPB={} peak output_pending={}",
        frontend.peak_dpb_occupancy, frontend.peak_output_pending
    );

    let discovery = VulkanDeviceDiscovery::new()?;
    let devices = discovery.enumerate_devices()?;
    let Some(device) = devices
        .iter()
        .find(|device| {
            device.vendor_id == 0x8086 && matches!(device.class, DeviceClass::IntegratedGpu)
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::IntegratedGpu))
        })
    else {
        println!("Intel VA proxy decode: no integrated GPU advertised");
        return Ok(());
    };
    let proxy_config = decoder_config_for_surface(
        device.id,
        proxy_h264.profile,
        BitDepth::new(proxy_h264.bit_depth)?,
        proxy_h264.chroma,
        proxy_h264.coded_width,
        proxy_h264.coded_height,
    );
    let vaapi = qgs_vaapi::VaapiVideoDiscovery::new(&devices);
    let capabilities = vaapi.query_video_capabilities(device.id)?;
    let supports_proxy = capabilities
        .decode
        .iter()
        .any(|capability| proxy_config.is_satisfied_by(capability));
    println!("Intel proxy capability:");
    println!("  device: {}", device.name);
    println!(
        "  H.264 {:?} {}-bit {:?}: {}",
        proxy_h264.profile,
        proxy_h264.bit_depth,
        proxy_h264.chroma,
        yes_no(supports_proxy)
    );
    if !supports_proxy {
        return Ok(());
    }

    let selected = selected_proxy_ordinals(proxy_video.samples.len());
    let normal_start = Instant::now();
    let normal = qgs_vaapi::decode_h264_access_units_for_observation(
        &devices,
        device.id,
        &proxy_config,
        &access_units,
        qgs_vaapi::VaapiDecodeMode::Normal,
        &selected,
    )?;
    let normal_elapsed = normal_start.elapsed();
    print_decode_timing(
        "Clean VA hardware decode",
        normal.frames,
        source_seconds,
        normal_elapsed,
    );
    print_va_observation(&normal);
    print_va_timing(&normal.timing);
    if normal.frames != proxy_video.samples.len() {
        return Err(format!(
            "clean VA decode expected {} frames, got {}",
            proxy_video.samples.len(),
            normal.frames
        )
        .into());
    }

    println!("Clean VA pool-size experiment:");
    for pool_size in [8_usize, 12, 16, 24, 32] {
        let pool_start = Instant::now();
        let observation = qgs_vaapi::decode_h264_access_units_for_observation_with_pool_size(
            &devices,
            device.id,
            &proxy_config,
            &access_units,
            qgs_vaapi::VaapiDecodeMode::Normal,
            &[],
            pool_size,
        )?;
        let elapsed = pool_start.elapsed();
        let seconds = elapsed.as_secs_f64().max(0.000_001);
        println!(
            "  pool={pool_size}: frames={} {:.3}s {:.2} fps recycle_sync={} peak_pending_recycle={} peak_unsynced={} min_free={} client_held={}",
            observation.frames,
            seconds,
            observation.frames as f64 / seconds,
            observation.pool_stats.recycle_sync_count,
            observation.pool_stats.peak_pending_recycle_surfaces,
            observation.peak_submitted_unsynced_surfaces,
            observation.pool_stats.minimum_free_surfaces,
            observation.max_client_held_outputs
        );
        if observation.frames != proxy_video.samples.len() {
            return Err(format!(
                "pool-size experiment {pool_size} expected {} frames, got {}",
                proxy_video.samples.len(),
                observation.frames
            )
            .into());
        }
    }

    let diagnostic_start = Instant::now();
    let diagnostic = qgs_vaapi::decode_h264_access_units_for_observation(
        &devices,
        device.id,
        &proxy_config,
        &access_units,
        qgs_vaapi::VaapiDecodeMode::Diagnostic,
        &selected,
    )?;
    let diagnostic_elapsed = diagnostic_start.elapsed();
    print_decode_timing(
        "Diagnostic VA hardware decode",
        diagnostic.frames,
        source_seconds,
        diagnostic_elapsed,
    );
    print_va_observation(&diagnostic);
    print_va_timing(&diagnostic.timing);
    println!(
        "  selected diagnostic checksums: {:?}",
        diagnostic.selected_output_checksums
    );
    if diagnostic.frames != proxy_video.samples.len() {
        return Err(format!(
            "diagnostic VA decode expected {} frames, got {}",
            proxy_video.samples.len(),
            diagnostic.frames
        )
        .into());
    }

    Ok(())
}

fn proxy_playback(
    original_path: &Path,
    proxy_path: &Path,
    profile: ProxyPlaybackProfile,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_sha256 = sha256_hex(original_path)?;
    let proxy_sha256 = sha256_hex(proxy_path)?;
    let case = identify_camera_case(&original_sha256, &proxy_sha256)?;
    if case.damaged_proxy {
        return diagnose_damaged_proxy_case(case, proxy_path);
    }

    let original_bytes = std::fs::read(original_path)?;
    let original = MediaSource::parse(&original_bytes)?;
    let source_open_start = Instant::now();
    let proxy = Mp4Source::open(proxy_path)?;
    let source_open_elapsed = source_open_start.elapsed();
    let proxy_video = proxy
        .video
        .as_ref()
        .ok_or("proxy has no H.264 video track")?;
    let proxy_h264 = classify_video_track(proxy_video)?;
    let access_units = proxy_video
        .samples
        .iter()
        .map(|sample| sample.annex_b.clone())
        .collect::<Vec<_>>();
    let frame_count = access_units.len();
    let rate = RationalRate::new(
        u64::from(proxy_video.frame_rate.numerator),
        u64::from(proxy_video.frame_rate.denominator),
    )?;
    let source_duration = rate.duration_for_frames(u64::try_from(frame_count)?)?;
    let presentation_rate = profile.presentation_rate(rate)?;
    let selected_frame_count = profile.selected_frame_count(frame_count);
    let target_duration =
        presentation_rate.duration_for_frames(u64::try_from(selected_frame_count)?)?;
    let config = PlaybackConfig::default().validate()?;

    println!("QGS bounded realtime proxy playback:");
    println!("  profile: {} ({})", profile.label(), profile.description());
    println!("  original: {}", case.original_label);
    println!("  proxy: {}", case.proxy_label);
    println!("  hashes: verified");
    println!("  original edit units: {}", original.index.video.len());
    println!(
        "  proxy: H.264 {:?}, {}-bit {:?}, visible {} x {}, coded {} x {}, {}/{} fps",
        proxy_h264.profile,
        proxy_h264.bit_depth,
        proxy_h264.chroma,
        proxy_h264.width,
        proxy_h264.height,
        proxy_h264.coded_width,
        proxy_h264.coded_height,
        rate.numerator(),
        rate.denominator()
    );
    println!("  source presentation frames: {frame_count}");
    println!("  source duration: {:.3}s", source_duration.as_secs_f64());
    println!(
        "  selected presentation frames: {selected_frame_count} at {}/{} fps target={:.3}s",
        presentation_rate.numerator(),
        presentation_rate.denominator(),
        target_duration.as_secs_f64()
    );
    println!(
        "  queue capacities: compressed={} decoded={} gpu={} presentation={} preroll={}",
        config.compressed_capacity,
        config.decoded_capacity,
        config.gpu_capacity,
        config.presentation_capacity,
        config.preroll_frames
    );

    let discovery = VulkanDeviceDiscovery::new()?;
    let devices = discovery.enumerate_devices()?;
    let Some(device) = devices
        .iter()
        .find(|device| {
            device.vendor_id == 0x8086 && matches!(device.class, DeviceClass::IntegratedGpu)
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::IntegratedGpu))
        })
    else {
        return Err("Intel VA proxy playback: no integrated GPU advertised".into());
    };

    let proxy_config = decoder_config_for_surface(
        device.id,
        proxy_h264.profile,
        BitDepth::new(proxy_h264.bit_depth)?,
        proxy_h264.chroma,
        proxy_h264.coded_width,
        proxy_h264.coded_height,
    );
    let vaapi = qgs_vaapi::VaapiVideoDiscovery::new(&devices);
    let capabilities = vaapi.query_video_capabilities(device.id)?;
    let supports_proxy = capabilities
        .decode
        .iter()
        .any(|capability| proxy_config.is_satisfied_by(capability));
    println!("Intel proxy capability:");
    println!("  device: {}", device.name);
    println!(
        "  H.264 {:?} {}-bit {:?}: {}",
        proxy_h264.profile,
        proxy_h264.bit_depth,
        proxy_h264.chroma,
        yes_no(supports_proxy)
    );
    if !supports_proxy {
        return Err("Intel VA backend does not support proxy configuration".into());
    }

    let decoder_create_start = Instant::now();
    let mut decoder = vaapi.create_decoder(&CreateDecoderRequest {
        config: proxy_config.clone(),
    })?;
    let decoder_create_elapsed = decoder_create_start.elapsed();
    let decoder_id = DecoderId::new(1)?;
    let visible_region = VisibleRegion {
        x: 0,
        y: 0,
        width: proxy_h264.width,
        height: proxy_h264.height,
    };
    let cpu_pool_capacity = 6_usize;
    let mut cpu_pool = qgs_vaapi::CpuNv12FramePool::new(
        proxy_h264.coded_width,
        proxy_h264.coded_height,
        visible_region,
        cpu_pool_capacity,
    )?;
    let mut gpu_processor = Nv12FrameProcessor::new(
        &discovery,
        Nv12FrameProcessorConfig {
            device_id: device.id,
            coded_width: proxy_h264.coded_width,
            coded_height: proxy_h264.coded_height,
            visible_width: proxy_h264.width,
            visible_height: proxy_h264.height,
            slot_count: config.gpu_capacity.max(1),
            conversion: YcbcrConversion::Rec709Limited,
            validation_readback: false,
        },
    )?;

    let playback_start = Instant::now();
    let mut clock = RealTimeClock::start_now();
    let mut sink = TestPresentationSink::new();
    let mut state = PlaybackState::Prerolling;
    let mut compressed_queue = BoundedQueue::new(config.compressed_capacity)?;
    let mut decoded_queue = BoundedQueue::new(config.decoded_capacity)?;
    let mut gpu_queue = BoundedQueue::new(config.gpu_capacity)?;
    let mut presentation_queue = BoundedQueue::new(config.presentation_capacity)?;
    let mut next_input = 0_usize;
    let mut next_output = 0_usize;
    let mut flushed = false;
    let mut access_units_submitted = 0_usize;
    let mut decoder_outputs = 0_usize;
    let mut intentionally_skipped_source_frames = 0_usize;
    let mut gpu_submissions = 0_usize;
    let mut gpu_completions = 0_usize;
    let mut decode_submit_elapsed = Duration::ZERO;
    let mut va_cpu_sync_copy_elapsed = Duration::ZERO;
    let mut gpu_submit_elapsed = Duration::ZERO;
    let mut gpu_completion_elapsed = Duration::ZERO;
    let mut presentation_wait_elapsed = Duration::ZERO;
    let mut max_lateness = Duration::ZERO;
    let mut lateness_values = Vec::new();
    let mut transfer_totals = Vec::new();
    let mut transfer_sync_times = Vec::new();
    let mut transfer_image_times = Vec::new();
    let mut transfer_copy_y_times = Vec::new();
    let mut transfer_copy_uv_times = Vec::new();
    let mut transfer_release_times = Vec::new();
    let mut transfer_bytes_copied = 0_usize;
    let mut first_va_image_layout = None;
    let mut pool_acquire_elapsed = Duration::ZERO;
    let mut upload_view_elapsed = Duration::ZERO;
    let mut pool_release_elapsed = Duration::ZERO;
    let mut gpu_submit_times = Vec::new();
    let mut gpu_completion_times = Vec::new();
    let progress_trace = std::env::var_os("QGS_STEP17_PROGRESS").is_some();
    let step17_diag = std::env::var_os("QGS_STEP17_DIAG").is_some();

    loop {
        while next_input < access_units.len() && !compressed_queue.is_full() {
            compressed_queue
                .try_push(ScheduledAccessUnit {
                    access_unit: access_units[next_input].clone(),
                })
                .map_err(|_| "compressed queue unexpectedly full")?;
            next_input += 1;
        }

        while !compressed_queue.is_empty() && !decoded_queue.is_full() {
            let scheduled = compressed_queue
                .pop_front()
                .ok_or("compressed queue unexpectedly empty")?;
            let started = Instant::now();
            let outputs = decoder.submit_access_unit(&SubmitAccessUnitRequest {
                decoder_id,
                data: scheduled.access_unit,
            })?;
            decode_submit_elapsed += started.elapsed();
            access_units_submitted += 1;
            for output in outputs {
                let source_position = u64::try_from(next_output)?;
                let presentation_position = profile.presentation_position(source_position);
                next_output += 1;
                decoded_queue
                    .try_push(DecodedPlaybackFrame {
                        source_position,
                        presentation_position,
                        surface: output,
                    })
                    .map_err(|_| "decoded queue unexpectedly full")?;
                decoder_outputs += 1;
            }
        }

        if next_input == access_units.len()
            && compressed_queue.is_empty()
            && !flushed
            && decoded_queue.len() < decoded_queue.capacity()
        {
            let started = Instant::now();
            let outputs = decoder.flush(&FlushDecoderRequest { decoder_id })?;
            decode_submit_elapsed += started.elapsed();
            flushed = true;
            for output in outputs {
                let source_position = u64::try_from(next_output)?;
                let presentation_position = profile.presentation_position(source_position);
                next_output += 1;
                decoded_queue
                    .try_push(DecodedPlaybackFrame {
                        source_position,
                        presentation_position,
                        surface: output,
                    })
                    .map_err(|_| "decoded queue unexpectedly full during flush")?;
                decoder_outputs += 1;
            }
        }

        while !decoded_queue.is_empty() && !gpu_queue.is_full() {
            let started = Instant::now();
            let decoded = decoded_queue
                .pop_front()
                .ok_or("decoded queue unexpectedly empty")?;
            let Some(presentation_position) = decoded.presentation_position else {
                intentionally_skipped_source_frames =
                    intentionally_skipped_source_frames.saturating_add(1);
                continue;
            };
            let identity = PlaybackFrameIdentity::from_position(presentation_position);
            let pool_started = Instant::now();
            let cpu_frame = cpu_pool.acquire()?;
            pool_acquire_elapsed += pool_started.elapsed();
            let transfer_started = Instant::now();
            if progress_trace {
                eprintln!(
                    "step17 progress: transfer start source={} presentation={}",
                    decoded.source_position, identity.presentation_position
                );
            }
            let (cpu_frame, transfer_timing) = qgs_vaapi::transfer_nv12_surface_timed(
                decoded.surface.resource.as_ref(),
                cpu_frame,
            )?;
            let transfer_elapsed = transfer_started.elapsed();
            va_cpu_sync_copy_elapsed += transfer_elapsed;
            transfer_totals.push(transfer_elapsed);
            transfer_sync_times.push(duration_from_ns(transfer_timing.sync_ns));
            transfer_image_times.push(duration_from_ns(transfer_timing.image_create_ns));
            transfer_copy_y_times.push(duration_from_ns(transfer_timing.copy_y_ns));
            transfer_copy_uv_times.push(duration_from_ns(transfer_timing.copy_uv_ns));
            transfer_release_times.push(duration_from_ns(transfer_timing.image_release_ns));
            transfer_bytes_copied =
                transfer_bytes_copied.saturating_add(transfer_timing.bytes_copied);
            if first_va_image_layout.is_none() {
                first_va_image_layout = transfer_timing.image_layout.clone();
            }
            if progress_trace {
                eprintln!(
                    "step17 progress: transfer done source={} presentation={}",
                    decoded.source_position, identity.presentation_position
                );
            }
            let upload_started = Instant::now();
            let upload = nv12_upload_for_cpu_surface(device.id, &cpu_frame)?;
            upload_view_elapsed += upload_started.elapsed();
            let token = match gpu_processor.submit_frame(
                &upload,
                FrameIdentity {
                    presentation_position: identity.presentation_position,
                },
            ) {
                Ok(token) => token,
                Err(FrameProcessorError::NoFrameSlotAvailable) => {
                    break;
                }
                Err(err) => return Err(Box::new(err)),
            };
            let release_started = Instant::now();
            cpu_pool.release(cpu_frame)?;
            pool_release_elapsed += release_started.elapsed();
            if progress_trace {
                eprintln!(
                    "step17 progress: gpu submit presentation={} token={}",
                    identity.presentation_position,
                    token.get()
                );
            }
            gpu_queue
                .try_push(GpuPendingPlaybackFrame { token, identity })
                .map_err(|_| "GPU queue unexpectedly full")?;
            gpu_submissions += 1;
            let submit_elapsed = started.elapsed();
            gpu_submit_elapsed += submit_elapsed;
            gpu_submit_times.push(submit_elapsed);
        }

        while !gpu_queue.is_empty() && !presentation_queue.is_full() {
            let started = Instant::now();
            let Some(front) = gpu_queue.front() else {
                break;
            };
            let completed = gpu_processor.poll_completed()?;
            if !completed.contains(&front.token) {
                let input_drained = flushed
                    && next_input == access_units.len()
                    && compressed_queue.is_empty()
                    && decoded_queue.is_empty();
                if gpu_queue.is_full() || input_drained {
                    gpu_processor.wait_for_completion(front.token)?;
                } else {
                    break;
                }
            }
            let processed = gpu_queue
                .pop_front()
                .ok_or("GPU queue unexpectedly empty")?;
            gpu_processor.retire_completed_token(processed.token)?;
            presentation_queue
                .try_push(ReadyPlaybackFrame {
                    identity: processed.identity,
                })
                .map_err(|_| "presentation queue unexpectedly full")?;
            if progress_trace {
                eprintln!(
                    "step17 progress: gpu complete presentation={} token={}",
                    processed.identity.presentation_position,
                    processed.token.get()
                );
            }
            gpu_completions += 1;
            let completion_elapsed = started.elapsed();
            gpu_completion_elapsed += completion_elapsed;
            gpu_completion_times.push(completion_elapsed);
        }

        if state == PlaybackState::Prerolling && presentation_queue.len() >= config.preroll_frames {
            state = PlaybackState::Playing;
            clock = RealTimeClock::start_now();
        }
        if state == PlaybackState::Prerolling && flushed && !presentation_queue.is_empty() {
            state = PlaybackState::Playing;
            clock = RealTimeClock::start_now();
        }

        if state == PlaybackState::Playing {
            if let Some(frame) = presentation_queue.front() {
                let expected =
                    presentation_rate.frame_offset(frame.identity.presentation_position)?;
                let wait_started = Instant::now();
                clock.sleep_until(expected);
                presentation_wait_elapsed += wait_started.elapsed();
                let actual = clock.now();
                let lateness = actual.saturating_sub(expected);
                max_lateness = max_lateness.max(lateness);
                lateness_values.push(lateness);
                let status = classify_presentation(lateness, config);
                let frame = presentation_queue
                    .pop_front()
                    .ok_or("presentation queue unexpectedly empty")?;
                sink.record(PresentationDecision {
                    identity: frame.identity,
                    expected,
                    actual,
                    lateness,
                    status,
                });
            }
        }

        if sink.decisions().len() == selected_frame_count {
            break;
        }

        if flushed
            && presentation_queue.is_empty()
            && decoded_queue.is_empty()
            && gpu_queue.is_empty()
        {
            state = PlaybackState::Completed;
            break;
        }
    }

    if state == PlaybackState::Playing {
        clock.sleep_until(target_duration);
        state = PlaybackState::Completed;
    }
    let playback_elapsed = playback_start.elapsed();

    if sink.decisions().len() != selected_frame_count {
        return Err(format!(
            "playback expected {selected_frame_count} presentation decisions, got {}",
            sink.decisions().len()
        )
        .into());
    }
    if decoder_outputs != frame_count {
        return Err(format!(
            "playback expected {frame_count} decoder outputs, got {decoder_outputs}"
        )
        .into());
    }

    let counts = sink.counts();
    let presented_total = counts.presented + counts.late;
    let mean_lateness = mean_duration(&lateness_values);
    let median_lateness = median_duration(&mut lateness_values);

    println!("Realtime playback result:");
    println!("  state: {:?}", state);
    println!("  access units submitted: {access_units_submitted}");
    println!("  decoder outputs: {decoder_outputs}");
    println!("  selected presentation frames: {selected_frame_count}");
    println!("  intentionally skipped source frames: {intentionally_skipped_source_frames}");
    println!("  GPU queue submissions: {gpu_submissions}");
    println!("  GPU queue completions: {gpu_completions}");
    println!(
        "  presentation decisions: {} presented={} on_time={} late={} dropped={} duplicated={}",
        sink.decisions().len(),
        presented_total,
        counts.presented,
        counts.late,
        counts.dropped,
        counts.duplicated
    );
    println!(
        "  playback wall-clock: {:.3}s target={:.3}s",
        playback_elapsed.as_secs_f64(),
        target_duration.as_secs_f64()
    );
    println!(
        "  lateness: max={:.3} ms mean={:.3} ms median={:.3} ms",
        duration_ms(max_lateness),
        duration_ms(mean_lateness),
        duration_ms(median_lateness)
    );
    println!(
        "  queue peaks: compressed={} decoded={} gpu={} presentation={}",
        compressed_queue.stats().peak_depth,
        decoded_queue.stats().peak_depth,
        gpu_queue.stats().peak_depth,
        presentation_queue.stats().peak_depth
    );
    println!(
        "  backpressure events: compressed={} decoded={} gpu={} presentation={}",
        compressed_queue.stats().backpressure_events,
        decoded_queue.stats().backpressure_events,
        gpu_queue.stats().backpressure_events,
        presentation_queue.stats().backpressure_events
    );
    let cpu_pool_stats = cpu_pool.stats();
    let gpu_counters = gpu_processor.counters();
    let gpu_diagnostics = gpu_processor.diagnostics();
    println!("  VA surfaces: transferred to bounded CPU NV12 pool before GPU upload; VA->Vulkan zero-copy remains frozen");
    println!(
        "  CPU NV12 pool: capacity={} bytes_per_frame={} peak_checked_out={} reused={}",
        cpu_pool_capacity,
        cpu_pool_stats.bytes_per_frame,
        cpu_pool_stats.peak_checked_out,
        cpu_pool_stats.frames_reused
    );
    println!(
        "  GPU processing: submissions={} completions={} slot_reuses={} staging_allocations={} gpu_plane_allocations={} output_allocations={}",
        gpu_submissions,
        gpu_completions,
        gpu_counters.slot_reuses,
        gpu_counters.staging_allocations,
        gpu_counters.gpu_plane_allocations,
        gpu_counters.output_allocations
    );
    println!("Stage timing observations:");
    println!(
        "  MP4 open/sample extraction: {:.3} ms",
        duration_ms(source_open_elapsed)
    );
    println!(
        "  VA decoder creation: {:.3} ms",
        duration_ms(decoder_create_elapsed)
    );
    println!(
        "  VA decode submit/flush aggregate: {:.3} ms",
        duration_ms(decode_submit_elapsed)
    );
    println!(
        "  VA sync + CPU transfer aggregate: {:.3} ms",
        duration_ms(va_cpu_sync_copy_elapsed)
    );
    print_duration_distribution("  VA transfer total/frame", &mut transfer_totals);
    print_duration_distribution("  VA transfer sync/frame", &mut transfer_sync_times);
    print_duration_distribution("  VA image derive/get/frame", &mut transfer_image_times);
    print_duration_distribution("  VA copy Y/frame", &mut transfer_copy_y_times);
    print_duration_distribution("  VA copy UV/frame", &mut transfer_copy_uv_times);
    print_duration_distribution("  VA image release/frame", &mut transfer_release_times);
    if let Some(layout) = &first_va_image_layout {
        print_va_nv12_image_layout("  VA NV12 image", layout);
    }
    println!(
        "  VA transfer bytes copied: total={} per_frame={}",
        transfer_bytes_copied,
        transfer_bytes_copied / selected_frame_count.max(1)
    );
    println!(
        "  CPU pool acquire/release/upload-view aggregate: acquire={:.3} ms upload_view={:.3} ms release={:.3} ms",
        duration_ms(pool_acquire_elapsed),
        duration_ms(upload_view_elapsed),
        duration_ms(pool_release_elapsed)
    );
    println!(
        "  decoded->GPU submit loop aggregate, including transfer: {:.3} ms",
        duration_ms(gpu_submit_elapsed)
    );
    print_duration_distribution(
        "  decoded->GPU submit loop/frame, including transfer",
        &mut gpu_submit_times,
    );
    println!(
        "  Vulkan completion handling aggregate: {:.3} ms",
        duration_ms(gpu_completion_elapsed)
    );
    print_duration_distribution("  Vulkan completion path/frame", &mut gpu_completion_times);
    print_nv12_gpu_diagnostics(&gpu_diagnostics);
    println!(
        "  presentation wait aggregate: {:.3} ms",
        duration_ms(presentation_wait_elapsed)
    );

    if step17_diag {
        run_step17_isolated_va_transfer_observation(
            &devices,
            device,
            proxy_config.clone(),
            &access_units,
            visible_region,
            source_duration,
        )?;
        run_step17_va_transfer_variant_observations(
            &devices,
            device,
            proxy_config.clone(),
            &access_units,
            visible_region,
            source_duration,
        )?;
        run_step17_as_fast_full_pixel_observation(
            &devices,
            &discovery,
            device,
            proxy_config,
            &access_units,
            visible_region,
            config,
            target_duration,
        )?;
        run_step17_synthetic_nv12_gpu_observation(
            &discovery,
            device,
            proxy_h264.coded_width,
            proxy_h264.coded_height,
            proxy_h264.width,
            proxy_h264.height,
            frame_count,
            target_duration,
        )?;
        if let Some(discrete_device) = devices
            .iter()
            .find(|candidate| matches!(candidate.class, DeviceClass::DiscreteGpu))
        {
            run_step17_synthetic_nv12_gpu_observation(
                &discovery,
                discrete_device,
                proxy_h264.coded_width,
                proxy_h264.coded_height,
                proxy_h264.width,
                proxy_h264.height,
                frame_count,
                target_duration,
            )?;
        }
        return Ok(());
    }

    if counts.dropped != 0 || counts.duplicated != 0 {
        return Err("real-time playback produced drops or duplicates".into());
    }
    if profile == ProxyPlaybackProfile::Journalist50iPreview
        && intentionally_skipped_source_frames != frame_count.saturating_sub(selected_frame_count)
    {
        return Err(
            "journalist-50i-preview profile skipped an unexpected number of source frames".into(),
        );
    }

    let selected = selected_proxy_ordinals(frame_count);
    let normal = qgs_vaapi::decode_h264_access_units_for_observation(
        &devices,
        device.id,
        &decoder_config_for_surface(
            device.id,
            proxy_h264.profile,
            BitDepth::new(proxy_h264.bit_depth)?,
            proxy_h264.chroma,
            proxy_h264.coded_width,
            proxy_h264.coded_height,
        ),
        &access_units,
        qgs_vaapi::VaapiDecodeMode::Normal,
        &selected,
    )?;
    println!("Clean VA ownership observation:");
    print_va_observation(&normal);

    let presentation_ordinals = proxy_presentation_ordinals(proxy_video)?;
    let proxy_positioned = proxy_video
        .samples
        .iter()
        .map(|sample| {
            Ok((
                *presentation_ordinals
                    .get(&sample.sample_index)
                    .ok_or("missing proxy playback presentation ordinal")?,
                sample.annex_b.clone(),
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let software_frames = decode_positioned_access_units_with_context(
        decoder_config_for_surface(
            device.id,
            proxy_h264.profile,
            BitDepth::new(proxy_h264.bit_depth)?,
            proxy_h264.chroma,
            proxy_h264.coded_width,
            proxy_h264.coded_height,
        ),
        &proxy_positioned,
    )?;
    println!("Selected-frame validation:");
    let mut software_by_presentation = software_frames.frames.iter().collect::<Vec<_>>();
    software_by_presentation.sort_by_key(|frame| frame.presentation_index);
    for ordinal in selected {
        let frame = software_frames
            .frames
            .iter()
            .find(|frame| frame.presentation_index == ordinal as u64)
            .or_else(|| software_by_presentation.get(ordinal).copied())
            .ok_or_else(|| format!("missing software validation frame {ordinal}"))?;
        run_proxy_gpu_frame(
            &discovery,
            device,
            &format!("proxy-requested-{ordinal}"),
            frame,
        )?;
    }

    Ok(())
}

struct ScheduledAccessUnit {
    access_unit: Vec<u8>,
}

struct DecodedPlaybackFrame {
    source_position: u64,
    presentation_position: Option<u64>,
    surface: BackendDecodedSurface,
}

struct ReadyPlaybackFrame {
    identity: PlaybackFrameIdentity,
}

struct GpuPendingPlaybackFrame {
    token: qgs_vulkan::FrameToken,
    identity: PlaybackFrameIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProxyPlaybackProfile {
    SourceRate,
    Journalist50iPreview,
}

impl ProxyPlaybackProfile {
    fn parse(value: &std::ffi::OsStr) -> Result<Self, Box<dyn std::error::Error>> {
        match value.to_str() {
            Some("source-rate") => Ok(Self::SourceRate),
            Some("journalist-50i-preview") => Ok(Self::Journalist50iPreview),
            Some(other) => Err(format!("unsupported proxy playback profile: {other}").into()),
            None => Err("proxy playback profile must be valid UTF-8".into()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SourceRate => "source-rate",
            Self::Journalist50iPreview => "journalist-50i-preview",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::SourceRate => "source-rate realtime proxy playback",
            Self::Journalist50iPreview => {
                "50i-compatible broadcast journalist preview over 50p source"
            }
        }
    }

    fn presentation_rate(
        self,
        source_rate: RationalRate,
    ) -> Result<RationalRate, Box<dyn std::error::Error>> {
        match self {
            Self::SourceRate => Ok(source_rate),
            Self::Journalist50iPreview => Ok(RationalRate::new(25, 1)?),
        }
    }

    fn presentation_position(self, source_position: u64) -> Option<u64> {
        match self {
            Self::SourceRate => Some(source_position),
            Self::Journalist50iPreview => source_position
                .is_multiple_of(2)
                .then_some(source_position / 2),
        }
    }

    fn selected_frame_count(self, source_frame_count: usize) -> usize {
        match self {
            Self::SourceRate => source_frame_count,
            Self::Journalist50iPreview => source_frame_count.div_ceil(2),
        }
    }
}

fn nv12_upload_for_cpu_surface<'a>(
    device_id: qgs_protocol::DeviceId,
    surface: &'a qgs_vaapi::CpuNv12Surface,
) -> Result<Nv12Upload<'a>, Box<dyn std::error::Error>> {
    Ok(Nv12Upload {
        device_id,
        coded_width: surface.desc.coded_width,
        coded_height: surface.desc.coded_height,
        visible_width: surface.desc.visible_region.width,
        visible_height: surface.desc.visible_region.height,
        y: Nv12Plane {
            width_bytes: u32::try_from(surface.y.width_bytes)?,
            height: u32::try_from(surface.y.height)?,
            stride_bytes: surface.y.stride_bytes,
            data: &surface.y.data,
        },
        uv: Nv12Plane {
            width_bytes: u32::try_from(surface.uv.width_bytes)?,
            height: u32::try_from(surface.uv.height)?,
            stride_bytes: surface.uv.stride_bytes,
            data: &surface.uv.data,
        },
        conversion: YcbcrConversion::Rec709Limited,
    })
}

fn run_step17_isolated_va_transfer_observation(
    devices: &[DeviceDesc],
    device: &DeviceDesc,
    proxy_config: DecoderConfig,
    access_units: &[Vec<u8>],
    visible_region: VisibleRegion,
    source_duration: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let vaapi = qgs_vaapi::VaapiVideoDiscovery::new(devices);
    let mut decoder = vaapi.create_decoder(&CreateDecoderRequest {
        config: proxy_config.clone(),
    })?;
    let decoder_id = DecoderId::new(17)?;
    let mut cpu_pool = qgs_vaapi::CpuNv12FramePool::new(
        proxy_config.coded_width,
        proxy_config.coded_height,
        visible_region,
        6,
    )?;
    let mut decode_elapsed = Duration::ZERO;
    let mut transfer_elapsed = Duration::ZERO;
    let mut frames = 0_usize;
    let mut transfer_totals = Vec::new();
    let started = Instant::now();
    for access_unit in access_units {
        let decode_started = Instant::now();
        let outputs = decoder.submit_access_unit(&SubmitAccessUnitRequest {
            decoder_id,
            data: access_unit.clone(),
        })?;
        decode_elapsed += decode_started.elapsed();
        for output in outputs {
            let frame = cpu_pool.acquire()?;
            let transfer_started = Instant::now();
            let (frame, _timing) =
                qgs_vaapi::transfer_nv12_surface_timed(output.resource.as_ref(), frame)?;
            let elapsed = transfer_started.elapsed();
            transfer_elapsed += elapsed;
            transfer_totals.push(elapsed);
            cpu_pool.release(frame)?;
            frames += 1;
        }
    }
    let decode_started = Instant::now();
    let outputs = decoder.flush(&FlushDecoderRequest { decoder_id })?;
    decode_elapsed += decode_started.elapsed();
    for output in outputs {
        let frame = cpu_pool.acquire()?;
        let transfer_started = Instant::now();
        let (frame, _timing) =
            qgs_vaapi::transfer_nv12_surface_timed(output.resource.as_ref(), frame)?;
        let elapsed = transfer_started.elapsed();
        transfer_elapsed += elapsed;
        transfer_totals.push(elapsed);
        cpu_pool.release(frame)?;
        frames += 1;
    }
    let elapsed = started.elapsed();
    println!("Step 17 isolated VA decode + CPU NV12 transfer:");
    println!(
        "  device: {} frames={} wall={:.3} ms source={:.3}s throughput={:.2} fps realtime={:.2}x",
        device.name,
        frames,
        duration_ms(elapsed),
        source_duration.as_secs_f64(),
        frames as f64 / elapsed.as_secs_f64().max(0.000_001),
        source_duration.as_secs_f64() / elapsed.as_secs_f64().max(0.000_001)
    );
    println!(
        "  decode aggregate={:.3} ms transfer aggregate={:.3} ms pool_reuses={}",
        duration_ms(decode_elapsed),
        duration_ms(transfer_elapsed),
        cpu_pool.stats().frames_reused
    );
    print_duration_distribution("  isolated VA transfer/frame", &mut transfer_totals);
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Step17TransferVariant {
    TightCoded,
    TightVisible,
    SourcePitchCoded,
    FullSourceRows,
}

impl Step17TransferVariant {
    const ALL: [Self; 4] = [
        Self::TightCoded,
        Self::TightVisible,
        Self::SourcePitchCoded,
        Self::FullSourceRows,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::TightCoded => "tight-coded",
            Self::TightVisible => "tight-visible",
            Self::SourcePitchCoded => "source-pitch-coded",
            Self::FullSourceRows => "full-source-rows",
        }
    }

    fn layout(
        self,
        proxy_config: &DecoderConfig,
        visible_region: VisibleRegion,
        image_layout: &qgs_vaapi::VaNv12ImageLayout,
    ) -> Result<qgs_vaapi::CpuNv12FrameLayout, Box<dyn std::error::Error>> {
        Ok(match self {
            Self::TightCoded => qgs_vaapi::CpuNv12FrameLayout::tight_coded(
                proxy_config.coded_width,
                proxy_config.coded_height,
            )?,
            Self::TightVisible => qgs_vaapi::CpuNv12FrameLayout::tight_visible(visible_region)?,
            Self::SourcePitchCoded => qgs_vaapi::CpuNv12FrameLayout::coded_with_destination_pitch(
                proxy_config.coded_width,
                proxy_config.coded_height,
                image_layout.pitches[0],
                image_layout.pitches[1],
            )?,
            Self::FullSourceRows => qgs_vaapi::CpuNv12FrameLayout::full_source_rows(
                usize::try_from(image_layout.height)?,
                image_layout.pitches[0],
                usize::try_from(image_layout.height)? / 2,
                image_layout.pitches[1],
            )?,
        })
    }
}

fn run_step17_va_transfer_variant_observations(
    devices: &[DeviceDesc],
    device: &DeviceDesc,
    proxy_config: DecoderConfig,
    access_units: &[Vec<u8>],
    visible_region: VisibleRegion,
    source_duration: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let image_layout =
        probe_step17_first_va_nv12_layout(devices, proxy_config.clone(), access_units)?;
    println!("Step 17C VA NV12 image layout:");
    print_va_nv12_image_layout("  probed VA NV12 image", &image_layout);

    for variant in Step17TransferVariant::ALL {
        let layout = variant.layout(&proxy_config, visible_region, &image_layout)?;
        run_step17_va_transfer_variant(
            devices,
            device,
            proxy_config.clone(),
            access_units,
            visible_region,
            source_duration,
            variant,
            layout,
        )?;
    }
    Ok(())
}

fn probe_step17_first_va_nv12_layout(
    devices: &[DeviceDesc],
    proxy_config: DecoderConfig,
    access_units: &[Vec<u8>],
) -> Result<qgs_vaapi::VaNv12ImageLayout, Box<dyn std::error::Error>> {
    let vaapi = qgs_vaapi::VaapiVideoDiscovery::new(devices);
    let mut decoder = vaapi.create_decoder(&CreateDecoderRequest {
        config: proxy_config,
    })?;
    let decoder_id = DecoderId::new(17)?;
    for access_unit in access_units {
        let outputs = decoder.submit_access_unit(&SubmitAccessUnitRequest {
            decoder_id,
            data: access_unit.clone(),
        })?;
        if let Some(output) = outputs.first() {
            return Ok(qgs_vaapi::describe_nv12_surface_image(
                output.resource.as_ref(),
            )?);
        }
    }
    let outputs = decoder.flush(&FlushDecoderRequest { decoder_id })?;
    let output = outputs
        .first()
        .ok_or("proxy decode did not produce a surface to inspect")?;
    Ok(qgs_vaapi::describe_nv12_surface_image(
        output.resource.as_ref(),
    )?)
}

fn run_step17_va_transfer_variant(
    devices: &[DeviceDesc],
    device: &DeviceDesc,
    proxy_config: DecoderConfig,
    access_units: &[Vec<u8>],
    visible_region: VisibleRegion,
    source_duration: Duration,
    variant: Step17TransferVariant,
    layout: qgs_vaapi::CpuNv12FrameLayout,
) -> Result<(), Box<dyn std::error::Error>> {
    let vaapi = qgs_vaapi::VaapiVideoDiscovery::new(devices);
    let mut decoder = vaapi.create_decoder(&CreateDecoderRequest {
        config: proxy_config.clone(),
    })?;
    let decoder_id = DecoderId::new(17)?;
    let mut cpu_pool = qgs_vaapi::CpuNv12FramePool::new_with_layout(
        proxy_config.coded_width,
        proxy_config.coded_height,
        visible_region,
        6,
        layout,
    )?;
    let mut decode_elapsed = Duration::ZERO;
    let mut transfer_elapsed = Duration::ZERO;
    let mut frames = 0_usize;
    let mut bytes_copied = 0_usize;
    let mut transfer_totals = Vec::new();
    let mut sync_times = Vec::new();
    let mut image_times = Vec::new();
    let mut copy_y_times = Vec::new();
    let mut copy_uv_times = Vec::new();
    let mut release_times = Vec::new();
    let started = Instant::now();
    for access_unit in access_units {
        let decode_started = Instant::now();
        let outputs = decoder.submit_access_unit(&SubmitAccessUnitRequest {
            decoder_id,
            data: access_unit.clone(),
        })?;
        decode_elapsed += decode_started.elapsed();
        for output in outputs {
            let frame = cpu_pool.acquire()?;
            let transfer_started = Instant::now();
            let (frame, timing) =
                qgs_vaapi::transfer_nv12_surface_timed(output.resource.as_ref(), frame)?;
            let elapsed = transfer_started.elapsed();
            transfer_elapsed += elapsed;
            transfer_totals.push(elapsed);
            sync_times.push(duration_from_ns(timing.sync_ns));
            image_times.push(duration_from_ns(timing.image_create_ns));
            copy_y_times.push(duration_from_ns(timing.copy_y_ns));
            copy_uv_times.push(duration_from_ns(timing.copy_uv_ns));
            release_times.push(duration_from_ns(timing.image_release_ns));
            bytes_copied = bytes_copied.saturating_add(timing.bytes_copied);
            cpu_pool.release(frame)?;
            frames += 1;
        }
    }
    let decode_started = Instant::now();
    let outputs = decoder.flush(&FlushDecoderRequest { decoder_id })?;
    decode_elapsed += decode_started.elapsed();
    for output in outputs {
        let frame = cpu_pool.acquire()?;
        let transfer_started = Instant::now();
        let (frame, timing) =
            qgs_vaapi::transfer_nv12_surface_timed(output.resource.as_ref(), frame)?;
        let elapsed = transfer_started.elapsed();
        transfer_elapsed += elapsed;
        transfer_totals.push(elapsed);
        sync_times.push(duration_from_ns(timing.sync_ns));
        image_times.push(duration_from_ns(timing.image_create_ns));
        copy_y_times.push(duration_from_ns(timing.copy_y_ns));
        copy_uv_times.push(duration_from_ns(timing.copy_uv_ns));
        release_times.push(duration_from_ns(timing.image_release_ns));
        bytes_copied = bytes_copied.saturating_add(timing.bytes_copied);
        cpu_pool.release(frame)?;
        frames += 1;
    }
    let elapsed = started.elapsed();
    println!("Step 17C VA transfer variant: {}", variant.label());
    println!(
        "  layout: y width={} height={} stride={} uv width={} height={} stride={} bytes/frame={}",
        layout.y_width_bytes,
        layout.y_height,
        layout.y_stride_bytes,
        layout.uv_width_bytes,
        layout.uv_height,
        layout.uv_stride_bytes,
        cpu_pool.stats().bytes_per_frame
    );
    println!(
        "  device: {} frames={} wall={:.3} ms source={:.3}s throughput={:.2} fps realtime={:.2}x",
        device.name,
        frames,
        duration_ms(elapsed),
        source_duration.as_secs_f64(),
        frames as f64 / elapsed.as_secs_f64().max(0.000_001),
        source_duration.as_secs_f64() / elapsed.as_secs_f64().max(0.000_001)
    );
    println!(
        "  decode aggregate={:.3} ms transfer aggregate={:.3} ms bytes_copied/frame={} pool_reuses={}",
        duration_ms(decode_elapsed),
        duration_ms(transfer_elapsed),
        bytes_copied / frames.max(1),
        cpu_pool.stats().frames_reused
    );
    print_duration_distribution("  variant VA transfer/frame", &mut transfer_totals);
    print_duration_distribution("  variant VA sync/frame", &mut sync_times);
    print_duration_distribution("  variant VA image derive/get/frame", &mut image_times);
    print_duration_distribution("  variant VA copy Y/frame", &mut copy_y_times);
    print_duration_distribution("  variant VA copy UV/frame", &mut copy_uv_times);
    print_duration_distribution("  variant VA image release/frame", &mut release_times);
    Ok(())
}

fn print_va_nv12_image_layout(label: &str, layout: &qgs_vaapi::VaNv12ImageLayout) {
    println!(
        "{label}: fourcc={} width={} height={} planes={} data_size={} derived={}",
        layout.fourcc_string(),
        layout.width,
        layout.height,
        layout.num_planes,
        layout.data_size,
        yes_no(layout.derived)
    );
    println!(
        "    offsets={:?} pitches={:?} y_uv_contiguous={}",
        layout.offsets,
        layout.pitches,
        yes_no(layout.y_uv_contiguous())
    );
}

fn run_step17_as_fast_full_pixel_observation(
    devices: &[DeviceDesc],
    discovery: &VulkanDeviceDiscovery,
    device: &DeviceDesc,
    proxy_config: DecoderConfig,
    access_units: &[Vec<u8>],
    visible_region: VisibleRegion,
    config: PlaybackConfig,
    source_duration: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let vaapi = qgs_vaapi::VaapiVideoDiscovery::new(devices);
    let mut decoder = vaapi.create_decoder(&CreateDecoderRequest {
        config: proxy_config.clone(),
    })?;
    let decoder_id = DecoderId::new(17)?;
    let mut cpu_pool = qgs_vaapi::CpuNv12FramePool::new(
        proxy_config.coded_width,
        proxy_config.coded_height,
        visible_region,
        6,
    )?;
    let mut gpu_processor = Nv12FrameProcessor::new(
        discovery,
        Nv12FrameProcessorConfig {
            device_id: device.id,
            coded_width: proxy_config.coded_width,
            coded_height: proxy_config.coded_height,
            visible_width: visible_region.width,
            visible_height: visible_region.height,
            slot_count: config.gpu_capacity.max(1),
            conversion: YcbcrConversion::Rec709Limited,
            validation_readback: false,
        },
    )?;
    let mut compressed_queue = BoundedQueue::new(config.compressed_capacity)?;
    let mut decoded_queue = BoundedQueue::new(config.decoded_capacity)?;
    let mut gpu_queue = BoundedQueue::new(config.gpu_capacity)?;
    let mut next_input = 0_usize;
    let mut next_output = 0_usize;
    let mut flushed = false;
    let mut access_units_submitted = 0_usize;
    let mut decoder_outputs = 0_usize;
    let mut gpu_submissions = 0_usize;
    let mut gpu_completions = 0_usize;
    let mut decode_elapsed = Duration::ZERO;
    let mut transfer_elapsed = Duration::ZERO;
    let mut gpu_submit_elapsed = Duration::ZERO;
    let mut gpu_completion_elapsed = Duration::ZERO;
    let mut transfer_bytes = 0_usize;
    let started = Instant::now();

    loop {
        while next_input < access_units.len() && !compressed_queue.is_full() {
            compressed_queue
                .try_push(ScheduledAccessUnit {
                    access_unit: access_units[next_input].clone(),
                })
                .map_err(|_| "compressed queue unexpectedly full")?;
            next_input += 1;
        }

        while !compressed_queue.is_empty() && !decoded_queue.is_full() {
            let scheduled = compressed_queue
                .pop_front()
                .ok_or("compressed queue unexpectedly empty")?;
            let decode_started = Instant::now();
            let outputs = decoder.submit_access_unit(&SubmitAccessUnitRequest {
                decoder_id,
                data: scheduled.access_unit,
            })?;
            decode_elapsed += decode_started.elapsed();
            access_units_submitted += 1;
            for output in outputs {
                let source_position = u64::try_from(next_output)?;
                next_output += 1;
                decoded_queue
                    .try_push(DecodedPlaybackFrame {
                        source_position,
                        presentation_position: Some(source_position),
                        surface: output,
                    })
                    .map_err(|_| "decoded queue unexpectedly full")?;
                decoder_outputs += 1;
            }
        }

        if next_input == access_units.len()
            && compressed_queue.is_empty()
            && !flushed
            && decoded_queue.len() < decoded_queue.capacity()
        {
            let decode_started = Instant::now();
            let outputs = decoder.flush(&FlushDecoderRequest { decoder_id })?;
            decode_elapsed += decode_started.elapsed();
            flushed = true;
            for output in outputs {
                let source_position = u64::try_from(next_output)?;
                next_output += 1;
                decoded_queue
                    .try_push(DecodedPlaybackFrame {
                        source_position,
                        presentation_position: Some(source_position),
                        surface: output,
                    })
                    .map_err(|_| "decoded queue unexpectedly full during flush")?;
                decoder_outputs += 1;
            }
        }

        while !decoded_queue.is_empty() && !gpu_queue.is_full() {
            let submit_started = Instant::now();
            let decoded = decoded_queue
                .pop_front()
                .ok_or("decoded queue unexpectedly empty")?;
            let presentation_position = decoded
                .presentation_position
                .ok_or("as-fast diagnostic expected selected frame")?;
            let identity = PlaybackFrameIdentity::from_position(presentation_position);
            let cpu_frame = cpu_pool.acquire()?;
            let transfer_started = Instant::now();
            let (cpu_frame, transfer_timing) = qgs_vaapi::transfer_nv12_surface_timed(
                decoded.surface.resource.as_ref(),
                cpu_frame,
            )?;
            transfer_elapsed += transfer_started.elapsed();
            transfer_bytes = transfer_bytes.saturating_add(transfer_timing.bytes_copied);
            let upload = nv12_upload_for_cpu_surface(device.id, &cpu_frame)?;
            let token = gpu_processor.submit_frame(
                &upload,
                FrameIdentity {
                    presentation_position: identity.presentation_position,
                },
            )?;
            cpu_pool.release(cpu_frame)?;
            gpu_queue
                .try_push(GpuPendingPlaybackFrame { token, identity })
                .map_err(|_| "GPU queue unexpectedly full")?;
            gpu_submissions += 1;
            gpu_submit_elapsed += submit_started.elapsed();
        }

        while !gpu_queue.is_empty() {
            let completion_started = Instant::now();
            let Some(front) = gpu_queue.front() else {
                break;
            };
            let completed = gpu_processor.poll_completed()?;
            if !completed.contains(&front.token) {
                if gpu_queue.is_full()
                    || (flushed && compressed_queue.is_empty() && decoded_queue.is_empty())
                {
                    gpu_processor.wait_for_completion(front.token)?;
                } else {
                    break;
                }
            }
            let processed = gpu_queue
                .pop_front()
                .ok_or("GPU queue unexpectedly empty")?;
            gpu_processor.retire_completed_token(processed.token)?;
            gpu_completions += 1;
            gpu_completion_elapsed += completion_started.elapsed();
        }

        if flushed
            && next_input == access_units.len()
            && compressed_queue.is_empty()
            && decoded_queue.is_empty()
            && gpu_queue.is_empty()
        {
            break;
        }
    }

    let elapsed = started.elapsed();
    println!("Step 17C as-fast full pixel path:");
    println!(
        "  device: {} frames={} wall={:.3} ms source={:.3}s throughput={:.2} fps realtime={:.2}x",
        device.name,
        gpu_completions,
        duration_ms(elapsed),
        source_duration.as_secs_f64(),
        gpu_completions as f64 / elapsed.as_secs_f64().max(0.000_001),
        source_duration.as_secs_f64() / elapsed.as_secs_f64().max(0.000_001)
    );
    println!(
        "  access_units={} decoder_outputs={} gpu_submissions={} gpu_completions={}",
        access_units_submitted, decoder_outputs, gpu_submissions, gpu_completions
    );
    println!(
        "  stage totals: decode={:.3} ms transfer={:.3} ms gpu_submit_loop={:.3} ms gpu_completion={:.3} ms bytes_copied/frame={}",
        duration_ms(decode_elapsed),
        duration_ms(transfer_elapsed),
        duration_ms(gpu_submit_elapsed),
        duration_ms(gpu_completion_elapsed),
        transfer_bytes / gpu_completions.max(1)
    );
    println!(
        "  queue peaks: compressed={} decoded={} gpu={}",
        compressed_queue.stats().peak_depth,
        decoded_queue.stats().peak_depth,
        gpu_queue.stats().peak_depth
    );
    print_nv12_gpu_diagnostics(&gpu_processor.diagnostics());
    Ok(())
}

fn run_step17_synthetic_nv12_gpu_observation(
    discovery: &VulkanDeviceDiscovery,
    device: &DeviceDesc,
    coded_width: u32,
    coded_height: u32,
    visible_width: u32,
    visible_height: u32,
    frame_count: usize,
    source_duration: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let y_len = usize::try_from(coded_width)?
        .checked_mul(usize::try_from(coded_height)?)
        .ok_or("synthetic NV12 Y size overflow")?;
    let uv_len = usize::try_from(coded_width)?
        .checked_mul(usize::try_from(coded_height / 2)?)
        .ok_or("synthetic NV12 UV size overflow")?;
    let y = vec![96_u8; y_len];
    let uv = vec![128_u8; uv_len];
    let upload = Nv12Upload {
        device_id: device.id,
        coded_width,
        coded_height,
        visible_width,
        visible_height,
        y: Nv12Plane {
            width_bytes: coded_width,
            height: coded_height,
            stride_bytes: usize::try_from(coded_width)?,
            data: &y,
        },
        uv: Nv12Plane {
            width_bytes: coded_width,
            height: coded_height / 2,
            stride_bytes: usize::try_from(coded_width)?,
            data: &uv,
        },
        conversion: YcbcrConversion::Rec709Limited,
    };
    let mut processor = Nv12FrameProcessor::new(
        discovery,
        Nv12FrameProcessorConfig {
            device_id: device.id,
            coded_width,
            coded_height,
            visible_width,
            visible_height,
            slot_count: 3,
            conversion: YcbcrConversion::Rec709Limited,
            validation_readback: false,
        },
    )?;
    let mut pending = VecDeque::new();
    let mut submit_times = Vec::new();
    let mut wait_times = Vec::new();
    let started = Instant::now();
    for index in 0..frame_count {
        loop {
            let submit_started = Instant::now();
            match processor.submit_frame(
                &upload,
                FrameIdentity {
                    presentation_position: u64::try_from(index)?,
                },
            ) {
                Ok(token) => {
                    submit_times.push(submit_started.elapsed());
                    pending.push_back(token);
                    break;
                }
                Err(FrameProcessorError::NoFrameSlotAvailable) => {
                    let token = pending
                        .pop_front()
                        .ok_or("synthetic GPU pending queue empty")?;
                    let wait_started = Instant::now();
                    processor.wait_for_completion(token)?;
                    processor.retire_completed_token(token)?;
                    wait_times.push(wait_started.elapsed());
                }
                Err(err) => return Err(Box::new(err)),
            }
        }
    }
    while let Some(token) = pending.pop_front() {
        let wait_started = Instant::now();
        processor.wait_for_completion(token)?;
        processor.retire_completed_token(token)?;
        wait_times.push(wait_started.elapsed());
    }
    let elapsed = started.elapsed();
    println!("Step 17 isolated synthetic CPU NV12 -> Vulkan processing:");
    println!(
        "  device: {} frames={} wall={:.3} ms source={:.3}s throughput={:.2} fps realtime={:.2}x",
        device.name,
        frame_count,
        duration_ms(elapsed),
        source_duration.as_secs_f64(),
        frame_count as f64 / elapsed.as_secs_f64().max(0.000_001),
        source_duration.as_secs_f64() / elapsed.as_secs_f64().max(0.000_001)
    );
    print_duration_distribution("  isolated Vulkan submit/frame", &mut submit_times);
    print_duration_distribution("  isolated Vulkan wait/retire", &mut wait_times);
    print_nv12_gpu_diagnostics(&processor.diagnostics());
    Ok(())
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn duration_from_ns(ns: u128) -> Duration {
    Duration::from_nanos(u64::try_from(ns).unwrap_or(u64::MAX))
}

fn mean_duration(values: &[Duration]) -> Duration {
    if values.is_empty() {
        return Duration::ZERO;
    }
    let total_ns = values
        .iter()
        .map(Duration::as_nanos)
        .fold(0_u128, |acc, value| acc.saturating_add(value));
    let mean_ns = total_ns / values.len() as u128;
    Duration::from_nanos(u64::try_from(mean_ns).unwrap_or(u64::MAX))
}

fn median_duration(values: &mut [Duration]) -> Duration {
    if values.is_empty() {
        return Duration::ZERO;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

fn print_duration_distribution(label: &str, values: &mut [Duration]) {
    if values.is_empty() {
        println!("{label}: no samples");
        return;
    }
    let total = values
        .iter()
        .copied()
        .fold(Duration::ZERO, |acc, value| acc.saturating_add(value));
    let avg = Duration::from_nanos(
        u64::try_from(total.as_nanos() / values.len() as u128).unwrap_or(u64::MAX),
    );
    values.sort_unstable();
    let median = values[values.len() / 2];
    let p90 = values[((values.len() * 9) / 10).min(values.len() - 1)];
    let max = *values.last().expect("nonempty");
    println!(
        "{label}: total={:.3} ms avg={:.3} ms median={:.3} ms p90={:.3} ms max={:.3} ms samples={}",
        duration_ms(total),
        duration_ms(avg),
        duration_ms(median),
        duration_ms(p90),
        duration_ms(max),
        values.len()
    );
}

fn print_nv12_gpu_diagnostics(diagnostics: &Nv12FrameProcessorDiagnostics) {
    println!("  NV12 Vulkan processor diagnostics:");
    println!(
        "    calls: submit={} poll={} fence_wait={} retire={} readback_enabled={}",
        diagnostics.submit_calls,
        diagnostics.poll_calls,
        diagnostics.fence_wait_calls,
        diagnostics.retire_calls,
        yes_no(diagnostics.output_readback_enabled)
    );
    println!(
        "    CPU prep: slot_acquire={:.3} ms compact_y_copy={:.3} ms compact_uv_copy={:.3} ms staging_write_y={:.3} ms staging_write_uv={:.3} ms",
        ns_ms(diagnostics.slot_acquire_ns),
        ns_ms(diagnostics.compact_y_copy_ns),
        ns_ms(diagnostics.compact_uv_copy_ns),
        ns_ms(diagnostics.staging_write_y_ns),
        ns_ms(diagnostics.staging_write_uv_ns)
    );
    println!(
        "    command/submit: pool_reset={:.3} ms fence_reset={:.3} ms begin={:.3} ms record={:.3} ms end={:.3} ms queue_submit={:.3} ms",
        ns_ms(diagnostics.command_pool_reset_ns),
        ns_ms(diagnostics.fence_reset_ns),
        ns_ms(diagnostics.command_begin_ns),
        ns_ms(diagnostics.command_record_ns),
        ns_ms(diagnostics.command_end_ns),
        ns_ms(diagnostics.queue_submit_ns)
    );
    println!(
        "    completion: poll={:.3} ms fence_wait={:.3} ms retire={:.3} ms",
        ns_ms(diagnostics.poll_ns),
        ns_ms(diagnostics.fence_wait_ns),
        ns_ms(diagnostics.retire_ns)
    );
}

fn ns_ms(ns: u128) -> f64 {
    ns as f64 / 1_000_000.0
}

struct FrontendOnlyObservation {
    frames: usize,
    peak_dpb_occupancy: usize,
    peak_output_pending: usize,
}

fn run_proxy_frontend_only(
    video: &qgs_mp4::Mp4VideoTrack,
) -> Result<FrontendOnlyObservation, Box<dyn std::error::Error>> {
    let mut state = qgs_codec_h264::H264DecoderState::new();
    let mut frames = 0_usize;
    for sample in &video.samples {
        let parsed = state.parse_access_unit(&sample.annex_b)?;
        let update = state.finish_picture(&parsed)?;
        frames = frames.saturating_add(update.output_ready.len());
    }
    let update = state.flush();
    frames = frames.saturating_add(update.output_ready.len());
    Ok(FrontendOnlyObservation {
        frames,
        peak_dpb_occupancy: state.max_dpb_occupancy(),
        peak_output_pending: state.max_output_pending(),
    })
}

fn selected_proxy_ordinals(frame_count: usize) -> Vec<usize> {
    if frame_count == 0 {
        return Vec::new();
    }
    let mut selected = vec![0, 53.min(frame_count - 1), frame_count - 1];
    selected.sort_unstable();
    selected.dedup();
    selected
}

fn print_decode_timing(
    label: &str,
    frames: usize,
    source_seconds: f64,
    elapsed: std::time::Duration,
) {
    let seconds = elapsed.as_secs_f64().max(0.000_001);
    let fps = frames as f64 / seconds;
    let realtime = if source_seconds > 0.0 {
        source_seconds / seconds
    } else {
        0.0
    };
    println!("{label}:");
    println!("  frames: {frames}");
    println!(
        "  DEVELOPMENT OBSERVATION - NOT A BENCHMARK: {:.3}s, {:.2} fps, {:.2}x realtime",
        seconds, fps, realtime
    );
}

fn print_va_observation(observation: &qgs_vaapi::VaapiDecodeObservation) {
    println!(
        "  pool: allocated={} reused={} recycled={} recycle_sync={} deferred={} peak_checked_out={} peak_pending_recycle={} min_free={}",
        observation.pool_stats.surfaces_allocated,
        observation.pool_stats.surface_reuse_count,
        observation.pool_stats.surfaces_recycled,
        observation.pool_stats.recycle_sync_count,
        observation.pool_stats.deferred_recycle_count,
        observation.pool_stats.peak_checked_out_surfaces,
        observation.pool_stats.peak_pending_recycle_surfaces,
        observation.pool_stats.minimum_free_surfaces
    );
    println!(
        "  peaks: DPB={} output_pending={} live_va_surfaces={} submitted_unsynced={} client_held_outputs={}",
        observation.peak_dpb_occupancy,
        observation.peak_output_pending,
        observation.peak_live_surfaces,
        observation.peak_submitted_unsynced_surfaces,
        observation.max_client_held_outputs
    );
    println!(
        "  diagnostics: frames={} export_probes={}",
        observation.diagnostic_frames, observation.diagnostic_export_probes
    );
}

fn print_va_timing(timing: &qgs_vaapi::VaapiDecodeTiming) {
    let total = timing.submit_total_ns.max(1);
    println!("  aggregate VA/backend timing:");
    print_timing_bucket("surface acquisition", timing.surface_acquire_ns, total);
    print_timing_bucket("H.264 frontend parse", timing.h264_parse_ns, total);
    print_timing_bucket("parameter/list build", timing.parameter_build_ns, total);
    print_timing_bucket("VA buffer creation", timing.buffer_create_ns, total);
    print_timing_bucket("vaBeginPicture", timing.begin_picture_ns, total);
    print_timing_bucket("vaRenderPicture", timing.render_picture_ns, total);
    print_timing_bucket("vaEndPicture", timing.end_picture_ns, total);
    print_timing_bucket("inline surface sync", timing.inline_sync_ns, total);
    print_timing_bucket("reclaim surface sync", timing.reclaim_sync_ns, total);
    print_timing_bucket("diagnostics", timing.diagnostics_ns, total);
    print_timing_bucket("finish picture", timing.finish_picture_ns, total);
    print_timing_bucket("output mapping", timing.output_mapping_ns, total);
    print_timing_bucket("release/drop", timing.release_ns, total);
    print_timing_bucket("flush", timing.flush_ns, total);
    print_timing_bucket("submit total", timing.submit_total_ns, total);
}

fn print_timing_bucket(label: &str, ns: u128, total_ns: u128) {
    let ms = ns as f64 / 1_000_000.0;
    let pct = (ns as f64 / total_ns as f64) * 100.0;
    println!("    {label}: {ms:.3} ms ({pct:.1}%)");
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
    let parsed = qgs_codec_h264::parse_annex_b_access_unit(
        access_units
            .first()
            .ok_or("Long-GOP fixture did not contain access units")?,
    )?;
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
        bit_depth: parsed.desc.bit_depth,
        chroma: parsed.desc.chroma,
        coded_width: parsed.desc.coded_width,
        coded_height: parsed.desc.coded_height,
        scan_mode: parsed.desc.scan_mode,
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
    proxy_throughput_paths: Option<(PathBuf, PathBuf)>,
    proxy_playback_paths: Option<(PathBuf, PathBuf)>,
    proxy_playback_profile: ProxyPlaybackProfile,
    qnc_journalist_demo_paths: Option<(PathBuf, PathBuf)>,
    original_audio_extract_path: Option<PathBuf>,
    broadcast_runtime_contract_paths: Option<(PathBuf, PathBuf)>,
    broadcast_runtime_state_machine_paths: Option<(PathBuf, PathBuf)>,
    broadcast_runtime_preroll_paths: Option<(PathBuf, PathBuf)>,
    broadcast_runtime_prepared_slots_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_events_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_payloads_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_video_payloads_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_device_boundary_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_test_presenter_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_test_audio_sink_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_simulate_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_original_video_payloads_paths: Option<(PathBuf, PathBuf)>,
    broadcast_player_runtime_verification_paths: Option<(PathBuf, PathBuf)>,
    linux_audio_device_probe_path: Option<PathBuf>,
    pipewire_audio_prototype_path: Option<PathBuf>,
    pipewire_audio_native_prototype_path: Option<PathBuf>,
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
        let mut proxy_throughput_original = None;
        let mut proxy_throughput_paths = None;
        let mut proxy_playback_original = None;
        let mut proxy_playback_paths = None;
        let mut proxy_playback_profile = ProxyPlaybackProfile::SourceRate;
        let mut qnc_journalist_demo_original = None;
        let mut qnc_journalist_demo_paths = None;
        let mut original_audio_extract_path = None;
        let mut broadcast_runtime_contract_original = None;
        let mut broadcast_runtime_contract_paths = None;
        let mut broadcast_runtime_state_machine_original = None;
        let mut broadcast_runtime_state_machine_paths = None;
        let mut broadcast_runtime_preroll_original = None;
        let mut broadcast_runtime_preroll_paths = None;
        let mut broadcast_runtime_prepared_slots_original = None;
        let mut broadcast_runtime_prepared_slots_paths = None;
        let mut broadcast_player_runtime_events_original = None;
        let mut broadcast_player_runtime_events_paths = None;
        let mut broadcast_player_runtime_payloads_original = None;
        let mut broadcast_player_runtime_payloads_paths = None;
        let mut broadcast_player_runtime_video_payloads_original = None;
        let mut broadcast_player_runtime_video_payloads_paths = None;
        let mut broadcast_player_runtime_device_boundary_original = None;
        let mut broadcast_player_runtime_device_boundary_paths = None;
        let mut broadcast_player_runtime_test_presenter_original = None;
        let mut broadcast_player_runtime_test_presenter_paths = None;
        let mut broadcast_player_runtime_test_audio_sink_original = None;
        let mut broadcast_player_runtime_test_audio_sink_paths = None;
        let mut broadcast_player_runtime_simulate_original = None;
        let mut broadcast_player_runtime_simulate_paths = None;
        let mut broadcast_player_runtime_original_video_payloads_original = None;
        let mut broadcast_player_runtime_original_video_payloads_paths = None;
        let mut broadcast_player_runtime_verification_original = None;
        let mut broadcast_player_runtime_verification_paths = None;
        let mut linux_audio_device_probe_path = None;
        let mut pipewire_audio_prototype_path = None;
        let mut pipewire_audio_native_prototype_path = None;
        let mut next_arg_is_proxy_original = false;
        let mut next_arg_is_proxy_proxy = false;
        let mut next_arg_is_proxy_throughput_original = false;
        let mut next_arg_is_proxy_throughput_proxy = false;
        let mut next_arg_is_proxy_playback_original = false;
        let mut next_arg_is_proxy_playback_proxy = false;
        let mut next_arg_is_proxy_playback_profile = false;
        let mut next_arg_is_qnc_journalist_demo_original = false;
        let mut next_arg_is_qnc_journalist_demo_proxy = false;
        let mut next_arg_is_original_audio_extract_path = false;
        let mut next_arg_is_broadcast_runtime_contract_original = false;
        let mut next_arg_is_broadcast_runtime_contract_proxy = false;
        let mut next_arg_is_broadcast_runtime_state_machine_original = false;
        let mut next_arg_is_broadcast_runtime_state_machine_proxy = false;
        let mut next_arg_is_broadcast_runtime_preroll_original = false;
        let mut next_arg_is_broadcast_runtime_preroll_proxy = false;
        let mut next_arg_is_broadcast_runtime_prepared_slots_original = false;
        let mut next_arg_is_broadcast_runtime_prepared_slots_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_events_original = false;
        let mut next_arg_is_broadcast_player_runtime_events_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_payloads_original = false;
        let mut next_arg_is_broadcast_player_runtime_payloads_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_video_payloads_original = false;
        let mut next_arg_is_broadcast_player_runtime_video_payloads_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_device_boundary_original = false;
        let mut next_arg_is_broadcast_player_runtime_device_boundary_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_test_presenter_original = false;
        let mut next_arg_is_broadcast_player_runtime_test_presenter_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_test_audio_sink_original = false;
        let mut next_arg_is_broadcast_player_runtime_test_audio_sink_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_simulate_original = false;
        let mut next_arg_is_broadcast_player_runtime_simulate_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_original_video_payloads_original = false;
        let mut next_arg_is_broadcast_player_runtime_original_video_payloads_proxy = false;
        let mut next_arg_is_broadcast_player_runtime_verification_original = false;
        let mut next_arg_is_broadcast_player_runtime_verification_proxy = false;
        let mut next_arg_is_linux_audio_device_probe_path = false;
        let mut next_arg_is_pipewire_audio_prototype_path = false;
        let mut next_arg_is_pipewire_audio_native_prototype_path = false;

        for arg in std::env::args_os().skip(1) {
            if next_arg_is_pipewire_audio_native_prototype_path {
                pipewire_audio_native_prototype_path = Some(PathBuf::from(arg));
                next_arg_is_pipewire_audio_native_prototype_path = false;
            } else if next_arg_is_pipewire_audio_prototype_path {
                pipewire_audio_prototype_path = Some(PathBuf::from(arg));
                next_arg_is_pipewire_audio_prototype_path = false;
            } else if next_arg_is_linux_audio_device_probe_path {
                linux_audio_device_probe_path = Some(PathBuf::from(arg));
                next_arg_is_linux_audio_device_probe_path = false;
            } else if next_arg_is_broadcast_player_runtime_verification_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_verification_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_verification_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_verification_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_verification_original {
                broadcast_player_runtime_verification_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_verification_original = false;
                next_arg_is_broadcast_player_runtime_verification_proxy = true;
            } else if next_arg_is_broadcast_player_runtime_original_video_payloads_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_original_video_payloads_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_original_video_payloads_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_original_video_payloads_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_original_video_payloads_original {
                broadcast_player_runtime_original_video_payloads_original =
                    Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_original_video_payloads_original = false;
                next_arg_is_broadcast_player_runtime_original_video_payloads_proxy = true;
            } else if next_arg_is_broadcast_player_runtime_simulate_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_simulate_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_simulate_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_simulate_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_simulate_original {
                broadcast_player_runtime_simulate_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_simulate_original = false;
                next_arg_is_broadcast_player_runtime_simulate_proxy = true;
            } else if next_arg_is_broadcast_player_runtime_test_audio_sink_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_test_audio_sink_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_test_audio_sink_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_test_audio_sink_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_test_audio_sink_original {
                broadcast_player_runtime_test_audio_sink_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_test_audio_sink_original = false;
                next_arg_is_broadcast_player_runtime_test_audio_sink_proxy = true;
            } else if next_arg_is_broadcast_player_runtime_test_presenter_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_test_presenter_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_test_presenter_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_test_presenter_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_test_presenter_original {
                broadcast_player_runtime_test_presenter_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_test_presenter_original = false;
                next_arg_is_broadcast_player_runtime_test_presenter_proxy = true;
            } else if next_arg_is_broadcast_player_runtime_device_boundary_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_device_boundary_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_device_boundary_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_device_boundary_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_device_boundary_original {
                broadcast_player_runtime_device_boundary_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_device_boundary_original = false;
                next_arg_is_broadcast_player_runtime_device_boundary_proxy = true;
            } else if next_arg_is_broadcast_player_runtime_video_payloads_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_video_payloads_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_video_payloads_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_video_payloads_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_video_payloads_original {
                broadcast_player_runtime_video_payloads_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_video_payloads_original = false;
                next_arg_is_broadcast_player_runtime_video_payloads_proxy = true;
            } else if next_arg_is_broadcast_player_runtime_payloads_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_payloads_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_payloads_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_payloads_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_payloads_original {
                broadcast_player_runtime_payloads_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_payloads_original = false;
                next_arg_is_broadcast_player_runtime_payloads_proxy = true;
            } else if next_arg_is_broadcast_player_runtime_events_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_player_runtime_events_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_player_runtime_events_paths = Some((original, proxy));
                next_arg_is_broadcast_player_runtime_events_proxy = false;
            } else if next_arg_is_broadcast_player_runtime_events_original {
                broadcast_player_runtime_events_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_player_runtime_events_original = false;
                next_arg_is_broadcast_player_runtime_events_proxy = true;
            } else if next_arg_is_broadcast_runtime_prepared_slots_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_runtime_prepared_slots_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_runtime_prepared_slots_paths = Some((original, proxy));
                next_arg_is_broadcast_runtime_prepared_slots_proxy = false;
            } else if next_arg_is_broadcast_runtime_prepared_slots_original {
                broadcast_runtime_prepared_slots_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_runtime_prepared_slots_original = false;
                next_arg_is_broadcast_runtime_prepared_slots_proxy = true;
            } else if next_arg_is_broadcast_runtime_preroll_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_runtime_preroll_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_runtime_preroll_paths = Some((original, proxy));
                next_arg_is_broadcast_runtime_preroll_proxy = false;
            } else if next_arg_is_broadcast_runtime_preroll_original {
                broadcast_runtime_preroll_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_runtime_preroll_original = false;
                next_arg_is_broadcast_runtime_preroll_proxy = true;
            } else if next_arg_is_broadcast_runtime_state_machine_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_runtime_state_machine_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_runtime_state_machine_paths = Some((original, proxy));
                next_arg_is_broadcast_runtime_state_machine_proxy = false;
            } else if next_arg_is_broadcast_runtime_state_machine_original {
                broadcast_runtime_state_machine_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_runtime_state_machine_original = false;
                next_arg_is_broadcast_runtime_state_machine_proxy = true;
            } else if next_arg_is_broadcast_runtime_contract_proxy {
                let proxy = PathBuf::from(arg);
                let original = broadcast_runtime_contract_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                broadcast_runtime_contract_paths = Some((original, proxy));
                next_arg_is_broadcast_runtime_contract_proxy = false;
            } else if next_arg_is_broadcast_runtime_contract_original {
                broadcast_runtime_contract_original = Some(PathBuf::from(arg));
                next_arg_is_broadcast_runtime_contract_original = false;
                next_arg_is_broadcast_runtime_contract_proxy = true;
            } else if next_arg_is_original_audio_extract_path {
                original_audio_extract_path = Some(PathBuf::from(arg));
                next_arg_is_original_audio_extract_path = false;
            } else if next_arg_is_qnc_journalist_demo_proxy {
                let proxy = PathBuf::from(arg);
                let original = qnc_journalist_demo_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                qnc_journalist_demo_paths = Some((original, proxy));
                next_arg_is_qnc_journalist_demo_proxy = false;
            } else if next_arg_is_qnc_journalist_demo_original {
                qnc_journalist_demo_original = Some(PathBuf::from(arg));
                next_arg_is_qnc_journalist_demo_original = false;
                next_arg_is_qnc_journalist_demo_proxy = true;
            } else if next_arg_is_proxy_playback_profile {
                proxy_playback_profile =
                    ProxyPlaybackProfile::parse(&arg).unwrap_or_else(|err| panic!("{err}"));
                next_arg_is_proxy_playback_profile = false;
            } else if next_arg_is_proxy_playback_proxy {
                let proxy = PathBuf::from(arg);
                let original = proxy_playback_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                proxy_playback_paths = Some((original, proxy));
                next_arg_is_proxy_playback_proxy = false;
            } else if next_arg_is_proxy_playback_original {
                proxy_playback_original = Some(PathBuf::from(arg));
                next_arg_is_proxy_playback_original = false;
                next_arg_is_proxy_playback_proxy = true;
            } else if next_arg_is_proxy_throughput_proxy {
                let proxy = PathBuf::from(arg);
                let original = proxy_throughput_original
                    .take()
                    .unwrap_or_else(|| PathBuf::from(""));
                proxy_throughput_paths = Some((original, proxy));
                next_arg_is_proxy_throughput_proxy = false;
            } else if next_arg_is_proxy_throughput_original {
                proxy_throughput_original = Some(PathBuf::from(arg));
                next_arg_is_proxy_throughput_original = false;
                next_arg_is_proxy_throughput_proxy = true;
            } else if next_arg_is_proxy_proxy {
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
            } else if arg == PROXY_THROUGHPUT_ARG {
                next_arg_is_proxy_throughput_original = true;
            } else if arg == PROXY_PLAYBACK_ARG {
                next_arg_is_proxy_playback_original = true;
            } else if arg == PROXY_PLAYBACK_PROFILE_ARG {
                next_arg_is_proxy_playback_profile = true;
            } else if arg == QNC_JOURNALIST_DEMO_ARG {
                next_arg_is_qnc_journalist_demo_original = true;
            } else if arg == ORIGINAL_AUDIO_EXTRACT_ARG {
                next_arg_is_original_audio_extract_path = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_CONTRACT_ARG
                || arg == LEGACY_BROADCAST_RUNTIME_CONTRACT_ARG
            {
                next_arg_is_broadcast_runtime_contract_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_STATE_MACHINE_ARG
                || arg == LEGACY_BROADCAST_RUNTIME_STATE_MACHINE_ARG
            {
                next_arg_is_broadcast_runtime_state_machine_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_PREROLL_ARG
                || arg == LEGACY_BROADCAST_RUNTIME_PREROLL_ARG
            {
                next_arg_is_broadcast_runtime_preroll_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_PREPARED_SLOTS_ARG
                || arg == LEGACY_BROADCAST_RUNTIME_PREPARED_SLOTS_ARG
            {
                next_arg_is_broadcast_runtime_prepared_slots_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_EVENTS_ARG {
                next_arg_is_broadcast_player_runtime_events_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_PAYLOADS_ARG {
                next_arg_is_broadcast_player_runtime_payloads_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_VIDEO_PAYLOADS_ARG {
                next_arg_is_broadcast_player_runtime_video_payloads_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_DEVICE_BOUNDARY_ARG {
                next_arg_is_broadcast_player_runtime_device_boundary_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_TEST_PRESENTER_ARG {
                next_arg_is_broadcast_player_runtime_test_presenter_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_TEST_AUDIO_SINK_ARG {
                next_arg_is_broadcast_player_runtime_test_audio_sink_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_SIMULATE_ARG {
                next_arg_is_broadcast_player_runtime_simulate_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_ORIGINAL_VIDEO_PAYLOADS_ARG {
                next_arg_is_broadcast_player_runtime_original_video_payloads_original = true;
            } else if arg == BROADCAST_PLAYER_RUNTIME_VERIFICATION_ARG {
                next_arg_is_broadcast_player_runtime_verification_original = true;
            } else if arg == LINUX_AUDIO_DEVICE_PROBE_ARG {
                next_arg_is_linux_audio_device_probe_path = true;
            } else if arg == PIPEWIRE_AUDIO_PROTOTYPE_ARG {
                next_arg_is_pipewire_audio_prototype_path = true;
            } else if arg == PIPEWIRE_AUDIO_NATIVE_PROTOTYPE_ARG {
                next_arg_is_pipewire_audio_native_prototype_path = true;
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
            proxy_throughput_paths,
            proxy_playback_paths,
            proxy_playback_profile,
            qnc_journalist_demo_paths,
            original_audio_extract_path,
            broadcast_runtime_contract_paths,
            broadcast_runtime_state_machine_paths,
            broadcast_runtime_preroll_paths,
            broadcast_runtime_prepared_slots_paths,
            broadcast_player_runtime_events_paths,
            broadcast_player_runtime_payloads_paths,
            broadcast_player_runtime_video_payloads_paths,
            broadcast_player_runtime_device_boundary_paths,
            broadcast_player_runtime_test_presenter_paths,
            broadcast_player_runtime_test_audio_sink_paths,
            broadcast_player_runtime_simulate_paths,
            broadcast_player_runtime_original_video_payloads_paths,
            broadcast_player_runtime_verification_paths,
            linux_audio_device_probe_path,
            pipewire_audio_prototype_path,
            pipewire_audio_native_prototype_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_pipewire_f32_interleaved_prototype_buffer, mxf_file_label, pcm_s24le_sample_to_f32,
        proxy_presentation_ordinals, selected_proxy_ordinals, SonyXmlSummary,
    };
    use qgs_media_runtime::{PcmAudioBlock, PcmAudioBlockLayout, PcmEndian, PcmSampleFormat};
    use qgs_mp4::{Mp4VideoSample, Mp4VideoTrack, Rational};
    use std::path::PathBuf;
    use std::time::Duration;

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

    #[test]
    fn selected_proxy_ordinals_are_bounded_and_deduplicated() {
        assert_eq!(selected_proxy_ordinals(0), Vec::<usize>::new());
        assert_eq!(selected_proxy_ordinals(1), vec![0]);
        assert_eq!(selected_proxy_ordinals(12), vec![0, 11]);
        assert_eq!(selected_proxy_ordinals(106), vec![0, 53, 105]);
    }

    #[test]
    fn pipewire_prototype_converts_s24le_to_f32_interleaved() {
        let blocks = vec![
            mono_block(10, 0, &[[0x00, 0x00, 0x00], [0xff, 0xff, 0x7f]]),
            mono_block(11, 1, &[[0x00, 0x00, 0x80], [0x00, 0x00, 0x40]]),
        ];

        let buffer = build_pipewire_f32_interleaved_prototype_buffer(&blocks, 48_000, 2).unwrap();

        assert_eq!(buffer.channels, 2);
        assert_eq!(buffer.sample_count, 2);
        assert_eq!(buffer.bytes.len(), 2 * 2 * 4);
        let values = buffer
            .bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(values[0], 0.0);
        assert_eq!(values[1], -1.0);
        assert!(values[2] > 0.999_999 && values[2] <= 1.0);
        assert_eq!(values[3], 0.5);
    }

    #[test]
    fn pipewire_prototype_rejects_short_blocks() {
        let blocks = vec![mono_block(10, 0, &[[0x00, 0x00, 0x00]])];

        assert!(build_pipewire_f32_interleaved_prototype_buffer(&blocks, 48_000, 2).is_err());
    }

    #[test]
    fn s24le_sample_conversion_sign_extends() {
        assert_eq!(
            pcm_s24le_sample_to_f32(&[0x00, 0x00, 0x80], 0).unwrap(),
            -1.0
        );
        assert_eq!(
            pcm_s24le_sample_to_f32(&[0x00, 0x00, 0x40], 0).unwrap(),
            0.5
        );
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

    fn mono_block(track_id: u32, channel_index: u16, samples: &[[u8; 3]]) -> PcmAudioBlock {
        PcmAudioBlock::new(
            Duration::ZERO,
            Duration::from_nanos(u64::try_from(samples.len()).unwrap() * 1_000_000_000 / 48_000),
            48_000,
            u32::try_from(samples.len()).unwrap(),
            PcmSampleFormat::SignedInteger {
                bits_per_sample: 24,
                endian: PcmEndian::Little,
            },
            PcmAudioBlockLayout::MonoTrack {
                track_id,
                channel_index,
            },
            samples
                .iter()
                .flat_map(|sample| sample.iter().copied())
                .collect(),
        )
        .unwrap()
    }
}

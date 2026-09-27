#![forbid(unsafe_code)]

use std::any::Any;
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use libva::{
    BorrowedBufferType, BufferType, Context, Display, H264PicFields, H264SeqFields, IQMatrix,
    IQMatrixBufferH264, Image, Picture, PictureH264, PictureParameter, PictureParameterBufferH264,
    SliceParameter, SliceParameterBufferH264, Surface, VAConfigAttrib, VAConfigAttribType,
    VAEntrypoint, VAProfile, VA_ATTRIB_NOT_SUPPORTED, VA_FOURCC_NV12, VA_FOURCC_P010,
    VA_FOURCC_UYVY, VA_FOURCC_Y210, VA_FOURCC_YUY2, VA_RT_FORMAT_YUV420, VA_RT_FORMAT_YUV420_10,
    VA_RT_FORMAT_YUV422, VA_RT_FORMAT_YUV422_10,
};
use qgs_codec_h264::{
    parse_annex_b_access_unit, H264DecoderState, H264Error, H264PictureId, ParsedH264AccessUnit,
    ParsedH264Reference,
};
use qgs_core::{
    BackendDecodedSurface, BackendDecoder, BackendResource, DecoderBackend, DecoderError,
    VideoCapabilityDiscovery, VideoCapabilityDiscoveryError,
};
use qgs_protocol::{
    BitDepth, ChromaSubsampling, CreateDecoderRequest, DecoderConfig, DeviceDesc, DeviceId,
    H264Profile, Mpeg2Profile, SubmitAccessUnitRequest, VideoCapabilities, VideoCodec,
    VideoDecodeCapability, VideoProfile, VideoSurfaceDesc, VideoSurfaceFormat,
    MAX_VIDEO_SURFACE_HEIGHT, MAX_VIDEO_SURFACE_WIDTH,
};

const DEFAULT_MAX_WIDTH: u32 = MAX_VIDEO_SURFACE_WIDTH;
const DEFAULT_MAX_HEIGHT: u32 = MAX_VIDEO_SURFACE_HEIGHT;

#[derive(Debug)]
pub struct VaapiVideoDiscovery {
    devices: BTreeMap<DeviceId, VaapiDevice>,
}

impl VaapiVideoDiscovery {
    pub fn new(qgs_devices: &[DeviceDesc]) -> Self {
        let render_nodes = enumerate_render_nodes();
        let mut devices = BTreeMap::new();

        for device in qgs_devices {
            let Some(render_node) = render_nodes.iter().find(|node| {
                node.vendor_id == device.vendor_id && node.device_id == device.device_id
            }) else {
                continue;
            };

            devices.insert(
                device.id,
                VaapiDevice {
                    path: render_node.path.clone(),
                    vendor_id: render_node.vendor_id,
                    device_id: render_node.device_id,
                    driver: render_node.driver.clone(),
                },
            );
        }

        Self { devices }
    }

    pub fn devices(&self) -> Vec<VaapiDeviceInfo> {
        self.devices
            .iter()
            .map(|(device_id, device)| VaapiDeviceInfo {
                device_id: *device_id,
                path: device.path.clone(),
                vendor_id: device.vendor_id,
                device_id_pci: device.device_id,
                driver: device.driver.clone(),
            })
            .collect()
    }
}

impl VideoCapabilityDiscovery for VaapiVideoDiscovery {
    fn query_video_capabilities(
        &self,
        device_id: DeviceId,
    ) -> Result<VideoCapabilities, VideoCapabilityDiscoveryError> {
        let device = self
            .devices
            .get(&device_id)
            .ok_or(VideoCapabilityDiscoveryError::UnknownDeviceId)?;

        let display = Display::open_drm_display(&device.path)
            .map_err(|_| VideoCapabilityDiscoveryError::BackendUnavailable)?;
        let capabilities = query_decode_capabilities(&display)
            .map_err(|_| VideoCapabilityDiscoveryError::BackendFailed)?;

        Ok(VideoCapabilities {
            device_id,
            decode: capabilities,
        })
    }
}

impl DecoderBackend for VaapiVideoDiscovery {
    fn create_decoder(
        &self,
        request: &CreateDecoderRequest,
    ) -> Result<Box<dyn BackendDecoder>, DecoderError> {
        request.config.validate()?;
        if request.config.codec != VideoCodec::H264
            || request.config.bit_depth.get() != 8
            || request.config.chroma != ChromaSubsampling::Cs420
            || request.config.coded_width == 0
            || request.config.coded_height == 0
        {
            return Err(DecoderError::UnsupportedDecodeConfiguration);
        }
        match request.config.profile {
            VideoProfile::H264(H264Profile::Baseline | H264Profile::Main | H264Profile::High) => {}
            _ => return Err(DecoderError::UnsupportedDecodeConfiguration),
        }

        let device = self
            .devices
            .get(&request.config.device_id)
            .ok_or(DecoderError::UnknownDeviceId)?;
        let display = Display::open_drm_display(&device.path)
            .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
        let capabilities = query_decode_capabilities(&display)
            .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
        if !capabilities
            .iter()
            .any(|capability| request.config.is_satisfied_by(capability))
        {
            return Err(DecoderError::UnsupportedDecodeConfiguration);
        }
        let profile = va_profile_from_h264(request.config.profile)?;
        let entrypoints = display
            .query_config_entrypoints(profile)
            .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
        if !entrypoints.contains(&VAEntrypoint::VAEntrypointVLD) {
            return Err(DecoderError::UnsupportedDecodeConfiguration);
        }
        let attrs = vec![VAConfigAttrib {
            type_: VAConfigAttribType::VAConfigAttribRTFormat,
            value: VA_RT_FORMAT_YUV420,
        }];
        let config = display
            .create_config(attrs, profile, VAEntrypoint::VAEntrypointVLD)
            .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
        let surfaces = display
            .create_surfaces(
                VA_RT_FORMAT_YUV420,
                Some(VA_FOURCC_NV12),
                request.config.coded_width,
                request.config.coded_height,
                None,
                vec![(); 24],
            )
            .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
        let context = display
            .create_context(
                &config,
                request.config.coded_width,
                request.config.coded_height,
                Some(&surfaces),
                true,
            )
            .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;

        Ok(Box::new(VaapiH264Decoder {
            context,
            available_surfaces: surfaces,
            decoded_surfaces: BTreeMap::new(),
            frontend: H264DecoderState::new(),
            device_id: request.config.device_id,
            config: request.config.clone(),
            max_live_surfaces: 0,
        }))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaapiDeviceInfo {
    pub device_id: DeviceId,
    pub path: PathBuf,
    pub vendor_id: u32,
    pub device_id_pci: u32,
    pub driver: Option<String>,
}

pub struct VaapiDecodedPrimeSurfaceDiagnostic {
    pub fourcc: u32,
    pub width: u32,
    pub height: u32,
    pub objects: Vec<VaapiPrimeObjectDiagnostic>,
    pub layers: Vec<VaapiPrimeLayerDiagnostic>,
    pub validation_checksum: u32,
    _surface: Surface<()>,
}

#[derive(Debug)]
pub struct VaapiPrimeObjectDiagnostic {
    pub fd: OwnedFd,
    pub size: u32,
    pub drm_format_modifier: u64,
}

#[derive(Clone, Debug)]
pub struct VaapiPrimeLayerDiagnostic {
    pub drm_format: u32,
    pub num_planes: u32,
    pub object_index: [u8; 4],
    pub offset: [u32; 4],
    pub pitch: [u32; 4],
}

pub fn decode_h264_drm_prime_for_diagnostic(
    render_node: &Path,
    access_unit: &[u8],
) -> Result<VaapiDecodedPrimeSurfaceDiagnostic, DecoderError> {
    let parsed = parse_annex_b_access_unit(access_unit).map_err(decoder_error_from_h264)?;
    let display = Display::open_drm_display(render_node)
        .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
    let profile = va_profile_from_h264(decoder_profile_from_parsed(&parsed))?;
    let entrypoints = display
        .query_config_entrypoints(profile)
        .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
    if !entrypoints.contains(&VAEntrypoint::VAEntrypointVLD) {
        return Err(DecoderError::UnsupportedDecodeConfiguration);
    }

    let attrs = vec![VAConfigAttrib {
        type_: VAConfigAttribType::VAConfigAttribRTFormat,
        value: VA_RT_FORMAT_YUV420,
    }];
    let config = display
        .create_config(attrs, profile, VAEntrypoint::VAEntrypointVLD)
        .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
    let surfaces = display
        .create_surfaces(
            VA_RT_FORMAT_YUV420,
            Some(VA_FOURCC_NV12),
            parsed.desc.coded_width,
            parsed.desc.coded_height,
            None,
            vec![()],
        )
        .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
    let context = display
        .create_context(
            &config,
            parsed.desc.coded_width,
            parsed.desc.coded_height,
            Some(&surfaces),
            true,
        )
        .map_err(|_| DecoderError::UnsupportedDecodeConfiguration)?;
    let surface = surfaces
        .into_iter()
        .next()
        .ok_or(DecoderError::DecodeFailed)?;
    let surface = decode_h264_access_unit(context, surface, &parsed, &BTreeMap::new())?;
    let validation_checksum =
        validation_checksum(&surface, parsed.desc.coded_width, parsed.desc.coded_height)
            .map_err(|_| DecoderError::DecodeFailed)?;
    let descriptor = surface
        .export_prime()
        .map_err(|_| DecoderError::DecodeFailed)?;
    let objects = descriptor
        .objects
        .into_iter()
        .map(|object| VaapiPrimeObjectDiagnostic {
            fd: object.fd,
            size: object.size,
            drm_format_modifier: object.drm_format_modifier,
        })
        .collect();
    let layers = descriptor
        .layers
        .into_iter()
        .map(|layer| VaapiPrimeLayerDiagnostic {
            drm_format: layer.drm_format,
            num_planes: layer.num_planes,
            object_index: layer.object_index,
            offset: layer.offset,
            pitch: layer.pitch,
        })
        .collect();

    Ok(VaapiDecodedPrimeSurfaceDiagnostic {
        fourcc: descriptor.fourcc,
        width: descriptor.width,
        height: descriptor.height,
        objects,
        layers,
        validation_checksum,
        _surface: surface,
    })
}

#[derive(Debug)]
struct VaapiDevice {
    path: PathBuf,
    vendor_id: u32,
    device_id: u32,
    driver: Option<String>,
}

fn decoder_profile_from_parsed(parsed: &ParsedH264AccessUnit) -> VideoProfile {
    qgs_codec_h264::decoder_profile(parsed)
}

fn parsed_access_unit_matches_config(
    parsed: &ParsedH264AccessUnit,
    config: &DecoderConfig,
) -> bool {
    config.codec == qgs_codec_h264::decoder_codec()
        && config.profile == qgs_codec_h264::decoder_profile(parsed)
        && config.bit_depth == parsed.desc.bit_depth
        && config.chroma == parsed.desc.chroma
        && config.coded_width == parsed.desc.coded_width
        && config.coded_height == parsed.desc.coded_height
        && config.scan_mode == parsed.desc.scan_mode
}

struct VaapiH264Decoder {
    context: Rc<Context>,
    available_surfaces: Vec<Surface<()>>,
    decoded_surfaces: BTreeMap<(u16, i32), DecodedSurfaceState>,
    frontend: H264DecoderState,
    device_id: DeviceId,
    config: DecoderConfig,
    max_live_surfaces: usize,
}

impl BackendDecoder for VaapiH264Decoder {
    fn submit_access_unit(
        &mut self,
        request: &SubmitAccessUnitRequest,
    ) -> Result<Vec<BackendDecodedSurface>, DecoderError> {
        let parsed = self
            .frontend
            .parse_access_unit(&request.data)
            .map_err(decoder_error_from_h264)?;
        if !parsed_access_unit_matches_config(&parsed, &self.config) {
            return Err(DecoderError::UnsupportedDecodeConfiguration);
        }
        let surface = self
            .available_surfaces
            .pop()
            .ok_or(DecoderError::DecodeFailed)?;
        let surface = decode_h264_access_unit(
            Rc::clone(&self.context),
            surface,
            &parsed,
            &self.decoded_surfaces,
        )?;
        let validation_checksum =
            validation_checksum(&surface, parsed.desc.coded_width, parsed.desc.coded_height)
                .map_err(|_| DecoderError::DecodeFailed)?;
        let export_probe = surface
            .export_prime()
            .ok()
            .map(|descriptor| VaapiExportProbe {
                object_count: descriptor.objects.len(),
                layer_count: descriptor.layers.len(),
                fourcc: descriptor.fourcc,
                width: descriptor.width,
                height: descriptor.height,
                object_sizes: descriptor
                    .objects
                    .iter()
                    .map(|object| object.size)
                    .collect(),
                modifiers: descriptor
                    .objects
                    .iter()
                    .map(|object| object.drm_format_modifier)
                    .collect(),
                layer_formats: descriptor
                    .layers
                    .iter()
                    .map(|layer| layer.drm_format)
                    .collect(),
                pitches: descriptor.layers.iter().map(|layer| layer.pitch).collect(),
                offsets: descriptor.layers.iter().map(|layer| layer.offset).collect(),
            });
        eprintln!(
            "qgs-vaapi: decoded H.264 frame on device {} validation checksum=0x{validation_checksum:08x}",
            self.device_id.get()
        );
        if let Some(probe) = &export_probe {
            eprintln!(
                "qgs-vaapi: VA surface DRM PRIME export probe succeeded: fourcc=0x{:08x} size={}x{} objects={} layers={} object_sizes={:?} modifiers={:?} layer_formats={:?} pitches={:?} offsets={:?}",
                probe.fourcc,
                probe.width,
                probe.height,
                probe.object_count,
                probe.layer_count,
                probe.object_sizes,
                probe.modifiers,
                probe.layer_formats,
                probe.pitches,
                probe.offsets
            );
        } else {
            eprintln!("qgs-vaapi: VA surface DRM PRIME export probe failed");
        }
        let id = parsed.picture.id();
        self.decoded_surfaces.insert(
            surface_key(&id),
            DecodedSurfaceState {
                surface: Rc::new(surface),
                desc: parsed.desc.clone(),
                validation_checksum,
                export_probe: export_probe.clone(),
            },
        );
        self.max_live_surfaces = self.max_live_surfaces.max(self.decoded_surfaces.len());
        let update = self
            .frontend
            .finish_picture(&parsed)
            .map_err(decoder_error_from_h264)?;
        eprintln!(
            "qgs-vaapi: H.264 DPB occupancy={} output_pending={} live_va_surfaces={}",
            update.max_dpb_occupancy,
            update.max_output_pending,
            self.decoded_surfaces.len()
        );
        let outputs = self.outputs_from_ids(&update.output_ready)?;
        for released in update.released {
            self.decoded_surfaces.remove(&surface_key(&released));
        }
        Ok(outputs)
    }

    fn flush(
        &mut self,
        _request: &qgs_protocol::FlushDecoderRequest,
    ) -> Result<Vec<BackendDecodedSurface>, DecoderError> {
        let update = self.frontend.flush();
        eprintln!(
            "qgs-vaapi: H.264 flush DPB occupancy={} output_pending={} max_live_va_surfaces={}",
            update.max_dpb_occupancy, update.max_output_pending, self.max_live_surfaces
        );
        let outputs = self.outputs_from_ids(&update.output_ready)?;
        for released in &update.released {
            self.decoded_surfaces.remove(&surface_key(released));
        }
        Ok(outputs)
    }
}

impl VaapiH264Decoder {
    fn outputs_from_ids(
        &self,
        output_ready: &[H264PictureId],
    ) -> Result<Vec<BackendDecodedSurface>, DecoderError> {
        output_ready
            .iter()
            .map(|id| {
                let surface = self
                    .decoded_surfaces
                    .get(&surface_key(id))
                    .ok_or(DecoderError::DecodeFailed)?;
                Ok(BackendDecodedSurface {
                    resource: Box::new(VaapiVideoSurface {
                        surface: Rc::clone(&surface.surface),
                        validation_checksum: surface.validation_checksum,
                        export_probe: surface.export_probe.clone(),
                    }),
                    desc: surface.desc.clone(),
                })
            })
            .collect()
    }
}

struct DecodedSurfaceState {
    surface: Rc<Surface<()>>,
    desc: VideoSurfaceDesc,
    validation_checksum: u32,
    export_probe: Option<VaapiExportProbe>,
}

struct VaapiVideoSurface {
    #[allow(dead_code)]
    surface: Rc<Surface<()>>,
    #[allow(dead_code)]
    validation_checksum: u32,
    #[allow(dead_code)]
    export_probe: Option<VaapiExportProbe>,
}

impl BackendResource for VaapiVideoSurface {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[allow(dead_code)]
#[derive(Clone)]
struct VaapiExportProbe {
    object_count: usize,
    layer_count: usize,
    fourcc: u32,
    width: u32,
    height: u32,
    object_sizes: Vec<u32>,
    modifiers: Vec<u64>,
    layer_formats: Vec<u32>,
    pitches: Vec<[u32; 4]>,
    offsets: Vec<[u32; 4]>,
}

fn decode_h264_access_unit(
    context: Rc<Context>,
    surface: Surface<()>,
    parsed: &ParsedH264AccessUnit,
    decoded_surfaces: &BTreeMap<(u16, i32), DecodedSurfaceState>,
) -> Result<Surface<()>, DecoderError> {
    let mut picture = Picture::new(0, Rc::clone(&context), surface);
    let pic_param = picture_parameter(parsed, picture.surface().id(), decoded_surfaces)?;
    picture.add_buffer(
        context
            .create_buffer(BufferType::PictureParameter(PictureParameter::H264(
                pic_param,
            )))
            .map_err(|_| DecoderError::DecodeFailed)?,
    );
    picture.add_buffer(
        context
            .create_buffer(BufferType::IQMatrix(IQMatrix::H264(
                IQMatrixBufferH264::new(
                    parsed.picture.scaling_list4x4,
                    parsed.picture.scaling_list8x8,
                ),
            )))
            .map_err(|_| DecoderError::DecodeFailed)?,
    );
    let slice_param = slice_parameters(parsed, decoded_surfaces)?;
    picture.add_buffer(
        context
            .create_buffer(BufferType::SliceParameter(SliceParameter::H264(
                slice_param,
            )))
            .map_err(|_| DecoderError::DecodeFailed)?,
    );
    let mut slice_data = Vec::new();
    for slice in &parsed.slices {
        slice_data.extend_from_slice(&slice.nal_bytes);
    }
    picture.add_buffer(
        context
            .create_buffer_borrowed(BorrowedBufferType::SliceData(&slice_data))
            .map_err(|_| DecoderError::DecodeFailed)?,
    );

    let picture = picture.begin().map_err(|_| DecoderError::DecodeFailed)?;
    let picture = picture.render().map_err(|_| DecoderError::DecodeFailed)?;
    let picture = picture.end().map_err(|_| DecoderError::DecodeFailed)?;
    let picture = picture.sync().map_err(|_| DecoderError::DecodeFailed)?;
    picture
        .take_surface()
        .map_err(|_| DecoderError::DecodeFailed)
}

fn picture_parameter(
    parsed: &ParsedH264AccessUnit,
    surface_id: u32,
    decoded_surfaces: &BTreeMap<(u16, i32), DecodedSurfaceState>,
) -> Result<PictureParameterBufferH264, DecoderError> {
    let picture = &parsed.picture;
    let current = PictureH264::new(
        surface_id,
        u32::from(picture.frame_num),
        0,
        picture.top_field_order_cnt,
        picture.bottom_field_order_cnt,
    );
    let references = reference_frames(parsed, decoded_surfaces)?;
    let seq_fields = H264SeqFields::new(
        picture.chroma_format_idc,
        0,
        picture.gaps_in_frame_num_value_allowed_flag as u32,
        picture.frame_mbs_only_flag as u32,
        picture.mb_adaptive_frame_field_flag as u32,
        picture.direct_8x8_inference_flag as u32,
        0,
        u32::from(picture.log2_max_frame_num_minus4),
        picture.pic_order_cnt_type,
        picture.log2_max_pic_order_cnt_lsb_minus4,
        picture.delta_pic_order_always_zero_flag as u32,
    );
    let pic_fields = H264PicFields::new(
        picture.entropy_coding_mode_flag as u32,
        picture.weighted_pred_flag as u32,
        u32::from(picture.weighted_bipred_idc),
        picture.transform_8x8_mode_flag as u32,
        picture.field_pic_flag as u32,
        picture.constrained_intra_pred_flag as u32,
        picture.pic_order_present_flag as u32,
        picture.deblocking_filter_control_present_flag as u32,
        picture.redundant_pic_cnt_present_flag as u32,
        picture.reference_pic_flag as u32,
    );
    Ok(PictureParameterBufferH264::new(
        current,
        references,
        picture.picture_width_in_mbs_minus1,
        picture.picture_height_in_mbs_minus1,
        picture.bit_depth_luma_minus8,
        picture.bit_depth_chroma_minus8,
        picture.num_ref_frames,
        &seq_fields,
        picture.num_slice_groups_minus1,
        picture.slice_group_map_type,
        picture.slice_group_change_rate_minus1,
        picture.pic_init_qp_minus26,
        picture.pic_init_qs_minus26,
        picture.chroma_qp_index_offset,
        picture.second_chroma_qp_index_offset,
        &pic_fields,
        picture.frame_num,
    ))
}

fn slice_parameters(
    parsed: &ParsedH264AccessUnit,
    decoded_surfaces: &BTreeMap<(u16, i32), DecodedSurfaceState>,
) -> Result<SliceParameterBufferH264, DecoderError> {
    let mut params = SliceParameterBufferH264::new_array();
    let mut offset = 0_u32;
    for slice in &parsed.slices {
        let ref_list0 = reference_list_array(&slice.ref_pic_list0, decoded_surfaces)?;
        let ref_list1 = reference_list_array(&slice.ref_pic_list1, decoded_surfaces)?;
        params.add_slice_parameter(
            slice.nal_bytes.len() as u32,
            offset,
            0,
            slice.slice_data_bit_offset,
            slice.first_mb_in_slice,
            slice.slice_type,
            slice.direct_spatial_mv_pred_flag,
            slice.num_ref_idx_l0_active_minus1,
            slice.num_ref_idx_l1_active_minus1,
            slice.cabac_init_idc,
            slice.slice_qp_delta,
            slice.disable_deblocking_filter_idc,
            slice.slice_alpha_c0_offset_div2,
            slice.slice_beta_offset_div2,
            ref_list0,
            ref_list1,
            0,
            0,
            0,
            [0; 32],
            [0; 32],
            0,
            [[0; 2]; 32],
            [[0; 2]; 32],
            0,
            [0; 32],
            [0; 32],
            0,
            [[0; 2]; 32],
            [[0; 2]; 32],
        );
        offset = offset.saturating_add(slice.nal_bytes.len() as u32);
    }
    Ok(params)
}

fn reference_frames(
    parsed: &ParsedH264AccessUnit,
    decoded_surfaces: &BTreeMap<(u16, i32), DecodedSurfaceState>,
) -> Result<[PictureH264; 16], DecoderError> {
    let mut references = invalid_picture_array_16();
    for (index, reference) in parsed.reference_frames.iter().take(16).enumerate() {
        references[index] = picture_from_reference(reference, decoded_surfaces)?;
    }
    Ok(references)
}

fn reference_list_array(
    refs: &[H264PictureId],
    decoded_surfaces: &BTreeMap<(u16, i32), DecodedSurfaceState>,
) -> Result<[PictureH264; 32], DecoderError> {
    let mut pictures = invalid_picture_array_32();
    for (index, id) in refs.iter().take(32).enumerate() {
        let Some(surface) = decoded_surfaces.get(&surface_key(id)) else {
            return Err(DecoderError::DecodeFailed);
        };
        pictures[index] = PictureH264::new(
            surface.surface.id(),
            u32::from(id.frame_num),
            0,
            id.poc,
            id.poc,
        );
    }
    Ok(pictures)
}

fn picture_from_reference(
    reference: &ParsedH264Reference,
    decoded_surfaces: &BTreeMap<(u16, i32), DecodedSurfaceState>,
) -> Result<PictureH264, DecoderError> {
    let Some(surface) = decoded_surfaces.get(&surface_key(&reference.id)) else {
        return Err(DecoderError::DecodeFailed);
    };
    Ok(PictureH264::new(
        surface.surface.id(),
        u32::from(reference.frame_num),
        0,
        reference.top_field_order_cnt,
        reference.bottom_field_order_cnt,
    ))
}

fn surface_key(id: &H264PictureId) -> (u16, i32) {
    (id.frame_num, id.poc)
}

fn invalid_picture() -> PictureH264 {
    PictureH264::new(u32::MAX, 0, 1, 0, 0)
}

fn invalid_picture_array_16() -> [PictureH264; 16] {
    std::array::from_fn(|_| invalid_picture())
}

fn invalid_picture_array_32() -> [PictureH264; 32] {
    std::array::from_fn(|_| invalid_picture())
}

fn validation_checksum(
    surface: &Surface<()>,
    width: u32,
    height: u32,
) -> Result<u32, Box<dyn std::error::Error>> {
    let image = Image::derive_from(surface, (width, height)).or_else(|_| {
        Image::create_from(
            surface,
            nv12_image_format(),
            (width, height),
            (width, height),
        )
    })?;
    let data = image.as_ref();
    let va_image = image.image();
    if va_image.num_planes < 2 {
        return Err("decoded validation image did not expose NV12 planes".into());
    }
    let width = width as usize;
    let height = height as usize;
    let mut hash = 2166136261_u32;
    for plane in 0..2 {
        let rows = if plane == 0 {
            height
        } else {
            height.div_ceil(2)
        };
        let offset = va_image.offsets[plane] as usize;
        let pitch = va_image.pitches[plane] as usize;
        for row in 0..rows {
            let start = offset + row * pitch;
            let end = start + width;
            for byte in &data[start..end] {
                hash ^= u32::from(*byte);
                hash = hash.wrapping_mul(16777619);
            }
        }
    }
    if hash == 2166136261 {
        return Err("decoded validation checksum was empty".into());
    }
    Ok(hash)
}

fn nv12_image_format() -> libva::VAImageFormat {
    libva::VAImageFormat {
        fourcc: VA_FOURCC_NV12,
        byte_order: libva::VA_LSB_FIRST,
        bits_per_pixel: 12,
        depth: 12,
        red_mask: 0,
        green_mask: 0,
        blue_mask: 0,
        alpha_mask: 0,
        va_reserved: [0; 4],
    }
}

fn va_profile_from_h264(profile: VideoProfile) -> Result<VAProfile::Type, DecoderError> {
    match profile {
        VideoProfile::H264(H264Profile::Baseline) => {
            Ok(VAProfile::VAProfileH264ConstrainedBaseline)
        }
        VideoProfile::H264(H264Profile::Main) => Ok(VAProfile::VAProfileH264Main),
        VideoProfile::H264(H264Profile::High) => Ok(VAProfile::VAProfileH264High),
        _ => Err(DecoderError::UnsupportedDecodeConfiguration),
    }
}

fn decoder_error_from_h264(error: H264Error) -> DecoderError {
    match error {
        H264Error::MalformedAnnexB
        | H264Error::MissingSps
        | H264Error::MissingPps
        | H264Error::MissingSlice
        | H264Error::Parser(_) => DecoderError::MalformedCompressedData,
        H264Error::UnsupportedFeature(_) => DecoderError::UnsupportedH264StreamFeature,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenderNode {
    path: PathBuf,
    vendor_id: u32,
    device_id: u32,
    driver: Option<String>,
}

fn enumerate_render_nodes() -> Vec<RenderNode> {
    let Ok(entries) = std::fs::read_dir("/dev/dri") else {
        return Vec::new();
    };

    let mut nodes = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if !file_name.starts_with("renderD") {
                return None;
            }
            render_node_from_path(entry.path())
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.path.cmp(&right.path));
    nodes
}

fn render_node_from_path(path: PathBuf) -> Option<RenderNode> {
    let name = path.file_name()?.to_string_lossy();
    let sysfs_device = PathBuf::from("/sys/class/drm")
        .join(name.as_ref())
        .join("device");
    let vendor_id = read_hex_u32(&sysfs_device.join("vendor"))?;
    let device_id = read_hex_u32(&sysfs_device.join("device"))?;
    let driver = std::fs::read_link(sysfs_device.join("driver"))
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });

    Some(RenderNode {
        path,
        vendor_id,
        device_id,
        driver,
    })
}

fn read_hex_u32(path: &Path) -> Option<u32> {
    let value = std::fs::read_to_string(path).ok()?;
    u32::from_str_radix(value.trim().trim_start_matches("0x"), 16).ok()
}

fn query_decode_capabilities(
    display: &Display,
) -> Result<Vec<VideoDecodeCapability>, Box<dyn std::error::Error>> {
    let profiles = display.query_config_profiles()?;
    let image_formats = display.query_image_formats().unwrap_or_default();
    let fourccs = image_formats
        .iter()
        .map(|format| format.fourcc)
        .collect::<Vec<_>>();
    let mut capabilities = Vec::new();

    for profile in profiles {
        let Some(mapped_profile) = map_profile(profile) else {
            continue;
        };
        let entrypoints = display.query_config_entrypoints(profile)?;
        if !entrypoints.contains(&VAEntrypoint::VAEntrypointVLD) {
            continue;
        }

        let mut attrs = vec![
            VAConfigAttrib {
                type_: VAConfigAttribType::VAConfigAttribRTFormat,
                value: 0,
            },
            VAConfigAttrib {
                type_: VAConfigAttribType::VAConfigAttribMaxPictureWidth,
                value: 0,
            },
            VAConfigAttrib {
                type_: VAConfigAttribType::VAConfigAttribMaxPictureHeight,
                value: 0,
            },
        ];
        display.get_config_attributes(profile, VAEntrypoint::VAEntrypointVLD, &mut attrs)?;
        let rt_format = attr_value(&attrs, VAConfigAttribType::VAConfigAttribRTFormat)
            .filter(|value| *value != VA_ATTRIB_NOT_SUPPORTED)
            .unwrap_or(0);
        let mut output_formats = output_formats_from_rt_and_fourcc(rt_format, &fourccs);
        output_formats.sort_by_key(|format| format.wire_value());
        output_formats.dedup();
        if output_formats.is_empty() {
            continue;
        }

        let max_width = attr_value(&attrs, VAConfigAttribType::VAConfigAttribMaxPictureWidth)
            .filter(|value| *value != VA_ATTRIB_NOT_SUPPORTED && *value != 0)
            .unwrap_or(DEFAULT_MAX_WIDTH);
        let max_height = attr_value(&attrs, VAConfigAttribType::VAConfigAttribMaxPictureHeight)
            .filter(|value| *value != VA_ATTRIB_NOT_SUPPORTED && *value != 0)
            .unwrap_or(DEFAULT_MAX_HEIGHT);

        let (codec, profile, bit_depth, chroma) = match mapped_profile {
            MappedProfile::H264(profile) => (
                qgs_protocol::VideoCodec::H264,
                VideoProfile::H264(profile),
                BitDepth::new(8)?,
                ChromaSubsampling::Cs420,
            ),
            MappedProfile::Mpeg2(profile) => (
                qgs_protocol::VideoCodec::Mpeg2,
                VideoProfile::Mpeg2(profile),
                BitDepth::new(8)?,
                ChromaSubsampling::Cs420,
            ),
        };

        let capability = VideoDecodeCapability {
            codec,
            profile,
            bit_depth,
            chroma,
            max_width,
            max_height,
            progressive_supported: true,
            interlaced_supported: false,
            output_surface_formats: output_formats,
        };
        capability.validate()?;
        push_unique_capability(&mut capabilities, capability);
    }

    Ok(capabilities)
}

fn attr_value(attrs: &[VAConfigAttrib], attr_type: VAConfigAttribType::Type) -> Option<u32> {
    attrs
        .iter()
        .find(|attr| attr.type_ == attr_type)
        .map(|attr| attr.value)
}

fn output_formats_from_rt_and_fourcc(rt_format: u32, fourccs: &[u32]) -> Vec<VideoSurfaceFormat> {
    let mut formats = Vec::new();
    if rt_format & VA_RT_FORMAT_YUV420 != 0 && fourccs.contains(&VA_FOURCC_NV12) {
        formats.push(VideoSurfaceFormat::Nv12);
    }
    if rt_format & VA_RT_FORMAT_YUV420_10 != 0 && fourccs.contains(&VA_FOURCC_P010) {
        formats.push(VideoSurfaceFormat::P010);
    }
    if rt_format & VA_RT_FORMAT_YUV422 != 0
        && (fourccs.contains(&VA_FOURCC_YUY2) || fourccs.contains(&VA_FOURCC_UYVY))
    {
        formats.push(VideoSurfaceFormat::Yuv422_8);
    }
    if rt_format & VA_RT_FORMAT_YUV422_10 != 0 && fourccs.contains(&VA_FOURCC_Y210) {
        formats.push(VideoSurfaceFormat::Yuv422_10);
    }
    formats
}

fn push_unique_capability(
    capabilities: &mut Vec<VideoDecodeCapability>,
    capability: VideoDecodeCapability,
) {
    if !capabilities.iter().any(|existing| existing == &capability) {
        capabilities.push(capability);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MappedProfile {
    H264(H264Profile),
    Mpeg2(Mpeg2Profile),
}

fn map_profile(profile: VAProfile::Type) -> Option<MappedProfile> {
    match profile {
        VAProfile::VAProfileH264ConstrainedBaseline => {
            Some(MappedProfile::H264(H264Profile::Baseline))
        }
        VAProfile::VAProfileH264Main => Some(MappedProfile::H264(H264Profile::Main)),
        VAProfile::VAProfileH264High => Some(MappedProfile::H264(H264Profile::High)),
        VAProfile::VAProfileMPEG2Main => Some(MappedProfile::Mpeg2(Mpeg2Profile::Main)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFESSIONAL_INTRA_FIXTURE: &[u8] =
        include_bytes!("../../../tests/fixtures/h264/professional-422-10bit-idr-128x72.h264");

    #[test]
    fn maps_va_h264_profiles_to_qgs_profiles() {
        assert_eq!(
            map_profile(VAProfile::VAProfileH264ConstrainedBaseline),
            Some(MappedProfile::H264(H264Profile::Baseline))
        );
        assert_eq!(
            map_profile(VAProfile::VAProfileH264Main),
            Some(MappedProfile::H264(H264Profile::Main))
        );
        assert_eq!(
            map_profile(VAProfile::VAProfileH264High),
            Some(MappedProfile::H264(H264Profile::High))
        );
    }

    #[test]
    fn maps_va_mpeg2_profile_to_qgs_profile() {
        assert_eq!(
            map_profile(VAProfile::VAProfileMPEG2Main),
            Some(MappedProfile::Mpeg2(Mpeg2Profile::Main))
        );
    }

    #[test]
    fn unsupported_va_profiles_are_ignored() {
        assert_eq!(map_profile(VAProfile::VAProfileJPEGBaseline), None);
        assert_eq!(map_profile(VAProfile::VAProfileNone), None);
    }

    #[test]
    fn decode_entrypoint_is_distinct_from_encode_entrypoints() {
        assert_ne!(
            VAEntrypoint::VAEntrypointVLD,
            VAEntrypoint::VAEntrypointEncSlice
        );
    }

    #[test]
    fn translates_surface_formats_from_rt_format_and_fourcc() {
        let formats = output_formats_from_rt_and_fourcc(
            VA_RT_FORMAT_YUV420 | VA_RT_FORMAT_YUV422 | VA_RT_FORMAT_YUV420_10,
            &[VA_FOURCC_NV12, VA_FOURCC_YUY2, VA_FOURCC_P010],
        );

        assert_eq!(
            formats,
            vec![
                VideoSurfaceFormat::Nv12,
                VideoSurfaceFormat::P010,
                VideoSurfaceFormat::Yuv422_8,
            ]
        );
    }

    #[test]
    fn ignores_rt_format_without_matching_surface_fourcc() {
        assert!(output_formats_from_rt_and_fourcc(VA_RT_FORMAT_YUV422_10, &[]).is_empty());
    }

    #[test]
    fn duplicate_capabilities_are_not_inserted() {
        let capability = VideoDecodeCapability {
            codec: qgs_protocol::VideoCodec::H264,
            profile: VideoProfile::H264(H264Profile::High),
            bit_depth: BitDepth::new(8).expect("bit depth"),
            chroma: ChromaSubsampling::Cs420,
            max_width: 1920,
            max_height: 1080,
            progressive_supported: true,
            interlaced_supported: false,
            output_surface_formats: vec![VideoSurfaceFormat::Nv12],
        };
        let mut capabilities = Vec::new();

        push_unique_capability(&mut capabilities, capability.clone());
        push_unique_capability(&mut capabilities, capability);

        assert_eq!(capabilities.len(), 1);
    }

    #[test]
    fn professional_h264_stream_does_not_match_8bit_420_decoder_config() {
        let parsed = qgs_codec_h264::parse_annex_b_access_unit(PROFESSIONAL_INTRA_FIXTURE)
            .expect("professional H.264 stream parses");
        let config = DecoderConfig {
            device_id: DeviceId::new(1).expect("device id"),
            codec: VideoCodec::H264,
            profile: VideoProfile::H264(H264Profile::High),
            bit_depth: BitDepth::new(8).expect("8-bit"),
            chroma: ChromaSubsampling::Cs420,
            coded_width: 128,
            coded_height: 72,
            scan_mode: qgs_protocol::ScanMode::Progressive,
        };

        assert!(!parsed_access_unit_matches_config(&parsed, &config));
    }

    #[test]
    fn render_node_matching_reads_known_sysfs_values_when_available() {
        let intel = render_node_from_path(PathBuf::from("/dev/dri/renderD128"));
        if let Some(node) = intel {
            assert_eq!(node.vendor_id, 0x8086);
        }
    }
}

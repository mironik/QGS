//! Video payload brick: persistent proxy/original picture decode → GPU frame.
//! Produces monitor-facing readback pixels; does not open windows or write files.
//! Backend tokens: VaapiCpuNv12Vulkan (proxy), SoftwareH264Yuv422P10Vulkan (legacy original).
//! The legacy original path still uses qgs-software-video/rsmpeg until the
//! qgs-h264-422p10 brick produces real yuv422p10le planes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use qgs_core::{
    BackendDecodedSurface, BackendDecoder, DecoderBackend, DeviceDiscovery,
    VideoCapabilityDiscovery,
};
use qgs_h264_422p10::H264422P10Frame;
use qgs_media_runtime::QgsPlaybackRepresentation;
use qgs_mp4::{classify_video_track, nearest_random_access_before, Mp4Source};
use qgs_protocol::{
    BitDepth, CreateDecoderRequest, DecoderId, FlushDecoderRequest, SubmitAccessUnitRequest,
    VisibleRegion,
};
use qgs_software_video::{decoder_config_for_surface, SoftwareH264Decoder};
use qgs_vulkan::{
    FrameIdentity, GpuFrameProcessor, GpuFrameProcessorConfig, Nv12FrameProcessor,
    Nv12FrameProcessorConfig, Nv12Plane, Nv12Upload, VulkanDeviceDiscovery, YcbcrConversion,
    Yuv422P10Plane, Yuv422P10Upload,
};

use crate::device_selection::{select_integrated_or_discrete_gpu, select_integrated_vulkan_gpu};

/// Monitor-facing GPU readback of one processed engine frame.
/// This is diagnostic display input, not the authoritative GPU product.
#[derive(Clone, Debug)]
pub struct EnginePixels {
    pub width: u32,
    pub height: u32,
    pub rgba_u16: Vec<u16>,
    pub checksum: u64,
}

/// Persistent QGS picture session. Owns proxy VAAPI + Nv12 GPU and original
/// software-decode + YUV422P10 GPU processors. Advances with the playhead.
/// Does not open a window or write files.
pub struct LiveEnginePictureSession {
    original_path: PathBuf,
    proxy_path: PathBuf,
    original_picture: Option<OriginalPictureSource>,
    proxy_picture: Option<ProxyPictureSource>,
}

struct ProxyPictureSource {
    track: qgs_mp4::Mp4VideoTrack,
    device_id: qgs_protocol::DeviceId,
    decoder_id: DecoderId,
    config: qgs_protocol::DecoderConfig,
    discovery: qgs_vaapi::VaapiVideoDiscovery,
    decoder: Box<dyn BackendDecoder>,
    cpu_pool: qgs_vaapi::CpuNv12FramePool,
    gpu: Nv12FrameProcessor,
    next_sample: usize,
    next_output: u64,
    started: bool,
    flushed: bool,
    pixels: BTreeMap<u64, EnginePixels>,
}

struct OriginalPictureSource {
    bytes: Vec<u8>,
    source: qgs_mxf::MediaSource,
    device_id: qgs_protocol::DeviceId,
    config: qgs_protocol::DecoderConfig,
    decoder: SoftwareH264Decoder,
    gpu: GpuFrameProcessor,
    next_edit_unit: u64,
    started: bool,
    surfaces: BTreeMap<u64, qgs_software_video::SoftwareVideoSurface>,
}

impl LiveEnginePictureSession {
    pub fn new(original_path: &Path, proxy_path: &Path) -> Self {
        Self {
            original_path: original_path.to_path_buf(),
            proxy_path: proxy_path.to_path_buf(),
            original_picture: None,
            proxy_picture: None,
        }
    }

    pub fn pixels_for(
        &mut self,
        frame: u64,
        picture: QgsPlaybackRepresentation,
    ) -> Result<EnginePixels, Box<dyn std::error::Error>> {
        match picture {
            QgsPlaybackRepresentation::Proxy => self.read_proxy_frame(frame),
            QgsPlaybackRepresentation::Original => self.read_original_frame(frame),
        }
    }

    fn read_proxy_frame(&mut self, frame: u64) -> Result<EnginePixels, Box<dyn std::error::Error>> {
        self.ensure_proxy_picture()?;
        self.advance_proxy_decoder(frame)?;
        self.proxy_picture
            .as_mut()
            .and_then(|picture| picture.pixels.remove(&frame))
            .ok_or_else(|| format!("missing decoded proxy frame {frame}").into())
    }

    fn advance_proxy_decoder(&mut self, frame: u64) -> Result<(), Box<dyn std::error::Error>> {
        let picture = self
            .proxy_picture
            .as_mut()
            .ok_or("proxy picture source is not loaded")?;
        if picture.pixels.contains_key(&frame) {
            return Ok(());
        }
        let last_index = picture.track.samples.len().saturating_sub(1);
        if frame > u64::try_from(last_index)? {
            return Err(format!("proxy picture frame {frame} is past the source").into());
        }
        if !picture.started || picture.next_output > frame || picture.flushed {
            let target = u32::try_from(frame)?;
            let start = nearest_random_access_before(&picture.track, target)
                .and_then(|sample_index| {
                    picture
                        .track
                        .samples
                        .iter()
                        .position(|sample| sample.sample_index == sample_index)
                })
                .unwrap_or(0);
            picture.decoder = picture
                .discovery
                .create_decoder(&CreateDecoderRequest {
                    config: picture.config.clone(),
                })
                .map_err(|err| format!("proxy picture decoder reset failed: {err:?}"))?;
            picture.pixels.clear();
            picture.next_sample = start;
            picture.next_output = u64::try_from(start)?;
            picture.started = true;
            picture.flushed = false;
        }
        let keep_from = frame.saturating_sub(2);
        while !picture.pixels.contains_key(&frame) {
            if picture.next_sample < picture.track.samples.len() {
                let sample = picture.track.samples[picture.next_sample].annex_b.clone();
                let outputs = picture
                    .decoder
                    .submit_access_unit(&SubmitAccessUnitRequest {
                        decoder_id: picture.decoder_id,
                        data: sample,
                    })
                    .map_err(|err| {
                        format!(
                            "proxy picture decode failed at sample {}: {err:?}",
                            picture.next_sample
                        )
                    })?;
                picture.next_sample = picture.next_sample.saturating_add(1);
                for output in outputs {
                    store_proxy_output(picture, output, keep_from)?;
                }
            } else if !picture.flushed {
                let outputs = picture
                    .decoder
                    .flush(&FlushDecoderRequest {
                        decoder_id: picture.decoder_id,
                    })
                    .map_err(|err| format!("proxy picture decoder flush failed: {err:?}"))?;
                picture.flushed = true;
                for output in outputs {
                    store_proxy_output(picture, output, keep_from)?;
                }
            } else {
                break;
            }
        }
        picture.pixels.retain(|index, _| *index >= keep_from);
        Ok(())
    }

    fn ensure_proxy_picture(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.proxy_picture.is_some() {
            return Ok(());
        }
        let proxy = Mp4Source::open(&self.proxy_path)?;
        let track = proxy.video.ok_or("proxy has no H.264 video track")?;
        let h264 = classify_video_track(&track)?;
        let discovery = VulkanDeviceDiscovery::new()?;
        let device = select_integrated_vulkan_gpu(&discovery)
            .map_err(|err| format!("live engine proxy picture: {err}"))?;
        let devices = discovery.enumerate_devices()?;
        let config = decoder_config_for_surface(
            device.id,
            h264.profile,
            BitDepth::new(h264.bit_depth)?,
            h264.chroma,
            h264.coded_width,
            h264.coded_height,
        );
        let vaapi = qgs_vaapi::VaapiVideoDiscovery::new(&devices);
        let capabilities = vaapi.query_video_capabilities(device.id)?;
        let supports_proxy = capabilities
            .decode
            .iter()
            .any(|capability| config.is_satisfied_by(capability));
        if !supports_proxy {
            return Err(
                "live engine proxy picture: Intel VA backend does not support this stream".into(),
            );
        }
        let decoder = vaapi
            .create_decoder(&CreateDecoderRequest {
                config: config.clone(),
            })
            .map_err(|err| format!("proxy picture decoder failed: {err:?}"))?;
        let visible_region = VisibleRegion {
            x: 0,
            y: 0,
            width: h264.width,
            height: h264.height,
        };
        let cpu_pool = qgs_vaapi::CpuNv12FramePool::new(
            h264.coded_width,
            h264.coded_height,
            visible_region,
            2,
        )
        .map_err(|err| format!("proxy picture cpu pool failed: {err:?}"))?;
        let gpu = Nv12FrameProcessor::new(
            &discovery,
            Nv12FrameProcessorConfig {
                device_id: device.id,
                coded_width: h264.coded_width,
                coded_height: h264.coded_height,
                visible_width: h264.width,
                visible_height: h264.height,
                slot_count: 2,
                conversion: YcbcrConversion::Rec709Limited,
                validation_readback: true,
            },
        )?;
        self.proxy_picture = Some(ProxyPictureSource {
            track,
            device_id: device.id,
            decoder_id: DecoderId::new(20)?,
            config,
            discovery: vaapi,
            decoder,
            cpu_pool,
            gpu,
            next_sample: 0,
            next_output: 0,
            started: false,
            flushed: false,
            pixels: BTreeMap::new(),
        });
        Ok(())
    }

    fn read_original_frame(
        &mut self,
        frame: u64,
    ) -> Result<EnginePixels, Box<dyn std::error::Error>> {
        self.ensure_original_picture()?;
        self.advance_original_decoder(frame)?;
        let picture = self
            .original_picture
            .as_mut()
            .ok_or("original picture source is not loaded")?;
        let surface = picture
            .surfaces
            .remove(&frame)
            .ok_or_else(|| format!("missing decoded original frame {frame}"))?;
        let upload = yuv422p10_upload_for_frame(picture.device_id, &surface)?;
        let submit = picture.gpu.submit_frame(
            &upload,
            FrameIdentity {
                presentation_position: frame,
            },
        );
        let wait = submit.and_then(|token| picture.gpu.wait_for_frame(token));
        picture.surfaces.insert(frame, surface);
        let readback = wait.map_err(|err| format!("original picture gpu path failed: {err:?}"))?;
        Ok(EnginePixels {
            width: readback.width,
            height: readback.height,
            rgba_u16: readback.rgba_u16,
            checksum: readback.checksum,
        })
    }

    fn advance_original_decoder(&mut self, frame: u64) -> Result<(), Box<dyn std::error::Error>> {
        let picture = self
            .original_picture
            .as_mut()
            .ok_or("original picture source is not loaded")?;
        if picture.surfaces.contains_key(&frame) {
            return Ok(());
        }
        let last_video_index = u64::try_from(picture.source.index.video.len().saturating_sub(1))?;
        if frame > last_video_index {
            return Err(format!("original picture frame {frame} is past the source").into());
        }
        if !picture.started || picture.next_edit_unit > frame {
            let start = picture
                .source
                .index
                .nearest_random_access_before(frame)
                .map(|entry| entry.edit_unit)
                .unwrap_or(frame);
            picture.decoder = SoftwareH264Decoder::new(picture.config.clone())
                .map_err(|err| format!("original picture decoder reset failed: {err:?}"))?;
            picture.surfaces.clear();
            picture.next_edit_unit = start;
            picture.started = true;
        }
        let decode_end = frame.saturating_add(16).min(last_video_index);
        while picture.next_edit_unit <= decode_end && !picture.surfaces.contains_key(&frame) {
            let index = usize::try_from(picture.next_edit_unit)?;
            let access_unit = picture
                .source
                .extract_video_access_unit(&picture.bytes, index)?;
            let edit_unit = picture.next_edit_unit;
            let decoded = picture
                .decoder
                .decode_access_unit_at(&access_unit, edit_unit)
                .map_err(|err| format!("original picture decode failed at {edit_unit}: {err:?}"))?;
            picture.next_edit_unit = picture.next_edit_unit.saturating_add(1);
            for surface in decoded {
                picture.surfaces.insert(surface.presentation_index, surface);
            }
        }
        let keep_from = frame.saturating_sub(2);
        picture.surfaces.retain(|index, _| *index >= keep_from);
        Ok(())
    }

    fn ensure_original_picture(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.original_picture.is_some() {
            return Ok(());
        }
        let bytes = std::fs::read(&self.original_path)?;
        let source = qgs_mxf::MediaSource::parse(&bytes)?;
        let first_access_unit = source.extract_video_access_unit(&bytes, 0)?;
        let parsed = qgs_codec_h264::parse_annex_b_access_unit(&first_access_unit)?;
        let discovery = VulkanDeviceDiscovery::new()?;
        let device = select_integrated_or_discrete_gpu(&discovery)
            .map_err(|err| format!("live engine original picture: {err}"))?;
        let config = decoder_config_for_surface(
            device.id,
            parsed.profile,
            parsed.desc.bit_depth,
            parsed.desc.chroma,
            parsed.desc.coded_width,
            parsed.desc.coded_height,
        );
        if !qgs_software_video::SoftwareVideoBackend::supports_config(&config) {
            return Err("original picture software decoder does not support this stream".into());
        }
        let decoder = SoftwareH264Decoder::new(config.clone())
            .map_err(|err| format!("original picture decoder failed: {err:?}"))?;
        let gpu = GpuFrameProcessor::new(
            &discovery,
            GpuFrameProcessorConfig {
                device_id: device.id,
                width: parsed.desc.coded_width,
                height: parsed.desc.coded_height,
                slot_count: 2,
                conversion: YcbcrConversion::Rec709Limited,
            },
        )?;
        self.original_picture = Some(OriginalPictureSource {
            bytes,
            source,
            device_id: device.id,
            config,
            decoder,
            gpu,
            next_edit_unit: 0,
            started: false,
            surfaces: BTreeMap::new(),
        });
        Ok(())
    }
}

fn store_proxy_output(
    picture: &mut ProxyPictureSource,
    output: BackendDecodedSurface,
    keep_from: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let frame = picture.next_output;
    picture.next_output = picture.next_output.saturating_add(1);
    if frame < keep_from {
        return Ok(());
    }
    let cpu_frame = picture
        .cpu_pool
        .acquire()
        .map_err(|err| format!("proxy picture cpu frame acquire failed: {err:?}"))?;
    let (cpu_frame, _) =
        qgs_vaapi::transfer_nv12_surface_timed(output.resource.as_ref(), cpu_frame)
            .map_err(|err| format!("proxy picture transfer failed: {err:?}"))?;
    let upload = nv12_upload_for_cpu_surface(picture.device_id, &cpu_frame)?;
    let token = picture
        .gpu
        .submit_frame(
            &upload,
            FrameIdentity {
                presentation_position: frame,
            },
        )
        .map_err(|err| format!("proxy picture gpu submit failed: {err:?}"))?;
    let readback = picture
        .gpu
        .wait_for_frame(token)
        .map_err(|err| format!("proxy picture gpu readback failed: {err:?}"))?;
    picture
        .cpu_pool
        .release(cpu_frame)
        .map_err(|err| format!("proxy picture cpu frame release failed: {err:?}"))?;
    picture.pixels.insert(
        frame,
        EnginePixels {
            width: readback.width,
            height: readback.height,
            rgba_u16: readback.rgba_u16,
            checksum: readback.checksum,
        },
    );
    Ok(())
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

pub fn yuv422p10_upload_for_native_frame<'a>(
    device_id: qgs_protocol::DeviceId,
    frame: &'a H264422P10Frame,
) -> Result<Yuv422P10Upload<'a>, Box<dyn std::error::Error>> {
    frame.validate_layout()?;
    let plane = |plane: &'a qgs_h264_422p10::H264422P10Plane| -> Yuv422P10Plane<'a> {
        Yuv422P10Plane {
            width_samples: plane.width_samples,
            height: plane.height,
            stride_bytes: plane.stride_bytes,
            data: &plane.data,
        }
    };
    Ok(Yuv422P10Upload {
        device_id,
        width: frame.coded_width,
        height: frame.coded_height,
        y: plane(&frame.y),
        cb: plane(&frame.cb),
        cr: plane(&frame.cr),
        conversion: YcbcrConversion::Rec709Limited,
    })
}

#[cfg(test)]
mod tests {
    use super::yuv422p10_upload_for_native_frame;
    use qgs_h264_422p10::{H264422P10Frame, H264422P10Plane};

    #[test]
    fn native_h264422p10_frame_maps_to_vulkan_upload_planes() {
        let y = H264422P10Plane {
            width_samples: 4,
            height: 2,
            stride_bytes: 8,
            data: vec![1; 16],
        };
        let cb = H264422P10Plane {
            width_samples: 2,
            height: 2,
            stride_bytes: 4,
            data: vec![2; 8],
        };
        let cr = H264422P10Plane {
            width_samples: 2,
            height: 2,
            stride_bytes: 4,
            data: vec![3; 8],
        };
        let frame = H264422P10Frame {
            presentation_index: 7,
            coded_width: 4,
            coded_height: 2,
            visible_width: 4,
            visible_height: 2,
            y,
            cb,
            cr,
        };

        let upload =
            yuv422p10_upload_for_native_frame(qgs_protocol::DeviceId::new(1).unwrap(), &frame)
                .expect("native upload");

        assert_eq!(upload.width, 4);
        assert_eq!(upload.height, 2);
        assert_eq!(upload.y.width_samples, 4);
        assert_eq!(upload.cb.width_samples, 2);
        assert_eq!(upload.cr.width_samples, 2);
        assert_eq!(upload.y.data[0], 1);
        assert_eq!(upload.cb.data[0], 2);
        assert_eq!(upload.cr.data[0], 3);
    }
}

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use libva::{
    Display, VAConfigAttrib, VAConfigAttribType, VAEntrypoint, VAProfile, VA_ATTRIB_NOT_SUPPORTED,
    VA_FOURCC_NV12, VA_FOURCC_P010, VA_FOURCC_UYVY, VA_FOURCC_Y210, VA_FOURCC_YUY2,
    VA_RT_FORMAT_YUV420, VA_RT_FORMAT_YUV420_10, VA_RT_FORMAT_YUV422, VA_RT_FORMAT_YUV422_10,
};
use qgs_core::{VideoCapabilityDiscovery, VideoCapabilityDiscoveryError};
use qgs_protocol::{
    BitDepth, ChromaSubsampling, DeviceDesc, DeviceId, H264Profile, Mpeg2Profile,
    VideoCapabilities, VideoDecodeCapability, VideoProfile, VideoSurfaceFormat,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaapiDeviceInfo {
    pub device_id: DeviceId,
    pub path: PathBuf,
    pub vendor_id: u32,
    pub device_id_pci: u32,
    pub driver: Option<String>,
}

#[derive(Debug)]
struct VaapiDevice {
    path: PathBuf,
    vendor_id: u32,
    device_id: u32,
    driver: Option<String>,
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
    fn render_node_matching_reads_known_sysfs_values_when_available() {
        let intel = render_node_from_path(PathBuf::from("/dev/dri/renderD128"));
        if let Some(node) = intel {
            assert_eq!(node.vendor_id, 0x8086);
        }
    }
}

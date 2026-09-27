#![forbid(unsafe_code)]

use core::fmt;

/// Identifies QGS protocol messages.
pub const PROTOCOL_MAGIC: u32 = 0x5147_5300;

/// The protocol version implemented by this crate.
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::new(0, 1);

/// Fixed QGS v0.1 wire header size in bytes.
pub const WIRE_HEADER_LEN: usize = 24;

/// Maximum payload size accepted by QGS v0.1 wire decoding.
pub const MAX_PAYLOAD_LEN: u32 = 4096;

/// Maximum number of devices in one DEVICE_LIST response.
pub const MAX_DEVICE_COUNT: usize = 16;

/// Maximum UTF-8 device name size in bytes.
pub const MAX_DEVICE_NAME_LEN: usize = 128;

/// Maximum number of memory heaps in DEVICE_CAPABILITIES.
pub const MAX_MEMORY_HEAP_COUNT: usize = 16;

/// Maximum number of memory types summarized in DEVICE_CAPABILITIES.
pub const MAX_MEMORY_TYPE_COUNT: usize = 32;

/// Conservative M1 single-buffer allocation safety limit.
pub const MAX_BUFFER_SIZE_BYTES: u64 = 64 * 1024 * 1024;

/// Conservative M1 maximum image width.
pub const MAX_IMAGE_WIDTH: u32 = 8192;

/// Conservative M1 maximum image height.
pub const MAX_IMAGE_HEIGHT: u32 = 8192;

/// Maximum number of decode capability entries in VIDEO_CAPABILITIES.
pub const MAX_VIDEO_DECODE_CAPABILITY_COUNT: usize = 32;

/// Maximum number of output surface formats per video decode capability.
pub const MAX_VIDEO_OUTPUT_FORMAT_COUNT: usize = 8;

/// Conservative M2 maximum coded VideoSurface width.
pub const MAX_VIDEO_SURFACE_WIDTH: u32 = 8192;

/// Conservative M2 maximum coded VideoSurface height.
pub const MAX_VIDEO_SURFACE_HEIGHT: u32 = 8192;

const DEVICE_ENTRY_FIXED_LEN: usize = 32;
const DEVICE_CAPABILITIES_FIXED_PREFIX_LEN: usize = 52;
const MEMORY_HEAP_ENTRY_LEN: usize = 16;
const QUERY_VIDEO_CAPABILITIES_PAYLOAD_LEN: usize = 8;
const VIDEO_CAPABILITIES_PREFIX_LEN: usize = 12;
const VIDEO_DECODE_CAPABILITY_FIXED_LEN: usize = 16;
const CREATE_BUFFER_PAYLOAD_LEN: usize = 24;
const CREATE_IMAGE_PAYLOAD_LEN: usize = 24;
const BUFFER_CREATED_PAYLOAD_LEN: usize = 24;
const IMAGE_CREATED_PAYLOAD_LEN: usize = 24;
const DESTROY_RESOURCE_PAYLOAD_LEN: usize = 8;
const RESOURCE_DESTROYED_PAYLOAD_LEN: usize = 8;
const EXPORT_RESOURCE_PAYLOAD_LEN: usize = 16;
const RESOURCE_EXPORTED_PAYLOAD_LEN: usize = 72;
const CREATE_SYNC_PAYLOAD_LEN: usize = 16;
const EXPORT_SYNC_PAYLOAD_LEN: usize = 24;
const SYNC_CREATED_PAYLOAD_LEN: usize = 8;
const SYNC_EXPORTED_PAYLOAD_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// A typed session identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SessionId(u64);

impl SessionId {
    pub fn new(raw: u64) -> Result<Self, ProtocolError> {
        if raw == 0 {
            Err(ProtocolError::InvalidSessionId)
        } else {
            Ok(Self(raw))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Conceptual message header shared by protocol messages.
///
/// These Rust in-memory types are intentionally separate from the wire ABI.
/// The v0.1 wire representation below is explicitly encoded little-endian
/// bytes; it is never produced by copying Rust struct memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageHeader {
    pub magic: u32,
    pub version: ProtocolVersion,
    pub kind: MessageKind,
    pub body_len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKind {
    Request,
    Response,
    Event,
}

impl MessageKind {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Request => 1,
            Self::Response => 2,
            Self::Event => 3,
        }
    }
}

impl TryFrom<u8> for MessageKind {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Request),
            2 => Ok(Self::Response),
            3 => Ok(Self::Event),
            _ => Err(ProtocolError::UnknownMessageKind { kind: value }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    Hello(HelloRequest),
    EnumerateDevices(EnumerateDevicesRequest),
    QueryDeviceCapabilities(QueryDeviceCapabilitiesRequest),
    QueryVideoCapabilities(QueryVideoCapabilitiesRequest),
    CreateBuffer(CreateBufferRequest),
    CreateImage(CreateImageRequest),
    DestroyResource(DestroyResourceRequest),
    ExportResource(ExportResourceRequest),
    CreateSync(CreateSyncRequest),
    ExportSync(ExportSyncRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelloRequest {
    pub min_version: ProtocolVersion,
    pub max_version: ProtocolVersion,
}

impl HelloRequest {
    pub const fn current() -> Self {
        Self {
            min_version: CURRENT_PROTOCOL_VERSION,
            max_version: CURRENT_PROTOCOL_VERSION,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumerateDevicesRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryDeviceCapabilitiesRequest {
    pub device_id: DeviceId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryVideoCapabilitiesRequest {
    pub device_id: DeviceId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateBufferRequest {
    pub desc: BufferDesc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateImageRequest {
    pub desc: ImageDesc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestroyResourceRequest {
    pub resource_id: ResourceId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportResourceRequest {
    pub resource_id: ResourceId,
    pub handle_type: ExternalHandleType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateSyncRequest {
    pub device_id: DeviceId,
    pub kind: SyncKind,
    pub handle_type: SyncExportHandleType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportSyncRequest {
    pub sync_id: SyncId,
    pub resource_id: ResourceId,
    pub fill_pattern: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Response {
    Welcome(WelcomeResponse),
    Error(ErrorResponse),
    DeviceList(DeviceListResponse),
    DeviceCapabilities(DeviceCapabilitiesResponse),
    VideoCapabilities(VideoCapabilitiesResponse),
    BufferCreated(BufferCreatedResponse),
    ImageCreated(ImageCreatedResponse),
    ResourceDestroyed(ResourceDestroyedResponse),
    ResourceExported(ResourceExportedResponse),
    SyncCreated(SyncCreatedResponse),
    SyncExported(SyncExportedResponse),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WelcomeResponse {
    pub version: ProtocolVersion,
    pub session_id: SessionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorResponse {
    pub code: ProtocolErrorCode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceDesc>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceId(u64);

impl DeviceId {
    pub fn new(raw: u64) -> Result<Self, ProtocolError> {
        if raw == 0 {
            Err(ProtocolError::InvalidDeviceId)
        } else {
            Ok(Self(raw))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceClass {
    IntegratedGpu,
    DiscreteGpu,
    Software,
    Other,
}

impl DeviceClass {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::IntegratedGpu => 1,
            Self::DiscreteGpu => 2,
            Self::Software => 3,
            Self::Other => 4,
        }
    }
}

impl TryFrom<u8> for DeviceClass {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::IntegratedGpu),
            2 => Ok(Self::DiscreteGpu),
            3 => Ok(Self::Software),
            4 => Ok(Self::Other),
            _ => Err(ProtocolError::MalformedPayload),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendApi {
    Vulkan,
}

impl BackendApi {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Vulkan => 1,
        }
    }
}

impl TryFrom<u8> for BackendApi {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Vulkan),
            _ => Err(ProtocolError::MalformedPayload),
        }
    }
}

impl fmt::Display for BackendApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vulkan => write!(f, "Vulkan"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApiVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ApiVersion {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceDesc {
    pub id: DeviceId,
    pub class: DeviceClass,
    pub vendor_id: u32,
    pub device_id: u32,
    pub name: String,
    pub backend: BackendApi,
    pub api_version: ApiVersion,
    pub driver_version: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceCapabilitiesResponse {
    pub capabilities: DeviceCapabilities,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceId(u64);

impl ResourceId {
    pub fn new(raw: u64) -> Result<Self, ProtocolError> {
        if raw == 0 {
            Err(ProtocolError::InvalidResourceId)
        } else {
            Ok(Self(raw))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    Buffer,
    Image,
    VideoSurface,
}

impl ResourceKind {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Buffer => 1,
            Self::Image => 2,
            Self::VideoSurface => 3,
        }
    }
}

impl TryFrom<u8> for ResourceKind {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Buffer),
            2 => Ok(Self::Image),
            3 => Ok(Self::VideoSurface),
            _ => Err(ProtocolError::MalformedPayload),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferUsageFlags(u32);

impl BufferUsageFlags {
    pub const TRANSFER_SRC: Self = Self(0x1);
    pub const TRANSFER_DST: Self = Self(0x2);
    pub const STORAGE: Self = Self(0x4);

    pub fn new(raw: u32) -> Result<Self, ProtocolError> {
        let flags = Self(raw);
        if raw == 0 || raw & !Self::all_known().bits() != 0 {
            Err(ProtocolError::InvalidBufferUsage { flags: raw })
        } else {
            Ok(flags)
        }
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    const fn all_known() -> Self {
        Self(Self::TRANSFER_SRC.0 | Self::TRANSFER_DST.0 | Self::STORAGE.0)
    }
}

impl std::ops::BitOr for BufferUsageFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Rgba8Unorm,
}

impl PixelFormat {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Rgba8Unorm => 1,
        }
    }
}

impl TryFrom<u8> for PixelFormat {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Rgba8Unorm),
            _ => Err(ProtocolError::UnsupportedPixelFormat),
        }
    }
}

impl fmt::Display for PixelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rgba8Unorm => write!(f, "Rgba8Unorm"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageUsageFlags(u32);

impl ImageUsageFlags {
    pub const TRANSFER_SRC: Self = Self(0x1);
    pub const TRANSFER_DST: Self = Self(0x2);
    pub const STORAGE: Self = Self(0x4);

    pub fn new(raw: u32) -> Result<Self, ProtocolError> {
        let flags = Self(raw);
        if raw == 0 || raw & !Self::all_known().bits() != 0 {
            Err(ProtocolError::UnsupportedImageUsage { flags: raw })
        } else {
            Ok(flags)
        }
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    const fn all_known() -> Self {
        Self(Self::TRANSFER_SRC.0 | Self::TRANSFER_DST.0 | Self::STORAGE.0)
    }
}

impl std::ops::BitOr for ImageUsageFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryPreference {
    pub device_preferred: bool,
    pub host_visible_required: bool,
    pub host_coherent_preferred: bool,
}

impl MemoryPreference {
    pub const fn device_preferred() -> Self {
        Self {
            device_preferred: true,
            host_visible_required: false,
            host_coherent_preferred: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferDesc {
    pub device_id: DeviceId,
    pub size_bytes: u64,
    pub usage: BufferUsageFlags,
    pub memory_preference: MemoryPreference,
    pub external_sharing: ExternalSharing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageDesc {
    pub device_id: DeviceId,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub usage: ImageUsageFlags,
    pub external_sharing: ExternalSharing,
}

impl ImageDesc {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_image_dimensions(self.width, self.height)?;
        ImageUsageFlags::new(self.usage.bits())?;
        if !matches!(
            self.external_sharing,
            ExternalSharing::None
                | ExternalSharing::Required {
                    handle_type: ExternalHandleType::DmaBuf
                }
                | ExternalSharing::Required {
                    handle_type: ExternalHandleType::OpaqueFd
                }
        ) {
            return Err(ProtocolError::UnsupportedImageExternalSharing);
        }
        Ok(())
    }
}

impl BufferDesc {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_buffer_size(self.size_bytes)?;
        BufferUsageFlags::new(self.usage.bits())?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectedMemoryProperties {
    pub device_local: bool,
    pub host_visible: bool,
    pub host_coherent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferCreatedResponse {
    pub resource_id: ResourceId,
    pub size_bytes: u64,
    pub selected_memory: SelectedMemoryProperties,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageCreatedResponse {
    pub resource_id: ResourceId,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub selected_memory: SelectedMemoryProperties,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoCodec {
    H264,
    Mpeg2,
}

impl VideoCodec {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::H264 => 1,
            Self::Mpeg2 => 2,
        }
    }
}

impl TryFrom<u8> for VideoCodec {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::H264),
            2 => Ok(Self::Mpeg2),
            _ => Err(ProtocolError::UnsupportedVideoCodec),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H264Profile {
    Baseline,
    Main,
    High,
    High422,
}

impl H264Profile {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Baseline => 1,
            Self::Main => 2,
            Self::High => 3,
            Self::High422 => 4,
        }
    }
}

impl TryFrom<u8> for H264Profile {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Baseline),
            2 => Ok(Self::Main),
            3 => Ok(Self::High),
            4 => Ok(Self::High422),
            _ => Err(ProtocolError::UnsupportedVideoProfile),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mpeg2Profile {
    Main,
    Profile422,
}

impl Mpeg2Profile {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Main => 1,
            Self::Profile422 => 2,
        }
    }
}

impl TryFrom<u8> for Mpeg2Profile {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Main),
            2 => Ok(Self::Profile422),
            _ => Err(ProtocolError::UnsupportedVideoProfile),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoProfile {
    H264(H264Profile),
    Mpeg2(Mpeg2Profile),
}

impl VideoProfile {
    pub const fn codec(self) -> VideoCodec {
        match self {
            Self::H264(_) => VideoCodec::H264,
            Self::Mpeg2(_) => VideoCodec::Mpeg2,
        }
    }

    pub const fn wire_value(self) -> u8 {
        match self {
            Self::H264(profile) => profile.wire_value(),
            Self::Mpeg2(profile) => profile.wire_value(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitDepth(u8);

impl BitDepth {
    pub fn new(value: u8) -> Result<Self, ProtocolError> {
        match value {
            8 | 10 | 12 => Ok(Self(value)),
            _ => Err(ProtocolError::InvalidVideoBitDepth { bit_depth: value }),
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChromaSubsampling {
    Cs420,
    Cs422,
    Cs444,
}

impl ChromaSubsampling {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Cs420 => 1,
            Self::Cs422 => 2,
            Self::Cs444 => 3,
        }
    }
}

impl TryFrom<u8> for ChromaSubsampling {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Cs420),
            2 => Ok(Self::Cs422),
            3 => Ok(Self::Cs444),
            _ => Err(ProtocolError::UnsupportedChromaSubsampling),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanMode {
    Progressive,
    Interlaced,
}

impl ScanMode {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Progressive => 1,
            Self::Interlaced => 2,
        }
    }
}

impl TryFrom<u8> for ScanMode {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Progressive),
            2 => Ok(Self::Interlaced),
            _ => Err(ProtocolError::UnsupportedScanMode),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldOrder {
    Unknown,
    TopFieldFirst,
    BottomFieldFirst,
}

impl FieldOrder {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::TopFieldFirst => 1,
            Self::BottomFieldFirst => 2,
        }
    }
}

impl TryFrom<u8> for FieldOrder {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::TopFieldFirst),
            2 => Ok(Self::BottomFieldFirst),
            _ => Err(ProtocolError::UnsupportedFieldOrder),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoSurfaceFormat {
    Nv12,
    P010,
    Yuv422_8,
    Yuv422_10,
}

impl VideoSurfaceFormat {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Nv12 => 1,
            Self::P010 => 2,
            Self::Yuv422_8 => 3,
            Self::Yuv422_10 => 4,
        }
    }

    pub const fn bit_depth(self) -> u8 {
        match self {
            Self::Nv12 | Self::Yuv422_8 => 8,
            Self::P010 | Self::Yuv422_10 => 10,
        }
    }

    pub const fn chroma(self) -> ChromaSubsampling {
        match self {
            Self::Nv12 | Self::P010 => ChromaSubsampling::Cs420,
            Self::Yuv422_8 | Self::Yuv422_10 => ChromaSubsampling::Cs422,
        }
    }
}

impl TryFrom<u8> for VideoSurfaceFormat {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Nv12),
            2 => Ok(Self::P010),
            3 => Ok(Self::Yuv422_8),
            4 => Ok(Self::Yuv422_10),
            _ => Err(ProtocolError::UnsupportedVideoSurfaceFormat),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisibleRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoSurfaceDesc {
    pub coded_width: u32,
    pub coded_height: u32,
    pub visible_region: VisibleRegion,
    pub format: VideoSurfaceFormat,
    pub bit_depth: BitDepth,
    pub chroma: ChromaSubsampling,
    pub scan_mode: ScanMode,
    pub field_order: FieldOrder,
}

impl VideoSurfaceDesc {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_video_surface_dimensions(self.coded_width, self.coded_height)?;
        validate_visible_region(self.visible_region, self.coded_width, self.coded_height)?;
        if self.format.bit_depth() != self.bit_depth.get()
            || self.format.chroma() != self.chroma
            || (self.scan_mode == ScanMode::Progressive && self.field_order != FieldOrder::Unknown)
        {
            return Err(ProtocolError::MalformedPayload);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoDecodeCapability {
    pub codec: VideoCodec,
    pub profile: VideoProfile,
    pub bit_depth: BitDepth,
    pub chroma: ChromaSubsampling,
    pub max_width: u32,
    pub max_height: u32,
    pub progressive_supported: bool,
    pub interlaced_supported: bool,
    pub output_surface_formats: Vec<VideoSurfaceFormat>,
}

impl VideoDecodeCapability {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.profile.codec() != self.codec {
            return Err(ProtocolError::UnsupportedVideoProfile);
        }
        validate_video_surface_dimensions(self.max_width, self.max_height)?;
        if !self.progressive_supported && !self.interlaced_supported {
            return Err(ProtocolError::MalformedPayload);
        }
        if self.output_surface_formats.is_empty()
            || self.output_surface_formats.len() > MAX_VIDEO_OUTPUT_FORMAT_COUNT
        {
            return Err(ProtocolError::VideoOutputFormatCountTooLarge {
                count: self.output_surface_formats.len(),
                max: MAX_VIDEO_OUTPUT_FORMAT_COUNT,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoCapabilities {
    pub device_id: DeviceId,
    pub decode: Vec<VideoDecodeCapability>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoCapabilitiesResponse {
    pub capabilities: VideoCapabilities,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceDestroyedResponse {
    pub resource_id: ResourceId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalHandleType {
    DmaBuf,
    OpaqueFd,
}

impl ExternalHandleType {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::DmaBuf => 1,
            Self::OpaqueFd => 2,
        }
    }
}

impl TryFrom<u8> for ExternalHandleType {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::DmaBuf),
            2 => Ok(Self::OpaqueFd),
            _ => Err(ProtocolError::MalformedPayload),
        }
    }
}

impl fmt::Display for ExternalHandleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DmaBuf => write!(f, "DMA-BUF"),
            Self::OpaqueFd => write!(f, "opaque-fd"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalSharing {
    None,
    Required { handle_type: ExternalHandleType },
}

impl ExternalSharing {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Required { handle_type } => handle_type.wire_value(),
        }
    }
}

impl TryFrom<u8> for ExternalSharing {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 | 2 => Ok(Self::Required {
                handle_type: ExternalHandleType::try_from(value)?,
            }),
            _ => Err(ProtocolError::MalformedPayload),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceExportedResponse {
    pub metadata: ExportedResourceMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportedResourceMetadata {
    pub resource_id: ResourceId,
    pub device_id: DeviceId,
    pub kind: ResourceKind,
    pub size_bytes: u64,
    pub allocation_size_bytes: u64,
    pub buffer_usage: Option<BufferUsageFlags>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub pixel_format: Option<PixelFormat>,
    pub image_usage: Option<ImageUsageFlags>,
    pub backend_memory_type_index: u32,
    pub backend_image_layout_token: Option<u64>,
    pub handle_type: ExternalHandleType,
    pub selected_memory: SelectedMemoryProperties,
    pub dedicated_allocation: bool,
    pub attachment_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SyncId(u64);

impl SyncId {
    pub fn new(raw: u64) -> Result<Self, ProtocolError> {
        if raw == 0 {
            Err(ProtocolError::InvalidSyncId)
        } else {
            Ok(Self(raw))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncKind {
    BinarySemaphore,
}

impl SyncKind {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::BinarySemaphore => 1,
        }
    }
}

impl TryFrom<u8> for SyncKind {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::BinarySemaphore),
            _ => Err(ProtocolError::MalformedPayload),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncExportHandleType {
    SyncFd,
}

impl SyncExportHandleType {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::SyncFd => 1,
        }
    }
}

impl TryFrom<u8> for SyncExportHandleType {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::SyncFd),
            _ => Err(ProtocolError::MalformedPayload),
        }
    }
}

impl fmt::Display for SyncExportHandleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SyncFd => write!(f, "sync-fd"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncCreatedResponse {
    pub sync_id: SyncId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncExportedResponse {
    pub metadata: ExportedSyncMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportedSyncMetadata {
    pub sync_id: SyncId,
    pub handle_type: SyncExportHandleType,
    pub attachment_count: u8,
    pub fill_pattern: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceCapabilities {
    pub device_id: DeviceId,
    pub compute: ComputeCapabilities,
    pub memory: MemoryCapabilities,
    pub interop: InteropCapabilities,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComputeCapabilities {
    pub supported: bool,
    pub max_workgroup_count: [u32; 3],
    pub max_workgroup_size: [u32; 3],
    pub max_workgroup_invocations: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryCapabilities {
    pub heaps: Vec<MemoryHeapDesc>,
    pub memory_type_count: u16,
    pub host_visible: bool,
    pub host_coherent: bool,
    pub device_local: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryHeapDesc {
    pub size_bytes: u64,
    pub device_local: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteropCapabilities {
    pub external_memory_fd: bool,
    pub dma_buf: bool,
    pub external_semaphore_fd: bool,
    pub external_fence_fd: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    SessionClosed { session_id: SessionId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolErrorCode {
    InvalidMagic = 1,
    UnknownMessageKind = 2,
    UnknownOpcode = 3,
    PayloadTooLarge = 4,
    TruncatedHeader = 5,
    TruncatedPayload = 6,
    MalformedPayload = 7,
    InvalidVersionRange = 8,
    UnsupportedVersion = 9,
    InvalidSessionId = 10,
    SessionIdsExhausted = 11,
    InvalidFlags = 12,
    InvalidDeviceId = 13,
    DeviceCountTooLarge = 14,
    DeviceNameTooLong = 15,
    InvalidUtf8 = 16,
    SessionRequired = 17,
    DiscoveryFailed = 18,
    UnknownDeviceId = 19,
    MemoryHeapCountTooLarge = 20,
    MemoryTypeCountTooLarge = 21,
    InvalidResourceId = 22,
    UnknownResource = 23,
    InvalidBufferSize = 24,
    AllocationFailed = 25,
    UnsupportedMemoryRequirements = 26,
    InvalidBufferUsage = 27,
    ResourceNotExportable = 28,
    UnsupportedExternalHandleType = 29,
    ExportFailed = 30,
    InvalidSyncId = 31,
    UnknownSync = 32,
    UnsupportedSyncHandleType = 33,
    SyncExportFailed = 34,
    InvalidImageDimensions = 35,
    UnsupportedPixelFormat = 36,
    UnsupportedImageUsage = 37,
    UnsupportedImageExternalSharing = 38,
    UnsupportedVideoCodec = 39,
    UnsupportedVideoProfile = 40,
    InvalidVideoBitDepth = 41,
    UnsupportedChromaSubsampling = 42,
    UnsupportedScanMode = 43,
    UnsupportedFieldOrder = 44,
    UnsupportedVideoSurfaceFormat = 45,
    InvalidVideoSurfaceDimensions = 46,
    InvalidVideoVisibleRegion = 47,
    VideoDecodeCapabilityCountTooLarge = 48,
    VideoOutputFormatCountTooLarge = 49,
}

impl ProtocolErrorCode {
    pub const fn wire_value(self) -> u32 {
        self as u32
    }
}

impl TryFrom<u32> for ProtocolErrorCode {
    type Error = ProtocolError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::InvalidMagic),
            2 => Ok(Self::UnknownMessageKind),
            3 => Ok(Self::UnknownOpcode),
            4 => Ok(Self::PayloadTooLarge),
            5 => Ok(Self::TruncatedHeader),
            6 => Ok(Self::TruncatedPayload),
            7 => Ok(Self::MalformedPayload),
            8 => Ok(Self::InvalidVersionRange),
            9 => Ok(Self::UnsupportedVersion),
            10 => Ok(Self::InvalidSessionId),
            11 => Ok(Self::SessionIdsExhausted),
            12 => Ok(Self::InvalidFlags),
            13 => Ok(Self::InvalidDeviceId),
            14 => Ok(Self::DeviceCountTooLarge),
            15 => Ok(Self::DeviceNameTooLong),
            16 => Ok(Self::InvalidUtf8),
            17 => Ok(Self::SessionRequired),
            18 => Ok(Self::DiscoveryFailed),
            19 => Ok(Self::UnknownDeviceId),
            20 => Ok(Self::MemoryHeapCountTooLarge),
            21 => Ok(Self::MemoryTypeCountTooLarge),
            22 => Ok(Self::InvalidResourceId),
            23 => Ok(Self::UnknownResource),
            24 => Ok(Self::InvalidBufferSize),
            25 => Ok(Self::AllocationFailed),
            26 => Ok(Self::UnsupportedMemoryRequirements),
            27 => Ok(Self::InvalidBufferUsage),
            28 => Ok(Self::ResourceNotExportable),
            29 => Ok(Self::UnsupportedExternalHandleType),
            30 => Ok(Self::ExportFailed),
            31 => Ok(Self::InvalidSyncId),
            32 => Ok(Self::UnknownSync),
            33 => Ok(Self::UnsupportedSyncHandleType),
            34 => Ok(Self::SyncExportFailed),
            35 => Ok(Self::InvalidImageDimensions),
            36 => Ok(Self::UnsupportedPixelFormat),
            37 => Ok(Self::UnsupportedImageUsage),
            38 => Ok(Self::UnsupportedImageExternalSharing),
            39 => Ok(Self::UnsupportedVideoCodec),
            40 => Ok(Self::UnsupportedVideoProfile),
            41 => Ok(Self::InvalidVideoBitDepth),
            42 => Ok(Self::UnsupportedChromaSubsampling),
            43 => Ok(Self::UnsupportedScanMode),
            44 => Ok(Self::UnsupportedFieldOrder),
            45 => Ok(Self::UnsupportedVideoSurfaceFormat),
            46 => Ok(Self::InvalidVideoSurfaceDimensions),
            47 => Ok(Self::InvalidVideoVisibleRegion),
            48 => Ok(Self::VideoDecodeCapabilityCountTooLarge),
            49 => Ok(Self::VideoOutputFormatCountTooLarge),
            _ => Err(ProtocolError::MalformedPayload),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidMagic {
        actual: u32,
    },
    InvalidSessionId,
    InvalidDeviceId,
    UnknownDeviceId,
    SessionIdsExhausted,
    UnknownMessageKind {
        kind: u8,
    },
    UnknownOpcode {
        kind: MessageKind,
        opcode: u8,
    },
    PayloadTooLarge {
        len: u32,
        max: u32,
    },
    TruncatedHeader {
        actual: usize,
        expected: usize,
    },
    TruncatedPayload {
        actual: usize,
        expected: usize,
    },
    TrailingPayload {
        len: usize,
    },
    InvalidFlags {
        flags: u16,
    },
    DeviceCountTooLarge {
        count: usize,
        max: usize,
    },
    DeviceNameTooLong {
        len: usize,
        max: usize,
    },
    InvalidUtf8,
    SessionRequired,
    DiscoveryFailed,
    MemoryHeapCountTooLarge {
        count: usize,
        max: usize,
    },
    MemoryTypeCountTooLarge {
        count: usize,
        max: usize,
    },
    InvalidResourceId,
    UnknownResource,
    InvalidBufferSize {
        size: u64,
        max: u64,
    },
    AllocationFailed,
    UnsupportedMemoryRequirements,
    InvalidBufferUsage {
        flags: u32,
    },
    ResourceNotExportable,
    UnsupportedExternalHandleType,
    ExportFailed,
    InvalidSyncId,
    UnknownSync,
    UnsupportedSyncHandleType,
    SyncExportFailed,
    InvalidImageDimensions {
        width: u32,
        height: u32,
    },
    UnsupportedPixelFormat,
    UnsupportedImageUsage {
        flags: u32,
    },
    UnsupportedImageExternalSharing,
    UnsupportedVideoCodec,
    UnsupportedVideoProfile,
    InvalidVideoBitDepth {
        bit_depth: u8,
    },
    UnsupportedChromaSubsampling,
    UnsupportedScanMode,
    UnsupportedFieldOrder,
    UnsupportedVideoSurfaceFormat,
    InvalidVideoSurfaceDimensions {
        width: u32,
        height: u32,
    },
    InvalidVideoVisibleRegion,
    VideoDecodeCapabilityCountTooLarge {
        count: usize,
        max: usize,
    },
    VideoOutputFormatCountTooLarge {
        count: usize,
        max: usize,
    },
    MalformedPayload,
    InvalidVersionRange {
        min: ProtocolVersion,
        max: ProtocolVersion,
    },
    UnsupportedVersion {
        min: ProtocolVersion,
        max: ProtocolVersion,
        supported: ProtocolVersion,
    },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic { actual } => write!(f, "invalid protocol magic: {actual:#010x}"),
            Self::InvalidSessionId => write!(f, "session id must be non-zero"),
            Self::InvalidDeviceId => write!(f, "device id must be non-zero"),
            Self::UnknownDeviceId => write!(f, "unknown device id"),
            Self::SessionIdsExhausted => write!(f, "session ids are exhausted"),
            Self::UnknownMessageKind { kind } => write!(f, "unknown message kind: {kind}"),
            Self::UnknownOpcode { kind, opcode } => {
                write!(f, "unknown opcode {opcode} for message kind {kind:?}")
            }
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload length {len} exceeds maximum {max}")
            }
            Self::TruncatedHeader { actual, expected } => {
                write!(
                    f,
                    "truncated header: got {actual} bytes, expected {expected}"
                )
            }
            Self::TruncatedPayload { actual, expected } => {
                write!(
                    f,
                    "truncated payload: got {actual} bytes, expected {expected}"
                )
            }
            Self::TrailingPayload { len } => {
                write!(f, "payload has {len} trailing bytes")
            }
            Self::InvalidFlags { flags } => write!(f, "invalid flags: {flags:#06x}"),
            Self::DeviceCountTooLarge { count, max } => {
                write!(f, "device count {count} exceeds maximum {max}")
            }
            Self::DeviceNameTooLong { len, max } => {
                write!(f, "device name length {len} exceeds maximum {max}")
            }
            Self::InvalidUtf8 => write!(f, "invalid UTF-8 device name"),
            Self::SessionRequired => write!(f, "session required"),
            Self::DiscoveryFailed => write!(f, "device discovery failed"),
            Self::MemoryHeapCountTooLarge { count, max } => {
                write!(f, "memory heap count {count} exceeds maximum {max}")
            }
            Self::MemoryTypeCountTooLarge { count, max } => {
                write!(f, "memory type count {count} exceeds maximum {max}")
            }
            Self::InvalidResourceId => write!(f, "resource id must be non-zero"),
            Self::UnknownResource => write!(f, "unknown resource id"),
            Self::InvalidBufferSize { size, max } => {
                write!(f, "invalid buffer size {size}; allowed range is 1..={max}")
            }
            Self::AllocationFailed => write!(f, "resource allocation failed"),
            Self::UnsupportedMemoryRequirements => {
                write!(f, "unsupported buffer memory requirements")
            }
            Self::InvalidBufferUsage { flags } => {
                write!(f, "invalid buffer usage flags: {flags:#010x}")
            }
            Self::ResourceNotExportable => write!(f, "resource is not exportable"),
            Self::UnsupportedExternalHandleType => {
                write!(f, "unsupported external handle type")
            }
            Self::ExportFailed => write!(f, "resource export failed"),
            Self::InvalidSyncId => write!(f, "sync id must be non-zero"),
            Self::UnknownSync => write!(f, "unknown sync id"),
            Self::UnsupportedSyncHandleType => write!(f, "unsupported sync handle type"),
            Self::SyncExportFailed => write!(f, "sync export failed"),
            Self::InvalidImageDimensions { width, height } => {
                write!(f, "invalid image dimensions {width}x{height}")
            }
            Self::UnsupportedPixelFormat => write!(f, "unsupported pixel format"),
            Self::UnsupportedImageUsage { flags } => {
                write!(f, "unsupported image usage flags: {flags:#010x}")
            }
            Self::UnsupportedImageExternalSharing => {
                write!(f, "unsupported image external sharing")
            }
            Self::UnsupportedVideoCodec => write!(f, "unsupported video codec"),
            Self::UnsupportedVideoProfile => write!(f, "unsupported video profile"),
            Self::InvalidVideoBitDepth { bit_depth } => {
                write!(f, "invalid video bit depth {bit_depth}")
            }
            Self::UnsupportedChromaSubsampling => {
                write!(f, "unsupported chroma subsampling")
            }
            Self::UnsupportedScanMode => write!(f, "unsupported scan mode"),
            Self::UnsupportedFieldOrder => write!(f, "unsupported field order"),
            Self::UnsupportedVideoSurfaceFormat => {
                write!(f, "unsupported video surface format")
            }
            Self::InvalidVideoSurfaceDimensions { width, height } => {
                write!(f, "invalid video surface dimensions {width}x{height}")
            }
            Self::InvalidVideoVisibleRegion => write!(f, "invalid video visible region"),
            Self::VideoDecodeCapabilityCountTooLarge { count, max } => {
                write!(
                    f,
                    "video decode capability count {count} exceeds maximum {max}"
                )
            }
            Self::VideoOutputFormatCountTooLarge { count, max } => {
                write!(f, "video output format count {count} exceeds maximum {max}")
            }
            Self::MalformedPayload => write!(f, "malformed payload"),
            Self::InvalidVersionRange { min, max } => {
                write!(f, "invalid protocol version range: {min}..={max}")
            }
            Self::UnsupportedVersion {
                min,
                max,
                supported,
            } => write!(
                f,
                "unsupported protocol version range {min}..={max}; supported version is {supported}"
            ),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<&ProtocolError> for ProtocolErrorCode {
    fn from(value: &ProtocolError) -> Self {
        match value {
            ProtocolError::InvalidMagic { .. } => Self::InvalidMagic,
            ProtocolError::InvalidSessionId => Self::InvalidSessionId,
            ProtocolError::InvalidDeviceId => Self::InvalidDeviceId,
            ProtocolError::UnknownDeviceId => Self::UnknownDeviceId,
            ProtocolError::SessionIdsExhausted => Self::SessionIdsExhausted,
            ProtocolError::UnknownMessageKind { .. } => Self::UnknownMessageKind,
            ProtocolError::UnknownOpcode { .. } => Self::UnknownOpcode,
            ProtocolError::PayloadTooLarge { .. } => Self::PayloadTooLarge,
            ProtocolError::TruncatedHeader { .. } => Self::TruncatedHeader,
            ProtocolError::TruncatedPayload { .. } => Self::TruncatedPayload,
            ProtocolError::TrailingPayload { .. } | ProtocolError::MalformedPayload => {
                Self::MalformedPayload
            }
            ProtocolError::InvalidFlags { .. } => Self::InvalidFlags,
            ProtocolError::DeviceCountTooLarge { .. } => Self::DeviceCountTooLarge,
            ProtocolError::DeviceNameTooLong { .. } => Self::DeviceNameTooLong,
            ProtocolError::InvalidUtf8 => Self::InvalidUtf8,
            ProtocolError::SessionRequired => Self::SessionRequired,
            ProtocolError::DiscoveryFailed => Self::DiscoveryFailed,
            ProtocolError::MemoryHeapCountTooLarge { .. } => Self::MemoryHeapCountTooLarge,
            ProtocolError::MemoryTypeCountTooLarge { .. } => Self::MemoryTypeCountTooLarge,
            ProtocolError::InvalidResourceId => Self::InvalidResourceId,
            ProtocolError::UnknownResource => Self::UnknownResource,
            ProtocolError::InvalidBufferSize { .. } => Self::InvalidBufferSize,
            ProtocolError::AllocationFailed => Self::AllocationFailed,
            ProtocolError::UnsupportedMemoryRequirements => Self::UnsupportedMemoryRequirements,
            ProtocolError::InvalidBufferUsage { .. } => Self::InvalidBufferUsage,
            ProtocolError::ResourceNotExportable => Self::ResourceNotExportable,
            ProtocolError::UnsupportedExternalHandleType => Self::UnsupportedExternalHandleType,
            ProtocolError::ExportFailed => Self::ExportFailed,
            ProtocolError::InvalidSyncId => Self::InvalidSyncId,
            ProtocolError::UnknownSync => Self::UnknownSync,
            ProtocolError::UnsupportedSyncHandleType => Self::UnsupportedSyncHandleType,
            ProtocolError::SyncExportFailed => Self::SyncExportFailed,
            ProtocolError::InvalidImageDimensions { .. } => Self::InvalidImageDimensions,
            ProtocolError::UnsupportedPixelFormat => Self::UnsupportedPixelFormat,
            ProtocolError::UnsupportedImageUsage { .. } => Self::UnsupportedImageUsage,
            ProtocolError::UnsupportedImageExternalSharing => Self::UnsupportedImageExternalSharing,
            ProtocolError::UnsupportedVideoCodec => Self::UnsupportedVideoCodec,
            ProtocolError::UnsupportedVideoProfile => Self::UnsupportedVideoProfile,
            ProtocolError::InvalidVideoBitDepth { .. } => Self::InvalidVideoBitDepth,
            ProtocolError::UnsupportedChromaSubsampling => Self::UnsupportedChromaSubsampling,
            ProtocolError::UnsupportedScanMode => Self::UnsupportedScanMode,
            ProtocolError::UnsupportedFieldOrder => Self::UnsupportedFieldOrder,
            ProtocolError::UnsupportedVideoSurfaceFormat => Self::UnsupportedVideoSurfaceFormat,
            ProtocolError::InvalidVideoSurfaceDimensions { .. } => {
                Self::InvalidVideoSurfaceDimensions
            }
            ProtocolError::InvalidVideoVisibleRegion => Self::InvalidVideoVisibleRegion,
            ProtocolError::VideoDecodeCapabilityCountTooLarge { .. } => {
                Self::VideoDecodeCapabilityCountTooLarge
            }
            ProtocolError::VideoOutputFormatCountTooLarge { .. } => {
                Self::VideoOutputFormatCountTooLarge
            }
            ProtocolError::InvalidVersionRange { .. } => Self::InvalidVersionRange,
            ProtocolError::UnsupportedVersion { .. } => Self::UnsupportedVersion,
        }
    }
}

pub fn validate_header(header: &MessageHeader) -> Result<(), ProtocolError> {
    if header.magic == PROTOCOL_MAGIC {
        Ok(())
    } else {
        Err(ProtocolError::InvalidMagic {
            actual: header.magic,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestOpcode {
    Hello,
    EnumerateDevices,
    QueryDeviceCapabilities,
    CreateBuffer,
    DestroyResource,
    ExportResource,
    CreateSync,
    ExportSync,
    CreateImage,
    QueryVideoCapabilities,
}

impl RequestOpcode {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Hello => 1,
            Self::EnumerateDevices => 2,
            Self::QueryDeviceCapabilities => 3,
            Self::CreateBuffer => 4,
            Self::DestroyResource => 5,
            Self::ExportResource => 6,
            Self::CreateSync => 7,
            Self::ExportSync => 8,
            Self::CreateImage => 9,
            Self::QueryVideoCapabilities => 10,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseOpcode {
    Welcome,
    Error,
    DeviceList,
    DeviceCapabilities,
    BufferCreated,
    ResourceDestroyed,
    ResourceExported,
    SyncCreated,
    SyncExported,
    ImageCreated,
    VideoCapabilities,
}

impl ResponseOpcode {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Welcome => 1,
            Self::Error => 2,
            Self::DeviceList => 3,
            Self::DeviceCapabilities => 4,
            Self::BufferCreated => 5,
            Self::ResourceDestroyed => 6,
            Self::ResourceExported => 7,
            Self::SyncCreated => 8,
            Self::SyncExported => 9,
            Self::ImageCreated => 10,
            Self::VideoCapabilities => 11,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireHeader {
    pub version: ProtocolVersion,
    pub kind: MessageKind,
    pub opcode: u8,
    pub flags: u16,
    pub payload_len: u32,
    pub request_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WireMessage {
    Hello {
        request_id: u64,
        request: HelloRequest,
    },
    EnumerateDevices {
        request_id: u64,
    },
    QueryDeviceCapabilities {
        request_id: u64,
        request: QueryDeviceCapabilitiesRequest,
    },
    QueryVideoCapabilities {
        request_id: u64,
        request: QueryVideoCapabilitiesRequest,
    },
    CreateBuffer {
        request_id: u64,
        request: CreateBufferRequest,
    },
    CreateImage {
        request_id: u64,
        request: CreateImageRequest,
    },
    DestroyResource {
        request_id: u64,
        request: DestroyResourceRequest,
    },
    ExportResource {
        request_id: u64,
        request: ExportResourceRequest,
    },
    CreateSync {
        request_id: u64,
        request: CreateSyncRequest,
    },
    ExportSync {
        request_id: u64,
        request: ExportSyncRequest,
    },
    Welcome {
        request_id: u64,
        response: WelcomeResponse,
    },
    Error {
        request_id: u64,
        response: ErrorResponse,
    },
    DeviceList {
        request_id: u64,
        response: DeviceListResponse,
    },
    DeviceCapabilities {
        request_id: u64,
        response: DeviceCapabilitiesResponse,
    },
    VideoCapabilities {
        request_id: u64,
        response: VideoCapabilitiesResponse,
    },
    BufferCreated {
        request_id: u64,
        response: BufferCreatedResponse,
    },
    ImageCreated {
        request_id: u64,
        response: ImageCreatedResponse,
    },
    ResourceDestroyed {
        request_id: u64,
        response: ResourceDestroyedResponse,
    },
    ResourceExported {
        request_id: u64,
        response: ResourceExportedResponse,
    },
    SyncCreated {
        request_id: u64,
        response: SyncCreatedResponse,
    },
    SyncExported {
        request_id: u64,
        response: SyncExportedResponse,
    },
}

impl WireMessage {
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Hello { request_id, .. }
            | Self::EnumerateDevices { request_id }
            | Self::QueryDeviceCapabilities { request_id, .. }
            | Self::QueryVideoCapabilities { request_id, .. }
            | Self::CreateBuffer { request_id, .. }
            | Self::CreateImage { request_id, .. }
            | Self::DestroyResource { request_id, .. }
            | Self::ExportResource { request_id, .. }
            | Self::CreateSync { request_id, .. }
            | Self::ExportSync { request_id, .. }
            | Self::Welcome { request_id, .. }
            | Self::Error { request_id, .. }
            | Self::DeviceList { request_id, .. }
            | Self::DeviceCapabilities { request_id, .. }
            | Self::VideoCapabilities { request_id, .. }
            | Self::BufferCreated { request_id, .. }
            | Self::ImageCreated { request_id, .. }
            | Self::ResourceDestroyed { request_id, .. }
            | Self::ResourceExported { request_id, .. }
            | Self::SyncCreated { request_id, .. }
            | Self::SyncExported { request_id, .. } => *request_id,
        }
    }
}

pub fn encode_wire_message(message: &WireMessage) -> Vec<u8> {
    let (kind, opcode, request_id, payload) = match message {
        WireMessage::Hello {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::Hello.wire_value(),
            *request_id,
            encode_hello_payload(request),
        ),
        WireMessage::EnumerateDevices { request_id } => (
            MessageKind::Request,
            RequestOpcode::EnumerateDevices.wire_value(),
            *request_id,
            Vec::new(),
        ),
        WireMessage::QueryDeviceCapabilities {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::QueryDeviceCapabilities.wire_value(),
            *request_id,
            encode_query_device_capabilities_payload(request),
        ),
        WireMessage::QueryVideoCapabilities {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::QueryVideoCapabilities.wire_value(),
            *request_id,
            encode_query_video_capabilities_payload(request),
        ),
        WireMessage::CreateBuffer {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::CreateBuffer.wire_value(),
            *request_id,
            encode_create_buffer_payload(request),
        ),
        WireMessage::CreateImage {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::CreateImage.wire_value(),
            *request_id,
            encode_create_image_payload(request),
        ),
        WireMessage::DestroyResource {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::DestroyResource.wire_value(),
            *request_id,
            encode_destroy_resource_payload(request),
        ),
        WireMessage::ExportResource {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::ExportResource.wire_value(),
            *request_id,
            encode_export_resource_payload(request),
        ),
        WireMessage::CreateSync {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::CreateSync.wire_value(),
            *request_id,
            encode_create_sync_payload(request),
        ),
        WireMessage::ExportSync {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::ExportSync.wire_value(),
            *request_id,
            encode_export_sync_payload(request),
        ),
        WireMessage::Welcome {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::Welcome.wire_value(),
            *request_id,
            encode_welcome_payload(response),
        ),
        WireMessage::Error {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::Error.wire_value(),
            *request_id,
            encode_error_payload(response),
        ),
        WireMessage::DeviceList {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::DeviceList.wire_value(),
            *request_id,
            encode_device_list_payload(response),
        ),
        WireMessage::DeviceCapabilities {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::DeviceCapabilities.wire_value(),
            *request_id,
            encode_device_capabilities_payload(response),
        ),
        WireMessage::VideoCapabilities {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::VideoCapabilities.wire_value(),
            *request_id,
            encode_video_capabilities_payload(response),
        ),
        WireMessage::BufferCreated {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::BufferCreated.wire_value(),
            *request_id,
            encode_buffer_created_payload(response),
        ),
        WireMessage::ImageCreated {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::ImageCreated.wire_value(),
            *request_id,
            encode_image_created_payload(response),
        ),
        WireMessage::ResourceDestroyed {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::ResourceDestroyed.wire_value(),
            *request_id,
            encode_resource_destroyed_payload(response),
        ),
        WireMessage::ResourceExported {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::ResourceExported.wire_value(),
            *request_id,
            encode_resource_exported_payload(response),
        ),
        WireMessage::SyncCreated {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::SyncCreated.wire_value(),
            *request_id,
            encode_sync_created_payload(response),
        ),
        WireMessage::SyncExported {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::SyncExported.wire_value(),
            *request_id,
            encode_sync_exported_payload(response),
        ),
    };

    let header = WireHeader {
        version: CURRENT_PROTOCOL_VERSION,
        kind,
        opcode,
        flags: 0,
        payload_len: payload.len() as u32,
        request_id,
    };

    let mut bytes = Vec::with_capacity(WIRE_HEADER_LEN + payload.len());
    encode_wire_header(&header, &mut bytes);
    bytes.extend_from_slice(&payload);
    bytes
}

pub fn decode_wire_message(bytes: &[u8]) -> Result<WireMessage, ProtocolError> {
    if bytes.len() < WIRE_HEADER_LEN {
        return Err(ProtocolError::TruncatedHeader {
            actual: bytes.len(),
            expected: WIRE_HEADER_LEN,
        });
    }

    let header = decode_wire_header(&bytes[..WIRE_HEADER_LEN])?;
    let payload_len = header.payload_len as usize;
    let actual_payload_len = bytes.len() - WIRE_HEADER_LEN;
    if actual_payload_len < payload_len {
        return Err(ProtocolError::TruncatedPayload {
            actual: actual_payload_len,
            expected: payload_len,
        });
    }
    if actual_payload_len > payload_len {
        return Err(ProtocolError::TrailingPayload {
            len: actual_payload_len - payload_len,
        });
    }

    decode_wire_message_parts(header, &bytes[WIRE_HEADER_LEN..])
}

pub fn decode_wire_header(bytes: &[u8]) -> Result<WireHeader, ProtocolError> {
    if bytes.len() < WIRE_HEADER_LEN {
        return Err(ProtocolError::TruncatedHeader {
            actual: bytes.len(),
            expected: WIRE_HEADER_LEN,
        });
    }
    if bytes.len() > WIRE_HEADER_LEN {
        return Err(ProtocolError::TrailingPayload {
            len: bytes.len() - WIRE_HEADER_LEN,
        });
    }

    let magic = read_u32(bytes, 0);
    if magic != PROTOCOL_MAGIC {
        return Err(ProtocolError::InvalidMagic { actual: magic });
    }

    let version = ProtocolVersion::new(read_u16(bytes, 4), read_u16(bytes, 6));
    let kind = MessageKind::try_from(bytes[8])?;
    let opcode = bytes[9];
    validate_opcode(kind, opcode)?;
    let flags = read_u16(bytes, 10);
    if flags != 0 {
        return Err(ProtocolError::InvalidFlags { flags });
    }
    let payload_len = read_u32(bytes, 12);
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(ProtocolError::PayloadTooLarge {
            len: payload_len,
            max: MAX_PAYLOAD_LEN,
        });
    }

    Ok(WireHeader {
        version,
        kind,
        opcode,
        flags,
        payload_len,
        request_id: read_u64(bytes, 16),
    })
}

pub fn decode_wire_message_parts(
    header: WireHeader,
    payload: &[u8],
) -> Result<WireMessage, ProtocolError> {
    if payload.len() != header.payload_len as usize {
        return Err(ProtocolError::TruncatedPayload {
            actual: payload.len(),
            expected: header.payload_len as usize,
        });
    }

    negotiate_protocol_version(header.version, header.version)?;

    match (header.kind, header.opcode) {
        (MessageKind::Request, 1) => Ok(WireMessage::Hello {
            request_id: header.request_id,
            request: decode_hello_payload(payload)?,
        }),
        (MessageKind::Request, 2) => {
            if payload.is_empty() {
                Ok(WireMessage::EnumerateDevices {
                    request_id: header.request_id,
                })
            } else {
                Err(ProtocolError::TrailingPayload { len: payload.len() })
            }
        }
        (MessageKind::Request, 3) => Ok(WireMessage::QueryDeviceCapabilities {
            request_id: header.request_id,
            request: decode_query_device_capabilities_payload(payload)?,
        }),
        (MessageKind::Request, 4) => Ok(WireMessage::CreateBuffer {
            request_id: header.request_id,
            request: decode_create_buffer_payload(payload)?,
        }),
        (MessageKind::Request, 5) => Ok(WireMessage::DestroyResource {
            request_id: header.request_id,
            request: decode_destroy_resource_payload(payload)?,
        }),
        (MessageKind::Request, 6) => Ok(WireMessage::ExportResource {
            request_id: header.request_id,
            request: decode_export_resource_payload(payload)?,
        }),
        (MessageKind::Request, 7) => Ok(WireMessage::CreateSync {
            request_id: header.request_id,
            request: decode_create_sync_payload(payload)?,
        }),
        (MessageKind::Request, 8) => Ok(WireMessage::ExportSync {
            request_id: header.request_id,
            request: decode_export_sync_payload(payload)?,
        }),
        (MessageKind::Request, 9) => Ok(WireMessage::CreateImage {
            request_id: header.request_id,
            request: decode_create_image_payload(payload)?,
        }),
        (MessageKind::Request, 10) => Ok(WireMessage::QueryVideoCapabilities {
            request_id: header.request_id,
            request: decode_query_video_capabilities_payload(payload)?,
        }),
        (MessageKind::Response, 1) => Ok(WireMessage::Welcome {
            request_id: header.request_id,
            response: decode_welcome_payload(payload)?,
        }),
        (MessageKind::Response, 2) => Ok(WireMessage::Error {
            request_id: header.request_id,
            response: decode_error_payload(payload)?,
        }),
        (MessageKind::Response, 3) => Ok(WireMessage::DeviceList {
            request_id: header.request_id,
            response: decode_device_list_payload(payload)?,
        }),
        (MessageKind::Response, 4) => Ok(WireMessage::DeviceCapabilities {
            request_id: header.request_id,
            response: decode_device_capabilities_payload(payload)?,
        }),
        (MessageKind::Response, 5) => Ok(WireMessage::BufferCreated {
            request_id: header.request_id,
            response: decode_buffer_created_payload(payload)?,
        }),
        (MessageKind::Response, 6) => Ok(WireMessage::ResourceDestroyed {
            request_id: header.request_id,
            response: decode_resource_destroyed_payload(payload)?,
        }),
        (MessageKind::Response, 7) => Ok(WireMessage::ResourceExported {
            request_id: header.request_id,
            response: decode_resource_exported_payload(payload)?,
        }),
        (MessageKind::Response, 8) => Ok(WireMessage::SyncCreated {
            request_id: header.request_id,
            response: decode_sync_created_payload(payload)?,
        }),
        (MessageKind::Response, 9) => Ok(WireMessage::SyncExported {
            request_id: header.request_id,
            response: decode_sync_exported_payload(payload)?,
        }),
        (MessageKind::Response, 10) => Ok(WireMessage::ImageCreated {
            request_id: header.request_id,
            response: decode_image_created_payload(payload)?,
        }),
        (MessageKind::Response, 11) => Ok(WireMessage::VideoCapabilities {
            request_id: header.request_id,
            response: decode_video_capabilities_payload(payload)?,
        }),
        _ => Err(ProtocolError::UnknownOpcode {
            kind: header.kind,
            opcode: header.opcode,
        }),
    }
}

pub fn negotiate_protocol_version(
    min: ProtocolVersion,
    max: ProtocolVersion,
) -> Result<ProtocolVersion, ProtocolError> {
    if min > max {
        return Err(ProtocolError::InvalidVersionRange { min, max });
    }

    // Major protocol versions are incompatible. Minor versions are
    // backward-compatible only within the same major version. With one server
    // version supported today, selecting the highest supported version means
    // selecting CURRENT_PROTOCOL_VERSION when it is inside a same-major client
    // range.
    let supported = CURRENT_PROTOCOL_VERSION;
    if min.major == supported.major
        && max.major == supported.major
        && min.minor <= supported.minor
        && max.minor >= supported.minor
    {
        Ok(supported)
    } else {
        Err(ProtocolError::UnsupportedVersion {
            min,
            max,
            supported,
        })
    }
}

pub fn handle_hello(
    request: &HelloRequest,
    session_id: SessionId,
) -> Result<WelcomeResponse, ProtocolError> {
    let version = negotiate_protocol_version(request.min_version, request.max_version)?;
    Ok(WelcomeResponse {
        version,
        session_id,
    })
}

fn encode_wire_header(header: &WireHeader, bytes: &mut Vec<u8>) {
    bytes.extend_from_slice(&PROTOCOL_MAGIC.to_le_bytes());
    bytes.extend_from_slice(&header.version.major.to_le_bytes());
    bytes.extend_from_slice(&header.version.minor.to_le_bytes());
    bytes.push(header.kind.wire_value());
    bytes.push(header.opcode);
    bytes.extend_from_slice(&header.flags.to_le_bytes());
    bytes.extend_from_slice(&header.payload_len.to_le_bytes());
    bytes.extend_from_slice(&header.request_id.to_le_bytes());
}

fn validate_opcode(kind: MessageKind, opcode: u8) -> Result<(), ProtocolError> {
    match (kind, opcode) {
        (MessageKind::Request, 1..=10) | (MessageKind::Response, 1..=11) => Ok(()),
        _ => Err(ProtocolError::UnknownOpcode { kind, opcode }),
    }
}

fn encode_hello_payload(request: &HelloRequest) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&request.min_version.major.to_le_bytes());
    bytes.extend_from_slice(&request.min_version.minor.to_le_bytes());
    bytes.extend_from_slice(&request.max_version.major.to_le_bytes());
    bytes.extend_from_slice(&request.max_version.minor.to_le_bytes());
    bytes
}

fn decode_hello_payload(bytes: &[u8]) -> Result<HelloRequest, ProtocolError> {
    if bytes.len() != 8 {
        return Err(ProtocolError::MalformedPayload);
    }

    let request = HelloRequest {
        min_version: ProtocolVersion::new(read_u16(bytes, 0), read_u16(bytes, 2)),
        max_version: ProtocolVersion::new(read_u16(bytes, 4), read_u16(bytes, 6)),
    };

    if request.min_version > request.max_version {
        return Err(ProtocolError::InvalidVersionRange {
            min: request.min_version,
            max: request.max_version,
        });
    }

    Ok(request)
}

fn encode_welcome_payload(response: &WelcomeResponse) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12);
    bytes.extend_from_slice(&response.version.major.to_le_bytes());
    bytes.extend_from_slice(&response.version.minor.to_le_bytes());
    bytes.extend_from_slice(&response.session_id.get().to_le_bytes());
    bytes
}

fn decode_welcome_payload(bytes: &[u8]) -> Result<WelcomeResponse, ProtocolError> {
    if bytes.len() != 12 {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(WelcomeResponse {
        version: ProtocolVersion::new(read_u16(bytes, 0), read_u16(bytes, 2)),
        session_id: SessionId::new(read_u64(bytes, 4))?,
    })
}

fn encode_error_payload(response: &ErrorResponse) -> Vec<u8> {
    response.code.wire_value().to_le_bytes().to_vec()
}

fn decode_error_payload(bytes: &[u8]) -> Result<ErrorResponse, ProtocolError> {
    if bytes.len() != 4 {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(ErrorResponse {
        code: ProtocolErrorCode::try_from(read_u32(bytes, 0))?,
    })
}

fn encode_query_device_capabilities_payload(request: &QueryDeviceCapabilitiesRequest) -> Vec<u8> {
    request.device_id.get().to_le_bytes().to_vec()
}

fn decode_query_device_capabilities_payload(
    bytes: &[u8],
) -> Result<QueryDeviceCapabilitiesRequest, ProtocolError> {
    if bytes.len() != 8 {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(QueryDeviceCapabilitiesRequest {
        device_id: DeviceId::new(read_u64(bytes, 0))?,
    })
}

fn encode_query_video_capabilities_payload(request: &QueryVideoCapabilitiesRequest) -> Vec<u8> {
    request.device_id.get().to_le_bytes().to_vec()
}

fn decode_query_video_capabilities_payload(
    bytes: &[u8],
) -> Result<QueryVideoCapabilitiesRequest, ProtocolError> {
    if bytes.len() != QUERY_VIDEO_CAPABILITIES_PAYLOAD_LEN {
        return if bytes.len() < QUERY_VIDEO_CAPABILITIES_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: QUERY_VIDEO_CAPABILITIES_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - QUERY_VIDEO_CAPABILITIES_PAYLOAD_LEN,
            })
        };
    }

    Ok(QueryVideoCapabilitiesRequest {
        device_id: DeviceId::new(read_u64(bytes, 0))?,
    })
}

fn encode_create_buffer_payload(request: &CreateBufferRequest) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CREATE_BUFFER_PAYLOAD_LEN);
    bytes.extend_from_slice(&request.desc.device_id.get().to_le_bytes());
    bytes.extend_from_slice(&request.desc.size_bytes.to_le_bytes());
    bytes.extend_from_slice(&request.desc.usage.bits().to_le_bytes());
    bytes.push(encode_memory_preference(request.desc.memory_preference));
    bytes.push(request.desc.external_sharing.wire_value());
    bytes.extend_from_slice(&[0_u8; 2]);
    bytes
}

fn decode_create_buffer_payload(bytes: &[u8]) -> Result<CreateBufferRequest, ProtocolError> {
    if bytes.len() != CREATE_BUFFER_PAYLOAD_LEN {
        return if bytes.len() < CREATE_BUFFER_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: CREATE_BUFFER_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - CREATE_BUFFER_PAYLOAD_LEN,
            })
        };
    }
    if bytes[22..24].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }

    let desc = BufferDesc {
        device_id: DeviceId::new(read_u64(bytes, 0))?,
        size_bytes: read_u64(bytes, 8),
        usage: BufferUsageFlags::new(read_u32(bytes, 16))?,
        memory_preference: decode_memory_preference(bytes[20])?,
        external_sharing: ExternalSharing::try_from(bytes[21])?,
    };
    desc.validate()?;

    Ok(CreateBufferRequest { desc })
}

fn encode_create_image_payload(request: &CreateImageRequest) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CREATE_IMAGE_PAYLOAD_LEN);
    bytes.extend_from_slice(&request.desc.device_id.get().to_le_bytes());
    bytes.extend_from_slice(&request.desc.width.to_le_bytes());
    bytes.extend_from_slice(&request.desc.height.to_le_bytes());
    bytes.push(request.desc.format.wire_value());
    bytes.push(request.desc.external_sharing.wire_value());
    bytes.extend_from_slice(&[0_u8; 2]);
    bytes.extend_from_slice(&request.desc.usage.bits().to_le_bytes());
    bytes
}

fn decode_create_image_payload(bytes: &[u8]) -> Result<CreateImageRequest, ProtocolError> {
    if bytes.len() != CREATE_IMAGE_PAYLOAD_LEN {
        return if bytes.len() < CREATE_IMAGE_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: CREATE_IMAGE_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - CREATE_IMAGE_PAYLOAD_LEN,
            })
        };
    }
    if bytes[18..20].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }

    let desc = ImageDesc {
        device_id: DeviceId::new(read_u64(bytes, 0))?,
        width: read_u32(bytes, 8),
        height: read_u32(bytes, 12),
        format: PixelFormat::try_from(bytes[16])?,
        external_sharing: ExternalSharing::try_from(bytes[17])?,
        usage: ImageUsageFlags::new(read_u32(bytes, 20))?,
    };
    desc.validate()?;

    Ok(CreateImageRequest { desc })
}

fn encode_destroy_resource_payload(request: &DestroyResourceRequest) -> Vec<u8> {
    request.resource_id.get().to_le_bytes().to_vec()
}

fn decode_destroy_resource_payload(bytes: &[u8]) -> Result<DestroyResourceRequest, ProtocolError> {
    if bytes.len() != DESTROY_RESOURCE_PAYLOAD_LEN {
        return if bytes.len() < DESTROY_RESOURCE_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: DESTROY_RESOURCE_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - DESTROY_RESOURCE_PAYLOAD_LEN,
            })
        };
    }

    Ok(DestroyResourceRequest {
        resource_id: ResourceId::new(read_u64(bytes, 0))?,
    })
}

fn encode_export_resource_payload(request: &ExportResourceRequest) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(EXPORT_RESOURCE_PAYLOAD_LEN);
    bytes.extend_from_slice(&request.resource_id.get().to_le_bytes());
    bytes.push(request.handle_type.wire_value());
    bytes.extend_from_slice(&[0_u8; 7]);
    bytes
}

fn decode_export_resource_payload(bytes: &[u8]) -> Result<ExportResourceRequest, ProtocolError> {
    if bytes.len() != EXPORT_RESOURCE_PAYLOAD_LEN {
        return if bytes.len() < EXPORT_RESOURCE_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: EXPORT_RESOURCE_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - EXPORT_RESOURCE_PAYLOAD_LEN,
            })
        };
    }
    if bytes[9..16].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(ExportResourceRequest {
        resource_id: ResourceId::new(read_u64(bytes, 0))?,
        handle_type: ExternalHandleType::try_from(bytes[8])?,
    })
}

fn encode_create_sync_payload(request: &CreateSyncRequest) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CREATE_SYNC_PAYLOAD_LEN);
    bytes.extend_from_slice(&request.device_id.get().to_le_bytes());
    bytes.push(request.kind.wire_value());
    bytes.push(request.handle_type.wire_value());
    bytes.extend_from_slice(&[0_u8; 6]);
    bytes
}

fn decode_create_sync_payload(bytes: &[u8]) -> Result<CreateSyncRequest, ProtocolError> {
    if bytes.len() != CREATE_SYNC_PAYLOAD_LEN {
        return if bytes.len() < CREATE_SYNC_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: CREATE_SYNC_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - CREATE_SYNC_PAYLOAD_LEN,
            })
        };
    }
    if bytes[10..16].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(CreateSyncRequest {
        device_id: DeviceId::new(read_u64(bytes, 0))?,
        kind: SyncKind::try_from(bytes[8])?,
        handle_type: SyncExportHandleType::try_from(bytes[9])?,
    })
}

fn encode_export_sync_payload(request: &ExportSyncRequest) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(EXPORT_SYNC_PAYLOAD_LEN);
    bytes.extend_from_slice(&request.sync_id.get().to_le_bytes());
    bytes.extend_from_slice(&request.resource_id.get().to_le_bytes());
    bytes.extend_from_slice(&request.fill_pattern.to_le_bytes());
    bytes.extend_from_slice(&[0_u8; 4]);
    bytes
}

fn decode_export_sync_payload(bytes: &[u8]) -> Result<ExportSyncRequest, ProtocolError> {
    if bytes.len() != EXPORT_SYNC_PAYLOAD_LEN {
        return if bytes.len() < EXPORT_SYNC_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: EXPORT_SYNC_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - EXPORT_SYNC_PAYLOAD_LEN,
            })
        };
    }
    if bytes[20..24].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(ExportSyncRequest {
        sync_id: SyncId::new(read_u64(bytes, 0))?,
        resource_id: ResourceId::new(read_u64(bytes, 8))?,
        fill_pattern: read_u32(bytes, 16),
    })
}

fn encode_device_list_payload(response: &DeviceListResponse) -> Vec<u8> {
    let count = response.devices.len();
    assert!(count <= MAX_DEVICE_COUNT);

    let mut bytes = Vec::with_capacity(2 + count * DEVICE_ENTRY_FIXED_LEN);
    bytes.extend_from_slice(&(count as u16).to_le_bytes());

    for device in &response.devices {
        let name = device.name.as_bytes();
        assert!(name.len() <= MAX_DEVICE_NAME_LEN);

        bytes.extend_from_slice(&device.id.get().to_le_bytes());
        bytes.push(device.class.wire_value());
        bytes.push(device.backend.wire_value());
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&device.vendor_id.to_le_bytes());
        bytes.extend_from_slice(&device.device_id.to_le_bytes());
        bytes.extend_from_slice(&device.api_version.major.to_le_bytes());
        bytes.extend_from_slice(&device.api_version.minor.to_le_bytes());
        bytes.extend_from_slice(&device.api_version.patch.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&device.driver_version.to_le_bytes());
        bytes.extend_from_slice(name);
    }

    bytes
}

fn decode_device_list_payload(bytes: &[u8]) -> Result<DeviceListResponse, ProtocolError> {
    if bytes.len() < 2 {
        return Err(ProtocolError::MalformedPayload);
    }

    let count = read_u16(bytes, 0) as usize;
    if count > MAX_DEVICE_COUNT {
        return Err(ProtocolError::DeviceCountTooLarge {
            count,
            max: MAX_DEVICE_COUNT,
        });
    }

    let minimum_len = 2_usize
        .checked_add(
            count
                .checked_mul(DEVICE_ENTRY_FIXED_LEN)
                .ok_or(ProtocolError::MalformedPayload)?,
        )
        .ok_or(ProtocolError::MalformedPayload)?;
    if bytes.len() < minimum_len {
        return Err(ProtocolError::TruncatedPayload {
            actual: bytes.len(),
            expected: minimum_len,
        });
    }

    let mut offset: usize = 2;
    let mut devices = Vec::with_capacity(count);

    for _ in 0..count {
        let fixed_end = offset
            .checked_add(DEVICE_ENTRY_FIXED_LEN)
            .ok_or(ProtocolError::MalformedPayload)?;
        if fixed_end > bytes.len() {
            return Err(ProtocolError::TruncatedPayload {
                actual: bytes.len() - offset,
                expected: DEVICE_ENTRY_FIXED_LEN,
            });
        }

        let id = DeviceId::new(read_u64(bytes, offset))?;
        let class = DeviceClass::try_from(bytes[offset + 8])?;
        let backend = BackendApi::try_from(bytes[offset + 9])?;
        let name_len = read_u16(bytes, offset + 10) as usize;
        if name_len > MAX_DEVICE_NAME_LEN {
            return Err(ProtocolError::DeviceNameTooLong {
                len: name_len,
                max: MAX_DEVICE_NAME_LEN,
            });
        }

        let name_start = fixed_end;
        let name_end = name_start
            .checked_add(name_len)
            .ok_or(ProtocolError::MalformedPayload)?;
        if name_end > bytes.len() {
            return Err(ProtocolError::TruncatedPayload {
                actual: bytes.len() - name_start,
                expected: name_len,
            });
        }

        let name = std::str::from_utf8(&bytes[name_start..name_end])
            .map_err(|_| ProtocolError::InvalidUtf8)?;

        devices.push(DeviceDesc {
            id,
            class,
            vendor_id: read_u32(bytes, offset + 12),
            device_id: read_u32(bytes, offset + 16),
            name: name.to_owned(),
            backend,
            api_version: ApiVersion::new(
                read_u16(bytes, offset + 20),
                read_u16(bytes, offset + 22),
                read_u16(bytes, offset + 24),
            ),
            driver_version: read_u32(bytes, offset + 28),
        });

        offset = name_end;
    }

    if offset != bytes.len() {
        return Err(ProtocolError::TrailingPayload {
            len: bytes.len() - offset,
        });
    }

    Ok(DeviceListResponse { devices })
}

fn encode_device_capabilities_payload(response: &DeviceCapabilitiesResponse) -> Vec<u8> {
    let capabilities = &response.capabilities;
    let heap_count = capabilities.memory.heaps.len();
    assert!(heap_count <= MAX_MEMORY_HEAP_COUNT);
    assert!(usize::from(capabilities.memory.memory_type_count) <= MAX_MEMORY_TYPE_COUNT);

    let mut bytes = Vec::with_capacity(
        DEVICE_CAPABILITIES_FIXED_PREFIX_LEN + heap_count * MEMORY_HEAP_ENTRY_LEN,
    );
    bytes.extend_from_slice(&capabilities.device_id.get().to_le_bytes());
    bytes.push(bool_to_u8(capabilities.compute.supported));
    bytes.extend_from_slice(&[0_u8; 3]);
    for value in capabilities.compute.max_workgroup_count {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in capabilities.compute.max_workgroup_size {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&capabilities.compute.max_workgroup_invocations.to_le_bytes());
    bytes.extend_from_slice(&(heap_count as u16).to_le_bytes());
    bytes.extend_from_slice(&capabilities.memory.memory_type_count.to_le_bytes());
    bytes.push(bool_to_u8(capabilities.memory.host_visible));
    bytes.push(bool_to_u8(capabilities.memory.host_coherent));
    bytes.push(bool_to_u8(capabilities.memory.device_local));
    bytes.push(0);
    bytes.push(bool_to_u8(capabilities.interop.external_memory_fd));
    bytes.push(bool_to_u8(capabilities.interop.dma_buf));
    bytes.push(bool_to_u8(capabilities.interop.external_semaphore_fd));
    bytes.push(bool_to_u8(capabilities.interop.external_fence_fd));

    for heap in &capabilities.memory.heaps {
        bytes.extend_from_slice(&heap.size_bytes.to_le_bytes());
        bytes.push(bool_to_u8(heap.device_local));
        bytes.extend_from_slice(&[0_u8; 7]);
    }

    bytes
}

fn decode_device_capabilities_payload(
    bytes: &[u8],
) -> Result<DeviceCapabilitiesResponse, ProtocolError> {
    if bytes.len() < DEVICE_CAPABILITIES_FIXED_PREFIX_LEN {
        return Err(ProtocolError::MalformedPayload);
    }

    let heap_count = read_u16(bytes, 40) as usize;
    if heap_count > MAX_MEMORY_HEAP_COUNT {
        return Err(ProtocolError::MemoryHeapCountTooLarge {
            count: heap_count,
            max: MAX_MEMORY_HEAP_COUNT,
        });
    }

    let memory_type_count = read_u16(bytes, 42);
    if usize::from(memory_type_count) > MAX_MEMORY_TYPE_COUNT {
        return Err(ProtocolError::MemoryTypeCountTooLarge {
            count: usize::from(memory_type_count),
            max: MAX_MEMORY_TYPE_COUNT,
        });
    }

    let expected_len = DEVICE_CAPABILITIES_FIXED_PREFIX_LEN
        .checked_add(
            heap_count
                .checked_mul(MEMORY_HEAP_ENTRY_LEN)
                .ok_or(ProtocolError::MalformedPayload)?,
        )
        .ok_or(ProtocolError::MalformedPayload)?;
    if bytes.len() < expected_len {
        return Err(ProtocolError::TruncatedPayload {
            actual: bytes.len(),
            expected: expected_len,
        });
    }
    if bytes.len() > expected_len {
        return Err(ProtocolError::TrailingPayload {
            len: bytes.len() - expected_len,
        });
    }

    let mut offset = DEVICE_CAPABILITIES_FIXED_PREFIX_LEN;
    let mut heaps = Vec::with_capacity(heap_count);
    for _ in 0..heap_count {
        heaps.push(MemoryHeapDesc {
            size_bytes: read_u64(bytes, offset),
            device_local: read_bool(bytes[offset + 8])?,
        });
        if bytes[offset + 9..offset + MEMORY_HEAP_ENTRY_LEN]
            .iter()
            .any(|value| *value != 0)
        {
            return Err(ProtocolError::MalformedPayload);
        }
        offset += MEMORY_HEAP_ENTRY_LEN;
    }

    if bytes[9..12].iter().any(|value| *value != 0) || bytes[47] != 0 {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(DeviceCapabilitiesResponse {
        capabilities: DeviceCapabilities {
            device_id: DeviceId::new(read_u64(bytes, 0))?,
            compute: ComputeCapabilities {
                supported: read_bool(bytes[8])?,
                max_workgroup_count: [
                    read_u32(bytes, 12),
                    read_u32(bytes, 16),
                    read_u32(bytes, 20),
                ],
                max_workgroup_size: [
                    read_u32(bytes, 24),
                    read_u32(bytes, 28),
                    read_u32(bytes, 32),
                ],
                max_workgroup_invocations: read_u32(bytes, 36),
            },
            memory: MemoryCapabilities {
                heaps,
                memory_type_count,
                host_visible: read_bool(bytes[44])?,
                host_coherent: read_bool(bytes[45])?,
                device_local: read_bool(bytes[46])?,
            },
            interop: InteropCapabilities {
                external_memory_fd: read_bool(bytes[48])?,
                dma_buf: read_bool(bytes[49])?,
                external_semaphore_fd: read_bool(bytes[50])?,
                external_fence_fd: read_bool(bytes[51])?,
            },
        },
    })
}

fn encode_video_capabilities_payload(response: &VideoCapabilitiesResponse) -> Vec<u8> {
    let capabilities = &response.capabilities;
    let count = capabilities.decode.len();
    assert!(count <= MAX_VIDEO_DECODE_CAPABILITY_COUNT);

    let mut bytes = Vec::with_capacity(VIDEO_CAPABILITIES_PREFIX_LEN);
    bytes.extend_from_slice(&capabilities.device_id.get().to_le_bytes());
    bytes.extend_from_slice(&(count as u16).to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());

    for capability in &capabilities.decode {
        capability
            .validate()
            .expect("video capability must be valid before encoding");
        let format_count = capability.output_surface_formats.len();
        bytes.push(capability.codec.wire_value());
        bytes.push(capability.profile.wire_value());
        bytes.push(capability.bit_depth.get());
        bytes.push(capability.chroma.wire_value());
        bytes.extend_from_slice(&capability.max_width.to_le_bytes());
        bytes.extend_from_slice(&capability.max_height.to_le_bytes());
        bytes.push(bool_to_u8(capability.progressive_supported));
        bytes.push(bool_to_u8(capability.interlaced_supported));
        bytes.push(format_count as u8);
        bytes.push(0);
        for format in &capability.output_surface_formats {
            bytes.push(format.wire_value());
        }
    }

    bytes
}

fn decode_video_capabilities_payload(
    bytes: &[u8],
) -> Result<VideoCapabilitiesResponse, ProtocolError> {
    if bytes.len() < VIDEO_CAPABILITIES_PREFIX_LEN {
        return Err(ProtocolError::MalformedPayload);
    }
    if bytes[10..12].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }

    let count = read_u16(bytes, 8) as usize;
    if count > MAX_VIDEO_DECODE_CAPABILITY_COUNT {
        return Err(ProtocolError::VideoDecodeCapabilityCountTooLarge {
            count,
            max: MAX_VIDEO_DECODE_CAPABILITY_COUNT,
        });
    }

    let mut offset = VIDEO_CAPABILITIES_PREFIX_LEN;
    let mut decode = Vec::with_capacity(count);
    for _ in 0..count {
        let fixed_end = offset
            .checked_add(VIDEO_DECODE_CAPABILITY_FIXED_LEN)
            .ok_or(ProtocolError::MalformedPayload)?;
        if fixed_end > bytes.len() {
            return Err(ProtocolError::TruncatedPayload {
                actual: bytes.len().saturating_sub(offset),
                expected: VIDEO_DECODE_CAPABILITY_FIXED_LEN,
            });
        }

        let codec = VideoCodec::try_from(bytes[offset])?;
        let profile = decode_video_profile(codec, bytes[offset + 1])?;
        let bit_depth = BitDepth::new(bytes[offset + 2])?;
        let chroma = ChromaSubsampling::try_from(bytes[offset + 3])?;
        let max_width = read_u32(bytes, offset + 4);
        let max_height = read_u32(bytes, offset + 8);
        let progressive_supported = read_bool(bytes[offset + 12])?;
        let interlaced_supported = read_bool(bytes[offset + 13])?;
        let format_count = bytes[offset + 14] as usize;
        if bytes[offset + 15] != 0 {
            return Err(ProtocolError::MalformedPayload);
        }
        if format_count > MAX_VIDEO_OUTPUT_FORMAT_COUNT {
            return Err(ProtocolError::VideoOutputFormatCountTooLarge {
                count: format_count,
                max: MAX_VIDEO_OUTPUT_FORMAT_COUNT,
            });
        }

        let formats_end = fixed_end
            .checked_add(format_count)
            .ok_or(ProtocolError::MalformedPayload)?;
        if formats_end > bytes.len() {
            return Err(ProtocolError::TruncatedPayload {
                actual: bytes.len().saturating_sub(fixed_end),
                expected: format_count,
            });
        }

        let mut output_surface_formats = Vec::with_capacity(format_count);
        for value in &bytes[fixed_end..formats_end] {
            output_surface_formats.push(VideoSurfaceFormat::try_from(*value)?);
        }

        let capability = VideoDecodeCapability {
            codec,
            profile,
            bit_depth,
            chroma,
            max_width,
            max_height,
            progressive_supported,
            interlaced_supported,
            output_surface_formats,
        };
        capability.validate()?;
        decode.push(capability);
        offset = formats_end;
    }

    if offset != bytes.len() {
        return Err(ProtocolError::TrailingPayload {
            len: bytes.len() - offset,
        });
    }

    Ok(VideoCapabilitiesResponse {
        capabilities: VideoCapabilities {
            device_id: DeviceId::new(read_u64(bytes, 0))?,
            decode,
        },
    })
}

fn encode_buffer_created_payload(response: &BufferCreatedResponse) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(BUFFER_CREATED_PAYLOAD_LEN);
    bytes.extend_from_slice(&response.resource_id.get().to_le_bytes());
    bytes.extend_from_slice(&response.size_bytes.to_le_bytes());
    bytes.push(encode_selected_memory(response.selected_memory));
    bytes.extend_from_slice(&[0_u8; 7]);
    bytes
}

fn decode_buffer_created_payload(bytes: &[u8]) -> Result<BufferCreatedResponse, ProtocolError> {
    if bytes.len() != BUFFER_CREATED_PAYLOAD_LEN {
        return if bytes.len() < BUFFER_CREATED_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: BUFFER_CREATED_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - BUFFER_CREATED_PAYLOAD_LEN,
            })
        };
    }
    if bytes[17..24].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(BufferCreatedResponse {
        resource_id: ResourceId::new(read_u64(bytes, 0))?,
        size_bytes: validate_buffer_size(read_u64(bytes, 8))?,
        selected_memory: decode_selected_memory(bytes[16])?,
    })
}

fn encode_image_created_payload(response: &ImageCreatedResponse) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(IMAGE_CREATED_PAYLOAD_LEN);
    bytes.extend_from_slice(&response.resource_id.get().to_le_bytes());
    bytes.extend_from_slice(&response.width.to_le_bytes());
    bytes.extend_from_slice(&response.height.to_le_bytes());
    bytes.push(response.format.wire_value());
    bytes.push(encode_selected_memory(response.selected_memory));
    bytes.extend_from_slice(&[0_u8; 6]);
    bytes
}

fn decode_image_created_payload(bytes: &[u8]) -> Result<ImageCreatedResponse, ProtocolError> {
    if bytes.len() != IMAGE_CREATED_PAYLOAD_LEN {
        return if bytes.len() < IMAGE_CREATED_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: IMAGE_CREATED_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - IMAGE_CREATED_PAYLOAD_LEN,
            })
        };
    }
    if bytes[18..24].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }
    let width = read_u32(bytes, 8);
    let height = read_u32(bytes, 12);
    validate_image_dimensions(width, height)?;

    Ok(ImageCreatedResponse {
        resource_id: ResourceId::new(read_u64(bytes, 0))?,
        width,
        height,
        format: PixelFormat::try_from(bytes[16])?,
        selected_memory: decode_selected_memory(bytes[17])?,
    })
}

fn encode_resource_destroyed_payload(response: &ResourceDestroyedResponse) -> Vec<u8> {
    response.resource_id.get().to_le_bytes().to_vec()
}

fn decode_resource_destroyed_payload(
    bytes: &[u8],
) -> Result<ResourceDestroyedResponse, ProtocolError> {
    if bytes.len() != RESOURCE_DESTROYED_PAYLOAD_LEN {
        return if bytes.len() < RESOURCE_DESTROYED_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: RESOURCE_DESTROYED_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - RESOURCE_DESTROYED_PAYLOAD_LEN,
            })
        };
    }

    Ok(ResourceDestroyedResponse {
        resource_id: ResourceId::new(read_u64(bytes, 0))?,
    })
}

fn encode_resource_exported_payload(response: &ResourceExportedResponse) -> Vec<u8> {
    let metadata = &response.metadata;
    let mut bytes = Vec::with_capacity(RESOURCE_EXPORTED_PAYLOAD_LEN);
    bytes.extend_from_slice(&metadata.resource_id.get().to_le_bytes());
    bytes.extend_from_slice(&metadata.device_id.get().to_le_bytes());
    bytes.push(metadata.kind.wire_value());
    bytes.push(metadata.handle_type.wire_value());
    bytes.push(encode_selected_memory(metadata.selected_memory));
    bytes.push(bool_to_u8(metadata.dedicated_allocation));
    bytes.push(metadata.attachment_count);
    bytes.push(metadata.pixel_format.map_or(0, PixelFormat::wire_value));
    bytes.extend_from_slice(&[0_u8; 2]);
    bytes.extend_from_slice(&metadata.allocation_size_bytes.to_le_bytes());
    bytes.extend_from_slice(&metadata.backend_memory_type_index.to_le_bytes());
    bytes.extend_from_slice(
        &metadata
            .buffer_usage
            .map_or(0, BufferUsageFlags::bits)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(
        &metadata
            .image_usage
            .map_or(0, ImageUsageFlags::bits)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&metadata.image_width.unwrap_or(0).to_le_bytes());
    bytes.extend_from_slice(&metadata.image_height.unwrap_or(0).to_le_bytes());
    bytes.extend_from_slice(&metadata.size_bytes.to_le_bytes());
    bytes.extend_from_slice(&[0_u8; 4]);
    bytes.extend_from_slice(
        &metadata
            .backend_image_layout_token
            .unwrap_or(0)
            .to_le_bytes(),
    );
    bytes
}

fn decode_resource_exported_payload(
    bytes: &[u8],
) -> Result<ResourceExportedResponse, ProtocolError> {
    if bytes.len() != RESOURCE_EXPORTED_PAYLOAD_LEN {
        return if bytes.len() < RESOURCE_EXPORTED_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: RESOURCE_EXPORTED_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - RESOURCE_EXPORTED_PAYLOAD_LEN,
            })
        };
    }
    if bytes[22..24].iter().any(|value| *value != 0)
        || bytes[60..64].iter().any(|value| *value != 0)
    {
        return Err(ProtocolError::MalformedPayload);
    }
    let kind = ResourceKind::try_from(bytes[16])?;
    let pixel_format = match bytes[21] {
        0 => None,
        value => Some(PixelFormat::try_from(value)?),
    };
    let buffer_usage_raw = read_u32(bytes, 36);
    let image_usage_raw = read_u32(bytes, 40);
    let image_width = read_u32(bytes, 44);
    let image_height = read_u32(bytes, 48);
    let size_bytes = read_u64(bytes, 52);
    let backend_image_layout_token = match read_u64(bytes, 64) {
        0 => None,
        value => Some(value),
    };
    let (buffer_usage, image_usage, image_width, image_height, pixel_format) = match kind {
        ResourceKind::Buffer => {
            validate_buffer_size(size_bytes)?;
            if image_usage_raw != 0
                || image_width != 0
                || image_height != 0
                || pixel_format.is_some()
                || backend_image_layout_token.is_some()
            {
                return Err(ProtocolError::MalformedPayload);
            }
            (
                Some(BufferUsageFlags::new(buffer_usage_raw)?),
                None,
                None,
                None,
                None,
            )
        }
        ResourceKind::Image => {
            if buffer_usage_raw != 0 {
                return Err(ProtocolError::MalformedPayload);
            }
            validate_image_dimensions(image_width, image_height)?;
            let format = pixel_format.ok_or(ProtocolError::UnsupportedPixelFormat)?;
            let pixel_bytes = image_byte_len(image_width, image_height, format)?;
            if size_bytes != pixel_bytes {
                return Err(ProtocolError::MalformedPayload);
            }
            (
                None,
                Some(ImageUsageFlags::new(image_usage_raw)?),
                Some(image_width),
                Some(image_height),
                Some(format),
            )
        }
        ResourceKind::VideoSurface => return Err(ProtocolError::MalformedPayload),
    };
    let allocation_size_bytes = read_u64(bytes, 24);
    if allocation_size_bytes < size_bytes {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(ResourceExportedResponse {
        metadata: ExportedResourceMetadata {
            resource_id: ResourceId::new(read_u64(bytes, 0))?,
            device_id: DeviceId::new(read_u64(bytes, 8))?,
            kind,
            size_bytes,
            allocation_size_bytes,
            buffer_usage,
            image_width,
            image_height,
            pixel_format,
            image_usage,
            backend_memory_type_index: read_u32(bytes, 32),
            backend_image_layout_token,
            handle_type: ExternalHandleType::try_from(bytes[17])?,
            selected_memory: decode_selected_memory(bytes[18])?,
            dedicated_allocation: read_bool(bytes[19])?,
            attachment_count: bytes[20],
        },
    })
}

fn encode_sync_created_payload(response: &SyncCreatedResponse) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SYNC_CREATED_PAYLOAD_LEN);
    bytes.extend_from_slice(&response.sync_id.get().to_le_bytes());
    bytes
}

fn decode_sync_created_payload(bytes: &[u8]) -> Result<SyncCreatedResponse, ProtocolError> {
    if bytes.len() != SYNC_CREATED_PAYLOAD_LEN {
        return if bytes.len() < SYNC_CREATED_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: SYNC_CREATED_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - SYNC_CREATED_PAYLOAD_LEN,
            })
        };
    }

    Ok(SyncCreatedResponse {
        sync_id: SyncId::new(read_u64(bytes, 0))?,
    })
}

fn encode_sync_exported_payload(response: &SyncExportedResponse) -> Vec<u8> {
    let metadata = &response.metadata;
    let mut bytes = Vec::with_capacity(SYNC_EXPORTED_PAYLOAD_LEN);
    bytes.extend_from_slice(&metadata.sync_id.get().to_le_bytes());
    bytes.push(metadata.handle_type.wire_value());
    bytes.push(metadata.attachment_count);
    bytes.extend_from_slice(&[0_u8; 2]);
    bytes.extend_from_slice(&metadata.fill_pattern.to_le_bytes());
    bytes
}

fn decode_sync_exported_payload(bytes: &[u8]) -> Result<SyncExportedResponse, ProtocolError> {
    if bytes.len() != SYNC_EXPORTED_PAYLOAD_LEN {
        return if bytes.len() < SYNC_EXPORTED_PAYLOAD_LEN {
            Err(ProtocolError::TruncatedPayload {
                actual: bytes.len(),
                expected: SYNC_EXPORTED_PAYLOAD_LEN,
            })
        } else {
            Err(ProtocolError::TrailingPayload {
                len: bytes.len() - SYNC_EXPORTED_PAYLOAD_LEN,
            })
        };
    }
    if bytes[10..12].iter().any(|value| *value != 0) || bytes[16..].iter().any(|value| *value != 0)
    {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(SyncExportedResponse {
        metadata: ExportedSyncMetadata {
            sync_id: SyncId::new(read_u64(bytes, 0))?,
            handle_type: SyncExportHandleType::try_from(bytes[8])?,
            attachment_count: bytes[9],
            fill_pattern: read_u32(bytes, 12),
        },
    })
}

fn validate_buffer_size(size: u64) -> Result<u64, ProtocolError> {
    if size == 0 || size > MAX_BUFFER_SIZE_BYTES {
        Err(ProtocolError::InvalidBufferSize {
            size,
            max: MAX_BUFFER_SIZE_BYTES,
        })
    } else {
        Ok(size)
    }
}

fn decode_video_profile(codec: VideoCodec, value: u8) -> Result<VideoProfile, ProtocolError> {
    match codec {
        VideoCodec::H264 => Ok(VideoProfile::H264(H264Profile::try_from(value)?)),
        VideoCodec::Mpeg2 => Ok(VideoProfile::Mpeg2(Mpeg2Profile::try_from(value)?)),
    }
}

pub fn validate_image_dimensions(width: u32, height: u32) -> Result<(), ProtocolError> {
    if width == 0 || height == 0 || width > MAX_IMAGE_WIDTH || height > MAX_IMAGE_HEIGHT {
        return Err(ProtocolError::InvalidImageDimensions { width, height });
    }
    image_byte_len(width, height, PixelFormat::Rgba8Unorm)?;
    Ok(())
}

pub fn validate_video_surface_dimensions(width: u32, height: u32) -> Result<(), ProtocolError> {
    if width == 0
        || height == 0
        || width > MAX_VIDEO_SURFACE_WIDTH
        || height > MAX_VIDEO_SURFACE_HEIGHT
    {
        Err(ProtocolError::InvalidVideoSurfaceDimensions { width, height })
    } else {
        Ok(())
    }
}

fn validate_visible_region(
    region: VisibleRegion,
    coded_width: u32,
    coded_height: u32,
) -> Result<(), ProtocolError> {
    if region.width == 0 || region.height == 0 {
        return Err(ProtocolError::InvalidVideoVisibleRegion);
    }
    let end_x = region
        .x
        .checked_add(region.width)
        .ok_or(ProtocolError::InvalidVideoVisibleRegion)?;
    let end_y = region
        .y
        .checked_add(region.height)
        .ok_or(ProtocolError::InvalidVideoVisibleRegion)?;
    if end_x > coded_width || end_y > coded_height {
        Err(ProtocolError::InvalidVideoVisibleRegion)
    } else {
        Ok(())
    }
}

pub fn image_byte_len(width: u32, height: u32, format: PixelFormat) -> Result<u64, ProtocolError> {
    let bytes_per_pixel = match format {
        PixelFormat::Rgba8Unorm => 4_u64,
    };
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .ok_or(ProtocolError::InvalidImageDimensions { width, height })
}

fn encode_memory_preference(preference: MemoryPreference) -> u8 {
    bool_to_u8(preference.device_preferred)
        | (bool_to_u8(preference.host_visible_required) << 1)
        | (bool_to_u8(preference.host_coherent_preferred) << 2)
}

fn decode_memory_preference(value: u8) -> Result<MemoryPreference, ProtocolError> {
    if value & !0x7 != 0 {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(MemoryPreference {
        device_preferred: value & 0x1 != 0,
        host_visible_required: value & 0x2 != 0,
        host_coherent_preferred: value & 0x4 != 0,
    })
}

fn encode_selected_memory(memory: SelectedMemoryProperties) -> u8 {
    bool_to_u8(memory.device_local)
        | (bool_to_u8(memory.host_visible) << 1)
        | (bool_to_u8(memory.host_coherent) << 2)
}

fn decode_selected_memory(value: u8) -> Result<SelectedMemoryProperties, ProtocolError> {
    if value & !0x7 != 0 {
        return Err(ProtocolError::MalformedPayload);
    }

    Ok(SelectedMemoryProperties {
        device_local: value & 0x1 != 0,
        host_visible: value & 0x2 != 0,
        host_coherent: value & 0x4 != 0,
    })
}

const fn bool_to_u8(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

fn read_bool(value: u8) -> Result<bool, ProtocolError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ProtocolError::MalformedPayload),
    }
}

fn read_u16(bytes: &[u8], start: usize) -> u16 {
    u16::from_le_bytes([bytes[start], bytes[start + 1]])
}

fn read_u32(bytes: &[u8], start: usize) -> u32 {
    u32::from_le_bytes([
        bytes[start],
        bytes[start + 1],
        bytes[start + 2],
        bytes[start + 3],
    ])
}

fn read_u64(bytes: &[u8], start: usize) -> u64 {
    u64::from_le_bytes([
        bytes[start],
        bytes[start + 1],
        bytes[start + 2],
        bytes[start + 3],
        bytes[start + 4],
        bytes[start + 5],
        bytes[start + 6],
        bytes[start + 7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_current_version_succeeds() {
        let selected =
            negotiate_protocol_version(CURRENT_PROTOCOL_VERSION, CURRENT_PROTOCOL_VERSION)
                .expect("exact current version should negotiate");

        assert_eq!(selected, CURRENT_PROTOCOL_VERSION);
    }

    #[test]
    fn client_range_containing_current_minor_selects_current_version() {
        let selected =
            negotiate_protocol_version(ProtocolVersion::new(0, 0), ProtocolVersion::new(0, 9))
                .expect("version should negotiate");

        assert_eq!(selected, CURRENT_PROTOCOL_VERSION);
    }

    #[test]
    fn rejects_invalid_version_range() {
        let err =
            negotiate_protocol_version(ProtocolVersion::new(0, 2), ProtocolVersion::new(0, 1))
                .expect_err("range should be invalid");

        assert_eq!(
            err,
            ProtocolError::InvalidVersionRange {
                min: ProtocolVersion::new(0, 2),
                max: ProtocolVersion::new(0, 1),
            }
        );
    }

    #[test]
    fn rejects_range_entirely_below_current_minor() {
        let err =
            negotiate_protocol_version(ProtocolVersion::new(0, 0), ProtocolVersion::new(0, 0))
                .expect_err("range below current minor should be unsupported");

        assert_eq!(
            err,
            ProtocolError::UnsupportedVersion {
                min: ProtocolVersion::new(0, 0),
                max: ProtocolVersion::new(0, 0),
                supported: CURRENT_PROTOCOL_VERSION,
            }
        );
    }

    #[test]
    fn rejects_different_major_version() {
        let err =
            negotiate_protocol_version(ProtocolVersion::new(1, 0), ProtocolVersion::new(1, 0))
                .expect_err("major version should be unsupported");

        assert_eq!(
            err,
            ProtocolError::UnsupportedVersion {
                min: ProtocolVersion::new(1, 0),
                max: ProtocolVersion::new(1, 0),
                supported: CURRENT_PROTOCOL_VERSION,
            }
        );
    }

    #[test]
    fn rejects_range_crossing_major_versions() {
        let err =
            negotiate_protocol_version(ProtocolVersion::new(0, 0), ProtocolVersion::new(1, 0))
                .expect_err("cross-major range should be unsupported");

        assert_eq!(
            err,
            ProtocolError::UnsupportedVersion {
                min: ProtocolVersion::new(0, 0),
                max: ProtocolVersion::new(1, 0),
                supported: CURRENT_PROTOCOL_VERSION,
            }
        );
    }

    #[test]
    fn sync_id_zero_is_rejected() {
        assert_eq!(SyncId::new(0), Err(ProtocolError::InvalidSyncId));
        assert_eq!(SyncId::new(9).expect("sync id").get(), 9);
    }

    #[test]
    fn create_sync_request_round_trips() {
        let message = WireMessage::CreateSync {
            request_id: 701,
            request: CreateSyncRequest {
                device_id: DeviceId::new(3).expect("device id"),
                kind: SyncKind::BinarySemaphore,
                handle_type: SyncExportHandleType::SyncFd,
            },
        };

        let decoded =
            decode_wire_message(&encode_wire_message(&message)).expect("decode create sync");

        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 701);
    }

    #[test]
    fn export_sync_request_round_trips() {
        let message = WireMessage::ExportSync {
            request_id: 702,
            request: ExportSyncRequest {
                sync_id: SyncId::new(4).expect("sync id"),
                resource_id: ResourceId::new(5).expect("resource id"),
                fill_pattern: 0x1a2b_3c4d,
            },
        };

        let decoded =
            decode_wire_message(&encode_wire_message(&message)).expect("decode export sync");

        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 702);
    }

    #[test]
    fn sync_created_response_round_trips() {
        let message = WireMessage::SyncCreated {
            request_id: 703,
            response: SyncCreatedResponse {
                sync_id: SyncId::new(6).expect("sync id"),
            },
        };

        let decoded =
            decode_wire_message(&encode_wire_message(&message)).expect("decode sync created");

        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 703);
    }

    #[test]
    fn sync_exported_response_round_trips() {
        let message = WireMessage::SyncExported {
            request_id: 704,
            response: SyncExportedResponse {
                metadata: ExportedSyncMetadata {
                    sync_id: SyncId::new(7).expect("sync id"),
                    handle_type: SyncExportHandleType::SyncFd,
                    attachment_count: 1,
                    fill_pattern: 0xfeed_beef,
                },
            },
        };

        let decoded =
            decode_wire_message(&encode_wire_message(&message)).expect("decode sync exported");

        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 704);
    }

    #[test]
    fn create_sync_rejects_reserved_bytes() {
        let mut payload = encode_create_sync_payload(&CreateSyncRequest {
            device_id: DeviceId::new(1).expect("device id"),
            kind: SyncKind::BinarySemaphore,
            handle_type: SyncExportHandleType::SyncFd,
        });
        payload[10] = 1;

        assert_eq!(
            decode_create_sync_payload(&payload),
            Err(ProtocolError::MalformedPayload)
        );
    }

    #[test]
    fn export_sync_rejects_truncated_payload() {
        let payload = [0_u8; EXPORT_SYNC_PAYLOAD_LEN - 1];

        assert_eq!(
            decode_export_sync_payload(&payload),
            Err(ProtocolError::TruncatedPayload {
                actual: EXPORT_SYNC_PAYLOAD_LEN - 1,
                expected: EXPORT_SYNC_PAYLOAD_LEN,
            })
        );
    }

    #[test]
    fn sync_exported_rejects_unknown_handle_type() {
        let mut payload = encode_sync_exported_payload(&SyncExportedResponse {
            metadata: ExportedSyncMetadata {
                sync_id: SyncId::new(1).expect("sync id"),
                handle_type: SyncExportHandleType::SyncFd,
                attachment_count: 1,
                fill_pattern: 1,
            },
        });
        payload[8] = 99;

        assert_eq!(
            decode_sync_exported_payload(&payload),
            Err(ProtocolError::MalformedPayload)
        );
    }

    #[test]
    fn sync_error_codes_are_stable() {
        assert_eq!(ProtocolErrorCode::InvalidSyncId.wire_value(), 31);
        assert_eq!(ProtocolErrorCode::UnknownSync.wire_value(), 32);
        assert_eq!(
            ProtocolErrorCode::UnsupportedSyncHandleType.wire_value(),
            33
        );
        assert_eq!(ProtocolErrorCode::SyncExportFailed.wire_value(), 34);
        assert_eq!(
            ProtocolErrorCode::try_from(32).expect("known sync error"),
            ProtocolErrorCode::UnknownSync
        );
        assert_eq!(
            ProtocolErrorCode::from(&ProtocolError::UnknownSync),
            ProtocolErrorCode::UnknownSync
        );
    }

    #[test]
    fn hello_wire_round_trip() {
        let message = WireMessage::Hello {
            request_id: 7,
            request: HelloRequest {
                min_version: ProtocolVersion::new(0, 0),
                max_version: ProtocolVersion::new(0, 9),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn welcome_wire_round_trip() {
        let message = WireMessage::Welcome {
            request_id: 7,
            response: WelcomeResponse {
                version: CURRENT_PROTOCOL_VERSION,
                session_id: SessionId::new(99).expect("session id"),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn error_wire_round_trip() {
        let message = WireMessage::Error {
            request_id: 7,
            response: ErrorResponse {
                code: ProtocolErrorCode::UnsupportedVersion,
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut bytes = encode_wire_message(&WireMessage::Hello {
            request_id: 1,
            request: HelloRequest::current(),
        });
        bytes[1] = 0;

        assert!(matches!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidMagic { .. })
        ));
    }

    #[test]
    fn rejects_truncated_header() {
        assert_eq!(
            decode_wire_message(&[0; WIRE_HEADER_LEN - 1]),
            Err(ProtocolError::TruncatedHeader {
                actual: WIRE_HEADER_LEN - 1,
                expected: WIRE_HEADER_LEN,
            })
        );
    }

    #[test]
    fn rejects_truncated_payload() {
        let mut bytes = encode_wire_message(&WireMessage::Hello {
            request_id: 1,
            request: HelloRequest::current(),
        });
        bytes.pop();

        assert!(matches!(
            decode_wire_message(&bytes),
            Err(ProtocolError::TruncatedPayload { .. })
        ));
    }

    #[test]
    fn rejects_oversized_payload_before_payload_decode() {
        let mut bytes = encode_wire_message(&WireMessage::Hello {
            request_id: 1,
            request: HelloRequest::current(),
        });
        bytes[12..16].copy_from_slice(&(MAX_PAYLOAD_LEN + 1).to_le_bytes());

        assert_eq!(
            decode_wire_header(&bytes[..WIRE_HEADER_LEN]),
            Err(ProtocolError::PayloadTooLarge {
                len: MAX_PAYLOAD_LEN + 1,
                max: MAX_PAYLOAD_LEN,
            })
        );
    }

    #[test]
    fn rejects_unknown_message_kind() {
        let mut bytes = encode_wire_message(&WireMessage::Hello {
            request_id: 1,
            request: HelloRequest::current(),
        });
        bytes[8] = 99;

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::UnknownMessageKind { kind: 99 })
        );
    }

    #[test]
    fn rejects_unknown_opcode() {
        let mut bytes = encode_wire_message(&WireMessage::Hello {
            request_id: 1,
            request: HelloRequest::current(),
        });
        bytes[9] = 99;

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::UnknownOpcode {
                kind: MessageKind::Request,
                opcode: 99,
            })
        );
    }

    #[test]
    fn preserves_request_id() {
        let message = WireMessage::Welcome {
            request_id: 123,
            response: WelcomeResponse {
                version: CURRENT_PROTOCOL_VERSION,
                session_id: SessionId::new(456).expect("session id"),
            },
        };

        let decoded = decode_wire_message(&encode_wire_message(&message)).expect("wire message");

        assert_eq!(decoded.request_id(), 123);
    }

    #[test]
    fn rejects_invalid_protocol_range_in_wire_header() {
        let mut bytes = encode_wire_message(&WireMessage::Hello {
            request_id: 1,
            request: HelloRequest::current(),
        });
        bytes[6..8].copy_from_slice(&0_u16.to_le_bytes());

        assert!(matches!(
            decode_wire_message(&bytes),
            Err(ProtocolError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn rejects_invalid_protocol_range_in_hello_payload() {
        let mut bytes = encode_wire_message(&WireMessage::Hello {
            request_id: 1,
            request: HelloRequest::current(),
        });
        bytes[24..26].copy_from_slice(&0_u16.to_le_bytes());
        bytes[26..28].copy_from_slice(&2_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&0_u16.to_le_bytes());
        bytes[30..32].copy_from_slice(&1_u16.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidVersionRange {
                min: ProtocolVersion::new(0, 2),
                max: ProtocolVersion::new(0, 1),
            })
        );
    }

    #[test]
    fn rejects_nonzero_flags() {
        let mut bytes = encode_wire_message(&WireMessage::Hello {
            request_id: 1,
            request: HelloRequest::current(),
        });
        bytes[10..12].copy_from_slice(&1_u16.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidFlags { flags: 1 })
        );
    }

    #[test]
    fn rejects_zero_session_id_in_welcome_payload() {
        let mut bytes = encode_wire_message(&WireMessage::Welcome {
            request_id: 1,
            response: WelcomeResponse {
                version: CURRENT_PROTOCOL_VERSION,
                session_id: SessionId::new(1).expect("session id"),
            },
        });
        bytes[28..36].copy_from_slice(&0_u64.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidSessionId)
        );
    }

    #[test]
    fn device_id_rejects_zero() {
        assert_eq!(DeviceId::new(0), Err(ProtocolError::InvalidDeviceId));
        assert_eq!(DeviceId::new(1).expect("device id").get(), 1);
    }

    #[test]
    fn device_class_wire_round_trip() {
        for class in [
            DeviceClass::IntegratedGpu,
            DeviceClass::DiscreteGpu,
            DeviceClass::Software,
            DeviceClass::Other,
        ] {
            assert_eq!(DeviceClass::try_from(class.wire_value()), Ok(class));
        }
    }

    #[test]
    fn device_desc_wire_round_trip() {
        let message = WireMessage::DeviceList {
            request_id: 9,
            response: DeviceListResponse {
                devices: vec![sample_device("Sample GPU", DeviceClass::IntegratedGpu, 1)],
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn device_list_with_zero_devices_round_trips() {
        let message = WireMessage::DeviceList {
            request_id: 10,
            response: DeviceListResponse {
                devices: Vec::new(),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn device_list_with_multiple_devices_round_trips() {
        let message = WireMessage::DeviceList {
            request_id: 11,
            response: DeviceListResponse {
                devices: vec![
                    sample_device("Integrated", DeviceClass::IntegratedGpu, 1),
                    sample_device("Discrete", DeviceClass::DiscreteGpu, 2),
                    sample_device("Software", DeviceClass::Software, 3),
                ],
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn accepts_maximum_valid_device_name_length() {
        let name = "x".repeat(MAX_DEVICE_NAME_LEN);
        let message = WireMessage::DeviceList {
            request_id: 12,
            response: DeviceListResponse {
                devices: vec![sample_device(&name, DeviceClass::Other, 1)],
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn rejects_overlong_device_name() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceList {
            request_id: 13,
            response: DeviceListResponse {
                devices: vec![sample_device("short", DeviceClass::Other, 1)],
            },
        });
        bytes[WIRE_HEADER_LEN + 2 + 10..WIRE_HEADER_LEN + 2 + 12]
            .copy_from_slice(&((MAX_DEVICE_NAME_LEN as u16) + 1).to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::DeviceNameTooLong {
                len: MAX_DEVICE_NAME_LEN + 1,
                max: MAX_DEVICE_NAME_LEN,
            })
        );
    }

    #[test]
    fn rejects_invalid_utf8_device_name() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceList {
            request_id: 14,
            response: DeviceListResponse {
                devices: vec![sample_device("x", DeviceClass::Other, 1)],
            },
        });
        let name_offset = WIRE_HEADER_LEN + 2 + DEVICE_ENTRY_FIXED_LEN;
        bytes[name_offset] = 0xff;

        assert_eq!(decode_wire_message(&bytes), Err(ProtocolError::InvalidUtf8));
    }

    #[test]
    fn rejects_excessive_device_count() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceList {
            request_id: 15,
            response: DeviceListResponse {
                devices: Vec::new(),
            },
        });
        bytes[WIRE_HEADER_LEN..WIRE_HEADER_LEN + 2]
            .copy_from_slice(&((MAX_DEVICE_COUNT as u16) + 1).to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::DeviceCountTooLarge {
                count: MAX_DEVICE_COUNT + 1,
                max: MAX_DEVICE_COUNT,
            })
        );
    }

    #[test]
    fn rejects_truncated_device_entry() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceList {
            request_id: 16,
            response: DeviceListResponse {
                devices: vec![sample_device("x", DeviceClass::Other, 1)],
            },
        });
        bytes.truncate(bytes.len() - 2);

        assert!(matches!(
            decode_wire_message(&bytes),
            Err(ProtocolError::TruncatedPayload { .. })
        ));
    }

    #[test]
    fn rejects_malformed_device_count() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceList {
            request_id: 17,
            response: DeviceListResponse {
                devices: Vec::new(),
            },
        });
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());
        bytes.truncate(WIRE_HEADER_LEN + 1);

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::MalformedPayload)
        );
    }

    #[test]
    fn preserves_request_id_for_enumerate_devices_and_device_list() {
        let enumerate = WireMessage::EnumerateDevices { request_id: 18 };
        let device_list = WireMessage::DeviceList {
            request_id: 19,
            response: DeviceListResponse {
                devices: vec![sample_device("x", DeviceClass::Other, 1)],
            },
        };

        let decoded_enumerate =
            decode_wire_message(&encode_wire_message(&enumerate)).expect("enumerate");
        let decoded_device_list =
            decode_wire_message(&encode_wire_message(&device_list)).expect("device list");

        assert_eq!(decoded_enumerate.request_id(), 18);
        assert_eq!(decoded_device_list.request_id(), 19);
    }

    #[test]
    fn capability_request_round_trip() {
        let message = WireMessage::QueryDeviceCapabilities {
            request_id: 20,
            request: QueryDeviceCapabilitiesRequest {
                device_id: DeviceId::new(7).expect("device id"),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn capability_response_round_trip() {
        let message = WireMessage::DeviceCapabilities {
            request_id: 21,
            response: DeviceCapabilitiesResponse {
                capabilities: sample_capabilities(7),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn rejects_zero_device_id_in_capability_request() {
        let mut bytes = encode_wire_message(&WireMessage::QueryDeviceCapabilities {
            request_id: 22,
            request: QueryDeviceCapabilitiesRequest {
                device_id: DeviceId::new(1).expect("device id"),
            },
        });
        bytes[WIRE_HEADER_LEN..WIRE_HEADER_LEN + 8].copy_from_slice(&0_u64.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidDeviceId)
        );
    }

    #[test]
    fn unknown_device_id_has_stable_error_code() {
        assert_eq!(
            ProtocolErrorCode::from(&ProtocolError::UnknownDeviceId),
            ProtocolErrorCode::UnknownDeviceId
        );
        assert_eq!(
            ProtocolErrorCode::try_from(ProtocolErrorCode::UnknownDeviceId.wire_value()),
            Ok(ProtocolErrorCode::UnknownDeviceId)
        );
    }

    #[test]
    fn compute_capability_encoding_round_trips() {
        let mut capabilities = sample_capabilities(8);
        capabilities.compute = ComputeCapabilities {
            supported: true,
            max_workgroup_count: [1, 2, 3],
            max_workgroup_size: [4, 5, 6],
            max_workgroup_invocations: 7,
        };
        let message = WireMessage::DeviceCapabilities {
            request_id: 23,
            response: DeviceCapabilitiesResponse { capabilities },
        };

        let decoded = decode_wire_message(&encode_wire_message(&message)).expect("capabilities");

        assert_eq!(decoded, message);
    }

    #[test]
    fn multiple_memory_heaps_round_trip() {
        let mut capabilities = sample_capabilities(9);
        capabilities.memory.heaps = vec![
            MemoryHeapDesc {
                size_bytes: 64 * 1024 * 1024,
                device_local: true,
            },
            MemoryHeapDesc {
                size_bytes: 128 * 1024 * 1024,
                device_local: false,
            },
        ];
        let message = WireMessage::DeviceCapabilities {
            request_id: 24,
            response: DeviceCapabilitiesResponse { capabilities },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn maximum_valid_heap_count_round_trips() {
        let mut capabilities = sample_capabilities(10);
        capabilities.memory.heaps = (0..MAX_MEMORY_HEAP_COUNT)
            .map(|index| MemoryHeapDesc {
                size_bytes: index as u64 + 1,
                device_local: index % 2 == 0,
            })
            .collect();
        let message = WireMessage::DeviceCapabilities {
            request_id: 25,
            response: DeviceCapabilitiesResponse { capabilities },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn rejects_excessive_heap_count() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceCapabilities {
            request_id: 26,
            response: DeviceCapabilitiesResponse {
                capabilities: sample_capabilities(11),
            },
        });
        bytes[WIRE_HEADER_LEN + 40..WIRE_HEADER_LEN + 42]
            .copy_from_slice(&((MAX_MEMORY_HEAP_COUNT as u16) + 1).to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::MemoryHeapCountTooLarge {
                count: MAX_MEMORY_HEAP_COUNT + 1,
                max: MAX_MEMORY_HEAP_COUNT,
            })
        );
    }

    #[test]
    fn rejects_excessive_memory_type_count() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceCapabilities {
            request_id: 27,
            response: DeviceCapabilitiesResponse {
                capabilities: sample_capabilities(12),
            },
        });
        bytes[WIRE_HEADER_LEN + 42..WIRE_HEADER_LEN + 44]
            .copy_from_slice(&((MAX_MEMORY_TYPE_COUNT as u16) + 1).to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::MemoryTypeCountTooLarge {
                count: MAX_MEMORY_TYPE_COUNT + 1,
                max: MAX_MEMORY_TYPE_COUNT,
            })
        );
    }

    #[test]
    fn rejects_truncated_heap_entry() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceCapabilities {
            request_id: 28,
            response: DeviceCapabilitiesResponse {
                capabilities: sample_capabilities(13),
            },
        });
        bytes.truncate(bytes.len() - 1);

        assert!(matches!(
            decode_wire_message(&bytes),
            Err(ProtocolError::TruncatedPayload { .. })
        ));
    }

    #[test]
    fn interop_capability_encoding_round_trips() {
        let mut capabilities = sample_capabilities(14);
        capabilities.interop = InteropCapabilities {
            external_memory_fd: true,
            dma_buf: false,
            external_semaphore_fd: true,
            external_fence_fd: false,
        };
        let message = WireMessage::DeviceCapabilities {
            request_id: 29,
            response: DeviceCapabilitiesResponse { capabilities },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn rejects_malformed_capability_payload() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceCapabilities {
            request_id: 30,
            response: DeviceCapabilitiesResponse {
                capabilities: sample_capabilities(15),
            },
        });
        bytes[WIRE_HEADER_LEN + 8] = 2;

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::MalformedPayload)
        );
    }

    #[test]
    fn preserves_request_id_for_capability_messages() {
        let request = WireMessage::QueryDeviceCapabilities {
            request_id: 31,
            request: QueryDeviceCapabilitiesRequest {
                device_id: DeviceId::new(1).expect("device id"),
            },
        };
        let response = WireMessage::DeviceCapabilities {
            request_id: 32,
            response: DeviceCapabilitiesResponse {
                capabilities: sample_capabilities(16),
            },
        };

        let decoded_request =
            decode_wire_message(&encode_wire_message(&request)).expect("capability request");
        let decoded_response =
            decode_wire_message(&encode_wire_message(&response)).expect("capability response");

        assert_eq!(decoded_request.request_id(), 31);
        assert_eq!(decoded_response.request_id(), 32);
    }

    #[test]
    fn rejects_trailing_capability_payload() {
        let mut bytes = encode_wire_message(&WireMessage::DeviceCapabilities {
            request_id: 33,
            response: DeviceCapabilitiesResponse {
                capabilities: sample_capabilities(17),
            },
        });
        let payload_len = (bytes.len() - WIRE_HEADER_LEN + 1) as u32;
        bytes[12..16].copy_from_slice(&payload_len.to_le_bytes());
        bytes.push(0);

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::TrailingPayload { len: 1 })
        );
    }

    #[test]
    fn resource_id_rejects_zero() {
        assert_eq!(ResourceId::new(0), Err(ProtocolError::InvalidResourceId));
        assert_eq!(ResourceId::new(1).expect("resource id").get(), 1);
    }

    #[test]
    fn create_buffer_request_round_trip() {
        let message = WireMessage::CreateBuffer {
            request_id: 34,
            request: CreateBufferRequest {
                desc: sample_buffer_desc(),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn external_sharing_wire_round_trip() {
        let mut desc = sample_buffer_desc();
        desc.external_sharing = ExternalSharing::Required {
            handle_type: ExternalHandleType::DmaBuf,
        };
        let message = WireMessage::CreateBuffer {
            request_id: 347,
            request: CreateBufferRequest { desc },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn rejects_unknown_external_sharing_value() {
        let mut bytes = encode_wire_message(&WireMessage::CreateBuffer {
            request_id: 348,
            request: CreateBufferRequest {
                desc: sample_buffer_desc(),
            },
        });
        bytes[WIRE_HEADER_LEN + 21] = 99;

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::MalformedPayload)
        );
    }

    #[test]
    fn buffer_created_response_round_trip() {
        let message = WireMessage::BufferCreated {
            request_id: 35,
            response: sample_buffer_created(9),
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn pixel_format_wire_round_trip() {
        assert_eq!(PixelFormat::Rgba8Unorm.wire_value(), 1);
        assert_eq!(PixelFormat::try_from(1), Ok(PixelFormat::Rgba8Unorm));
    }

    #[test]
    fn rejects_invalid_pixel_format() {
        assert_eq!(
            PixelFormat::try_from(99),
            Err(ProtocolError::UnsupportedPixelFormat)
        );
    }

    #[test]
    fn resource_kind_wire_round_trip() {
        assert_eq!(ResourceKind::try_from(1), Ok(ResourceKind::Buffer));
        assert_eq!(ResourceKind::try_from(2), Ok(ResourceKind::Image));
        assert_eq!(ResourceKind::try_from(3), Ok(ResourceKind::VideoSurface));
    }

    #[test]
    fn image_desc_round_trip() {
        let desc = sample_image_desc();
        let message = WireMessage::CreateImage {
            request_id: 350,
            request: CreateImageRequest { desc },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn rejects_zero_image_width() {
        let mut bytes = encode_wire_message(&WireMessage::CreateImage {
            request_id: 351,
            request: CreateImageRequest {
                desc: sample_image_desc(),
            },
        });
        bytes[WIRE_HEADER_LEN + 8..WIRE_HEADER_LEN + 12].copy_from_slice(&0_u32.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidImageDimensions {
                width: 0,
                height: 64,
            })
        );
    }

    #[test]
    fn rejects_zero_image_height() {
        let mut bytes = encode_wire_message(&WireMessage::CreateImage {
            request_id: 352,
            request: CreateImageRequest {
                desc: sample_image_desc(),
            },
        });
        bytes[WIRE_HEADER_LEN + 12..WIRE_HEADER_LEN + 16].copy_from_slice(&0_u32.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidImageDimensions {
                width: 64,
                height: 0,
            })
        );
    }

    #[test]
    fn rejects_oversized_image_width() {
        let mut bytes = encode_wire_message(&WireMessage::CreateImage {
            request_id: 353,
            request: CreateImageRequest {
                desc: sample_image_desc(),
            },
        });
        bytes[WIRE_HEADER_LEN + 8..WIRE_HEADER_LEN + 12]
            .copy_from_slice(&(MAX_IMAGE_WIDTH + 1).to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidImageDimensions {
                width: MAX_IMAGE_WIDTH + 1,
                height: 64,
            })
        );
    }

    #[test]
    fn rejects_oversized_image_height() {
        let mut bytes = encode_wire_message(&WireMessage::CreateImage {
            request_id: 354,
            request: CreateImageRequest {
                desc: sample_image_desc(),
            },
        });
        bytes[WIRE_HEADER_LEN + 12..WIRE_HEADER_LEN + 16]
            .copy_from_slice(&(MAX_IMAGE_HEIGHT + 1).to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidImageDimensions {
                width: 64,
                height: MAX_IMAGE_HEIGHT + 1,
            })
        );
    }

    #[test]
    fn image_byte_len_rejects_dimension_multiplication_overflow() {
        assert_eq!(
            image_byte_len(u32::MAX, u32::MAX, PixelFormat::Rgba8Unorm),
            Err(ProtocolError::InvalidImageDimensions {
                width: u32::MAX,
                height: u32::MAX,
            })
        );
    }

    #[test]
    fn create_image_request_round_trip() {
        let message = WireMessage::CreateImage {
            request_id: 355,
            request: CreateImageRequest {
                desc: sample_image_desc(),
            },
        };

        let decoded = decode_wire_message(&encode_wire_message(&message)).expect("decode image");
        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 355);
    }

    #[test]
    fn image_created_response_round_trip() {
        let message = WireMessage::ImageCreated {
            request_id: 356,
            response: sample_image_created(12),
        };

        let decoded = decode_wire_message(&encode_wire_message(&message)).expect("decode image");
        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 356);
    }

    #[test]
    fn destroy_resource_request_round_trip() {
        let message = WireMessage::DestroyResource {
            request_id: 36,
            request: DestroyResourceRequest {
                resource_id: ResourceId::new(9).expect("resource id"),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn resource_destroyed_response_round_trip() {
        let message = WireMessage::ResourceDestroyed {
            request_id: 37,
            response: ResourceDestroyedResponse {
                resource_id: ResourceId::new(9).expect("resource id"),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn export_resource_request_round_trip() {
        let message = WireMessage::ExportResource {
            request_id: 371,
            request: ExportResourceRequest {
                resource_id: ResourceId::new(9).expect("resource id"),
                handle_type: ExternalHandleType::OpaqueFd,
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn resource_exported_response_round_trip() {
        let message = WireMessage::ResourceExported {
            request_id: 372,
            response: sample_resource_exported(9),
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn image_export_metadata_round_trip() {
        let message = WireMessage::ResourceExported {
            request_id: 375,
            response: sample_image_resource_exported(13),
        };

        let decoded = decode_wire_message(&encode_wire_message(&message)).expect("decode export");
        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 375);
    }

    #[test]
    fn rejects_malformed_image_export_metadata() {
        let mut bytes = encode_wire_message(&WireMessage::ResourceExported {
            request_id: 376,
            response: sample_image_resource_exported(13),
        });
        bytes[WIRE_HEADER_LEN + 36..WIRE_HEADER_LEN + 40].copy_from_slice(&0x1_u32.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::MalformedPayload)
        );
    }

    #[test]
    fn rejects_truncated_image_payload() {
        let mut bytes = encode_wire_message(&WireMessage::CreateImage {
            request_id: 377,
            request: CreateImageRequest {
                desc: sample_image_desc(),
            },
        });
        bytes.truncate(bytes.len() - 1);

        assert!(matches!(
            decode_wire_message(&bytes),
            Err(ProtocolError::TruncatedPayload { .. })
        ));
    }

    #[test]
    fn rejects_zero_resource_id_in_export_request() {
        let mut bytes = encode_wire_message(&WireMessage::ExportResource {
            request_id: 373,
            request: ExportResourceRequest {
                resource_id: ResourceId::new(9).expect("resource id"),
                handle_type: ExternalHandleType::DmaBuf,
            },
        });
        bytes[WIRE_HEADER_LEN..WIRE_HEADER_LEN + 8].copy_from_slice(&0_u64.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidResourceId)
        );
    }

    #[test]
    fn preserves_export_attachment_metadata() {
        let mut response = sample_resource_exported(11);
        response.metadata.attachment_count = 1;
        let message = WireMessage::ResourceExported {
            request_id: 374,
            response,
        };

        let decoded = decode_wire_message(&encode_wire_message(&message)).expect("decode export");
        let WireMessage::ResourceExported { response, .. } = decoded else {
            panic!("expected RESOURCE_EXPORTED");
        };
        assert_eq!(response.metadata.attachment_count, 1);
    }

    #[test]
    fn rejects_zero_sized_buffer_payload() {
        let mut bytes = encode_wire_message(&WireMessage::CreateBuffer {
            request_id: 38,
            request: CreateBufferRequest {
                desc: sample_buffer_desc(),
            },
        });
        bytes[WIRE_HEADER_LEN + 8..WIRE_HEADER_LEN + 16].copy_from_slice(&0_u64.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidBufferSize {
                size: 0,
                max: MAX_BUFFER_SIZE_BYTES,
            })
        );
    }

    #[test]
    fn rejects_oversized_buffer_payload() {
        let mut bytes = encode_wire_message(&WireMessage::CreateBuffer {
            request_id: 39,
            request: CreateBufferRequest {
                desc: sample_buffer_desc(),
            },
        });
        bytes[WIRE_HEADER_LEN + 8..WIRE_HEADER_LEN + 16]
            .copy_from_slice(&(MAX_BUFFER_SIZE_BYTES + 1).to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidBufferSize {
                size: MAX_BUFFER_SIZE_BYTES + 1,
                max: MAX_BUFFER_SIZE_BYTES,
            })
        );
    }

    #[test]
    fn rejects_truncated_create_buffer_payload() {
        let mut bytes = encode_wire_message(&WireMessage::CreateBuffer {
            request_id: 40,
            request: CreateBufferRequest {
                desc: sample_buffer_desc(),
            },
        });
        bytes.truncate(bytes.len() - 1);

        assert!(matches!(
            decode_wire_message(&bytes),
            Err(ProtocolError::TruncatedPayload { .. })
        ));
    }

    #[test]
    fn rejects_invalid_buffer_usage_flags() {
        let mut bytes = encode_wire_message(&WireMessage::CreateBuffer {
            request_id: 41,
            request: CreateBufferRequest {
                desc: sample_buffer_desc(),
            },
        });
        bytes[WIRE_HEADER_LEN + 16..WIRE_HEADER_LEN + 20].copy_from_slice(&0x8_u32.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidBufferUsage { flags: 0x8 })
        );
    }

    #[test]
    fn rejects_zero_resource_id_in_destroy_request() {
        let mut bytes = encode_wire_message(&WireMessage::DestroyResource {
            request_id: 42,
            request: DestroyResourceRequest {
                resource_id: ResourceId::new(1).expect("resource id"),
            },
        });
        bytes[WIRE_HEADER_LEN..WIRE_HEADER_LEN + 8].copy_from_slice(&0_u64.to_le_bytes());

        assert_eq!(
            decode_wire_message(&bytes),
            Err(ProtocolError::InvalidResourceId)
        );
    }

    #[test]
    fn preserves_request_id_for_resource_messages() {
        let create = WireMessage::CreateBuffer {
            request_id: 43,
            request: CreateBufferRequest {
                desc: sample_buffer_desc(),
            },
        };
        let created = WireMessage::BufferCreated {
            request_id: 44,
            response: sample_buffer_created(10),
        };
        let destroy = WireMessage::DestroyResource {
            request_id: 45,
            request: DestroyResourceRequest {
                resource_id: ResourceId::new(10).expect("resource id"),
            },
        };
        let destroyed = WireMessage::ResourceDestroyed {
            request_id: 46,
            response: ResourceDestroyedResponse {
                resource_id: ResourceId::new(10).expect("resource id"),
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&create))
                .expect("create buffer")
                .request_id(),
            43
        );
        assert_eq!(
            decode_wire_message(&encode_wire_message(&created))
                .expect("buffer created")
                .request_id(),
            44
        );
        assert_eq!(
            decode_wire_message(&encode_wire_message(&destroy))
                .expect("destroy resource")
                .request_id(),
            45
        );
        assert_eq!(
            decode_wire_message(&encode_wire_message(&destroyed))
                .expect("resource destroyed")
                .request_id(),
            46
        );
    }

    #[test]
    fn resource_error_codes_are_stable() {
        for (error, code) in [
            (
                ProtocolError::InvalidResourceId,
                ProtocolErrorCode::InvalidResourceId,
            ),
            (
                ProtocolError::UnknownResource,
                ProtocolErrorCode::UnknownResource,
            ),
            (
                ProtocolError::InvalidBufferSize {
                    size: 0,
                    max: MAX_BUFFER_SIZE_BYTES,
                },
                ProtocolErrorCode::InvalidBufferSize,
            ),
            (
                ProtocolError::AllocationFailed,
                ProtocolErrorCode::AllocationFailed,
            ),
            (
                ProtocolError::UnsupportedMemoryRequirements,
                ProtocolErrorCode::UnsupportedMemoryRequirements,
            ),
            (
                ProtocolError::InvalidImageDimensions {
                    width: 0,
                    height: 64,
                },
                ProtocolErrorCode::InvalidImageDimensions,
            ),
            (
                ProtocolError::UnsupportedPixelFormat,
                ProtocolErrorCode::UnsupportedPixelFormat,
            ),
            (
                ProtocolError::UnsupportedImageUsage { flags: 0x8 },
                ProtocolErrorCode::UnsupportedImageUsage,
            ),
            (
                ProtocolError::UnsupportedImageExternalSharing,
                ProtocolErrorCode::UnsupportedImageExternalSharing,
            ),
            (
                ProtocolError::ResourceNotExportable,
                ProtocolErrorCode::ResourceNotExportable,
            ),
            (
                ProtocolError::UnsupportedExternalHandleType,
                ProtocolErrorCode::UnsupportedExternalHandleType,
            ),
            (ProtocolError::ExportFailed, ProtocolErrorCode::ExportFailed),
        ] {
            assert_eq!(ProtocolErrorCode::from(&error), code);
            assert_eq!(ProtocolErrorCode::try_from(code.wire_value()), Ok(code));
        }
    }

    #[test]
    fn video_codec_profile_and_surface_enums_round_trip() {
        for codec in [VideoCodec::H264, VideoCodec::Mpeg2] {
            assert_eq!(VideoCodec::try_from(codec.wire_value()), Ok(codec));
        }
        for profile in [
            H264Profile::Baseline,
            H264Profile::Main,
            H264Profile::High,
            H264Profile::High422,
        ] {
            assert_eq!(H264Profile::try_from(profile.wire_value()), Ok(profile));
        }
        for profile in [Mpeg2Profile::Main, Mpeg2Profile::Profile422] {
            assert_eq!(Mpeg2Profile::try_from(profile.wire_value()), Ok(profile));
        }
        for chroma in [
            ChromaSubsampling::Cs420,
            ChromaSubsampling::Cs422,
            ChromaSubsampling::Cs444,
        ] {
            assert_eq!(ChromaSubsampling::try_from(chroma.wire_value()), Ok(chroma));
        }
        for scan in [ScanMode::Progressive, ScanMode::Interlaced] {
            assert_eq!(ScanMode::try_from(scan.wire_value()), Ok(scan));
        }
        for field_order in [
            FieldOrder::Unknown,
            FieldOrder::TopFieldFirst,
            FieldOrder::BottomFieldFirst,
        ] {
            assert_eq!(
                FieldOrder::try_from(field_order.wire_value()),
                Ok(field_order)
            );
        }
        for format in [
            VideoSurfaceFormat::Nv12,
            VideoSurfaceFormat::P010,
            VideoSurfaceFormat::Yuv422_8,
            VideoSurfaceFormat::Yuv422_10,
        ] {
            assert_eq!(
                VideoSurfaceFormat::try_from(format.wire_value()),
                Ok(format)
            );
        }
    }

    #[test]
    fn video_unknown_enum_values_are_rejected() {
        assert_eq!(
            VideoCodec::try_from(99),
            Err(ProtocolError::UnsupportedVideoCodec)
        );
        assert_eq!(
            H264Profile::try_from(99),
            Err(ProtocolError::UnsupportedVideoProfile)
        );
        assert_eq!(
            ChromaSubsampling::try_from(99),
            Err(ProtocolError::UnsupportedChromaSubsampling)
        );
        assert_eq!(
            ScanMode::try_from(99),
            Err(ProtocolError::UnsupportedScanMode)
        );
        assert_eq!(
            FieldOrder::try_from(99),
            Err(ProtocolError::UnsupportedFieldOrder)
        );
        assert_eq!(
            VideoSurfaceFormat::try_from(99),
            Err(ProtocolError::UnsupportedVideoSurfaceFormat)
        );
    }

    #[test]
    fn bit_depth_validates_supported_values() {
        assert_eq!(BitDepth::new(8).expect("8-bit").get(), 8);
        assert_eq!(BitDepth::new(10).expect("10-bit").get(), 10);
        assert_eq!(BitDepth::new(12).expect("12-bit").get(), 12);
        assert_eq!(
            BitDepth::new(9),
            Err(ProtocolError::InvalidVideoBitDepth { bit_depth: 9 })
        );
    }

    #[test]
    fn video_surface_desc_validates_dimensions_and_crop() {
        let desc = sample_video_surface_desc();
        assert_eq!(desc.validate(), Ok(()));

        let mut zero_width = desc.clone();
        zero_width.coded_width = 0;
        assert_eq!(
            zero_width.validate(),
            Err(ProtocolError::InvalidVideoSurfaceDimensions {
                width: 0,
                height: 1080,
            })
        );

        let mut oversized = desc.clone();
        oversized.coded_height = MAX_VIDEO_SURFACE_HEIGHT + 1;
        assert_eq!(
            oversized.validate(),
            Err(ProtocolError::InvalidVideoSurfaceDimensions {
                width: 1920,
                height: MAX_VIDEO_SURFACE_HEIGHT + 1,
            })
        );

        let mut invalid_crop = desc.clone();
        invalid_crop.visible_region.x = 1900;
        invalid_crop.visible_region.width = 64;
        assert_eq!(
            invalid_crop.validate(),
            Err(ProtocolError::InvalidVideoVisibleRegion)
        );
    }

    #[test]
    fn video_surface_desc_rejects_inconsistent_format_semantics() {
        let mut desc = sample_video_surface_desc();
        desc.bit_depth = BitDepth::new(10).expect("10-bit");

        assert_eq!(desc.validate(), Err(ProtocolError::MalformedPayload));

        let mut progressive_with_field_order = sample_video_surface_desc();
        progressive_with_field_order.field_order = FieldOrder::TopFieldFirst;
        assert_eq!(
            progressive_with_field_order.validate(),
            Err(ProtocolError::MalformedPayload)
        );
    }

    #[test]
    fn resource_kind_video_surface_round_trips() {
        assert_eq!(ResourceKind::try_from(1), Ok(ResourceKind::Buffer));
        assert_eq!(ResourceKind::try_from(2), Ok(ResourceKind::Image));
        assert_eq!(ResourceKind::try_from(3), Ok(ResourceKind::VideoSurface));
    }

    #[test]
    fn video_capability_entry_and_list_round_trip() {
        let message = WireMessage::VideoCapabilities {
            request_id: 901,
            response: VideoCapabilitiesResponse {
                capabilities: VideoCapabilities {
                    device_id: DeviceId::new(1).expect("device id"),
                    decode: vec![sample_h264_8bit_420_capability()],
                },
            },
        };

        let decoded = decode_wire_message(&encode_wire_message(&message)).expect("decode video");
        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 901);
    }

    #[test]
    fn empty_video_capability_list_round_trips() {
        let message = WireMessage::VideoCapabilities {
            request_id: 902,
            response: VideoCapabilitiesResponse {
                capabilities: VideoCapabilities {
                    device_id: DeviceId::new(2).expect("device id"),
                    decode: Vec::new(),
                },
            },
        };

        assert_eq!(
            decode_wire_message(&encode_wire_message(&message)),
            Ok(message)
        );
    }

    #[test]
    fn query_video_capabilities_request_round_trips() {
        let message = WireMessage::QueryVideoCapabilities {
            request_id: 903,
            request: QueryVideoCapabilitiesRequest {
                device_id: DeviceId::new(5).expect("device id"),
            },
        };

        let decoded =
            decode_wire_message(&encode_wire_message(&message)).expect("decode video query");
        assert_eq!(decoded, message);
        assert_eq!(decoded.request_id(), 903);
    }

    #[test]
    fn video_capability_reference_models_are_representable() {
        assert_eq!(sample_h264_8bit_420_capability().validate(), Ok(()));
        assert_eq!(
            sample_mpeg2_8bit_422_interlaced_capability().validate(),
            Ok(())
        );
        assert_eq!(sample_h264_10bit_422_capability().validate(), Ok(()));
    }

    #[test]
    fn video_capability_rejects_mismatched_profile() {
        let mut capability = sample_h264_8bit_420_capability();
        capability.profile = VideoProfile::Mpeg2(Mpeg2Profile::Main);

        assert_eq!(
            capability.validate(),
            Err(ProtocolError::UnsupportedVideoProfile)
        );
    }

    #[test]
    fn video_capability_rejects_empty_output_formats() {
        let mut capability = sample_h264_8bit_420_capability();
        capability.output_surface_formats.clear();

        assert_eq!(
            capability.validate(),
            Err(ProtocolError::VideoOutputFormatCountTooLarge {
                count: 0,
                max: MAX_VIDEO_OUTPUT_FORMAT_COUNT,
            })
        );
    }

    #[test]
    fn rejects_excessive_video_capability_count() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1_u64.to_le_bytes());
        payload.extend_from_slice(&((MAX_VIDEO_DECODE_CAPABILITY_COUNT as u16) + 1).to_le_bytes());
        payload.extend_from_slice(&0_u16.to_le_bytes());

        assert_eq!(
            decode_video_capabilities_payload(&payload),
            Err(ProtocolError::VideoDecodeCapabilityCountTooLarge {
                count: MAX_VIDEO_DECODE_CAPABILITY_COUNT + 1,
                max: MAX_VIDEO_DECODE_CAPABILITY_COUNT,
            })
        );
    }

    #[test]
    fn rejects_excessive_video_output_format_count() {
        let mut payload = encode_video_capabilities_payload(&VideoCapabilitiesResponse {
            capabilities: VideoCapabilities {
                device_id: DeviceId::new(1).expect("device id"),
                decode: vec![sample_h264_8bit_420_capability()],
            },
        });
        payload[VIDEO_CAPABILITIES_PREFIX_LEN + 14] = (MAX_VIDEO_OUTPUT_FORMAT_COUNT as u8) + 1;

        assert_eq!(
            decode_video_capabilities_payload(&payload),
            Err(ProtocolError::VideoOutputFormatCountTooLarge {
                count: MAX_VIDEO_OUTPUT_FORMAT_COUNT + 1,
                max: MAX_VIDEO_OUTPUT_FORMAT_COUNT,
            })
        );
    }

    #[test]
    fn rejects_truncated_video_capability_entry() {
        let mut payload = encode_video_capabilities_payload(&VideoCapabilitiesResponse {
            capabilities: VideoCapabilities {
                device_id: DeviceId::new(1).expect("device id"),
                decode: vec![sample_h264_8bit_420_capability()],
            },
        });
        payload.truncate(payload.len() - 1);

        assert!(matches!(
            decode_video_capabilities_payload(&payload),
            Err(ProtocolError::TruncatedPayload { .. })
        ));
    }

    #[test]
    fn rejects_trailing_video_capability_payload() {
        let mut payload = encode_video_capabilities_payload(&VideoCapabilitiesResponse {
            capabilities: VideoCapabilities {
                device_id: DeviceId::new(1).expect("device id"),
                decode: Vec::new(),
            },
        });
        payload.push(0);

        assert_eq!(
            decode_video_capabilities_payload(&payload),
            Err(ProtocolError::TrailingPayload { len: 1 })
        );
    }

    #[test]
    fn rejects_invalid_video_capability_bit_depth_from_wire() {
        let mut payload = encode_video_capabilities_payload(&VideoCapabilitiesResponse {
            capabilities: VideoCapabilities {
                device_id: DeviceId::new(1).expect("device id"),
                decode: vec![sample_h264_8bit_420_capability()],
            },
        });
        payload[VIDEO_CAPABILITIES_PREFIX_LEN + 2] = 9;

        assert_eq!(
            decode_video_capabilities_payload(&payload),
            Err(ProtocolError::InvalidVideoBitDepth { bit_depth: 9 })
        );
    }

    #[test]
    fn video_error_codes_are_stable() {
        for (error, code) in [
            (
                ProtocolError::UnsupportedVideoCodec,
                ProtocolErrorCode::UnsupportedVideoCodec,
            ),
            (
                ProtocolError::UnsupportedVideoProfile,
                ProtocolErrorCode::UnsupportedVideoProfile,
            ),
            (
                ProtocolError::InvalidVideoBitDepth { bit_depth: 9 },
                ProtocolErrorCode::InvalidVideoBitDepth,
            ),
            (
                ProtocolError::UnsupportedChromaSubsampling,
                ProtocolErrorCode::UnsupportedChromaSubsampling,
            ),
            (
                ProtocolError::UnsupportedScanMode,
                ProtocolErrorCode::UnsupportedScanMode,
            ),
            (
                ProtocolError::UnsupportedFieldOrder,
                ProtocolErrorCode::UnsupportedFieldOrder,
            ),
            (
                ProtocolError::UnsupportedVideoSurfaceFormat,
                ProtocolErrorCode::UnsupportedVideoSurfaceFormat,
            ),
            (
                ProtocolError::InvalidVideoSurfaceDimensions {
                    width: 0,
                    height: 1080,
                },
                ProtocolErrorCode::InvalidVideoSurfaceDimensions,
            ),
            (
                ProtocolError::InvalidVideoVisibleRegion,
                ProtocolErrorCode::InvalidVideoVisibleRegion,
            ),
            (
                ProtocolError::VideoDecodeCapabilityCountTooLarge {
                    count: MAX_VIDEO_DECODE_CAPABILITY_COUNT + 1,
                    max: MAX_VIDEO_DECODE_CAPABILITY_COUNT,
                },
                ProtocolErrorCode::VideoDecodeCapabilityCountTooLarge,
            ),
            (
                ProtocolError::VideoOutputFormatCountTooLarge {
                    count: MAX_VIDEO_OUTPUT_FORMAT_COUNT + 1,
                    max: MAX_VIDEO_OUTPUT_FORMAT_COUNT,
                },
                ProtocolErrorCode::VideoOutputFormatCountTooLarge,
            ),
        ] {
            assert_eq!(ProtocolErrorCode::from(&error), code);
            assert_eq!(ProtocolErrorCode::try_from(code.wire_value()), Ok(code));
        }
    }

    fn sample_video_surface_desc() -> VideoSurfaceDesc {
        VideoSurfaceDesc {
            coded_width: 1920,
            coded_height: 1080,
            visible_region: VisibleRegion {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            format: VideoSurfaceFormat::Nv12,
            bit_depth: BitDepth::new(8).expect("bit depth"),
            chroma: ChromaSubsampling::Cs420,
            scan_mode: ScanMode::Progressive,
            field_order: FieldOrder::Unknown,
        }
    }

    fn sample_h264_8bit_420_capability() -> VideoDecodeCapability {
        VideoDecodeCapability {
            codec: VideoCodec::H264,
            profile: VideoProfile::H264(H264Profile::High),
            bit_depth: BitDepth::new(8).expect("bit depth"),
            chroma: ChromaSubsampling::Cs420,
            max_width: 1920,
            max_height: 1080,
            progressive_supported: true,
            interlaced_supported: false,
            output_surface_formats: vec![VideoSurfaceFormat::Nv12],
        }
    }

    fn sample_mpeg2_8bit_422_interlaced_capability() -> VideoDecodeCapability {
        VideoDecodeCapability {
            codec: VideoCodec::Mpeg2,
            profile: VideoProfile::Mpeg2(Mpeg2Profile::Profile422),
            bit_depth: BitDepth::new(8).expect("bit depth"),
            chroma: ChromaSubsampling::Cs422,
            max_width: 1920,
            max_height: 1080,
            progressive_supported: true,
            interlaced_supported: true,
            output_surface_formats: vec![VideoSurfaceFormat::Yuv422_8],
        }
    }

    fn sample_h264_10bit_422_capability() -> VideoDecodeCapability {
        VideoDecodeCapability {
            codec: VideoCodec::H264,
            profile: VideoProfile::H264(H264Profile::High422),
            bit_depth: BitDepth::new(10).expect("bit depth"),
            chroma: ChromaSubsampling::Cs422,
            max_width: 3840,
            max_height: 2160,
            progressive_supported: true,
            interlaced_supported: false,
            output_surface_formats: vec![VideoSurfaceFormat::Yuv422_10],
        }
    }

    fn sample_device(name: &str, class: DeviceClass, raw_id: u64) -> DeviceDesc {
        DeviceDesc {
            id: DeviceId::new(raw_id).expect("device id"),
            class,
            vendor_id: 0x1234,
            device_id: 0x5678,
            name: name.to_owned(),
            backend: BackendApi::Vulkan,
            api_version: ApiVersion::new(1, 3, 0),
            driver_version: 42,
        }
    }

    fn sample_capabilities(raw_id: u64) -> DeviceCapabilities {
        DeviceCapabilities {
            device_id: DeviceId::new(raw_id).expect("device id"),
            compute: ComputeCapabilities {
                supported: true,
                max_workgroup_count: [65_535, 65_535, 65_535],
                max_workgroup_size: [1024, 1024, 64],
                max_workgroup_invocations: 1024,
            },
            memory: MemoryCapabilities {
                heaps: vec![MemoryHeapDesc {
                    size_bytes: 256 * 1024 * 1024,
                    device_local: true,
                }],
                memory_type_count: 4,
                host_visible: true,
                host_coherent: true,
                device_local: true,
            },
            interop: InteropCapabilities {
                external_memory_fd: true,
                dma_buf: true,
                external_semaphore_fd: true,
                external_fence_fd: true,
            },
        }
    }

    fn sample_buffer_desc() -> BufferDesc {
        BufferDesc {
            device_id: DeviceId::new(1).expect("device id"),
            size_bytes: 1024 * 1024,
            usage: BufferUsageFlags::TRANSFER_SRC
                | BufferUsageFlags::TRANSFER_DST
                | BufferUsageFlags::STORAGE,
            memory_preference: MemoryPreference {
                device_preferred: true,
                host_visible_required: false,
                host_coherent_preferred: true,
            },
            external_sharing: ExternalSharing::None,
        }
    }

    fn sample_buffer_created(raw_id: u64) -> BufferCreatedResponse {
        BufferCreatedResponse {
            resource_id: ResourceId::new(raw_id).expect("resource id"),
            size_bytes: 1024 * 1024,
            selected_memory: SelectedMemoryProperties {
                device_local: true,
                host_visible: true,
                host_coherent: false,
            },
        }
    }

    fn sample_image_desc() -> ImageDesc {
        ImageDesc {
            device_id: DeviceId::new(1).expect("device id"),
            width: 64,
            height: 64,
            format: PixelFormat::Rgba8Unorm,
            usage: ImageUsageFlags::TRANSFER_SRC
                | ImageUsageFlags::TRANSFER_DST
                | ImageUsageFlags::STORAGE,
            external_sharing: ExternalSharing::Required {
                handle_type: ExternalHandleType::DmaBuf,
            },
        }
    }

    fn sample_image_created(raw_id: u64) -> ImageCreatedResponse {
        ImageCreatedResponse {
            resource_id: ResourceId::new(raw_id).expect("resource id"),
            width: 64,
            height: 64,
            format: PixelFormat::Rgba8Unorm,
            selected_memory: SelectedMemoryProperties {
                device_local: true,
                host_visible: false,
                host_coherent: false,
            },
        }
    }

    fn sample_resource_exported(raw_id: u64) -> ResourceExportedResponse {
        ResourceExportedResponse {
            metadata: ExportedResourceMetadata {
                resource_id: ResourceId::new(raw_id).expect("resource id"),
                device_id: DeviceId::new(1).expect("device id"),
                kind: ResourceKind::Buffer,
                size_bytes: 1024 * 1024,
                allocation_size_bytes: 1024 * 1024,
                buffer_usage: Some(BufferUsageFlags::TRANSFER_SRC | BufferUsageFlags::TRANSFER_DST),
                image_width: None,
                image_height: None,
                pixel_format: None,
                image_usage: None,
                backend_memory_type_index: 0,
                backend_image_layout_token: None,
                handle_type: ExternalHandleType::DmaBuf,
                selected_memory: SelectedMemoryProperties {
                    device_local: true,
                    host_visible: true,
                    host_coherent: true,
                },
                dedicated_allocation: false,
                attachment_count: 1,
            },
        }
    }

    fn sample_image_resource_exported(raw_id: u64) -> ResourceExportedResponse {
        ResourceExportedResponse {
            metadata: ExportedResourceMetadata {
                resource_id: ResourceId::new(raw_id).expect("resource id"),
                device_id: DeviceId::new(1).expect("device id"),
                kind: ResourceKind::Image,
                size_bytes: image_byte_len(64, 64, PixelFormat::Rgba8Unorm).expect("image bytes"),
                allocation_size_bytes: 64 * 64 * 4,
                buffer_usage: None,
                image_width: Some(64),
                image_height: Some(64),
                pixel_format: Some(PixelFormat::Rgba8Unorm),
                image_usage: Some(
                    ImageUsageFlags::TRANSFER_SRC
                        | ImageUsageFlags::TRANSFER_DST
                        | ImageUsageFlags::STORAGE,
                ),
                backend_memory_type_index: 0,
                backend_image_layout_token: Some(0x0102_0304_0506_0708),
                handle_type: ExternalHandleType::DmaBuf,
                selected_memory: SelectedMemoryProperties {
                    device_local: true,
                    host_visible: false,
                    host_coherent: false,
                },
                dedicated_allocation: true,
                attachment_count: 1,
            },
        }
    }
}

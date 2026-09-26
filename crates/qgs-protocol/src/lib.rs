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

const DEVICE_ENTRY_FIXED_LEN: usize = 32;
const DEVICE_CAPABILITIES_FIXED_PREFIX_LEN: usize = 52;
const MEMORY_HEAP_ENTRY_LEN: usize = 16;
const CREATE_BUFFER_PAYLOAD_LEN: usize = 24;
const BUFFER_CREATED_PAYLOAD_LEN: usize = 24;
const DESTROY_RESOURCE_PAYLOAD_LEN: usize = 8;
const RESOURCE_DESTROYED_PAYLOAD_LEN: usize = 8;

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
    CreateBuffer(CreateBufferRequest),
    DestroyResource(DestroyResourceRequest),
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
pub struct CreateBufferRequest {
    pub desc: BufferDesc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestroyResourceRequest {
    pub resource_id: ResourceId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Response {
    Welcome(WelcomeResponse),
    Error(ErrorResponse),
    DeviceList(DeviceListResponse),
    DeviceCapabilities(DeviceCapabilitiesResponse),
    BufferCreated(BufferCreatedResponse),
    ResourceDestroyed(ResourceDestroyedResponse),
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
}

impl ResourceKind {
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Buffer => 1,
        }
    }
}

impl TryFrom<u8> for ResourceKind {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Buffer),
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
pub struct ResourceDestroyedResponse {
    pub resource_id: ResourceId,
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
}

impl RequestOpcode {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Hello => 1,
            Self::EnumerateDevices => 2,
            Self::QueryDeviceCapabilities => 3,
            Self::CreateBuffer => 4,
            Self::DestroyResource => 5,
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
    CreateBuffer {
        request_id: u64,
        request: CreateBufferRequest,
    },
    DestroyResource {
        request_id: u64,
        request: DestroyResourceRequest,
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
    BufferCreated {
        request_id: u64,
        response: BufferCreatedResponse,
    },
    ResourceDestroyed {
        request_id: u64,
        response: ResourceDestroyedResponse,
    },
}

impl WireMessage {
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Hello { request_id, .. }
            | Self::EnumerateDevices { request_id }
            | Self::QueryDeviceCapabilities { request_id, .. }
            | Self::CreateBuffer { request_id, .. }
            | Self::DestroyResource { request_id, .. }
            | Self::Welcome { request_id, .. }
            | Self::Error { request_id, .. }
            | Self::DeviceList { request_id, .. }
            | Self::DeviceCapabilities { request_id, .. }
            | Self::BufferCreated { request_id, .. }
            | Self::ResourceDestroyed { request_id, .. } => *request_id,
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
        WireMessage::CreateBuffer {
            request_id,
            request,
        } => (
            MessageKind::Request,
            RequestOpcode::CreateBuffer.wire_value(),
            *request_id,
            encode_create_buffer_payload(request),
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
        WireMessage::BufferCreated {
            request_id,
            response,
        } => (
            MessageKind::Response,
            ResponseOpcode::BufferCreated.wire_value(),
            *request_id,
            encode_buffer_created_payload(response),
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
        (MessageKind::Request, 1..=5) | (MessageKind::Response, 1..=6) => Ok(()),
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

fn encode_create_buffer_payload(request: &CreateBufferRequest) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CREATE_BUFFER_PAYLOAD_LEN);
    bytes.extend_from_slice(&request.desc.device_id.get().to_le_bytes());
    bytes.extend_from_slice(&request.desc.size_bytes.to_le_bytes());
    bytes.extend_from_slice(&request.desc.usage.bits().to_le_bytes());
    bytes.push(encode_memory_preference(request.desc.memory_preference));
    bytes.extend_from_slice(&[0_u8; 3]);
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
    if bytes[21..24].iter().any(|value| *value != 0) {
        return Err(ProtocolError::MalformedPayload);
    }

    let desc = BufferDesc {
        device_id: DeviceId::new(read_u64(bytes, 0))?,
        size_bytes: read_u64(bytes, 8),
        usage: BufferUsageFlags::new(read_u32(bytes, 16))?,
        memory_preference: decode_memory_preference(bytes[20])?,
    };
    desc.validate()?;

    Ok(CreateBufferRequest { desc })
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
        ] {
            assert_eq!(ProtocolErrorCode::from(&error), code);
            assert_eq!(ProtocolErrorCode::try_from(code.wire_value()), Ok(code));
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
}

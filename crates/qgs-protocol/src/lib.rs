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

const DEVICE_ENTRY_FIXED_LEN: usize = 32;

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
pub enum Response {
    Welcome(WelcomeResponse),
    Error(ErrorResponse),
    DeviceList(DeviceListResponse),
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
}

impl RequestOpcode {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Hello => 1,
            Self::EnumerateDevices => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseOpcode {
    Welcome,
    Error,
    DeviceList,
}

impl ResponseOpcode {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Welcome => 1,
            Self::Error => 2,
            Self::DeviceList => 3,
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
}

impl WireMessage {
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Hello { request_id, .. }
            | Self::EnumerateDevices { request_id }
            | Self::Welcome { request_id, .. }
            | Self::Error { request_id, .. }
            | Self::DeviceList { request_id, .. } => *request_id,
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
        (MessageKind::Request, 1 | 2) | (MessageKind::Response, 1..=3) => Ok(()),
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
}

#![forbid(unsafe_code)]

use core::fmt;

/// Identifies QGS protocol messages.
///
/// This constant is part of the protocol model. It is not, by itself, a
/// complete wire-format definition.
pub const PROTOCOL_MAGIC: u32 = 0x5147_5300;

/// The protocol version implemented by this crate.
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::new(0, 1);

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
/// These Rust in-memory types are intentionally not the future wire format.
/// A later transport must define byte order, alignment, framing, validation,
/// and compatibility rules explicitly instead of serializing these structs.
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    Hello(HelloRequest),
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
pub enum Response {
    Welcome(WelcomeResponse),
    Error(ProtocolError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WelcomeResponse {
    pub version: ProtocolVersion,
    pub session_id: SessionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    SessionClosed { session_id: SessionId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidMagic {
        actual: u32,
    },
    InvalidSessionId,
    SessionIdsExhausted,
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
            Self::SessionIdsExhausted => write!(f, "session ids are exhausted"),
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

pub fn validate_header(header: &MessageHeader) -> Result<(), ProtocolError> {
    if header.magic == PROTOCOL_MAGIC {
        Ok(())
    } else {
        Err(ProtocolError::InvalidMagic {
            actual: header.magic,
        })
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
}

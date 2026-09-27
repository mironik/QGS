#![forbid(unsafe_code)]

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use qgs_protocol::{
    handle_hello, BufferCreatedResponse, BufferDesc, CreateDecoderRequest, CreateSyncRequest,
    DecodeOutputResponse, DecoderCreatedResponse, DecoderDestroyedResponse, DecoderId,
    DestroyDecoderRequest, DeviceCapabilities, DeviceDesc, DeviceId, ExportResourceRequest,
    ExportSyncRequest, ExportedResourceMetadata, ExportedSyncMetadata, HelloRequest,
    ImageCreatedResponse, ImageDesc, ProtocolError, ResourceDestroyedResponse, ResourceId,
    ResourceKind, SelectedMemoryProperties, SessionId, SubmitAccessUnitRequest,
    SyncCreatedResponse, SyncId, VideoCapabilities, VideoSurfaceDesc, WelcomeResponse,
};

pub trait DeviceDiscovery {
    fn enumerate_devices(&self) -> Result<Vec<DeviceDesc>, DeviceDiscoveryError>;

    fn query_device_capabilities(
        &self,
        device_id: DeviceId,
    ) -> Result<DeviceCapabilities, DeviceDiscoveryError>;
}

pub trait VideoCapabilityDiscovery {
    fn query_video_capabilities(
        &self,
        device_id: DeviceId,
    ) -> Result<VideoCapabilities, VideoCapabilityDiscoveryError>;
}

#[derive(Debug)]
pub enum VideoCapabilityDiscoveryError {
    BackendUnavailable,
    BackendFailed,
    UnknownDeviceId,
    Protocol(ProtocolError),
}

impl std::fmt::Display for VideoCapabilityDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendUnavailable => write!(f, "video capability backend is unavailable"),
            Self::BackendFailed => write!(f, "video capability backend failed"),
            Self::UnknownDeviceId => write!(f, "unknown device id"),
            Self::Protocol(err) => {
                write!(
                    f,
                    "video capability backend produced invalid protocol data: {err}"
                )
            }
        }
    }
}

impl std::error::Error for VideoCapabilityDiscoveryError {}

impl From<ProtocolError> for VideoCapabilityDiscoveryError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

#[derive(Debug)]
pub enum DeviceDiscoveryError {
    BackendUnavailable,
    BackendFailed,
    UnknownDeviceId,
    Protocol(ProtocolError),
}

impl std::fmt::Display for DeviceDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendUnavailable => write!(f, "device discovery backend is unavailable"),
            Self::BackendFailed => write!(f, "device discovery backend failed"),
            Self::UnknownDeviceId => write!(f, "unknown device id"),
            Self::Protocol(err) => {
                write!(f, "device discovery produced invalid protocol data: {err}")
            }
        }
    }
}

impl std::error::Error for DeviceDiscoveryError {}

impl From<ProtocolError> for DeviceDiscoveryError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

pub trait ResourceBackend {
    fn create_buffer(&self, desc: &BufferDesc) -> Result<BackendBufferAllocation, ResourceError>;

    fn create_image(&self, desc: &ImageDesc) -> Result<BackendImageAllocation, ResourceError>;
}

pub trait BackendResource {
    fn as_any(&self) -> &dyn Any;

    fn export(
        &self,
        _request: &ExportResourceRequest,
    ) -> Result<BackendResourceExport, ResourceError> {
        Err(ResourceError::ResourceNotExportable)
    }
}

pub trait SyncBackend {
    fn create_sync(&self, request: &CreateSyncRequest) -> Result<Box<dyn BackendSync>, SyncError>;
}

pub trait DecoderBackend {
    fn create_decoder(
        &self,
        request: &CreateDecoderRequest,
    ) -> Result<Box<dyn BackendDecoder>, DecoderError>;
}

pub trait BackendSync {
    fn export_for_resource(
        &self,
        _request: &ExportSyncRequest,
        _resource: &dyn BackendResource,
    ) -> Result<BackendSyncExport, SyncError> {
        Err(SyncError::SyncExportFailed)
    }
}

pub trait BackendDecoder {
    fn submit_access_unit(
        &mut self,
        request: &SubmitAccessUnitRequest,
    ) -> Result<BackendDecodedSurface, DecoderError>;
}

pub struct BackendBufferAllocation {
    pub resource: Box<dyn BackendResource>,
    pub selected_memory: SelectedMemoryProperties,
}

pub struct BackendImageAllocation {
    pub resource: Box<dyn BackendResource>,
    pub selected_memory: SelectedMemoryProperties,
}

#[derive(Debug)]
pub struct BackendResourceExport {
    pub metadata: ExportedResourceMetadata,
    pub handle: std::fs::File,
}

#[derive(Debug)]
pub struct BackendSyncExport {
    pub metadata: ExportedSyncMetadata,
    pub handle: std::fs::File,
}

pub struct BackendDecodedSurface {
    pub resource: Box<dyn BackendResource>,
    pub desc: VideoSurfaceDesc,
}

#[derive(Debug)]
pub enum ResourceError {
    UnknownResource,
    InvalidBufferSize,
    InvalidImageDimensions,
    AllocationFailed,
    UnsupportedMemoryRequirements,
    UnsupportedPixelFormat,
    UnsupportedImageUsage,
    UnsupportedImageExternalSharing,
    ResourceNotExportable,
    UnsupportedExternalHandleType,
    ExportFailed,
    UnknownDeviceId,
    Protocol(ProtocolError),
}

#[derive(Debug)]
pub enum SyncError {
    UnknownSync,
    UnknownResource,
    UnknownDeviceId,
    UnsupportedSyncHandleType,
    SyncExportFailed,
    Protocol(ProtocolError),
}

#[derive(Debug)]
pub enum DecoderError {
    UnknownDecoder,
    UnknownDeviceId,
    UnsupportedDecodeConfiguration,
    MalformedCompressedData,
    CompressedPacketTooLarge,
    UnsupportedH264StreamFeature,
    DecodeFailed,
    Protocol(ProtocolError),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSync => write!(f, "unknown sync id"),
            Self::UnknownResource => write!(f, "unknown resource id"),
            Self::UnknownDeviceId => write!(f, "unknown device id"),
            Self::UnsupportedSyncHandleType => write!(f, "unsupported sync handle type"),
            Self::SyncExportFailed => write!(f, "sync export failed"),
            Self::Protocol(err) => write!(f, "invalid sync protocol data: {err}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<ProtocolError> for SyncError {
    fn from(value: ProtocolError) -> Self {
        match value {
            ProtocolError::UnknownSync => Self::UnknownSync,
            ProtocolError::UnknownResource => Self::UnknownResource,
            ProtocolError::UnknownDeviceId => Self::UnknownDeviceId,
            ProtocolError::UnsupportedSyncHandleType => Self::UnsupportedSyncHandleType,
            ProtocolError::SyncExportFailed => Self::SyncExportFailed,
            err => Self::Protocol(err),
        }
    }
}

impl std::fmt::Display for DecoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDecoder => write!(f, "unknown decoder"),
            Self::UnknownDeviceId => write!(f, "unknown device id"),
            Self::UnsupportedDecodeConfiguration => write!(f, "unsupported decode configuration"),
            Self::MalformedCompressedData => write!(f, "malformed compressed data"),
            Self::CompressedPacketTooLarge => write!(f, "compressed packet too large"),
            Self::UnsupportedH264StreamFeature => write!(f, "unsupported H.264 stream feature"),
            Self::DecodeFailed => write!(f, "decode failed"),
            Self::Protocol(err) => write!(f, "invalid decoder protocol data: {err}"),
        }
    }
}

impl std::error::Error for DecoderError {}

impl From<ProtocolError> for DecoderError {
    fn from(value: ProtocolError) -> Self {
        match value {
            ProtocolError::UnknownDecoder => Self::UnknownDecoder,
            ProtocolError::UnknownDeviceId => Self::UnknownDeviceId,
            ProtocolError::UnsupportedDecodeConfiguration => Self::UnsupportedDecodeConfiguration,
            ProtocolError::MalformedCompressedData => Self::MalformedCompressedData,
            ProtocolError::CompressedPacketTooLarge { .. } => Self::CompressedPacketTooLarge,
            ProtocolError::UnsupportedH264StreamFeature => Self::UnsupportedH264StreamFeature,
            ProtocolError::DecodeFailed => Self::DecodeFailed,
            err => Self::Protocol(err),
        }
    }
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownResource => write!(f, "unknown resource id"),
            Self::InvalidBufferSize => write!(f, "invalid buffer size"),
            Self::InvalidImageDimensions => write!(f, "invalid image dimensions"),
            Self::AllocationFailed => write!(f, "resource allocation failed"),
            Self::UnsupportedMemoryRequirements => write!(f, "unsupported memory requirements"),
            Self::UnsupportedPixelFormat => write!(f, "unsupported pixel format"),
            Self::UnsupportedImageUsage => write!(f, "unsupported image usage"),
            Self::UnsupportedImageExternalSharing => {
                write!(f, "unsupported image external sharing")
            }
            Self::ResourceNotExportable => write!(f, "resource is not exportable"),
            Self::UnsupportedExternalHandleType => write!(f, "unsupported external handle type"),
            Self::ExportFailed => write!(f, "resource export failed"),
            Self::UnknownDeviceId => write!(f, "unknown device id"),
            Self::Protocol(err) => write!(f, "invalid resource protocol data: {err}"),
        }
    }
}

impl std::error::Error for ResourceError {}

impl From<ProtocolError> for ResourceError {
    fn from(value: ProtocolError) -> Self {
        match value {
            ProtocolError::InvalidBufferSize { .. } => Self::InvalidBufferSize,
            ProtocolError::InvalidImageDimensions { .. } => Self::InvalidImageDimensions,
            ProtocolError::UnknownDeviceId => Self::UnknownDeviceId,
            ProtocolError::UnknownResource => Self::UnknownResource,
            ProtocolError::AllocationFailed => Self::AllocationFailed,
            ProtocolError::UnsupportedMemoryRequirements => Self::UnsupportedMemoryRequirements,
            ProtocolError::UnsupportedPixelFormat => Self::UnsupportedPixelFormat,
            ProtocolError::UnsupportedImageUsage { .. } => Self::UnsupportedImageUsage,
            ProtocolError::UnsupportedImageExternalSharing => Self::UnsupportedImageExternalSharing,
            ProtocolError::ResourceNotExportable => Self::ResourceNotExportable,
            ProtocolError::UnsupportedExternalHandleType => Self::UnsupportedExternalHandleType,
            ProtocolError::ExportFailed => Self::ExportFailed,
            err => Self::Protocol(err),
        }
    }
}

#[derive(Debug)]
pub struct SessionManager {
    next_session_id: AtomicU64,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self {
            next_session_id: AtomicU64::new(1),
        }
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_session(&self) -> Result<Session, ProtocolError> {
        let raw = self
            .next_session_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1).filter(|_| next != u64::MAX)
            })
            .map_err(|_| ProtocolError::SessionIdsExhausted)?;
        let id = SessionId::new(raw)?;
        Ok(Session {
            id,
            resources: ResourceRegistry::new(),
            syncs: SyncRegistry::new(),
            decoders: DecoderRegistry::new(),
        })
    }

    pub fn handle_hello(&self, request: &HelloRequest) -> Result<WelcomeResponse, ProtocolError> {
        let session = self.create_session()?;
        handle_hello(request, session.id())
    }
}

pub struct Session {
    id: SessionId,
    resources: ResourceRegistry,
    syncs: SyncRegistry,
    decoders: DecoderRegistry,
}

impl Session {
    pub const fn id(&self) -> SessionId {
        self.id
    }

    pub fn create_buffer(
        &mut self,
        backend: &impl ResourceBackend,
        desc: &BufferDesc,
    ) -> Result<BufferCreatedResponse, ResourceError> {
        desc.validate()?;
        let id = self.resources.allocate_id()?;
        let allocation = backend.create_buffer(desc)?;
        self.resources.insert(
            id,
            ResourceEntry {
                kind: ResourceKind::Buffer,
                resource: allocation.resource,
            },
        );

        Ok(BufferCreatedResponse {
            resource_id: id,
            size_bytes: desc.size_bytes,
            selected_memory: allocation.selected_memory,
        })
    }

    pub fn create_image(
        &mut self,
        backend: &impl ResourceBackend,
        desc: &ImageDesc,
    ) -> Result<ImageCreatedResponse, ResourceError> {
        desc.validate()?;
        let id = self.resources.allocate_id()?;
        let allocation = backend.create_image(desc)?;
        self.resources.insert(
            id,
            ResourceEntry {
                kind: ResourceKind::Image,
                resource: allocation.resource,
            },
        );

        Ok(ImageCreatedResponse {
            resource_id: id,
            width: desc.width,
            height: desc.height,
            format: desc.format,
            selected_memory: allocation.selected_memory,
        })
    }

    pub fn destroy_resource(
        &mut self,
        resource_id: ResourceId,
    ) -> Result<ResourceDestroyedResponse, ResourceError> {
        self.resources.remove(resource_id)?;
        Ok(ResourceDestroyedResponse { resource_id })
    }

    pub fn export_resource(
        &self,
        request: &ExportResourceRequest,
    ) -> Result<BackendResourceExport, ResourceError> {
        self.resources
            .get(request.resource_id)?
            .resource
            .export(request)
    }

    pub fn create_sync(
        &mut self,
        backend: &impl SyncBackend,
        request: &CreateSyncRequest,
    ) -> Result<SyncCreatedResponse, SyncError> {
        let id = self.syncs.allocate_id()?;
        let sync = backend.create_sync(request)?;
        self.syncs.insert(id, SyncEntry { sync });

        Ok(SyncCreatedResponse { sync_id: id })
    }

    pub fn export_sync(&self, request: &ExportSyncRequest) -> Result<BackendSyncExport, SyncError> {
        let sync = self.syncs.get(request.sync_id)?;
        let resource = self
            .resources
            .get(request.resource_id)
            .map_err(|_| SyncError::UnknownResource)?;

        sync.sync
            .export_for_resource(request, resource.resource.as_ref())
    }

    pub fn create_decoder(
        &mut self,
        backend: &impl DecoderBackend,
        request: &CreateDecoderRequest,
    ) -> Result<DecoderCreatedResponse, DecoderError> {
        request.config.validate()?;
        let id = self.decoders.allocate_id()?;
        let decoder = backend.create_decoder(request)?;
        self.decoders.insert(id, DecoderEntry { decoder });
        Ok(DecoderCreatedResponse { decoder_id: id })
    }

    pub fn submit_access_unit(
        &mut self,
        request: &SubmitAccessUnitRequest,
    ) -> Result<DecodeOutputResponse, DecoderError> {
        if request.data.len() > qgs_protocol::MAX_COMPRESSED_DECODE_PACKET_BYTES {
            return Err(DecoderError::CompressedPacketTooLarge);
        }
        let decoded = self
            .decoders
            .get_mut(request.decoder_id)?
            .decoder
            .submit_access_unit(request)?;
        decoded.desc.validate()?;
        let resource_id = self
            .resources
            .allocate_id()
            .map_err(|_| DecoderError::DecodeFailed)?;
        self.resources.insert(
            resource_id,
            ResourceEntry {
                kind: ResourceKind::VideoSurface,
                resource: decoded.resource,
            },
        );
        Ok(DecodeOutputResponse {
            decoder_id: request.decoder_id,
            resource_id,
            surface: decoded.desc,
        })
    }

    pub fn destroy_decoder(
        &mut self,
        request: &DestroyDecoderRequest,
    ) -> Result<DecoderDestroyedResponse, DecoderError> {
        self.decoders.remove(request.decoder_id)?;
        Ok(DecoderDestroyedResponse {
            decoder_id: request.decoder_id,
        })
    }

    pub fn resource_count(&self) -> usize {
        self.resources.len()
    }

    pub fn sync_count(&self) -> usize {
        self.syncs.len()
    }

    pub fn decoder_count(&self) -> usize {
        self.decoders.len()
    }
}

struct ResourceRegistry {
    next_resource_id: u64,
    resources: BTreeMap<ResourceId, ResourceEntry>,
}

impl ResourceRegistry {
    fn new() -> Self {
        Self {
            next_resource_id: 1,
            resources: BTreeMap::new(),
        }
    }

    fn allocate_id(&mut self) -> Result<ResourceId, ResourceError> {
        let id = ResourceId::new(self.next_resource_id)?;
        self.next_resource_id = self
            .next_resource_id
            .checked_add(1)
            .ok_or(ResourceError::AllocationFailed)?;
        Ok(id)
    }

    fn insert(&mut self, id: ResourceId, entry: ResourceEntry) {
        self.resources.insert(id, entry);
    }

    fn remove(&mut self, id: ResourceId) -> Result<ResourceEntry, ResourceError> {
        self.resources
            .remove(&id)
            .ok_or(ResourceError::UnknownResource)
    }

    fn get(&self, id: ResourceId) -> Result<&ResourceEntry, ResourceError> {
        self.resources
            .get(&id)
            .ok_or(ResourceError::UnknownResource)
    }

    fn len(&self) -> usize {
        self.resources.len()
    }
}

impl Drop for ResourceRegistry {
    fn drop(&mut self) {
        if !self.resources.is_empty() {
            eprintln!(
                "qgs-core: releasing {} resource(s) owned by session",
                self.resources.len()
            );
        }
    }
}

struct ResourceEntry {
    #[allow(dead_code)]
    kind: ResourceKind,
    #[allow(dead_code)]
    resource: Box<dyn BackendResource>,
}

struct SyncRegistry {
    next_sync_id: u64,
    syncs: BTreeMap<SyncId, SyncEntry>,
}

impl SyncRegistry {
    fn new() -> Self {
        Self {
            next_sync_id: 1,
            syncs: BTreeMap::new(),
        }
    }

    fn allocate_id(&mut self) -> Result<SyncId, SyncError> {
        let id = SyncId::new(self.next_sync_id)?;
        self.next_sync_id = self
            .next_sync_id
            .checked_add(1)
            .ok_or(SyncError::SyncExportFailed)?;
        Ok(id)
    }

    fn insert(&mut self, id: SyncId, entry: SyncEntry) {
        self.syncs.insert(id, entry);
    }

    fn get(&self, id: SyncId) -> Result<&SyncEntry, SyncError> {
        self.syncs.get(&id).ok_or(SyncError::UnknownSync)
    }

    fn len(&self) -> usize {
        self.syncs.len()
    }
}

impl Drop for SyncRegistry {
    fn drop(&mut self) {
        if !self.syncs.is_empty() {
            eprintln!(
                "qgs-core: releasing {} sync object(s) owned by session",
                self.syncs.len()
            );
        }
    }
}

struct SyncEntry {
    sync: Box<dyn BackendSync>,
}

struct DecoderRegistry {
    next_decoder_id: u64,
    decoders: BTreeMap<DecoderId, DecoderEntry>,
}

impl DecoderRegistry {
    fn new() -> Self {
        Self {
            next_decoder_id: 1,
            decoders: BTreeMap::new(),
        }
    }

    fn allocate_id(&mut self) -> Result<DecoderId, DecoderError> {
        let id = DecoderId::new(self.next_decoder_id)?;
        self.next_decoder_id = self
            .next_decoder_id
            .checked_add(1)
            .ok_or(DecoderError::DecodeFailed)?;
        Ok(id)
    }

    fn insert(&mut self, id: DecoderId, entry: DecoderEntry) {
        self.decoders.insert(id, entry);
    }

    fn get_mut(&mut self, id: DecoderId) -> Result<&mut DecoderEntry, DecoderError> {
        self.decoders
            .get_mut(&id)
            .ok_or(DecoderError::UnknownDecoder)
    }

    fn remove(&mut self, id: DecoderId) -> Result<DecoderEntry, DecoderError> {
        self.decoders
            .remove(&id)
            .ok_or(DecoderError::UnknownDecoder)
    }

    fn len(&self) -> usize {
        self.decoders.len()
    }
}

impl Drop for DecoderRegistry {
    fn drop(&mut self) {
        if !self.decoders.is_empty() {
            eprintln!(
                "qgs-core: releasing {} decoder(s) owned by session",
                self.decoders.len()
            );
        }
    }
}

struct DecoderEntry {
    decoder: Box<dyn BackendDecoder>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    use qgs_protocol::{
        BufferUsageFlags, CreateSyncRequest, DeviceId, ExportResourceRequest, ExportSyncRequest,
        ExternalHandleType, ExternalSharing, ImageUsageFlags, MemoryPreference, PixelFormat,
        SyncExportHandleType, SyncKind, MAX_BUFFER_SIZE_BYTES,
    };

    #[test]
    fn creates_unique_sessions() {
        let sessions = SessionManager::new();

        let first = sessions.create_session().expect("first session");
        let second = sessions.create_session().expect("second session");

        assert_ne!(first.id(), second.id());
        assert_eq!(first.id().get(), 1);
        assert_eq!(second.id().get(), 2);
    }

    #[test]
    fn refuses_to_wrap_session_ids_to_zero() {
        let sessions = SessionManager {
            next_session_id: AtomicU64::new(u64::MAX),
        };

        let err = match sessions.create_session() {
            Ok(_) => panic!("session ids should be exhausted before wrapping"),
            Err(err) => err,
        };

        assert_eq!(err, ProtocolError::SessionIdsExhausted);
        assert_eq!(sessions.next_session_id.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn create_buffer_returns_valid_nonzero_resource_id() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");

        let created = session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("buffer created");

        assert_ne!(created.resource_id.get(), 0);
        assert_eq!(session.resource_count(), 1);
    }

    #[test]
    fn destroy_buffer_removes_resource() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");
        let created = session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("buffer created");

        session
            .destroy_resource(created.resource_id)
            .expect("resource destroyed");

        assert_eq!(session.resource_count(), 0);
        assert_eq!(backend.drop_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn create_image_returns_valid_nonzero_resource_id() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");

        let created = session
            .create_image(&backend, &sample_image_desc())
            .expect("image created");

        assert_ne!(created.resource_id.get(), 0);
        assert_eq!(session.resource_count(), 1);
    }

    #[test]
    fn destroy_image_removes_resource() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");
        let created = session
            .create_image(&backend, &sample_image_desc())
            .expect("image created");

        session
            .destroy_resource(created.resource_id)
            .expect("resource destroyed");

        assert_eq!(session.resource_count(), 0);
        assert_eq!(backend.drop_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn zero_width_image_is_rejected_before_backend_allocation() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");
        let mut desc = sample_image_desc();
        desc.width = 0;

        let err = session
            .create_image(&backend, &desc)
            .expect_err("zero-width image is invalid");

        assert!(matches!(err, ResourceError::InvalidImageDimensions));
        assert_eq!(backend.create_image_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn destroy_unknown_resource_fails() {
        let mut session = SessionManager::new().create_session().expect("session");

        let err = session
            .destroy_resource(ResourceId::new(99).expect("resource id"))
            .expect_err("resource is unknown");

        assert!(matches!(err, ResourceError::UnknownResource));
    }

    #[test]
    fn zero_sized_buffer_is_rejected_before_backend_allocation() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");
        let mut desc = sample_buffer_desc();
        desc.size_bytes = 0;

        let err = session
            .create_buffer(&backend, &desc)
            .expect_err("zero-sized buffer is invalid");

        assert!(matches!(err, ResourceError::InvalidBufferSize));
        assert_eq!(backend.create_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn oversized_buffer_is_rejected_before_backend_allocation() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");
        let mut desc = sample_buffer_desc();
        desc.size_bytes = MAX_BUFFER_SIZE_BYTES + 1;

        let err = session
            .create_buffer(&backend, &desc)
            .expect_err("oversized buffer is invalid");

        assert!(matches!(err, ResourceError::InvalidBufferSize));
        assert_eq!(backend.create_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn multiple_resources_receive_distinct_ids() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");

        let first = session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("first buffer");
        let second = session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("second buffer");

        assert_ne!(first.resource_id, second.resource_id);
        assert_eq!(session.resource_count(), 2);
    }

    #[test]
    fn resources_belong_to_their_session() {
        let backend = MockBackend::default();
        let sessions = SessionManager::new();
        let mut first_session = sessions.create_session().expect("first session");
        let second_session = sessions.create_session().expect("second session");

        let created = first_session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("buffer created");

        assert_eq!(first_session.id().get(), 1);
        assert_eq!(second_session.id().get(), 2);
        assert_eq!(created.resource_id.get(), 1);
        assert_eq!(first_session.resource_count(), 1);
        assert_eq!(second_session.resource_count(), 0);
    }

    #[test]
    fn session_cannot_destroy_another_sessions_resource() {
        let backend = MockBackend::default();
        let sessions = SessionManager::new();
        let mut first_session = sessions.create_session().expect("first session");
        let mut second_session = sessions.create_session().expect("second session");
        let created = first_session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("buffer created");

        let err = second_session
            .destroy_resource(created.resource_id)
            .expect_err("resource belongs to another session");

        assert!(matches!(err, ResourceError::UnknownResource));
        assert_eq!(first_session.resource_count(), 1);
    }

    #[test]
    fn session_cannot_destroy_another_sessions_image() {
        let backend = MockBackend::default();
        let sessions = SessionManager::new();
        let mut first_session = sessions.create_session().expect("first session");
        let mut second_session = sessions.create_session().expect("second session");
        let created = first_session
            .create_image(&backend, &sample_image_desc())
            .expect("image created");

        let err = second_session
            .destroy_resource(created.resource_id)
            .expect_err("image belongs to another session");

        assert!(matches!(err, ResourceError::UnknownResource));
        assert_eq!(first_session.resource_count(), 1);
    }

    #[test]
    fn non_exportable_resource_export_is_rejected() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");
        let created = session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("buffer created");

        let err = session
            .export_resource(&ExportResourceRequest {
                resource_id: created.resource_id,
                handle_type: ExternalHandleType::DmaBuf,
            })
            .expect_err("mock resource is not exportable");

        assert!(matches!(err, ResourceError::ResourceNotExportable));
    }

    #[test]
    fn session_cannot_export_another_sessions_resource() {
        let backend = MockBackend::default();
        let sessions = SessionManager::new();
        let mut first_session = sessions.create_session().expect("first session");
        let second_session = sessions.create_session().expect("second session");
        let created = first_session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("buffer created");

        let err = second_session
            .export_resource(&ExportResourceRequest {
                resource_id: created.resource_id,
                handle_type: ExternalHandleType::DmaBuf,
            })
            .expect_err("resource belongs to another session");

        assert!(matches!(err, ResourceError::UnknownResource));
    }

    #[test]
    fn dropping_session_releases_all_owned_resources() {
        let backend = MockBackend::default();
        {
            let mut session = SessionManager::new().create_session().expect("session");
            session
                .create_buffer(&backend, &sample_buffer_desc())
                .expect("first buffer");
            session
                .create_buffer(&backend, &sample_buffer_desc())
                .expect("second buffer");
            assert_eq!(session.resource_count(), 2);
        }

        assert_eq!(backend.drop_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn dropping_session_releases_owned_images() {
        let backend = MockBackend::default();
        {
            let mut session = SessionManager::new().create_session().expect("session");
            session
                .create_image(&backend, &sample_image_desc())
                .expect("first image");
            session
                .create_image(&backend, &sample_image_desc())
                .expect("second image");
            assert_eq!(session.resource_count(), 2);
        }

        assert_eq!(backend.drop_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn video_surface_resource_kind_uses_session_registry_model() {
        let backend = MockBackend::default();
        let mut registry = ResourceRegistry::new();
        let id = registry.allocate_id().expect("resource id");

        registry.insert(
            id,
            ResourceEntry {
                kind: ResourceKind::VideoSurface,
                resource: Box::new(MockResource {
                    drop_count: Arc::clone(&backend.drop_count),
                }),
            },
        );

        assert_eq!(registry.len(), 1);
        let removed = registry.remove(id).expect("resource removed");
        assert_eq!(removed.kind, ResourceKind::VideoSurface);
        drop(removed);
        assert_eq!(backend.drop_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn client_disconnect_session_drop_releases_resources() {
        let backend = MockBackend::default();
        simulate_client_disconnect(&backend);

        assert_eq!(backend.drop_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn client_disconnect_session_drop_releases_images() {
        let backend = MockBackend::default();
        {
            let mut session = SessionManager::new().create_session().expect("session");
            for _ in 0..3 {
                session
                    .create_image(&backend, &sample_image_desc())
                    .expect("image created");
            }
        }

        assert_eq!(backend.drop_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn create_sync_returns_unique_nonzero_ids() {
        let backend = MockBackend::default();
        let mut session = SessionManager::new().create_session().expect("session");

        let first = session
            .create_sync(&backend, &sample_sync_request())
            .expect("first sync");
        let second = session
            .create_sync(&backend, &sample_sync_request())
            .expect("second sync");

        assert_ne!(first.sync_id.get(), 0);
        assert_ne!(first.sync_id, second.sync_id);
        assert_eq!(session.sync_count(), 2);
    }

    #[test]
    fn unknown_sync_export_is_rejected() {
        let session = SessionManager::new().create_session().expect("session");

        let err = session
            .export_sync(&ExportSyncRequest {
                sync_id: SyncId::new(99).expect("sync id"),
                resource_id: ResourceId::new(1).expect("resource id"),
                fill_pattern: 1,
            })
            .expect_err("sync is unknown");

        assert!(matches!(err, SyncError::UnknownSync));
    }

    #[test]
    fn session_cannot_export_another_sessions_sync() {
        let backend = MockBackend::default();
        let sessions = SessionManager::new();
        let mut first_session = sessions.create_session().expect("first session");
        let mut second_session = sessions.create_session().expect("second session");
        let created = first_session
            .create_sync(&backend, &sample_sync_request())
            .expect("sync created");
        let buffer = second_session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("buffer created");

        let err = second_session
            .export_sync(&ExportSyncRequest {
                sync_id: created.sync_id,
                resource_id: buffer.resource_id,
                fill_pattern: 1,
            })
            .expect_err("sync belongs to another session");

        assert!(matches!(err, SyncError::UnknownSync));
    }

    #[test]
    fn session_cannot_export_sync_for_another_sessions_resource() {
        let backend = MockBackend::default();
        let sessions = SessionManager::new();
        let mut first_session = sessions.create_session().expect("first session");
        let mut second_session = sessions.create_session().expect("second session");
        let sync = first_session
            .create_sync(&backend, &sample_sync_request())
            .expect("sync created");
        let buffer = second_session
            .create_buffer(&backend, &sample_buffer_desc())
            .expect("buffer created");

        let err = first_session
            .export_sync(&ExportSyncRequest {
                sync_id: sync.sync_id,
                resource_id: buffer.resource_id,
                fill_pattern: 1,
            })
            .expect_err("resource belongs to another session");

        assert!(matches!(err, SyncError::UnknownResource));
    }

    #[test]
    fn dropping_session_releases_owned_syncs() {
        let backend = MockBackend::default();
        {
            let mut session = SessionManager::new().create_session().expect("session");
            session
                .create_sync(&backend, &sample_sync_request())
                .expect("first sync");
            session
                .create_sync(&backend, &sample_sync_request())
                .expect("second sync");
            assert_eq!(session.sync_count(), 2);
        }

        assert_eq!(backend.sync_drop_count.load(Ordering::Relaxed), 2);
    }

    fn simulate_client_disconnect(backend: &MockBackend) {
        let mut session = SessionManager::new().create_session().expect("session");
        for _ in 0..3 {
            session
                .create_buffer(backend, &sample_buffer_desc())
                .expect("buffer created");
        }
    }

    fn sample_buffer_desc() -> BufferDesc {
        BufferDesc {
            device_id: DeviceId::new(1).expect("device id"),
            size_bytes: 1024,
            usage: BufferUsageFlags::TRANSFER_SRC | BufferUsageFlags::TRANSFER_DST,
            memory_preference: MemoryPreference::device_preferred(),
            external_sharing: ExternalSharing::None,
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
            external_sharing: ExternalSharing::None,
        }
    }

    fn sample_sync_request() -> CreateSyncRequest {
        CreateSyncRequest {
            device_id: DeviceId::new(1).expect("device id"),
            kind: SyncKind::BinarySemaphore,
            handle_type: SyncExportHandleType::SyncFd,
        }
    }

    #[derive(Default)]
    struct MockBackend {
        create_count: Arc<AtomicUsize>,
        create_image_count: Arc<AtomicUsize>,
        drop_count: Arc<AtomicUsize>,
        sync_drop_count: Arc<AtomicUsize>,
    }

    impl ResourceBackend for MockBackend {
        fn create_buffer(
            &self,
            _desc: &BufferDesc,
        ) -> Result<BackendBufferAllocation, ResourceError> {
            self.create_count.fetch_add(1, Ordering::Relaxed);
            Ok(BackendBufferAllocation {
                resource: Box::new(MockResource {
                    drop_count: Arc::clone(&self.drop_count),
                }),
                selected_memory: SelectedMemoryProperties {
                    device_local: true,
                    host_visible: true,
                    host_coherent: true,
                },
            })
        }

        fn create_image(&self, _desc: &ImageDesc) -> Result<BackendImageAllocation, ResourceError> {
            self.create_image_count.fetch_add(1, Ordering::Relaxed);
            Ok(BackendImageAllocation {
                resource: Box::new(MockResource {
                    drop_count: Arc::clone(&self.drop_count),
                }),
                selected_memory: SelectedMemoryProperties {
                    device_local: true,
                    host_visible: false,
                    host_coherent: false,
                },
            })
        }
    }

    impl SyncBackend for MockBackend {
        fn create_sync(
            &self,
            _request: &CreateSyncRequest,
        ) -> Result<Box<dyn BackendSync>, SyncError> {
            Ok(Box::new(MockSync {
                drop_count: Arc::clone(&self.sync_drop_count),
            }))
        }
    }

    struct MockResource {
        drop_count: Arc<AtomicUsize>,
    }

    impl BackendResource for MockResource {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    impl Drop for MockResource {
        fn drop(&mut self) {
            self.drop_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    struct MockSync {
        drop_count: Arc<AtomicUsize>,
    }

    impl BackendSync for MockSync {}

    impl Drop for MockSync {
        fn drop(&mut self) {
            self.drop_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

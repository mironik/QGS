#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use qgs_protocol::{
    handle_hello, BufferCreatedResponse, BufferDesc, DeviceCapabilities, DeviceDesc, DeviceId,
    HelloRequest, ProtocolError, ResourceDestroyedResponse, ResourceId, ResourceKind,
    SelectedMemoryProperties, SessionId, WelcomeResponse,
};

pub trait DeviceDiscovery {
    fn enumerate_devices(&self) -> Result<Vec<DeviceDesc>, DeviceDiscoveryError>;

    fn query_device_capabilities(
        &self,
        device_id: DeviceId,
    ) -> Result<DeviceCapabilities, DeviceDiscoveryError>;
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
}

pub trait BackendResource: Send {}

impl<T: Send> BackendResource for T {}

pub struct BackendBufferAllocation {
    pub resource: Box<dyn BackendResource>,
    pub selected_memory: SelectedMemoryProperties,
}

#[derive(Debug)]
pub enum ResourceError {
    UnknownResource,
    InvalidBufferSize,
    AllocationFailed,
    UnsupportedMemoryRequirements,
    UnknownDeviceId,
    Protocol(ProtocolError),
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownResource => write!(f, "unknown resource id"),
            Self::InvalidBufferSize => write!(f, "invalid buffer size"),
            Self::AllocationFailed => write!(f, "resource allocation failed"),
            Self::UnsupportedMemoryRequirements => write!(f, "unsupported memory requirements"),
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
            ProtocolError::UnknownDeviceId => Self::UnknownDeviceId,
            ProtocolError::UnknownResource => Self::UnknownResource,
            ProtocolError::AllocationFailed => Self::AllocationFailed,
            ProtocolError::UnsupportedMemoryRequirements => Self::UnsupportedMemoryRequirements,
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

    pub fn destroy_resource(
        &mut self,
        resource_id: ResourceId,
    ) -> Result<ResourceDestroyedResponse, ResourceError> {
        self.resources.remove(resource_id)?;
        Ok(ResourceDestroyedResponse { resource_id })
    }

    pub fn resource_count(&self) -> usize {
        self.resources.len()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    use qgs_protocol::{BufferUsageFlags, DeviceId, MemoryPreference, MAX_BUFFER_SIZE_BYTES};

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
    fn client_disconnect_session_drop_releases_resources() {
        let backend = MockBackend::default();
        simulate_client_disconnect(&backend);

        assert_eq!(backend.drop_count.load(Ordering::Relaxed), 3);
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
        }
    }

    #[derive(Default)]
    struct MockBackend {
        create_count: Arc<AtomicUsize>,
        drop_count: Arc<AtomicUsize>,
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
    }

    struct MockResource {
        drop_count: Arc<AtomicUsize>,
    }

    impl Drop for MockResource {
        fn drop(&mut self) {
            self.drop_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

use std::any::Any;
use std::ffi::CStr;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ash::vk;
use qgs_core::{
    BackendBufferAllocation, BackendImageAllocation, BackendResource, BackendResourceExport,
    BackendSync, BackendSyncExport, DeviceDiscovery, DeviceDiscoveryError, ResourceBackend,
    ResourceError, SyncBackend, SyncError, VideoCapabilityDiscovery, VideoCapabilityDiscoveryError,
};
use qgs_protocol::{
    ApiVersion, BackendApi, BufferDesc, BufferUsageFlags, ComputeCapabilities, CreateSyncRequest,
    DeviceCapabilities, DeviceClass, DeviceDesc, DeviceId, ExportResourceRequest,
    ExportSyncRequest, ExportedResourceMetadata, ExportedSyncMetadata, ExternalHandleType,
    ExternalSharing, ImageDesc, ImageUsageFlags, InteropCapabilities, MemoryCapabilities,
    MemoryHeapDesc, PixelFormat, ResourceKind, SelectedMemoryProperties, SyncExportHandleType,
    SyncKind, VideoCapabilities, MAX_DEVICE_COUNT, MAX_DEVICE_NAME_LEN, MAX_MEMORY_HEAP_COUNT,
    MAX_MEMORY_TYPE_COUNT,
};

mod haswell_video_diagnostic;

pub use haswell_video_diagnostic::{
    diagnose_haswell_video_import, DiagnosticDrmLayer, DiagnosticDrmObject, DiagnosticDrmPlane,
    HaswellVideoDiagnosticInput, HaswellVideoDiagnosticReport,
};

const SHARED_VALIDATION_MARKER: &[u8] = b"QGS-M1S6";
const COMPUTE_LOCAL_SIZE_X: u32 = 64;
const COMPUTE_INCREMENT_SHADER: [u32; 147] = [
    119734787, 65536, 0, 22, 0, 131089, 1, 720906, 1599492179, 1599227979, 1919906931, 1600481121,
    1717990754, 1935635045, 1634889588, 1667196263, 1936941420, 0, 196622, 0, 1, 393231, 5, 1,
    1852399981, 0, 9, 393232, 1, 17, 64, 1, 1, 196611, 2, 450, 262215, 9, 11, 28, 262215, 10, 6, 4,
    327752, 11, 0, 35, 0, 196679, 11, 2, 262215, 13, 34, 0, 262215, 13, 33, 0, 131091, 2, 196641,
    3, 2, 262165, 4, 32, 0, 262187, 4, 5, 1, 262187, 4, 6, 0, 262167, 7, 4, 3, 262176, 8, 1, 7,
    262203, 8, 9, 1, 196637, 10, 4, 196638, 11, 10, 262176, 12, 12, 11, 262203, 12, 13, 12, 262176,
    14, 1, 4, 262176, 15, 12, 4, 327734, 2, 1, 0, 3, 131320, 16, 327745, 14, 17, 9, 6, 262205, 4,
    18, 17, 393281, 15, 19, 13, 6, 18, 262205, 4, 20, 19, 327808, 4, 21, 20, 5, 196670, 19, 21,
    65789, 65592,
];
const IMAGE_INVERT_SHADER: [u32; 208] = [
    119734787, 65536, 0, 35, 0, 131089, 1, 196622, 0, 1, 393231, 5, 3, 1852399981, 0, 11, 393232,
    3, 17, 8, 8, 1, 196611, 2, 450, 262149, 3, 1852399981, 0, 524293, 11, 1197436007, 1633841004,
    1986939244, 1952539503, 1231974249, 68, 196613, 14, 6778217, 262215, 11, 11, 28, 262215, 14,
    34, 0, 262215, 14, 33, 0, 131091, 1, 196641, 2, 1, 262165, 4, 32, 0, 262165, 5, 32, 1, 196630,
    6, 32, 262167, 7, 4, 3, 262167, 8, 5, 2, 262167, 9, 6, 4, 262176, 10, 1, 7, 262203, 10, 11, 1,
    589849, 12, 6, 1, 0, 0, 0, 2, 4, 262176, 13, 0, 12, 262203, 13, 14, 0, 262187, 4, 15, 0,
    262187, 4, 16, 1, 262187, 6, 17, 1065353216, 327734, 1, 3, 0, 2, 131320, 18, 262205, 7, 19, 11,
    327761, 4, 20, 19, 0, 327761, 4, 21, 19, 1, 262268, 5, 22, 20, 262268, 5, 23, 21, 327760, 8,
    24, 22, 23, 262205, 12, 25, 14, 327778, 9, 26, 25, 24, 327761, 6, 27, 26, 0, 327761, 6, 28, 26,
    1, 327761, 6, 29, 26, 2, 327761, 6, 30, 26, 3, 327816, 6, 31, 17, 27, 327816, 6, 32, 17, 28,
    327816, 6, 33, 17, 29, 458832, 9, 34, 31, 32, 33, 30, 262243, 25, 24, 34, 65789, 65592,
];

type BackendResult<T> = Result<T, BackendError>;

#[derive(Clone, Copy, Debug)]
enum BackendError {
    Vk(vk::Result),
    NoMemoryType,
    InvalidInput,
}

impl From<vk::Result> for BackendError {
    fn from(value: vk::Result) -> Self {
        Self::Vk(value)
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vk(result) => write!(f, "vulkan error {result:?}"),
            Self::NoMemoryType => write!(f, "no compatible Vulkan memory type"),
            Self::InvalidInput => write!(f, "invalid Vulkan backend input"),
        }
    }
}

#[derive(Debug)]
pub struct VulkanDeviceDiscovery {
    _instance: Arc<GpuInstance>,
    devices: Vec<RegisteredDevice>,
}

impl VulkanDeviceDiscovery {
    pub fn new() -> Result<Self, DeviceDiscoveryError> {
        let instance = Arc::new(GpuInstance::new().map_err(|err| {
            eprintln!("vulkan instance creation failed: {err:?}");
            DeviceDiscoveryError::BackendUnavailable
        })?);
        let physical_devices = instance.enumerate_physical_devices().map_err(|err| {
            eprintln!("vulkan physical-device enumeration failed: {err:?}");
            DeviceDiscoveryError::BackendFailed
        })?;

        let mut devices = Vec::new();
        for (index, physical_device) in physical_devices.into_iter().enumerate() {
            if devices.len() >= MAX_DEVICE_COUNT {
                break;
            }

            let properties = instance.physical_device_properties(physical_device);
            let memory_properties = instance.physical_device_memory_properties(physical_device);
            let queue_family_properties =
                instance.physical_device_queue_family_properties(physical_device);
            let queue_family_index = select_queue_family(&queue_family_properties)
                .ok_or(DeviceDiscoveryError::BackendFailed)?;
            let desc = describe_device((index as u64) + 1, properties)?;
            let extension_support = DeviceExtensionSupport::query(&instance, physical_device)
                .map_err(|err| {
                    eprintln!("vulkan extension query failed: {err:?}");
                    DeviceDiscoveryError::BackendFailed
                })?;
            let device = Arc::new(
                GpuDevice::new(
                    instance.clone(),
                    physical_device,
                    properties,
                    memory_properties,
                    queue_family_properties,
                    queue_family_index,
                    extension_support,
                )
                .map_err(|err| {
                    eprintln!("vulkan logical-device creation failed: {err:?}");
                    DeviceDiscoveryError::BackendFailed
                })?,
            );

            devices.push(RegisteredDevice { desc, device });
        }

        Ok(Self {
            _instance: instance,
            devices,
        })
    }

    pub fn import_and_validate_external_buffer(
        &self,
        source_device: &DeviceDesc,
        metadata: &ExportedResourceMetadata,
        handle: File,
    ) -> Result<(), ResourceError> {
        if metadata.device_id != source_device.id
            || metadata.kind != ResourceKind::Buffer
            || metadata.size_bytes == 0
            || metadata.allocation_size_bytes < metadata.size_bytes
            || metadata.attachment_count != 1
            || !metadata.selected_memory.host_visible
            || !metadata.selected_memory.host_coherent
        {
            return Err(ResourceError::ExportFailed);
        }

        let device = self.match_export_device(source_device)?;
        let usage = map_buffer_usage(metadata.buffer_usage.ok_or(ResourceError::ExportFailed)?)?;
        let handle_type = map_external_memory_handle_type(metadata.handle_type)?;
        validate_external_buffer_support(&device, usage, handle_type)?;
        let imported = import_external_buffer(&device, metadata, usage, handle_type, handle)?;
        let read = imported.read_bytes(0, SHARED_VALIDATION_MARKER.len())?;

        if read.as_slice() == SHARED_VALIDATION_MARKER {
            Ok(())
        } else {
            Err(ResourceError::ExportFailed)
        }
    }

    pub fn import_wait_and_validate_synced_buffer(
        &self,
        source_device: &DeviceDesc,
        resource_metadata: &ExportedResourceMetadata,
        resource_handle: File,
        sync_metadata: &ExportedSyncMetadata,
        sync_handle: File,
    ) -> Result<(), ResourceError> {
        if sync_metadata.handle_type != SyncExportHandleType::SyncFd
            || sync_metadata.attachment_count != 1
            || resource_metadata.device_id != source_device.id
            || resource_metadata.kind != ResourceKind::Buffer
            || resource_metadata.size_bytes == 0
            || resource_metadata.allocation_size_bytes < resource_metadata.size_bytes
            || resource_metadata.attachment_count != 1
        {
            return Err(ResourceError::ExportFailed);
        }

        let device = self.match_export_device(source_device)?;
        let usage = map_buffer_usage(
            resource_metadata
                .buffer_usage
                .ok_or(ResourceError::ExportFailed)?,
        )?;
        let handle_type = map_external_memory_handle_type(resource_metadata.handle_type)?;
        validate_external_buffer_support(&device, usage, handle_type)?;
        validate_sync_fd_support(&device).map_err(|_| ResourceError::ExportFailed)?;

        let imported_buffer = import_external_buffer(
            &device,
            resource_metadata,
            usage,
            handle_type,
            resource_handle,
        )?;
        let imported_semaphore = GpuSemaphore::new(device.clone(), false).map_err(|err| {
            eprintln!("vulkan sync semaphore creation failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        imported_semaphore
            .import_sync_fd(sync_handle)
            .map_err(|err| {
                eprintln!("vulkan sync-fd import failed: {err:?}");
                ResourceError::ExportFailed
            })?;

        validate_synced_gpu_copy(
            &device,
            &imported_buffer,
            &imported_semaphore,
            sync_metadata.fill_pattern,
        )
    }

    pub fn import_wait_and_run_compute_increment_proof(
        &self,
        source_device: &DeviceDesc,
        resource_metadata: &ExportedResourceMetadata,
        resource_handle: File,
        sync_metadata: &ExportedSyncMetadata,
        sync_handle: File,
        input_values: &[u32],
    ) -> Result<Vec<u32>, ResourceError> {
        if input_values.is_empty()
            || !input_values
                .len()
                .is_multiple_of(COMPUTE_LOCAL_SIZE_X as usize)
        {
            return Err(ResourceError::InvalidBufferSize);
        }
        let input_bytes = u64::try_from(input_values.len())
            .ok()
            .and_then(|len| len.checked_mul(std::mem::size_of::<u32>() as u64))
            .ok_or(ResourceError::InvalidBufferSize)?;
        if input_bytes > resource_metadata.size_bytes
            || sync_metadata.handle_type != SyncExportHandleType::SyncFd
            || sync_metadata.attachment_count != 1
            || resource_metadata.device_id != source_device.id
            || resource_metadata.kind != ResourceKind::Buffer
            || resource_metadata.attachment_count != 1
            || !resource_metadata
                .buffer_usage
                .ok_or(ResourceError::ExportFailed)?
                .contains(BufferUsageFlags::STORAGE)
        {
            return Err(ResourceError::ExportFailed);
        }

        let device = self.match_export_device(source_device)?;
        if !device.queue_supports_compute() {
            return Err(ResourceError::UnsupportedMemoryRequirements);
        }
        let usage = map_buffer_usage(
            resource_metadata
                .buffer_usage
                .ok_or(ResourceError::ExportFailed)?,
        )?;
        let handle_type = map_external_memory_handle_type(resource_metadata.handle_type)?;
        validate_external_buffer_support(&device, usage, handle_type)?;
        validate_sync_fd_support(&device).map_err(|_| ResourceError::ExportFailed)?;

        let imported_buffer = import_external_buffer(
            &device,
            resource_metadata,
            usage,
            handle_type,
            resource_handle,
        )?;
        let imported_semaphore = GpuSemaphore::new(device.clone(), false).map_err(|err| {
            eprintln!("vulkan sync semaphore creation failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        imported_semaphore
            .import_sync_fd(sync_handle)
            .map_err(|err| {
                eprintln!("vulkan sync-fd import failed: {err:?}");
                ResourceError::ExportFailed
            })?;

        run_compute_increment_proof(
            &device,
            &imported_buffer,
            &imported_semaphore,
            input_values,
            input_bytes,
        )
    }

    pub fn import_wait_and_run_image_invert_proof(
        &self,
        source_device: &DeviceDesc,
        resource_metadata: &ExportedResourceMetadata,
        resource_handle: File,
        sync_metadata: &ExportedSyncMetadata,
        sync_handle: File,
        input_pixels: &[u8],
    ) -> Result<Vec<u8>, ResourceError> {
        if sync_metadata.handle_type != SyncExportHandleType::SyncFd
            || sync_metadata.attachment_count != 1
            || resource_metadata.kind != ResourceKind::Image
            || resource_metadata.device_id != source_device.id
            || resource_metadata.attachment_count != 1
        {
            return Err(ResourceError::ExportFailed);
        }
        let width = resource_metadata
            .image_width
            .ok_or(ResourceError::ExportFailed)?;
        let height = resource_metadata
            .image_height
            .ok_or(ResourceError::ExportFailed)?;
        if !width.is_multiple_of(8) || !height.is_multiple_of(8) {
            return Err(ResourceError::InvalidImageDimensions);
        }
        let format = resource_metadata
            .pixel_format
            .ok_or(ResourceError::ExportFailed)?;
        let image_usage = resource_metadata
            .image_usage
            .ok_or(ResourceError::ExportFailed)?;
        let expected_len =
            qgs_protocol::image_byte_len(width, height, format).map_err(ResourceError::from)?;
        if input_pixels.len() as u64 != expected_len {
            return Err(ResourceError::InvalidImageDimensions);
        }

        let device = self.match_export_device(source_device)?;
        if !device.queue_supports_compute() {
            return Err(ResourceError::UnsupportedMemoryRequirements);
        }
        let vk_format = map_pixel_format(format)?;
        let usage = map_image_usage(image_usage)?;
        let handle_type = map_external_memory_handle_type(resource_metadata.handle_type)?;
        let tiling = if resource_metadata.handle_type == ExternalHandleType::DmaBuf {
            vk::ImageTiling::LINEAR
        } else {
            vk::ImageTiling::OPTIMAL
        };
        validate_external_image_support(&device, vk_format, usage, tiling, handle_type)?;
        validate_sync_fd_support(&device).map_err(|_| ResourceError::ExportFailed)?;

        let imported_image = import_external_image(
            &device,
            ImportedImageInfo {
                metadata: resource_metadata,
                format: vk_format,
                usage,
                tiling,
                handle_type,
                handle: resource_handle,
            },
        )?;
        let imported_semaphore = GpuSemaphore::new(device.clone(), false).map_err(|err| {
            eprintln!("vulkan sync semaphore creation failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        imported_semaphore
            .import_sync_fd(sync_handle)
            .map_err(|err| {
                eprintln!("vulkan sync-fd import failed: {err:?}");
                ResourceError::ExportFailed
            })?;

        run_image_invert_proof(&device, &imported_image, &imported_semaphore, input_pixels)
    }

    fn match_export_device(
        &self,
        source_device: &DeviceDesc,
    ) -> Result<Arc<GpuDevice>, ResourceError> {
        self.devices
            .iter()
            .find(|device| device_matches_export_source(&device.desc, source_device))
            .map(|device| device.device.clone())
            .ok_or(ResourceError::UnknownDeviceId)
    }
}

impl DeviceDiscovery for VulkanDeviceDiscovery {
    fn enumerate_devices(&self) -> Result<Vec<DeviceDesc>, DeviceDiscoveryError> {
        Ok(self
            .devices
            .iter()
            .map(|device| device.desc.clone())
            .collect())
    }

    fn query_device_capabilities(
        &self,
        device_id: DeviceId,
    ) -> Result<DeviceCapabilities, DeviceDiscoveryError> {
        let device = self
            .devices
            .iter()
            .find(|device| device.desc.id == device_id)
            .ok_or(DeviceDiscoveryError::UnknownDeviceId)?;

        Ok(describe_capabilities(device.desc.id, &device.device))
    }
}

impl VideoCapabilityDiscovery for VulkanDeviceDiscovery {
    fn query_video_capabilities(
        &self,
        device_id: DeviceId,
    ) -> Result<VideoCapabilities, VideoCapabilityDiscoveryError> {
        self.devices
            .iter()
            .find(|device| device.desc.id == device_id)
            .ok_or(VideoCapabilityDiscoveryError::UnknownDeviceId)?;

        Ok(VideoCapabilities {
            device_id,
            decode: Vec::new(),
        })
    }
}

impl ResourceBackend for VulkanDeviceDiscovery {
    fn create_buffer(&self, desc: &BufferDesc) -> Result<BackendBufferAllocation, ResourceError> {
        desc.validate()?;
        let device = self
            .devices
            .iter()
            .find(|device| device.desc.id == desc.device_id)
            .ok_or(ResourceError::UnknownDeviceId)?
            .device
            .clone();
        let usage = map_buffer_usage(desc.usage)?;
        let external_handle_type = match desc.external_sharing {
            ExternalSharing::None => None,
            ExternalSharing::Required { handle_type } => {
                let handle_type = map_external_memory_handle_type(handle_type)?;
                validate_external_buffer_support(&device, usage, handle_type)?;
                Some(handle_type)
            }
        };
        let buffer = GpuBuffer::new(
            device.clone(),
            desc.size_bytes,
            usage,
            external_handle_type,
            required_memory_flags(desc),
            preferred_memory_flags(desc),
            false,
        )?;
        let selected_memory = selected_memory_properties(&device, buffer.memory_type_index)?;
        Ok(BackendBufferAllocation {
            resource: Box::new(VulkanBufferResource {
                buffer,
                desc: desc.clone(),
                selected_memory,
            }),
            selected_memory,
        })
    }

    fn create_image(&self, desc: &ImageDesc) -> Result<BackendImageAllocation, ResourceError> {
        desc.validate()?;
        let device = self
            .devices
            .iter()
            .find(|device| device.desc.id == desc.device_id)
            .ok_or(ResourceError::UnknownDeviceId)?
            .device
            .clone();
        let format = map_pixel_format(desc.format)?;
        let usage = map_image_usage(desc.usage)?;
        let (external_handle_type, tiling) = match desc.external_sharing {
            ExternalSharing::None => (None, vk::ImageTiling::OPTIMAL),
            ExternalSharing::Required { handle_type } => {
                let handle_type = map_external_memory_handle_type(handle_type)?;
                let tiling = if handle_type == vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT {
                    vk::ImageTiling::LINEAR
                } else {
                    vk::ImageTiling::OPTIMAL
                };
                validate_external_image_support(&device, format, usage, tiling, handle_type)?;
                (Some(handle_type), tiling)
            }
        };
        let image = GpuImage::new(
            device.clone(),
            ImageCreateParams {
                format,
                extent: [desc.width, desc.height, 1],
                usage,
                tiling,
                external_handle_type,
                required: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                preferred: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            },
        )?;
        let selected_memory = selected_memory_properties(&device, image.memory_type_index)?;
        Ok(BackendImageAllocation {
            resource: Box::new(VulkanImageResource {
                image,
                desc: desc.clone(),
                selected_memory,
            }),
            selected_memory,
        })
    }
}

impl SyncBackend for VulkanDeviceDiscovery {
    fn create_sync(&self, request: &CreateSyncRequest) -> Result<Box<dyn BackendSync>, SyncError> {
        if request.kind != SyncKind::BinarySemaphore {
            return Err(SyncError::SyncExportFailed);
        }
        if request.handle_type != SyncExportHandleType::SyncFd {
            return Err(SyncError::UnsupportedSyncHandleType);
        }

        let device = self
            .devices
            .iter()
            .find(|device| device.desc.id == request.device_id)
            .ok_or(SyncError::UnknownDeviceId)?
            .device
            .clone();
        validate_sync_fd_support(&device)?;
        let semaphore = GpuSemaphore::new(device, true).map_err(|err| {
            eprintln!("vulkan sync semaphore creation failed: {err:?}");
            SyncError::SyncExportFailed
        })?;

        Ok(Box::new(VulkanSyncResource {
            device_id: request.device_id,
            handle_type: request.handle_type,
            semaphore,
            exported: AtomicBool::new(false),
            pending: Mutex::new(Vec::new()),
        }))
    }
}

#[derive(Debug)]
struct RegisteredDevice {
    desc: DeviceDesc,
    device: Arc<GpuDevice>,
}

#[derive(Debug)]
struct VulkanBufferResource {
    buffer: GpuBuffer,
    desc: BufferDesc,
    selected_memory: SelectedMemoryProperties,
}

#[derive(Debug)]
struct VulkanImageResource {
    image: GpuImage,
    desc: ImageDesc,
    selected_memory: SelectedMemoryProperties,
}

#[derive(Debug)]
struct VulkanSyncResource {
    device_id: DeviceId,
    handle_type: SyncExportHandleType,
    semaphore: GpuSemaphore,
    exported: AtomicBool,
    pending: Mutex<Vec<PendingSubmission>>,
}

impl Drop for VulkanSyncResource {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.clear();
        }
    }
}

impl BackendSync for VulkanSyncResource {
    fn export_for_resource(
        &self,
        request: &ExportSyncRequest,
        resource: &dyn BackendResource,
    ) -> Result<BackendSyncExport, SyncError> {
        if self.handle_type != SyncExportHandleType::SyncFd {
            return Err(SyncError::UnsupportedSyncHandleType);
        }
        if self.exported.swap(true, Ordering::AcqRel) {
            return Err(SyncError::SyncExportFailed);
        }

        let pending = if let Some(buffer) = resource.as_any().downcast_ref::<VulkanBufferResource>()
        {
            if buffer.desc.device_id != self.device_id {
                return Err(SyncError::UnknownResource);
            }
            submit_gpu_fill(&buffer.buffer, &self.semaphore, request.fill_pattern)?
        } else if let Some(image) = resource.as_any().downcast_ref::<VulkanImageResource>() {
            if image.desc.device_id != self.device_id {
                return Err(SyncError::UnknownResource);
            }
            submit_gpu_image_clear(&image.image, &self.semaphore, request.fill_pattern)?
        } else {
            return Err(SyncError::SyncExportFailed);
        };
        self.pending
            .lock()
            .map_err(|_| SyncError::SyncExportFailed)?
            .push(pending);

        let handle = self.semaphore.export_sync_fd().map_err(|err| {
            eprintln!("vulkan sync-fd export failed: {err:?}");
            SyncError::SyncExportFailed
        })?;

        Ok(BackendSyncExport {
            metadata: ExportedSyncMetadata {
                sync_id: request.sync_id,
                handle_type: self.handle_type,
                attachment_count: 1,
                fill_pattern: request.fill_pattern,
            },
            handle,
        })
    }
}

impl BackendResource for VulkanBufferResource {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn export(
        &self,
        request: &ExportResourceRequest,
    ) -> Result<BackendResourceExport, ResourceError> {
        let ExternalSharing::Required { handle_type } = self.desc.external_sharing else {
            return Err(ResourceError::ResourceNotExportable);
        };
        if request.handle_type != handle_type {
            return Err(ResourceError::UnsupportedExternalHandleType);
        }

        self.buffer.write_bytes(0, SHARED_VALIDATION_MARKER)?;
        let vk_handle_type = map_external_memory_handle_type(handle_type)?;
        let handle = self.buffer.memory.export_fd(vk_handle_type)?;

        Ok(BackendResourceExport {
            metadata: ExportedResourceMetadata {
                resource_id: request.resource_id,
                device_id: self.desc.device_id,
                kind: ResourceKind::Buffer,
                size_bytes: self.desc.size_bytes,
                allocation_size_bytes: self.buffer.memory.size,
                buffer_usage: Some(self.desc.usage),
                image_width: None,
                image_height: None,
                pixel_format: None,
                image_usage: None,
                backend_memory_type_index: self.buffer.memory_type_index,
                backend_image_layout_token: None,
                handle_type,
                selected_memory: self.selected_memory,
                dedicated_allocation: true,
                attachment_count: 1,
            },
            handle,
        })
    }
}

impl BackendResource for VulkanImageResource {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn export(
        &self,
        request: &ExportResourceRequest,
    ) -> Result<BackendResourceExport, ResourceError> {
        let ExternalSharing::Required { handle_type } = self.desc.external_sharing else {
            return Err(ResourceError::ResourceNotExportable);
        };
        if request.handle_type != handle_type {
            return Err(ResourceError::UnsupportedExternalHandleType);
        }

        let vk_handle_type = map_external_memory_handle_type(handle_type)?;
        let handle = self.image.memory.export_fd(vk_handle_type)?;

        Ok(BackendResourceExport {
            metadata: ExportedResourceMetadata {
                resource_id: request.resource_id,
                device_id: self.desc.device_id,
                kind: ResourceKind::Image,
                size_bytes: qgs_protocol::image_byte_len(
                    self.desc.width,
                    self.desc.height,
                    self.desc.format,
                )?,
                allocation_size_bytes: self.image.memory.size,
                buffer_usage: None,
                image_width: Some(self.desc.width),
                image_height: Some(self.desc.height),
                pixel_format: Some(self.desc.format),
                image_usage: Some(self.desc.usage),
                backend_memory_type_index: self.image.memory_type_index,
                backend_image_layout_token: None,
                handle_type,
                selected_memory: self.selected_memory,
                dedicated_allocation: true,
                attachment_count: 1,
            },
            handle,
        })
    }
}

struct GpuInstance {
    _entry: ash::Entry,
    instance: ash::Instance,
}

impl std::fmt::Debug for GpuInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuInstance").finish_non_exhaustive()
    }
}

impl GpuInstance {
    fn new() -> BackendResult<Self> {
        let entry = ash::Entry::linked();
        let app_name = c"qgs-vulkan";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(1)
            .engine_name(app_name)
            .engine_version(1)
            .api_version(vk::make_api_version(0, 1, 2, 0));
        let validation_layer = c"VK_LAYER_KHRONOS_validation";
        let layer_names =
            if validation_requested() && validation_layer_available(&entry, validation_layer)? {
                vec![validation_layer.as_ptr()]
            } else {
                Vec::new()
            };
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layer_names);
        // SAFETY: The create info references static C string literals and no
        // enabled extension/layer arrays. The returned instance is owned by
        // GpuInstance and destroyed in Drop.
        let instance = unsafe { entry.create_instance(&create_info, None) }?;
        Ok(Self {
            _entry: entry,
            instance,
        })
    }

    fn enumerate_physical_devices(&self) -> BackendResult<Vec<vk::PhysicalDevice>> {
        // SAFETY: self.instance is a live Vulkan instance.
        unsafe { self.instance.enumerate_physical_devices() }.map_err(BackendError::from)
    }

    fn physical_device_properties(
        &self,
        physical_device: vk::PhysicalDevice,
    ) -> vk::PhysicalDeviceProperties {
        // SAFETY: physical_device was returned by this instance during enumeration.
        unsafe {
            self.instance
                .get_physical_device_properties(physical_device)
        }
    }

    fn physical_device_memory_properties(
        &self,
        physical_device: vk::PhysicalDevice,
    ) -> vk::PhysicalDeviceMemoryProperties {
        // SAFETY: physical_device was returned by this instance during enumeration.
        unsafe {
            self.instance
                .get_physical_device_memory_properties(physical_device)
        }
    }

    fn physical_device_queue_family_properties(
        &self,
        physical_device: vk::PhysicalDevice,
    ) -> Vec<vk::QueueFamilyProperties> {
        // SAFETY: physical_device was returned by this instance during enumeration.
        unsafe {
            self.instance
                .get_physical_device_queue_family_properties(physical_device)
        }
    }
}

fn validation_requested() -> bool {
    matches!(
        std::env::var("QGS_VULKAN_ENABLE_VALIDATION").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn validation_layer_available(entry: &ash::Entry, name: &CStr) -> BackendResult<bool> {
    // SAFETY: entry is live and the loader writes the returned layer-property
    // vector. QGS only reads fixed null-terminated layer_name fields.
    let layers = unsafe { entry.enumerate_instance_layer_properties() }?;
    Ok(layers.iter().any(|layer| {
        // SAFETY: Vulkan layer_name fields are fixed-size null-terminated strings.
        let layer_name = unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) };
        layer_name == name
    }))
}

impl Drop for GpuInstance {
    fn drop(&mut self) {
        // SAFETY: GpuInstance owns this instance. GpuDevice holds an Arc back to
        // the instance, so devices are dropped before the final instance Arc.
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DeviceExtensionSupport {
    khr_external_memory: bool,
    khr_external_memory_fd: bool,
    ext_external_memory_dma_buf: bool,
    khr_dedicated_allocation: bool,
    khr_external_semaphore: bool,
    khr_external_semaphore_fd: bool,
    khr_external_fence_fd: bool,
    ext_image_drm_format_modifier: bool,
}

impl DeviceExtensionSupport {
    fn query(instance: &GpuInstance, physical_device: vk::PhysicalDevice) -> BackendResult<Self> {
        // SAFETY: physical_device was returned by this live instance.
        let properties = unsafe {
            instance
                .instance
                .enumerate_device_extension_properties(physical_device)
        }?;
        let has = |name: &CStr| {
            properties.iter().any(|extension| {
                // SAFETY: Vulkan extension_name fields are fixed-size null-terminated strings.
                let extension_name = unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) };
                extension_name == name
            })
        };
        Ok(Self {
            khr_external_memory: has(c"VK_KHR_external_memory"),
            khr_external_memory_fd: has(ash::khr::external_memory_fd::NAME),
            ext_external_memory_dma_buf: has(c"VK_EXT_external_memory_dma_buf"),
            khr_dedicated_allocation: has(c"VK_KHR_dedicated_allocation"),
            khr_external_semaphore: has(c"VK_KHR_external_semaphore"),
            khr_external_semaphore_fd: has(ash::khr::external_semaphore_fd::NAME),
            khr_external_fence_fd: has(c"VK_KHR_external_fence_fd"),
            ext_image_drm_format_modifier: has(c"VK_EXT_image_drm_format_modifier"),
        })
    }
}

struct GpuDevice {
    instance: Arc<GpuInstance>,
    physical_device: vk::PhysicalDevice,
    properties: vk::PhysicalDeviceProperties,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    queue_family_properties: Vec<vk::QueueFamilyProperties>,
    extension_support: DeviceExtensionSupport,
    device: ash::Device,
    external_memory_fd: ash::khr::external_memory_fd::Device,
    external_semaphore_fd: ash::khr::external_semaphore_fd::Device,
    queue: vk::Queue,
    queue_family_index: u32,
}

impl std::fmt::Debug for GpuDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuDevice")
            .field("physical_device", &self.physical_device)
            .field("queue_family_index", &self.queue_family_index)
            .field("extension_support", &self.extension_support)
            .finish_non_exhaustive()
    }
}

impl GpuDevice {
    fn new(
        instance: Arc<GpuInstance>,
        physical_device: vk::PhysicalDevice,
        properties: vk::PhysicalDeviceProperties,
        memory_properties: vk::PhysicalDeviceMemoryProperties,
        queue_family_properties: Vec<vk::QueueFamilyProperties>,
        queue_family_index: u32,
        extension_support: DeviceExtensionSupport,
    ) -> BackendResult<Self> {
        let priorities = [1.0_f32];
        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities)];
        let mut extension_names = Vec::new();
        if extension_support.khr_external_memory {
            extension_names.push(c"VK_KHR_external_memory".as_ptr());
        }
        if extension_support.khr_external_memory_fd {
            extension_names.push(ash::khr::external_memory_fd::NAME.as_ptr());
        }
        if extension_support.ext_external_memory_dma_buf {
            extension_names.push(c"VK_EXT_external_memory_dma_buf".as_ptr());
        }
        if extension_support.khr_dedicated_allocation {
            extension_names.push(c"VK_KHR_dedicated_allocation".as_ptr());
        }
        if extension_support.khr_external_semaphore {
            extension_names.push(c"VK_KHR_external_semaphore".as_ptr());
        }
        if extension_support.khr_external_semaphore_fd {
            extension_names.push(ash::khr::external_semaphore_fd::NAME.as_ptr());
        }
        if extension_support.ext_image_drm_format_modifier {
            extension_names.push(c"VK_EXT_image_drm_format_modifier".as_ptr());
        }
        let create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&extension_names);
        // SAFETY: physical_device and queue_family_index were queried from
        // instance. extension_names contains only device-supported extensions.
        let device = unsafe {
            instance
                .instance
                .create_device(physical_device, &create_info, None)
        }?;
        // SAFETY: queue index 0 exists because the selected family reported a
        // non-zero queue count and was used to create the logical device.
        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
        let external_memory_fd =
            ash::khr::external_memory_fd::Device::new(&instance.instance, &device);
        let external_semaphore_fd =
            ash::khr::external_semaphore_fd::Device::new(&instance.instance, &device);

        Ok(Self {
            instance,
            physical_device,
            properties,
            memory_properties,
            queue_family_properties,
            extension_support,
            device,
            external_memory_fd,
            external_semaphore_fd,
            queue,
            queue_family_index,
        })
    }

    fn queue_supports_compute(&self) -> bool {
        self.queue_family_properties
            .get(self.queue_family_index as usize)
            .is_some_and(|queue| queue.queue_flags.contains(vk::QueueFlags::COMPUTE))
    }

    fn choose_memory_type(
        &self,
        memory_type_bits: u32,
        required: vk::MemoryPropertyFlags,
        preferred: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        choose_memory_type_from_properties(
            &self.memory_properties,
            memory_type_bits,
            required,
            preferred,
        )
    }

    fn memory_fd_type_bits(
        &self,
        handle_type: vk::ExternalMemoryHandleTypeFlags,
        file: &File,
    ) -> BackendResult<u32> {
        let mut properties = vk::MemoryFdPropertiesKHR::default();
        // SAFETY: file.as_raw_fd() borrows the fd for the duration of the call.
        // vkGetMemoryFdPropertiesKHR does not consume POSIX fd ownership; the
        // caller retains the File for the later import operation.
        unsafe {
            self.external_memory_fd.get_memory_fd_properties(
                handle_type,
                file.as_raw_fd(),
                &mut properties,
            )
        }?;
        Ok(properties.memory_type_bits)
    }
}

impl Drop for GpuDevice {
    fn drop(&mut self) {
        // SAFETY: GpuDevice owns this logical device. Resource wrappers hold an
        // Arc<GpuDevice>, so they are dropped before the final device owner.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
        }
    }
}

#[derive(Debug)]
struct GpuMemory {
    device: Arc<GpuDevice>,
    memory: vk::DeviceMemory,
    size: u64,
    memory_type_index: u32,
    mapped: Mutex<Option<usize>>,
}

impl GpuMemory {
    fn allocate(
        device: Arc<GpuDevice>,
        requirements: vk::MemoryRequirements,
        required: vk::MemoryPropertyFlags,
        preferred: vk::MemoryPropertyFlags,
        export_handle_type: Option<vk::ExternalMemoryHandleTypeFlags>,
        dedicated: Option<DedicatedResource>,
    ) -> BackendResult<Self> {
        let memory_type_index = device
            .choose_memory_type(requirements.memory_type_bits, required, preferred)
            .ok_or(BackendError::NoMemoryType)?;
        let mut export_info = export_handle_type
            .map(|handle_types| vk::ExportMemoryAllocateInfo::default().handle_types(handle_types));
        let mut dedicated_info = dedicated.map(|dedicated| match dedicated {
            DedicatedResource::Buffer(buffer) => {
                vk::MemoryDedicatedAllocateInfo::default().buffer(buffer)
            }
            DedicatedResource::Image(image) => {
                vk::MemoryDedicatedAllocateInfo::default().image(image)
            }
        });
        let mut allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        if let Some(info) = export_info.as_mut() {
            allocate_info = allocate_info.push_next(info);
        }
        if let Some(info) = dedicated_info.as_mut() {
            allocate_info = allocate_info.push_next(info);
        }
        // SAFETY: allocation size and memory type index come from Vulkan memory
        // requirements and QGS memory selection. pNext structs live through call.
        let memory = unsafe { device.device.allocate_memory(&allocate_info, None) }?;
        Ok(Self {
            device,
            memory,
            size: requirements.size,
            memory_type_index,
            mapped: Mutex::new(None),
        })
    }

    fn import_fd(
        device: Arc<GpuDevice>,
        size: u64,
        memory_type_index: u32,
        handle_type: vk::ExternalMemoryHandleTypeFlags,
        file: File,
        dedicated: Option<DedicatedResource>,
    ) -> BackendResult<Self> {
        let raw_fd = file.into_raw_fd();
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(handle_type)
            .fd(raw_fd);
        let mut dedicated_info = dedicated.map(|dedicated| match dedicated {
            DedicatedResource::Buffer(buffer) => {
                vk::MemoryDedicatedAllocateInfo::default().buffer(buffer)
            }
            DedicatedResource::Image(image) => {
                vk::MemoryDedicatedAllocateInfo::default().image(image)
            }
        });
        let mut allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(size)
            .memory_type_index(memory_type_index)
            .push_next(&mut import_info);
        if let Some(info) = dedicated_info.as_mut() {
            allocate_info = allocate_info.push_next(info);
        }
        // SAFETY: raw_fd is moved out of File and consumed by Vulkan on
        // successful import. On allocation failure Vulkan leaves ownership with
        // the application; QGS reconstructs File below so it can be closed.
        let memory = match unsafe { device.device.allocate_memory(&allocate_info, None) } {
            Ok(memory) => memory,
            Err(err) => {
                // SAFETY: Vulkan did not consume fd on allocation failure.
                unsafe {
                    drop(File::from_raw_fd(raw_fd));
                }
                return Err(BackendError::Vk(err));
            }
        };
        Ok(Self {
            device,
            memory,
            size,
            memory_type_index,
            mapped: Mutex::new(None),
        })
    }

    fn export_fd(
        &self,
        handle_type: vk::ExternalMemoryHandleTypeFlags,
    ) -> Result<File, ResourceError> {
        let info = vk::MemoryGetFdInfoKHR::default()
            .memory(self.memory)
            .handle_type(handle_type);
        // SAFETY: self.memory is a live memory object allocated with export
        // support for this handle type.
        let fd = unsafe { self.device.external_memory_fd.get_memory_fd(&info) }.map_err(|err| {
            eprintln!("vulkan memory fd export failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        // SAFETY: vkGetMemoryFdKHR returns a new owned POSIX fd on success.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn map(&self) -> BackendResult<*mut std::ffi::c_void> {
        let mut mapped = self.mapped.lock().map_err(|_| BackendError::InvalidInput)?;
        if let Some(ptr) = *mapped {
            return Ok(ptr as *mut std::ffi::c_void);
        }
        // SAFETY: self.memory is live, offset 0 and size self.size are inside
        // the allocation, and QGS serializes mapping through mapped mutex.
        let ptr = unsafe {
            self.device
                .device
                .map_memory(self.memory, 0, self.size, vk::MemoryMapFlags::empty())
        }?;
        *mapped = Some(ptr as usize);
        Ok(ptr)
    }

    fn flush(&self, offset: u64, size: u64) -> BackendResult<()> {
        if self.is_host_coherent() {
            return Ok(());
        }
        let range = vk::MappedMemoryRange::default()
            .memory(self.memory)
            .offset(offset)
            .size(size);
        // SAFETY: range describes a currently mapped byte range in this memory.
        unsafe { self.device.device.flush_mapped_memory_ranges(&[range]) }?;
        Ok(())
    }

    fn invalidate(&self, offset: u64, size: u64) -> BackendResult<()> {
        if self.is_host_coherent() {
            return Ok(());
        }
        let range = vk::MappedMemoryRange::default()
            .memory(self.memory)
            .offset(offset)
            .size(size);
        // SAFETY: range describes a currently mapped byte range in this memory.
        unsafe { self.device.device.invalidate_mapped_memory_ranges(&[range]) }?;
        Ok(())
    }

    fn is_host_coherent(&self) -> bool {
        self.device.memory_properties.memory_types[self.memory_type_index as usize]
            .property_flags
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
    }
}

impl Drop for GpuMemory {
    fn drop(&mut self) {
        // SAFETY: GpuMemory owns this memory. Any mapping was created by this
        // wrapper and must be unmapped before freeing.
        unsafe {
            if self
                .mapped
                .lock()
                .ok()
                .and_then(|mut mapped| mapped.take())
                .is_some()
            {
                self.device.device.unmap_memory(self.memory);
            }
            self.device.device.free_memory(self.memory, None);
        }
    }
}

#[derive(Clone, Copy)]
enum DedicatedResource {
    Buffer(vk::Buffer),
    Image(vk::Image),
}

#[derive(Debug)]
struct GpuBuffer {
    device: Arc<GpuDevice>,
    buffer: vk::Buffer,
    memory: Arc<GpuMemory>,
    size: u64,
    memory_type_index: u32,
}

struct ImportedBufferInfo {
    size: u64,
    usage: vk::BufferUsageFlags,
    handle_type: vk::ExternalMemoryHandleTypeFlags,
    file: File,
    allocation_size: u64,
    memory_type_index: u32,
    dedicated: bool,
}

impl GpuBuffer {
    fn new(
        device: Arc<GpuDevice>,
        size: u64,
        usage: vk::BufferUsageFlags,
        external_handle_type: Option<vk::ExternalMemoryHandleTypeFlags>,
        required: vk::MemoryPropertyFlags,
        preferred: vk::MemoryPropertyFlags,
        import_only: bool,
    ) -> Result<Self, ResourceError> {
        let mut external_info = external_handle_type.map(|handle_types| {
            vk::ExternalMemoryBufferCreateInfo::default().handle_types(handle_types)
        });
        let mut create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        if let Some(info) = external_info.as_mut() {
            create_info = create_info.push_next(info);
        }
        // SAFETY: create_info is fully initialized and pNext storage lives for
        // the duration of the call.
        let buffer = unsafe { device.device.create_buffer(&create_info, None) }.map_err(|err| {
            eprintln!("vulkan buffer creation failed: {err:?}");
            ResourceError::AllocationFailed
        })?;
        // SAFETY: buffer is live and was created by this device.
        let requirements = unsafe { device.device.get_buffer_memory_requirements(buffer) };
        let memory = GpuMemory::allocate(
            device.clone(),
            requirements,
            required,
            preferred,
            external_handle_type,
            Some(DedicatedResource::Buffer(buffer)),
        )
        .map_err(|err| {
            eprintln!("vulkan buffer memory allocation failed: {err:?}");
            // SAFETY: buffer was created by this device and has not been bound.
            unsafe {
                device.device.destroy_buffer(buffer, None);
            }
            ResourceError::AllocationFailed
        })?;
        // SAFETY: buffer and memory are from the same device. Offset 0 satisfies
        // Vulkan's reported alignment because it is always aligned.
        unsafe { device.device.bind_buffer_memory(buffer, memory.memory, 0) }.map_err(|err| {
            eprintln!("vulkan buffer memory bind failed: {err:?}");
            ResourceError::AllocationFailed
        })?;
        let memory_type_index = memory.memory_type_index;
        let memory = Arc::new(memory);
        let buffer = Self {
            device,
            buffer,
            memory,
            size,
            memory_type_index,
        };
        if !import_only && required.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) {
            let _ = buffer.memory.map().map_err(|err| {
                eprintln!("vulkan buffer memory map failed: {err:?}");
                ResourceError::AllocationFailed
            })?;
        }
        Ok(buffer)
    }

    fn imported(device: Arc<GpuDevice>, info: ImportedBufferInfo) -> Result<Self, ResourceError> {
        let mut external_info =
            vk::ExternalMemoryBufferCreateInfo::default().handle_types(info.handle_type);
        let create_info = vk::BufferCreateInfo::default()
            .push_next(&mut external_info)
            .size(info.size)
            .usage(info.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        // SAFETY: create_info is fully initialized and pNext storage lives for
        // the duration of the call.
        let buffer = unsafe { device.device.create_buffer(&create_info, None) }.map_err(|err| {
            eprintln!("vulkan imported buffer creation failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        let dedicated_resource = info.dedicated.then_some(DedicatedResource::Buffer(buffer));
        let memory = GpuMemory::import_fd(
            device.clone(),
            info.allocation_size,
            info.memory_type_index,
            info.handle_type,
            info.file,
            dedicated_resource,
        )
        .map_err(|err| {
            eprintln!("vulkan imported buffer memory import failed: {err:?}");
            // SAFETY: buffer was created by this device and has not been bound.
            unsafe {
                device.device.destroy_buffer(buffer, None);
            }
            ResourceError::ExportFailed
        })?;
        // SAFETY: buffer and memory are from the same device. Offset 0 satisfies
        // Vulkan's reported alignment because it is always aligned.
        unsafe { device.device.bind_buffer_memory(buffer, memory.memory, 0) }.map_err(|err| {
            eprintln!("vulkan imported buffer memory bind failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        Ok(Self {
            device,
            buffer,
            memory: Arc::new(memory),
            size: info.size,
            memory_type_index: info.memory_type_index,
        })
    }

    fn write_bytes(&self, offset: u64, bytes: &[u8]) -> Result<(), ResourceError> {
        let size = u64::try_from(bytes.len()).map_err(|_| ResourceError::InvalidBufferSize)?;
        if offset.checked_add(size).is_none_or(|end| end > self.size) {
            return Err(ResourceError::InvalidBufferSize);
        }
        let ptr = self.memory.map().map_err(|err| {
            eprintln!("vulkan buffer map for write failed: {err:?}");
            ResourceError::UnsupportedMemoryRequirements
        })?;
        // SAFETY: offset..offset+size was bounds-checked above, and ptr points
        // to the mapped allocation for this buffer.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                ptr.cast::<u8>().add(offset as usize),
                bytes.len(),
            );
        }
        self.memory.flush(offset, size).map_err(|err| {
            eprintln!("vulkan buffer flush failed: {err:?}");
            ResourceError::ExportFailed
        })
    }

    fn read_bytes(&self, offset: u64, len: usize) -> Result<Vec<u8>, ResourceError> {
        let size = u64::try_from(len).map_err(|_| ResourceError::InvalidBufferSize)?;
        if offset.checked_add(size).is_none_or(|end| end > self.size) {
            return Err(ResourceError::InvalidBufferSize);
        }
        let ptr = self.memory.map().map_err(|err| {
            eprintln!("vulkan buffer map for read failed: {err:?}");
            ResourceError::UnsupportedMemoryRequirements
        })?;
        self.memory.invalidate(offset, size).map_err(|err| {
            eprintln!("vulkan buffer invalidate failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        let mut out = vec![0_u8; len];
        // SAFETY: offset..offset+size was bounds-checked above, ptr points to
        // mapped allocation memory, and out is valid for len bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(
                ptr.cast::<u8>().add(offset as usize),
                out.as_mut_ptr(),
                len,
            );
        }
        Ok(out)
    }
}

impl Drop for GpuBuffer {
    fn drop(&mut self) {
        // SAFETY: GpuBuffer owns this VkBuffer. Bound memory is held by Arc and
        // remains alive until after the buffer is destroyed.
        unsafe {
            let _ = self.device.device.device_wait_idle();
            self.device.device.destroy_buffer(self.buffer, None);
        }
    }
}

#[derive(Debug)]
struct GpuImage {
    device: Arc<GpuDevice>,
    image: vk::Image,
    memory: Arc<GpuMemory>,
    format: vk::Format,
    extent: [u32; 3],
    memory_type_index: u32,
}

struct ImageCreateParams {
    format: vk::Format,
    extent: [u32; 3],
    usage: vk::ImageUsageFlags,
    tiling: vk::ImageTiling,
    external_handle_type: Option<vk::ExternalMemoryHandleTypeFlags>,
    required: vk::MemoryPropertyFlags,
    preferred: vk::MemoryPropertyFlags,
}

impl GpuImage {
    fn new(device: Arc<GpuDevice>, params: ImageCreateParams) -> Result<Self, ResourceError> {
        let mut external_info = params.external_handle_type.map(|handle_types| {
            vk::ExternalMemoryImageCreateInfo::default().handle_types(handle_types)
        });
        let mut create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(params.format)
            .extent(vk::Extent3D {
                width: params.extent[0],
                height: params.extent[1],
                depth: params.extent[2],
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(params.tiling)
            .usage(params.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        if let Some(info) = external_info.as_mut() {
            create_info = create_info.push_next(info);
        }
        // SAFETY: create_info is fully initialized and pNext storage lives for
        // the duration of the call.
        let image = unsafe { device.device.create_image(&create_info, None) }.map_err(|err| {
            eprintln!("vulkan image creation failed: {err:?}");
            ResourceError::AllocationFailed
        })?;
        // SAFETY: image is live and was created by this device.
        let requirements = unsafe { device.device.get_image_memory_requirements(image) };
        let memory = GpuMemory::allocate(
            device.clone(),
            requirements,
            params.required,
            params.preferred,
            params.external_handle_type,
            Some(DedicatedResource::Image(image)),
        )
        .map_err(|err| {
            eprintln!("vulkan image memory allocation failed: {err:?}");
            // SAFETY: image was created by this device and has not been bound.
            unsafe {
                device.device.destroy_image(image, None);
            }
            ResourceError::AllocationFailed
        })?;
        // SAFETY: image and memory are from the same device. Offset 0 satisfies
        // Vulkan's reported alignment because it is always aligned.
        unsafe { device.device.bind_image_memory(image, memory.memory, 0) }.map_err(|err| {
            eprintln!("vulkan image memory bind failed: {err:?}");
            ResourceError::AllocationFailed
        })?;
        let memory_type_index = memory.memory_type_index;
        Ok(Self {
            device,
            image,
            memory: Arc::new(memory),
            format: params.format,
            extent: params.extent,
            memory_type_index,
        })
    }

    fn imported(
        device: Arc<GpuDevice>,
        metadata: &ExportedResourceMetadata,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        tiling: vk::ImageTiling,
        handle_type: vk::ExternalMemoryHandleTypeFlags,
        file: File,
    ) -> Result<Self, ResourceError> {
        let width = metadata.image_width.ok_or(ResourceError::ExportFailed)?;
        let height = metadata.image_height.ok_or(ResourceError::ExportFailed)?;
        let mut external_info =
            vk::ExternalMemoryImageCreateInfo::default().handle_types(handle_type);
        let create_info = vk::ImageCreateInfo::default()
            .push_next(&mut external_info)
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(tiling)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // SAFETY: create_info is fully initialized and pNext storage lives for
        // the duration of the call.
        let image = unsafe { device.device.create_image(&create_info, None) }.map_err(|err| {
            eprintln!("vulkan imported image creation failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        let dedicated = metadata
            .dedicated_allocation
            .then_some(DedicatedResource::Image(image));
        let memory = GpuMemory::import_fd(
            device.clone(),
            metadata.allocation_size_bytes,
            metadata.backend_memory_type_index,
            handle_type,
            file,
            dedicated,
        )
        .map_err(|err| {
            eprintln!("vulkan imported image memory import failed: {err:?}");
            // SAFETY: image was created by this device and has not been bound.
            unsafe {
                device.device.destroy_image(image, None);
            }
            ResourceError::ExportFailed
        })?;
        // SAFETY: image and memory are from the same device. Offset 0 satisfies
        // Vulkan's reported alignment because it is always aligned.
        unsafe { device.device.bind_image_memory(image, memory.memory, 0) }.map_err(|err| {
            eprintln!("vulkan imported image memory bind failed: {err:?}");
            ResourceError::ExportFailed
        })?;
        Ok(Self {
            device,
            image,
            memory: Arc::new(memory),
            format,
            extent: [width, height, 1],
            memory_type_index: metadata.backend_memory_type_index,
        })
    }

    fn create_view(&self) -> BackendResult<GpuImageView> {
        let create_info = vk::ImageViewCreateInfo::default()
            .image(self.image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(self.format)
            .subresource_range(color_subresource_range());
        // SAFETY: image is live, format/view type match image creation, and
        // the view is owned by GpuImageView.
        let view = unsafe { self.device.device.create_image_view(&create_info, None) }?;
        Ok(GpuImageView {
            device: self.device.clone(),
            view,
        })
    }
}

impl Drop for GpuImage {
    fn drop(&mut self) {
        // SAFETY: GpuImage owns this VkImage. Bound memory is held by Arc and
        // remains alive until after the image is destroyed.
        unsafe {
            let _ = self.device.device.device_wait_idle();
            self.device.device.destroy_image(self.image, None);
        }
    }
}

#[derive(Debug)]
struct GpuImageView {
    device: Arc<GpuDevice>,
    view: vk::ImageView,
}

impl Drop for GpuImageView {
    fn drop(&mut self) {
        // SAFETY: GpuImageView owns this image view.
        unsafe {
            self.device.device.destroy_image_view(self.view, None);
        }
    }
}

#[derive(Debug)]
struct GpuSemaphore {
    device: Arc<GpuDevice>,
    semaphore: vk::Semaphore,
}

impl GpuSemaphore {
    fn new(device: Arc<GpuDevice>, export_sync_fd: bool) -> BackendResult<Self> {
        let mut export_info = export_sync_fd.then(|| {
            vk::ExportSemaphoreCreateInfo::default()
                .handle_types(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
        });
        let mut create_info = vk::SemaphoreCreateInfo::default();
        if let Some(info) = export_info.as_mut() {
            create_info = create_info.push_next(info);
        }
        // SAFETY: create_info is valid and pNext storage lives for call.
        let semaphore = unsafe { device.device.create_semaphore(&create_info, None) }?;
        Ok(Self { device, semaphore })
    }

    fn export_sync_fd(&self) -> BackendResult<File> {
        let info = vk::SemaphoreGetFdInfoKHR::default()
            .semaphore(self.semaphore)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        // SAFETY: semaphore was created with sync FD export support and has a
        // pending/available signal payload submitted by QGS before export.
        let fd = unsafe { self.device.external_semaphore_fd.get_semaphore_fd(&info) }?;
        // SAFETY: vkGetSemaphoreFdKHR returns a new owned POSIX fd on success.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn import_sync_fd(&self, file: File) -> BackendResult<()> {
        let raw_fd = file.into_raw_fd();
        let info = vk::ImportSemaphoreFdInfoKHR::default()
            .semaphore(self.semaphore)
            .flags(vk::SemaphoreImportFlags::TEMPORARY)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
            .fd(raw_fd);
        // SAFETY: raw_fd is moved from File and consumed by Vulkan on success.
        // On failure, Vulkan leaves ownership with QGS and the fd is closed.
        match unsafe { self.device.external_semaphore_fd.import_semaphore_fd(&info) } {
            Ok(()) => Ok(()),
            Err(err) => {
                // SAFETY: Vulkan did not consume fd on import failure.
                unsafe {
                    drop(File::from_raw_fd(raw_fd));
                }
                Err(BackendError::Vk(err))
            }
        }
    }
}

impl Drop for GpuSemaphore {
    fn drop(&mut self) {
        // SAFETY: GpuSemaphore owns this semaphore.
        unsafe {
            self.device.device.destroy_semaphore(self.semaphore, None);
        }
    }
}

#[derive(Debug)]
struct GpuFence {
    device: Arc<GpuDevice>,
    fence: vk::Fence,
}

impl GpuFence {
    fn new(device: Arc<GpuDevice>) -> BackendResult<Self> {
        // SAFETY: create info is valid and fence is owned by GpuFence.
        let fence = unsafe {
            device
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }?;
        Ok(Self { device, fence })
    }

    fn wait(&self) -> BackendResult<()> {
        // SAFETY: fence belongs to this device and is live.
        unsafe {
            self.device
                .device
                .wait_for_fences(&[self.fence], true, 5_000_000_000)
        }?;
        Ok(())
    }
}

impl Drop for GpuFence {
    fn drop(&mut self) {
        // SAFETY: GpuFence owns this fence.
        unsafe {
            self.device.device.destroy_fence(self.fence, None);
        }
    }
}

#[derive(Debug)]
struct GpuCommandPool {
    device: Arc<GpuDevice>,
    pool: vk::CommandPool,
}

impl GpuCommandPool {
    fn new(device: Arc<GpuDevice>) -> BackendResult<Self> {
        let create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(device.queue_family_index);
        // SAFETY: queue_family_index belongs to this device.
        let pool = unsafe { device.device.create_command_pool(&create_info, None) }?;
        Ok(Self { device, pool })
    }

    fn allocate_primary(&self) -> BackendResult<vk::CommandBuffer> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: pool is live and belongs to this device.
        let command_buffers =
            unsafe { self.device.device.allocate_command_buffers(&allocate_info) }?;
        Ok(command_buffers[0])
    }
}

impl Drop for GpuCommandPool {
    fn drop(&mut self) {
        // SAFETY: GpuCommandPool owns this command pool. PendingSubmission waits
        // on its fence before dropping command pools for submitted work.
        unsafe {
            self.device.device.destroy_command_pool(self.pool, None);
        }
    }
}

#[derive(Debug)]
struct PendingSubmission {
    fence: GpuFence,
    _command_pool: GpuCommandPool,
}

impl Drop for PendingSubmission {
    fn drop(&mut self) {
        let _ = self.fence.wait();
    }
}

fn one_time_commands<T>(
    device: &Arc<GpuDevice>,
    record: impl FnOnce(vk::CommandBuffer) -> Result<T, ResourceError>,
    wait_semaphore: Option<&GpuSemaphore>,
    signal_semaphore: Option<&GpuSemaphore>,
    wait: bool,
) -> Result<(T, PendingSubmission), ResourceError> {
    let command_pool = GpuCommandPool::new(device.clone()).map_err(|err| {
        eprintln!("vulkan command pool creation failed: {err:?}");
        ResourceError::ExportFailed
    })?;
    let command_buffer = command_pool.allocate_primary().map_err(|err| {
        eprintln!("vulkan command buffer allocation failed: {err:?}");
        ResourceError::ExportFailed
    })?;
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: command_buffer is newly allocated and not currently recording.
    unsafe { device.device.begin_command_buffer(command_buffer, &begin) }.map_err(|err| {
        eprintln!("vulkan command buffer begin failed: {err:?}");
        ResourceError::ExportFailed
    })?;
    let output = record(command_buffer)?;
    // SAFETY: command_buffer is currently recording.
    unsafe { device.device.end_command_buffer(command_buffer) }.map_err(|err| {
        eprintln!("vulkan command buffer end failed: {err:?}");
        ResourceError::ExportFailed
    })?;
    let fence = GpuFence::new(device.clone()).map_err(|err| {
        eprintln!("vulkan fence creation failed: {err:?}");
        ResourceError::ExportFailed
    })?;
    let wait_semaphores = wait_semaphore
        .map(|semaphore| vec![semaphore.semaphore])
        .unwrap_or_default();
    let wait_stages = wait_semaphore
        .map(|_| vec![vk::PipelineStageFlags::ALL_COMMANDS])
        .unwrap_or_default();
    let signal_semaphores = signal_semaphore
        .map(|semaphore| vec![semaphore.semaphore])
        .unwrap_or_default();
    let command_buffers = [command_buffer];
    let submit = vk::SubmitInfo::default()
        .wait_semaphores(&wait_semaphores)
        .wait_dst_stage_mask(&wait_stages)
        .command_buffers(&command_buffers)
        .signal_semaphores(&signal_semaphores);
    // SAFETY: queue belongs to device, command buffer is executable, semaphore
    // and fence handles belong to same device and are kept alive by wrappers.
    unsafe {
        device
            .device
            .queue_submit(device.queue, &[submit], fence.fence)
    }
    .map_err(|err| {
        eprintln!("vulkan queue submit failed: {err:?}");
        ResourceError::ExportFailed
    })?;
    if wait {
        fence.wait().map_err(|err| {
            eprintln!("vulkan fence wait failed: {err:?}");
            ResourceError::ExportFailed
        })?;
    }
    Ok((
        output,
        PendingSubmission {
            fence,
            _command_pool: command_pool,
        },
    ))
}

fn describe_device(
    raw_id: u64,
    properties: vk::PhysicalDeviceProperties,
) -> Result<DeviceDesc, DeviceDiscoveryError> {
    Ok(DeviceDesc {
        id: DeviceId::new(raw_id)?,
        class: classify_device_type(properties.device_type),
        vendor_id: properties.vendor_id,
        device_id: properties.device_id,
        name: bounded_name(properties.device_name.as_ptr())?,
        backend: BackendApi::Vulkan,
        api_version: to_qgs_api_version(properties.api_version),
        driver_version: properties.driver_version,
    })
}

fn describe_capabilities(device_id: DeviceId, device: &GpuDevice) -> DeviceCapabilities {
    DeviceCapabilities {
        device_id,
        compute: describe_compute_capabilities(device),
        memory: describe_memory_capabilities(device),
        interop: describe_interop_capabilities(device),
    }
}

fn describe_compute_capabilities(device: &GpuDevice) -> ComputeCapabilities {
    let limits = device.properties.limits;
    ComputeCapabilities {
        supported: device.queue_supports_compute(),
        max_workgroup_count: limits.max_compute_work_group_count,
        max_workgroup_size: limits.max_compute_work_group_size,
        max_workgroup_invocations: limits.max_compute_work_group_invocations,
    }
}

fn describe_memory_capabilities(device: &GpuDevice) -> MemoryCapabilities {
    let mut heaps = Vec::new();
    for index in 0..device.memory_properties.memory_heap_count {
        if heaps.len() >= MAX_MEMORY_HEAP_COUNT {
            break;
        }
        let heap = device.memory_properties.memory_heaps[index as usize];
        heaps.push(MemoryHeapDesc {
            size_bytes: heap.size,
            device_local: heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL),
        });
    }

    let mut host_visible = false;
    let mut host_coherent = false;
    let mut device_local = false;
    for index in 0..device
        .memory_properties
        .memory_type_count
        .min(MAX_MEMORY_TYPE_COUNT as u32)
    {
        let flags = device.memory_properties.memory_types[index as usize].property_flags;
        host_visible |= flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE);
        host_coherent |= flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT);
        device_local |= flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
    }

    MemoryCapabilities {
        heaps,
        memory_type_count: device.memory_properties.memory_type_count as u16,
        host_visible,
        host_coherent,
        device_local,
    }
}

fn describe_interop_capabilities(device: &GpuDevice) -> InteropCapabilities {
    InteropCapabilities {
        external_memory_fd: device.extension_support.khr_external_memory
            && device.extension_support.khr_external_memory_fd,
        dma_buf: device.extension_support.khr_external_memory
            && device.extension_support.khr_external_memory_fd
            && device.extension_support.ext_external_memory_dma_buf,
        external_semaphore_fd: device.extension_support.khr_external_semaphore
            && device.extension_support.khr_external_semaphore_fd,
        external_fence_fd: device.extension_support.khr_external_fence_fd,
    }
}

fn classify_device_type(device_type: vk::PhysicalDeviceType) -> DeviceClass {
    match device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => DeviceClass::DiscreteGpu,
        vk::PhysicalDeviceType::INTEGRATED_GPU => DeviceClass::IntegratedGpu,
        vk::PhysicalDeviceType::CPU => DeviceClass::Software,
        vk::PhysicalDeviceType::VIRTUAL_GPU => DeviceClass::Other,
        _ => DeviceClass::Other,
    }
}

fn bounded_name(ptr: *const std::os::raw::c_char) -> Result<String, DeviceDiscoveryError> {
    // SAFETY: Vulkan guarantees device_name is a null-terminated fixed-size C string.
    let name = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| DeviceDiscoveryError::BackendFailed)?;
    let mut name = name.to_string();
    if name.len() > MAX_DEVICE_NAME_LEN {
        name.truncate(MAX_DEVICE_NAME_LEN);
    }
    Ok(name)
}

fn to_qgs_api_version(version: u32) -> ApiVersion {
    ApiVersion {
        major: vk::api_version_major(version) as u16,
        minor: vk::api_version_minor(version) as u16,
        patch: vk::api_version_patch(version) as u16,
    }
}

fn select_queue_family(queues: &[vk::QueueFamilyProperties]) -> Option<u32> {
    queues
        .iter()
        .enumerate()
        .find(|(_, queue)| {
            queue.queue_count > 0 && queue.queue_flags.contains(vk::QueueFlags::COMPUTE)
        })
        .or_else(|| {
            queues
                .iter()
                .enumerate()
                .find(|(_, queue)| queue.queue_count > 0)
        })
        .and_then(|(index, _)| u32::try_from(index).ok())
}

fn map_buffer_usage(usage: BufferUsageFlags) -> Result<vk::BufferUsageFlags, ResourceError> {
    let mut flags = vk::BufferUsageFlags::empty();
    if usage.contains(BufferUsageFlags::TRANSFER_SRC) {
        flags |= vk::BufferUsageFlags::TRANSFER_SRC;
    }
    if usage.contains(BufferUsageFlags::TRANSFER_DST) {
        flags |= vk::BufferUsageFlags::TRANSFER_DST;
    }
    if usage.contains(BufferUsageFlags::STORAGE) {
        flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
    }
    if flags.is_empty() {
        Err(ResourceError::UnsupportedMemoryRequirements)
    } else {
        Ok(flags)
    }
}

fn map_pixel_format(format: PixelFormat) -> Result<vk::Format, ResourceError> {
    match format {
        PixelFormat::Rgba8Unorm => Ok(vk::Format::R8G8B8A8_UNORM),
    }
}

fn map_image_usage(usage: ImageUsageFlags) -> Result<vk::ImageUsageFlags, ResourceError> {
    let mut flags = vk::ImageUsageFlags::empty();
    if usage.contains(ImageUsageFlags::TRANSFER_SRC) {
        flags |= vk::ImageUsageFlags::TRANSFER_SRC;
    }
    if usage.contains(ImageUsageFlags::TRANSFER_DST) {
        flags |= vk::ImageUsageFlags::TRANSFER_DST;
    }
    if usage.contains(ImageUsageFlags::STORAGE) {
        flags |= vk::ImageUsageFlags::STORAGE;
    }
    if flags.is_empty() {
        Err(ResourceError::UnsupportedImageUsage)
    } else {
        Ok(flags)
    }
}

fn required_memory_flags(desc: &BufferDesc) -> vk::MemoryPropertyFlags {
    let mut flags = vk::MemoryPropertyFlags::empty();
    if desc.memory_preference.host_visible_required {
        flags |= vk::MemoryPropertyFlags::HOST_VISIBLE;
    }
    flags
}

fn preferred_memory_flags(desc: &BufferDesc) -> vk::MemoryPropertyFlags {
    let mut flags = vk::MemoryPropertyFlags::empty();
    if desc.memory_preference.device_preferred {
        flags |= vk::MemoryPropertyFlags::DEVICE_LOCAL;
    }
    if desc.memory_preference.host_coherent_preferred {
        flags |= vk::MemoryPropertyFlags::HOST_COHERENT;
    }
    flags
}

fn selected_memory_properties(
    device: &GpuDevice,
    memory_type_index: u32,
) -> Result<SelectedMemoryProperties, ResourceError> {
    let memory_type = device
        .memory_properties
        .memory_types
        .get(memory_type_index as usize)
        .ok_or(ResourceError::UnsupportedMemoryRequirements)?;
    let flags = memory_type.property_flags;
    Ok(SelectedMemoryProperties {
        device_local: flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL),
        host_visible: flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE),
        host_coherent: flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT),
    })
}

fn map_external_memory_handle_type(
    handle_type: ExternalHandleType,
) -> Result<vk::ExternalMemoryHandleTypeFlags, ResourceError> {
    match handle_type {
        ExternalHandleType::OpaqueFd => Ok(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD),
        ExternalHandleType::DmaBuf => Ok(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
    }
}

fn validate_external_buffer_support(
    device: &GpuDevice,
    usage: vk::BufferUsageFlags,
    handle_type: vk::ExternalMemoryHandleTypeFlags,
) -> Result<(), ResourceError> {
    if handle_type == vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT
        && !device.extension_support.ext_external_memory_dma_buf
    {
        return Err(ResourceError::UnsupportedExternalHandleType);
    }
    if !device.extension_support.khr_external_memory
        || !device.extension_support.khr_external_memory_fd
    {
        return Err(ResourceError::UnsupportedExternalHandleType);
    }
    let info = vk::PhysicalDeviceExternalBufferInfo::default()
        .flags(vk::BufferCreateFlags::empty())
        .usage(usage)
        .handle_type(handle_type);
    let mut properties = vk::ExternalBufferProperties::default();
    // SAFETY: device.physical_device belongs to device.instance, and info is valid.
    unsafe {
        device
            .instance
            .instance
            .get_physical_device_external_buffer_properties(
                device.physical_device,
                &info,
                &mut properties,
            )
    };
    let features = properties
        .external_memory_properties
        .external_memory_features;
    if features.contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE)
        && features.contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
    {
        Ok(())
    } else {
        Err(ResourceError::UnsupportedExternalHandleType)
    }
}

fn validate_external_image_support(
    device: &GpuDevice,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    tiling: vk::ImageTiling,
    handle_type: vk::ExternalMemoryHandleTypeFlags,
) -> Result<(), ResourceError> {
    if handle_type == vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT
        && !device.extension_support.ext_external_memory_dma_buf
    {
        return Err(ResourceError::UnsupportedExternalHandleType);
    }
    if !device.extension_support.khr_external_memory
        || !device.extension_support.khr_external_memory_fd
    {
        return Err(ResourceError::UnsupportedExternalHandleType);
    }
    let mut info = vk::PhysicalDeviceExternalImageFormatInfo::default().handle_type(handle_type);
    let mut external_properties = vk::ExternalImageFormatProperties::default();
    let mut format_properties =
        vk::ImageFormatProperties2::default().push_next(&mut external_properties);
    let format_info = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(format)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(tiling)
        .usage(usage)
        .flags(vk::ImageCreateFlags::empty())
        .push_next(&mut info);
    // SAFETY: physical device belongs to instance, and pNext structures live
    // through the call.
    unsafe {
        device
            .instance
            .instance
            .get_physical_device_image_format_properties2(
                device.physical_device,
                &format_info,
                &mut format_properties,
            )
    }
    .map_err(|_| ResourceError::UnsupportedExternalHandleType)?;
    let features = external_properties
        .external_memory_properties
        .external_memory_features;
    if features.contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE)
        && features.contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
    {
        Ok(())
    } else {
        Err(ResourceError::UnsupportedExternalHandleType)
    }
}

fn validate_sync_fd_support(device: &GpuDevice) -> Result<(), SyncError> {
    if device.extension_support.khr_external_semaphore
        && device.extension_support.khr_external_semaphore_fd
    {
        Ok(())
    } else {
        Err(SyncError::UnsupportedSyncHandleType)
    }
}

fn import_external_buffer(
    device: &Arc<GpuDevice>,
    metadata: &ExportedResourceMetadata,
    usage: vk::BufferUsageFlags,
    handle_type: vk::ExternalMemoryHandleTypeFlags,
    handle: File,
) -> Result<GpuBuffer, ResourceError> {
    let fd_memory_type_bits = device
        .memory_fd_type_bits(handle_type, &handle)
        .map_err(|err| {
            eprintln!("vulkan imported buffer fd-property query failed: {err:?}");
            ResourceError::ExportFailed
        })?;
    let memory_type_bit = 1_u32
        .checked_shl(metadata.backend_memory_type_index)
        .ok_or(ResourceError::UnsupportedMemoryRequirements)?;
    if fd_memory_type_bits & memory_type_bit == 0 {
        return Err(ResourceError::UnsupportedMemoryRequirements);
    }
    GpuBuffer::imported(
        device.clone(),
        ImportedBufferInfo {
            size: metadata.size_bytes,
            usage,
            handle_type,
            file: handle,
            allocation_size: metadata.allocation_size_bytes,
            memory_type_index: metadata.backend_memory_type_index,
            dedicated: metadata.dedicated_allocation,
        },
    )
}

struct ImportedImageInfo<'a> {
    metadata: &'a ExportedResourceMetadata,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    tiling: vk::ImageTiling,
    handle_type: vk::ExternalMemoryHandleTypeFlags,
    handle: File,
}

fn import_external_image(
    device: &Arc<GpuDevice>,
    info: ImportedImageInfo<'_>,
) -> Result<GpuImage, ResourceError> {
    let fd_memory_type_bits = device
        .memory_fd_type_bits(info.handle_type, &info.handle)
        .map_err(|err| {
            eprintln!("vulkan imported image fd-property query failed: {err:?}");
            ResourceError::ExportFailed
        })?;
    let memory_type_bit = 1_u32
        .checked_shl(info.metadata.backend_memory_type_index)
        .ok_or(ResourceError::UnsupportedMemoryRequirements)?;
    if fd_memory_type_bits & memory_type_bit == 0 {
        return Err(ResourceError::UnsupportedMemoryRequirements);
    }
    GpuImage::imported(
        device.clone(),
        info.metadata,
        info.format,
        info.usage,
        info.tiling,
        info.handle_type,
        info.handle,
    )
}

fn submit_gpu_fill(
    buffer: &GpuBuffer,
    semaphore: &GpuSemaphore,
    pattern: u32,
) -> Result<PendingSubmission, SyncError> {
    let (_, pending) = one_time_commands(
        &buffer.device,
        |command_buffer| {
            // SAFETY: command_buffer is recording. buffer is live and created
            // with TRANSFER_DST usage by QGS callers that request sync export.
            unsafe {
                buffer.device.device.cmd_fill_buffer(
                    command_buffer,
                    buffer.buffer,
                    0,
                    buffer.size,
                    pattern,
                );
            }
            Ok(())
        },
        None,
        Some(semaphore),
        false,
    )
    .map_err(|_| SyncError::SyncExportFailed)?;
    Ok(pending)
}

fn submit_gpu_image_clear(
    image: &GpuImage,
    semaphore: &GpuSemaphore,
    pattern: u32,
) -> Result<PendingSubmission, SyncError> {
    let (_, pending) = one_time_commands(
        &image.device,
        |command_buffer| {
            transition_image(
                &image.device,
                command_buffer,
                ImageTransition {
                    image: image.image,
                    old_layout: vk::ImageLayout::UNDEFINED,
                    new_layout: vk::ImageLayout::GENERAL,
                    src_access: vk::AccessFlags::empty(),
                    dst_access: vk::AccessFlags::TRANSFER_WRITE,
                    src_stage: vk::PipelineStageFlags::TOP_OF_PIPE,
                    dst_stage: vk::PipelineStageFlags::TRANSFER,
                },
            );
            let bytes = pattern.to_le_bytes();
            let color = vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, f32::from(bytes[3]) / f32::from(u8::MAX)],
            };
            // SAFETY: command_buffer is recording and image has TRANSFER_DST
            // usage. QGS transitioned it to GENERAL immediately above.
            unsafe {
                image.device.device.cmd_clear_color_image(
                    command_buffer,
                    image.image,
                    vk::ImageLayout::GENERAL,
                    &color,
                    &[color_subresource_range()],
                );
            }
            Ok(())
        },
        None,
        Some(semaphore),
        false,
    )
    .map_err(|_| SyncError::SyncExportFailed)?;
    Ok(pending)
}

fn validate_synced_gpu_copy(
    device: &Arc<GpuDevice>,
    imported_buffer: &GpuBuffer,
    semaphore: &GpuSemaphore,
    pattern: u32,
) -> Result<(), ResourceError> {
    let readback = GpuBuffer::new(
        device.clone(),
        std::mem::size_of::<u32>() as u64,
        vk::BufferUsageFlags::TRANSFER_DST,
        None,
        vk::MemoryPropertyFlags::HOST_VISIBLE,
        vk::MemoryPropertyFlags::HOST_COHERENT,
        false,
    )?;
    let (_, _pending) = one_time_commands(
        device,
        |command_buffer| {
            let copy = vk::BufferCopy::default()
                .src_offset(0)
                .dst_offset(0)
                .size(std::mem::size_of::<u32>() as u64);
            // SAFETY: command buffer is recording and both buffers are live with
            // the required transfer usages.
            unsafe {
                device.device.cmd_copy_buffer(
                    command_buffer,
                    imported_buffer.buffer,
                    readback.buffer,
                    &[copy],
                );
            }
            Ok(())
        },
        Some(semaphore),
        None,
        true,
    )?;
    let bytes = readback.read_bytes(0, std::mem::size_of::<u32>())?;
    let observed = u32::from_le_bytes(bytes.try_into().map_err(|_| ResourceError::ExportFailed)?);
    if observed == pattern {
        Ok(())
    } else {
        Err(ResourceError::ExportFailed)
    }
}

fn run_compute_increment_proof(
    device: &Arc<GpuDevice>,
    imported_buffer: &GpuBuffer,
    semaphore: &GpuSemaphore,
    input_values: &[u32],
    input_bytes: u64,
) -> Result<Vec<u32>, ResourceError> {
    let upload = GpuBuffer::new(
        device.clone(),
        input_bytes,
        vk::BufferUsageFlags::TRANSFER_SRC,
        None,
        vk::MemoryPropertyFlags::HOST_VISIBLE,
        vk::MemoryPropertyFlags::HOST_COHERENT,
        false,
    )?;
    upload.write_bytes(0, cast_u32_slice(input_values))?;
    let readback = GpuBuffer::new(
        device.clone(),
        input_bytes,
        vk::BufferUsageFlags::TRANSFER_DST,
        None,
        vk::MemoryPropertyFlags::HOST_VISIBLE,
        vk::MemoryPropertyFlags::HOST_COHERENT,
        false,
    )?;
    let pipeline = ComputePipelineState::new_storage_buffer(device.clone())?;
    let descriptor = pipeline.write_storage_buffer(imported_buffer)?;
    let groups = u32::try_from(input_values.len() / COMPUTE_LOCAL_SIZE_X as usize)
        .map_err(|_| ResourceError::InvalidBufferSize)?;

    let (_, _pending) = one_time_commands(
        device,
        |command_buffer| {
            let copy = vk::BufferCopy::default().size(input_bytes);
            // SAFETY: command buffer is recording; buffers and pipeline objects
            // are live for the whole submission.
            unsafe {
                device.device.cmd_copy_buffer(
                    command_buffer,
                    upload.buffer,
                    imported_buffer.buffer,
                    &[copy],
                );
                device.device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    pipeline.pipeline,
                );
                device.device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    pipeline.layout,
                    0,
                    &[descriptor.set],
                    &[],
                );
                device.device.cmd_dispatch(command_buffer, groups, 1, 1);
                device.device.cmd_copy_buffer(
                    command_buffer,
                    imported_buffer.buffer,
                    readback.buffer,
                    &[copy],
                );
            }
            Ok(())
        },
        Some(semaphore),
        None,
        true,
    )?;
    let bytes = readback.read_bytes(0, input_bytes as usize)?;
    let mut out = Vec::with_capacity(input_values.len());
    let (chunks, remainder) = bytes.as_chunks::<4>();
    if !remainder.is_empty() {
        return Err(ResourceError::ExportFailed);
    }
    for chunk in chunks {
        out.push(u32::from_le_bytes(*chunk));
    }
    Ok(out)
}

fn run_image_invert_proof(
    device: &Arc<GpuDevice>,
    imported_image: &GpuImage,
    semaphore: &GpuSemaphore,
    input_pixels: &[u8],
) -> Result<Vec<u8>, ResourceError> {
    let readback = GpuBuffer::new(
        device.clone(),
        input_pixels.len() as u64,
        vk::BufferUsageFlags::TRANSFER_DST,
        None,
        vk::MemoryPropertyFlags::HOST_VISIBLE,
        vk::MemoryPropertyFlags::HOST_COHERENT,
        false,
    )?;
    let view = imported_image.create_view().map_err(|err| {
        eprintln!("vulkan imported image view creation failed: {err:?}");
        ResourceError::ExportFailed
    })?;
    let pipeline = ComputePipelineState::new_storage_image(device.clone())?;
    let descriptor = pipeline.write_storage_image(&view)?;

    let (_, _pending) = one_time_commands(
        device,
        |command_buffer| {
            transition_image(
                device,
                command_buffer,
                ImageTransition {
                    image: imported_image.image,
                    old_layout: vk::ImageLayout::GENERAL,
                    new_layout: vk::ImageLayout::GENERAL,
                    src_access: vk::AccessFlags::TRANSFER_WRITE,
                    dst_access: vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                    src_stage: vk::PipelineStageFlags::TRANSFER,
                    dst_stage: vk::PipelineStageFlags::COMPUTE_SHADER,
                },
            );
            // SAFETY: command buffer is recording; image/view/pipeline/buffer are
            // live and usages were validated before import.
            unsafe {
                device.device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    pipeline.pipeline,
                );
                device.device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    pipeline.layout,
                    0,
                    &[descriptor.set],
                    &[],
                );
                device.device.cmd_dispatch(
                    command_buffer,
                    imported_image.extent[0].div_ceil(8),
                    imported_image.extent[1].div_ceil(8),
                    1,
                );
            }
            transition_image(
                device,
                command_buffer,
                ImageTransition {
                    image: imported_image.image,
                    old_layout: vk::ImageLayout::GENERAL,
                    new_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    src_access: vk::AccessFlags::SHADER_WRITE,
                    dst_access: vk::AccessFlags::TRANSFER_READ,
                    src_stage: vk::PipelineStageFlags::COMPUTE_SHADER,
                    dst_stage: vk::PipelineStageFlags::TRANSFER,
                },
            );
            let copy = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(0)
                        .base_array_layer(0)
                        .layer_count(1),
                )
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: imported_image.extent[0],
                    height: imported_image.extent[1],
                    depth: 1,
                });
            // SAFETY: command buffer is recording; image is in TRANSFER_SRC_OPTIMAL
            // and readback buffer has TRANSFER_DST usage.
            unsafe {
                device.device.cmd_copy_image_to_buffer(
                    command_buffer,
                    imported_image.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    readback.buffer,
                    &[copy],
                );
            }
            Ok(())
        },
        Some(semaphore),
        None,
        true,
    )?;
    readback.read_bytes(0, input_pixels.len())
}

struct ImageTransition {
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
}

fn transition_image(
    device: &GpuDevice,
    command_buffer: vk::CommandBuffer,
    transition: ImageTransition,
) {
    let barrier = vk::ImageMemoryBarrier::default()
        .src_access_mask(transition.src_access)
        .dst_access_mask(transition.dst_access)
        .old_layout(transition.old_layout)
        .new_layout(transition.new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(transition.image)
        .subresource_range(color_subresource_range());
    // SAFETY: command buffer is recording; barrier references a live image and
    // a single color subresource range used by QGS RGBA images.
    unsafe {
        device.device.cmd_pipeline_barrier(
            command_buffer,
            transition.src_stage,
            transition.dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

fn color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
}

#[derive(Debug)]
struct ComputePipelineState {
    device: Arc<GpuDevice>,
    shader: vk::ShaderModule,
    descriptor_set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    descriptor_pool: vk::DescriptorPool,
    descriptor_type: vk::DescriptorType,
}

impl ComputePipelineState {
    fn new_storage_buffer(device: Arc<GpuDevice>) -> Result<Self, ResourceError> {
        Self::new(
            device,
            &COMPUTE_INCREMENT_SHADER,
            vk::DescriptorType::STORAGE_BUFFER,
        )
    }

    fn new_storage_image(device: Arc<GpuDevice>) -> Result<Self, ResourceError> {
        Self::new(
            device,
            &IMAGE_INVERT_SHADER,
            vk::DescriptorType::STORAGE_IMAGE,
        )
    }

    fn new(
        device: Arc<GpuDevice>,
        shader_code: &[u32],
        descriptor_type: vk::DescriptorType,
    ) -> Result<Self, ResourceError> {
        let shader_info = vk::ShaderModuleCreateInfo::default().code(shader_code);
        // SAFETY: shader_code contains embedded SPIR-V words controlled by QGS.
        let shader =
            unsafe { device.device.create_shader_module(&shader_info, None) }.map_err(|err| {
                eprintln!("vulkan shader module creation failed: {err:?}");
                ResourceError::ExportFailed
            })?;
        let binding = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(descriptor_type)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];
        let descriptor_set_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&binding);
        // SAFETY: descriptor set layout info references local binding array for call.
        let descriptor_set_layout = unsafe {
            device
                .device
                .create_descriptor_set_layout(&descriptor_set_layout_info, None)
        }
        .map_err(|err| {
            eprintln!("vulkan descriptor set layout creation failed: {err:?}");
            // SAFETY: shader was created above and is owned here on failure.
            unsafe {
                device.device.destroy_shader_module(shader, None);
            }
            ResourceError::ExportFailed
        })?;
        let set_layouts = [descriptor_set_layout];
        let pipeline_layout_info =
            vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        // SAFETY: pipeline layout info references live descriptor set layout.
        let layout = unsafe {
            device
                .device
                .create_pipeline_layout(&pipeline_layout_info, None)
        }
        .map_err(|err| {
            eprintln!("vulkan pipeline layout creation failed: {err:?}");
            // SAFETY: objects are live and owned by this constructor.
            unsafe {
                device
                    .device
                    .destroy_descriptor_set_layout(descriptor_set_layout, None);
                device.device.destroy_shader_module(shader, None);
            }
            ResourceError::ExportFailed
        })?;
        let entry_point = c"main";
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader)
            .name(entry_point);
        let pipeline_info = vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(layout);
        // SAFETY: shader module and pipeline layout are live and compatible with
        // the fixed QGS descriptor contract.
        let pipeline = unsafe {
            device.device.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[pipeline_info],
                None,
            )
        }
        .map_err(|(_, err)| {
            eprintln!("vulkan compute pipeline creation failed: {err:?}");
            // SAFETY: objects are live and owned by this constructor.
            unsafe {
                device.device.destroy_pipeline_layout(layout, None);
                device
                    .device
                    .destroy_descriptor_set_layout(descriptor_set_layout, None);
                device.device.destroy_shader_module(shader, None);
            }
            ResourceError::ExportFailed
        })?[0];
        let pool_size = [vk::DescriptorPoolSize::default()
            .ty(descriptor_type)
            .descriptor_count(1)];
        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_size);
        // SAFETY: descriptor pool info is valid for one fixed descriptor set.
        let descriptor_pool = unsafe {
            device
                .device
                .create_descriptor_pool(&descriptor_pool_info, None)
        }
        .map_err(|err| {
            eprintln!("vulkan descriptor pool creation failed: {err:?}");
            // SAFETY: objects are live and owned by this constructor.
            unsafe {
                device.device.destroy_pipeline(pipeline, None);
                device.device.destroy_pipeline_layout(layout, None);
                device
                    .device
                    .destroy_descriptor_set_layout(descriptor_set_layout, None);
                device.device.destroy_shader_module(shader, None);
            }
            ResourceError::ExportFailed
        })?;

        Ok(Self {
            device,
            shader,
            descriptor_set_layout,
            layout,
            pipeline,
            descriptor_pool,
            descriptor_type,
        })
    }

    fn allocate_set(&self) -> Result<vk::DescriptorSet, ResourceError> {
        let set_layouts = [self.descriptor_set_layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&set_layouts);
        // SAFETY: descriptor pool and set layout are live.
        let sets = unsafe { self.device.device.allocate_descriptor_sets(&allocate_info) }.map_err(
            |err| {
                eprintln!("vulkan descriptor set allocation failed: {err:?}");
                ResourceError::ExportFailed
            },
        )?;
        Ok(sets[0])
    }

    fn write_storage_buffer(&self, buffer: &GpuBuffer) -> Result<DescriptorSet, ResourceError> {
        if self.descriptor_type != vk::DescriptorType::STORAGE_BUFFER {
            return Err(ResourceError::ExportFailed);
        }
        let set = self.allocate_set()?;
        let buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(buffer.buffer)
            .offset(0)
            .range(buffer.size)];
        let write = [vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&buffer_info)];
        // SAFETY: descriptor set is allocated from this pool, and buffer info
        // points to a live buffer for the call.
        unsafe {
            self.device.device.update_descriptor_sets(&write, &[]);
        }
        Ok(DescriptorSet { set })
    }

    fn write_storage_image(&self, view: &GpuImageView) -> Result<DescriptorSet, ResourceError> {
        if self.descriptor_type != vk::DescriptorType::STORAGE_IMAGE {
            return Err(ResourceError::ExportFailed);
        }
        let set = self.allocate_set()?;
        let image_info = [vk::DescriptorImageInfo::default()
            .image_view(view.view)
            .image_layout(vk::ImageLayout::GENERAL)];
        let write = [vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .image_info(&image_info)];
        // SAFETY: descriptor set is allocated from this pool, and image view is
        // live for the duration of the call and subsequent submission.
        unsafe {
            self.device.device.update_descriptor_sets(&write, &[]);
        }
        Ok(DescriptorSet { set })
    }
}

impl Drop for ComputePipelineState {
    fn drop(&mut self) {
        // SAFETY: ComputePipelineState owns these objects. They are destroyed in
        // dependency order after submissions using them have completed.
        unsafe {
            self.device
                .device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.device.destroy_pipeline(self.pipeline, None);
            self.device
                .device
                .destroy_pipeline_layout(self.layout, None);
            self.device
                .device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.device.destroy_shader_module(self.shader, None);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DescriptorSet {
    set: vk::DescriptorSet,
}

fn cast_u32_slice(values: &[u32]) -> &[u8] {
    // SAFETY: u32 has no invalid byte patterns, and the returned byte slice is
    // tied to values' lifetime with length exactly values.len() * size_of::<u32>().
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

fn device_matches_export_source(left: &DeviceDesc, right: &DeviceDesc) -> bool {
    left.backend == right.backend
        && left.vendor_id == right.vendor_id
        && left.device_id == right.device_id
        && left.name == right.name
}

fn choose_memory_type_from_properties(
    properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    required: vk::MemoryPropertyFlags,
    preferred: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let mut fallback = None;
    for index in 0..properties.memory_type_count {
        if (memory_type_bits & (1_u32 << index)) == 0 {
            continue;
        }
        let flags = properties.memory_types[index as usize].property_flags;
        if !flags.contains(required) {
            continue;
        }
        if flags.contains(preferred) {
            return Some(index);
        }
        fallback.get_or_insert(index);
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_properties(types: &[vk::MemoryPropertyFlags]) -> vk::PhysicalDeviceMemoryProperties {
        let mut properties = vk::PhysicalDeviceMemoryProperties {
            memory_type_count: types.len() as u32,
            ..Default::default()
        };
        for (index, flags) in types.iter().enumerate() {
            properties.memory_types[index].property_flags = *flags;
        }
        properties
    }

    #[test]
    fn memory_type_selection_prefers_preferred_flags() {
        let properties = memory_properties(&[
            vk::MemoryPropertyFlags::HOST_VISIBLE,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ]);

        assert_eq!(
            choose_memory_type_from_properties(
                &properties,
                0b111,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
                vk::MemoryPropertyFlags::HOST_COHERENT,
            ),
            Some(1)
        );
    }

    #[test]
    fn memory_type_selection_falls_back_to_required_only() {
        let properties = memory_properties(&[
            vk::MemoryPropertyFlags::HOST_VISIBLE,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ]);

        assert_eq!(
            choose_memory_type_from_properties(
                &properties,
                0b11,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
                vk::MemoryPropertyFlags::HOST_COHERENT,
            ),
            Some(0)
        );
    }

    #[test]
    fn memory_type_selection_respects_type_bits() {
        let properties = memory_properties(&[
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ]);

        assert_eq!(
            choose_memory_type_from_properties(
                &properties,
                0b10,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
                vk::MemoryPropertyFlags::HOST_COHERENT,
            ),
            None
        );
    }

    #[test]
    fn qgs_api_version_maps_vulkan_components() {
        let version = to_qgs_api_version(vk::make_api_version(0, 1, 3, 281));
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 3);
        assert_eq!(version.patch, 281);
    }
}

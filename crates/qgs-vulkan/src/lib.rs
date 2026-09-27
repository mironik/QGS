#![deny(unsafe_code)]

use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use qgs_core::{
    BackendBufferAllocation, BackendImageAllocation, BackendResource, BackendResourceExport,
    BackendSync, BackendSyncExport, DeviceDiscovery, DeviceDiscoveryError, ResourceBackend,
    ResourceError, SyncBackend, SyncError,
};
use qgs_protocol::{
    ApiVersion, BackendApi, BufferDesc, BufferUsageFlags, ComputeCapabilities, CreateSyncRequest,
    DeviceCapabilities, DeviceClass, DeviceDesc, DeviceId, ExportResourceRequest,
    ExportSyncRequest, ExportedResourceMetadata, ExportedSyncMetadata, ExternalHandleType,
    ExternalSharing, ImageDesc, ImageUsageFlags, InteropCapabilities, MemoryCapabilities,
    MemoryHeapDesc, PixelFormat, ResourceKind, SelectedMemoryProperties, SyncExportHandleType,
    SyncKind, MAX_DEVICE_COUNT, MAX_DEVICE_NAME_LEN, MAX_MEMORY_HEAP_COUNT, MAX_MEMORY_TYPE_COUNT,
};
use vulkano::buffer::{
    Buffer, BufferCreateInfo, BufferMemory, BufferUsage, ExternalBufferInfo, RawBuffer, Subbuffer,
};
use vulkano::command_buffer::allocator::{
    StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo,
};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferSubmitInfo, CommandBufferUsage,
    CopyBufferInfo, CopyImageToBufferInfo, PrimaryAutoCommandBuffer, SemaphoreSubmitInfo,
    SubmitInfo,
};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::{DescriptorSet, WriteDescriptorSet};
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::device::{
    Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo, QueueFlags,
};
use vulkano::format::{ClearColorValue, Format};
use vulkano::image::sys::RawImage;
use vulkano::image::view::ImageView;
use vulkano::image::{
    Image, ImageCreateInfo, ImageDrmFormatModifierInfo, ImageFormatInfo, ImageMemory, ImageTiling,
    ImageType, ImageUsage, SampleCount,
};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::memory::allocator::{
    AllocationCreateInfo, GenericMemoryAllocatorCreateInfo, MemoryAllocatePreference,
    MemoryTypeFilter, StandardMemoryAllocator,
};
use vulkano::memory::{
    DedicatedAllocation, ExternalMemoryHandleType, ExternalMemoryHandleTypes, MemoryAllocateInfo,
    MemoryHeapFlags, MemoryImportInfo, MemoryMapInfo, MemoryPropertyFlags, ResourceMemory,
};
use vulkano::pipeline::compute::ComputePipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{
    ComputePipeline, Pipeline, PipelineBindPoint, PipelineLayout, PipelineShaderStageCreateInfo,
};
use vulkano::sync::fence::{Fence, FenceCreateInfo};
use vulkano::sync::semaphore::{
    ExternalSemaphoreHandleType, ExternalSemaphoreHandleTypes, ExternalSemaphoreInfo, Semaphore,
    SemaphoreCreateInfo, SemaphoreType,
};
use vulkano::{Version, VulkanLibrary};

#[allow(unsafe_code)]
mod external_compute;
#[allow(unsafe_code)]
mod external_memory;
#[allow(unsafe_code)]
mod external_sync;

const SHARED_VALIDATION_MARKER: &[u8] = b"QGS-M1S6";
const COMPUTE_LOCAL_SIZE_X: u32 = 64;
const COMPUTE_INCREMENT_SHADER: [u32; 136] = [
    119734787, 65536, 0, 22, 0, 131089, 1, 196622, 0, 1, 393231, 5, 1, 1852399981, 0, 9, 393232, 1,
    17, 64, 1, 1, 196611, 2, 450, 262215, 9, 11, 28, 262215, 10, 6, 4, 327752, 11, 0, 35, 0,
    196679, 11, 2, 262215, 13, 34, 0, 262215, 13, 33, 0, 131091, 2, 196641, 3, 2, 262165, 4, 32, 0,
    262187, 4, 5, 1, 262187, 4, 6, 0, 262167, 7, 4, 3, 262176, 8, 1, 7, 262203, 8, 9, 1, 196637,
    10, 4, 196638, 11, 10, 262176, 12, 12, 11, 262203, 12, 13, 12, 262176, 14, 1, 4, 262176, 15,
    12, 4, 327734, 2, 1, 0, 3, 131320, 16, 327745, 14, 17, 9, 6, 262205, 4, 18, 17, 393281, 15, 19,
    13, 6, 18, 262205, 4, 20, 19, 327808, 4, 21, 20, 5, 196670, 19, 21, 65789, 65592,
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

#[derive(Debug)]
pub struct VulkanDeviceDiscovery {
    devices: Vec<RegisteredDevice>,
}

impl VulkanDeviceDiscovery {
    pub fn new() -> Result<Self, DeviceDiscoveryError> {
        let library = VulkanLibrary::new().map_err(|_| DeviceDiscoveryError::BackendUnavailable)?;
        let instance = Instance::new(library, InstanceCreateInfo::default())
            .map_err(|_| DeviceDiscoveryError::BackendFailed)?;
        let physical_devices = instance
            .enumerate_physical_devices()
            .map_err(|_| DeviceDiscoveryError::BackendFailed)?;

        let mut devices = Vec::new();
        for (index, physical_device) in physical_devices.enumerate() {
            if devices.len() >= MAX_DEVICE_COUNT {
                break;
            }

            let desc = describe_device((index as u64) + 1, &physical_device)?;
            let queue_family_properties = physical_device.queue_family_properties();
            let queue_family_index = queue_family_properties
                .iter()
                .position(|queue| {
                    queue.queue_count > 0
                        && queue.queue_flags.intersects(QueueFlags::GRAPHICS)
                        && queue.queue_flags.intersects(QueueFlags::COMPUTE)
                })
                .or_else(|| {
                    queue_family_properties.iter().position(|queue| {
                        queue.queue_count > 0 && queue.queue_flags.intersects(QueueFlags::COMPUTE)
                    })
                })
                .or_else(|| {
                    queue_family_properties
                        .iter()
                        .position(|queue| queue.queue_count > 0)
                })
                .ok_or(DeviceDiscoveryError::BackendFailed)?
                as u32;
            let supported_extensions = physical_device.supported_extensions();
            let enabled_extensions = DeviceExtensions {
                khr_external_memory: supported_extensions.khr_external_memory,
                khr_external_memory_fd: supported_extensions.khr_external_memory_fd,
                ext_external_memory_dma_buf: supported_extensions.ext_external_memory_dma_buf,
                khr_dedicated_allocation: supported_extensions.khr_dedicated_allocation,
                khr_external_semaphore: supported_extensions.khr_external_semaphore,
                khr_external_semaphore_fd: supported_extensions.khr_external_semaphore_fd,
                ext_image_drm_format_modifier: supported_extensions.ext_image_drm_format_modifier,
                ..DeviceExtensions::empty()
            };
            let (logical_device, mut queues) = Device::new(
                physical_device.clone(),
                DeviceCreateInfo {
                    enabled_extensions,
                    queue_create_infos: vec![QueueCreateInfo {
                        queue_family_index,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            )
            .map_err(|_| DeviceDiscoveryError::BackendFailed)?;
            let queue = queues.next().ok_or(DeviceDiscoveryError::BackendFailed)?;
            let memory_allocator =
                Arc::new(StandardMemoryAllocator::new_default(logical_device.clone()));
            let command_allocator = Arc::new(StandardCommandBufferAllocator::new(
                logical_device.clone(),
                StandardCommandBufferAllocatorCreateInfo {
                    primary_buffer_count: 4,
                    ..Default::default()
                },
            ));
            let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
                logical_device.clone(),
                Default::default(),
            ));

            devices.push(RegisteredDevice {
                desc,
                physical_device,
                logical_device,
                queue,
                memory_allocator,
                command_allocator,
                descriptor_set_allocator,
            });
        }

        Ok(Self { devices })
    }

    pub fn import_and_validate_external_buffer(
        &self,
        source_device: &DeviceDesc,
        metadata: &ExportedResourceMetadata,
        handle: File,
    ) -> Result<(), ResourceError> {
        if metadata.device_id != source_device.id
            || metadata.size_bytes == 0
            || metadata.allocation_size_bytes < metadata.size_bytes
            || metadata.attachment_count != 1
            || !metadata.selected_memory.host_visible
            || !metadata.selected_memory.host_coherent
        {
            return Err(ResourceError::ExportFailed);
        }

        let device = self
            .devices
            .iter()
            .find(|device| device_matches_export_source(&device.desc, source_device))
            .ok_or(ResourceError::UnknownDeviceId)?;

        if metadata.kind != ResourceKind::Buffer {
            return Err(ResourceError::ExportFailed);
        }
        let usage = map_buffer_usage(metadata.buffer_usage.ok_or(ResourceError::ExportFailed)?)?;
        let handle_type = map_external_handle_type(metadata.handle_type)?;
        validate_external_buffer_support(device, usage, handle_type)?;

        let raw_buffer = RawBuffer::new(
            device.logical_device.clone(),
            BufferCreateInfo {
                size: metadata.size_bytes,
                usage,
                external_memory_handle_types: ExternalMemoryHandleTypes::from(handle_type),
                ..Default::default()
            },
        )
        .map_err(|err| {
            eprintln!("vulkan import raw-buffer creation failed: {err}");
            ResourceError::ExportFailed
        })?;

        let dedicated_allocation = metadata
            .dedicated_allocation
            .then_some(DedicatedAllocation::Buffer(&raw_buffer));
        let mut imported_memory = external_memory::import_device_memory(
            device.logical_device.clone(),
            MemoryAllocateInfo {
                allocation_size: metadata.allocation_size_bytes,
                memory_type_index: metadata.backend_memory_type_index,
                dedicated_allocation,
                ..Default::default()
            },
            MemoryImportInfo::Fd {
                handle_type,
                file: handle,
            },
        )
        .map_err(|err| {
            eprintln!("vulkan external memory import failed: {err}");
            ResourceError::ExportFailed
        })?;
        imported_memory
            .map(MemoryMapInfo {
                offset: 0,
                size: metadata.allocation_size_bytes,
                ..Default::default()
            })
            .map_err(|err| {
                eprintln!("vulkan imported memory map failed: {err}");
                ResourceError::ExportFailed
            })?;

        let imported_buffer = raw_buffer
            .bind_memory(ResourceMemory::new_dedicated(imported_memory))
            .map_err(|(err, _, _)| {
                eprintln!("vulkan imported memory bind failed: {err}");
                ResourceError::ExportFailed
            })?;
        let imported_subbuffer: Subbuffer<[u8]> = Subbuffer::from(Arc::new(imported_buffer));
        let read = imported_subbuffer.read().map_err(|err| {
            eprintln!("vulkan imported memory read failed: {err}");
            ResourceError::ExportFailed
        })?;

        if read.get(..SHARED_VALIDATION_MARKER.len()) == Some(SHARED_VALIDATION_MARKER) {
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
        {
            return Err(ResourceError::ExportFailed);
        }
        if resource_metadata.device_id != source_device.id
            || resource_metadata.size_bytes == 0
            || resource_metadata.allocation_size_bytes < resource_metadata.size_bytes
            || resource_metadata.attachment_count != 1
        {
            return Err(ResourceError::ExportFailed);
        }

        let device = self
            .devices
            .iter()
            .find(|device| device_matches_export_source(&device.desc, source_device))
            .ok_or(ResourceError::UnknownDeviceId)?;

        if resource_metadata.kind != ResourceKind::Buffer {
            return Err(ResourceError::ExportFailed);
        }
        let usage = map_buffer_usage(
            resource_metadata
                .buffer_usage
                .ok_or(ResourceError::ExportFailed)?,
        )?;
        let handle_type = map_external_handle_type(resource_metadata.handle_type)?;
        validate_external_buffer_support(device, usage, handle_type)?;
        validate_sync_fd_support(device).map_err(|_| ResourceError::ExportFailed)?;

        let imported_buffer = import_external_buffer(
            device,
            resource_metadata,
            usage,
            handle_type,
            resource_handle,
        )?;
        let imported_semaphore = Semaphore::new(
            device.logical_device.clone(),
            SemaphoreCreateInfo::default(),
        )
        .map_err(|err| {
            eprintln!("vulkan sync import semaphore creation failed: {err}");
            ResourceError::ExportFailed
        })?;
        let imported_semaphore = Arc::new(imported_semaphore);
        external_sync::import_sync_fd(&imported_semaphore, sync_handle).map_err(|err| {
            eprintln!("vulkan sync-fd import failed: {err}");
            ResourceError::ExportFailed
        })?;

        validate_synced_gpu_copy(
            device,
            imported_buffer,
            imported_semaphore,
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
        if input_bytes > resource_metadata.size_bytes {
            return Err(ResourceError::InvalidBufferSize);
        }
        if sync_metadata.handle_type != SyncExportHandleType::SyncFd
            || sync_metadata.attachment_count != 1
        {
            return Err(ResourceError::ExportFailed);
        }
        if resource_metadata.device_id != source_device.id
            || resource_metadata.size_bytes == 0
            || resource_metadata.allocation_size_bytes < resource_metadata.size_bytes
            || resource_metadata.attachment_count != 1
            || resource_metadata.kind != ResourceKind::Buffer
            || !resource_metadata
                .buffer_usage
                .ok_or(ResourceError::ExportFailed)?
                .contains(BufferUsageFlags::STORAGE)
        {
            return Err(ResourceError::ExportFailed);
        }

        let device = self
            .devices
            .iter()
            .find(|device| device_matches_export_source(&device.desc, source_device))
            .ok_or(ResourceError::UnknownDeviceId)?;
        let supports_compute = device
            .physical_device
            .queue_family_properties()
            .get(device.queue.queue_family_index() as usize)
            .is_some_and(|queue| queue.queue_flags.intersects(QueueFlags::COMPUTE));
        if !supports_compute {
            return Err(ResourceError::UnsupportedMemoryRequirements);
        }

        let usage = map_buffer_usage(
            resource_metadata
                .buffer_usage
                .ok_or(ResourceError::ExportFailed)?,
        )?;
        let handle_type = map_external_handle_type(resource_metadata.handle_type)?;
        validate_external_buffer_support(device, usage, handle_type)?;
        validate_sync_fd_support(device).map_err(|_| ResourceError::ExportFailed)?;

        let imported_buffer = import_external_buffer(
            device,
            resource_metadata,
            usage,
            handle_type,
            resource_handle,
        )?;
        let imported_semaphore = Semaphore::new(
            device.logical_device.clone(),
            SemaphoreCreateInfo::default(),
        )
        .map_err(|err| {
            eprintln!("vulkan compute sync import semaphore creation failed: {err}");
            ResourceError::ExportFailed
        })?;
        let imported_semaphore = Arc::new(imported_semaphore);
        external_sync::import_sync_fd(&imported_semaphore, sync_handle).map_err(|err| {
            eprintln!("vulkan compute sync-fd import failed: {err}");
            ResourceError::ExportFailed
        })?;

        run_compute_increment_proof(
            device,
            imported_buffer,
            imported_semaphore,
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
        {
            return Err(ResourceError::ExportFailed);
        }
        if resource_metadata.kind != ResourceKind::Image
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

        let device = self
            .devices
            .iter()
            .find(|device| device_matches_export_source(&device.desc, source_device))
            .ok_or(ResourceError::UnknownDeviceId)?;
        let supports_compute = device
            .physical_device
            .queue_family_properties()
            .get(device.queue.queue_family_index() as usize)
            .is_some_and(|queue| queue.queue_flags.intersects(QueueFlags::COMPUTE));
        if !supports_compute {
            return Err(ResourceError::UnsupportedMemoryRequirements);
        }

        let vk_format = map_pixel_format(format)?;
        let usage = map_image_usage(image_usage)?;
        let handle_type = map_external_handle_type(resource_metadata.handle_type)?;
        let (tiling, drm_format_modifiers) = if handle_type == ExternalMemoryHandleType::DmaBuf {
            if let Some(modifier) = resource_metadata.backend_image_layout_token {
                validate_external_image_support(
                    device,
                    vk_format,
                    usage,
                    ImageTiling::DrmFormatModifier,
                    Some(modifier),
                    handle_type,
                )?;
                (ImageTiling::DrmFormatModifier, vec![modifier])
            } else {
                validate_external_image_support(
                    device,
                    vk_format,
                    usage,
                    ImageTiling::Linear,
                    None,
                    handle_type,
                )?;
                (ImageTiling::Linear, Vec::new())
            }
        } else {
            validate_external_image_support(
                device,
                vk_format,
                usage,
                ImageTiling::Optimal,
                None,
                handle_type,
            )?;
            (ImageTiling::Optimal, Vec::new())
        };
        validate_sync_fd_support(device).map_err(|_| ResourceError::ExportFailed)?;

        let imported_image = import_external_image(
            device,
            ImportedImageInfo {
                metadata: resource_metadata,
                format: vk_format,
                usage,
                tiling,
                drm_format_modifiers,
                handle_type,
                handle: resource_handle,
            },
        )?;
        let imported_semaphore = Semaphore::new(
            device.logical_device.clone(),
            SemaphoreCreateInfo::default(),
        )
        .map_err(|err| {
            eprintln!("vulkan image sync import semaphore creation failed: {err}");
            ResourceError::ExportFailed
        })?;
        let imported_semaphore = Arc::new(imported_semaphore);
        external_sync::import_sync_fd(&imported_semaphore, sync_handle).map_err(|err| {
            eprintln!("vulkan image sync-fd import failed: {err}");
            ResourceError::ExportFailed
        })?;

        run_image_invert_proof(device, imported_image, imported_semaphore, input_pixels)
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

        Ok(describe_capabilities(
            device.desc.id,
            &device.physical_device,
        ))
    }
}

impl ResourceBackend for VulkanDeviceDiscovery {
    fn create_buffer(&self, desc: &BufferDesc) -> Result<BackendBufferAllocation, ResourceError> {
        let device = self
            .devices
            .iter()
            .find(|device| device.desc.id == desc.device_id)
            .ok_or(ResourceError::UnknownDeviceId)?;

        let usage = map_buffer_usage(desc.usage)?;
        let memory_type_filter = map_memory_preference(desc);
        let external_handle_types = match desc.external_sharing {
            ExternalSharing::None => ExternalMemoryHandleTypes::empty(),
            ExternalSharing::Required { handle_type } => {
                let handle_type = map_external_handle_type(handle_type)?;
                validate_external_buffer_support(device, usage, handle_type)?;
                ExternalMemoryHandleTypes::from(handle_type)
            }
        };
        let allocator = if external_handle_types.is_empty() {
            device.memory_allocator.clone()
        } else {
            Arc::new(export_memory_allocator(
                device.logical_device.clone(),
                external_handle_types,
            ))
        };
        let buffer = Buffer::new_slice::<u8>(
            allocator,
            BufferCreateInfo {
                usage,
                external_memory_handle_types: external_handle_types,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter,
                allocate_preference: if external_handle_types.is_empty() {
                    MemoryAllocatePreference::Unknown
                } else {
                    MemoryAllocatePreference::AlwaysAllocate
                },
                ..Default::default()
            },
            desc.size_bytes,
        )
        .map_err(|err| {
            eprintln!("vulkan buffer allocation failed: {err}");
            ResourceError::AllocationFailed
        })?;

        let selected_memory = selected_memory_properties(&device.physical_device, &buffer)?;

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
            .ok_or(ResourceError::UnknownDeviceId)?;

        let format = map_pixel_format(desc.format)?;
        let usage = map_image_usage(desc.usage)?;
        let (external_handle_types, tiling, drm_format_modifiers) = match desc.external_sharing {
            ExternalSharing::None => (
                ExternalMemoryHandleTypes::empty(),
                ImageTiling::Optimal,
                Vec::new(),
            ),
            ExternalSharing::Required { handle_type } => {
                let handle_type = map_external_handle_type(handle_type)?;
                let (tiling, drm_format_modifiers) =
                    external_image_layout(device, format, usage, handle_type)?;
                (
                    ExternalMemoryHandleTypes::from(handle_type),
                    tiling,
                    drm_format_modifiers,
                )
            }
        };
        let allocator = if external_handle_types.is_empty() {
            device.memory_allocator.clone()
        } else {
            Arc::new(export_memory_allocator(
                device.logical_device.clone(),
                external_handle_types,
            ))
        };
        let image = Image::new(
            allocator,
            ImageCreateInfo {
                image_type: ImageType::Dim2d,
                format,
                extent: [desc.width, desc.height, 1],
                usage,
                tiling,
                samples: SampleCount::Sample1,
                drm_format_modifiers,
                external_memory_handle_types: external_handle_types,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
                allocate_preference: if external_handle_types.is_empty() {
                    MemoryAllocatePreference::Unknown
                } else {
                    MemoryAllocatePreference::AlwaysAllocate
                },
                ..Default::default()
            },
        )
        .map_err(|err| {
            eprintln!("vulkan image allocation failed: {err}");
            ResourceError::AllocationFailed
        })?;

        let selected_memory = selected_image_memory_properties(&device.physical_device, &image)?;

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
            .ok_or(SyncError::UnknownDeviceId)?;
        validate_sync_fd_support(device)?;

        let semaphore = Semaphore::new(
            device.logical_device.clone(),
            SemaphoreCreateInfo {
                export_handle_types: ExternalSemaphoreHandleTypes::SYNC_FD,
                ..Default::default()
            },
        )
        .map_err(|err| {
            eprintln!("vulkan sync creation failed: {err}");
            SyncError::SyncExportFailed
        })?;

        Ok(Box::new(VulkanSyncResource {
            device_id: request.device_id,
            handle_type: request.handle_type,
            semaphore: Arc::new(semaphore),
            queue: device.queue.clone(),
            command_allocator: device.command_allocator.clone(),
            exported: AtomicBool::new(false),
            pending_command_buffers: Mutex::new(Vec::new()),
        }))
    }
}

#[derive(Debug)]
struct RegisteredDevice {
    desc: DeviceDesc,
    physical_device: Arc<PhysicalDevice>,
    logical_device: Arc<Device>,
    queue: Arc<Queue>,
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
}

#[derive(Debug)]
struct VulkanBufferResource {
    buffer: Subbuffer<[u8]>,
    desc: BufferDesc,
    selected_memory: SelectedMemoryProperties,
}

#[derive(Debug)]
struct VulkanImageResource {
    image: Arc<Image>,
    desc: ImageDesc,
    selected_memory: SelectedMemoryProperties,
}

struct VulkanSyncResource {
    device_id: DeviceId,
    handle_type: SyncExportHandleType,
    semaphore: Arc<Semaphore>,
    queue: Arc<Queue>,
    command_allocator: Arc<StandardCommandBufferAllocator>,
    exported: AtomicBool,
    pending_command_buffers: Mutex<Vec<Arc<PrimaryAutoCommandBuffer>>>,
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

        let command_buffer =
            if let Some(buffer) = resource.as_any().downcast_ref::<VulkanBufferResource>() {
                if buffer.desc.device_id != self.device_id {
                    return Err(SyncError::UnknownResource);
                }
                submit_gpu_fill(
                    self.command_allocator.clone(),
                    self.queue.clone(),
                    buffer.buffer.clone(),
                    self.semaphore.clone(),
                    request.fill_pattern,
                )?
            } else if let Some(image) = resource.as_any().downcast_ref::<VulkanImageResource>() {
                if image.desc.device_id != self.device_id {
                    return Err(SyncError::UnknownResource);
                }
                submit_gpu_image_clear(
                    self.command_allocator.clone(),
                    self.queue.clone(),
                    image.image.clone(),
                    self.semaphore.clone(),
                    request.fill_pattern,
                )?
            } else {
                return Err(SyncError::SyncExportFailed);
            };
        self.pending_command_buffers
            .lock()
            .map_err(|_| SyncError::SyncExportFailed)?
            .push(command_buffer);

        let handle = external_sync::export_sync_fd(&self.semaphore).map_err(|err| {
            eprintln!("vulkan sync-fd export failed: {err}");
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
    fn as_any(&self) -> &dyn std::any::Any {
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

        write_validation_marker(&self.buffer)?;
        let (memory_type_index, allocation_size_bytes, dedicated_allocation) =
            buffer_memory_export_info(&self.buffer)?;
        let vk_handle_type = map_external_handle_type(handle_type)?;
        let handle = match self.buffer.buffer().memory() {
            BufferMemory::Normal(memory) => memory
                .device_memory()
                .export_fd(vk_handle_type)
                .map_err(|err| {
                    eprintln!("vulkan memory export failed: {err}");
                    ResourceError::ExportFailed
                })?,
            BufferMemory::Sparse | BufferMemory::External => {
                return Err(ResourceError::ExportFailed)
            }
            _ => return Err(ResourceError::ExportFailed),
        };

        Ok(BackendResourceExport {
            metadata: ExportedResourceMetadata {
                resource_id: request.resource_id,
                device_id: self.desc.device_id,
                kind: ResourceKind::Buffer,
                size_bytes: self.desc.size_bytes,
                allocation_size_bytes,
                buffer_usage: Some(self.desc.usage),
                image_width: None,
                image_height: None,
                pixel_format: None,
                image_usage: None,
                backend_memory_type_index: memory_type_index,
                backend_image_layout_token: None,
                handle_type,
                selected_memory: self.selected_memory,
                dedicated_allocation,
                attachment_count: 1,
            },
            handle,
        })
    }
}

impl BackendResource for VulkanImageResource {
    fn as_any(&self) -> &dyn std::any::Any {
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

        let (memory_type_index, allocation_size_bytes, dedicated_allocation) =
            image_memory_export_info(&self.image)?;
        let backend_image_layout_token = if handle_type == ExternalHandleType::DmaBuf {
            self.image
                .drm_format_modifier()
                .map(|(modifier, _)| modifier)
        } else {
            None
        };
        let vk_handle_type = map_external_handle_type(handle_type)?;
        let handle = match self.image.memory() {
            ImageMemory::Normal(memory) => memory
                .first()
                .ok_or(ResourceError::ExportFailed)?
                .device_memory()
                .export_fd(vk_handle_type)
                .map_err(|err| {
                    eprintln!("vulkan image memory export failed: {err}");
                    ResourceError::ExportFailed
                })?,
            ImageMemory::Sparse | ImageMemory::Swapchain { .. } | ImageMemory::External => {
                return Err(ResourceError::ExportFailed);
            }
            _ => return Err(ResourceError::ExportFailed),
        };

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
                allocation_size_bytes,
                buffer_usage: None,
                image_width: Some(self.desc.width),
                image_height: Some(self.desc.height),
                pixel_format: Some(self.desc.format),
                image_usage: Some(self.desc.usage),
                backend_memory_type_index: memory_type_index,
                backend_image_layout_token,
                handle_type,
                selected_memory: self.selected_memory,
                dedicated_allocation,
                attachment_count: 1,
            },
            handle,
        })
    }
}

fn describe_device(
    raw_id: u64,
    physical_device: &PhysicalDevice,
) -> Result<DeviceDesc, DeviceDiscoveryError> {
    let properties = physical_device.properties();
    let name = bounded_name(&properties.device_name);
    let api_version = to_qgs_api_version(physical_device.api_version());
    let id = DeviceId::new(raw_id)?;

    Ok(DeviceDesc {
        id,
        class: classify_device_type(properties.device_type),
        vendor_id: properties.vendor_id,
        device_id: properties.device_id,
        name,
        backend: BackendApi::Vulkan,
        api_version,
        driver_version: properties.driver_version,
    })
}

fn describe_capabilities(
    device_id: DeviceId,
    physical_device: &PhysicalDevice,
) -> DeviceCapabilities {
    DeviceCapabilities {
        device_id,
        compute: describe_compute_capabilities(physical_device),
        memory: describe_memory_capabilities(physical_device),
        interop: describe_interop_capabilities(physical_device),
    }
}

fn describe_compute_capabilities(physical_device: &PhysicalDevice) -> ComputeCapabilities {
    let properties = physical_device.properties();
    let supported = physical_device
        .queue_family_properties()
        .iter()
        .any(|queue| queue.queue_count > 0 && queue.queue_flags.intersects(QueueFlags::COMPUTE));

    ComputeCapabilities {
        supported,
        max_workgroup_count: properties.max_compute_work_group_count,
        max_workgroup_size: properties.max_compute_work_group_size,
        max_workgroup_invocations: properties.max_compute_work_group_invocations,
    }
}

fn describe_memory_capabilities(physical_device: &PhysicalDevice) -> MemoryCapabilities {
    let memory = physical_device.memory_properties();

    let heaps = memory
        .memory_heaps
        .iter()
        .take(MAX_MEMORY_HEAP_COUNT)
        .map(|heap| MemoryHeapDesc {
            size_bytes: heap.size,
            device_local: heap.flags.intersects(MemoryHeapFlags::DEVICE_LOCAL),
        })
        .collect();

    let memory_types = memory
        .memory_types
        .iter()
        .take(MAX_MEMORY_TYPE_COUNT)
        .collect::<Vec<_>>();

    MemoryCapabilities {
        heaps,
        memory_type_count: u16::try_from(memory_types.len()).unwrap_or(u16::MAX),
        host_visible: memory_types.iter().any(|memory_type| {
            memory_type
                .property_flags
                .intersects(MemoryPropertyFlags::HOST_VISIBLE)
        }),
        host_coherent: memory_types.iter().any(|memory_type| {
            memory_type
                .property_flags
                .intersects(MemoryPropertyFlags::HOST_COHERENT)
        }),
        device_local: memory_types.iter().any(|memory_type| {
            memory_type
                .property_flags
                .intersects(MemoryPropertyFlags::DEVICE_LOCAL)
        }),
    }
}

fn describe_interop_capabilities(physical_device: &PhysicalDevice) -> InteropCapabilities {
    let extensions = physical_device.supported_extensions();

    InteropCapabilities {
        external_memory_fd: extensions.khr_external_memory_fd,
        dma_buf: extensions.ext_external_memory_dma_buf,
        external_semaphore_fd: extensions.khr_external_semaphore_fd,
        external_fence_fd: extensions.khr_external_fence_fd,
    }
}

pub fn classify_device_type(device_type: PhysicalDeviceType) -> DeviceClass {
    match device_type {
        PhysicalDeviceType::IntegratedGpu => DeviceClass::IntegratedGpu,
        PhysicalDeviceType::DiscreteGpu => DeviceClass::DiscreteGpu,
        PhysicalDeviceType::Cpu => DeviceClass::Software,
        PhysicalDeviceType::VirtualGpu | PhysicalDeviceType::Other => DeviceClass::Other,
        _ => DeviceClass::Other,
    }
}

fn bounded_name(name: &str) -> String {
    if name.len() <= MAX_DEVICE_NAME_LEN {
        return name.to_owned();
    }

    let mut end = MAX_DEVICE_NAME_LEN;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_owned()
}

fn to_qgs_api_version(version: Version) -> ApiVersion {
    ApiVersion::new(
        u16::try_from(version.major).unwrap_or(u16::MAX),
        u16::try_from(version.minor).unwrap_or(u16::MAX),
        u16::try_from(version.patch).unwrap_or(u16::MAX),
    )
}

fn map_buffer_usage(usage: BufferUsageFlags) -> Result<BufferUsage, ResourceError> {
    let mut mapped = BufferUsage::empty();
    if usage.contains(BufferUsageFlags::TRANSFER_SRC) {
        mapped |= BufferUsage::TRANSFER_SRC;
    }
    if usage.contains(BufferUsageFlags::TRANSFER_DST) {
        mapped |= BufferUsage::TRANSFER_DST;
    }
    if usage.contains(BufferUsageFlags::STORAGE) {
        mapped |= BufferUsage::STORAGE_BUFFER;
    }

    if mapped.is_empty() {
        Err(ResourceError::UnsupportedMemoryRequirements)
    } else {
        Ok(mapped)
    }
}

fn map_pixel_format(format: PixelFormat) -> Result<Format, ResourceError> {
    match format {
        PixelFormat::Rgba8Unorm => Ok(Format::R8G8B8A8_UNORM),
    }
}

fn map_image_usage(usage: ImageUsageFlags) -> Result<ImageUsage, ResourceError> {
    let mut mapped = ImageUsage::empty();
    if usage.contains(ImageUsageFlags::TRANSFER_SRC) {
        mapped |= ImageUsage::TRANSFER_SRC;
    }
    if usage.contains(ImageUsageFlags::TRANSFER_DST) {
        mapped |= ImageUsage::TRANSFER_DST;
    }
    if usage.contains(ImageUsageFlags::STORAGE) {
        mapped |= ImageUsage::STORAGE;
    }

    if mapped.is_empty() {
        Err(ResourceError::UnsupportedImageUsage)
    } else {
        Ok(mapped)
    }
}

fn map_memory_preference(desc: &BufferDesc) -> MemoryTypeFilter {
    let preference = desc.memory_preference;
    let mut filter = MemoryTypeFilter::empty();

    if preference.device_preferred {
        filter = filter | MemoryTypeFilter::PREFER_DEVICE;
    }
    if preference.host_visible_required {
        filter = filter | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE;
    }
    if preference.host_coherent_preferred {
        filter.preferred_flags |= MemoryPropertyFlags::HOST_COHERENT;
    }

    filter
}

fn selected_memory_properties(
    physical_device: &PhysicalDevice,
    buffer: &Subbuffer<[u8]>,
) -> Result<SelectedMemoryProperties, ResourceError> {
    let memory_type_index = match buffer.buffer().memory() {
        BufferMemory::Normal(memory) => memory.device_memory().memory_type_index(),
        BufferMemory::Sparse | BufferMemory::External => {
            return Err(ResourceError::AllocationFailed);
        }
        _ => return Err(ResourceError::AllocationFailed),
    };
    let memory_type = physical_device
        .memory_properties()
        .memory_types
        .get(memory_type_index as usize)
        .ok_or(ResourceError::AllocationFailed)?;
    let flags = memory_type.property_flags;

    Ok(SelectedMemoryProperties {
        device_local: flags.intersects(MemoryPropertyFlags::DEVICE_LOCAL),
        host_visible: flags.intersects(MemoryPropertyFlags::HOST_VISIBLE),
        host_coherent: flags.intersects(MemoryPropertyFlags::HOST_COHERENT),
    })
}

fn selected_image_memory_properties(
    physical_device: &PhysicalDevice,
    image: &Image,
) -> Result<SelectedMemoryProperties, ResourceError> {
    let memory_type_index = match image.memory() {
        ImageMemory::Normal(memory) => memory
            .first()
            .ok_or(ResourceError::AllocationFailed)?
            .device_memory()
            .memory_type_index(),
        ImageMemory::Sparse | ImageMemory::Swapchain { .. } | ImageMemory::External => {
            return Err(ResourceError::AllocationFailed);
        }
        _ => return Err(ResourceError::AllocationFailed),
    };
    let memory_type = physical_device
        .memory_properties()
        .memory_types
        .get(memory_type_index as usize)
        .ok_or(ResourceError::AllocationFailed)?;
    let flags = memory_type.property_flags;

    Ok(SelectedMemoryProperties {
        device_local: flags.intersects(MemoryPropertyFlags::DEVICE_LOCAL),
        host_visible: flags.intersects(MemoryPropertyFlags::HOST_VISIBLE),
        host_coherent: flags.intersects(MemoryPropertyFlags::HOST_COHERENT),
    })
}

fn map_external_handle_type(
    handle_type: ExternalHandleType,
) -> Result<ExternalMemoryHandleType, ResourceError> {
    match handle_type {
        ExternalHandleType::DmaBuf => Ok(ExternalMemoryHandleType::DmaBuf),
        ExternalHandleType::OpaqueFd => Ok(ExternalMemoryHandleType::OpaqueFd),
    }
}

fn validate_external_buffer_support(
    device: &RegisteredDevice,
    usage: BufferUsage,
    handle_type: ExternalMemoryHandleType,
) -> Result<(), ResourceError> {
    let extensions = device.logical_device.enabled_extensions();
    if !extensions.khr_external_memory_fd {
        return Err(ResourceError::UnsupportedExternalHandleType);
    }
    if handle_type == ExternalMemoryHandleType::DmaBuf && !extensions.ext_external_memory_dma_buf {
        return Err(ResourceError::UnsupportedExternalHandleType);
    }

    let mut external_buffer_info = ExternalBufferInfo::handle_type(handle_type);
    external_buffer_info.usage = usage;

    let properties = device
        .physical_device
        .external_buffer_properties(external_buffer_info)
        .map_err(|err| {
            eprintln!("vulkan external-buffer property query failed: {err}");
            ResourceError::UnsupportedExternalHandleType
        })?;

    if !properties.external_memory_properties.exportable
        || !properties.external_memory_properties.importable
    {
        return Err(ResourceError::ResourceNotExportable);
    }

    Ok(())
}

fn external_image_layout(
    device: &RegisteredDevice,
    format: Format,
    usage: ImageUsage,
    handle_type: ExternalMemoryHandleType,
) -> Result<(ImageTiling, Vec<u64>), ResourceError> {
    if handle_type == ExternalMemoryHandleType::DmaBuf {
        validate_external_image_support(
            device,
            format,
            usage,
            ImageTiling::Linear,
            None,
            handle_type,
        )?;
        return Ok((ImageTiling::Linear, Vec::new()));
    }

    validate_external_image_support(
        device,
        format,
        usage,
        ImageTiling::Optimal,
        None,
        handle_type,
    )?;
    Ok((ImageTiling::Optimal, Vec::new()))
}

fn validate_external_image_support(
    device: &RegisteredDevice,
    format: Format,
    usage: ImageUsage,
    tiling: ImageTiling,
    drm_format_modifier: Option<u64>,
    handle_type: ExternalMemoryHandleType,
) -> Result<(), ResourceError> {
    let extensions = device.logical_device.enabled_extensions();
    if !extensions.khr_external_memory_fd {
        return Err(ResourceError::UnsupportedExternalHandleType);
    }
    if handle_type == ExternalMemoryHandleType::DmaBuf && !extensions.ext_external_memory_dma_buf {
        return Err(ResourceError::UnsupportedExternalHandleType);
    }

    let properties = device
        .physical_device
        .image_format_properties(ImageFormatInfo {
            format,
            image_type: ImageType::Dim2d,
            tiling,
            usage,
            drm_format_modifier_info: drm_format_modifier.map(|drm_format_modifier| {
                ImageDrmFormatModifierInfo {
                    drm_format_modifier,
                    ..Default::default()
                }
            }),
            external_memory_handle_type: Some(handle_type),
            ..Default::default()
        })
        .map_err(|err| {
            eprintln!("vulkan external-image property query failed: {err}");
            ResourceError::UnsupportedImageExternalSharing
        })?
        .ok_or(ResourceError::UnsupportedImageExternalSharing)?;

    if !properties.external_memory_properties.exportable
        || !properties.external_memory_properties.importable
    {
        return Err(ResourceError::ResourceNotExportable);
    }

    Ok(())
}

fn validate_sync_fd_support(device: &RegisteredDevice) -> Result<(), SyncError> {
    let extensions = device.logical_device.enabled_extensions();
    if !extensions.khr_external_semaphore_fd {
        return Err(SyncError::UnsupportedSyncHandleType);
    }

    let mut info = ExternalSemaphoreInfo::handle_type(ExternalSemaphoreHandleType::SyncFd);
    info.semaphore_type = SemaphoreType::Binary;
    let properties = device
        .physical_device
        .external_semaphore_properties(info)
        .map_err(|err| {
            eprintln!("vulkan external semaphore property query failed: {err}");
            SyncError::UnsupportedSyncHandleType
        })?;

    if properties.exportable && properties.importable {
        Ok(())
    } else {
        Err(SyncError::UnsupportedSyncHandleType)
    }
}

fn submit_gpu_fill(
    command_allocator: Arc<StandardCommandBufferAllocator>,
    queue: Arc<Queue>,
    buffer: Subbuffer<[u8]>,
    signal_semaphore: Arc<Semaphore>,
    pattern: u32,
) -> Result<Arc<PrimaryAutoCommandBuffer>, SyncError> {
    if buffer.size() < 4 || !buffer.size().is_multiple_of(4) {
        return Err(SyncError::SyncExportFailed);
    }

    let fill_buffer = buffer.reinterpret::<[u32]>();
    let mut builder = AutoCommandBufferBuilder::primary(
        command_allocator,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .map_err(|err| {
        eprintln!("vulkan producer command buffer creation failed: {err}");
        SyncError::SyncExportFailed
    })?;
    builder.fill_buffer(fill_buffer, pattern).map_err(|err| {
        eprintln!("vulkan producer fill command recording failed: {err}");
        SyncError::SyncExportFailed
    })?;
    let command_buffer = builder.build().map_err(|err| {
        eprintln!("vulkan producer command buffer build failed: {err}");
        SyncError::SyncExportFailed
    })?;

    let submit = SubmitInfo {
        command_buffers: vec![CommandBufferSubmitInfo::new(command_buffer.clone())],
        signal_semaphores: vec![SemaphoreSubmitInfo::new(signal_semaphore)],
        ..Default::default()
    };
    external_sync::submit_queue(&queue, &[submit], None).map_err(|err| {
        eprintln!("vulkan producer queue submit failed: {err}");
        SyncError::SyncExportFailed
    })?;

    Ok(command_buffer)
}

fn submit_gpu_image_clear(
    command_allocator: Arc<StandardCommandBufferAllocator>,
    queue: Arc<Queue>,
    image: Arc<Image>,
    signal_semaphore: Arc<Semaphore>,
    pattern: u32,
) -> Result<Arc<PrimaryAutoCommandBuffer>, SyncError> {
    let r = f32::from((pattern & 0xff) as u8) / 255.0;
    let g = f32::from(((pattern >> 8) & 0xff) as u8) / 255.0;
    let b = f32::from(((pattern >> 16) & 0xff) as u8) / 255.0;
    let a = f32::from(((pattern >> 24) & 0xff) as u8) / 255.0;
    let mut builder = AutoCommandBufferBuilder::primary(
        command_allocator,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .map_err(|err| {
        eprintln!("vulkan image producer command buffer creation failed: {err}");
        SyncError::SyncExportFailed
    })?;
    let mut clear = ClearColorImageInfo::image(image);
    clear.clear_value = ClearColorValue::Float([r, g, b, a]);
    builder.clear_color_image(clear).map_err(|err| {
        eprintln!("vulkan image producer clear command recording failed: {err}");
        SyncError::SyncExportFailed
    })?;
    let command_buffer = builder.build().map_err(|err| {
        eprintln!("vulkan image producer command buffer build failed: {err}");
        SyncError::SyncExportFailed
    })?;

    let submit = SubmitInfo {
        command_buffers: vec![CommandBufferSubmitInfo::new(command_buffer.clone())],
        signal_semaphores: vec![SemaphoreSubmitInfo::new(signal_semaphore)],
        ..Default::default()
    };
    external_sync::submit_queue(&queue, &[submit], None).map_err(|err| {
        eprintln!("vulkan image producer queue submit failed: {err}");
        SyncError::SyncExportFailed
    })?;

    Ok(command_buffer)
}

fn import_external_buffer(
    device: &RegisteredDevice,
    metadata: &ExportedResourceMetadata,
    usage: BufferUsage,
    handle_type: ExternalMemoryHandleType,
    handle: File,
) -> Result<Subbuffer<[u8]>, ResourceError> {
    let raw_buffer = RawBuffer::new(
        device.logical_device.clone(),
        BufferCreateInfo {
            size: metadata.size_bytes,
            usage,
            external_memory_handle_types: ExternalMemoryHandleTypes::from(handle_type),
            ..Default::default()
        },
    )
    .map_err(|err| {
        eprintln!("vulkan import raw-buffer creation failed: {err}");
        ResourceError::ExportFailed
    })?;

    let dedicated_allocation = metadata
        .dedicated_allocation
        .then_some(DedicatedAllocation::Buffer(&raw_buffer));
    let mut imported_memory = external_memory::import_device_memory(
        device.logical_device.clone(),
        MemoryAllocateInfo {
            allocation_size: metadata.allocation_size_bytes,
            memory_type_index: metadata.backend_memory_type_index,
            dedicated_allocation,
            ..Default::default()
        },
        MemoryImportInfo::Fd {
            handle_type,
            file: handle,
        },
    )
    .map_err(|err| {
        eprintln!("vulkan external memory import failed: {err}");
        ResourceError::ExportFailed
    })?;
    imported_memory
        .map(MemoryMapInfo {
            offset: 0,
            size: metadata.allocation_size_bytes,
            ..Default::default()
        })
        .map_err(|err| {
            eprintln!("vulkan imported memory map failed: {err}");
            ResourceError::ExportFailed
        })?;

    let imported_buffer = raw_buffer
        .bind_memory(ResourceMemory::new_dedicated(imported_memory))
        .map_err(|(err, _, _)| {
            eprintln!("vulkan imported memory bind failed: {err}");
            ResourceError::ExportFailed
        })?;
    Ok(Subbuffer::from(Arc::new(imported_buffer)))
}

struct ImportedImageInfo<'a> {
    metadata: &'a ExportedResourceMetadata,
    format: Format,
    usage: ImageUsage,
    tiling: ImageTiling,
    drm_format_modifiers: Vec<u64>,
    handle_type: ExternalMemoryHandleType,
    handle: File,
}

fn import_external_image(
    device: &RegisteredDevice,
    info: ImportedImageInfo<'_>,
) -> Result<Arc<Image>, ResourceError> {
    let ImportedImageInfo {
        metadata,
        format,
        usage,
        tiling,
        drm_format_modifiers,
        handle_type,
        handle,
    } = info;
    let width = metadata.image_width.ok_or(ResourceError::ExportFailed)?;
    let height = metadata.image_height.ok_or(ResourceError::ExportFailed)?;
    let raw_image = RawImage::new(
        device.logical_device.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format,
            extent: [width, height, 1],
            usage,
            tiling,
            samples: SampleCount::Sample1,
            drm_format_modifiers,
            external_memory_handle_types: ExternalMemoryHandleTypes::from(handle_type),
            ..Default::default()
        },
    )
    .map_err(|err| {
        eprintln!("vulkan import raw-image creation failed: {err}");
        ResourceError::ExportFailed
    })?;

    let dedicated_allocation = metadata
        .dedicated_allocation
        .then_some(DedicatedAllocation::Image(&raw_image));
    let imported_memory = external_memory::import_device_memory(
        device.logical_device.clone(),
        MemoryAllocateInfo {
            allocation_size: metadata.allocation_size_bytes,
            memory_type_index: metadata.backend_memory_type_index,
            dedicated_allocation,
            ..Default::default()
        },
        MemoryImportInfo::Fd {
            handle_type,
            file: handle,
        },
    )
    .map_err(|err| {
        eprintln!("vulkan external image memory import failed: {err}");
        ResourceError::ExportFailed
    })?;

    let imported_image = raw_image
        .bind_memory([ResourceMemory::new_dedicated(imported_memory)])
        .map_err(|(err, _, _)| {
            eprintln!("vulkan imported image memory bind failed: {err}");
            ResourceError::ExportFailed
        })?;
    Ok(Arc::new(imported_image))
}

fn validate_synced_gpu_copy(
    device: &RegisteredDevice,
    imported_buffer: Subbuffer<[u8]>,
    wait_semaphore: Arc<Semaphore>,
    expected_pattern: u32,
) -> Result<(), ResourceError> {
    let readback = Buffer::new_slice::<u8>(
        device.memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        4,
    )
    .map_err(|err| {
        eprintln!("vulkan readback buffer allocation failed: {err}");
        ResourceError::AllocationFailed
    })?;

    let mut builder = AutoCommandBufferBuilder::primary(
        device.command_allocator.clone(),
        device.queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .map_err(|err| {
        eprintln!("vulkan consumer command buffer creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    builder
        .copy_buffer(CopyBufferInfo::buffers(
            imported_buffer.slice(..4),
            readback.clone(),
        ))
        .map_err(|err| {
            eprintln!("vulkan consumer copy command recording failed: {err}");
            ResourceError::ExportFailed
        })?;
    let command_buffer = builder.build().map_err(|err| {
        eprintln!("vulkan consumer command buffer build failed: {err}");
        ResourceError::ExportFailed
    })?;
    let fence = Arc::new(
        Fence::new(device.logical_device.clone(), FenceCreateInfo::default()).map_err(|err| {
            eprintln!("vulkan consumer fence creation failed: {err}");
            ResourceError::ExportFailed
        })?,
    );
    let submit = SubmitInfo {
        wait_semaphores: vec![SemaphoreSubmitInfo::new(wait_semaphore)],
        command_buffers: vec![CommandBufferSubmitInfo::new(command_buffer)],
        ..Default::default()
    };
    external_sync::submit_queue(&device.queue, &[submit], Some(&fence)).map_err(|err| {
        eprintln!("vulkan consumer queue submit failed: {err}");
        ResourceError::ExportFailed
    })?;
    fence
        .wait(Some(std::time::Duration::from_secs(5)))
        .map_err(|err| {
            eprintln!("vulkan consumer fence wait failed: {err}");
            ResourceError::ExportFailed
        })?;

    let read = readback.read().map_err(|err| {
        eprintln!("vulkan readback map failed: {err}");
        ResourceError::ExportFailed
    })?;
    if read.get(..4) == Some(&expected_pattern.to_le_bytes()) {
        Ok(())
    } else {
        Err(ResourceError::ExportFailed)
    }
}

fn run_image_invert_proof(
    device: &RegisteredDevice,
    imported_image: Arc<Image>,
    wait_semaphore: Arc<Semaphore>,
    input_pixels: &[u8],
) -> Result<Vec<u8>, ResourceError> {
    let readback = Buffer::new_slice::<u8>(
        device.memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        input_pixels.len() as u64,
    )
    .map_err(|err| {
        eprintln!("vulkan image readback allocation failed: {err}");
        ResourceError::AllocationFailed
    })?;

    let shader =
        external_compute::create_shader_module(device.logical_device.clone(), &IMAGE_INVERT_SHADER)
            .map_err(|err| {
                eprintln!("vulkan image shader module creation failed: {err}");
                ResourceError::ExportFailed
            })?;
    let entry_point = shader
        .entry_point("main")
        .ok_or(ResourceError::ExportFailed)?;
    let stage = PipelineShaderStageCreateInfo::new(entry_point);
    let layout = PipelineLayout::new(
        device.logical_device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages([&stage])
            .into_pipeline_layout_create_info(device.logical_device.clone())
            .map_err(|err| {
                eprintln!("vulkan image pipeline layout reflection failed: {err}");
                ResourceError::ExportFailed
            })?,
    )
    .map_err(|err| {
        eprintln!("vulkan image pipeline layout creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    let pipeline = ComputePipeline::new(
        device.logical_device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(stage, layout),
    )
    .map_err(|err| {
        eprintln!("vulkan image compute pipeline creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    let image_view = ImageView::new_default(imported_image.clone()).map_err(|err| {
        eprintln!("vulkan image view creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    let descriptor_set = DescriptorSet::new(
        device.descriptor_set_allocator.clone(),
        pipeline.layout().set_layouts()[0].clone(),
        [WriteDescriptorSet::image_view(0, image_view)],
        [],
    )
    .map_err(|err| {
        eprintln!("vulkan image descriptor set creation failed: {err}");
        ResourceError::ExportFailed
    })?;

    let mut builder = AutoCommandBufferBuilder::primary(
        device.command_allocator.clone(),
        device.queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .map_err(|err| {
        eprintln!("vulkan image command buffer creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    builder
        .bind_pipeline_compute(pipeline.clone())
        .map_err(|err| {
            eprintln!("vulkan image pipeline bind failed: {err}");
            ResourceError::ExportFailed
        })?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            pipeline.layout().clone(),
            0,
            descriptor_set,
        )
        .map_err(|err| {
            eprintln!("vulkan image descriptor bind failed: {err}");
            ResourceError::ExportFailed
        })?;
    let extent = imported_image.extent();
    let groups_x = extent[0].div_ceil(8);
    let groups_y = extent[1].div_ceil(8);
    external_compute::dispatch_compute(&mut builder, [groups_x, groups_y, 1]).map_err(|err| {
        eprintln!("vulkan image dispatch recording failed: {err}");
        ResourceError::ExportFailed
    })?;
    builder
        .copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(
            imported_image,
            readback.clone(),
        ))
        .map_err(|err| {
            eprintln!("vulkan image readback copy recording failed: {err}");
            ResourceError::ExportFailed
        })?;

    let command_buffer = builder.build().map_err(|err| {
        eprintln!("vulkan image command buffer build failed: {err}");
        ResourceError::ExportFailed
    })?;
    let fence = Arc::new(
        Fence::new(device.logical_device.clone(), FenceCreateInfo::default()).map_err(|err| {
            eprintln!("vulkan image fence creation failed: {err}");
            ResourceError::ExportFailed
        })?,
    );
    let submit = SubmitInfo {
        wait_semaphores: vec![SemaphoreSubmitInfo::new(wait_semaphore)],
        command_buffers: vec![CommandBufferSubmitInfo::new(command_buffer)],
        ..Default::default()
    };
    external_sync::submit_queue(&device.queue, &[submit], Some(&fence)).map_err(|err| {
        eprintln!("vulkan image queue submit failed: {err}");
        ResourceError::ExportFailed
    })?;
    fence
        .wait(Some(std::time::Duration::from_secs(5)))
        .map_err(|err| {
            eprintln!("vulkan image fence wait failed: {err}");
            ResourceError::ExportFailed
        })?;

    let read = readback.read().map_err(|err| {
        eprintln!("vulkan image readback map failed: {err}");
        ResourceError::ExportFailed
    })?;
    Ok(read.to_vec())
}

fn run_compute_increment_proof(
    device: &RegisteredDevice,
    imported_buffer: Subbuffer<[u8]>,
    wait_semaphore: Arc<Semaphore>,
    input_values: &[u32],
    input_bytes: u64,
) -> Result<Vec<u32>, ResourceError> {
    let storage_buffer = imported_buffer.slice(..input_bytes).reinterpret::<[u32]>();
    let upload = Buffer::from_iter(
        device.memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        input_values.iter().copied(),
    )
    .map_err(|err| {
        eprintln!("vulkan compute upload allocation failed: {err}");
        ResourceError::AllocationFailed
    })?;
    let readback = Buffer::new_slice::<u32>(
        device.memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        input_values.len() as u64,
    )
    .map_err(|err| {
        eprintln!("vulkan compute readback allocation failed: {err}");
        ResourceError::AllocationFailed
    })?;

    let shader = external_compute::create_shader_module(
        device.logical_device.clone(),
        &COMPUTE_INCREMENT_SHADER,
    )
    .map_err(|err| {
        eprintln!("vulkan compute shader module creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    let entry_point = shader
        .entry_point("main")
        .ok_or(ResourceError::ExportFailed)?;
    let stage = PipelineShaderStageCreateInfo::new(entry_point);
    let layout = PipelineLayout::new(
        device.logical_device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages([&stage])
            .into_pipeline_layout_create_info(device.logical_device.clone())
            .map_err(|err| {
                eprintln!("vulkan compute pipeline layout reflection failed: {err}");
                ResourceError::ExportFailed
            })?,
    )
    .map_err(|err| {
        eprintln!("vulkan compute pipeline layout creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    let pipeline = ComputePipeline::new(
        device.logical_device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(stage, layout),
    )
    .map_err(|err| {
        eprintln!("vulkan compute pipeline creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    let descriptor_set = DescriptorSet::new(
        device.descriptor_set_allocator.clone(),
        pipeline.layout().set_layouts()[0].clone(),
        [WriteDescriptorSet::buffer(0, storage_buffer.clone())],
        [],
    )
    .map_err(|err| {
        eprintln!("vulkan compute descriptor set creation failed: {err}");
        ResourceError::ExportFailed
    })?;

    let mut builder = AutoCommandBufferBuilder::primary(
        device.command_allocator.clone(),
        device.queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .map_err(|err| {
        eprintln!("vulkan compute command buffer creation failed: {err}");
        ResourceError::ExportFailed
    })?;
    builder
        .copy_buffer(CopyBufferInfo::buffers(upload, storage_buffer.clone()))
        .map_err(|err| {
            eprintln!("vulkan compute upload copy recording failed: {err}");
            ResourceError::ExportFailed
        })?
        .bind_pipeline_compute(pipeline.clone())
        .map_err(|err| {
            eprintln!("vulkan compute pipeline bind failed: {err}");
            ResourceError::ExportFailed
        })?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            pipeline.layout().clone(),
            0,
            descriptor_set,
        )
        .map_err(|err| {
            eprintln!("vulkan compute descriptor bind failed: {err}");
            ResourceError::ExportFailed
        })?;
    let groups_x = (input_values.len() as u32).div_ceil(COMPUTE_LOCAL_SIZE_X);
    external_compute::dispatch_compute(&mut builder, [groups_x, 1, 1]).map_err(|err| {
        eprintln!("vulkan compute dispatch recording failed: {err}");
        ResourceError::ExportFailed
    })?;
    builder
        .copy_buffer(CopyBufferInfo::buffers(storage_buffer, readback.clone()))
        .map_err(|err| {
            eprintln!("vulkan compute readback copy recording failed: {err}");
            ResourceError::ExportFailed
        })?;

    let command_buffer = builder.build().map_err(|err| {
        eprintln!("vulkan compute command buffer build failed: {err}");
        ResourceError::ExportFailed
    })?;
    let fence = Arc::new(
        Fence::new(device.logical_device.clone(), FenceCreateInfo::default()).map_err(|err| {
            eprintln!("vulkan compute fence creation failed: {err}");
            ResourceError::ExportFailed
        })?,
    );
    let submit = SubmitInfo {
        wait_semaphores: vec![SemaphoreSubmitInfo::new(wait_semaphore)],
        command_buffers: vec![CommandBufferSubmitInfo::new(command_buffer)],
        ..Default::default()
    };
    external_sync::submit_queue(&device.queue, &[submit], Some(&fence)).map_err(|err| {
        eprintln!("vulkan compute queue submit failed: {err}");
        ResourceError::ExportFailed
    })?;
    fence
        .wait(Some(std::time::Duration::from_secs(5)))
        .map_err(|err| {
            eprintln!("vulkan compute fence wait failed: {err}");
            ResourceError::ExportFailed
        })?;

    let read = readback.read().map_err(|err| {
        eprintln!("vulkan compute readback map failed: {err}");
        ResourceError::ExportFailed
    })?;
    Ok(read.to_vec())
}

fn export_memory_allocator(
    device: Arc<Device>,
    handle_types: ExternalMemoryHandleTypes,
) -> StandardMemoryAllocator {
    let memory_properties = device.physical_device().memory_properties();
    let mut block_sizes = Vec::with_capacity(memory_properties.memory_types.len());
    let mut export_handle_types = Vec::with_capacity(memory_properties.memory_types.len());
    let mut memory_type_bits = u32::MAX;

    for (index, memory_type) in memory_properties.memory_types.iter().enumerate() {
        let heap_size = memory_properties.memory_heaps[memory_type.heap_index as usize].size;
        const LARGE_HEAP_THRESHOLD: u64 = 1024 * 1024 * 1024;
        block_sizes.push(if heap_size >= LARGE_HEAP_THRESHOLD {
            256 * 1024 * 1024
        } else {
            64 * 1024 * 1024
        });
        export_handle_types.push(handle_types);

        if memory_type.property_flags.intersects(
            MemoryPropertyFlags::LAZILY_ALLOCATED
                | MemoryPropertyFlags::PROTECTED
                | MemoryPropertyFlags::DEVICE_COHERENT
                | MemoryPropertyFlags::RDMA_CAPABLE,
        ) {
            memory_type_bits &= !(1 << index);
        }
    }

    StandardMemoryAllocator::new(
        device,
        GenericMemoryAllocatorCreateInfo {
            block_sizes: &block_sizes,
            memory_type_bits,
            export_handle_types: &export_handle_types,
            ..Default::default()
        },
    )
}

fn buffer_memory_export_info(buffer: &Subbuffer<[u8]>) -> Result<(u32, u64, bool), ResourceError> {
    match buffer.buffer().memory() {
        BufferMemory::Normal(memory) => Ok((
            memory.device_memory().memory_type_index(),
            memory.device_memory().allocation_size(),
            memory.device_memory().is_dedicated(),
        )),
        BufferMemory::Sparse | BufferMemory::External => Err(ResourceError::ExportFailed),
        _ => Err(ResourceError::ExportFailed),
    }
}

fn image_memory_export_info(image: &Image) -> Result<(u32, u64, bool), ResourceError> {
    match image.memory() {
        ImageMemory::Normal(memory) => {
            let memory = memory.first().ok_or(ResourceError::ExportFailed)?;
            Ok((
                memory.device_memory().memory_type_index(),
                memory.device_memory().allocation_size(),
                memory.device_memory().is_dedicated(),
            ))
        }
        ImageMemory::Sparse | ImageMemory::Swapchain { .. } | ImageMemory::External => {
            Err(ResourceError::ExportFailed)
        }
        _ => Err(ResourceError::ExportFailed),
    }
}

fn write_validation_marker(buffer: &Subbuffer<[u8]>) -> Result<(), ResourceError> {
    if buffer.size() < SHARED_VALIDATION_MARKER.len() as u64 {
        return Err(ResourceError::InvalidBufferSize);
    }

    let mut write = buffer.write().map_err(|err| {
        eprintln!("vulkan marker write failed: {err}");
        ResourceError::UnsupportedMemoryRequirements
    })?;
    write[..SHARED_VALIDATION_MARKER.len()].copy_from_slice(SHARED_VALIDATION_MARKER);
    Ok(())
}

fn device_matches_export_source(candidate: &DeviceDesc, source: &DeviceDesc) -> bool {
    candidate.backend == source.backend
        && candidate.vendor_id == source.vendor_id
        && candidate.device_id == source.device_id
        && candidate.name == source.name
        && candidate.api_version == source.api_version
        && candidate.driver_version == source.driver_version
}

pub fn validation_marker_len() -> usize {
    SHARED_VALIDATION_MARKER.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_vulkan_device_types_to_qgs_classes() {
        assert_eq!(
            classify_device_type(PhysicalDeviceType::IntegratedGpu),
            DeviceClass::IntegratedGpu
        );
        assert_eq!(
            classify_device_type(PhysicalDeviceType::DiscreteGpu),
            DeviceClass::DiscreteGpu
        );
        assert_eq!(
            classify_device_type(PhysicalDeviceType::Cpu),
            DeviceClass::Software
        );
        assert_eq!(
            classify_device_type(PhysicalDeviceType::VirtualGpu),
            DeviceClass::Other
        );
        assert_eq!(
            classify_device_type(PhysicalDeviceType::Other),
            DeviceClass::Other
        );
    }

    #[test]
    fn bounds_device_names_without_breaking_utf8() {
        let long_name = format!("{}é", "x".repeat(MAX_DEVICE_NAME_LEN));
        let bounded = bounded_name(&long_name);

        assert!(bounded.len() <= MAX_DEVICE_NAME_LEN);
        assert!(bounded.is_char_boundary(bounded.len()));
    }
}

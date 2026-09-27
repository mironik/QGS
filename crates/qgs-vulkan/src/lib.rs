#![deny(unsafe_code)]

use std::fs::File;
use std::sync::Arc;

use qgs_core::{
    BackendBufferAllocation, BackendResource, BackendResourceExport, DeviceDiscovery,
    DeviceDiscoveryError, ResourceBackend, ResourceError,
};
use qgs_protocol::{
    ApiVersion, BackendApi, BufferDesc, BufferUsageFlags, ComputeCapabilities, DeviceCapabilities,
    DeviceClass, DeviceDesc, DeviceId, ExportResourceRequest, ExportedResourceMetadata,
    ExternalHandleType, ExternalSharing, InteropCapabilities, MemoryCapabilities, MemoryHeapDesc,
    SelectedMemoryProperties, MAX_DEVICE_COUNT, MAX_DEVICE_NAME_LEN, MAX_MEMORY_HEAP_COUNT,
    MAX_MEMORY_TYPE_COUNT,
};
use vulkano::buffer::{
    Buffer, BufferCreateInfo, BufferMemory, BufferUsage, ExternalBufferInfo, RawBuffer, Subbuffer,
};
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::device::{Device, DeviceCreateInfo, DeviceExtensions, QueueCreateInfo, QueueFlags};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::memory::allocator::{
    AllocationCreateInfo, GenericMemoryAllocatorCreateInfo, MemoryAllocatePreference,
    MemoryTypeFilter, StandardMemoryAllocator,
};
use vulkano::memory::{
    DedicatedAllocation, ExternalMemoryHandleType, ExternalMemoryHandleTypes, MemoryAllocateInfo,
    MemoryHeapFlags, MemoryImportInfo, MemoryMapInfo, MemoryPropertyFlags, ResourceMemory,
};
use vulkano::{Version, VulkanLibrary};

#[allow(unsafe_code)]
mod external_memory;

const SHARED_VALIDATION_MARKER: &[u8] = b"QGS-M1S6";

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
            let queue_family_index = physical_device
                .queue_family_properties()
                .iter()
                .position(|queue| queue.queue_count > 0)
                .ok_or(DeviceDiscoveryError::BackendFailed)?
                as u32;
            let supported_extensions = physical_device.supported_extensions();
            let enabled_extensions = DeviceExtensions {
                khr_external_memory: supported_extensions.khr_external_memory,
                khr_external_memory_fd: supported_extensions.khr_external_memory_fd,
                ext_external_memory_dma_buf: supported_extensions.ext_external_memory_dma_buf,
                khr_dedicated_allocation: supported_extensions.khr_dedicated_allocation,
                ..DeviceExtensions::empty()
            };
            let (logical_device, _) = Device::new(
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
            let memory_allocator =
                Arc::new(StandardMemoryAllocator::new_default(logical_device.clone()));

            devices.push(RegisteredDevice {
                desc,
                physical_device,
                logical_device,
                memory_allocator,
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

        let usage = map_buffer_usage(metadata.usage)?;
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
}

#[derive(Debug)]
struct RegisteredDevice {
    desc: DeviceDesc,
    physical_device: Arc<PhysicalDevice>,
    #[allow(dead_code)]
    logical_device: Arc<Device>,
    memory_allocator: Arc<StandardMemoryAllocator>,
}

#[derive(Debug)]
struct VulkanBufferResource {
    buffer: Subbuffer<[u8]>,
    desc: BufferDesc,
    selected_memory: SelectedMemoryProperties,
}

impl BackendResource for VulkanBufferResource {
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
                size_bytes: self.desc.size_bytes,
                allocation_size_bytes,
                usage: self.desc.usage,
                backend_memory_type_index: memory_type_index,
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

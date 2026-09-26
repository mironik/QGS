#![forbid(unsafe_code)]

use std::sync::Arc;

use qgs_core::{DeviceDiscovery, DeviceDiscoveryError};
use qgs_protocol::{
    ApiVersion, BackendApi, ComputeCapabilities, DeviceCapabilities, DeviceClass, DeviceDesc,
    DeviceId, InteropCapabilities, MemoryCapabilities, MemoryHeapDesc, MAX_DEVICE_COUNT,
    MAX_DEVICE_NAME_LEN, MAX_MEMORY_HEAP_COUNT, MAX_MEMORY_TYPE_COUNT,
};
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::device::QueueFlags;
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::memory::{MemoryHeapFlags, MemoryPropertyFlags};
use vulkano::{Version, VulkanLibrary};

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
            devices.push(RegisteredDevice {
                desc,
                physical_device,
            });
        }

        Ok(Self { devices })
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

#[derive(Debug)]
struct RegisteredDevice {
    desc: DeviceDesc,
    physical_device: Arc<PhysicalDevice>,
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

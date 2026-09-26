#![forbid(unsafe_code)]

use qgs_core::{DeviceDiscovery, DeviceDiscoveryError};
use qgs_protocol::{
    ApiVersion, BackendApi, DeviceClass, DeviceDesc, DeviceId, MAX_DEVICE_COUNT,
    MAX_DEVICE_NAME_LEN,
};
use vulkano::device::physical::PhysicalDeviceType;
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::{Version, VulkanLibrary};

#[derive(Debug, Default)]
pub struct VulkanDeviceDiscovery;

impl VulkanDeviceDiscovery {
    pub const fn new() -> Self {
        Self
    }
}

impl DeviceDiscovery for VulkanDeviceDiscovery {
    fn enumerate_devices(&self) -> Result<Vec<DeviceDesc>, DeviceDiscoveryError> {
        enumerate_vulkan_devices()
    }
}

pub fn enumerate_vulkan_devices() -> Result<Vec<DeviceDesc>, DeviceDiscoveryError> {
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

        let properties = physical_device.properties();
        let name = bounded_name(&properties.device_name);
        let api_version = to_qgs_api_version(physical_device.api_version());
        let id = DeviceId::new((index as u64) + 1)?;

        devices.push(DeviceDesc {
            id,
            class: classify_device_type(properties.device_type),
            vendor_id: properties.vendor_id,
            device_id: properties.device_id,
            name,
            backend: BackendApi::Vulkan,
            api_version,
            driver_version: properties.driver_version,
        });
    }

    Ok(devices)
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

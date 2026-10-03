//! Device/backend selection helpers for live picture bricks.
//! VAAPI for 8-bit 4:2:0 NV12 proxy; software H.264 when VA cannot, then Vulkan upload.

use qgs_core::DeviceDiscovery;
use qgs_protocol::{DeviceClass, DeviceDesc};
use qgs_vulkan::VulkanDeviceDiscovery;

pub fn select_integrated_vulkan_gpu(
    discovery: &VulkanDeviceDiscovery,
) -> Result<DeviceDesc, Box<dyn std::error::Error>> {
    let devices = discovery.enumerate_devices()?;
    devices
        .iter()
        .find(|device| {
            device.vendor_id == 0x8086 && matches!(device.class, DeviceClass::IntegratedGpu)
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::IntegratedGpu))
        })
        .cloned()
        .ok_or_else(|| "no integrated GPU advertised".into())
}

pub fn select_integrated_or_discrete_gpu(
    discovery: &VulkanDeviceDiscovery,
) -> Result<DeviceDesc, Box<dyn std::error::Error>> {
    let devices = discovery.enumerate_devices()?;
    devices
        .iter()
        .find(|device| {
            device.vendor_id == 0x8086 && matches!(device.class, DeviceClass::IntegratedGpu)
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::IntegratedGpu))
        })
        .or_else(|| {
            devices
                .iter()
                .find(|device| matches!(device.class, DeviceClass::DiscreteGpu))
        })
        .cloned()
        .ok_or_else(|| "no Vulkan GPU advertised".into())
}

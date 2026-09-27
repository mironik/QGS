use std::sync::Arc;

use vulkano::device::Device;
use vulkano::memory::{DeviceMemory, MemoryAllocateInfo, MemoryImportInfo};
use vulkano::{Validated, VulkanError};

pub(super) fn import_device_memory(
    device: Arc<Device>,
    allocate_info: MemoryAllocateInfo<'_>,
    import_info: MemoryImportInfo,
) -> Result<DeviceMemory, Validated<VulkanError>> {
    // SAFETY: The safe caller constructs `import_info` from an owned `File`
    // received through SCM_RIGHTS, validates the handle type against the
    // resource metadata and enabled device extensions, uses the exporter
    // allocation size and memory type index, and binds the imported memory
    // only to a buffer created with matching size/usage/external handle type.
    // The `File` is moved into Vulkano and is not used again by QGS code.
    unsafe { DeviceMemory::import(device, allocate_info, import_info) }
}

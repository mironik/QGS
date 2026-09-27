use std::sync::Arc;

use vulkano::command_buffer::{AutoCommandBufferBuilder, PrimaryAutoCommandBuffer};
use vulkano::device::Device;
use vulkano::shader::{ShaderModule, ShaderModuleCreateInfo};
use vulkano::{Validated, ValidationError, VulkanError};

pub fn create_shader_module(
    device: Arc<Device>,
    words: &[u32],
) -> Result<Arc<ShaderModule>, Validated<VulkanError>> {
    // SAFETY: The SPIR-V module is a fixed QGS-owned shader embedded in
    // qgs-vulkan, not client-provided input. Its descriptor set, binding,
    // storage-buffer access, entry point, and local size are validated during
    // pipeline creation before dispatch is recorded.
    unsafe { ShaderModule::new(device, ShaderModuleCreateInfo::new(words)) }
}

pub fn dispatch_compute(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    group_counts: [u32; 3],
) -> Result<(), Box<ValidationError>> {
    // SAFETY: The safe qgs-vulkan compute proof binds the fixed QGS-owned
    // pipeline and descriptor set before dispatch, bounds group counts from the
    // input element count, and uses a shader that accesses only the imported
    // storage buffer through gl_GlobalInvocationID.x.
    unsafe { builder.dispatch(group_counts) }.map(|_| ())
}

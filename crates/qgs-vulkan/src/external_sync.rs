use std::fs::File;
use std::sync::Arc;

use vulkano::command_buffer::SubmitInfo;
use vulkano::device::Queue;
use vulkano::sync::fence::Fence;
use vulkano::sync::semaphore::{
    ExternalSemaphoreHandleType, ImportSemaphoreFdInfo, Semaphore, SemaphoreImportFlags,
};
use vulkano::{Validated, VulkanError};

pub fn export_sync_fd(semaphore: &Semaphore) -> Result<File, Validated<VulkanError>> {
    // SAFETY: QGS creates this as a binary semaphore with SYNC_FD export support,
    // submits exactly one GPU signal operation before export, and never submits a
    // wait operation on the producer-side semaphore. The semaphore has no imported
    // swapchain payload and is retained by the session-owned sync object while the
    // signal operation may still be pending.
    unsafe { semaphore.export_fd(ExternalSemaphoreHandleType::SyncFd) }
}

pub fn import_sync_fd(semaphore: &Semaphore, file: File) -> Result<(), Validated<VulkanError>> {
    // SAFETY: The FD is received as a QGS transport attachment for a SYNC_FD
    // export produced by qgs-vulkan on the matching physical device. SYNC_FD
    // imports use temporary payload semantics, and this fresh semaphore is not in
    // use by the device before import. Vulkano consumes the File on successful
    // import; on failure the File is dropped by normal Rust ownership.
    unsafe {
        semaphore.import_fd(ImportSemaphoreFdInfo {
            flags: SemaphoreImportFlags::TEMPORARY,
            handle_type: ExternalSemaphoreHandleType::SyncFd,
            file: Some(file),
            ..ImportSemaphoreFdInfo::handle_type(ExternalSemaphoreHandleType::SyncFd)
        })
    }
}

pub fn submit_queue(
    queue: &Arc<Queue>,
    submit_infos: &[SubmitInfo],
    fence: Option<&Arc<Fence>>,
) -> Result<(), Validated<VulkanError>> {
    queue.with(|mut guard| {
        // SAFETY: Callers build one-time command buffers for this queue family,
        // keep command buffers, semaphores, and optional fences alive through the
        // operation lifetime, and use non-overlapping QGS demo resources for the
        // submitted work. Vulkano still validates device/queue compatibility,
        // command-buffer usage, semaphore stage masks, and fence state.
        unsafe { guard.submit(submit_infos, fence) }
    })
}

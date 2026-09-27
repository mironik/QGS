use std::ffi::{c_void, CStr, CString};
use std::fs::File;
use std::os::fd::{AsRawFd, IntoRawFd, OwnedFd};
use std::sync::{Arc, Mutex};

use ash::vk;

const VK_NV12_FORMAT: vk::Format = vk::Format::G8_B8R8_2PLANE_420_UNORM;
const DMA_BUF_HANDLE_TYPE: vk::ExternalMemoryHandleTypeFlags =
    vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT;
const FOREIGN_QUEUE_FAMILY: u32 = vk::QUEUE_FAMILY_FOREIGN_EXT;
const ACQUIRE_OLD_LAYOUT: vk::ImageLayout = vk::ImageLayout::GENERAL;
const ACQUIRE_NEW_LAYOUT: vk::ImageLayout = vk::ImageLayout::GENERAL;

#[derive(Debug)]
pub struct HaswellVideoDiagnosticInput {
    pub vendor_id: u32,
    pub device_id: u32,
    pub width: u32,
    pub height: u32,
    pub drm_fourcc: u32,
    pub objects: Vec<DiagnosticDrmObject>,
    pub layers: Vec<DiagnosticDrmLayer>,
}

#[derive(Debug)]
pub struct DiagnosticDrmObject {
    pub fd: OwnedFd,
    pub size: u32,
    pub modifier: u64,
}

#[derive(Clone, Debug)]
pub struct DiagnosticDrmLayer {
    pub drm_format: u32,
    pub planes: Vec<DiagnosticDrmPlane>,
}

#[derive(Clone, Debug)]
pub struct DiagnosticDrmPlane {
    pub object_index: u32,
    pub offset: u32,
    pub pitch: u32,
}

#[derive(Clone, Debug)]
pub struct HaswellVideoDiagnosticReport {
    pub validation_messages: Vec<String>,
    pub physical_device_name: String,
    pub queue_family_index: u32,
    pub modifier: u64,
    pub modifier_exposed: bool,
    pub modifier_plane_count: Option<u32>,
    pub modifier_tiling_features_raw: Option<u64>,
    pub supports_sampled_image: bool,
    pub supports_transfer_src: bool,
    pub supports_transfer_dst: bool,
    pub supports_ycbcr_linear_filter: bool,
    pub supports_ycbcr_separate_reconstruction_filter: bool,
    pub memory_fd_type_bits: Option<u32>,
    pub image_memory_size: Option<u64>,
    pub image_memory_alignment: Option<u64>,
    pub image_memory_type_bits: Option<u32>,
    pub selected_memory_type_index: Option<u32>,
    pub va_object_count: usize,
    pub nv12_format_plane_count: u32,
    pub vulkan_modifier_memory_plane_count: Option<u32>,
    pub image_create_disjoint: bool,
    pub imported_memory_objects: usize,
    pub vulkan_memory_bindings: usize,
    pub binding_offsets: Vec<u64>,
    pub barrier: Option<AcquireBarrierLog>,
    pub queue_submit_result: Option<String>,
    pub fence_wait_result: Option<String>,
    pub gpu_read_attempted: bool,
    pub gpu_read_result: Option<String>,
    pub classification: String,
    pub recommendation: String,
}

#[derive(Clone, Debug)]
pub struct AcquireBarrierLog {
    pub src_stage_mask: String,
    pub src_access_mask: String,
    pub dst_stage_mask: String,
    pub dst_access_mask: String,
    pub old_layout: String,
    pub new_layout: String,
    pub src_queue_family_index: u32,
    pub dst_queue_family_index: u32,
    pub aspect_mask: String,
    pub base_mip_level: u32,
    pub level_count: u32,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

pub fn diagnose_haswell_video_import(
    input: HaswellVideoDiagnosticInput,
) -> Result<HaswellVideoDiagnosticReport, String> {
    // SAFETY: The raw diagnostic validates its QGS-owned input before each
    // Vulkan call and does not expose raw handles outside qgs-vulkan.
    unsafe { diagnose_haswell_video_import_raw(input) }
}

unsafe fn diagnose_haswell_video_import_raw(
    input: HaswellVideoDiagnosticInput,
) -> Result<HaswellVideoDiagnosticReport, String> {
    let validation_messages = Arc::new(Mutex::new(Vec::new()));
    let entry = ash::Entry::linked();
    let app_name = CString::new("qgs-step4b-diagnostic").map_err(|err| err.to_string())?;
    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(1)
        .engine_name(&app_name)
        .engine_version(1)
        .api_version(vk::make_api_version(0, 1, 2, 0));
    let layer_name = CString::new("VK_LAYER_KHRONOS_validation").map_err(|err| err.to_string())?;
    let layer_names = [layer_name.as_ptr()];
    let instance_extension_names = [ash::ext::debug_utils::NAME.as_ptr()];
    let validation_enables = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
    let mut validation_features =
        vk::ValidationFeaturesEXT::default().enabled_validation_features(&validation_enables);
    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_layer_names(&layer_names)
        .enabled_extension_names(&instance_extension_names)
        .push_next(&mut validation_features);
    // SAFETY: The create info points to stable local C strings and validation
    // feature storage for the duration of the call.
    let instance = unsafe { entry.create_instance(&create_info, None) }
        .map_err(|err| format!("vkCreateInstance failed: {err:?}"))?;
    let debug_utils = ash::ext::debug_utils::Instance::new(&entry, &instance);
    let mut callback_messages = validation_messages.clone();
    let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(validation_callback))
        .user_data((&mut callback_messages as *mut Arc<Mutex<Vec<String>>>).cast::<c_void>());
    // SAFETY: The callback user data points to a live Arc for the messenger lifetime below.
    let messenger = unsafe { debug_utils.create_debug_utils_messenger(&debug_info, None) }
        .map_err(|err| format!("vkCreateDebugUtilsMessengerEXT failed: {err:?}"))?;

    let result = run_raw_diagnostic(&entry, &instance, &input, validation_messages.clone());

    // SAFETY: The messenger was created from this instance and has not been destroyed.
    unsafe {
        debug_utils.destroy_debug_utils_messenger(messenger, None);
    }
    // SAFETY: No child objects from this instance remain alive past this point.
    unsafe {
        instance.destroy_instance(None);
    }

    result
}

unsafe extern "system" fn validation_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    ty: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user_data: *mut c_void,
) -> vk::Bool32 {
    if !data.is_null() && !user_data.is_null() {
        // SAFETY: Vulkan validation callbacks provide a null-terminated
        // message pointer for the duration of this callback invocation.
        let message = unsafe { CStr::from_ptr((*data).p_message) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: user_data is the Arc pointer supplied when the debug
        // messenger was created, and the messenger is destroyed before the Arc
        // goes out of scope.
        let messages = unsafe { &*(user_data.cast::<Arc<Mutex<Vec<String>>>>()) };
        if let Ok(mut messages) = messages.lock() {
            messages.push(format!("{severity:?} {ty:?}: {message}"));
        }
    }
    vk::FALSE
}

unsafe fn run_raw_diagnostic(
    _entry: &ash::Entry,
    instance: &ash::Instance,
    input: &HaswellVideoDiagnosticInput,
    validation_messages: Arc<Mutex<Vec<String>>>,
) -> Result<HaswellVideoDiagnosticReport, String> {
    let mut report = HaswellVideoDiagnosticReport {
        validation_messages: Vec::new(),
        physical_device_name: String::new(),
        queue_family_index: 0,
        modifier: input
            .objects
            .first()
            .ok_or("VA descriptor had no DRM objects")?
            .modifier,
        modifier_exposed: false,
        modifier_plane_count: None,
        modifier_tiling_features_raw: None,
        supports_sampled_image: false,
        supports_transfer_src: false,
        supports_transfer_dst: false,
        supports_ycbcr_linear_filter: false,
        supports_ycbcr_separate_reconstruction_filter: false,
        memory_fd_type_bits: None,
        image_memory_size: None,
        image_memory_alignment: None,
        image_memory_type_bits: None,
        selected_memory_type_index: None,
        va_object_count: input.objects.len(),
        nv12_format_plane_count: 2,
        vulkan_modifier_memory_plane_count: None,
        image_create_disjoint: false,
        imported_memory_objects: 0,
        vulkan_memory_bindings: 0,
        binding_offsets: Vec::new(),
        barrier: None,
        queue_submit_result: None,
        fence_wait_result: None,
        gpu_read_attempted: false,
        gpu_read_result: None,
        classification: String::new(),
        recommendation: "FREEZE HASWELL ZERO-COPY PATH AND VALIDATE M2 STEP 4 ON MODERN INTEL/AMD"
            .to_string(),
    };

    // SAFETY: The Vulkan instance is live and owned by the diagnostic.
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices failed: {err:?}"))?;
    let physical_device = physical_devices
        .into_iter()
        .find(|device| {
            // SAFETY: device was returned by this live Vulkan instance.
            let props = unsafe { instance.get_physical_device_properties(*device) };
            props.vendor_id == input.vendor_id && props.device_id == input.device_id
        })
        .ok_or("matching Intel Vulkan physical device not found")?;
    // SAFETY: physical_device was returned by this live Vulkan instance.
    let props = unsafe { instance.get_physical_device_properties(physical_device) };
    // SAFETY: Vulkan guarantees device_name is a null-terminated fixed-size C string.
    report.physical_device_name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: physical_device was returned by this live Vulkan instance.
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    report.queue_family_index = queue_families
        .iter()
        .position(|family| {
            family.queue_count > 0
                && family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && family.queue_flags.contains(vk::QueueFlags::TRANSFER)
        })
        .ok_or("no graphics+transfer queue family found")? as u32;

    let mut modifier_list = vk::DrmFormatModifierPropertiesList2EXT::default();
    let mut format_props = vk::FormatProperties2::default().push_next(&mut modifier_list);
    // SAFETY: The output structures are valid for the duration of the call.
    unsafe {
        instance.get_physical_device_format_properties2(
            physical_device,
            VK_NV12_FORMAT,
            &mut format_props,
        );
    }
    let count = modifier_list.drm_format_modifier_count;
    let mut modifier_props = vec![vk::DrmFormatModifierProperties2EXT::default(); count as usize];
    let mut modifier_list = vk::DrmFormatModifierPropertiesList2EXT::default()
        .drm_format_modifier_properties(&mut modifier_props);
    let mut format_props = vk::FormatProperties2::default().push_next(&mut modifier_list);
    // SAFETY: modifier_props is sized from the previous query and remains live
    // while Vulkan writes the property list.
    unsafe {
        instance.get_physical_device_format_properties2(
            physical_device,
            VK_NV12_FORMAT,
            &mut format_props,
        );
    }
    let Some(matching_modifier) = modifier_props
        .iter()
        .find(|props| props.drm_format_modifier == report.modifier)
        .copied()
    else {
        report.classification =
            "A. VA modifier not exposed/usable by Vulkan: exact DRM modifier was not listed for VK_FORMAT_G8_B8R8_2PLANE_420_UNORM".to_string();
        report.validation_messages = validation_messages_snapshot(&validation_messages);
        return Ok(report);
    };
    report.modifier_exposed = true;
    report.modifier_plane_count = Some(matching_modifier.drm_format_modifier_plane_count);
    report.vulkan_modifier_memory_plane_count =
        Some(matching_modifier.drm_format_modifier_plane_count);
    let features = matching_modifier.drm_format_modifier_tiling_features;
    report.modifier_tiling_features_raw = Some(features.as_raw());
    report.supports_sampled_image = features.contains(vk::FormatFeatureFlags2::SAMPLED_IMAGE);
    report.supports_transfer_src = features.contains(vk::FormatFeatureFlags2::TRANSFER_SRC);
    report.supports_transfer_dst = features.contains(vk::FormatFeatureFlags2::TRANSFER_DST);
    report.supports_ycbcr_linear_filter =
        features.contains(vk::FormatFeatureFlags2::SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER);
    report.supports_ycbcr_separate_reconstruction_filter = features.contains(
        vk::FormatFeatureFlags2::SAMPLED_IMAGE_YCBCR_CONVERSION_SEPARATE_RECONSTRUCTION_FILTER,
    );

    if matching_modifier.drm_format_modifier_plane_count != 1 || input.objects.len() != 1 {
        report.classification =
            "B. incorrect or unsupported QGS memory-plane/binding implementation: exact modifier reports two Vulkan memory planes, so the previous one-binding path is invalid".to_string();
        report.validation_messages = validation_messages_snapshot(&validation_messages);
        return Ok(report);
    }

    let priorities = [1.0_f32];
    let queue_info = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(report.queue_family_index)
        .queue_priorities(&priorities)];
    let device_extension_names = [
        ash::khr::external_memory_fd::NAME.as_ptr(),
        c"VK_KHR_external_memory".as_ptr(),
        c"VK_EXT_external_memory_dma_buf".as_ptr(),
        c"VK_EXT_image_drm_format_modifier".as_ptr(),
        c"VK_EXT_queue_family_foreign".as_ptr(),
    ];
    let device_create = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_info)
        .enabled_extension_names(&device_extension_names);
    // SAFETY: The selected physical device and queue family were queried from this instance.
    let device = unsafe { instance.create_device(physical_device, &device_create, None) }
        .map_err(|err| format!("vkCreateDevice failed: {err:?}"))?;
    let external_memory_fd = ash::khr::external_memory_fd::Device::new(instance, &device);
    // SAFETY: Queue index 0 exists because the selected queue family reported
    // at least one queue and was used during logical-device creation.
    let queue = unsafe { device.get_device_queue(report.queue_family_index, 0) };

    let object = &input.objects[0];
    let mut fd_props = vk::MemoryFdPropertiesKHR::default();
    // SAFETY: vkGetMemoryFdPropertiesKHR borrows the DMA-BUF fd; ownership
    // remains with object.fd. The handle type matches the exported VA object.
    unsafe {
        external_memory_fd
            .get_memory_fd_properties(DMA_BUF_HANDLE_TYPE, object.fd.as_raw_fd(), &mut fd_props)
            .map_err(|err| format!("vkGetMemoryFdPropertiesKHR failed: {err:?}"))?;
    }
    report.memory_fd_type_bits = Some(fd_props.memory_type_bits);

    let plane_layout = first_vulkan_plane_layout(input)?;
    let plane_layouts = [plane_layout];
    let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
        .drm_format_modifier(report.modifier)
        .plane_layouts(&plane_layouts);
    let mut external_image_info =
        vk::ExternalMemoryImageCreateInfo::default().handle_types(DMA_BUF_HANDLE_TYPE);
    let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC;
    let image_create = vk::ImageCreateInfo::default()
        .push_next(&mut external_image_info)
        .push_next(&mut modifier_info)
        .image_type(vk::ImageType::TYPE_2D)
        .format(VK_NV12_FORMAT)
        .extent(vk::Extent3D {
            width: input.width,
            height: input.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    // SAFETY: The image create info is populated from bounded VA descriptor metadata.
    let image = unsafe { device.create_image(&image_create, None) }
        .map_err(|err| format!("vkCreateImage failed: {err:?}"))?;
    // SAFETY: image was created from this logical device and is not destroyed yet.
    let requirements = unsafe { device.get_image_memory_requirements(image) };
    report.image_memory_size = Some(requirements.size);
    report.image_memory_alignment = Some(requirements.alignment);
    report.image_memory_type_bits = Some(requirements.memory_type_bits);
    let selected_memory_type = select_memory_type(
        fd_props.memory_type_bits & requirements.memory_type_bits,
        // SAFETY: physical_device was returned by this live Vulkan instance.
        unsafe { instance.get_physical_device_memory_properties(physical_device) },
    )
    .ok_or("no compatible Vulkan memory type for imported DMA-BUF")?;
    report.selected_memory_type_index = Some(selected_memory_type);

    let import_file = File::open(format!("/proc/self/fd/{}", object.fd.as_raw_fd()))
        .map_err(|err| format!("failed to duplicate DMA-BUF FD for import: {err}"))?;
    let import_raw_fd = import_file.into_raw_fd();
    let mut import_info = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(DMA_BUF_HANDLE_TYPE)
        .fd(import_raw_fd);
    let allocate_info = vk::MemoryAllocateInfo::default()
        .push_next(&mut import_info)
        .allocation_size(requirements.size)
        .memory_type_index(selected_memory_type);
    // SAFETY: import_raw_fd is a duplicated DMA-BUF fd intentionally consumed
    // by Vulkan on successful import; allocation size and memory type were
    // selected from the image requirements and fd properties.
    let memory = match unsafe { device.allocate_memory(&allocate_info, None) } {
        Ok(memory) => memory,
        Err(err) => {
            report.classification = format!("G. memory import failed: {err:?}");
            report.validation_messages = validation_messages_snapshot(&validation_messages);
            // SAFETY: image was created from this device and no imported
            // memory was created on this failure path.
            unsafe {
                device.destroy_image(image, None);
                device.destroy_device(None);
            }
            return Ok(report);
        }
    };
    report.imported_memory_objects = 1;
    let bind = vk::BindImageMemoryInfo::default()
        .image(image)
        .memory(memory)
        .memory_offset(0);
    // SAFETY: image and memory were created from the same logical device and
    // the binding offset is aligned to zero.
    match unsafe { device.bind_image_memory2(&[bind]) } {
        Ok(()) => {
            report.vulkan_memory_bindings = 1;
            report.binding_offsets.push(0);
        }
        Err(err) => {
            report.classification = format!("B. image memory bind failed: {err:?}");
            report.validation_messages = validation_messages_snapshot(&validation_messages);
            // SAFETY: memory and image are live objects from this device and
            // are destroyed once on this failure path.
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
                device.destroy_device(None);
            }
            return Ok(report);
        }
    }

    let command_pool_info =
        vk::CommandPoolCreateInfo::default().queue_family_index(report.queue_family_index);
    // SAFETY: The command pool queue family belongs to this logical device.
    let command_pool = unsafe { device.create_command_pool(&command_pool_info, None) }
        .map_err(|err| format!("vkCreateCommandPool failed: {err:?}"))?;
    let command_buffer_allocate = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    // SAFETY: command_pool is live and was created from this device.
    let command_buffer = unsafe { device.allocate_command_buffers(&command_buffer_allocate) }
        .map_err(|err| format!("vkAllocateCommandBuffers failed: {err:?}"))?[0];
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: command_buffer is newly allocated and not currently recording.
    unsafe {
        device
            .begin_command_buffer(command_buffer, &begin)
            .map_err(|err| format!("vkBeginCommandBuffer failed: {err:?}"))?;
    }
    let barrier = vk::ImageMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
        .dst_access_mask(vk::AccessFlags::MEMORY_READ)
        .old_layout(ACQUIRE_OLD_LAYOUT)
        .new_layout(ACQUIRE_NEW_LAYOUT)
        .src_queue_family_index(FOREIGN_QUEUE_FAMILY)
        .dst_queue_family_index(report.queue_family_index)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    report.barrier = Some(AcquireBarrierLog {
        src_stage_mask: format!("{:?}", vk::PipelineStageFlags::ALL_COMMANDS),
        src_access_mask: format!("{:?}", vk::AccessFlags::MEMORY_WRITE),
        dst_stage_mask: format!("{:?}", vk::PipelineStageFlags::ALL_COMMANDS),
        dst_access_mask: format!("{:?}", vk::AccessFlags::MEMORY_READ),
        old_layout: format!("{ACQUIRE_OLD_LAYOUT:?}"),
        new_layout: format!("{ACQUIRE_NEW_LAYOUT:?}"),
        src_queue_family_index: FOREIGN_QUEUE_FAMILY,
        dst_queue_family_index: report.queue_family_index,
        aspect_mask: format!("{:?}", vk::ImageAspectFlags::COLOR),
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    });
    // SAFETY: The command buffer is recording, the image is live and bound,
    // and the barrier is diagnostic-only metadata for the spec-justified
    // foreign ownership acquire path.
    unsafe {
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer failed: {err:?}"))?;
    }
    // SAFETY: The fence is created from the live diagnostic device.
    let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }
        .map_err(|err| format!("vkCreateFence failed: {err:?}"))?;
    let command_buffers = [command_buffer];
    let submit = [vk::SubmitInfo::default().command_buffers(&command_buffers)];
    // SAFETY: queue, command buffer, and fence all belong to this device; this
    // submission intentionally contains only the acquire barrier.
    let submit_result = unsafe { device.queue_submit(queue, &submit, fence) };
    report.queue_submit_result = Some(format!("{submit_result:?}"));
    if let Err(err) = submit_result {
        report.classification = format!(
            "E. acquire-only vkQueueSubmit failed on spec-justified FOREIGN_EXT path: {err:?}"
        );
        report.validation_messages = validation_messages_snapshot(&validation_messages);
        cleanup_raw(&device, command_pool, fence, memory, image);
        return Ok(report);
    }
    // SAFETY: fence belongs to this device and was used for the acquire-only
    // submission above.
    let wait_result = unsafe { device.wait_for_fences(&[fence], true, 5_000_000_000) };
    report.fence_wait_result = Some(format!("{wait_result:?}"));
    if let Err(err) = wait_result {
        report.classification =
            format!("E. acquire-only fence wait failed after successful submit: {err:?}");
        report.validation_messages = validation_messages_snapshot(&validation_messages);
        cleanup_raw(&device, command_pool, fence, memory, image);
        return Ok(report);
    }

    report.classification =
        "G. acquire-only succeeded; GPU read path not executed by this conservative diagnostic"
            .to_string();
    report.validation_messages = validation_messages_snapshot(&validation_messages);
    cleanup_raw(&device, command_pool, fence, memory, image);
    Ok(report)
}

fn first_vulkan_plane_layout(
    input: &HaswellVideoDiagnosticInput,
) -> Result<vk::SubresourceLayout, String> {
    let layer = input.layers.first().ok_or("VA descriptor had no layers")?;
    let plane = layer.planes.first().ok_or("VA descriptor had no planes")?;
    Ok(vk::SubresourceLayout {
        offset: u64::from(plane.offset),
        size: 0,
        row_pitch: u64::from(plane.pitch),
        array_pitch: 0,
        depth_pitch: 0,
    })
}

unsafe fn cleanup_raw(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    fence: vk::Fence,
    memory: vk::DeviceMemory,
    image: vk::Image,
) {
    // SAFETY: All objects were created from this logical device and are destroyed once.
    unsafe {
        let _ = device.device_wait_idle();
        device.destroy_fence(fence, None);
        device.destroy_command_pool(command_pool, None);
        device.free_memory(memory, None);
        device.destroy_image(image, None);
        device.destroy_device(None);
    }
}

fn select_memory_type(
    memory_type_bits: u32,
    properties: vk::PhysicalDeviceMemoryProperties,
) -> Option<u32> {
    (0..properties.memory_type_count).find(|index| {
        let supported = (memory_type_bits & (1_u32 << index)) != 0;
        let flags = properties.memory_types[*index as usize].property_flags;
        supported && flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
    })
}

fn validation_messages_snapshot(messages: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    messages
        .lock()
        .map(|messages| messages.clone())
        .unwrap_or_default()
}

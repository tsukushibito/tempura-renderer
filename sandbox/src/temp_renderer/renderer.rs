use ash::extensions::{
    ext::DebugUtils,
    khr::{Surface, Swapchain},
};
use ash::vk::PhysicalDevice;
use ash::{vk, Device, Entry, Instance};
use raw_window_handle::HasRawWindowHandle;
use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::c_char;

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number: i32 = callback_data.message_id_number as i32;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!(
        "{:?}:\n{:?} [{} ({})] : {}\n",
        message_severity,
        message_type,
        message_id_name,
        &message_id_number.to_string(),
        message,
    );

    vk::FALSE
}

pub struct Renderer {
    pub entry: Entry,
    pub instance: Instance,
    pub debug_utils_loader: DebugUtils,
    pub surface_loader: Surface,
    pub swapchain_loader: Swapchain,
    pub pdevice: PhysicalDevice,
    pub device: Device,
    pub debug_callback: vk::DebugUtilsMessengerEXT,
    pub surface: vk::SurfaceKHR,
    pub command_pool: vk::CommandPool,
    pub present_image_views: Vec<vk::ImageView>,
    pub depth_image: vk::Image,
    pub depth_image_view: vk::ImageView,
    pub depth_image_memory: vk::DeviceMemory,
    pub setup_commands_reuse_fence: vk::Fence,
    pub draw_commands_reuse_fence: vk::Fence,
    pub rendering_complete_semaphore: vk::Semaphore,
    pub present_complete_semaphore: vk::Semaphore,
}

impl Renderer {
    pub fn new(window_handle: &dyn HasRawWindowHandle) -> Self {
        unsafe {
            let entry = Entry::linked();
            let instance = create_instance(&entry, window_handle);
            let debug_utils_loader = DebugUtils::new(&entry, &instance);
            let debug_callback = create_debug_call_back(&debug_utils_loader);
            let surface = create_surface(&entry, &instance, window_handle);
            let surface_loader = Surface::new(&entry, &instance);
            let (pdevice, queue_family_index) =
                get_physical_device(&entry, &instance, &surface, &surface_loader);
            let device = create_device(&instance, &pdevice, queue_family_index);
            let present_queue = device.get_device_queue(queue_family_index, 0);

            let surface_format = surface_loader
                .get_physical_device_surface_formats(pdevice, surface)
                .unwrap()[0];
            let swapchain_loader = Swapchain::new(&instance, &device);
            let (swapchain, surface_resolution) = create_swapchain(
                &pdevice,
                &surface_loader,
                &surface,
                &surface_format,
                &swapchain_loader,
            );

            let command_pool = create_command_pool(&device, queue_family_index);
            let command_buffers = create_command_buffers(&device, &command_pool);
            let setup_command_buffer = command_buffers[0];
            let draw_command_buffer = command_buffers[1];

            let present_image_views =
                create_present_image_views(&device, &swapchain_loader, &swapchain, &surface_format);
            let device_memory_properties = instance.get_physical_device_memory_properties(pdevice);
            let (depth_image, depth_image_memory) =
                create_depth_image(&instance, &pdevice, &device, &surface_resolution);

            let fence_create_info =
                *vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);

            let draw_commands_reuse_fence = device
                .create_fence(&fence_create_info, None)
                .expect("Create fence failed.");
            let setup_commands_reuse_fence = device
                .create_fence(&fence_create_info, None)
                .expect("Create fence failed.");

            optimize_depth_image_layout(
                &device,
                &setup_command_buffer,
                &setup_commands_reuse_fence,
                &present_queue,
                &depth_image,
            );

            let depth_image_view_info = *vk::ImageViewCreateInfo::builder()
                .subresource_range(
                    *vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .level_count(1)
                        .layer_count(1),
                )
                .image(depth_image)
                .format(vk::Format::D16_UNORM)
                .view_type(vk::ImageViewType::TYPE_2D);

            let depth_image_view = device
                .create_image_view(&depth_image_view_info, None)
                .unwrap();

            let semaphore_create_info = vk::SemaphoreCreateInfo::default();

            let present_complete_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .unwrap();
            let rendering_complete_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .unwrap();

            Self {
                entry,
                instance,
                debug_utils_loader,
                surface_loader,
                swapchain_loader,
                pdevice,
                device,
                debug_callback,
                surface,
                command_pool,
                present_image_views,
                depth_image,
                depth_image_view,
                depth_image_memory,
                setup_commands_reuse_fence,
                draw_commands_reuse_fence,
                rendering_complete_semaphore,
                present_complete_semaphore,
            }
        }
    }
}

// 以下、Vulkanオブジェクト作成用関数

unsafe fn create_instance(entry: &Entry, window_handle: &dyn HasRawWindowHandle) -> Instance {
    let app_info = vk::ApplicationInfo {
        api_version: vk::make_api_version(0, 1, 0, 0),
        ..Default::default()
    };

    let layer_names = [CStr::from_bytes_with_nul_unchecked(
        b"VK_LAYER_KHRONOS_validation\0",
    )];
    let layer_names_raw: Vec<*const c_char> = layer_names
        .iter()
        .map(|raw_name| raw_name.as_ptr())
        .collect();

    let mut extension_names = ash_window::enumerate_required_extensions(&window_handle)
        .unwrap()
        .to_vec();
    extension_names.push(DebugUtils::name().as_ptr());

    let create_info = *vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_layer_names(&layer_names_raw)
        .enabled_extension_names(&extension_names);

    entry
        .create_instance(&create_info, None)
        .expect("Instance creation error")
}

unsafe fn create_debug_call_back(debug_utils_loader: &DebugUtils) -> vk::DebugUtilsMessengerEXT {
    let debug_info = *vk::DebugUtilsMessengerCreateInfoEXT::builder()
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
        .pfn_user_callback(Some(vulkan_debug_callback));
    debug_utils_loader
        .create_debug_utils_messenger(&debug_info, None)
        .unwrap()
}

unsafe fn create_surface(
    entry: &Entry,
    instance: &Instance,
    window_handle: &dyn HasRawWindowHandle,
) -> vk::SurfaceKHR {
    ash_window::create_surface(entry, instance, &window_handle, None).unwrap()
}

unsafe fn get_physical_device(
    entry: &Entry,
    instance: &Instance,
    surface: &vk::SurfaceKHR,
    surface_loader: &Surface,
) -> (PhysicalDevice, u32) {
    let pdevices = instance
        .enumerate_physical_devices()
        .expect("Physical device error");
    let (pdevice, queue_family_index) = pdevices
        .iter()
        .find_map(|pdevice| {
            instance
                .get_physical_device_queue_family_properties(*pdevice)
                .iter()
                .enumerate()
                .find_map(|(index, info)| {
                    let supports_graphic_and_surface = info
                        .queue_flags
                        .contains(vk::QueueFlags::GRAPHICS)
                        && surface_loader
                            .get_physical_device_surface_support(*pdevice, index as u32, *surface)
                            .unwrap();
                    if supports_graphic_and_surface {
                        Some((*pdevice, index))
                    } else {
                        None
                    }
                })
        })
        .expect("Couldn't find suitable device.");
    (pdevice, queue_family_index as u32)
}

unsafe fn create_device(
    instance: &Instance,
    pdevice: &vk::PhysicalDevice,
    queue_family_index: u32,
) -> Device {
    let device_extension_names_raw = [Swapchain::name().as_ptr()];
    let features = vk::PhysicalDeviceFeatures {
        shader_clip_distance: 1,
        ..Default::default()
    };
    let priorities = [1.0];
    let queue_info = *vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(queue_family_index)
        .queue_priorities(&priorities);
    let device_create_info = *vk::DeviceCreateInfo::builder()
        .queue_create_infos(std::slice::from_ref(&queue_info))
        .enabled_extension_names(&device_extension_names_raw)
        .enabled_features(&features);
    let device: Device = instance
        .create_device(*pdevice, &device_create_info, None)
        .unwrap();
    device
}

unsafe fn create_swapchain(
    pdevice: &PhysicalDevice,
    surface_loader: &Surface,
    surface: &vk::SurfaceKHR,
    surface_format: &vk::SurfaceFormatKHR,
    swapchain_loader: &Swapchain,
) -> (vk::SwapchainKHR, vk::Extent2D) {
    let surface_capabilities = surface_loader
        .get_physical_device_surface_capabilities(*pdevice, *surface)
        .unwrap();
    let mut desired_image_count = surface_capabilities.min_image_count + 1;
    if surface_capabilities.max_image_count > 0
        && desired_image_count > surface_capabilities.max_image_count
    {
        desired_image_count = surface_capabilities.max_image_count;
    }
    let surface_resolution = match surface_capabilities.current_extent.width {
        std::u32::MAX => vk::Extent2D {
            width: 1920,
            height: 1080,
        },
        _ => surface_capabilities.current_extent,
    };
    let pre_transform = if surface_capabilities
        .supported_transforms
        .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
    {
        vk::SurfaceTransformFlagsKHR::IDENTITY
    } else {
        surface_capabilities.current_transform
    };
    let present_modes = surface_loader
        .get_physical_device_surface_present_modes(*pdevice, *surface)
        .unwrap();
    let present_mode = present_modes
        .iter()
        .cloned()
        .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
        .unwrap_or(vk::PresentModeKHR::FIFO);
    let swapchain_create_info = *vk::SwapchainCreateInfoKHR::builder()
        .surface(*surface)
        .min_image_count(desired_image_count)
        .image_color_space(surface_format.color_space)
        .image_format(surface_format.format)
        .image_extent(surface_resolution)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(pre_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .image_array_layers(1);
    let swapchain = swapchain_loader
        .create_swapchain(&swapchain_create_info, None)
        .unwrap();
    (swapchain, surface_resolution)
}

unsafe fn create_command_pool(device: &Device, queue_family_index: u32) -> vk::CommandPool {
    let pool_create_info = *vk::CommandPoolCreateInfo::builder()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(queue_family_index);
    device.create_command_pool(&pool_create_info, None).unwrap()
}

unsafe fn create_command_buffers(
    device: &Device,
    pool: &vk::CommandPool,
) -> Vec<vk::CommandBuffer> {
    let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
        .command_buffer_count(2)
        .command_pool(*pool)
        .level(vk::CommandBufferLevel::PRIMARY);

    device
        .allocate_command_buffers(&command_buffer_allocate_info)
        .unwrap()
}

unsafe fn create_present_image_views(
    device: &Device,
    swapchain_loader: &Swapchain,
    swapchain: &vk::SwapchainKHR,
    surface_format: &vk::SurfaceFormatKHR,
) -> Vec<vk::ImageView> {
    let present_images = swapchain_loader.get_swapchain_images(*swapchain).unwrap();
    present_images
        .iter()
        .map(|&image| {
            let create_view_info = *vk::ImageViewCreateInfo::builder()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::R,
                    g: vk::ComponentSwizzle::G,
                    b: vk::ComponentSwizzle::B,
                    a: vk::ComponentSwizzle::A,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image(image);
            device.create_image_view(&create_view_info, None).unwrap()
        })
        .collect()
}

fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && memory_type.property_flags & flags == flags
        })
        .map(|(index, _memory_type)| index as _)
}

unsafe fn create_depth_image(
    instance: &Instance,
    pdevice: &PhysicalDevice,
    device: &Device,
    surface_resolution: &vk::Extent2D,
) -> (vk::Image, vk::DeviceMemory) {
    let device_memory_properties = instance.get_physical_device_memory_properties(*pdevice);
    let depth_image_create_info = *vk::ImageCreateInfo::builder()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::D16_UNORM)
        .extent((*surface_resolution).into())
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let depth_image = device.create_image(&depth_image_create_info, None).unwrap();
    let depth_image_memory_req = device.get_image_memory_requirements(depth_image);
    let depth_image_memory_index = find_memorytype_index(
        &depth_image_memory_req,
        &device_memory_properties,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )
    .expect("Unable to find suitable memory index for depth image.");
    let depth_image_allocate_info = *vk::MemoryAllocateInfo::builder()
        .allocation_size(depth_image_memory_req.size)
        .memory_type_index(depth_image_memory_index);

    let depth_image_memory = device
        .allocate_memory(&depth_image_allocate_info, None)
        .unwrap();

    device
        .bind_image_memory(depth_image, depth_image_memory, 0)
        .expect("Unable to bind depth image memory");

    (depth_image, depth_image_memory)
}

unsafe fn optimize_depth_image_layout(
    device: &Device,
    setup_command_buffer: &vk::CommandBuffer,
    setup_commands_reuse_fence: &vk::Fence,
    present_queue: &vk::Queue,
    depth_image: &vk::Image,
) {
    record_submit_commandbuffer(
        device,
        *setup_command_buffer,
        *setup_commands_reuse_fence,
        *present_queue,
        &[],
        &[],
        &[],
        |device, setup_command_buffer| {
            let layout_transition_barriers = *vk::ImageMemoryBarrier::builder()
                .image(*depth_image)
                .dst_access_mask(
                    vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                        | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                )
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .subresource_range(
                    *vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .layer_count(1)
                        .level_count(1),
                );

            device.cmd_pipeline_barrier(
                setup_command_buffer,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[layout_transition_barriers],
            );
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn record_submit_commandbuffer<F: FnOnce(&Device, vk::CommandBuffer)>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    command_buffer_reuse_fence: vk::Fence,
    submit_queue: vk::Queue,
    wait_mask: &[vk::PipelineStageFlags],
    wait_semaphores: &[vk::Semaphore],
    signal_semaphores: &[vk::Semaphore],
    f: F,
) {
    unsafe {
        device
            .wait_for_fences(&[command_buffer_reuse_fence], true, std::u64::MAX)
            .expect("Wait for fence failed.");

        device
            .reset_fences(&[command_buffer_reuse_fence])
            .expect("Reset fences failed.");

        device
            .reset_command_buffer(
                command_buffer,
                vk::CommandBufferResetFlags::RELEASE_RESOURCES,
            )
            .expect("Reset command buffer failed.");

        let command_buffer_begin_info = *vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        device
            .begin_command_buffer(command_buffer, &command_buffer_begin_info)
            .expect("Begin commandbuffer");
        f(device, command_buffer);
        device
            .end_command_buffer(command_buffer)
            .expect("End commandbuffer");

        let command_buffers = vec![command_buffer];

        let submit_info = *vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_mask)
            .command_buffers(&command_buffers)
            .signal_semaphores(signal_semaphores);

        device
            .queue_submit(submit_queue, &[submit_info], command_buffer_reuse_fence)
            .expect("queue submit failed.");
    }
}

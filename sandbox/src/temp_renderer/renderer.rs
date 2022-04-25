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
    entry: Entry,
    instance: Instance,
    device: Device,
}

impl Renderer {
    pub fn new(window_handle: &dyn HasRawWindowHandle) -> Self {
        unsafe {
            let entry = Entry::linked();
            let instance = create_instance(&entry, window_handle);
            let debug_callback = create_debug_call_back(&entry, &instance);
            let surface = create_surface(&entry, &instance, window_handle);
            let (pdevice, queue_family_index) = get_physical_device(&entry, &instance, &surface);
            let device = create_device(&instance, &pdevice, queue_family_index);
            Self {
                entry: entry,
                instance: instance,
                device: device,
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

    let instance = entry
        .create_instance(&create_info, None)
        .expect("Instance creation error");
    instance
}

unsafe fn create_debug_call_back(entry: &Entry, instance: &Instance) -> vk::DebugUtilsMessengerEXT {
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
    let debug_utils_loader = DebugUtils::new(&entry, &instance);
    let debug_callback = debug_utils_loader
        .create_debug_utils_messenger(&debug_info, None)
        .unwrap();
    debug_callback
}

unsafe fn create_surface(
    entry: &Entry,
    instance: &Instance,
    window_handle: &dyn HasRawWindowHandle,
) -> vk::SurfaceKHR {
    ash_window::create_surface(&entry, &instance, &window_handle, None).unwrap()
}

unsafe fn get_physical_device(
    entry: &Entry,
    instance: &Instance,
    surface: &vk::SurfaceKHR,
) -> (PhysicalDevice, u32) {
    let pdevices = instance
        .enumerate_physical_devices()
        .expect("Physical device error");
    let surface_loader = Surface::new(&entry, &instance);
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

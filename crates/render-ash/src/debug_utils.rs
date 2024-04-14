use std::{
    ffi::{c_void, CStr},
    os::raw::c_char,
};

use ash::vk::{self, Handle};
use tracing::{debug, error, info, warn};

pub const EXT_NAME: &'static CStr = ash::ext::debug_utils::NAME;

unsafe fn c_str_from_ptr<'a>(str_ptr: *const c_char) -> &'a CStr {
    if str_ptr.is_null() {
        c""
    } else {
        CStr::from_ptr(str_ptr)
    }
}

unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _p_user_data: *mut c_void,
) -> vk::Bool32 {
    use vk::DebugUtilsMessageSeverityFlagsEXT;

    if std::thread::panicking() {
        return vk::FALSE;
    }

    let message = c_str_from_ptr((*p_callback_data).p_message);
    let message_id_name = c_str_from_ptr((*p_callback_data).p_message_id_name);
    let message_id_number = (*p_callback_data).message_id_number;

    // TODO: queues, labels, objects, ...

    match message_severity {
        DebugUtilsMessageSeverityFlagsEXT::VERBOSE => {
            debug!(
                "Vk[{:?},#{},{:?}]: {}",
                message_type,
                message_id_number,
                message_id_name,
                message.to_string_lossy()
            )
        }
        DebugUtilsMessageSeverityFlagsEXT::INFO => {
            info!(
                "Vk[{:?},#{},{:?}]: {}",
                message_type,
                message_id_number,
                message_id_name,
                message.to_string_lossy()
            )
        }
        DebugUtilsMessageSeverityFlagsEXT::WARNING => {
            warn!(
                "Vk[{:?},#{},{:?}]: {}",
                message_type,
                message_id_number,
                message_id_name,
                message.to_string_lossy()
            )
        }
        DebugUtilsMessageSeverityFlagsEXT::ERROR => {
            error!(
                "Vk[{:?},#{},{:?}]: {}",
                message_type,
                message_id_number,
                message_id_name,
                message.to_string_lossy()
            )
        }
        _ => {
            warn!(
                "Vk[{:?},#{},{:?}]: {}",
                message_type,
                message_id_number,
                message_id_name,
                message.to_string_lossy()
            )
        }
    };

    vk::FALSE
}

// stack-allocated buffer for keeping a copy of the object_name (for appending \0-byte)
// + optional Vector for allocations
struct CStrBuf {
    buf: [u8; 64],
    alloc: Vec<u8>,
}

impl CStrBuf {
    #[inline]
    const fn new() -> Self {
        Self {
            buf: [0; 64],
            alloc: Vec::new(),
        }
    }

    #[inline]
    fn get_cstr<'a>(&'a mut self, s: &'a str) -> &'a CStr {
        if s.ends_with('\0') {
            // SAFETY: string always ends with 0-byte.
            // Don't care, if there are 0-bytes before end.
            unsafe { CStr::from_bytes_with_nul_unchecked(s.as_bytes()) }
        } else {
            let bytes = s.as_bytes();
            let len = bytes.len();
            if len < self.buf.len() {
                self.buf[..len].copy_from_slice(bytes);
                self.buf[len] = 0;
                // SAFETY: string always ends with 0-byte.
                // Don't care, if there are 0-bytes before end.
                return unsafe { CStr::from_bytes_with_nul_unchecked(&self.buf[..len + 1]) };
            }
            self.alloc.clear();
            self.alloc.reserve_exact(len + 1);
            self.alloc.extend_from_slice(s.as_bytes());
            self.alloc.push(0);
            // SAFETY: string always ends with 0-byte.
            // Don't care, if there are 0-bytes before end.
            unsafe { CStr::from_bytes_with_nul_unchecked(&self.alloc) }
        }
    }
}

pub struct InstanceDebugUtils {
    functions: ash::ext::debug_utils::Instance,
    utils_messenger: vk::DebugUtilsMessengerEXT,
}
#[repr(transparent)]
pub struct DeviceDebugUtils(ash::ext::debug_utils::Device);

impl InstanceDebugUtils {

    pub fn new(
        entry: &ash::Entry,
        instance: &ash::Instance,
        message_severities: vk::DebugUtilsMessageSeverityFlagsEXT,
    ) -> Self {
        let functions = ash::ext::debug_utils::Instance::new(entry, instance);
        if message_severities.is_empty() {
            Self {
                functions,
                utils_messenger: vk::DebugUtilsMessengerEXT::null(),
            }
        } else {
            let utils_messenger = unsafe {
                functions
                    .create_debug_utils_messenger(
                        &vk::DebugUtilsMessengerCreateInfoEXT::default()
                            .message_severity(message_severities)
                            .message_type(
                                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                            )
                            .pfn_user_callback(Some(debug_callback)),
                        None,
                    )
                    .expect("Debug Utils Callback")
            };

            Self {
                functions,
                utils_messenger,
            }
        }
    }

    #[inline(always)]
    pub fn is_messenger_enabled(&self) -> bool {
        !self.utils_messenger.is_null()
    }
}

impl DeviceDebugUtils {
    pub fn new(
        instance: &ash::Instance,
        device: &ash::Device,
    ) -> Self {
        let functions = ash::ext::debug_utils::Device::new(instance, device);
        Self(functions)
    }

    #[inline(always)]
    pub unsafe fn object_name<H: Handle>(
        &self,
        handle: H,
        object_name: &str,
    ) {
        self._object_name(H::TYPE, handle.as_raw(), object_name)
    }
    #[inline(always)]
    pub unsafe fn object_name_cstr<H: Handle>(
        &self,
        handle: H,
        object_name: &CStr,
    ) {
        self._object_name_cstr(H::TYPE, handle.as_raw(), object_name)
    }

    #[inline]
    unsafe fn _object_name(
        &self,
        object_type: vk::ObjectType,
        object_handle: u64,
        object_name: &str,
    ) {
        if object_handle == 0 {
            return;
        }

        let mut cstr_buf = CStrBuf::new();
        self._object_name_cstr(
            object_type,
            object_handle,
            cstr_buf.get_cstr(object_name),
        )
    }

    unsafe fn _object_name_cstr(
        &self,
        object_type: vk::ObjectType,
        object_handle: u64,
        object_name: &CStr,
    ) {
        if object_handle == 0 {
            return;
        }
        let _result = self.0.set_debug_utils_object_name(
            &vk::DebugUtilsObjectNameInfoEXT {
                object_handle,
                object_type,    
                p_object_name: object_name.as_ptr(),
                ..Default::default()
            },
        );
    }

    #[inline]
    pub unsafe fn cmd_insert_debug_label(&self, command_buffer: vk::CommandBuffer, label: &str) {
        let mut cstr_buf = CStrBuf::new();
        self.cmd_insert_debug_label_cstr(command_buffer, cstr_buf.get_cstr(label))
    }
    pub unsafe fn cmd_insert_debug_label_cstr(
        &self,
        command_buffer: vk::CommandBuffer,
        label: &CStr,
    ) {
        self.0.cmd_insert_debug_utils_label(
            command_buffer,
            &vk::DebugUtilsLabelEXT::default().label_name(label),
        );
    }

    #[inline]
    pub unsafe fn cmd_begin_debug_label(&self, command_buffer: vk::CommandBuffer, label: &str) {
        let mut cstr_buf = CStrBuf::new();
        self.cmd_begin_debug_label_cstr(command_buffer, cstr_buf.get_cstr(label))
    }
    pub unsafe fn cmd_begin_debug_label_cstr(
        &self,
        command_buffer: vk::CommandBuffer,
        label: &CStr,
    ) {
        self.0.cmd_begin_debug_utils_label(
            command_buffer,
            &vk::DebugUtilsLabelEXT::default().label_name(label),
        );
    }

    #[inline]
    pub unsafe fn cmd_end_debug_label(&self, command_buffer: vk::CommandBuffer) {
        self.0.cmd_end_debug_utils_label(command_buffer);
    }

    #[inline]
    pub unsafe fn queue_insert_debug_label(&self, queue: vk::Queue, label: &str) {
        let mut cstr_buf = CStrBuf::new();
        self.queue_insert_debug_label_cstr(queue, cstr_buf.get_cstr(label))
    }

    pub unsafe fn queue_insert_debug_label_cstr(&self, queue: vk::Queue, label: &CStr) {
        self.0.queue_insert_debug_utils_label(
            queue,
            &vk::DebugUtilsLabelEXT::default().label_name(label),
        );
    }

    #[inline]
    pub unsafe fn queue_begin_debug_label(&self, queue: vk::Queue, label: &str) {
        let mut cstr_buf = CStrBuf::new();
        self.queue_begin_debug_label_cstr(queue, cstr_buf.get_cstr(label))
    }

    pub unsafe fn queue_begin_debug_label_cstr(&self, queue: vk::Queue, label: &CStr) {
        self.0.queue_begin_debug_utils_label(
            queue,
            &vk::DebugUtilsLabelEXT::default().label_name(label),
        );
    }

    #[inline]
    pub unsafe fn queue_end_debug_label(&self, queue: vk::Queue) {
        self.0.queue_end_debug_utils_label(queue);
    }
}

impl Drop for InstanceDebugUtils {
    fn drop(&mut self) {
        if self.utils_messenger != vk::DebugUtilsMessengerEXT::null() {
            let utils_messenger = std::mem::take(&mut self.utils_messenger);
            unsafe {
                self.functions
                    .destroy_debug_utils_messenger(utils_messenger, None);
            }
        }
    }
}



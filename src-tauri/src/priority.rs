#[cfg(target_os = "windows")]
extern "system" {
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn SetPriorityClass(hProcess: *mut std::ffi::c_void, dwPriorityClass: u32) -> i32;
}

#[cfg(target_os = "windows")]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;
#[cfg(target_os = "windows")]
const NORMAL_PRIORITY_CLASS: u32 = 0x00000020;

/// RAII guard that temporarily lowers process priority to BELOW_NORMAL during
/// intensive compression or extraction operations, ensuring that foreground games,
/// audio rendering, and OS responsiveness stay 100% smooth even under maximum thread load.
pub struct ScopedPriorityGuard {
    #[cfg(target_os = "windows")]
    active: bool,
}

impl ScopedPriorityGuard {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            unsafe {
                SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS);
            }
            Self { active: true }
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self {}
        }
    }
}

impl Default for ScopedPriorityGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ScopedPriorityGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        if self.active {
            unsafe {
                SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
            }
        }
    }
}

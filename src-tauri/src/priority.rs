#[cfg(target_os = "windows")]
extern "system" {
    fn GetCurrentThread() -> *mut std::ffi::c_void;
    fn SetThreadPriority(hThread: *mut std::ffi::c_void, nPriority: i32) -> i32;
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn SetPriorityClass(hProcess: *mut std::ffi::c_void, dwPriorityClass: u32) -> i32;
}

#[cfg(target_os = "windows")]
const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
#[cfg(target_os = "windows")]
const THREAD_PRIORITY_NORMAL: i32 = 0;
#[cfg(target_os = "windows")]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;
#[cfg(target_os = "windows")]
const NORMAL_PRIORITY_CLASS: u32 = 0x00000020;

/// RAII guard that temporarily lowers worker thread and process priority to BELOW_NORMAL_PRIORITY_CLASS
/// and THREAD_PRIORITY_BELOW_NORMAL during intensive compression or extraction operations.
/// This ensures all internal worker threads (e.g. zstdmt, rayon) and the main worker thread
/// yield CPU cycles to foreground games and 3D rendering without causing OS frame drops.
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
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
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
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL);
            }
        }
    }
}

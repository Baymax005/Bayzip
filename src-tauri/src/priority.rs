#[cfg(target_os = "windows")]
extern "system" {
    fn GetCurrentThread() -> *mut std::ffi::c_void;
    fn SetThreadPriority(hThread: *mut std::ffi::c_void, nPriority: i32) -> i32;
}

#[cfg(target_os = "windows")]
const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
#[cfg(target_os = "windows")]
const THREAD_PRIORITY_NORMAL: i32 = 0;

/// RAII guard that temporarily lowers worker thread priority to THREAD_PRIORITY_BELOW_NORMAL
/// during intensive compression or extraction operations, ensuring that foreground games,
/// audio rendering, and OS responsiveness stay 100% smooth without triggering Windows process-level
/// background I/O disk throttling (IoPriorityLow).
pub struct ScopedPriorityGuard {
    #[cfg(target_os = "windows")]
    active: bool,
}

impl ScopedPriorityGuard {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            unsafe {
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
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL);
            }
        }
    }
}

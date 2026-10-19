use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

/// Global toggle for Background Priority Shield (Smooth System Responsiveness).
/// When true (default): Bayzip runs at BELOW_NORMAL so games/apps stay smooth.
/// When false: Bayzip runs at full NORMAL priority for maximum foreground aggression.
pub static BACKGROUND_SHIELD_ENABLED: AtomicBool = AtomicBool::new(true);

/// Atomic reference counter tracking how many active operations currently hold a priority guard.
static ACTIVE_GUARD_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn set_background_shield(enabled: bool) {
    BACKGROUND_SHIELD_ENABLED.store(enabled, Ordering::SeqCst);
    #[cfg(target_os = "windows")]
    if !enabled {
        // If disabled mid-flight, immediately restore normal priority
        unsafe {
            SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL);
        }
    }
}

pub fn is_background_shield_enabled() -> bool {
    BACKGROUND_SHIELD_ENABLED.load(Ordering::Relaxed)
}

/// RAII guard that temporarily lowers worker thread and process priority to BELOW_NORMAL_PRIORITY_CLASS
/// and THREAD_PRIORITY_BELOW_NORMAL during intensive compression or extraction operations.
/// Uses atomic reference counting so overlapping operations never prematurely restore normal priority.
pub struct ScopedPriorityGuard {
    #[cfg(target_os = "windows")]
    active: bool,
}

impl ScopedPriorityGuard {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            if is_background_shield_enabled() {
                let prev = ACTIVE_GUARD_COUNT.fetch_add(1, Ordering::SeqCst);
                if prev == 0 {
                    unsafe {
                        SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS);
                        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                    }
                }
                return Self { active: true };
            }
            Self { active: false }
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
            let prev = ACTIVE_GUARD_COUNT.fetch_sub(1, Ordering::SeqCst);
            if prev == 1 {
                // Last active compression/extraction finished — restore normal priority
                unsafe {
                    SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
                    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL);
                }
            }
        }
    }
}


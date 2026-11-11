use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

#[cfg(target_os = "windows")]
extern "system" {
    fn GetCurrentThread() -> *mut std::ffi::c_void;
    fn GetThreadPriority(hThread: *mut std::ffi::c_void) -> i32;
    fn SetThreadPriority(hThread: *mut std::ffi::c_void, nPriority: i32) -> i32;
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn GetPriorityClass(hProcess: *mut std::ffi::c_void) -> u32;
    fn SetPriorityClass(hProcess: *mut std::ffi::c_void, dwPriorityClass: u32) -> i32;
}

#[cfg(target_os = "windows")]
pub const THREAD_PRIORITY_ERROR_RETURN: i32 = 0x7FFFFFFF;
#[cfg(target_os = "windows")]
pub const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
#[cfg(target_os = "windows")]
pub const THREAD_PRIORITY_NORMAL: i32 = 0;
#[cfg(target_os = "windows")]
pub const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;
#[cfg(target_os = "windows")]
pub const NORMAL_PRIORITY_CLASS: u32 = 0x00000020;

#[cfg(target_os = "windows")]
static ACTIVE_PROCESS_PRIORITY_GUARDS: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "windows")]
static ORIGINAL_PROCESS_PRIORITY_CLASS: AtomicU32 = AtomicU32::new(NORMAL_PRIORITY_CLASS);

/// Global toggle for Background Priority Shield (Smooth System Responsiveness).
/// When true (default): Bayzip runs at BELOW_NORMAL so games/apps stay smooth.
/// When false: Bayzip runs at full NORMAL priority for maximum foreground aggression.
pub static BACKGROUND_SHIELD_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_background_shield(enabled: bool) {
    BACKGROUND_SHIELD_ENABLED.store(enabled, Ordering::SeqCst);
    #[cfg(target_os = "windows")]
    if !enabled {
        // If disabled mid-flight, immediately restore normal priority on the current thread and process
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL);
            SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
        }
    }
}

pub fn is_background_shield_enabled() -> bool {
    BACKGROUND_SHIELD_ENABLED.load(Ordering::Relaxed)
}

/// Returns the process creation flags for child processes (e.g. tar.exe),
/// incorporating BELOW_NORMAL_PRIORITY_CLASS when background shield is enabled.
pub fn get_child_process_creation_flags() -> u32 {
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    if is_background_shield_enabled() {
        CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS
    } else {
        CREATE_NO_WINDOW
    }
}

/// RAII guard that temporarily lowers both thread priority and process priority class
/// to BELOW_NORMAL during intensive compression or extraction operations.
/// 
/// Refactored per CRITICAL-05: Depresses the process-wide priority class in addition
/// to thread priority so that multi-threaded compression pools (libzstd, 7-zip)
/// and child processes run smoothly without starving foreground games or OS UI.
#[derive(Debug)]
pub struct ScopedPriorityGuard {
    #[cfg(target_os = "windows")]
    previous_priority: Option<i32>,
    #[cfg(target_os = "windows")]
    process_depressed: bool,
}

impl ScopedPriorityGuard {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            if is_background_shield_enabled() {
                unsafe {
                    // 1. Depress process priority class so that internal libzstd / 7z worker thread pools
                    // are governed by BELOW_NORMAL_PRIORITY_CLASS (CRITICAL-05)
                    if ACTIVE_PROCESS_PRIORITY_GUARDS.fetch_add(1, Ordering::SeqCst) == 0 {
                        let proc = GetCurrentProcess();
                        let current_class = GetPriorityClass(proc);
                        if current_class != 0 {
                            ORIGINAL_PROCESS_PRIORITY_CLASS.store(current_class, Ordering::SeqCst);
                        }
                        SetPriorityClass(proc, BELOW_NORMAL_PRIORITY_CLASS);
                    }

                    // 2. Depress the calling thread's own priority
                    let thread = GetCurrentThread();
                    let current_priority = GetThreadPriority(thread);
                    if current_priority != THREAD_PRIORITY_ERROR_RETURN {
                        let success = SetThreadPriority(thread, THREAD_PRIORITY_BELOW_NORMAL);
                        if success != 0 {
                            return Self {
                                previous_priority: Some(current_priority),
                                process_depressed: true,
                            };
                        }
                    }
                    return Self {
                        previous_priority: None,
                        process_depressed: true,
                    };
                }
            }
            Self {
                previous_priority: None,
                process_depressed: false,
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self {}
        }
    }

    #[cfg(target_os = "windows")]
    pub fn previous_priority(&self) -> Option<i32> {
        self.previous_priority
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
        {
            if let Some(prev) = self.previous_priority {
                unsafe {
                    let thread = GetCurrentThread();
                    SetThreadPriority(thread, prev);
                }
            }
            if self.process_depressed
                && ACTIVE_PROCESS_PRIORITY_GUARDS.fetch_sub(1, Ordering::SeqCst) == 1
            {
                unsafe {
                    let proc = GetCurrentProcess();
                    let orig = ORIGINAL_PROCESS_PRIORITY_CLASS.load(Ordering::SeqCst);
                    SetPriorityClass(proc, orig);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    #[cfg(target_os = "windows")]
    fn test_scoped_priority_guard_restores_thread_priority() {
        let _lock = TEST_LOCK.lock().unwrap();
        set_background_shield(true);
        unsafe {
            let thread = GetCurrentThread();
            SetThreadPriority(thread, THREAD_PRIORITY_NORMAL);
            let initial = GetThreadPriority(thread);
            assert_eq!(initial, THREAD_PRIORITY_NORMAL);

            {
                let guard = ScopedPriorityGuard::new();
                assert_eq!(guard.previous_priority(), Some(THREAD_PRIORITY_NORMAL));
                let lowered = GetThreadPriority(thread);
                assert_eq!(lowered, THREAD_PRIORITY_BELOW_NORMAL);
                let proc_class = GetPriorityClass(GetCurrentProcess());
                assert_eq!(proc_class, BELOW_NORMAL_PRIORITY_CLASS);
            }

            // After drop, thread priority and process priority class must be restored to NORMAL
            let restored = GetThreadPriority(thread);
            assert_eq!(restored, THREAD_PRIORITY_NORMAL);
            let restored_class = GetPriorityClass(GetCurrentProcess());
            assert_eq!(restored_class, NORMAL_PRIORITY_CLASS);
        }
    }

    #[test]
    fn test_background_shield_toggle() {
        let _lock = TEST_LOCK.lock().unwrap();
        set_background_shield(false);
        assert!(!is_background_shield_enabled());
        let guard = ScopedPriorityGuard::new();
        assert_eq!(guard.previous_priority(), None);
        set_background_shield(true);
        assert!(is_background_shield_enabled());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_nested_scoped_priority_guards_unwind_correctly() {
        let _lock = TEST_LOCK.lock().unwrap();
        set_background_shield(true);
        unsafe {
            let thread = GetCurrentThread();
            SetThreadPriority(thread, THREAD_PRIORITY_NORMAL);
            assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_NORMAL);

            {
                let outer_guard = ScopedPriorityGuard::new();
                assert_eq!(outer_guard.previous_priority(), Some(THREAD_PRIORITY_NORMAL));
                assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_BELOW_NORMAL);

                {
                    let inner_guard = ScopedPriorityGuard::new();
                    assert_eq!(inner_guard.previous_priority(), Some(THREAD_PRIORITY_BELOW_NORMAL));
                    assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_BELOW_NORMAL);
                }

                // Inner guard dropped; priority should still be BELOW_NORMAL
                assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_BELOW_NORMAL);
            }

            // Outer guard dropped; priority must be restored to NORMAL
            assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_NORMAL);
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_concurrent_threads_priority_guards_restore_independently() {
        let _lock = TEST_LOCK.lock().unwrap();
        set_background_shield(true);
        let handles: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    unsafe {
                        let thread = GetCurrentThread();
                        SetThreadPriority(thread, THREAD_PRIORITY_NORMAL);
                        assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_NORMAL);

                        {
                            let guard = ScopedPriorityGuard::new();
                            assert_eq!(guard.previous_priority(), Some(THREAD_PRIORITY_NORMAL));
                            assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_BELOW_NORMAL);
                        }

                        // Thread must be restored to NORMAL upon guard drop
                        assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_NORMAL);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Worker thread panicked");
        }
    }
}


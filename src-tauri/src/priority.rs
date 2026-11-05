use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "windows")]
extern "system" {
    fn GetCurrentThread() -> *mut std::ffi::c_void;
    fn GetThreadPriority(hThread: *mut std::ffi::c_void) -> i32;
    fn SetThreadPriority(hThread: *mut std::ffi::c_void, nPriority: i32) -> i32;
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

/// Global toggle for Background Priority Shield (Smooth System Responsiveness).
/// When true (default): Bayzip runs at BELOW_NORMAL so games/apps stay smooth.
/// When false: Bayzip runs at full NORMAL priority for maximum foreground aggression.
pub static BACKGROUND_SHIELD_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_background_shield(enabled: bool) {
    BACKGROUND_SHIELD_ENABLED.store(enabled, Ordering::SeqCst);
    #[cfg(target_os = "windows")]
    if !enabled {
        // If disabled mid-flight, immediately restore normal priority on the current thread
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL);
        }
    }
}

pub fn is_background_shield_enabled() -> bool {
    BACKGROUND_SHIELD_ENABLED.load(Ordering::Relaxed)
}

/// RAII guard that temporarily lowers worker thread priority to THREAD_PRIORITY_BELOW_NORMAL
/// during intensive compression or extraction operations.
/// 
/// Refactored per V-CONC-01: Saves the calling thread's original OS thread priority upon
/// creation and restores that exact priority upon `Drop` per-thread. This eliminates
/// the global refcount coupling that previously caused Tokio pooled worker threads
/// to remain permanently depressed across unrelated tasks.
#[derive(Debug)]
pub struct ScopedPriorityGuard {
    #[cfg(target_os = "windows")]
    previous_priority: Option<i32>,
}

impl ScopedPriorityGuard {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            if is_background_shield_enabled() {
                unsafe {
                    let thread = GetCurrentThread();
                    let current_priority = GetThreadPriority(thread);
                    if current_priority != THREAD_PRIORITY_ERROR_RETURN {
                        let success = SetThreadPriority(thread, THREAD_PRIORITY_BELOW_NORMAL);
                        if success != 0 {
                            return Self {
                                previous_priority: Some(current_priority),
                            };
                        }
                    }
                }
            }
            Self {
                previous_priority: None,
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
        if let Some(prev) = self.previous_priority {
            unsafe {
                let thread = GetCurrentThread();
                SetThreadPriority(thread, prev);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn test_scoped_priority_guard_restores_thread_priority() {
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
            }

            // After drop, thread priority must be restored to NORMAL (not left depressed)
            let restored = GetThreadPriority(thread);
            assert_eq!(restored, THREAD_PRIORITY_NORMAL);
        }
    }

    #[test]
    fn test_background_shield_toggle() {
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


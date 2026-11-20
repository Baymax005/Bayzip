use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
extern "system" {
    fn GetCurrentThread() -> *mut std::ffi::c_void;
    fn GetCurrentThreadId() -> u32;
    fn OpenThread(dwDesiredAccess: u32, bInheritHandle: i32, dwThreadId: u32) -> *mut std::ffi::c_void;
    fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
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
const THREAD_SET_AND_QUERY_INFO: u32 = 0x0C60;

#[cfg(target_os = "windows")]
struct ActiveThreadPriority {
    thread_id: u32,
    handle_usize: usize,
    original_priority: i32,
    ref_count: usize,
}

#[cfg(target_os = "windows")]
struct ProcessPriorityState {
    active_guards: usize,
    original_class: u32,
    active_threads: Vec<ActiveThreadPriority>,
}

#[cfg(target_os = "windows")]
static PROCESS_PRIORITY_STATE: Mutex<ProcessPriorityState> = Mutex::new(ProcessPriorityState {
    active_guards: 0,
    original_class: NORMAL_PRIORITY_CLASS,
    active_threads: Vec::new(),
});

/// Global toggle for Background Priority Shield (Smooth System Responsiveness).
/// When true (default): Bayzip runs at BELOW_NORMAL so games/apps stay smooth.
/// When false: Bayzip runs at full NORMAL priority for maximum foreground aggression.
pub static BACKGROUND_SHIELD_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_background_shield(enabled: bool) {
    BACKGROUND_SHIELD_ENABLED.store(enabled, Ordering::SeqCst);
    #[cfg(target_os = "windows")]
    if let Ok(state) = PROCESS_PRIORITY_STATE.lock() {
        if state.active_guards > 0 {
            unsafe {
                let proc = GetCurrentProcess();
                let target_class = if enabled {
                    BELOW_NORMAL_PRIORITY_CLASS
                } else {
                    state.original_class
                };
                SetPriorityClass(proc, target_class);
                for entry in &state.active_threads {
                    if entry.handle_usize != 0 {
                        let target_prio = if enabled {
                            THREAD_PRIORITY_BELOW_NORMAL
                        } else {
                            entry.original_priority
                        };
                        SetThreadPriority(entry.handle_usize as *mut std::ffi::c_void, target_prio);
                    }
                }
            }
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

#[cfg(test)]
thread_local! {
    static IN_PRIORITY_UNIT_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
    registered: bool,
}

impl ScopedPriorityGuard {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            #[cfg(test)]
            if !IN_PRIORITY_UNIT_TEST.with(|c| c.get()) {
                return Self {
                    previous_priority: None,
                    registered: false,
                };
            }

            let mut registered = false;
            let mut previous_priority = None;

            if let Ok(mut state) = PROCESS_PRIORITY_STATE.lock() {
                unsafe {
                    let proc = GetCurrentProcess();
                    if state.active_guards == 0 {
                        let orig = GetPriorityClass(proc);
                        state.original_class = if orig != 0 { orig } else { NORMAL_PRIORITY_CLASS };
                    }
                    state.active_guards += 1;
                    registered = true;

                    let shield_enabled = is_background_shield_enabled();
                    if state.active_guards == 1 && shield_enabled {
                        SetPriorityClass(proc, BELOW_NORMAL_PRIORITY_CLASS);
                    }

                    let thread = GetCurrentThread();
                    let current_priority = GetThreadPriority(thread);
                    let tid = GetCurrentThreadId();

                    if let Some(existing) = state.active_threads.iter_mut().find(|t| t.thread_id == tid) {
                        existing.ref_count += 1;
                    } else if current_priority != THREAD_PRIORITY_ERROR_RETURN {
                        let raw_handle = OpenThread(THREAD_SET_AND_QUERY_INFO, 0, tid);
                        state.active_threads.push(ActiveThreadPriority {
                            thread_id: tid,
                            handle_usize: raw_handle as usize,
                            original_priority: current_priority,
                            ref_count: 1,
                        });
                    }

                    if shield_enabled
                        && current_priority != THREAD_PRIORITY_ERROR_RETURN
                        && SetThreadPriority(thread, THREAD_PRIORITY_BELOW_NORMAL) != 0
                    {
                        previous_priority = Some(current_priority);
                    }
                }
            }

            Self {
                previous_priority,
                registered,
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
            if self.registered {
                if let Ok(mut state) = PROCESS_PRIORITY_STATE.lock() {
                    unsafe {
                        let thread = GetCurrentThread();
                        let tid = GetCurrentThreadId();
                        let mut removed_entry: Option<ActiveThreadPriority> = None;

                        if let Some(pos) = state.active_threads.iter().position(|t| t.thread_id == tid) {
                            state.active_threads[pos].ref_count =
                                state.active_threads[pos].ref_count.saturating_sub(1);
                            if state.active_threads[pos].ref_count == 0 {
                                removed_entry = Some(state.active_threads.swap_remove(pos));
                            }
                        }

                        if let Some(entry) = removed_entry {
                            if entry.original_priority != THREAD_PRIORITY_ERROR_RETURN {
                                SetThreadPriority(thread, entry.original_priority);
                            }
                            if entry.handle_usize != 0 {
                                CloseHandle(entry.handle_usize as *mut std::ffi::c_void);
                            }
                        } else if is_background_shield_enabled() {
                            if let Some(prev) = self.previous_priority {
                                SetThreadPriority(thread, prev);
                            }
                        }

                        state.active_guards = state.active_guards.saturating_sub(1);
                        if state.active_guards == 0 {
                            let proc = GetCurrentProcess();
                            SetPriorityClass(proc, state.original_class);
                        }
                    }
                }
            } else if let Some(prev) = self.previous_priority {
                unsafe {
                    SetThreadPriority(GetCurrentThread(), prev);
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

    struct PriorityTestLockGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

    impl PriorityTestLockGuard {
        fn new() -> Self {
            let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            IN_PRIORITY_UNIT_TEST.with(|c| c.set(true));
            Self(guard)
        }
    }

    impl Drop for PriorityTestLockGuard {
        fn drop(&mut self) {
            IN_PRIORITY_UNIT_TEST.with(|c| c.set(false));
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_scoped_priority_guard_restores_thread_priority() {
        let _lock = PriorityTestLockGuard::new();
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
        let _lock = PriorityTestLockGuard::new();
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
        let _lock = PriorityTestLockGuard::new();
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
        let _lock = PriorityTestLockGuard::new();
        set_background_shield(true);
        let handles: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    IN_PRIORITY_UNIT_TEST.with(|c| c.set(true));
                    unsafe {
                        let thread = GetCurrentThread();
                        for _ in 0..5 {
                            SetThreadPriority(thread, THREAD_PRIORITY_NORMAL);
                            if GetThreadPriority(thread) == THREAD_PRIORITY_NORMAL {
                                break;
                            }
                            std::thread::yield_now();
                        }
                        assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_NORMAL);

                        {
                            let guard = ScopedPriorityGuard::new();
                            assert_eq!(guard.previous_priority(), Some(THREAD_PRIORITY_NORMAL));
                            assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_BELOW_NORMAL);
                        }

                        // Thread must be restored to NORMAL upon guard drop
                        assert_eq!(GetThreadPriority(thread), THREAD_PRIORITY_NORMAL);
                    }
                    IN_PRIORITY_UNIT_TEST.with(|c| c.set(false));
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Worker thread panicked");
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_midflight_background_shield_toggle_syncs_process_priority() {
        let _lock = PriorityTestLockGuard::new();
        set_background_shield(true);
        unsafe {
            let proc = GetCurrentProcess();
            SetPriorityClass(proc, NORMAL_PRIORITY_CLASS);
            assert_eq!(GetPriorityClass(proc), NORMAL_PRIORITY_CLASS);

            let (tx_ready, rx_ready) = std::sync::mpsc::channel::<()>();
            let (tx_step1, rx_step1) = std::sync::mpsc::channel::<()>();
            let (tx_check1, rx_check1) = std::sync::mpsc::channel::<i32>();
            let (tx_step2, rx_step2) = std::sync::mpsc::channel::<()>();
            let (tx_check2, rx_check2) = std::sync::mpsc::channel::<i32>();

            let worker = std::thread::spawn(move || {
                IN_PRIORITY_UNIT_TEST.with(|c| c.set(true));
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL);
                {
                    let _worker_guard = ScopedPriorityGuard::new();
                    assert_eq!(GetThreadPriority(GetCurrentThread()), THREAD_PRIORITY_BELOW_NORMAL);
                    tx_ready.send(()).unwrap();

                    rx_step1.recv().unwrap();
                    tx_check1.send(GetThreadPriority(GetCurrentThread())).unwrap();

                    rx_step2.recv().unwrap();
                    tx_check2.send(GetThreadPriority(GetCurrentThread())).unwrap();
                }
                assert_eq!(GetThreadPriority(GetCurrentThread()), THREAD_PRIORITY_NORMAL);
                IN_PRIORITY_UNIT_TEST.with(|c| c.set(false));
            });

            rx_ready.recv().unwrap();
            assert_eq!(GetPriorityClass(proc), BELOW_NORMAL_PRIORITY_CLASS);

            // Disabling mid-flight restores both process class and worker thread priority immediately
            set_background_shield(false);
            assert_eq!(GetPriorityClass(proc), NORMAL_PRIORITY_CLASS);
            tx_step1.send(()).unwrap();
            assert_eq!(rx_check1.recv().unwrap(), THREAD_PRIORITY_NORMAL);

            // Re-enabling mid-flight re-applies BELOW_NORMAL to both process class and worker thread
            set_background_shield(true);
            assert_eq!(GetPriorityClass(proc), BELOW_NORMAL_PRIORITY_CLASS);
            tx_step2.send(()).unwrap();
            assert_eq!(rx_check2.recv().unwrap(), THREAD_PRIORITY_BELOW_NORMAL);

            worker.join().unwrap();
            assert_eq!(GetPriorityClass(proc), NORMAL_PRIORITY_CLASS);
        }
    }
}


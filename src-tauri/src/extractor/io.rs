use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::inspector::{
    clear_readonly,
    get_optimal_extract_inflight_budget,
    get_optimal_extract_writer_threads,
    safe_atomic_rename,
};
use crate::priority::ScopedPriorityGuard;
use crate::types::TaskCancelToken;
use super::{current_extraction_token, is_extraction_cancelled};

pub const PREALLOC_THRESHOLD_BYTES: u64 = 64 * 1024;
pub const ASYNC_INMEMORY_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Opens a file optimized for sequential front-to-back streaming reads (`FILE_FLAG_SEQUENTIAL_SCAN`).
pub fn open_sequential_read(path: &Path) -> std::io::Result<File> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x0800_0000) // FILE_FLAG_SEQUENTIAL_SCAN
            .open(path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        File::open(path)
    }
}

/// Opens a file with shared read access optimized for concurrent random-offset reads (`FILE_FLAG_RANDOM_ACCESS`).
pub fn open_random_read(path: &Path) -> std::io::Result<File> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001 | 0x0000_0002 | 0x0000_0004) // FILE_SHARE_READ | WRITE | DELETE
            .custom_flags(0x1000_0000) // FILE_FLAG_RANDOM_ACCESS
            .open(path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        File::open(path)
    }
}

/// Reserves contiguous NTFS clusters via `SetFileInformationByHandle(FileAllocationInfo)`
/// without altering logical EOF or triggering zero-fill penalties.
pub fn preallocate_file(file: &File, expected_size: u64) {
    if expected_size < PREALLOC_THRESHOLD_BYTES || expected_size > i64::MAX as u64 {
        return;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            FileAllocationInfo, SetFileInformationByHandle, FILE_ALLOCATION_INFO,
        };

        let alloc_info = FILE_ALLOCATION_INFO {
            AllocationSize: expected_size as i64,
        };
        unsafe {
            let _ = SetFileInformationByHandle(
                HANDLE(file.as_raw_handle()),
                FileAllocationInfo,
                &alloc_info as *const FILE_ALLOCATION_INFO as *const std::ffi::c_void,
                std::mem::size_of::<FILE_ALLOCATION_INFO>() as u32,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = file;
}

/// Creates or truncates an output file with sequential write hints and upfront NTFS cluster pre-allocation.
pub fn create_output_file_fast(path: &Path, expected_size: u64) -> std::io::Result<File> {
    #[cfg(target_os = "windows")]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .custom_flags(0x0800_0000) // FILE_FLAG_SEQUENTIAL_SCAN
            .open(path)?
    };
    #[cfg(not(target_os = "windows"))]
    let file = File::create(path)?;

    preallocate_file(&file, expected_size);
    Ok(file)
}

/// Writes an in-memory file payload to disk.
/// Uses direct sequential creation + cluster pre-allocation for brand-new files, and reserves
/// `.baypart` + `safe_atomic_rename` for overwriting pre-existing paths.
pub fn write_file_payload(out_path: &Path, data: &[u8], file_existed: bool) -> Result<(), String> {
    if !file_existed {
        let mut out_file = match create_output_file_fast(out_path, data.len() as u64) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                clear_readonly(out_path);
                create_output_file_fast(out_path, data.len() as u64)
                    .map_err(|e2| format!("Failed to create output file {:?}: {}", out_path, e2))?
            }
            Err(e) => return Err(format!("Failed to create output file {:?}: {}", out_path, e)),
        };

        if !data.is_empty() {
            if let Err(e) = out_file.write_all(data) {
                drop(out_file);
                let _ = std::fs::remove_file(out_path);
                return Err(format!("Error writing extracted file {:?}: {}", out_path, e));
            }
        }
        Ok(())
    } else {
        let part_path = {
            let mut p = out_path.as_os_str().to_os_string();
            p.push(".baypart");
            PathBuf::from(p)
        };

        let mut out_file = create_output_file_fast(&part_path, data.len() as u64)
            .map_err(|e| format!("Failed to create temporary output file {:?}: {}", part_path, e))?;

        if !data.is_empty() {
            if let Err(e) = out_file.write_all(data) {
                drop(out_file);
                let _ = std::fs::remove_file(&part_path);
                return Err(format!("Error writing extracted file {:?}: {}", out_path, e));
            }
        }
        drop(out_file);

        if let Err(e) = safe_atomic_rename(&part_path, out_path) {
            let _ = std::fs::remove_file(&part_path);
            return Err(format!("Failed to finalize extracted file {:?}: {}", out_path, e));
        }
        Ok(())
    }
}

/// Caches created directory paths during an extraction run to avoid repeated `create_dir_all` syscalls.
#[derive(Debug, Default)]
pub struct DirectoryCache {
    created: HashSet<PathBuf>,
}

impl DirectoryCache {
    pub fn new() -> Self {
        Self {
            created: HashSet::new(),
        }
    }

    pub fn mark_existing(&mut self, dir: &Path) {
        if !dir.as_os_str().is_empty() {
            self.created.insert(dir.to_path_buf());
        }
    }

    pub fn ensure_dir(&mut self, dir: &Path) -> std::io::Result<()> {
        if dir.as_os_str().is_empty() || self.created.contains(dir) {
            return Ok(());
        }
        std::fs::create_dir_all(dir)?;
        let mut cur: Option<&Path> = Some(dir);
        while let Some(p) = cur {
            if p.as_os_str().is_empty() || !self.created.insert(p.to_path_buf()) {
                break;
            }
            cur = p.parent();
        }
        Ok(())
    }

    pub fn ensure_parent(&mut self, file_path: &Path) -> std::io::Result<()> {
        if let Some(parent) = file_path.parent() {
            self.ensure_dir(parent)?;
        }
        Ok(())
    }

    pub fn cached_len(&self) -> usize {
        self.created.len()
    }
}

struct PendingFileWrite {
    out_path: PathBuf,
    data: Vec<u8>,
    file_existed: bool,
}

struct PoolState {
    queue: VecDeque<PendingFileWrite>,
    in_flight_bytes: usize,
    pending_count: usize,
    closed: bool,
    aborted: bool,
    first_error: Option<String>,
}

struct PoolShared {
    state: Mutex<PoolState>,
    not_empty: Condvar,
    space_or_idle: Condvar,
    max_budget_bytes: usize,
    cancel_token: Option<TaskCancelToken>,
}

fn is_pool_cancelled(cancel_token: &Option<TaskCancelToken>) -> bool {
    match cancel_token {
        Some(token) => token.is_cancelled(),
        None => is_extraction_cancelled(),
    }
}

fn normalize_write_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn drain_queued_jobs(state: &mut PoolState) {
    while let Some(dropped) = state.queue.pop_front() {
        state.in_flight_bytes = state.in_flight_bytes.saturating_sub(dropped.data.len());
        state.pending_count = state.pending_count.saturating_sub(1);
    }
}

fn worker_loop(shared: Arc<PoolShared>) {
    let _priority_guard = ScopedPriorityGuard::new();
    loop {
        let job = {
            let mut state = match shared.state.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            loop {
                if state.aborted || is_pool_cancelled(&shared.cancel_token) {
                    state.aborted = true;
                    drain_queued_jobs(&mut state);
                    shared.space_or_idle.notify_all();
                    shared.not_empty.notify_all();
                    return;
                }
                if let Some(item) = state.queue.pop_front() {
                    break item;
                }
                if state.closed {
                    shared.space_or_idle.notify_all();
                    return;
                }
                let (guard, _) = shared
                    .not_empty
                    .wait_timeout(state, Duration::from_millis(50))
                    .unwrap_or_else(|e| e.into_inner());
                state = guard;
            }
        };

        let job_len = job.data.len();
        let write_res = write_file_payload(&job.out_path, &job.data, job.file_existed);
        drop(job);

        let mut state = match shared.state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        state.in_flight_bytes = state.in_flight_bytes.saturating_sub(job_len);
        state.pending_count = state.pending_count.saturating_sub(1);
        if let Err(err) = write_res {
            if state.first_error.is_none() {
                state.first_error = Some(err);
            }
            state.aborted = true;
            drain_queued_jobs(&mut state);
            shared.not_empty.notify_all();
        }
        shared.space_or_idle.notify_all();
    }
}

/// Bounded memory-budgeted producer-consumer file writer pool.
/// Decouples CPU decompression from NTFS file creation and write latency while
/// strictly honoring storage bus parallelism caps and OS Background Priority Shield.
pub struct AsyncFileWriterPool {
    shared: Arc<PoolShared>,
    workers: Vec<std::thread::JoinHandle<()>>,
    seen_paths: HashSet<String>,
}

impl AsyncFileWriterPool {
    pub fn new_for_dest(dest_dir: &Path) -> Self {
        let workers = get_optimal_extract_writer_threads(dest_dir);
        let budget = get_optimal_extract_inflight_budget(dest_dir);
        let token = current_extraction_token();
        Self::with_config(workers, budget, token)
    }

    pub fn with_config(
        worker_count: usize,
        max_budget_bytes: usize,
        cancel_token: Option<TaskCancelToken>,
    ) -> Self {
        let count = worker_count.max(1);
        let budget = max_budget_bytes.max(1024);
        let shared = Arc::new(PoolShared {
            state: Mutex::new(PoolState {
                queue: VecDeque::new(),
                in_flight_bytes: 0,
                pending_count: 0,
                closed: false,
                aborted: false,
                first_error: None,
            }),
            not_empty: Condvar::new(),
            space_or_idle: Condvar::new(),
            max_budget_bytes: budget,
            cancel_token,
        });

        let mut workers = Vec::with_capacity(count);
        for _ in 0..count {
            let shared_clone = Arc::clone(&shared);
            workers.push(std::thread::spawn(move || worker_loop(shared_clone)));
        }

        Self {
            shared,
            workers,
            seen_paths: HashSet::new(),
        }
    }

    pub fn submit(
        &mut self,
        out_path: PathBuf,
        data: Vec<u8>,
        file_existed: bool,
    ) -> Result<(), String> {
        let key = normalize_write_key(&out_path);
        let effective_existed = if self.seen_paths.contains(&key) {
            self.wait_idle()?;
            true
        } else {
            self.seen_paths.insert(key);
            file_existed
        };

        let job_len = data.len();
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "Writer pool lock poisoned".to_string())?;

        loop {
            if is_pool_cancelled(&self.shared.cancel_token) {
                state.aborted = true;
                drain_queued_jobs(&mut state);
                self.shared.not_empty.notify_all();
                self.shared.space_or_idle.notify_all();
                return Err("Extraction cancelled by user".to_string());
            }
            if let Some(ref err) = state.first_error {
                return Err(err.clone());
            }
            if state.aborted {
                return Err("Extraction cancelled by user".to_string());
            }
            if state.in_flight_bytes == 0
                || state.in_flight_bytes.saturating_add(job_len) <= self.shared.max_budget_bytes
            {
                break;
            }
            let (guard, _) = self
                .shared
                .space_or_idle
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap_or_else(|e| e.into_inner());
            state = guard;
        }

        state.in_flight_bytes = state.in_flight_bytes.saturating_add(job_len);
        state.pending_count = state.pending_count.saturating_add(1);
        state.queue.push_back(PendingFileWrite {
            out_path,
            data,
            file_existed: effective_existed,
        });
        self.shared.not_empty.notify_one();
        Ok(())
    }

    /// Synchronizes any prior async writes targeting the same case-insensitive path
    /// before a large file is streamed directly on the producer thread.
    pub fn prepare_direct_write(&mut self, out_path: &Path) -> Result<bool, String> {
        let key = normalize_write_key(out_path);
        if self.seen_paths.contains(&key) {
            self.wait_idle()?;
            Ok(true)
        } else {
            self.seen_paths.insert(key);
            Ok(false)
        }
    }

    pub fn wait_idle(&self) -> Result<(), String> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "Writer pool lock poisoned".to_string())?;
        while state.pending_count > 0 {
            if is_pool_cancelled(&self.shared.cancel_token) {
                state.aborted = true;
                drain_queued_jobs(&mut state);
                self.shared.not_empty.notify_all();
                self.shared.space_or_idle.notify_all();
                return Err("Extraction cancelled by user".to_string());
            }
            if let Some(ref err) = state.first_error {
                return Err(err.clone());
            }
            let (guard, _) = self
                .shared
                .space_or_idle
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap_or_else(|e| e.into_inner());
            state = guard;
        }
        if let Some(ref err) = state.first_error {
            return Err(err.clone());
        }
        if is_pool_cancelled(&self.shared.cancel_token) || state.aborted {
            return Err("Extraction cancelled by user".to_string());
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<(), String> {
        if let Ok(mut state) = self.shared.state.lock() {
            state.closed = true;
        }
        self.shared.not_empty.notify_all();

        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }

        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| "Writer pool lock poisoned".to_string())?;
        if let Some(ref err) = state.first_error {
            return Err(err.clone());
        }
        if is_pool_cancelled(&self.shared.cancel_token) || state.aborted {
            return Err("Extraction cancelled by user".to_string());
        }
        Ok(())
    }

    pub fn abort_and_join(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.aborted = true;
            state.closed = true;
            drain_queued_jobs(&mut state);
        }
        self.shared.not_empty.notify_all();
        self.shared.space_or_idle.notify_all();
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Drop for AsyncFileWriterPool {
    fn drop(&mut self) {
        if !self.workers.is_empty() {
            self.abort_and_join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_directory_cache_creates_nested_and_tracks_ancestors() {
        let base = std::env::current_dir().unwrap().join("target").join("test_dir_cache");
        let _ = std::fs::remove_dir_all(&base);

        let mut cache = DirectoryCache::new();
        let deep = base.join("a").join("b").join("c");
        cache.ensure_dir(&deep).unwrap();
        assert!(deep.is_dir());
        assert!(cache.cached_len() >= 4);

        // Subsequent ensure calls on ancestors or same dir succeed cleanly without modifying count
        let prev_len = cache.cached_len();
        cache.ensure_dir(&base.join("a").join("b")).unwrap();
        cache.ensure_parent(&deep.join("file.txt")).unwrap();
        assert_eq!(cache.cached_len(), prev_len);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_create_output_file_fast_preallocates_without_extending_eof() {
        let base = std::env::current_dir().unwrap().join("target").join("test_prealloc_fast");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let target = base.join("large_prealloc.bin");
        {
            let mut f = create_output_file_fast(&target, 256 * 1024).unwrap();
            // FileAllocationInfo reserves physical clusters without inflating logical EOF
            assert_eq!(f.metadata().unwrap().len(), 0);
            f.write_all(&vec![0x5Au8; 1024]).unwrap();
        }

        assert_eq!(std::fs::metadata(&target).unwrap().len(), 1024);
        let mut read_back = Vec::new();
        open_sequential_read(&target).unwrap().read_to_end(&mut read_back).unwrap();
        assert_eq!(read_back, vec![0x5Au8; 1024]);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_write_file_payload_direct_and_atomic_overwrite() {
        let base = std::env::current_dir().unwrap().join("target").join("test_write_payload");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let out = base.join("sample.dat");
        write_file_payload(&out, b"initial-content", false).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"initial-content");

        write_file_payload(&out, b"updated-content", true).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"updated-content");
        assert!(!base.join("sample.dat.baypart").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_async_writer_pool_writes_files_and_handles_case_duplicates() {
        let base = std::env::current_dir().unwrap().join("target").join("test_async_pool_dup");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let mut pool = AsyncFileWriterPool::with_config(3, 1024 * 1024, None);
        pool.submit(base.join("alpha.txt"), b"alpha".to_vec(), false).unwrap();
        pool.submit(base.join("CaseFile.txt"), b"first".to_vec(), false).unwrap();
        pool.submit(base.join("casefile.txt"), b"second".to_vec(), false).unwrap();

        // Large direct write waits on pending async writes when targeting an already-seen path
        assert!(pool.prepare_direct_write(&base.join("ALPHA.TXT")).unwrap());
        assert!(!pool.prepare_direct_write(&base.join("beta.bin")).unwrap());

        pool.finish().unwrap();

        assert_eq!(std::fs::read(base.join("alpha.txt")).unwrap(), b"alpha");
        assert_eq!(std::fs::read(base.join("casefile.txt")).unwrap(), b"second");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_async_writer_pool_respects_byte_budget_and_propagates_errors() {
        let base = std::env::current_dir().unwrap().join("target").join("test_async_pool_budget");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        // Tight budget (1024 bytes): second 700-byte file waits for first to drain without deadlock
        let mut pool = AsyncFileWriterPool::with_config(2, 1024, None);
        pool.submit(base.join("one.bin"), vec![1u8; 700], false).unwrap();
        pool.submit(base.join("two.bin"), vec![2u8; 700], false).unwrap();
        pool.finish().unwrap();
        assert_eq!(std::fs::read(base.join("one.bin")).unwrap(), vec![1u8; 700]);
        assert_eq!(std::fs::read(base.join("two.bin")).unwrap(), vec![2u8; 700]);

        // Error propagation: attempt to write under a regular file path
        let blocking_file = base.join("not_a_dir");
        std::fs::write(&blocking_file, b"file").unwrap();
        let mut err_pool = AsyncFileWriterPool::with_config(2, 64 * 1024, None);
        err_pool.submit(blocking_file.join("child.txt"), b"fail".to_vec(), false).unwrap();
        let res = err_pool.finish();
        assert!(res.is_err());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_async_writer_pool_abort_joins_before_cleanup() {
        let base = std::env::current_dir().unwrap().join("target").join("test_async_pool_abort");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let token = TaskCancelToken::new();
        let mut pool = AsyncFileWriterPool::with_config(2, 64 * 1024, Some(token.clone()));
        for i in 0..8 {
            let _ = pool.submit(base.join(format!("f_{}.bin", i)), vec![0xAAu8; 4096], false);
        }
        token.cancel();
        pool.abort_and_join();

        // Submitting after cancel must fail immediately with cancellation error
        let late = pool.submit(base.join("late.bin"), vec![1u8; 16], false);
        assert!(late.is_err());
        assert!(late.unwrap_err().contains("cancelled"));

        // Cleanup succeeds cleanly with zero open handles left behind
        std::fs::remove_dir_all(&base).unwrap();
    }
}

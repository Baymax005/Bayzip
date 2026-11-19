use std::cell::RefCell;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub mod utils;
pub mod io;
pub mod bay;
pub mod zip;
pub mod tar;
pub mod sevenz;
pub mod system_tar;

pub use utils::*;
pub use io::*;
pub use bay::*;
pub use zip::*;
pub use tar::*;
pub use sevenz::*;
pub use system_tar::*;

use crate::types::{ArchiveFormat, ExtractResponse, ExtractProgress, TaskCancelToken};
use crate::priority::ScopedPriorityGuard;
use crate::inspector::{
    detect_archive_type,
    archive_stem_name,
    inspect_archive,
    has_single_root_dir,
    get_single_root_dir,
};

thread_local! {
    static CURRENT_EXTRACTION_TOKEN: RefCell<Option<TaskCancelToken>> = const { RefCell::new(None) };
    static CURRENT_EXTRACTION_BYTES: RefCell<Option<Arc<AtomicU64>>> = const { RefCell::new(None) };
}

static ACTIVE_EXTRACTION_TOKEN: Mutex<Vec<TaskCancelToken>> = Mutex::new(Vec::new());

pub struct ExtractionTaskGuard {
    token: TaskCancelToken,
    bytes_extracted: Arc<AtomicU64>,
}

impl ExtractionTaskGuard {
    pub fn new() -> Self {
        let token = TaskCancelToken::new();
        let bytes_extracted = Arc::new(AtomicU64::new(0));
        CURRENT_EXTRACTION_TOKEN.with(|cell| {
            *cell.borrow_mut() = Some(token.clone());
        });
        CURRENT_EXTRACTION_BYTES.with(|cell| {
            *cell.borrow_mut() = Some(Arc::clone(&bytes_extracted));
        });
        if let Ok(mut lock) = ACTIVE_EXTRACTION_TOKEN.lock() {
            lock.push(token.clone());
        }
        Self {
            token,
            bytes_extracted,
        }
    }

    pub fn token(&self) -> TaskCancelToken {
        self.token.clone()
    }

    pub fn bytes_extracted(&self) -> u64 {
        self.bytes_extracted.load(Ordering::Relaxed)
    }
}

impl Default for ExtractionTaskGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ExtractionTaskGuard {
    fn drop(&mut self) {
        CURRENT_EXTRACTION_TOKEN.with(|cell| {
            *cell.borrow_mut() = None;
        });
        CURRENT_EXTRACTION_BYTES.with(|cell| {
            *cell.borrow_mut() = None;
        });
        if let Ok(mut lock) = ACTIVE_EXTRACTION_TOKEN.lock() {
            lock.retain(|t| !Arc::ptr_eq(&t.0, &self.token.0));
        }
    }
}

pub(crate) fn current_extraction_token() -> Option<TaskCancelToken> {
    CURRENT_EXTRACTION_TOKEN.with(|cell| cell.borrow().clone())
}

pub(crate) fn record_extracted_bytes(bytes: u64) {
    if bytes == 0 {
        return;
    }
    CURRENT_EXTRACTION_BYTES.with(|cell| {
        if let Some(counter) = cell.borrow().as_ref() {
            counter.fetch_add(bytes, Ordering::Relaxed);
        }
    });
}

pub fn cancel_extraction_flag() {
    if let Ok(lock) = ACTIVE_EXTRACTION_TOKEN.lock() {
        for token in lock.iter() {
            token.cancel();
        }
    }
}

pub fn is_extraction_cancelled() -> bool {
    let thread_cancelled = CURRENT_EXTRACTION_TOKEN.with(|cell| {
        cell.borrow().as_ref().map(|t| t.is_cancelled())
    });
    if let Some(cancelled) = thread_cancelled {
        return cancelled;
    }
    if let Ok(lock) = ACTIVE_EXTRACTION_TOKEN.lock() {
        return lock.iter().any(|t| t.is_cancelled());
    }
    false
}

pub fn reset_extraction_cancel_flag() {
    // Retained for backwards compatibility; ExtractionTaskGuard provides isolated tokens per task
}

pub fn universal_extract(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    let format = detect_archive_type(archive_path);
    match format {
        ArchiveFormat::Bay => {
            extract_bay_archive(archive_path, dest_dir, password, expected_size, app, excluded)
        },
        ArchiveFormat::Zip => {
            extract_zip_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                .or_else(|e| {
                    if should_abort_fallback(&e) {
                        return Err(e);
                    }
                    extract_bay_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                        .or_else(|e2| {
                            if should_abort_fallback(&e2) {
                                return Err(e2);
                            }
                            extract_with_system_tar(archive_path, dest_dir, app, excluded)
                        })
                })
        },
        ArchiveFormat::SevenZip => {
            extract_7z_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                .or_else(|e| {
                    if should_abort_fallback(&e) {
                        return Err(e);
                    }
                    extract_with_system_tar(archive_path, dest_dir, app, excluded)
                })
        },
        ArchiveFormat::Tar | ArchiveFormat::TarGz | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd | ArchiveFormat::Iso | ArchiveFormat::Cab => {
            extract_with_system_tar(archive_path, dest_dir, app, excluded)
                .or_else(|e| {
                    if should_abort_fallback(&e) {
                        return Err(e);
                    }
                    extract_tar_archive(archive_path, dest_dir, expected_size, app, excluded)
                })
        },
        ArchiveFormat::Rar => {
            extract_with_system_tar(archive_path, dest_dir, app, excluded)
        },
        ArchiveFormat::Unknown => {
            extract_zip_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                .or_else(|e| {
                    if should_abort_fallback(&e) {
                        return Err(e);
                    }
                    extract_7z_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                })
                .or_else(|e| {
                    if should_abort_fallback(&e) {
                        return Err(e);
                    }
                    extract_bay_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                })
                .or_else(|e| {
                    if should_abort_fallback(&e) {
                        return Err(e);
                    }
                    extract_with_system_tar(archive_path, dest_dir, app, excluded)
                })
                .or_else(|e| {
                    if should_abort_fallback(&e) {
                        return Err(e);
                    }
                    extract_tar_archive(archive_path, dest_dir, expected_size, app, excluded)
                })
        }
    }
}

pub fn extract_archive_sync(
    app: AppHandle,
    archive_path: String,
    destination: Option<String>,
    password: Option<String>,
    expected_size: Option<u64>,
    extract_to_subfolder: Option<bool>,
    excluded_paths: Option<Vec<String>>,
) -> Result<ExtractResponse, String> {
    let _task_guard = ExtractionTaskGuard::new();
    reset_extraction_cancel_flag();
    let _priority_guard = ScopedPriorityGuard::new();

    let path = Path::new(&archive_path);
    if !path.exists() {
        return Err("Archive file does not exist".into());
    }

    let start_time = Instant::now();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = archive_stem_name(path);

    let format = detect_archive_type(path);
    let is_tar_or_rar = matches!(format, ArchiveFormat::Rar | ArchiveFormat::Tar | ArchiveFormat::TarGz | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd | ArchiveFormat::Iso | ArchiveFormat::Cab)
        || path.extension().and_then(|s| s.to_str()).map(|ext| {
            let l = ext.to_lowercase();
            l == "rar" || l == "tar" || l == "gz" || l == "tgz" || l == "bz2" || l == "xz" || l == "zst" || l == "iso" || l == "cab"
        }).unwrap_or(false);

    let rar_single_root_match = if is_tar_or_rar && extract_to_subfolder.unwrap_or(true) {
        if let Ok(info) = inspect_archive(&archive_path, password.as_deref()) {
            if let Some(root_dir) = get_single_root_dir(&info.entries) {
                root_dir.eq_ignore_ascii_case(&stem)
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    let use_subfolder = extract_to_subfolder.unwrap_or(true);
    let dest_dir = if let Some(custom_dest) = destination.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(custom_dest);
        if !use_subfolder {
            p
        } else if rar_single_root_match {
            if p.file_name().and_then(|s| s.to_str()).map(|n| n.eq_ignore_ascii_case(&stem)).unwrap_or(false) {
                p.parent().map(|d| d.to_path_buf()).unwrap_or_else(|| parent.to_path_buf())
            } else {
                p
            }
        } else {
            // If the user or frontend selected a destination folder, we only resolve a unique name
            // if `p` actually ends with the archive stem subfolder!
            // We must NEVER rename an existing parent directory (like Downloads, Desktop, or Documents) into Downloads (1)!
            let is_stem_folder = p.file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.eq_ignore_ascii_case(&stem) || base_folder_stem(n).eq_ignore_ascii_case(&stem))
                .unwrap_or(false);

            if is_stem_folder {
                resolve_unique_folder_name(&p)
            } else {
                p
            }
        }
    } else if !use_subfolder || rar_single_root_match {
        parent.to_path_buf()
    } else {
        resolve_unique_folder_name(&parent.join(&stem))
    };

    let target_folder = if rar_single_root_match {
        dest_dir.join(&stem)
    } else {
        dest_dir.clone()
    };
    let target_folder_existed_before = target_folder.exists();
    let pwd_ref = password.as_deref();

    let excluded_set: Option<std::collections::HashSet<String>> =
        excluded_paths.and_then(crate::inspector::build_excluded_set);

    let res = universal_extract(path, &dest_dir, pwd_ref, expected_size, Some(&app), excluded_set.as_ref());

    let extracted_files = match res {
        Ok(files) => files,
        Err(e) => {
            // User requested cancellation or fatal error occurred:
            // Clean up the half-extracted folder if it was newly created for this extraction!
            if !target_folder_existed_before && target_folder.exists() {
                for _ in 0..5 {
                    if std::fs::remove_dir_all(&target_folder).is_ok() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(60));
                }
            }

            if is_extraction_cancelled() || e.to_lowercase().contains("cancel") {
                return Err("Extraction cancelled by user".to_string());
            } else {
                return Err(e);
            }
        }
    };

    let recorded_bytes = _task_guard.bytes_extracted();
    let total_final_bytes: u64 = if recorded_bytes > 0 || extracted_files.is_empty() {
        recorded_bytes
    } else {
        extracted_files.iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    };

    let _ = app.emit("extract_progress", ExtractProgress {
        percent: 100.0,
        current_file: "Completed".into(),
        bytes_extracted: total_final_bytes,
    });

    let mut final_output_dir = target_folder;
    if !extracted_files.is_empty() {
        let first_path = Path::new(&extracted_files[0]);
        if let Ok(first_rel) = first_path.strip_prefix(&dest_dir) {
            if let Some(first_comp) = first_rel.components().next() {
                let candidate_sub = dest_dir.join(first_comp.as_os_str());
                if candidate_sub.is_dir() {
                    let all_in_candidate = extracted_files.iter().all(|f| {
                        Path::new(f).starts_with(&candidate_sub)
                    });
                    if all_in_candidate {
                        final_output_dir = candidate_sub;
                    }
                }
            }
        }
    }

    Ok(ExtractResponse {
        success: true,
        output_directory: final_output_dir.to_string_lossy().to_string(),
        file_count: extracted_files.len(),
        elapsed_ms: start_time.elapsed().as_millis() as u64,
        error: None,
    })
}

pub fn headless_extract_here(archive_path: &str) -> Result<(), String> {
    let _task_guard = ExtractionTaskGuard::new();
    let _priority_guard = ScopedPriorityGuard::new();

    let p = Path::new(archive_path);
    if !p.exists() {
        return Err("Archive file does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    universal_extract(p, parent, None, None, None, None)?;

    #[cfg(target_os = "windows")]
    {
        let mut refresh_cmd = Command::new(crate::inspector::get_system32_exe("ie4uinit.exe"));
        refresh_cmd.arg("-show");
        refresh_cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = refresh_cmd.output();
    }
    Ok(())
}

pub fn headless_extract_to_subfolder(archive_path: &str) -> Result<(), String> {
    let _task_guard = ExtractionTaskGuard::new();
    let _priority_guard = ScopedPriorityGuard::new();

    let p = Path::new(archive_path);
    if !p.exists() {
        return Err("Archive file does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));

    // Check if the archive already contains a single root directory wrapping all files
    let has_single_root = if let Ok(info) = inspect_archive(archive_path, None) {
        has_single_root_dir(&info.entries)
    } else {
        false
    };

    let dest = if has_single_root {
        parent.to_path_buf()
    } else {
        let stem = archive_stem_name(p);
        let d = resolve_unique_folder_name(&parent.join(&stem));
        let _ = std::fs::create_dir_all(&d);
        d
    };

    if let Err(e) = universal_extract(p, &dest, None, None, None, None) {
        // If extraction failed and we created a subfolder, clean it up completely
        if !has_single_root {
            let _ = std::fs::remove_dir_all(&dest);
        }
        return Err(e);
    }

    #[cfg(target_os = "windows")]
    {
        let mut refresh_cmd = Command::new(crate::inspector::get_system32_exe("ie4uinit.exe"));
        refresh_cmd.arg("-show");
        refresh_cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = refresh_cmd.output();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction_task_guard_isolation() {
        {
            let guard1 = ExtractionTaskGuard::new();
            assert!(!is_extraction_cancelled());
            cancel_extraction_flag();
            assert!(is_extraction_cancelled());
            assert!(guard1.token().is_cancelled());
        }
        {
            let guard2 = ExtractionTaskGuard::new();
            assert!(!is_extraction_cancelled());
            assert!(!guard2.token().is_cancelled());
        }
    }

    #[test]
    fn test_extraction_task_guard_records_bytes_isolated() {
        {
            let guard1 = ExtractionTaskGuard::new();
            assert_eq!(guard1.bytes_extracted(), 0);
            assert!(current_extraction_token().is_some());
            record_extracted_bytes(1024);
            record_extracted_bytes(2048);
            assert_eq!(guard1.bytes_extracted(), 3072);
        }
        {
            let guard2 = ExtractionTaskGuard::new();
            assert_eq!(guard2.bytes_extracted(), 0);
            record_extracted_bytes(512);
            assert_eq!(guard2.bytes_extracted(), 512);
        }
    }
}

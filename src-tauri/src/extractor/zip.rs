use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tauri::AppHandle;
#[cfg(not(test))]
use tauri::Emitter;

#[cfg(not(test))]
use crate::types::ExtractProgress;
use crate::types::TaskCancelToken;
use crate::priority::ScopedPriorityGuard;
use crate::inspector::{
    clear_readonly,
    get_drive_bus_kind,
    get_drive_seek_penalty,
    get_optimal_extract_chunk_size,
    safe_atomic_rename,
    sanitize_archive_path,
    DriveBusKind,
};
use super::{current_extraction_token, is_extraction_cancelled};
use super::io::{
    create_output_file_fast,
    open_random_read,
    open_sequential_read,
    write_file_payload,
    DirectoryCache,
};
use super::utils::{
    base_folder_stem,
    cleanup_extracted_files,
    is_path_excluded,
    strip_root_component,
};

const ZIP_READ_BUF_SIZE: usize = 256 * 1024; // 1x Windows Cache Manager 256 KB VACB

struct ZipWorkItem {
    index: usize,
    safe_path: PathBuf,
    outpath: PathBuf,
    file_existed: bool,
}

enum EntryOutcome {
    Extracted(String),
    Skipped,
}

/// Computes optimal worker count for parallel ZIP extraction based on storage bus & seek penalty.
/// Uses independent file handles on SSD/NVMe (`2-8` workers) and stays sequential (`1` worker)
/// on mechanical HDDs, removable USB drives, or tiny archives (`< 4` files).
pub fn calculate_zip_worker_threads(
    bus: DriveBusKind,
    incurs_seek_penalty: bool,
    total_cores: usize,
    item_count: usize,
) -> usize {
    if item_count < 4 || total_cores <= 1 {
        return 1;
    }
    match bus {
        DriveBusKind::Nvme => total_cores.clamp(1, 8).min(item_count),
        DriveBusKind::Sata if !incurs_seek_penalty => total_cores.clamp(1, 6).min(item_count),
        DriveBusKind::Sata | DriveBusKind::UsbRemovable | DriveBusKind::Unknown => 1,
    }
}

fn is_external_cancel(cancel_token: Option<&TaskCancelToken>) -> bool {
    match cancel_token {
        Some(token) => token.is_cancelled(),
        None => is_extraction_cancelled(),
    }
}

fn is_zip_aborted(aborted: &AtomicBool, cancel_token: Option<&TaskCancelToken>) -> bool {
    aborted.load(Ordering::Acquire) || is_external_cancel(cancel_token)
}

fn ensure_dir_tracked(
    dir: &Path,
    dest_dir: &Path,
    dir_cache: &mut DirectoryCache,
    dest_was_empty: bool,
    newly_created: &mut Vec<String>,
) -> Result<(), String> {
    if dir == dest_dir || dir.as_os_str().is_empty() || dir_cache.contains(dir) {
        return Ok(());
    }

    let mut missing = Vec::new();
    let mut cur = dir;
    while cur != dest_dir && cur.starts_with(dest_dir) {
        if dir_cache.contains(cur) {
            break;
        }
        let already_exists = if dest_was_empty {
            false
        } else {
            std::fs::symlink_metadata(cur).is_ok()
        };
        if already_exists {
            break;
        }
        missing.push(cur.to_path_buf());
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }

    dir_cache
        .ensure_dir(dir)
        .map_err(|e| format!("Failed to create directory {:?}: {}", dir, e))?;

    for created_dir in missing.into_iter().rev() {
        newly_created.push(created_dir.to_string_lossy().to_string());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_emit_zip_progress(
    app: Option<&AppHandle>,
    last_emit: &Mutex<Instant>,
    current_file: &Path,
    total_extracted: &AtomicU64,
    total_expected_bytes: u64,
    processed_count: &AtomicUsize,
    total_items: usize,
    force: bool,
) {
    #[cfg(not(test))]
    if let Some(app_handle) = app {
        let should_emit = if force {
            if let Ok(mut guard) = last_emit.lock() {
                *guard = Instant::now();
                true
            } else {
                false
            }
        } else if let Ok(mut guard) = last_emit.try_lock() {
            if guard.elapsed().as_millis() >= 50 {
                *guard = Instant::now();
                true
            } else {
                false
            }
        } else {
            false
        };

        if should_emit {
            let bytes = total_extracted.load(Ordering::Relaxed);
            let done = processed_count.load(Ordering::Relaxed);
            let pct = if total_expected_bytes > 0 {
                ((bytes as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
            } else {
                ((done as f64 / total_items.max(1) as f64) * 100.0).min(99.0)
            };
            let _ = app_handle.emit(
                "extract_progress",
                ExtractProgress {
                    percent: pct,
                    current_file: current_file.to_string_lossy().to_string(),
                    bytes_extracted: bytes,
                },
            );
        }
    }
    #[cfg(test)]
    let _ = (
        app,
        last_emit,
        current_file,
        total_extracted,
        total_expected_bytes,
        processed_count,
        total_items,
        force,
    );
}

#[allow(clippy::too_many_arguments)]
fn extract_single_zip_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    item: &ZipWorkItem,
    pwd_opt: Option<&str>,
    io_buf: &mut [u8],
    chunk_size: usize,
    aborted: &AtomicBool,
    cancel_token: Option<&TaskCancelToken>,
    total_extracted: &AtomicU64,
    processed_count: &AtomicUsize,
    total_items: usize,
    total_expected_bytes: u64,
    last_emit: &Mutex<Instant>,
    app: Option<&AppHandle>,
    newly_created: &mut Vec<String>,
) -> Result<EntryOutcome, Option<String>> {
    if is_zip_aborted(aborted, cancel_token) {
        return Err(is_external_cancel(cancel_token).then(|| "Extraction cancelled by user".to_string()));
    }

    let mut file = match pwd_opt {
        Some(pwd) => match archive.by_index_decrypt(item.index, pwd.as_bytes()) {
            Ok(f) => f,
            Err(zip::result::ZipError::InvalidPassword) => {
                return Err(Some("Incorrect password: Could not decrypt file".to_string()));
            }
            Err(e) => return Err(Some(format!("Zip entry error: {}", e))),
        },
        None => match archive.by_index(item.index) {
            Ok(f) => f,
            Err(zip::result::ZipError::UnsupportedArchive(
                zip::result::ZipError::PASSWORD_REQUIRED,
            ))
            | Err(zip::result::ZipError::InvalidPassword) => {
                return Err(Some(
                    "Password required: This ZIP archive is encrypted with AES/password."
                        .to_string(),
                ));
            }
            Err(e) => return Err(Some(format!("Zip entry error: {}", e))),
        },
    };

    if file.is_symlink() {
        return Ok(EntryOutcome::Skipped);
    }

    if file.is_dir() {
        let dir_existed = std::fs::symlink_metadata(&item.outpath).is_ok();
        let _ = std::fs::create_dir_all(&item.outpath);
        if !dir_existed && item.outpath.exists() {
            newly_created.push(item.outpath.to_string_lossy().to_string());
        }
        return Ok(EntryOutcome::Skipped);
    }

    let entry_is_enc = pwd_opt.is_some() && file.encrypted();
    let file_size = file.size();
    let out_str = item.outpath.to_string_lossy().to_string();

    if file_size <= io_buf.len() as u64 {
        let sz = file_size as usize;
        if sz > 0 {
            file.read_exact(&mut io_buf[..sz]).map_err(|e| {
                Some(if entry_is_enc {
                    format!("Incorrect password or corrupted encrypted ZIP entry: {}", e)
                } else {
                    format!("Zip read error: {}", e)
                })
            })?;
        }
        let mut eof_check = [0u8; 1];
        let extra = file.read(&mut eof_check).map_err(|e| {
            Some(if entry_is_enc {
                format!("Incorrect password or corrupted encrypted ZIP entry: {}", e)
            } else {
                format!("Zip CRC/read error: {}", e)
            })
        })?;
        if extra != 0 {
            return Err(Some(format!(
                "Zip entry '{}' exceeded declared size",
                item.safe_path.display()
            )));
        }

        if !item.file_existed {
            newly_created.push(out_str.clone());
        }
        write_file_payload(&item.outpath, &io_buf[..sz], item.file_existed).map_err(Some)?;
        if sz > 0 {
            total_extracted.fetch_add(file_size, Ordering::Relaxed);
        }
    } else {
        if !item.file_existed {
            newly_created.push(out_str.clone());
        }

        let target_write_path: PathBuf = if item.file_existed {
            let mut p = item.outpath.as_os_str().to_os_string();
            p.push(".baypart");
            PathBuf::from(p)
        } else {
            item.outpath.clone()
        };

        let out_file = match create_output_file_fast(&target_write_path, file_size) {
            Ok(f) => f,
            Err(e) if !item.file_existed && e.kind() == std::io::ErrorKind::PermissionDenied => {
                clear_readonly(&target_write_path);
                create_output_file_fast(&target_write_path, file_size)
                    .map_err(|e2| Some(format!("Create file error: {}", e2)))?
            }
            Err(e) => {
                return Err(Some(if item.file_existed {
                    format!("Create temporary file error: {}", e)
                } else {
                    format!("Create file error: {}", e)
                }));
            }
        };

        let buf_cap = if file_size <= 1024 * 1024 {
            256 * 1024
        } else {
            chunk_size
        };
        let mut writer = BufWriter::with_capacity(buf_cap, out_file);
        let mut remaining = file_size;

        while remaining > 0 {
            if is_zip_aborted(aborted, cancel_token) {
                drop(writer);
                let _ = std::fs::remove_file(&target_write_path);
                return Err(
                    is_external_cancel(cancel_token).then(|| "Extraction cancelled by user".to_string())
                );
            }
            let to_read = (remaining as usize).min(io_buf.len());
            match file.read(&mut io_buf[..to_read]) {
                Ok(0) => {
                    drop(writer);
                    let _ = std::fs::remove_file(&target_write_path);
                    return Err(Some(format!(
                        "Premature EOF inside zip entry '{}'",
                        item.safe_path.display()
                    )));
                }
                Ok(n) => {
                    if let Err(e) = writer.write_all(&io_buf[..n]) {
                        drop(writer);
                        let _ = std::fs::remove_file(&target_write_path);
                        return Err(Some(format!("Zip write error: {}", e)));
                    }
                    remaining -= n as u64;
                    total_extracted.fetch_add(n as u64, Ordering::Relaxed);

                    maybe_emit_zip_progress(
                        app,
                        last_emit,
                        &item.safe_path,
                        total_extracted,
                        total_expected_bytes,
                        processed_count,
                        total_items,
                        false,
                    );
                }
                Err(e) => {
                    drop(writer);
                    let _ = std::fs::remove_file(&target_write_path);
                    return Err(Some(if entry_is_enc {
                        format!("Incorrect password or corrupted encrypted ZIP entry: {}", e)
                    } else {
                        format!("Zip read error: {}", e)
                    }));
                }
            }
        }

        let mut eof_check = [0u8; 1];
        match file.read(&mut eof_check) {
            Ok(0) => {}
            Ok(_) => {
                drop(writer);
                let _ = std::fs::remove_file(&target_write_path);
                return Err(Some(format!(
                    "Zip entry '{}' exceeded declared size",
                    item.safe_path.display()
                )));
            }
            Err(e) => {
                drop(writer);
                let _ = std::fs::remove_file(&target_write_path);
                return Err(Some(if entry_is_enc {
                    format!("Incorrect password or corrupted encrypted ZIP entry: {}", e)
                } else {
                    format!("Zip CRC/read error: {}", e)
                }));
            }
        }

        if let Err(e) = writer.flush() {
            drop(writer);
            let _ = std::fs::remove_file(&target_write_path);
            return Err(Some(format!("Zip flush error: {}", e)));
        }
        drop(writer);

        if item.file_existed {
            if let Err(e) = safe_atomic_rename(&target_write_path, &item.outpath) {
                let _ = std::fs::remove_file(&target_write_path);
                return Err(Some(format!("Failed to finalize extracted file: {}", e)));
            }
        }
    }

    let done = processed_count.fetch_add(1, Ordering::Relaxed) + 1;
    maybe_emit_zip_progress(
        app,
        last_emit,
        &item.safe_path,
        total_extracted,
        total_expected_bytes,
        processed_count,
        total_items,
        done == total_items,
    );

    Ok(EntryOutcome::Extracted(out_str))
}

fn compute_zip_root_strip<R: Read + Seek>(
    archive: &zip::ZipArchive<R>,
    dest_dir: &Path,
) -> Option<String> {
    let dest_stem = dest_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(base_folder_stem)?;
    let mut candidate_root: Option<String> = None;
    let mut is_single = true;

    for name in archive.file_names() {
        let clean = name
            .trim()
            .trim_start_matches(['/', '\\'])
            .trim_start_matches("./");
        if clean.is_empty() || clean == "." {
            continue;
        }
        if !clean.contains('/') && !clean.contains('\\') && !name.ends_with('/') {
            is_single = false;
            break;
        }
        let first = clean.split(['/', '\\']).next().unwrap_or("");
        if first.is_empty() {
            continue;
        }
        match &candidate_root {
            None => candidate_root = Some(first.to_string()),
            Some(root) => {
                if !root.eq_ignore_ascii_case(first) {
                    is_single = false;
                    break;
                }
            }
        }
    }

    if is_single
        && candidate_root
            .as_deref()
            .map(|r| r.eq_ignore_ascii_case(dest_stem))
            .unwrap_or(false)
    {
        Some(dest_stem.to_string())
    } else {
        None
    }
}

pub fn extract_zip_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    extract_zip_archive_with_workers(
        archive_path,
        dest_dir,
        password,
        expected_size,
        app,
        excluded,
        None,
    )
}

pub(crate) fn extract_zip_archive_with_workers(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
    worker_override: Option<usize>,
) -> Result<Vec<String>, String> {
    let dest_was_empty = !dest_dir.exists()
        || std::fs::read_dir(dest_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);

    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    let mut dir_cache = DirectoryCache::new();
    dir_cache.mark_existing(dest_dir);

    let raw_file = open_sequential_read(archive_path)
        .map_err(|e| format!("Failed to open zip: {}", e))?;
    let reader = BufReader::with_capacity(ZIP_READ_BUF_SIZE, raw_file);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("Invalid zip archive: {}", e))?;

    let root_strip = compute_zip_root_strip(&archive, dest_dir);
    let total_entries = archive.len();
    let total_expected_bytes: u64 = if let Some(sz) = expected_size.filter(|&s| s > 0) {
        sz
    } else {
        std::fs::metadata(archive_path)
            .map(|m| m.len() * 2)
            .unwrap_or(1)
    };
    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let cancel_token = current_extraction_token();

    // Pass 1: Zero-seek Central Directory scan, path sanitization, deduplication, and dir creation
    let mut newly_created = Vec::new();
    let mut work_items: Vec<ZipWorkItem> = Vec::with_capacity(total_entries);
    let mut seen_file_indices: HashMap<String, usize> = HashMap::with_capacity(total_entries);

    let pass1_res = (|| -> Result<(), String> {
        for i in 0..total_entries {
            if is_external_cancel(cancel_token.as_ref()) {
                return Err("Extraction cancelled by user".to_string());
            }

            let raw_name = match archive.name_for_index(i) {
                Some(n) => n,
                None => continue,
            };
            if is_path_excluded(raw_name, excluded) {
                continue;
            }
            let safe_path = match sanitize_archive_path(raw_name) {
                Some(p) => p,
                None => continue,
            };

            let effective_path = match root_strip {
                Some(ref prefix) => match strip_root_component(&safe_path, prefix) {
                    Some(stripped) => {
                        if stripped.as_os_str().is_empty() {
                            continue;
                        }
                        stripped.to_path_buf()
                    }
                    None => safe_path.clone(),
                },
                None => safe_path.clone(),
            };

            let outpath = dest_dir.join(&effective_path);
            let clean = raw_name.replace('\\', "/");

            if clean.ends_with('/') {
                ensure_dir_tracked(
                    &outpath,
                    dest_dir,
                    &mut dir_cache,
                    dest_was_empty,
                    &mut newly_created,
                )?;
            } else {
                if let Some(parent) = outpath.parent() {
                    ensure_dir_tracked(
                        parent,
                        dest_dir,
                        &mut dir_cache,
                        dest_was_empty,
                        &mut newly_created,
                    )?;
                }

                let dedup_key = outpath
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_end_matches('/')
                    .to_ascii_lowercase();
                let file_existed = if dest_was_empty {
                    false
                } else {
                    std::fs::symlink_metadata(&outpath).is_ok()
                };

                let item = ZipWorkItem {
                    index: i,
                    safe_path,
                    outpath,
                    file_existed,
                };

                if let Some(&slot) = seen_file_indices.get(&dedup_key) {
                    work_items[slot] = item;
                } else {
                    let slot = work_items.len();
                    seen_file_indices.insert(dedup_key, slot);
                    work_items.push(item);
                }
            }
        }
        Ok(())
    })();

    if let Err(err) = pass1_res {
        cleanup_extracted_files(&newly_created);
        return Err(err);
    }

    if work_items.is_empty() {
        super::record_extracted_bytes(0);
        return Ok(Vec::new());
    }

    let total_items = work_items.len();
    let worker_count = match worker_override {
        Some(n) => n.max(1).min(total_items),
        None => {
            let total_cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            calculate_zip_worker_threads(
                get_drive_bus_kind(dest_dir),
                get_drive_seek_penalty(dest_dir),
                total_cores,
                total_items,
            )
        }
    };

    let chunk_size = get_optimal_extract_chunk_size(dest_dir);
    let total_extracted = AtomicU64::new(0);
    let processed_count = AtomicUsize::new(0);
    let aborted = AtomicBool::new(false);
    let last_emit = Mutex::new(Instant::now());
    let mut extracted = Vec::with_capacity(total_items);

    if worker_count <= 1 {
        let mut io_buf = vec![0u8; ZIP_READ_BUF_SIZE];
        let mut err_opt: Option<String> = None;

        for item in &work_items {
            match extract_single_zip_entry(
                &mut archive,
                item,
                pwd_opt,
                &mut io_buf,
                chunk_size,
                &aborted,
                cancel_token.as_ref(),
                &total_extracted,
                &processed_count,
                total_items,
                total_expected_bytes,
                &last_emit,
                app,
                &mut newly_created,
            ) {
                Ok(EntryOutcome::Extracted(path_str)) => extracted.push(path_str),
                Ok(EntryOutcome::Skipped) => {}
                Err(Some(err)) => {
                    err_opt = Some(err);
                    break;
                }
                Err(None) => {
                    err_opt = Some("Extraction cancelled by user".to_string());
                    break;
                }
            }
        }

        if let Some(err) = err_opt {
            cleanup_extracted_files(&newly_created);
            return Err(err);
        }
    } else {
        // Release the Pass 1 sequential handle before opening per-worker random-access handles
        drop(archive);

        let next_idx = AtomicUsize::new(0);
        let first_error: Mutex<Option<String>> = Mutex::new(None);
        let work_ref = &work_items;
        let next_ref = &next_idx;
        let aborted_ref = &aborted;
        let first_error_ref = &first_error;
        let total_extracted_ref = &total_extracted;
        let processed_ref = &processed_count;
        let last_emit_ref = &last_emit;
        let cancel_ref = cancel_token.as_ref();

        let record_err = |err: String| {
            if let Ok(mut lock) = first_error_ref.lock() {
                if lock.is_none() {
                    *lock = Some(err);
                }
            }
            aborted_ref.store(true, Ordering::Release);
        };

        let worker_results: Vec<(Vec<String>, Vec<String>)> = std::thread::scope(|s| {
            let mut handles = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                handles.push(s.spawn(|| {
                    let _priority_guard = ScopedPriorityGuard::new();
                    let mut local_extracted = Vec::new();
                    let mut local_newly_created = Vec::new();

                    let raw_worker_file = match open_random_read(archive_path) {
                        Ok(f) => f,
                        Err(e) => {
                            record_err(format!("Failed to open zip: {}", e));
                            return (local_extracted, local_newly_created);
                        }
                    };
                    let worker_reader = BufReader::with_capacity(ZIP_READ_BUF_SIZE, raw_worker_file);
                    let mut worker_archive = match zip::ZipArchive::new(worker_reader) {
                        Ok(a) => a,
                        Err(e) => {
                            record_err(format!("Invalid zip archive: {}", e));
                            return (local_extracted, local_newly_created);
                        }
                    };

                    let mut io_buf = vec![0u8; ZIP_READ_BUF_SIZE];

                    loop {
                        if is_zip_aborted(aborted_ref, cancel_ref) {
                            if is_external_cancel(cancel_ref) {
                                record_err("Extraction cancelled by user".to_string());
                            }
                            break;
                        }
                        let pos = next_ref.fetch_add(1, Ordering::Relaxed);
                        if pos >= work_ref.len() {
                            break;
                        }

                        match extract_single_zip_entry(
                            &mut worker_archive,
                            &work_ref[pos],
                            pwd_opt,
                            &mut io_buf,
                            chunk_size,
                            aborted_ref,
                            cancel_ref,
                            total_extracted_ref,
                            processed_ref,
                            total_items,
                            total_expected_bytes,
                            last_emit_ref,
                            app,
                            &mut local_newly_created,
                        ) {
                            Ok(EntryOutcome::Extracted(path_str)) => local_extracted.push(path_str),
                            Ok(EntryOutcome::Skipped) => {}
                            Err(Some(err)) => {
                                record_err(err);
                                break;
                            }
                            Err(None) => {
                                break;
                            }
                        }
                    }

                    (local_extracted, local_newly_created)
                }));
            }

            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| (Vec::new(), Vec::new())))
                .collect()
        });

        for (mut local_ext, mut local_new) in worker_results {
            extracted.append(&mut local_ext);
            newly_created.append(&mut local_new);
        }

        if let Some(err) = first_error.into_inner().unwrap_or(None) {
            cleanup_extracted_files(&newly_created);
            return Err(err);
        }
        if is_external_cancel(cancel_token.as_ref()) {
            cleanup_extracted_files(&newly_created);
            return Err("Extraction cancelled by user".to_string());
        }
    }

    let final_bytes = total_extracted.load(Ordering::Relaxed);
    super::record_extracted_bytes(final_bytes);
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use crate::extractor::ExtractionTaskGuard;
    use zip::write::SimpleFileOptions;
    use zip::{AesMode, CompressionMethod, ZipWriter};

    #[test]
    fn test_calculate_zip_worker_threads_by_storage_profile() {
        assert_eq!(calculate_zip_worker_threads(DriveBusKind::Nvme, false, 16, 3), 1);
        assert_eq!(calculate_zip_worker_threads(DriveBusKind::Nvme, false, 1, 20), 1);
        assert_eq!(calculate_zip_worker_threads(DriveBusKind::Nvme, false, 16, 5), 5);
        assert_eq!(calculate_zip_worker_threads(DriveBusKind::Nvme, false, 16, 100), 8);

        assert_eq!(calculate_zip_worker_threads(DriveBusKind::Sata, false, 16, 100), 6);
        assert_eq!(calculate_zip_worker_threads(DriveBusKind::Sata, true, 16, 100), 1);
        assert_eq!(calculate_zip_worker_threads(DriveBusKind::UsbRemovable, true, 16, 100), 1);
        assert_eq!(calculate_zip_worker_threads(DriveBusKind::Unknown, false, 16, 100), 1);
    }

    #[test]
    fn test_parallel_zip_extraction_multifile_nested_and_exclusions() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_parallel_zip_multi");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let zip_path = base.join("bundle.zip");
        let dest_dir = base.join("out");

        let large_payload: Vec<u8> = (0..(320 * 1024)).map(|i| (i % 251) as u8).collect();
        let stored_payload = b"stored-uncompressed-binary-payload-1234567890".to_vec();

        {
            let f = File::create(&zip_path).unwrap();
            let mut zw = ZipWriter::new(f);
            let opts_deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let opts_stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

            zw.add_directory("assets/", opts_deflate).unwrap();
            zw.add_directory("assets/sub/", opts_deflate).unwrap();
            zw.add_directory("skip_dir/", opts_deflate).unwrap();

            for i in 0..8 {
                zw.start_file(format!("assets/file_{}.txt", i), opts_deflate).unwrap();
                zw.write_all(format!("payload-{}", i).as_bytes()).unwrap();
            }

            zw.start_file("assets/sub/large.bin", opts_deflate).unwrap();
            zw.write_all(&large_payload).unwrap();

            zw.start_file("assets/sub/raw.dat", opts_stored).unwrap();
            zw.write_all(&stored_payload).unwrap();

            zw.start_file("assets/sub/empty.txt", opts_deflate).unwrap();
            zw.start_file("skip_dir/secret.txt", opts_deflate).unwrap();
            zw.write_all(b"should-be-excluded").unwrap();
            zw.finish().unwrap();
        }

        let excluded = crate::inspector::build_excluded_set(["skip_dir"]);
        let guard = ExtractionTaskGuard::new();
        let files = extract_zip_archive_with_workers(
            &zip_path,
            &dest_dir,
            None,
            None,
            None,
            excluded.as_ref(),
            Some(4),
        )
        .unwrap();

        assert_eq!(files.len(), 11);
        for i in 0..8 {
            assert_eq!(
                std::fs::read_to_string(dest_dir.join(format!("assets/file_{}.txt", i))).unwrap(),
                format!("payload-{}", i)
            );
        }
        assert_eq!(std::fs::read(dest_dir.join("assets/sub/large.bin")).unwrap(), large_payload);
        assert_eq!(std::fs::read(dest_dir.join("assets/sub/raw.dat")).unwrap(), stored_payload);
        assert_eq!(std::fs::read(dest_dir.join("assets/sub/empty.txt")).unwrap(), b"");
        assert!(!dest_dir.join("skip_dir/secret.txt").exists());
        assert!(guard.bytes_extracted() > large_payload.len() as u64);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_parallel_zip_handles_case_duplicate_entries_last_wins() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_parallel_zip_dedup");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let zip_path = base.join("dup.zip");
        let dest_dir = base.join("out");

        {
            let f = File::create(&zip_path).unwrap();
            let mut zw = ZipWriter::new(f);
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

            zw.start_file("Note.txt", opts).unwrap();
            zw.write_all(b"first-version").unwrap();
            for i in 0..5 {
                zw.start_file(format!("extra_{}.txt", i), opts).unwrap();
                zw.write_all(b"filler").unwrap();
            }
            zw.start_file("note.txt", opts).unwrap();
            zw.write_all(b"final-version").unwrap();
            zw.finish().unwrap();
        }

        let files = extract_zip_archive_with_workers(&zip_path, &dest_dir, None, None, None, None, Some(4))
            .unwrap();
        assert_eq!(files.len(), 6);
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("note.txt")).unwrap(),
            "final-version"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_parallel_zip_atomic_overwrite_and_rejects_zip_slip() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_parallel_zip_overwrite_sec");
        let _ = std::fs::remove_dir_all(&base);
        let dest_dir = base.join("out");
        std::fs::create_dir_all(&dest_dir).unwrap();

        // Pre-existing file to verify atomic .baypart overwrite path
        std::fs::write(dest_dir.join("existing.txt"), b"old-data").unwrap();
        let zip_path = base.join("sec.zip");

        {
            let f = File::create(&zip_path).unwrap();
            let mut zw = ZipWriter::new(f);
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

            zw.start_file("../escape.txt", opts).unwrap();
            zw.write_all(b"malicious").unwrap();
            zw.start_file("existing.txt", opts).unwrap();
            zw.write_all(b"new-safe-data").unwrap();
            for i in 0..4 {
                zw.start_file(format!("safe_{}.txt", i), opts).unwrap();
                zw.write_all(b"ok").unwrap();
            }
            zw.finish().unwrap();
        }

        extract_zip_archive_with_workers(&zip_path, &dest_dir, None, None, None, None, Some(4)).unwrap();

        assert!(!base.join("escape.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("existing.txt")).unwrap(),
            "new-safe-data"
        );
        assert!(!dest_dir.join("existing.txt.baypart").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_parallel_zip_encrypted_password_error_and_rollback() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_parallel_zip_enc");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let zip_path = base.join("encrypted.zip");
        let dest_dir = base.join("out");
        let password = "correct_zip_password";

        {
            let f = File::create(&zip_path).unwrap();
            let mut zw = ZipWriter::new(f);
            let plain_opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let enc_opts = plain_opts.with_aes_encryption(AesMode::Aes256, password);

            zw.start_file("public.txt", plain_opts).unwrap();
            zw.write_all(b"unencrypted-header-file").unwrap();
            for i in 0..6 {
                zw.start_file(format!("secret_{}.txt", i), enc_opts).unwrap();
                zw.write_all(format!("secret-body-{}", i).as_bytes()).unwrap();
            }
            zw.finish().unwrap();
        }

        // 1. Missing password fails and rolls back public.txt
        let res_no_pwd =
            extract_zip_archive_with_workers(&zip_path, &dest_dir, None, None, None, None, Some(4));
        assert!(res_no_pwd.is_err());
        assert!(res_no_pwd.unwrap_err().contains("Password required"));
        assert!(!dest_dir.join("public.txt").exists());

        // 2. Wrong password fails and leaves zero partial files
        let res_wrong = extract_zip_archive_with_workers(
            &zip_path,
            &dest_dir,
            Some("wrong_pass"),
            None,
            None,
            None,
            Some(4),
        );
        assert!(res_wrong.is_err());
        assert!(res_wrong.unwrap_err().contains("Incorrect password"));
        assert!(!dest_dir.join("public.txt").exists());
        assert!(!dest_dir.join("secret_0.txt").exists());

        // 3. Correct password extracts all files cleanly in parallel
        let extracted = extract_zip_archive_with_workers(
            &zip_path,
            &dest_dir,
            Some(password),
            None,
            None,
            None,
            Some(4),
        )
        .unwrap();
        assert_eq!(extracted.len(), 7);
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("public.txt")).unwrap(),
            "unencrypted-header-file"
        );
        for i in 0..6 {
            assert_eq!(
                std::fs::read_to_string(dest_dir.join(format!("secret_{}.txt", i))).unwrap(),
                format!("secret-body-{}", i)
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}

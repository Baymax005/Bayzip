use std::fs::File;
use std::io::{Read, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

use crate::types::ExtractProgress;
use crate::inspector::{
    sanitize_archive_path,
    get_single_root_dir,
    safe_atomic_rename,
    clear_readonly,
};
use super::is_extraction_cancelled;
use super::utils::{
    base_folder_stem,
    strip_root_component,
    cleanup_extracted_files,
    is_path_excluded,
};

pub(crate) fn extract_tar_entries<R: Read>(
    mut archive: tar::Archive<R>,
    dest_dir: &Path,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
    root_strip: Option<&str>,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create dest: {}", e))?;

    let total_expected_bytes = expected_size.unwrap_or(1).max(1);
    let mut extracted_files = Vec::new();
    let mut newly_created = Vec::new();
    let mut total_extracted: u64 = 0;
    let mut last_emit = Instant::now();
    let mut io_buf = vec![0u8; 256 * 1024];

    let res = (|| -> Result<(), String> {
        let entries = archive.entries().map_err(|e| format!("Tar read entries error: {}", e))?;
        for entry_res in entries {
        if is_extraction_cancelled() {
            return Err("Extraction cancelled by user".to_string());
        }

        let mut entry = entry_res.map_err(|e| format!("Tar entry error: {}", e))?;
        let entry_path = entry.path().map_err(|e| format!("Tar path error: {}", e))?;
        let path_str = entry_path.to_string_lossy().to_string();

        if is_path_excluded(&path_str, excluded) {
            continue;
        }

        let safe_path = match sanitize_archive_path(&path_str) {
            Some(p) => p,
            None => {
                // Safely skip any path traversal / Zip-Slip / invalid entry
                continue;
            }
        };

        let effective_path = match root_strip {
            Some(prefix) => {
                match strip_root_component(&safe_path, prefix) {
                    Some(stripped) => {
                        if stripped.as_os_str().is_empty() {
                            let _ = std::fs::create_dir_all(dest_dir);
                            continue;
                        }
                        stripped.to_path_buf()
                    }
                    None => safe_path.clone(),
                }
            }
            None => safe_path.clone(),
        };

        let out_path = dest_dir.join(&effective_path);
        let clean = path_str.replace('\\', "/");
        let entry_type = entry.header().entry_type();
        let is_dir = entry_type.is_dir() || clean.ends_with('/');
        if !is_dir && !entry_type.is_file() && !entry_type.is_contiguous() {
            continue;
        }

        if is_dir {
            let dir_existed = std::fs::symlink_metadata(&out_path).is_ok();
            let _ = std::fs::create_dir_all(&out_path);
            if !dir_existed && out_path.exists() {
                newly_created.push(out_path.to_string_lossy().to_string());
            }
        } else {
            if let Some(p) = out_path.parent() {
                let _ = std::fs::create_dir_all(p);
            }

            let file_size = entry.size();
            let file_existed = std::fs::symlink_metadata(&out_path).is_ok();

            if file_size <= 64 * 1024 && !file_existed {
                clear_readonly(&out_path);
                let mut out_file = File::create(&out_path).map_err(|e| format!("Create file error: {}", e))?;
                let sz = file_size as usize;
                if sz > 0 {
                    entry.read_exact(&mut io_buf[..sz]).map_err(|e| format!("Tar read error: {}", e))?;
                    out_file.write_all(&io_buf[..sz]).map_err(|e| format!("Tar write error: {}", e))?;
                    total_extracted = total_extracted.saturating_add(file_size);
                }
            } else {
                let part_path = {
                    let mut p = out_path.as_os_str().to_os_string();
                    p.push(".baypart");
                    PathBuf::from(p)
                };

                let out_file = File::create(&part_path).map_err(|e| format!("Create temporary file error: {}", e))?;
                let cap = if file_size <= 64 * 1024 {
                    32 * 1024
                } else if file_size <= 1024 * 1024 {
                    256 * 1024
                } else {
                    512 * 1024
                };
                let mut writer = BufWriter::with_capacity(cap, out_file);
                let mut remaining = file_size;

                while remaining > 0 {
                    if is_extraction_cancelled() {
                        drop(writer);
                        let _ = std::fs::remove_file(&part_path);
                        return Err("Extraction cancelled by user".to_string());
                    }
                    let to_read = (remaining as usize).min(io_buf.len());
                    match entry.read(&mut io_buf[..to_read]) {
                        Ok(0) => {
                            drop(writer);
                            let _ = std::fs::remove_file(&part_path);
                            return Err(format!("Premature EOF inside tar entry '{}'", safe_path.display()));
                        }
                        Ok(n) => {
                            writer.write_all(&io_buf[..n]).map_err(|e| format!("Tar write error: {}", e))?;
                            remaining = remaining.saturating_sub(n as u64);
                            total_extracted = total_extracted.saturating_add(n as u64);

                            if let Some(app_handle) = app {
                                if last_emit.elapsed().as_millis() >= 80 {
                                    last_emit = Instant::now();
                                    let pct = ((total_extracted as f64 / total_expected_bytes as f64) * 100.0).min(99.0);
                                    let _ = app_handle.emit("extract_progress", ExtractProgress {
                                        percent: pct,
                                        current_file: safe_path.to_string_lossy().to_string(),
                                        bytes_extracted: total_extracted,
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            drop(writer);
                            let _ = std::fs::remove_file(&part_path);
                            return Err(format!("Tar read error: {}", e));
                        }
                    }
                }
                if let Err(e) = writer.flush() {
                    drop(writer);
                    let _ = std::fs::remove_file(&part_path);
                    return Err(format!("Flush error: {}", e));
                }
                drop(writer);
                if let Err(e) = safe_atomic_rename(&part_path, &out_path) {
                    let _ = std::fs::remove_file(&part_path);
                    return Err(format!("Failed to finalize extracted file: {}", e));
                }
            }

            let out_str = out_path.to_string_lossy().to_string();
            if !file_existed {
                newly_created.push(out_str.clone());
            }
            extracted_files.push(out_str);
        }
    }
    Ok(())
    })();

    if let Err(e) = res {
        cleanup_extracted_files(&newly_created);
        return Err(e);
    }

    Ok(extracted_files)
}

pub fn extract_tar_archive(
    archive_path: &Path,
    dest_dir: &Path,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    let raw_file = File::open(archive_path).map_err(|e| format!("Failed to open tar: {}", e))?;
    let reader = BufReader::with_capacity(4 * 1024 * 1024, raw_file);
    let is_gz = archive_path.to_string_lossy().to_lowercase().ends_with(".gz") 
             || archive_path.to_string_lossy().to_lowercase().ends_with(".tgz");
    
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create dest: {}", e))?;
    
    let root_strip = {
        let dest_stem = dest_dir.file_name().and_then(|s| s.to_str()).map(base_folder_stem);
        if let Some(prefix) = dest_stem {
            let (entries, _, _) = crate::inspector::list_tar_entries(archive_path);
            if let Some(single_root) = get_single_root_dir(&entries) {
                if single_root.eq_ignore_ascii_case(prefix) {
                    Some(prefix.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    if is_gz {
        let tar = flate2::read::GzDecoder::new(reader);
        let archive = tar::Archive::new(tar);
        extract_tar_entries(archive, dest_dir, expected_size, app, excluded, root_strip.as_deref())
    } else {
        let archive = tar::Archive::new(reader);
        extract_tar_entries(archive, dest_dir, expected_size, app, excluded, root_strip.as_deref())
    }
}

use std::fs::File;
use std::io::{Read, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

use crate::types::ExtractProgress;
use crate::inspector::{sanitize_archive_path, safe_atomic_rename, clear_readonly};
use super::is_extraction_cancelled;
use super::utils::{
    base_folder_stem,
    strip_root_component,
    cleanup_extracted_files,
    is_path_excluded,
};

pub fn extract_zip_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    let raw_file = File::open(archive_path).map_err(|e| format!("Failed to open zip: {}", e))?;
    let reader = BufReader::with_capacity(4 * 1024 * 1024, raw_file);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("Invalid zip archive: {}", e))?;
    let mut extracted = Vec::new();
    let mut newly_created = Vec::new();
    let root_strip = {
        let dest_stem = dest_dir.file_name().and_then(|s| s.to_str()).map(base_folder_stem);
        if let Some(prefix) = dest_stem {
            let mut candidate_root: Option<String> = None;
            let mut is_single = true;
            for name in archive.file_names() {
                let clean = name.trim().trim_start_matches(['/', '\\']).trim_start_matches("./");
                if clean.is_empty() || clean == "." { continue; }
                if !clean.contains('/') && !clean.contains('\\') && !name.ends_with('/') {
                    is_single = false;
                    break;
                }
                let first = clean.split(['/', '\\']).next().unwrap_or("");
                if first.is_empty() { continue; }
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
            if is_single && candidate_root.as_deref().map(|r| r.eq_ignore_ascii_case(prefix)).unwrap_or(false) {
                Some(prefix.to_string())
            } else {
                None
            }
        } else {
            None
        }
    };
    let total_entries = archive.len();
    let mut total_extracted: u64 = 0;
    let mut last_emit = Instant::now();

    let total_expected_bytes: u64 = if let Some(sz) = expected_size.filter(|&s| s > 0) {
        sz
    } else {
        std::fs::metadata(archive_path).map(|m| m.len() * 2).unwrap_or(1)
    };

    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let is_enc = pwd_opt.is_some() && crate::inspector::is_archive_encrypted(archive_path);

    // Single 2 MB reusable buffer outside the zip loop
    let mut io_buf = vec![0u8; 256 * 1024];

    let res = (|| -> Result<(), String> {
        for i in 0..total_entries {
        if is_extraction_cancelled() {
            return Err("Extraction cancelled by user".to_string());
        }
        let mut file = match pwd_opt {
            Some(pwd) => {
                match archive.by_index_decrypt(i, pwd.as_bytes()) {
                    Ok(f) => f,
                    Err(zip::result::ZipError::InvalidPassword) => {
                        return Err("Incorrect password: Could not decrypt file".to_string());
                    },
                    Err(e) => return Err(format!("Zip entry error: {}", e)),
                }
            },
            None => {
                match archive.by_index(i) {
                    Ok(f) => f,
                    Err(zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED)) => {
                        return Err("Password required: This ZIP archive is encrypted with AES/password.".to_string());
                    },
                    Err(zip::result::ZipError::InvalidPassword) => {
                        return Err("Password required: This ZIP archive is encrypted with AES/password.".to_string());
                    },
                    Err(e) => return Err(format!("Zip entry error: {}", e)),
                }
            }
        };

        let raw_name = file.name();
        if is_path_excluded(raw_name, excluded) {
            continue;
        }
        let safe_path = match sanitize_archive_path(raw_name) {
            Some(p) => p,
            None => continue,
        };

        let effective_path = match root_strip {
            Some(ref prefix) => {
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

        let outpath = dest_dir.join(&effective_path);
        let clean = raw_name.replace('\\', "/");
        if file.is_symlink() {
            continue;
        }
        if file.is_dir() || clean.ends_with('/') {
            let dir_existed = std::fs::symlink_metadata(&outpath).is_ok();
            let _ = std::fs::create_dir_all(&outpath);
            if !dir_existed && outpath.exists() {
                newly_created.push(outpath.to_string_lossy().to_string());
            }
        } else {
            if let Some(p) = outpath.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let file_size = file.size();
            let file_existed = std::fs::symlink_metadata(&outpath).is_ok();

            if file_size <= 64 * 1024 && !file_existed {
                let sz = file_size as usize;
                if sz > 0 {
                    file.read_exact(&mut io_buf[..sz]).map_err(|e| {
                        if is_enc {
                            format!("Incorrect password or corrupted encrypted ZIP entry: {}", e)
                        } else {
                            format!("Zip read error: {}", e)
                        }
                    })?;
                }
                let mut eof_check = [0u8; 1];
                let extra = file.read(&mut eof_check).map_err(|e| {
                    if is_enc {
                        format!("Incorrect password or corrupted encrypted ZIP entry: {}", e)
                    } else {
                        format!("Zip CRC/read error: {}", e)
                    }
                })?;
                if extra != 0 {
                    return Err(format!("Zip entry '{}' exceeded declared size", safe_path.display()));
                }
                clear_readonly(&outpath);
                let mut out_file = File::create(&outpath).map_err(|e| format!("Create file error: {}", e))?;
                if sz > 0 {
                    out_file.write_all(&io_buf[..sz]).map_err(|e| format!("Zip write error: {}", e))?;
                    total_extracted = total_extracted.saturating_add(file_size);
                }
            } else {
                let part_path = {
                    let mut p = outpath.as_os_str().to_os_string();
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
                    match file.read(&mut io_buf[..to_read]) {
                        Ok(0) => {
                            drop(writer);
                            let _ = std::fs::remove_file(&part_path);
                            return Err(format!("Premature EOF inside zip entry '{}'", safe_path.display()));
                        }
                        Ok(n) => {
                            writer.write_all(&io_buf[..n]).map_err(|e| format!("Zip write error: {}", e))?;
                            remaining -= n as u64;
                            total_extracted = total_extracted.saturating_add(n as u64);

                            if let Some(app_handle) = app {
                                if last_emit.elapsed().as_millis() >= 50 {
                                    last_emit = Instant::now();
                                    let pct = if total_expected_bytes > 0 {
                                        ((total_extracted as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
                                    } else {
                                        ((i as f64 / total_entries.max(1) as f64) * 100.0).min(99.0)
                                    };
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
                            return Err(if is_enc {
                                format!("Incorrect password or corrupted encrypted ZIP entry: {}", e)
                            } else {
                                format!("Zip read error: {}", e)
                            });
                        }
                    }
                }
                let mut eof_check = [0u8; 1];
                match file.read(&mut eof_check) {
                    Ok(0) => {}
                    Ok(_) => {
                        drop(writer);
                        let _ = std::fs::remove_file(&part_path);
                        return Err(format!("Zip entry '{}' exceeded declared size", safe_path.display()));
                    }
                    Err(e) => {
                        drop(writer);
                        let _ = std::fs::remove_file(&part_path);
                        return Err(if is_enc {
                            format!("Incorrect password or corrupted encrypted ZIP entry: {}", e)
                        } else {
                            format!("Zip CRC/read error: {}", e)
                        });
                    }
                }
                if let Err(e) = writer.flush() {
                    drop(writer);
                    let _ = std::fs::remove_file(&part_path);
                    return Err(format!("Zip flush error: {}", e));
                }
                drop(writer);
                if let Err(e) = safe_atomic_rename(&part_path, &outpath) {
                    let _ = std::fs::remove_file(&part_path);
                    return Err(format!("Failed to finalize extracted file: {}", e));
                }
            }

            let out_str = outpath.to_string_lossy().to_string();
            if !file_existed {
                newly_created.push(out_str.clone());
            }
            extracted.push(out_str);
        }

        if let Some(app_handle) = app {
            if last_emit.elapsed().as_millis() >= 50 || i + 1 == total_entries {
                last_emit = Instant::now();
                let pct = if total_expected_bytes > 0 {
                    ((total_extracted as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
                } else {
                    (((i + 1) as f64 / total_entries.max(1) as f64) * 100.0).min(99.0)
                };
                let _ = app_handle.emit("extract_progress", ExtractProgress {
                    percent: pct,
                    current_file: safe_path.to_string_lossy().to_string(),
                    bytes_extracted: total_extracted,
                });
            }
        }
    }

    Ok(())
    })();

    if let Err(e) = res {
        cleanup_extracted_files(&newly_created);
        return Err(e);
    }

    super::record_extracted_bytes(total_extracted);
    Ok(extracted)
}

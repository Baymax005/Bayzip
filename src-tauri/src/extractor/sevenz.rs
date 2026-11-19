use std::fs::File;
use std::io::{Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use sevenz_rust2::Password;

use crate::types::ExtractProgress;
use crate::inspector::{sanitize_archive_path, safe_atomic_rename, clear_readonly};
use super::is_extraction_cancelled;
use super::utils::{
    base_folder_stem,
    strip_root_component,
    cleanup_extracted_files,
    is_path_excluded,
};
use super::system_tar::extract_with_system_tar;

pub fn extract_7z_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create dest: {}", e))?;

    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let raw_file = File::open(archive_path).map_err(|e| format!("Failed to open 7z: {}", e))?;
    let reader = BufReader::with_capacity(4 * 1024 * 1024, raw_file);

    let pwd = pwd_opt.map(Password::from).unwrap_or_else(Password::empty);

    let mut sz_reader = match sevenz_rust2::ArchiveReader::new(reader, pwd) {
        Ok(r) => r,
        Err(sevenz_rust2::Error::PasswordRequired) => {
            return Err("Password required: This 7z archive is encrypted with AES-256.".into());
        }
        Err(e) => {
            if is_extraction_cancelled() || e.to_string().to_lowercase().contains("cancelled") {
                return Err("Extraction cancelled by user".to_string());
            }
            if crate::inspector::is_archive_encrypted(archive_path) {
                return Err(if pwd_opt.is_none() {
                    "Password required: This 7z archive is encrypted with AES-256.".into()
                } else {
                    "Incorrect password: Could not decrypt 7z archive.".into()
                });
            }
            return extract_with_system_tar(archive_path, dest_dir, app, excluded)
                .map_err(|_| format!("7z open error: {}", e));
        }
    };

    let total_files = sz_reader.archive().files.len();
    let total_expected_bytes: u64 = if let Some(sz) = expected_size.filter(|&s| s > 0) {
        sz
    } else {
        let sum: u64 = sz_reader.archive().files.iter().map(|f| f.size()).sum();
        if sum > 0 { sum } else { std::fs::metadata(archive_path).map(|m| m.len() * 2).unwrap_or(1) }
    };

    let mut extracted = Vec::new();
    let mut newly_created = Vec::new();
    let root_strip = {
        let dest_stem = dest_dir.file_name().and_then(|s| s.to_str()).map(base_folder_stem);
        if let Some(prefix) = dest_stem {
            let mut candidate_root: Option<String> = None;
            let mut is_single = true;
            for f in &sz_reader.archive().files {
                let clean = f.name().trim().trim_start_matches(['/', '\\']).trim_start_matches("./");
                if clean.is_empty() || clean == "." { continue; }
                if !clean.contains('/') && !clean.contains('\\') && !f.is_directory() && !f.name().ends_with('/') {
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
    let mut processed_count = 0;
    let mut last_emit = Instant::now();
    let mut total_extracted: u64 = 0;

    // Single 256 KB reusable buffer outside the 7z entry loop
    let mut io_buf = vec![0u8; 256 * 1024];

    let res = sz_reader.for_each_entries(|entry, reader| {
        if is_extraction_cancelled() {
            return Err(sevenz_rust2::Error::from(std::io::Error::new(std::io::ErrorKind::Interrupted, "Extraction cancelled by user")));
        }
        let raw_name = entry.name();
        if is_path_excluded(raw_name, excluded) {
            return Ok(true);
        }
        let safe_path = match sanitize_archive_path(raw_name) {
            Some(p) => p,
            None => return Ok(true),
        };

        let effective_path = match root_strip {
            Some(ref prefix) => {
                match strip_root_component(&safe_path, prefix) {
                    Some(stripped) => {
                        if stripped.as_os_str().is_empty() {
                            let _ = std::fs::create_dir_all(dest_dir);
                            return Ok(true);
                        }
                        stripped.to_path_buf()
                    }
                    None => safe_path.clone(),
                }
            }
            None => safe_path.clone(),
        };

        let out_path = dest_dir.join(&effective_path);
        let clean = raw_name.replace('\\', "/");
        if entry.is_directory() || clean.ends_with('/') {
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
                let mut out_file = File::create(&out_path)
                    .map_err(sevenz_rust2::Error::from)?;
                let sz = file_size as usize;
                if sz > 0 {
                    reader.read_exact(&mut io_buf[..sz]).map_err(sevenz_rust2::Error::from)?;
                    out_file.write_all(&io_buf[..sz]).map_err(sevenz_rust2::Error::from)?;
                    total_extracted = total_extracted.saturating_add(file_size);
                }
            } else {
                let part_path = {
                    let mut p = out_path.as_os_str().to_os_string();
                    p.push(".baypart");
                    PathBuf::from(p)
                };

                let out_file = File::create(&part_path)
                    .map_err(sevenz_rust2::Error::from)?;
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
                        return Err(sevenz_rust2::Error::from(std::io::Error::new(std::io::ErrorKind::Interrupted, "Extraction cancelled by user")));
                    }
                    let to_read = (remaining as usize).min(io_buf.len());
                    match reader.read(&mut io_buf[..to_read]) {
                        Ok(0) => {
                            drop(writer);
                            let _ = std::fs::remove_file(&part_path);
                            return Err(sevenz_rust2::Error::from(std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                format!("Premature EOF inside 7z entry '{}'", safe_path.display()),
                            )));
                        }
                        Ok(n) => {
                            writer.write_all(&io_buf[..n]).map_err(sevenz_rust2::Error::from)?;
                            remaining -= n as u64;
                            total_extracted = total_extracted.saturating_add(n as u64);

                            if let Some(app_handle) = app {
                                if last_emit.elapsed().as_millis() >= 50 {
                                    last_emit = Instant::now();
                                    let pct = if total_expected_bytes > 0 {
                                        ((total_extracted as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
                                    } else if total_files > 0 {
                                        ((processed_count as f64 / total_files as f64) * 100.0).min(99.0)
                                    } else {
                                        50.0
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
                            return Err(sevenz_rust2::Error::from(e));
                        }
                    }
                }
                if let Err(e) = writer.flush() {
                    drop(writer);
                    let _ = std::fs::remove_file(&part_path);
                    return Err(sevenz_rust2::Error::from(e));
                }
                drop(writer);
                if let Err(e) = safe_atomic_rename(&part_path, &out_path) {
                    let _ = std::fs::remove_file(&part_path);
                    return Err(sevenz_rust2::Error::from(e));
                }
            }

            let out_str = out_path.to_string_lossy().to_string();
            if !file_existed {
                newly_created.push(out_str.clone());
            }
            extracted.push(out_str);
        }

        processed_count += 1;
        if let Some(app_handle) = app {
            if last_emit.elapsed().as_millis() >= 50 || processed_count == total_files {
                last_emit = Instant::now();
                let pct = if total_expected_bytes > 0 {
                    ((total_extracted as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
                } else if total_files > 0 {
                    ((processed_count as f64 / total_files as f64) * 100.0).min(99.0)
                } else {
                    50.0
                };
                let _ = app_handle.emit("extract_progress", ExtractProgress {
                    percent: pct,
                    current_file: safe_path.to_string_lossy().to_string(),
                    bytes_extracted: total_extracted,
                });
            }
        }

        Ok(true)
    });

    match res {
        Ok(_) => Ok(extracted),
        Err(sevenz_rust2::Error::PasswordRequired) => {
            cleanup_extracted_files(&newly_created);
            Err("Password required: This 7z archive is encrypted with AES-256.".into())
        }
        Err(e) => {
            cleanup_extracted_files(&newly_created);
            let err_lower = e.to_string().to_lowercase();
            if is_extraction_cancelled() || err_lower.contains("cancelled") || err_lower.contains("canceled") {
                Err("Extraction cancelled by user".into())
            } else if err_lower.contains("password") || crate::inspector::is_archive_encrypted(archive_path) {
                if pwd_opt.is_none() {
                    Err("Password required: This 7z archive is encrypted with AES-256.".into())
                } else {
                    Err("Incorrect password: Could not decrypt 7z archive.".into())
                }
            } else {
                extract_with_system_tar(archive_path, dest_dir, app, excluded)
                    .map_err(|_| format!("7z extraction error: {}", e))
            }
        }
    }
}

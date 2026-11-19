use std::io::{BufReader, BufRead};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

use crate::types::ExtractProgress;
use crate::priority::ScopedPriorityGuard;
use crate::inspector::sanitize_archive_path;
use super::is_extraction_cancelled;
use super::utils::{cleanup_extracted_files, is_path_excluded};

struct StagingDirGuard(Option<PathBuf>);

impl Drop for StagingDirGuard {
    fn drop(&mut self) {
        if let Some(dir) = self.0.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

pub(crate) fn normalize_fs_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn create_dir_all_tracked(
    target_dir: &Path,
    stop_at: &Path,
    newly_created: &mut Vec<String>,
) -> Result<(), String> {
    if target_dir.exists() {
        return Ok(());
    }
    let mut missing = Vec::new();
    let mut cur = target_dir;
    while cur != stop_at && !cur.exists() {
        missing.push(cur.to_path_buf());
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }
    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create destination directory '{}': {}", target_dir.display(), e))?;
    for dir in missing.into_iter().rev() {
        newly_created.push(dir.to_string_lossy().to_string());
    }
    Ok(())
}

fn finalize_staged_extraction_inner(
    stage_root: &Path,
    dest_dir: &Path,
    excluded: Option<&std::collections::HashSet<String>>,
    preexisting_paths: &std::collections::HashSet<String>,
    newly_created: &mut Vec<String>,
    extracted_files: &mut Vec<String>,
) -> Result<u64, String> {
    let mut kept_bytes: u64 = 0;
    let mut dir_stack = vec![PathBuf::new()];
    while let Some(rel_dir) = dir_stack.pop() {
        if is_extraction_cancelled() {
            return Err("Extraction cancelled by user".into());
        }
        let current_stage = stage_root.join(&rel_dir);
        let entries = std::fs::read_dir(&current_stage)
            .map_err(|e| format!("Failed to inspect staged extraction directory: {}", e))?;

        for entry_res in entries {
            let entry = entry_res.map_err(|e| format!("Failed reading staged entry: {}", e))?;
            let rel_path = rel_dir.join(entry.file_name());
            let clean_rel = rel_path.to_string_lossy().replace('\\', "/");

            if is_path_excluded(&clean_rel, excluded) {
                continue;
            }
            let Some(safe_rel) = sanitize_archive_path(&clean_rel) else {
                continue;
            };
            let ft = entry
                .file_type()
                .map_err(|e| format!("Failed inspecting staged file type: {}", e))?;
            if ft.is_symlink() {
                return Err(format!(
                    "Security violation: staged extraction produced symlink '{}'. Extraction aborted.",
                    clean_rel
                ));
            }

            let target_path = dest_dir.join(&safe_rel);
            let target_key = normalize_fs_key(&target_path);

            if ft.is_dir() {
                if !preexisting_paths.contains(&target_key) {
                    create_dir_all_tracked(&target_path, dest_dir, newly_created)?;
                } else {
                    std::fs::create_dir_all(&target_path)
                        .map_err(|e| format!("Failed preparing directory '{}': {}", target_path.display(), e))?;
                }
                dir_stack.push(rel_path);
            } else if ft.is_file() {
                if let Some(parent) = target_path.parent() {
                    create_dir_all_tracked(parent, dest_dir, newly_created)?;
                }
                let was_preexisting =
                    preexisting_paths.contains(&target_key) || std::fs::symlink_metadata(&target_path).is_ok();
                let entry_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let src_path = entry.path();
                if std::fs::rename(&src_path, &target_path).is_err() {
                    std::fs::copy(&src_path, &target_path).map_err(|e| {
                        format!("Failed moving extracted file '{}': {}", target_path.display(), e)
                    })?;
                    let _ = std::fs::remove_file(&src_path);
                }
                kept_bytes = kept_bytes.saturating_add(entry_size);
                let target_str = target_path.to_string_lossy().to_string();
                if !was_preexisting {
                    newly_created.push(target_str.clone());
                }
                extracted_files.push(target_str);
            }
        }
    }
    Ok(kept_bytes)
}

#[cfg(test)]
pub(crate) fn finalize_staged_extraction(
    stage_root: &Path,
    dest_dir: &Path,
    excluded: Option<&std::collections::HashSet<String>>,
    preexisting_paths: &std::collections::HashSet<String>,
    newly_created: &mut Vec<String>,
    extracted_files: &mut Vec<String>,
) -> Result<(), String> {
    finalize_staged_extraction_inner(
        stage_root,
        dest_dir,
        excluded,
        preexisting_paths,
        newly_created,
        extracted_files,
    )
    .map(|_| ())
}

pub fn extract_with_system_tar(
    archive_path: &Path,
    dest_dir: &Path,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    let _priority_guard = ScopedPriorityGuard::new();
    if is_extraction_cancelled() {
        return Err("Extraction cancelled by user".into());
    }

    // Pre-inspect archive entries with tar.exe -tf to strictly prevent Zip-Slip / path traversal before writing to disk
    let mut check_cmd = Command::new(crate::inspector::get_system32_exe("tar.exe"));
    check_cmd.args(["-tf", &archive_path.to_string_lossy()]);
    #[cfg(target_os = "windows")]
    check_cmd.creation_flags(crate::priority::get_child_process_creation_flags());
    check_cmd.stdout(Stdio::piped());
    check_cmd.stderr(Stdio::piped());

    let output = check_cmd
        .output()
        .map_err(|e| format!("Failed to inspect archive with system tar: {}", e))?;

    if !output.status.success() {
        let err_text = String::from_utf8_lossy(&output.stderr);
        return Err(format!("System tar archive inspection failed: {}", err_text.trim()));
    }

    let mut preexisting_paths = std::collections::HashSet::new();
    let mut total_entries: usize = 0;
    let mut included_entries: usize = 0;
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    for raw_line in stdout_str.lines() {
        let trimmed = raw_line.trim_end_matches(['\r', '\n']);
        if trimmed.trim().is_empty() {
            continue;
        }
        let clean = trimmed.replace('\\', "/");
        if clean == "." || clean == "./" {
            continue;
        }
        let Some(safe_path) = sanitize_archive_path(&clean) else {
            return Err(format!(
                "Security violation: archive entry '{}' contains dangerous path or traversal pattern. Extraction aborted.",
                trimmed
            ));
        };
        total_entries += 1;
        if !is_path_excluded(&clean, excluded) {
            included_entries += 1;
        }
        let candidate = dest_dir.join(&safe_path);
        if std::fs::symlink_metadata(&candidate).is_ok() {
            preexisting_paths.insert(normalize_fs_key(&candidate));
        }
    }

    // Secondary fail-closed inspection with -tvf to block symlinks and hardlinks before extraction
    let mut tvf_cmd = Command::new(crate::inspector::get_system32_exe("tar.exe"));
    tvf_cmd.args(["-tvf", &archive_path.to_string_lossy()]);
    #[cfg(target_os = "windows")]
    tvf_cmd.creation_flags(crate::priority::get_child_process_creation_flags());
    tvf_cmd.stdout(Stdio::piped());
    tvf_cmd.stderr(Stdio::piped());

    let tvf_output = tvf_cmd
        .output()
        .map_err(|e| format!("Failed to verify archive entry links with system tar: {}", e))?;
    if !tvf_output.status.success() {
        let err_text = String::from_utf8_lossy(&tvf_output.stderr);
        return Err(format!("System tar link inspection failed: {}", err_text.trim()));
    }
    let tvf_str = String::from_utf8_lossy(&tvf_output.stdout);
    for line in tvf_str.lines() {
        let l = line.trim_start();
        if l.is_empty() {
            continue;
        }
        if l.starts_with('l') || l.starts_with('h') || l.contains(" -> ") || l.contains(" link to ") {
            return Err(format!(
                "Security violation: archive contains symlink or hardlink entry '{}'. Extraction aborted.",
                l.trim()
            ));
        }
    }

    let has_exclusions = excluded.is_some_and(|set| !set.is_empty());
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    if has_exclusions && total_entries > 0 && included_entries == 0 {
        return Ok(Vec::new());
    }

    // Determine estimated uncompressed size or archive file size on disk
    let archive_size = std::fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);
    let mut total_expected_bytes = archive_size.max(1);

    // If we can get total uncompressed size from inspector, use it
    let (_rar_items, _total_count, uncompressed_size) = crate::inspector::list_rar_entries(archive_path);
    if uncompressed_size > 0 {
        total_expected_bytes = uncompressed_size;
    }

    // Isolate excluded extractions in a same-volume staging folder so bsdtar case/glob
    // differences can never overwrite preexisting destination files or leak excluded paths.
    let mut staging_guard = StagingDirGuard(None);
    let extract_target: PathBuf = if has_exclusions {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut idx = 0u32;
        let stage_path = loop {
            let candidate = dest_dir.join(format!(".bayzip_tar_stage_{}_{}_{}", std::process::id(), stamp, idx));
            if !candidate.exists() {
                break candidate;
            }
            idx = idx.saturating_add(1);
        };
        std::fs::create_dir_all(&stage_path)
            .map_err(|e| format!("Failed to create staging directory: {}", e))?;
        staging_guard = StagingDirGuard(Some(stage_path.clone()));
        stage_path
    } else {
        dest_dir.to_path_buf()
    };

    let mut cmd = Command::new(crate::inspector::get_system32_exe("tar.exe"));
    let args: Vec<String> = vec![
        "-xvf".into(),
        archive_path.to_string_lossy().to_string(),
        "-C".into(),
        extract_target.to_string_lossy().to_string(),
        "--no-same-permissions".into(),
        "--no-same-owner".into(),
    ];
    cmd.args(&args);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(crate::priority::get_child_process_creation_flags());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("System extraction error: {}", e))?;
    let stderr = child.stderr.take().ok_or_else(|| "Failed to capture tar.exe stderr".to_string())?;

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let reader_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut extracted_files = Vec::new();
    let mut newly_created = Vec::new();
    let mut current_file_name = String::new();
    let mut current_file_path: Option<PathBuf> = None;
    let mut completed_bytes: u64 = 0;
    let mut last_emit = Instant::now();
    let mut error_lines = Vec::new();

    let record_direct_entry = |rel_path: &str,
                               current_path_slot: &mut Option<PathBuf>,
                               current_name_slot: &mut String,
                               completed_total: &mut u64,
                               newly: &mut Vec<String>,
                               extracted: &mut Vec<String>| {
        let clean_rel = rel_path.replace('\\', "/");
        if let Some(prev_path) = current_path_slot.take() {
            if let Ok(m) = std::fs::metadata(&prev_path) {
                *completed_total += m.len();
            }
        }
        let Some(safe_path) = sanitize_archive_path(&clean_rel) else {
            *current_name_slot = clean_rel;
            return;
        };
        let out_path = extract_target.join(&safe_path);
        *current_name_slot = safe_path.to_string_lossy().to_string();
        *current_path_slot = Some(out_path.clone());

        if !has_exclusions {
            let norm_key = normalize_fs_key(&out_path);
            let was_preexisting = preexisting_paths.contains(&norm_key);
            let out_str = out_path.to_string_lossy().to_string();
            if !was_preexisting {
                newly.push(out_str.clone());
            }
            extracted.push(out_str);
        }
    };

    loop {
        if is_extraction_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader_handle.join();
            cleanup_extracted_files(&newly_created);
            return Err("Extraction cancelled by user".into());
        }

        while let Ok(line) = rx.try_recv() {
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.trim().is_empty() {
                continue;
            }

            if let Some(rel_path) = trimmed.trim_start().strip_prefix("x ") {
                record_direct_entry(
                    rel_path,
                    &mut current_file_path,
                    &mut current_file_name,
                    &mut completed_bytes,
                    &mut newly_created,
                    &mut extracted_files,
                );
            } else if trimmed.to_lowercase().contains("error") || trimmed.to_lowercase().contains("fatal") {
                error_lines.push(trimmed.trim().to_string());
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = reader_handle.join();

                while let Ok(line) = rx.try_recv() {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if let Some(rel_path) = trimmed.trim_start().strip_prefix("x ") {
                        record_direct_entry(
                            rel_path,
                            &mut current_file_path,
                            &mut current_file_name,
                            &mut completed_bytes,
                            &mut newly_created,
                            &mut extracted_files,
                        );
                    } else if trimmed.to_lowercase().contains("error") || trimmed.to_lowercase().contains("fatal") {
                        error_lines.push(trimmed.trim().to_string());
                    }
                }

                if !status.success() {
                    let err_msg = if !error_lines.is_empty() {
                        error_lines.join("; ")
                    } else {
                        format!("tar.exe exited with error code: {:?}", status.code())
                    };
                    cleanup_extracted_files(&newly_created);
                    return Err(format!("Extraction error: {}", err_msg));
                }
                break;
            }
            Ok(None) => {
                let active_file_size = current_file_path.as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())
                    .unwrap_or(0);

                let current_total = completed_bytes + active_file_size;

                if let Some(app_handle) = app {
                    if last_emit.elapsed().as_millis() >= 80 {
                        last_emit = Instant::now();
                        let pct = if total_expected_bytes > 0 {
                            ((current_total as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
                        } else {
                            50.0
                        };

                        let _ = app_handle.emit("extract_progress", ExtractProgress {
                            percent: pct,
                            current_file: current_file_name.clone(),
                            bytes_extracted: current_total,
                        });
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader_handle.join();
                cleanup_extracted_files(&newly_created);
                return Err(format!("Error monitoring tar.exe process: {}", e));
            }
        }
    }

    if has_exclusions {
        match finalize_staged_extraction_inner(
            &extract_target,
            dest_dir,
            excluded,
            &preexisting_paths,
            &mut newly_created,
            &mut extracted_files,
        ) {
            Ok(kept_bytes) => {
                super::record_extracted_bytes(kept_bytes);
            }
            Err(err) => {
                cleanup_extracted_files(&newly_created);
                return Err(err);
            }
        }
        drop(staging_guard);
    } else {
        if let Some(prev_path) = current_file_path.take() {
            if let Ok(m) = std::fs::metadata(&prev_path) {
                completed_bytes = completed_bytes.saturating_add(m.len());
            }
        }
        super::record_extracted_bytes(completed_bytes);
        if extracted_files.is_empty() {
            if let Ok(entries) = std::fs::read_dir(dest_dir) {
                for entry in entries.flatten() {
                    extracted_files.push(entry.path().to_string_lossy().to_string());
                }
            }
        }
        if extracted_files.is_empty() {
            extracted_files.push(dest_dir.to_string_lossy().to_string());
        }
    }

    Ok(extracted_files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finalize_staged_extraction_preserves_preexisting_and_filters_case_glob_segments() {
        let base_dir = std::env::current_dir().unwrap().join("target").join("test_staged_tar_filter");
        let _ = std::fs::remove_dir_all(&base_dir);
        let stage_dir = base_dir.join("stage");
        let dest_dir = base_dir.join("dest");
        std::fs::create_dir_all(stage_dir.join("Assets/Audio")).unwrap();
        std::fs::create_dir_all(stage_dir.join("dir[1]")).unwrap();
        std::fs::create_dir_all(stage_dir.join("other/sub")).unwrap();
        std::fs::create_dir_all(dest_dir.join("Assets")).unwrap();

        // Preexisting file that must NOT be clobbered by excluded entry even on case mismatch
        std::fs::write(dest_dir.join("Assets/Secret.txt"), b"preexisting-user-data").unwrap();
        std::fs::write(stage_dir.join("Assets/Secret.txt"), b"excluded-archive-data").unwrap();
        std::fs::write(stage_dir.join("Assets/Audio/theme.ogg"), b"excluded-audio").unwrap();
        std::fs::write(stage_dir.join("dir[1]/blocked.bin"), b"excluded-glob").unwrap();
        std::fs::write(stage_dir.join("other/sub/keep.txt"), b"kept-sub-component").unwrap();

        let excluded = crate::inspector::build_excluded_set([
            "assets/secret.txt",
            "assets/audio",
            "dir[1]",
            "sub",
        ]);
        let mut preexisting = std::collections::HashSet::new();
        preexisting.insert(normalize_fs_key(&dest_dir.join("Assets")));
        preexisting.insert(normalize_fs_key(&dest_dir.join("Assets/Secret.txt")));

        let mut newly_created = Vec::new();
        let mut extracted_files = Vec::new();
        let _guard = StagingDirGuard(Some(stage_dir.clone()));

        finalize_staged_extraction(
            &stage_dir,
            &dest_dir,
            excluded.as_ref(),
            &preexisting,
            &mut newly_created,
            &mut extracted_files,
        )
        .unwrap();
        drop(_guard);

        assert_eq!(
            std::fs::read_to_string(dest_dir.join("Assets/Secret.txt")).unwrap(),
            "preexisting-user-data"
        );
        assert!(!dest_dir.join("Assets/Audio").exists());
        assert!(!dest_dir.join("dir[1]").exists());
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("other/sub/keep.txt")).unwrap(),
            "kept-sub-component"
        );
        assert!(!stage_dir.exists());

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}

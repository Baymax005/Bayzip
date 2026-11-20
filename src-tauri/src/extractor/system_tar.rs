use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::time::{Duration, Instant};
use tauri::AppHandle;

use crate::inspector::{safe_atomic_rename, sanitize_archive_path};
use crate::priority::ScopedPriorityGuard;
use super::io::DirectoryCache;
use super::is_extraction_cancelled;
use super::utils::{cleanup_extracted_files, emit_extract_progress, is_path_excluded};

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

#[derive(Debug, PartialEq, Eq)]
enum TvfLineResult<'a> {
    Empty,
    Entry {
        raw_name: &'a str,
        size: u64,
        is_dir: bool,
    },
    Unparseable,
}

fn is_time_or_year(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() == 5 && bytes[2] == b':' {
        return bytes[0].is_ascii_digit()
            && bytes[1].is_ascii_digit()
            && bytes[3].is_ascii_digit()
            && bytes[4].is_ascii_digit();
    }
    if bytes.len() == 4 && bytes.iter().all(|b| b.is_ascii_digit()) {
        if let Ok(year) = s.parse::<u16>() {
            return (1900..=2999).contains(&year);
        }
    }
    false
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 {
        return false;
    }
    let yyyy_mm_dd = (b[4] == b'-' || b[4] == b'/')
        && (b[7] == b'-' || b[7] == b'/')
        && b[0..4].iter().all(|c| c.is_ascii_digit())
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[8..10].iter().all(|c| c.is_ascii_digit());
    let dd_mm_yyyy = (b[2] == b'-' || b[2] == b'/')
        && (b[5] == b'-' || b[5] == b'/')
        && b[0..2].iter().all(|c| c.is_ascii_digit())
        && b[3..5].iter().all(|c| c.is_ascii_digit())
        && b[6..10].iter().all(|c| c.is_ascii_digit());
    yyyy_mm_dd || dd_mm_yyyy
}

fn is_strict_english_month(s: &str) -> bool {
    let t = s.trim_end_matches('.');
    matches!(
        t.to_ascii_lowercase().as_str(),
        "jan" | "feb" | "mar" | "apr" | "may" | "jun" | "jul" | "aug" | "sep" | "oct" | "nov" | "dec"
    )
}

fn is_localized_month_fallback(s: &str) -> bool {
    let t = s.trim_end_matches('.');
    !t.is_empty()
        && t.len() <= 8
        && t.chars().any(|c| c.is_alphabetic())
        && !t.chars().any(|c| c.is_ascii_digit())
}

fn is_day_token(s: &str) -> bool {
    s.parse::<u8>().is_ok_and(|d| (1..=31).contains(&d))
}

fn find_tvf_size_and_name_start(
    tokens: &[(usize, usize, &str)],
    allow_localized_month: bool,
) -> Option<(u64, usize)> {
    let min_time_idx = if allow_localized_month { 6 } else { 4 };
    for i in 3..tokens.len().saturating_sub(1) {
        if i < min_time_idx && !is_iso_date(tokens[i - 1].2) {
            continue;
        }
        if !is_time_or_year(tokens[i].2) {
            continue;
        }

        // 2-column ISO date: <size> <YYYY-MM-DD> <HH:MM> <name>
        if is_iso_date(tokens[i - 1].2) {
            if let Ok(size) = tokens[i - 2].2.parse::<u64>() {
                return Some((size, tokens[i].1));
            }
        }

        let month_match = |tok: &str| {
            is_strict_english_month(tok)
                || (allow_localized_month && is_localized_month_fallback(tok))
        };

        // 3-column US/BSD date: <size> <Mon> <DD> <HH:MM|YYYY> <name>
        if i >= 4
            && is_day_token(tokens[i - 1].2)
            && month_match(tokens[i - 2].2)
        {
            if let Ok(size) = tokens[i - 3].2.parse::<u64>() {
                return Some((size, tokens[i].1));
            }
        }

        // 3-column European date: <size> <DD> <Mon> <HH:MM|YYYY> <name>
        if i >= 4
            && month_match(tokens[i - 1].2)
            && is_day_token(tokens[i - 2].2)
        {
            if let Ok(size) = tokens[i - 3].2.parse::<u64>() {
                return Some((size, tokens[i].1));
            }
        }
    }
    None
}

/// Fail-closed parser for `bsdtar -tvf` output lines.
/// Rejects symlinks, hardlinks, and special device/FIFO/socket entries immediately while
/// preserving consecutive/trailing spaces in filenames for `sanitize_archive_path`.
fn parse_bsdtar_tvf_line(raw_line: &str) -> Result<TvfLineResult<'_>, String> {
    let line = raw_line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() {
        return Ok(TvfLineResult::Empty);
    }

    let trimmed_start = line.trim_start();
    let mode_char = trimmed_start.chars().next().unwrap_or('?');

    if matches!(mode_char, 'l' | 'h' | 'b' | 'c' | 'p' | 's')
        || trimmed_start.contains(" -> ")
        || trimmed_start.contains(" link to ")
    {
        return Err(format!(
            "Security violation: archive contains symlink, hardlink, or special device entry '{}'. Extraction aborted.",
            trimmed_start.trim()
        ));
    }

    if !matches!(mode_char, '-' | 'd') {
        return Ok(TvfLineResult::Unparseable);
    }

    let mut tokens: Vec<(usize, usize, &str)> = Vec::with_capacity(10);
    let bytes = trimmed_start.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }
        let start = idx;
        while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        tokens.push((start, idx, &trimmed_start[start..idx]));
    }

    if tokens.len() < 5 {
        return Ok(TvfLineResult::Unparseable);
    }

    let matched = find_tvf_size_and_name_start(&tokens, false)
        .or_else(|| find_tvf_size_and_name_start(&tokens, true));

    let Some((parsed_size, name_start)) = matched else {
        return Ok(TvfLineResult::Unparseable);
    };

    let after_date = &trimmed_start[name_start..];
    let raw_name = after_date.strip_prefix(' ').unwrap_or(after_date);
    if raw_name.is_empty() {
        return Ok(TvfLineResult::Unparseable);
    }

    let is_dir = mode_char == 'd' || raw_name.ends_with('/') || raw_name.ends_with('\\');
    let size = if is_dir { 0 } else { parsed_size };
    Ok(TvfLineResult::Entry {
        raw_name,
        size,
        is_dir,
    })
}

struct SystemTarInspection {
    preexisting_paths: HashSet<String>,
    entry_sizes: HashMap<String, u64>,
    total_entries: usize,
    included_entries: usize,
    total_uncompressed_bytes: u64,
    had_unparseable: bool,
}

fn run_tar_streaming_lines(
    args: &[&str],
    spawn_err_prefix: &str,
    fail_err_prefix: &str,
    mut on_line: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    let mut cmd = Command::new(crate::inspector::get_system32_exe("tar.exe"));
    cmd.args(args);
    cmd.env("LC_ALL", "C");
    cmd.env("LANG", "C");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(crate::priority::get_child_process_creation_flags());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("{}: {}", spawn_err_prefix, e))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{}: failed to capture stdout", spawn_err_prefix))?;
    let stderr = child.stderr.take();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let stdout_handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx.send(std::mem::take(&mut line)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let stderr_handle = stderr.map(|mut err_pipe| {
        std::thread::spawn(move || {
            let mut err_buf = String::new();
            let _ = err_pipe.read_to_string(&mut err_buf);
            err_buf
        })
    });

    let kill_and_join = |child: &mut std::process::Child,
                         stdout_h: std::thread::JoinHandle<()>,
                         stderr_h: Option<std::thread::JoinHandle<String>>| {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stdout_h.join();
        if let Some(h) = stderr_h {
            let _ = h.join();
        }
    };

    let mut abort_err: Option<String> = None;
    let status = loop {
        if is_extraction_cancelled() {
            abort_err = Some("Extraction cancelled by user".into());
            break None;
        }

        while let Ok(line) = rx.try_recv() {
            if let Err(err) = on_line(&line) {
                abort_err = Some(err);
                break;
            }
        }
        if abort_err.is_some() {
            break None;
        }

        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => {
                abort_err = Some(format!("{}: {}", spawn_err_prefix, e));
                break None;
            }
        }
    };

    if let Some(err) = abort_err {
        kill_and_join(&mut child, stdout_handle, stderr_handle);
        return Err(err);
    }

    let _ = stdout_handle.join();
    let err_text = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    while let Ok(line) = rx.try_recv() {
        on_line(&line)?;
    }

    if let Some(st) = status {
        if !st.success() {
            return Err(format!("{}: {}", fail_err_prefix, err_text.trim()));
        }
    }

    Ok(())
}

fn inspect_archive_with_system_tar(
    archive_path: &Path,
    dest_dir: &Path,
    dest_was_empty: bool,
    excluded: Option<&HashSet<String>>,
) -> Result<SystemTarInspection, String> {
    let mut inspection = SystemTarInspection {
        preexisting_paths: HashSet::new(),
        entry_sizes: HashMap::new(),
        total_entries: 0,
        included_entries: 0,
        total_uncompressed_bytes: 0,
        had_unparseable: false,
    };
    let archive_arg = archive_path.to_string_lossy().to_string();

    run_tar_streaming_lines(
        &["-tvf", &archive_arg],
        "Failed to inspect archive with system tar",
        "System tar archive inspection failed",
        |raw_line| match parse_bsdtar_tvf_line(raw_line)? {
            TvfLineResult::Empty => Ok(()),
            TvfLineResult::Unparseable => {
                inspection.had_unparseable = true;
                Ok(())
            }
            TvfLineResult::Entry {
                raw_name,
                size,
                is_dir,
            } => {
                let clean = raw_name.replace('\\', "/");
                if clean == "." || clean == "./" {
                    return Ok(());
                }
                let Some(safe_path) = sanitize_archive_path(&clean) else {
                    return Err(format!(
                        "Security violation: archive entry '{}' contains dangerous path or traversal pattern. Extraction aborted.",
                        raw_name
                    ));
                };
                if safe_path.as_os_str().is_empty() {
                    return Ok(());
                }

                inspection.total_entries += 1;
                if !is_path_excluded(&clean, excluded) {
                    inspection.included_entries += 1;
                }
                inspection.total_uncompressed_bytes =
                    inspection.total_uncompressed_bytes.saturating_add(size);

                if !is_dir {
                    inspection
                        .entry_sizes
                        .insert(normalize_fs_key(&safe_path), size);
                }

                if !dest_was_empty {
                    let candidate = dest_dir.join(&safe_path);
                    if std::fs::symlink_metadata(&candidate).is_ok() {
                        inspection
                            .preexisting_paths
                            .insert(normalize_fs_key(&candidate));
                    }
                }
                Ok(())
            }
        },
    )?;

    // Fail-closed fallback ONLY if an unusual -tvf line format appeared: verify raw paths via -tf
    // and force staged extraction so symlink_metadata/reparse guards validate every extracted file.
    if inspection.had_unparseable {
        inspection.preexisting_paths.clear();
        inspection.entry_sizes.clear();
        inspection.total_entries = 0;
        inspection.included_entries = 0;
        inspection.total_uncompressed_bytes = 0;

        run_tar_streaming_lines(
            &["-tf", &archive_arg],
            "Failed to inspect archive with system tar",
            "System tar archive inspection failed",
            |raw_line| {
                let trimmed = raw_line.trim_end_matches(['\r', '\n']);
                if trimmed.trim().is_empty() {
                    return Ok(());
                }
                let clean = trimmed.replace('\\', "/");
                if clean == "." || clean == "./" {
                    return Ok(());
                }
                let Some(safe_path) = sanitize_archive_path(&clean) else {
                    return Err(format!(
                        "Security violation: archive entry '{}' contains dangerous path or traversal pattern. Extraction aborted.",
                        trimmed
                    ));
                };
                if safe_path.as_os_str().is_empty() {
                    return Ok(());
                }

                inspection.total_entries += 1;
                if !is_path_excluded(&clean, excluded) {
                    inspection.included_entries += 1;
                }
                if !dest_was_empty {
                    let candidate = dest_dir.join(&safe_path);
                    if std::fs::symlink_metadata(&candidate).is_ok() {
                        inspection
                            .preexisting_paths
                            .insert(normalize_fs_key(&candidate));
                    }
                }
                Ok(())
            },
        )?;
    }

    Ok(inspection)
}

fn finalize_staged_extraction_inner(
    stage_root: &Path,
    dest_dir: &Path,
    excluded: Option<&HashSet<String>>,
    preexisting_paths: &HashSet<String>,
    newly_created: &mut Vec<String>,
    extracted_files: &mut Vec<String>,
) -> Result<u64, String> {
    let dest_was_empty = !dest_dir.exists()
        || std::fs::read_dir(dest_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);
    let mut dir_cache = DirectoryCache::new();
    dir_cache.mark_existing(dest_dir);

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
            if ft.is_symlink()
                || std::fs::symlink_metadata(entry.path())
                    .map(|m| super::io::is_reparse_or_symlink(&m))
                    .unwrap_or(false)
            {
                return Err(format!(
                    "Security violation: staged extraction produced symlink '{}'. Extraction aborted.",
                    clean_rel
                ));
            }

            let target_path = dest_dir.join(&safe_rel);
            let target_key = normalize_fs_key(&target_path);

            if ft.is_dir() {
                if !preexisting_paths.contains(&target_key) {
                    dir_cache.ensure_dir_tracked(
                        &target_path,
                        dest_dir,
                        dest_was_empty,
                        newly_created,
                    )?;
                } else {
                    super::io::check_existing_target_safe(&target_path, false)?;
                    dir_cache.ensure_dir(&target_path).map_err(|e| {
                        format!(
                            "Failed preparing directory '{}': {}",
                            target_path.display(),
                            e
                        )
                    })?;
                }
                dir_stack.push(rel_path);
            } else if ft.is_file() {
                dir_cache.ensure_parent_tracked(
                    &target_path,
                    dest_dir,
                    dest_was_empty,
                    newly_created,
                )?;
                let was_preexisting = if preexisting_paths.contains(&target_key) {
                    super::io::check_existing_target_safe(&target_path, false)?;
                    true
                } else {
                    super::io::check_existing_target_safe(&target_path, dest_was_empty)?
                };
                let entry_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let src_path = entry.path();
                if safe_atomic_rename(&src_path, &target_path).is_err() {
                    std::fs::copy(&src_path, &target_path).map_err(|e| {
                        format!(
                            "Failed moving extracted file '{}': {}",
                            target_path.display(),
                            e
                        )
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
    excluded: Option<&HashSet<String>>,
    preexisting_paths: &HashSet<String>,
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
    excluded: Option<&HashSet<String>>,
) -> Result<Vec<String>, String> {
    let _priority_guard = ScopedPriorityGuard::new();
    if is_extraction_cancelled() {
        return Err("Extraction cancelled by user".into());
    }

    let dest_was_empty = !dest_dir.exists()
        || std::fs::read_dir(dest_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);

    let inspection =
        inspect_archive_with_system_tar(archive_path, dest_dir, dest_was_empty, excluded)?;

    let has_exclusions = excluded.is_some_and(|set| !set.is_empty());
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    if has_exclusions && inspection.total_entries > 0 && inspection.included_entries == 0 {
        return Ok(Vec::new());
    }

    let archive_size = std::fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);
    let total_expected_bytes = if inspection.total_uncompressed_bytes > 0 {
        inspection.total_uncompressed_bytes
    } else {
        archive_size.max(1)
    };

    // Isolate excluded or unparseable-listing extractions in a same-volume staging folder so bsdtar
    // case/glob differences or unknown entry types can never bypass symlink/reparse guards.
    let use_staging = has_exclusions || inspection.had_unparseable;
    let mut staging_guard = StagingDirGuard(None);
    let extract_target: PathBuf = if use_staging {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut idx = 0u32;
        let stage_path = loop {
            let candidate =
                dest_dir.join(format!(".bayzip_tar_stage_{}_{}_{}", std::process::id(), stamp, idx));
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
    cmd.env("LC_ALL", "C");
    cmd.env("LANG", "C");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(crate::priority::get_child_process_creation_flags());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("System extraction error: {}", e))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture tar.exe stderr".to_string())?;

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let reader_handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx.send(std::mem::take(&mut line)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut extracted_files = Vec::new();
    let mut newly_created = Vec::new();
    let mut current_file_name = String::new();
    let mut current_file_path: Option<PathBuf> = None;
    let mut current_file_rel_key: Option<String> = None;
    let mut completed_bytes: u64 = 0;
    let mut last_emit = Instant::now();
    let mut error_lines = Vec::new();

    let lookup_completed_size = |prev_path: &Path, prev_rel_key: Option<&str>| -> u64 {
        if let Some(key) = prev_rel_key {
            if let Some(&sz) = inspection.entry_sizes.get(key) {
                return sz;
            }
        }
        std::fs::metadata(prev_path).map(|m| m.len()).unwrap_or(0)
    };

    let record_direct_entry = |rel_path: &str,
                                   current_path_slot: &mut Option<PathBuf>,
                                   current_rel_slot: &mut Option<String>,
                                   current_name_slot: &mut String,
                                   completed_total: &mut u64,
                                   newly: &mut Vec<String>,
                                   extracted: &mut Vec<String>| {
        let clean_rel = rel_path.replace('\\', "/");
        if let Some(prev_path) = current_path_slot.take() {
            let prev_key = current_rel_slot.take();
            *completed_total = completed_total
                .saturating_add(lookup_completed_size(&prev_path, prev_key.as_deref()));
        } else {
            *current_rel_slot = None;
        }

        let Some(safe_path) = sanitize_archive_path(&clean_rel) else {
            *current_name_slot = clean_rel;
            return;
        };
        let out_path = extract_target.join(&safe_path);
        *current_name_slot = safe_path.to_string_lossy().to_string();
        *current_rel_slot = (!clean_rel.ends_with('/')).then(|| normalize_fs_key(&safe_path));
        *current_path_slot = Some(out_path.clone());

        if !use_staging {
            let norm_key = normalize_fs_key(&out_path);
            let was_preexisting = inspection.preexisting_paths.contains(&norm_key);
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
                    &mut current_file_rel_key,
                    &mut current_file_name,
                    &mut completed_bytes,
                    &mut newly_created,
                    &mut extracted_files,
                );
            } else if trimmed.to_lowercase().contains("error")
                || trimmed.to_lowercase().contains("fatal")
            {
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
                            &mut current_file_rel_key,
                            &mut current_file_name,
                            &mut completed_bytes,
                            &mut newly_created,
                            &mut extracted_files,
                        );
                    } else if trimmed.to_lowercase().contains("error")
                        || trimmed.to_lowercase().contains("fatal")
                    {
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
                let active_file_size = current_file_path
                    .as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())
                    .unwrap_or(0);

                let current_total = completed_bytes.saturating_add(active_file_size);
                emit_extract_progress(
                    app,
                    &mut last_emit,
                    &current_file_name,
                    current_total,
                    total_expected_bytes,
                );

                std::thread::sleep(Duration::from_millis(50));
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

    if use_staging {
        match finalize_staged_extraction_inner(
            &extract_target,
            dest_dir,
            excluded,
            &inspection.preexisting_paths,
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
            let prev_key = current_file_rel_key.take();
            completed_bytes = completed_bytes
                .saturating_add(lookup_completed_size(&prev_path, prev_key.as_deref()));
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
    use crate::extractor::ExtractionTaskGuard;

    #[test]
    fn test_finalize_staged_extraction_preserves_preexisting_and_filters_case_glob_segments() {
        let base_dir = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_staged_tar_filter");
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
        let mut preexisting = HashSet::new();
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

    #[test]
    fn test_parse_bsdtar_tvf_line_formats_spaces_and_security() {
        // 1. Standard Mon DD HH:MM
        assert_eq!(
            parse_bsdtar_tvf_line("-rw-r--r--  0 0      0        1234 Nov 20 01:28 docs/hello.txt"),
            Ok(TvfLineResult::Entry {
                raw_name: "docs/hello.txt",
                size: 1234,
                is_dir: false,
            })
        );

        // 2. Mon DD YYYY + space in owner + consecutive spaces inside filename
        assert_eq!(
            parse_bsdtar_tvf_line(
                "-rw-r--r--  0 NT AUTHORITY system   512 Nov  5  2024 docs/my  cool  file.txt"
            ),
            Ok(TvfLineResult::Entry {
                raw_name: "docs/my  cool  file.txt",
                size: 512,
                is_dir: false,
            })
        );

        // 3. Directory entry (size forced to 0) + DD Mon HH:MM format
        assert_eq!(
            parse_bsdtar_tvf_line("drwxr-xr-x  0 staff  20       4096 20 Nov 01:28 assets/"),
            Ok(TvfLineResult::Entry {
                raw_name: "assets/",
                size: 0,
                is_dir: true,
            })
        );

        // 4. ISO YYYY-MM-DD HH:MM + trailing space preserved for downstream sanitize_archive_path
        assert_eq!(
            parse_bsdtar_tvf_line("-rw-r--r--  0 0      0          42 2026-11-20 01:28 evil.exe \r\n"),
            Ok(TvfLineResult::Entry {
                raw_name: "evil.exe ",
                size: 42,
                is_dir: false,
            })
        );

        // 5. Numeric uid/gid collision resilience ("staff 20" must not match as month/day + size 2048 as year)
        assert_eq!(
            parse_bsdtar_tvf_line("-rw-r--r--  1 0      staff  20   2048 Nov 20 01:28 real.bin"),
            Ok(TvfLineResult::Entry {
                raw_name: "real.bin",
                size: 2048,
                is_dir: false,
            })
        );

        // 6. Security fail-closed rejections for symlinks, hardlinks, FIFOs, and char devices
        assert!(
            parse_bsdtar_tvf_line("lrwxrwxrwx  0 0      0           0 Nov 20 01:28 bad -> /etc/passwd")
                .unwrap_err()
                .contains("Security violation")
        );
        assert!(
            parse_bsdtar_tvf_line("hrw-r--r--  0 0      0           0 Nov 20 01:28 hard link to real.bin")
                .unwrap_err()
                .contains("Security violation")
        );
        assert!(
            parse_bsdtar_tvf_line("prw-r--r--  0 0      0           0 Nov 20 01:28 pipe_node")
                .unwrap_err()
                .contains("Security violation")
        );
        assert!(
            parse_bsdtar_tvf_line("crw-r--r--  0 0      0         1,2 Nov 20 01:28 char_node")
                .unwrap_err()
                .contains("Security violation")
        );
    }

    #[test]
    fn test_extract_with_system_tar_single_pass_roundtrip_exclusions_and_security() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_system_tar_e2e");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let tar_path = base.join("bundle.tar");
        let append_file = |builder: &mut tar::Builder<Vec<u8>>, name: &str, data: &[u8]| {
            let mut hdr = tar::Header::new_gnu();
            hdr.set_size(data.len() as u64);
            hdr.set_mode(0o644);
            hdr.set_cksum();
            builder.append_data(&mut hdr, name, data).unwrap();
        };

        {
            let mut builder = tar::Builder::new(Vec::new());
            append_file(&mut builder, "docs/my  spaced  file.txt", b"new-spaced-content");
            append_file(&mut builder, "app/main.txt", b"main-payload-bytes");
            append_file(&mut builder, "excluded/secret.txt", b"archive-secret-ignored");
            std::fs::write(&tar_path, builder.into_inner().unwrap()).unwrap();
        }

        // A. Direct extraction without exclusions (verifies single-pass inspection + bytes_extracted)
        let dest_direct = base.join("out_direct");
        let guard = ExtractionTaskGuard::new();
        let files_direct = extract_with_system_tar(&tar_path, &dest_direct, None, None).unwrap();
        assert!(!files_direct.is_empty());
        assert_eq!(
            std::fs::read_to_string(dest_direct.join("docs/my  spaced  file.txt")).unwrap(),
            "new-spaced-content"
        );
        assert_eq!(
            std::fs::read_to_string(dest_direct.join("app/main.txt")).unwrap(),
            "main-payload-bytes"
        );
        assert_eq!(
            std::fs::read_to_string(dest_direct.join("excluded/secret.txt")).unwrap(),
            "archive-secret-ignored"
        );
        assert_eq!(
            guard.bytes_extracted(),
            (18 + 18 + 22) as u64
        );
        drop(guard);

        // B. Staged extraction with exclusions + preexisting file overwrite & preexisting excluded protection
        let dest_staged = base.join("out_staged");
        std::fs::create_dir_all(dest_staged.join("app")).unwrap();
        std::fs::create_dir_all(dest_staged.join("excluded")).unwrap();
        std::fs::write(dest_staged.join("app/main.txt"), b"old-main").unwrap();
        std::fs::write(dest_staged.join("excluded/secret.txt"), b"keep-user-secret").unwrap();

        let excluded = crate::inspector::build_excluded_set(["excluded"]);
        let guard_staged = ExtractionTaskGuard::new();
        let files_staged =
            extract_with_system_tar(&tar_path, &dest_staged, None, excluded.as_ref()).unwrap();
        assert_eq!(files_staged.len(), 2);
        assert_eq!(
            std::fs::read_to_string(dest_staged.join("docs/my  spaced  file.txt")).unwrap(),
            "new-spaced-content"
        );
        assert_eq!(
            std::fs::read_to_string(dest_staged.join("app/main.txt")).unwrap(),
            "main-payload-bytes"
        );
        assert_eq!(
            std::fs::read_to_string(dest_staged.join("excluded/secret.txt")).unwrap(),
            "keep-user-secret"
        );
        assert_eq!(guard_staged.bytes_extracted(), 36);
        assert!(
            std::fs::read_dir(&dest_staged)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with(".bayzip_tar_stage_"))
        );

        // C. Reject symlink entry via -tvf single-pass inspection before any write
        let symlink_tar = base.join("symlink.tar");
        {
            let mut builder = tar::Builder::new(Vec::new());
            let mut sym = tar::Header::new_gnu();
            sym.set_entry_type(tar::EntryType::Symlink);
            sym.set_size(0);
            sym.set_mode(0o777);
            builder.append_link(&mut sym, "bad_link", "target.txt").unwrap();
            std::fs::write(&symlink_tar, builder.into_inner().unwrap()).unwrap();
        }
        let dest_sym = base.join("out_sym");
        let sym_err = extract_with_system_tar(&symlink_tar, &dest_sym, None, None).unwrap_err();
        assert!(sym_err.contains("Security violation"));
        assert!(!dest_sym.exists());

        // D. Reject Zip-Slip path traversal entry via single-pass inspection before any write
        let slip_tar = base.join("slip.tar");
        {
            let mut builder = tar::Builder::new(Vec::new());
            let mut hdr = tar::Header::new_old();
            let slip_name = b"../escape.txt";
            hdr.as_old_mut().name[..slip_name.len()].copy_from_slice(slip_name);
            hdr.set_entry_type(tar::EntryType::Regular);
            hdr.set_size(5);
            hdr.set_mode(0o644);
            hdr.set_cksum();
            builder.append(&hdr, &b"pwned"[..]).unwrap();
            std::fs::write(&slip_tar, builder.into_inner().unwrap()).unwrap();
        }
        let dest_slip = base.join("out_slip");
        let slip_err = extract_with_system_tar(&slip_tar, &dest_slip, None, None).unwrap_err();
        assert!(slip_err.contains("Security violation"));
        assert!(!base.join("escape.txt").exists());
        assert!(!dest_slip.exists());

        let _ = std::fs::remove_dir_all(&base);
    }
}

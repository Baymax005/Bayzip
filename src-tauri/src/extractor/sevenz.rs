use std::collections::{HashMap, HashSet};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tauri::AppHandle;
#[cfg(not(test))]
use tauri::Emitter;
use sevenz_rust2::{Archive, BlockDecoder, EncoderMethod, Error as SevenzError, Password};

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
    AsyncFileWriterPool,
    DirectoryCache,
    ASYNC_INMEMORY_MAX_BYTES,
};
use super::utils::{
    base_folder_stem,
    cleanup_extracted_files,
    emit_extract_progress,
    is_path_excluded,
    strip_root_component,
};
use super::system_tar::extract_with_system_tar;

const SEVENZ_IO_BUF_SIZE: usize = 256 * 1024;
const SEVENZ_MAX_CONCURRENT_DICT_BYTES: u64 = 512 * 1024 * 1024;

struct Planned7zFile {
    safe_path: PathBuf,
    out_path: PathBuf,
    file_existed: bool,
}

enum SevenzFailure {
    LocalIo(String),
    Archive(SevenzError),
}

fn normalize_7z_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn estimate_block_max_dict_bytes(archive: &Archive) -> u64 {
    let mut max_dict = 0u64;
    for block in &archive.blocks {
        for coder in &block.coders {
            let id = coder.encoder_method_id();
            if id == EncoderMethod::ID_LZMA2 {
                if let Some(&prop) = coder.properties().first() {
                    let p = prop & 0x3F;
                    let dic = if p == 40 {
                        0xFFFF_FFFFu64
                    } else if p < 40 {
                        ((2 | (p & 1)) as u64) << ((p / 2) + 11)
                    } else {
                        0
                    };
                    max_dict = max_dict.max(dic);
                }
            } else if id == EncoderMethod::ID_LZMA {
                let props = coder.properties();
                if props.len() >= 5 {
                    let dic = u32::from_le_bytes([props[1], props[2], props[3], props[4]]) as u64;
                    max_dict = max_dict.max(dic);
                }
            }
        }
    }
    max_dict
}

/// Computes optimal `(block_workers, lzma2_threads)` for `.7z` extraction based on
/// storage bus, seek penalty, block count, LZMA/LZMA2 dictionary RAM budget, and cross-block path safety.
pub fn calculate_7z_parallelism(
    bus: DriveBusKind,
    incurs_seek_penalty: bool,
    total_cores: usize,
    block_count: usize,
    max_dict_bytes: u64,
    has_cross_block_collision: bool,
) -> (usize, u32) {
    let cores = total_cores.max(1);
    if block_count <= 1 || has_cross_block_collision {
        return (1, cores.clamp(1, 8) as u32);
    }

    let mut block_workers = match bus {
        DriveBusKind::Nvme => cores.clamp(1, 8).min(block_count),
        DriveBusKind::Sata if !incurs_seek_penalty => cores.clamp(1, 6).min(block_count),
        DriveBusKind::Sata | DriveBusKind::UsbRemovable | DriveBusKind::Unknown => 1,
    };

    if let Some(mem_cap_u64) = SEVENZ_MAX_CONCURRENT_DICT_BYTES.checked_div(max_dict_bytes) {
        let mem_cap = mem_cap_u64.max(1) as usize;
        block_workers = block_workers.min(mem_cap).max(1);
    }

    let lzma2_threads = (cores / block_workers).clamp(1, 4) as u32;
    (block_workers, lzma2_threads)
}

fn is_7z_cancelled(cancel_token: Option<&TaskCancelToken>, aborted: Option<&AtomicBool>) -> bool {
    if let Some(flag) = aborted {
        if flag.load(Ordering::Acquire) {
            return true;
        }
    }
    match cancel_token {
        Some(token) => token.is_cancelled(),
        None => is_extraction_cancelled(),
    }
}

/// Drains remaining bytes of a skipped entry inside a solid `.7z` block so subsequent
/// entries remain aligned and full-block CRC32 verification succeeds.
fn drain_7z_entry(
    reader: &mut dyn Read,
    io_buf: &mut [u8],
    cancel_token: Option<&TaskCancelToken>,
    aborted: Option<&AtomicBool>,
) -> Result<(), SevenzError> {
    loop {
        if is_7z_cancelled(cancel_token, aborted) {
            return Err(SevenzError::from(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Extraction cancelled by user",
            )));
        }
        match reader.read(io_buf) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(e) => return Err(SevenzError::from(e)),
        }
    }
}

fn compute_7z_root_strip(archive: &Archive, dest_dir: &Path) -> Option<String> {
    let dest_stem = dest_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(base_folder_stem)?;
    let mut candidate_root: Option<String> = None;
    let mut is_single = true;

    for f in &archive.files {
        let clean = f
            .name()
            .trim()
            .trim_start_matches(['/', '\\'])
            .trim_start_matches("./");
        if clean.is_empty() || clean == "." {
            continue;
        }
        if !clean.contains('/')
            && !clean.contains('\\')
            && !f.is_directory()
            && !f.name().ends_with('/')
        {
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

#[allow(clippy::too_many_arguments)]
fn stream_7z_entry_to_disk(
    out_path: &Path,
    safe_path: &Path,
    entry_reader: &mut dyn Read,
    file_size: u64,
    effective_existed: bool,
    chunk_size: usize,
    io_buf: &mut [u8],
    cancel_token: Option<&TaskCancelToken>,
    aborted: Option<&AtomicBool>,
    mut on_chunk: impl FnMut(u64),
) -> Result<(), SevenzFailure> {
    let target_write_path: PathBuf = if effective_existed {
        let mut p = out_path.as_os_str().to_os_string();
        p.push(".baypart");
        PathBuf::from(p)
    } else {
        out_path.to_path_buf()
    };

    let out_file = match create_output_file_fast(&target_write_path, file_size) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            clear_readonly(&target_write_path);
            create_output_file_fast(&target_write_path, file_size).map_err(|e2| {
                SevenzFailure::LocalIo(format!(
                    "Failed to create output file {:?}: {}",
                    target_write_path, e2
                ))
            })?
        }
        Err(e) => {
            return Err(SevenzFailure::LocalIo(format!(
                "Failed to create output file {:?}: {}",
                target_write_path, e
            )));
        }
    };

    let mut writer = BufWriter::with_capacity(chunk_size, out_file);
    let mut remaining = file_size;

    while remaining > 0 {
        if is_7z_cancelled(cancel_token, aborted) {
            drop(writer);
            let _ = std::fs::remove_file(&target_write_path);
            return Err(SevenzFailure::Archive(SevenzError::from(
                std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Extraction cancelled by user",
                ),
            )));
        }
        let to_read = (remaining as usize).min(io_buf.len());
        match entry_reader.read(&mut io_buf[..to_read]) {
            Ok(0) => {
                drop(writer);
                let _ = std::fs::remove_file(&target_write_path);
                return Err(SevenzFailure::Archive(SevenzError::from(
                    std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        format!("Premature EOF inside 7z entry '{}'", safe_path.display()),
                    ),
                )));
            }
            Ok(n) => {
                if let Err(e) = writer.write_all(&io_buf[..n]) {
                    drop(writer);
                    let _ = std::fs::remove_file(&target_write_path);
                    return Err(SevenzFailure::LocalIo(format!(
                        "Error writing extracted file {:?}: {}",
                        target_write_path, e
                    )));
                }
                remaining -= n as u64;
                on_chunk(n as u64);
            }
            Err(e) => {
                drop(writer);
                let _ = std::fs::remove_file(&target_write_path);
                return Err(SevenzFailure::Archive(SevenzError::from(e)));
            }
        }
    }

    if let Err(e) = writer.flush() {
        drop(writer);
        let _ = std::fs::remove_file(&target_write_path);
        return Err(SevenzFailure::LocalIo(format!("Flush error: {}", e)));
    }
    drop(writer);

    if effective_existed {
        if let Err(e) = safe_atomic_rename(&target_write_path, out_path) {
            let _ = std::fs::remove_file(&target_write_path);
            return Err(SevenzFailure::LocalIo(format!(
                "Failed to finalize extracted file {:?}: {}",
                out_path, e
            )));
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_emit_7z_progress(
    app: Option<&AppHandle>,
    last_emit: &Mutex<Instant>,
    current_file: &Path,
    total_extracted: &AtomicU64,
    total_expected_bytes: u64,
    processed_count: &AtomicUsize,
    total_files: usize,
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
            } else if total_files > 0 {
                ((done as f64 / total_files as f64) * 100.0).min(99.0)
            } else {
                50.0
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
        total_files,
        force,
    );
}

fn is_cancelled_sevenz_error(err: &SevenzError) -> bool {
    match err {
        SevenzError::MaybeBadPassword(e) | SevenzError::Io(e, _)
            if e.kind() == std::io::ErrorKind::Interrupted =>
        {
            true
        }
        _ => {
            let lower = err.to_string().to_lowercase();
            lower.contains("cancelled") || lower.contains("canceled")
        }
    }
}

fn map_sevenz_failure(
    failure: SevenzFailure,
    archive_path: &Path,
    dest_dir: &Path,
    pwd_opt: Option<&str>,
    archive_has_aes: bool,
    app: Option<&AppHandle>,
    excluded: Option<&HashSet<String>>,
) -> Result<Vec<String>, String> {
    match failure {
        SevenzFailure::LocalIo(msg) => Err(msg),
        SevenzFailure::Archive(e) => {
            if is_extraction_cancelled() || is_cancelled_sevenz_error(&e) {
                return Err("Extraction cancelled by user".into());
            }
            if matches!(e, SevenzError::PasswordRequired) {
                return Err(
                    "Password required: This 7z archive is encrypted with AES-256.".into(),
                );
            }
            let err_lower = e.to_string().to_lowercase();
            if matches!(e, SevenzError::MaybeBadPassword(_))
                || err_lower.contains("password")
                || archive_has_aes
                || crate::inspector::is_archive_encrypted(archive_path)
            {
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

pub fn extract_7z_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&HashSet<String>>,
) -> Result<Vec<String>, String> {
    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let pwd = pwd_opt.map(Password::from).unwrap_or_else(Password::empty);

    let raw_file = open_sequential_read(archive_path)
        .map_err(|e| format!("Failed to open 7z: {}", e))?;
    let read_cap = get_optimal_extract_chunk_size(archive_path).max(256 * 1024);
    let mut reader = BufReader::with_capacity(read_cap, raw_file);

    let archive = match Archive::read(&mut reader, &pwd) {
        Ok(a) => a,
        Err(SevenzError::PasswordRequired) => {
            return Err("Password required: This 7z archive is encrypted with AES-256.".into());
        }
        Err(SevenzError::MaybeBadPassword(_)) => {
            return Err(if pwd_opt.is_none() {
                "Password required: This 7z archive is encrypted with AES-256.".into()
            } else {
                "Incorrect password: Could not decrypt 7z archive.".into()
            });
        }
        Err(e) => {
            if is_extraction_cancelled() || is_cancelled_sevenz_error(&e) {
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

    let archive_has_aes = archive.blocks.iter().any(|b| {
        b.coders
            .iter()
            .any(|c| c.encoder_method_id() == EncoderMethod::ID_AES256_SHA256)
    });
    if archive_has_aes && pwd_opt.is_none() {
        return Err("Password required: This 7z archive is encrypted with AES-256.".into());
    }

    let total_files = archive.files.len();
    let total_expected_bytes: u64 = if let Some(sz) = expected_size.filter(|&s| s > 0) {
        sz
    } else {
        let sum: u64 = archive.files.iter().map(|f| f.size()).sum();
        if sum > 0 {
            sum
        } else {
            std::fs::metadata(archive_path)
                .map(|m| m.len() * 2)
                .unwrap_or(1)
        }
    };

    let dest_was_empty = !dest_dir.exists()
        || std::fs::read_dir(dest_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create dest: {}", e))?;

    let mut dir_cache = DirectoryCache::new();
    dir_cache.mark_existing(dest_dir);

    let root_strip = compute_7z_root_strip(&archive, dest_dir);
    let mut newly_created: Vec<String> = Vec::new();
    let mut planned: Vec<Option<Planned7zFile>> = Vec::with_capacity(archive.files.len());
    let mut block_has_wanted = vec![false; archive.blocks.len()];
    let mut empty_files: Vec<Planned7zFile> = Vec::new();
    let mut seen_file_blocks: HashMap<String, Option<usize>> = HashMap::new();
    let mut has_cross_block_collision = false;

    let plan_res = (|| -> Result<(), String> {
        for (file_index, entry) in archive.files.iter().enumerate() {
            let raw_name = entry.name();
            if entry.is_anti_item() || is_path_excluded(raw_name, excluded) {
                planned.push(None);
                continue;
            }
            let Some(safe_path) = sanitize_archive_path(raw_name) else {
                planned.push(None);
                continue;
            };

            let effective_path = match root_strip {
                Some(ref prefix) => match strip_root_component(&safe_path, prefix) {
                    Some(stripped) => {
                        if stripped.as_os_str().is_empty() {
                            planned.push(None);
                            continue;
                        }
                        stripped.to_path_buf()
                    }
                    None => safe_path.clone(),
                },
                None => safe_path.clone(),
            };

            let out_path = dest_dir.join(&effective_path);
            let is_dir = entry.is_directory() || raw_name.replace('\\', "/").ends_with('/');

            if is_dir {
                dir_cache.ensure_dir_tracked(&out_path, dest_dir, dest_was_empty, &mut newly_created)?;
                planned.push(None);
                continue;
            }

            dir_cache.ensure_parent_tracked(&out_path, dest_dir, dest_was_empty, &mut newly_created)?;
            let file_existed =
                super::io::check_existing_target_safe(&out_path, dest_was_empty)?;

            let block_idx = archive
                .stream_map
                .file_block_index
                .get(file_index)
                .copied()
                .flatten();

            let key = normalize_7z_path_key(&out_path);
            if let Some(&prev_block) = seen_file_blocks.get(&key) {
                if prev_block != block_idx {
                    has_cross_block_collision = true;
                }
            } else {
                seen_file_blocks.insert(key, block_idx);
            }

            let target = Planned7zFile {
                safe_path,
                out_path,
                file_existed,
            };

            if let Some(b_idx) = block_idx {
                if b_idx < block_has_wanted.len() {
                    block_has_wanted[b_idx] = true;
                }
                planned.push(Some(target));
            } else {
                empty_files.push(target);
                planned.push(None);
            }
        }
        Ok(())
    })();

    if let Err(e) = plan_res {
        cleanup_extracted_files(&newly_created);
        return Err(e);
    }

    let wanted_blocks: Vec<usize> = block_has_wanted
        .iter()
        .enumerate()
        .filter_map(|(idx, &wanted)| wanted.then_some(idx))
        .collect();

    let total_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let max_dict_bytes = estimate_block_max_dict_bytes(&archive);
    let (block_workers, lzma2_threads) = calculate_7z_parallelism(
        get_drive_bus_kind(dest_dir),
        get_drive_seek_penalty(dest_dir),
        total_cores,
        wanted_blocks.len(),
        max_dict_bytes,
        has_cross_block_collision,
    );

    let chunk_size = get_optimal_extract_chunk_size(dest_dir);

    if block_workers <= 1 {
        let mut extracted: Vec<String> = Vec::new();
        let mut writer_pool = AsyncFileWriterPool::new_for_dest(dest_dir);
        let mut io_buf = vec![0u8; SEVENZ_IO_BUF_SIZE];
        let mut last_emit = Instant::now();
        let mut total_extracted: u64 = 0;
        let mut local_failure: Option<SevenzFailure> = None;

        let mut block_err: Option<SevenzError> = None;
        for &block_index in &wanted_blocks {
            let start_idx = archive.stream_map.block_first_file_index[block_index];
            let mut cursor = start_idx;

            let decoder = BlockDecoder::new(lzma2_threads, block_index, &archive, &pwd, &mut reader);
            let res = decoder.for_each_entries(&mut |entry, entry_reader| {
                let file_index = cursor;
                cursor += 1;

                if is_extraction_cancelled() {
                    return Err(SevenzError::from(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Extraction cancelled by user",
                    )));
                }

                let Some(target) = planned.get(file_index).and_then(|opt| opt.as_ref()) else {
                    if entry.has_stream() && entry.size() > 0 {
                        drain_7z_entry(entry_reader, &mut io_buf, None, None)?;
                    }
                    return Ok(true);
                };

                let file_size = entry.size();
                let out_str = target.out_path.to_string_lossy().to_string();

                if file_size <= ASYNC_INMEMORY_MAX_BYTES {
                    let sz = file_size as usize;
                    let mut payload = vec![0u8; sz];
                    let mut read_off = 0usize;
                    while read_off < sz {
                        if is_extraction_cancelled() {
                            return Err(SevenzError::from(std::io::Error::new(
                                std::io::ErrorKind::Interrupted,
                                "Extraction cancelled by user",
                            )));
                        }
                        let slice_end = (read_off + SEVENZ_IO_BUF_SIZE).min(sz);
                        match entry_reader.read(&mut payload[read_off..slice_end]) {
                            Ok(0) => {
                                return Err(SevenzError::from(std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    format!(
                                        "Premature EOF inside 7z entry '{}'",
                                        target.safe_path.display()
                                    ),
                                )));
                            }
                            Ok(n) => read_off += n,
                            Err(e) => return Err(SevenzError::from(e)),
                        }
                    }

                    if !target.file_existed {
                        newly_created.push(out_str.clone());
                    }
                    if let Err(e) =
                        writer_pool.submit(target.out_path.clone(), payload, target.file_existed)
                    {
                        local_failure = Some(SevenzFailure::LocalIo(e));
                        return Err(SevenzError::Other("writer_pool_error".into()));
                    }
                    total_extracted = total_extracted.saturating_add(file_size);
                    emit_extract_progress(
                        app,
                        &mut last_emit,
                        &target.safe_path.to_string_lossy(),
                        total_extracted,
                        total_expected_bytes,
                    );
                } else {
                    let seen_before = match writer_pool.prepare_direct_write(&target.out_path) {
                        Ok(seen) => seen,
                        Err(e) => {
                            local_failure = Some(SevenzFailure::LocalIo(e));
                            return Err(SevenzError::Other("writer_pool_error".into()));
                        }
                    };
                    let effective_existed = target.file_existed || seen_before;
                    if !effective_existed {
                        newly_created.push(out_str.clone());
                    }

                    if let Err(failure) = stream_7z_entry_to_disk(
                        &target.out_path,
                        &target.safe_path,
                        entry_reader,
                        file_size,
                        effective_existed,
                        chunk_size,
                        &mut io_buf,
                        None,
                        None,
                        |delta| {
                            total_extracted = total_extracted.saturating_add(delta);
                            emit_extract_progress(
                                app,
                                &mut last_emit,
                                &target.safe_path.to_string_lossy(),
                                total_extracted,
                                total_expected_bytes,
                            );
                        },
                    ) {
                        match failure {
                            SevenzFailure::Archive(se) => return Err(se),
                            local @ SevenzFailure::LocalIo(_) => {
                                local_failure = Some(local);
                                return Err(SevenzError::Other("local_io_error".into()));
                            }
                        }
                    }
                }

                extracted.push(out_str);
                Ok(true)
            });

            if let Err(e) = res {
                block_err = Some(e);
                break;
            }
        }

        if let Some(failure) = local_failure.take().or_else(|| block_err.map(SevenzFailure::Archive)) {
            writer_pool.abort_and_join();
            cleanup_extracted_files(&newly_created);
            return map_sevenz_failure(
                failure,
                archive_path,
                dest_dir,
                pwd_opt,
                archive_has_aes,
                app,
                excluded,
            );
        }

        for target in &empty_files {
            if is_extraction_cancelled() {
                writer_pool.abort_and_join();
                cleanup_extracted_files(&newly_created);
                return Err("Extraction cancelled by user".into());
            }
            let out_str = target.out_path.to_string_lossy().to_string();
            if !target.file_existed {
                newly_created.push(out_str.clone());
            }
            if let Err(e) = writer_pool.submit(target.out_path.clone(), Vec::new(), target.file_existed)
            {
                writer_pool.abort_and_join();
                cleanup_extracted_files(&newly_created);
                return Err(e);
            }
            extracted.push(out_str);
        }

        if let Err(e) = writer_pool.finish() {
            cleanup_extracted_files(&newly_created);
            return Err(e);
        }

        super::record_extracted_bytes(total_extracted);
        return Ok(extracted);
    }

    // Mode B: Multi-block LPT parallel extraction (SSD / NVMe, zero cross-block path collisions)
    drop(reader);

    let mut work_blocks = wanted_blocks;
    work_blocks.sort_by(|&a, &b| {
        archive.blocks[b]
            .get_unpack_size()
            .cmp(&archive.blocks[a].get_unpack_size())
    });

    let next_job = AtomicUsize::new(0);
    let aborted = AtomicBool::new(false);
    let first_error: Mutex<Option<SevenzFailure>> = Mutex::new(None);
    let total_extracted = AtomicU64::new(0);
    let processed_count = AtomicUsize::new(0);
    let last_emit = Mutex::new(Instant::now());
    let cancel_token = current_extraction_token();

    let worker_results: Vec<(Vec<String>, Vec<String>)> = std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(block_workers);
        for _ in 0..block_workers {
            handles.push(s.spawn(|| -> (Vec<String>, Vec<String>) {
                let _priority_guard = ScopedPriorityGuard::new();
                let mut local_extracted = Vec::new();
                let mut local_newly_created = Vec::new();
                let mut local_seen: HashSet<String> = HashSet::new();
                let mut io_buf = vec![0u8; SEVENZ_IO_BUF_SIZE];

                let raw_file = match open_random_read(archive_path) {
                    Ok(f) => f,
                    Err(e) => {
                        let mut guard = first_error.lock().unwrap_or_else(|err| err.into_inner());
                        if guard.is_none() {
                            *guard = Some(SevenzFailure::LocalIo(format!(
                                "Failed to open 7z archive in worker: {}",
                                e
                            )));
                        }
                        aborted.store(true, Ordering::Release);
                        return (local_extracted, local_newly_created);
                    }
                };
                let mut worker_reader = BufReader::with_capacity(SEVENZ_IO_BUF_SIZE, raw_file);

                loop {
                    if is_7z_cancelled(cancel_token.as_ref(), Some(&aborted)) {
                        aborted.store(true, Ordering::Release);
                        break;
                    }
                    let pos = next_job.fetch_add(1, Ordering::Relaxed);
                    if pos >= work_blocks.len() {
                        break;
                    }
                    let block_index = work_blocks[pos];
                    let start_idx = archive.stream_map.block_first_file_index[block_index];
                    let mut cursor = start_idx;
                    let mut local_failure: Option<SevenzFailure> = None;

                    let decoder = BlockDecoder::new(
                        lzma2_threads,
                        block_index,
                        &archive,
                        &pwd,
                        &mut worker_reader,
                    );

                    let res = decoder.for_each_entries(&mut |entry, entry_reader| {
                        let file_index = cursor;
                        cursor += 1;

                        if is_7z_cancelled(cancel_token.as_ref(), Some(&aborted)) {
                            aborted.store(true, Ordering::Release);
                            return Err(SevenzError::from(std::io::Error::new(
                                std::io::ErrorKind::Interrupted,
                                "Extraction cancelled by user",
                            )));
                        }

                        let Some(target) = planned.get(file_index).and_then(|opt| opt.as_ref()) else {
                            if entry.has_stream() && entry.size() > 0 {
                                drain_7z_entry(
                                    entry_reader,
                                    &mut io_buf,
                                    cancel_token.as_ref(),
                                    Some(&aborted),
                                )?;
                            }
                            return Ok(true);
                        };

                        let key = normalize_7z_path_key(&target.out_path);
                        let effective_existed =
                            target.file_existed || !local_seen.insert(key);
                        let out_str = target.out_path.to_string_lossy().to_string();
                        if !effective_existed {
                            local_newly_created.push(out_str.clone());
                        }

                        let file_size = entry.size();
                        if file_size <= io_buf.len() as u64 {
                            let sz = file_size as usize;
                            if sz > 0 {
                                if let Err(e) = entry_reader.read_exact(&mut io_buf[..sz]) {
                                    return Err(SevenzError::from(e));
                                }
                            }
                            if let Err(msg) = write_file_payload(
                                &target.out_path,
                                &io_buf[..sz],
                                effective_existed,
                            ) {
                                local_failure = Some(SevenzFailure::LocalIo(msg));
                                return Err(SevenzError::Other("local_io_error".into()));
                            }
                            if sz > 0 {
                                total_extracted.fetch_add(file_size, Ordering::Relaxed);
                            }
                        } else if let Err(failure) = stream_7z_entry_to_disk(
                            &target.out_path,
                            &target.safe_path,
                            entry_reader,
                            file_size,
                            effective_existed,
                            chunk_size,
                            &mut io_buf,
                            cancel_token.as_ref(),
                            Some(&aborted),
                            |delta| {
                                total_extracted.fetch_add(delta, Ordering::Relaxed);
                                maybe_emit_7z_progress(
                                    app,
                                    &last_emit,
                                    &target.safe_path,
                                    &total_extracted,
                                    total_expected_bytes,
                                    &processed_count,
                                    total_files,
                                    false,
                                );
                            },
                        ) {
                            match failure {
                                SevenzFailure::Archive(se) => return Err(se),
                                local @ SevenzFailure::LocalIo(_) => {
                                    local_failure = Some(local);
                                    return Err(SevenzError::Other("local_io_error".into()));
                                }
                            }
                        }

                        local_extracted.push(out_str);
                        let done = processed_count.fetch_add(1, Ordering::Relaxed) + 1;
                        maybe_emit_7z_progress(
                            app,
                            &last_emit,
                            &target.safe_path,
                            &total_extracted,
                            total_expected_bytes,
                            &processed_count,
                            total_files,
                            done == total_files,
                        );
                        Ok(true)
                    });

                    if let Some(failure) = local_failure.take().or_else(|| res.err().map(SevenzFailure::Archive)) {
                        let mut guard = first_error.lock().unwrap_or_else(|err| err.into_inner());
                        if guard.is_none() {
                            *guard = Some(failure);
                        }
                        aborted.store(true, Ordering::Release);
                        break;
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

    let mut extracted = Vec::with_capacity(archive.files.len());
    for (local_ext, local_new) in worker_results {
        newly_created.extend(local_new);
        extracted.extend(local_ext);
    }

    if let Some(failure) = first_error
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
    {
        cleanup_extracted_files(&newly_created);
        return map_sevenz_failure(
            failure,
            archive_path,
            dest_dir,
            pwd_opt,
            archive_has_aes,
            app,
            excluded,
        );
    }

    if is_7z_cancelled(cancel_token.as_ref(), Some(&aborted)) {
        cleanup_extracted_files(&newly_created);
        return Err("Extraction cancelled by user".into());
    }

    let mut seen_empty: HashSet<String> = HashSet::new();
    for target in &empty_files {
        if is_extraction_cancelled() {
            cleanup_extracted_files(&newly_created);
            return Err("Extraction cancelled by user".into());
        }
        let key = normalize_7z_path_key(&target.out_path);
        let effective_existed = target.file_existed || !seen_empty.insert(key);
        let out_str = target.out_path.to_string_lossy().to_string();
        if !effective_existed {
            newly_created.push(out_str.clone());
        }
        if let Err(e) = write_file_payload(&target.out_path, &[], effective_existed) {
            cleanup_extracted_files(&newly_created);
            return Err(e);
        }
        extracted.push(out_str);
    }

    let final_bytes = total_extracted.load(Ordering::Relaxed);
    super::record_extracted_bytes(final_bytes);
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::ExtractionTaskGuard;
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader};
    use std::io::Cursor;

    fn make_entry(name: &str, data: &[u8]) -> ArchiveEntry {
        let mut entry = ArchiveEntry::new_file(name);
        entry.size = data.len() as u64;
        entry.has_stream = !data.is_empty();
        entry
    }

    #[test]
    fn test_calculate_7z_parallelism_by_storage_and_dict_memory() {
        // NVMe multi-block scales up to 8 block workers (split with LZMA2 threads)
        assert_eq!(
            calculate_7z_parallelism(DriveBusKind::Nvme, false, 8, 12, 4 * 1024 * 1024, false),
            (8, 1)
        );
        assert_eq!(
            calculate_7z_parallelism(DriveBusKind::Nvme, false, 16, 4, 4 * 1024 * 1024, false),
            (4, 4)
        );

        // SATA SSD caps block workers at 6
        assert_eq!(
            calculate_7z_parallelism(DriveBusKind::Sata, false, 16, 12, 4 * 1024 * 1024, false),
            (6, 2)
        );

        // Mechanical HDD, USB removable, single solid block, and cross-block collisions serialize to 1 block worker
        assert_eq!(
            calculate_7z_parallelism(DriveBusKind::Sata, true, 16, 12, 4 * 1024 * 1024, false),
            (1, 4)
        );
        assert_eq!(
            calculate_7z_parallelism(
                DriveBusKind::UsbRemovable,
                false,
                16,
                12,
                4 * 1024 * 1024,
                false
            ),
            (1, 4)
        );
        assert_eq!(
            calculate_7z_parallelism(DriveBusKind::Nvme, false, 16, 1, 4 * 1024 * 1024, false),
            (1, 8)
        );
        assert_eq!(
            calculate_7z_parallelism(DriveBusKind::Nvme, false, 16, 8, 4 * 1024 * 1024, true),
            (1, 8)
        );

        // Huge 256 MB dictionary caps concurrent block workers to 512 MB / 256 MB = 2
        assert_eq!(
            calculate_7z_parallelism(
                DriveBusKind::Nvme,
                false,
                16,
                10,
                256 * 1024 * 1024,
                false
            ),
            (2, 4)
        );
    }

    #[test]
    fn test_parallel_7z_multiblock_extraction_exclusions_empty_files_and_zip_slip() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_7z_multiblock");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let sz_path = base.join("multi.7z");
        let dest_dir = base.join("out");
        let large_payload: Vec<u8> = (0..(2 * 1024 * 1024 + 128 * 1024))
            .map(|i| ((i * 29) % 241) as u8)
            .collect();

        {
            let mut writer = ArchiveWriter::create(&sz_path).unwrap();
            let dir_entry = ArchiveEntry::new_directory("nested/");
            writer.push_archive_entry::<&[u8]>(dir_entry, None).unwrap();

            for i in 0..10 {
                let data = format!("block-file-{}-content", i).into_bytes();
                let name = format!("nested/f_{:02}.txt", i);
                writer
                    .push_archive_entry(make_entry(&name, &data), Some(Cursor::new(data.clone())))
                    .unwrap();
            }

            let zero_entry = ArchiveEntry::new_file("nested/empty.txt");
            writer.push_archive_entry::<&[u8]>(zero_entry, None).unwrap();

            writer
                .push_archive_entry(
                    make_entry("big/stream.bin", &large_payload),
                    Some(Cursor::new(large_payload.as_slice())),
                )
                .unwrap();

            let exc_data = b"should-not-extract".to_vec();
            writer
                .push_archive_entry(
                    make_entry("excluded/secret.txt", &exc_data),
                    Some(Cursor::new(exc_data.clone())),
                )
                .unwrap();

            let slip_data = b"pwned".to_vec();
            writer
                .push_archive_entry(
                    make_entry("../escape.txt", &slip_data),
                    Some(Cursor::new(slip_data.clone())),
                )
                .unwrap();

            writer.finish().unwrap();
        }

        let excluded = crate::inspector::build_excluded_set(["excluded"]);
        let guard = ExtractionTaskGuard::new();
        let files =
            extract_7z_archive(&sz_path, &dest_dir, None, None, None, excluded.as_ref()).unwrap();

        assert_eq!(files.len(), 12); // 10 small files + 1 empty + 1 large
        for i in 0..10 {
            let content =
                std::fs::read_to_string(dest_dir.join(format!("nested/f_{:02}.txt", i))).unwrap();
            assert_eq!(content, format!("block-file-{}-content", i));
        }
        assert_eq!(std::fs::read(dest_dir.join("nested/empty.txt")).unwrap(), b"");
        assert_eq!(
            std::fs::read(dest_dir.join("big/stream.bin")).unwrap(),
            large_payload
        );
        assert!(!dest_dir.join("excluded/secret.txt").exists());
        assert!(!base.join("escape.txt").exists());
        assert!(guard.bytes_extracted() > large_payload.len() as u64);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_solid_7z_extraction_drains_excluded_entries_without_desync() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_7z_solid_drain");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let sz_path = base.join("solid.7z");
        let dest_dir = base.join("out");

        let d1 = vec![b'A'; 4096];
        let d2 = vec![b'B'; 65536];
        let d3 = vec![b'C'; 32768];
        let d4 = b"after-excluded-and-slip-still-intact".to_vec();

        {
            let mut writer = ArchiveWriter::create(&sz_path).unwrap();
            let entries = vec![
                make_entry("keep/first.txt", &d1),
                make_entry("skip_me/middle.bin", &d2),
                make_entry("../evil_solid.txt", &d3),
                make_entry("keep/last.txt", &d4),
            ];
            let readers = vec![
                SourceReader::new(Cursor::new(d1.clone())),
                SourceReader::new(Cursor::new(d2.clone())),
                SourceReader::new(Cursor::new(d3.clone())),
                SourceReader::new(Cursor::new(d4.clone())),
            ];
            writer.push_archive_entries(entries, readers).unwrap();
            writer.finish().unwrap();
        }

        let excluded = crate::inspector::build_excluded_set(["skip_me"]);
        let files =
            extract_7z_archive(&sz_path, &dest_dir, None, None, None, excluded.as_ref()).unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(std::fs::read(dest_dir.join("keep/first.txt")).unwrap(), d1);
        assert_eq!(std::fs::read(dest_dir.join("keep/last.txt")).unwrap(), d4);
        assert!(!dest_dir.join("skip_me/middle.bin").exists());
        assert!(!base.join("evil_solid.txt").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_7z_cross_block_case_duplicate_serializes_last_wins() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_7z_cross_dup");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let sz_path = base.join("dup.7z");
        let dest_dir = base.join("out");

        {
            let mut writer = ArchiveWriter::create(&sz_path).unwrap();
            let first = b"first-block-version".to_vec();
            let second = b"second-block-wins".to_vec();
            writer
                .push_archive_entry(
                    make_entry("Docs/Note.txt", &first),
                    Some(Cursor::new(first.clone())),
                )
                .unwrap();
            writer
                .push_archive_entry(
                    make_entry("docs/note.txt", &second),
                    Some(Cursor::new(second.clone())),
                )
                .unwrap();
            writer.finish().unwrap();
        }

        let files = extract_7z_archive(&sz_path, &dest_dir, None, None, None, None).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("docs/note.txt")).unwrap(),
            "second-block-wins"
        );
        assert!(!dest_dir.join("docs/note.txt.baypart").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_7z_encrypted_password_handling_and_roundtrip() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_7z_encrypted");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let sz_path = base.join("secret.7z");
        let dest_dir = base.join("out");
        let pwd = "strong_7z_password";

        {
            let mut writer = ArchiveWriter::create(&sz_path).unwrap();
            let methods: Vec<sevenz_rust2::EncoderConfiguration> = vec![
                sevenz_rust2::encoder_options::AesEncoderOptions::new(pwd.into()).into(),
                sevenz_rust2::encoder_options::Lzma2Options::from_level(1).into(),
            ];
            writer.set_content_methods(methods);
            writer.set_encrypt_header(true);

            for i in 0..5 {
                let data = format!("encrypted-block-{}", i).into_bytes();
                let name = format!("vault/item_{}.txt", i);
                writer
                    .push_archive_entry(make_entry(&name, &data), Some(Cursor::new(data.clone())))
                    .unwrap();
            }
            writer.finish().unwrap();
        }

        let missing = extract_7z_archive(&sz_path, &dest_dir, None, None, None, None);
        assert!(missing.is_err());
        assert!(missing.unwrap_err().contains("Password required"));

        let wrong = extract_7z_archive(&sz_path, &dest_dir, Some("wrong_pw"), None, None, None);
        assert!(wrong.is_err());
        let wrong_msg = wrong.unwrap_err();
        assert!(wrong_msg.contains("Incorrect password"));

        let files = extract_7z_archive(&sz_path, &dest_dir, Some(pwd), None, None, None).unwrap();
        assert_eq!(files.len(), 5);
        for i in 0..5 {
            assert_eq!(
                std::fs::read_to_string(dest_dir.join(format!("vault/item_{}.txt", i))).unwrap(),
                format!("encrypted-block-{}", i)
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}

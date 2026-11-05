use std::fs::File;
use std::io::{Read, Write, Seek, BufReader, BufWriter, BufRead};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use crate::crypto::ChunkedAesReader;
use sevenz_rust2::Password;

use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use std::sync::atomic::{AtomicBool, Ordering};

pub static CANCEL_EXTRACTION: AtomicBool = AtomicBool::new(false);

pub fn cancel_extraction_flag() {
    CANCEL_EXTRACTION.store(true, Ordering::SeqCst);
}

pub fn is_extraction_cancelled() -> bool {
    CANCEL_EXTRACTION.load(Ordering::Relaxed)
}

pub fn reset_extraction_cancel_flag() {
    CANCEL_EXTRACTION.store(false, Ordering::SeqCst);
}

use crate::types::{ArchiveFormat, ExtractResponse, ExtractProgress};
use crate::priority::ScopedPriorityGuard;
use crate::inspector::{
    sanitize_archive_path,
    detect_archive_type,
    archive_stem_name,
    inspect_archive,
    has_single_root_dir,
    get_single_root_dir,
    safe_atomic_rename,
    clear_readonly,
};

pub fn base_folder_stem(name: &str) -> &str {
    if let Some(pos) = name.rfind(" (") {
        if name.ends_with(')') {
            return &name[..pos];
        }
    }
    name
}

pub fn strip_root_component<'a>(path: &'a Path, root_name: &str) -> Option<&'a Path> {
    let mut components = path.components();
    if let Some(std::path::Component::Normal(first)) = components.next() {
        if first.to_string_lossy().eq_ignore_ascii_case(root_name) {
            return Some(components.as_path());
        }
    }
    None
}

pub fn resolve_unique_folder_name(target_dir: &Path) -> PathBuf {
    let Some(name) = target_dir.file_name().and_then(|s| s.to_str()) else {
        return target_dir.to_path_buf();
    };
    if !target_dir.exists() {
        return target_dir.to_path_buf();
    }
    let parent = target_dir.parent().unwrap_or_else(|| Path::new("."));
    let stem = base_folder_stem(name);
    let mut counter = 1;
    loop {
        let candidate = parent.join(format!("{} ({})", stem, counter));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

pub fn is_path_excluded(raw_path: &str, excluded: Option<&std::collections::HashSet<String>>) -> bool {
    if let Some(set) = excluded {
        let clean = raw_path.replace('\\', "/");
        let trimmed = clean.trim_start_matches('/');
        let lower = trimmed.to_lowercase();
        if set.contains(trimmed) || set.contains(&lower) {
            return true;
        }
        for exc in set.iter() {
            let exc_clean = exc.trim_end_matches('/');
            let exc_lower = exc_clean.to_lowercase();
            if lower.starts_with(&format!("{}/", exc_lower)) || lower == exc_lower {
                return true;
            }
        }
        false
    } else {
        false
    }
}

/// Strictly drains exactly `bytes_to_drain` uncompressed bytes from the archive decompressor.
/// If EOF or an I/O error occurs before all bytes are drained, returns an explicit error
/// to abort extraction immediately and prevent container stream desynchronization (V-SEC-02).
pub fn drain_bay_entry_bytes<R: Read>(
    decoder: &mut R,
    bytes_to_drain: u64,
    buf: &mut [u8],
) -> Result<(), String> {
    let mut remaining = bytes_to_drain;
    while remaining > 0 {
        if is_extraction_cancelled() {
            return Err("Extraction cancelled by user".to_string());
        }
        let to_read = (remaining as usize).min(buf.len());
        match decoder.read(&mut buf[..to_read]) {
            Ok(0) => {
                return Err(format!(
                    "Decompression stream desynchronization: premature EOF encountered while draining entry bytes ({} bytes remaining of {})",
                    remaining, bytes_to_drain
                ));
            }
            Ok(n) => {
                remaining -= n as u64;
            }
            Err(e) => {
                return Err(format!(
                    "I/O error while draining entry payload from decompressor: {}",
                    e
                ));
            }
        }
    }
    Ok(())
}

pub fn extract_bay_stream<R: Read>(
    mut decoder: R,
    dest_dir: &Path,
    total_expected_bytes: u64,
    app: Option<&AppHandle>,
    single_root_prefix: Option<&str>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    let mut extracted_files = Vec::new();
    let root_strip = single_root_prefix.filter(|prefix| {
        dest_dir.file_name()
            .and_then(|s| s.to_str())
            .map(base_folder_stem)
            .map(|dest_stem| dest_stem.eq_ignore_ascii_case(prefix))
            .unwrap_or(false)
    });
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    let mut last_emit = Instant::now();
    let mut total_extracted: u64 = 0;

    let mut first_byte = [0u8; 1];
    let mut has_first = false;

    // Check if the stream starts with redundant b"BAY..." signature (from legacy files)
    match decoder.read_exact(&mut first_byte) {
        Ok(_) => {
            if first_byte[0] == b'B' {
                // Peek/skip remaining signature bytes (AYZIP\x02, AYBRO\x02, AYLZ4\x02)
                let mut rest_sig = [0u8; 6];
                let _ = decoder.read_exact(&mut rest_sig);
            } else {
                has_first = true;
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(extracted_files),
        Err(e) => return Err(format!("Error reading archive header: {}", e)),
    }

    // Allocate single 256 KB reusable I/O buffer outside the entry loop (optimal for L2/L3 cache)
    let mut io_buf = vec![0u8; 256 * 1024];

    loop {
        if is_extraction_cancelled() {
            return Err("Extraction cancelled by user".to_string());
        }

        let entry_type = if has_first {
            has_first = false;
            first_byte[0]
        } else {
            let mut b = [0u8; 1];
            match decoder.read_exact(&mut b) {
                Ok(_) => b[0],
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(format!("Error reading archive entry: {}", e)),
            }
        };

        if entry_type == 0 {
            break;
        }

        let mut len_bytes = [0u8; 4];
        decoder.read_exact(&mut len_bytes)
            .map_err(|e| format!("Failed to read entry name length: {}", e))?;
        let name_len = u32::from_le_bytes(len_bytes) as usize;
        if name_len > 65536 {
            return Err("Malformed archive: entry name length exceeds 64 KB safety limit".to_string());
        }
        let mut name_buf = vec![0u8; name_len];
        decoder.read_exact(&mut name_buf)
            .map_err(|e| format!("Failed to read entry name: {}", e))?;
        let rel_path_str = String::from_utf8_lossy(&name_buf).to_string();

        let mut size_bytes = [0u8; 8];
        decoder.read_exact(&mut size_bytes)
            .map_err(|e| format!("Failed to read entry size: {}", e))?;
        let file_size = u64::from_le_bytes(size_bytes);

        let is_excluded = is_path_excluded(&rel_path_str, excluded);
        let safe_rel_path = match sanitize_archive_path(&rel_path_str) {
            Some(p) if !is_excluded => p,
            _ => {
                drain_bay_entry_bytes(&mut decoder, file_size, &mut io_buf)?;
                continue;
            }
        };

        let effective_path = match root_strip {
            Some(prefix) => {
                match strip_root_component(&safe_rel_path, prefix) {
                    Some(stripped) => {
                        if stripped.as_os_str().is_empty() {
                            let _ = std::fs::create_dir_all(dest_dir);
                            drain_bay_entry_bytes(&mut decoder, file_size, &mut io_buf)?;
                            continue;
                        }
                        stripped.to_path_buf()
                    }
                    None => safe_rel_path.clone(),
                }
            }
            None => safe_rel_path.clone(),
        };

        let out_path = dest_dir.join(&effective_path);

        let is_dir = entry_type == 2 || rel_path_str.ends_with('/');
        if is_dir {
            let _ = std::fs::create_dir_all(&out_path);
            extracted_files.push(out_path.to_string_lossy().to_string());
            drain_bay_entry_bytes(&mut decoder, file_size, &mut io_buf)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if file_size <= 64 * 1024 {
            clear_readonly(&out_path);
            let mut out_file = File::create(&out_path)
                .map_err(|e| format!("Failed to create output file {:?}: {}", out_path, e))?;
            // Tiny files (<= 64 KB): Direct write to disk without BufWriter allocation overhead
            let sz = file_size as usize;
            if sz > 0 {
                decoder.read_exact(&mut io_buf[..sz])
                    .map_err(|e| format!("Error reading decompressed entry: {}", e))?;
                out_file.write_all(&io_buf[..sz])
                    .map_err(|e| format!("Error writing extracted file: {}", e))?;
                total_extracted += file_size;
            }

            if let Some(app_handle) = app {
                if last_emit.elapsed().as_millis() >= 50 {
                    last_emit = Instant::now();
                    let pct = if total_expected_bytes > 0 {
                        ((total_extracted as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
                    } else {
                        50.0
                    };
                    let _ = app_handle.emit("extract_progress", ExtractProgress {
                        percent: pct,
                        current_file: safe_rel_path.to_string_lossy().to_string(),
                        bytes_extracted: total_extracted,
                    });
                }
            }
        } else {
            // Atomic extraction via .baypart: prevents leaving corrupted partial files on disk if interrupted
            let part_path = {
                let mut p = out_path.as_os_str().to_os_string();
                p.push(".baypart");
                PathBuf::from(p)
            };

            let out_file = File::create(&part_path)
                .map_err(|e| format!("Failed to create temporary output file {:?}: {}", part_path, e))?;

            // Optimal buffer capacity: 32 KB for small files, 256 KB for medium, 512 KB for large files
            let cap = if file_size <= 64 * 1024 {
                32 * 1024
            } else if file_size <= 1024 * 1024 {
                256 * 1024
            } else {
                512 * 1024
            };
            let mut writer = BufWriter::with_capacity(cap, out_file);

            let mut remaining = file_size;
            let mut write_err: Option<String> = None;

            while remaining > 0 {
                if is_extraction_cancelled() {
                    write_err = Some("Extraction cancelled by user".into());
                    break;
                }
                let to_read = (remaining as usize).min(io_buf.len());
                match decoder.read(&mut io_buf[..to_read]) {
                    Ok(0) => {
                        write_err = Some("Premature EOF inside archive entry".into());
                        break;
                    }
                    Ok(n) => {
                        if let Err(e) = writer.write_all(&io_buf[..n]) {
                            write_err = Some(format!("Error writing extracted file: {}", e));
                            break;
                        }
                        remaining -= n as u64;
                        total_extracted += n as u64;

                        if let Some(app_handle) = app {
                            if last_emit.elapsed().as_millis() >= 50 {
                                last_emit = Instant::now();
                                let pct = if total_expected_bytes > 0 {
                                    ((total_extracted as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
                                } else {
                                    50.0
                                };
                                let _ = app_handle.emit("extract_progress", ExtractProgress {
                                    percent: pct,
                                    current_file: safe_rel_path.to_string_lossy().to_string(),
                                    bytes_extracted: total_extracted,
                                });
                            }
                        }
                    }
                    Err(e) => {
                        write_err = Some(format!("Error reading decompressed stream: {}", e));
                        break;
                    }
                }
            }

            if let Some(err) = write_err {
                drop(writer);
                let _ = std::fs::remove_file(&part_path);
                return Err(err);
            }

            writer.flush().map_err(|e| format!("Flush error: {}", e))?;
            drop(writer);

            safe_atomic_rename(&part_path, &out_path)
                .map_err(|e| format!("Failed to finalize extracted file {:?}: {}", out_path, e))?;
        }

        extracted_files.push(out_path.to_string_lossy().to_string());
    }

    Ok(extracted_files)
}

pub fn extract_bay_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    let total_expected_bytes: u64 = if let Some(sz) = expected_size.filter(|&s| s > 0) {
        sz
    } else {
        std::fs::metadata(archive_path).map(|m| m.len() * 2).unwrap_or(1)
    };

    let single_root_prefix = if crate::inspector::has_instant_bay_toc(archive_path) {
        let (entries, _, _) = crate::inspector::list_bay_entries(archive_path, password);
        get_single_root_dir(&entries)
    } else {
        None
    };
    let s_prefix = single_root_prefix.as_deref();

    let raw_file = File::open(archive_path)
        .map_err(|e| format!("Failed to open .bay archive: {}", e))?;
    let mut file = BufReader::with_capacity(1024 * 1024, raw_file);

    let mut magic = [0u8; 8];
    let n = file.read(&mut magic)
        .map_err(|e| format!("Invalid or corrupted .bay header: {}", e))?;

    // Encrypted .bay v3 (Streaming chunked AEAD - Constant ~4 MB RAM)
    if n >= 7 && magic.starts_with(b"BAYENC\x03") {
        let pwd = match password {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => return Err("Password required: This .bay archive is encrypted with AES-256-GCM.".into()),
        };

        file.seek(std::io::SeekFrom::Start(8)).map_err(|e| e.to_string())?;
        let mut enc_reader = ChunkedAesReader::new(file, pwd)
            .map_err(|e| format!("Failed to initialize decryption: {}", e))?;

        let mut inner_sig = [0u8; 8];
        enc_reader.read_exact(&mut inner_sig)
            .map_err(|_| "Incorrect password: Failed to decrypt AES-256-GCM archive.".to_string())?;

        if inner_sig.starts_with(b"BAYBRO\x02") {
            let decoder = brotli::Decompressor::new(enc_reader, 64 * 1024);
            return extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded);
        } else if inner_sig.starts_with(b"BAYLZ4\x02") {
            let decoder = lz4_flex::frame::FrameDecoder::new(enc_reader);
            return extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded);
        } else if inner_sig.starts_with(b"BAYZIP\x02") {
            let decoder = zstd::stream::Decoder::new(enc_reader)
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded);
        } else {
            let combined = std::io::Cursor::new(inner_sig).chain(enc_reader);
            let decoder = zstd::stream::Decoder::new(combined)
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded);
        }
    }

    // Legacy encrypted .bay v1 & v2 (deprecated in v2.0+)
    if n >= 7 && (magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
        return Err("Deprecated archive: Legacy v1/v2 encryption is no longer supported. Please re-encrypt using modern Bayzip v2.0+ streaming encryption.".to_string());
    }

    // Unencrypted .bay
    file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut magic_header = [0u8; 8];
    let n = file.read(&mut magic_header).unwrap_or(0);

    if n >= 7 && magic_header.starts_with(b"BAYBRO\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = brotli::Decompressor::new(file, 64 * 1024);
        extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded)
    } else if n >= 7 && magic_header.starts_with(b"BAYLZ4\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded)
    } else if n >= 7 && magic_header.starts_with(b"BAYZIP\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = zstd::stream::Decoder::new(file)
            .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;
        extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded)
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        // Raw LZ4 frame
        file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded)
    } else {
        // Standard Zstd
        file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let decoder = zstd::stream::Decoder::new(file)
            .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;
        extract_bay_stream(decoder, dest_dir, total_expected_bytes, app, s_prefix, excluded)
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
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    let raw_file = File::open(archive_path).map_err(|e| format!("Failed to open zip: {}", e))?;
    let reader = BufReader::with_capacity(4 * 1024 * 1024, raw_file);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("Invalid zip archive: {}", e))?;
    let mut extracted = Vec::new();
    let root_strip = {
        let dest_stem = dest_dir.file_name().and_then(|s| s.to_str()).map(base_folder_stem);
        if let Some(prefix) = dest_stem {
            let mut candidate_root: Option<String> = None;
            let mut is_single = true;
            for name in archive.file_names() {
                let clean = name.trim().trim_start_matches(|c| c == '/' || c == '\\').trim_start_matches("./");
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

    // Single 2 MB reusable buffer outside the zip loop
    let mut io_buf = vec![0u8; 256 * 1024];

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
        if file.is_dir() || clean.ends_with('/') {
            let _ = std::fs::create_dir_all(&outpath);
        } else {
            if let Some(p) = outpath.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let file_size = file.size();

            if file_size <= 64 * 1024 {
                clear_readonly(&outpath);
                let mut out_file = File::create(&outpath).map_err(|e| format!("Create file error: {}", e))?;
                let sz = file_size as usize;
                if sz > 0 {
                    file.read_exact(&mut io_buf[..sz]).map_err(|e| format!("Zip read error: {}", e))?;
                    out_file.write_all(&io_buf[..sz]).map_err(|e| format!("Zip write error: {}", e))?;
                    total_extracted += file_size;
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
                        Ok(0) => break,
                        Ok(n) => {
                            writer.write_all(&io_buf[..n]).map_err(|e| format!("Zip write error: {}", e))?;
                            remaining -= n as u64;
                            total_extracted += n as u64;

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
                            return Err(format!("Zip read error: {}", e));
                        }
                    }
                }
                writer.flush().map_err(|e| format!("Zip flush error: {}", e))?;
                drop(writer);
                safe_atomic_rename(&part_path, &outpath)
                    .map_err(|e| format!("Failed to finalize extracted file: {}", e))?;
            }

            extracted.push(outpath.to_string_lossy().to_string());
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
    Ok(extracted)
}

fn extract_tar_entries<R: Read>(
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
    let mut total_extracted: u64 = 0;
    let mut last_emit = Instant::now();
    let mut io_buf = vec![0u8; 256 * 1024];

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
        let header = entry.header();
        let is_dir = header.entry_type().is_dir() || clean.ends_with('/');

        if is_dir {
            let _ = std::fs::create_dir_all(&out_path);
        } else {
            if let Some(p) = out_path.parent() {
                let _ = std::fs::create_dir_all(p);
            }

            let file_size = entry.size();

            if file_size <= 64 * 1024 {
                clear_readonly(&out_path);
                let mut out_file = File::create(&out_path).map_err(|e| format!("Create file error: {}", e))?;
                let sz = file_size as usize;
                if sz > 0 {
                    entry.read_exact(&mut io_buf[..sz]).map_err(|e| format!("Tar read error: {}", e))?;
                    out_file.write_all(&io_buf[..sz]).map_err(|e| format!("Tar write error: {}", e))?;
                    total_extracted += file_size;
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
                        Ok(0) => break,
                        Ok(n) => {
                            writer.write_all(&io_buf[..n]).map_err(|e| format!("Tar write error: {}", e))?;
                            remaining = remaining.saturating_sub(n as u64);
                            total_extracted += n as u64;

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
                writer.flush().map_err(|e| format!("Flush error: {}", e))?;
                drop(writer);
                safe_atomic_rename(&part_path, &out_path)
                    .map_err(|e| format!("Failed to finalize extracted file: {}", e))?;
            }

            extracted_files.push(out_path.to_string_lossy().to_string());
        }
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

    let pwd = pwd_opt.map(|s| Password::from(s)).unwrap_or_else(Password::empty);

    let mut sz_reader = match sevenz_rust2::ArchiveReader::new(reader, pwd) {
        Ok(r) => r,
        Err(sevenz_rust2::Error::PasswordRequired) => {
            return Err("Password required: This 7z archive is encrypted with AES-256.".into());
        }
        Err(e) => {
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
    let root_strip = {
        let dest_stem = dest_dir.file_name().and_then(|s| s.to_str()).map(base_folder_stem);
        if let Some(prefix) = dest_stem {
            let mut candidate_root: Option<String> = None;
            let mut is_single = true;
            for f in &sz_reader.archive().files {
                let clean = f.name().trim().trim_start_matches(|c| c == '/' || c == '\\').trim_start_matches("./");
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
            let _ = std::fs::create_dir_all(&out_path);
        } else {
            if let Some(p) = out_path.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let file_size = entry.size();

            if file_size <= 64 * 1024 {
                clear_readonly(&out_path);
                let mut out_file = File::create(&out_path)
                    .map_err(sevenz_rust2::Error::from)?;
                let sz = file_size as usize;
                if sz > 0 {
                    reader.read_exact(&mut io_buf[..sz]).map_err(sevenz_rust2::Error::from)?;
                    out_file.write_all(&io_buf[..sz]).map_err(sevenz_rust2::Error::from)?;
                    total_extracted += file_size;
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
                        Ok(0) => break,
                        Ok(n) => {
                            writer.write_all(&io_buf[..n]).map_err(sevenz_rust2::Error::from)?;
                            remaining -= n as u64;
                            total_extracted += n as u64;

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
                writer.flush().map_err(sevenz_rust2::Error::from)?;
                drop(writer);
                safe_atomic_rename(&part_path, &out_path)
                    .map_err(sevenz_rust2::Error::from)?;
            }

            extracted.push(out_path.to_string_lossy().to_string());
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
            Err("Password required: This 7z archive is encrypted with AES-256.".into())
        }
        Err(e) => {
            if e.to_string().to_lowercase().contains("password") {
                Err("Incorrect password: Could not decrypt 7z archive.".into())
            } else {
                extract_with_system_tar(archive_path, dest_dir, app, excluded)
                    .map_err(|_| format!("7z extraction error: {}", e))
            }
        }
    }
}

pub fn extract_with_system_tar(
    archive_path: &Path,
    dest_dir: &Path,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    let _priority_guard = ScopedPriorityGuard::new();

    // Pre-inspect archive entries with tar.exe -tf to strictly prevent Zip-Slip / path traversal before writing to disk
    let mut check_cmd = Command::new("tar.exe");
    check_cmd.args(["-tf", &archive_path.to_string_lossy()]);
    #[cfg(target_os = "windows")]
    check_cmd.creation_flags(CREATE_NO_WINDOW);
    check_cmd.stdout(Stdio::piped());
    check_cmd.stderr(Stdio::piped());

    let output = check_cmd
        .output()
        .map_err(|e| format!("Failed to inspect archive with system tar: {}", e))?;

    if !output.status.success() {
        let err_text = String::from_utf8_lossy(&output.stderr);
        return Err(format!("System tar archive inspection failed: {}", err_text.trim()));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    for raw_line in stdout_str.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let clean = trimmed.replace('\\', "/");
        if clean == "." || clean == "./" {
            continue;
        }
        if sanitize_archive_path(&clean).is_none() {
            return Err(format!(
                "Security violation: archive entry '{}' contains dangerous path or traversal pattern. Extraction aborted.",
                trimmed
            ));
        }
    }

    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    // Determine estimated uncompressed size or archive file size on disk
    let archive_size = std::fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);
    let mut total_expected_bytes = archive_size.max(1);

    // If we can get total uncompressed size from inspector, use it
    let (_rar_items, _total_count, uncompressed_size) = crate::inspector::list_rar_entries(archive_path);
    if uncompressed_size > 0 {
        total_expected_bytes = uncompressed_size;
    }

    let mut cmd = Command::new("tar.exe");
    let mut args: Vec<String> = vec![
        "-xvf".into(),
        archive_path.to_string_lossy().to_string(),
        "-C".into(),
        dest_dir.to_string_lossy().to_string(),
    ];
    if let Some(set) = excluded {
        for exc in set.iter() {
            args.push(format!("--exclude={}", exc));
        }
    }
    cmd.args(&args);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
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
    let mut current_file_name = String::new();
    let mut current_file_path: Option<PathBuf> = None;
    let mut completed_bytes: u64 = 0;
    let mut last_emit = Instant::now();
    let mut error_lines = Vec::new();

    loop {
        if is_extraction_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader_handle.join();
            return Err("Extraction cancelled by user".into());
        }

        while let Ok(line) = rx.try_recv() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(rel_path) = trimmed.strip_prefix("x ") {
                let clean_rel = rel_path.trim().replace('\\', "/");
                let safe = sanitize_archive_path(&clean_rel);

                if let Some(prev_path) = current_file_path.take() {
                    if let Ok(m) = std::fs::metadata(&prev_path) {
                        completed_bytes += m.len();
                    }
                }

                if let Some(safe_path) = safe {
                    let out_path = dest_dir.join(&safe_path);
                    if is_path_excluded(&clean_rel, excluded) {
                        if out_path.is_file() {
                            let _ = std::fs::remove_file(&out_path);
                        } else if out_path.is_dir() {
                            let _ = std::fs::remove_dir(&out_path);
                        }
                    } else {
                        current_file_name = safe_path.to_string_lossy().to_string();
                        current_file_path = Some(out_path.clone());
                        extracted_files.push(out_path.to_string_lossy().to_string());
                    }
                } else {
                    current_file_name = clean_rel;
                }
            } else if trimmed.to_lowercase().contains("error") || trimmed.to_lowercase().contains("fatal") {
                error_lines.push(trimmed.to_string());
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = reader_handle.join();

                while let Ok(line) = rx.try_recv() {
                    let trimmed = line.trim();
                    if let Some(rel_path) = trimmed.strip_prefix("x ") {
                        let clean_rel = rel_path.trim().replace('\\', "/");
                        if let Some(safe_path) = sanitize_archive_path(&clean_rel) {
                            let out_path = dest_dir.join(&safe_path);
                            if is_path_excluded(&clean_rel, excluded) {
                                if out_path.is_file() {
                                    let _ = std::fs::remove_file(&out_path);
                                } else if out_path.is_dir() {
                                    let _ = std::fs::remove_dir(&out_path);
                                }
                            } else {
                                extracted_files.push(out_path.to_string_lossy().to_string());
                            }
                        }
                    } else if trimmed.to_lowercase().contains("error") || trimmed.to_lowercase().contains("fatal") {
                        error_lines.push(trimmed.to_string());
                    }
                }

                if !status.success() {
                    let err_msg = if !error_lines.is_empty() {
                        error_lines.join("; ")
                    } else {
                        format!("tar.exe exited with error code: {:?}", status.code())
                    };
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
                return Err(format!("Error monitoring tar.exe process: {}", e));
            }
        }
    }

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

    Ok(extracted_files)
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
                    if e.contains("Password") || e.contains("password") {
                        return Err(e);
                    }
                    extract_bay_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                        .or_else(|_| extract_with_system_tar(archive_path, dest_dir, app, excluded))
                })
        },
        ArchiveFormat::SevenZip => {
            extract_7z_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                .or_else(|e| {
                    if e.contains("Password") || e.contains("password") {
                        return Err(e);
                    }
                    extract_with_system_tar(archive_path, dest_dir, app, excluded)
                })
        },
        ArchiveFormat::Tar | ArchiveFormat::TarGz | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd | ArchiveFormat::Iso | ArchiveFormat::Cab => {
            extract_with_system_tar(archive_path, dest_dir, app, excluded)
                .or_else(|_| extract_tar_archive(archive_path, dest_dir, expected_size, app, excluded))
        },
        ArchiveFormat::Rar => {
            extract_with_system_tar(archive_path, dest_dir, app, excluded)
        },
        ArchiveFormat::Unknown => {
            extract_zip_archive(archive_path, dest_dir, password, expected_size, app, excluded)
                .or_else(|_| extract_7z_archive(archive_path, dest_dir, password, expected_size, app, excluded))
                .or_else(|_| extract_bay_archive(archive_path, dest_dir, password, expected_size, app, excluded))
                .or_else(|_| extract_with_system_tar(archive_path, dest_dir, app, excluded))
                .or_else(|_| extract_tar_archive(archive_path, dest_dir, expected_size, app, excluded))
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

    let dest_dir = if let Some(custom_dest) = destination {
        if custom_dest.trim().is_empty() {
            if rar_single_root_match {
                parent.to_path_buf()
            } else {
                resolve_unique_folder_name(&parent.join(&stem))
            }
        } else {
            let p = PathBuf::from(custom_dest);
            if rar_single_root_match {
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
        }
    } else {
        if rar_single_root_match {
            parent.to_path_buf()
        } else {
            resolve_unique_folder_name(&parent.join(&stem))
        }
    };

    let target_folder = if rar_single_root_match {
        dest_dir.join(&stem)
    } else {
        dest_dir.clone()
    };
    let target_folder_existed_before = target_folder.exists();
    let pwd_ref = password.as_deref();

    let excluded_set: Option<std::collections::HashSet<String>> = excluded_paths.filter(|p| !p.is_empty()).map(|paths| {
        paths.into_iter()
            .flat_map(|p| {
                let clean = p.replace('\\', "/").trim_start_matches('/').to_string();
                let lower = clean.to_lowercase();
                vec![clean, lower]
            })
            .collect()
    });

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

    let total_final_bytes: u64 = extracted_files.iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();

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
    let _priority_guard = ScopedPriorityGuard::new();

    let p = Path::new(archive_path);
    if !p.exists() {
        return Err("Archive file does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    universal_extract(p, parent, None, None, None, None)?;

    #[cfg(target_os = "windows")]
    {
        let mut refresh_cmd = Command::new("cmd");
        refresh_cmd.args(["/C", "ie4uinit.exe -show"]);
        refresh_cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = refresh_cmd.output();
    }
    Ok(())
}

pub fn headless_extract_to_subfolder(archive_path: &str) -> Result<(), String> {
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
        let mut refresh_cmd = Command::new("cmd");
        refresh_cmd.args(["/C", "ie4uinit.exe -show"]);
        refresh_cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = refresh_cmd.output();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_drain_bay_entry_bytes_exact_match() {
        let payload = vec![0xABu8; 1024];
        let mut cursor = Cursor::new(payload);
        let mut buf = vec![0u8; 256];

        let result = drain_bay_entry_bytes(&mut cursor, 1024, &mut buf);
        assert!(result.is_ok());
        assert_eq!(cursor.position(), 1024);
    }

    #[test]
    fn test_drain_bay_entry_bytes_premature_eof() {
        let payload = vec![0xCDu8; 500];
        let mut cursor = Cursor::new(payload);
        let mut buf = vec![0u8; 256];

        let result = drain_bay_entry_bytes(&mut cursor, 1000, &mut buf);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Decompression stream desynchronization: premature EOF encountered"));
        assert!(err.contains("500 bytes remaining of 1000"));
    }

    #[test]
    fn test_drain_bay_entry_bytes_zero_length() {
        let payload = vec![0xEFu8; 10];
        let mut cursor = Cursor::new(payload);
        let mut buf = vec![0u8; 256];

        let result = drain_bay_entry_bytes(&mut cursor, 0, &mut buf);
        assert!(result.is_ok());
        assert_eq!(cursor.position(), 0);
    }

    #[test]
    fn test_drain_bay_entry_bytes_unaligned_chunking() {
        let payload = vec![0x42u8; 997];
        let mut cursor = Cursor::new(payload);
        let mut buf = vec![0u8; 13]; // Unaligned prime chunk size

        let result = drain_bay_entry_bytes(&mut cursor, 997, &mut buf);
        assert!(result.is_ok());
        assert_eq!(cursor.position(), 997);
    }

    struct FailingReader;
    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "simulated broken pipe"))
        }
    }

    #[test]
    fn test_drain_bay_entry_bytes_io_error() {
        let mut failing = FailingReader;
        let mut buf = vec![0u8; 64];

        let result = drain_bay_entry_bytes(&mut failing, 100, &mut buf);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("I/O error while draining entry payload from decompressor"));
        assert!(err.contains("simulated broken pipe"));
    }
}

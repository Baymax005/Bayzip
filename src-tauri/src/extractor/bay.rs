use std::fs::File;
use std::io::{Read, Write, Seek, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::AppHandle;

use crate::crypto::ChunkedAesReader;
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
    emit_extract_progress,
};

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
    let mut newly_created = Vec::new();
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

    let res = (|| -> Result<(), String> {
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
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Err("Malformed or truncated .bay archive (missing EOF marker)".to_string());
                }
                Err(e) => return Err(format!("Error reading archive entry: {}", e)),
            }
        };

        if entry_type == 0 {
            loop {
                match decoder.read(&mut io_buf) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => {
                        return Err(format!(
                            "Archive stream integrity or decryption verification failed at EOF: {}",
                            e
                        ));
                    }
                }
            }
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
            let dir_existed = std::fs::symlink_metadata(&out_path).is_ok();
            let _ = std::fs::create_dir_all(&out_path);
            let out_str = out_path.to_string_lossy().to_string();
            if !dir_existed && out_path.exists() {
                newly_created.push(out_str.clone());
            }
            extracted_files.push(out_str);
            drain_bay_entry_bytes(&mut decoder, file_size, &mut io_buf)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let file_existed = std::fs::symlink_metadata(&out_path).is_ok();

        if file_size <= 64 * 1024 && !file_existed {
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
                total_extracted = total_extracted.saturating_add(file_size);
            }

            emit_extract_progress(
                app,
                &mut last_emit,
                &safe_rel_path.to_string_lossy(),
                total_extracted,
                total_expected_bytes,
            );
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
                        total_extracted = total_extracted.saturating_add(n as u64);

                        emit_extract_progress(
                            app,
                            &mut last_emit,
                            &safe_rel_path.to_string_lossy(),
                            total_extracted,
                            total_expected_bytes,
                        );
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

            if let Err(e) = writer.flush() {
                drop(writer);
                let _ = std::fs::remove_file(&part_path);
                return Err(format!("Flush error: {}", e));
            }
            drop(writer);

            if let Err(e) = safe_atomic_rename(&part_path, &out_path) {
                let _ = std::fs::remove_file(&part_path);
                return Err(format!("Failed to finalize extracted file {:?}: {}", out_path, e));
            }
        }

        let out_str = out_path.to_string_lossy().to_string();
        if !file_existed {
            newly_created.push(out_str.clone());
        }
        extracted_files.push(out_str);
    }

    Ok(())
    })();

    if let Err(e) = res {
        cleanup_extracted_files(&newly_created);
        return Err(e);
    }

    super::record_extracted_bytes(total_extracted);
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

    #[test]
    fn test_extract_bay_stream_eof_drain_and_truncation_detection() {
        let test_dir = std::env::current_dir().unwrap().join("target").join("test_bay_eof_extract");
        let _ = std::fs::remove_dir_all(&test_dir);
        let _ = std::fs::create_dir_all(&test_dir);

        // 1. Missing EOF 0u8 marker after file entry -> Error
        let mut truncated = Vec::new();
        truncated.push(1u8);
        let name = b"hello.txt";
        truncated.extend_from_slice(&(name.len() as u32).to_le_bytes());
        truncated.extend_from_slice(name);
        truncated.extend_from_slice(&5u64.to_le_bytes());
        truncated.extend_from_slice(b"world");
        let res = extract_bay_stream(std::io::Cursor::new(truncated), &test_dir, 5, None, None, None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("missing EOF marker"));
        assert!(!test_dir.join("hello.txt").exists());

        // 2. Encrypted stream truncated right after entry_type == 0 (missing ChunkedAesReader 0u32 EOF marker)
        let password = "stream_eof_pass";
        let mut raw_stream = Vec::new();
        {
            let mut writer = crate::crypto::ChunkedAesWriter::new(&mut raw_stream, password).unwrap();
            writer.write_all(&[1u8]).unwrap();
            let entry_name = b"ok.txt";
            writer.write_all(&(entry_name.len() as u32).to_le_bytes()).unwrap();
            writer.write_all(entry_name).unwrap();
            writer.write_all(&2u64.to_le_bytes()).unwrap();
            writer.write_all(b"hi").unwrap();
            writer.write_all(&[0u8]).unwrap(); // entry_type == 0 present inside stream
            writer.finish().unwrap();
        }
        // Chop off the final 4-byte ChunkedAesWriter 0u32 EOF marker!
        let chopped = &raw_stream[8..raw_stream.len() - 4];
        let enc_reader = ChunkedAesReader::new(chopped, password).unwrap();
        let res_eof = extract_bay_stream(enc_reader, &test_dir, 2, None, None, None);
        assert!(res_eof.is_err());
        assert!(res_eof.unwrap_err().contains("verification failed at EOF"));
        // Partial/rolled-back file must be cleaned up on EOF verification failure
        assert!(!test_dir.join("ok.txt").exists());

        let _ = std::fs::remove_dir_all(&test_dir);
    }
}

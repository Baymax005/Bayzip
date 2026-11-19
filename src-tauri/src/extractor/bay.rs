use std::io::{BufRead, BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Instant;
use tauri::AppHandle;
use zeroize::Zeroize;

use crate::crypto::ChunkedAesReader;
use crate::inspector::{
    clear_readonly,
    get_optimal_extract_chunk_size,
    get_single_root_dir,
    safe_atomic_rename,
    sanitize_archive_path,
};
use crate::priority::ScopedPriorityGuard;
use super::{current_extraction_token, is_extraction_cancelled};
use super::io::{
    create_output_file_fast,
    open_sequential_read,
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

/// Decouples AES-256-GCM chunk decryption (`ChunkedAesReader`) onto a dedicated read-ahead
/// thread using a fixed-capacity ring of recycled `512 KB` buffers (`4 MB` total ring)
/// and implements `BufRead` so `zstd::stream::Decoder::with_buffer` reads directly with zero double-buffering.
struct PipedAesBufReader {
    data_rx: Option<Receiver<Result<Vec<u8>, std::io::Error>>>,
    recycle_tx: Option<SyncSender<Vec<u8>>>,
    current_buf: Vec<u8>,
    offset: usize,
    eof: bool,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl PipedAesBufReader {
    const RING_SLOTS: usize = 8;
    const BUF_CAP: usize = 512 * 1024;

    fn new<R: Read + Send + 'static>(
        mut enc_reader: ChunkedAesReader<R>,
        prefix: Option<[u8; 8]>,
    ) -> Self {
        let (data_tx, data_rx) =
            mpsc::sync_channel::<Result<Vec<u8>, std::io::Error>>(Self::RING_SLOTS);
        let (recycle_tx, recycle_rx) = mpsc::sync_channel::<Vec<u8>>(Self::RING_SLOTS);

        for _ in 0..Self::RING_SLOTS {
            let _ = recycle_tx.send(Vec::with_capacity(Self::BUF_CAP));
        }

        let cancel_token = current_extraction_token();
        let worker = std::thread::spawn(move || {
            let _priority_guard = ScopedPriorityGuard::new();
            loop {
                let cancelled = match &cancel_token {
                    Some(token) => token.is_cancelled(),
                    None => is_extraction_cancelled(),
                };
                if cancelled {
                    let _ = data_tx.send(Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Extraction cancelled by user",
                    )));
                    break;
                }

                let mut buf = match recycle_rx.recv() {
                    Ok(mut recycled) => {
                        recycled.resize(Self::BUF_CAP, 0);
                        recycled
                    }
                    Err(_) => break,
                };

                let mut filled = 0;
                let mut read_err: Option<std::io::Error> = None;
                while filled < buf.len() {
                    match enc_reader.read(&mut buf[filled..]) {
                        Ok(0) => break,
                        Ok(n) => filled += n,
                        Err(e) => {
                            read_err = Some(e);
                            break;
                        }
                    }
                }

                if filled > 0 {
                    buf.truncate(filled);
                    if let Err(mpsc::SendError(Ok(mut b))) = data_tx.send(Ok(buf)) {
                        b.zeroize();
                        break;
                    }
                } else {
                    buf.zeroize();
                    buf.clear();
                    drop(buf);
                }

                if let Some(err) = read_err {
                    let _ = data_tx.send(Err(err));
                    break;
                }

                if filled == 0 {
                    break;
                }
            }

            while let Ok(mut b) = recycle_rx.try_recv() {
                b.zeroize();
            }
        });

        Self {
            data_rx: Some(data_rx),
            recycle_tx: Some(recycle_tx),
            current_buf: prefix.map(|p| p.to_vec()).unwrap_or_default(),
            offset: 0,
            eof: false,
            worker: Some(worker),
        }
    }
}

impl BufRead for PipedAesBufReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.offset < self.current_buf.len() {
            return Ok(&self.current_buf[self.offset..]);
        }
        if self.eof {
            return Ok(&[]);
        }

        if self.current_buf.capacity() >= Self::BUF_CAP {
            let mut old = std::mem::take(&mut self.current_buf);
            old.zeroize();
            old.clear();
            if let Some(tx) = &self.recycle_tx {
                let _ = tx.try_send(old);
            }
        } else {
            self.current_buf.zeroize();
            self.current_buf.clear();
        }
        self.offset = 0;

        let Some(rx) = &self.data_rx else {
            self.eof = true;
            return Ok(&[]);
        };

        match rx.recv() {
            Ok(Ok(buf)) => {
                self.current_buf = buf;
                Ok(&self.current_buf)
            }
            Ok(Err(e)) => {
                self.eof = true;
                Err(e)
            }
            Err(_) => {
                self.eof = true;
                Ok(&[])
            }
        }
    }

    fn consume(&mut self, amt: usize) {
        self.offset = self.offset.saturating_add(amt).min(self.current_buf.len());
    }
}

impl Read for PipedAesBufReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let avail = self.fill_buf()?;
        if avail.is_empty() {
            return Ok(0);
        }
        let n = avail.len().min(buf.len());
        buf[..n].copy_from_slice(&avail[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl Drop for PipedAesBufReader {
    fn drop(&mut self) {
        self.current_buf.zeroize();
        self.current_buf.clear();
        if let Some(rx) = self.data_rx.take() {
            while let Ok(item) = rx.try_recv() {
                if let Ok(mut b) = item {
                    b.zeroize();
                }
            }
            drop(rx);
        }
        self.recycle_tx.take();
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
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
    let mut newly_created = Vec::new();
    let root_strip = single_root_prefix.filter(|prefix| {
        dest_dir
            .file_name()
            .and_then(|s| s.to_str())
            .map(base_folder_stem)
            .map(|dest_stem| dest_stem.eq_ignore_ascii_case(prefix))
            .unwrap_or(false)
    });

    let dest_was_empty = !dest_dir.exists()
        || std::fs::read_dir(dest_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);

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
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(extracted_files),
        Err(e) => return Err(format!("Error reading archive header: {}", e)),
    }

    let mut dir_cache = DirectoryCache::new();
    dir_cache.mark_existing(dest_dir);
    let mut writer_pool = AsyncFileWriterPool::new_for_dest(dest_dir);
    let chunk_size = get_optimal_extract_chunk_size(dest_dir);

    // 256 KB reusable I/O buffer (2 x ZSTD_DStreamOutSize)
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
                        return Err(
                            "Malformed or truncated .bay archive (missing EOF marker)".to_string(),
                        );
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
            decoder
                .read_exact(&mut len_bytes)
                .map_err(|e| format!("Failed to read entry name length: {}", e))?;
            let name_len = u32::from_le_bytes(len_bytes) as usize;
            if name_len > 65536 {
                return Err(
                    "Malformed archive: entry name length exceeds 64 KB safety limit".to_string(),
                );
            }
            let mut name_buf = vec![0u8; name_len];
            decoder
                .read_exact(&mut name_buf)
                .map_err(|e| format!("Failed to read entry name: {}", e))?;
            let rel_path_str = String::from_utf8_lossy(&name_buf).to_string();

            let mut size_bytes = [0u8; 8];
            decoder
                .read_exact(&mut size_bytes)
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
                Some(prefix) => match strip_root_component(&safe_rel_path, prefix) {
                    Some(stripped) => {
                        if stripped.as_os_str().is_empty() {
                            drain_bay_entry_bytes(&mut decoder, file_size, &mut io_buf)?;
                            continue;
                        }
                        stripped.to_path_buf()
                    }
                    None => safe_rel_path.clone(),
                },
                None => safe_rel_path.clone(),
            };

            let out_path = dest_dir.join(&effective_path);

            let is_dir = entry_type == 2 || rel_path_str.ends_with('/');
            if is_dir {
                dir_cache.ensure_dir_tracked(
                    &out_path,
                    dest_dir,
                    dest_was_empty,
                    &mut newly_created,
                )?;
                extracted_files.push(out_path.to_string_lossy().to_string());
                drain_bay_entry_bytes(&mut decoder, file_size, &mut io_buf)?;
                continue;
            }

            dir_cache.ensure_parent_tracked(
                &out_path,
                dest_dir,
                dest_was_empty,
                &mut newly_created,
            )?;

            let file_existed = if dest_was_empty {
                false
            } else {
                std::fs::symlink_metadata(&out_path).is_ok()
            };
            let out_str = out_path.to_string_lossy().to_string();

            if file_size <= ASYNC_INMEMORY_MAX_BYTES {
                let sz = file_size as usize;
                let mut payload = vec![0u8; sz];
                let mut read_off = 0;
                while read_off < sz {
                    if is_extraction_cancelled() {
                        return Err("Extraction cancelled by user".into());
                    }
                    let slice_end = (read_off + 256 * 1024).min(sz);
                    match decoder.read(&mut payload[read_off..slice_end]) {
                        Ok(0) => return Err("Premature EOF inside archive entry".into()),
                        Ok(n) => read_off += n,
                        Err(e) => {
                            return Err(format!("Error reading decompressed entry: {}", e));
                        }
                    }
                }

                if !file_existed {
                    newly_created.push(out_str.clone());
                }
                writer_pool.submit(out_path, payload, file_existed)?;
                total_extracted = total_extracted.saturating_add(file_size);

                emit_extract_progress(
                    app,
                    &mut last_emit,
                    &safe_rel_path.to_string_lossy(),
                    total_extracted,
                    total_expected_bytes,
                );
            } else {
                let seen_before = writer_pool.prepare_direct_write(&out_path)?;
                let effective_existed = file_existed || seen_before;
                if !effective_existed {
                    newly_created.push(out_str.clone());
                }

                let target_write_path: PathBuf = if effective_existed {
                    let mut p = out_path.as_os_str().to_os_string();
                    p.push(".baypart");
                    PathBuf::from(p)
                } else {
                    out_path.clone()
                };

                let out_file = match create_output_file_fast(&target_write_path, file_size) {
                    Ok(f) => f,
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        clear_readonly(&target_write_path);
                        create_output_file_fast(&target_write_path, file_size).map_err(|e2| {
                            format!("Failed to create output file {:?}: {}", target_write_path, e2)
                        })?
                    }
                    Err(e) => {
                        return Err(format!(
                            "Failed to create output file {:?}: {}",
                            target_write_path, e
                        ));
                    }
                };

                let mut writer = BufWriter::with_capacity(chunk_size, out_file);
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
                    let _ = std::fs::remove_file(&target_write_path);
                    return Err(err);
                }

                if let Err(e) = writer.flush() {
                    drop(writer);
                    let _ = std::fs::remove_file(&target_write_path);
                    return Err(format!("Flush error: {}", e));
                }
                drop(writer);

                if effective_existed {
                    if let Err(e) = safe_atomic_rename(&target_write_path, &out_path) {
                        let _ = std::fs::remove_file(&target_write_path);
                        return Err(format!(
                            "Failed to finalize extracted file {:?}: {}",
                            out_path, e
                        ));
                    }
                }
            }

            extracted_files.push(out_str);
        }

        Ok(())
    })();

    if let Err(e) = res {
        writer_pool.abort_and_join();
        cleanup_extracted_files(&newly_created);
        return Err(e);
    }

    if let Err(e) = writer_pool.finish() {
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
        std::fs::metadata(archive_path)
            .map(|m| m.len() * 2)
            .unwrap_or(1)
    };

    let single_root_prefix = if crate::inspector::has_instant_bay_toc(archive_path) {
        let (entries, _, _) = crate::inspector::list_bay_entries(archive_path, password);
        get_single_root_dir(&entries)
    } else {
        None
    };
    let s_prefix = single_root_prefix.as_deref();

    let raw_file = open_sequential_read(archive_path)
        .map_err(|e| format!("Failed to open .bay archive: {}", e))?;
    let read_cap = get_optimal_extract_chunk_size(archive_path).max(256 * 1024);
    let mut file = BufReader::with_capacity(read_cap, raw_file);

    let mut magic = [0u8; 8];
    let n = file
        .read(&mut magic)
        .map_err(|e| format!("Invalid or corrupted .bay header: {}", e))?;

    // Encrypted .bay v3 (3-stage pipeline: Read-Ahead AES-256-GCM -> Decompress -> Async Writer Pool)
    if n >= 7 && magic.starts_with(b"BAYENC\x03") {
        let pwd = match password {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => {
                return Err(
                    "Password required: This .bay archive is encrypted with AES-256-GCM.".into(),
                )
            }
        };

        file.seek(std::io::SeekFrom::Start(8))
            .map_err(|e| e.to_string())?;
        let mut enc_reader = ChunkedAesReader::new(file, pwd)
            .map_err(|e| format!("Failed to initialize decryption: {}", e))?;

        let mut inner_sig = [0u8; 8];
        enc_reader
            .read_exact(&mut inner_sig)
            .map_err(|_| "Incorrect password: Failed to decrypt AES-256-GCM archive.".to_string())?;

        if inner_sig.starts_with(b"BAYBRO\x02") {
            let piped = PipedAesBufReader::new(enc_reader, None);
            let decoder = brotli::Decompressor::new(piped, 64 * 1024);
            return extract_bay_stream(
                decoder,
                dest_dir,
                total_expected_bytes,
                app,
                s_prefix,
                excluded,
            );
        } else if inner_sig.starts_with(b"BAYLZ4\x02") {
            let piped = PipedAesBufReader::new(enc_reader, None);
            let decoder = lz4_flex::frame::FrameDecoder::new(piped);
            return extract_bay_stream(
                decoder,
                dest_dir,
                total_expected_bytes,
                app,
                s_prefix,
                excluded,
            );
        } else if inner_sig.starts_with(b"BAYZIP\x02") {
            let piped = PipedAesBufReader::new(enc_reader, None);
            let decoder = zstd::stream::Decoder::with_buffer(piped)
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return extract_bay_stream(
                decoder,
                dest_dir,
                total_expected_bytes,
                app,
                s_prefix,
                excluded,
            );
        } else {
            let piped = PipedAesBufReader::new(enc_reader, Some(inner_sig));
            let decoder = zstd::stream::Decoder::with_buffer(piped)
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return extract_bay_stream(
                decoder,
                dest_dir,
                total_expected_bytes,
                app,
                s_prefix,
                excluded,
            );
        }
    }

    // Legacy encrypted .bay v1 & v2 (deprecated in v2.0+)
    if n >= 7 && (magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
        return Err("Deprecated archive: Legacy v1/v2 encryption is no longer supported. Please re-encrypt using modern Bayzip v2.0+ streaming encryption.".to_string());
    }

    // Unencrypted .bay
    file.seek(std::io::SeekFrom::Start(0))
        .map_err(|e| e.to_string())?;
    let mut magic_header = [0u8; 8];
    let n = file.read(&mut magic_header).unwrap_or(0);

    if n >= 7 && magic_header.starts_with(b"BAYBRO\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 {
            8
        } else {
            7
        };
        file.seek(std::io::SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let decoder = brotli::Decompressor::new(file, 64 * 1024);
        extract_bay_stream(
            decoder,
            dest_dir,
            total_expected_bytes,
            app,
            s_prefix,
            excluded,
        )
    } else if n >= 7 && magic_header.starts_with(b"BAYLZ4\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 {
            8
        } else {
            7
        };
        file.seek(std::io::SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        extract_bay_stream(
            decoder,
            dest_dir,
            total_expected_bytes,
            app,
            s_prefix,
            excluded,
        )
    } else if n >= 7 && magic_header.starts_with(b"BAYZIP\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 {
            8
        } else {
            7
        };
        file.seek(std::io::SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let decoder = zstd::stream::Decoder::with_buffer(file)
            .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?
            .single_frame();
        extract_bay_stream(
            decoder,
            dest_dir,
            total_expected_bytes,
            app,
            s_prefix,
            excluded,
        )
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        // Raw LZ4 frame
        file.seek(std::io::SeekFrom::Start(0))
            .map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        extract_bay_stream(
            decoder,
            dest_dir,
            total_expected_bytes,
            app,
            s_prefix,
            excluded,
        )
    } else {
        // Standard Zstd
        file.seek(std::io::SeekFrom::Start(0))
            .map_err(|e| e.to_string())?;
        let decoder = zstd::stream::Decoder::with_buffer(file)
            .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?
            .single_frame();
        extract_bay_stream(
            decoder,
            dest_dir,
            total_expected_bytes,
            app,
            s_prefix,
            excluded,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::ExtractionTaskGuard;
    use std::fs::File;
    use std::io::Cursor;

    fn write_bay_file_entry<W: Write>(writer: &mut W, name: &str, data: &[u8]) {
        writer.write_all(&[1u8]).unwrap();
        let name_bytes = name.as_bytes();
        writer
            .write_all(&(name_bytes.len() as u32).to_le_bytes())
            .unwrap();
        writer.write_all(name_bytes).unwrap();
        writer
            .write_all(&(data.len() as u64).to_le_bytes())
            .unwrap();
        writer.write_all(data).unwrap();
    }

    fn write_bay_dir_entry<W: Write>(writer: &mut W, name: &str) {
        writer.write_all(&[2u8]).unwrap();
        let name_bytes = name.as_bytes();
        writer
            .write_all(&(name_bytes.len() as u32).to_le_bytes())
            .unwrap();
        writer.write_all(name_bytes).unwrap();
        writer.write_all(&0u64.to_le_bytes()).unwrap();
    }

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
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "simulated broken pipe",
            ))
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
        let test_dir = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_bay_eof_extract");
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
        let res = extract_bay_stream(
            std::io::Cursor::new(truncated),
            &test_dir,
            5,
            None,
            None,
            None,
        );
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("missing EOF marker"));
        assert!(!test_dir.join("hello.txt").exists());

        // 2. Encrypted stream truncated right after entry_type == 0 (missing ChunkedAesReader 0u32 EOF marker)
        let password = "stream_eof_pass";
        let mut raw_stream = Vec::new();
        {
            let mut writer =
                crate::crypto::ChunkedAesWriter::new(&mut raw_stream, password).unwrap();
            writer.write_all(&[1u8]).unwrap();
            let entry_name = b"ok.txt";
            writer
                .write_all(&(entry_name.len() as u32).to_le_bytes())
                .unwrap();
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

    #[test]
    fn test_extract_bay_archive_unencrypted_zstd_multifile_and_large_streaming() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_bay_pipelined_plain");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let bay_path = base.join("bundle.bay");
        let dest_dir = base.join("out");
        let large_payload: Vec<u8> = (0..(2 * 1024 * 1024 + 512 * 1024))
            .map(|i| ((i * 31) % 253) as u8)
            .collect();

        {
            let mut out = File::create(&bay_path).unwrap();
            out.write_all(b"BAYZIP\x02\x00").unwrap();
            let mut enc = zstd::stream::Encoder::new(out, 1).unwrap();
            write_bay_dir_entry(&mut enc, "docs/");
            write_bay_file_entry(&mut enc, "docs/Readme.txt", b"old-readme");
            write_bay_file_entry(&mut enc, "docs/readme.txt", b"new-readme");
            write_bay_file_entry(&mut enc, "docs/empty.txt", b"");
            write_bay_dir_entry(&mut enc, "excluded/");
            write_bay_file_entry(&mut enc, "excluded/secret.txt", b"hidden");
            write_bay_file_entry(&mut enc, "assets/large.bin", &large_payload);
            enc.write_all(&[0u8]).unwrap();
            let mut raw = enc.finish().unwrap();
            raw.flush().unwrap();
        }

        let excluded = crate::inspector::build_excluded_set(["excluded"]);
        let guard = ExtractionTaskGuard::new();
        let extracted = extract_bay_archive(
            &bay_path,
            &dest_dir,
            None,
            None,
            None,
            excluded.as_ref(),
        )
        .unwrap();

        assert!(!extracted.is_empty());
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("docs/readme.txt")).unwrap(),
            "new-readme"
        );
        assert_eq!(std::fs::read(dest_dir.join("docs/empty.txt")).unwrap(), b"");
        assert!(!dest_dir.join("excluded/secret.txt").exists());
        assert_eq!(
            std::fs::read(dest_dir.join("assets/large.bin")).unwrap(),
            large_payload
        );
        assert!(guard.bytes_extracted() > large_payload.len() as u64);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_extract_bay_archive_encrypted_v3_pipelined_roundtrip_and_wrong_password() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_bay_pipelined_enc");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let bay_path = base.join("secure.bay");
        let dest_dir = base.join("out");
        let password = "bay_pipeline_password";

        // Generate > 4 MB incompressible payload to span multiple 4 MB AES chunks
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut large_incompressible = vec![0u8; 4 * 1024 * 1024 + 256 * 1024];
        for chunk in large_incompressible.as_chunks_mut::<8>().0 {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            let v = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
            chunk.copy_from_slice(&v.to_le_bytes());
        }

        {
            let file = File::create(&bay_path).unwrap();
            let mut enc_writer = crate::crypto::ChunkedAesWriter::new(file, password).unwrap();
            enc_writer.write_all(b"BAYZIP\x02\x00").unwrap();
            let mut zstd_enc = zstd::stream::Encoder::new(enc_writer, 1).unwrap();
            write_bay_file_entry(&mut zstd_enc, "meta/info.txt", b"encrypted-pipeline-ok");
            write_bay_file_entry(&mut zstd_enc, "payload/data.bin", &large_incompressible);
            zstd_enc.write_all(&[0u8]).unwrap();
            let enc_writer = zstd_enc.finish().unwrap();
            let mut raw = enc_writer.finish().unwrap();
            raw.flush().unwrap();
        }

        // 1. Missing password fails cleanly
        let err_missing = extract_bay_archive(&bay_path, &dest_dir, None, None, None, None);
        assert!(err_missing.is_err());
        assert!(err_missing.unwrap_err().contains("Password required"));

        // 2. Wrong password fails cleanly
        let err_wrong =
            extract_bay_archive(&bay_path, &dest_dir, Some("bad_password"), None, None, None);
        assert!(err_wrong.is_err());
        assert!(err_wrong.unwrap_err().contains("Incorrect password"));

        // 3. Correct password decrypts & extracts across multi-chunk 3-stage pipeline
        let guard = ExtractionTaskGuard::new();
        let files =
            extract_bay_archive(&bay_path, &dest_dir, Some(password), None, None, None).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("meta/info.txt")).unwrap(),
            "encrypted-pipeline-ok"
        );
        assert_eq!(
            std::fs::read(dest_dir.join("payload/data.bin")).unwrap(),
            large_incompressible
        );
        assert!(guard.bytes_extracted() > large_incompressible.len() as u64);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_extract_bay_roundtrip_with_real_baytoc_and_bayetoc_trailers() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_bay_trailers");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        // A. Real Zstd (.bay) created by headless_quick_compress_bay_items (includes BAYTOC\x02\x00)
        let src_dir = base.join("src_proj");
        std::fs::create_dir_all(src_dir.join("nested")).unwrap();
        std::fs::write(src_dir.join("nested/info.txt"), b"real-baytoc-zstd").unwrap();
        std::fs::write(src_dir.join("empty.txt"), b"").unwrap();
        let blob: Vec<u8> = (0..(300 * 1024)).map(|i| (i % 251) as u8).collect();
        std::fs::write(src_dir.join("blob.bin"), &blob).unwrap();

        let zstd_bay = base.join("real_zstd.bay");
        crate::compressor::bay::headless_quick_compress_bay_items(
            &[src_dir.as_path()],
            &zstd_bay,
            false,
        )
        .unwrap();
        assert!(crate::inspector::has_instant_bay_toc(&zstd_bay));

        let out_zstd = base.join("out_zstd");
        let files_zstd = extract_bay_archive(&zstd_bay, &out_zstd, None, None, None, None).unwrap();
        assert_eq!(files_zstd.len(), 5); // 2 dirs + 3 files
        assert_eq!(
            std::fs::read_to_string(out_zstd.join("src_proj/nested/info.txt")).unwrap(),
            "real-baytoc-zstd"
        );
        assert_eq!(std::fs::read(out_zstd.join("src_proj/empty.txt")).unwrap(), b"");
        assert_eq!(std::fs::read(out_zstd.join("src_proj/blob.bin")).unwrap(), blob);

        let (tested_count, tested_bytes, test_errors) =
            crate::tester::test_bay_archive(&zstd_bay, None, None).unwrap();
        assert!(test_errors.is_empty(), "Unexpected test errors: {:?}", test_errors);
        assert_eq!(tested_count, 5); // 2 dirs + 3 files
        assert!(tested_bytes >= blob.len() as u64);

        // B. LZ4 + BAYTOC\x02\x00 trailer
        let lz4_bay = base.join("lz4_toc.bay");
        {
            let mut raw = File::create(&lz4_bay).unwrap();
            raw.write_all(b"BAYLZ4\x02\x00").unwrap();
            let mut enc = lz4_flex::frame::FrameEncoder::new(raw);
            write_bay_file_entry(&mut enc, "a.txt", b"lz4-baytoc-ok");
            enc.write_all(&[0u8]).unwrap();
            let mut raw = enc.finish().unwrap();
            let toc = vec![crate::types::ArchiveEntryItem {
                name: "a.txt".to_string(),
                size: 13,
                is_dir: false,
            }];
            crate::compressor::bay::write_bay_toc_trailer(&mut raw, &toc, None).unwrap();
        }
        assert!(crate::inspector::has_instant_bay_toc(&lz4_bay));
        let out_lz4 = base.join("out_lz4");
        let files_lz4 = extract_bay_archive(&lz4_bay, &out_lz4, None, None, None, None).unwrap();
        assert_eq!(files_lz4.len(), 1);
        assert_eq!(std::fs::read_to_string(out_lz4.join("a.txt")).unwrap(), "lz4-baytoc-ok");

        // C. Brotli + BAYTOC\x02\x00 trailer
        let bro_bay = base.join("bro_toc.bay");
        {
            let mut raw = File::create(&bro_bay).unwrap();
            raw.write_all(b"BAYBRO\x02\x00").unwrap();
            let mut enc = brotli::CompressorWriter::new(raw, 4096, 1, 20);
            write_bay_file_entry(&mut enc, "b.txt", b"brotli-baytoc-ok");
            enc.write_all(&[0u8]).unwrap();
            let mut raw = enc.into_inner();
            let toc = vec![crate::types::ArchiveEntryItem {
                name: "b.txt".to_string(),
                size: 16,
                is_dir: false,
            }];
            crate::compressor::bay::write_bay_toc_trailer(&mut raw, &toc, None).unwrap();
        }
        assert!(crate::inspector::has_instant_bay_toc(&bro_bay));
        let out_bro = base.join("out_bro");
        let files_bro = extract_bay_archive(&bro_bay, &out_bro, None, None, None, None).unwrap();
        assert_eq!(files_bro.len(), 1);
        assert_eq!(std::fs::read_to_string(out_bro.join("b.txt")).unwrap(), "brotli-baytoc-ok");

        // D. Encrypted BAYENC\x03 + BAYZIP\x02\x00 + BAYETOC\x03 trailer
        let enc_bay = base.join("enc_etoc.bay");
        let pwd = "trailer_enc_password";
        {
            let raw = File::create(&enc_bay).unwrap();
            let mut enc_writer = crate::crypto::ChunkedAesWriter::new(raw, pwd).unwrap();
            enc_writer.write_all(b"BAYZIP\x02\x00").unwrap();
            let mut zstd_enc = zstd::stream::Encoder::new(enc_writer, 1).unwrap();
            write_bay_file_entry(&mut zstd_enc, "sec/key.txt", b"encrypted-bayetoc-ok");
            zstd_enc.write_all(&[0u8]).unwrap();
            let enc_writer = zstd_enc.finish().unwrap();
            let mut raw = enc_writer.finish().unwrap();
            let toc = vec![crate::types::ArchiveEntryItem {
                name: "sec/key.txt".to_string(),
                size: 20,
                is_dir: false,
            }];
            crate::compressor::bay::write_bay_toc_trailer(&mut raw, &toc, Some(pwd)).unwrap();
        }
        assert!(crate::inspector::has_instant_bay_toc(&enc_bay));
        let out_enc = base.join("out_enc");
        let files_enc =
            extract_bay_archive(&enc_bay, &out_enc, Some(pwd), None, None, None).unwrap();
        assert_eq!(files_enc.len(), 1);
        assert_eq!(
            std::fs::read_to_string(out_enc.join("sec/key.txt")).unwrap(),
            "encrypted-bayetoc-ok"
        );
        let (_, _, enc_test_errors) =
            crate::tester::test_bay_archive(&enc_bay, Some(pwd), None).unwrap();
        assert!(enc_test_errors.is_empty(), "Unexpected enc test errors: {:?}", enc_test_errors);

        let _ = std::fs::remove_dir_all(&base);
    }
}

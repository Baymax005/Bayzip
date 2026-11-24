use std::fs::File;
use std::io::{Read, Seek, BufReader};
use std::path::Path;
use std::time::Instant;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use sevenz_rust2::Password;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::types::{
    ArchiveFormat, TestArchiveProgress, TestArchiveResponse, TestArchiveErrorItem, TaskCancelToken,
};
use crate::priority::ScopedPriorityGuard;
use crate::crypto::ChunkedAesReader;
use crate::inspector::detect_archive_type;

thread_local! {
    static CURRENT_TEST_TOKEN: RefCell<Option<TaskCancelToken>> = const { RefCell::new(None) };
}

static ACTIVE_TEST_TOKEN: Mutex<Vec<TaskCancelToken>> = Mutex::new(Vec::new());

pub struct TestTaskGuard {
    token: TaskCancelToken,
}

impl TestTaskGuard {
    pub fn new() -> Self {
        let token = TaskCancelToken::new();
        CURRENT_TEST_TOKEN.with(|cell| {
            *cell.borrow_mut() = Some(token.clone());
        });
        if let Ok(mut lock) = ACTIVE_TEST_TOKEN.lock() {
            lock.push(token.clone());
        }
        Self { token }
    }

    pub fn token(&self) -> TaskCancelToken {
        self.token.clone()
    }
}

impl Default for TestTaskGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestTaskGuard {
    fn drop(&mut self) {
        CURRENT_TEST_TOKEN.with(|cell| {
            *cell.borrow_mut() = None;
        });
        if let Ok(mut lock) = ACTIVE_TEST_TOKEN.lock() {
            lock.retain(|t| !Arc::ptr_eq(&t.0, &self.token.0));
        }
    }
}

pub fn cancel_test_flag() {
    if let Ok(lock) = ACTIVE_TEST_TOKEN.lock() {
        for token in lock.iter() {
            token.cancel();
        }
    }
}

pub fn is_test_cancelled() -> bool {
    let thread_cancelled = CURRENT_TEST_TOKEN.with(|cell| {
        cell.borrow().as_ref().map(|t| t.is_cancelled())
    });
    if let Some(cancelled) = thread_cancelled {
        return cancelled;
    }
    if let Ok(lock) = ACTIVE_TEST_TOKEN.lock() {
        return lock.iter().any(|t| t.is_cancelled());
    }
    false
}

pub fn reset_test_cancel_flag() {
    // Retained for backwards compatibility; TestTaskGuard provides isolated tokens per task
}

fn emit_progress(
    app: Option<&AppHandle>,
    last_emit: &mut Instant,
    current_file: &str,
    bytes_tested: u64,
    total_bytes: u64,
) {
    #[cfg(not(test))]
    if let Some(app_handle) = app {
        if last_emit.elapsed().as_millis() >= 40 {
            *last_emit = Instant::now();
            let pct = if total_bytes > 0 {
                ((bytes_tested as f64 / total_bytes as f64) * 100.0).min(99.0)
            } else {
                50.0
            };
            let _ = app_handle.emit("test_progress", TestArchiveProgress {
                percent: pct,
                current_file: current_file.to_string(),
                bytes_tested,
                total_bytes,
            });
        }
    }
    #[cfg(test)]
    let _ = (app, last_emit, current_file, bytes_tested, total_bytes);
}

pub fn test_bay_stream<R: Read>(
    mut decoder: R,
    total_expected_bytes: u64,
    app: Option<&AppHandle>,
) -> Result<(usize, u64, Vec<TestArchiveErrorItem>), String> {
    let mut tested_count = 0usize;
    let mut total_tested = 0u64;
    let mut errors = Vec::new();
    let mut last_emit = Instant::now();

    let mut io_buf = vec![0u8; 256 * 1024];
    let mut saw_any_entry = false;

    loop {
        if is_test_cancelled() {
            return Err("Testing cancelled by user".to_string());
        }

        let mut b = [0u8; 1];
        let entry_type = loop {
            match decoder.read(&mut b) {
                Ok(0) => {
                    if !saw_any_entry {
                        return Ok((0, 0, errors));
                    }
                    errors.push(TestArchiveErrorItem {
                        file_name: "Archive Stream".to_string(),
                        error: "Unexpected end of stream (missing EOF marker)".to_string(),
                    });
                    return Ok((tested_count, total_tested, errors));
                }
                Ok(_) => {
                    saw_any_entry = true;
                    break b[0];
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: "Archive Stream".to_string(),
                        error: format!("Stream header read error: {}", e),
                    });
                    return Ok((tested_count, total_tested, errors));
                }
            }
        };

        if entry_type == 0 {
            loop {
                match decoder.read(&mut io_buf) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => {
                        errors.push(TestArchiveErrorItem {
                            file_name: "Stream EOF".to_string(),
                            error: format!(
                                "Archive stream integrity or decryption verification failed at EOF: {}",
                                e
                            ),
                        });
                        break;
                    }
                }
            }
            break;
        }
        if entry_type != 1 && entry_type != 2 {
            errors.push(TestArchiveErrorItem {
                file_name: "Archive Stream".to_string(),
                error: format!("Malformed .bay archive: invalid entry type {}", entry_type),
            });
            break;
        }

        let mut len_bytes = [0u8; 4];
        if let Err(e) = decoder.read_exact(&mut len_bytes) {
            errors.push(TestArchiveErrorItem {
                file_name: "Archive Entry".to_string(),
                error: format!("Failed to read entry name length: {}", e),
            });
            break;
        }
        let name_len = u32::from_le_bytes(len_bytes) as usize;
        if name_len > 65536 {
            errors.push(TestArchiveErrorItem {
                file_name: "Archive Entry".to_string(),
                error: "Malformed entry name length exceeds 64 KB".to_string(),
            });
            break;
        }

        let mut name_buf = vec![0u8; name_len];
        if let Err(e) = decoder.read_exact(&mut name_buf) {
            errors.push(TestArchiveErrorItem {
                file_name: "Archive Entry".to_string(),
                error: format!("Failed to read entry name: {}", e),
            });
            break;
        }
        let rel_path_str = String::from_utf8_lossy(&name_buf).to_string();

        let mut size_bytes = [0u8; 8];
        if let Err(e) = decoder.read_exact(&mut size_bytes) {
            errors.push(TestArchiveErrorItem {
                file_name: rel_path_str,
                error: format!("Failed to read entry size: {}", e),
            });
            break;
        }
        let file_size = u64::from_le_bytes(size_bytes);

        tested_count += 1;
        emit_progress(app, &mut last_emit, &rel_path_str, total_tested, total_expected_bytes);

        // Read through file payload to verify Zstd/stream checksums without writing to disk
        let mut remaining = file_size;
        let mut read_failed = false;
        while remaining > 0 {
            if is_test_cancelled() {
                return Err("Testing cancelled by user".to_string());
            }
            let to_read = (remaining as usize).min(io_buf.len());
            match decoder.read(&mut io_buf[..to_read]) {
                Ok(0) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: rel_path_str.clone(),
                        error: "Unexpected end of stream (file truncated)".to_string(),
                    });
                    read_failed = true;
                    break;
                }
                Ok(n) => {
                    remaining -= n as u64;
                    total_tested += n as u64;
                    emit_progress(app, &mut last_emit, &rel_path_str, total_tested, total_expected_bytes);
                }
                Err(e) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: rel_path_str.clone(),
                        error: format!("Decompression error: {}", e),
                    });
                    read_failed = true;
                    break;
                }
            }
        }

        if read_failed {
            break;
        }
    }

    Ok((tested_count, total_tested, errors))
}

pub fn test_bay_archive(
    archive_path: &Path,
    password: Option<&str>,
    app: Option<&AppHandle>,
) -> Result<(usize, u64, Vec<TestArchiveErrorItem>), String> {
    let total_expected_bytes: u64 = std::fs::metadata(archive_path)
        .map(|m| m.len() * 2)
        .unwrap_or(1);

    let raw_file = File::open(archive_path)
        .map_err(|e| format!("Failed to open .bay archive: {}", e))?;
    let mut file = BufReader::with_capacity(1024 * 1024, raw_file);

    let mut magic = [0u8; 8];
    let n = file.read(&mut magic)
        .map_err(|e| format!("Invalid or corrupted .bay header: {}", e))?;
    if n < 8 {
        return Err("Invalid or unsupported .bay archive header".to_string());
    }

    // Encrypted .bay v4 (Argon2id + Chunked AEAD)
    if &magic == crate::crypto::BAY_ENC_V4_MAGIC {
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

        if &inner_sig == crate::inspector::bay::BAY_BROTLI_MAGIC {
            let decoder = brotli::Decompressor::new(enc_reader, 64 * 1024);
            return test_bay_stream(decoder, total_expected_bytes, app);
        } else if &inner_sig == crate::inspector::bay::BAY_LZ4_MAGIC {
            let decoder = lz4_flex::frame::FrameDecoder::new(enc_reader);
            return test_bay_stream(decoder, total_expected_bytes, app);
        } else if &inner_sig == crate::inspector::bay::BAY_ZSTD_MAGIC {
            let decoder = zstd::stream::Decoder::new(enc_reader)
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return test_bay_stream(decoder, total_expected_bytes, app);
        } else {
            return Err("Invalid or unsupported inner .bay header".to_string());
        }
    }

    // Unencrypted .bay (exact 8-byte header)
    if &magic == crate::inspector::bay::BAY_BROTLI_MAGIC {
        file.seek(std::io::SeekFrom::Start(8)).map_err(|e| e.to_string())?;
        let decoder = brotli::Decompressor::new(file, 64 * 1024);
        test_bay_stream(decoder, total_expected_bytes, app)
    } else if &magic == crate::inspector::bay::BAY_LZ4_MAGIC {
        file.seek(std::io::SeekFrom::Start(8)).map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        test_bay_stream(decoder, total_expected_bytes, app)
    } else if &magic == crate::inspector::bay::BAY_ZSTD_MAGIC {
        test_unencrypted_zstd_bay(file.into_inner(), 8, total_expected_bytes, app)
    } else {
        Err("Invalid or unsupported .bay archive header".to_string())
    }
}

struct SeekTableVerifyReader {
    file: BufReader<File>,
    frames: Vec<crate::inspector::bay::ZstdSeekFrame>,
    frame_idx: usize,
    decompressor: zstd::bulk::Decompressor<'static>,
    comp_buf: Vec<u8>,
    decomp_buf: Vec<u8>,
    decomp_offset: usize,
    decomp_len: usize,
    seek_table_error: Option<String>,
}

impl SeekTableVerifyReader {
    fn new(
        mut raw: File,
        offset: u64,
        table: crate::inspector::bay::ZstdSeekTable,
    ) -> Result<Self, String> {
        raw.seek(std::io::SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let decompressor = zstd::bulk::Decompressor::new()
            .map_err(|e| format!("Failed to create Zstd decompressor: {}", e))?;
        Ok(Self {
            file: BufReader::with_capacity(1024 * 1024, raw),
            frames: table.frames,
            frame_idx: 0,
            decompressor,
            comp_buf: Vec::new(),
            decomp_buf: Vec::with_capacity(table.max_decomp_frame_bytes),
            decomp_offset: 0,
            decomp_len: 0,
            seek_table_error: None,
        })
    }
}

impl Read for SeekTableVerifyReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.decomp_offset >= self.decomp_len {
            if self.frame_idx >= self.frames.len() {
                return Ok(0);
            }
            if is_test_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Testing cancelled by user",
                ));
            }
            let frame = self.frames[self.frame_idx];
            self.comp_buf.resize(frame.comp_size as usize, 0);
            self.file.read_exact(&mut self.comp_buf)?;

            let expected = frame.decomp_size as usize;
            self.decomp_buf.clear();
            if self.decomp_buf.capacity() < expected {
                self.decomp_buf
                    .reserve(expected - self.decomp_buf.capacity());
            }

            let decoded = self
                .decompressor
                .decompress_to_buffer(&self.comp_buf, &mut self.decomp_buf)
                .map_err(|e| {
                    let msg = format!(
                        "Zstd Seek Table frame {} decode error: {}",
                        self.frame_idx, e
                    );
                    self.seek_table_error = Some(msg.clone());
                    std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
                })?;

            if decoded != expected {
                let msg = format!(
                    "Seek Table frame {} size mismatch: expected {} bytes, decoded {} bytes",
                    self.frame_idx, expected, decoded
                );
                self.seek_table_error = Some(msg.clone());
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, msg));
            }

            self.decomp_offset = 0;
            self.decomp_len = decoded;
            self.frame_idx += 1;
        }

        let avail = self.decomp_len - self.decomp_offset;
        let n = avail.min(buf.len());
        buf[..n].copy_from_slice(&self.decomp_buf[self.decomp_offset..self.decomp_offset + n]);
        self.decomp_offset += n;
        Ok(n)
    }
}

fn test_unencrypted_zstd_bay(
    mut raw: File,
    offset: u64,
    total_expected_bytes: u64,
    app: Option<&AppHandle>,
) -> Result<(usize, u64, Vec<TestArchiveErrorItem>), String> {
    let file_len = raw.metadata().map(|m| m.len()).unwrap_or(0);
    let payload_end = crate::inspector::bay::locate_bay_payload_end(&mut raw, file_len);

    if payload_end >= offset.saturating_add(17) {
        let mut tail_magic = [0u8; 4];
        if raw.seek(std::io::SeekFrom::Start(payload_end - 4)).is_ok()
            && raw.read_exact(&mut tail_magic).is_ok()
            && tail_magic == crate::inspector::bay::ZSTD_SEEKABLE_FOOTER_MAGIC.to_le_bytes()
        {
            let Some(seek_table) =
                crate::inspector::bay::read_zstd_seek_table(&mut raw, offset, payload_end)
            else {
                return Ok((
                    0,
                    0,
                    vec![TestArchiveErrorItem {
                        file_name: "Zstd Seek Table".to_string(),
                        error: "Corrupted RFC 8878 Zstd seek table header or frame bounds"
                            .to_string(),
                    }],
                ));
            };

            let mut reader = SeekTableVerifyReader::new(raw, offset, seek_table)?;
            let test_res = test_bay_stream(&mut reader, total_expected_bytes, app);
            if let Some(seek_err) = reader.seek_table_error.take() {
                let (count, tested, mut errors) = test_res.unwrap_or_default();
                if !errors.iter().any(|e| e.file_name == "Zstd Seek Table") {
                    errors.push(TestArchiveErrorItem {
                        file_name: "Zstd Seek Table".to_string(),
                        error: seek_err,
                    });
                }
                return Ok((count, tested, errors));
            }
            return test_res;
        }
    }

    raw.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let bounded = BufReader::with_capacity(
        1024 * 1024,
        raw.take(payload_end.saturating_sub(offset)),
    );
    let decoder = zstd::stream::Decoder::with_buffer(bounded)
        .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;
    test_bay_stream(decoder, total_expected_bytes, app)
}

pub fn test_zip_archive(
    archive_path: &Path,
    password: Option<&str>,
    app: Option<&AppHandle>,
) -> Result<(usize, u64, Vec<TestArchiveErrorItem>), String> {
    let raw_file = File::open(archive_path)
        .map_err(|e| format!("Failed to open zip archive: {}", e))?;
    let reader = BufReader::with_capacity(2 * 1024 * 1024, raw_file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| format!("Invalid zip archive: {}", e))?;

    let total_entries = archive.len();
    let total_expected_bytes: u64 = (0..total_entries)
        .filter_map(|i| archive.by_index(i).ok().map(|e| e.size()))
        .sum();

    let mut tested_count = 0usize;
    let mut total_tested = 0u64;
    let mut errors = Vec::new();
    let mut last_emit = Instant::now();
    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let mut io_buf = vec![0u8; 256 * 1024];

    for i in 0..total_entries {
        if is_test_cancelled() {
            return Err("Testing cancelled by user".to_string());
        }

        let mut file = match pwd_opt {
            Some(pwd) => match archive.by_index_decrypt(i, pwd.as_bytes()) {
                Ok(f) => f,
                Err(zip::result::ZipError::InvalidPassword) => {
                    return Err("Incorrect password: Could not decrypt file".to_string());
                }
                Err(e) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: format!("Entry #{}", i),
                        error: format!("Zip entry error: {}", e),
                    });
                    continue;
                }
            },
            None => match archive.by_index(i) {
                Ok(f) => f,
                Err(zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED)) => {
                    return Err("Password required: This ZIP archive is encrypted with AES/password.".to_string());
                }
                Err(zip::result::ZipError::InvalidPassword) => {
                    return Err("Password required: This ZIP archive is encrypted.".to_string());
                }
                Err(e) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: format!("Entry #{}", i),
                        error: format!("Zip entry error: {}", e),
                    });
                    continue;
                }
            },
        };

        let file_name = file.name().to_string();
        if file.is_dir() {
            continue;
        }

        tested_count += 1;
        emit_progress(app, &mut last_emit, &file_name, total_tested, total_expected_bytes);

        // Decompress the entire stream to sink.
        // In zip-rs, reading to EOF automatically validates CRC32 against the local file header!
        loop {
            if is_test_cancelled() {
                return Err("Testing cancelled by user".to_string());
            }
            match file.read(&mut io_buf) {
                Ok(0) => break,
                Ok(n) => {
                    total_tested += n as u64;
                    emit_progress(app, &mut last_emit, &file_name, total_tested, total_expected_bytes);
                }
                Err(e) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: file_name.clone(),
                        error: format!("CRC32 or data error: {}", e),
                    });
                    break;
                }
            }
        }
    }

    Ok((tested_count, total_tested, errors))
}

pub fn test_7z_archive(
    archive_path: &Path,
    password: Option<&str>,
    app: Option<&AppHandle>,
) -> Result<(usize, u64, Vec<TestArchiveErrorItem>), String> {
    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let raw_file = File::open(archive_path)
        .map_err(|e| format!("Failed to open 7z: {}", e))?;
    let reader = BufReader::with_capacity(2 * 1024 * 1024, raw_file);
    let pwd = pwd_opt.map(Password::from).unwrap_or_else(Password::empty);

    let mut sz_reader = match sevenz_rust2::ArchiveReader::new(reader, pwd) {
        Ok(r) => r,
        Err(sevenz_rust2::Error::PasswordRequired) => {
            return Err("Password required: This 7z archive is encrypted with AES-256.".into());
        }
        Err(e) => {
            if crate::inspector::is_archive_encrypted(archive_path) {
                return Err(if pwd_opt.is_none() {
                    "Password required: This 7z archive is encrypted with AES-256.".into()
                } else {
                    "Incorrect password: Could not decrypt 7z archive.".into()
                });
            }
            return Err(format!("7z open error: {}", e));
        }
    };

    let total_expected_bytes: u64 = sz_reader.archive().files.iter().map(|f| f.size()).sum();
    let mut tested_count = 0usize;
    let mut total_tested = 0u64;
    let mut errors = Vec::new();
    let mut last_emit = Instant::now();
    let mut io_buf = vec![0u8; 256 * 1024];

    let res = sz_reader.for_each_entries(|entry, reader| {
        if is_test_cancelled() {
            return Err(sevenz_rust2::Error::from(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Testing cancelled by user",
            )));
        }

        let name = entry.name().to_string();
        if entry.is_directory() {
            return Ok(true);
        }

        tested_count += 1;
        emit_progress(app, &mut last_emit, &name, total_tested, total_expected_bytes);

        loop {
            if is_test_cancelled() {
                return Ok(false);
            }
            match reader.read(&mut io_buf) {
                Ok(0) => break,
                Ok(n) => {
                    total_tested += n as u64;
                    emit_progress(app, &mut last_emit, &name, total_tested, total_expected_bytes);
                }
                Err(e) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: name.clone(),
                        error: format!("7z stream error: {}", e),
                    });
                    break;
                }
            }
        }

        Ok(true)
    });

    if let Err(e) = res {
        if is_test_cancelled() {
            return Err("Testing cancelled by user".to_string());
        }
        if e.to_string().to_lowercase().contains("password") || crate::inspector::is_archive_encrypted(archive_path) {
            return Err(if pwd_opt.is_none() {
                "Password required: This 7z archive is encrypted with AES-256.".into()
            } else {
                "Incorrect password: Could not decrypt 7z archive.".into()
            });
        }
        errors.push(TestArchiveErrorItem {
            file_name: "7z Container".to_string(),
            error: format!("Archive decoding error: {}", e),
        });
    }

    if is_test_cancelled() {
        return Err("Testing cancelled by user".to_string());
    }

    Ok((tested_count, total_tested, errors))
}

pub fn test_tar_archive(
    archive_path: &Path,
    app: Option<&AppHandle>,
) -> Result<(usize, u64, Vec<TestArchiveErrorItem>), String> {
    let path_str = archive_path.to_string_lossy().to_lowercase();
    let detected = detect_archive_type(archive_path);
    if path_str.ends_with(".bz2")
        || path_str.ends_with(".tbz2")
        || path_str.ends_with(".xz")
        || path_str.ends_with(".txz")
        || path_str.ends_with(".iso")
        || path_str.ends_with(".cab")
        || (path_str.ends_with(".zst") && !path_str.ends_with(".tar.zst"))
        || matches!(
            detected,
            ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Iso | ArchiveFormat::Cab
        )
    {
        return Err("Delegate compressed container to system tar".into());
    }

    let raw_file = File::open(archive_path)
        .map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut reader = BufReader::with_capacity(1024 * 1024, raw_file);
    let mut magic = [0u8; 4];
    let n = reader.read(&mut magic).unwrap_or(0);
    reader
        .seek(std::io::SeekFrom::Start(0))
        .map_err(|e| format!("Failed to rewind tar reader: {}", e))?;

    let is_gz = (n >= 2 && magic[0] == 0x1F && magic[1] == 0x8B)
        || path_str.ends_with(".tar.gz")
        || path_str.ends_with(".tgz");
    let is_zst = (n >= 4 && magic[0..4] == [0x28, 0xB5, 0x2F, 0xFD])
        || path_str.ends_with(".tar.zst");

    let mut tested_count = 0usize;
    let mut total_tested = 0u64;
    let mut errors = Vec::new();
    let mut last_emit = Instant::now();
    let mut io_buf = vec![0u8; 256 * 1024];

    let mut process_tar = |mut archive: tar::Archive<Box<dyn Read>>| -> Result<(), String> {
        let entries = archive.entries().map_err(|e| format!("Tar entries error: {}", e))?;
        for entry_res in entries {
            if is_test_cancelled() {
                return Err("Testing cancelled by user".to_string());
            }
            match entry_res {
                Ok(mut entry) => {
                    let name = entry.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                    tested_count += 1;
                    emit_progress(app, &mut last_emit, &name, total_tested, 0);

                    loop {
                        if is_test_cancelled() {
                            return Err("Testing cancelled by user".to_string());
                        }
                        match entry.read(&mut io_buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                total_tested += n as u64;
                                emit_progress(app, &mut last_emit, &name, total_tested, 0);
                            }
                            Err(e) => {
                                errors.push(TestArchiveErrorItem {
                                    file_name: name.clone(),
                                    error: format!("Tar checksum/data error: {}", e),
                                });
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: "Tar block".to_string(),
                        error: format!("Tar header read error: {}", e),
                    });
                }
            }
        }
        Ok(())
    };

    if is_gz {
        let gz = flate2::bufread::MultiGzDecoder::new(reader);
        let archive = tar::Archive::new(Box::new(gz) as Box<dyn Read>);
        process_tar(archive)?;
    } else if is_zst {
        let zstd_dec = zstd::stream::Decoder::with_buffer(reader).map_err(|e| e.to_string())?;
        let archive = tar::Archive::new(Box::new(zstd_dec) as Box<dyn Read>);
        process_tar(archive)?;
    } else {
        let archive = tar::Archive::new(Box::new(reader) as Box<dyn Read>);
        process_tar(archive)?;
    }

    Ok((tested_count, total_tested, errors))
}

pub fn test_with_system_tar(
    archive_path: &Path,
    app: Option<&AppHandle>,
) -> Result<(usize, u64, Vec<TestArchiveErrorItem>), String> {
    let mut cmd = std::process::Command::new(crate::inspector::get_system32_exe("tar.exe"));
    cmd.args(["-xvf"]).arg(archive_path).arg("-O");
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(crate::priority::get_child_process_creation_flags());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn tar: {}", e))?;
    let total_expected_bytes: u64 = std::fs::metadata(archive_path)
        .map(|m| m.len() * 2)
        .unwrap_or(0);

    let tested_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let current_entry = Arc::new(Mutex::new(String::from("Verifying archive stream...")));

    let stderr_handle = child.stderr.take().map(|stderr| {
        let tested_ref = tested_count.clone();
        let entry_ref = current_entry.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = BufReader::new(stderr);
            let mut diagnostics = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(entry_name) = trimmed.strip_prefix("x ") {
                    tested_ref.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let Ok(mut guard) = entry_ref.lock() {
                        *guard = entry_name.to_string();
                    }
                } else {
                    diagnostics.push(trimmed.to_string());
                }
            }
            diagnostics
        })
    });

    let mut total_tested = 0u64;
    let mut last_emit = Instant::now();
    let mut discard_buf = vec![0u8; 256 * 1024];

    if let Some(mut stdout) = child.stdout.take() {
        loop {
            if is_test_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(handle) = stderr_handle {
                    let _ = handle.join();
                }
                return Err("Testing cancelled by user".to_string());
            }
            match stdout.read(&mut discard_buf) {
                Ok(0) => break,
                Ok(n) => {
                    total_tested = total_tested.saturating_add(n as u64);
                    let label = current_entry
                        .lock()
                        .map(|g| g.clone())
                        .unwrap_or_else(|_| "Verifying archive stream...".to_string());
                    emit_progress(app, &mut last_emit, &label, total_tested, total_expected_bytes);
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(handle) = stderr_handle {
                        let _ = handle.join();
                    }
                    return Err(format!("Tar stream read error: {}", e));
                }
            }
        }
    }

    if is_test_cancelled() {
        let _ = child.kill();
        let _ = child.wait();
        if let Some(handle) = stderr_handle {
            let _ = handle.join();
        }
        return Err("Testing cancelled by user".to_string());
    }

    let status = child.wait().map_err(|e| format!("Tar process error: {}", e))?;
    let stderr_lines = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    let final_count = tested_count.load(std::sync::atomic::Ordering::Relaxed);
    let mut errors = Vec::new();
    if !status.success() {
        let err_msg = stderr_lines.join("\n");
        errors.push(TestArchiveErrorItem {
            file_name: archive_path.to_string_lossy().to_string(),
            error: if err_msg.trim().is_empty() {
                "Archive corrupted or invalid format".to_string()
            } else {
                err_msg
            },
        });
    }

    Ok((final_count, total_tested, errors))
}

fn should_abort_test_fallback(err: &str) -> bool {
    let lower = err.to_lowercase();
    is_test_cancelled()
        || lower.contains("cancelled")
        || lower.contains("canceled")
        || lower.contains("password")
        || lower.contains("decrypt")
}

pub fn test_archive_sync(
    app: AppHandle,
    archive_path: String,
    password: Option<String>,
) -> Result<TestArchiveResponse, String> {
    let _task_guard = TestTaskGuard::new();
    reset_test_cancel_flag();
    let _priority_guard = ScopedPriorityGuard::new();

    let path = Path::new(&archive_path);
    if !path.exists() {
        return Err("Archive file does not exist".into());
    }

    let start_time = Instant::now();
    let format = detect_archive_type(path);
    let pwd_opt = password.as_deref().map(|p| p.trim()).filter(|p| !p.is_empty());

    let test_result = match format {
        ArchiveFormat::Bay => test_bay_archive(path, pwd_opt, Some(&app)),
        ArchiveFormat::Zip => test_zip_archive(path, pwd_opt, Some(&app))
            .or_else(|e| {
                if should_abort_test_fallback(&e) {
                    return Err(e);
                }
                test_bay_archive(path, pwd_opt, Some(&app))
                    .or_else(|e2| {
                        if should_abort_test_fallback(&e2) {
                            return Err(e2);
                        }
                        test_with_system_tar(path, Some(&app))
                    })
            }),
        ArchiveFormat::SevenZip => test_7z_archive(path, pwd_opt, Some(&app))
            .or_else(|e| {
                if should_abort_test_fallback(&e) {
                    return Err(e);
                }
                test_with_system_tar(path, Some(&app))
            }),
        ArchiveFormat::Tar | ArchiveFormat::TarGz | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd => {
            test_tar_archive(path, Some(&app))
                .or_else(|e| {
                    if should_abort_test_fallback(&e) {
                        return Err(e);
                    }
                    test_with_system_tar(path, Some(&app))
                })
        }
        ArchiveFormat::Rar | ArchiveFormat::Iso | ArchiveFormat::Cab => {
            test_with_system_tar(path, Some(&app))
        }
        ArchiveFormat::Unknown => {
            test_zip_archive(path, pwd_opt, Some(&app))
                .or_else(|e| {
                    if should_abort_test_fallback(&e) {
                        return Err(e);
                    }
                    test_7z_archive(path, pwd_opt, Some(&app))
                })
                .or_else(|e| {
                    if should_abort_test_fallback(&e) {
                        return Err(e);
                    }
                    test_bay_archive(path, pwd_opt, Some(&app))
                })
                .or_else(|e| {
                    if should_abort_test_fallback(&e) {
                        return Err(e);
                    }
                    test_with_system_tar(path, Some(&app))
                })
                .or_else(|e| {
                    if should_abort_test_fallback(&e) {
                        return Err(e);
                    }
                    test_tar_archive(path, Some(&app))
                })
        }
    };

    let elapsed_ms = start_time.elapsed().as_millis() as u64;

    match test_result {
        Ok((tested_files, total_bytes, errors)) => {
            let is_ok = errors.is_empty();
            let _ = app.emit("test_progress", TestArchiveProgress {
                percent: 100.0,
                current_file: if is_ok {
                    "Verification complete: 100% OK".to_string()
                } else {
                    format!("Verification finished with {} error(s)", errors.len())
                },
                bytes_tested: total_bytes,
                total_bytes,
            });

            Ok(TestArchiveResponse {
                success: true,
                is_ok,
                total_files: tested_files,
                tested_files,
                total_bytes,
                errors,
                elapsed_ms,
                error: None,
            })
        }
        Err(e) => {
            Ok(TestArchiveResponse {
                success: false,
                is_ok: false,
                total_files: 0,
                tested_files: 0,
                total_bytes: 0,
                errors: vec![TestArchiveErrorItem {
                    file_name: archive_path,
                    error: e.clone(),
                }],
                elapsed_ms,
                error: Some(e),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tester_task_guard_isolation() {
        {
            let guard1 = TestTaskGuard::new();
            assert!(!is_test_cancelled());
            cancel_test_flag();
            assert!(is_test_cancelled());
            assert!(guard1.token().is_cancelled());
        }
        {
            let guard2 = TestTaskGuard::new();
            assert!(!is_test_cancelled());
            assert!(!guard2.token().is_cancelled());
        }
    }

    #[test]
    fn test_should_abort_test_fallback_blocks_cancel_and_password() {
        assert!(should_abort_test_fallback("Testing cancelled by user"));
        assert!(should_abort_test_fallback("Operation Canceled"));
        assert!(should_abort_test_fallback("Incorrect password: Could not decrypt 7z archive."));
        assert!(should_abort_test_fallback("Failed to decrypt stream"));
        assert!(!should_abort_test_fallback("Unknown archive magic bytes"));
    }

    #[test]
    fn test_test_bay_stream_detects_missing_eof_marker() {
        let truncated_stream: Vec<u8> = Vec::new();
        // Empty after 1-byte header read vs truncated right after entry:
        let mut partial_entry = Vec::new();
        partial_entry.push(1u8); // file entry
        let name = b"file.txt";
        partial_entry.extend_from_slice(&(name.len() as u32).to_le_bytes());
        partial_entry.extend_from_slice(name);
        partial_entry.extend_from_slice(&3u64.to_le_bytes());
        partial_entry.extend_from_slice(b"abc");
        // Missing 0u8 EOF marker!

        let (count, bytes, errors) = test_bay_stream(std::io::Cursor::new(partial_entry), 3, None).unwrap();
        assert_eq!(count, 1);
        assert_eq!(bytes, 3);
        assert!(!errors.is_empty());
        assert!(errors[0].error.contains("missing EOF marker"));
        drop(truncated_stream);
    }

    #[test]
    fn test_test_tar_archive_delegates_xz_and_bz2() {
        let xz_err = test_tar_archive(Path::new("dummy_archive.tar.xz"), None).unwrap_err();
        assert!(xz_err.contains("Delegate compressed container"));
        let bz2_err = test_tar_archive(Path::new("dummy_archive.tar.bz2"), None).unwrap_err();
        assert!(bz2_err.contains("Delegate compressed container"));
    }

    #[test]
    fn test_concurrent_test_task_guards_do_not_clobber_tokens() {
        let g1 = TestTaskGuard::new();
        let t1 = g1.token();
        let g2 = TestTaskGuard::new();
        let t2 = g2.token();

        assert!(!t1.is_cancelled());
        assert!(!t2.is_cancelled());

        // Dropping g2 must not unregister g1's active token
        drop(g2);
        cancel_test_flag();
        assert!(t1.is_cancelled());
        assert!(is_test_cancelled());
        drop(g1);
    }
}


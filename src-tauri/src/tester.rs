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
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use crate::types::{
    ArchiveFormat, TestArchiveProgress, TestArchiveResponse, TestArchiveErrorItem, TaskCancelToken,
};
use crate::priority::ScopedPriorityGuard;
use crate::crypto::ChunkedAesReader;
use crate::inspector::detect_archive_type;

thread_local! {
    static CURRENT_TEST_TOKEN: RefCell<Option<TaskCancelToken>> = const { RefCell::new(None) };
}

static ACTIVE_TEST_TOKEN: Mutex<Option<TaskCancelToken>> = Mutex::new(None);

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
            *lock = Some(token.clone());
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
            if let Some(ref current) = *lock {
                if Arc::ptr_eq(&current.0, &self.token.0) {
                    *lock = None;
                }
            }
        }
    }
}

pub fn cancel_test_flag() {
    if let Ok(lock) = ACTIVE_TEST_TOKEN.lock() {
        if let Some(ref token) = *lock {
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
        if let Some(ref token) = *lock {
            return token.is_cancelled();
        }
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

    let mut first_byte = [0u8; 1];
    let mut has_first = false;

    match decoder.read_exact(&mut first_byte) {
        Ok(_) => {
            if first_byte[0] == b'B' {
                let mut rest_sig = [0u8; 6];
                let _ = decoder.read_exact(&mut rest_sig);
            } else {
                has_first = true;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok((0, 0, errors)),
        Err(e) => return Err(format!("Error reading archive header: {}", e)),
    }

    let mut io_buf = vec![0u8; 256 * 1024];

    loop {
        if is_test_cancelled() {
            return Err("Testing cancelled by user".to_string());
        }

        let entry_type = if has_first {
            has_first = false;
            first_byte[0]
        } else {
            let mut b = [0u8; 1];
            match decoder.read_exact(&mut b) {
                Ok(_) => b[0],
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => {
                    errors.push(TestArchiveErrorItem {
                        file_name: "Archive Stream".to_string(),
                        error: format!("Stream header read error: {}", e),
                    });
                    break;
                }
            }
        };

        if entry_type == 0 {
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

    // Encrypted .bay v3 (Streaming chunked AEAD)
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
            return test_bay_stream(decoder, total_expected_bytes, app);
        } else if inner_sig.starts_with(b"BAYLZ4\x02") {
            let decoder = lz4_flex::frame::FrameDecoder::new(enc_reader);
            return test_bay_stream(decoder, total_expected_bytes, app);
        } else if inner_sig.starts_with(b"BAYZIP\x02") {
            let decoder = zstd::stream::Decoder::new(enc_reader)
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return test_bay_stream(decoder, total_expected_bytes, app);
        } else {
            let combined = std::io::Cursor::new(inner_sig).chain(enc_reader);
            let decoder = zstd::stream::Decoder::new(combined)
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return test_bay_stream(decoder, total_expected_bytes, app);
        }
    }

    if n >= 7 && (magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
        return Err("Deprecated archive: Legacy v1/v2 encryption is no longer supported.".to_string());
    }

    // Unencrypted .bay
    file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut magic_header = [0u8; 8];
    let n = file.read(&mut magic_header).unwrap_or(0);

    if n >= 7 && magic_header.starts_with(b"BAYBRO\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = brotli::Decompressor::new(file, 64 * 1024);
        test_bay_stream(decoder, total_expected_bytes, app)
    } else if n >= 7 && magic_header.starts_with(b"BAYLZ4\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        test_bay_stream(decoder, total_expected_bytes, app)
    } else if n >= 7 && magic_header.starts_with(b"BAYZIP\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = zstd::stream::Decoder::new(file)
            .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;
        test_bay_stream(decoder, total_expected_bytes, app)
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        test_bay_stream(decoder, total_expected_bytes, app)
    } else {
        file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let decoder = zstd::stream::Decoder::new(file)
            .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;
        test_bay_stream(decoder, total_expected_bytes, app)
    }
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
    let pwd = pwd_opt.map(|s| Password::from(s)).unwrap_or_else(Password::empty);

    let mut sz_reader = match sevenz_rust2::ArchiveReader::new(reader, pwd) {
        Ok(r) => r,
        Err(sevenz_rust2::Error::PasswordRequired) => {
            return Err("Password required: This 7z archive is encrypted with AES-256.".into());
        }
        Err(e) => return Err(format!("7z open error: {}", e)),
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
    let raw_file = File::open(archive_path)
        .map_err(|e| format!("Failed to open archive: {}", e))?;
    let reader = BufReader::with_capacity(1024 * 1024, raw_file);
    let path_str = archive_path.to_string_lossy().to_lowercase();

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

    if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
        let gz = flate2::read::GzDecoder::new(reader);
        let archive = tar::Archive::new(Box::new(gz) as Box<dyn Read>);
        process_tar(archive)?;
    } else if path_str.ends_with(".tar.zst") {
        let zstd_dec = zstd::stream::Decoder::new(reader).map_err(|e| e.to_string())?;
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
    cmd.arg("-tf").arg(archive_path);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn tar: {}", e))?;
    let mut tested_count = 0usize;
    let mut errors = Vec::new();
    let mut last_emit = Instant::now();

    if let Some(stdout) = child.stdout.take() {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            if is_test_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Testing cancelled by user".to_string());
            }
            if let Ok(l) = line {
                tested_count += 1;
                emit_progress(app, &mut last_emit, &l, tested_count as u64 * 1024, 0);
            }
        }
    }

    let status = child.wait().map_err(|e| format!("Tar process error: {}", e))?;
    if !status.success() {
        let mut err_msg = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut err_msg);
        }
        errors.push(TestArchiveErrorItem {
            file_name: archive_path.to_string_lossy().to_string(),
            error: if err_msg.trim().is_empty() {
                "Archive corrupted or invalid format".to_string()
            } else {
                err_msg
            },
        });
    }

    Ok((tested_count, 0, errors))
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
                if e.contains("Password") || e.contains("password") {
                    return Err(e);
                }
                test_bay_archive(path, pwd_opt, Some(&app))
                    .or_else(|_| test_with_system_tar(path, Some(&app)))
            }),
        ArchiveFormat::SevenZip => test_7z_archive(path, pwd_opt, Some(&app))
            .or_else(|e| {
                if e.contains("Password") || e.contains("password") {
                    return Err(e);
                }
                test_with_system_tar(path, Some(&app))
            }),
        ArchiveFormat::Tar | ArchiveFormat::TarGz | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd => {
            test_tar_archive(path, Some(&app))
                .or_else(|_| test_with_system_tar(path, Some(&app)))
        }
        ArchiveFormat::Rar | ArchiveFormat::Iso | ArchiveFormat::Cab => {
            test_with_system_tar(path, Some(&app))
        }
        ArchiveFormat::Unknown => {
            test_zip_archive(path, pwd_opt, Some(&app))
                .or_else(|_| test_7z_archive(path, pwd_opt, Some(&app)))
                .or_else(|_| test_bay_archive(path, pwd_opt, Some(&app)))
                .or_else(|_| test_with_system_tar(path, Some(&app)))
                .or_else(|_| test_tar_archive(path, Some(&app)))
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
}


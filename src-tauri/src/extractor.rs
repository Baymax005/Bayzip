use std::fs::File;
use std::io::{Read, Write, Seek, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce, Key
};
use sevenz_rust2::Password;

#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use crate::types::{ArchiveFormat, ExtractResponse, ExtractProgress};
use crate::priority::ScopedPriorityGuard;
use crate::inspector::{
    sanitize_archive_path,
    detect_archive_type,
    archive_stem_name,
    inspect_archive,
    has_single_root_dir,
    derive_bay_key,
};

pub fn extract_bay_stream<R: Read>(
    mut decoder: R,
    dest_dir: &Path,
    app: Option<&AppHandle>,
) -> Result<Vec<String>, String> {
    let mut extracted_files = Vec::new();
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

    loop {
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
        let mut name_buf = vec![0u8; name_len];
        decoder.read_exact(&mut name_buf)
            .map_err(|e| format!("Failed to read entry name: {}", e))?;
        let rel_path_str = String::from_utf8_lossy(&name_buf).to_string();

        let mut size_bytes = [0u8; 8];
        decoder.read_exact(&mut size_bytes)
            .map_err(|e| format!("Failed to read entry size: {}", e))?;
        let file_size = u64::from_le_bytes(size_bytes);

        let safe_rel_path = match sanitize_archive_path(&rel_path_str) {
            Some(p) => p,
            None => {
                let mut remaining = file_size;
                let mut buf = vec![0u8; 512 * 1024];
                while remaining > 0 {
                    let to_read = (remaining as usize).min(buf.len());
                    match decoder.read(&mut buf[..to_read]) {
                        Ok(0) => break,
                        Ok(n) => remaining -= n as u64,
                        Err(_) => break,
                    }
                }
                continue;
            }
        };
        let out_path = dest_dir.join(&safe_rel_path);

        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let out_file = File::create(&out_path)
            .map_err(|e| format!("Failed to create output file {:?}: {}", out_path, e))?;
        let mut writer = BufWriter::with_capacity(512 * 1024, out_file);

        let mut remaining = file_size;
        let mut buf = vec![0u8; 512 * 1024];
        let mut write_err: Option<String> = None;

        while remaining > 0 {
            let to_read = (remaining as usize).min(buf.len());
            match decoder.read(&mut buf[..to_read]) {
                Ok(0) => {
                    write_err = Some("Premature EOF inside archive entry".into());
                    break;
                }
                Ok(n) => {
                    if let Err(e) = writer.write_all(&buf[..n]) {
                        write_err = Some(format!("Error writing extracted file: {}", e));
                        break;
                    }
                    remaining -= n as u64;
                    total_extracted += n as u64;

                    if let Some(app_handle) = app {
                        if last_emit.elapsed().as_millis() >= 50 {
                            last_emit = Instant::now();
                            let _ = app_handle.emit("extract_progress", ExtractProgress {
                                percent: 50.0,
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
            let _ = std::fs::remove_file(&out_path);
            return Err(err);
        }

        writer.flush().map_err(|e| format!("Flush error: {}", e))?;
        extracted_files.push(out_path.to_string_lossy().to_string());
    }

    Ok(extracted_files)
}

pub fn extract_bay_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    app: Option<&AppHandle>,
) -> Result<Vec<String>, String> {
    let mut file = File::open(archive_path)
        .map_err(|e| format!("Failed to open .bay archive: {}", e))?;

    let mut magic = [0u8; 8];
    let n = file.read(&mut magic)
        .map_err(|e| format!("Invalid or corrupted .bay header: {}", e))?;

    if n >= 7 && (magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
        let pwd = match password {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => return Err("Password required: This .bay archive is encrypted with AES-256-GCM.".into()),
        };

        // Determine offset after BAYENC signature
        let header_len = if magic.starts_with(b"BAYENC\x02") { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(header_len)).map_err(|e| e.to_string())?;

        let mut salt = [0u8; 16];
        file.read_exact(&mut salt)
            .map_err(|e| format!("Failed to read encryption salt: {}", e))?;
        let mut nonce_bytes = [0u8; 12];
        file.read_exact(&mut nonce_bytes)
            .map_err(|e| format!("Failed to read encryption nonce: {}", e))?;

        let mut ciphertext = Vec::new();
        file.read_to_end(&mut ciphertext)
            .map_err(|e| format!("Failed to read ciphertext: {}", e))?;

        let key_bytes = derive_bay_key(pwd, &salt);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = cipher.decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| "Incorrect password: Failed to decrypt AES-256-GCM archive.".to_string())?;

        // Detect container compression inside decrypted plaintext
        if plaintext.starts_with(b"BAYBRO\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            let decoder = brotli::Decompressor::new(&plaintext[offset..], 64 * 1024);
            return extract_bay_stream(decoder, dest_dir, app);
        } else if plaintext.starts_with(b"BAYLZ4\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            let decoder = lz4_flex::frame::FrameDecoder::new(&plaintext[offset..]);
            return extract_bay_stream(decoder, dest_dir, app);
        } else if plaintext.starts_with(b"BAYZIP\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            let decoder = zstd::stream::Decoder::new(&plaintext[offset..])
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return extract_bay_stream(decoder, dest_dir, app);
        } else if plaintext.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
            // Raw LZ4 frame
            let decoder = lz4_flex::frame::FrameDecoder::new(&plaintext[..]);
            return extract_bay_stream(decoder, dest_dir, app);
        } else {
            // Raw Zstandard fallback
            let decoder = zstd::stream::Decoder::new(&plaintext[..])
                .map_err(|e| format!("Zstd decoder error: {}", e))?;
            return extract_bay_stream(decoder, dest_dir, app);
        }
    }

    // Unencrypted .bay
    file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut magic_header = [0u8; 8];
    let n = file.read(&mut magic_header).unwrap_or(0);

    if n >= 7 && magic_header.starts_with(b"BAYBRO\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = brotli::Decompressor::new(file, 64 * 1024);
        extract_bay_stream(decoder, dest_dir, app)
    } else if n >= 7 && magic_header.starts_with(b"BAYLZ4\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        extract_bay_stream(decoder, dest_dir, app)
    } else if n >= 7 && magic_header.starts_with(b"BAYZIP\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        file.seek(std::io::SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let decoder = zstd::stream::Decoder::new(file)
            .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;
        extract_bay_stream(decoder, dest_dir, app)
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        // Raw LZ4 frame
        file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        extract_bay_stream(decoder, dest_dir, app)
    } else {
        // Standard Zstd
        file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let decoder = zstd::stream::Decoder::new(file)
            .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;
        extract_bay_stream(decoder, dest_dir, app)
    }
}

pub fn extract_zip_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    app: Option<&AppHandle>,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    let file = File::open(archive_path).map_err(|e| format!("Failed to open zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip archive: {}", e))?;
    let mut extracted = Vec::new();
    let total_entries = archive.len();

    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());

    for i in 0..total_entries {
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
        let safe_path = match sanitize_archive_path(raw_name) {
            Some(p) => p,
            None => continue,
        };

        let outpath = dest_dir.join(&safe_path);
        let clean = raw_name.replace('\\', "/");
        if file.is_dir() || clean.ends_with('/') {
            let _ = std::fs::create_dir_all(&outpath);
        } else {
            if let Some(p) = outpath.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let outfile = File::create(&outpath).map_err(|e| format!("Create file error: {}", e))?;
            let mut writer = BufWriter::with_capacity(256 * 1024, outfile);
            std::io::copy(&mut file, &mut writer).map_err(|e| format!("Extract write error: {}", e))?;
            extracted.push(outpath.to_string_lossy().to_string());
        }

        if let Some(app_handle) = app {
            let pct = ((i + 1) as f64 / total_entries.max(1) as f64) * 100.0;
            let _ = app_handle.emit("extract_progress", ExtractProgress {
                percent: pct,
                current_file: safe_path.to_string_lossy().to_string(),
                bytes_extracted: 0,
            });
        }
    }
    Ok(extracted)
}

pub fn extract_tar_archive(
    archive_path: &Path,
    dest_dir: &Path,
) -> Result<Vec<String>, String> {
    let file = File::open(archive_path).map_err(|e| format!("Failed to open tar: {}", e))?;
    let is_gz = archive_path.to_string_lossy().to_lowercase().ends_with(".gz") 
             || archive_path.to_string_lossy().to_lowercase().ends_with(".tgz");
    
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create dest: {}", e))?;
    
    if is_gz {
        let tar = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(dest_dir).map_err(|e| format!("Tar unpack error: {}", e))?;
    } else {
        let mut archive = tar::Archive::new(file);
        archive.unpack(dest_dir).map_err(|e| format!("Tar unpack error: {}", e))?;
    }
    Ok(vec![dest_dir.to_string_lossy().to_string()])
}

pub fn extract_7z_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    app: Option<&AppHandle>,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create dest: {}", e))?;

    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let file = File::open(archive_path).map_err(|e| format!("Failed to open 7z: {}", e))?;

    let pwd = pwd_opt.map(|s| Password::from(s)).unwrap_or_else(Password::empty);

    let mut sz_reader = match sevenz_rust2::ArchiveReader::new(file, pwd) {
        Ok(r) => r,
        Err(sevenz_rust2::Error::PasswordRequired) => {
            return Err("Password required: This 7z archive is encrypted with AES-256.".into());
        }
        Err(e) => {
            return extract_with_system_tar(archive_path, dest_dir)
                .map_err(|_| format!("7z open error: {}", e));
        }
    };

    let total_files = sz_reader.archive().files.len();
    let mut extracted = Vec::new();
    let mut processed_count = 0;
    let mut last_emit = Instant::now();

    let res = sz_reader.for_each_entries(|entry, reader| {
        let raw_name = entry.name();
        let safe_path = match sanitize_archive_path(raw_name) {
            Some(p) => p,
            None => return Ok(true),
        };

        let out_path = dest_dir.join(&safe_path);
        let clean = raw_name.replace('\\', "/");
        if entry.is_directory() || clean.ends_with('/') {
            let _ = std::fs::create_dir_all(&out_path);
        } else {
            if let Some(p) = out_path.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let out_file = File::create(&out_path)
                .map_err(sevenz_rust2::Error::from)?;
            let mut writer = BufWriter::with_capacity(256 * 1024, out_file);
            std::io::copy(reader, &mut writer).map_err(sevenz_rust2::Error::from)?;
            extracted.push(out_path.to_string_lossy().to_string());
        }

        processed_count += 1;
        if let Some(app_handle) = app {
            if last_emit.elapsed().as_millis() >= 50 || processed_count == total_files {
                last_emit = Instant::now();
                let pct = if total_files > 0 {
                    ((processed_count as f64 / total_files as f64) * 100.0).min(99.0)
                } else {
                    50.0
                };
                let _ = app_handle.emit("extract_progress", ExtractProgress {
                    percent: pct,
                    current_file: safe_path.to_string_lossy().to_string(),
                    bytes_extracted: 0,
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
                extract_with_system_tar(archive_path, dest_dir)
                    .map_err(|_| format!("7z extraction error: {}", e))
            }
        }
    }
}

pub fn extract_with_system_tar(
    archive_path: &Path,
    dest_dir: &Path,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    let mut cmd = Command::new("tar.exe");
    cmd.args(["-xf", &archive_path.to_string_lossy(), "-C", &dest_dir.to_string_lossy()]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().map_err(|e| format!("System extraction error: {}", e))?;
    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Extraction error: {}", err_msg.trim()));
    }

    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dest_dir) {
        for entry in entries.flatten() {
            files.push(entry.path().to_string_lossy().to_string());
        }
    }
    if files.is_empty() {
        files.push(dest_dir.to_string_lossy().to_string());
    }
    Ok(files)
}

pub fn universal_extract(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    app: Option<&AppHandle>,
) -> Result<Vec<String>, String> {
    let format = detect_archive_type(archive_path);
    match format {
        ArchiveFormat::Bay => {
            extract_bay_archive(archive_path, dest_dir, password, app)
        },
        ArchiveFormat::Zip => {
            extract_zip_archive(archive_path, dest_dir, password, app)
                .or_else(|e| {
                    if e.contains("Password") || e.contains("password") {
                        return Err(e);
                    }
                    extract_bay_archive(archive_path, dest_dir, password, app)
                        .or_else(|_| extract_with_system_tar(archive_path, dest_dir))
                })
        },
        ArchiveFormat::SevenZip => {
            extract_7z_archive(archive_path, dest_dir, password, app)
                .or_else(|e| {
                    if e.contains("Password") || e.contains("password") {
                        return Err(e);
                    }
                    extract_with_system_tar(archive_path, dest_dir)
                })
        },
        ArchiveFormat::Tar | ArchiveFormat::TarGz => {
            extract_tar_archive(archive_path, dest_dir)
                .or_else(|_| extract_with_system_tar(archive_path, dest_dir))
        },
        ArchiveFormat::Rar => {
            extract_with_system_tar(archive_path, dest_dir)
        },
        ArchiveFormat::Unknown => {
            extract_zip_archive(archive_path, dest_dir, password, app)
                .or_else(|_| extract_7z_archive(archive_path, dest_dir, password, app))
                .or_else(|_| extract_bay_archive(archive_path, dest_dir, password, app))
                .or_else(|_| extract_tar_archive(archive_path, dest_dir))
                .or_else(|_| extract_with_system_tar(archive_path, dest_dir))
        }
    }
}

pub fn extract_archive_sync(
    app: AppHandle,
    archive_path: String,
    destination: Option<String>,
    password: Option<String>,
) -> Result<ExtractResponse, String> {
    let _priority_guard = ScopedPriorityGuard::new();

    let path = Path::new(&archive_path);
    if !path.exists() {
        return Err("Archive file does not exist".into());
    }

    let start_time = Instant::now();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = archive_stem_name(path);

    let dest_dir = if let Some(custom_dest) = destination {
        if custom_dest.trim().is_empty() {
            parent.join(&stem)
        } else {
            PathBuf::from(custom_dest)
        }
    } else {
        parent.join(&stem)
    };

    let pwd_ref = password.as_deref();
    let extracted_files = universal_extract(path, &dest_dir, pwd_ref, Some(&app))?;

    let _ = app.emit("extract_progress", ExtractProgress {
        percent: 100.0,
        current_file: "Completed".into(),
        bytes_extracted: 0,
    });

    Ok(ExtractResponse {
        success: true,
        output_directory: dest_dir.to_string_lossy().to_string(),
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
    universal_extract(p, parent, None, None)?;

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
        let d = parent.join(&stem);
        let _ = std::fs::create_dir_all(&d);
        d
    };

    if let Err(e) = universal_extract(p, &dest, None, None) {
        // If extraction failed and we created a subfolder, clean it up if empty
        if !has_single_root {
            let _ = std::fs::remove_dir(&dest);
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

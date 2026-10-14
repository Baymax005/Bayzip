use std::fs::File;
use std::io::{Read, Write, Seek, BufWriter};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use zip::write::SimpleFileOptions;
use zip::AesMode;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce, Key
};
use rand::RngCore;
use sevenz_rust2::{ArchiveWriter, ArchiveEntry, EncoderConfiguration};

#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use std::sync::atomic::{AtomicBool, Ordering};

pub static CANCEL_COMPRESSION: AtomicBool = AtomicBool::new(false);

pub fn cancel_compression_flag() {
    CANCEL_COMPRESSION.store(true, Ordering::SeqCst);
}

pub fn is_compression_cancelled() -> bool {
    CANCEL_COMPRESSION.load(Ordering::Relaxed)
}

pub fn reset_compression_cancel_flag() {
    CANCEL_COMPRESSION.store(false, Ordering::SeqCst);
}

use crate::types::{CompressPayload, CompressResponse, CompressProgress};
use crate::priority::ScopedPriorityGuard;
use crate::inspector::{archive_stem_name, derive_bay_key};

/// Exact CPU Cores scaling matrix for both .bay and .bay_gamer:
/// 0   -> 1 core
/// 25  -> max(1, total_cores / 4)
/// 50  -> max(1, total_cores / 2)
/// 75  -> max(1, (total_cores * 3) / 4)
/// 100 -> total_cores
pub fn calculate_target_threads(slider_value: i32, total_cores: usize) -> usize {
    match slider_value {
        s if s <= 0 => 1,
        25 => (total_cores / 4).max(1),
        50 => (total_cores / 2).max(1),
        75 => ((total_cores * 3) / 4).max(1),
        100 => total_cores.max(1),
        s if s <= 25 => (total_cores / 4).max(1),
        s if s <= 50 => (total_cores / 2).max(1),
        s if s <= 75 => ((total_cores * 3) / 4).max(1),
        _ => total_cores.max(1),
    }
}

/// Exact Zstd Level matrix:
/// For .bay_gamer: 0 -> Level 1, 25 -> Level 1, 50 -> Level 2, 75 & 100 -> Level 3 (locked max at Level 3).
/// For standard .bay: 0 -> Level 1, 25 -> Level 3, 50 -> Level 7, 75 -> Level 14, 100 -> Level 19.
pub fn calculate_zstd_level(slider_value: i32, is_gamer: bool) -> i32 {
    if is_gamer {
        match slider_value {
            s if s <= 25 => 1,
            s if s <= 50 => 2,
            _ => 3,
        }
    } else {
        match slider_value {
            s if s <= 0 => 1,
            s if s <= 25 => 3,
            s if s <= 50 => 7,
            s if s <= 75 => 14,
            _ => 19,
        }
    }
}

pub fn compute_path_size(p: &Path) -> (u64, bool) {
    if let Ok(meta) = std::fs::symlink_metadata(p) {
        if meta.is_file() {
            (meta.len(), false)
        } else if meta.is_dir() {
            let mut total: u64 = 0;
            fn walk_dir(dir: &Path, total: &mut u64, depth: usize) {
                if depth > 32 {
                    return;
                }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let ep = entry.path();
                        if let Ok(m) = std::fs::symlink_metadata(&ep) {
                            if m.is_file() {
                                *total += m.len();
                            } else if m.is_dir() && !m.file_type().is_symlink() {
                                walk_dir(&ep, total, depth + 1);
                            }
                        }
                    }
                }
            }
            walk_dir(p, &mut total, 0);
            (total, true)
        } else {
            (0, false)
        }
    } else {
        (0, false)
    }
}

pub fn write_file_to_bay_archive<W: Write>(
    filepath: &Path,
    rel_name: &str,
    encoder: &mut W,
    bytes_processed: &mut u64,
    total_original: u64,
    app: &AppHandle,
    last_emit: &mut Instant,
) -> Result<(), String> {
    let file_meta = std::fs::metadata(filepath).ok();
    let file_size = file_meta.map(|m| m.len()).unwrap_or(0);

    encoder.write_all(&[1u8]).map_err(|e| e.to_string())?; // 1 = File entry
    let filename_bytes = rel_name.as_bytes();
    encoder.write_all(&(filename_bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    encoder.write_all(filename_bytes).map_err(|e| e.to_string())?;
    encoder.write_all(&file_size.to_le_bytes()).map_err(|e| e.to_string())?;

    if let Ok(mut f) = File::open(filepath) {
        let mut buffer = [0u8; 256 * 1024];
        loop {
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            match f.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if is_compression_cancelled() {
                        return Err("Compression cancelled by user".to_string());
                    }
                    encoder.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                    *bytes_processed += n as u64;
                    if last_emit.elapsed().as_millis() >= 50 {
                        *last_emit = Instant::now();
                        let pct = if total_original > 0 {
                            ((*bytes_processed as f64 / total_original as f64) * 100.0).min(99.0)
                        } else {
                            50.0
                        };
                        let _ = app.emit("compress_progress", CompressProgress {
                            percent: pct,
                            current_file: rel_name.to_string(),
                            bytes_processed: *bytes_processed,
                            total_bytes: total_original,
                        });
                    }
                }
                Err(_) => break,
            }
        }
    }
    Ok(())
}

pub fn write_dir_to_bay_archive<W: Write>(
    dir: &Path,
    base_parent: &Path,
    encoder: &mut W,
    bytes_processed: &mut u64,
    total_original: u64,
    app: &AppHandle,
    depth: usize,
    last_emit: &mut Instant,
) -> Result<(), String> {
    if depth > 32 {
        return Ok(());
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let ep = entry.path();
            if let Ok(m) = std::fs::symlink_metadata(&ep) {
                if m.is_file() {
                    let rel = ep.strip_prefix(base_parent).unwrap_or(&ep);
                    write_file_to_bay_archive(&ep, &rel.to_string_lossy(), encoder, bytes_processed, total_original, app, last_emit)?;
                } else if m.is_dir() && !m.file_type().is_symlink() {
                    write_dir_to_bay_archive(&ep, base_parent, encoder, bytes_processed, total_original, app, depth + 1, last_emit)?;
                }
            }
        }
    }
    Ok(())
}

pub fn compress_bay(
    files: &[String],
    output_path: &Path,
    algorithm: &str,
    slider_value: i32,
    password: Option<&str>,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    let pwd_trimmed = password.map(|s| s.trim()).filter(|s| !s.is_empty());
    let is_encrypted = pwd_trimmed.is_some();

    let target_dest = if is_encrypted {
        std::env::temp_dir().join(format!(
            "bayzip_raw_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ))
    } else {
        output_path.to_path_buf()
    };

    let res = (|| -> Result<(), String> {
        let first_path = Path::new(&files[0]);
        let parent_dir = first_path.parent().unwrap_or_else(|| Path::new("."));

        let algo_lower = algorithm.to_lowercase();
        let mut bytes_processed: u64 = 0;
        let mut last_emit = Instant::now();

        let mut out_file = File::create(&target_dest)
            .map_err(|e| format!("Failed to create intermediate file: {}", e))?;

        if algo_lower.contains("brotli") {
            out_file.write_all(b"BAYBRO\x02\x00")
                .map_err(|e| format!("Header write error: {}", e))?;
            let quality = match slider_value {
                0 => 1,
                25 => 4,
                50 => 7,
                75 => 9,
                _ => 11,
            };
            let mut encoder = brotli::CompressorWriter::new(BufWriter::new(out_file), 64 * 1024, quality, 22);
            for file_str in files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut bytes_processed, total_original, app, &mut last_emit)?;
                } else if p.is_dir() {
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
            encoder.flush().map_err(|e| format!("Brotli finalize error: {}", e))?;
        } else if algo_lower.contains("lz4") {
            out_file.write_all(b"BAYLZ4\x02\x00")
                .map_err(|e| format!("Header write error: {}", e))?;
            let mut encoder = lz4_flex::frame::FrameEncoder::new(BufWriter::new(out_file));
            for file_str in files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut bytes_processed, total_original, app, &mut last_emit)?;
                } else if p.is_dir() {
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
            let _ = encoder.finish().map_err(|e| format!("LZ4 finalize error: {}", e))?;
        } else {
            // Default to Zstandard or .bay_gamer preset
            out_file.write_all(b"BAYZIP\x02\x00")
                .map_err(|e| format!("Header write error: {}", e))?;
            let is_gamer = algo_lower.contains("gamer");
            let zstd_level = calculate_zstd_level(slider_value, is_gamer);
            let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let threads = calculate_target_threads(slider_value, total_cores);

            let mut encoder = zstd::stream::Encoder::new(BufWriter::new(out_file), zstd_level)
                .map_err(|e| format!("Failed to initialize compression encoder: {}", e))?;
            let _ = encoder.multithread(threads as u32);
            for file_str in files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut bytes_processed, total_original, app, &mut last_emit)?;
                } else if p.is_dir() {
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
            encoder.finish().map_err(|e| format!("Failed to finalize compression: {}", e))?;
        }

        if let Some(pwd) = pwd_trimmed {
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let mut raw_data = Vec::new();
            File::open(&target_dest)
                .and_then(|mut f| f.read_to_end(&mut raw_data))
                .map_err(|e| format!("Failed to read unencrypted buffer: {}", e))?;
            let _ = std::fs::remove_file(&target_dest);

            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }

            let mut salt = [0u8; 16];
            let mut nonce_bytes = [0u8; 12];
            rand::thread_rng().fill_bytes(&mut salt);
            rand::thread_rng().fill_bytes(&mut nonce_bytes);

            let key_bytes = derive_bay_key(pwd, &salt);
            let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
            let cipher = Aes256Gcm::new(key);
            let nonce = Nonce::from_slice(&nonce_bytes);

            let ciphertext = cipher.encrypt(nonce, raw_data.as_ref())
                .map_err(|e| format!("AES-256 encryption error: {}", e))?;

            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }

            let mut final_file = File::create(output_path)
                .map_err(|e| format!("Failed to create encrypted output file: {}", e))?;
            final_file.write_all(b"BAYENC\x02\x00").map_err(|e| e.to_string())?;
            final_file.write_all(&salt).map_err(|e| e.to_string())?;
            final_file.write_all(&nonce_bytes).map_err(|e| e.to_string())?;
            final_file.write_all(&ciphertext).map_err(|e| e.to_string())?;
        }

        Ok(())
    })();

    if res.is_err() && is_encrypted && target_dest.exists() {
        let _ = std::fs::remove_file(&target_dest);
    }

    res
}

pub fn compress_zip(
    files: &[String],
    output_path: &Path,
    algorithm: &str,
    slider_value: i32,
    password: Option<&str>,
    encryption: Option<&str>,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create zip file: {}", e))?;
    let mut writer = zip::ZipWriter::new(out_file);
    
    let algo_lower = algorithm.to_lowercase();
    let method = if algo_lower.contains("bzip2") {
        zip::CompressionMethod::Bzip2
    } else if slider_value == 0 || algo_lower.contains("store") {
        zip::CompressionMethod::Stored
    } else {
        zip::CompressionMethod::Deflated
    };

    let mut bytes_processed: u64 = 0;
    let mut last_emit = Instant::now();

    let has_pwd = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let aes_mode = match encryption {
        Some("aes-128") => AesMode::Aes128,
        _ => AesMode::Aes256,
    };

    for file_str in files {
        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }
        let p = Path::new(file_str);
        
        let file_len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .large_file(file_len >= 0xFFFF_FFFF);

        if p.is_file() {
            let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            if let Some(pwd) = has_pwd {
                writer.start_file(file_name, options.with_aes_encryption(aes_mode, pwd))
                    .map_err(|e| format!("Zip start file error: {}", e))?;
            } else {
                writer.start_file(file_name, options)
                    .map_err(|e| format!("Zip start file error: {}", e))?;
            }
            
            if let Ok(mut f) = File::open(p) {
                let mut buf = [0u8; 128 * 1024];
                loop {
                    if is_compression_cancelled() {
                        return Err("Compression cancelled by user".to_string());
                    }
                    match f.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if is_compression_cancelled() {
                                return Err("Compression cancelled by user".to_string());
                            }
                            writer.write_all(&buf[..n])
                                .map_err(|e| format!("Zip write error: {}", e))?;
                            bytes_processed += n as u64;
                            if last_emit.elapsed().as_millis() >= 50 {
                                last_emit = Instant::now();
                                let pct = if total_original > 0 {
                                    ((bytes_processed as f64 / total_original as f64) * 100.0).min(99.0)
                                } else {
                                    50.0
                                };
                                let _ = app.emit("compress_progress", CompressProgress {
                                    percent: pct,
                                    current_file: file_name.to_string(),
                                    bytes_processed,
                                    total_bytes: total_original,
                                });
                            }
                        }
                        Err(e) => return Err(format!("File read error: {}", e)),
                    }
                }
            }
        } else if p.is_dir() {
            let parent_dir = p.parent().unwrap_or(p);
            let root_rel = p.strip_prefix(parent_dir).unwrap_or(p);
            let root_str = root_rel.to_string_lossy().replace('\\', "/");
            let clean_root = root_str.trim_start_matches('/').trim_end_matches('/');
            if !clean_root.is_empty() {
                let dir_name = format!("{}/", clean_root);
                let dir_options = SimpleFileOptions::default()
                    .compression_method(method)
                    .large_file(false);
                if let Some(pwd) = has_pwd {
                    let _ = writer.add_directory(&dir_name, dir_options.with_aes_encryption(aes_mode, pwd));
                } else {
                    let _ = writer.add_directory(&dir_name, dir_options);
                }
            }

            fn add_dir_to_zip<W: Write + std::io::Seek>(
                dir: &Path,
                base: &Path,
                writer: &mut zip::ZipWriter<W>,
                method: zip::CompressionMethod,
                has_pwd: Option<&str>,
                aes_mode: AesMode,
                bytes_processed: &mut u64,
                total_original: u64,
                app: &AppHandle,
                last_emit: &mut Instant,
                depth: usize,
            ) -> Result<(), String> {
                if depth > 32 { return Ok(()); }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if is_compression_cancelled() {
                            return Err("Compression cancelled by user".to_string());
                        }
                        let path = entry.path();
                        if let Ok(m) = std::fs::symlink_metadata(&path) {
                            let rel = path.strip_prefix(base).unwrap_or(&path);
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            let clean_rel = rel_str.trim_start_matches('/').trim_end_matches('/');
                            if clean_rel.is_empty() { continue; }
                            
                            let is_large = m.len() >= 0xFFFF_FFFF;
                            let options = SimpleFileOptions::default()
                                .compression_method(method)
                                .large_file(is_large);

                            if m.is_dir() && !m.file_type().is_symlink() {
                                let dir_name = format!("{}/", clean_rel);
                                if let Some(pwd) = has_pwd {
                                    let _ = writer.add_directory(&dir_name, options.with_aes_encryption(aes_mode, pwd));
                                } else {
                                    let _ = writer.add_directory(&dir_name, options);
                                }
                                add_dir_to_zip(&path, base, writer, method, has_pwd, aes_mode, bytes_processed, total_original, app, last_emit, depth + 1)?;
                            } else if m.is_file() {
                                if let Some(pwd) = has_pwd {
                                    writer.start_file(clean_rel, options.with_aes_encryption(aes_mode, pwd))
                                        .map_err(|e| format!("Zip start file error: {}", e))?;
                                } else {
                                    writer.start_file(clean_rel, options)
                                        .map_err(|e| format!("Zip start file error: {}", e))?;
                                }
                                if let Ok(mut f) = File::open(&path) {
                                    let mut buf = [0u8; 128 * 1024];
                                    loop {
                                        if is_compression_cancelled() {
                                            return Err("Compression cancelled by user".to_string());
                                        }
                                        match f.read(&mut buf) {
                                            Ok(0) => break,
                                            Ok(n) => {
                                                if is_compression_cancelled() {
                                                    return Err("Compression cancelled by user".to_string());
                                                }
                                                writer.write_all(&buf[..n])
                                                    .map_err(|e| format!("Zip write error: {}", e))?;
                                                *bytes_processed += n as u64;
                                                if last_emit.elapsed().as_millis() >= 50 {
                                                    *last_emit = Instant::now();
                                                    let pct = if total_original > 0 {
                                                        ((*bytes_processed as f64 / total_original as f64) * 100.0).min(99.0)
                                                    } else {
                                                        50.0
                                                    };
                                                    let _ = app.emit("compress_progress", CompressProgress {
                                                        percent: pct,
                                                        current_file: rel_str.clone(),
                                                        bytes_processed: *bytes_processed,
                                                        total_bytes: total_original,
                                                    });
                                                }
                                            }
                                            Err(e) => return Err(format!("File read error: {}", e)),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            add_dir_to_zip(p, parent_dir, &mut writer, method, has_pwd, aes_mode, &mut bytes_processed, total_original, app, &mut last_emit, 0)?;
        }
    }

    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    writer.finish().map_err(|e| format!("Failed to finalize zip archive: {}", e))?;
    Ok(())
}

pub fn compress_tar(
    files: &[String],
    output_path: &Path,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create tar file: {}", e))?;
    let mut builder = tar::Builder::new(BufWriter::new(out_file));

    for (i, file_str) in files.iter().enumerate() {
        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }
        let p = Path::new(file_str);
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        if p.is_file() {
            builder.append_path_with_name(p, name)
                .map_err(|e| format!("Tar append file error: {}", e))?;
        } else if p.is_dir() {
            builder.append_dir_all(name, p)
                .map_err(|e| format!("Tar append dir error: {}", e))?;
        }
        let pct = ((i + 1) as f64 / files.len() as f64) * 100.0;
        let _ = app.emit("compress_progress", CompressProgress {
            percent: pct,
            current_file: name.to_string(),
            bytes_processed: total_original,
            total_bytes: total_original,
        });
    }

    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    builder.finish().map_err(|e| format!("Tar finalize error: {}", e))?;
    Ok(())
}

pub fn compress_7z(
    files: &[String],
    output_path: &Path,
    slider_value: i32,
    password: Option<&str>,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut sz = ArchiveWriter::create(output_path)
        .map_err(|e| format!("Failed to create 7z archive: {}", e))?;

    let pwd_opt = password.map(|p| p.trim()).filter(|p| !p.is_empty());
    let mut methods: Vec<EncoderConfiguration> = Vec::new();
    if let Some(pwd) = pwd_opt {
        methods.push(sevenz_rust2::encoder_options::AesEncoderOptions::new(pwd.into()).into());
        sz.set_encrypt_header(true);
    } else {
        sz.set_encrypt_header(false);
    }

    let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let threads = calculate_target_threads(slider_value, total_cores) as u32;
    let lzma_level = match slider_value {
        s if s <= 0 => 1,
        25 => 3,
        50 => 5,
        75 => 7,
        _ => 9,
    };

    let lzma2_opt = if threads > 1 {
        sevenz_rust2::encoder_options::Lzma2Options::from_level_mt(lzma_level, threads, 32 * 1024 * 1024)
    } else {
        sevenz_rust2::encoder_options::Lzma2Options::from_level(lzma_level)
    };
    methods.push(lzma2_opt.into());
    sz.set_content_methods(methods);

    let mut bytes_processed: u64 = 0;
    let mut last_emit = Instant::now();

    for file_str in files {
        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }
        let p = Path::new(file_str);
        if !p.exists() {
            continue;
        }
        let parent = p.parent().unwrap_or(p);

        if p.is_file() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            let mut entry = ArchiveEntry::from_path(p, name.to_string());
            entry.is_directory = false;
            entry.has_stream = true;
            let f = File::open(p).map_err(|e| format!("Failed to open file {:?}: {}", p, e))?;
            sz.push_archive_entry(entry, Some(f))
                .map_err(|e| format!("7z entry error: {}", e))?;
            bytes_processed += std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);

            let pct = if total_original > 0 {
                ((bytes_processed as f64 / total_original as f64) * 100.0).min(99.0)
            } else {
                50.0
            };
            let _ = app.emit("compress_progress", CompressProgress {
                percent: pct,
                current_file: name.to_string(),
                bytes_processed,
                total_bytes: total_original,
            });
        } else if p.is_dir() {
            let rel_root = p.strip_prefix(parent).unwrap_or(p).to_string_lossy().replace('\\', "/");
            let clean_root = rel_root.trim_start_matches('/').trim_end_matches('/');
            if !clean_root.is_empty() {
                let mut dir_entry = ArchiveEntry::from_path(p, format!("{}/", clean_root));
                dir_entry.is_directory = true;
                dir_entry.has_stream = false;
                let _ = sz.push_archive_entry::<&[u8]>(dir_entry, None);
            }

            fn add_dir_7z<W: Write + Seek>(
                dir: &Path,
                base: &Path,
                sz: &mut ArchiveWriter<W>,
                bytes_processed: &mut u64,
                total_original: u64,
                app: &AppHandle,
                last_emit: &mut Instant,
                depth: usize,
            ) -> Result<(), String> {
                if depth > 32 { return Ok(()); }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if is_compression_cancelled() {
                            return Err("Compression cancelled by user".to_string());
                        }
                        let path = entry.path();
                        if let Ok(m) = std::fs::symlink_metadata(&path) {
                            let rel = path.strip_prefix(base).unwrap_or(&path);
                            let clean = rel.to_string_lossy().replace('\\', "/");
                            let trimmed = clean.trim_start_matches('/');
                            if trimmed.is_empty() { continue; }

                            if m.is_dir() && !m.file_type().is_symlink() {
                                let mut d_entry = ArchiveEntry::from_path(&path, format!("{}/", trimmed.trim_end_matches('/')));
                                d_entry.is_directory = true;
                                d_entry.has_stream = false;
                                let _ = sz.push_archive_entry::<&[u8]>(d_entry, None);
                                add_dir_7z(&path, base, sz, bytes_processed, total_original, app, last_emit, depth + 1)?;
                            } else if m.is_file() {
                                let mut f_entry = ArchiveEntry::from_path(&path, trimmed.to_string());
                                f_entry.is_directory = false;
                                f_entry.has_stream = true;
                                let f = File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;
                                sz.push_archive_entry(f_entry, Some(f))
                                    .map_err(|e| format!("7z entry error: {}", e))?;
                                *bytes_processed += m.len();
                                if last_emit.elapsed().as_millis() >= 50 {
                                    *last_emit = Instant::now();
                                    let pct = if total_original > 0 {
                                        ((*bytes_processed as f64 / total_original as f64) * 100.0).min(99.0)
                                    } else {
                                        50.0
                                    };
                                    let _ = app.emit("compress_progress", CompressProgress {
                                        percent: pct,
                                        current_file: trimmed.to_string(),
                                        bytes_processed: *bytes_processed,
                                        total_bytes: total_original,
                                    });
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            add_dir_7z(p, parent, &mut sz, &mut bytes_processed, total_original, app, &mut last_emit, 0)?;
        }
    }

    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    sz.finish().map_err(|e| format!("7z finalize error: {}", e))?;
    Ok(())
}

pub fn headless_quick_compress_zip(p: &Path, output_path: &Path) -> Result<(), String> {
    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create zip file: {}", e))?;
    let mut writer = zip::ZipWriter::new(out_file);
    let file_len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(file_len >= 0xFFFF_FFFF);

    let parent = p.parent().unwrap_or(p);

    if p.is_file() {
        let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        writer.start_file(file_name, options)
            .map_err(|e| format!("Zip start error: {}", e))?;
        if let Ok(mut f) = File::open(p) {
            std::io::copy(&mut f, &mut writer)
                .map_err(|e| format!("Zip copy error: {}", e))?;
        }
    } else if p.is_dir() {
        fn add_dir<W: Write + std::io::Seek>(
            dir: &Path,
            base: &Path,
            writer: &mut zip::ZipWriter<W>,
            depth: usize,
        ) -> Result<(), String> {
            if depth > 32 { return Ok(()); }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Ok(m) = std::fs::symlink_metadata(&path) {
                        let rel = path.strip_prefix(base).unwrap_or(&path);
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        if m.is_dir() && !m.file_type().is_symlink() {
                            let dir_opt = SimpleFileOptions::default()
                                .compression_method(zip::CompressionMethod::Deflated)
                                .large_file(false);
                            let _ = writer.add_directory(format!("{}/", rel_str), dir_opt);
                            add_dir(&path, base, writer, depth + 1)?;
                        } else if m.is_file() {
                            let file_opt = SimpleFileOptions::default()
                                .compression_method(zip::CompressionMethod::Deflated)
                                .large_file(m.len() >= 0xFFFF_FFFF);
                            writer.start_file(&rel_str, file_opt)
                                .map_err(|e| format!("Zip start error: {}", e))?;
                            if let Ok(mut f) = File::open(&path) {
                                std::io::copy(&mut f, writer)
                                    .map_err(|e| format!("Zip write error: {}", e))?;
                            }
                        }
                    }
                }
            }
            Ok(())
        }
        add_dir(p, parent, &mut writer, 0)?;
    }

    writer.finish().map_err(|e| format!("Failed to finalize zip: {}", e))?;
    Ok(())
}

pub fn headless_quick_compress_bay(p: &Path, output_path: &Path) -> Result<(), String> {
    let mut out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    out_file.write_all(b"BAYZIP\x02\x00")
        .map_err(|e| format!("Header write error: {}", e))?;
    
    // Balanced profile level 7 (50% slider equivalent)
    let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let threads = calculate_target_threads(50, total_cores);
    let zstd_level = calculate_zstd_level(50, false);
    let mut encoder = zstd::stream::Encoder::new(out_file, zstd_level)
        .map_err(|e| format!("Encoder error: {}", e))?;
    let _ = encoder.multithread(threads as u32);

    let parent = p.parent().unwrap_or(p);

    if p.is_file() {
        let file_meta = std::fs::metadata(p).ok();
        let file_size = file_meta.map(|m| m.len()).unwrap_or(0);
        let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let filename_bytes = filename.as_bytes();
        
        let _ = encoder.write_all(&[1u8]); // 1 = file entry
        let _ = encoder.write_all(&(filename_bytes.len() as u32).to_le_bytes());
        let _ = encoder.write_all(filename_bytes);
        let _ = encoder.write_all(&file_size.to_le_bytes());

        if let Ok(mut f) = File::open(p) {
            let mut buffer = [0u8; 128 * 1024];
            loop {
                match f.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = encoder.write_all(&buffer[..n]);
                    }
                    Err(_) => break,
                }
            }
        }
    } else if p.is_dir() {
        fn add_dir(dir: &Path, base_parent: &Path, encoder: &mut zstd::stream::Encoder<File>, depth: usize) {
            if depth > 32 {
                return;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if let Ok(m) = std::fs::symlink_metadata(&entry_path) {
                        if m.is_file() {
                            let rel_name = entry_path.strip_prefix(base_parent).unwrap_or(&entry_path);
                            let rel_str = rel_name.to_string_lossy().replace('\\', "/");
                            let bytes = rel_str.as_bytes();
                            let file_size = m.len();
                            let _ = encoder.write_all(&[1u8]);
                            let _ = encoder.write_all(&(bytes.len() as u32).to_le_bytes());
                            let _ = encoder.write_all(bytes);
                            let _ = encoder.write_all(&file_size.to_le_bytes());
                            if let Ok(mut f) = File::open(&entry_path) {
                                let mut buf = [0u8; 128 * 1024];
                                loop {
                                    match f.read(&mut buf) {
                                        Ok(0) => break,
                                        Ok(n) => { let _ = encoder.write_all(&buf[..n]); },
                                        Err(_) => break,
                                    }
                                }
                            }
                        } else if m.is_dir() && !m.file_type().is_symlink() {
                            add_dir(&entry_path, base_parent, encoder, depth + 1);
                        }
                    }
                }
            }
        }
        add_dir(p, parent, &mut encoder, 0);
    }

    let _ = encoder.write_all(&[0u8]); // 0 = EOF
    encoder.finish().map_err(|e| format!("Finalize error: {}", e))?;
    Ok(())
}

pub fn headless_quick_compress_7z(p: &Path, output_path: &Path) -> Result<(), String> {
    let mut sz = ArchiveWriter::create(output_path)
        .map_err(|e| format!("Failed to create 7z: {}", e))?;
    sz.set_encrypt_header(false);

    let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let threads = calculate_target_threads(50, total_cores) as u32;
    let lzma2_opt = if threads > 1 {
        sevenz_rust2::encoder_options::Lzma2Options::from_level_mt(5, threads, 32 * 1024 * 1024)
    } else {
        sevenz_rust2::encoder_options::Lzma2Options::from_level(5)
    };
    sz.set_content_methods(vec![lzma2_opt.into()]);

    let parent = p.parent().unwrap_or(p);
    if p.is_file() {
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let mut entry = ArchiveEntry::from_path(p, name.to_string());
        entry.is_directory = false;
        entry.has_stream = true;
        let f = File::open(p).map_err(|e| format!("Failed to open file: {}", e))?;
        sz.push_archive_entry(entry, Some(f))
            .map_err(|e| format!("7z entry error: {}", e))?;
    } else if p.is_dir() {
        let rel_root = p.strip_prefix(parent).unwrap_or(p).to_string_lossy().replace('\\', "/");
        let clean_root = rel_root.trim_start_matches('/').trim_end_matches('/');
        if !clean_root.is_empty() {
            let mut dir_entry = ArchiveEntry::from_path(p, format!("{}/", clean_root));
            dir_entry.is_directory = true;
            dir_entry.has_stream = false;
            let _ = sz.push_archive_entry::<&[u8]>(dir_entry, None);
        }

        fn add_dir_7z_quick<W: Write + Seek>(dir: &Path, base: &Path, sz: &mut ArchiveWriter<W>, depth: usize) -> Result<(), String> {
            if depth > 32 { return Ok(()); }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Ok(m) = std::fs::symlink_metadata(&path) {
                        let rel = path.strip_prefix(base).unwrap_or(&path);
                        let clean = rel.to_string_lossy().replace('\\', "/");
                        let trimmed = clean.trim_start_matches('/');
                        if trimmed.is_empty() { continue; }

                        if m.is_dir() && !m.file_type().is_symlink() {
                            let mut d_entry = ArchiveEntry::from_path(&path, format!("{}/", trimmed.trim_end_matches('/')));
                            d_entry.is_directory = true;
                            d_entry.has_stream = false;
                            let _ = sz.push_archive_entry::<&[u8]>(d_entry, None);
                            add_dir_7z_quick(&path, base, sz, depth + 1)?;
                        } else if m.is_file() {
                            let mut f_entry = ArchiveEntry::from_path(&path, trimmed.to_string());
                            f_entry.is_directory = false;
                            f_entry.has_stream = true;
                            if let Ok(f) = File::open(&path) {
                                let _ = sz.push_archive_entry(f_entry, Some(f));
                            }
                        }
                    }
                }
            }
            Ok(())
        }
        add_dir_7z_quick(p, parent, &mut sz, 0)?;
    }
    sz.finish().map_err(|e| format!("7z finalize error: {}", e))?;
    Ok(())
}

pub fn headless_quick_compress(format: &str, target_path_str: &str) -> Result<(), String> {
    let _priority_guard = ScopedPriorityGuard::new();

    let p = Path::new(target_path_str);
    if !p.exists() {
        return Err("Target path does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    let stem = archive_stem_name(p);
    let ext = if format.starts_with('.') {
        format.to_string()
    } else {
        format!(".{}", format)
    };
    let output_path = parent.join(format!("{}{}", stem, ext));

    let fmt_clean = format.trim_start_matches('.').to_lowercase();
    match fmt_clean.as_str() {
        "zip" => headless_quick_compress_zip(p, &output_path)?,
        "7z" => headless_quick_compress_7z(p, &output_path)?,
        "tar" => {
            let out_file = File::create(&output_path)
                .map_err(|e| format!("Failed to create tar file: {}", e))?;
            let mut builder = tar::Builder::new(BufWriter::new(out_file));
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            if p.is_file() {
                builder.append_path_with_name(p, name)
                    .map_err(|e| format!("Tar append file error: {}", e))?;
            } else if p.is_dir() {
                builder.append_dir_all(name, p)
                    .map_err(|e| format!("Tar append dir error: {}", e))?;
            }
            builder.finish().map_err(|e| format!("Tar finalize error: {}", e))?;
        },
        _ => headless_quick_compress_bay(p, &output_path)?,
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

pub fn compress_files_sync(app: AppHandle, payload: CompressPayload) -> Result<CompressResponse, String> {
    reset_compression_cancel_flag();
    let _priority_guard = ScopedPriorityGuard::new();

    if payload.files.is_empty() {
        return Err("No files selected to compress".to_string());
    }

    let start_time = Instant::now();
    let first_path = Path::new(&payload.files[0]);
    let parent_dir = if let Some(ref out_dir) = payload.output_dir {
        let trimmed = out_dir.trim();
        if !trimmed.is_empty() {
            let p = Path::new(trimmed);
            let _ = std::fs::create_dir_all(p);
            p.to_path_buf()
        } else {
            first_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
        }
    } else {
        first_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    };
    
    let base_name = if let Some(ref name) = payload.custom_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            trimmed.to_string()
        } else if payload.files.len() == 1 {
            first_path.file_stem().and_then(|s| s.to_str()).unwrap_or("archive").to_string()
        } else {
            "bayzip_archive".to_string()
        }
    } else if payload.files.len() == 1 {
        first_path.file_stem().and_then(|s| s.to_str()).unwrap_or("archive").to_string()
    } else {
        "bayzip_archive".to_string()
    };

    let ext = if payload.format.starts_with('.') {
        payload.format.clone()
    } else {
        format!(".{}", payload.format)
    };
    
    let archive_name = format!("{}{}", base_name, ext);
    let output_path = parent_dir.join(&archive_name);
    
    // Calculate total original size across files & folders
    let mut total_original: u64 = 0;
    for file_str in &payload.files {
        let (s, _) = compute_path_size(Path::new(file_str));
        total_original += s;
    }

    // Initial 0% progress event
    let _ = app.emit("compress_progress", CompressProgress {
        percent: 0.0,
        current_file: "Preparing files...".into(),
        bytes_processed: 0,
        total_bytes: total_original,
    });

    let pwd_ref = payload.password.as_deref();
    let enc_ref = payload.encryption.as_deref();
    let fmt_clean = payload.format.trim_start_matches('.').to_lowercase();
    let compress_res = match fmt_clean.as_str() {
        "zip" => {
            compress_zip(
                &payload.files,
                &output_path,
                &payload.algorithm,
                payload.slider_value,
                pwd_ref,
                enc_ref,
                &app,
                total_original,
            )
        },
        "tar" => {
            compress_tar(&payload.files, &output_path, &app, total_original)
        },
        "7z" => {
            compress_7z(&payload.files, &output_path, payload.slider_value, pwd_ref, &app, total_original)
        },
        "bay_gamer" => {
            compress_bay(
                &payload.files,
                &output_path,
                "gamer",
                payload.slider_value,
                pwd_ref,
                &app,
                total_original,
            )
        },
        _ => {
            compress_bay(
                &payload.files,
                &output_path,
                &payload.algorithm,
                payload.slider_value,
                pwd_ref,
                &app,
                total_original,
            )
        }
    };

    if let Err(e) = compress_res {
        if is_compression_cancelled() || e.contains("cancelled") {
            if output_path.exists() {
                let _ = std::fs::remove_file(&output_path);
            }
            return Err("Compression cancelled by user".to_string());
        }
        return Err(e);
    }

    if is_compression_cancelled() {
        if output_path.exists() {
            let _ = std::fs::remove_file(&output_path);
        }
        return Err("Compression cancelled by user".to_string());
    }

    // Final 100% progress event
    let _ = app.emit("compress_progress", CompressProgress {
        percent: 100.0,
        current_file: "Completed".into(),
        bytes_processed: total_original,
        total_bytes: total_original,
    });

    let compressed_size = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(CompressResponse {
        success: true,
        output_path: output_path.to_string_lossy().to_string(),
        original_size: total_original,
        compressed_size,
        elapsed_ms: start_time.elapsed().as_millis() as u64,
        error: None,
    })
}

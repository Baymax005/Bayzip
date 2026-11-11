use std::fs::File;
use std::io::{Read, Write, BufWriter};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use zip::write::SimpleFileOptions;
use zip::AesMode;

use crate::types::CompressProgress;
use crate::inspector::{safe_atomic_rename, get_optimal_read_slice, get_optimal_write_buffer};
use super::{is_compression_cancelled, get_io_buffers};
use super::utils::{is_precompressed_file, is_path_excluded};

#[allow(clippy::too_many_arguments)]
pub fn compress_zip(
    files: &[String],
    output_path: &Path,
    algorithm: &str,
    slider_value: i32,
    password: Option<&str>,
    encryption: Option<&str>,
    app: &AppHandle,
    total_original: u64,
    smart_bypass: bool,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<(), String> {
    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let temp_output = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.zip".into())
    ));

    let res = (|| -> Result<(), String> {
        let (read_buf_size, mut write_buf_size) = get_io_buffers(slider_value);
        write_buf_size = get_optimal_write_buffer(&temp_output, write_buf_size);
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create zip file: {}", e))?;
        // Buffer capacity scaled by Hardware Allocation slider & destination drive bus
        let buf_writer = BufWriter::with_capacity(write_buf_size, out_file);
        let mut writer = zip::ZipWriter::new(buf_writer);
    
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

    // Allocate read buffer scaled by hardware allocation
    let mut read_buf = vec![0u8; read_buf_size];

    let deflate_level = match slider_value {
        0..=25 => 1,
        26..=60 => 4,
        61..=85 => 6,
        _ => 9,
    };

    for file_str in files {
        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }
        let p = Path::new(file_str);
        let parent_dir = p.parent().unwrap_or(p);
        let root_rel = p.strip_prefix(parent_dir).unwrap_or(p);
        let root_str = root_rel.to_string_lossy().replace('\\', "/");
        let clean_root = root_str.trim_start_matches('/').trim_end_matches('/');
        if is_path_excluded(clean_root, excluded) {
            continue;
        }
        
        let file_len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let is_precomp = smart_bypass && is_precompressed_file(p);
        let entry_method = if is_precomp {
            zip::CompressionMethod::Stored
        } else {
            method
        };
        let options = SimpleFileOptions::default()
            .compression_method(entry_method)
            .compression_level(if entry_method == zip::CompressionMethod::Stored { None } else { Some(deflate_level) })
            .large_file(file_len >= 0xFFFF_FFFF);

        if p.is_file() {
            let slice_size = get_optimal_read_slice(p);
            let mut f = File::open(p).map_err(|e| format!("Failed to open file '{}': {}", p.display(), e))?;
            let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            if let Some(pwd) = has_pwd {
                writer.start_file(file_name, options.with_aes_encryption(aes_mode, pwd))
                    .map_err(|e| format!("Zip start file error: {}", e))?;
            } else {
                writer.start_file(file_name, options)
                    .map_err(|e| format!("Zip start file error: {}", e))?;
            }
            
            loop {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let to_read = read_buf.len().min(slice_size);
                match f.read(&mut read_buf[..to_read]) {
                    Ok(0) => break,
                    Ok(n) => {
                        if is_compression_cancelled() {
                            return Err("Compression cancelled by user".to_string());
                        }
                        writer.write_all(&read_buf[..n])
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
        } else if p.is_dir() {
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

            #[allow(clippy::too_many_arguments)]
            fn add_dir_to_zip<W: Write + std::io::Seek>(
                dir: &Path,
                base: &Path,
                writer: &mut zip::ZipWriter<W>,
                read_buf: &mut [u8],
                method: zip::CompressionMethod,
                deflate_level: i64,
                has_pwd: Option<&str>,
                aes_mode: AesMode,
                bytes_processed: &mut u64,
                total_original: u64,
                app: &AppHandle,
                last_emit: &mut Instant,
                depth: usize,
                smart_bypass: bool,
                excluded: Option<&std::collections::HashSet<String>>,
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
                            if clean_rel.is_empty() || is_path_excluded(clean_rel, excluded) { continue; }
                            
                            let is_large = m.len() >= 0xFFFF_FFFF;
                            let is_precomp = smart_bypass && is_precompressed_file(&path);
                            let entry_method = if is_precomp {
                                zip::CompressionMethod::Stored
                            } else {
                                method
                            };
                            let options = SimpleFileOptions::default()
                                .compression_method(entry_method)
                                .compression_level(if entry_method == zip::CompressionMethod::Stored { None } else { Some(deflate_level) })
                                .large_file(is_large);

                            if m.is_dir() && !m.file_type().is_symlink() {
                                let dir_name = format!("{}/", clean_rel);
                                if let Some(pwd) = has_pwd {
                                    let _ = writer.add_directory(&dir_name, options.with_aes_encryption(aes_mode, pwd));
                                } else {
                                    let _ = writer.add_directory(&dir_name, options);
                                }
                                add_dir_to_zip(&path, base, writer, read_buf, method, deflate_level, has_pwd, aes_mode, bytes_processed, total_original, app, last_emit, depth + 1, smart_bypass, excluded)?;
                            } else if m.is_file() {
                                let mut f = match File::open(&path) {
                                    Ok(file) => file,
                                    Err(_) => continue, // Skip unreadable/locked file without creating ghost 0-byte entry!
                                };
                                if let Some(pwd) = has_pwd {
                                    writer.start_file(clean_rel, options.with_aes_encryption(aes_mode, pwd))
                                        .map_err(|e| format!("Zip start file error: {}", e))?;
                                } else {
                                    writer.start_file(clean_rel, options)
                                        .map_err(|e| format!("Zip start file error: {}", e))?;
                                }
                                let slice_size = get_optimal_read_slice(&path);
                                loop {
                                    if is_compression_cancelled() {
                                        return Err("Compression cancelled by user".to_string());
                                    }
                                    let to_read = read_buf.len().min(slice_size);
                                    match f.read(&mut read_buf[..to_read]) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            if is_compression_cancelled() {
                                                return Err("Compression cancelled by user".to_string());
                                            }
                                            writer.write_all(&read_buf[..n])
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
                Ok(())
            }
            add_dir_to_zip(p, parent_dir, &mut writer, &mut read_buf, method, deflate_level, has_pwd, aes_mode, &mut bytes_processed, total_original, app, &mut last_emit, 0, smart_bypass, excluded)?;
        }
    }

        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }

        let mut inner = writer.finish().map_err(|e| format!("Failed to finalize zip archive: {}", e))?;
        inner.flush().map_err(|e| format!("Failed to flush zip buffer: {}", e))?;
        drop(inner);

        safe_atomic_rename(&temp_output, output_path)
            .map_err(|e| format!("Failed to rename temporary zip file to destination: {}", e))?;

        Ok(())
    })();

    if res.is_err() && temp_output.exists() {
        let _ = std::fs::remove_file(&temp_output);
    }

    res
}

pub fn headless_quick_compress_zip(p: &Path, output_path: &Path) -> Result<(), String> {
    headless_quick_compress_zip_items(&[p], output_path)
}

pub fn headless_quick_compress_zip_items(paths: &[&Path], output_path: &Path) -> Result<(), String> {
    let temp_output = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.zip".into())
    ));

    let res = (|| -> Result<(), String> {
        let write_buf_size = get_optimal_write_buffer(&temp_output, 1024 * 1024);
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create temporary zip file: {}", e))?;
        // Buffer scaled by destination bus to prevent disk thrashing and maximize sequential write speeds
        let buf_writer = BufWriter::with_capacity(write_buf_size, out_file);
        let mut writer = zip::ZipWriter::new(buf_writer);

        fn add_dir<W: Write + std::io::Seek>(
            dir: &Path,
            base: &Path,
            writer: &mut zip::ZipWriter<W>,
            depth: usize,
        ) -> Result<(), String> {
            if depth > 32 || is_compression_cancelled() { return Ok(()); }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if is_compression_cancelled() {
                        return Err("Compression cancelled by user".to_string());
                    }
                    let path = entry.path();
                    if let Ok(m) = std::fs::symlink_metadata(&path) {
                        let rel = path.strip_prefix(base).unwrap_or(&path);
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        if m.is_dir() && !m.file_type().is_symlink() {
                            let dir_opt = SimpleFileOptions::default()
                                .compression_method(zip::CompressionMethod::Deflated)
                                .compression_level(Some(6))
                                .large_file(false);
                            let _ = writer.add_directory(format!("{}/", rel_str), dir_opt);
                            add_dir(&path, base, writer, depth + 1)?;
                        } else if m.is_file() {
                            if let Ok(mut f) = File::open(&path) {
                                let is_precomp = is_precompressed_file(&path);
                                let entry_method = if is_precomp {
                                    zip::CompressionMethod::Stored
                                } else {
                                    zip::CompressionMethod::Deflated
                                };
                                let file_opt = SimpleFileOptions::default()
                                    .compression_method(entry_method)
                                    .compression_level(if is_precomp { None } else { Some(6) })
                                    .large_file(m.len() >= 0xFFFF_FFFF);
                                writer.start_file(&rel_str, file_opt)
                                    .map_err(|e| format!("Zip start error: {}", e))?;
                                std::io::copy(&mut f, writer)
                                    .map_err(|e| format!("Zip write error: {}", e))?;
                            }
                        }
                    }
                }
            }
            Ok(())
        }

        for &p in paths {
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let parent = p.parent().unwrap_or(p);
            if p.is_file() {
                if let Ok(mut f) = File::open(p) {
                    let file_len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                    let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    let is_precomp = is_precompressed_file(p);
                    let entry_method = if is_precomp {
                        zip::CompressionMethod::Stored
                    } else {
                        zip::CompressionMethod::Deflated
                    };
                    let file_opt = SimpleFileOptions::default()
                        .compression_method(entry_method)
                        .compression_level(if is_precomp { None } else { Some(6) })
                        .large_file(file_len >= 0xFFFF_FFFF);
                    writer.start_file(file_name, file_opt)
                        .map_err(|e| format!("Zip start error: {}", e))?;
                    std::io::copy(&mut f, &mut writer)
                        .map_err(|e| format!("Zip copy error: {}", e))?;
                }
            } else if p.is_dir() {
                let rel_name = p.strip_prefix(parent).unwrap_or(p);
                let rel_str = rel_name.to_string_lossy().replace('\\', "/");
                let clean_rel = rel_str.trim_start_matches('/').to_string();
                if !clean_rel.is_empty() {
                    let dir_opt = SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated)
                        .compression_level(Some(6))
                        .large_file(false);
                    let _ = writer.add_directory(format!("{}/", clean_rel), dir_opt);
                }
                add_dir(p, parent, &mut writer, 0)?;
            }
        }

        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }

        writer.finish().map_err(|e| format!("Failed to finalize zip: {}", e))?;

        safe_atomic_rename(&temp_output, output_path)
            .map_err(|e| format!("Failed to rename temporary zip file to destination: {}", e))?;

        Ok(())
    })();

    if res.is_err() && temp_output.exists() {
        let _ = std::fs::remove_file(&temp_output);
    }

    res
}

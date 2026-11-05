use std::fs::File;
use std::io::{Read, Write, BufWriter};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

use crate::types::{ArchiveEntryItem, CompressProgress};
use crate::inspector::{safe_atomic_rename, get_optimal_read_slice, get_optimal_write_buffer};
use crate::crypto::ChunkedAesWriter;
use super::{is_compression_cancelled, calculate_target_threads, calculate_zstd_level, get_io_buffers, is_gamer_preset};
use super::utils::{is_precompressed_file, is_path_excluded};

pub fn collect_bay_toc(dir: &Path, base: &Path, list: &mut Vec<ArchiveEntryItem>, depth: usize) {
    if depth > 32 { return; }
    if let Ok(m) = std::fs::symlink_metadata(dir) {
        let rel = dir.strip_prefix(base).unwrap_or(dir);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let clean = rel_str.trim_start_matches('/').to_string();
        if !clean.is_empty() {
            if m.is_dir() && !m.file_type().is_symlink() {
                list.push(ArchiveEntryItem {
                    name: format!("{}/", clean.trim_end_matches('/')),
                    size: 0,
                    is_dir: true,
                });
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        collect_bay_toc(&entry.path(), base, list, depth + 1);
                    }
                }
            } else if m.is_file() {
                list.push(ArchiveEntryItem {
                    name: clean,
                    size: m.len(),
                    is_dir: false,
                });
            }
        }
    }
}

pub fn write_file_to_bay_archive<W: Write>(
    filepath: &Path,
    rel_name: &str,
    encoder: &mut W,
    read_buf: &mut [u8],
    bytes_processed: &mut u64,
    total_original: u64,
    app: &AppHandle,
    last_emit: &mut Instant,
    toc_entries: &mut Vec<ArchiveEntryItem>,
) -> Result<(), String> {
    // Open file first and check access BEFORE writing header to prevent framing corruption
    let mut f = match File::open(filepath) {
        Ok(file) => file,
        Err(_) => {
            // File is locked or inaccessible: skip without writing corrupted 0-byte header
            return Ok(());
        }
    };
    let file_meta = f.metadata().ok();
    let file_size = file_meta.map(|m| m.len()).unwrap_or(0);

    encoder.write_all(&[1u8]).map_err(|e| e.to_string())?; // 1 = File entry
    let filename_bytes = rel_name.as_bytes();
    encoder.write_all(&(filename_bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    encoder.write_all(filename_bytes).map_err(|e| e.to_string())?;
    encoder.write_all(&file_size.to_le_bytes()).map_err(|e| e.to_string())?;

    toc_entries.push(ArchiveEntryItem {
        name: rel_name.to_string(),
        size: file_size,
        is_dir: false,
    });

    let slice_size = get_optimal_read_slice(filepath);
    let mut written_bytes: u64 = 0;
    while written_bytes < file_size {
        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }
        let to_read = ((file_size - written_bytes) as usize)
            .min(read_buf.len())
            .min(slice_size);
        match f.read(&mut read_buf[..to_read]) {
            Ok(0) => break,
            Ok(n) => {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                encoder.write_all(&read_buf[..n]).map_err(|e| e.to_string())?;
                written_bytes += n as u64;
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

    // Framing safeguard: If the file was truncated during reading, pad with zero bytes
    // to strictly preserve container stream framing alignment for extraction!
    if written_bytes < file_size {
        let padding_needed = file_size - written_bytes;
        let zeros = [0u8; 8192];
        let mut rem = padding_needed;
        while rem > 0 {
            let to_write = (rem as usize).min(zeros.len());
            encoder.write_all(&zeros[..to_write]).map_err(|e| e.to_string())?;
            rem -= to_write as u64;
        }
    }

    // Ensure final byte count of file is emitted to UI
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

    Ok(())
}

pub fn write_dir_to_bay_archive<W: Write>(
    dir: &Path,
    base_parent: &Path,
    encoder: &mut W,
    read_buf: &mut [u8],
    bytes_processed: &mut u64,
    total_original: u64,
    app: &AppHandle,
    depth: usize,
    last_emit: &mut Instant,
    toc_entries: &mut Vec<ArchiveEntryItem>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<(), String> {
    if depth > 32 {
        return Ok(());
    }
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = read_dir.flatten().collect();
        // Solid archiving heuristic:
        // Group compressible files together before high-entropy pre-compressed media.
        entries.sort_by_key(|e| is_precompressed_file(&e.path()));
        for entry in entries {
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let ep = entry.path();
            let rel = ep.strip_prefix(base_parent).unwrap_or(&ep);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let clean_rel = rel_str.trim_start_matches('/');
            if is_path_excluded(clean_rel, excluded) {
                continue;
            }
            if let Ok(m) = std::fs::symlink_metadata(&ep) {
                if m.is_file() {
                    write_file_to_bay_archive(&ep, clean_rel, encoder, read_buf, bytes_processed, total_original, app, last_emit, toc_entries)?;
                } else if m.is_dir() && !m.file_type().is_symlink() {
                    let dir_name = format!("{}/", clean_rel);
                    let dir_bytes = dir_name.as_bytes();
                    encoder.write_all(&[2u8]).map_err(|e| e.to_string())?; // 2 = Directory entry
                    encoder.write_all(&(dir_bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
                    encoder.write_all(dir_bytes).map_err(|e| e.to_string())?;
                    encoder.write_all(&0u64.to_le_bytes()).map_err(|e| e.to_string())?;
                    toc_entries.push(ArchiveEntryItem {
                        name: dir_name,
                        size: 0,
                        is_dir: true,
                    });
                    write_dir_to_bay_archive(&ep, base_parent, encoder, read_buf, bytes_processed, total_original, app, depth + 1, last_emit, toc_entries, excluded)?;
                }
            }
        }
    }
    Ok(())
}

pub enum BayOutputWriter {
    Plain(BufWriter<File>),
    Encrypted(ChunkedAesWriter<BufWriter<File>>),
}

impl Write for BayOutputWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            BayOutputWriter::Plain(w) => w.write(buf),
            BayOutputWriter::Encrypted(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            BayOutputWriter::Plain(w) => w.flush(),
            BayOutputWriter::Encrypted(w) => w.flush(),
        }
    }
}

impl BayOutputWriter {
    pub fn finish(self) -> std::io::Result<BufWriter<File>> {
        match self {
            BayOutputWriter::Plain(mut w) => {
                w.flush()?;
                Ok(w)
            }
            BayOutputWriter::Encrypted(w) => w.finish(),
        }
    }
}

pub fn write_bay_toc_trailer<W: Write>(
    file_writer: &mut W,
    toc_entries: &[ArchiveEntryItem],
    password: Option<&str>,
) -> std::io::Result<()> {
    if toc_entries.is_empty() {
        return Ok(());
    }

    let toc_json = serde_json::to_vec(toc_entries).unwrap_or_default();
    if toc_json.is_empty() {
        return Ok(());
    }

    if let Some(pwd) = password.filter(|p| !p.trim().is_empty()) {
        use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit}};
        use rand::RngCore;

        let mut salt = [0u8; 16];
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let key_bytes = crate::crypto::derive_bay_key(pwd.trim(), &salt);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let encrypted_toc = cipher
            .encrypt(nonce, toc_json.as_slice())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("TOC encryption error: {}", e)))?;

        let enc_toc_len = encrypted_toc.len() as u64;

        file_writer.write_all(&encrypted_toc)?;
        file_writer.write_all(&enc_toc_len.to_le_bytes())?;
        file_writer.write_all(&salt)?;
        file_writer.write_all(&nonce_bytes)?;
        file_writer.write_all(b"BAYETOC\x03")?;
        file_writer.flush()?;
    } else {
        let toc_len = toc_json.len() as u64;
        file_writer.write_all(&toc_json)?;
        file_writer.write_all(&toc_len.to_le_bytes())?;
        file_writer.write_all(b"BAYTOC\x02\x00")?;
        file_writer.flush()?;
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
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<(), String> {
    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    let pwd_trimmed = password.map(|s| s.trim()).filter(|s| !s.is_empty());

    let temp_final = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.bay".into())
    ));

    let res = (|| -> Result<(), String> {
        let first_path = Path::new(&files[0]);
        let parent_dir = first_path.parent().unwrap_or_else(|| Path::new("."));

        let algo_lower = algorithm.to_lowercase();
        let ext_is_gamer = output_path.extension()
            .and_then(|s| s.to_str())
            .map(is_gamer_preset)
            .unwrap_or(false);
        let is_gamer = is_gamer_preset(&algo_lower)
            || algo_lower.contains("bayg") 
            || algo_lower.contains("gamer") 
            || ext_is_gamer;
        let zstd_level = calculate_zstd_level(slider_value, is_gamer);
        let mut bytes_processed: u64 = 0;
        let mut last_emit = Instant::now();

        let (read_buf_size, mut write_buf_size) = get_io_buffers(slider_value);
        write_buf_size = get_optimal_write_buffer(&temp_final, write_buf_size);
        let out_file = File::create(&temp_final)
            .map_err(|e| format!("Failed to create intermediate file: {}", e))?;
        // Buffer capacity scaled by destination drive bus (512 KB USB / 1 MB SATA / 2 MB NVMe)
        let buf_out = BufWriter::with_capacity(write_buf_size, out_file);

        let out_writer = if let Some(pwd) = pwd_trimmed {
            BayOutputWriter::Encrypted(
                ChunkedAesWriter::new(buf_out, pwd).map_err(|e| e.to_string())?
            )
        } else {
            BayOutputWriter::Plain(buf_out)
        };

        // Allocate read buffer scaled by hardware allocation
        let mut read_buf = vec![0u8; read_buf_size];

        // Dynamic TOC entries collected in-flight without double-scanning disk
        let mut toc_entries: Vec<ArchiveEntryItem> = Vec::new();

        // Solid archiving heuristic: group compressible text/code/binaries together first,
        // placing high-entropy pre-compressed media at the end.
        let mut ordered_files = files.to_vec();
        ordered_files.sort_by_key(|f| is_precompressed_file(Path::new(f)));

        if algo_lower.contains("brotli") {
            let mut out_sink = out_writer;
            out_sink.write_all(b"BAYBRO\x02\x00")
                .map_err(|e| format!("Header write error: {}", e))?;
            let quality = match slider_value {
                0 => 1,
                25 => 4,
                50 => 7,
                75 => 9,
                _ => 11,
            };
            let mut encoder = brotli::CompressorWriter::new(out_sink, 64 * 1024, quality, 22);
            for file_str in &ordered_files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                let rel = p.strip_prefix(parent_dir).unwrap_or(p);
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let clean_rel = rel_str.trim_start_matches('/').to_string();
                if is_path_excluded(&clean_rel, excluded) {
                    continue;
                }
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, &mut last_emit, &mut toc_entries)?;
                } else if p.is_dir() {
                    if !clean_rel.is_empty() {
                        let dir_name = format!("{}/", clean_rel);
                        let dir_bytes = dir_name.as_bytes();
                        encoder.write_all(&[2u8]).map_err(|e| e.to_string())?;
                        encoder.write_all(&(dir_bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
                        encoder.write_all(dir_bytes).map_err(|e| e.to_string())?;
                        encoder.write_all(&0u64.to_le_bytes()).map_err(|e| e.to_string())?;
                        toc_entries.push(ArchiveEntryItem {
                            name: dir_name,
                            size: 0,
                            is_dir: true,
                        });
                    }
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, 0, &mut last_emit, &mut toc_entries, excluded)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
            let _ = app.emit("compress_progress", CompressProgress {
                percent: 99.0,
                current_file: "Finalizing archive...".to_string(),
                bytes_processed,
                total_bytes: total_original,
            });
            encoder.flush().map_err(|e| format!("Brotli flush error: {}", e))?;
            let finished_writer = encoder.into_inner();
            let mut file_writer = finished_writer.finish().map_err(|e| format!("Brotli finalize error: {}", e))?;
            write_bay_toc_trailer(&mut file_writer, &toc_entries, pwd_trimmed)
                .map_err(|e| format!("Failed to write TOC: {}", e))?;
        } else if algo_lower.contains("lz4") {
            let mut out_sink = out_writer;
            out_sink.write_all(b"BAYLZ4\x02\x00")
                .map_err(|e| format!("Header write error: {}", e))?;
            let mut encoder = lz4_flex::frame::FrameEncoder::new(out_sink);
            for file_str in &ordered_files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                let rel = p.strip_prefix(parent_dir).unwrap_or(p);
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let clean_rel = rel_str.trim_start_matches('/').to_string();
                if is_path_excluded(&clean_rel, excluded) {
                    continue;
                }
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, &mut last_emit, &mut toc_entries)?;
                } else if p.is_dir() {
                    if !clean_rel.is_empty() {
                        let dir_name = format!("{}/", clean_rel);
                        let dir_bytes = dir_name.as_bytes();
                        encoder.write_all(&[2u8]).map_err(|e| e.to_string())?;
                        encoder.write_all(&(dir_bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
                        encoder.write_all(dir_bytes).map_err(|e| e.to_string())?;
                        encoder.write_all(&0u64.to_le_bytes()).map_err(|e| e.to_string())?;
                        toc_entries.push(ArchiveEntryItem {
                            name: dir_name,
                            size: 0,
                            is_dir: true,
                        });
                    }
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, 0, &mut last_emit, &mut toc_entries, excluded)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
            let _ = app.emit("compress_progress", CompressProgress {
                percent: 99.0,
                current_file: "Finalizing archive...".to_string(),
                bytes_processed,
                total_bytes: total_original,
            });
            let finished_writer = encoder.finish().map_err(|e| format!("LZ4 finalize error: {}", e))?;
            let mut file_writer = finished_writer.finish().map_err(|e| format!("LZ4 finalize error: {}", e))?;
            write_bay_toc_trailer(&mut file_writer, &toc_entries, pwd_trimmed)
                .map_err(|e| format!("Failed to write TOC: {}", e))?;
        } else {
            // Default to Zstandard or .bayg preset
            let mut out_sink = out_writer;
            out_sink.write_all(b"BAYZIP\x02\x00")
                .map_err(|e| format!("Header write error: {}", e))?;
            let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let mut threads = calculate_target_threads(slider_value, total_cores);
            if zstd_level >= 19 {
                threads = threads.min(8);
            }

            let mut encoder = zstd::stream::Encoder::new(out_sink, zstd_level)
                .map_err(|e| format!("Failed to initialize compression encoder: {}", e))?;
            let _ = encoder.multithread(threads as u32);
            if is_gamer {
                let _ = encoder.include_checksum(false);
            }
            for file_str in &ordered_files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                let rel = p.strip_prefix(parent_dir).unwrap_or(p);
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let clean_rel = rel_str.trim_start_matches('/').to_string();
                if is_path_excluded(&clean_rel, excluded) {
                    continue;
                }
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, &mut last_emit, &mut toc_entries)?;
                } else if p.is_dir() {
                    if !clean_rel.is_empty() {
                        let dir_name = format!("{}/", clean_rel);
                        let dir_bytes = dir_name.as_bytes();
                        encoder.write_all(&[2u8]).map_err(|e| e.to_string())?;
                        encoder.write_all(&(dir_bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
                        encoder.write_all(dir_bytes).map_err(|e| e.to_string())?;
                        encoder.write_all(&0u64.to_le_bytes()).map_err(|e| e.to_string())?;
                        toc_entries.push(ArchiveEntryItem {
                            name: dir_name,
                            size: 0,
                            is_dir: true,
                        });
                    }
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, 0, &mut last_emit, &mut toc_entries, excluded)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
            let _ = app.emit("compress_progress", CompressProgress {
                percent: 99.0,
                current_file: "Finalizing archive...".to_string(),
                bytes_processed,
                total_bytes: total_original,
            });
            let finished_writer = encoder.finish().map_err(|e| format!("Failed to finalize compression: {}", e))?;
            let mut file_writer = finished_writer.finish().map_err(|e| format!("Failed to finalize output stream: {}", e))?;
            write_bay_toc_trailer(&mut file_writer, &toc_entries, pwd_trimmed)
                .map_err(|e| format!("Failed to write TOC: {}", e))?;
        }

        // Atomic rename of completed archive
        safe_atomic_rename(&temp_final, output_path)
            .map_err(|e| format!("Failed to rename temporary archive to destination: {}", e))?;

        Ok(())
    })();

    if res.is_err() && temp_final.exists() {
        let _ = std::fs::remove_file(&temp_final);
    }

    res
}

pub fn headless_quick_compress_bay(p: &Path, output_path: &Path, is_gamer: bool) -> Result<(), String> {
    headless_quick_compress_bay_items(&[p], output_path, is_gamer)
}

pub fn headless_quick_compress_bay_items(paths: &[&Path], output_path: &Path, is_gamer: bool) -> Result<(), String> {
    let mut toc_entries: Vec<ArchiveEntryItem> = Vec::new();

    let (read_buf_size, mut write_buf_size) = get_io_buffers(50);
    write_buf_size = get_optimal_write_buffer(output_path, write_buf_size);
    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    let mut buf_out = BufWriter::with_capacity(write_buf_size, out_file);
    buf_out.write_all(b"BAYZIP\x02\x00")
        .map_err(|e| format!("Header write error: {}", e))?;
    
    // Balanced profile (50% slider equivalent)
    let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let threads = calculate_target_threads(50, total_cores);
    let zstd_level = calculate_zstd_level(50, is_gamer);
    let mut encoder = zstd::stream::Encoder::new(buf_out, zstd_level)
        .map_err(|e| format!("Encoder error: {}", e))?;
    let _ = encoder.multithread(threads as u32);
    if is_gamer {
        let _ = encoder.include_checksum(false);
    }

    let mut read_buf = vec![0u8; read_buf_size];

    fn add_dir<W: Write>(dir: &Path, base_parent: &Path, encoder: &mut zstd::stream::Encoder<W>, read_buf: &mut [u8], depth: usize, toc_entries: &mut Vec<ArchiveEntryItem>) {
        if depth > 32 {
            return;
        }
        if let Ok(read_dir) = std::fs::read_dir(dir) {
            let mut entries: Vec<_> = read_dir.flatten().collect();
            entries.sort_by_key(|e| is_precompressed_file(&e.path()));
            for entry in entries {
                let entry_path = entry.path();
                if let Ok(m) = std::fs::symlink_metadata(&entry_path) {
                    if m.is_file() {
                        if let Ok(mut f) = File::open(&entry_path) {
                            let rel_name = entry_path.strip_prefix(base_parent).unwrap_or(&entry_path);
                            let rel_str = rel_name.to_string_lossy().replace('\\', "/");
                            let clean_rel = rel_str.trim_start_matches('/').to_string();
                            let bytes = clean_rel.as_bytes();
                            let file_size = f.metadata().map(|m| m.len()).unwrap_or(m.len());
                            let _ = encoder.write_all(&[1u8]);
                            let _ = encoder.write_all(&(bytes.len() as u32).to_le_bytes());
                            let _ = encoder.write_all(bytes);
                            let _ = encoder.write_all(&file_size.to_le_bytes());
                            toc_entries.push(ArchiveEntryItem {
                                name: clean_rel,
                                size: file_size,
                                is_dir: false,
                            });

                            let slice_size = get_optimal_read_slice(&entry_path);
                            let mut written: u64 = 0;
                            while written < file_size {
                                let to_read = ((file_size - written) as usize).min(read_buf.len()).min(slice_size);
                                match f.read(&mut read_buf[..to_read]) {
                                    Ok(0) => break,
                                    Ok(n) => { 
                                        let _ = encoder.write_all(&read_buf[..n]); 
                                        written += n as u64;
                                    },
                                    Err(_) => break,
                                }
                            }
                            if written < file_size {
                                let zeros = [0u8; 4096];
                                let mut rem = file_size - written;
                                while rem > 0 {
                                    let to_write = (rem as usize).min(zeros.len());
                                    let _ = encoder.write_all(&zeros[..to_write]);
                                    rem -= to_write as u64;
                                }
                            }
                        }
                    } else if m.is_dir() && !m.file_type().is_symlink() {
                        let rel_name = entry_path.strip_prefix(base_parent).unwrap_or(&entry_path);
                        let rel_str = rel_name.to_string_lossy().replace('\\', "/");
                        let clean_rel = rel_str.trim_start_matches('/').to_string();
                        if !clean_rel.is_empty() {
                            let dir_name = format!("{}/", clean_rel);
                            let dir_bytes = dir_name.as_bytes();
                            let _ = encoder.write_all(&[2u8]);
                            let _ = encoder.write_all(&(dir_bytes.len() as u32).to_le_bytes());
                            let _ = encoder.write_all(dir_bytes);
                            let _ = encoder.write_all(&0u64.to_le_bytes());
                            toc_entries.push(ArchiveEntryItem {
                                name: dir_name,
                                size: 0,
                                is_dir: true,
                            });
                        }
                        add_dir(&entry_path, base_parent, encoder, read_buf, depth + 1, toc_entries);
                    }
                }
            }
        }
    }

    for &p in paths {
        let parent = p.parent().unwrap_or(p);
        if p.is_file() {
            if let Ok(mut f) = File::open(p) {
                let file_meta = f.metadata().ok();
                let file_size = file_meta.map(|m| m.len()).unwrap_or(0);
                let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                let filename_bytes = filename.as_bytes();
                
                let _ = encoder.write_all(&[1u8]); // 1 = file entry
                let _ = encoder.write_all(&(filename_bytes.len() as u32).to_le_bytes());
                let _ = encoder.write_all(filename_bytes);
                let _ = encoder.write_all(&file_size.to_le_bytes());
                toc_entries.push(ArchiveEntryItem {
                    name: filename.to_string(),
                    size: file_size,
                    is_dir: false,
                });

                let slice_size = get_optimal_read_slice(p);
                let mut written: u64 = 0;
                while written < file_size {
                    let to_read = ((file_size - written) as usize).min(read_buf.len()).min(slice_size);
                    match f.read(&mut read_buf[..to_read]) {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = encoder.write_all(&read_buf[..n]);
                            written += n as u64;
                        }
                        Err(_) => break,
                    }
                }
                if written < file_size {
                    let zeros = [0u8; 4096];
                    let mut rem = file_size - written;
                    while rem > 0 {
                        let to_write = (rem as usize).min(zeros.len());
                        let _ = encoder.write_all(&zeros[..to_write]);
                        rem -= to_write as u64;
                    }
                }
            }
        } else if p.is_dir() {
            let rel_name = p.strip_prefix(parent).unwrap_or(p);
            let rel_str = rel_name.to_string_lossy().replace('\\', "/");
            let clean_rel = rel_str.trim_start_matches('/').to_string();
            if !clean_rel.is_empty() {
                let dir_name = format!("{}/", clean_rel);
                let dir_bytes = dir_name.as_bytes();
                let _ = encoder.write_all(&[2u8]);
                let _ = encoder.write_all(&(dir_bytes.len() as u32).to_le_bytes());
                let _ = encoder.write_all(dir_bytes);
                let _ = encoder.write_all(&0u64.to_le_bytes());
                toc_entries.push(ArchiveEntryItem {
                    name: dir_name,
                    size: 0,
                    is_dir: true,
                });
            }
            add_dir(p, parent, &mut encoder, &mut read_buf, 0, &mut toc_entries);
        }
    }

    let _ = encoder.write_all(&[0u8]); // 0 = EOF
    let mut finished_writer = encoder.finish().map_err(|e| format!("Finalize error: {}", e))?;
    write_bay_toc_trailer(&mut finished_writer, &toc_entries, None)
        .map_err(|e| format!("Failed to write TOC: {}", e))?;
    Ok(())
}

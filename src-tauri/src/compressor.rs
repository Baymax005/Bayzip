use std::fs::File;
use std::io::{Read, Write, Seek, BufWriter};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use zip::write::SimpleFileOptions;
use zip::AesMode;
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

/// Reader adapter that tracks streaming bytes in real-time and detects cancellation
pub struct ProgressReader<'a, R: Read> {
    inner: R,
    bytes_processed: &'a mut u64,
    total_original: u64,
    app: &'a AppHandle,
    last_emit: &'a mut Instant,
    current_file: &'a str,
}

impl<'a, R: Read> ProgressReader<'a, R> {
    pub fn new(
        inner: R,
        bytes_processed: &'a mut u64,
        total_original: u64,
        app: &'a AppHandle,
        last_emit: &'a mut Instant,
        current_file: &'a str,
    ) -> Self {
        Self {
            inner,
            bytes_processed,
            total_original,
            app,
            last_emit,
            current_file,
        }
    }
}

impl<'a, R: Read> Read for ProgressReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if is_compression_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Compression cancelled by user",
            ));
        }
        let n = self.inner.read(buf)?;
        if n > 0 {
            *self.bytes_processed += n as u64;
            if self.last_emit.elapsed().as_millis() >= 50 {
                *self.last_emit = Instant::now();
                let pct = if self.total_original > 0 {
                    ((*self.bytes_processed as f64 / self.total_original as f64) * 100.0).min(99.0)
                } else {
                    50.0
                };
                let _ = self.app.emit(
                    "compress_progress",
                    CompressProgress {
                        percent: pct,
                        current_file: self.current_file.to_string(),
                        bytes_processed: *self.bytes_processed,
                        total_bytes: self.total_original,
                    },
                );
            }
        }
        Ok(n)
    }
}

impl<'a, R: Read + Seek> Seek for ProgressReader<'a, R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

use crate::types::{CompressPayload, CompressResponse, CompressProgress};
use crate::priority::ScopedPriorityGuard;
use crate::inspector::{archive_stem_name, safe_atomic_rename};
use crate::crypto::ChunkedAesWriter;

/// Exact CPU Cores scaling matrix for both .bay and .bayg:
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
/// For .bayg: <= 25 -> Level -2 (fast acceleration), 50 -> Level 1, 75 & 100 -> Level 2
/// For standard .bay: 0 -> Level 1, 25 -> Level 3, 50 -> Level 7, 75 -> Level 14, 100 -> Level 19.
pub fn calculate_zstd_level(slider_value: i32, is_gamer: bool) -> i32 {
    if is_gamer {
        match slider_value {
            s if s <= 25 => -2,
            s if s <= 50 => 1,
            _ => 2,
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

/// RAM-Scaled Read and Write Buffers by Hardware Allocation Slider:
/// 0%   -> 2 MB read / 16 MB write
/// 25%  -> 4 MB read / 32 MB write
/// 50%  -> 8 MB read / 64 MB write
/// 75%  -> 16 MB read / 96 MB write
/// 100% -> 16 MB read / 128 MB write
pub fn get_io_buffers(slider_value: i32) -> (usize, usize) {
    match slider_value {
        s if s <= 0 => (2 * 1024 * 1024, 16 * 1024 * 1024),
        s if s <= 25 => (4 * 1024 * 1024, 32 * 1024 * 1024),
        s if s <= 50 => (8 * 1024 * 1024, 64 * 1024 * 1024),
        s if s <= 75 => (16 * 1024 * 1024, 96 * 1024 * 1024),
        _ => (16 * 1024 * 1024, 128 * 1024 * 1024),
    }
}

pub fn collect_bay_toc(dir: &Path, base: &Path, list: &mut Vec<crate::types::ArchiveEntryItem>, depth: usize) {
    if depth > 32 { return; }
    if let Ok(m) = std::fs::symlink_metadata(dir) {
        let rel = dir.strip_prefix(base).unwrap_or(dir);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let clean = rel_str.trim_start_matches('/').to_string();
        if !clean.is_empty() {
            if m.is_dir() && !m.file_type().is_symlink() {
                list.push(crate::types::ArchiveEntryItem {
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
                list.push(crate::types::ArchiveEntryItem {
                    name: clean,
                    size: m.len(),
                    is_dir: false,
                });
            }
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
    read_buf: &mut [u8],
    bytes_processed: &mut u64,
    total_original: u64,
    app: &AppHandle,
    last_emit: &mut Instant,
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

    let mut written_bytes: u64 = 0;
    while written_bytes < file_size {
        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }
        let to_read = ((file_size - written_bytes) as usize).min(read_buf.len());
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
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    let clean_rel = rel_str.trim_start_matches('/');
                    write_file_to_bay_archive(&ep, clean_rel, encoder, read_buf, bytes_processed, total_original, app, last_emit)?;
                } else if m.is_dir() && !m.file_type().is_symlink() {
                    let rel = ep.strip_prefix(base_parent).unwrap_or(&ep);
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    let clean_rel = rel_str.trim_start_matches('/').to_string();
                    if !clean_rel.is_empty() {
                        let dir_name = format!("{}/", clean_rel);
                        let dir_bytes = dir_name.as_bytes();
                        encoder.write_all(&[2u8]).map_err(|e| e.to_string())?; // 2 = Directory entry
                        encoder.write_all(&(dir_bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
                        encoder.write_all(dir_bytes).map_err(|e| e.to_string())?;
                        encoder.write_all(&0u64.to_le_bytes()).map_err(|e| e.to_string())?;
                    }
                    write_dir_to_bay_archive(&ep, base_parent, encoder, read_buf, bytes_processed, total_original, app, depth + 1, last_emit)?;
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
    toc_entries: &[crate::types::ArchiveEntryItem],
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
        let mut bytes_processed: u64 = 0;
        let mut last_emit = Instant::now();

        let (read_buf_size, write_buf_size) = get_io_buffers(slider_value);
        let out_file = File::create(&temp_final)
            .map_err(|e| format!("Failed to create intermediate file: {}", e))?;
        // Buffer capacity scaled by Hardware Allocation slider to maximize sequential throughput
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

        let mut toc_entries: Vec<crate::types::ArchiveEntryItem> = Vec::new();
        for file_str in files {
            let p = Path::new(file_str);
            if p.is_file() {
                let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                let sz = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                toc_entries.push(crate::types::ArchiveEntryItem {
                    name: filename.to_string(),
                    size: sz,
                    is_dir: false,
                });
            } else if p.is_dir() {
                collect_bay_toc(p, parent_dir, &mut toc_entries, 0);
            }
        }

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
            for file_str in files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, &mut last_emit)?;
                } else if p.is_dir() {
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
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
            for file_str in files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, &mut last_emit)?;
                } else if p.is_dir() {
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
            let finished_writer = encoder.finish().map_err(|e| format!("LZ4 finalize error: {}", e))?;
            let mut file_writer = finished_writer.finish().map_err(|e| format!("LZ4 finalize error: {}", e))?;
            write_bay_toc_trailer(&mut file_writer, &toc_entries, pwd_trimmed)
                .map_err(|e| format!("Failed to write TOC: {}", e))?;
        } else {
            // Default to Zstandard or .bayg preset
            let mut out_sink = out_writer;
            out_sink.write_all(b"BAYZIP\x02\x00")
                .map_err(|e| format!("Header write error: {}", e))?;
            let is_gamer = algo_lower.contains("gamer") || algo_lower.contains("bayg");
            let zstd_level = calculate_zstd_level(slider_value, is_gamer);
            let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let threads = calculate_target_threads(slider_value, total_cores);

            let mut encoder = zstd::stream::Encoder::new(out_sink, zstd_level)
                .map_err(|e| format!("Failed to initialize compression encoder: {}", e))?;
            let _ = encoder.multithread(threads as u32);
            if is_gamer {
                let _ = encoder.include_checksum(false);
            }
            for file_str in files {
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                let p = Path::new(file_str);
                if p.is_file() {
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    write_file_to_bay_archive(p, filename, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, &mut last_emit)?;
                } else if p.is_dir() {
                    write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut read_buf, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
                }
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let _ = encoder.write_all(&[0u8]); // EOF
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

    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let temp_output = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.zip".into())
    ));

    let res = (|| -> Result<(), String> {
        let (read_buf_size, write_buf_size) = get_io_buffers(slider_value);
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create zip file: {}", e))?;
        // Buffer capacity scaled by Hardware Allocation slider to maximize sequential throughput
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
        0..=15 => 1,
        16..=35 => 3,
        36..=65 => 6,
        66..=85 => 7,
        _ => 9,
    };

    for file_str in files {
        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }
        let p = Path::new(file_str);
        
        let file_len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .compression_level(Some(deflate_level))
            .large_file(file_len >= 0xFFFF_FFFF);

        if p.is_file() {
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
                match f.read(&mut read_buf) {
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
                                .compression_level(Some(deflate_level))
                                .large_file(is_large);

                            if m.is_dir() && !m.file_type().is_symlink() {
                                let dir_name = format!("{}/", clean_rel);
                                if let Some(pwd) = has_pwd {
                                    let _ = writer.add_directory(&dir_name, options.with_aes_encryption(aes_mode, pwd));
                                } else {
                                    let _ = writer.add_directory(&dir_name, options);
                                }
                                add_dir_to_zip(&path, base, writer, read_buf, method, deflate_level, has_pwd, aes_mode, bytes_processed, total_original, app, last_emit, depth + 1)?;
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
                                loop {
                                    if is_compression_cancelled() {
                                        return Err("Compression cancelled by user".to_string());
                                    }
                                    match f.read(read_buf) {
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
            add_dir_to_zip(p, parent_dir, &mut writer, &mut read_buf, method, deflate_level, has_pwd, aes_mode, &mut bytes_processed, total_original, app, &mut last_emit, 0)?;
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

pub fn compress_tar(
    files: &[String],
    output_path: &Path,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
    if is_compression_cancelled() {
        return Err("Compression cancelled by user".to_string());
    }

    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let temp_output = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.tar".into())
    ));

    let res = (|| -> Result<(), String> {
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create tar file: {}", e))?;
        let mut builder = tar::Builder::new(BufWriter::with_capacity(32 * 1024 * 1024, out_file));

        fn add_dir_to_tar<W: Write>(
            dir: &Path,
            base: &Path,
            builder: &mut tar::Builder<W>,
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
                            let _ = builder.append_dir(trimmed, &path);
                            add_dir_to_tar(&path, base, builder, bytes_processed, total_original, app, last_emit, depth + 1)?;
                        } else if m.is_file() {
                            builder.append_path_with_name(&path, trimmed)
                                .map_err(|e| format!("Tar append file error: {}", e))?;
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

        let mut bytes_processed = 0u64;
        let mut last_emit = Instant::now();

        for file_str in files {
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            let p = Path::new(file_str);
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            if p.is_file() {
                builder.append_path_with_name(p, name)
                    .map_err(|e| format!("Tar append file error: {}", e))?;
                if let Ok(m) = p.metadata() {
                    bytes_processed += m.len();
                }
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
                let parent = p.parent().unwrap_or(p);
                let _ = builder.append_dir(name, p);
                add_dir_to_tar(p, parent, &mut builder, &mut bytes_processed, total_original, app, &mut last_emit, 0)?;
            }
        }

        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }

        builder.finish().map_err(|e| format!("Tar finalize error: {}", e))?;
        drop(builder);

        safe_atomic_rename(&temp_output, output_path)
            .map_err(|e| format!("Failed to rename temporary tar file to destination: {}", e))?;

        Ok(())
    })();

    if res.is_err() && temp_output.exists() {
        let _ = std::fs::remove_file(&temp_output);
    }

    res
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

    let temp_output = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.7z".into())
    ));

    let res = (|| -> Result<(), String> {
        let mut sz = ArchiveWriter::create(&temp_output)
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
                let prog_reader = ProgressReader::new(f, &mut bytes_processed, total_original, app, &mut last_emit, name);
                sz.push_archive_entry(entry, Some(prog_reader))
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
                                    let prog_reader = ProgressReader::new(f, bytes_processed, total_original, app, last_emit, trimmed);
                                    sz.push_archive_entry(f_entry, Some(prog_reader))
                                        .map_err(|e| format!("7z entry error: {}", e))?;
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

        let inner = sz.finish().map_err(|e| format!("7z finalize error: {}", e))?;
        drop(inner);

        safe_atomic_rename(&temp_output, output_path)
            .map_err(|e| format!("Failed to rename temporary 7z file to destination: {}", e))?;

        Ok(())
    })();

    if res.is_err() && temp_output.exists() {
        let _ = std::fs::remove_file(&temp_output);
    }

    res
}

pub fn headless_quick_compress_zip(p: &Path, output_path: &Path) -> Result<(), String> {
    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create zip file: {}", e))?;
    // 32 MB buffer to prevent disk thrashing and maximize sequential write speeds
    let buf_writer = BufWriter::with_capacity(32 * 1024 * 1024, out_file);
    let mut writer = zip::ZipWriter::new(buf_writer);
    let file_len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .large_file(file_len >= 0xFFFF_FFFF);

    let parent = p.parent().unwrap_or(p);

    if p.is_file() {
        if let Ok(mut f) = File::open(p) {
            let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            writer.start_file(file_name, options)
                .map_err(|e| format!("Zip start error: {}", e))?;
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
                                .compression_level(Some(6))
                                .large_file(false);
                            let _ = writer.add_directory(format!("{}/", rel_str), dir_opt);
                            add_dir(&path, base, writer, depth + 1)?;
                        } else if m.is_file() {
                            if let Ok(mut f) = File::open(&path) {
                                let file_opt = SimpleFileOptions::default()
                                    .compression_method(zip::CompressionMethod::Deflated)
                                    .compression_level(Some(6))
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
        add_dir(p, parent, &mut writer, 0)?;
    }

    writer.finish().map_err(|e| format!("Failed to finalize zip: {}", e))?;
    Ok(())
}

pub fn headless_quick_compress_bay(p: &Path, output_path: &Path, is_gamer: bool) -> Result<(), String> {
    let parent = p.parent().unwrap_or(p);
    let mut toc_entries: Vec<crate::types::ArchiveEntryItem> = Vec::new();
    if p.is_file() {
        let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let sz = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        toc_entries.push(crate::types::ArchiveEntryItem {
            name: filename.to_string(),
            size: sz,
            is_dir: false,
        });
    } else if p.is_dir() {
        collect_bay_toc(p, parent, &mut toc_entries, 0);
    }

    let (read_buf_size, write_buf_size) = get_io_buffers(50);
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

            let mut written: u64 = 0;
            while written < file_size {
                let to_read = ((file_size - written) as usize).min(read_buf.len());
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
        fn add_dir<W: Write>(dir: &Path, base_parent: &Path, encoder: &mut zstd::stream::Encoder<W>, read_buf: &mut [u8], depth: usize) {
            if depth > 32 {
                return;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
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

                                let mut written: u64 = 0;
                                while written < file_size {
                                    let to_read = ((file_size - written) as usize).min(read_buf.len());
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
                            }
                            add_dir(&entry_path, base_parent, encoder, read_buf, depth + 1);
                        }
                    }
                }
            }
        }
        add_dir(p, parent, &mut encoder, &mut read_buf, 0);
    }

    let _ = encoder.write_all(&[0u8]); // 0 = EOF
    let mut finished_writer = encoder.finish().map_err(|e| format!("Finalize error: {}", e))?;
    write_bay_toc_trailer(&mut finished_writer, &toc_entries, None)
        .map_err(|e| format!("Failed to write TOC: {}", e))?;
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
            let mut builder = tar::Builder::new(BufWriter::with_capacity(32 * 1024 * 1024, out_file));
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            if p.is_file() {
                builder.append_path_with_name(p, name)
                    .map_err(|e| format!("Tar append file error: {}", e))?;
            } else if p.is_dir() {
                fn add_dir_headless<W: Write>(dir: &Path, base: &Path, builder: &mut tar::Builder<W>, depth: usize) -> Result<(), String> {
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
                                    let _ = builder.append_dir(trimmed, &path);
                                    add_dir_headless(&path, base, builder, depth + 1)?;
                                } else if m.is_file() {
                                    builder.append_path_with_name(&path, trimmed)
                                        .map_err(|e| format!("Tar append file error: {}", e))?;
                                }
                            }
                        }
                    }
                    Ok(())
                }
                let _ = builder.append_dir(name, p);
                add_dir_headless(p, parent, &mut builder, 0)?;
            }
            builder.finish().map_err(|e| format!("Tar finalize error: {}", e))?;
        },
        "bayg" => headless_quick_compress_bay(p, &output_path, true)?,
        _ => headless_quick_compress_bay(p, &output_path, false)?,
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
        "bayg" => {
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

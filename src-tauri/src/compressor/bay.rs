use std::fs::File;
use std::io::{Read, Write, BufWriter};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use zeroize::Zeroize;

use crate::types::{ArchiveEntryItem, CompressProgress};
use crate::inspector::{safe_atomic_rename, get_optimal_read_slice, get_optimal_write_buffer};
use crate::crypto::ChunkedAesWriter;
use super::{is_compression_cancelled, calculate_target_threads, calculate_zstd_level, get_io_buffers, is_gamer_preset};
use super::utils::{is_precompressed_file, is_path_excluded, is_excluded_or_temp_artifact};

pub const BAY_GAMER_SEEK_FRAME_SIZE: usize = 8 * 1024 * 1024; // 8 MB frames (.bayg)
pub const BAY_STD_SEEK_FRAME_SIZE: usize = 16 * 1024 * 1024;  // 16 MB frames (.bay)

/// Writes independent concatenated Zstd frames followed by an RFC 8878 Seekable Skippable Frame
/// (`0x184D2A5E`, footer `0x8F92EAB1`) whenever 2 or more frames are emitted.
pub struct SeekableZstdWriter<W: Write> {
    inner: Option<W>,
    compressor: zstd::bulk::Compressor<'static>,
    frame_target_bytes: usize,
    frame_buf: Vec<u8>,
    comp_buf: Vec<u8>,
    seek_entries: Vec<(u32, u32)>,
}

impl<W: Write> SeekableZstdWriter<W> {
    pub fn new(
        inner: W,
        level: i32,
        threads: u32,
        include_checksum: bool,
        frame_target_bytes: usize,
    ) -> std::io::Result<Self> {
        let mut compressor = zstd::bulk::Compressor::new(level)?;
        let _ = compressor.multithread(threads);
        let _ = compressor.include_checksum(include_checksum);
        let clamped = frame_target_bytes.clamp(64 * 1024, 32 * 1024 * 1024);

        Ok(Self {
            inner: Some(inner),
            compressor,
            frame_target_bytes: clamped,
            frame_buf: Vec::new(),
            comp_buf: Vec::new(),
            seek_entries: Vec::new(),
        })
    }

    fn flush_frame(&mut self) -> std::io::Result<()> {
        if self.frame_buf.is_empty() {
            return Ok(());
        }
        let decomp_len = u32::try_from(self.frame_buf.len())
            .map_err(|_| std::io::Error::other("Seekable Zstd frame uncompressed size exceeds u32"))?;
        let bound = zstd::zstd_safe::compress_bound(self.frame_buf.len());
        self.comp_buf.clear();
        self.comp_buf.reserve(bound);

        let written = self
            .compressor
            .compress_to_buffer(&self.frame_buf, &mut self.comp_buf)?;
        let comp_len = u32::try_from(written)
            .map_err(|_| std::io::Error::other("Seekable Zstd frame compressed size exceeds u32"))?;

        let inner = self.inner.as_mut().expect("seekable zstd inner present");
        inner.write_all(&self.comp_buf[..written])?;
        self.seek_entries.push((comp_len, decomp_len));
        self.frame_buf.zeroize();
        self.frame_buf.clear();
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<W> {
        self.flush_frame()?;
        let mut inner = self.inner.take().expect("seekable zstd inner present");
        if self.seek_entries.len() >= 2 {
            let num_frames = u32::try_from(self.seek_entries.len())
                .map_err(|_| std::io::Error::other("Seek table frame count exceeds u32"))?;
            let payload_u64 = (self.seek_entries.len() as u64)
                .checked_mul(8)
                .and_then(|n| n.checked_add(9))
                .ok_or_else(|| std::io::Error::other("Seek table size overflow"))?;
            let table_payload_size = u32::try_from(payload_u64)
                .map_err(|_| std::io::Error::other("Seek table size exceeds u32"))?;

            inner.write_all(
                &crate::inspector::bay::ZSTD_SEEKABLE_SKIPPABLE_MAGIC.to_le_bytes(),
            )?;
            inner.write_all(&table_payload_size.to_le_bytes())?;
            for &(comp_sz, decomp_sz) in &self.seek_entries {
                inner.write_all(&comp_sz.to_le_bytes())?;
                inner.write_all(&decomp_sz.to_le_bytes())?;
            }
            inner.write_all(&num_frames.to_le_bytes())?;
            inner.write_all(&[0u8])?;
            inner.write_all(
                &crate::inspector::bay::ZSTD_SEEKABLE_FOOTER_MAGIC.to_le_bytes(),
            )?;
        }
        inner.flush()?;
        self.frame_buf.zeroize();
        self.comp_buf.zeroize();
        Ok(inner)
    }
}

impl<W: Write> Drop for SeekableZstdWriter<W> {
    fn drop(&mut self) {
        self.frame_buf.zeroize();
        self.comp_buf.zeroize();
    }
}

impl<W: Write> Write for SeekableZstdWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut offset = 0usize;
        while offset < buf.len() {
            let avail = self.frame_target_bytes - self.frame_buf.len();
            let take = avail.min(buf.len() - offset);
            self.frame_buf.extend_from_slice(&buf[offset..offset + take]);
            offset += take;
            if self.frame_buf.len() >= self.frame_target_bytes {
                self.flush_frame()?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(ref mut inner) = self.inner {
            inner.flush()
        } else {
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
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
            Err(e) => return Err(format!("Failed reading '{}': {}", rel_name, e)),
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

    // Throttle final file progress emission to avoid flooding IPC during small-file archiving
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

    Ok(())
}

#[allow(clippy::too_many_arguments)]
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
    output_path: Option<&Path>,
    temp_path: Option<&Path>,
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
            if is_excluded_or_temp_artifact(&ep, output_path, temp_path) {
                continue;
            }
            let rel = ep.strip_prefix(base_parent).unwrap_or(&ep);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let clean_rel = rel_str.trim_start_matches('/');
            if is_path_excluded(clean_rel, excluded) {
                continue;
            }
            if let Ok(m) = std::fs::symlink_metadata(&ep) {
                if m.file_type().is_symlink() {
                    continue;
                }
                if m.is_file() {
                    write_file_to_bay_archive(&ep, clean_rel, encoder, read_buf, bytes_processed, total_original, app, last_emit, toc_entries)?;
                } else if m.is_dir() {
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
                    write_dir_to_bay_archive(&ep, base_parent, encoder, read_buf, bytes_processed, total_original, app, depth + 1, last_emit, toc_entries, excluded, output_path, temp_path)?;
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_bay_items<W: Write>(
    ordered_files: &[String],
    output_path: &Path,
    temp_path: &Path,
    encoder: &mut W,
    read_buf: &mut [u8],
    bytes_processed: &mut u64,
    total_original: u64,
    app: &AppHandle,
    last_emit: &mut Instant,
    toc_entries: &mut Vec<ArchiveEntryItem>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<(), String> {
    for file_str in ordered_files {
        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }
        let p = Path::new(file_str);
        if is_excluded_or_temp_artifact(p, Some(output_path), Some(temp_path)) {
            continue;
        }
        let parent_dir = p.parent().unwrap_or_else(|| Path::new("."));
        let rel = p.strip_prefix(parent_dir).unwrap_or(p);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let clean_rel = rel_str.trim_start_matches('/').to_string();
        if is_path_excluded(&clean_rel, excluded) {
            continue;
        }
        if let Ok(m) = std::fs::symlink_metadata(p) {
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_file() {
                let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                write_file_to_bay_archive(p, filename, encoder, read_buf, bytes_processed, total_original, app, last_emit, toc_entries)?;
            } else if m.is_dir() {
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
                write_dir_to_bay_archive(p, parent_dir, encoder, read_buf, bytes_processed, total_original, app, 0, last_emit, toc_entries, excluded, Some(output_path), Some(temp_path))?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::large_enum_variant)]
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

struct ZeroizingBuf(Vec<u8>);

impl Drop for ZeroizingBuf {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl BayOutputWriter {
    pub fn finish(
        self,
    ) -> std::io::Result<(BufWriter<File>, Option<crate::crypto::BayTocContext>)> {
        match self {
            BayOutputWriter::Plain(mut w) => {
                w.flush()?;
                Ok((w, None))
            }
            BayOutputWriter::Encrypted(w) => {
                let (inner, toc_ctx) = w.finish()?;
                Ok((inner, Some(toc_ctx)))
            }
        }
    }
}

pub fn write_bay_toc_trailer<W: Write>(
    file_writer: &mut W,
    toc_entries: &[ArchiveEntryItem],
    toc_ctx: Option<&crate::crypto::BayTocContext>,
) -> std::io::Result<()> {
    if toc_entries.is_empty() {
        return Ok(());
    }

    let mut toc_json = serde_json::to_vec(toc_entries).unwrap_or_default();
    if toc_json.is_empty() || (toc_json.len() as u64) > crate::inspector::bay::MAX_TOC_SIZE {
        toc_json.zeroize();
        return Ok(());
    }

    let res = (|| -> std::io::Result<()> {
        if let Some(ctx) = toc_ctx {
            let (mut encrypted_toc, toc_nonce) = ctx.encrypt_toc(&toc_json)?;
            let enc_toc_len = encrypted_toc.len() as u64;
            if enc_toc_len > crate::inspector::bay::MAX_TOC_SIZE {
                encrypted_toc.zeroize();
                return Ok(());
            }

            file_writer.write_all(&encrypted_toc)?;
            encrypted_toc.zeroize();
            file_writer.write_all(&enc_toc_len.to_le_bytes())?;
            file_writer.write_all(&toc_nonce)?;
            file_writer.write_all(crate::crypto::BAY_ETOC_V4_MAGIC)?;
            file_writer.flush()?;
        } else {
            let toc_len = toc_json.len() as u64;
            file_writer.write_all(&toc_json)?;
            file_writer.write_all(&toc_len.to_le_bytes())?;
            file_writer.write_all(crate::inspector::bay::BAY_TOC_MAGIC)?;
            file_writer.flush()?;
        }
        Ok(())
    })();

    toc_json.zeroize();
    res
}

#[allow(clippy::too_many_arguments)]
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

    if files.is_empty() {
        return Err("No files provided for compression".to_string());
    }

    let pwd_trimmed = password.map(|s| s.trim()).filter(|s| !s.is_empty());

    let temp_final = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.bay".into())
    ));

    let res = (|| -> Result<(), String> {
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

        // Allocate read buffer scaled by hardware allocation and zeroize on exit
        let mut read_buf = ZeroizingBuf(vec![0u8; read_buf_size]);

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
                s if s <= 0 => 1,
                s if s <= 25 => 4,
                s if s <= 50 => 7,
                s if s <= 75 => 9,
                _ => 11,
            };
            let mut encoder = brotli::CompressorWriter::new(out_sink, 64 * 1024, quality, 22);
            write_bay_items(
                &ordered_files,
                output_path,
                &temp_final,
                &mut encoder,
                &mut read_buf.0,
                &mut bytes_processed,
                total_original,
                app,
                &mut last_emit,
                &mut toc_entries,
                excluded,
            )?;
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            encoder.write_all(&[0u8]).map_err(|e| format!("EOF marker write error: {}", e))?;
            let _ = app.emit("compress_progress", CompressProgress {
                percent: 99.0,
                current_file: "Finalizing archive...".to_string(),
                bytes_processed,
                total_bytes: total_original,
            });
            encoder.flush().map_err(|e| format!("Brotli flush error: {}", e))?;
            let finished_writer = encoder.into_inner();
            let (mut file_writer, toc_ctx) = finished_writer.finish().map_err(|e| format!("Brotli finalize error: {}", e))?;
            write_bay_toc_trailer(&mut file_writer, &toc_entries, toc_ctx.as_ref())
                .map_err(|e| format!("Failed to write TOC: {}", e))?;
        } else if algo_lower.contains("lz4") {
            let mut out_sink = out_writer;
            out_sink.write_all(b"BAYLZ4\x02\x00")
                .map_err(|e| format!("Header write error: {}", e))?;
            let mut encoder = lz4_flex::frame::FrameEncoder::new(out_sink);
            write_bay_items(
                &ordered_files,
                output_path,
                &temp_final,
                &mut encoder,
                &mut read_buf.0,
                &mut bytes_processed,
                total_original,
                app,
                &mut last_emit,
                &mut toc_entries,
                excluded,
            )?;
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            encoder.write_all(&[0u8]).map_err(|e| format!("EOF marker write error: {}", e))?;
            let _ = app.emit("compress_progress", CompressProgress {
                percent: 99.0,
                current_file: "Finalizing archive...".to_string(),
                bytes_processed,
                total_bytes: total_original,
            });
            let finished_writer = encoder.finish().map_err(|e| format!("LZ4 finalize error: {}", e))?;
            let (mut file_writer, toc_ctx) = finished_writer.finish().map_err(|e| format!("LZ4 finalize error: {}", e))?;
            write_bay_toc_trailer(&mut file_writer, &toc_entries, toc_ctx.as_ref())
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

            if pwd_trimmed.is_none() {
                let frame_size = if is_gamer {
                    BAY_GAMER_SEEK_FRAME_SIZE
                } else {
                    BAY_STD_SEEK_FRAME_SIZE
                };
                let mut encoder = SeekableZstdWriter::new(
                    out_sink,
                    zstd_level,
                    threads as u32,
                    !is_gamer,
                    frame_size,
                )
                .map_err(|e| format!("Failed to initialize compression encoder: {}", e))?;
                write_bay_items(
                    &ordered_files,
                    output_path,
                    &temp_final,
                    &mut encoder,
                    &mut read_buf.0,
                    &mut bytes_processed,
                    total_original,
                    app,
                    &mut last_emit,
                    &mut toc_entries,
                    excluded,
                )?;
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                encoder.write_all(&[0u8]).map_err(|e| format!("EOF marker write error: {}", e))?;
                let _ = app.emit("compress_progress", CompressProgress {
                    percent: 99.0,
                    current_file: "Finalizing archive...".to_string(),
                    bytes_processed,
                    total_bytes: total_original,
                });
                let finished_writer = encoder.finish().map_err(|e| format!("Failed to finalize compression: {}", e))?;
                let (mut file_writer, toc_ctx) = finished_writer.finish().map_err(|e| format!("Failed to finalize output stream: {}", e))?;
                write_bay_toc_trailer(&mut file_writer, &toc_entries, toc_ctx.as_ref())
                    .map_err(|e| format!("Failed to write TOC: {}", e))?;
            } else {
                let mut encoder = zstd::stream::Encoder::new(out_sink, zstd_level)
                    .map_err(|e| format!("Failed to initialize compression encoder: {}", e))?;
                let _ = encoder.multithread(threads as u32);
                if is_gamer {
                    let _ = encoder.include_checksum(false);
                }
                write_bay_items(
                    &ordered_files,
                    output_path,
                    &temp_final,
                    &mut encoder,
                    &mut read_buf.0,
                    &mut bytes_processed,
                    total_original,
                    app,
                    &mut last_emit,
                    &mut toc_entries,
                    excluded,
                )?;
                if is_compression_cancelled() {
                    return Err("Compression cancelled by user".to_string());
                }
                encoder.write_all(&[0u8]).map_err(|e| format!("EOF marker write error: {}", e))?;
                let _ = app.emit("compress_progress", CompressProgress {
                    percent: 99.0,
                    current_file: "Finalizing archive...".to_string(),
                    bytes_processed,
                    total_bytes: total_original,
                });
                let finished_writer = encoder.finish().map_err(|e| format!("Failed to finalize compression: {}", e))?;
                let (mut file_writer, toc_ctx) = finished_writer.finish().map_err(|e| format!("Failed to finalize output stream: {}", e))?;
                write_bay_toc_trailer(&mut file_writer, &toc_entries, toc_ctx.as_ref())
                    .map_err(|e| format!("Failed to write TOC: {}", e))?;
            }
        }

        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
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
    let temp_output = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.bay".into())
    ));

    let res = (|| -> Result<(), String> {
        let mut toc_entries: Vec<ArchiveEntryItem> = Vec::new();

        let (read_buf_size, mut write_buf_size) = get_io_buffers(50);
        write_buf_size = get_optimal_write_buffer(&temp_output, write_buf_size);
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create intermediate file: {}", e))?;
        let mut buf_out = BufWriter::with_capacity(write_buf_size, out_file);
        buf_out.write_all(b"BAYZIP\x02\x00")
            .map_err(|e| format!("Header write error: {}", e))?;
        
        // Balanced profile (50% slider equivalent)
        let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let threads = calculate_target_threads(50, total_cores);
        let zstd_level = calculate_zstd_level(50, is_gamer);
        let frame_size = if is_gamer {
            BAY_GAMER_SEEK_FRAME_SIZE
        } else {
            BAY_STD_SEEK_FRAME_SIZE
        };
        let mut encoder = SeekableZstdWriter::new(
            buf_out,
            zstd_level,
            threads as u32,
            !is_gamer,
            frame_size,
        )
        .map_err(|e| format!("Encoder error: {}", e))?;

        let mut read_buf = vec![0u8; read_buf_size];

        #[allow(clippy::too_many_arguments)]
        fn add_dir<E: Write>(
            dir: &Path,
            base_parent: &Path,
            encoder: &mut E,
            read_buf: &mut [u8],
            depth: usize,
            toc_entries: &mut Vec<ArchiveEntryItem>,
            output_path: &Path,
            temp_path: &Path,
        ) -> Result<(), String> {
            if depth > 32 {
                return Ok(());
            }
            if is_compression_cancelled() {
                return Err("Compression cancelled by user".to_string());
            }
            if let Ok(read_dir) = std::fs::read_dir(dir) {
                let mut entries: Vec<_> = read_dir.flatten().collect();
                entries.sort_by_key(|e| is_precompressed_file(&e.path()));
                for entry in entries {
                    if is_compression_cancelled() {
                        return Err("Compression cancelled by user".to_string());
                    }
                    let entry_path = entry.path();
                    if is_excluded_or_temp_artifact(&entry_path, Some(output_path), Some(temp_path)) {
                        continue;
                    }
                    if let Ok(m) = std::fs::symlink_metadata(&entry_path) {
                        if m.file_type().is_symlink() {
                            continue;
                        }
                        if m.is_file() {
                            let mut f = File::open(&entry_path).map_err(|e| e.to_string())?;
                            let rel_name = entry_path.strip_prefix(base_parent).unwrap_or(&entry_path);
                            let rel_str = rel_name.to_string_lossy().replace('\\', "/");
                            let clean_rel = rel_str.trim_start_matches('/').to_string();
                            let bytes = clean_rel.as_bytes();
                            let file_size = f.metadata().map(|m| m.len()).unwrap_or(m.len());
                            encoder.write_all(&[1u8]).map_err(|e| e.to_string())?;
                            encoder.write_all(&(bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
                            encoder.write_all(bytes).map_err(|e| e.to_string())?;
                            encoder.write_all(&file_size.to_le_bytes()).map_err(|e| e.to_string())?;
                            toc_entries.push(ArchiveEntryItem {
                                name: clean_rel.clone(),
                                size: file_size,
                                is_dir: false,
                            });

                            let slice_size = get_optimal_read_slice(&entry_path);
                            let mut written: u64 = 0;
                            while written < file_size {
                                if is_compression_cancelled() {
                                    return Err("Compression cancelled by user".to_string());
                                }
                                let to_read = ((file_size - written) as usize).min(read_buf.len()).min(slice_size);
                                match f.read(&mut read_buf[..to_read]) {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        encoder.write_all(&read_buf[..n]).map_err(|e| e.to_string())?;
                                        written += n as u64;
                                    }
                                    Err(e) => return Err(format!("Failed reading '{}': {}", clean_rel, e)),
                                }
                            }
                            if written < file_size {
                                let zeros = [0u8; 4096];
                                let mut rem = file_size - written;
                                while rem > 0 {
                                    let to_write = (rem as usize).min(zeros.len());
                                    encoder.write_all(&zeros[..to_write]).map_err(|e| e.to_string())?;
                                    rem -= to_write as u64;
                                }
                            }
                        } else if m.is_dir() {
                            let rel_name = entry_path.strip_prefix(base_parent).unwrap_or(&entry_path);
                            let rel_str = rel_name.to_string_lossy().replace('\\', "/");
                            let clean_rel = rel_str.trim_start_matches('/').to_string();
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
                            add_dir(&entry_path, base_parent, encoder, read_buf, depth + 1, toc_entries, output_path, temp_path)?;
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
            if is_excluded_or_temp_artifact(p, Some(output_path), Some(&temp_output)) {
                continue;
            }
            let parent = p.parent().unwrap_or(p);
            if let Ok(m) = std::fs::symlink_metadata(p) {
                if m.file_type().is_symlink() {
                    continue;
                }
                if m.is_file() {
                    let mut f = File::open(p).map_err(|e| e.to_string())?;
                    let file_meta = f.metadata().ok();
                    let file_size = file_meta.map(|m| m.len()).unwrap_or(0);
                    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                    let filename_bytes = filename.as_bytes();

                    encoder.write_all(&[1u8]).map_err(|e| e.to_string())?; // 1 = file entry
                    encoder.write_all(&(filename_bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
                    encoder.write_all(filename_bytes).map_err(|e| e.to_string())?;
                    encoder.write_all(&file_size.to_le_bytes()).map_err(|e| e.to_string())?;
                    toc_entries.push(ArchiveEntryItem {
                        name: filename.to_string(),
                        size: file_size,
                        is_dir: false,
                    });

                    let slice_size = get_optimal_read_slice(p);
                    let mut written: u64 = 0;
                    while written < file_size {
                        if is_compression_cancelled() {
                            return Err("Compression cancelled by user".to_string());
                        }
                        let to_read = ((file_size - written) as usize).min(read_buf.len()).min(slice_size);
                        match f.read(&mut read_buf[..to_read]) {
                            Ok(0) => break,
                            Ok(n) => {
                                encoder.write_all(&read_buf[..n]).map_err(|e| e.to_string())?;
                                written += n as u64;
                            }
                            Err(e) => return Err(format!("Failed reading '{}': {}", filename, e)),
                        }
                    }
                    if written < file_size {
                        let zeros = [0u8; 4096];
                        let mut rem = file_size - written;
                        while rem > 0 {
                            let to_write = (rem as usize).min(zeros.len());
                            encoder.write_all(&zeros[..to_write]).map_err(|e| e.to_string())?;
                            rem -= to_write as u64;
                        }
                    }
                } else if m.is_dir() {
                    let rel_name = p.strip_prefix(parent).unwrap_or(p);
                    let rel_str = rel_name.to_string_lossy().replace('\\', "/");
                    let clean_rel = rel_str.trim_start_matches('/').to_string();
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
                    add_dir(p, parent, &mut encoder, &mut read_buf, 0, &mut toc_entries, output_path, &temp_output)?;
                }
            }
        }

        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }

        encoder.write_all(&[0u8]).map_err(|e| e.to_string())?; // 0 = EOF
        let mut finished_writer = encoder.finish().map_err(|e| format!("Finalize error: {}", e))?;
        write_bay_toc_trailer(&mut finished_writer, &toc_entries, None)
            .map_err(|e| format!("Failed to write TOC: {}", e))?;
        drop(finished_writer);

        if is_compression_cancelled() {
            return Err("Compression cancelled by user".to_string());
        }

        safe_atomic_rename(&temp_output, output_path)
            .map_err(|e| format!("Failed to rename temporary file to destination: {}", e))?;

        Ok(())
    })();

    if res.is_err() && temp_output.exists() {
        let _ = std::fs::remove_file(&temp_output);
    }

    res
}

use std::io::{Read, Seek};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use std::sync::atomic::{AtomicBool, Ordering};

pub mod utils;
pub mod bay;
pub mod zip;
pub mod tar;
pub mod sevenz;

pub use utils::*;
pub use bay::*;
pub use zip::*;
pub use tar::*;
pub use sevenz::*;

use crate::types::{CompressPayload, CompressResponse, CompressProgress};
use crate::priority::ScopedPriorityGuard;
use crate::inspector::archive_stem_name;

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
    slice_size: usize,
}

impl<'a, R: Read> ProgressReader<'a, R> {
    pub fn new(
        inner: R,
        bytes_processed: &'a mut u64,
        total_original: u64,
        app: &'a AppHandle,
        last_emit: &'a mut Instant,
        current_file: &'a str,
        slice_size: usize,
    ) -> Self {
        Self {
            inner,
            bytes_processed,
            total_original,
            app,
            last_emit,
            current_file,
            slice_size: if slice_size == 0 { 2 * 1024 * 1024 } else { slice_size },
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
        let to_read = buf.len().min(self.slice_size);
        let n = self.inner.read(&mut buf[..to_read])?;
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
/// For standard .bay: 0 -> Level 1, 25 -> Level 3, 50 -> Level 6, 75 -> Level 9, 100 -> Level 19.
pub fn calculate_zstd_level(slider_value: i32, is_gamer: bool) -> i32 {
    if is_gamer {
        match slider_value {
            s if s <= 25 => -2,
            s if s <= 50 => 1,
            _ => 3,
        }
    } else {
        match slider_value {
            s if s <= 0 => 1,
            s if s <= 25 => 3,
            s if s <= 50 => 6,
            s if s <= 75 => 9,
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
        "tar" => headless_quick_compress_tar(p, &output_path)?,
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
    
    let excluded_set: Option<std::collections::HashSet<String>> = payload.excluded_paths.as_ref().map(|paths| {
        paths.iter().map(|p| p.replace('\\', "/").trim_start_matches('/').to_lowercase()).collect()
    });
    let excluded_ref = excluded_set.as_ref();

    // Calculate total original size across files & folders (accounting for excluded paths)
    let mut total_original: u64 = 0;
    for file_str in &payload.files {
        let p = Path::new(file_str);
        let parent = p.parent().unwrap_or(p);
        let (s, _) = compute_path_size_filtered(p, parent, excluded_ref);
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
    let smart_bypass = payload.smart_bypass.unwrap_or(true);
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
                smart_bypass,
                excluded_ref,
            )
        },
        "tar" => {
            compress_tar(&payload.files, &output_path, &app, total_original, excluded_ref)
        },
        "7z" => {
            compress_7z(&payload.files, &output_path, payload.slider_value, pwd_ref, &app, total_original, excluded_ref)
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
                excluded_ref,
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
                excluded_ref,
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

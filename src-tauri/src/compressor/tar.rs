use std::fs::File;
use std::io::{Read, Write, BufWriter};
use std::path::Path;
use std::time::Instant;
use tauri::AppHandle;

use crate::inspector::{safe_atomic_rename, get_optimal_read_slice, get_optimal_write_buffer};
use super::{is_compression_cancelled, ProgressReader};
use super::utils::{is_path_excluded, is_excluded_or_temp_artifact};

pub fn compress_tar(
    files: &[String],
    output_path: &Path,
    app: &AppHandle,
    total_original: u64,
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
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.tar".into())
    ));

    let res = (|| -> Result<(), String> {
        let write_buf_size = get_optimal_write_buffer(&temp_output, 1024 * 1024);
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create tar file: {}", e))?;
        let mut builder = ::tar::Builder::new(BufWriter::with_capacity(write_buf_size, out_file));

        #[allow(clippy::too_many_arguments)]
        fn add_dir_to_tar<W: Write>(
            dir: &Path,
            base: &Path,
            builder: &mut ::tar::Builder<W>,
            bytes_processed: &mut u64,
            total_original: u64,
            app: &AppHandle,
            last_emit: &mut Instant,
            depth: usize,
            excluded: Option<&std::collections::HashSet<String>>,
            output_path: &Path,
            temp_path: &Path,
        ) -> Result<(), String> {
            if depth > 32 { return Ok(()); }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if is_compression_cancelled() {
                        return Err("Compression cancelled by user".to_string());
                    }
                    let path = entry.path();
                    if is_excluded_or_temp_artifact(&path, Some(output_path), Some(temp_path)) {
                        continue;
                    }
                    if let Ok(m) = std::fs::symlink_metadata(&path) {
                        if m.file_type().is_symlink() {
                            continue;
                        }
                        let rel = path.strip_prefix(base).unwrap_or(&path);
                        let clean = rel.to_string_lossy().replace('\\', "/");
                        let trimmed = clean.trim_start_matches('/');
                        if trimmed.is_empty() || is_path_excluded(trimmed, excluded) { continue; }

                        if m.is_dir() {
                            let _ = builder.append_dir(trimmed, &path);
                            add_dir_to_tar(&path, base, builder, bytes_processed, total_original, app, last_emit, depth + 1, excluded, output_path, temp_path)?;
                        } else if m.is_file() {
                            if let Ok(f) = File::open(&path) {
                                let file_meta = f.metadata().ok();
                                let sz = file_meta.as_ref().map(|fm| fm.len()).unwrap_or(m.len());
                                let mtime = file_meta
                                    .as_ref()
                                    .and_then(|fm| fm.modified().ok())
                                    .or_else(|| m.modified().ok())
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0);
                                let mut header = ::tar::Header::new_gnu();
                                header.set_size(sz);
                                header.set_mode(0o644);
                                header.set_mtime(mtime);
                                header.set_cksum();
                                let slice_size = get_optimal_read_slice(&path);
                                let mut prog_reader = ProgressReader::new(f.take(sz), bytes_processed, total_original, app, last_emit, trimmed, slice_size);
                                builder.append_data(&mut header, trimmed, &mut prog_reader)
                                    .map_err(|e| format!("Tar append file error: {}", e))?;
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
            if is_excluded_or_temp_artifact(p, Some(output_path), Some(&temp_output)) {
                continue;
            }
            let Ok(root_meta) = std::fs::symlink_metadata(p) else {
                continue;
            };
            if root_meta.file_type().is_symlink() {
                continue;
            }
            let parent = p.parent().unwrap_or(p);
            let rel = p.strip_prefix(parent).unwrap_or(p);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let clean_rel = rel_str.trim_start_matches('/').trim_end_matches('/');
            if is_path_excluded(clean_rel, excluded) {
                continue;
            }
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            if root_meta.is_file() {
                if let Ok(f) = File::open(p) {
                    let file_meta = f.metadata().ok();
                    let sz = file_meta.as_ref().map(|fm| fm.len()).unwrap_or(root_meta.len());
                    let mtime = file_meta
                        .as_ref()
                        .and_then(|fm| fm.modified().ok())
                        .or_else(|| root_meta.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let mut header = ::tar::Header::new_gnu();
                    header.set_size(sz);
                    header.set_mode(0o644);
                    header.set_mtime(mtime);
                    header.set_cksum();
                    let slice_size = get_optimal_read_slice(p);
                    let mut prog_reader = ProgressReader::new(f.take(sz), &mut bytes_processed, total_original, app, &mut last_emit, name, slice_size);
                    builder.append_data(&mut header, name, &mut prog_reader)
                        .map_err(|e| format!("Tar append file error: {}", e))?;
                }
            } else if root_meta.is_dir() {
                let _ = builder.append_dir(name, p);
                add_dir_to_tar(p, parent, &mut builder, &mut bytes_processed, total_original, app, &mut last_emit, 0, excluded, output_path, &temp_output)?;
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

pub fn headless_quick_compress_tar(p: &Path, output_path: &Path) -> Result<(), String> {
    headless_quick_compress_tar_items(&[p], output_path)
}

pub fn headless_quick_compress_tar_items(paths: &[&Path], output_path: &Path) -> Result<(), String> {
    let temp_output = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.tar".into())
    ));

    let res = (|| -> Result<(), String> {
        let write_buf_size = get_optimal_write_buffer(&temp_output, 1024 * 1024);
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create temporary tar file: {}", e))?;
        let mut builder = ::tar::Builder::new(BufWriter::with_capacity(write_buf_size, out_file));

        fn add_dir_headless<W: Write>(
            dir: &Path,
            base: &Path,
            builder: &mut ::tar::Builder<W>,
            depth: usize,
            output_path: &Path,
            temp_path: &Path,
        ) -> Result<(), String> {
            if depth > 32 || is_compression_cancelled() { return Ok(()); }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if is_compression_cancelled() {
                        return Err("Compression cancelled by user".to_string());
                    }
                    let path = entry.path();
                    if is_excluded_or_temp_artifact(&path, Some(output_path), Some(temp_path)) {
                        continue;
                    }
                    if let Ok(m) = std::fs::symlink_metadata(&path) {
                        if m.file_type().is_symlink() {
                            continue;
                        }
                        let rel = path.strip_prefix(base).unwrap_or(&path);
                        let clean = rel.to_string_lossy().replace('\\', "/");
                        let trimmed = clean.trim_start_matches('/');
                        if trimmed.is_empty() { continue; }

                        if m.is_dir() {
                            let _ = builder.append_dir(trimmed, &path);
                            add_dir_headless(&path, base, builder, depth + 1, output_path, temp_path)?;
                        } else if m.is_file() {
                            builder.append_path_with_name(&path, trimmed)
                                .map_err(|e| format!("Tar append file error: {}", e))?;
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
            let Ok(root_meta) = std::fs::symlink_metadata(p) else {
                continue;
            };
            if root_meta.file_type().is_symlink() {
                continue;
            }
            let parent = p.parent().unwrap_or(p);
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            if root_meta.is_file() {
                builder.append_path_with_name(p, name)
                    .map_err(|e| format!("Tar append file error: {}", e))?;
            } else if root_meta.is_dir() {
                let rel_name = p.strip_prefix(parent).unwrap_or(p);
                let clean = rel_name.to_string_lossy().replace('\\', "/");
                let trimmed = clean.trim_start_matches('/');
                if !trimmed.is_empty() {
                    let _ = builder.append_dir(trimmed, p);
                }
                add_dir_headless(p, parent, &mut builder, 0, output_path, &temp_output)?;
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

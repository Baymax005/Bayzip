use std::fs::File;
use std::io::{Write, Seek, BufWriter};
use std::path::Path;
use std::time::Instant;
use tauri::AppHandle;
use sevenz_rust2::{ArchiveWriter, ArchiveEntry, EncoderConfiguration};

use crate::inspector::{safe_atomic_rename, get_optimal_read_slice, get_optimal_write_buffer};
use super::{is_compression_cancelled, calculate_target_threads, ProgressReader};
use super::utils::{is_precompressed_file, is_path_excluded, is_excluded_or_temp_artifact};

pub fn compress_7z(
    files: &[String],
    output_path: &Path,
    slider_value: i32,
    password: Option<&str>,
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
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.7z".into())
    ));

    let res = (|| -> Result<(), String> {
        let write_buf_size = get_optimal_write_buffer(&temp_output, 1024 * 1024);
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create 7z archive: {}", e))?;
        let buf_writer = BufWriter::with_capacity(write_buf_size, out_file);
        let mut sz = ArchiveWriter::new(buf_writer)
            .map_err(|e| format!("Failed to create 7z writer: {}", e))?;

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
            s if s <= 25 => 2,
            s if s <= 50 => 3,
            s if s <= 75 => 5,
            _ => 9,
        };

        let lzma2_opt = if threads > 1 {
            sevenz_rust2::encoder_options::Lzma2Options::from_level_mt(lzma_level, threads, 4 * 1024 * 1024)
        } else {
            sevenz_rust2::encoder_options::Lzma2Options::from_level(lzma_level)
        };
        methods.push(lzma2_opt.into());
        sz.set_content_methods(methods);

        let mut bytes_processed: u64 = 0;
        let mut last_emit = Instant::now();

        // Solid archiving heuristic: group compressible text/code/binaries together first,
        // placing high-entropy pre-compressed media at the end.
        let mut ordered_files = files.to_vec();
        ordered_files.sort_by_key(|f| is_precompressed_file(Path::new(f)));

        for file_str in &ordered_files {
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
            let rel_root = p.strip_prefix(parent).unwrap_or(p).to_string_lossy().replace('\\', "/");
            let clean_root = rel_root.trim_start_matches('/').trim_end_matches('/');
            if is_path_excluded(clean_root, excluded) {
                continue;
            }

            if root_meta.is_file() {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                let mut entry = ArchiveEntry::from_path(p, name.to_string());
                entry.is_directory = false;
                entry.has_stream = true;
                let f = File::open(p).map_err(|e| format!("Failed to open file {:?}: {}", p, e))?;
                let slice_size = get_optimal_read_slice(p);
                let prog_reader = ProgressReader::new(f, &mut bytes_processed, total_original, app, &mut last_emit, name, slice_size);
                sz.push_archive_entry(entry, Some(prog_reader))
                    .map_err(|e| format!("7z entry error: {}", e))?;
            } else if root_meta.is_dir() {
                if !clean_root.is_empty() {
                    let mut dir_entry = ArchiveEntry::from_path(p, format!("{}/", clean_root));
                    dir_entry.is_directory = true;
                    dir_entry.has_stream = false;
                    let _ = sz.push_archive_entry::<&[u8]>(dir_entry, None);
                }

                #[allow(clippy::too_many_arguments)]
                fn add_dir_7z<W: Write + Seek>(
                    dir: &Path,
                    base: &Path,
                    sz: &mut ArchiveWriter<W>,
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
                    if let Ok(read_dir) = std::fs::read_dir(dir) {
                        let mut entries: Vec<_> = read_dir.flatten().collect();
                        entries.sort_by_key(|e| is_precompressed_file(&e.path()));
                        for entry in entries {
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
                                    let mut d_entry = ArchiveEntry::from_path(&path, format!("{}/", trimmed.trim_end_matches('/')));
                                    d_entry.is_directory = true;
                                    d_entry.has_stream = false;
                                    let _ = sz.push_archive_entry::<&[u8]>(d_entry, None);
                                    add_dir_7z(&path, base, sz, bytes_processed, total_original, app, last_emit, depth + 1, excluded, output_path, temp_path)?;
                                } else if m.is_file() {
                                    let mut f_entry = ArchiveEntry::from_path(&path, trimmed.to_string());
                                    f_entry.is_directory = false;
                                    f_entry.has_stream = true;
                                    let f = File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;
                                    let slice_size = get_optimal_read_slice(&path);
                                    let prog_reader = ProgressReader::new(f, bytes_processed, total_original, app, last_emit, trimmed, slice_size);
                                    sz.push_archive_entry(f_entry, Some(prog_reader))
                                        .map_err(|e| format!("7z entry error: {}", e))?;
                                }
                            }
                        }
                    }
                    Ok(())
                }
                add_dir_7z(p, parent, &mut sz, &mut bytes_processed, total_original, app, &mut last_emit, 0, excluded, output_path, &temp_output)?;
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

pub fn headless_quick_compress_7z(p: &Path, output_path: &Path) -> Result<(), String> {
    headless_quick_compress_7z_items(&[p], output_path)
}

pub fn headless_quick_compress_7z_items(paths: &[&Path], output_path: &Path) -> Result<(), String> {
    let temp_output = output_path.with_file_name(format!(
        "{}.baytmp",
        output_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "archive.7z".into())
    ));

    let res = (|| -> Result<(), String> {
        let write_buf_size = get_optimal_write_buffer(&temp_output, 1024 * 1024);
        let out_file = File::create(&temp_output)
            .map_err(|e| format!("Failed to create 7z: {}", e))?;
        let buf_writer = BufWriter::with_capacity(write_buf_size, out_file);
        let mut sz = ArchiveWriter::new(buf_writer)
            .map_err(|e| format!("Failed to create 7z: {}", e))?;
        sz.set_encrypt_header(false);

        let total_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let threads = calculate_target_threads(50, total_cores) as u32;
        let lzma2_opt = if threads > 1 {
            sevenz_rust2::encoder_options::Lzma2Options::from_level_mt(3, threads, 4 * 1024 * 1024)
        } else {
            sevenz_rust2::encoder_options::Lzma2Options::from_level(3)
        };
        sz.set_content_methods(vec![lzma2_opt.into()]);

        fn add_dir_7z_quick<W: Write + Seek>(
            dir: &Path,
            base: &Path,
            sz: &mut ArchiveWriter<W>,
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
                            let mut d_entry = ArchiveEntry::from_path(&path, format!("{}/", trimmed.trim_end_matches('/')));
                            d_entry.is_directory = true;
                            d_entry.has_stream = false;
                            let _ = sz.push_archive_entry::<&[u8]>(d_entry, None);
                            add_dir_7z_quick(&path, base, sz, depth + 1, output_path, temp_path)?;
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
            if root_meta.is_file() {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                let mut entry = ArchiveEntry::from_path(p, name.to_string());
                entry.is_directory = false;
                entry.has_stream = true;
                let f = File::open(p).map_err(|e| format!("Failed to open file: {}", e))?;
                sz.push_archive_entry(entry, Some(f))
                    .map_err(|e| format!("7z entry error: {}", e))?;
            } else if root_meta.is_dir() {
                let rel_root = p.strip_prefix(parent).unwrap_or(p).to_string_lossy().replace('\\', "/");
                let clean_root = rel_root.trim_start_matches('/').trim_end_matches('/');
                if !clean_root.is_empty() {
                    let mut dir_entry = ArchiveEntry::from_path(p, format!("{}/", clean_root));
                    dir_entry.is_directory = true;
                    dir_entry.has_stream = false;
                    let _ = sz.push_archive_entry::<&[u8]>(dir_entry, None);
                }
                add_dir_7z_quick(p, parent, &mut sz, 0, output_path, &temp_output)?;
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

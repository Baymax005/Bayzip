use sysinfo::System;
use serde::Serialize;
use std::process::Command;
use std::fs::File;
use std::io::{Read, Write, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use zip::write::SimpleFileOptions;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Serialize)]
struct SystemSpecs {
    total_ram_gb: f64,
    free_ram_gb: f64,
    cpu_cores: usize,
}

#[tauri::command]
fn get_system_specs() -> SystemSpecs {
    // Ultra-fast query: only refresh memory, do not scan every process on the OS
    let mut sys = System::new();
    sys.refresh_memory();
    
    let total_ram_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let free_ram_gb = sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    
    SystemSpecs {
        total_ram_gb,
        free_ram_gb,
        cpu_cores,
    }
}

#[tauri::command]
fn get_cli_args() -> Vec<String> {
    std::env::args().skip(1).filter(|arg| !arg.starts_with("--")).collect()
}

fn build_target_reg(target: &str, exe: &str) -> String {
    format!(
        "[-HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\Bayzip]\r\n\r\n\
        [-HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract]\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract]\r\n\
        \"MUIVerb\"=\"Extract with Bayzip\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\
        \"SubCommands\"=\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell]\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\01_extract_sub]\r\n\
        \"MUIVerb\"=\"Extract to dedicated folder\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\01_extract_sub\\command]\r\n\
        @=\"\\\"{exe}\\\" --extract-to-folder \\\"%1\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\02_extract_here]\r\n\
        \"MUIVerb\"=\"Extract Here\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\02_extract_here\\command]\r\n\
        @=\"\\\"{exe}\\\" --extract-here \\\"%1\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\03_open]\r\n\
        \"MUIVerb\"=\"Open in Bayzip...\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\03_open\\command]\r\n\
        @=\"\\\"{exe}\\\" \\\"%1\\\"\"\r\n\r\n\
        [-HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress]\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress]\r\n\
        \"MUIVerb\"=\"Add to archive with Bayzip\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\
        \"SubCommands\"=\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell]\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\01_bay]\r\n\
        \"MUIVerb\"=\"Add to .bay (Ultra-Fast)\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\01_bay\\command]\r\n\
        @=\"\\\"{exe}\\\" --quick-compress .bay \\\"%1\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\02_zip]\r\n\
        \"MUIVerb\"=\"Add to .zip (Standard)\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\02_zip\\command]\r\n\
        @=\"\\\"{exe}\\\" --quick-compress .zip \\\"%1\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\03_7z]\r\n\
        \"MUIVerb\"=\"Add to .7z (High Ratio)\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\03_7z\\command]\r\n\
        @=\"\\\"{exe}\\\" --quick-compress .7z \\\"%1\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\04_tar]\r\n\
        \"MUIVerb\"=\"Add to .tar (Universal)\"\r\n\
        \"Icon\"=\"\\\"{exe}\\\"\"\r\n\r\n\
        [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\04_tar\\command]\r\n\
        @=\"\\\"{exe}\\\" --quick-compress .tar \\\"%1\\\"\"\r\n\r\n"
    )
}

#[tauri::command]
async fn register_context_menu() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            if let Ok(exe_path) = std::env::current_exe() {
                let exe_str = exe_path.to_string_lossy().to_string();
                let escaped_exe = exe_str.replace('\\', "\\\\").replace('"', "\\\"");

                let reg_content = format!(
                    "Windows Registry Editor Version 5.00\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\.bay]\r\n\
                    @=\"Bayzip.Archive\"\r\n\
                    \"Content Type\"=\"application/x-bayzip\"\r\n\
                    \"PerceivedType\"=\"compressed\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\.bay\\OpenWithProgids]\r\n\
                    \"Bayzip.Archive\"=\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive]\r\n\
                    @=\"Bayzip Compressed Archive\"\r\n\
                    \"FriendlyTypeName\"=\"Bayzip Compressed Archive\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\DefaultIcon]\r\n\
                    @=\"\\\"{escaped_exe}\\\",0\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell]\r\n\
                    @=\"open\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\open]\r\n\
                    @=\"Open in Bayzip\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\open\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\extract_sub]\r\n\
                    @=\"Extract to dedicated folder\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\extract_sub\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --extract-to-folder \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\extract_here]\r\n\
                    @=\"Extract Here\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\extract_here\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --extract-here \\\"%1\\\"\"\r\n\r\n\
                    {}{}{}{}{}{}{}{}",
                    build_target_reg("*", &escaped_exe),
                    build_target_reg("Directory", &escaped_exe),
                    build_target_reg("SystemFileAssociations\\.bay", &escaped_exe),
                    build_target_reg("SystemFileAssociations\\.zip", &escaped_exe),
                    build_target_reg("SystemFileAssociations\\.7z", &escaped_exe),
                    build_target_reg("SystemFileAssociations\\.rar", &escaped_exe),
                    build_target_reg("SystemFileAssociations\\.tar", &escaped_exe),
                    build_target_reg("SystemFileAssociations\\.gz", &escaped_exe)
                );

                let temp_reg = std::env::temp_dir().join("bayzip_context.reg");
                if std::fs::write(&temp_reg, reg_content).is_ok() {
                    let mut cmd = Command::new("reg");
                    cmd.args(["import", &temp_reg.to_string_lossy()]);
                    cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = cmd.output();
                    let _ = std::fs::remove_file(temp_reg);

                    // Notify Windows Explorer to reload file associations and icons immediately
                    let mut refresh_cmd = Command::new("cmd");
                    refresh_cmd.args(["/C", "ie4uinit.exe -show"]);
                    refresh_cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = refresh_cmd.output();

                    return Ok("Separate 'Extract with Bayzip' and 'Add to archive with Bayzip' menus registered successfully!".into());
                }
            }
            Err("Could not determine executable path.".into())
        })
        .await
        .map_err(|e| format!("Registry sync error: {}", e))?
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("Context menu not applicable on non-Windows OS".into())
    }
}

pub fn headless_quick_compress(format: &str, target_path: &str) -> Result<(), String> {
    let p = Path::new(target_path);
    if !p.exists() {
        return Err("Target path does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("archive");
    let ext = if format.starts_with('.') { format.to_string() } else { format!(".{}", format) };
    let output_path = parent.join(format!("{}{}", stem, ext));

    let fmt_clean = format.trim_start_matches('.').to_lowercase();
    match fmt_clean.as_str() {
        "zip" => headless_quick_compress_zip(p, &output_path),
        "tar" => {
            let out_file = File::create(&output_path)
                .map_err(|e| format!("Failed to create tar file: {}", e))?;
            let mut builder = tar::Builder::new(BufWriter::new(out_file));
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            if p.is_file() {
                builder.append_path_with_name(p, name).map_err(|e| e.to_string())?;
            } else if p.is_dir() {
                builder.append_dir_all(name, p).map_err(|e| e.to_string())?;
            }
            builder.finish().map_err(|e| e.to_string())?;
            Ok(())
        },
        "7z" => {
            sevenz_rust::compress_to_path(p, &output_path)
                .map_err(|e| format!("7z compress error: {}", e))
        },
        _ => headless_quick_compress_bay(p, &output_path),
    }
}

fn headless_quick_compress_zip(p: &Path, output_path: &Path) -> Result<(), String> {
    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create zip file: {}", e))?;
    let mut writer = zip::ZipWriter::new(out_file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);

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
            options: SimpleFileOptions,
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
                            let _ = writer.add_directory(format!("{}/", rel_str), options);
                            add_dir(&path, base, writer, options, depth + 1)?;
                        } else if m.is_file() {
                            writer.start_file(&rel_str, options)
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
        add_dir(p, parent, &mut writer, options, 0)?;
    }

    writer.finish().map_err(|e| format!("Failed to finalize zip: {}", e))?;
    Ok(())
}

fn headless_quick_compress_bay(p: &Path, output_path: &Path) -> Result<(), String> {
    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    
    // Balanced profile level 7 (50% slider equivalent)
    let mut encoder = zstd::stream::Encoder::new(out_file, 7)
        .map_err(|e| format!("Encoder error: {}", e))?;

    let _ = encoder.write_all(b"BAYZIP\x02");

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
                            let rel_str = rel_name.to_string_lossy();
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

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct SelectedItemInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

fn compute_path_size(p: &Path) -> (u64, bool) {
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

#[tauri::command]
fn get_selected_items_info(paths: Vec<String>) -> Vec<SelectedItemInfo> {
    paths
        .into_iter()
        .map(|path_str| {
            let p = Path::new(&path_str);
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&path_str)
                .to_string();
            let (size, is_dir) = compute_path_size(p);
            SelectedItemInfo {
                path: path_str,
                name,
                size,
                is_dir,
            }
        })
        .collect()
}

#[derive(serde::Deserialize)]
pub struct CompressPayload {
    files: Vec<String>,
    format: String,
    algorithm: String,
    slider_value: i32,
    custom_name: Option<String>,
}

#[derive(Clone, serde::Serialize)]
pub struct CompressProgress {
    pub percent: f64,
    pub current_file: String,
    pub bytes_processed: u64,
    pub total_bytes: u64,
}

#[derive(serde::Serialize)]
pub struct CompressResponse {
    success: bool,
    output_path: String,
    original_size: u64,
    compressed_size: u64,
    elapsed_ms: u64,
    error: Option<String>,
}

fn write_file_to_bay_archive(
    filepath: &Path,
    rel_name: &str,
    encoder: &mut zstd::stream::Encoder<File>,
    bytes_processed: &mut u64,
    total_original: u64,
    app: &AppHandle,
    last_emit: &mut Instant,
) {
    let file_meta = std::fs::metadata(filepath).ok();
    let file_size = file_meta.map(|m| m.len()).unwrap_or(0);

    let _ = encoder.write_all(&[1u8]); // 1 = File entry
    let filename_bytes = rel_name.as_bytes();
    let _ = encoder.write_all(&(filename_bytes.len() as u32).to_le_bytes());
    let _ = encoder.write_all(filename_bytes);
    let _ = encoder.write_all(&file_size.to_le_bytes());

    if let Ok(mut f) = File::open(filepath) {
        let mut buffer = [0u8; 256 * 1024];
        loop {
            match f.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = encoder.write_all(&buffer[..n]);
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
}

fn write_dir_to_bay_archive(
    dir: &Path,
    base_parent: &Path,
    encoder: &mut zstd::stream::Encoder<File>,
    bytes_processed: &mut u64,
    total_original: u64,
    app: &AppHandle,
    depth: usize,
    last_emit: &mut Instant,
) {
    if depth > 32 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let ep = entry.path();
            if let Ok(m) = std::fs::symlink_metadata(&ep) {
                if m.is_file() {
                    let rel = ep.strip_prefix(base_parent).unwrap_or(&ep);
                    write_file_to_bay_archive(&ep, &rel.to_string_lossy(), encoder, bytes_processed, total_original, app, last_emit);
                } else if m.is_dir() && !m.file_type().is_symlink() {
                    write_dir_to_bay_archive(&ep, base_parent, encoder, bytes_processed, total_original, app, depth + 1, last_emit);
                }
            }
        }
    }
}

fn compress_bay(
    files: &[String],
    output_path: &Path,
    slider_value: i32,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
    let first_path = Path::new(&files[0]);
    let parent_dir = first_path.parent().unwrap_or_else(|| Path::new("."));

    let zstd_level = match slider_value {
        0 => 1,
        25 => 3,
        50 => 7,
        75 => 14,
        _ => 19,
    };

    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    
    let mut encoder = zstd::stream::Encoder::new(out_file, zstd_level)
        .map_err(|e| format!("Failed to initialize compression encoder: {}", e))?;

    let _ = encoder.write_all(b"BAYZIP\x02");

    let mut bytes_processed: u64 = 0;
    let mut last_emit = Instant::now();

    for file_str in files {
        let p = Path::new(file_str);
        if p.is_file() {
            let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            write_file_to_bay_archive(p, filename, &mut encoder, &mut bytes_processed, total_original, app, &mut last_emit);
        } else if p.is_dir() {
            write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut bytes_processed, total_original, app, 0, &mut last_emit);
        }
    }

    let _ = encoder.write_all(&[0u8]); // EOF marker
    encoder.finish().map_err(|e| format!("Failed to finalize compression: {}", e))?;
    Ok(())
}

fn compress_zip(
    files: &[String],
    output_path: &Path,
    slider_value: i32,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create zip file: {}", e))?;
    let mut writer = zip::ZipWriter::new(out_file);
    
    let method = if slider_value == 0 {
        zip::CompressionMethod::Stored
    } else {
        zip::CompressionMethod::Deflated
    };

    let options = SimpleFileOptions::default()
        .compression_method(method)
        .large_file(true);

    let mut bytes_processed: u64 = 0;
    let mut last_emit = Instant::now();

    for file_str in files {
        let p = Path::new(file_str);
        if p.is_file() {
            let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            writer.start_file(file_name, options)
                .map_err(|e| format!("Zip start file error: {}", e))?;
            
            if let Ok(mut f) = File::open(p) {
                let mut buf = [0u8; 128 * 1024];
                loop {
                    match f.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
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
            fn add_dir_to_zip<W: Write + std::io::Seek>(
                dir: &Path,
                base: &Path,
                writer: &mut zip::ZipWriter<W>,
                options: SimpleFileOptions,
                bytes_processed: &mut u64,
                total_original: u64,
                app: &AppHandle,
                last_emit: &mut Instant,
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
                                let dir_name = format!("{}/", rel_str);
                                let _ = writer.add_directory(&dir_name, options);
                                add_dir_to_zip(&path, base, writer, options, bytes_processed, total_original, app, last_emit, depth + 1)?;
                            } else if m.is_file() {
                                writer.start_file(&rel_str, options)
                                    .map_err(|e| format!("Zip start file error: {}", e))?;
                                if let Ok(mut f) = File::open(&path) {
                                    let mut buf = [0u8; 128 * 1024];
                                    loop {
                                        match f.read(&mut buf) {
                                            Ok(0) => break,
                                            Ok(n) => {
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
            add_dir_to_zip(p, parent_dir, &mut writer, options, &mut bytes_processed, total_original, app, &mut last_emit, 0)?;
        }
    }

    writer.finish().map_err(|e| format!("Failed to finalize zip archive: {}", e))?;
    Ok(())
}

fn compress_tar(
    files: &[String],
    output_path: &Path,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
    let out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create tar file: {}", e))?;
    let mut builder = tar::Builder::new(BufWriter::new(out_file));

    for (i, file_str) in files.iter().enumerate() {
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

    builder.finish().map_err(|e| format!("Tar finalize error: {}", e))?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn compress_7z(
    files: &[String],
    output_path: &Path,
) -> Result<(), String> {
    if files.len() == 1 {
        let p = Path::new(&files[0]);
        sevenz_rust::compress_to_path(p, output_path)
            .map_err(|e| format!("7z compress error: {}", e))
    } else {
        let temp_dir = std::env::temp_dir().join(format!(
            "bayzip_7z_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("Failed to create temp dir: {}", e))?;
        
        for file_str in files {
            let p = Path::new(file_str);
            if let Some(name) = p.file_name() {
                let target = temp_dir.join(name);
                if p.is_file() {
                    let _ = std::fs::copy(p, &target);
                } else if p.is_dir() {
                    let _ = copy_dir_recursive(p, &target);
                }
            }
        }
        let res = sevenz_rust::compress_to_path(&temp_dir, output_path);
        let _ = std::fs::remove_dir_all(&temp_dir);
        res.map_err(|e| format!("7z compress error: {}", e))
    }
}

#[tauri::command]
fn compress_files(app: AppHandle, payload: CompressPayload) -> Result<CompressResponse, String> {
    if payload.files.is_empty() {
        return Err("No files selected to compress".to_string());
    }

    let start_time = Instant::now();
    let first_path = Path::new(&payload.files[0]);
    let parent_dir = first_path.parent().unwrap_or_else(|| Path::new("."));
    
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

    let fmt_clean = payload.format.trim_start_matches('.').to_lowercase();
    match fmt_clean.as_str() {
        "zip" => {
            compress_zip(&payload.files, &output_path, payload.slider_value, &app, total_original)?;
        },
        "tar" => {
            compress_tar(&payload.files, &output_path, &app, total_original)?;
        },
        "7z" => {
            compress_7z(&payload.files, &output_path)?;
        },
        _ => {
            compress_bay(&payload.files, &output_path, payload.slider_value, &app, total_original)?;
        }
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

#[derive(Clone, serde::Serialize)]
pub struct ExtractProgress {
    pub percent: f64,
    pub current_file: String,
    pub bytes_extracted: u64,
}

#[derive(serde::Serialize)]
pub struct ExtractResponse {
    pub success: bool,
    pub output_directory: String,
    pub file_count: usize,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ArchiveInfo {
    pub path: String,
    pub filename: String,
    pub size: u64,
    pub format: String,
    pub is_archive: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum ArchiveFormat {
    Bay,
    Zip,
    SevenZip,
    Tar,
    TarGz,
    Rar,
    Unknown,
}

fn detect_archive_type(path: &Path) -> ArchiveFormat {
    if let Ok(mut file) = File::open(path) {
        let mut magic = [0u8; 16];
        if let Ok(n) = file.read(&mut magic) {
            // Bayzip magic: BAYZIP\x01 or BAYZIP\x02
            if n >= 7 && (&magic[..7] == b"BAYZIP\x01" || &magic[..7] == b"BAYZIP\x02") {
                return ArchiveFormat::Bay;
            }
            // Zstandard frame header: 0xFD2FB528 in little-endian -> [0x28, 0xB5, 0x2F, 0xFD]
            if n >= 4 && magic[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
                return ArchiveFormat::Bay;
            }
            // PK ZIP: PK\x03\x04 or PK\x05\x06 or PK\x07\x08
            if n >= 4 && magic[0] == 0x50 && magic[1] == 0x4B {
                return ArchiveFormat::Zip;
            }
            // 7-Zip: 7z\xBC\xAF\x27\x1C
            if n >= 6 && magic[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
                return ArchiveFormat::SevenZip;
            }
            // RAR: Rar!\x1A\x07
            if n >= 6 && magic[0..6] == [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07] {
                return ArchiveFormat::Rar;
            }
            // Gzip (tar.gz / tgz): 0x1F, 0x8B
            if n >= 2 && magic[0] == 0x1F && magic[1] == 0x8B {
                return ArchiveFormat::TarGz;
            }
        }
    }

    // Fallback to extension
    let path_str = path.to_string_lossy().to_lowercase();
    if path_str.ends_with(".bay") || path_str.ends_with(".bay_gamer") {
        ArchiveFormat::Bay
    } else if path_str.ends_with(".zip") {
        ArchiveFormat::Zip
    } else if path_str.ends_with(".7z") {
        ArchiveFormat::SevenZip
    } else if path_str.ends_with(".rar") {
        ArchiveFormat::Rar
    } else if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
        ArchiveFormat::TarGz
    } else if path_str.ends_with(".tar") {
        ArchiveFormat::Tar
    } else {
        ArchiveFormat::Unknown
    }
}

#[tauri::command]
fn get_archive_info(path: String) -> Result<ArchiveInfo, String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err("File not found".into());
    }
    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("archive").to_string();
    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    
    let detected = detect_archive_type(p);
    let (format, is_archive) = match detected {
        ArchiveFormat::Bay => ("Bayzip Archive (.bay)".to_string(), true),
        ArchiveFormat::Zip => ("ZIP Archive (.zip)".to_string(), true),
        ArchiveFormat::SevenZip => ("7-Zip Archive (.7z)".to_string(), true),
        ArchiveFormat::Rar => ("RAR Archive (.rar)".to_string(), true),
        ArchiveFormat::Tar | ArchiveFormat::TarGz => ("TAR Archive (.tar)".to_string(), true),
        ArchiveFormat::Unknown => {
            let lower = path.to_lowercase();
            if lower.ends_with(".bay") {
                ("Bayzip Archive (.bay)".to_string(), true)
            } else if lower.ends_with(".zip") {
                ("ZIP Archive (.zip)".to_string(), true)
            } else if lower.ends_with(".7z") {
                ("7-Zip Archive (.7z)".to_string(), true)
            } else if lower.ends_with(".rar") {
                ("RAR Archive (.rar)".to_string(), true)
            } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                ("TAR Archive (.tar)".to_string(), true)
            } else {
                ("Unknown".to_string(), false)
            }
        }
    };

    Ok(ArchiveInfo {
        path,
        filename,
        size,
        format,
        is_archive,
    })
}

fn extract_bay_archive(
    archive_path: &Path,
    dest_dir: &Path,
    app: Option<&AppHandle>,
) -> Result<Vec<String>, String> {
    let file = File::open(archive_path)
        .map_err(|e| format!("Failed to open .bay archive: {}", e))?;
    
    let mut decoder = zstd::stream::Decoder::new(file)
        .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;
    
    let mut magic = [0u8; 7];
    decoder.read_exact(&mut magic)
        .map_err(|e| format!("Invalid or corrupted .bay header: {}", e))?;
    
    let mut extracted_files = Vec::new();
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    let mut last_emit = Instant::now();

    if &magic == b"BAYZIP\x01" {
        // Backwards-compatible legacy single-file format
        let mut len_bytes = [0u8; 4];
        decoder.read_exact(&mut len_bytes)
            .map_err(|e| format!("Failed to read filename length: {}", e))?;
        let name_len = u32::from_le_bytes(len_bytes) as usize;
        let mut name_buf = vec![0u8; name_len];
        decoder.read_exact(&mut name_buf)
            .map_err(|e| format!("Failed to read filename: {}", e))?;
        let filename = String::from_utf8_lossy(&name_buf).to_string();
        
        let out_path = dest_dir.join(&filename);
        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let out_file = File::create(&out_path)
            .map_err(|e| format!("Failed to create output file: {}", e))?;
        let mut writer = BufWriter::with_capacity(512 * 1024, out_file);
        
        let mut buf = vec![0u8; 512 * 1024];
        let mut total_copied: u64 = 0;
        loop {
            match decoder.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    writer.write_all(&buf[..n])
                        .map_err(|e| format!("Failed to write decompressed data: {}", e))?;
                    total_copied += n as u64;
                    if let Some(app_handle) = app {
                        if last_emit.elapsed().as_millis() >= 50 {
                            last_emit = Instant::now();
                            let _ = app_handle.emit("extract_progress", ExtractProgress {
                                percent: 50.0,
                                current_file: filename.clone(),
                                bytes_extracted: total_copied,
                            });
                        }
                    }
                }
                Err(e) => return Err(format!("Decompression error: {}", e)),
            }
        }
        writer.flush().map_err(|e| format!("Flush error: {}", e))?;
        extracted_files.push(out_path.to_string_lossy().to_string());
    } else if &magic == b"BAYZIP\x02" {
        // Robust Multi-file format with entry metadata and file size
        let mut total_extracted: u64 = 0;
        loop {
            let mut entry_type = [0u8; 1];
            match decoder.read_exact(&mut entry_type) {
                Ok(_) => {},
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(format!("Error reading archive entry: {}", e)),
            }
            if entry_type[0] == 0 {
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
            
            let out_path = dest_dir.join(&rel_path_str);
            if let Some(parent) = out_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            
            let out_file = File::create(&out_path)
                .map_err(|e| format!("Failed to create output file {:?}: {}", out_path, e))?;
            let mut writer = BufWriter::with_capacity(512 * 1024, out_file);
            
            let mut remaining = file_size;
            let mut buf = vec![0u8; 512 * 1024];
            while remaining > 0 {
                let to_read = (remaining as usize).min(buf.len());
                let n = decoder.read(&mut buf[..to_read])
                    .map_err(|e| format!("Error reading decompressed stream: {}", e))?;
                if n == 0 {
                    return Err("Premature EOF inside archive entry".into());
                }
                writer.write_all(&buf[..n])
                    .map_err(|e| format!("Error writing extracted file: {}", e))?;
                remaining -= n as u64;
                total_extracted += n as u64;
                
                if let Some(app_handle) = app {
                    if last_emit.elapsed().as_millis() >= 50 {
                        last_emit = Instant::now();
                        let _ = app_handle.emit("extract_progress", ExtractProgress {
                            percent: 50.0,
                            current_file: rel_path_str.clone(),
                            bytes_extracted: total_extracted,
                        });
                    }
                }
            }
            writer.flush().map_err(|e| format!("Flush error: {}", e))?;
            extracted_files.push(out_path.to_string_lossy().to_string());
        }
    } else {
        return Err("Unrecognized .bay archive format or invalid header".into());
    }

    Ok(extracted_files)
}

fn extract_zip_archive(
    archive_path: &Path,
    dest_dir: &Path,
    app: Option<&AppHandle>,
) -> Result<Vec<String>, String> {
    let file = File::open(archive_path).map_err(|e| format!("Failed to open zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip archive: {}", e))?;
    let mut extracted = Vec::new();
    let total_entries = archive.len();

    for i in 0..total_entries {
        let mut file = archive.by_index(i).map_err(|e| format!("Zip entry error: {}", e))?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') || file.is_dir() {
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
                current_file: file.name().to_string(),
                bytes_extracted: 0,
            });
        }
    }
    Ok(extracted)
}

fn extract_tar_archive(
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

fn extract_7z_archive(
    archive_path: &Path,
    dest_dir: &Path,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create dest: {}", e))?;
    sevenz_rust::decompress_file(archive_path, dest_dir)
        .map_err(|e| format!("7z extraction error: {}", e))?;
    Ok(vec![dest_dir.to_string_lossy().to_string()])
}

fn extract_with_system_tar(
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

fn universal_extract(
    archive_path: &Path,
    dest_dir: &Path,
    app: Option<&AppHandle>,
) -> Result<Vec<String>, String> {
    let format = detect_archive_type(archive_path);
    match format {
        ArchiveFormat::Bay => {
            extract_bay_archive(archive_path, dest_dir, app)
        },
        ArchiveFormat::Zip => {
            extract_zip_archive(archive_path, dest_dir, app)
                .or_else(|_| extract_bay_archive(archive_path, dest_dir, app))
                .or_else(|_| extract_with_system_tar(archive_path, dest_dir))
        },
        ArchiveFormat::SevenZip => {
            extract_7z_archive(archive_path, dest_dir)
                .or_else(|_| extract_with_system_tar(archive_path, dest_dir))
        },
        ArchiveFormat::Tar | ArchiveFormat::TarGz => {
            extract_tar_archive(archive_path, dest_dir)
                .or_else(|_| extract_with_system_tar(archive_path, dest_dir))
        },
        ArchiveFormat::Rar => {
            extract_with_system_tar(archive_path, dest_dir)
        },
        ArchiveFormat::Unknown => {
            extract_bay_archive(archive_path, dest_dir, app)
                .or_else(|_| extract_zip_archive(archive_path, dest_dir, app))
                .or_else(|_| extract_7z_archive(archive_path, dest_dir))
                .or_else(|_| extract_tar_archive(archive_path, dest_dir))
                .or_else(|_| extract_with_system_tar(archive_path, dest_dir))
        }
    }
}

#[tauri::command]
fn extract_archive(
    app: AppHandle,
    archive_path: String,
    destination: Option<String>,
) -> Result<ExtractResponse, String> {
    let path = Path::new(&archive_path);
    if !path.exists() {
        return Err("Archive file does not exist".into());
    }

    let start_time = Instant::now();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("extracted");

    let dest_dir = if let Some(custom_dest) = destination {
        if custom_dest.trim().is_empty() {
            parent.join(stem)
        } else {
            PathBuf::from(custom_dest)
        }
    } else {
        parent.join(stem)
    };

    let extracted_files = universal_extract(path, &dest_dir, Some(&app))?;

    if let Some(app_handle) = Some(&app) {
        let _ = app_handle.emit("extract_progress", ExtractProgress {
            percent: 100.0,
            current_file: "Completed".into(),
            bytes_extracted: 0,
        });
    }

    Ok(ExtractResponse {
        success: true,
        output_directory: dest_dir.to_string_lossy().to_string(),
        file_count: extracted_files.len(),
        elapsed_ms: start_time.elapsed().as_millis() as u64,
        error: None,
    })
}

pub fn headless_extract_here(archive_path: &str) -> Result<(), String> {
    let p = Path::new(archive_path);
    if !p.exists() {
        return Err("Archive file does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    universal_extract(p, parent, None)?;

    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("explorer.exe");
        cmd.arg(parent);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.spawn();
    }
    Ok(())
}

pub fn headless_extract_to_subfolder(archive_path: &str) -> Result<(), String> {
    let p = Path::new(archive_path);
    if !p.exists() {
        return Err("Archive file does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("extracted");
    let dest = parent.join(stem);
    universal_extract(p, &dest, None)?;

    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("explorer.exe");
        cmd.arg(&dest);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.spawn();
    }
    Ok(())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet, 
            get_system_specs, 
            get_cli_args, 
            register_context_menu,
            compress_files,
            get_selected_items_info,
            extract_archive,
            get_archive_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

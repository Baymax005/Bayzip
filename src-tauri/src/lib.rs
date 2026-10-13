use sysinfo::System;
use serde::Serialize;
use std::process::Command;
use std::fs::File;
use std::io::{Read, Write, Seek, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use zip::write::SimpleFileOptions;
use zip::AesMode;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce, Key
};
use sha2::{Sha256, Digest};
use rand::RngCore;
use sevenz_rust2::{ArchiveWriter, ArchiveEntry, EncoderConfiguration, EncoderMethod, Password};

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
    std::env::args()
        .skip(1)
        .filter(|arg| !arg.starts_with("--"))
        .map(|arg| arg.trim_matches('\"').to_string())
        .collect()
}

pub fn register_context_menu_sync() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            let exe_str = exe_path.to_string_lossy().to_string();
            let escaped_exe = exe_str.replace('\\', "\\\\").replace('"', "\\\"");

            let app_dir = std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir())
                .join("Bayzip");
            let _ = std::fs::create_dir_all(&app_dir);
            let icon_file = app_dir.join("bayzip.ico");
            let _ = std::fs::write(&icon_file, include_bytes!("../icons/icon.ico"));
            let icon_path_str = icon_file.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");

            let archive_exts = [
                ".bay",
                ".bay_gamer",
                ".zip",
                ".7z",
                ".rar",
                ".tar",
                ".gz",
                ".tgz",
            ];

            let mut reg_content = String::from("Windows Registry Editor Version 5.00\r\n\r\n");

            // 1. Register .bay and .bay_gamer file association (ProgIDs & DefaultIcons)
            reg_content.push_str(&format!(
                "[HKEY_CURRENT_USER\\Software\\Classes\\.bay]\r\n\
                @=\"Bayzip.Archive\"\r\n\
                \"Content Type\"=\"application/x-bayzip\"\r\n\
                \"PerceivedType\"=\"compressed\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay\\DefaultIcon]\r\n\
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay\\OpenWithProgids]\r\n\
                \"Bayzip.Archive\"=\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive]\r\n\
                @=\"Bayzip Compressed Archive\"\r\n\
                \"FriendlyTypeName\"=\"Bayzip Compressed Archive\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\DefaultIcon]\r\n\
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell]\r\n\
                @=\"open\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\open]\r\n\
                @=\"Open in Bayzip\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\open\\command]\r\n\
                @=\"\\\"{escaped_exe}\\\" \\\"%1\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay_gamer]\r\n\
                @=\"Bayzip.GamerArchive\"\r\n\
                \"Content Type\"=\"application/x-bayzip-gamer\"\r\n\
                \"PerceivedType\"=\"compressed\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay_gamer\\DefaultIcon]\r\n\
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay_gamer\\OpenWithProgids]\r\n\
                \"Bayzip.GamerArchive\"=\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive]\r\n\
                @=\"Bayzip Gamer Mod Pack\"\r\n\
                \"FriendlyTypeName\"=\"Bayzip Gamer Mod Pack\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\DefaultIcon]\r\n\
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\shell]\r\n\
                @=\"open\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\shell\\open]\r\n\
                @=\"Open in Bayzip\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\shell\\open\\command]\r\n\
                @=\"\\\"{escaped_exe}\\\" \\\"%1\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\AppX9rkaq77s0jzh1tyccadx9ghba15r6t3h\\DefaultIcon]\r\n\
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n"
            ));

            // 2. Register "Extract with Bayzip" for each archive format under SystemFileAssociations and Bayzip ProgIDs
            let mut extract_targets = vec!["Bayzip.Archive".to_string(), "Bayzip.GamerArchive".to_string()];
            for ext in &archive_exts {
                extract_targets.push(format!("SystemFileAssociations\\{}", ext));
            }

            for target in extract_targets {
                reg_content.push_str(&format!(
                    "[-HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract]\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract]\r\n\
                    \"MUIVerb\"=\"Extract with Bayzip\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\
                    \"SubCommands\"=\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell]\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\01_extract_dialog]\r\n\
                    \"MUIVerb\"=\"Extract files...\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\01_extract_dialog\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --gui-extract \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\02_extract_sub]\r\n\
                    \"MUIVerb\"=\"Extract to subfolder\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\02_extract_sub\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --extract-to-folder \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\03_extract_here]\r\n\
                    \"MUIVerb\"=\"Extract Here\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\03_extract_here\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --extract-here \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\04_open]\r\n\
                    \"MUIVerb\"=\"Open in Bayzip\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\04_open\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" \\\"%1\\\"\"\r\n\r\n"
                ));
            }

            // 3. Register "Add to archive with Bayzip" for * (all files) and Directory (folders)
            for target in &["*", "Directory"] {
                reg_content.push_str(&format!(
                    "[-HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress]\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress]\r\n\
                    \"MUIVerb\"=\"Add to archive with Bayzip\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\
                    \"SubCommands\"=\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell]\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\00_options]\r\n\
                    \"MUIVerb\"=\"Add to archive...\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\00_options\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\01_bay]\r\n\
                    \"MUIVerb\"=\"Add to .bay (Ultra-Fast)\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\01_bay\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --quick-compress .bay \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\02_zip]\r\n\
                    \"MUIVerb\"=\"Add to .zip (Standard)\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\02_zip\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --quick-compress .zip \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\03_7z]\r\n\
                    \"MUIVerb\"=\"Add to .7z (High Ratio)\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\03_7z\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --quick-compress .7z \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\04_tar]\r\n\
                    \"MUIVerb\"=\"Add to .tar (Universal)\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\04_tar\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --quick-compress .tar \\\"%1\\\"\"\r\n\r\n"
                ));
            }

            // Write as UTF-16 LE with BOM (Required by Windows reg.exe)
            let temp_reg = std::env::temp_dir().join("bayzip_context.reg");
            let mut utf16_bytes: Vec<u8> = vec![0xFF, 0xFE];
            for u in reg_content.encode_utf16() {
                utf16_bytes.extend_from_slice(&u.to_le_bytes());
            }

            if std::fs::write(&temp_reg, utf16_bytes).is_ok() {
                let mut cmd = Command::new("reg");
                cmd.args(["import", &temp_reg.to_string_lossy()]);
                cmd.creation_flags(CREATE_NO_WINDOW);
                let _ = cmd.output();
                let _ = std::fs::remove_file(temp_reg);

                // Refresh Windows Explorer shell & clear icon cache
                let mut refresh_cmd = Command::new("cmd");
                refresh_cmd.args(["/C", "ie4uinit.exe -ClearIconCache & ie4uinit.exe -show"]);
                refresh_cmd.creation_flags(CREATE_NO_WINDOW);
                let _ = refresh_cmd.output();

                return Ok("Context menu registered successfully!".into());
            }
        }
        Err("Could not determine executable path.".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("Context menu not applicable on non-Windows OS".into())
    }
}

#[tauri::command]
async fn register_context_menu() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        register_context_menu_sync()
    })
    .await
    .map_err(|e| format!("Registry sync error: {}", e))?
}

fn headless_quick_compress_zip(p: &Path, output_path: &Path) -> Result<(), String> {
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

fn headless_quick_compress_bay(p: &Path, output_path: &Path) -> Result<(), String> {
    let mut out_file = File::create(output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    out_file.write_all(b"BAYZIP\x02\x00")
        .map_err(|e| format!("Header write error: {}", e))?;
    
    // Balanced profile level 7 (50% slider equivalent)
    let mut encoder = zstd::stream::Encoder::new(out_file, 7)
        .map_err(|e| format!("Encoder error: {}", e))?;

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

fn headless_quick_compress_7z(p: &Path, output_path: &Path) -> Result<(), String> {
    let mut sz = ArchiveWriter::create(output_path)
        .map_err(|e| format!("Failed to create 7z: {}", e))?;
    sz.set_encrypt_header(false);
    sz.set_content_methods(vec![EncoderMethod::LZMA2.into()]);

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
    let p = Path::new(target_path_str);
    if !p.exists() {
        return Err("Target path does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("archive");
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
    encryption: Option<String>,
    password: Option<String>,
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

fn derive_bay_key(password: &str, salt: &[u8; 16]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(salt);
    let mut derived = hasher.finalize();
    for _ in 1..5_000 {
        let mut h = Sha256::new();
        h.update(derived);
        h.update(salt);
        derived = h.finalize();
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&derived);
    key
}

fn write_file_to_bay_archive<W: Write>(
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
            match f.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
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

fn write_dir_to_bay_archive<W: Write>(
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

fn compress_bay(
    files: &[String],
    output_path: &Path,
    algorithm: &str,
    slider_value: i32,
    password: Option<&str>,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
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
            let p = Path::new(file_str);
            if p.is_file() {
                let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                write_file_to_bay_archive(p, filename, &mut encoder, &mut bytes_processed, total_original, app, &mut last_emit)?;
            } else if p.is_dir() {
                write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
            }
        }
        let _ = encoder.write_all(&[0u8]); // EOF
        encoder.flush().map_err(|e| format!("Brotli finalize error: {}", e))?;
    } else if algo_lower.contains("lz4") {
        out_file.write_all(b"BAYLZ4\x02\x00")
            .map_err(|e| format!("Header write error: {}", e))?;
        let mut encoder = lz4_flex::frame::FrameEncoder::new(BufWriter::new(out_file));
        for file_str in files {
            let p = Path::new(file_str);
            if p.is_file() {
                let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                write_file_to_bay_archive(p, filename, &mut encoder, &mut bytes_processed, total_original, app, &mut last_emit)?;
            } else if p.is_dir() {
                write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
            }
        }
        let _ = encoder.write_all(&[0u8]); // EOF
        let _ = encoder.finish().map_err(|e| format!("LZ4 finalize error: {}", e))?;
    } else {
        // Default to Zstandard (Recommended)
        out_file.write_all(b"BAYZIP\x02\x00")
            .map_err(|e| format!("Header write error: {}", e))?;
        let zstd_level = match slider_value {
            0 => 1,
            25 => 3,
            50 => 7,
            75 => 14,
            _ => 19,
        };
        let mut encoder = zstd::stream::Encoder::new(BufWriter::new(out_file), zstd_level)
            .map_err(|e| format!("Failed to initialize compression encoder: {}", e))?;
        for file_str in files {
            let p = Path::new(file_str);
            if p.is_file() {
                let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                write_file_to_bay_archive(p, filename, &mut encoder, &mut bytes_processed, total_original, app, &mut last_emit)?;
            } else if p.is_dir() {
                write_dir_to_bay_archive(p, parent_dir, &mut encoder, &mut bytes_processed, total_original, app, 0, &mut last_emit)?;
            }
        }
        let _ = encoder.write_all(&[0u8]); // EOF
        encoder.finish().map_err(|e| format!("Failed to finalize compression: {}", e))?;
    }

    if let Some(pwd) = pwd_trimmed {
        let mut raw_data = Vec::new();
        File::open(&target_dest)
            .and_then(|mut f| f.read_to_end(&mut raw_data))
            .map_err(|e| format!("Failed to read unencrypted buffer: {}", e))?;
        let _ = std::fs::remove_file(&target_dest);

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

        let mut final_file = File::create(output_path)
            .map_err(|e| format!("Failed to create encrypted output file: {}", e))?;
        final_file.write_all(b"BAYENC\x02\x00").map_err(|e| e.to_string())?;
        final_file.write_all(&salt).map_err(|e| e.to_string())?;
        final_file.write_all(&nonce_bytes).map_err(|e| e.to_string())?;
        final_file.write_all(&ciphertext).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn compress_zip(
    files: &[String],
    output_path: &Path,
    algorithm: &str,
    slider_value: i32,
    password: Option<&str>,
    encryption: Option<&str>,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
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
            add_dir_to_zip(p, parent_dir, &mut writer, method, has_pwd, aes_mode, &mut bytes_processed, total_original, app, &mut last_emit, 0)?;
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

fn compress_7z(
    files: &[String],
    output_path: &Path,
    password: Option<&str>,
    app: &AppHandle,
    total_original: u64,
) -> Result<(), String> {
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
    methods.push(EncoderMethod::LZMA2.into());
    sz.set_content_methods(methods);

    let mut bytes_processed: u64 = 0;
    let mut last_emit = Instant::now();

    for file_str in files {
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

    sz.finish().map_err(|e| format!("7z finalize error: {}", e))?;
    Ok(())
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

    let pwd_ref = payload.password.as_deref();
    let enc_ref = payload.encryption.as_deref();
    let fmt_clean = payload.format.trim_start_matches('.').to_lowercase();
    match fmt_clean.as_str() {
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
            )?;
        },
        "tar" => {
            compress_tar(&payload.files, &output_path, &app, total_original)?;
        },
        "7z" => {
            compress_7z(&payload.files, &output_path, pwd_ref, &app, total_original)?;
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
            )?;
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

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ArchiveEntryItem {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(serde::Serialize)]
pub struct ArchiveInfo {
    pub path: String,
    pub filename: String,
    pub size: u64,
    pub format: String,
    pub is_archive: bool,
    pub is_encrypted: bool,
    pub file_count: usize,
    pub total_uncompressed_size: u64,
    pub entries: Vec<ArchiveEntryItem>,
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
            // Bayzip magic signatures:
            // BAYZIP\x01, BAYZIP\x02, BAYBRO\x02, BAYLZ4\x02, BAYENC\x01, BAYENC\x02
            if n >= 7 && (
                &magic[..7] == b"BAYZIP\x01" ||
                &magic[..7] == b"BAYZIP\x02" ||
                &magic[..7] == b"BAYBRO\x02" ||
                &magic[..7] == b"BAYLZ4\x02" ||
                &magic[..7] == b"BAYENC\x01" ||
                &magic[..7] == b"BAYENC\x02"
            ) {
                return ArchiveFormat::Bay;
            }
            // Zstandard frame header: 0xFD2FB528 in little-endian -> [0x28, 0xB5, 0x2F, 0xFD]
            if n >= 4 && magic[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
                return ArchiveFormat::Bay;
            }
            // LZ4 frame header: 0x184D2204 in little-endian -> [0x04, 0x22, 0x4D, 0x18]
            if n >= 4 && magic[0..4] == [0x04, 0x22, 0x4D, 0x18] {
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

fn is_archive_encrypted(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else { return false; };
    let mut header = [0u8; 16];
    let n = file.read(&mut header).unwrap_or(0);
    if n >= 7 && (header.starts_with(b"BAYENC\x01") || header.starts_with(b"BAYENC\x02")) {
        return true;
    }
    // Check ZIP encryption
    if n >= 4 && header[0] == 0x50 && header[1] == 0x4B {
        if let Ok(file_for_zip) = File::open(path) {
            if let Ok(mut zip) = zip::ZipArchive::new(file_for_zip) {
                for i in 0..zip.len() {
                    match zip.by_index(i) {
                        Ok(entry) => {
                            if entry.encrypted() {
                                return true;
                            }
                        },
                        Err(zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED)) => {
                            return true;
                        },
                        Err(zip::result::ZipError::InvalidPassword) => {
                            return true;
                        },
                        _ => {}
                    }
                }
            }
        }
    }
    // Check 7z encryption
    if n >= 6 && header[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        if let Ok(file) = File::open(path) {
            match sevenz_rust2::ArchiveReader::new(file, Password::empty()) {
                Ok(mut reader) => {
                    if let Err(sevenz_rust2::Error::PasswordRequired) = reader.for_each_entries(|_, _| Ok(false)) {
                        return true;
                    }
                }
                Err(sevenz_rust2::Error::PasswordRequired) => {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

fn list_zip_entries(p: &Path, _password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed = 0;
    if let Ok(file) = File::open(p) {
        if let Ok(mut archive) = zip::ZipArchive::new(file) {
            for i in 0..archive.len() {
                if let Ok(entry) = archive.by_index_raw(i) {
                    let clean = entry.name().replace('\\', "/");
                    let name = clean.trim_start_matches('/').to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let size = entry.size();
                    let is_dir = entry.is_dir() || clean.ends_with('/');
                    total_uncompressed += size;
                    items.push(ArchiveEntryItem {
                        name,
                        size,
                        is_dir,
                    });
                }
            }
        }
    }
    let count = items.len();
    (items, count, total_uncompressed)
}

fn list_7z_entries(p: &Path, password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed = 0;
    if let Ok(file) = File::open(p) {
        let pwd = password.map(|s| Password::from(s)).unwrap_or_else(Password::empty);
        if let Ok(sz) = sevenz_rust2::ArchiveReader::new(file, pwd) {
            for entry in sz.archive().files.iter() {
                let raw_name = entry.name();
                let clean = raw_name.replace('\\', "/");
                let trimmed = clean.trim_start_matches('/');
                if trimmed.is_empty() {
                    continue;
                }
                let size = entry.size();
                let is_dir = entry.is_directory() || trimmed.ends_with('/');
                total_uncompressed += size;
                items.push(ArchiveEntryItem {
                    name: trimmed.to_string(),
                    size,
                    is_dir,
                });
            }
        }
    }
    let count = items.len();
    (items, count, total_uncompressed)
}

fn list_tar_entries(p: &Path) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed = 0;
    let is_gz = p.to_string_lossy().to_lowercase().ends_with(".gz") || p.to_string_lossy().to_lowercase().ends_with(".tgz");
    if let Ok(file) = File::open(p) {
        if is_gz {
            let gz = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(gz);
            if let Ok(entries) = archive.entries() {
                for entry in entries.flatten() {
                    if let Ok(path) = entry.path() {
                        let name = path.to_string_lossy().replace('\\', "/").trim_start_matches('/').to_string();
                        if !name.is_empty() {
                            let size = entry.size();
                            let is_dir = entry.header().entry_type().is_dir();
                            total_uncompressed += size;
                            items.push(ArchiveEntryItem { name, size, is_dir });
                        }
                    }
                }
            }
        } else {
            let mut archive = tar::Archive::new(file);
            if let Ok(entries) = archive.entries() {
                for entry in entries.flatten() {
                    if let Ok(path) = entry.path() {
                        let name = path.to_string_lossy().replace('\\', "/").trim_start_matches('/').to_string();
                        if !name.is_empty() {
                            let size = entry.size();
                            let is_dir = entry.header().entry_type().is_dir();
                            total_uncompressed += size;
                            items.push(ArchiveEntryItem { name, size, is_dir });
                        }
                    }
                }
            }
        }
    }
    let count = items.len();
    (items, count, total_uncompressed)
}

fn list_bay_stream_entries<R: Read>(mut decoder: R) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_size = 0u64;

    let mut first_byte = [0u8; 1];
    let mut has_first = false;
    if decoder.read_exact(&mut first_byte).is_ok() {
        if first_byte[0] == b'B' {
            let mut rest = [0u8; 6];
            let _ = decoder.read_exact(&mut rest);
        } else {
            has_first = true;
        }
    } else {
        return (items, 0, 0);
    }

    loop {
        let entry_type = if has_first {
            has_first = false;
            first_byte[0]
        } else {
            let mut b = [0u8; 1];
            match decoder.read_exact(&mut b) {
                Ok(_) => b[0],
                Err(_) => break,
            }
        };

        if entry_type == 0 {
            break;
        }

        let mut len_bytes = [0u8; 4];
        if decoder.read_exact(&mut len_bytes).is_err() { break; }
        let name_len = u32::from_le_bytes(len_bytes) as usize;
        let mut name_buf = vec![0u8; name_len];
        if decoder.read_exact(&mut name_buf).is_err() { break; }
        let raw_name = String::from_utf8_lossy(&name_buf);
        let clean = raw_name.replace('\\', "/");
        let name = clean.trim_start_matches('/').to_string();

        let mut size_bytes = [0u8; 8];
        if decoder.read_exact(&mut size_bytes).is_err() { break; }
        let file_size = u64::from_le_bytes(size_bytes);

        total_size += file_size;
        items.push(ArchiveEntryItem {
            name,
            size: file_size,
            is_dir: false,
        });

        // Skip file_size bytes
        let mut skipped = 0;
        let mut discard = [0u8; 64 * 1024];
        while skipped < file_size {
            let to_read = ((file_size - skipped) as usize).min(discard.len());
            match decoder.read(&mut discard[..to_read]) {
                Ok(0) => break,
                Ok(n) => skipped += n as u64,
                Err(_) => break,
            }
        }
    }

    let count = items.len();
    (items, count, total_size)
}

fn list_bay_entries(p: &Path, password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let Ok(mut file) = File::open(p) else { return (Vec::new(), 0, 0); };
    let mut magic = [0u8; 8];
    let Ok(n) = file.read(&mut magic) else { return (Vec::new(), 0, 0); };

    if n >= 7 && (magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
        let Some(pwd) = password.filter(|s| !s.trim().is_empty()) else {
            return (Vec::new(), 0, 0);
        };
        let header_len = if magic.starts_with(b"BAYENC\x02") { 8 } else { 7 };
        if file.seek(std::io::SeekFrom::Start(header_len)).is_err() { return (Vec::new(), 0, 0); }
        let mut salt = [0u8; 16];
        if file.read_exact(&mut salt).is_err() { return (Vec::new(), 0, 0); }
        let mut nonce_bytes = [0u8; 12];
        if file.read_exact(&mut nonce_bytes).is_err() { return (Vec::new(), 0, 0); }
        let mut ciphertext = Vec::new();
        if file.read_to_end(&mut ciphertext).is_err() { return (Vec::new(), 0, 0); }

        let key_bytes = derive_bay_key(pwd, &salt);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let Ok(plaintext) = cipher.decrypt(nonce, ciphertext.as_ref()) else {
            return (Vec::new(), 0, 0);
        };

        if plaintext.starts_with(b"BAYBRO\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            let decoder = brotli::Decompressor::new(&plaintext[offset..], 64 * 1024);
            return list_bay_stream_entries(decoder);
        } else if plaintext.starts_with(b"BAYLZ4\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            let decoder = lz4_flex::frame::FrameDecoder::new(&plaintext[offset..]);
            return list_bay_stream_entries(decoder);
        } else if plaintext.starts_with(b"BAYZIP\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            if let Ok(decoder) = zstd::stream::Decoder::new(&plaintext[offset..]) {
                return list_bay_stream_entries(decoder);
            }
        } else if plaintext.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
            let decoder = lz4_flex::frame::FrameDecoder::new(&plaintext[..]);
            return list_bay_stream_entries(decoder);
        } else if let Ok(decoder) = zstd::stream::Decoder::new(&plaintext[..]) {
            return list_bay_stream_entries(decoder);
        }
        return (Vec::new(), 0, 0);
    }

    let _ = file.seek(std::io::SeekFrom::Start(0));
    let mut magic_header = [0u8; 8];
    let n = file.read(&mut magic_header).unwrap_or(0);
    if n >= 7 && magic_header.starts_with(b"BAYBRO\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let decoder = brotli::Decompressor::new(file, 64 * 1024);
        list_bay_stream_entries(decoder)
    } else if n >= 7 && magic_header.starts_with(b"BAYLZ4\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        list_bay_stream_entries(decoder)
    } else if n >= 7 && magic_header.starts_with(b"BAYZIP\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        if let Ok(decoder) = zstd::stream::Decoder::new(file) {
            list_bay_stream_entries(decoder)
        } else {
            (Vec::new(), 0, 0)
        }
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        list_bay_stream_entries(decoder)
    } else {
        let _ = file.seek(std::io::SeekFrom::Start(0));
        if let Ok(decoder) = zstd::stream::Decoder::new(file) {
            list_bay_stream_entries(decoder)
        } else {
            (Vec::new(), 0, 0)
        }
    }
}

pub fn inspect_archive(path: &str, password: Option<&str>) -> Result<ArchiveInfo, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err("File not found".into());
    }
    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("archive").to_string();
    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    
    let detected = detect_archive_type(p);
    let is_enc = is_archive_encrypted(p);
    let pwd_ref = password;

    let (entries, file_count, total_uncompressed_size) = match detected {
        ArchiveFormat::Zip => list_zip_entries(p, pwd_ref),
        ArchiveFormat::SevenZip => list_7z_entries(p, pwd_ref),
        ArchiveFormat::Tar | ArchiveFormat::TarGz => list_tar_entries(p),
        ArchiveFormat::Bay => list_bay_entries(p, pwd_ref),
        _ => {
            let lower = path.to_lowercase();
            if lower.ends_with(".zip") {
                list_zip_entries(p, pwd_ref)
            } else if lower.ends_with(".7z") {
                list_7z_entries(p, pwd_ref)
            } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                list_tar_entries(p)
            } else if lower.ends_with(".bay") || lower.ends_with(".bay_gamer") {
                list_bay_entries(p, pwd_ref)
            } else {
                (Vec::new(), 0, 0)
            }
        }
    };

    let (format, is_archive) = match detected {
        ArchiveFormat::Bay => ("Bayzip Archive (.bay)".to_string(), true),
        ArchiveFormat::Zip => ("ZIP Archive (.zip)".to_string(), true),
        ArchiveFormat::SevenZip => ("7-Zip Archive (.7z)".to_string(), true),
        ArchiveFormat::Rar => ("RAR Archive (.rar)".to_string(), true),
        ArchiveFormat::Tar | ArchiveFormat::TarGz => ("TAR Archive (.tar)".to_string(), true),
        ArchiveFormat::Unknown => {
            let lower = path.to_lowercase();
            if lower.ends_with(".bay") || lower.ends_with(".bay_gamer") {
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
        path: path.to_string(),
        filename,
        size,
        format,
        is_archive,
        is_encrypted: is_enc,
        file_count,
        total_uncompressed_size,
        entries,
    })
}

#[tauri::command]
fn get_archive_info(path: String, password: Option<String>) -> Result<ArchiveInfo, String> {
    inspect_archive(&path, password.as_deref())
}

fn extract_bay_stream<R: Read>(
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

        let sanitized = rel_path_str.replace('\\', "/");
        let trimmed_name = sanitized.trim_start_matches('/');
        let out_path = dest_dir.join(trimmed_name);

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
                                current_file: trimmed_name.to_string(),
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

fn extract_bay_archive(
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

fn extract_zip_archive(
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
        let clean = raw_name.replace('\\', "/");
        let sanitized = clean.trim_start_matches('/');
        if sanitized.is_empty() {
            continue;
        }

        let safe_path = PathBuf::from(sanitized);
        if safe_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            continue;
        }

        let outpath = dest_dir.join(&safe_path);

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
                current_file: sanitized.to_string(),
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
        let clean = raw_name.replace('\\', "/");
        let trimmed = clean.trim_start_matches('/');
        if trimmed.is_empty() {
            return Ok(true);
        }

        let safe_path = PathBuf::from(trimmed);
        if safe_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Ok(true);
        }

        let out_path = dest_dir.join(&safe_path);

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
                    current_file: trimmed.to_string(),
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

#[tauri::command]
fn extract_archive(
    app: AppHandle,
    archive_path: String,
    destination: Option<String>,
    password: Option<String>,
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

    let pwd_ref = password.as_deref();
    let extracted_files = universal_extract(path, &dest_dir, pwd_ref, Some(&app))?;

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
    let p = Path::new(archive_path);
    if !p.exists() {
        return Err("Archive file does not exist".into());
    }
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("extracted");
    let dest = parent.join(stem);
    let _ = std::fs::create_dir_all(&dest);
    universal_extract(p, &dest, None, None)?;

    #[cfg(target_os = "windows")]
    {
        let mut refresh_cmd = Command::new("cmd");
        refresh_cmd.args(["/C", "ie4uinit.exe -show"]);
        refresh_cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = refresh_cmd.output();
    }
    Ok(())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    {
        // Auto-register and sync context menu in background on startup
        std::thread::spawn(|| {
            let _ = register_context_menu_sync();
        });
    }

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

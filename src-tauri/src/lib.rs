use sysinfo::System;
use serde::Serialize;
use std::process::Command;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

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

#[tauri::command]
async fn register_context_menu() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            if let Ok(exe_path) = std::env::current_exe() {
                let exe_str = exe_path.to_string_lossy().to_string();
                let cmd_str = format!("\"{}\" \"%1\"", exe_str);

                // Write a temporary .reg file to import all keys silently in one atomic operation
                let reg_content = format!(
                    "Windows Registry Editor Version 5.00\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\*\\shell\\Bayzip]\r\n\
                    @=\"Compress with Bayzip\"\r\n\
                    \"Icon\"=\"{}\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\*\\shell\\Bayzip\\command]\r\n\
                    @=\"{}\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Directory\\shell\\Bayzip]\r\n\
                    @=\"Compress with Bayzip\"\r\n\
                    \"Icon\"=\"{}\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\Directory\\shell\\Bayzip\\command]\r\n\
                    @=\"{}\"\r\n",
                    exe_str.replace('\\', "\\\\").replace('"', "\\\""),
                    cmd_str.replace('\\', "\\\\").replace('"', "\\\""),
                    exe_str.replace('\\', "\\\\").replace('"', "\\\""),
                    cmd_str.replace('\\', "\\\\").replace('"', "\\\""),
                );

                let temp_reg = std::env::temp_dir().join("bayzip_context.reg");
                if std::fs::write(&temp_reg, reg_content).is_ok() {
                    let mut cmd = Command::new("reg");
                    cmd.args(["import", &temp_reg.to_string_lossy()]);
                    cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = cmd.output();
                    let _ = std::fs::remove_file(temp_reg);
                    return Ok("Context menu registered successfully!".into());
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

#[derive(serde::Deserialize)]
pub struct CompressPayload {
    files: Vec<String>,
    format: String,
    algorithm: String,
    slider_value: i32,
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

#[tauri::command]
fn compress_files(app: AppHandle, payload: CompressPayload) -> Result<CompressResponse, String> {
    if payload.files.is_empty() {
        return Err("No files selected to compress".to_string());
    }

    let start_time = Instant::now();
    let first_path = Path::new(&payload.files[0]);
    let parent_dir = first_path.parent().unwrap_or_else(|| Path::new("."));
    
    let archive_name = if payload.files.len() == 1 {
        let stem = first_path.file_stem().and_then(|s| s.to_str()).unwrap_or("archive");
        format!("{}{}", stem, payload.format)
    } else {
        format!("bayzip_archive{}", payload.format)
    };
    
    let output_path = parent_dir.join(&archive_name);
    
    // Calculate total original size
    let mut total_original: u64 = 0;
    for file_str in &payload.files {
        if let Ok(meta) = std::fs::metadata(file_str) {
            total_original += meta.len();
        }
    }

    // Initial 0% progress event
    let _ = app.emit("compress_progress", CompressProgress {
        percent: 0.0,
        current_file: "Preparing files...".into(),
        bytes_processed: 0,
        total_bytes: total_original,
    });

    let zstd_level = match payload.slider_value {
        0 => 1,
        25 => 3,
        50 => 7,
        75 => 14,
        _ => 19,
    };

    let out_file = File::create(&output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    
    let mut encoder = zstd::stream::Encoder::new(out_file, zstd_level)
        .map_err(|e| format!("Failed to initialize compression encoder: {}", e))?;

    let _ = encoder.write_all(b"BAYZIP\x01");

    let mut bytes_processed: u64 = 0;

    for file_str in &payload.files {
        let p = Path::new(file_str);
        let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let filename_bytes = filename.as_bytes();
        
        let _ = encoder.write_all(&(filename_bytes.len() as u32).to_le_bytes());
        let _ = encoder.write_all(filename_bytes);

        if let Ok(mut f) = File::open(p) {
            let mut buffer = [0u8; 128 * 1024];
            loop {
                match f.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = encoder.write_all(&buffer[..n]);
                        bytes_processed += n as u64;
                        let pct = if total_original > 0 {
                            ((bytes_processed as f64 / total_original as f64) * 100.0).min(99.0)
                        } else {
                            50.0
                        };
                        let _ = app.emit("compress_progress", CompressProgress {
                            percent: pct,
                            current_file: filename.to_string(),
                            bytes_processed,
                            total_bytes: total_original,
                        });
                    }
                    Err(_) => break,
                }
            }
        }
    }

    encoder.finish().map_err(|e| format!("Failed to finalize compression: {}", e))?;

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
            compress_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

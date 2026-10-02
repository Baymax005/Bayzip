use sysinfo::System;
use serde::Serialize;
use std::process::Command;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;

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
fn register_context_menu() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            let exe_str = exe_path.to_string_lossy().to_string();
            let cmd_str = format!("\"{}\" \"%1\"", exe_str);
            
            // Add for all files (*)
            let _ = Command::new("reg")
                .args(["add", "HKCU\\Software\\Classes\\*\\shell\\Bayzip", "/ve", "/d", "Compress with Bayzip", "/f"])
                .output();
            let _ = Command::new("reg")
                .args(["add", "HKCU\\Software\\Classes\\*\\shell\\Bayzip", "/v", "Icon", "/d", &exe_str, "/f"])
                .output();
            let _ = Command::new("reg")
                .args(["add", "HKCU\\Software\\Classes\\*\\shell\\Bayzip\\command", "/ve", "/d", &cmd_str, "/f"])
                .output();

            // Add for directories (folders)
            let _ = Command::new("reg")
                .args(["add", "HKCU\\Software\\Classes\\Directory\\shell\\Bayzip", "/ve", "/d", "Compress with Bayzip", "/f"])
                .output();
            let _ = Command::new("reg")
                .args(["add", "HKCU\\Software\\Classes\\Directory\\shell\\Bayzip", "/v", "Icon", "/d", &exe_str, "/f"])
                .output();
            let _ = Command::new("reg")
                .args(["add", "HKCU\\Software\\Classes\\Directory\\shell\\Bayzip\\command", "/ve", "/d", &cmd_str, "/f"])
                .output();

            return Ok("Context menu registered successfully!".into());
        }
        Err("Could not determine executable path.".into())
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
fn compress_files(payload: CompressPayload) -> Result<CompressResponse, String> {
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
    
    let mut total_original: u64 = 0;
    for file_str in &payload.files {
        if let Ok(meta) = std::fs::metadata(file_str) {
            total_original += meta.len();
        }
    }

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

    for file_str in &payload.files {
        let p = Path::new(file_str);
        let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let filename_bytes = filename.as_bytes();
        
        let _ = encoder.write_all(&(filename_bytes.len() as u32).to_le_bytes());
        let _ = encoder.write_all(filename_bytes);

        if let Ok(mut f) = File::open(p) {
            let mut buffer = [0u8; 64 * 1024];
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
    }

    encoder.finish().map_err(|e| format!("Failed to finalize compression: {}", e))?;

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

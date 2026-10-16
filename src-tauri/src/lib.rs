pub mod types;
pub mod crypto;
pub mod priority;
pub mod registry;
pub mod inspector;
pub mod compressor;
pub mod extractor;

use std::path::Path;
use sysinfo::System;
use tauri::AppHandle;

pub use types::*;
pub use priority::ScopedPriorityGuard;
pub use registry::register_context_menu_sync;
pub use inspector::{detect_archive_type, inspect_archive, sanitize_archive_path, archive_stem_name};
pub use compressor::{headless_quick_compress, compress_files_sync, compute_path_size};
pub use extractor::{headless_extract_here, headless_extract_to_subfolder, universal_extract, extract_archive_sync};

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
fn get_cli_args() -> StartupCommand {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut action = None;
    let mut paths = Vec::new();

    for arg in args {
        let clean = arg.trim_matches('\"').to_string();
        if clean == "--gui-compress" {
            action = Some("compress".to_string());
        } else if clean == "--gui-extract" || clean == "--extract-here" || clean == "--extract-to-folder" {
            action = Some("extract".to_string());
        } else if !clean.starts_with("--") {
            paths.push(clean);
        }
    }

    StartupCommand { action, paths }
}

#[tauri::command]
async fn register_context_menu() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        register_context_menu_sync()
    })
    .await
    .map_err(|e| format!("Registry sync error: {}", e))?
}

#[tauri::command]
async fn get_selected_items_info(paths: Vec<String>) -> Result<Vec<SelectedItemInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| format!("Query error: {}", e))
}

#[tauri::command]
async fn compress_files(app: AppHandle, payload: CompressPayload) -> Result<CompressResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        compress_files_sync(app, payload)
    })
    .await
    .map_err(|e| format!("Compression task error: {}", e))?
}

#[tauri::command]
async fn extract_archive(
    app: AppHandle,
    archive_path: String,
    destination: Option<String>,
    password: Option<String>,
    expected_size: Option<u64>,
) -> Result<ExtractResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        extract_archive_sync(app, archive_path, destination, password, expected_size)
    })
    .await
    .map_err(|e| format!("Extraction task error: {}", e))?
}

#[tauri::command]
async fn get_archive_info(path: String, password: Option<String>) -> Result<ArchiveInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        inspect_archive(&path, password.as_deref())
    })
    .await
    .map_err(|e| format!("Archive inspect error: {}", e))?
}

#[tauri::command]
async fn preview_archive_entry(
    archive_path: String,
    entry_name: String,
    password: Option<String>,
) -> Result<ArchiveEntryPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        inspector::preview_archive_entry_sync(&archive_path, &entry_name, password.as_deref())
    })
    .await
    .map_err(|e| format!("Preview task error: {}", e))?
}

#[tauri::command]
fn cancel_compression() {
    crate::compressor::cancel_compression_flag();
}

#[tauri::command]
fn cancel_extraction() {
    crate::extractor::cancel_extraction_flag();
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
            get_system_specs, 
            get_cli_args, 
            register_context_menu,
            compress_files,
            cancel_compression,
            cancel_extraction,
            get_selected_items_info,
            extract_archive,
            get_archive_info,
            preview_archive_entry
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

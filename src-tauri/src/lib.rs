pub mod types;
pub mod crypto;
pub mod priority;
pub mod registry;
pub mod inspector;
pub mod compressor;
pub mod extractor;
pub mod tester;

use std::path::Path;
use sysinfo::System;
use tauri::{AppHandle, Emitter};

pub use types::*;
pub use priority::ScopedPriorityGuard;
pub use registry::{register_context_menu_sync, unregister_context_menu_sync};
pub use inspector::{detect_archive_type, inspect_archive, sanitize_archive_path, archive_stem_name, has_instant_bay_toc, safe_atomic_rename, get_single_root_dir, clear_readonly};
pub use compressor::{headless_quick_compress, headless_quick_compress_targets, compress_files_sync, compute_path_size};
pub use extractor::{headless_extract_here, headless_extract_to_subfolder, universal_extract, extract_archive_sync};
pub use tester::{test_archive_sync, cancel_test_flag};

#[tauri::command]
async fn get_file_size(path: String) -> u64 {
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    })
    .await
    .unwrap_or(0)
}

#[tauri::command]
fn get_background_priority_shield() -> bool {
    priority::is_background_shield_enabled()
}

#[tauri::command]
fn set_background_priority_shield(enabled: bool) {
    priority::set_background_shield(enabled);
}

#[tauri::command]
fn get_smart_media_bypass() -> bool {
    compressor::utils::is_smart_bypass_enabled()
}

#[tauri::command]
fn set_smart_media_bypass(app: AppHandle, enabled: bool) {
    compressor::utils::set_smart_bypass(enabled);
    let _ = app.emit("smart_media_bypass_changed", enabled);
}

pub fn collect_system_specs() -> SystemSpecs {
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
async fn get_system_specs() -> SystemSpecs {
    tauri::async_runtime::spawn_blocking(collect_system_specs)
        .await
        .unwrap_or_else(|_| SystemSpecs {
            total_ram_gb: 8.0,
            free_ram_gb: 4.0,
            cpu_cores: 4,
        })
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
async fn unregister_context_menu() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        unregister_context_menu_sync()
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
    extract_to_subfolder: Option<bool>,
    excluded_paths: Option<Vec<String>>,
) -> Result<ExtractResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        extract_archive_sync(app, archive_path, destination, password, expected_size, extract_to_subfolder, excluded_paths)
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
async fn inspect_staged_items(paths: Vec<String>) -> Result<Vec<ArchiveEntryItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        inspector::inspect_staged_items_sync(&paths)
    })
    .await
    .map_err(|e| format!("Staged items inspection error: {}", e))?
}

#[tauri::command]
async fn preview_staged_file(path: String) -> Result<ArchiveEntryPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        inspector::preview_staged_file_sync(&path)
    })
    .await
    .map_err(|e| format!("Staged file preview error: {}", e))?
}

#[tauri::command]
fn cancel_compression() {
    crate::compressor::cancel_compression_flag();
}

#[tauri::command]
fn cancel_extraction() {
    crate::extractor::cancel_extraction_flag();
}

#[tauri::command]
async fn test_archive(
    app: AppHandle,
    archive_path: String,
    password: Option<String>,
) -> Result<TestArchiveResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        tester::test_archive_sync(app, archive_path, password)
    })
    .await
    .map_err(|e| format!("Archive testing task panicked: {}", e))?
}

#[tauri::command]
fn cancel_test_archive() {
    crate::tester::cancel_test_flag();
}

#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        Ok(())
    } else {
        let window = tauri::WebviewWindowBuilder::new(
            &app,
            "settings",
            tauri::WebviewUrl::App("index.html?window=settings".into()),
        )
        .title("Bayzip Settings")
        .inner_size(580.0, 560.0)
        .resizable(false)
        .decorations(false)
        .shadow(true)
        .center()
        .build()
        .map_err(|e| format!("Failed to create settings window: {}", e))?;

        let _ = window.set_focus();
        Ok(())
    }
}

#[tauri::command]
async fn close_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
async fn minimize_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.minimize();
    }
    Ok(())
}

#[tauri::command]
async fn get_file_icon(extension: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(inspector::get_cached_icon(&extension))
    })
    .await
    .map_err(|e| format!("Icon extraction failed: {}", e))?
}

#[tauri::command]
async fn get_file_icons(extensions: Vec<String>) -> Result<std::collections::HashMap<String, String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(inspector::get_cached_icons_batch(extensions))
    })
    .await
    .map_err(|e| format!("Batch icon extraction failed: {}", e))?
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
            unregister_context_menu,
            compress_files,
            cancel_compression,
            cancel_extraction,
            test_archive,
            cancel_test_archive,
            get_selected_items_info,
            extract_archive,
            get_archive_info,
            get_file_size,
            preview_archive_entry,
            inspect_staged_items,
            preview_staged_file,
            get_file_icon,
            get_file_icons,
            get_background_priority_shield,
            set_background_priority_shield,
            get_smart_media_bypass,
            set_smart_media_bypass,
            open_settings_window,
            close_settings_window,
            minimize_settings_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_system_specs() {
        let specs = collect_system_specs();
        assert!(specs.cpu_cores >= 1);
        assert!(specs.total_ram_gb > 0.0);
        assert!(specs.free_ram_gb >= 0.0);
    }
}

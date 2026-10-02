use sysinfo::System;
use serde::Serialize;
use std::process::Command;

#[derive(Serialize)]
struct SystemSpecs {
    total_ram_gb: f64,
    free_ram_gb: f64,
    cpu_cores: usize,
}

#[tauri::command]
fn get_system_specs() -> SystemSpecs {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let total_ram_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let free_ram_gb = sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    
    SystemSpecs {
        total_ram_gb,
        free_ram_gb,
        cpu_cores: sys.cpus().len(),
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

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, get_system_specs, get_cli_args, register_context_menu])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

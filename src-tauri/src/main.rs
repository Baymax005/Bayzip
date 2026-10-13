// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == "--quick-compress" {
        let format = &args[2];
        let target_file = args[3].trim_matches('\"');
        let _ = tauri_app_lib::headless_quick_compress(format, target_file);
        return;
    }
    if args.len() >= 3 && args[1] == "--extract-here" {
        let target_file = args[2].trim_matches('\"');
        if let Err(_) = tauri_app_lib::headless_extract_here(target_file) {
            // If headless extraction fails (e.g. archive is password protected), open the GUI
            tauri_app_lib::run();
            return;
        }
        return;
    }
    if args.len() >= 3 && args[1] == "--extract-to-folder" {
        let target_file = args[2].trim_matches('\"');
        if let Err(_) = tauri_app_lib::headless_extract_to_subfolder(target_file) {
            // If headless extraction fails (e.g. archive is password protected), open the GUI
            tauri_app_lib::run();
            return;
        }
        return;
    }
    tauri_app_lib::run();
}

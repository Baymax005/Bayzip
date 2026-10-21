// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == "--quick-compress" {
        let format = &args[2];
        let targets: Vec<&str> = args[3..].iter().map(|s| s.trim_matches('\"')).collect();
        let _ = bayzip_lib::headless_quick_compress_targets(format, &targets);
        return;
    }
    if args.len() >= 3 && args[1] == "--extract-here" {
        for target_file in &args[2..] {
            let target_clean = target_file.trim_matches('\"');
            if bayzip_lib::headless_extract_here(target_clean).is_err() {
                // If headless extraction fails (e.g. archive is password protected), open the GUI
                bayzip_lib::run();
                return;
            }
        }
        return;
    }
    if args.len() >= 3 && args[1] == "--extract-to-folder" {
        for target_file in &args[2..] {
            let target_clean = target_file.trim_matches('\"');
            if bayzip_lib::headless_extract_to_subfolder(target_clean).is_err() {
                // If headless extraction fails (e.g. archive is password protected), open the GUI
                bayzip_lib::run();
                return;
            }
        }
        return;
    }
    bayzip_lib::run();
}

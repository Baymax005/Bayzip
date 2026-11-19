use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::AppHandle;
#[cfg(not(test))]
use tauri::Emitter;

#[cfg(not(test))]
use crate::types::ExtractProgress;
use crate::inspector::clear_readonly;
use super::is_extraction_cancelled;

pub use crate::inspector::is_path_excluded;

pub fn cleanup_extracted_files(files: &[String]) {
    for file_str in files.iter().rev() {
        let p = Path::new(file_str);
        if p.is_file() {
            clear_readonly(p);
            let _ = std::fs::remove_file(p);
        } else if p.is_dir() {
            let _ = std::fs::remove_dir(p);
        }
        let part_path = {
            let mut s = p.as_os_str().to_os_string();
            s.push(".baypart");
            PathBuf::from(s)
        };
        if part_path.exists() {
            clear_readonly(&part_path);
            let _ = std::fs::remove_file(&part_path);
        }
    }
}

pub fn base_folder_stem(name: &str) -> &str {
    if let Some(pos) = name.rfind(" (") {
        if name.ends_with(')') {
            return &name[..pos];
        }
    }
    name
}

pub fn strip_root_component<'a>(path: &'a Path, root_name: &str) -> Option<&'a Path> {
    let mut components = path.components();
    if let Some(std::path::Component::Normal(first)) = components.next() {
        if first.to_string_lossy().eq_ignore_ascii_case(root_name) {
            return Some(components.as_path());
        }
    }
    None
}

pub fn resolve_unique_folder_name(target_dir: &Path) -> PathBuf {
    let Some(name) = target_dir.file_name().and_then(|s| s.to_str()) else {
        return target_dir.to_path_buf();
    };
    if !target_dir.exists() {
        return target_dir.to_path_buf();
    }
    let parent = target_dir.parent().unwrap_or_else(|| Path::new("."));
    let stem = base_folder_stem(name);
    let mut counter = 1;
    loop {
        let candidate = parent.join(format!("{} ({})", stem, counter));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

pub fn emit_extract_progress(
    app: Option<&AppHandle>,
    last_emit: &mut Instant,
    current_file: &str,
    total_extracted: u64,
    total_expected_bytes: u64,
) {
    #[cfg(not(test))]
    if let Some(app_handle) = app {
        if last_emit.elapsed().as_millis() >= 50 {
            *last_emit = Instant::now();
            let pct = if total_expected_bytes > 0 {
                ((total_extracted as f64 / total_expected_bytes as f64) * 100.0).min(99.0)
            } else {
                50.0
            };
            let _ = app_handle.emit("extract_progress", ExtractProgress {
                percent: pct,
                current_file: current_file.to_string(),
                bytes_extracted: total_extracted,
            });
        }
    }
    #[cfg(test)]
    let _ = (app, last_emit, current_file, total_extracted, total_expected_bytes);
}

pub fn should_abort_fallback(err: &str) -> bool {
    let lower = err.to_lowercase();
    is_extraction_cancelled()
        || lower.contains("cancelled")
        || lower.contains("canceled")
        || lower.contains("password")
        || lower.contains("decrypt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_extracted_files_safely_removes_created_files() {
        let test_dir = std::env::current_dir().unwrap().join("target").join("test_extract_cleanup");
        let _ = std::fs::create_dir_all(&test_dir);
        let file1 = test_dir.join("test_file.txt");
        let file2 = test_dir.join("test_part.txt.baypart");
        std::fs::write(&file1, b"hello").unwrap();
        std::fs::write(&file2, b"partial").unwrap();

        let files = vec![
            file1.to_string_lossy().to_string(),
            test_dir.join("test_part.txt").to_string_lossy().to_string(),
        ];

        cleanup_extracted_files(&files);

        assert!(!file1.exists());
        assert!(!file2.exists());
        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_should_abort_fallback_on_cancel_and_password() {
        assert!(should_abort_fallback("Extraction cancelled by user"));
        assert!(should_abort_fallback("Operation canceled"));
        assert!(should_abort_fallback("Incorrect password: Could not decrypt file"));
        assert!(should_abort_fallback("Failed to decrypt 7z header"));
        assert!(!should_abort_fallback("Invalid zip archive header"));
    }
}

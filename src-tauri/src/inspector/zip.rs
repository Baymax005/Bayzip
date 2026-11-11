use std::fs::File;
use std::io::Read;
use std::path::Path;
use crate::types::ArchiveEntryItem;

pub fn list_zip_entries(p: &Path, _password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed: u64 = 0;
    if let Ok(file) = File::open(p) {
        if let Ok(mut zip) = zip::ZipArchive::new(file) {
            for i in 0..zip.len() {
                if let Ok(entry) = zip.by_index_raw(i) {
                    let clean = entry.name().replace('\\', "/");
                    let name = clean.trim_start_matches('/').to_string();
                    if !name.is_empty() {
                        let size = entry.size();
                        let is_dir = entry.is_dir() || clean.ends_with('/');
                        total_uncompressed = total_uncompressed.saturating_add(size);
                        items.push(ArchiveEntryItem { name, size, is_dir });
                    }
                }
            }
        }
    }
    let count = items.len();
    (items, count, total_uncompressed)
}

pub fn preview_zip_entry_bytes(path: &Path, entry_name: &str, password: Option<&str>) -> Result<(Vec<u8>, u64), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip archive: {}", e))?;
    let target = entry_name.replace('\\', "/").trim_start_matches('/').to_string();
    let max_preview = 15 * 1024 * 1024;

    for i in 0..zip.len() {
        let name_match = {
            if let Ok(entry) = zip.by_index_raw(i) {
                let clean = entry.name().replace('\\', "/").trim_start_matches('/').to_string();
                clean == target
            } else {
                false
            }
        };

        if name_match {
            let mut entry = match password {
                Some(pwd) if !pwd.trim().is_empty() => {
                    zip.by_index_decrypt(i, pwd.trim().as_bytes())
                        .map_err(|e| format!("Password decryption failed: {}", e))?
                }
                _ => {
                    zip.by_index(i).map_err(|e| {
                        if e.to_string().contains("password") || e.to_string().contains("Password") {
                            "Password required for encrypted ZIP archive".to_string()
                        } else {
                            format!("Failed to open zip entry: {}", e)
                        }
                    })?
                }
            };

            if entry.is_dir() {
                return Err("Cannot preview a folder".into());
            }

            let file_size = entry.size();
            let to_read = (file_size as usize).min(max_preview);
            let mut buf = vec![0u8; to_read];
            entry.read_exact(&mut buf).map_err(|e| format!("Failed to read zip entry data: {}", e))?;
            return Ok((buf, file_size));
        }
    }

    Err(format!("Entry '{}' not found in zip archive", entry_name))
}

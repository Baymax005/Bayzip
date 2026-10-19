use std::fs::File;
use std::io::Read;
use std::path::Path;
use crate::types::ArchiveEntryItem;

pub fn list_tar_entries(p: &Path) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed = 0;
    let is_gz = p.to_string_lossy().to_lowercase().ends_with(".gz") || p.to_string_lossy().to_lowercase().ends_with(".tgz");
    if let Ok(file) = File::open(p) {
        if is_gz {
            let gz = flate2::read::GzDecoder::new(file);
            let mut archive = ::tar::Archive::new(gz);
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
            let mut archive = ::tar::Archive::new(file);
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

pub fn preview_tar_entry_bytes(path: &Path, entry_name: &str) -> Result<(Vec<u8>, u64), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let target = entry_name.replace('\\', "/").trim_start_matches('/').to_string();
    let max_preview = 15 * 1024 * 1024;
    let is_gz = path.to_string_lossy().to_lowercase().ends_with(".gz") || path.to_string_lossy().to_lowercase().ends_with(".tgz");

    if is_gz {
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = ::tar::Archive::new(gz);
        let entries = archive.entries().map_err(|e| format!("Failed to read tar.gz entries: {}", e))?;
        for entry in entries.flatten() {
            if let Ok(p) = entry.path() {
                let clean = p.to_string_lossy().replace('\\', "/").trim_start_matches('/').to_string();
                if clean == target {
                    if entry.header().entry_type().is_dir() {
                        return Err("Cannot preview a folder".into());
                    }
                    let size = entry.size();
                    let to_read = (size as usize).min(max_preview);
                    let mut buf = vec![0u8; to_read];
                    let mut reader = entry;
                    reader.read_exact(&mut buf).map_err(|e| format!("Failed to read tar entry: {}", e))?;
                    return Ok((buf, size));
                }
            }
        }
    } else {
        let mut archive = ::tar::Archive::new(file);
        let entries = archive.entries().map_err(|e| format!("Failed to read tar entries: {}", e))?;
        for entry in entries.flatten() {
            if let Ok(p) = entry.path() {
                let clean = p.to_string_lossy().replace('\\', "/").trim_start_matches('/').to_string();
                if clean == target {
                    if entry.header().entry_type().is_dir() {
                        return Err("Cannot preview a folder".into());
                    }
                    let size = entry.size();
                    let to_read = (size as usize).min(max_preview);
                    let mut buf = vec![0u8; to_read];
                    let mut reader = entry;
                    reader.read_exact(&mut buf).map_err(|e| format!("Failed to read tar entry: {}", e))?;
                    return Ok((buf, size));
                }
            }
        }
    }

    Err(format!("Entry '{}' not found in tar archive", entry_name))
}

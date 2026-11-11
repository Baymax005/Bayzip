use std::fs::File;
use std::path::Path;
use sevenz_rust2::Password;
use crate::types::ArchiveEntryItem;

pub fn list_7z_entries(p: &Path, password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed: u64 = 0;
    if let Ok(file) = File::open(p) {
        let pwd = password.map(Password::from).unwrap_or_else(Password::empty);
        if let Ok(sz) = sevenz_rust2::ArchiveReader::new(file, pwd) {
            for entry in sz.archive().files.iter() {
                let raw_name = entry.name();
                let clean = raw_name.replace('\\', "/");
                let trimmed = clean.trim_start_matches('/');
                if trimmed.is_empty() {
                    continue;
                }
                let size = entry.size();
                let is_dir = entry.is_directory() || trimmed.ends_with('/');
                total_uncompressed = total_uncompressed.saturating_add(size);
                items.push(ArchiveEntryItem {
                    name: trimmed.to_string(),
                    size,
                    is_dir,
                });
            }
        }
    }
    let count = items.len();
    (items, count, total_uncompressed)
}

pub fn preview_7z_entry_bytes(path: &Path, entry_name: &str, password: Option<&str>) -> Result<(Vec<u8>, u64), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let pwd = password.map(|s| Password::from(s.trim())).unwrap_or_else(Password::empty);
    let mut sz = sevenz_rust2::ArchiveReader::new(file, pwd)
        .map_err(|e| format!("Failed to open 7z archive: {}", e))?;
    let target = entry_name.replace('\\', "/").trim_start_matches('/').to_string();
    let max_preview = 15 * 1024 * 1024;

    let mut result: Option<(Vec<u8>, u64)> = None;

    sz.for_each_entries(|entry, reader| {
        let clean = entry.name().replace('\\', "/").trim_start_matches('/').to_string();
        if clean == target {
            if entry.is_directory() {
                return Ok(false);
            }
            let size = entry.size();
            let to_read = (size as usize).min(max_preview);
            let mut buf = vec![0u8; to_read];
            if let Err(e) = reader.read_exact(&mut buf) {
                return Err(sevenz_rust2::Error::from(e));
            }
            result = Some((buf, size));
            return Ok(false); // Stop iterating
        }
        Ok(true)
    }).map_err(|e| format!("7z reading error: {}", e))?;

    match result {
        Some(res) => Ok(res),
        None => Err(format!("Entry '{}' not found in 7z archive", entry_name)),
    }
}

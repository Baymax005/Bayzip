use crate::types::{DriveInfo, QuickAccessItem};

pub fn get_quick_access_sync() -> Vec<QuickAccessItem> {
    let mut list = Vec::new();
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let p = std::path::Path::new(&user_profile);
        let shortcuts = [
            ("Desktop", "Desktop", "🖥️"),
            ("Downloads", "Downloads", "📥"),
            ("Documents", "Documents", "📑"),
            ("Pictures", "Pictures", "🖼️"),
            ("Music", "Music", "🎵"),
            ("Videos", "Videos", "🎬"),
        ];
        for (name, folder, icon) in shortcuts {
            let primary = p.join(folder);
            let onedrive = p.join("OneDrive").join(folder);
            let valid_path = if primary.exists() {
                Some(primary)
            } else if onedrive.exists() {
                Some(onedrive)
            } else {
                None
            };

            if let Some(path) = valid_path {
                list.push(QuickAccessItem {
                    name: name.to_string(),
                    path: path.to_string_lossy().to_string(),
                    icon: icon.to_string(),
                });
            }
        }
    }
    list
}

pub fn get_system_drives_sync() -> Vec<DriveInfo> {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    let mut drives: Vec<DriveInfo> = disks
        .iter()
        .map(|disk| {
            let mount = disk.mount_point().to_string_lossy().to_string();
            let name = disk.name().to_string_lossy().to_string();
            let is_removable = disk.is_removable();
            DriveInfo {
                mount_point: mount,
                name: if name.is_empty() {
                    "Local Disk".to_string()
                } else {
                    name
                },
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                is_removable,
            }
        })
        .collect();

    drives.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    drives
}

pub fn browse_directory_sync(dir_path: &str) -> Result<Vec<crate::types::DirectoryItem>, String> {
    let mut clean = dir_path.trim().to_string();
    if clean.len() == 2 && clean.ends_with(':') {
        clean.push('\\');
    }

    let mut p = std::path::PathBuf::from(&clean);

    // Expand ~ prefix to USERPROFILE
    if clean.starts_with('~') {
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let rest = clean.trim_start_matches('~').trim_start_matches(['/', '\\']);
            p = std::path::Path::new(&user_profile).join(rest);
        }
    }

    // Smart alias resolution for user library folders if path does not exist as typed
    if !p.exists() {
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let up = std::path::Path::new(&user_profile);
            let lower = clean.to_lowercase().replace('/', "\\");
            let target_name = if lower == "downloads" || lower == "c:\\downloads" || lower == "c:downloads" {
                Some("Downloads")
            } else if lower == "desktop" || lower == "c:\\desktop" || lower == "c:desktop" {
                Some("Desktop")
            } else if lower == "documents" || lower == "c:\\documents" || lower == "c:documents" {
                Some("Documents")
            } else if lower == "pictures" || lower == "c:\\pictures" || lower == "c:pictures" {
                Some("Pictures")
            } else if lower == "music" || lower == "c:\\music" || lower == "c:music" {
                Some("Music")
            } else if lower == "videos" || lower == "c:\\videos" || lower == "c:videos" {
                Some("Videos")
            } else {
                None
            };

            if let Some(folder_name) = target_name {
                let candidate1 = up.join(folder_name);
                let candidate2 = up.join("OneDrive").join(folder_name);
                if candidate1.exists() {
                    p = candidate1;
                } else if candidate2.exists() {
                    p = candidate2;
                }
            }
        }
    }

    if !p.exists() {
        return Err(format!("Path does not exist: {}", dir_path));
    }
    let rd = match std::fs::read_dir(&p) {
        Ok(entries) => entries,
        Err(e) => return Err(format!("Failed to read directory: {}", e)),
    };

    let mut items = Vec::new();
    for entry in rd.flatten() {
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry_path.is_dir();
        let size = if is_dir {
            0
        } else {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        };
        items.push(crate::types::DirectoryItem {
            name,
            path: entry_path.to_string_lossy().to_string(),
            is_dir,
            size,
        });
    }

    items.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(items)
}

pub fn delete_to_recycle_bin_sync(path_str: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::{
            SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FO_DELETE,
            SHFILEOPSTRUCTW,
        };

        let path = std::path::Path::new(path_str);
        if !path.exists() {
            return Err("File or folder does not exist".to_string());
        }

        // SHFileOperation requires double null-terminated wide string (path\0\0)
        let mut wide: Vec<u16> = std::ffi::OsStr::new(path_str).encode_wide().collect();
        wide.push(0);
        wide.push(0);

        let mut op = SHFILEOPSTRUCTW {
            hwnd: HWND(std::ptr::null_mut()),
            wFunc: FO_DELETE,
            pFrom: PCWSTR(wide.as_ptr()),
            pTo: PCWSTR::null(),
            fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT).0 as u16,
            fAnyOperationsAborted: windows::Win32::Foundation::BOOL(0),
            hNameMappings: std::ptr::null_mut(),
            lpszProgressTitle: PCWSTR::null(),
        };

        let result = unsafe { SHFileOperationW(&mut op) };
        if result == 0 && !op.fAnyOperationsAborted.as_bool() {
            Ok(())
        } else {
            Err(format!(
                "Failed to send to Recycle Bin (error code: {})",
                result
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let path = std::path::Path::new(path_str);
        if path.is_dir() {
            std::fs::remove_dir_all(path).map_err(|e| e.to_string())
        } else {
            std::fs::remove_file(path).map_err(|e| e.to_string())
        }
    }
}

pub fn validate_file_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if trimmed.ends_with('.') || trimmed.ends_with(' ') {
        return Err("Name cannot end with a period or space on Windows".to_string());
    }
    const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    if trimmed.chars().any(|c| INVALID_CHARS.contains(&c) || (c as u32) < 32) {
        return Err("Name contains invalid characters (< > : \" / \\ | ? *)".to_string());
    }
    let stem = trimmed.split('.').next().unwrap_or(trimmed).to_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return Err(format!("'{}' is a reserved system name on Windows", stem));
    }
    Ok(())
}

pub fn create_directory_sync(path_str: &str) -> Result<(), String> {
    let path = std::path::Path::new(path_str);
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        validate_file_name(name)?;
    }
    std::fs::create_dir_all(path).map_err(|e| e.to_string())
}

pub fn rename_path_sync(old_path_str: &str, new_path_str: &str) -> Result<(), String> {
    let old_p = std::path::Path::new(old_path_str);
    let new_p = std::path::Path::new(new_path_str);
    if !old_p.exists() {
        return Err("Source file or folder does not exist".to_string());
    }
    if let Some(new_name) = new_p.file_name().and_then(|s| s.to_str()) {
        validate_file_name(new_name)?;
    }
    if let Err(_err) = std::fs::rename(old_p, new_p) {
        // Cross-device volume move fallback (e.g. across C:\ and D:\)
        if old_p.is_dir() {
            copy_dir_recursive(old_p, new_p).map_err(|e| format!("Cross-device directory move failed: {}", e))?;
            let _ = std::fs::remove_dir_all(old_p);
            Ok(())
        } else {
            std::fs::copy(old_p, new_p).map_err(|e| format!("Cross-device file move failed: {}", e))?;
            let _ = std::fs::remove_file(old_p);
            Ok(())
        }
    } else {
        Ok(())
    }
}

pub fn copy_path_sync(src_str: &str, dest_parent_dir: &str) -> Result<String, String> {
    if crate::compressor::is_compression_cancelled() || crate::extractor::is_extraction_cancelled() {
        return Err("Operation cancelled by user".to_string());
    }
    let src = std::path::Path::new(src_str);
    if !src.exists() {
        return Err("Source does not exist".to_string());
    }
    let dest_dir = std::path::Path::new(dest_parent_dir);
    if !dest_dir.is_dir() {
        return Err("Destination is not a directory".to_string());
    }
    let file_name = src.file_name().ok_or_else(|| "Invalid source filename".to_string())?;
    let mut target = dest_dir.join(file_name);

    if target.exists() {
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = src.extension().and_then(|s| s.to_str());
        let mut counter = 1;
        loop {
            let new_name = match ext {
                Some(e) => format!("{} - Copy ({}).{}", stem, counter, e),
                None => format!("{} - Copy ({})", stem, counter),
            };
            let cand = dest_dir.join(&new_name);
            if !cand.exists() {
                target = cand;
                break;
            }
            counter += 1;
        }
    }

    if src.is_dir() {
        copy_dir_recursive(src, &target)?;
    } else {
        std::fs::copy(src, &target).map_err(|e| e.to_string())?;
    }

    Ok(target.to_string_lossy().to_string())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    if crate::compressor::is_compression_cancelled() || crate::extractor::is_extraction_cancelled() {
        return Err("Operation cancelled by user".to_string());
    }
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        if crate::compressor::is_compression_cancelled() || crate::extractor::is_extraction_cancelled() {
            return Err("Operation cancelled by user".to_string());
        }
        let entry_path = entry.path();
        let entry_target = dst.join(entry.file_name());
        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &entry_target)?;
        } else {
            std::fs::copy(&entry_path, &entry_target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}


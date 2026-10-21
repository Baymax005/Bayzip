use crate::types::DriveInfo;

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

pub fn create_directory_sync(path_str: &str) -> Result<(), String> {
    let path = std::path::Path::new(path_str);
    std::fs::create_dir_all(path).map_err(|e| e.to_string())
}

pub fn rename_path_sync(old_path_str: &str, new_path_str: &str) -> Result<(), String> {
    let old_p = std::path::Path::new(old_path_str);
    let new_p = std::path::Path::new(new_path_str);
    if !old_p.exists() {
        return Err("Source file or folder does not exist".to_string());
    }
    std::fs::rename(old_p, new_p).map_err(|e| e.to_string())
}

pub fn copy_path_sync(src_str: &str, dest_parent_dir: &str) -> Result<String, String> {
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
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
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


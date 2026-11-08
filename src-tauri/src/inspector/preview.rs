use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::types::{ArchiveEntryPreview, ArchiveFormat};
use super::format::detect_archive_type;
use super::bay::preview_bay_entry_bytes;
use super::zip::preview_zip_entry_bytes;
use super::sevenz::preview_7z_entry_bytes;
use super::tar::preview_tar_entry_bytes;
use super::rar::preview_rar_entry_bytes;

pub fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

pub const RESTRICTED_EXECUTABLE_EXTS: &[&str] = &[
    "exe", "dll", "sys", "msi", "com", "scr", "pif", "cpl", "hta", "drv", "ocx"
];

pub const RESTRICTED_SCRIPT_EXTS: &[&str] = &[
    "bat", "cmd", "vbs", "vbe", "wsf", "wsh", "ps1", "ps1xml", "ps2", "psc1", "reg"
];

pub const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "ico", "svg"
];

pub const TEXT_CODE_EXTS: &[&str] = &[
    "txt", "md", "markdown", "json", "yaml", "yml", "toml", "xml", "html", "htm",
    "css", "scss", "sass", "less", "js", "mjs", "cjs", "ts", "mts", "cts", "jsx",
    "tsx", "rs", "c", "cpp", "cc", "cxx", "h", "hpp", "cs", "go", "py", "rb", "php",
    "java", "kt", "kts", "swift", "sh", "bash", "zsh", "csv", "tsv", "log", "ini",
    "cfg", "conf", "env", "sql", "gitignore", "gitattributes", "editorconfig",
    "dockerfile", "r", "lua", "dart", "proto", "graphql", "properties"
];

pub fn preview_archive_entry_sync(
    archive_path: &str,
    entry_name: &str,
    password: Option<&str>,
) -> Result<ArchiveEntryPreview, String> {
    let p = Path::new(archive_path);
    if !p.exists() {
        return Err("Archive file not found".into());
    }

    let ext = entry_name.rsplit('.').next().unwrap_or("").to_lowercase();

    // Check restricted executables first
    if RESTRICTED_EXECUTABLE_EXTS.contains(&ext.as_str()) {
        return Ok(ArchiveEntryPreview {
            name: entry_name.to_string(),
            size: 0,
            preview_type: "restricted".to_string(),
            content: None,
            mime_type: None,
            is_restricted: true,
            warning: Some("Restricted Executable: Binaries (.exe, .dll, .sys, etc.) cannot be executed inside the preview viewer for system security.".to_string()),
        });
    }

    let detected = detect_archive_type(p);
    let (bytes, total_size) = match detected {
        ArchiveFormat::Bay => preview_bay_entry_bytes(p, entry_name, password)?,
        ArchiveFormat::Zip => preview_zip_entry_bytes(p, entry_name, password)?,
        ArchiveFormat::SevenZip => preview_7z_entry_bytes(p, entry_name, password)?,
        ArchiveFormat::Tar | ArchiveFormat::TarGz => preview_tar_entry_bytes(p, entry_name)?,
        ArchiveFormat::Rar | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd | ArchiveFormat::Iso | ArchiveFormat::Cab => preview_rar_entry_bytes(p, entry_name)?,
        ArchiveFormat::Unknown => {
            let lower = archive_path.to_lowercase();
            if lower.ends_with(".bay") || lower.ends_with(".bayg") {
                preview_bay_entry_bytes(p, entry_name, password)?
            } else if lower.ends_with(".zip") {
                preview_zip_entry_bytes(p, entry_name, password)?
            } else if lower.ends_with(".7z") {
                preview_7z_entry_bytes(p, entry_name, password)?
            } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                preview_tar_entry_bytes(p, entry_name)?
            } else if lower.ends_with(".rar") || lower.ends_with(".tar.bz2") || lower.ends_with(".bz2") || lower.ends_with(".tar.xz") || lower.ends_with(".xz") || lower.ends_with(".tar.zst") || lower.ends_with(".zst") || lower.ends_with(".iso") || lower.ends_with(".cab") {
                preview_rar_entry_bytes(p, entry_name)?
            } else {
                return Err("Unsupported archive format for preview".into());
            }
        }
    };

    // If restricted script (.bat, .cmd, .ps1, .vbs)
    if RESTRICTED_SCRIPT_EXTS.contains(&ext.as_str()) {
        let text = String::from_utf8_lossy(&bytes).to_string();
        return Ok(ArchiveEntryPreview {
            name: entry_name.to_string(),
            size: total_size,
            preview_type: "restricted".to_string(),
            content: Some(text),
            mime_type: Some("text/plain".to_string()),
            is_restricted: true,
            warning: Some("Execution Blocked: Shell scripts (.bat, .cmd, .ps1, .vbs) are displayed in safe read-only preview mode. Accidental execution is completely blocked.".to_string()),
        });
    }

    // Check image types
    if IMAGE_EXTS.contains(&ext.as_str()) {
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "ico" => "image/x-icon",
            "svg" => "image/svg+xml",
            _ => "image/png",
        };
        let data_url = format!("data:{};base64,{}", mime, base64_encode(&bytes));
        return Ok(ArchiveEntryPreview {
            name: entry_name.to_string(),
            size: total_size,
            preview_type: "image".to_string(),
            content: Some(data_url),
            mime_type: Some(mime.to_string()),
            is_restricted: false,
            warning: None,
        });
    }

    // Check text/code or UTF-8
    let is_text_ext = TEXT_CODE_EXTS.contains(&ext.as_str());
    let is_valid_utf8 = !bytes.is_empty() && !bytes[..bytes.len().min(512)].contains(&0) && std::str::from_utf8(&bytes).is_ok();

    if is_text_ext || is_valid_utf8 {
        let (content_str, warning) = if bytes.len() > 1_000_000 {
            let truncated = String::from_utf8_lossy(&bytes[..1_000_000]).to_string();
            (truncated, Some("Preview truncated to 1 MB for high performance.".to_string()))
        } else {
            (String::from_utf8_lossy(&bytes).to_string(), None)
        };

        return Ok(ArchiveEntryPreview {
            name: entry_name.to_string(),
            size: total_size,
            preview_type: "text".to_string(),
            content: Some(content_str),
            mime_type: Some("text/plain".to_string()),
            is_restricted: false,
            warning,
        });
    }

    // Other binary
    Ok(ArchiveEntryPreview {
        name: entry_name.to_string(),
        size: total_size,
        preview_type: "binary".to_string(),
        content: None,
        mime_type: None,
        is_restricted: false,
        warning: Some("Preview unavailable: Binary files require full extraction to view.".to_string()),
    })
}

pub fn preview_staged_file_sync(file_path: &str) -> Result<ArchiveEntryPreview, String> {
    let p = Path::new(file_path);
    if !p.exists() {
        return Err("File not found".into());
    }
    let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    let total_size = meta.len();

    // Check restricted executables first
    if RESTRICTED_EXECUTABLE_EXTS.contains(&ext.as_str()) {
        return Ok(ArchiveEntryPreview {
            name: file_name.to_string(),
            size: total_size,
            preview_type: "restricted".to_string(),
            content: None,
            mime_type: None,
            is_restricted: true,
            warning: Some("Restricted Executable: Binaries (.exe, .dll, .sys, etc.) cannot be executed inside the preview viewer for system security.".to_string()),
        });
    }

    // Limit read size to 10 MB for preview safety
    let max_read = (10 * 1024 * 1024).min(total_size as usize);
    let mut file = File::open(p).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut bytes = vec![0u8; max_read];
    let n = file.read(&mut bytes).map_err(|e| format!("Failed to read file: {}", e))?;
    bytes.truncate(n);

    // If restricted script (.bat, .cmd, .ps1, .vbs)
    if RESTRICTED_SCRIPT_EXTS.contains(&ext.as_str()) {
        let text = String::from_utf8_lossy(&bytes).to_string();
        return Ok(ArchiveEntryPreview {
            name: file_name.to_string(),
            size: total_size,
            preview_type: "restricted".to_string(),
            content: Some(text),
            mime_type: Some("text/plain".to_string()),
            is_restricted: true,
            warning: Some("Execution Blocked: Shell scripts (.bat, .cmd, .ps1, .vbs) are displayed in safe read-only preview mode. Accidental execution is completely blocked.".to_string()),
        });
    }

    // Check image types
    if IMAGE_EXTS.contains(&ext.as_str()) {
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "ico" => "image/x-icon",
            "svg" => "image/svg+xml",
            _ => "image/png",
        };
        let data_url = format!("data:{};base64,{}", mime, base64_encode(&bytes));
        return Ok(ArchiveEntryPreview {
            name: file_name.to_string(),
            size: total_size,
            preview_type: "image".to_string(),
            content: Some(data_url),
            mime_type: Some(mime.to_string()),
            is_restricted: false,
            warning: None,
        });
    }

    // Check text/code or valid UTF-8
    let is_text_ext = TEXT_CODE_EXTS.contains(&ext.as_str());
    let is_valid_utf8 = !bytes.is_empty() && !bytes[..bytes.len().min(512)].contains(&0) && std::str::from_utf8(&bytes).is_ok();

    if is_text_ext || is_valid_utf8 {
        let (content_str, warning) = if bytes.len() > 1_000_000 {
            let truncated = String::from_utf8_lossy(&bytes[..1_000_000]).to_string();
            (truncated, Some("Preview truncated to 1 MB for high performance.".to_string()))
        } else {
            (String::from_utf8_lossy(&bytes).to_string(), None)
        };

        return Ok(ArchiveEntryPreview {
            name: file_name.to_string(),
            size: total_size,
            preview_type: "text".to_string(),
            content: Some(content_str),
            mime_type: Some("text/plain".to_string()),
            is_restricted: false,
            warning,
        });
    }

    // Generic binary file
    Ok(ArchiveEntryPreview {
        name: file_name.to_string(),
        size: total_size,
        preview_type: "binary".to_string(),
        content: None,
        mime_type: Some("application/octet-stream".to_string()),
        is_restricted: false,
        warning: Some("Binary File: Direct preview is not supported for this file type.".to_string()),
    })
}

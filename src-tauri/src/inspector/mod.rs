use std::path::{Path, PathBuf};

pub mod hardware;
pub mod format;
pub mod bay;
pub mod zip;
pub mod sevenz;
pub mod tar;
pub mod rar;
pub mod preview;
pub mod staged;
pub mod icon;

pub use hardware::*;
pub use format::*;
pub use bay::*;
pub use zip::*;
pub use sevenz::*;
pub use tar::*;
pub use rar::*;
pub use preview::*;
pub use staged::*;
pub use icon::*;

use crate::types::{ArchiveEntryItem, ArchiveInfo, ArchiveFormat};

const DOS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    "CONIN$", "CONOUT$", "CLOCK$", "CONFIG$",
];

pub fn sanitize_archive_path(raw: &str) -> Option<PathBuf> {
    if raw.is_empty() {
        return None;
    }
    // Reject trailing periods and spaces anywhere on the path (Windows path normalization bypass)
    if raw.ends_with('.') || raw.ends_with(' ') {
        return None;
    }

    let clean = raw.replace('\\', "/");
    if clean.is_empty() || clean == "." {
        return None;
    }

    // Reject absolute paths, root paths, and UNC network paths
    if clean.starts_with('/') || clean.starts_with('\\') {
        return None;
    }

    // Reject drive prefix (e.g. C:, D:)
    if clean.len() >= 2 && clean.as_bytes()[1] == b':' && clean.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }

    let p = Path::new(&clean);
    let mut normalized = PathBuf::new();
    for component in p.components() {
        match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return None,
            std::path::Component::Normal(os_str) => {
                let s = os_str.to_string_lossy();
                // Block NTFS Alternate Data Streams (ADS), null bytes, colons, and illegal Windows characters (V-SEC-08)
                const INVALID_CHARS: &[char] = &[':', '\0', '<', '>', '"', '|', '?', '*'];
                if s.chars().any(|c| INVALID_CHARS.contains(&c) || (c as u32) < 32) {
                    return None;
                }
                // Reject trailing periods and spaces (Windows path normalization bypass, e.g. "malicious.exe .", "secret.txt.")
                if s.ends_with('.') || s.ends_with(' ') || s.starts_with(' ') {
                    return None;
                }
                // Check DOS reserved device names (e.g. CON, AUX, NUL, COM1-9, LPT1-9, CONIN$, CONOUT$, regardless of extension)
                let stem = s.split('.').next().unwrap_or("").trim();
                if DOS_RESERVED_NAMES.iter().any(|&r| stem.eq_ignore_ascii_case(r)) {
                    return None;
                }
                normalized.push(os_str);
            }
            std::path::Component::CurDir => {
                // Safely skip redundant '.' components
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return None;
    }
    Some(normalized)
}

pub fn archive_stem_name(p: &Path) -> String {
    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("extracted");
    let lower = filename.to_lowercase();

    let compound_exts = [
        ".tar.gz",
        ".tar.zst",
        ".tar.zstd",
        ".tar.bz2",
        ".tar.xz",
        ".tar.lz4",
        ".tar.br",
        ".bayg",
    ];

    for ext in compound_exts {
        if lower.ends_with(ext) {
            let cut_len = filename.len() - ext.len();
            return filename[..cut_len].to_string();
        }
    }

    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename)
        .to_string()
}

pub fn get_single_root_dir(entries: &[ArchiveEntryItem]) -> Option<String> {
    let mut candidate_root: Option<&str> = None;
    let mut non_empty_count = 0;

    for entry in entries {
        let clean = entry.name.trim().trim_start_matches(['/', '\\']).trim_start_matches("./");
        if clean.is_empty() || clean == "." {
            continue;
        }
        non_empty_count += 1;

        let first_segment = match clean.split(['/', '\\']).next() {
            Some(s) if !s.is_empty() => s,
            _ => return None,
        };

        // If it has no slash and is a file, then it is a top-level file (not a wrapped single root)
        if !clean.contains('/') && !clean.contains('\\') && !entry.is_dir {
            return None;
        }

        match candidate_root {
            None => candidate_root = Some(first_segment),
            Some(r) => {
                if !r.eq_ignore_ascii_case(first_segment) {
                    return None;
                }
            }
        }
    }

    if non_empty_count > 0 {
        candidate_root.map(|s| s.to_string())
    } else {
        None
    }
}

pub fn has_single_root_dir(entries: &[ArchiveEntryItem]) -> bool {
    get_single_root_dir(entries).is_some()
}

#[allow(clippy::permissions_set_readonly_false)]
pub fn clear_readonly(p: &Path) {
    if let Ok(metadata) = p.metadata() {
        let mut perms = metadata.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(p, perms);
        }
    }
}

pub fn safe_atomic_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        clear_readonly(to);
        if std::fs::rename(from, to).is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(to);
    }
    std::fs::rename(from, to)
}

/// Securely resolve Windows System32 executables to eliminate PATH hijacking risks (V-SEC-07).
pub fn get_system32_exe(exe_name: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let candidate = system_root.join("System32").join(exe_name);
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(exe_name)
}

pub fn inspect_archive(path: &str, password: Option<&str>) -> Result<ArchiveInfo, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err("File not found".into());
    }
    if p.is_dir() {
        return Err("Path is a directory, not an archive".into());
    }
    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("archive").to_string();
    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    
    let lower = path.to_lowercase();
    let detected = detect_archive_type(p);
    let is_enc = is_archive_encrypted(p);
    let pwd_ref = password;
    let pwd_clean = pwd_ref.map(|s| s.trim()).filter(|s| !s.is_empty());

    let is_bay = detected == ArchiveFormat::Bay
        || lower.ends_with(".bay")
        || lower.ends_with(".bayg");

    if is_bay && is_enc && pwd_clean.is_none() {
        let format = if lower.ends_with(".bayg") {
            "Bayzip Gamer Archive (.bayg)".to_string()
        } else {
            "Bayzip Archive (.bay)".to_string()
        };
        return Ok(ArchiveInfo {
            path: path.to_string(),
            filename,
            size,
            file_count: 0,
            total_uncompressed_size: 0,
            format,
            is_archive: true,
            is_encrypted: true,
            entries: Vec::new(),
        });
    }

    let (entries, file_count, total_uncompressed_size) = match detected {
        ArchiveFormat::Zip => list_zip_entries(p, pwd_ref),
        ArchiveFormat::SevenZip => list_7z_entries(p, pwd_ref),
        ArchiveFormat::Tar | ArchiveFormat::TarGz | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd | ArchiveFormat::Iso | ArchiveFormat::Cab => list_tar_entries(p),
        ArchiveFormat::Rar => list_rar_entries(p),
        ArchiveFormat::Bay => list_bay_entries(p, pwd_ref),
        _ => {
            let lower = path.to_lowercase();
            if lower.ends_with(".zip") {
                list_zip_entries(p, pwd_ref)
            } else if lower.ends_with(".7z") {
                list_7z_entries(p, pwd_ref)
            } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".tar.bz2") || lower.ends_with(".bz2") || lower.ends_with(".tar.xz") || lower.ends_with(".xz") || lower.ends_with(".tar.zst") || lower.ends_with(".zst") || lower.ends_with(".iso") || lower.ends_with(".cab") {
                list_tar_entries(p)
            } else if lower.ends_with(".rar") {
                list_rar_entries(p)
            } else if lower.ends_with(".bay") || lower.ends_with(".bayg") {
                list_bay_entries(p, pwd_ref)
            } else {
                (Vec::new(), 0, 0)
            }
        }
    };

    let (format, is_archive) = match detected {
        ArchiveFormat::Bay => {
            let lower = path.to_lowercase();
            if lower.ends_with(".bayg") {
                ("Bayzip Gamer Archive (.bayg)".to_string(), true)
            } else {
                ("Bayzip Archive (.bay)".to_string(), true)
            }
        },
        ArchiveFormat::Zip => ("ZIP Archive (.zip)".to_string(), true),
        ArchiveFormat::SevenZip => ("7-Zip Archive (.7z)".to_string(), true),
        ArchiveFormat::Rar => ("RAR Archive (.rar)".to_string(), true),
        ArchiveFormat::Tar => ("TAR Archive (.tar)".to_string(), true),
        ArchiveFormat::TarGz => ("GZip Archive (.gz / .tgz)".to_string(), true),
        ArchiveFormat::Bzip2 => ("BZip2 Archive (.bz2)".to_string(), true),
        ArchiveFormat::Xz => ("XZ Archive (.xz)".to_string(), true),
        ArchiveFormat::Zstd => ("Zstandard Archive (.zst)".to_string(), true),
        ArchiveFormat::Iso => ("ISO Disc Image (.iso)".to_string(), true),
        ArchiveFormat::Cab => ("Microsoft Cabinet (.cab)".to_string(), true),
        ArchiveFormat::Unknown => {
            let lower = path.to_lowercase();
            if lower.ends_with(".bayg") {
                ("Bayzip Gamer Archive (.bayg)".to_string(), true)
            } else if lower.ends_with(".bay") {
                ("Bayzip Archive (.bay)".to_string(), true)
            } else if lower.ends_with(".zip") {
                ("ZIP Archive (.zip)".to_string(), true)
            } else if lower.ends_with(".7z") {
                ("7-Zip Archive (.7z)".to_string(), true)
            } else if lower.ends_with(".rar") {
                ("RAR Archive (.rar)".to_string(), true)
            } else if lower.ends_with(".tar") {
                ("TAR Archive (.tar)".to_string(), true)
            } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".gz") {
                ("GZip Archive (.gz / .tgz)".to_string(), true)
            } else if lower.ends_with(".tar.bz2") || lower.ends_with(".bz2") {
                ("BZip2 Archive (.bz2)".to_string(), true)
            } else if lower.ends_with(".tar.xz") || lower.ends_with(".xz") {
                ("XZ Archive (.xz)".to_string(), true)
            } else if lower.ends_with(".tar.zst") || lower.ends_with(".zst") {
                ("Zstandard Archive (.zst)".to_string(), true)
            } else if lower.ends_with(".iso") {
                ("ISO Disc Image (.iso)".to_string(), true)
            } else if lower.ends_with(".cab") {
                ("Microsoft Cabinet (.cab)".to_string(), true)
            } else {
                ("Unknown".to_string(), false)
            }
        }
    };

    if !is_archive {
        return Err("The selected file is not a supported archive format".into());
    }

    if is_enc {
        if let Some(pwd) = pwd_clean {
            if entries.is_empty() {
                return Err("Incorrect password: Could not decrypt archive contents.".to_string());
            }
            if detected == ArchiveFormat::Zip || lower.ends_with(".zip") {
                zip::verify_zip_password(p, pwd)?;
            } else if detected == ArchiveFormat::SevenZip || lower.ends_with(".7z") {
                sevenz::verify_7z_password(p, pwd)?;
            }
        }
    }

    Ok(ArchiveInfo {
        path: path.to_string(),
        filename,
        size,
        format,
        is_archive,
        is_encrypted: is_enc,
        file_count,
        total_uncompressed_size,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parent_traversal() {
        assert_eq!(sanitize_archive_path("../../evil.txt"), None);
        assert_eq!(sanitize_archive_path("foo/../../bar.txt"), None);
        assert_eq!(sanitize_archive_path(".."), None);
        assert_eq!(sanitize_archive_path("../"), None);
        assert_eq!(sanitize_archive_path("a/b/../../../c.txt"), None);
    }

    #[test]
    fn test_absolute_and_drive_paths() {
        assert_eq!(sanitize_archive_path("/etc/passwd"), None);
        assert_eq!(sanitize_archive_path(r"C:\Windows\System32\calc.exe"), None);
        assert_eq!(sanitize_archive_path("C:/Windows/System32/calc.exe"), None);
        assert_eq!(sanitize_archive_path(r"\\server\share\file.txt"), None);
        assert_eq!(sanitize_archive_path(r"\Windows\System32"), None);
    }

    #[test]
    fn test_trailing_period_and_space_evasion() {
        assert_eq!(sanitize_archive_path("malicious.exe ."), None);
        assert_eq!(sanitize_archive_path("secret.txt."), None);
        assert_eq!(sanitize_archive_path("folder./file.txt"), None);
        assert_eq!(sanitize_archive_path("folder /file.txt"), None);
        assert_eq!(sanitize_archive_path("evil.txt "), None);
    }

    #[test]
    fn test_dos_reserved_devices() {
        assert_eq!(sanitize_archive_path("CON"), None);
        assert_eq!(sanitize_archive_path("NUL"), None);
        assert_eq!(sanitize_archive_path("CONIN$"), None);
        assert_eq!(sanitize_archive_path("CONOUT$"), None);
        assert_eq!(sanitize_archive_path("CLOCK$"), None);
        assert_eq!(sanitize_archive_path("CONFIG$"), None);
        assert_eq!(sanitize_archive_path("AUX.txt"), None);
        assert_eq!(sanitize_archive_path("sub/con.zip"), None);
        assert_eq!(sanitize_archive_path("PRN"), None);
        assert_eq!(sanitize_archive_path("COM1"), None);
        assert_eq!(sanitize_archive_path("com9.dat"), None);
        assert_eq!(sanitize_archive_path("LPT1"), None);
        assert_eq!(sanitize_archive_path("lpt5.txt"), None);
    }

    #[test]
    fn test_invalid_characters() {
        assert_eq!(sanitize_archive_path("test:file.txt"), None);
        assert_eq!(sanitize_archive_path("test<file.txt"), None);
        assert_eq!(sanitize_archive_path("test>file.txt"), None);
        assert_eq!(sanitize_archive_path("test\"file.txt"), None);
        assert_eq!(sanitize_archive_path("test|file.txt"), None);
        assert_eq!(sanitize_archive_path("test?file.txt"), None);
        assert_eq!(sanitize_archive_path("test*file.txt"), None);
        assert_eq!(sanitize_archive_path("test\0file.txt"), None);
        assert_eq!(sanitize_archive_path("test\x1Ffile.txt"), None);
    }

    #[test]
    fn test_valid_paths() {
        let valid_nested = sanitize_archive_path("folder/subfolder/file.txt");
        assert!(valid_nested.is_some());
        assert_eq!(valid_nested.unwrap(), PathBuf::from("folder").join("subfolder").join("file.txt"));

        let valid_curdir = sanitize_archive_path("./normal.txt");
        assert!(valid_curdir.is_some());
        assert_eq!(valid_curdir.unwrap(), PathBuf::from("normal.txt"));

        let normal = sanitize_archive_path("normal.txt");
        assert!(normal.is_some());
        assert_eq!(normal.unwrap(), PathBuf::from("normal.txt"));
    }

    #[test]
    fn test_empty_and_dot_paths() {
        assert_eq!(sanitize_archive_path(""), None);
        assert_eq!(sanitize_archive_path("   "), None);
        assert_eq!(sanitize_archive_path("."), None);
        assert_eq!(sanitize_archive_path("./"), None);
    }

    #[test]
    fn test_rejects_unencrypted_baytoc_on_bayenc_archive() {
        let entries = vec![ArchiveEntryItem {
            name: "secret.txt".to_string(),
            size: 100,
            is_dir: false,
        }];
        let json_bytes = serde_json::to_vec(&entries).unwrap();
        let toc_len = json_bytes.len() as u64;

        let mut buffer = Vec::new();
        buffer.extend_from_slice(crate::crypto::BAY_ENC_V3_MAGIC);
        buffer.extend_from_slice(&[0xAAu8; 32]); // Encrypted body placeholder
        buffer.extend_from_slice(&json_bytes);
        buffer.extend_from_slice(&toc_len.to_le_bytes());
        buffer.extend_from_slice(b"BAYTOC\x02\x00");

        let file_len = buffer.len() as u64;
        let mut cursor = std::io::Cursor::new(buffer);
        let result = bay::read_bay_trailer_stream(&mut cursor, file_len, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Security violation"));
    }

    #[test]
    fn test_inspect_encrypted_bay_wrong_password_returns_err() {
        use std::io::Write;
        use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Key, Nonce};

        let test_dir = std::env::current_dir().unwrap().join("target").join("test_inspect_enc_bay");
        let _ = std::fs::create_dir_all(&test_dir);
        let bay_path = test_dir.join("locked_test.bay");

        {
            let file = std::fs::File::create(&bay_path).unwrap();
            let mut enc_writer = crate::crypto::ChunkedAesWriter::new(file, "correct_pass").unwrap();
            enc_writer.write_all(b"BAYZIP\x02\x00").unwrap();
            let mut zstd_enc = zstd::stream::Encoder::new(enc_writer, 1).unwrap();
            zstd_enc.write_all(&[0u8]).unwrap(); // EOF entry
            let enc_writer = zstd_enc.finish().unwrap();
            let mut raw_file = enc_writer.finish().unwrap();

            // Write BAYETOC\x03 trailer
            let toc = serde_json::to_vec(&vec![ArchiveEntryItem {
                name: "secret.txt".to_string(),
                size: 42,
                is_dir: false,
            }]).unwrap();
            let salt = [0x33u8; 16];
            let nonce_bytes = [0x44u8; 12];
            let key_bytes = crate::crypto::derive_bay_key("correct_pass", &salt);
            let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
            let enc_toc = cipher.encrypt(Nonce::from_slice(&nonce_bytes), toc.as_ref()).unwrap();
            raw_file.write_all(&enc_toc).unwrap();
            raw_file.write_all(&(enc_toc.len() as u64).to_le_bytes()).unwrap();
            raw_file.write_all(&salt).unwrap();
            raw_file.write_all(&nonce_bytes).unwrap();
            raw_file.write_all(b"BAYETOC\x03").unwrap();
        }

        let res_no_pwd = inspect_archive(&bay_path.to_string_lossy(), None).unwrap();
        assert!(res_no_pwd.is_encrypted);
        assert!(res_no_pwd.entries.is_empty());

        let res_wrong = inspect_archive(&bay_path.to_string_lossy(), Some("wrong_pass"));
        assert!(res_wrong.is_err());
        assert!(res_wrong.unwrap_err().contains("Incorrect password"));

        let res_ok = inspect_archive(&bay_path.to_string_lossy(), Some("correct_pass")).unwrap();
        assert!(res_ok.is_encrypted);
        assert_eq!(res_ok.entries.len(), 1);
        assert_eq!(res_ok.entries[0].name, "secret.txt");

        let _ = std::fs::remove_dir_all(&test_dir);
    }
}

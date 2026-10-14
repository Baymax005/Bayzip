use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use sevenz_rust2::Password;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce, Key,
};
use sha2::{Sha256, Digest};

use crate::types::{ArchiveEntryItem, ArchiveInfo, ArchiveFormat};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn derive_bay_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(salt);
    let mut current = hasher.finalize();

    for _ in 1..10_000 {
        let mut h = Sha256::new();
        h.update(&current);
        h.update(salt);
        current = h.finalize();
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&current);
    key
}

pub fn sanitize_archive_path(raw: &str) -> Option<PathBuf> {
    let clean = raw.replace('\\', "/");
    let trimmed = clean.trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let p = Path::new(trimmed);
    for component in p.components() {
        match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return None,
            _ => {}
        }
    }
    Some(PathBuf::from(trimmed))
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
        ".bay_gamer",
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

pub fn has_single_root_dir(entries: &[ArchiveEntryItem]) -> bool {
    let mut candidate_root: Option<&str> = None;
    let mut non_empty_count = 0;

    for entry in entries {
        let clean = entry.name.trim().trim_start_matches(|c| c == '/' || c == '\\');
        if clean.is_empty() || clean == "." {
            continue;
        }
        non_empty_count += 1;

        let first_segment = match clean.split(['/', '\\']).next() {
            Some(s) if !s.is_empty() => s,
            _ => return false,
        };

        // If it has no slash and is a file, then it is a top-level file (not a wrapped single root)
        if !clean.contains('/') && !clean.contains('\\') && !entry.is_dir {
            return false;
        }

        match candidate_root {
            None => candidate_root = Some(first_segment),
            Some(r) => {
                if r != first_segment {
                    return false;
                }
            }
        }
    }

    non_empty_count > 0 && candidate_root.is_some()
}

pub fn detect_archive_type(path: &Path) -> ArchiveFormat {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return ArchiveFormat::Unknown,
    };

    let mut magic = [0u8; 8];
    let bytes_read = file.read(&mut magic).unwrap_or(0);
    if bytes_read >= 7 && (magic.starts_with(b"BAYZIP\x02") || magic.starts_with(b"BAYBRO\x02") || magic.starts_with(b"BAYLZ4\x02") || magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
        return ArchiveFormat::Bay;
    }
    if bytes_read >= 4 && magic[0] == 0x04 && magic[1] == 0x22 && magic[2] == 0x4D && magic[3] == 0x18 {
        return ArchiveFormat::Bay;
    }
    if bytes_read >= 4 && magic[0] == 0x50 && magic[1] == 0x4B && (magic[2] == 0x03 || magic[2] == 0x05 || magic[2] == 0x07) {
        return ArchiveFormat::Zip;
    }
    if bytes_read >= 6 && magic[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        return ArchiveFormat::SevenZip;
    }
    if bytes_read >= 4 && magic[0..4] == [0x52, 0x61, 0x72, 0x21] {
        return ArchiveFormat::Rar;
    }
    if bytes_read >= 2 && magic[0] == 0x1F && magic[1] == 0x8B {
        return ArchiveFormat::TarGz;
    }

    if let Ok(_) = file.seek(std::io::SeekFrom::Start(257)) {
        let mut ustar = [0u8; 5];
        if file.read_exact(&mut ustar).is_ok() && &ustar == b"ustar" {
            return ArchiveFormat::Tar;
        }
    }

    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "bay" | "bay_gamer" => ArchiveFormat::Bay,
        "zip" => ArchiveFormat::Zip,
        "7z" => ArchiveFormat::SevenZip,
        "rar" => ArchiveFormat::Rar,
        "tar" => ArchiveFormat::Tar,
        "gz" | "tgz" => ArchiveFormat::TarGz,
        _ => ArchiveFormat::Unknown,
    }
}

pub fn is_archive_encrypted(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else { return false; };
    let mut header = [0u8; 16];
    let n = file.read(&mut header).unwrap_or(0);
    if n >= 7 && (header.starts_with(b"BAYENC\x01") || header.starts_with(b"BAYENC\x02")) {
        return true;
    }
    // Check ZIP encryption
    if n >= 4 && header[0] == 0x50 && header[1] == 0x4B {
        if let Ok(file_for_zip) = File::open(path) {
            if let Ok(mut zip) = zip::ZipArchive::new(file_for_zip) {
                for i in 0..zip.len() {
                    match zip.by_index(i) {
                        Ok(entry) => {
                            if entry.encrypted() {
                                return true;
                            }
                        },
                        Err(zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED)) => {
                            return true;
                        },
                        Err(zip::result::ZipError::InvalidPassword) => {
                            return true;
                        },
                        _ => {}
                    }
                }
            }
        }
    }
    // Check 7z encryption
    if n >= 6 && header[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        if let Ok(file) = File::open(path) {
            match sevenz_rust2::ArchiveReader::new(file, Password::empty()) {
                Ok(mut reader) => {
                    if let Err(sevenz_rust2::Error::PasswordRequired) = reader.for_each_entries(|_, _| Ok(false)) {
                        return true;
                    }
                }
                Err(sevenz_rust2::Error::PasswordRequired) => {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

pub fn list_zip_entries(p: &Path, _password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed = 0;
    if let Ok(file) = File::open(p) {
        if let Ok(mut zip) = zip::ZipArchive::new(file) {
            for i in 0..zip.len() {
                if let Ok(entry) = zip.by_index(i) {
                    let clean = entry.name().replace('\\', "/");
                    let name = clean.trim_start_matches('/').to_string();
                    if !name.is_empty() {
                        let size = entry.size();
                        let is_dir = entry.is_dir() || clean.ends_with('/');
                        total_uncompressed += size;
                        items.push(ArchiveEntryItem { name, size, is_dir });
                    }
                }
            }
        }
    }
    let count = items.len();
    (items, count, total_uncompressed)
}

pub fn list_7z_entries(p: &Path, password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed = 0;
    if let Ok(file) = File::open(p) {
        let pwd = password.map(|s| Password::from(s)).unwrap_or_else(Password::empty);
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
                total_uncompressed += size;
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

pub fn list_tar_entries(p: &Path) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed = 0;
    let is_gz = p.to_string_lossy().to_lowercase().ends_with(".gz") || p.to_string_lossy().to_lowercase().ends_with(".tgz");
    if let Ok(file) = File::open(p) {
        if is_gz {
            let gz = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(gz);
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
            let mut archive = tar::Archive::new(file);
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

pub fn list_rar_entries(p: &Path) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut cmd = Command::new("tar.exe");
    cmd.args(["-tf", &p.to_string_lossy()]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    if let Ok(output) = cmd.output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let clean = trimmed.replace('\\', "/");
                let name = clean.trim_start_matches('/').to_string();
                let is_dir = clean.ends_with('/');
                items.push(ArchiveEntryItem {
                    name,
                    size: 0,
                    is_dir,
                });
            }
        }
    }
    let count = items.len();
    (items, count, 0)
}

pub fn list_bay_stream_entries<R: Read>(mut decoder: R) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_size = 0u64;

    let mut first_byte = [0u8; 1];
    let mut has_first = false;
    if decoder.read_exact(&mut first_byte).is_ok() {
        if first_byte[0] == b'B' {
            let mut rest = [0u8; 6];
            let _ = decoder.read_exact(&mut rest);
        } else {
            has_first = true;
        }
    }

    loop {
        let entry_type = if has_first {
            has_first = false;
            first_byte[0]
        } else {
            let mut b = [0u8; 1];
            match decoder.read_exact(&mut b) {
                Ok(_) => b[0],
                Err(_) => break,
            }
        };

        if entry_type == 0 {
            break;
        }

        let mut len_bytes = [0u8; 4];
        if decoder.read_exact(&mut len_bytes).is_err() { break; }
        let name_len = u32::from_le_bytes(len_bytes) as usize;
        let mut name_buf = vec![0u8; name_len];
        if decoder.read_exact(&mut name_buf).is_err() { break; }
        let rel_path_str = String::from_utf8_lossy(&name_buf).to_string();

        let mut size_bytes = [0u8; 8];
        if decoder.read_exact(&mut size_bytes).is_err() { break; }
        let file_size = u64::from_le_bytes(size_bytes);

        total_size += file_size;
        let clean = rel_path_str.replace('\\', "/");
        let is_dir = clean.ends_with('/') || entry_type == 2;
        items.push(ArchiveEntryItem {
            name: clean.trim_start_matches('/').to_string(),
            size: file_size,
            is_dir,
        });

        // Skip file payload in metadata-only listing
        let mut to_skip = file_size;
        let mut sink = [0u8; 128 * 1024];
        while to_skip > 0 {
            let chunk = (to_skip as usize).min(sink.len());
            match decoder.read(&mut sink[..chunk]) {
                Ok(0) => break,
                Ok(n) => to_skip -= n as u64,
                Err(_) => break,
            }
        }
    }

    let count = items.len();
    (items, count, total_size)
}

pub fn list_bay_entries(p: &Path, password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut file = match File::open(p) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), 0, 0),
    };

    let mut magic = [0u8; 8];
    let n = file.read(&mut magic).unwrap_or(0);

    // Encrypted .bay
    if n >= 7 && (magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
        let pwd = match password {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => return (Vec::new(), 0, 0),
        };

        let header_len = if magic.starts_with(b"BAYENC\x02") { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(header_len));
        let mut salt = [0u8; 16];
        if file.read_exact(&mut salt).is_err() { return (Vec::new(), 0, 0); }
        let mut nonce_bytes = [0u8; 12];
        if file.read_exact(&mut nonce_bytes).is_err() { return (Vec::new(), 0, 0); }

        let mut ciphertext = Vec::new();
        if file.read_to_end(&mut ciphertext).is_err() { return (Vec::new(), 0, 0); }

        let key_bytes = derive_bay_key(pwd, &salt);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let Ok(plaintext) = cipher.decrypt(nonce, ciphertext.as_ref()) else {
            return (Vec::new(), 0, 0);
        };

        if plaintext.starts_with(b"BAYBRO\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            let decoder = brotli::Decompressor::new(&plaintext[offset..], 64 * 1024);
            return list_bay_stream_entries(decoder);
        } else if plaintext.starts_with(b"BAYLZ4\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            let decoder = lz4_flex::frame::FrameDecoder::new(&plaintext[offset..]);
            return list_bay_stream_entries(decoder);
        } else if plaintext.starts_with(b"BAYZIP\x02") {
            let offset = if plaintext.len() >= 8 && plaintext[6] == 2 && plaintext[7] == 0 { 8 } else { 7 };
            if let Ok(decoder) = zstd::stream::Decoder::new(&plaintext[offset..]) {
                return list_bay_stream_entries(decoder);
            }
        } else if plaintext.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
            let decoder = lz4_flex::frame::FrameDecoder::new(&plaintext[..]);
            return list_bay_stream_entries(decoder);
        } else if let Ok(decoder) = zstd::stream::Decoder::new(&plaintext[..]) {
            return list_bay_stream_entries(decoder);
        }
        return (Vec::new(), 0, 0);
    }

    let _ = file.seek(std::io::SeekFrom::Start(0));
    let mut magic_header = [0u8; 8];
    let n = file.read(&mut magic_header).unwrap_or(0);
    if n >= 7 && magic_header.starts_with(b"BAYBRO\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let decoder = brotli::Decompressor::new(file, 64 * 1024);
        list_bay_stream_entries(decoder)
    } else if n >= 7 && magic_header.starts_with(b"BAYLZ4\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        list_bay_stream_entries(decoder)
    } else if n >= 7 && magic_header.starts_with(b"BAYZIP\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        if let Ok(decoder) = zstd::stream::Decoder::new(file) {
            list_bay_stream_entries(decoder)
        } else {
            (Vec::new(), 0, 0)
        }
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        list_bay_stream_entries(decoder)
    } else {
        let _ = file.seek(std::io::SeekFrom::Start(0));
        if let Ok(decoder) = zstd::stream::Decoder::new(file) {
            list_bay_stream_entries(decoder)
        } else {
            (Vec::new(), 0, 0)
        }
    }
}

pub fn inspect_archive(path: &str, password: Option<&str>) -> Result<ArchiveInfo, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err("File not found".into());
    }
    let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("archive").to_string();
    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    
    let detected = detect_archive_type(p);
    let is_enc = is_archive_encrypted(p);
    let pwd_ref = password;

    let (entries, file_count, total_uncompressed_size) = match detected {
        ArchiveFormat::Zip => list_zip_entries(p, pwd_ref),
        ArchiveFormat::SevenZip => list_7z_entries(p, pwd_ref),
        ArchiveFormat::Tar | ArchiveFormat::TarGz => list_tar_entries(p),
        ArchiveFormat::Rar => list_rar_entries(p),
        ArchiveFormat::Bay => list_bay_entries(p, pwd_ref),
        _ => {
            let lower = path.to_lowercase();
            if lower.ends_with(".zip") {
                list_zip_entries(p, pwd_ref)
            } else if lower.ends_with(".7z") {
                list_7z_entries(p, pwd_ref)
            } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                list_tar_entries(p)
            } else if lower.ends_with(".rar") {
                list_rar_entries(p)
            } else if lower.ends_with(".bay") || lower.ends_with(".bay_gamer") {
                list_bay_entries(p, pwd_ref)
            } else {
                (Vec::new(), 0, 0)
            }
        }
    };

    let (format, is_archive) = match detected {
        ArchiveFormat::Bay => ("Bayzip Archive (.bay)".to_string(), true),
        ArchiveFormat::Zip => ("ZIP Archive (.zip)".to_string(), true),
        ArchiveFormat::SevenZip => ("7-Zip Archive (.7z)".to_string(), true),
        ArchiveFormat::Rar => ("RAR Archive (.rar)".to_string(), true),
        ArchiveFormat::Tar | ArchiveFormat::TarGz => ("TAR Archive (.tar)".to_string(), true),
        ArchiveFormat::Unknown => {
            let lower = path.to_lowercase();
            if lower.ends_with(".bay") || lower.ends_with(".bay_gamer") {
                ("Bayzip Archive (.bay)".to_string(), true)
            } else if lower.ends_with(".zip") {
                ("ZIP Archive (.zip)".to_string(), true)
            } else if lower.ends_with(".7z") {
                ("7-Zip Archive (.7z)".to_string(), true)
            } else if lower.ends_with(".rar") {
                ("RAR Archive (.rar)".to_string(), true)
            } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                ("TAR Archive (.tar)".to_string(), true)
            } else {
                ("Unknown".to_string(), false)
            }
        }
    };

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

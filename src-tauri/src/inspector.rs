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

pub use crate::crypto::derive_bay_key;
use crate::crypto::ChunkedAesReader;
use crate::types::{ArchiveEntryItem, ArchiveInfo, ArchiveFormat, ArchiveEntryPreview};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const DOS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

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
            std::path::Component::Normal(os_str) => {
                let s = os_str.to_string_lossy();
                // Block NTFS Alternate Data Streams (ADS), null bytes, and colons
                if s.contains(':') || s.contains('\0') {
                    return None;
                }
                // Check DOS reserved device names (e.g. CON, AUX, NUL, COM1, regardless of extension)
                let stem = s.split('.').next().unwrap_or("").trim();
                if DOS_RESERVED_NAMES.iter().any(|&r| stem.eq_ignore_ascii_case(r)) {
                    return None;
                }
            }
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
        let clean = entry.name.trim().trim_start_matches(|c| c == '/' || c == '\\');
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

pub fn detect_archive_type(path: &Path) -> ArchiveFormat {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return ArchiveFormat::Unknown,
    };

    let mut magic = [0u8; 8];
    let bytes_read = file.read(&mut magic).unwrap_or(0);
    if bytes_read >= 6 && (magic.starts_with(b"BAYZIP") || magic.starts_with(b"BAYBRO") || magic.starts_with(b"BAYLZ4") || magic.starts_with(b"BAYENC")) {
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
    // BZip2: 0x42 0x5A 0x68 ("BZh")
    if bytes_read >= 3 && &magic[0..3] == b"BZh" {
        return ArchiveFormat::Bzip2;
    }
    // XZ: 0xFD 0x37 0x7A 0x58 0x5A 0x00 ("\xFD7zXZ\x00")
    if bytes_read >= 6 && &magic[0..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
        return ArchiveFormat::Xz;
    }
    // Zstandard: 0x28 0xB5 0x2F 0xFD (0xFD2FB528 Little Endian)
    if bytes_read >= 4 && &magic[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
        return ArchiveFormat::Zstd;
    }
    // Microsoft Cabinet: 0x4D 0x53 0x43 0x46 ("MSCF")
    if bytes_read >= 4 && &magic[0..4] == b"MSCF" {
        return ArchiveFormat::Cab;
    }

    // POSIX TAR (ustar at offset 257)
    if let Ok(_) = file.seek(std::io::SeekFrom::Start(257)) {
        let mut ustar = [0u8; 5];
        if file.read_exact(&mut ustar).is_ok() && &ustar == b"ustar" {
            return ArchiveFormat::Tar;
        }
    }

    // ISO 9660 Disc Image: Sector 16 starts at 32768 (0x8000). Byte 0 is descriptor type, bytes 1..6 are "CD001"
    if let Ok(_) = file.seek(std::io::SeekFrom::Start(0x8000 + 1)) {
        let mut cd001 = [0u8; 5];
        if file.read_exact(&mut cd001).is_ok() && &cd001 == b"CD001" {
            return ArchiveFormat::Iso;
        }
    }

    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "bay" | "bayg" => ArchiveFormat::Bay,
        "zip" => ArchiveFormat::Zip,
        "7z" => ArchiveFormat::SevenZip,
        "rar" => ArchiveFormat::Rar,
        "tar" => ArchiveFormat::Tar,
        "gz" | "tgz" => ArchiveFormat::TarGz,
        "bz2" => ArchiveFormat::Bzip2,
        "xz" => ArchiveFormat::Xz,
        "zst" => ArchiveFormat::Zstd,
        "iso" => ArchiveFormat::Iso,
        "cab" => ArchiveFormat::Cab,
        _ => ArchiveFormat::Unknown,
    }
}

pub fn has_instant_bay_toc(p: &Path) -> bool {
    let Ok(mut file) = File::open(p) else { return false; };
    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if file_len < 24 {
        return false;
    }
    if file.seek(std::io::SeekFrom::End(-8)).is_ok() {
        let mut magic = [0u8; 8];
        if file.read_exact(&mut magic).is_ok() {
            return &magic == b"BAYTOC\x02\x00" || &magic == b"BAYETOC\x03";
        }
    }
    false
}

fn check_rar4_encrypted(file: &mut File) -> bool {
    let mut buf = [0u8; 4096];
    let _ = file.seek(std::io::SeekFrom::Start(7));
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return false,
    };
    let mut offset = 0;
    while offset + 7 <= n {
        let head_type = buf[offset + 2];
        let head_flags = u16::from_le_bytes([buf[offset + 3], buf[offset + 4]]);
        let head_size = u16::from_le_bytes([buf[offset + 5], buf[offset + 6]]) as usize;
        if head_size < 7 {
            break;
        }
        // 0x73 is MAIN_HEAD (MHD_PASSWORD = 0x0080)
        if head_type == 0x73 && (head_flags & 0x0080) != 0 {
            return true;
        }
        // 0x74 is FILE_HEAD (LHD_PASSWORD = 0x0004)
        if head_type == 0x74 && (head_flags & 0x0004) != 0 {
            return true;
        }
        let add_size = if (head_flags & 0x8000) != 0 && offset + 11 <= n {
            u32::from_le_bytes([buf[offset + 7], buf[offset + 8], buf[offset + 9], buf[offset + 10]]) as usize
        } else {
            0
        };
        offset += head_size.saturating_add(add_size);
    }
    false
}

fn read_rar5_vint(buf: &[u8], offset: &mut usize) -> Option<u64> {
    let mut val = 0u64;
    let mut shift = 0;
    while *offset < buf.len() {
        let b = buf[*offset];
        *offset += 1;
        val |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Some(val);
        }
        shift += 7;
        if shift > 56 {
            return None;
        }
    }
    None
}

fn check_rar5_encrypted(file: &mut File) -> bool {
    let mut buf = [0u8; 8192];
    let _ = file.seek(std::io::SeekFrom::Start(8));
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return false,
    };
    let mut offset = 0;
    while offset + 4 < n {
        offset += 4; // Skip CRC32
        let header_size = match read_rar5_vint(&buf[..n], &mut offset) {
            Some(sz) => sz as usize,
            None => break,
        };
        let header_start = offset;
        let header_type = match read_rar5_vint(&buf[..n], &mut offset) {
            Some(t) => t,
            None => break,
        };
        // Header type 4 is H_CRYPT (entire archive header is encrypted)
        if header_type == 4 {
            return true;
        }
        let header_flags = match read_rar5_vint(&buf[..n], &mut offset) {
            Some(f) => f,
            None => break,
        };
        // Header type 2 is file header (flag 0x0004 indicates password protection)
        if header_type == 2 && (header_flags & 0x0004) != 0 {
            return true;
        }
        offset = header_start.saturating_add(header_size);
    }
    false
}

pub fn is_archive_encrypted(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else { return false; };
    let mut header = [0u8; 16];
    let n = file.read(&mut header).unwrap_or(0);
    if n >= 6 && header.starts_with(b"BAYENC") {
        return true;
    }
    if file.seek(std::io::SeekFrom::End(-8)).is_ok() {
        let mut trailer = [0u8; 8];
        if file.read_exact(&mut trailer).is_ok() && &trailer == b"BAYETOC\x03" {
            return true;
        }
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
    // Check RAR encryption (RAR4 and RAR5)
    if n >= 7 && &header[0..7] == b"Rar!\x1a\x07\x00" {
        if check_rar4_encrypted(&mut file) {
            return true;
        }
    } else if n >= 8 && &header[0..8] == b"Rar!\x1a\x07\x01\x00" {
        if check_rar5_encrypted(&mut file) {
            return true;
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
                if let Ok(entry) = zip.by_index_raw(i) {
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
    let mut total_uncompressed = 0u64;

    // Try verbose list first (-tvf) to extract accurate entry sizes and total uncompressed bytes
    let mut cmd_verbose = Command::new("tar.exe");
    cmd_verbose.args(["-tvf", &p.to_string_lossy()]);
    #[cfg(target_os = "windows")]
    cmd_verbose.creation_flags(CREATE_NO_WINDOW);

    if let Ok(output) = cmd_verbose.output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let is_dir_perm = trimmed.starts_with('d');
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 6 {
                    // Detect date token column to support both 3-column dates (Oct 18 04:11)
                    // and 2-column ISO/locale dates (2026-10-18 04:11)
                    let months = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
                    let mut date_col_idx = None;
                    let mut date_span = 2; // 2 columns by default (e.g. 2026-10-18 04:11)

                    for (idx, token) in parts.iter().enumerate() {
                        let lower_token = token.to_lowercase();
                        if months.contains(&lower_token.as_str()) {
                            date_col_idx = Some(idx);
                            date_span = 3; // month, day, time/year
                            break;
                        } else if token.len() >= 8 && (token.contains('-') || token.contains('/')) && token.chars().any(|c| c.is_ascii_digit()) {
                            date_col_idx = Some(idx);
                            date_span = 2; // date, time
                            break;
                        }
                    }

                    let (parsed_size, name_start_col) = match date_col_idx {
                        Some(d_idx) => {
                            let sz = if d_idx > 0 {
                                parts[d_idx - 1].parse::<u64>().unwrap_or(0)
                            } else {
                                0
                            };
                            (sz, (d_idx + date_span).min(parts.len()))
                        }
                        None => {
                            let mut sz = 0u64;
                            let mut start = 8;
                            if let Ok(s) = parts[4].parse::<u64>() {
                                sz = s;
                            } else if parts.len() >= 7 {
                                if let Ok(s) = parts[3].parse::<u64>() {
                                    sz = s;
                                    start = 7;
                                } else if let Ok(s) = parts[5].parse::<u64>() {
                                    sz = s;
                                    start = 9;
                                }
                            }
                            (sz, start.min(parts.len()))
                        }
                    };

                    // Extract the exact filename slice directly from `trimmed` by stepping
                    // past prefix columns to preserve all consecutive spaces within the filename!
                    let mut search_from = 0;
                    for i in 0..name_start_col {
                        if let Some(pos) = trimmed[search_from..].find(parts[i]) {
                            search_from += pos + parts[i].len();
                        }
                    }
                    let raw_name_slice = trimmed[search_from..].trim_start();
                    let raw_name = if !raw_name_slice.is_empty() {
                        raw_name_slice.to_string()
                    } else if parts.len() > name_start_col {
                        parts[name_start_col..].join(" ")
                    } else {
                        parts.last().unwrap_or(&"").to_string()
                    };

                    let clean = raw_name.replace('\\', "/");
                    let name = clean.trim_start_matches('/').to_string();
                    if !name.is_empty() {
                        let is_dir = is_dir_perm || clean.ends_with('/');
                        let size = if is_dir { 0 } else { parsed_size };
                        total_uncompressed += size;
                        items.push(ArchiveEntryItem {
                            name,
                            size,
                            is_dir,
                        });
                    }
                }
            }
        }
    }

    // Fallback to standard -tf if verbose parsing produced no entries
    if items.is_empty() {
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
    }

    let count = items.len();
    (items, count, total_uncompressed)
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

    // Allocate 2 MB fast skipping buffer
    let mut sink = vec![0u8; 2 * 1024 * 1024];

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
        if name_len > 65536 { break; }
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

        // Fast skip file payload in metadata-only listing
        let mut to_skip = file_size;
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

    // Instant TOC (Table of Contents) check at EOF:
    // If BAYTOC or BAYETOC trailer exists, we read metadata in < 1ms without decompressing gigabytes!
    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if file_len >= 24 {
        if file.seek(std::io::SeekFrom::End(-8)).is_ok() {
            let mut trailer_magic = [0u8; 8];
            if file.read_exact(&mut trailer_magic).is_ok() {
                if &trailer_magic == b"BAYTOC\x02\x00" {
                    if file.seek(std::io::SeekFrom::End(-16)).is_ok() {
                        let mut len_bytes = [0u8; 8];
                        if file.read_exact(&mut len_bytes).is_ok() {
                            let toc_len = u64::from_le_bytes(len_bytes);
                            if toc_len > 0 && toc_len < file_len - 16 {
                                if file.seek(std::io::SeekFrom::End(-16 - toc_len as i64)).is_ok() {
                                    let mut toc_bytes = vec![0u8; toc_len as usize];
                                    if file.read_exact(&mut toc_bytes).is_ok() {
                                        if let Ok(entries) = serde_json::from_slice::<Vec<ArchiveEntryItem>>(&toc_bytes) {
                                            let count = entries.len();
                                            let total_uncompressed: u64 = entries.iter().map(|e| e.size).sum();
                                            return (entries, count, total_uncompressed);
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if &trailer_magic == b"BAYETOC\x03" {
                    let pwd = match password {
                        Some(p) if !p.trim().is_empty() => p.trim(),
                        _ => return (Vec::new(), 0, 0),
                    };

                    // Seek to SeekFrom::End(-44):
                    // Layout: [enc_toc_bytes][enc_toc_len: 8B][salt: 16B][nonce: 12B][b"BAYETOC\x03": 8B]
                    // Total trailer length after enc_toc_bytes = 44 bytes.
                    if file_len >= 44 && file.seek(std::io::SeekFrom::End(-44)).is_ok() {
                        let mut meta = [0u8; 36];
                        if file.read_exact(&mut meta).is_ok() {
                            let mut len_bytes = [0u8; 8];
                            len_bytes.copy_from_slice(&meta[0..8]);
                            let enc_toc_len = u64::from_le_bytes(len_bytes);
                            let salt = &meta[8..24];
                            let nonce_bytes = &meta[24..36];

                            if enc_toc_len > 0 && enc_toc_len < file_len - 44 {
                                if file.seek(std::io::SeekFrom::End(-44 - enc_toc_len as i64)).is_ok() {
                                    let mut enc_bytes = vec![0u8; enc_toc_len as usize];
                                    if file.read_exact(&mut enc_bytes).is_ok() {
                                        let key_bytes = derive_bay_key(pwd, salt);
                                        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
                                        let cipher = Aes256Gcm::new(key);
                                        let nonce = Nonce::from_slice(nonce_bytes);

                                        if let Ok(decrypted) = cipher.decrypt(nonce, enc_bytes.as_ref()) {
                                            if let Ok(entries) = serde_json::from_slice::<Vec<ArchiveEntryItem>>(&decrypted) {
                                                let count = entries.len();
                                                let total_uncompressed: u64 = entries.iter().map(|e| e.size).sum();
                                                return (entries, count, total_uncompressed);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    return (Vec::new(), 0, 0);
                }
            }
        }
    }

    let _ = file.seek(std::io::SeekFrom::Start(0));

    let mut magic = [0u8; 8];
    let n = file.read(&mut magic).unwrap_or(0);

    // Encrypted .bay v3 (Streaming chunked AEAD - Constant ~1 MB RAM)
    if n >= 7 && magic.starts_with(b"BAYENC\x03") {
        let pwd = match password {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => return (Vec::new(), 0, 0),
        };

        let _ = file.seek(std::io::SeekFrom::Start(8));
        let Ok(mut enc_reader) = ChunkedAesReader::new(file, pwd) else {
            return (Vec::new(), 0, 0);
        };

        let mut inner_sig = [0u8; 8];
        if enc_reader.read_exact(&mut inner_sig).is_err() {
            return (Vec::new(), 0, 0);
        }

        if inner_sig.starts_with(b"BAYBRO\x02") {
            let decoder = brotli::Decompressor::new(enc_reader, 64 * 1024);
            return list_bay_stream_entries(decoder);
        } else if inner_sig.starts_with(b"BAYLZ4\x02") {
            let decoder = lz4_flex::frame::FrameDecoder::new(enc_reader);
            return list_bay_stream_entries(decoder);
        } else if inner_sig.starts_with(b"BAYZIP\x02") {
            if let Ok(decoder) = zstd::stream::Decoder::new(enc_reader) {
                return list_bay_stream_entries(decoder);
            }
        } else {
            let combined = std::io::Cursor::new(inner_sig).chain(enc_reader);
            if let Ok(decoder) = zstd::stream::Decoder::new(combined) {
                return list_bay_stream_entries(decoder);
            }
        }
        return (Vec::new(), 0, 0);
    }

    // Legacy encrypted .bay v1 & v2 (deprecated in v2.0+)
    if n >= 7 && (magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
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

pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

const RESTRICTED_EXECUTABLE_EXTS: &[&str] = &[
    "exe", "dll", "sys", "msi", "com", "scr", "pif", "cpl", "hta", "drv", "ocx"
];

const RESTRICTED_SCRIPT_EXTS: &[&str] = &[
    "bat", "cmd", "vbs", "vbe", "wsf", "wsh", "ps1", "ps1xml", "ps2", "psc1", "reg"
];

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "ico", "svg"
];

const TEXT_CODE_EXTS: &[&str] = &[
    "txt", "md", "markdown", "json", "yaml", "yml", "toml", "xml", "html", "htm",
    "css", "scss", "sass", "less", "js", "mjs", "cjs", "ts", "mts", "cts", "jsx",
    "tsx", "rs", "c", "cpp", "cc", "cxx", "h", "hpp", "cs", "go", "py", "rb", "php",
    "java", "kt", "kts", "swift", "sh", "bash", "zsh", "csv", "tsv", "log", "ini",
    "cfg", "conf", "env", "sql", "gitignore", "gitattributes", "editorconfig",
    "dockerfile", "r", "lua", "dart", "proto", "graphql", "properties"
];

fn read_bay_entry_from_stream<R: Read>(mut decoder: R, target_entry: &str) -> Result<(Vec<u8>, u64), String> {
    let target = target_entry.replace('\\', "/").trim_start_matches('/').to_string();
    let max_preview_bytes = 15 * 1024 * 1024; // 15 MB max

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
        if name_len > 65536 { break; }
        let mut name_buf = vec![0u8; name_len];
        if decoder.read_exact(&mut name_buf).is_err() { break; }
        let rel_path_str = String::from_utf8_lossy(&name_buf).to_string();

        let mut size_bytes = [0u8; 8];
        if decoder.read_exact(&mut size_bytes).is_err() { break; }
        let file_size = u64::from_le_bytes(size_bytes);

        let clean = rel_path_str.replace('\\', "/").trim_start_matches('/').to_string();
        if clean == target {
            if entry_type == 2 || clean.ends_with('/') {
                return Err("Cannot preview a folder".into());
            }
            let to_read = (file_size as usize).min(max_preview_bytes);
            let mut data = vec![0u8; to_read];
            decoder.read_exact(&mut data).map_err(|e| format!("Failed to read entry: {}", e))?;
            return Ok((data, file_size));
        }

        // Skip payload
        let mut to_skip = file_size;
        let mut sink = [0u8; 64 * 1024];
        while to_skip > 0 {
            let chunk = (to_skip as usize).min(sink.len());
            match decoder.read(&mut sink[..chunk]) {
                Ok(0) => break,
                Ok(n) => to_skip -= n as u64,
                Err(_) => break,
            }
        }
    }

    Err(format!("Entry '{}' not found in archive", target_entry))
}

fn preview_bay_entry_bytes(path: &Path, entry_name: &str, password: Option<&str>) -> Result<(Vec<u8>, u64), String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open .bay archive: {}", e))?;

    let mut magic = [0u8; 8];
    let n = file.read(&mut magic).unwrap_or(0);

    // Encrypted .bay v3 (Streaming chunked AEAD - Constant ~1 MB RAM)
    if n >= 7 && magic.starts_with(b"BAYENC\x03") {
        let pwd = match password {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => return Err("Password required for encrypted .bay archive".into()),
        };

        let _ = file.seek(std::io::SeekFrom::Start(8));
        let mut enc_reader = ChunkedAesReader::new(file, pwd)
            .map_err(|e| format!("Decryption error: {}", e))?;

        let mut inner_sig = [0u8; 8];
        enc_reader.read_exact(&mut inner_sig)
            .map_err(|_| "Incorrect password for .bay archive".to_string())?;

        if inner_sig.starts_with(b"BAYBRO\x02") {
            let decoder = brotli::Decompressor::new(enc_reader, 64 * 1024);
            return read_bay_entry_from_stream(decoder, entry_name);
        } else if inner_sig.starts_with(b"BAYLZ4\x02") {
            let decoder = lz4_flex::frame::FrameDecoder::new(enc_reader);
            return read_bay_entry_from_stream(decoder, entry_name);
        } else if inner_sig.starts_with(b"BAYZIP\x02") {
            let decoder = zstd::stream::Decoder::new(enc_reader).map_err(|e| e.to_string())?;
            return read_bay_entry_from_stream(decoder, entry_name);
        } else {
            let combined = std::io::Cursor::new(inner_sig).chain(enc_reader);
            let decoder = zstd::stream::Decoder::new(combined).map_err(|e| e.to_string())?;
            return read_bay_entry_from_stream(decoder, entry_name);
        }
    }

    // Legacy encrypted .bay v1 & v2 (deprecated in v2.0+)
    if n >= 7 && (magic.starts_with(b"BAYENC\x01") || magic.starts_with(b"BAYENC\x02")) {
        return Err("Deprecated archive: Legacy v1/v2 encryption format is no longer supported. Please re-encrypt using modern Bayzip v2.0+ streaming encryption.".to_string());
    }

    let _ = file.seek(std::io::SeekFrom::Start(0));
    let mut magic_header = [0u8; 8];
    let n = file.read(&mut magic_header).unwrap_or(0);
    if n >= 7 && magic_header.starts_with(b"BAYBRO\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let decoder = brotli::Decompressor::new(file, 64 * 1024);
        read_bay_entry_from_stream(decoder, entry_name)
    } else if n >= 7 && magic_header.starts_with(b"BAYLZ4\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        read_bay_entry_from_stream(decoder, entry_name)
    } else if n >= 7 && magic_header.starts_with(b"BAYZIP\x02") {
        let offset = if n >= 8 && magic_header[6] == 2 && magic_header[7] == 0 { 8 } else { 7 };
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let decoder = zstd::stream::Decoder::new(file).map_err(|e| e.to_string())?;
        read_bay_entry_from_stream(decoder, entry_name)
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        read_bay_entry_from_stream(decoder, entry_name)
    } else {
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let decoder = zstd::stream::Decoder::new(file).map_err(|e| e.to_string())?;
        read_bay_entry_from_stream(decoder, entry_name)
    }
}

fn preview_zip_entry_bytes(path: &Path, entry_name: &str, password: Option<&str>) -> Result<(Vec<u8>, u64), String> {
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

fn preview_7z_entry_bytes(path: &Path, entry_name: &str, password: Option<&str>) -> Result<(Vec<u8>, u64), String> {
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

fn preview_tar_entry_bytes(path: &Path, entry_name: &str) -> Result<(Vec<u8>, u64), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let target = entry_name.replace('\\', "/").trim_start_matches('/').to_string();
    let max_preview = 15 * 1024 * 1024;
    let is_gz = path.to_string_lossy().to_lowercase().ends_with(".gz") || path.to_string_lossy().to_lowercase().ends_with(".tgz");

    if is_gz {
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
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
        let mut archive = tar::Archive::new(file);
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

fn preview_rar_entry_bytes(path: &Path, entry_name: &str) -> Result<(Vec<u8>, u64), String> {
    let clean_name = entry_name.trim();
    if clean_name.is_empty() || clean_name.starts_with('-') {
        return Err("Invalid entry name for archive preview".into());
    }

    let mut cmd = Command::new("tar.exe");
    // Use -- delimiter before entry_name to strictly prevent CLI flag injection
    cmd.args(["-xf", &path.to_string_lossy(), "-O", "--", clean_name]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to run tar command: {}", e))?;
    let mut stdout = child.stdout.take().ok_or_else(|| "Failed to capture stdout".to_string())?;

    let max_preview = 15 * 1024 * 1024; // Strict 15 MB limit matching other preview formats
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];

    while buf.len() < max_preview {
        let to_read = (max_preview - buf.len()).min(chunk.len());
        match stdout.read(&mut chunk[..to_read]) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }

    let mut stderr_msg = String::new();
    if let Some(mut err_pipe) = child.stderr.take() {
        let _ = err_pipe.read_to_string(&mut stderr_msg);
    }
    let _ = child.kill();
    let _ = child.wait();

    if buf.is_empty() {
        let err = if !stderr_msg.trim().is_empty() {
            format!("Failed to preview entry: {}", stderr_msg.trim())
        } else {
            format!("Entry '{}' not found or empty in RAR archive", entry_name)
        };
        return Err(err);
    }

    let size = buf.len() as u64;
    Ok((buf, size))
}

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

/// Hardware bus classification of a storage volume
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveBusKind {
    UsbRemovable, // External USB / Flash Thumb Drives / SD Cards / Removable Media
    Sata,         // Internal SATA SSD / HDD / SCSI / SAS / RAID
    Nvme,         // Internal PCIe NVMe SSD / SCM
    Unknown,      // Network / UNC / Virtual / Unknown
}

/// Query the physical drive bus kind for a given path.
/// Automatically detects and caches drive letters A through Z via Win32 IOCTL_STORAGE_QUERY_PROPERTY.
#[cfg(target_os = "windows")]
pub fn get_drive_bus_kind(path: &Path) -> DriveBusKind {
    use std::sync::atomic::{AtomicU8, Ordering};

    // Fast static cache for drive letters A through Z (26 slots)
    // 0 = unqueried, 1 = UsbRemovable, 2 = Sata, 3 = Nvme, 4 = Unknown
    static DRIVE_BUS_CACHE: [AtomicU8; 26] = [
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0),
    ];

    let path_str = path.to_string_lossy();
    let mut drive_letter_opt = None;
    let chars: Vec<char> = path_str.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == ':' && i > 0 && chars[i - 1].is_ascii_alphabetic() {
            drive_letter_opt = Some(chars[i - 1].to_ascii_uppercase());
            break;
        }
    }

    let drive_letter = match drive_letter_opt {
        Some(dl) => dl,
        None => {
            let canon = path.canonicalize()
                .or_else(|_| {
                    if let Some(parent) = path.parent() {
                        parent.canonicalize()
                    } else {
                        Err(std::io::Error::new(std::io::ErrorKind::NotFound, ""))
                    }
                })
                .or_else(|_| std::env::current_dir());
            if let Ok(canon_path) = canon {
                let canon_str = canon_path.to_string_lossy();
                let c_chars: Vec<char> = canon_str.chars().collect();
                let mut found = None;
                for i in 0..c_chars.len() {
                    if c_chars[i] == ':' && i > 0 && c_chars[i - 1].is_ascii_alphabetic() {
                        found = Some(c_chars[i - 1].to_ascii_uppercase());
                        break;
                    }
                }
                match found {
                    Some(dl) => dl,
                    None => return DriveBusKind::Unknown,
                }
            } else {
                return DriveBusKind::Unknown;
            }
        }
    };

    let cache_idx = (drive_letter as u8).saturating_sub(b'A') as usize;
    if cache_idx < 26 {
        let cached = DRIVE_BUS_CACHE[cache_idx].load(Ordering::Relaxed);
        match cached {
            1 => return DriveBusKind::UsbRemovable,
            2 => return DriveBusKind::Sata,
            3 => return DriveBusKind::Nvme,
            4 => return DriveBusKind::Unknown,
            _ => {}
        }
    }

    let bus_kind = detect_drive_bus(drive_letter);

    if cache_idx < 26 {
        let val = match bus_kind {
            DriveBusKind::UsbRemovable => 1,
            DriveBusKind::Sata => 2,
            DriveBusKind::Nvme => 3,
            DriveBusKind::Unknown => 4,
        };
        DRIVE_BUS_CACHE[cache_idx].store(val, Ordering::Relaxed);
    }

    bus_kind
}

#[cfg(target_os = "windows")]
fn detect_drive_bus(drive_letter: char) -> DriveBusKind {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDriveTypeW(lpRootPathName: *const u16) -> u32;
        fn CreateFileW(
            lpFileName: *const u16,
            dwDesiredAccess: u32,
            dwShareMode: u32,
            lpSecurityAttributes: *const std::ffi::c_void,
            dwCreationDisposition: u32,
            dwFlagsAndAttributes: u32,
            hTemplateFile: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        fn DeviceIoControl(
            hDevice: *mut std::ffi::c_void,
            dwIoControlCode: u32,
            lpInBuffer: *const std::ffi::c_void,
            nInBufferSize: u32,
            lpOutBuffer: *mut std::ffi::c_void,
            nOutBufferSize: u32,
            lpBytesReturned: *mut u32,
            lpOverlapped: *mut std::ffi::c_void,
        ) -> i32;
        fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
    }

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_CDROM: u32 = 5;

    let root_path = format!("{}:\\", drive_letter);
    let root_wide = to_wide(&root_path);
    let drive_type = unsafe { GetDriveTypeW(root_wide.as_ptr()) };
    if drive_type == DRIVE_REMOVABLE || drive_type == DRIVE_CDROM {
        return DriveBusKind::UsbRemovable;
    }

    let volume_path = format!("\\\\.\\{}:", drive_letter);
    let volume_wide = to_wide(&volume_path);

    const FILE_SHARE_READ: u32 = 0x00000001;
    const FILE_SHARE_WRITE: u32 = 0x00000002;
    const FILE_SHARE_DELETE: u32 = 0x00000004;
    const OPEN_EXISTING: u32 = 3;
    const INVALID_HANDLE_VALUE: isize = -1;
    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D1400;

    let handle = unsafe {
        CreateFileW(
            volume_wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };

    if handle as isize == INVALID_HANDLE_VALUE || handle.is_null() {
        return DriveBusKind::Unknown;
    }

    let query: [u8; 12] = [0u8; 12];
    let mut out_buf: [u8; 1024] = [0u8; 1024];
    let mut bytes_returned: u32 = 0;

    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            query.as_ptr() as *const _,
            query.len() as u32,
            out_buf.as_mut_ptr() as *mut _,
            out_buf.len() as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };

    unsafe {
        CloseHandle(handle);
    }

    if ok != 0 && bytes_returned >= 32 {
        let removable_media = out_buf[10] != 0;
        let bus_type = u32::from_le_bytes([
            out_buf[28],
            out_buf[29],
            out_buf[30],
            out_buf[31],
        ]);

        // BusTypeUsb = 7, BusTypeSd = 12 (0x0C), BusTypeMmc = 13 (0x0D), or marked RemovableMedia
        if bus_type == 7 || bus_type == 12 || bus_type == 13 || removable_media {
            return DriveBusKind::UsbRemovable;
        }

        // BusTypeNvme = 17 (0x11), BusTypeScm = 18 (0x12)
        if bus_type == 17 || bus_type == 18 {
            return DriveBusKind::Nvme;
        }

        // BusTypeSata = 11, BusTypeAta = 3, BusTypeAtapi = 2, BusTypeScsi = 1, BusTypeSas = 10, BusTypeRAID = 8
        if bus_type == 11 || bus_type == 3 || bus_type == 2 || bus_type == 1 || bus_type == 10 || bus_type == 8 {
            return DriveBusKind::Sata;
        }
    }

    DriveBusKind::Unknown
}

/// Query the optimal sequential read slice size for a given file path based on the underlying physical drive bus.
/// - External USB / Flash Thumb Drives / Ext HDDs: Fixed 1 MB (safe against controller timeouts/retries)
/// - Internal SATA (SSD / HDD): Fixed 4 MB (smooth OS throughput)
/// - Internal PCIe NVMe SSD: Fixed 8 MB (maximum multi-gigabyte line speed)
/// - Fallback / Unknown / Network share: Fixed 2 MB
#[cfg(target_os = "windows")]
pub fn get_optimal_read_slice(path: &Path) -> usize {
    match get_drive_bus_kind(path) {
        DriveBusKind::UsbRemovable => 1024 * 1024,       // 1 MB: External USB / Flash Thumb Drives
        DriveBusKind::Sata => 4 * 1024 * 1024,          // 4 MB: Internal SATA (SSD / HDD)
        DriveBusKind::Nvme => 8 * 1024 * 1024,          // 8 MB: Internal PCIe NVMe SSD
        DriveBusKind::Unknown => 2 * 1024 * 1024,       // 2 MB: Network / UNC / Fallback
    }
}

/// Query the optimal sequential write buffer size for a target output archive based on the destination drive bus.
/// Prevents slow USB thumb drives from locking in multi-second synchronous write stalls under Windows Quick-Removal policy.
/// - External USB / Flash Thumb Drives: Capped at 16 MB max (instant cancellation response & smooth progress bar)
/// - Internal SATA SSD / HDD: Capped at 64 MB max (smooth sequential flushing without head thrashing)
/// - Internal PCIe NVMe SSD: Full requested buffer up to 128 MB (unleashes multi-gigabyte PCIe burst line rate)
/// - Fallback / Unknown / Network share: Capped at 32 MB max
#[cfg(target_os = "windows")]
pub fn get_optimal_write_buffer(output_path: &Path, requested_buffer: usize) -> usize {
    let cap = match get_drive_bus_kind(output_path) {
        DriveBusKind::UsbRemovable => 16 * 1024 * 1024, // 16 MB max
        DriveBusKind::Sata => 64 * 1024 * 1024,         // 64 MB max
        DriveBusKind::Nvme => 128 * 1024 * 1024,        // 128 MB max
        DriveBusKind::Unknown => 32 * 1024 * 1024,      // 32 MB max
    };
    requested_buffer.min(cap).max(1024 * 1024)
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveBusKind {
    UsbRemovable,
    Sata,
    Nvme,
    Unknown,
}

#[cfg(not(target_os = "windows"))]
pub fn get_drive_bus_kind(_path: &Path) -> DriveBusKind {
    DriveBusKind::Unknown
}

#[cfg(not(target_os = "windows"))]
pub fn get_optimal_read_slice(_path: &Path) -> usize {
    4 * 1024 * 1024
}

#[cfg(not(target_os = "windows"))]
pub fn get_optimal_write_buffer(_output_path: &Path, requested_buffer: usize) -> usize {
    requested_buffer.min(64 * 1024 * 1024).max(1024 * 1024)
}


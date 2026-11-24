use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use sevenz_rust2::Password;
use crate::types::ArchiveFormat;

pub fn detect_archive_type(path: &Path) -> ArchiveFormat {
    if path.is_dir() {
        return ArchiveFormat::Unknown;
    }

    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    // Exclude common non-archive document & media formats that might contain ZIP containers
    if matches!(
        ext.as_str(),
        "docx" | "docm" | "dotx" | "dotm" | "xlsx" | "xlsm" | "xlsb" | "xltx" | "xltm"
            | "pptx" | "pptm" | "ppsx" | "potx" | "odt" | "ods" | "odp" | "epub"
            | "pages" | "numbers" | "key" | "pdf" | "doc" | "xls" | "ppt" | "pkt"
            | "txt" | "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "mp4" | "mkv"
            | "mp3" | "wav" | "flac" | "exe" | "dll"
    ) {
        return ArchiveFormat::Unknown;
    }

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return ArchiveFormat::Unknown,
    };

    let mut magic = [0u8; 8];
    let bytes_read = file.read(&mut magic).unwrap_or(0);
    if bytes_read == 8
        && (&magic == crate::inspector::bay::BAY_ZSTD_MAGIC
            || &magic == crate::inspector::bay::BAY_BROTLI_MAGIC
            || &magic == crate::inspector::bay::BAY_LZ4_MAGIC
            || &magic == crate::crypto::BAY_ENC_V4_MAGIC)
    {
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
    if bytes_read >= 6 && magic[0..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
        return ArchiveFormat::Xz;
    }
    // Zstandard: 0x28 0xB5 0x2F 0xFD (0xFD2FB528 Little Endian)
    if bytes_read >= 4 && magic[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
        return ArchiveFormat::Zstd;
    }
    // Microsoft Cabinet: 0x4D 0x53 0x43 0x46 ("MSCF")
    if bytes_read >= 4 && &magic[0..4] == b"MSCF" {
        return ArchiveFormat::Cab;
    }

    // POSIX TAR (ustar at offset 257)
    if file.seek(std::io::SeekFrom::Start(257)).is_ok() {
        let mut ustar = [0u8; 5];
        if file.read_exact(&mut ustar).is_ok() && &ustar == b"ustar" {
            return ArchiveFormat::Tar;
        }
    }

    // ISO 9660 Disc Image: Sector 16 starts at 32768 (0x8000). Byte 0 is descriptor type, bytes 1..6 are "CD001"
    if file.seek(std::io::SeekFrom::Start(0x8000 + 1)).is_ok() {
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
            return &magic == crate::inspector::bay::BAY_TOC_MAGIC
                || &magic == crate::crypto::BAY_ETOC_V4_MAGIC;
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
    if n >= 8 && &header[0..8] == crate::crypto::BAY_ENC_V4_MAGIC {
        return true;
    }
    if file.seek(std::io::SeekFrom::End(-8)).is_ok() {
        let mut trailer = [0u8; 8];
        if file.read_exact(&mut trailer).is_ok() && &trailer == crate::crypto::BAY_ETOC_V4_MAGIC {
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
    } else if n >= 8 && &header[0..8] == b"Rar!\x1a\x07\x01\x00" && check_rar5_encrypted(&mut file) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_archive_type_exact_bay_headers_no_lz4_zst_collision() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_format_detect");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let write_bytes = |name: &str, bytes: &[u8]| {
            let p = base.join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };

        // Raw LZ4 frame must not masquerade as ArchiveFormat::Bay
        let raw_lz4 = write_bytes("raw_frame.lz4", &[0x04, 0x22, 0x4D, 0x18, 0x60, 0x40, 0x82, 0x00]);
        assert_ne!(detect_archive_type(&raw_lz4), ArchiveFormat::Bay);

        // Raw Zstd frame must be Zstd, not Bay
        let raw_zst = write_bytes("raw_frame.zst", &[0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x58, 0x00, 0x00]);
        assert_eq!(detect_archive_type(&raw_zst), ArchiveFormat::Zstd);

        // Legacy BAYENC\x03\x00 and 7-byte headers must not match Bay
        let legacy_enc = write_bytes("legacy.dat", b"BAYENC\x03\x00");
        assert_eq!(detect_archive_type(&legacy_enc), ArchiveFormat::Unknown);
        let short_hdr = write_bytes("short.dat", b"BAYZIP\x02");
        assert_eq!(detect_archive_type(&short_hdr), ArchiveFormat::Unknown);

        // Exact 8-byte modern Bay headers must all resolve to ArchiveFormat::Bay
        for (idx, magic) in [
            crate::inspector::bay::BAY_ZSTD_MAGIC,
            crate::inspector::bay::BAY_BROTLI_MAGIC,
            crate::inspector::bay::BAY_LZ4_MAGIC,
            crate::crypto::BAY_ENC_V4_MAGIC,
        ]
        .iter()
        .enumerate()
        {
            let p = write_bytes(&format!("valid_{}.dat", idx), *magic);
            assert_eq!(detect_archive_type(&p), ArchiveFormat::Bay);
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}

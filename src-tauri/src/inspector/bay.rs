use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce, Key,
};

use crate::crypto::{derive_bay_key, ChunkedAesReader};
use crate::types::ArchiveEntryItem;

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

/// Maximum allowable memory allocation for archive Table of Contents (TOC) metadata (64 MB).
pub const MAX_TOC_SIZE: u64 = 64 * 1024 * 1024;

pub trait BayTrailerSource {
    fn with_file<R, F: FnOnce(&mut File) -> Result<R, String>>(&mut self, f: F) -> Result<R, String>;
}

impl BayTrailerSource for &Path {
    fn with_file<R, F: FnOnce(&mut File) -> Result<R, String>>(&mut self, f: F) -> Result<R, String> {
        let mut file = File::open(self).map_err(|e| format!("Failed to open .bay archive: {}", e))?;
        f(&mut file)
    }
}

impl BayTrailerSource for std::path::PathBuf {
    fn with_file<R, F: FnOnce(&mut File) -> Result<R, String>>(&mut self, f: F) -> Result<R, String> {
        let mut file = File::open(self).map_err(|e| format!("Failed to open .bay archive: {}", e))?;
        f(&mut file)
    }
}

impl BayTrailerSource for &std::path::PathBuf {
    fn with_file<R, F: FnOnce(&mut File) -> Result<R, String>>(&mut self, f: F) -> Result<R, String> {
        let mut file = File::open(self).map_err(|e| format!("Failed to open .bay archive: {}", e))?;
        f(&mut file)
    }
}

impl BayTrailerSource for &mut File {
    fn with_file<R, F: FnOnce(&mut File) -> Result<R, String>>(&mut self, f: F) -> Result<R, String> {
        f(self)
    }
}

impl BayTrailerSource for File {
    fn with_file<R, F: FnOnce(&mut File) -> Result<R, String>>(&mut self, f: F) -> Result<R, String> {
        f(self)
    }
}

impl BayTrailerSource for &str {
    fn with_file<R, F: FnOnce(&mut File) -> Result<R, String>>(&mut self, f: F) -> Result<R, String> {
        let mut file = File::open(self).map_err(|e| format!("Failed to open .bay archive: {}", e))?;
        f(&mut file)
    }
}

/// Reads and parses the instant Table of Contents (TOC) trailer from a stream with known length.
///
/// Enforces strict bounded memory allocation safety caps (V-SEC-03):
/// 1. Verifies declared TOC/ETOC size does not exceed `MAX_TOC_SIZE` (64 MB).
/// 2. Verifies declared TOC/ETOC size does not exceed the remaining archive file bounds.
/// Corrupted or maliciously crafted archives declaring oversized TOCs are rejected with an explicit error
/// before heap allocation occurs.
pub fn read_bay_trailer_stream<R: Read + Seek>(
    reader: &mut R,
    file_len: u64,
    password: Option<&str>,
) -> Result<(Vec<ArchiveEntryItem>, usize, u64), String> {
    if file_len < 16 {
        return Err("Archive file too small to contain a BAYTOC or BAYETOC trailer".into());
    }

    reader.seek(std::io::SeekFrom::End(-8))
        .map_err(|e| format!("Failed to seek archive trailer: {}", e))?;
    let mut trailer_magic = [0u8; 8];
    reader.read_exact(&mut trailer_magic)
        .map_err(|e| format!("Failed to read archive trailer magic: {}", e))?;

    if &trailer_magic == b"BAYTOC\x02\x00" {
        reader.seek(std::io::SeekFrom::End(-16))
            .map_err(|e| format!("Failed to seek BAYTOC length: {}", e))?;
        let mut len_bytes = [0u8; 8];
        reader.read_exact(&mut len_bytes)
            .map_err(|e| format!("Failed to read BAYTOC length: {}", e))?;
        let toc_len = u64::from_le_bytes(len_bytes);

        // V-SEC-03: Enforce strict bounded allocation safety checks
        if toc_len == 0 {
            return Err("Malformed archive: BAYTOC length declared as 0 bytes".into());
        }
        if toc_len > MAX_TOC_SIZE {
            return Err(format!(
                "Malformed archive: BAYTOC length ({} bytes) exceeds safety limit of {} bytes (64 MB)",
                toc_len, MAX_TOC_SIZE
            ));
        }
        if toc_len > file_len.saturating_sub(16) {
            return Err(format!(
                "Malformed archive: BAYTOC length ({} bytes) exceeds total archive file bounds ({} bytes)",
                toc_len, file_len
            ));
        }

        // Seek and allocate only after bounds verification passes
        reader.seek(std::io::SeekFrom::End(-16 - toc_len as i64))
            .map_err(|e| format!("Failed to seek to BAYTOC data: {}", e))?;
        let mut toc_bytes = vec![0u8; toc_len as usize];
        reader.read_exact(&mut toc_bytes)
            .map_err(|e| format!("Failed to read BAYTOC payload: {}", e))?;

        let entries: Vec<ArchiveEntryItem> = serde_json::from_slice(&toc_bytes)
            .map_err(|e| format!("Failed to parse BAYTOC JSON metadata: {}", e))?;
        let count = entries.len();
        let total_uncompressed: u64 = entries.iter().map(|e| e.size).sum();
        return Ok((entries, count, total_uncompressed));
    } else if &trailer_magic == b"BAYETOC\x03" {
        let pwd = match password {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => return Err("Password required: Encrypted BAYETOC trailer detected".into()),
        };

        if file_len < 44 {
            return Err("Archive too small for BAYETOC trailer".into());
        }

        reader.seek(std::io::SeekFrom::End(-44))
            .map_err(|e| format!("Failed to seek BAYETOC header: {}", e))?;
        let mut meta = [0u8; 36];
        reader.read_exact(&mut meta)
            .map_err(|e| format!("Failed to read BAYETOC metadata: {}", e))?;

        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&meta[0..8]);
        let enc_toc_len = u64::from_le_bytes(len_bytes);
        let salt = &meta[8..24];
        let nonce_bytes = &meta[24..36];

        // V-SEC-03: Enforce strict bounded allocation safety checks
        if enc_toc_len == 0 {
            return Err("Malformed archive: Encrypted BAYETOC length declared as 0 bytes".into());
        }
        if enc_toc_len > MAX_TOC_SIZE {
            return Err(format!(
                "Malformed archive: Encrypted BAYETOC length ({} bytes) exceeds safety limit of {} bytes (64 MB)",
                enc_toc_len, MAX_TOC_SIZE
            ));
        }
        if enc_toc_len > file_len.saturating_sub(44) {
            return Err(format!(
                "Malformed archive: Encrypted BAYETOC length ({} bytes) exceeds total archive file bounds ({} bytes)",
                enc_toc_len, file_len
            ));
        }

        // Seek and allocate only after bounds verification passes
        reader.seek(std::io::SeekFrom::End(-44 - enc_toc_len as i64))
            .map_err(|e| format!("Failed to seek to encrypted BAYETOC data: {}", e))?;
        let mut enc_bytes = vec![0u8; enc_toc_len as usize];
        reader.read_exact(&mut enc_bytes)
            .map_err(|e| format!("Failed to read encrypted BAYETOC payload: {}", e))?;

        let key_bytes = derive_bay_key(pwd, salt);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(nonce_bytes);

        let decrypted = cipher.decrypt(nonce, enc_bytes.as_ref())
            .map_err(|_| "Incorrect password or corrupted BAYETOC payload".to_string())?;

        let entries: Vec<ArchiveEntryItem> = serde_json::from_slice(&decrypted)
            .map_err(|e| format!("Failed to parse decrypted BAYETOC metadata: {}", e))?;
        let count = entries.len();
        let total_uncompressed: u64 = entries.iter().map(|e| e.size).sum();
        return Ok((entries, count, total_uncompressed));
    }

    Err("No valid BAYTOC or BAYETOC trailer magic found at archive EOF".into())
}

pub fn read_bay_trailer_from_file(
    file: &mut File,
    password: Option<&str>,
) -> Result<(Vec<ArchiveEntryItem>, usize, u64), String> {
    let file_len = file.metadata()
        .map(|m| m.len())
        .map_err(|e| format!("Failed to read file metadata: {}", e))?;
    read_bay_trailer_stream(file, file_len, password)
}

pub fn read_bay_trailer<S: BayTrailerSource>(
    mut source: S,
    password: Option<&str>,
) -> Result<(Vec<ArchiveEntryItem>, usize, u64), String> {
    source.with_file(|file| read_bay_trailer_from_file(file, password))
}

pub fn list_bay_entries(p: &Path, password: Option<&str>) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut file = match File::open(p) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), 0, 0),
    };

    // Instant TOC (Table of Contents) check at EOF:
    // If BAYTOC or BAYETOC trailer exists, we read metadata in < 1ms without decompressing gigabytes!
    match read_bay_trailer_from_file(&mut file, password) {
        Ok(result) => return result,
        Err(e) if e.contains("safety limit") || e.contains("exceeds total archive file bounds") => {
            // Malformed/malicious archive declaring oversized TOC - safely reject
            return (Vec::new(), 0, 0);
        }
        Err(_) => {
            // No trailer or invalid password - fall back to stream decompression
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

pub fn read_bay_entry_from_stream<R: Read>(mut decoder: R, target_entry: &str) -> Result<(Vec<u8>, u64), String> {
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

pub fn preview_bay_entry_bytes(path: &Path, entry_name: &str, password: Option<&str>) -> Result<(Vec<u8>, u64), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_bay_trailer_rejects_oversized_toc() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"DUMMY_ARCHIVE_DATA_12345678");
        let oversized_len: u64 = 100 * 1024 * 1024;
        buffer.extend_from_slice(&oversized_len.to_le_bytes());
        buffer.extend_from_slice(b"BAYTOC\x02\x00");

        let file_len = buffer.len() as u64;
        let mut cursor = Cursor::new(buffer);
        let result = read_bay_trailer_stream(&mut cursor, file_len, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("exceeds safety limit of 67108864 bytes (64 MB)"));
    }

    #[test]
    fn test_read_bay_trailer_rejects_toc_exceeding_file_bounds() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"SMALL_ARCHIVE");
        let bounds_exceeding_len: u64 = 1000;
        buffer.extend_from_slice(&bounds_exceeding_len.to_le_bytes());
        buffer.extend_from_slice(b"BAYTOC\x02\x00");

        let file_len = buffer.len() as u64;
        let mut cursor = Cursor::new(buffer);
        let result = read_bay_trailer_stream(&mut cursor, file_len, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("exceeds total archive file bounds"));
    }

    #[test]
    fn test_read_bay_trailer_rejects_zero_toc_len() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"SOME_DATA_FOR_ARCHIVE");
        let zero_len: u64 = 0;
        buffer.extend_from_slice(&zero_len.to_le_bytes());
        buffer.extend_from_slice(b"BAYTOC\x02\x00");

        let file_len = buffer.len() as u64;
        let mut cursor = Cursor::new(buffer);
        let result = read_bay_trailer_stream(&mut cursor, file_len, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("BAYTOC length declared as 0 bytes"));
    }

    #[test]
    fn test_read_bay_trailer_valid_toc() {
        let entries = vec![
            ArchiveEntryItem {
                name: "test.txt".to_string(),
                size: 1234,
                is_dir: false,
            },
            ArchiveEntryItem {
                name: "folder/".to_string(),
                size: 0,
                is_dir: true,
            },
        ];
        let json_bytes = serde_json::to_vec(&entries).unwrap();
        let toc_len = json_bytes.len() as u64;

        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"REAL_BAY_STREAM_PAYLOAD");
        buffer.extend_from_slice(&json_bytes);
        buffer.extend_from_slice(&toc_len.to_le_bytes());
        buffer.extend_from_slice(b"BAYTOC\x02\x00");

        let file_len = buffer.len() as u64;
        let mut cursor = Cursor::new(buffer);
        let result = read_bay_trailer_stream(&mut cursor, file_len, None);

        assert!(result.is_ok());
        let (parsed_entries, count, total_size) = result.unwrap();
        assert_eq!(count, 2);
        assert_eq!(total_size, 1234);
        assert_eq!(parsed_entries[0].name, "test.txt");
        assert_eq!(parsed_entries[1].name, "folder/");
    }

    #[test]
    fn test_read_bay_trailer_rejects_oversized_encrypted_toc() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"DUMMY_ENCRYPTED_STREAM_PAYLOAD");
        let oversized_len: u64 = 100 * 1024 * 1024;
        buffer.extend_from_slice(&oversized_len.to_le_bytes()); // 8 bytes
        buffer.extend_from_slice(&[0x11u8; 16]); // salt 16 bytes
        buffer.extend_from_slice(&[0x22u8; 12]); // nonce 12 bytes
        buffer.extend_from_slice(b"BAYETOC\x03"); // 8 bytes

        let file_len = buffer.len() as u64;
        let mut cursor = Cursor::new(buffer);
        let result = read_bay_trailer_stream(&mut cursor, file_len, Some("password123"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Encrypted BAYETOC length"));
        assert!(err.contains("exceeds safety limit of 67108864 bytes (64 MB)"));
    }

    #[test]
    fn test_read_bay_trailer_rejects_encrypted_toc_exceeding_file_bounds() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"SHORT");
        let bounds_exceeding_len: u64 = 5000;
        buffer.extend_from_slice(&bounds_exceeding_len.to_le_bytes());
        buffer.extend_from_slice(&[0x11u8; 16]);
        buffer.extend_from_slice(&[0x22u8; 12]);
        buffer.extend_from_slice(b"BAYETOC\x03");

        let file_len = buffer.len() as u64;
        let mut cursor = Cursor::new(buffer);
        let result = read_bay_trailer_stream(&mut cursor, file_len, Some("password123"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Encrypted BAYETOC length"));
        assert!(err.contains("exceeds total archive file bounds"));
    }

    #[test]
    fn test_read_bay_trailer_rejects_zero_encrypted_toc_len() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"DUMMY_PAYLOAD");
        let zero_len: u64 = 0;
        buffer.extend_from_slice(&zero_len.to_le_bytes());
        buffer.extend_from_slice(&[0x11u8; 16]);
        buffer.extend_from_slice(&[0x22u8; 12]);
        buffer.extend_from_slice(b"BAYETOC\x03");

        let file_len = buffer.len() as u64;
        let mut cursor = Cursor::new(buffer);
        let result = read_bay_trailer_stream(&mut cursor, file_len, Some("password123"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Encrypted BAYETOC length declared as 0 bytes"));
    }

    #[test]
    fn test_read_bay_trailer_rejects_file_too_small() {
        let buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(buffer);
        let result = read_bay_trailer_stream(&mut cursor, 10, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Archive file too small"));
    }
}

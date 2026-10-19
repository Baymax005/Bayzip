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

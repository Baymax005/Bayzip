use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce, Key,
};
use zeroize::Zeroize;

use crate::crypto::{derive_bay_key, ChunkedAesReader};
use crate::types::ArchiveEntryItem;

pub fn is_valid_bay_inner_sig_suffix(rest: &[u8; 6]) -> bool {
    rest == b"AYZIP\x02" || rest == b"AYBRO\x02" || rest == b"AYLZ4\x02"
}

pub fn list_bay_stream_entries<R: Read>(mut decoder: R) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_size = 0u64;

    let mut first_byte = [0u8; 1];
    let mut has_first = false;
    if decoder.read_exact(&mut first_byte).is_ok() {
        if first_byte[0] == b'B' {
            let mut rest = [0u8; 6];
            if decoder.read_exact(&mut rest).is_err() || !is_valid_bay_inner_sig_suffix(&rest) {
                return (Vec::new(), 0, 0);
            }
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
        if entry_type != 1 && entry_type != 2 {
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

        total_size = total_size.saturating_add(file_size);
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

pub const ZSTD_SEEKABLE_SKIPPABLE_MAGIC: u32 = 0x184D_2A5E;
pub const ZSTD_SEEKABLE_FOOTER_MAGIC: u32 = 0x8F92_EAB1;
pub const MAX_SEEKABLE_FRAMES: usize = 1_000_000;
pub const MAX_SEEKABLE_FRAME_BYTES: u32 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZstdSeekFrame {
    pub comp_offset: u64,
    pub comp_size: u32,
    pub decomp_offset: u64,
    pub decomp_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZstdSeekTable {
    pub frames: Vec<ZstdSeekFrame>,
    pub total_decompressed: u64,
    pub max_decomp_frame_bytes: usize,
}

/// Returns the byte offset where compressed payload ends (stripping a valid `BAYTOC\x02\x00`
/// trailer if present at EOF, otherwise returning `file_len`).
pub fn locate_bay_payload_end<R: Read + Seek>(reader: &mut R, file_len: u64) -> u64 {
    if file_len < 16 {
        return file_len;
    }
    if reader.seek(std::io::SeekFrom::End(-8)).is_err() {
        return file_len;
    }
    let mut magic = [0u8; 8];
    if reader.read_exact(&mut magic).is_err() || &magic != b"BAYTOC\x02\x00" {
        return file_len;
    }
    if reader.seek(std::io::SeekFrom::End(-16)).is_err() {
        return file_len;
    }
    let mut len_bytes = [0u8; 8];
    if reader.read_exact(&mut len_bytes).is_err() {
        return file_len;
    }
    let toc_len = u64::from_le_bytes(len_bytes);
    if toc_len > 0 && toc_len <= MAX_TOC_SIZE && toc_len <= file_len.saturating_sub(16) {
        file_len - 16 - toc_len
    } else {
        file_len
    }
}

/// Parses an RFC 8878 Zstd Seekable skippable frame (`0x184D2A5E`, footer `0x8F92EAB1`) located
/// immediately before `payload_end`, validating frame bounds and exact compressed-offset alignment.
pub fn read_zstd_seek_table<R: Read + Seek>(
    reader: &mut R,
    stream_start: u64,
    payload_end: u64,
) -> Option<ZstdSeekTable> {
    if payload_end < stream_start.saturating_add(17) {
        return None;
    }
    reader.seek(std::io::SeekFrom::Start(payload_end - 9)).ok()?;
    let mut footer = [0u8; 9];
    reader.read_exact(&mut footer).ok()?;

    let footer_magic = u32::from_le_bytes([footer[5], footer[6], footer[7], footer[8]]);
    if footer_magic != ZSTD_SEEKABLE_FOOTER_MAGIC {
        return None;
    }

    let num_frames = u32::from_le_bytes([footer[0], footer[1], footer[2], footer[3]]) as usize;
    let descriptor = footer[4];
    if !(2..=MAX_SEEKABLE_FRAMES).contains(&num_frames) || (descriptor & 0x7F) != 0 {
        return None;
    }

    let entry_size: u64 = if (descriptor & 0x80) != 0 { 12 } else { 8 };
    let table_payload_size = (num_frames as u64).checked_mul(entry_size)?.checked_add(9)?;
    if table_payload_size > u32::MAX as u64 {
        return None;
    }
    let skippable_total = table_payload_size.checked_add(8)?;
    let skippable_start = payload_end.checked_sub(skippable_total)?;
    if skippable_start < stream_start {
        return None;
    }

    reader.seek(std::io::SeekFrom::Start(skippable_start)).ok()?;
    let mut hdr = [0u8; 8];
    reader.read_exact(&mut hdr).ok()?;
    let skippable_magic = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
    let skippable_payload_len = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
    if skippable_magic != ZSTD_SEEKABLE_SKIPPABLE_MAGIC
        || skippable_payload_len != table_payload_size as u32
    {
        return None;
    }

    let entries_byte_len = num_frames.checked_mul(entry_size as usize)?;
    let mut table_buf = vec![0u8; entries_byte_len];
    reader.read_exact(&mut table_buf).ok()?;

    let mut frames = Vec::with_capacity(num_frames);
    let mut comp_cursor = stream_start;
    let mut decomp_cursor = 0u64;
    let mut max_decomp = 0usize;
    let stride = entry_size as usize;

    for chunk in table_buf.chunks_exact(stride) {
        let comp_size = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let decomp_size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        if comp_size < 8
            || decomp_size == 0
            || comp_size > MAX_SEEKABLE_FRAME_BYTES
            || decomp_size > MAX_SEEKABLE_FRAME_BYTES
        {
            return None;
        }
        frames.push(ZstdSeekFrame {
            comp_offset: comp_cursor,
            comp_size,
            decomp_offset: decomp_cursor,
            decomp_size,
        });
        comp_cursor = comp_cursor.checked_add(comp_size as u64)?;
        decomp_cursor = decomp_cursor.checked_add(decomp_size as u64)?;
        max_decomp = max_decomp.max(decomp_size as usize);
    }

    if comp_cursor != skippable_start {
        return None;
    }

    Some(ZstdSeekTable {
        frames,
        total_decompressed: decomp_cursor,
        max_decomp_frame_bytes: max_decomp,
    })
}

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
///
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
        reader.seek(std::io::SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek archive header: {}", e))?;
        let mut header_magic = [0u8; 8];
        if reader.read_exact(&mut header_magic).is_ok() && header_magic.starts_with(b"BAYENC") {
            return Err("Security violation: unencrypted BAYTOC trailer on encrypted BAYENC archive".to_string());
        }

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

        let mut key_bytes = derive_bay_key(pwd, salt);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        key_bytes.zeroize();
        let nonce = Nonce::from_slice(nonce_bytes);

        let mut decrypted = cipher.decrypt(nonce, enc_bytes.as_ref())
            .map_err(|_| "Incorrect password or corrupted BAYETOC payload".to_string())?;

        let entries_res: Result<Vec<ArchiveEntryItem>, _> = serde_json::from_slice(&decrypted);
        decrypted.zeroize();
        let entries = entries_res
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
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let payload_end = locate_bay_payload_end(&mut file, file_len);
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let bounded = file.take(payload_end.saturating_sub(offset));
        if let Ok(decoder) = zstd::stream::Decoder::new(bounded) {
            list_bay_stream_entries(decoder)
        } else {
            (Vec::new(), 0, 0)
        }
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        list_bay_stream_entries(decoder)
    } else {
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let payload_end = locate_bay_payload_end(&mut file, file_len);
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let bounded = file.take(payload_end);
        if let Ok(decoder) = zstd::stream::Decoder::new(bounded) {
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
            if decoder.read_exact(&mut rest).is_err() || !is_valid_bay_inner_sig_suffix(&rest) {
                return Err("Malformed .bay archive: invalid inner signature header".into());
            }
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
        if entry_type != 1 && entry_type != 2 {
            return Err(format!("Malformed .bay archive: invalid entry type {}", entry_type));
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
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let payload_end = locate_bay_payload_end(&mut file, file_len);
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let bounded = file.take(payload_end.saturating_sub(offset));
        let decoder = zstd::stream::Decoder::new(bounded).map_err(|e| e.to_string())?;
        read_bay_entry_from_stream(decoder, entry_name)
    } else if n >= 4 && magic_header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let decoder = lz4_flex::frame::FrameDecoder::new(file);
        read_bay_entry_from_stream(decoder, entry_name)
    } else {
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let payload_end = locate_bay_payload_end(&mut file, file_len);
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let bounded = file.take(payload_end);
        let decoder = zstd::stream::Decoder::new(bounded).map_err(|e| e.to_string())?;
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

    #[test]
    fn test_locate_bay_payload_end_and_read_zstd_seek_table_validation() {
        let stream_start = 8u64;
        let frame0 = vec![0x11u8; 120];
        let frame1 = vec![0x22u8; 80];

        let build_blob = |f0_comp: u32, f1_comp: u32, decomp_sz: u32, frame_count: u32, append_toc: bool| {
            let mut buf = Vec::new();
            buf.extend_from_slice(b"BAYZIP\x02\x00");
            buf.extend_from_slice(&frame0);
            buf.extend_from_slice(&frame1);

            let table_payload_len = frame_count * 8 + 9;
            buf.extend_from_slice(&ZSTD_SEEKABLE_SKIPPABLE_MAGIC.to_le_bytes());
            buf.extend_from_slice(&table_payload_len.to_le_bytes());
            for i in 0..frame_count {
                let cs = if i == 0 { f0_comp } else { f1_comp };
                buf.extend_from_slice(&cs.to_le_bytes());
                buf.extend_from_slice(&decomp_sz.to_le_bytes());
            }
            buf.extend_from_slice(&frame_count.to_le_bytes());
            buf.push(0u8);
            buf.extend_from_slice(&ZSTD_SEEKABLE_FOOTER_MAGIC.to_le_bytes());

            if append_toc {
                let toc = br#"[{"name":"a.bin","size":4096,"is_dir":false}]"#;
                buf.extend_from_slice(toc);
                buf.extend_from_slice(&(toc.len() as u64).to_le_bytes());
                buf.extend_from_slice(b"BAYTOC\x02\x00");
            }
            buf
        };

        // 1. Valid seek table + BAYTOC\x02\x00 trailer
        let valid = build_blob(120, 80, 256 * 1024, 2, true);
        let valid_len = valid.len() as u64;
        let mut cur = Cursor::new(valid);
        let payload_end = locate_bay_payload_end(&mut cur, valid_len);
        assert!(payload_end < valid_len);
        let table = read_zstd_seek_table(&mut cur, stream_start, payload_end).unwrap();
        assert_eq!(table.frames.len(), 2);
        assert_eq!(table.frames[0].comp_offset, 8);
        assert_eq!(table.frames[0].comp_size, 120);
        assert_eq!(table.frames[1].comp_offset, 128);
        assert_eq!(table.frames[1].comp_size, 80);
        assert_eq!(table.frames[1].decomp_offset, 256 * 1024);
        assert_eq!(table.total_decompressed, 512 * 1024);

        // 2. Tampered compressed size sum -> None
        let bad_sum = build_blob(120, 79, 256 * 1024, 2, false);
        let bad_sum_len = bad_sum.len() as u64;
        let mut cur = Cursor::new(bad_sum);
        assert!(read_zstd_seek_table(&mut cur, stream_start, bad_sum_len).is_none());

        // 3. Oversized frame exceeds MAX_SEEKABLE_FRAME_BYTES -> None
        let oversized = build_blob(120, 80, MAX_SEEKABLE_FRAME_BYTES + 1, 2, false);
        let oversized_len = oversized.len() as u64;
        let mut cur = Cursor::new(oversized);
        assert!(read_zstd_seek_table(&mut cur, stream_start, oversized_len).is_none());

        // 4. Frame count < 2 -> None
        let single_frame = build_blob(200, 0, 256 * 1024, 1, false);
        let single_len = single_frame.len() as u64;
        let mut cur = Cursor::new(single_frame);
        assert!(read_zstd_seek_table(&mut cur, stream_start, single_len).is_none());
    }
}

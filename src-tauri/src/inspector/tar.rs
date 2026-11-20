use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use crate::types::ArchiveEntryItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TarInspectKind {
    None,
    Gzip,
    Zstd,
}

fn detect_tar_inspect_kind(reader: &mut BufReader<File>, p: &Path) -> TarInspectKind {
    let mut magic = [0u8; 4];
    let n = reader.read(&mut magic).unwrap_or(0);
    let _ = reader.seek(SeekFrom::Start(0));

    if n >= 2 && magic[0] == 0x1F && magic[1] == 0x8B {
        return TarInspectKind::Gzip;
    }
    if n >= 4 && magic[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
        return TarInspectKind::Zstd;
    }
    let lower = p.to_string_lossy().to_lowercase();
    if lower.ends_with(".gz") || lower.ends_with(".tgz") {
        TarInspectKind::Gzip
    } else if lower.ends_with(".zst") || lower.ends_with(".tzst") || lower.ends_with(".zstd") {
        TarInspectKind::Zstd
    } else {
        TarInspectKind::None
    }
}

fn collect_tar_stream_entries<R: Read>(
    mut archive: ::tar::Archive<R>,
    items: &mut Vec<ArchiveEntryItem>,
    total_uncompressed: &mut u64,
) {
    if let Ok(entries) = archive.entries() {
        for entry in entries.flatten() {
            if let Ok(path) = entry.path() {
                let name = path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches('/')
                    .to_string();
                if !name.is_empty() {
                    let size = entry.size();
                    let is_dir = entry.header().entry_type().is_dir();
                    *total_uncompressed = total_uncompressed.saturating_add(size);
                    items.push(ArchiveEntryItem { name, size, is_dir });
                }
            }
        }
    }
}

fn preview_from_tar_archive<R: Read>(
    mut archive: ::tar::Archive<R>,
    target: &str,
    max_preview: usize,
) -> Result<Option<(Vec<u8>, u64)>, String> {
    let entries = archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?;
    for entry in entries.flatten() {
        if let Ok(p) = entry.path() {
            let clean = p
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
                .to_string();
            if clean == target {
                if entry.header().entry_type().is_dir() {
                    return Err("Cannot preview a folder".into());
                }
                let size = entry.size();
                let to_read = (size as usize).min(max_preview);
                let mut buf = vec![0u8; to_read];
                let mut reader = entry;
                reader
                    .read_exact(&mut buf)
                    .map_err(|e| format!("Failed to read tar entry: {}", e))?;
                return Ok(Some((buf, size)));
            }
        }
    }
    Ok(None)
}

pub fn list_tar_entries(p: &Path) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed: u64 = 0;
    if let Ok(file) = File::open(p) {
        let mut reader = BufReader::with_capacity(256 * 1024, file);
        match detect_tar_inspect_kind(&mut reader, p) {
            TarInspectKind::Gzip => {
                let gz = flate2::bufread::MultiGzDecoder::new(reader);
                collect_tar_stream_entries(::tar::Archive::new(gz), &mut items, &mut total_uncompressed);
            }
            TarInspectKind::Zstd => {
                if let Ok(zst) = zstd::stream::Decoder::with_buffer(reader) {
                    collect_tar_stream_entries(::tar::Archive::new(zst), &mut items, &mut total_uncompressed);
                }
            }
            TarInspectKind::None => {
                collect_tar_stream_entries(::tar::Archive::new(reader), &mut items, &mut total_uncompressed);
            }
        }
    }
    if items.is_empty() {
        return super::rar::list_rar_entries(p);
    }
    let count = items.len();
    (items, count, total_uncompressed)
}

pub fn preview_tar_entry_bytes(path: &Path, entry_name: &str) -> Result<(Vec<u8>, u64), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let target = entry_name.replace('\\', "/").trim_start_matches('/').to_string();
    let max_preview = 15 * 1024 * 1024;

    let hit = match detect_tar_inspect_kind(&mut reader, path) {
        TarInspectKind::Gzip => {
            let gz = flate2::bufread::MultiGzDecoder::new(reader);
            preview_from_tar_archive(::tar::Archive::new(gz), &target, max_preview)?
        }
        TarInspectKind::Zstd => {
            let zst = zstd::stream::Decoder::with_buffer(reader)
                .map_err(|e| format!("Failed to initialize tar.zst preview decoder: {}", e))?;
            preview_from_tar_archive(::tar::Archive::new(zst), &target, max_preview)?
        }
        TarInspectKind::None => {
            preview_from_tar_archive(::tar::Archive::new(reader), &target, max_preview)?
        }
    };

    if let Some(found) = hit {
        return Ok(found);
    }

    super::rar::preview_rar_entry_bytes(path, entry_name)
}

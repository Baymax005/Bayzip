use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub static SMART_BYPASS_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn is_smart_bypass_enabled() -> bool {
    SMART_BYPASS_ENABLED.load(Ordering::Relaxed)
}

pub fn set_smart_bypass(enabled: bool) {
    SMART_BYPASS_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Smart Pre-Compressed Media Bypass:
/// Detects formats that are already compressed with specialized lossless or lossy codecs
/// (e.g. H.264/HEVC/AV1 video, AAC/MP3/FLAC audio, JPEG/PNG/WebP images, and existing archives).
/// Re-compressing these files burns 100% CPU for negligible space savings (0.0% - 0.5%).
/// Bypassing compression and storing them uncompressed allows full saturation of NVMe/SSD speeds.
pub fn is_precompressed_extension(path: &Path) -> bool {
    let ext = match path.extension().and_then(|s| s.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => return false,
    };
    matches!(
        ext.as_str(),
        // Video
        "mp4" | "m4v" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "bik" |
        // Audio
        "mp3" | "m4a" | "flac" | "ogg" | "opus" | "aac" | "wma" | "ape" |
        // Images
        "jpg" | "jpeg" | "png" | "webp" | "avif" | "gif" | "heif" | "heic" | "jxl" | "tif" | "tiff" |
        // Archives & Packages
        "zip" | "zipx" | "7z" | "rar" | "gz" | "bz2" | "tbz2" | "xz" | "txz" | "zst" | "tzst" | "cab" | "iso" | "jar" | "apk" | "vpk" | "pak" |
        // Office compressed containers
        "docx" | "xlsx" | "pptx"
    )
}

/// Smart Pre-Compressed Media & Archive Detection:
/// 1. Fast Tier: In-memory file extension check (O(1), zero disk I/O).
/// 2. Deep Tier: For extensionless or generic extensions (.dat, .bin, .pak, .res, .asset),
///    peeks the first 16 bytes for compressed container magic signatures
///    (ZIP PK\x03\x04, 7-Zip, RAR, GZIP, BZIP2, XZ, Zstd, JPEG, PNG, WebP, Matroska, MP4 ftyp).
///
/// Only reads 16 bytes — NEVER reads or scans the entire file.
pub fn is_precompressed_file(path: &Path) -> bool {
    // 1. Instant extension check
    if is_precompressed_extension(path) {
        return true;
    }

    // 2. Lightweight 16-byte magic number inspection for custom/missing extensions
    if let Ok(mut f) = File::open(path) {
        let mut header = [0u8; 16];
        if let Ok(n) = f.read(&mut header) {
            if n >= 2 && &header[0..2] == b"\x1f\x8b" {
                return true; // GZIP
            }
            if n >= 3 && &header[0..3] == b"BZh" {
                return true; // BZip2
            }
            if n >= 3 && &header[0..3] == b"\xff\xd8\xff" {
                return true; // JPEG
            }
            if n >= 3 && &header[0..3] == b"ID3" {
                return true; // MP3 with ID3 tag
            }
            if n >= 4 {
                if &header[0..4] == b"PK\x03\x04" || &header[0..4] == b"PK\x05\x06" {
                    return true; // ZIP, APK, JAR, DOCX, XLSX, PPTX
                }
                if &header[0..4] == b"Rar!" {
                    return true; // RAR 4 / RAR 5
                }
                if &header[0..4] == b"\x28\xb5\x2f\xfd" {
                    return true; // Zstandard
                }
                if &header[0..4] == b"\x89PNG" {
                    return true; // PNG
                }
                if &header[0..4] == b"GIF8" {
                    return true; // GIF
                }
                if &header[0..4] == b"\x1a\x45\xdf\xa3" {
                    return true; // Matroska / WebM
                }
                if &header[0..4] == b"OggS" {
                    return true; // OGG / Opus
                }
                if &header[0..4] == b"fLaC" {
                    return true; // FLAC
                }
                if &header[0..4] == b"MSCF" {
                    return true; // Microsoft CAB
                }
            }
            if n >= 6 {
                if &header[0..6] == b"7z\xbc\xaf'\x1c" {
                    return true; // 7-Zip
                }
                if &header[0..6] == b"\xfd7zXZ\x00" {
                    return true; // XZ
                }
            }
            if n >= 8 && &header[4..8] == b"ftyp" {
                return true; // MP4, M4A, MOV, AVIF, HEIC container
            }
            if n >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP" {
                return true; // WebP
            }
        }
    }

    false
}

pub fn is_path_excluded(clean_rel: &str, excluded: Option<&std::collections::HashSet<String>>) -> bool {
    if let Some(set) = excluded {
        let clean = clean_rel.replace('\\', "/").trim_start_matches('/').trim_end_matches('/').to_lowercase();
        if set.contains(&clean) {
            return true;
        }
        for exc in set {
            let exc_clean = exc.trim_end_matches('/');
            if clean.starts_with(&format!("{}/", exc_clean)) || clean == exc_clean {
                return true;
            }
        }
    }
    false
}

pub fn compute_path_size_filtered(
    p: &Path,
    base_parent: &Path,
    excluded: Option<&std::collections::HashSet<String>>,
) -> (u64, bool) {
    if let Ok(meta) = std::fs::symlink_metadata(p) {
        if meta.is_file() {
            let rel = p.strip_prefix(base_parent).unwrap_or(p);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let clean = rel_str.trim_start_matches('/');
            if is_path_excluded(clean, excluded) {
                (0, false)
            } else {
                (meta.len(), false)
            }
        } else if meta.is_dir() {
            let mut total: u64 = 0;
            fn walk_dir(
                dir: &Path,
                base_parent: &Path,
                total: &mut u64,
                depth: usize,
                excluded: Option<&std::collections::HashSet<String>>,
            ) {
                if depth > 32 {
                    return;
                }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let ep = entry.path();
                        let rel = ep.strip_prefix(base_parent).unwrap_or(&ep);
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        let clean = rel_str.trim_start_matches('/');
                        if is_path_excluded(clean, excluded) {
                            continue;
                        }
                        if let Ok(m) = entry.metadata() {
                            if m.is_file() {
                                *total += m.len();
                            } else if m.is_dir() && !m.file_type().is_symlink() {
                                walk_dir(&ep, base_parent, total, depth + 1, excluded);
                            }
                        }
                    }
                }
            }
            let rel = p.strip_prefix(base_parent).unwrap_or(p);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let clean = rel_str.trim_start_matches('/');
            if !is_path_excluded(clean, excluded) {
                walk_dir(p, base_parent, &mut total, 0, excluded);
            }
            (total, true)
        } else {
            (0, false)
        }
    } else {
        (0, false)
    }
}

pub fn compute_path_size(p: &Path) -> (u64, bool) {
    let parent = p.parent().unwrap_or(p);
    compute_path_size_filtered(p, parent, None)
}

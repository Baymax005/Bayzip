use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;
use tauri::AppHandle;

use crate::inspector::{
    clear_readonly,
    get_optimal_extract_chunk_size,
    safe_atomic_rename,
    sanitize_archive_path,
};
use super::io::{
    create_output_file_fast,
    open_sequential_read,
    AsyncFileWriterPool,
    DirectoryCache,
    ASYNC_INMEMORY_MAX_BYTES,
};
use super::is_extraction_cancelled;
use super::utils::{
    base_folder_stem,
    cleanup_extracted_files,
    emit_extract_progress,
    is_path_excluded,
    strip_root_component,
};

const TAR_IO_BUF_SIZE: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TarCompression {
    None,
    Gzip,
    Zstd,
}

fn detect_tar_compression(magic: &[u8], archive_path: &Path) -> TarCompression {
    if magic.len() >= 2 && magic[0] == 0x1F && magic[1] == 0x8B {
        return TarCompression::Gzip;
    }
    if magic.len() >= 4 && magic[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
        return TarCompression::Zstd;
    }
    let lower = archive_path.to_string_lossy().to_lowercase();
    if lower.ends_with(".gz") || lower.ends_with(".tgz") {
        TarCompression::Gzip
    } else if lower.ends_with(".zst") || lower.ends_with(".tzst") || lower.ends_with(".zstd") {
        TarCompression::Zstd
    } else {
        TarCompression::None
    }
}

fn hoist_single_root_dir(
    dest_dir: &Path,
    actual_root: &str,
    extracted_files: &mut [String],
) -> Result<(), String> {
    let root_path = dest_dir.join(actual_root);
    if !root_path.is_dir() {
        return Ok(());
    }

    let pid = std::process::id();
    let mut counter = 0u32;
    let temp_root = loop {
        let candidate = dest_dir.join(format!(".bayzip_hoist_{}_{}", pid, counter));
        if !candidate.exists() {
            break candidate;
        }
        counter += 1;
    };

    std::fs::rename(&root_path, &temp_root).map_err(|e| {
        format!(
            "Failed to stage single-root directory {:?} during hoist: {}",
            root_path, e
        )
    })?;

    let read_res = std::fs::read_dir(&temp_root).map_err(|e| {
        format!(
            "Failed to list single-root directory {:?} during hoist: {}",
            temp_root, e
        )
    })?;

    for entry_res in read_res {
        let entry = entry_res.map_err(|e| format!("Hoist read_dir entry error: {}", e))?;
        let src = entry.path();
        let target = dest_dir.join(entry.file_name());
        std::fs::rename(&src, &target).map_err(|e| {
            format!(
                "Failed moving {:?} to {:?} during hoist: {}",
                src, target, e
            )
        })?;
    }

    let _ = std::fs::remove_dir(&temp_root);

    for path_str in extracted_files.iter_mut() {
        if let Ok(rel_to_dest) = Path::new(path_str.as_str()).strip_prefix(dest_dir) {
            let mut comps = rel_to_dest.components();
            if comps.next().is_some() {
                let hoisted_rel = comps.as_path();
                if !hoisted_rel.as_os_str().is_empty() {
                    *path_str = dest_dir.join(hoisted_rel).to_string_lossy().to_string();
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn extract_tar_entries<R: Read>(
    mut archive: tar::Archive<R>,
    dest_dir: &Path,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
    root_strip: Option<&str>,
) -> Result<Vec<String>, String> {
    let dest_was_empty = !dest_dir.exists()
        || std::fs::read_dir(dest_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);

    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create dest: {}", e))?;

    let mut dir_cache = DirectoryCache::new();
    dir_cache.mark_existing(dest_dir);

    let mut writer_pool = AsyncFileWriterPool::new_for_dest(dest_dir);
    let chunk_size = get_optimal_extract_chunk_size(dest_dir);

    let total_expected_bytes = expected_size.unwrap_or(1).max(1);
    let mut extracted_files = Vec::new();
    let mut newly_created = Vec::new();
    let mut total_extracted: u64 = 0;
    let mut last_emit = Instant::now();
    let mut io_buf = vec![0u8; TAR_IO_BUF_SIZE];

    let dest_stem = dest_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(base_folder_stem);
    let mut single_root_valid = dest_was_empty && root_strip.is_none() && dest_stem.is_some();
    let mut single_root_candidate: Option<String> = None;

    let res = (|| -> Result<(), String> {
        let entries = archive
            .entries()
            .map_err(|e| format!("Tar read entries error: {}", e))?;

        for entry_res in entries {
            if is_extraction_cancelled() {
                return Err("Extraction cancelled by user".to_string());
            }

            let mut entry = entry_res.map_err(|e| format!("Tar entry error: {}", e))?;
            let entry_path = entry
                .path()
                .map_err(|e| format!("Tar path error: {}", e))?;
            let path_str = entry_path.to_string_lossy().to_string();

            let Some(safe_path) = sanitize_archive_path(&path_str) else {
                continue;
            };
            if safe_path.as_os_str().is_empty() {
                continue;
            }

            let clean = path_str.replace('\\', "/");
            let entry_type = entry.header().entry_type();
            let is_dir = entry_type.is_dir() || clean.ends_with('/');
            if !is_dir && !entry_type.is_file() && !entry_type.is_contiguous() {
                continue;
            }

            if single_root_valid {
                let comp_count = safe_path.components().count();
                if !is_dir && comp_count == 1 {
                    single_root_valid = false;
                } else if let Some(Component::Normal(first)) = safe_path.components().next() {
                    let first_str = first.to_string_lossy();
                    match &single_root_candidate {
                        None => single_root_candidate = Some(first_str.to_string()),
                        Some(existing) if !existing.eq_ignore_ascii_case(&first_str) => {
                            single_root_valid = false;
                        }
                        Some(_) => {}
                    }
                }
            }

            if is_path_excluded(&path_str, excluded) {
                continue;
            }

            let effective_path = match root_strip {
                Some(prefix) => match strip_root_component(&safe_path, prefix) {
                    Some(stripped) => {
                        if stripped.as_os_str().is_empty() {
                            continue;
                        }
                        stripped.to_path_buf()
                    }
                    None => safe_path.clone(),
                },
                None => safe_path.clone(),
            };

            let out_path = dest_dir.join(&effective_path);

            if is_dir {
                dir_cache.ensure_dir_tracked(
                    &out_path,
                    dest_dir,
                    dest_was_empty,
                    &mut newly_created,
                )?;
                continue;
            }

            dir_cache.ensure_parent_tracked(
                &out_path,
                dest_dir,
                dest_was_empty,
                &mut newly_created,
            )?;

            let file_size = entry.size();
            let file_existed = if dest_was_empty {
                false
            } else {
                std::fs::symlink_metadata(&out_path).is_ok()
            };
            let out_str = out_path.to_string_lossy().to_string();

            if file_size <= ASYNC_INMEMORY_MAX_BYTES {
                let sz = file_size as usize;
                let mut payload = vec![0u8; sz];
                let mut read_off = 0usize;
                while read_off < sz {
                    if is_extraction_cancelled() {
                        return Err("Extraction cancelled by user".to_string());
                    }
                    let slice_end = (read_off + TAR_IO_BUF_SIZE).min(sz);
                    match entry.read(&mut payload[read_off..slice_end]) {
                        Ok(0) => {
                            return Err(format!(
                                "Premature EOF inside tar entry '{}'",
                                safe_path.display()
                            ));
                        }
                        Ok(n) => read_off += n,
                        Err(e) => return Err(format!("Tar read error: {}", e)),
                    }
                }

                if !file_existed {
                    newly_created.push(out_str.clone());
                }
                writer_pool.submit(out_path, payload, file_existed)?;
                total_extracted = total_extracted.saturating_add(file_size);

                emit_extract_progress(
                    app,
                    &mut last_emit,
                    &safe_path.to_string_lossy(),
                    total_extracted,
                    total_expected_bytes,
                );
            } else {
                let seen_before = writer_pool.prepare_direct_write(&out_path)?;
                let effective_existed = file_existed || seen_before;
                if !effective_existed {
                    newly_created.push(out_str.clone());
                }

                let target_write_path: PathBuf = if effective_existed {
                    let mut p = out_path.as_os_str().to_os_string();
                    p.push(".baypart");
                    PathBuf::from(p)
                } else {
                    out_path.clone()
                };

                let out_file = match create_output_file_fast(&target_write_path, file_size) {
                    Ok(f) => f,
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        clear_readonly(&target_write_path);
                        create_output_file_fast(&target_write_path, file_size).map_err(|e2| {
                            format!(
                                "Create output file error {:?}: {}",
                                target_write_path, e2
                            )
                        })?
                    }
                    Err(e) => {
                        return Err(format!(
                            "Create output file error {:?}: {}",
                            target_write_path, e
                        ));
                    }
                };

                let mut writer = BufWriter::with_capacity(chunk_size, out_file);
                let mut remaining = file_size;
                let mut write_err: Option<String> = None;

                while remaining > 0 {
                    if is_extraction_cancelled() {
                        write_err = Some("Extraction cancelled by user".to_string());
                        break;
                    }
                    let to_read = (remaining as usize).min(io_buf.len());
                    match entry.read(&mut io_buf[..to_read]) {
                        Ok(0) => {
                            write_err = Some(format!(
                                "Premature EOF inside tar entry '{}'",
                                safe_path.display()
                            ));
                            break;
                        }
                        Ok(n) => {
                            if let Err(e) = writer.write_all(&io_buf[..n]) {
                                write_err = Some(format!("Tar write error: {}", e));
                                break;
                            }
                            remaining -= n as u64;
                            total_extracted = total_extracted.saturating_add(n as u64);

                            emit_extract_progress(
                                app,
                                &mut last_emit,
                                &safe_path.to_string_lossy(),
                                total_extracted,
                                total_expected_bytes,
                            );
                        }
                        Err(e) => {
                            write_err = Some(format!("Tar read error: {}", e));
                            break;
                        }
                    }
                }

                if let Some(err) = write_err {
                    drop(writer);
                    let _ = std::fs::remove_file(&target_write_path);
                    return Err(err);
                }

                if let Err(e) = writer.flush() {
                    drop(writer);
                    let _ = std::fs::remove_file(&target_write_path);
                    return Err(format!("Flush error: {}", e));
                }
                drop(writer);

                if effective_existed {
                    if let Err(e) = safe_atomic_rename(&target_write_path, &out_path) {
                        let _ = std::fs::remove_file(&target_write_path);
                        return Err(format!("Failed to finalize extracted file: {}", e));
                    }
                }
            }

            extracted_files.push(out_str);
        }

        Ok(())
    })();

    if let Err(e) = res {
        writer_pool.abort_and_join();
        cleanup_extracted_files(&newly_created);
        return Err(e);
    }

    if let Err(e) = writer_pool.finish() {
        cleanup_extracted_files(&newly_created);
        return Err(e);
    }

    if single_root_valid {
        if let (Some(actual_root), Some(d_stem)) = (single_root_candidate.as_deref(), dest_stem) {
            if actual_root.eq_ignore_ascii_case(d_stem) {
                hoist_single_root_dir(dest_dir, actual_root, &mut extracted_files)?;
            }
        }
    }

    super::record_extracted_bytes(total_extracted);
    Ok(extracted_files)
}

pub fn extract_tar_archive(
    archive_path: &Path,
    dest_dir: &Path,
    expected_size: Option<u64>,
    app: Option<&AppHandle>,
    excluded: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<String>, String> {
    let raw_file = open_sequential_read(archive_path)
        .map_err(|e| format!("Failed to open tar: {}", e))?;
    let file_len = raw_file.metadata().map(|m| m.len()).unwrap_or(0);
    let read_cap = get_optimal_extract_chunk_size(archive_path).max(256 * 1024);
    let mut reader = BufReader::with_capacity(read_cap, raw_file);

    let mut magic = [0u8; 4];
    let n = reader.read(&mut magic).unwrap_or(0);
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|e| format!("Failed to rewind tar reader: {}", e))?;

    let comp = detect_tar_compression(&magic[..n], archive_path);
    let effective_expected = expected_size.filter(|&s| s > 0).or_else(|| {
        Some(match comp {
            TarCompression::None => file_len.max(1),
            TarCompression::Gzip | TarCompression::Zstd => file_len.saturating_mul(2).max(1),
        })
    });

    match comp {
        TarCompression::Gzip => {
            let gz = flate2::bufread::MultiGzDecoder::new(reader);
            let archive = tar::Archive::new(gz);
            extract_tar_entries(archive, dest_dir, effective_expected, app, excluded, None)
        }
        TarCompression::Zstd => {
            let zst = zstd::stream::Decoder::with_buffer(reader)
                .map_err(|e| format!("Failed to initialize tar.zst decoder: {}", e))?;
            let archive = tar::Archive::new(zst);
            extract_tar_entries(archive, dest_dir, effective_expected, app, excluded, None)
        }
        TarCompression::None => {
            let archive = tar::Archive::new(reader);
            extract_tar_entries(archive, dest_dir, effective_expected, app, excluded, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::ExtractionTaskGuard;
    use std::fs::File;

    fn append_tar_file(builder: &mut tar::Builder<Vec<u8>>, path: &str, data: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, data).unwrap();
    }

    fn append_tar_dir(builder: &mut tar::Builder<Vec<u8>>, path: &str) {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, path, &[][..]).unwrap();
    }

    #[test]
    fn test_extract_plain_tar_multifile_large_exclusions_and_zip_slip() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_tar_plain");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let tar_path = base.join("bundle.tar");
        let dest_dir = base.join("out");
        let large_payload: Vec<u8> = (0..(2 * 1024 * 1024 + 128 * 1024))
            .map(|i| ((i * 19) % 239) as u8)
            .collect();

        {
            let mut builder = tar::Builder::new(Vec::new());
            append_tar_dir(&mut builder, "docs/");
            append_tar_file(&mut builder, "docs/hello.txt", b"hello-tar");
            append_tar_file(&mut builder, "excluded/secret.txt", b"do-not-extract");
            append_tar_file(&mut builder, "big/data.bin", &large_payload);

            // Symlink entry (must be skipped safely)
            let mut sym_hdr = tar::Header::new_gnu();
            sym_hdr.set_entry_type(tar::EntryType::Symlink);
            sym_hdr.set_size(0);
            sym_hdr.set_mode(0o777);
            builder
                .append_link(&mut sym_hdr, "docs/bad_sym.txt", "target.txt")
                .unwrap();

            // Raw Zip-Slip header ("../escape.txt")
            let mut slip_hdr = tar::Header::new_old();
            let slip_name = b"../escape.txt";
            slip_hdr.as_old_mut().name[..slip_name.len()].copy_from_slice(slip_name);
            slip_hdr.set_entry_type(tar::EntryType::Regular);
            slip_hdr.set_size(5);
            slip_hdr.set_mode(0o644);
            slip_hdr.set_cksum();
            builder.append(&slip_hdr, &b"pwned"[..]).unwrap();

            let tar_bytes = builder.into_inner().unwrap();
            std::fs::write(&tar_path, tar_bytes).unwrap();
        }

        let excluded = crate::inspector::build_excluded_set(["excluded"]);
        let guard = ExtractionTaskGuard::new();
        let files =
            extract_tar_archive(&tar_path, &dest_dir, None, None, excluded.as_ref()).unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("docs/hello.txt")).unwrap(),
            "hello-tar"
        );
        assert_eq!(
            std::fs::read(dest_dir.join("big/data.bin")).unwrap(),
            large_payload
        );
        assert!(!dest_dir.join("excluded/secret.txt").exists());
        assert!(!dest_dir.join("docs/bad_sym.txt").exists());
        assert!(!base.join("escape.txt").exists());
        assert!(guard.bytes_extracted() > large_payload.len() as u64);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_extract_tar_gz_multimember_pigz_and_single_root_hoist() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_tar_gz_hoist");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let tgz_path = base.join("myproj.tar.gz");
        let dest_dir = base.join("myproj");

        let mut builder = tar::Builder::new(Vec::new());
        append_tar_dir(&mut builder, "myproj/");
        append_tar_dir(&mut builder, "myproj/myproj/");
        append_tar_file(&mut builder, "myproj/readme.txt", b"root-readme");
        append_tar_file(
            &mut builder,
            "myproj/myproj/nested.txt",
            b"same-name-nested-ok",
        );
        let tar_bytes = builder.into_inner().unwrap();

        // Split ONE tar stream into 2 concatenated gzip members (pigz style)
        let mid = tar_bytes.len() / 2;
        let mut tgz_bytes = Vec::new();
        {
            let mut gz1 =
                flate2::write::GzEncoder::new(&mut tgz_bytes, flate2::Compression::fast());
            gz1.write_all(&tar_bytes[..mid]).unwrap();
            gz1.finish().unwrap();
        }
        {
            let mut gz2 =
                flate2::write::GzEncoder::new(&mut tgz_bytes, flate2::Compression::fast());
            gz2.write_all(&tar_bytes[mid..]).unwrap();
            gz2.finish().unwrap();
        }
        std::fs::write(&tgz_path, tgz_bytes).unwrap();

        let guard = ExtractionTaskGuard::new();
        let files = extract_tar_archive(&tgz_path, &dest_dir, None, None, None).unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("readme.txt")).unwrap(),
            "root-readme"
        );
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("myproj/nested.txt")).unwrap(),
            "same-name-nested-ok"
        );
        assert!(!dest_dir.join("myproj/readme.txt").exists());
        assert!(guard.bytes_extracted() > 0);
        assert!(files.iter().all(|p| !p.contains("myproj\\myproj\\readme")));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_extract_tar_zst_multiframe_roundtrip() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_tar_zst");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let tzst_path = base.join("bundle.tar.zst");
        let dest_dir = base.join("out");

        let mut builder = tar::Builder::new(Vec::new());
        let big = vec![0x7Au8; 64 * 1024];
        append_tar_file(&mut builder, "alpha/first.txt", b"frame-one");
        append_tar_file(&mut builder, "beta/second.bin", &big);
        let tar_bytes = builder.into_inner().unwrap();

        // Split ONE tar stream into 2 independent Zstd frames (pzstd / zstd --mt style)
        let mid = tar_bytes.len() / 2;
        let mut zst_bytes = Vec::new();
        {
            let mut enc1 = zstd::stream::Encoder::new(&mut zst_bytes, 1).unwrap();
            enc1.write_all(&tar_bytes[..mid]).unwrap();
            enc1.finish().unwrap();
        }
        {
            let mut enc2 = zstd::stream::Encoder::new(&mut zst_bytes, 1).unwrap();
            enc2.write_all(&tar_bytes[mid..]).unwrap();
            enc2.finish().unwrap();
        }
        std::fs::write(&tzst_path, zst_bytes).unwrap();

        let guard = ExtractionTaskGuard::new();
        let files = extract_tar_archive(&tzst_path, &dest_dir, None, None, None).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("alpha/first.txt")).unwrap(),
            "frame-one"
        );
        assert_eq!(std::fs::read(dest_dir.join("beta/second.bin")).unwrap(), big);
        assert_eq!(guard.bytes_extracted(), (9 + big.len()) as u64);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_extract_tar_atomic_overwrite_and_case_duplicates() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_tar_dup");
        let _ = std::fs::remove_dir_all(&base);
        let dest_dir = base.join("out");
        std::fs::create_dir_all(dest_dir.join("sub")).unwrap();
        std::fs::write(dest_dir.join("existing.txt"), b"before").unwrap();

        let tar_path = base.join("dup.tar");
        {
            let mut builder = tar::Builder::new(Vec::new());
            append_tar_file(&mut builder, "existing.txt", b"after-existing");
            append_tar_file(&mut builder, "sub/Case.txt", b"first-case");
            append_tar_file(&mut builder, "sub/case.txt", b"second-case-wins");
            let tar_bytes = builder.into_inner().unwrap();
            std::fs::write(&tar_path, tar_bytes).unwrap();
        }

        let files = extract_tar_archive(&tar_path, &dest_dir, None, None, None).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("existing.txt")).unwrap(),
            "after-existing"
        );
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("sub/case.txt")).unwrap(),
            "second-case-wins"
        );
        assert!(!dest_dir.join("existing.txt.baypart").exists());
        assert!(!dest_dir.join("sub/case.txt.baypart").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_extract_tar_rejects_non_tar_gz_and_zst_cleanly() {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_tar_non_tar");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let raw_gz = base.join("plain.txt.gz");
        {
            let f = File::create(&raw_gz).unwrap();
            let mut gz = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
            gz.write_all(b"plain text payload shorter than 512 bytes")
                .unwrap();
            gz.finish().unwrap();
        }
        let out_gz = base.join("out_gz");
        let res_gz = extract_tar_archive(&raw_gz, &out_gz, None, None, None);
        assert!(res_gz.is_err());

        let raw_zst = base.join("plain_long.txt.zst");
        {
            let f = File::create(&raw_zst).unwrap();
            let mut zst = zstd::stream::Encoder::new(f, 1).unwrap();
            zst.write_all(&vec![b'Z'; 1024]).unwrap();
            zst.finish().unwrap();
        }
        let out_zst = base.join("out_zst");
        let res_zst = extract_tar_archive(&raw_zst, &out_zst, None, None, None);
        assert!(res_zst.is_err());

        let _ = std::fs::remove_dir_all(&base);
    }
}

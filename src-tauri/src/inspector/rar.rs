use std::io::Read;
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::types::ArchiveEntryItem;
use super::get_system32_exe;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn list_rar_entries(p: &Path) -> (Vec<ArchiveEntryItem>, usize, u64) {
    let mut items = Vec::new();
    let mut total_uncompressed = 0u64;

    // Try verbose list first (-tvf) to extract accurate entry sizes and total uncompressed bytes
    let mut cmd_verbose = Command::new(get_system32_exe("tar.exe"));
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
                    for part in parts.iter().take(name_start_col) {
                        if let Some(pos) = trimmed[search_from..].find(part) {
                            search_from += pos + part.len();
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
                        total_uncompressed = total_uncompressed.saturating_add(size);
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
        let mut cmd = Command::new(get_system32_exe("tar.exe"));
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

pub fn preview_rar_entry_bytes(path: &Path, entry_name: &str) -> Result<(Vec<u8>, u64), String> {
    let clean_name = entry_name.trim();
    if clean_name.is_empty() || clean_name.starts_with('-') {
        return Err("Invalid entry name for archive preview".into());
    }

    let mut cmd = Command::new(get_system32_exe("tar.exe"));
    // Use -- delimiter before entry_name to strictly prevent CLI flag injection
    cmd.args(["-xf", &path.to_string_lossy(), "-O", "--", clean_name]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(crate::priority::get_child_process_creation_flags());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to run tar command: {}", e))?;
    let mut stdout = child.stdout.take().ok_or_else(|| "Failed to capture stdout".to_string())?;
    let err_handle = child.stderr.take().map(|mut err_pipe| {
        std::thread::spawn(move || {
            let mut msg = String::new();
            let _ = err_pipe.read_to_string(&mut msg);
            msg
        })
    });

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

    let hit_limit = buf.len() >= max_preview;
    drop(stdout);
    if hit_limit {
        let _ = child.kill();
    }
    let exit_status = child.wait().ok();
    let stderr_msg = err_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    if buf.is_empty() {
        let succeeded = exit_status.map(|s| s.success()).unwrap_or(false);
        if succeeded && stderr_msg.trim().is_empty() {
            return Ok((buf, 0));
        }
        let err = if !stderr_msg.trim().is_empty() {
            format!("Failed to preview entry: {}", stderr_msg.trim())
        } else {
            format!("Entry '{}' not found or unreadable in archive", entry_name)
        };
        return Err(err);
    }

    let size = buf.len() as u64;
    Ok((buf, size))
}

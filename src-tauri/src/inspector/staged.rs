use std::path::Path;
use crate::types::ArchiveEntryItem;

pub fn inspect_staged_items_sync(paths: &[String]) -> Result<Vec<ArchiveEntryItem>, String> {
    let mut entries = Vec::new();
    for path_str in paths {
        let p = Path::new(path_str);
        if !p.exists() {
            continue;
        }
        let base_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("item");
        if p.is_file() {
            let meta = std::fs::symlink_metadata(p).map_err(|e| e.to_string())?;
            entries.push(ArchiveEntryItem {
                name: base_name.to_string(),
                size: meta.len(),
                is_dir: false,
            });
        } else if p.is_dir() {
            entries.push(ArchiveEntryItem {
                name: format!("{}/", base_name),
                size: 0,
                is_dir: true,
            });

            fn walk_staged_dir(dir: &Path, rel_prefix: &str, entries: &mut Vec<ArchiveEntryItem>, depth: usize) {
                if depth > 32 { return; }
                if let Ok(rd) = std::fs::read_dir(dir) {
                    for entry in rd.flatten() {
                        let path = entry.path();
                        let file_name = match path.file_name().and_then(|s| s.to_str()) {
                            Some(s) => s,
                            None => continue,
                        };
                        let rel = format!("{}/{}", rel_prefix, file_name);
                        if let Ok(m) = std::fs::symlink_metadata(&path) {
                            if m.is_dir() {
                                entries.push(ArchiveEntryItem {
                                    name: format!("{}/", rel),
                                    size: 0,
                                    is_dir: true,
                                });
                                if !m.file_type().is_symlink() {
                                    walk_staged_dir(&path, &rel, entries, depth + 1);
                                }
                            } else {
                                entries.push(ArchiveEntryItem {
                                    name: rel,
                                    size: m.len(),
                                    is_dir: false,
                                });
                            }
                        }
                    }
                }
            }
            walk_staged_dir(p, base_name, &mut entries, 0);
        }
    }
    Ok(entries)
}

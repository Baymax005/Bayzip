use std::path::Path;

fn main() {
    let out_dir = std::env::var("OUT_DIR").map(std::path::PathBuf::from).unwrap();
    let out_dll = out_dir.join("bayzip_shell.dll");
    let candidates = [
        Path::new("target/release/bayzip_shell.dll"),
        Path::new("crates/bayzip_shell/target/release/bayzip_shell.dll"),
        Path::new("target/debug/bayzip_shell.dll"),
        Path::new("crates/bayzip_shell/target/debug/bayzip_shell.dll"),
    ];

    let newest_dll = candidates
        .iter()
        .filter_map(|p| {
            let meta = std::fs::metadata(p).ok()?;
            if meta.is_file() && meta.len() > 1024 {
                let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                Some((modified, *p))
            } else {
                None
            }
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, p)| p);

    if let Some(dll_path) = newest_dll {
        let _ = std::fs::copy(dll_path, &out_dll);
    } else {
        let _ = std::fs::write(&out_dll, b"");
    }

    println!("cargo:rerun-if-changed=crates/bayzip_shell/src/lib.rs");
    println!("cargo:rerun-if-changed=crates/bayzip_shell/Cargo.toml");
    println!("cargo:rerun-if-changed=target/release/bayzip_shell.dll");

    tauri_build::build();

    let lib_resource = out_dir.join("libresource.a");
    if lib_resource.exists() {
        println!("cargo:rustc-link-arg={}", lib_resource.display());
    }
}

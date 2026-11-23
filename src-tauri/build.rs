use std::path::Path;

fn pick_newest_valid_dll<'a>(paths: &[&'a Path]) -> Option<&'a Path> {
    paths
        .iter()
        .copied()
        .filter_map(|p| {
            let meta = std::fs::metadata(p).ok()?;
            if meta.is_file() && meta.len() > 1024 {
                let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                Some((modified, p))
            } else {
                None
            }
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, p)| p)
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").map(std::path::PathBuf::from).unwrap();
    let out_dll = out_dir.join("bayzip_shell.dll");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let release_candidates = [
        Path::new("target/release/bayzip_shell.dll"),
        Path::new("crates/bayzip_shell/target/release/bayzip_shell.dll"),
    ];
    let debug_candidates = [
        Path::new("target/debug/bayzip_shell.dll"),
        Path::new("crates/bayzip_shell/target/debug/bayzip_shell.dll"),
    ];

    if profile == "release" {
        if let Some(dll_path) = pick_newest_valid_dll(&release_candidates) {
            std::fs::copy(dll_path, &out_dll).unwrap_or_else(|e| {
                panic!(
                    "Failed to copy release bayzip_shell.dll from {}: {}",
                    dll_path.display(),
                    e
                )
            });
        } else if target_os == "windows" {
            panic!(
                "Missing release bayzip_shell.dll! Build it first with: cargo build --manifest-path src-tauri/Cargo.toml -p bayzip_shell --release"
            );
        } else {
            std::fs::write(&out_dll, b"").expect("Failed to write empty shell DLL placeholder");
        }
    } else if let Some(dll_path) =
        pick_newest_valid_dll(&debug_candidates).or_else(|| pick_newest_valid_dll(&release_candidates))
    {
        std::fs::copy(dll_path, &out_dll).unwrap_or_else(|e| {
            panic!(
                "Failed to copy bayzip_shell.dll from {}: {}",
                dll_path.display(),
                e
            )
        });
    } else {
        std::fs::write(&out_dll, b"").expect("Failed to write empty shell DLL placeholder");
        println!(
            "cargo:warning=bayzip_shell.dll not found for debug build; embedding empty placeholder"
        );
    }

    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-changed=crates/bayzip_shell/src/lib.rs");
    println!("cargo:rerun-if-changed=crates/bayzip_shell/Cargo.toml");
    println!("cargo:rerun-if-changed=target/release/bayzip_shell.dll");
    println!("cargo:rerun-if-changed=crates/bayzip_shell/target/release/bayzip_shell.dll");
    println!("cargo:rerun-if-changed=target/debug/bayzip_shell.dll");
    println!("cargo:rerun-if-changed=crates/bayzip_shell/target/debug/bayzip_shell.dll");

    tauri_build::build();

    let lib_resource = out_dir.join("libresource.a");
    if lib_resource.exists() {
        println!("cargo:rustc-link-arg={}", lib_resource.display());
    }
}

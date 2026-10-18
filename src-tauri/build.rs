use std::path::Path;

fn main() {
    let release_dll = Path::new("target/release/bayzip_shell.dll");
    let crates_dir = Path::new("crates/bayzip_shell/target/release");
    let crates_dll = crates_dir.join("bayzip_shell.dll");

    if release_dll.exists() {
        let _ = std::fs::create_dir_all(crates_dir);
        let _ = std::fs::copy(release_dll, &crates_dll);
    } else if !crates_dll.exists() {
        let _ = std::fs::create_dir_all(crates_dir);
        let _ = std::fs::write(&crates_dll, b"");
    }

    println!("cargo:rerun-if-changed=crates/bayzip_shell/src");
    println!("cargo:rerun-if-changed=target/release/bayzip_shell.dll");

    tauri_build::build();
}


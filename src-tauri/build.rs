use std::path::Path;

fn main() {
    let out_dir = std::env::var("OUT_DIR").map(std::path::PathBuf::from).unwrap();
    let out_dll = out_dir.join("bayzip_shell.dll");
    let release_dll = Path::new("target/release/bayzip_shell.dll");
    let crates_dll = Path::new("crates/bayzip_shell/target/release/bayzip_shell.dll");

    if release_dll.exists() {
        let _ = std::fs::copy(release_dll, &out_dll);
    } else if crates_dll.exists() {
        let _ = std::fs::copy(crates_dll, &out_dll);
    } else {
        let _ = std::fs::write(&out_dll, b"");
    }

    println!("cargo:rerun-if-changed=crates/bayzip_shell/src");
    println!("cargo:rerun-if-changed=target/release/bayzip_shell.dll");

    tauri_build::build();
}

use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
#[link(name = "shell32")]
extern "system" {
    fn SHChangeNotify(
        w_event_id: i32,
        u_flags: u32,
        dw_item1: *const std::ffi::c_void,
        dw_item2: *const std::ffi::c_void,
    );
}

#[cfg(target_os = "windows")]
fn get_powershell_exe() -> std::path::PathBuf {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let ps = std::path::PathBuf::from(&system_root).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
    if ps.exists() {
        ps
    } else {
        std::path::PathBuf::from("powershell.exe")
    }
}

#[cfg(target_os = "windows")]
pub fn notify_shell_associations_changed() {
    unsafe {
        // SHCNE_ASSOCCHANGED = 0x08000000, SHCNF_FLUSH = 0x1000
        SHChangeNotify(0x0800_0000, 0x1000, std::ptr::null(), std::ptr::null());
    }
}

pub fn register_context_menu_sync() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            let exe_str = exe_path.to_string_lossy().to_string();
            let escaped_exe = exe_str.replace('\\', "\\\\").replace('"', "\\\"");

            let app_dir = std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("Bayzip");
            let _ = std::fs::create_dir_all(&app_dir);
            let icon_file = app_dir.join("bayzip.ico");
            let _ = std::fs::write(&icon_file, include_bytes!("../icons/icon.ico"));
            let icon_path_str = icon_file.to_string_lossy().replace('\\', "\\\\");

            // Deploy Modern Windows 11 sparse package files
            let manifest_file = app_dir.join("AppxManifest.xml");
            let _ = std::fs::write(&manifest_file, include_str!("../crates/bayzip_shell/AppxManifest.xml"));
            let icons_dir = app_dir.join("icons");
            let _ = std::fs::create_dir_all(&icons_dir);
            let _ = std::fs::write(icons_dir.join("StoreLogo.png"), include_bytes!("../icons/StoreLogo.png"));
            let _ = std::fs::write(icons_dir.join("Square150x150Logo.png"), include_bytes!("../icons/Square150x150Logo.png"));
            let _ = std::fs::write(icons_dir.join("Square44x44Logo.png"), include_bytes!("../icons/Square44x44Logo.png"));

            // Copy binary to app directory if separate
            let dest_exe = app_dir.join("bayzip.exe");
            if exe_path != dest_exe {
                let _ = std::fs::copy(&exe_path, &dest_exe);
            }

            // Deploy bayzip_shell.dll to %LOCALAPPDATA%\Bayzip\bayzip_shell.dll (Embedded directly in binary)
            let dest_dll = app_dir.join("bayzip_shell.dll");
            let embedded_dll = include_bytes!(concat!(env!("OUT_DIR"), "/bayzip_shell.dll"));
            if embedded_dll.len() > 1024 {
                let _ = std::fs::write(&dest_dll, embedded_dll);
            }
            if !dest_dll.exists() || dest_dll.metadata().map(|m| m.len()).unwrap_or(0) == 0 {
                if let Some(exe_parent) = exe_path.parent() {
                    let candidate_dll = exe_parent.join("bayzip_shell.dll");
                    if candidate_dll.exists() {
                        let _ = std::fs::copy(&candidate_dll, &dest_dll);
                    } else {
                        let build_dll = exe_parent.join("crates").join("bayzip_shell").join("target").join("release").join("bayzip_shell.dll");
                        if build_dll.exists() {
                            let _ = std::fs::copy(&build_dll, &dest_dll);
                        }
                    }
                }
            }

            let archive_exts = [
                ".bay",
                ".bayg",
                ".zip",
                ".7z",
                ".rar",
                ".tar",
                ".gz",
                ".tgz",
                ".bz2",
                ".xz",
                ".zst",
                ".iso",
                ".cab",
            ];

            let mut reg_content = String::from("Windows Registry Editor Version 5.00\r\n\r\n");

            // Explicitly delete legacy and deprecated registry associations for .baygamers / .baygamer / .bay_gamer
            reg_content.push_str(
                "[-HKEY_CURRENT_USER\\Software\\Classes\\.baygamers]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\SystemFileAssociations\\.baygamers]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.baygamers]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\.baygamer]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\SystemFileAssociations\\.baygamer]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.baygamer]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\.bay_gamer]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\SystemFileAssociations\\.bay_gamer]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.bay_gamer]\r\n\r\n"
            );

            // 1. File Association for .bay and .bayg (with clean DefaultIcon and ShellEx purging)
            reg_content.push_str(&format!(
                "[HKEY_CURRENT_USER\\Software\\Bayzip]\r\n\
                \"ExePath\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\.bay\\ShellEx]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\ShellEx]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\.bayg\\ShellEx]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\ShellEx]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.bay\\OpenWithList]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.bayg\\OpenWithList]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\.bay\\OpenWithProgids]\r\n\
                [-HKEY_CURRENT_USER\\Software\\Classes\\.bayg\\OpenWithProgids]\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay]\r\n\
                @=\"Bayzip.Archive\"\r\n\
                \"Content Type\"=\"application/x-bayzip\"\r\n\
                \"PerceivedType\"=\"compressed\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay\\DefaultIcon]\r\n\
                @=\"{icon_path_str}\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay\\OpenWithProgids]\r\n\
                \"Bayzip.Archive\"=\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive]\r\n\
                @=\"Bayzip Archive\"\r\n\
                \"FriendlyTypeName\"=\"Bayzip Archive\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\DefaultIcon]\r\n\
                @=\"{icon_path_str}\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell]\r\n\
                @=\"open\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\open]\r\n\
                @=\"Open in Bayzip\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\shell\\open\\command]\r\n\
                @=\"\\\"{escaped_exe}\\\" --gui-extract \\\"%1\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bayg]\r\n\
                @=\"Bayzip.GamerArchive\"\r\n\
                \"Content Type\"=\"application/x-bayzip-gamer\"\r\n\
                \"PerceivedType\"=\"compressed\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bayg\\DefaultIcon]\r\n\
                @=\"{icon_path_str}\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bayg\\OpenWithProgids]\r\n\
                \"Bayzip.GamerArchive\"=\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive]\r\n\
                @=\"Bayzip Gamer Mod Pack\"\r\n\
                \"FriendlyTypeName\"=\"Bayzip Gamer Mod Pack\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\DefaultIcon]\r\n\
                @=\"{icon_path_str}\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\shell]\r\n\
                @=\"open\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\shell\\open]\r\n\
                @=\"Open in Bayzip\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\shell\\open\\command]\r\n\
                @=\"\\\"{escaped_exe}\\\" --gui-extract \\\"%1\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.bay\\OpenWithProgids]\r\n\
                \"Bayzip.Archive\"=hex(0):\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.bayg\\OpenWithProgids]\r\n\
                \"Bayzip.GamerArchive\"=hex(0):\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Bayzip\\Capabilities]\r\n\
                \"ApplicationDescription\"=\"Bayzip Archiver\"\r\n\
                \"ApplicationName\"=\"Bayzip\"\r\n\r\n\
                [-HKEY_CURRENT_USER\\Software\\Bayzip\\Capabilities\\FileAssociations]\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Bayzip\\Capabilities\\FileAssociations]\r\n\
                \".bay\"=\"Bayzip.Archive\"\r\n\
                \".bayg\"=\"Bayzip.GamerArchive\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\RegisteredApplications]\r\n\
                \"Bayzip\"=\"Software\\\\Bayzip\\\\Capabilities\"\r\n\r\n"
            ));

            // 2. Register "Extract with Bayzip" for each archive format under SystemFileAssociations and Bayzip ProgIDs
            let mut extract_targets = vec!["Bayzip.Archive".to_string(), "Bayzip.GamerArchive".to_string()];
            for ext in &archive_exts {
                extract_targets.push(format!("SystemFileAssociations\\{}", ext));
            }

            for target in extract_targets {
                reg_content.push_str(&format!(
                    "[-HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract]\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract]\r\n\
                    \"MUIVerb\"=\"Extract with Bayzip\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\
                    \"SubCommands\"=\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell]\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\01_extract_dialog]\r\n\
                    \"MUIVerb\"=\"Extract files...\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\01_extract_dialog\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --gui-extract \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\02_extract_sub]\r\n\
                    \"MUIVerb\"=\"Extract to subfolder\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\02_extract_sub\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --extract-to-folder \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\03_extract_here]\r\n\
                    \"MUIVerb\"=\"Extract Here\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\03_extract_here\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --extract-here \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\04_open]\r\n\
                    \"MUIVerb\"=\"Open in Bayzip\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipExtract\\shell\\04_open\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --gui-extract \\\"%1\\\"\"\r\n\r\n"
                ));
            }

            // 3. Register "Add to archive with Bayzip" for * (all files) and Directory (folders)
            for target in &["*", "Directory"] {
                reg_content.push_str(&format!(
                    "[-HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress]\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress]\r\n\
                    \"MUIVerb\"=\"Add to archive with Bayzip\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\
                    \"SubCommands\"=\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell]\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\00_options]\r\n\
                    \"MUIVerb\"=\"Add to archive...\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\00_options\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --gui-compress \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\01_bay]\r\n\
                    \"MUIVerb\"=\"Add to .bay (Ultra-Fast)\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\01_bay\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --quick-compress .bay \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\02_zip]\r\n\
                    \"MUIVerb\"=\"Add to .zip (Standard)\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\02_zip\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --quick-compress .zip \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\03_7z]\r\n\
                    \"MUIVerb\"=\"Add to .7z (High Ratio)\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\03_7z\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --quick-compress .7z \\\"%1\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\04_tar]\r\n\
                    \"MUIVerb\"=\"Add to .tar (Universal)\"\r\n\
                    \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                    [HKEY_CURRENT_USER\\Software\\Classes\\{target}\\shell\\BayzipCompress\\shell\\04_tar\\command]\r\n\
                    @=\"\\\"{escaped_exe}\\\" --quick-compress .tar \\\"%1\\\"\"\r\n\r\n"
                ));
            }

            // Write as UTF-16 LE with BOM (Required by Windows reg.exe) in private app directory (V-SEC-05)
            let temp_reg = app_dir.join(format!("bayzip_reg_{}_{}.reg", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
            let mut utf16_bytes: Vec<u8> = vec![0xFF, 0xFE];
            for u in reg_content.encode_utf16() {
                utf16_bytes.extend_from_slice(&u.to_le_bytes());
            }

            if std::fs::write(&temp_reg, utf16_bytes).is_ok() {
                let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
                let reg_exe = std::path::PathBuf::from(&system_root).join("System32\\reg.exe");
                let mut cmd = Command::new(if reg_exe.exists() { reg_exe } else { std::path::PathBuf::from("reg.exe") });
                cmd.args(["import", &temp_reg.to_string_lossy()]);
                cmd.creation_flags(CREATE_NO_WINDOW);
                let _ = cmd.output();
                let _ = std::fs::remove_file(temp_reg);

                // Refresh Windows Explorer shell & clear icon cache
                let mut refresh_cmd = Command::new(crate::inspector::get_system32_exe("ie4uinit.exe"));
                refresh_cmd.arg("-show");
                refresh_cmd.creation_flags(CREATE_NO_WINDOW);
                let _ = refresh_cmd.output();

                #[cfg(target_os = "windows")]
                notify_shell_associations_changed();

                // Register Windows 11 Modern Context Menu (Sparse Package Identity)
                let mut is_win11 = false;
                let mut ver_cmd = Command::new(get_powershell_exe());
                ver_cmd.args(["-NoProfile", "-NonInteractive", "-Command", "[System.Environment]::OSVersion.Version.Build"]);
                ver_cmd.creation_flags(CREATE_NO_WINDOW);
                if let Ok(output) = ver_cmd.output() {
                    if let Ok(s) = String::from_utf8(output.stdout) {
                        if let Ok(build) = s.trim().parse::<u64>() {
                            if build >= 22000 {
                                is_win11 = true;
                            }
                        }
                    }
                }

                if is_win11 && dest_dll.exists() && manifest_file.exists() {
                    let manifest_escaped = manifest_file.to_string_lossy().replace('\'', "''");
                    let app_dir_escaped = app_dir.to_string_lossy().replace('\'', "''");
                    let mut ps_cmd = Command::new(get_powershell_exe());
                    let ps_script = format!(
                        "Add-AppxPackage -Register '{}' -ExternalLocation '{}' -ErrorAction SilentlyContinue",
                        manifest_escaped,
                        app_dir_escaped
                    );
                    ps_cmd.args(["-NoProfile", "-NonInteractive", "-Command", &ps_script]);
                    ps_cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = ps_cmd.output();
                }

                // Force Windows Shell and File Explorer to flush icon cache and reload associations
                #[cfg(target_os = "windows")]
                {
                    notify_shell_associations_changed();

                    let mut refresh_cmd = Command::new(crate::inspector::get_system32_exe("ie4uinit.exe"));
                    refresh_cmd.arg("-show");
                    refresh_cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = refresh_cmd.output();
                }

                return Ok("Context menu registered successfully!".into());
            }
        }
        Err("Could not determine executable path.".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("Context menu not applicable on non-Windows OS".into())
    }
}

pub fn unregister_context_menu_sync() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        // 1. Unregister Windows 11 Modern Context Menu Sparse Package
        let mut ps_cmd = Command::new(get_powershell_exe());
        ps_cmd.args(["-NoProfile", "-NonInteractive", "-Command", "Get-AppxPackage -Name 'Bayzip.ShellExtension' | Remove-AppxPackage -ErrorAction SilentlyContinue"]);
        ps_cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = ps_cmd.output();

        // 2. Unregister Classic Registry Shell Integration
        let mut reg_content = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
        reg_content.push_str(
            "[-HKEY_CURRENT_USER\\Software\\Bayzip]\r\n\
            [-HKEY_CURRENT_USER\\Software\\Classes\\*\\shell\\BayzipCompress]\r\n\
            [-HKEY_CURRENT_USER\\Software\\Classes\\Directory\\shell\\BayzipCompress]\r\n\
            [-HKEY_CURRENT_USER\\Software\\Classes\\.bay]\r\n\
            [-HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive]\r\n\
            [-HKEY_CURRENT_USER\\Software\\Classes\\.bayg]\r\n\
            [-HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive]\r\n\
            [-HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.bay]\r\n\
            [-HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.bayg]\r\n\
            [HKEY_CURRENT_USER\\Software\\RegisteredApplications]\r\n\
            \"Bayzip\"=-\r\n\r\n"
        );

        let archive_exts = [
            ".bay", ".bayg", ".zip", ".7z", ".rar", ".tar", ".gz", ".tgz", ".bz2", ".xz", ".zst", ".iso", ".cab",
        ];
        for ext in &archive_exts {
            reg_content.push_str(&format!(
                "[-HKEY_CURRENT_USER\\Software\\Classes\\SystemFileAssociations\\{ext}\\shell\\BayzipExtract]\r\n"
            ));
        }

        let app_dir = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Bayzip");
        let _ = std::fs::create_dir_all(&app_dir);
        let temp_reg = app_dir.join(format!("bayzip_unreg_{}_{}.reg", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
        let mut utf16_bytes: Vec<u8> = vec![0xFF, 0xFE];
        for u in reg_content.encode_utf16() {
            utf16_bytes.extend_from_slice(&u.to_le_bytes());
        }

        if std::fs::write(&temp_reg, utf16_bytes).is_ok() {
            let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
            let reg_exe = std::path::PathBuf::from(&system_root).join("System32\\reg.exe");
            let mut cmd = Command::new(if reg_exe.exists() { reg_exe } else { std::path::PathBuf::from("reg.exe") });
            cmd.args(["import", &temp_reg.to_string_lossy()]);
            cmd.creation_flags(CREATE_NO_WINDOW);
            let _ = cmd.output();
            let _ = std::fs::remove_file(temp_reg);

            #[cfg(target_os = "windows")]
            notify_shell_associations_changed();

            return Ok("Context menu unregistered successfully!".into());
        }
        Err("Could not unregister context menu.".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("Context menu not applicable on non-Windows OS".into())
    }
}

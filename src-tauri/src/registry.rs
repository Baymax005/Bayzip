use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn register_context_menu_sync() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            let exe_str = exe_path.to_string_lossy().to_string();
            let escaped_exe = exe_str.replace('\\', "\\\\").replace('"', "\\\"");

            let app_dir = std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir())
                .join("Bayzip");
            let _ = std::fs::create_dir_all(&app_dir);
            let icon_file = app_dir.join("bayzip.ico");
            let _ = std::fs::write(&icon_file, include_bytes!("../icons/icon.ico"));
            let icon_path_str = icon_file.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");

            let archive_exts = [
                ".bay",
                ".bayg",
                ".zip",
                ".7z",
                ".rar",
                ".tar",
                ".gz",
                ".tgz",
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

            // 1. File Association for .bay and .bayg
            reg_content.push_str(&format!(
                "[HKEY_CURRENT_USER\\Software\\Classes\\.bay]\r\n\
                @=\"Bayzip.Archive\"\r\n\
                \"Content Type\"=\"application/x-bayzip\"\r\n\
                \"PerceivedType\"=\"compressed\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay\\DefaultIcon]\r\n\
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay\\OpenWithProgids]\r\n\
                \"Bayzip.Archive\"=\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bay\\ShellEx\\{{e357fccd-a995-4576-b01f-234630154e96}}]\r\n\
                @=\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive]\r\n\
                @=\"Bayzip Archive\"\r\n\
                \"FriendlyTypeName\"=\"Bayzip Archive\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\DefaultIcon]\r\n\
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.Archive\\ShellEx\\{{e357fccd-a995-4576-b01f-234630154e96}}]\r\n\
                @=\"\"\r\n\r\n\
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
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bayg\\OpenWithProgids]\r\n\
                \"Bayzip.GamerArchive\"=\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\.bayg\\ShellEx\\{{e357fccd-a995-4576-b01f-234630154e96}}]\r\n\
                @=\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive]\r\n\
                @=\"Bayzip Gamer Mod Pack\"\r\n\
                \"FriendlyTypeName\"=\"Bayzip Gamer Mod Pack\"\r\n\
                \"Icon\"=\"\\\"{escaped_exe}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\DefaultIcon]\r\n\
                @=\"\\\"{icon_path_str}\\\"\"\r\n\r\n\
                [HKEY_CURRENT_USER\\Software\\Classes\\Bayzip.GamerArchive\\ShellEx\\{{e357fccd-a995-4576-b01f-234630154e96}}]\r\n\
                @=\"\"\r\n\r\n\
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

            // Write as UTF-16 LE with BOM (Required by Windows reg.exe)
            let temp_reg = std::env::temp_dir().join("bayzip_context.reg");
            let mut utf16_bytes: Vec<u8> = vec![0xFF, 0xFE];
            for u in reg_content.encode_utf16() {
                utf16_bytes.extend_from_slice(&u.to_le_bytes());
            }

            if std::fs::write(&temp_reg, utf16_bytes).is_ok() {
                let mut cmd = Command::new("reg");
                cmd.args(["import", &temp_reg.to_string_lossy()]);
                cmd.creation_flags(CREATE_NO_WINDOW);
                let _ = cmd.output();
                let _ = std::fs::remove_file(temp_reg);

                // Refresh Windows Explorer shell & clear icon cache
                let mut refresh_cmd = Command::new("cmd");
                refresh_cmd.args(["/C", "ie4uinit.exe -ClearIconCache & ie4uinit.exe -show"]);
                refresh_cmd.creation_flags(CREATE_NO_WINDOW);
                let _ = refresh_cmd.output();

                #[cfg(target_os = "windows")]
                {
                    #[link(name = "shell32")]
                    extern "system" {
                        fn SHChangeNotify(
                            w_event_id: i32,
                            u_flags: u32,
                            dw_item1: *const std::ffi::c_void,
                            dw_item2: *const std::ffi::c_void,
                        );
                    }
                    unsafe {
                        // SHCNE_ASSOCCHANGED = 0x08000000, SHCNF_IDLIST = 0
                        SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null());
                    }
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

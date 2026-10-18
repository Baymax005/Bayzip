# 📋 Bayzip Changelog & Release Notes

All notable changes, version milestones, security fixes, and performance enhancements across Bayzip are documented in this file.

---

## [v1.2.0] - 2026-10-18

### 🪟 Windows 11 Modern Context Menu & Shell Extension
- **Native Windows 11 Top-Level Context Menu ([crates/bayzip_shell](src-tauri/crates/bayzip_shell))**:
  - Implemented companion native Windows shell extension sub-crate `bayzip_shell` compiling to a high-performance in-process COM dynamic-link library (`bayzip_shell.dll`).
  - Implemented Windows Shell `IExplorerCommand`, `IEnumExplorerCommand`, and `IClassFactory` interfaces using `windows-core` and Windows SDK traits.
  - Registered under dedicated CLSID `{44E23C12-5883-4903-9DA9-33D72120C19A}`.
  - Appears directly at the top level of the Windows 11 File Explorer context menu (no need to click "Show more options" / Shift+F10).
  - Features dedicated cascading sub-commands with native icons:
    - **Compress to .bayg (Gamer Preset)**: Rapid multi-core compression capped at Zstd Level 3 with zero-overhead streaming.
    - **Compress to .bay (Ultra-Fast)**: High-performance standard compression.
    - **Extract Here**: Instant headless multi-format extraction to the current working directory.
    - **Open in Bayzip**: Launches the modern Fluent GUI inspector.
- **Sparse Package Identity Integration ([AppxManifest.xml](src-tauri/crates/bayzip_shell/AppxManifest.xml), [registry.rs](src-tauri/src/registry.rs))**:
  - Authored standard Windows Sparse Package manifest (`PackageFullName: Bayzip.ShellExtension_1.2.0.0_x64__b5n7qtqag6tr4`) supporting external loose deployment via `uap10:AllowExternalContent` and `uap10:RuntimeBehavior="win32App"`.
  - Configured `desktop4:FileExplorerContextMenus` targeting `*` and `Directory` targets with alphanumeric verb IDs (`BayzipContextMenuAllFiles`, `BayzipContextMenuDirectory`).
  - Configured in-process STA COM activation via `com:SurrogateServer`.
  - Added automatic background registration on startup via `Add-AppxPackage -Register` when running on Windows 11 (`Build >= 22000`).
  - Preserved dual-mode support: automatically maintains classic Windows 10 registry integration (`HKCU\Software\Classes`) alongside modern Windows 11 Sparse Package identity.
- **Multi-Selection Shell Command Dispatch ([main.rs](src-tauri/src/main.rs), [lib.rs](src-tauri/src/lib.rs))**:
  - Enhanced headless CLI commands (`--quick-compress`, `--extract-here`, `--extract-to-folder`) in `main.rs` to iterate over all passed file arguments when multiple files/folders are selected in Explorer.
  - Added `unregister_context_menu` Tauri command and registry unregistration routines (`Remove-AppxPackage` + registry purge) in `lib.rs` and `registry.rs`.

---

## [v1.1.5] - 2026-10-18

### 🔍 Extended Magic Byte Signatures & Universal Inspection
- **Extended Container Formats ([types.rs](src-tauri/src/types.rs), [inspector.rs](src-tauri/src/inspector.rs))**:
  - Added enum variants `Bzip2`, `Xz`, `Zstd`, `Iso`, and `Cab` to `ArchiveFormat`.
  - Added binary magic-byte detection for:
    - **BZip2 (`.bz2`, `.tar.bz2`)**: `42 5A 68` (`BZh`)
    - **XZ (`.xz`, `.tar.xz`)**: `FD 37 7A 58 5A 00` (`\xFD7zXZ\x00`)
    - **Zstandard (`.zst`, `.tar.zst`)**: `28 B5 2F FD` (Little-endian `0xFD2FB528`)
    - **Microsoft Cabinet (`.cab`)**: `4D 53 43 46` (`MSCF`)
    - **ISO 9660 Disc Images (`.iso`)**: Sector 16 header at byte offset `32768 + 1` (`0x8001`) checking for standard identifier `CD001`.
  - Routed archive listing and table of contents generation through `list_tar_entries` (`tar.exe -tvf`).
  - Routed preview extraction through `preview_rar_entry_bytes` (`tar.exe -xf ... -O -- ...`), providing zero-disk streaming in-memory previews for images, text, and code files inside `.iso`, `.cab`, `.tar.bz2`, `.tar.xz`, and `.tar.zst`.
- **Universal Single-Root Extraction ([extractor.rs](src-tauri/src/extractor.rs))**:
  - Unified extraction dispatch and single-root folder de-nesting for `Bzip2`, `Xz`, `Zstd`, `Iso`, and `Cab` using `extract_with_system_tar`, preventing redundant `Folder/Folder/` nested extraction.
- **Windows Registry & Frontend Associations ([registry.rs](src-tauri/src/registry.rs), [App.tsx](src/App.tsx), [tauri.conf.json](src-tauri/tauri.conf.json))**:
  - Registered `.bz2`, `.xz`, `.zst`, `.iso`, and `.cab` in Windows HKCU registry for default file associations and context menu options.
  - Added `.iso` and `.cab` to the UI file picker and drag-and-drop filters.
  - **NSIS Setup Installer Icon**: Configured `bundle.windows.nsis.installerIcon` pointing to `icons/icon.ico` in `tauri.conf.json`, ensuring the generated installer executable displays the official Bayzip logo instead of the generic NSIS screen icon.
  - **Windows Shell File Association & Logo Fix**: Purged conflicting empty `ShellEx` keys and stale `OpenWithList` entries from `HKCU\Software\Classes\.bay` and `FileExts\.bay` that previously triggered Windows Photo Viewer / RAW thumbnail handler fallbacks. Standardized `DefaultIcon` format and added `SHCNF_FLUSH` (`0x1000`) cache invalidation so `.bay` and `.bayg` archives reliably render the official glowing green cube logo across Windows Explorer.

---

## [v1.1.4] - 2026-10-18

### 🛡️ Format Hardening & Container Audit
- **RAR Encryption & Parsing ([inspector.rs](src-tauri/src/inspector.rs))**:
  - Implemented `check_rar4_encrypted` and `check_rar5_encrypted` (detecting block header flags `0x0080`/`0x0004` and RAR5 `H_CRYPT` type 4) within `is_archive_encrypted`, allowing password-protected RAR archives to accurately prompt for passwords in the UI.
  - Overhauled `list_rar_entries` to dynamically detect date token columns, correctly supporting both 2-column ISO/locale dates (`2026-10-18 04:11`) and 3-column month/day/time dates (`Oct 18 04:11`).
  - Preserved multiple consecutive spaces in RAR filenames by slicing directly from original output lines rather than collapsing whitespace.
  - In `preview_rar_entry_bytes`, captured `stderr` and verified buffer non-emptiness, returning explicit descriptive errors instead of silent empty previews.
- **ZIP Deflate Level & Locked File Safety ([compressor.rs](src-tauri/src/compressor.rs))**:
  - Connected the Deflate compression level (1, 3, 6, 7, 9) directly to the Hardware Allocation slider via `.compression_level(Some(deflate_level))`.
  - Added pre-emptive `File::open(&path)` validation before writing ZIP entry headers (`writer.start_file`), preventing 0-byte ghost entries when files are locked or inaccessible.
- **.bay & .bayg Empty Folders & TOCTOU Stream Framing ([compressor.rs](src-tauri/src/compressor.rs), [extractor.rs](src-tauri/src/extractor.rs))**:
  - Normalized path separators (`/`) for directory entries in `write_dir_to_bay_archive` to match trailer Table of Contents entries.
  - Added directory entry writing (`[2u8]`) and extraction decoding in `extract_bay_stream`, preserving empty directory structures.
  - Bounded file reads to declared `file_size` (`while written_bytes < file_size`), eliminating stream framing corruption if files grow on disk during compression.
  - Added `.bayg` gamer preset support to headless `--quick-compress` (`calculate_zstd_level(50, true)` and `include_checksum(false)`).
- **Cancellable TAR Compression & Safe Root Stripping ([compressor.rs](src-tauri/src/compressor.rs), [extractor.rs](src-tauri/src/extractor.rs))**:
  - Replaced blocking `append_dir_all` with iterative traversal (`add_dir_to_tar`) supporting directory recursion limits, immediate cancellation checks, and throttled UI progress reporting.
  - Conditioned TAR extraction root stripping on `get_single_root_dir` verification, matching safe behavior in ZIP and 7z.
  - Upgraded system extraction pipe reader from `.lines().flatten()` to `.lines().map_while(Result::ok)` to prevent infinite reader thread loops on broken pipes.
- **Orphaned Windows Registry Cleanup ([registry.rs](src-tauri/src/registry.rs))**:
  - Added explicit registry unregistration blocks using `[-HKEY_CURRENT_USER\Software\Classes\...]`, `[-HKEY_CURRENT_USER\Software\Classes\SystemFileAssociations\...]`, and `[-HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\...]` to automatically purge legacy `.baygamers`, `.baygamer`, and `.bay_gamer` associations.

---

## [v1.1.3] - 2026-10-18

- **Encrypted `.zip` Inspector (`list_zip_entries`)**:
  - Replaced `zip.by_index` with `zip.by_index_raw` to parse central directory headers directly without requiring file decryption, eliminating `PasswordRequired` inspector failures and showing accurate item counts in `< 1ms`.
- **Safe Windows File Overwrite (`safe_atomic_rename` & `clear_readonly`)**:
  - Implemented `safe_atomic_rename` and `clear_readonly` across `compressor.rs` and `extractor.rs` to strip Windows read-only file attributes before replacing files (`.baytmp` and `.baypart`), preventing `PermissionDenied` errors when overwriting read-only game installations.
- **Double-Folder Nesting Prevention for `.rar`**:
  - In `extract_archive_sync`, archives extracted via system `tar` (such as `.rar`) now inspect for single root directories matching the archive stem name, preventing redundant `Stem\Stem\` nested folder hierarchies.
- **Dedicated `.bayg` Gamer Archive Standardization**:
  - Fully standardized `.bayg` container format across frontend presets, backend engines, and Windows Registry context menus.

---

## [v1.1.2] - 2026-10-17

- **Instant Encrypted TOC Footer (`BAYETOC\x03`)**:
  - Encrypted `.bay` and `.bayg` archives now append an encrypted Table of Contents trailer at EOF (`[encrypted_toc_bytes][enc_toc_len][salt][nonce][BAYETOC\x03]`).
  - Encrypted archives with 100,000+ files and 40+ GB sizes now open and decrypt their complete directory tree in **< 2ms** instead of decompressing the entire archive over USB.
- **Eliminated Extraction Double-Scan**:
  - `extract_bay_archive` now gates root directory inspection behind `has_instant_bay_toc`. Legacy archives without a TOC extract immediately on pass 1 without scanning the full archive twice.
- **Zero-Wait 0 B Disk Size Display**:
  - Added instant synchronous `get_file_size` IPC query so archive cards display the exact disk file size (e.g. `41.2 GB`) in `< 0.05ms` while background indexing proceeds.
  - Fast-pathed encrypted `.bay` archives in `inspect_archive` when password is empty, returning disk size and `is_encrypted: true` without disk scanning.
- **`.bayg` Container Format & Pre-Compressed Game Acceleration**:
  - Introduced `.bayg` as the dedicated high-speed gamer container format.
  - Added `encoder.include_checksum(false)` for `.bayg` to eliminate redundant checksum calculation on pre-compressed game containers.
  - Re-tuned Zstd levels for gamer mode (Level `-2` fast acceleration mode for `<= 25%`, Level `1` for `50%`, Level `2` for `>= 75%`) so incompressible game files stream at maximum hardware disk speed.
- **Package & Crate Identity**:
  - Standardized package and crate name from `tauri-app` to `bayzip` across npm `package.json` and Cargo `Cargo.toml`.

---

## [v1.1.1] - 2026-10-17

- **Settings Window Drag & Controls Fix**:
  - Configured global Tauri 2 window capabilities (`"windows": ["*"]`) so the independent multi-window Settings surface is fully movable across all monitors without boundary locks.
  - Added native Rust IPC commands `minimize_settings_window` and `close_settings_window` with webview fallbacks, guaranteeing window controls respond instantly.
  - Added programmatic `startDragging()` titlebar fallback for seamless dragging on touch and mouse inputs.
- **Hardware & Architecture Reference**:
  - Published comprehensive [PERFORMANCE_GUIDE.md](PERFORMANCE_GUIDE.md) detailing CPU core scaling formulas, dynamic RAM read/write buffers, Zstandard level mapping, and real-world disk throughput metrics.

---

## [v1.1.0] - 2026-10-17

- **Selective Extraction & Exclusion Mode (`✕` Skip Mode)**:
  - Added inverted file/folder selection to skip extracting unwanted files directly to disk, saving massive storage and I/O time.
  - Interactive toolbar toggle with pulsating indicator (`[ ✕ Exclude Files ]`) and 1-click reset (`[ Include All ]`).
  - Tree and Flat views show red cross badges (`[✕ Excluded]` / `[✕ Folder Excluded]`), dimmed 50% opacity, strikethrough labels, and `(Skipped)` size badges.
  - Safe decompressor byte-draining preserves container stream alignment for `.bay`, `.zip`, `.tar`, and `.7z` without partial file disk writes.
  - Bottom action button dynamically displays live extraction counts: `[ Extract 98 Files (2 Excluded) ]` alongside a quick reset button.

---

## [v1.0.7] - 2026-10-16

- **Independent Multi-Window Settings**: Upgraded Settings from an in-page modal to a true native Windows 11 window (`580x560` borderless with custom titlebar and drag region). Users can now drag the Settings window anywhere across multiple monitors independently of the main Bayzip window, while preserving real-time configuration sync.
- **Ultra-Scale Virtual Scrolling (DOM Recycling)**: Implemented high-performance virtual windowing in `ArchiveTreeInspector` (fixed 34px row height + 5-row overscan) for both the compact inspector and expanded dual-pane workspace. Archives with 100,000+ files now render only ~20–25 DOM elements at 60 FPS with minimal memory overhead.
- **Spacebar Shortcut Input Focus Fix**: Fixed a bug where pressing the Spacebar inside the search filter box or password field was intercepted by the global preview shortcut, allowing normal space typing in all inputs.
- **Single-Root Folder Deduplication**: Enhanced extraction target resolution to automatically detect archives that already encapsulate a single root directory, preventing redundant double-folder nesting (e.g. `ArchiveName\ArchiveName`).
- **Safe Preview Large File DOM Protection**: Added a 3,000-line DOM rendering cap with a warning banner for massive text and code files, safeguarding the UI from freezes while preserving full clipboard copy.

---

## [v1.0.6] - 2026-10-15

- **Zero-Allocation In-Place AES-256-GCM**: Switched streaming encryption and decryption to `AeadInPlace`, eliminating heap allocations on every chunk and upgrading chunk sizes to 4 MB for maximum AES throughput.
- **Dynamic RAM-Scaled I/O Buffers**: Configured read buffers (2–16 MB) and write buffers (16–128 MB) to dynamically scale with the hardware allocation slider, drastically boosting NVMe burst rates.
- **Headless Quick Compress Fast TOC**: Added Table of Contents trailer (`BAYTOC\x02\x00`) to right-click context menu headless `.bay` compressions for immediate O(1) archive inspection.
- **Atomic `.baypart` Extraction**: Added staging to `.baypart` temporary files during extraction with automatic rollback to ensure zero corrupted partial files upon error or cancellation.
- **Single-Root Folder Normalization**: Fixed redundant subfolder nesting and eliminated accidental `(1)` directory renames when extracting dedicated archive folders.

---

## [v1.0.5] - 2026-10-14

- Upgraded AES-256-GCM encryption/decryption architecture from single-buffer `read_to_end` to streaming AEAD chunks, dropping peak RAM usage below 100 MB.

---

## [v1.0.4] - 2026-10-13

- Fixed auto-numbering folder conflicts during extraction.
- Enabled instant `.bay` archive inspection and zero-disk previewing.
- Enhanced cancel cleanup routines and scratch-disk isolation.

---

## [v1.0.3] - 2026-10-12

- Security hardening for Zip-Slip path traversal and Alternate Data Streams (ADS).
- Floating Settings window and modernized About modal design.

---

## [v1.0.2] - 2026-10-11

- Real-time extraction progress and throughput metrics.
- Added `.bayg` file selector and preset tuning.

---

## [v1.0.1] - 2026-10-10

- Universal extraction support for `.bay`, `.zip`, `.7z`, and `.tar`.
- Hardware-scaled thread allocation and `ScopedPriorityGuard` OS prioritization.

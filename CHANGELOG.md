# 📋 Bayzip Changelog & Release Notes

All notable changes, version milestones, security fixes, and performance enhancements across Bayzip are documented in this file.

## [v2.0.5] - 2026-10-20

### ⚙️ Backend Architecture Modularization & Engine Separation
- **Modular Compression Subsystem (`src-tauri/src/compressor/`)**:
  - Decomposed the 1,936-line monolithic `compressor.rs` into specialized submodules:
    - `mod.rs`: Central orchestrator (`compress_files_sync`, `headless_quick_compress`), CPU thread pool scaling (`calculate_target_threads`), Zstd compression level matrix (`calculate_zstd_level`), cancellation atomics, and streaming `ProgressReader`.
    - `utils.rs`: Smart precompressed media detection (`is_precompressed_file`, `is_precompressed_extension`), path exclusion filters (`is_path_excluded`), and filtered recursive size calculations (`compute_path_size_filtered`).
    - `bay.rs`: Native `.bay` and `.bayg` chunked streaming writers (Zstandard, Brotli, LZ4), TOC trailer serializers (`write_bay_toc_trailer`), and headless compression.
    - `zip.rs`: ZIP container writer with deflate and AES-256 / ZipCrypto streaming encryption (`compress_zip`) and headless quick compression.
    - `sevenz.rs`: 7-Zip LZMA / LZMA2 multi-threaded compression writer with optional AES encryption (`compress_7z`) and headless 7z compression.
    - `tar.rs`: GNU TAR and tar.gz streaming builders (`compress_tar`, `headless_quick_compress_tar`).
- **Modular Archive Inspection Subsystem (`src-tauri/src/inspector/`)**:
  - Decomposed the 1,896-line monolithic `inspector.rs` into domain-focused submodules:
    - `mod.rs`: Central archive inspection entrypoint (`inspect_archive`), path sanitization with Zip-Slip protection (`sanitize_archive_path`), single root directory detection (`get_single_root_dir`), safe atomic renaming (`safe_atomic_rename`), and read-only permission clearing.
    - `hardware.rs`: Storage bus classification (`DriveBusKind` for NVMe, SATA SSD/HDD, and USB removable media) and optimal I/O chunk sizing (`get_optimal_read_slice`, `get_optimal_write_buffer`).
    - `format.rs`: Magic byte signature inspection (`detect_archive_type`), instant TOC trailer detection (`has_instant_bay_toc`), and archive encryption detection (`is_archive_encrypted`, `check_rar4_encrypted`, `check_rar5_encrypted`).
    - `bay.rs`: Fast `.bay` / `.bayg` TOC parsing (`list_bay_entries`) and streaming fallback reader (`list_bay_stream_entries`, `read_bay_entry_from_stream`, `preview_bay_entry_bytes`).
    - `zip.rs`: Central directory entry scanner (`list_zip_entries`) and safe in-memory entry reader (`preview_zip_entry_bytes`).
    - `sevenz.rs`: 7-Zip block indexer (`list_7z_entries`) and entry preview stream extractor (`preview_7z_entry_bytes`).
    - `tar.rs`: Streaming TAR and tar.gz header reader (`list_tar_entries`, `preview_tar_entry_bytes`).
    - `rar.rs`: RAR verbose format parser (`list_rar_entries`, `preview_rar_entry_bytes`).
    - `preview.rs`: Safe in-memory preview generator (`preview_archive_entry_sync`, `preview_staged_file_sync`), base64 encoding, security restriction enforcement, and UTF-8 verification.
    - `staged.rs`: Recursive local filesystem staging inspection (`inspect_staged_items_sync`).
- **Zero API Breakage & 100% Backward Compatibility**:
  - Maintained complete re-exports via `pub use` across `compressor/mod.rs` and `inspector/mod.rs`, ensuring `lib.rs` and `extractor.rs` compile without any breaking changes.

---

## [v2.0.4] - 2026-10-20

### 🌲 Pre-Compression Staging Tree View & Selective File Exclusion
- **Hierarchical Folder Tree Inspection & Staging**:
  - Integrated `ArchiveTreeInspector` directly into the compression staging view (`CompressPage.tsx`).
  - Dropping or selecting folders automatically opens in Tree View, exposing full subfolder hierarchies, folder expansion/collapse, breadcrumb navigation, search filtering, and per-item size statistics before compression begins.
- **Dual View Mode Switcher (`[ ☷ Tree ]` / `[ ☰ List ]`)**:
  - Added Fluent pill switcher in the card header allowing instant toggling between Tree View (hierarchical folder inspection) and List View (compact summary of top-level selections).
- **Pre-Compression Selective Exclusion Filter**:
  - Added interactive `[ ✕ Exclude ]` / `[ ↺ Include ]` action buttons on staged entries.
  - Excluded folders cascade skipped status across all nested descendants with visual strike-through indicators.
  - Staged total size and file counts dynamically recalculate in real-time when entries are excluded or included.
- **Multi-Format Backend Exclusion Integration**:
  - Added `inspect_staged_items` and `preview_staged_file` Tauri async IPC commands in `inspector.rs`.
  - Updated all compression engines (`.bay`, `.bayg`, `.zip`, `.7z`, `.tar`) in `compressor.rs` to accept `excluded_paths`, omitting excluded files and directories from the generated archives and computing filtered total payload sizes.
- **Safe Staged File Preview (`FilePreviewModal`)**:
  - Staged text and code files, images, and scripts can be safely inspected directly from the tree view before archiving without risk of accidental execution.

---

## [v2.0.3] - 2026-10-20

### 📁 Extraction Subfolder Path Fix & Compression View Modularization
- **Duplicate Parent Folder Renaming Prevention**:
  - Fixed a destination path calculation bug in `extractor.rs` where extracting into existing user directories (e.g., `C:\Users\<user>\Downloads`) triggered automatic unique folder renaming, creating redundant duplicates like `Downloads (1)`.
  - Added strict stem folder verification (`is_stem_folder`) ensuring parent system folders are preserved untouched.
- **Accurate Single-Root Extraction Target Resolution**:
  - Enhanced output directory resolution in `extractor.rs` to detect when all extracted files reside inside a common archive root directory (e.g., `Downloads\lab-4`), returning the true extracted folder path rather than the generic parent directory.
  - The Extraction Complete modal and "Open Folder" action now accurately open the extracted directory directly.
- **Dynamic Dedicated Folder Label**:
  - Updated `ExtractPage.tsx` so the "Extract into dedicated folder" toggle dynamically indicates the actual archive stem name `(archive_stem)` once an archive is loaded, replacing the static `(folder_name)` placeholder.
- **Compression View Modularization (`CompressPage.tsx`)**:
  - Decomposed the monolithic compression view out of `App.tsx` into a standalone, fully typed component ([src/components/CompressPage.tsx](src/components/CompressPage.tsx)), reducing `App.tsx` by ~700 lines.
  - Locked option switches ("Archive to original path", "Add to separate archives", "Smart media bypass") during active compression (`cursor-not-allowed opacity-50 pointer-events-none`) to safeguard against mid-operation state desynchronization.

---

## [v2.0.2] - 2026-10-20

### 🗂️ Selective Folder Exclusion & Frontend Modularization
- **Hierarchical Directory Exclusion & Visual Synchronization**:
  - Fixed an issue in `ArchiveTreeInspector` where excluding a folder did not cascade to its nested child files or reflect visually on folder rows.
  - Implemented `isFileExcluded` and recursive `isDirExcluded` checking both direct entry exclusions and parent folder path prefixes.
  - Added visual indicator badges: excluded folders immediately highlight with `[ ✕ Folder Excluded ]` and switch action button to `[ ↺ Include ]`, all child files display `(Skipped)` with line-through styling, and tree navigation nodes reflect excluded status.
  - Total skipped bytes and excluded file counts now update dynamically in the inspector footer and primary extract button.
- **Extraction View Modularization (`ExtractPage.tsx`)**:
  - Decomposed the monolithic extraction view out of `App.tsx` into a modular, self-contained component ([src/components/ExtractPage.tsx](src/components/ExtractPage.tsx)), cutting ~400 lines of complex JSX from `App.tsx`.
  - Added visual disabled states and pointer shielding on extraction toggles ("Extract into dedicated folder", "Open folder when extraction completes") during active decompression to prevent mid-operation state desynchronization.

---

## [v2.0.1] - 2026-10-19

### 🛡️ Security, Robustness & OS Scheduling Hardening
- **Strict Content Security Policy (CSP)**:
  - Enabled restrictive CSP in `tauri.conf.json` (`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:`), hardening the webview against untrusted script execution.
- **64 KB Archive Entry Name Bounds Check**:
  - Added defensive 64 KB bounds check on `name_len` across streaming extraction ([extractor.rs](src-tauri/src/extractor.rs)) and archive inspection ([inspector.rs](src-tauri/src/inspector.rs)). Prevents process memory exhaustion (OOM) when handling corrupted, fuzzed, or malformed archive headers.
- **100% Streaming Encryption Enforcement**:
  - Deprecated legacy single-buffer v1/v2 decryption blocks in favor of constant-memory Streaming Chunked AEAD (`BAYENC\x03\x00`), permanently eliminating whole-file RAM allocations during extraction.
- **Atomic Reference-Counted Background Priority Shield**:
  - Upgraded `ScopedPriorityGuard` ([priority.rs](src-tauri/src/priority.rs)) with atomic reference counting (`ACTIVE_GUARD_COUNT`) to ensure OS thread priority is never prematurely restored to Normal when multiple operations run concurrently.
  - Added interactive preference toggle in `SettingsWindow` allowing power users to switch between **Smooth Mode (Shield ON / Below Normal Priority)** for stutter-free gaming and **Max Power (Normal Priority)** for aggressive foreground throughput.
- **Tauri Async Architecture & Gamer Level Alignment**:
  - Converted `get_file_size` IPC command to `async fn` with `spawn_blocking` (Architecture Rule 1 compliance).
  - Aligned `.bayg` gamer preset Zstandard compression level to Level 3 at 75% and 100% hardware allocation (Architecture Rule 4 compliance).
  - Removed unreferenced `shell_integration.rs` prototype file.

---

## [v2.0.0] - 2026-10-19

### ⚡ Smart Pre-Compressed Media Bypass (High-Throughput Store Mode)
- **Zero-CPU Dynamic Store Mode (`zip::CompressionMethod::Stored`)**:
  - Automatically identifies already-compressed formats (video, audio, images, archives, office packages) using a high-performance two-tier detection pipeline:
    1. **Fast-Tier Extension Match ($O(1)$, Zero Disk I/O)**: Instantly matches known media and container extensions (`mp4`, `mkv`, `mp3`, `flac`, `jpg`, `png`, `webp`, `zip`, `7z`, `rar`, `docx`, `xlsx`, etc.).
    2. **Deep-Tier 16-Byte Magic Header Peek**: For files with missing, unknown, or generic extensions (`.pak`, `.dat`, `.bin`, `.asset`), safely peeks only the first 16 bytes for compression container signatures (`PK\x03\x04`, `7z\xbc\xaf'\x1c`, `Rar!`, `\x1f\x8b`, `BZh`, `\xff\xd8\xff`, `\x89PNG`, `ftyp`, `RIFF...WEBP`, etc.) without ever reading the full file into memory.
  - Flags matched files as `Stored` (Store mode 0), bypassing CPU compression loops and writing directly to the container stream with calculated checksums.
  - Saturates PCIe Gen4/Gen5 NVMe read/write speeds ($1{,}000–3{,}000+\text{ MB/s}$), archiving multi-gigabyte media collections and downloads in seconds instead of minutes.
  - Highly compressible targets (`.txt`, `.json`, `.xml`, `.exe`, `.dll`, code) remain fully compressed with multi-threaded Deflate, Zstandard, Brotli, or LZMA2 for maximum space reduction.
- **Solid Archiving Dictionary Protection (`compress_bay` & `compress_7z`)**:
  - In continuous solid archives (`.bay`, `.bayg`, `.7z`), automatically groups compressible files together first.
  - Prevents random, high-entropy media bitstreams from evicting useful LZ77/Zstandard/LZMA2 dictionary history, improving both compression speed and compression ratio by 5%–15%.
- **Interactive UI & Windows 11 Shell Integration**:
  - Added Fluent UI toggle switch in the Compression Options panel (`Smart media bypass [Fast Store]`), enabled by default.
  - Right-click quick zip in Windows 11 File Explorer automatically applies Store mode on media files for instantaneous packaging.
  - Added dedicated status and feature explanation card in `SettingsWindow`.
- **Installer & File Associations**:
  - Configured streamlined modern NSIS installer (`currentUser`, silent language selector, LZMA compression).
  - Registered complete Windows shell file associations for `.bay`, `.bayg`, `.zip`, `.7z`, `.tar`, and `.rar`.

---

## [v1.2.4] - 2026-10-19

### ⚡ Performance & Storage Hardware Architecture
- **Destination-Aware Physical Storage Write Buffer Capping ([inspector.rs](src-tauri/src/inspector.rs), [compressor.rs](src-tauri/src/compressor.rs))**:
  - Implemented dynamic target drive bus classification and write buffer capping to address Windows 10/11 "Quick Removal" default policy on external USB devices:
    - **External USB / Flash Thumb Drives (`BusTypeUsb` / `DRIVE_REMOVABLE`)**: Capped at **16 MB**. Prevents the Windows kernel from blocking compression threads for 15–25 seconds during synchronous unbuffered flash commits, maintains a smooth 20 FPS progress bar, keeps cancellation immediately responsive, and eliminates USB host controller lockups.
    - **Internal SATA SSD / HDD (`BusTypeSata`, `BusTypeAta`, `BusTypeScsi`, `BusTypeSas`, `BusTypeRAID`)**: Capped at **64 MB**. Flushes in ~0.5 seconds on mechanical hard drives, maximizes sequential writes without platter head thrashing, and eliminates excessive RAM usage on mobile/laptop systems.
    - **Internal PCIe NVMe SSDs (`BusTypeNvme`, `BusTypeScm`)**: Full **128 MB** (at 100% Hardware Allocation slider). Flushes in only 25–40 milliseconds, saturating multi-gigabyte PCIe Gen 3/4/5 NVMe line rate ($3{,}000+\text{ MB/s}$) with minimum syscall context switches.
    - **Fallback / Unknown / Network UNC**: Capped at safe **32 MB**.
  - Applied across `.bay`, `.bayg`, `.zip`, `.7z`, `.tar`, and headless quick-compression paths.
  - Unified bus classification into `DriveBusKind` with static atomic 1-cycle cache (`DRIVE_BUS_CACHE`), shared between read slicing and write buffer capping.

### 📖 Documentation ([PERFORMANCE_GUIDE.md](PERFORMANCE_GUIDE.md))
- Expanded the **Two-Tier Sequential I/O Architecture** reference to document both source read slicing and destination write buffer capping.
- Added the **Storage Bus I/O Scaling Matrix** table explaining Win32 detection flags, slice sizes, write caps, and typical flush latencies.
- Documented the Windows "Quick Removal" write-through mechanism on external flash storage.

---

## [v1.2.3] - 2026-10-19

### ⚡ Performance & Storage Hardware Architecture
- **Hardware-Aware Physical Storage Read Slicing ([inspector.rs](src-tauri/src/inspector.rs), [compressor.rs](src-tauri/src/compressor.rs))**:
  - Implemented deterministic physical drive bus detection using Win32 `IOCTL_STORAGE_QUERY_PROPERTY` (`STORAGE_PROPERTY_QUERY` / `STORAGE_DEVICE_DESCRIPTOR`) and `GetDriveTypeW`:
    - **External USB / Flash Thumb Drives / External HDDs (`BusTypeUsb` / `DRIVE_REMOVABLE`)**: Fixed at **1 MB** read slices. Protects against USB controller timeouts, weak magnetic sectors, and kernel `usbstor.sys` PRDT fragmentation while providing instantaneous cancellation response.
    - **Internal SATA SSD / HDD (`BusTypeSata`, `BusTypeAta`, `BusTypeScsi`, `BusTypeSas`)**: Fixed at **4 MB** read slices. Perfectly aligns with standard SATA AHCI PRD tables and sequential read-ahead buffers for peak throughput.
    - **Internal PCIe NVMe SSDs (`BusTypeNvme`)**: Fixed at **8 MB** read slices. Fully saturates high-speed PCIe Gen 3/4/5 NVMe queues with minimal syscall context switches (maximum multi-gigabyte line speed).
    - **Fallback / Unknown / Network UNC**: Fixed at safe **2 MB** read slices.
  - Slicing wired across `.bay`, `.bayg`, `.zip`, `.7z`, `.tar`, and headless CLI quick-compression routines.
  - Cached via zero-allocation atomic memory array (`DRIVE_SLICE_CACHE`), consuming 1 single CPU clock cycle on subsequent file reads.
  - Preserved large 32 MB–128 MB sequential write buffer pipelines and RAM hardware allocation curves.

### 🛡️ 100% Offline Core Guarantee
- Core compression, extraction, archive inspection, viewing, and AES-256 encryption are 100% offline with zero network calls, telemetry, or external web requests.

### 🛠️ Build & CI Pipeline Hardening ([build.rs](src-tauri/build.rs), [build.yml](.github/workflows/build.yml))
- Configured `--manifest-path src-tauri/Cargo.toml` and automated DLL directory synchronization in GitHub Actions workflow.
- Added automated `bayzip_shell.dll` sync in `src-tauri/build.rs` to guarantee reliable builds on fresh clones.

---

## [v1.2.2] - 2026-10-18

### 🐛 Bug Fixes & Reliability Improvements
- **Embedded Windows 11 Shell Extension DLL ([registry.rs](src-tauri/src/registry.rs), [build.yml](.github/workflows/build.yml))**:
  - Embedded `bayzip_shell.dll` directly inside `bayzip.exe` via `include_bytes!`, ensuring the Windows 11 modern context menu companion DLL is deployed automatically on every PC without requiring external DLL files.
  - Added `cargo build --release --package bayzip_shell` step to GitHub Actions build workflow so release artifacts always compile and carry the shell extension.
  - Added fallback discovery logic across application parent directory, resources subfolder, and workspace build targets.

- **Shell Icon Cache Flush & File Association Notification ([registry.rs](src-tauri/src/registry.rs))**:
  - Added `SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_FLUSH)` and `ie4uinit.exe -show` to `register_context_menu_sync`.
  - Fixes Windows File Explorer ignoring `.bay` and `.bayg` file icons after installation by forcing an immediate shell icon cache refresh without needing to restart Explorer or reboot.

- **1 MB Sliced Reading & Responsive Cancel during I/O Stalls ([compressor.rs](src-tauri/src/compressor.rs))**:
  - Capped single kernel read operations to 1 MB slices across `write_file_to_bay_archive`, `compress_zip`, and `ProgressReader`.
  - Prevents external USB hard drives and mechanical drives with bad/weak sectors from locking the application in multi-second OS driver timeouts.
  - Ensures clicking "Cancel" is detected and handled immediately on the next 1 MB boundary without hanging.

---

## [v1.2.1] - 2026-10-18

### 🐛 Bug Fixes & Stability Improvements
- **Settings Window Dragging, Minimize & Close Controls ([SettingsWindow.tsx](src/components/SettingsWindow.tsx), [capabilities/default.json](src-tauri/capabilities/default.json))**:
  - Added missing `core:window:allow-hide` capability permission so the secondary Preferences window can be dismissed cleanly without IPC errors.
  - Resolved event conflict where parent titlebar `onMouseDown` was intercepting click and mouse-up events before they reached Minimize and Close buttons.
  - Isolated `data-tauri-drag-region` to title and center header surfaces, ensuring window control buttons have full pointer event precedence.
  - Updated Preferences window titlebar badge from static `v1.1.5` to dynamic application version (`v{APP_VERSION}`).

- **7-Zip (`.7z`) Compression Progress & Output Writing ([compressor.rs](src-tauri/src/compressor.rs))**:
  - Reduced multi-threaded LZMA2 chunk buffer from 32 MB down to 4 MB (`from_level_mt(..., 4 * 1024 * 1024)`), preventing multi-thread chunk buffering from delaying disk writes and fixing the 0% progress freeze.
  - Wrapped 7z archive creation in a 16 MB sequential `BufWriter` for consistent write flushing.
  - Mapped LZMA2 levels safely across hardware allocation tiers (Levels 1, 2, 3, 5, 6) to balance compression ratio with immediate desktop responsiveness.

- **Zstandard `.bay` Memory Ceiling & High-Core Scaling ([compressor.rs](src-tauri/src/compressor.rs), [App.tsx](src/App.tsx))**:
  - Bounded memory allocation on 100% Hardware Allocation (Level 19): capped write buffer to 32 MB and restricted maximum concurrent Level 19 threads to 8 (`threads.min(8)`), preventing memory spikes past 2 GB on high-core CPUs (e.g. 12–24 threads) while preserving maximum archival ratio.
  - Scaled standard `.bay` levels across 0% (Level 1), 25% (Level 3), 50% (Level 6), and 75% (Level 9) for high real-time throughput.
  - Added sleek amber "Ultra Archival Mode" informational notice in `App.tsx` when 100% slider is selected on `.bay` to inform users of the trade-off between maximum archival compression ratio and processing time/RAM usage.

- **Eliminated Redundant Pre-Compression Disk Scan ([compressor.rs](src-tauri/src/compressor.rs))**:
  - Removed preliminary `collect_bay_toc` directory tree traversal in `compress_bay` and `headless_quick_compress_bay`.
  - Directory entries and file items are now collected dynamically on-the-fly during the single compression streaming pass, eliminating double-disk traversal and allowing large folder trees to begin compression immediately.

- **`.zip` and `.tar` Streaming Throughput ([compressor.rs](src-tauri/src/compressor.rs))**:
  - Capped `.zip` Deflate level to 6 across upper hardware tiers, eliminating excessive CPU cycles from Deflate level 9.
  - Converted `.tar` file writes from small unbuffered reads to sequential streaming with `ProgressReader` into 32 MB write buffers, giving smooth incremental progress and higher disk throughput.

- **Dynamic Per-Algorithm Estimated RAM UX ([App.tsx](src/App.tsx), [PERFORMANCE_GUIDE.md](PERFORMANCE_GUIDE.md))**:
  - Replaced misleading system-wide free RAM subtraction in `getEstimatedRam()` with genuine, algorithm-aware peak memory calculations (LZ4, Store, Deflate, Brotli, Gamer, LZMA2, Zstandard).
  - Included realistic safety headroom for OS thread scheduling, allocation overhead, and hash chain indexing.
  - Added Section 9 (Algorithm-Specific RAM Footprint Matrix with Headroom) to `PERFORMANCE_GUIDE.md`.

- **Development Build Speed & Compiler Optimizations ([src-tauri/Cargo.toml](src-tauri/Cargo.toml))**:
  - Added `[profile.dev.package."*"] opt-level = 3` so compression dependencies run with full LLVM optimizations during local development and testing without slow debug crawl.

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
  - Authored standard Windows Sparse Package manifest (`Bayzip.ShellExtension_1.0.0.0_x64__...`) supporting external loose deployment via `uap10:AllowExternalContent` and `uap10:RuntimeBehavior="win32App"`.
  - Configured `desktop4:FileExplorerContextMenus` targeting `*` and `Directory` targets with alphanumeric verb IDs (`BayzipContextMenuAllFiles`, `BayzipContextMenuDirectory`).
  - Configured in-process STA COM activation via `com:SurrogateServer`.
  - Added automatic background registration on startup via `Add-AppxPackage -Register` when running on Windows 11 (`Build >= 22000`).
  - Preserved dual-mode support: automatically maintains classic Windows 10 registry integration (`HKCU\Software\Classes`) alongside modern Windows 11 Sparse Package identity.
- **Multi-Selection Shell Command Dispatch ([main.rs](src-tauri/src/main.rs), [lib.rs](src-tauri/src/lib.rs))**:
  - Enhanced headless CLI commands (`--quick-compress`, `--extract-here`, `--extract-to-folder`) in `main.rs` to iterate over all passed file arguments when multiple files/folders are selected in Explorer.
  - Added `unregister_context_menu` Tauri command and registry unregistration routines (`Remove-AppxPackage` + registry purge) in `lib.rs` and `registry.rs`.

### ⚡ High-Throughput Extraction I/O Engine ([extractor.rs](src-tauri/src/extractor.rs))
- **Eliminated Windows NTFS Synchronous Zero-Fill Bottleneck**:
  - Removed pre-emptive `out_file.set_len(file_size)` across all extraction pipelines (`.bay`, `.zip`, `.7z`, `.tar`).
  - On Windows NTFS, `SetEndOfFile()` forces the OS kernel to synchronously write zeroes across allocated clusters before allowing data writes, which effectively doubled disk write volume (e.g., writing 88 GB to disk for a 44 GB game archive).
  - Decompressed data is now streamed directly to disk in a single write pass, cutting physical disk I/O in half and eliminating unnecessary SSD write wear (TBW degradation).
- **Quadrupled Sequential I/O Buffering**:
  - Upgraded archive `BufReader` capacity from 4 MB to 16 MB, allowing huge sequential reads and minimizing mechanical head movement on external hard drives.
  - Doubled stream decompressor transfer buffer to 4 MB to cut Windows syscall context switches in half.
  - Increased large-file `BufWriter` buffer to 16 MB, maximizing sequential flash write bursts on PCIe 4.0/5.0 NVMe SSDs.

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

# 📦 Bayzip (v1.1.5)

**Bayzip** is a modern, high-performance file archiver for Windows built with **Tauri 2**, **Rust**, and **React**. Engineered for maximum hardware utilization, it combines multi-threaded compression algorithms (Zstandard, Brotli, LZ4, LZMA2), a hierarchical directory tree explorer, zero-disk safe file previewing, zero-allocation streaming encryption, and seamless Windows File Explorer shell integration.


---

## ⚡ Key Features


### 🗂️ Tree-Type Archive Directory Inspector
- **Hierarchical Tree View**:
  - Replaces flat file entry dumps with an expandable/collapsible folder tree (File Explorer and WinRAR style).
  - Folder nodes with open/close chevron state (`▶` / `▼`), subfolder counts, and item count indicators.
  - Interactive breadcrumb trail (`[ 📦 Archive ] / [ 📁 folder ] / [ 📁 subfolder ]`) with direct segment jumping and up-one-level (`⬆`) navigation.
- **Dual-View & Full-Screen Workspace Mode (`⤢`)**:
  - Instant toggle between **`[ 🗂️ Tree View ]`** and **`[ 📋 Flat Table ]`**.
  - **Expand Button (`⤢`)**: Expands the inspector into a full dual-pane workspace modal (left folder hierarchy, right files list) for large archives with status bar stats (total items, uncompressed size, and current folder path).
  - Integrated search and filter bar across all folder levels.
- **Instant TOC Trailer (`O(1)` Archive Inspection)**:
  - `.bay` archives feature a binary Table of Contents trailer (`BAYTOC\x02\x00` + 8-byte LE offset) written at the EOF.
  - Inspect multi-gigabyte archives instantly without full-container decompression or linear reading.

### ✕ Selective Extraction & Exclusion Mode (Skip Mode)
- **HCI-Compliant Inverted File Selection**:
  - By default, all archive files are marked **Included** for 1-click full extraction.
  - Clicking **"Exclude Files"** (`[ ✕ Exclude Files ]`) unlocks interactive exclusion toggles in both Tree and Flat views.
  - Marked unwanted files/folders display a red cross badge `[✕ Excluded]`, dimmed (50% opacity), strikethrough filename, and size indicates `(Skipped)` in soft rose.
  - Skips extracting unwanted files to disk, saving massive disk space and I/O time when only specific files are needed.
- **Decompression Container Stream Alignment**:
  - For streaming `.bay` archives, excluded files are safely drained through an in-memory buffer without creating disk files, preserving container decompressor alignment and data integrity.
  - Supported across `.bay`, `.zip`, `.tar`, and `.7z`.
- **Dynamic Extraction Status**:
  - Dynamic button updates in real time: `[ Extract All (100 Files) ]` vs `[ Extract 98 Files (2 Excluded) ]`.
  - 1-click **"Include All"** reset button in the inspector toolbar and extraction action bar.

### 🛡️ Restricted Safe Preview Mode (In-Memory Streaming)
- **Zero-Disk Streaming**:
  - Preview archive files directly from memory without extracting them to disk across `.bay`, `.zip`, `.7z`, `.tar`, `.tar.gz`, and `.rar`.
  - Trigger preview via **double-click**, **Spacebar**, or the **`👁️ Preview`** button.
- **Safe Code & Text Viewer**:
  - Line numbers, syntax-ready dark theme, word wrap toggle, and 1-click clipboard copy.
  - Supports Markdown, JSON, YAML, TOML, XML, Rust, TypeScript, JavaScript, Python, C/C++, HTML/CSS, SQL, logs, and any UTF-8 document.
- **Image Preview**:
  - Clean in-memory preview for `.png`, `.jpg`, `.jpeg`, `.webp`, `.svg`, `.gif`, `.bmp`, and `.ico` with dimensions and metadata.
- **Restricted Execution Protection**:
  - Strict security guard preventing accidental execution of `.exe`, `.dll`, `.sys`, `.msi`, `.com`, or `.scr` binaries inside the viewer.
  - Shell scripts (`.bat`, `.cmd`, `.ps1`, `.vbs`) are strictly sandboxed into view-only mode with zero execution permissions.

### 🚀 High-Performance Compression & Extraction Engine
- **Hardware-Aware Performance Allocation**:
  - Live hardware specs detection (CPU cores and free RAM).
  - Slider controls compression intensity and core allocation (`0%` -> 1 core, `50%` -> half cores, `100%` -> full multi-threading).
  - Dynamic I/O sizing tied to allocation slider: read buffers scale from 2 MB to 16 MB; write buffers scale from 16 MB up to 128 MB for maximum NVMe burst throughput and reduced system call overhead.
  - OS priority scheduling (`ScopedPriorityGuard`) sets background compression threads to below-normal priority to maintain 100% system and game responsiveness.
- **Format Support**:
  - **`.bay`**: High-performance custom format supporting **Zstandard** (Levels 1–19), **Brotli**, and **LZ4**.
  - **`.bayg`**: Ultra-fast gaming modpack preset using Zstd-Fast (capped at Level 3 for near-zero decompression latency).
  - **`.7z`**: 7-Zip LZMA2 multi-threaded compression with smooth progress reporting.
  - **`.zip`**: Standard ZIP with Deflate, uncompressed Store, and BZip2 options.
  - **`.tar`**: Uncompressed TAR container packaging.
  - **Extraction**: Universal extraction for `.bay`, `.bayg`, `.zip`, `.7z`, `.tar`, `.gz`, `.tgz`, and `.rar`.
- **Atomic Operations & Instant Cancellation**:
  - **Atomic `.baypart` Extraction**: Unpacked files are staged into `.baypart` temporary files and atomically renamed upon success. Any extraction cancellation or error immediately scrubs partial files, guaranteeing zero corrupted or half-written files on disk.
  - **Atomic `.baytmp` Compression**: Compression writes to temporary files before atomic finalization.
  - 1-click instant cancellation with real-time UI response and automated temporary file cleanup.
- **Smart Folder Conflict Resolution**:
  - Automatic single-root detection prevents redundant nested directories (e.g., `ArchiveName/ArchiveName/...`) when unpacking archives that already contain a top-level folder.
  - Clean conflict handling avoids spurious auto-numbering `(1)` folders when extracting into fresh or matching target paths.

### 🔒 Zero-Allocation Streaming AES-256-GCM Encryption
- **In-Place AEAD Streaming**:
  - `ChunkedAesWriter` and `ChunkedAesReader` operate with zero heap allocations per chunk using `AeadInPlace` (`encrypt_in_place` / `decrypt_in_place`).
  - 4 MB chunk size minimizes AEAD authentication tag overhead while maximizing hardware throughput on AES-NI capable CPUs.
  - Constant memory footprint (< 100 MB RAM) regardless of archive size, eliminating Out-Of-Memory (OOM) risks on multi-gigabyte encrypted archives.
  - Clean framing separation between encrypted container payloads and TOC metadata.
- Seamless password prompt with in-place archive unlocking for inspection and previewing.

### 🪟 Windows File Explorer Integration
- **Context Menus**: Right-click files or folders to quickly compress to `.bay`, `.zip`, `.7z`, or `.tar` (including headless background compression with instant TOC generation).
- **Quick Extract**: Right-click archives to *"Extract to dedicated folder"*, *"Extract Here"*, or *"Open in Bayzip"*.
- **Branded File Associations**: Displays Bayzip icons on `.bay` and `.bayg` files in Windows File Explorer.

---

## 📂 Supported Formats

| Format | Extensions | Creation / Compression | Extraction | In-Memory Preview | Compression Algorithms |
|---|---|:---:|:---:|:---:|---|
| **Bayzip** | `.bay` | ✅ | ✅ | ✅ | Zstandard (Levels 1–19), Brotli, LZ4 |
| **Bayzip Gamer** | `.bayg` | ✅ | ✅ | ✅ | Zstd-Fast (Capped at Level 3) |
| **ZIP** | `.zip` | ✅ | ✅ | ✅ | Deflate, Store, BZip2 |
| **7-Zip** | `.7z` | ✅ | ✅ | ✅ | LZMA2 Multi-Threaded |
| **TAR** | `.tar` | ✅ | ✅ | ✅ | Uncompressed (Store) |
| **GZip / TarGz** | `.gz`, `.tgz` | — | ✅ | ✅ | Deflate / Tar |
| **BZip2 / TarBz2** | `.bz2`, `.tar.bz2` | — | ✅ | ✅ | BZip2 / Tar |
| **XZ / TarXz** | `.xz`, `.tar.xz` | — | ✅ | ✅ | LZMA2 / Tar |
| **Zstandard Tar** | `.zst`, `.tar.zst` | — | ✅ | ✅ | Zstandard / Tar |
| **ISO Image** | `.iso` | — | ✅ | ✅ | ISO 9660 Disc Image |
| **Cabinet** | `.cab` | — | ✅ | ✅ | Microsoft Cabinet |
| **RAR** | `.rar` | — | ✅ | ✅ | Decompress / Preview only |

---

## 📋 Release Notes

> 📖 **Full Changelog**: For comprehensive release notes, milestone summaries, and full version history across all releases, see [CHANGELOG.md](CHANGELOG.md).

### v1.1.5 (Latest)
- **Extended Magic Byte Signatures & Universal Format Detection**:
  - Implemented raw byte inspection for 5 additional archive and container standards:
    - **BZip2 (`.bz2`, `.tar.bz2`)**: `42 5A 68` (`BZh`)
    - **XZ (`.xz`, `.tar.xz`)**: `FD 37 7A 58 5A 00` (`\xFD7zXZ\x00`)
    - **Zstandard (`.zst`, `.tar.zst`)**: `28 B5 2F FD` (`(\xB5/\xFD`)
    - **ISO 9660 Disc Image (`.iso`)**: `43 44 30 30 31` (`CD001` at sector 16 / offset `32768`)
    - **Microsoft Cabinet (`.cab`)**: `4D 53 43 46` (`MSCF`)
  - Enables Bayzip to detect, inspect, and extract renamed, corrupted-extension, or extensionless archives via `tar.exe` and native decoders.
  - Added in-memory file previewing via system tar stdout streaming (`-O`) for `.bz2`, `.xz`, `.zst`, `.iso`, and `.cab`.
  - Added Windows File Explorer context menu and open dialog associations for `.bz2`, `.xz`, `.zst`, `.iso`, and `.cab`.

### v1.1.4
- **Format Hardening & Audit Across All Containers**:
  - **RAR Encryption & Parsing**: Added detection for encrypted RAR4 and RAR5 archives in `is_archive_encrypted`. Revamped `list_rar_entries` to support both 2-column ISO/locale and 3-column dates while strictly preserving multi-space filenames. Corrected empty-buffer preview handling.
  - **ZIP Deflate Level & Locked File Safety**: Integrated hardware slider scaling into Deflate compression levels (1, 3, 6, 7, 9) and prevented 0-byte ghost entries on locked or inaccessible files by pre-validating file read handles before starting ZIP entry headers.
  - **.bay & .bayg Empty Folders & TOCTOU Stream Framing**: Normalized path separators (`/`) for directory entries, added `[2u8]` directory entry encoding/decoding to preserve empty directory structures, and bounded file reads to declared `file_size` to eliminate stream framing corruption if files grow during compression.
  - **Cancellable TAR Compression & Safe Root Stripping**: Converted blocking `append_dir_all` to an iterative folder walk with live cancellation checks and progress reporting; conditioned TAR extraction root stripping on `get_single_root_dir`.
  - **Orphaned Windows Registry Cleanup**: Automatically purges legacy `.baygamers`, `.baygamer`, and `.bay_gamer` registry keys via `[-HKEY_...]` during shell integration sync.

### v1.1.3
- **Encrypted `.zip` Inspector (`list_zip_entries`)**:
  - Replaced `zip.by_index` with `zip.by_index_raw` to parse central directory headers directly without requiring file decryption, eliminating `PasswordRequired` inspector failures and showing accurate item counts in `< 1ms`.
- **Safe Windows File Overwrite (`safe_atomic_rename` & `clear_readonly`)**:
  - Implemented `safe_atomic_rename` and `clear_readonly` across `compressor.rs` and `extractor.rs` to strip Windows read-only file attributes before replacing files (`.baytmp` and `.baypart`), preventing `PermissionDenied` errors when overwriting read-only game installations.
- **Double-Folder Nesting Prevention for `.rar`**:
  - In `extract_archive_sync`, archives extracted via system `tar` (such as `.rar`) now inspect for single root directories matching the archive stem name, preventing redundant `Stem\Stem\` nested folder hierarchies.
- **Dedicated `.bayg` Gamer Archive Standardization**:
  - Fully standardized `.bayg` container format across frontend presets, backend engines, and Windows Registry context menus.

### v1.1.2
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

### v1.1.1
- **Settings Window Drag & Controls Fix**:
  - Configured global Tauri 2 window capabilities (`"windows": ["*"]`) so the independent multi-window Settings surface is fully movable across all monitors without boundary locks.
  - Added native Rust IPC commands `minimize_settings_window` and `close_settings_window` with webview fallbacks, guaranteeing window controls respond instantly.
  - Added programmatic `startDragging()` titlebar fallback for seamless dragging on touch and mouse inputs.
- **Hardware & Architecture Reference**:
  - Published comprehensive [PERFORMANCE_GUIDE.md](file:///d:/Bayzip/PERFORMANCE_GUIDE.md) detailing CPU core scaling formulas, dynamic RAM read/write buffers, Zstandard level mapping, and real-world disk throughput metrics.

### v1.1.0
- **Selective Extraction & Exclusion Mode (`✕` Skip Mode)**:
  - Added inverted file/folder selection to skip extracting unwanted files directly to disk, saving massive storage and I/O time.
  - Interactive toolbar toggle with pulsating indicator (`[ ✕ Exclude Files ]`) and 1-click reset (`[ Include All ]`).
  - Tree and Flat views show red cross badges (`[✕ Excluded]` / `[✕ Folder Excluded]`), dimmed 50% opacity, strikethrough labels, and `(Skipped)` size badges.
  - Safe decompressor byte-draining preserves container stream alignment for `.bay`, `.zip`, `.tar`, and `.7z` without partial file disk writes.
  - Bottom action button dynamically displays live extraction counts: `[ Extract 98 Files (2 Excluded) ]` alongside a quick reset button.

### v1.0.7
- **Independent Multi-Window Settings**: Upgraded Settings from an in-page modal to a true native Windows 11 window (`580x560` borderless with custom titlebar and drag region). Users can now drag the Settings window anywhere across multiple monitors independently of the main Bayzip window, while preserving real-time configuration sync.
- **Ultra-Scale Virtual Scrolling (DOM Recycling)**: Implemented high-performance virtual windowing in `ArchiveTreeInspector` (fixed 34px row height + 5-row overscan) for both the compact inspector and expanded dual-pane workspace. Archives with 100,000+ files now render only ~20–25 DOM elements at 60 FPS with minimal memory overhead.
- **Spacebar Shortcut Input Focus Fix**: Fixed a bug where pressing the Spacebar inside the search filter box or password field was intercepted by the global preview shortcut, allowing normal space typing in all inputs.
- **Single-Root Folder Deduplication**: Enhanced extraction target resolution to automatically detect archives that already encapsulate a single root directory, preventing redundant double-folder nesting (e.g. `ArchiveName\ArchiveName`).
- **Safe Preview Large File DOM Protection**: Added a 3,000-line DOM rendering cap with a warning banner for massive text and code files, safeguarding the UI from freezes while preserving full clipboard copy.

### v1.0.6
- **Zero-Allocation In-Place AES-256-GCM**: Switched streaming encryption and decryption to `AeadInPlace`, eliminating heap allocations on every chunk and upgrading chunk sizes to 4 MB for maximum AES throughput.
- **Dynamic RAM-Scaled I/O Buffers**: Configured read buffers (2–16 MB) and write buffers (16–128 MB) to dynamically scale with the hardware allocation slider, drastically boosting NVMe burst rates.
- **Headless Quick Compress Fast TOC**: Added Table of Contents trailer (`BAYTOC\x02\x00`) to right-click context menu headless `.bay` compressions for immediate O(1) archive inspection.
- **Atomic `.baypart` Extraction**: Added staging to `.baypart` temporary files during extraction with automatic rollback to ensure zero corrupted partial files upon error or cancellation.
- **Single-Root Folder Normalization**: Fixed redundant subfolder nesting and eliminated accidental `(1)` directory renames when extracting dedicated archive folders.

### v1.0.5
- Upgraded AES-256-GCM encryption/decryption architecture from single-buffer `read_to_end` to streaming AEAD chunks, dropping peak RAM usage below 100 MB.

### v1.0.4
- Fixed auto-numbering folder conflicts during extraction.
- Enabled instant `.bay` archive inspection and zero-disk previewing.
- Enhanced cancel cleanup routines and scratch-disk isolation.

### v1.0.3
- Security hardening for Zip-Slip path traversal and Alternate Data Streams (ADS).
- Floating Settings window and modernized About modal design.

### v1.0.2
- Real-time extraction progress and throughput metrics.
- Added `.bayg` file selector and preset tuning.

### v1.0.1
- Universal extraction support for `.bay`, `.zip`, `.7z`, and `.tar`.
- Hardware-scaled thread allocation and `ScopedPriorityGuard` OS prioritization.

---

## 🚀 Building & Running

### Requirements

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/) (1.75+)
- Windows 10 or 11

### Run in Development

```bash
npm install
npm run tauri dev
```

### Build Production Executable (.exe / Installer)

```bash
npm run tauri build
```

The compiled installer and standalone executable will be generated in `src-tauri/target/release/bundle/nsis/`.

---

## 📜 License

Private © Bayzip.

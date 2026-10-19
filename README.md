# 📦 Bayzip (v2.0.1)

**Bayzip** is a modern, high-performance file archiver for Windows built with **Tauri 2**, **Rust**, and **React**. Engineered for maximum hardware utilization, it combines multi-threaded compression algorithms (Zstandard, Brotli, LZ4, LZMA2), a hierarchical directory tree explorer, zero-disk safe file previewing, zero-allocation streaming encryption, destination-aware storage buffer capping, smart pre-compressed media bypass, and seamless Windows File Explorer shell integration.


---

## ⚡ Key Features

### 🚀 Smart Pre-Compressed Media Bypass (High-Throughput Store Mode)
- **Zero-CPU Dynamic Store Mode (`zip::CompressionMethod::Stored`)**:
  - Automatically identifies pre-compressed video, audio, image, and archive formats (`.mp4`, `.mkv`, `.avi`, `.mov`, `.wmv`, `.mp3`, `.flac`, `.ogg`, `.opus`, `.aac`, `.jpg`, `.jpeg`, `.png`, `.webp`, `.avif`, `.gif`, `.zip`, `.7z`, `.rar`, `.gz`, `.bz2`, `.xz`, `.zst`, `.docx`, `.xlsx`, `.pptx`, `.apk`, `.vpk`, `.pak`).
  - Directly streams already-compressed files to disk uncompressed in Store mode, completely bypassing CPU compression loops.
  - Saturates PCIe Gen4/Gen5 NVMe read/write speeds (1,000–3,000+ MB/s), archiving media directories and downloads in seconds instead of minutes.
  - Highly compressible assets (`.txt`, `.json`, `.xml`, `.exe`, `.dll`, code, game maps) remain fully compressed with multi-threaded Deflate, Zstandard, Brotli, or LZMA2 for maximum space reduction.
- **Solid Archiving Dictionary Protection**:
  - In continuous solid archives (`.bay`, `.bayg`, `.7z`), automatically groups compressible files together first.
  - Prevents random, high-entropy media bitstreams from evicting useful LZ77/Zstd dictionary history, improving both compression speed and compression ratio by 5%–15%.
- **Interactive UI Toggle & Windows Explorer Shell Integration**:
  - Added a toggle switch in the Compression Options view (`Smart media bypass [Fast Store]`), active by default.
  - Right-click quick zip via Windows 11 context menu automatically leverages Store mode for instant media archiving.
  - Dedicated preferences status card in Settings.

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
  - **Source & Destination Hardware-Aware I/O Architecture**:
    - **Physical Read Slicing**: Queries underlying drive bus via Win32 `IOCTL_STORAGE_QUERY_PROPERTY` to slice reads safely (1 MB for USB, 4 MB for SATA, 8 MB for NVMe).
    - **Destination-Aware Write Capping**: Automatically caps output write buffers (16 MB for USB thumb drives under Windows Quick-Removal policy, 64 MB for SATA HDD/SSD, up to 128 MB for PCIe NVMe line rate), eliminating 20-second thread hangs on external flash while keeping progress bars at 20 FPS with instant cancellation.
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
- **Native Windows 11 Top-Level Context Menu (`IExplorerCommand`)**:
  - Implements Microsoft's modern Windows 11 shell extension standard via in-process COM companion DLL (`bayzip_shell.dll`).
  - Appears directly at the top level of the Windows 11 right-click menu without needing "Show more options" (`Shift+F10`).
  - Features cascading sub-commands with native Fluent icons:
    - *"Compress to .bayg (Gamer Preset)"*
    - *"Compress to .bay (Ultra-Fast)"*
    - *"Extract Here"*
    - *"Open in Bayzip"*
  - Multi-selection support: processes all selected files and folders in batch.
- **Sparse Package Identity (`AppxManifest.xml`)**:
  - Automatically deploys and registers a Windows Sparse Package on Windows 11 (`Build >= 22000`) for seamless modern shell integration.
- **Classic Windows 10 & Legacy Fallback**:
  - Dual-mode support maintains standard `HKCU\Software\Classes` shell registry keys for Windows 10 and classic Explorer contexts.
- **Branded File Associations**:
  - Displays official Bayzip glowing cube icons on `.bay` and `.bayg` files across File Explorer.

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

### v2.0.6 (Latest)
- **Hardened Path Traversal & Zip-Slip Protection**: Sanitizes `CurDir` (`.`) and `./` artifacts cleanly without breaking relative subfolder paths, strictly rejecting unauthorized root and prefix directory traversals.
- **Robust Single-Root Folder Deduplication**: Hardened candidate root scanning across `.bay`, `.zip`, `.7z`, and `.rar` to eliminate false-positive root stripping on loose top-level files.
- **Precision Folder Exclusion Filtering**: Standardized trailing-slash normalization across compression engines, ensuring excluded folders and their nested descendants are reliably skipped.
- **Modularized Subsystem Architecture**: Fully decoupled backend engines into dedicated modules (`compressor/` and `inspector/`) with zero API breakage, optimized thread management, and hardware-scaled I/O.

> 📖 **Full Changelog**: For comprehensive release notes, milestone summaries, and full version history across all releases (v1.0.1 through v2.0.6), see [CHANGELOG.md](CHANGELOG.md).

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

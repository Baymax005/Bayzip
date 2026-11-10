<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Bayzip Logo" width="100" height="100" />
</p>

<h1 align="center">Bayzip</h1>

<p align="center">
  Fast, hardware-accelerated archiver for Windows built with Tauri 2, Rust, and React.
</p>

<p align="center">
  <a href="https://github.com/Baymax005/Bayzip/releases"><img src="https://img.shields.io/badge/version-v2.4.1-0078D4?style=flat-square&logo=semver&logoColor=white" alt="Version v2.4.1" /></a>
  <a href="#windows-shell-integration"><img src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6?style=flat-square&logo=windows11&logoColor=white" alt="Windows 10 | 11" /></a>
  <a href="https://v2.tauri.app/"><img src="https://img.shields.io/badge/Tauri-v2.0-FFC131?style=flat-square&logo=tauri&logoColor=black" alt="Tauri 2.0" /></a>
  <a href="src-tauri/Cargo.toml"><img src="https://img.shields.io/badge/Rust-2021%20Edition-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust 2021 Edition" /></a>
  <a href="package.json"><img src="https://img.shields.io/badge/React-19.1-61DAFB?style=flat-square&logo=react&logoColor=black" alt="React 19.1" /></a>
  <a href="package.json"><img src="https://img.shields.io/badge/TypeScript-~6.0-3178C6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript ~6.0" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-059669?style=flat-square" alt="MIT License" /></a>
</p>

<p align="center">
  <a href="src-tauri/src/crypto.rs"><img src="https://img.shields.io/badge/security-AES--256--GCM%20AEAD-10B981?style=flat-square&logo=shield&logoColor=white" alt="AES-256-GCM AEAD Security" /></a>
  <a href="TESTS.md"><img src="https://img.shields.io/badge/tests-31%2F31%20passing-success?style=flat-square&logo=checkmarx&logoColor=white" alt="31/31 Automated Tests Passing" /></a>
  <a href="src-tauri/src/compressor/mod.rs"><img src="https://img.shields.io/badge/engine-Zstandard%20%7C%20LZ4%20%7C%20Brotli-F59E0B?style=flat-square&logo=speedtest&logoColor=white" alt="Compression Engine" /></a>
  <a href="src-tauri/crates/bayzip_shell/src/lib.rs"><img src="https://img.shields.io/badge/shell-Windows%2011%20COM-6366F1?style=flat-square&logo=windows&logoColor=white" alt="Windows 11 COM Shell Extension" /></a>
  <a href="CHANGELOG.md"><img src="https://img.shields.io/badge/audit-35%2F35%20remediated-8B5CF6?style=flat-square&logo=target&logoColor=white" alt="35/35 Technical Audit Findings Remediated" /></a>
</p>

<p align="center">
  <a href="#-key-features"><b>⚡ Key Features</b></a> •
  <a href="#-storage-calibrated-streaming-io"><b>💾 I/O Calibration</b></a> •
  <a href="#-zero-allocation-streaming-aes-256-gcm-encryption"><b>🔒 Security & Cryptography</b></a> •
  <a href="#-windows-file-explorer-integration"><b>🪟 Shell Extension</b></a> •
  <a href="#-supported-formats"><b>📂 Supported Formats</b></a> •
  <a href="TESTS.md"><b>🧪 Test Report</b></a> •
  <a href="#-building--running"><b>🚀 Quick Start</b></a> •
  <a href="CHANGELOG.md"><b>📋 Changelog</b></a>
</p>

---

## ⚡ Highlights

| Feature | Description | Architecture Component |
|---|---|---|
| **🛡️ In-Place Archive Integrity & CRC-32 Tester** | High-throughput zero-disk-write stream verification engine validating container parity, CRC-32 checksums, and encrypted blocks with live UI telemetry. | `tester.rs` |
| **🏎️ Ultra-Fast Gamer Preset (`.bayg`)** | Locked at Zstandard Level 3 with multi-core parallelism for maximum game installation and transfer throughput. | `compressor/bay.rs` |
| **🛡️ Zip-Slip & Path Traversal Proof** | Multi-layer sanitization rejecting directory escapes (`..`), DOS device aliases (`CLOCK$`, `CONFIG$`, `CON`), NTFS ADS colons, and illegal characters. | `inspector/mod.rs` |
| **⚡ 100k-File Virtualized UI** | Silky 60 FPS scrolling and $O(1)$ folder lookups using custom debounced virtual scrolling and memoized tree hierarchies. | `VirtualScrollList.tsx` |
| **🔒 Streaming Chunked AEAD Encryption** | 4 MB chunked AES-256-GCM streaming with constant < 100 MB RAM footprint and cryptographic EOF truncation detection. | `crypto.rs` |
| **🪟 Windows 11 Native Context Menu** | In-process Win32 COM DLL (`IExplorerCommand`) top-level menu integration with MSIX Sparse Package identity and verified binary resolution. | `bayzip_shell.dll` |
| **💾 Storage Bus-Aware Buffer Slicing** | Dynamic buffer calibration (NVMe 2 MB, SATA 1 MB, USB 512 KB) preventing controller lockups, platter thrashing, and RAW disk corruption. | `compressor/mod.rs` |

---

## ⚡ Key Features

### 🛡️ In-Place Archive Integrity Tester & CRC-32 Verification (v2.4.0)
- **High-Throughput Zero-Disk-Write Verification (`src-tauri/src/tester.rs`)**:
  - Validates container parity and entry checksums by streaming decompressed payloads directly into an in-memory discard sink (`std::io::sink()`) with zero disk writes.
  - Multi-container integrity checking for `.bay`, `.bayg`, `.zip` (rolling CRC-32 vs central directory headers), `.7z` (folder block decoding and CRC validation), `.tar.*` (block header checksums), and system tar fallback for `.rar`, `.iso`, and `.cab`.
  - Runs inside `tauri::async_runtime::spawn_blocking` protected by `ScopedPriorityGuard` (`THREAD_PRIORITY_BELOW_NORMAL`) ensuring smooth system and gaming performance.
  - Real-time cancellation (`cancel_test_archive` / `CANCEL_TEST`) and ~40ms throttled progress event emitter (`test_progress`).
- **In-Place UI Telemetry & Verification Indicators (`ArchiveTreeInspector.tsx`)**:
  - `2px` Sky Blue (`#38BDF8`) live stream line along the top edge of the footer status bar with active file, percentage, throughput telemetry, and inline `[ Cancel ]` button.
  - Flashes emerald green `✓ Stream Verified 100% OK (X items, 0 CRC errors)` on completion.
  - Displays persistent `✓ OK` badges on verified archive rows.

### 🖥️ Fluent Zinc & Sky Single-Screen Desktop Utility (v2.4.0)
- **Zero-Scroll Main Window Architecture (`App.tsx`)**:
  - Desktop-native layout locked to `h-screen overflow-hidden flex flex-col bg-[#141416]` — the main chrome never scrolls; only the inner file viewer scrolls.
  - Neutral dark zinc/graphite palette (`#141416` / `#1C1C1F`) with crisp `1px solid #2E2E34` borders and `#38BDF8` sky blue interactive accents.
- **Unified Top Ribbon Toolbar**:
  - Single streamlined action bar with contextual sky-blue primary button states (`[ 📦 Compress... ]` when staging files; `[ 📤 Extract... ]` when opening archives).
- **Keka-Style Compact Popup Dialogs with Live Streaming Morph**:
  - Completely eliminated `backdrop-blur` and heavy window overlays; dialogs stay neatly contained within the File Viewer bounds.
  - Compact `CompressModal` with notched hardware slider (`Store`, `Fast`, `Balanced`, `High`, `Ultra`) and archive format dropdowns.
  - Smoothly morphs upon action into a **Live Streaming Progress View** showing throughput, ETA, streamed bytes, current item, and minimize-to-footer option.

### 🗃️ Unified Desktop Drop Hub & Fast Directory Tree Inspector (v2.3.7)
- **High-Performance Archive Directory Inspector (`ArchiveTreeInspector.tsx`)**:
  - Direct, fast tree & list view of loaded archive contents with instant folder hierarchy navigation.
  - Interactive parent navigation (`..`) and clickable breadcrumb navigation (`📦 root / folder / subfolder`).
  - Real-time search and filter input with instantaneous filename matching.
  - In-memory file content preview modal (`FilePreviewModal.tsx`) for images, text, and code files.
  - Selective extraction: include/exclude specific files and subfolders with 1-click checkboxes.
  - Password-protected archive unlock banner for encrypted archives.
- **Standalone VirtualScrollList (`VirtualScrollList.tsx`)**:
  - Reusable, generic virtual scroll list component handling 100,000+ files at smooth 60 FPS with `requestAnimationFrame` scroll debouncing.
- **Focused Compression & Extraction Workflows**:
  - Clean separation: fast archive creation (drop files, format presets, multi-core slider, encryption) and fast extraction (open, inspect, extract).

### 🚀 Smart Pre-Compressed Media Bypass (High-Throughput Store Mode)
- **Zero-CPU Dynamic Store Mode (`zip::CompressionMethod::Stored`) [ZIP & TAR]**:
  - Automatically identifies pre-compressed video, audio, image, and archive formats (`.mp4`, `.mkv`, `.avi`, `.mov`, `.wmv`, `.mp3`, `.flac`, `.ogg`, `.opus`, `.aac`, `.jpg`, `.jpeg`, `.png`, `.webp`, `.avif`, `.gif`, `.zip`, `.7z`, `.rar`, `.gz`, `.bz2`, `.xz`, `.zst`, `.docx`, `.xlsx`, `.pptx`, `.apk`, `.vpk`, `.pak`, `.iso`).
  - Directly streams already-compressed files uncompressed in Store mode, completely bypassing CPU compression loops.
  - Saturates PCIe Gen4/Gen5 NVMe read/write speeds (1,000–3,000+ MB/s), archiving media directories and downloads in seconds instead of minutes.
  - Highly compressible assets (`.txt`, `.json`, `.xml`, `.exe`, `.dll`, source code) remain fully compressed with multi-threaded Deflate for maximum space reduction.

---

## 💾 Storage-Calibrated Streaming I/O

Bayzip inspects physical drive characteristics via Win32 IOCTL (`IOCTL_STORAGE_QUERY_PROPERTY`) to calibrate read slices and write buffers to the exact bus capabilities:

```mermaid
flowchart TD
    Drive[Target Drive Path] --> Query["Win32 IOCTL Query (IOCTL_STORAGE_QUERY_PROPERTY)"]
    
    subgraph Bus Detection ["Bus Architecture Detection"]
        Query --> Detect{STORAGE_BUS_TYPE}
        Detect -->|BusTypeUsb| USB["External USB Flash (BusTypeUsb)"]
        Detect -->|BusTypeSata / ATAPI| SATA["SATA SSD / HDD (BusTypeSata)"]
        Detect -->|BusTypeNvme| NVME["PCIe M.2 NVMe SSD (BusTypeNvme)"]
        Detect -->|Network / UNC / Unknown| NET["Network Share / Remote (DRIVE_REMOTE)"]
    end

    subgraph Read Slicing ["Source Read Slicing (f.read)"]
        USB --> R_USB["256 KB Read Slice<br/>(Instant Cancel / Zero USB Stalls)"]
        SATA --> R_SATA["512 KB Read Slice<br/>(Smooth Platter Throughput)"]
        NVME --> R_NVME["1 MB Read Slice<br/>(Saturates PCIe Multi-GB Bus)"]
        NET --> R_NET["256 KB Read Slice<br/>(Safe Network Buffer)"]
    end

    subgraph Safe Buffer Capping ["Destination Write Capping (BufWriter)"]
        USB --> W_USB["512 KB Safe Cap<br/>(Prevents RAW Disks & Quick-Removal Freezes)"]
        SATA --> W_SATA["1 MB Safe Cap<br/>(Prevents Platter Thrashing & Queue Congestion)"]
        NVME --> W_NVME["2 MB Calibrated Cap<br/>(Flushes in 1–2 ms; Zero NVMe Queue Stalls)"]
        NET --> W_NET["512 KB Safe Cap<br/>(Prevents Packet Queue Overflows)"]
    end
```

| Storage Medium | Bus Type | Read Slice | Write Buffer Cap | Flush Latency | Core Hardware Safeguard |
|---|:---:|:---:|:---:|:---:|---|
| **External USB / Flash Drive** | `BusTypeUsb` | **256 KB** | **512 KB** | ~0.04 – 0.08s | Prevents Windows Quick-Removal lockups, 20s stalls, and RAW partition corruption |
| **Internal Mechanical HDD / SATA SSD** | `BusTypeSata` | **512 KB** | **1 MB** | ~0.01 – 0.02s | Prevents mechanical platter head thrashing and disk queue congestion |
| **Internal PCIe NVMe SSD (M.2)** | `BusTypeNvme` | **1 MB** | **2 MB** | ~0.001 – 0.002s | Maximum PCIe line rate; flushes in 1–2ms without controller queue saturation |
| **Network Share / UNC / Fallback** | `DRIVE_REMOTE` | **256 KB** | **512 KB** | Safe generic | Prevents packet queue overflows and network driver latency |

---

## 🔒 Zero-Allocation Streaming AES-256-GCM Encryption

- **In-Place AEAD Streaming**:
  - `ChunkedAesWriter` and `ChunkedAesReader` operate with zero heap allocations per chunk using `AeadInPlace` (`encrypt_in_place` / `decrypt_in_place`).
  - 4 MB chunk size minimizes AEAD authentication tag overhead while maximizing hardware throughput on AES-NI capable CPUs.
  - Constant memory footprint (< 100 MB RAM) regardless of archive size, eliminating Out-Of-Memory (OOM) risks on multi-gigabyte encrypted archives.
  - Explicit 4-byte EOF end-of-stream delimiter verifies payload integrity and rejects truncated streams with `UnexpectedEof`.
- **Key Derivation**: Argon2id / PBKDF2 salt-derived symmetric keys with random 8-byte nonces.

---

## 🪟 Windows File Explorer Integration
<a id="windows-shell-integration"></a>

- **Native Windows 11 Top-Level Context Menu (`IExplorerCommand`)**:
  - Implements Microsoft's modern Windows 11 shell extension standard via companion in-process COM DLL (`bayzip_shell.dll`).
  - Appears directly at the top level of the Windows 11 right-click menu without needing "Show more options" (`Shift+F10`).
  - Cascading sub-commands with native Fluent icons:
    - *"Compress to .bayg (Gamer Preset)"*
    - *"Compress to .bay (Ultra-Fast)"*
    - *"Extract Here"*
    - *"Open in Bayzip"*
  - Multi-selection support: processes all selected files and folders in unified batches.
- **Sparse Package Identity (`AppxManifest.xml`)**:
  - Deploys a Windows Sparse Package on Windows 11 (`Build >= 22000`) for seamless modern shell integration.
- **Classic Windows 10 Fallback**:
  - Dual-mode support maintains standard `HKCU\Software\Classes` shell registry keys for Windows 10 and classic Explorer contexts.

---

## 📂 Supported Formats

| Format | Extension | Create | Extract | Preview | Algorithms & Engines |
|---|---|:---:|:---:|:---:|---|
| **Bayzip** | `.bay` | ✅ | ✅ | ✅ | Zstandard (Levels 1–19), Brotli, LZ4 |
| **Bayzip Gamer** | `.bayg` | ✅ | ✅ | ✅ | Zstd-Fast (Capped at Level 3 for Max Throughput) |
| **ZIP** | `.zip` | ✅ | ✅ | ✅ | Deflate, Store (Zero-CPU Smart Bypass), BZip2 |
| **7-Zip** | `.7z` | ✅ | ✅ | ✅ | LZMA2 Multi-Threaded |
| **TAR** | `.tar` | ✅ | ✅ | ✅ | POSIX Tar (Store) |
| **GZip / TarGz** | `.gz`, `.tgz` | — | ✅ | ✅ | Deflate / Tar |
| **BZip2 / TarBz2** | `.bz2`, `.tar.bz2` | — | ✅ | ✅ | BZip2 / Tar |
| **XZ / TarXz** | `.xz`, `.tar.xz` | — | ✅ | ✅ | LZMA2 / Tar |
| **Zstandard Tar** | `.zst`, `.tar.zst` | — | ✅ | ✅ | Zstandard / Tar |
| **ISO Image** | `.iso` | — | ✅ | ✅ | ISO 9660 Disc Image |
| **Cabinet** | `.cab` | — | ✅ | ✅ | Microsoft Cabinet |
| **RAR** | `.rar` | — | ✅ | ✅ | Uncompressed & Standard RAR via libarchive |

---

## 🚀 Building & Running

### Prerequisites
- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/) (2021 Edition / 1.77+) with `x86_64-pc-windows-msvc` target
- Windows 10 or 11 (64-bit)

### Development
```bash
# Clone the repository
git clone https://github.com/Baymax005/Bayzip.git
cd Bayzip

# Install frontend dependencies
npm install

# Run desktop app in development mode
npm run tauri dev
```

### Production Build (.exe / NSIS Installer)
```bash
npm run tauri build
```
The compiled installer will be located in `src-tauri/target/release/bundle/nsis/`.

<a id="automated-tests"></a>
### Run Test Suite
```bash
# Backend unit tests (crypto, path sanitization, priority guards, specs)
cargo test --manifest-path src-tauri/Cargo.toml

# Frontend TypeScript typecheck
npx tsc --noEmit
```

---

## 👤 Author

**Muhammad Ali** ([@Baymax005](https://github.com/Baymax005))

## 📜 License

Distributed under the MIT License. See [LICENSE](LICENSE) for details.

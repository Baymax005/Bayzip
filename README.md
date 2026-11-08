<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Bayzip Logo" width="100" height="100" />
</p>

<h1 align="center">Bayzip</h1>

<p align="center">
  <b>Next-Generation, Hardware-Accelerated High-Throughput Archiver for Windows</b><br>
  <i>Engineered with Tauri 2, Rust & React 19 for maximum multi-core saturation, streaming AEAD encryption, and native Windows 11 Fluent shell integration.</i>
</p>

<p align="center">
  <a href="https://github.com/Baymax005/bayzip-private/releases"><img src="https://img.shields.io/badge/version-v2.3.4-0078D4?style=for-the-badge&logo=semver&logoColor=white" alt="Version 2.3.4" /></a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6?style=for-the-badge&logo=windows11&logoColor=white" alt="Platform Windows" />
  <img src="https://img.shields.io/badge/Tauri-v2.0-FFC131?style=for-the-badge&logo=tauri&logoColor=black" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/Rust-2021%20Edition-DEA584?style=for-the-badge&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/React-19-61DAFB?style=for-the-badge&logo=react&logoColor=black" alt="React 19" />
  <img src="https://img.shields.io/badge/TypeScript-5.6-3178C6?style=for-the-badge&logo=typescript&logoColor=white" alt="TypeScript" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Security-AES--256--GCM%20AEAD-10B981?style=for-the-badge&logo=shield&logoColor=white" alt="AES-256-GCM AEAD" />
  <img src="https://img.shields.io/badge/Tests-31%2F31%20Passing-success?style=for-the-badge&logo=checkmarx&logoColor=white" alt="31/31 Tests Passing" />
  <img src="https://img.shields.io/badge/Engine-Zstandard%20%7C%20LZ4%20%7C%20Brotli-F59E0B?style=for-the-badge&logo=speedtest&logoColor=white" alt="Compression Engine" />
  <img src="https://img.shields.io/badge/Shell-Windows%2011%20Native%20COM-6366F1?style=for-the-badge&logo=windows&logoColor=white" alt="Windows 11 Shell" />
  <img src="https://img.shields.io/badge/Audit-35%2F35%20Verified-8B5CF6?style=for-the-badge&logo=target&logoColor=white" alt="Audit 35/35" />
  <img src="https://img.shields.io/badge/License-MIT-059669?style=for-the-badge" alt="License MIT" />
</p>

<p align="center">
  <a href="#-key-features"><b>⚡ Key Features</b></a> •
  <a href="#-storage-calibrated-streaming-io"><b>💾 I/O Calibration</b></a> •
  <a href="#-zero-allocation-streaming-aes-256-gcm-encryption"><b>🔒 Security & Cryptography</b></a> •
  <a href="#-windows-file-explorer-integration"><b>🪟 Shell Extension</b></a> •
  <a href="#-supported-formats"><b>📂 Supported Formats</b></a> •
  <a href="#-building--running"><b>🚀 Quick Start</b></a> •
  <a href="CHANGELOG.md"><b>📋 Changelog</b></a>
</p>

---

## ⚡ Highlights

| Feature | Description | Architecture Component |
|---|---|---|
| **🏎️ Ultra-Fast Gamer Preset (`.bayg`)** | Locked at Zstandard Level 3 with multi-core parallelism for maximum game installation and transfer throughput. | `compressor/bay.rs` |
| **🛡️ Zip-Slip & Path Traversal Proof** | Multi-layer sanitization rejecting directory escapes (`..`), DOS device aliases (`CLOCK$`, `CONFIG$`, `CON`), NTFS ADS colons, and illegal characters. | `inspector/mod.rs` |
| **⚡ 100k-File Virtualized UI** | Silky 60 FPS scrolling and $O(1)$ folder lookups using custom debounced virtual scrolling and memoized tree hierarchies. | `VirtualScrollList.tsx` |
| **🔒 Streaming Chunked AEAD Encryption** | 4 MB chunked AES-256-GCM streaming with constant < 100 MB RAM footprint and cryptographic EOF truncation detection. | `crypto.rs` |
| **🪟 Windows 11 Native Context Menu** | In-process Win32 COM DLL (`IExplorerCommand`) top-level menu integration with MSIX Sparse Package identity and verified binary resolution. | `bayzip_shell.dll` |
| **💾 Storage Bus-Aware Buffer Slicing** | Dynamic buffer calibration (NVMe 2 MB, SATA 1 MB, USB 512 KB) preventing controller lockups, platter thrashing, and RAW disk corruption. | `compressor/mod.rs` |

---

## ⚡ Key Features

### 🖥️ Desktop Archiver Workspace & Modal Extraction Dialog (v2.3.4)
- **Primary Desktop Archiver Action Toolbar (`WorkspaceHeader.tsx`)**:
  - Primary desktop archiver action toolbar:
    - `📂 Open` (`Ctrl+O`): Browse and inspect any supported archive format directly.
    - `⚡ Extract` (`Ctrl+E`): Triggers the desktop extraction options modal dialog.
    - `📦 Add`: Toggles the advanced multi-threaded compression options panel.
    - `🧪 Test`: Runs immediate cryptographic and format integrity checks on archive headers.
- **Dedicated Desktop Extraction Popup Modal (`WorkspaceExtractModal.tsx`)**:
  - Native modal dialog with folder browsing, subfolder extraction switches, and "Open destination folder on complete" automation.
  - Decryption password field with show/hide toggle and auto-focus for encrypted archives.
  - Keyboard-hardened: `Enter` confirms extraction, `Esc` dismisses, with cyclic focus trapping.
- **Standalone VirtualScrollList (`VirtualScrollList.tsx`)**:
  - Reusable, generic virtual scroll list component handling 100,000+ files at smooth 60 FPS with `requestAnimationFrame` scroll debouncing.

### ⫴ Instant Dual-Pane Split View, Drag-to-Move & Staging
- **Instant Dual-Pane Split View (`⫴` / `Ctrl+\`)**:
  - Side-by-side exploration of local disk directories and archive contents with synchronized/independent directory navigation.
  - Interactive resizable split divider with minimum bounds clamping (200px) and drag handle.
  - Interactive drag-over drop zones for dropping folders into dual-pane staging targets.
- **Interactive Drag-to-Move & Cross-Pane Staging**:
  - Unified drag-and-drop orchestration (`useWorkspaceDragDrop.ts`) across tree folders, table rows, and dual panes.
  - Allows picking items and moving them between folders, staging queues, or onto breadcrumb pills with zero highlight ghosting on cancel.
- **Workspace Panel Layout Controls (`WorkspaceHeader.tsx`)**:
  - Dedicated top-bar 5-icon layout toggle cluster:
    - `◧` **Left Explorer / Folder Tree** (`Ctrl+B`): Toggles tree visibility, allowing full-width file view expansion.
    - `⫴` **Dual-Pane Split** (`Ctrl+\`): Toggles instant split view.
    - `⬒` **Bottom Queue Drawer** (`Ctrl+J`): Toggles the staging/extract queue drawer with active item badge counter.
    - `◨` **Right Options Sidebar**: Toggles the compression/extraction options panel.
    - `🗗` **Compact View** (`Esc`): Restores the compact card view.

### 🏝️ Dynamic Island Header Progress HUD
- **Top Header Dynamic Island HUD (`WorkspaceHeaderProgressHUD.tsx`)**:
  - Centered titlebar progress pill featuring an energetic pulsing status dot, mini progress bar, bold %, and dropdown chevron.
  - Dropdown card reveals real-time **Estimated Time (ETA)**, live **Throughput Speed (`MB/s`)**, monospace file path, byte counts, and 1-click cancellation.
  - Isolated from Win32 DWM window dragging via `stopPropagation()`.

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
- [Rust](https://www.rust-lang.org/) (1.75+) with `x86_64-pc-windows-msvc` target
- Windows 10 or 11 (64-bit)

### Development
```bash
# Clone the repository
git clone https://github.com/Baymax005/bayzip-private.git
cd bayzip-private

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

### Run Test Suite
```bash
# Backend unit tests (crypto, path sanitization, priority guards, specs)
cargo test --manifest-path src-tauri/Cargo.toml

# Frontend TypeScript typecheck
npx tsc --noEmit
```

---

## 📜 License

This project is licensed under the **MIT License** — see the [LICENSE](LICENSE) file for details.

---

<p align="center">
  <b>Bayzip</b> • Engineered with ❤️ by <a href="https://github.com/Baymax005">Muhammad Ali (Baymax)</a>
</p>

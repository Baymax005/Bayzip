# 📦 Bayzip

**Bayzip** is a modern file archiver for Windows built with **Tauri 2**, **Rust**, and **React**. It offers fast multi-algorithm compression, archive extraction, and Windows File Explorer shell integration.

---

## ⚡ Current Features

- **Multi-Format Compression**:
  - **`.bay`**: High-performance custom archive format supporting **Zstandard**, **Brotli**, and **LZ4** algorithms.
  - **`.bay_gamer`**: Gaming mod pack preset using fast Zstandard compression for quick packing and unpacking.
  - **`.7z`**: 7-Zip format using **LZMA2** compression.
  - **`.zip`**: Standard ZIP format with **Deflate**, uncompressed **Store**, and **BZip2** options (with Windows Explorer compatibility safeguards).
  - **`.tar`**: Uncompressed TAR container packaging.
- **Archive Extraction & Inspection**:
  - Extracts and lists contents of `.bay`, `.bay_gamer`, `.zip`, `.7z`, `.tar`, `.gz`, `.tgz`, and `.rar` archives.
  - In-app archive file search to filter entries before extracting.
- **Windows File Explorer Integration**:
  - **Context Menus**: Right-click files or folders to quickly compress to `.bay`, `.zip`, `.7z`, or `.tar`.
  - **Quick Extract**: Right-click archives to *"Extract to dedicated folder"*, *"Extract Here"*, or *"Open in Bayzip"*.
  - **File Associations & Icons**: Shows Bayzip icons on `.bay` and `.bay_gamer` files in File Explorer.
- **Password Protection**:
  - Encrypt archives with **AES-256** or **AES-128**.
  - Password prompt for extracting encrypted archives.
- **Hardware-Aware Performance Slider**:
  - Live system specs detection (CPU cores and free RAM).
  - Slider controls compression intensity and displays estimated RAM and CPU usage.
- **User Interface**:
  - Dark Windows 11 Fluent-style UI.
  - Drag-and-drop support for files and folders.
  - Progress bar with percentage, current file, elapsed time, and compression ratio.
  - Separate archives option (compress each selected file/folder into its own archive).

---

## 📂 Supported Formats

| Format | Extensions | Creation / Compression | Extraction | Compression Method |
|---|---|:---:|:---:|---|
| **Bayzip** | `.bay` | ✅ | ✅ | Zstandard, Brotli, LZ4 |
| **Bayzip Gamer** | `.bay_gamer` | ✅ | ✅ | Zstd-Fast |
| **ZIP** | `.zip` | ✅ | ✅ | Deflate, Store, BZip2 |
| **7-Zip** | `.7z` | ✅ | ✅ | LZMA2 |
| **TAR** | `.tar` | ✅ | ✅ | Uncompressed (Store) |
| **GZip / TarGz** | `.gz`, `.tgz` | — | ✅ | Deflate / Tar |
| **RAR** | `.rar` | — | ✅ | Decompress only |

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

### Build Production Executable (.exe)

```bash
npm run tauri build
```

The compiled installer and executable will be placed in `src-tauri/target/release/bundle/nsis/`.

---

## 📜 License

Private © Bayzip.

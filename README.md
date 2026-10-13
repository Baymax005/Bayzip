# 📦 Bayzip

**Bayzip** is a modern, ultra-fast, high-compression archiving tool designed for Windows. Built on **Tauri 2**, **Rust**, and **React**, it combines state-of-the-art compression algorithms with a sleek Windows 11 Fluent dark interface and deep Windows File Explorer integration.

---

## ⚡ Highlights

- 🚀 **Next-Gen Compression**: Support for **Zstandard**, **Brotli**, **LZ4**, **LZMA2**, **Deflate**, and **BZip2**.
- 🎮 **Gamer Mod Pack (`.bay_gamer`)**: Tailored for gamers with NVMe-speed decompression (3+ GB/s) for mods, textures, and fast asset streaming.
- 🪟 **Seamless Windows Integration**:
  - Native Windows File Explorer context menus (*"Add to archive with Bayzip"*, *"Extract with Bayzip"*).
  - High-resolution custom icons and file associations for `.bay` and `.bay_gamer`.
  - Smart ZIP compatibility: creates 100% Windows Explorer compatible `.zip` files (standard Deflate with dynamic ZIP64 handling).
- 🔒 **Military-Grade Security**: Optional AES-256 and AES-128 encryption with PBKDF2/Argon-strength key derivation.
- 💻 **Hardware-Adaptive Engine**: Real-time system specs monitoring (CPU cores and RAM) with a customizable hardware allocation slider.
- 🎨 **Modern Fluent UI**: Clean, responsive dark aesthetic with real-time compression progress, speed benchmarks, and compression ratio analytics.

---

## 📂 Supported Formats & Algorithms

| Format | Extensions | Supported Compression Algorithms | Recommended Use Case |
|---|---|---|---|
| **Bayzip Archive** | `.bay` | Zstandard (Default), Brotli, LZ4 | General backups, extreme compression ratio & fast decompression |
| **Gamer Mod Pack** | `.bay_gamer` | Zstd-Fast (Auto) | Game mods, high-res texture packs, lightning-fast game loading |
| **7-Zip Archive** | `.7z` | LZMA2, LZMA, BZip2 | Maximum archive density, cross-platform distribution |
| **ZIP Archive** | `.zip` | Deflate (Standard), Store, BZip2 | Universal cross-platform and Windows File Explorer compatibility |
| **TAR Archive** | `.tar` | None (Store / Packaging) | Raw archival container |

---

## 🛠️ Windows File Explorer Features

- **Context Menu Integration**: Right-click any file or directory to instantly add to `.bay`, `.zip`, `.7z`, or `.tar`.
- **1-Click Extraction**: Right-click any supported archive to extract to a dedicated folder or directly in place.
- **File Associations**: Double-clicking `.bay` or `.bay_gamer` files launches Bayzip with an instant archive inspector.

---

## 🚀 Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (v18 or newer)
- [Rust & Cargo](https://www.rust-lang.org/tools/install) (1.75+ or newer)
- Windows 10 / 11

### Running in Development Mode

```bash
# 1. Install frontend dependencies
npm install

# 2. Run the Tauri development app
npm run tauri dev
```

### Building the Production Executable (.exe)

To generate the standalone Windows installer and portable `.exe`:

```bash
npm run tauri build
```

The compiled binaries and installers will be generated in:
```
src-tauri/target/release/bundle/nsis/
src-tauri/target/release/bundle/msi/
src-tauri/target/release/
```

---

## 📜 License

Private & Proprietary © Bayzip. All rights reserved.

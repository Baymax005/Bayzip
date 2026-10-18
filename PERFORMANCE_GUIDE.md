# ⚡ Bayzip Performance, Hardware Allocation & Algorithm Reference

This document serves as the permanent technical reference for Bayzip's hardware scaling, RAM buffer allocations, compression levels, and disk throughput metrics across all supported formats.

---

## 🎚️ 1. Hardware Allocation Slider Matrix

The **Hardware Allocation** slider controls three core parameters simultaneously:
1. **CPU Thread Allocation**: The exact number of CPU cores assigned to worker threads.
2. **I/O RAM Buffers**: Dynamic memory assigned to read and write operations to minimize disk system calls.
3. **OS Priority Scheduling**: Automatically applies `ScopedPriorityGuard` (`THREAD_PRIORITY_BELOW_NORMAL` / `0x00004000`) so foreground games, audio, and the Windows GUI remain 100% responsive.

| Slider Position | CPU Threads Assigned | Read Buffer (RAM) | Write Buffer (RAM) | OS Thread Priority | Recommended Use Case |
| :---: | :---: | :---: | :---: | :---: | :--- |
| **0% (Eco / Single)** | **1 Core** | **2 MB** | **16 MB** | Below Normal | Competitive gaming in foreground (Valorant, CS2) |
| **25% (Low)** | `max(1, Total / 4)` | **4 MB** | **32 MB** | Below Normal | Multitasking, streaming, light workloads |
| **50% (Balanced)** | `max(1, Total / 2)` | **8 MB** | **64 MB** | Below Normal | Standard daily balance of speed and power |
| **75% (Performance)**| `max(1, (Total * 3) / 4)` | **16 MB** | **96 MB** | Below Normal | Fast compression when idle or web browsing |
| **100% (Maximum)** | **All Cores (100%)** | **16 MB** | **128 MB** | Below Normal | Dedicated maximum hardware throughput |

### 🔄 Source & Destination Hardware-Aware Sequential I/O Architecture (v1.2.4)

Bayzip decouples memory allocation from physical drive I/O to guarantee zero freezes on slow drives and line-rate speed on fast drives across both reading and writing:

1. **Source Storage Read Slicing (`get_optimal_read_slice`)**:
   - The exact maximum byte count requested per `f.read()` system call from the Windows kernel storage driver (`usbstor.sys`, `storahci.sys`, `stornvme.sys`).
   - Automatically detected via Win32 `IOCTL_STORAGE_QUERY_PROPERTY` and cached atomically:
     - **External USB / Flash Thumb Drives / Ext HDDs**: **Fixed 1 MB** (safe against controller retries/bad sectors)
     - **Internal SATA SSD / HDD**: **Fixed 4 MB** (aligns with AHCI PRD tables & read-ahead buffers)
     - **Internal PCIe NVMe SSD**: **Fixed 8 MB** (saturates multi-gigabyte PCIe line rate)
     - **Network / UNC Share**: **Fixed 2 MB** (safe fallback)

2. **Destination Storage Write Buffer Capping (`get_optimal_write_buffer`)**:
   - **The Windows "Quick Removal" Problem**: Starting in Windows 10 (1809) and continuing in Windows 11, Microsoft sets the default removal policy for all external USB storage devices to **Quick Removal**. Under this policy, Windows completely **disables disk write caching**. Every write flush bypasses OS write-back caching and directly stalls until physically committed to flash cells. On budget USB flash drives writing at $5\text{ MB/s} - 15\text{ MB/s}$, dumping an uncapped 128 MB write buffer locks the operating system thread for **8 to 25+ seconds**, causing UI progress freezes and unresponsive cancellation.
   - **Destination-Aware Write Matrix**: Bayzip queries the target output drive's physical bus and applies a hardware-calibrated cap:
     - **External USB / Flash Thumb Drives (`BusTypeUsb` / `DRIVE_REMOVABLE`)**: Capped at **16 MB**. Flushes in ~1.0–1.8 seconds on slow flash, guarantees smooth 20 FPS progress bar updates, instant cancellation response, and eliminates USB controller queue lockups.
     - **Internal SATA SSD / HDD (`BusTypeSata`)**: Capped at **64 MB**. Flushes in ~0.5 seconds on mechanical hard drives, maximizes sequential writes without platter head thrashing, and prevents RAM waste on laptops.
     - **Internal PCIe NVMe SSD (`BusTypeNvme`)**: Full **128 MB** (at 100% Hardware Allocation slider). Flushes in only 25–40 milliseconds, saturating multi-gigabyte PCIe Gen 3/4/5 NVMe line rate ($3{,}000+\text{ MB/s}$) with minimum syscall context switches.
     - **Network / UNC Share / Unknown**: Capped at safe **32 MB**.

### 📊 Storage Bus I/O Scaling Matrix

| Storage Bus Type | Win32 Detection | Source Read Slice | Max Destination Write Cap | Typical Flush Duration | Core Benefit |
| :--- | :--- | :---: | :---: | :---: | :--- |
| **External USB / Flash Thumb Drive** | `BusTypeUsb` / `DRIVE_REMOVABLE` | **1 MB** | **16 MB** | ~1.0 – 1.8s | Zero thread stalls under Windows Quick-Removal; responsive Cancel |
| **Internal SATA SSD / HDD** | `BusTypeSata` / `BusTypeAta` / `Scsi` | **4 MB** | **64 MB** | ~0.4 – 0.7s | Eliminates HDD head thrashing; smooth sequential writes |
| **Internal PCIe NVMe SSD** | `BusTypeNvme` / `BusTypeScm` | **8 MB** | **128 MB** | ~0.02 – 0.04s | Maximum multi-gigabyte PCIe line rate ($3{,}000+\text{ MB/s}$) |
| **Network / UNC / Fallback** | Fallback / `DRIVE_REMOTE` | **2 MB** | **32 MB** | Safe generic | Prevents network buffer congestion and generic bus stalls |

$$\text{Effective Syscall Read Chunk} = \min(\text{Slider RAM Buffer}, \text{Hardware Read Slice})$$
$$\text{Effective Output Write Buffer} = \min(\text{Slider RAM Buffer}, \text{Destination Write Cap})$$

*Example:* At 100% Slider, writing an archive directly to an **External USB Thumb Drive** uses a safe, responsive **16 MB write buffer** (preventing 25-second OS thread hangs), while writing to an **Internal NVMe SSD** uses the full **128 MB write buffer** for raw PCIe burst throughput. In both directions, Bayzip optimizes for the physical drive medium.

---


## 🏎️ 2. Format: `.bayg` (Fast Gaming & Modpack Preset)

The `.bayg` container is engineered specifically for **game mods, texture packs, and local disk backups**. Its primary goal is **instant, near-zero-latency decompression** that saturates modern NVMe SSDs.

* **Primary Engine**: Multi-threaded **Zstandard Fast**.
* **Decompression Throughput**: **1.8 GB/s to 3.2 GB/s** (Direct NVMe burst speeds).
* **Compression Throughput**: **400 MB/s to 900+ MB/s** (CPU/Disk dependent).
* **Decompression Latency**: Near zero (files unpack at raw copy-paste speeds).

### Compression Level Lock by Slider:
| Slider Value | Zstandard Level | Working RAM per Thread | Total Peak RAM (16T CPU) |
| :---: | :---: | :---: | :---: |
| **0% – 25%** | **Level 1** | ~2.5 MB | ~40 MB |
| **50%** | **Level 2** | ~3.0 MB | ~90 MB (inc. 64 MB buffer) |
| **75% – 100%** | **Level 3 (Hard-Capped)** | ~3.5 MB | **~160 MB – 190 MB (inc. 128 MB buffer)** |

> [!NOTE]
> In `.bayg`, Zstd is locked at a maximum of **Level 3** across 75% and 100%. This is intentional: higher levels increase decompression CPU overhead, which slows down game loading and extraction.

---

## 📦 3. Format: Standard `.bay` (High Ratio & Universal)

Standard `.bay` archives scale from fast daily compression all the way to **Ultra-Density Ratio Mode** matching 7-Zip.

### A. Zstandard (Recommended)
Scales dynamically from Level 1 up to Level 19:
| Slider Value | Zstandard Level | Working RAM per Thread | Ratio Profile |
| :---: | :---: | :---: | :--- |
| **0%** | **Level 1** | ~2.5 MB | Fast draft packaging |
| **25%** | **Level 3** | ~3.5 MB | Balanced daily compression |
| **50%** | **Level 6** | ~6.5 MB | Strong ratio / fast multi-core speed |
| **75%** | **Level 9** | ~12 MB | High compression density (100+ MB/s) |
| **100%** | **Level 19 (Ultra)** | **~64 MB (Capped)** | **Maximum space saving (matches 7z Ultra)** |

* **Total Peak RAM at 100%**: Strictly bounded to **~450 MB – 600 MB** (write buffer capped to 32 MB and worker threads capped to a maximum of 8 to eliminate multi-gigabyte RAM spikes).
* **Inspection Speed**: **$O(1)$ Instant** via binary `BAYTOC\x02\x00` trailer written at the end of the file.

### B. Brotli (Text & Web Data)
* **Best For**: Source code repositories, JSON, XML, HTML, and text-heavy datasets.
* **Working RAM**: ~16 MB per thread.
* **Throughput**: 30 MB/s – 120 MB/s compression.

### C. LZ4 (Fastest Raw Throughput)
* **Best For**: Maximum raw I/O throughput (> 1,200 MB/s compression, > 4,000 MB/s decompression).
* **Working RAM**: < 1 MB per thread.
* **CPU Impact**: Negligible (< 10% CPU usage).

---

## 🗜️ 4. Format: `.7z` (7-Zip LZMA2)

* **Primary Engine**: LZMA2 Multi-threaded.
* **Primary Goal**: **Smallest file size possible** for cloud uploads, Discord, or slow internet sharing.
* **Dictionary Scaling**: Scales from 16 MB (0%) up to 64 MB (100%).
* **Working RAM**: **150 MB to 800 MB** depending on thread count and dictionary size.
* **Compression Speed**: 25 MB/s – 80 MB/s (heavily CPU-bound).
* **Decompression Speed**: 40 MB/s – 120 MB/s.

---

## 🗃️ 5. Format: `.zip` & `.tar`

| Format | Algorithm | Throughput | RAM Footprint | Compatibility |
| :---: | :---: | :---: | :---: | :---: |
| **`.zip`** | Deflate | 60 MB/s – 160 MB/s | ~15 MB | 100% universal across Windows, Mac, Linux, iOS, Android |
| **`.zip`** | Store (Uncompressed) | Disk write limited (> 1 GB/s) | ~5 MB | Instant packaging without CPU usage |
| **`.tar`** | Raw Container | Disk write limited (> 1 GB/s) | ~5 MB | Standard Unix / Linux archive container |

---

## 🔒 6. Encryption Specs (AES-256-GCM)

* **Cipher**: AES-256-GCM via `AeadInPlace` (authenticated encryption with associated data).
* **Key Derivation Function (KDF)**: **Argon2id** (16-byte random salt, 64 MB memory cost, 3 time iterations).
* **Streaming Chunks**: **4 MB per chunk** with unique 12-byte nonce and 16-byte authentication tag per block.
* **Memory Allocation**: **0 heap allocations per chunk** (in-place buffer reuse).

---

## 💾 7. Real-World Disk Hardware Throughput Guide

Understanding real-world drive physical limits helps distinguish between software performance and hardware bottlenecks:

| Drive Type | Typical Read Speed | Typical Write Speed | Simultaneous Read + Write (Same Drive) |
| :--- | :---: | :---: | :---: |
| **PCIe Gen 4 NVMe SSD (Internal)** | 4,000 – 7,000 MB/s | 3,000 – 5,000 MB/s | **2,500+ MB/s** (Zero bottleneck) |
| **PCIe Gen 3 NVMe SSD (Internal)** | 2,000 – 3,500 MB/s | 1,500 – 2,800 MB/s | **1,200+ MB/s** |
| **SATA 2.5" SSD (Internal)** | 500 – 550 MB/s | 450 – 520 MB/s | **~250 MB/s** |
| **External USB 3.0 HDD (2.5" 5400 RPM)** | **80 – 120 MB/s** | **70 – 110 MB/s** | **~40 – 95 MB/s Total Combined** |

### Why "Active Time: 100%" Occurs on External USB Hard Drives:
1. **Mechanical Seeking**: Mechanical hard drives have a single physical actuator arm. When reading game files and writing the `.bayg` file to the same drive, the mechanical head must physically jump between read and write tracks.
2. **Bandwidth Splitting**: An external USB mechanical drive has a physical ceiling of ~100 MB/s. If reading at **74 MB/s**, only **~23 MB/s** physical bandwidth remains for writing.
3. **The Benefit of Bayzip's Hardware-Aware Buffer**: Without Bayzip's buffered writes, the drive head would thrash every few kilobytes, dropping speed to ~30 MB/s. With Bayzip's calibrated 16 MB write buffer cap on external USB devices (and 64 MB on internal SATA), writes occur in smooth sequential bursts without risking multi-second write freezes under Windows Quick-Removal policy.

---

## ⚡ 8. Extraction Pipeline I/O & Memory Allocation Specs

| Pipeline Stage | Buffer Type | RAM Allocation | Chunk / Transfer Size | Purpose |
| :--- | :--- | :---: | :---: | :--- |
| **Archive Source Read** | `BufReader` | **16 MB** | 16 MB blocks | Minimizes disk read seeks on archive container |
| **Stream Decompressor Pipe** | `io_buf` vector | **4 MB** | 4 MB transfer | Cuts Windows kernel syscall context switches (`NtWriteFile`) |
| **Medium File Writer** | `BufWriter` | **2 MB** | 2 MB bursts | Smooth buffered writes for files $\le$ 16 MB |
| **Large File Writer** | `BufWriter` | **16 MB** | 16 MB bursts | Saturates multi-channel NVMe flash / avoids HDD thrashing |
| **Total Peak Extraction RAM** | Constant | **~36 MB** | Dynamic reuse | Fixed memory footprint regardless of archive size |

### Write Staging & Buffer Allocation by File Size

| File Category | File Size Threshold | Target File Mode | Write Buffer Capacity | Temporary Staging | Windows Zero-Fill Overhead |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Tiny Files** | $\le$ 64 KB | Direct File | **Unbuffered** (Direct write) | None | **0 bytes** |
| **Medium Files** | 64 KB – 16 MB | Staged `.baypart` | **2 MB (`BufWriter`)** | `.baypart` $\to$ Atomic rename | **0 bytes** |
| **Large Files** | > 16 MB | Staged `.baypart` | **16 MB (`BufWriter`)** | `.baypart` $\to$ Atomic rename | **0 bytes** |

### Disk Write Volume & Multiplier Comparison

| Metric / Scenario | Legacy Pre-1.2.0 | v1.2.0 Optimized | Efficiency Gain |
| :--- | :---: | :---: | :---: |
| **Windows NTFS Write Multiplier** | **2.0×** (Zero-fill + Data write) | **1.0×** (Single-pass direct write) | **50% I/O reduction** |
| **44.3 GB Game Archive Extraction** | **88.6 GB** written to disk | **44.3 GB** written to disk | **-44.3 GB disk write load** |
| **Archive Read Block Size** | 4 MB | **16 MB** | **4× larger sequential reads** |
| **Decompressor I/O Pipe Block** | 2 MB | **4 MB** | **2× fewer system calls** |
| **Large File Burst Write Buffer** | 8 MB | **16 MB** | **2× larger sequential writes** |
| **SSD Write Wear (TBW Endurance)** | Double write wear | **Zero extra wear** | **100% standard SSD preservation** |

---

## 📊 9. Algorithm-Specific RAM Footprint Matrix (with Headroom)

Peak working memory consists of thread encoder buffers, sequential I/O buffers, hash chain tables, and safety headroom for OS thread scheduling:

| Format | Algorithm | 0% Slider | 25% Slider | 50% Slider | 75% Slider | 100% Slider |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: |
| **`.tar`** | None (Store) | `< 0.1 GB` | `< 0.1 GB` | `< 0.1 GB` | `< 0.1 GB` | `< 0.1 GB` |
| **`.zip`** | Deflate (Standard) | `< 0.1 GB` | `< 0.1 GB` | `< 0.1 GB` | `~0.1 GB` | `~0.1 GB` |
| **`.zip`** | Store (Uncompressed) | `< 0.1 GB` | `< 0.1 GB` | `< 0.1 GB` | `< 0.1 GB` | `< 0.1 GB` |
| **`.zip` / `.7z`** | BZip2 | `~0.1 GB` | `~0.1 GB` | `~0.1 GB` | `~0.2 GB` | `~0.2 GB` |
| **`.bay`** | LZ4 (Fastest) | `< 0.1 GB` | `< 0.1 GB` | `< 0.1 GB` | `~0.1 GB` | `~0.1 GB` |
| **`.bay`** | Brotli | `~0.1 GB` | `~0.1 GB` | `~0.2 GB` | `~0.3 GB` | `~0.45 GB` |
| **`.bayg`** | Zstandard Fast | `< 0.1 GB` | `< 0.1 GB` | `~0.15 GB` | `~0.2 GB` | `~0.25 GB` |
| **`.bay`** | Zstandard (Recommended) | `~0.15 GB` | `~0.25 GB` | `~0.35 GB` | `~0.5 GB` | **`~0.75 GB (Peak Capped)`** |
| **`.7z`** | LZMA2 Multi-Thread | `~0.15 GB` | `~0.25 GB` | `~0.4 GB` | `~0.6 GB` | **`~0.8 GB (Peak)`** |

> [!NOTE]
> Values above represent **realistic peak allocations with safety margin** (including OS thread allocation overhead, hash table indexing, and sequential I/O buffers), ensuring users are never surprised by memory usage.



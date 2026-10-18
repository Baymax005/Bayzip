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
| **50%** | **Level 7** | ~8 MB | Strong ratio / good speed |
| **75%** | **Level 14** | ~16 MB | High compression density |
| **100%** | **Level 19 (Ultra)** | **~32 MB – 64 MB** | **Maximum space saving (90–95% of 7z Ultra)** |

* **Total Peak RAM at 100%**: ~350 MB – 500 MB (due to 128 MB I/O buffer + Zstd-19 sliding window).
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
3. **The Benefit of Bayzip's 64 MB Buffer**: Without Bayzip's large write buffer, the drive head would thrash every few kilobytes, dropping speed to ~30 MB/s. With the 64 MB RAM buffer, writes occur in smooth sequential bursts, allowing the drive to sustain its maximum possible physical speed (~97 MB/s).

---

## ⚡ 8. High-Throughput Extraction I/O Architecture (v1.2.0)

In `v1.2.0`, Bayzip's extraction pipelines across all formats (`.bay`, `.bayg`, `.zip`, `.7z`, `.tar`) were overhauled to eliminate Windows filesystem stalls, reduce SSD write wear, and maximize hardware saturation:

### A. Elimination of Windows NTFS Synchronous Zero-Filling (`set_len` Removal)
* **The Windows NTFS Trap**: In Unix/Linux systems, pre-allocating disk space (`posix_fallocate`) is instantaneous. On Windows NTFS, however, calling `SetEndOfFile()` (`File::set_len`) on an unprivileged handle forces the operating system kernel to **synchronously zero-fill every allocated cluster on disk** to satisfy Windows C2 security isolation requirements.
* **The Penalty**: For a 44 GB game archive with large 2–4 GB package files, Windows was writing **44 GB of zeroes** to disk, and then writing the **44 GB of actual decompressed game files** — generating **88 GB of physical disk writes**! On a 35 MB/s external USB drive, zero-filling added 20+ minutes of pure waste; on PCIe Gen 4/5 NVMe SSDs, it burned twice as much drive write endurance (TBW).
* **The Optimization**: Removed all pre-emptive `set_len` calls across all unpackers. Decompressed streams are written directly to disk in a single pass, cutting disk write volume **cleanly in half (50% reduction)**.

### B. Upgraded Sequential I/O Buffering
* **16 MB Archive Read Buffer (`BufReader`)**: Reads compressed container streams in massive 16 MB blocks. On mechanical drives, this keeps the read head planted on the archive track far longer; on NVMe SSDs, it maximizes multi-channel flash read bandwidth.
* **4 MB Decompressor Transfer Buffer (`io_buf`)**: Doubles the per-chunk read/write transfer size between the compression decoder and file writers, cutting Windows kernel context switches (`NtWriteFile`) in half.
* **16 MB Large-File Write Buffer (`BufWriter`)**: Accumulates decompressed bytes for files larger than 16 MB into 16 MB sequential write bursts, maximizing disk cache coalescence and eliminating fragmented micro-writes.

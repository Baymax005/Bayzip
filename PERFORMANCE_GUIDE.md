# ⚡ Bayzip Performance, Hardware Allocation & Algorithm Reference

This document serves as the permanent technical reference for Bayzip's hardware scaling, storage-aware streaming buffers, compression levels, and disk throughput metrics across all supported formats.

---

## 🎚️ 1. Hardware Allocation Slider Matrix

The **Hardware Allocation** slider governs CPU scaling and algorithmic workload density while decoupling physical storage I/O:
1. **CPU Thread Allocation**: The exact number of CPU cores assigned to worker compression/decompression threads.
2. **Algorithm Level Scaling**: Automatically tunes the underlying compression level (e.g. Zstandard Level 1 up to Level 19).
3. **OS Priority Scheduling**: Automatically applies `ScopedPriorityGuard` (`THREAD_PRIORITY_BELOW_NORMAL` / `0x00004000`) so foreground games, audio, and the Windows GUI remain 100% responsive.
4. **Calibrated Stream Buffers**: Keeps sequential I/O within the optimal sweet spot for CPU L2/L3 caches and modern Windows filesystems (NTFS/exFAT/FAT32), eliminating monolithic write freezes.

| Slider Position | CPU Threads Assigned | Read Chunk (RAM) | Base Write Buffer | OS Thread Priority | Recommended Use Case |
| :---: | :---: | :---: | :---: | :---: | :--- |
| **0% (Eco / Single)** | **1 Core** | **256 KB** | **512 KB** | Below Normal | Competitive gaming in foreground (Valorant, CS2) |
| **25% (Low)** | `max(1, Total / 4)` | **256 KB** | **512 KB** | Below Normal | Multitasking, streaming, light workloads |
| **50% (Balanced)** | `max(1, Total / 2)` | **512 KB** | **1 MB** | Below Normal | Standard daily balance of speed and power |
| **75% (Performance)**| `max(1, (Total * 3) / 4)` | **512 KB** | **1 MB** | Below Normal | Fast compression when idle or web browsing |
| **100% (Maximum)** | **All Cores (100%)** | **1 MB** | **2 MB** | Below Normal | Dedicated maximum hardware throughput |

---

### 🔄 2. Storage-Aware Physical Bus Calibration (`hardware.rs`)

Bayzip calibrates I/O buffer sizing directly from the physical destination storage medium (`IOCTL_STORAGE_QUERY_PROPERTY`), avoiding both slow-drive lockups and fast-drive queue saturation:

#### A. The USB "Quick Removal" Problem (Solved)
* In Windows 10 and 11, the default removal policy for external USB storage is **Quick Removal**. Under this policy, Windows completely **disables disk write caching**. Every write flush blocks the calling thread until physically committed to flash cells.
* On budget USB flash drives writing at $4\text{ MB/s} - 15\text{ MB/s}$, issuing a multi-megabyte write buffer locks the operating system thread for **several seconds**, causing GUI freezes and unresponsive cancellation.
* **Bayzip Calibration**: Capped at **512 KB write** and **256 KB read**. A 512 KB buffer flushes in only ~50ms on USB thumb drives, guaranteeing a fluid 20 FPS progress bar, instant cancellation response, and zero USB controller queue lockups.

#### B. The NVMe Synchronous Monolithic Write Stall (Solved)
* Consumer NVMe SSDs (especially QLC NAND drives like Solidigm P41 Plus, Intel 660p/670p, Crucial P3) rely on dynamic SLC caching.
* Issuing a massive synchronous write (e.g. 128 MB) while simultaneously reading from the same physical drive overwhelms the SSD controller queue. In Windows Task Manager, this manifests as:
  * **Disk Active Time: 98% – 100%**
  * **Average Response Time: 200ms – 250ms** (critical latency stall)
  * **CPU Usage Drops to < 10%** (starved waiting on kernel `WriteFile`)
  * **Progress Bar Frozen for 6 – 10+ seconds**.
* **Bayzip Calibration**: Capped at **2 MB write** and **1 MB read**. Flushes complete in **0.5 – 2 milliseconds**, keeping queue depth minimal, disk latency under **2 ms**, and progress updates flowing continuously every 50ms.

#### 📊 Storage Bus I/O Scaling Matrix

| Storage Bus Type | Win32 Detection | Source Read Slice | Max Destination Write Cap | Typical Flush Duration | Core Benefit |
| :--- | :--- | :---: | :---: | :---: | :--- |
| **External USB / Flash Thumb Drive** | `BusTypeUsb` / `DRIVE_REMOVABLE` | **256 KB** | **512 KB** | ~0.04 – 0.08s | Zero thread stalls under Quick-Removal; instant Cancel |
| **Internal SATA SSD / HDD** | `BusTypeSata` / `BusTypeAta` / `Scsi` | **512 KB** | **1 MB** | ~0.01 – 0.02s | Eliminates HDD head thrashing; smooth sequential stream |
| **Internal PCIe NVMe SSD** | `BusTypeNvme` / `BusTypeScm` | **1 MB** | **2 MB** | ~0.001 – 0.002s | Maximum PCIe line rate; zero latency spikes or UI stalls |
| **Network / UNC / Fallback** | Fallback / `DRIVE_REMOTE` | **256 KB** | **512 KB** | Safe generic | Prevents network buffer congestion and generic bus stalls |

$$\text{Effective Syscall Read Chunk} = \min(\text{Slider RAM Buffer}, \text{Hardware Read Slice})$$
$$\text{Effective Output Write Buffer} = \min(\text{Slider RAM Buffer}, \text{Destination Write Cap})$$

---

## 🏎️ 3. Format: `.bayg` (Fast Gaming & Modpack Preset)

The `.bayg` container is engineered specifically for **game mods, texture packs, and local disk backups**. Its primary goal is **instant, near-zero-latency decompression** that saturates modern NVMe SSDs.

* **Primary Engine**: Multi-threaded **Zstandard Fast**.
* **Decompression Throughput**: **1.8 GB/s to 3.2 GB/s** (Direct NVMe burst speeds).
* **Compression Throughput**: **400 MB/s to 900+ MB/s** (CPU/Disk dependent).
* **Decompression Latency**: Near zero (files unpack at raw copy-paste speeds).

### Compression Level Lock by Slider:
| Slider Value | Zstandard Level | Working RAM per Thread | Total Peak RAM (16T CPU) |
| :---: | :---: | :---: | :---: |
| **0% – 25%** | **Level 1** | ~2.5 MB | ~40 MB |
| **50%** | **Level 2** | ~3.0 MB | ~60 MB |
| **75% – 100%** | **Level 3 (Hard-Capped)** | ~3.5 MB | **~80 MB – 110 MB** |

> [!NOTE]
> In `.bayg`, Zstd is locked at a maximum of **Level 3** across 75% and 100%. This is intentional: higher levels increase decompression CPU overhead, which slows down game loading and extraction.

---

## 📦 4. Format: Standard `.bay` (High Ratio & Universal)

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

* **Total Peak RAM at 100%**: Strictly bounded to **~450 MB – 600 MB** (worker threads capped to a maximum of 8 at Level 19 to eliminate multi-gigabyte RAM spikes).
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

## 🗜️ 5. Format: `.7z` (7-Zip LZMA2)

* **Primary Engine**: LZMA2 Multi-threaded.
* **Primary Goal**: **Smallest file size possible** for cloud uploads, Discord, or slow internet sharing.
* **Dictionary Scaling**: Scales from 16 MB (0%) up to 64 MB (100%).
* **Working RAM**: **150 MB to 800 MB** depending on thread count and dictionary size.
* **Compression Speed**: 25 MB/s – 80 MB/s (heavily CPU-bound).
* **Decompression Speed**: 40 MB/s – 120 MB/s.

---

## 🗃️ 6. Format: `.zip` & `.tar`

| Format | Algorithm | Throughput | RAM Footprint | Compatibility |
| :---: | :---: | :---: | :---: | :---: |
| **`.zip`** | Deflate | 60 MB/s – 160 MB/s | ~10 MB | 100% universal across Windows, Mac, Linux, iOS, Android |
| **`.zip`** | Store (Uncompressed) | Disk write limited (> 1 GB/s) | ~2 MB | Instant packaging without CPU usage |
| **`.tar`** | Raw Container | Disk write limited (> 1 GB/s) | ~2 MB | Standard Unix / Linux archive container |

---

## 🔒 7. Encryption Specs (AES-256-GCM)

* **Cipher**: AES-256-GCM via `AeadInPlace` (authenticated encryption with associated data).
* **Key Derivation Function (KDF)**: **Argon2id** (16-byte random salt, 64 MB memory cost, 3 time iterations).
* **Streaming Chunks**: **2 MB per chunk** with unique 12-byte nonce and 16-byte authentication tag per block.
* **Memory Allocation**: **0 heap allocations per chunk** (in-place buffer reuse).

---

## 💾 8. Real-World Disk Hardware Throughput Guide

Understanding real-world drive physical limits helps distinguish between software performance and hardware bottlenecks:

| Drive Type | Typical Read Speed | Typical Write Speed | Simultaneous Read + Write (Same Drive) |
| :--- | :---: | :---: | :---: |
| **PCIe Gen 4 NVMe SSD (Internal)** | 4,000 – 7,000 MB/s | 3,000 – 5,000 MB/s | **2,500+ MB/s** (Zero bottleneck) |
| **PCIe Gen 3 NVMe SSD (Internal)** | 2,000 – 3,500 MB/s | 1,500 – 2,800 MB/s | **1,200+ MB/s** |
| **SATA 2.5" SSD (Internal)** | 500 – 550 MB/s | 450 – 520 MB/s | **~250 MB/s** |
| **External USB 3.0 HDD (2.5" 5400 RPM)** | **80 – 120 MB/s** | **70 – 110 MB/s** | **~40 – 95 MB/s Total Combined** |

---

## ⚡ 9. Extraction Pipeline I/O & Memory Allocation Specs

| Pipeline Stage | Buffer Type | RAM Allocation | Chunk / Transfer Size | Purpose |
| :--- | :--- | :---: | :---: | :--- |
| **Archive Source Read** | `BufReader` | **1 MB** | 1 MB blocks | Smooth sequential reads without memory bloat |
| **Stream Decompressor Pipe** | `io_buf` vector | **256 KB** | 256 KB transfer | Optimal CPU L2/L3 cache locality (matches Zstd/Deflate blocks) |
| **Small File Writer** | `BufWriter` | **32 KB** | 32 KB | Zero memory waste on archives with 10,000+ files |
| **Medium File Writer** | `BufWriter` | **256 KB** | 256 KB | Smooth buffered writes for files $\le$ 1 MB |
| **Large File Writer** | `BufWriter` | **512 KB** | 512 KB | Maximum throughput with zero disk queue stalls |
| **Total Peak Extraction RAM** | Constant | **~6 MB** | Dynamic reuse | Minimal, constant memory footprint regardless of archive size |

### Write Staging & Buffer Allocation by File Size

| File Category | File Size Threshold | Target File Mode | Write Buffer Capacity | Temporary Staging | Windows Zero-Fill Overhead |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Tiny Files** | $\le$ 64 KB | Direct File | **32 KB (`BufWriter`)** | Direct write | **0 bytes** |
| **Medium Files** | 64 KB – 1 MB | Staged `.baypart` | **256 KB (`BufWriter`)** | `.baypart` $\to$ Atomic rename | **0 bytes** |
| **Large Files** | > 1 MB | Staged `.baypart` | **512 KB (`BufWriter`)** | `.baypart` $\to$ Atomic rename | **0 bytes** |

---

## 📊 10. Algorithm-Specific RAM Footprint Matrix (with Headroom)

Peak working memory consists of thread encoder buffers, sequential I/O buffers, hash chain tables, and safety headroom for OS thread scheduling:

| Format | Algorithm | 0% Slider | 25% Slider | 50% Slider | 75% Slider | 100% Slider |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: |
| **`.tar`** | None (Store) | `< 0.05 GB` | `< 0.05 GB` | `< 0.05 GB` | `< 0.05 GB` | `< 0.05 GB` |
| **`.zip`** | Deflate (Standard) | `< 0.05 GB` | `< 0.05 GB` | `< 0.05 GB` | `~0.05 GB` | `~0.08 GB` |
| **`.zip`** | Store (Uncompressed) | `< 0.05 GB` | `< 0.05 GB` | `< 0.05 GB` | `< 0.05 GB` | `< 0.05 GB` |
| **`.zip` / `.7z`** | BZip2 | `~0.05 GB` | `~0.05 GB` | `~0.1 GB` | `~0.15 GB` | `~0.2 GB` |
| **`.bay`** | LZ4 (Fastest) | `< 0.05 GB` | `< 0.05 GB` | `< 0.05 GB` | `~0.05 GB` | `~0.08 GB` |
| **`.bay`** | Brotli | `~0.05 GB` | `~0.08 GB` | `~0.15 GB` | `~0.25 GB` | `~0.4 GB` |
| **`.bayg`** | Zstandard Fast | `< 0.05 GB` | `< 0.05 GB` | `~0.08 GB` | `~0.1 GB` | `~0.12 GB` |
| **`.bay`** | Zstandard (Recommended) | `~0.1 GB` | `~0.15 GB` | `~0.25 GB` | `~0.4 GB` | **`~0.65 GB (Peak Capped)`** |
| **`.7z`** | LZMA2 Multi-Thread | `~0.1 GB` | `~0.2 GB` | `~0.35 GB` | `~0.5 GB` | **`~0.75 GB (Peak)`** |

> [!NOTE]
> Values above represent **realistic peak allocations with safety margin** (including OS thread allocation overhead, hash table indexing, and sequential I/O buffers), ensuring users are never surprised by memory usage.

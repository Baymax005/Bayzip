# 📋 Bayzip Changelog & Release Notes

All notable changes, version milestones, security fixes, and performance enhancements across Bayzip are documented in this file.

## [v2.3.7] - 2026-11-10

### ⚡ Unified Single-Window Archiver & Smart Drop Hub
- **Unified Smart Drop Hub (`src/components/UnifiedDropHub.tsx`)**:
  - Replaced disjointed empty states with a single, intelligent Windows 11 Fluent dark-mode Drop Hub.
  - Active drag-over reactive halo with ambient Fluent Cyan glow and format support badges (`.bayg`, `.bay`, `.zip`, `.7z`, `.tar`, `.rar`, `.gz`, `.xz`, `.iso`, `.cab`).
  - Action cards for 1-click archive opening and file/folder compression selection.
- **Intelligent Context-Aware Auto-Routing (`src/App.tsx`)**:
  - Automatically identifies dropped filesystem items via `isArchiveFile`:
    - **Archives dropped**: Auto-activates Extract mode and begins background directory tree inspection.
    - **Files/Folders dropped**: Auto-activates Compress mode and stages items for multithreaded compression.
  - Seamless view return: closing an archive or clearing staged files returns directly to the Drop Hub.
  - Top segmented control (`[ 📦 Compress ]` and `[ ⚡ Extract ]`) preserved for manual mode toggling at all times.
- **Adaptive Inspector Layout & Keyboard Ergonomics (`ArchiveTreeInspector.tsx`)**:
  - Upgraded table container to responsive `flex-1 min-h-[320px]`, dynamically expanding file listings on window resize or maximization.
  - Added global `Ctrl+F` / `Cmd+F` shortcut to focus and highlight search filter.
  - Added `Backspace` and `Alt+ArrowUp` for instant parent directory navigation.
  - Added `Enter` to navigate into highlighted folders and `Space` for quick in-memory file preview.

## [v2.3.6] - 2026-11-09

### 🧹 Streamlined Architecture & Workspace Retirement
- **Retired Experimental Workspace Mode**:
  - Removed full-screen workspace takeover modal, dual-pane explorer, local drive browser (`C:\`, `D:\`), staged queue drawer, and cross-pane drag-to-move hooks.
  - Eliminated over 6,400+ lines of technical debt and maintenance burden.
  - Reduced production client JS bundle by ~148 kB (from 558 kB to 410 kB) and CSS bundle from 86 kB to 58 kB.
  - Permanently fixed WebView2 HTML5 drag-and-drop platform limitations and ghost highlight glitches.
- **Pure Archive Tree Inspector (`ArchiveTreeInspector.tsx`)**:
  - Refactored into a focused, modular archive viewer (604 lines).
  - Retained fast virtualized 100k-file rendering (`VirtualScrollList.tsx`), directory hierarchy navigation (`..` parent directory, breadcrumbs `📦 root / folder / subfolder`), real-time search filtering, and instant file previews (`FilePreviewModal.tsx`).
- **Laser Focus on Core Fast Archiving**:
  - Re-anchored Bayzip as a lightning-fast, dedicated Windows 11 archiver with instantaneous Compress and Extract workflows.
- **Backend Cleanliness (`src-tauri`)**:
  - Removed unused logical drive enumeration and filesystem manipulation commands (`get_system_drives`, `browse_directory`, `create_directory`, `delete_to_recycle_bin`, `rename_path`, `copy_path`).
  - Retained all core compression, extraction, cryptographic streaming, and hardware bus-calibrated I/O engines with 31/31 unit tests passing.

## [v2.3.5] - 2026-11-08

### 🔍 Archive Inspection & Format Preview Hardening
- **Libarchive Fallback for Compound Tar, ISO & Cabinet Archives (`inspector/tar.rs`)**:
  - Automatically queries Windows system `tar.exe` (libarchive 3.8) when pure Rust parsers return 0 items.
  - Enables full directory tree hierarchy exploration and safe in-memory previews for `.iso`, `.cab`, `.tar.xz`, `.tar.bz2`, and `.tar.zst` files.

## [v2.3.4] - 2026-11-08

### ⚡ 100k-File Virtualization & High-Throughput Performance (Phase 5)
- **Virtualized Tree View & Smooth Scrolling (`V-VIRT-01`, `V-PERF-08`)**:
  - Maintained 60 FPS scrolling on massive archive datasets (100,000+ files) with requestAnimationFrame debouncing and virtualized DOM item rendering.
- **$O(1)$ Tree Exclusion & Lookup Optimization (`V-PERF-01`, `V-PERF-07`)**:
  - Replaced $O(N^2)$ recursive searches in `isDirExcluded` with direct $O(k)$ hierarchy inspection using `TreeNode` node maps.
  - Eliminated intermediate `Array.from` heap allocations by iterating directly over Sets in `isFileExcluded` and `isDirExcluded`.
- **Progress Event Heap Allocation Elimination (`V-PERF-02`)**:
  - Hoisted and memoized `folderItems` and `pane2FolderItems` using `useMemo`, eliminating over 400,000 object allocations per second during 50ms progress HUD telemetry updates.
- **Stat Computation Path Optimization (`V-PERF-03`)**:
  - Replaced string splitting and array creation with non-allocating basename slicing and direct code-point comparisons in `computeStats`.

### 🛡️ Win32 Shell Extension & Build Pipeline Hardening (Phase 6)
- **Isolated Build Script DLL Generation (`V-BUILD-01`)**:
  - Build script copies compiled `bayzip_shell.dll` strictly into Cargo's `OUT_DIR` rather than mutating the source tree in `crates/bayzip_shell`.
  - Linked safely into `src-tauri/src/registry.rs` via `concat!(env!("OUT_DIR"), "/bayzip_shell.dll")`.
- **Process-Unique Registry Temp Files (`V-SEC-05`)**:
  - Replaced static temporary `.reg` files with process-isolated files in `%LOCALAPPDATA%\Bayzip` using `std::process::id()`.
  - Enforced execution exclusively via `%SystemRoot%\System32\reg.exe` and immediate file unlinking.
- **PowerShell Script Injection Elimination (`V-SEC-06`)**:
  - Sanitized MSIX package path strings in `Add-AppxPackage` invocation using `.replace('\'', "''")`, preventing shell quote-breakout exploits.
- **Hardened Shell Extension Binary Resolution (`V-SEC-07`)**:
  - `bayzip_shell.dll` strictly checks verified `%LOCALAPPDATA%\Bayzip\bayzip.exe`, `%ProgramFiles%\Bayzip\bayzip.exe`, and `%ProgramFiles(x86)%\Bayzip\bayzip.exe`.
  - Completely eliminated unauthenticated plaintext `bayzip_path.txt` file reads and unanchored relative binary searches.
- **Deep-Defense Path Sanitization (`V-SEC-08`)**:
  - Added legacy device names `CLOCK$` and `CONFIG$` to `DOS_RESERVED_NAMES`.
  - Added validation rejecting colons (`:`), wildcards (`*`, `?`), redirection characters (`<`, `>`), quotes (`"`), pipes (`|`), null bytes (`\0`), and ASCII control characters `< 32`.
- **Physical Root Container Deletion Protection (`V-DATA-01`)**:
  - Enforced guard preventing deletion of the root directory container in `ArchiveTreeInspector.tsx`.
- **Extract Modal Option Routing (`V-INT-01`)**:
  - Connected `WorkspaceExtractModal` options (destination, extractToSubfolder, openFolderOnExtract) directly to the parent `handleExtract` pipeline.

### 🧩 Architectural Modularity, Cryptography & Test Verification (Phase 7)
- **Cryptographic Streaming AEAD End-of-Stream Authentication (`V-SEC-04`)**:
  - `ChunkedAesReader` verifies the 4-byte EOF end-of-stream delimiter (`chunk_len == 0`), throwing `std::io::ErrorKind::UnexpectedEof` immediately if the ciphertext is truncated.
- **Frontend Utility Centralization (`V-CODE-01`)**:
  - Extracted and centralized duplicated `formatBytes` and `getArchiveStemName` helpers into `src/utils/format.ts` and `src/utils/archive.ts`, deduplicating across 8 UI components.
- **Decoupled Standalone VirtualScrollList Component (`V-ARCH-02`)**:
  - Extracted generic `VirtualScrollList<T>` into `src/components/VirtualScrollList.tsx` with clean TypeScript props and RAF debounced scroll state.
- **Genuine Archive CRC & Header Verification (`CODE-03`)**:
  - Updated "Test Archive" modal action to perform genuine format and header verification rather than artificial timeout loops.
- **Standard Base64 Crate Adoption (`CODE-04`)**:
  - Replaced 22-line manual bit-shifting base64 encoder with `base64::engine::general_purpose::STANDARD.encode` in `src-tauri/src/inspector/preview.rs`.
- **Comprehensive Unit Test Suite (`V-TEST-01`)**:
  - Expanded unit test coverage to 31 tests covering chunked AES encryption roundtrip, truncation detection, wrong password rejection, extended DOS device sanitization, and invalid characters. All 31 tests passing with 0 errors.

## [v2.3.3] - 2026-11-07

### 🖥️ Desktop Archiver Workspace & Dialog UX (Phase 3)
- **Titlebar HUD Click Interception (V-UX-04 / WorkspaceHeaderProgressHUD.tsx)**:
  - Attached `onMouseDown={(e) => e.stopPropagation()}` to Dynamic Island HUD cancel buttons, chevron toggle, and dropdown container, preventing Windows DWM window dragging from intercepting clicks.
- **Parent Directory (..) Double Navigation Fix (V-UX-07 / ArchiveTreeInspector.tsx)**:
  - Removed conflicting onClick handlers from all parent directory navigation rows, strictly adhering to Windows Explorer double-click conventions and preventing multi-tier leaps.
- **Disk VirtualScrollList ScrollTop Reset (V-UX-08 / ArchiveTreeInspector.tsx)**:
  - Attached dynamic path keys (disk-virtual-pane1-... and disk-virtual-pane2-...) to VirtualScrollList instances, resetting scrollTop to 0 upon folder changes and eliminating blank view states.
- **WorkspaceExtractModal Keyboard Navigation Hardening (V-UX-09 / WorkspaceExtractModal.tsx)**:
  - Refined Enter key handling so focused buttons (Browse, Cancel) retain their native click activation without triggering extraction.
  - Implemented cyclic Tab focus trapping and auto-focusing on destination or password fields.
- **Context Menu Visual Hierarchy & Deletion Hazard Resolution (V-UX-02 / WorkspaceContextMenu.tsx)**:
  - Visually distinguished non-destructive queue unstaging (⊖ Remove from Staged in neutral slate/amber) and non-destructive folder/file exclusion (⊘ Exclude Folder in neutral slate/cyan) from destructive physical disk deletion (🗑️ Delete to Recycle Bin with dedicated divider and crimson danger styling).
- **Multi-Archive CLI / Context Menu Mode Detection (V-UX-01 / App.tsx)**:
  - Corrected startup argument handling using cmd.paths.filter(isArchiveFile) so multi-archive selection launches cleanly into Extract mode for the selected archives.
- **Tauri Drag Region Cleanup & Modal Dismissal Standardization (TAURI-01 & UX-02)**:
  - Removed invalid boolean data-tauri-drag-region="false" attributes across Titlebar, WorkspaceHeader, HUD, and context menus.
  - Introduced useModalDismiss hook with Escape key and backdrop dismissal across PasswordModal, ResultModals, and WorkspaceExtractModal.

### 🔄 Drag-and-Drop Lifecycle & Live Telemetry Polish (Phase 4)
- **Persistent Drag Highlight Ghost Elimination (V-UX-03 / useWorkspaceDragDrop.ts)**:
  - Added `onDragEnd: () => setDragOverTargetKey(null)` to both archive and disk item drag properties, immediately clearing hover highlight ghosts upon drag abort or Escape dismissal.
- **Root Directory Split Drop Failure & Pane 2 Drop Intent Conflict (V-UX-05 / WorkspaceDualPaneView.tsx)**:
  - Enabled `targetFolder === ""` to be accepted as the archive root during split-view drops.
  - Routed file drops directly onto active Pane 2 to staging (onDropDiskItemToStage) rather than navigating Pane 2 away.
- **Splitter Ratio Pixel Clamping on Narrow Windows (WS-12 / WorkspaceDualPaneView.tsx)**:
  - Enforced a minimum 200px width per pane during split resizing, preventing panels from squishing into unreadable widths.
- **Win32 Drive Root Navigation Auto-Formatting (WS-10 / WorkspaceNavBar.tsx)**:
  - Automatically appends a trailing backslash when typing single drive letters (e.g. C: becomes C:\), preventing Win32 drive relative path bugs.
- **Cancellable Recursive Directory Copying (V-PERF-06 / drives.rs)**:
  - Injected cooperative cancellation checks (is_compression_cancelled() and is_extraction_cancelled()) inside copy_dir_recursive and copy_path_sync, allowing users to interrupt multi-gigabyte transfers cleanly.
- **Throughput & ETA Calculation Stabilization (V-UX-06 / WorkspaceHeaderProgressHUD.tsx)**:
  - Keyed operation timer resets to total bytes rather than per-file label changes, eliminating speed spikes and continuous ETA resets.

## [v2.3.2] - 2026-11-05

### 🛡️ Security, Stream Integrity & Path Sanitization Hardening (Phase 1 & Phase 2)
- **Pre-Extraction Zip-Slip Defense (`V-SEC-01` / `extractor.rs`)**:
  - `extract_with_system_tar` now runs `tar.exe -tf` to inspect the archive Table of Contents before extraction begins, validating all paths and aborting immediately if directory traversal (`..`, root paths, drive prefixes) is detected before writing to disk.
- **Deep Path Sanitization Hardening (`V-SEC-08` / `inspector/mod.rs`)**:
  - `sanitize_archive_path` now blocks Windows path normalization bypasses (trailing periods and spaces, e.g. `file.exe .`), root/UNC paths, Windows drive prefixes, NTFS Alternate Data Streams (`:`), null bytes, and all DOS reserved devices (including `CONIN$`, `CONOUT$`, `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`).
- **Strict Container Stream Alignment & Premature EOF Trapping (`V-SEC-02` / `extractor.rs`)**:
  - Implemented `drain_bay_entry_bytes`, verifying exact consumption of `entry.uncompressed_size` bytes when draining excluded entries during streaming extraction. Catches premature EOF and stream read errors immediately, preventing container header desynchronization.
- **Bounded Trailer TOC Memory Allocation (`V-SEC-03` / `inspector/bay.rs`)**:
  - Enforced a strict 64 MB upper-bound memory cap (`MAX_TOC_SIZE`) and file-size bounds checks on `BAYTOC\x02\x00` and `BAYETOC\x03` trailers, safely rejecting corrupted or malicious archives before heap allocation can trigger out-of-memory crashes.
- **Root Container Archive Deletion Safeguard (`V-DATA-01` / `ArchiveTreeInspector.tsx`)**:
  - Guarded `handleDeleteItem` against deleting root container nodes or matching the backing archive file on disk, safely limiting deletion to clearing in-memory staged items.

### ⚙️ Concurrency, OS Priority & Performance Hardening
- **Thread-Local Priority Restoration (`V-CONC-01` / `priority.rs`)**:
  - Refactored `ScopedPriorityGuard` to store calling thread's native Win32 thread priority (`GetThreadPriority`) and restore it on `Drop` specifically for that thread. Completely removed the global atomic refcount coupling, preventing Tokio worker threads from having their priority permanently depressed.
- **Asynchronous System Specs Discovery (`V-PERF-04` / `lib.rs`)**:
  - Converted `get_system_specs` to an `async fn` offloading hardware metric queries to `tauri::async_runtime::spawn_blocking`, ensuring the main Tauri IPC thread never blocks during app launch or settings queries.
- **Gamer Preset Cap & Clean `.bayg` Unification (`V-PERF-05` / `compressor/`)**:
  - Cleanly removed deprecated `.bay_gamer` across active production code and documentation, standardizing on **`.bayg`** as the official Gamer Preset format name.
  - Normalized format matching across `bayg` and `gamer`, strictly enforcing Zstandard Level 3 caps across 75% and 100% hardware allocations (Rule 4 compliance).
- **Desktop Extraction Modal Integration (`V-INT-01` / `App.tsx` & `ExtractPage.tsx`)**:
  - Wired custom destination path, subfolder toggle, auto-open directory, and decryption password from `WorkspaceExtractModal` directly into backend extraction commands.
- **Embedded Unit Test Suite**:
  - 27 unit tests embedded natively across core modules (`cargo test --lib` passes in 0.00s with 0 failures).

## [v2.3.1] - 2026-10-26

### ⚡ Unified Desktop Archiver Workspace & Modal Extraction
- **Primary Desktop Archiver Action Toolbar (`WorkspaceHeader.tsx`)**:
  - Replaced the previous Compress / Extract split-mode segmented buttons with an intuitive desktop archiver toolbar:
    - **`📂 Open`** (`Ctrl+O`): Instantly open and inspect any archive file (`.bay`, `.bayg`, `.zip`, `.7z`, `.tar`, `.rar`, etc.) from your computer.
    - **`⚡ Extract`** (`Ctrl+E`): Opens the dedicated desktop **Extraction Path and Options** popup dialog.
    - **`📦 Add`**: Toggles the advanced compression options sidebar to configure algorithms (Zstd, Brotli, LZ4, LZMA2), compression level, password encryption, volume splitting, and staging.
    - **`🧪 Test`**: Executes archive integrity test with live Dynamic Island HUD feedback.
    - **Active Archive Badge**: Monospace pill displaying the active archive name directly in the workspace titlebar.
- **Dedicated Desktop Extraction Modal (`WorkspaceExtractModal.tsx`)**:
  - Replaced the old full-screen/workspace extraction mode with a desktop archiver popup dialog:
    - Target extraction destination input with native folder picker (`@tauri-apps/plugin-dialog`).
    - Extraction checkboxes:
      - `Extract to subfolder: <archive_name>\` (enabled by default).
      - `Open destination folder after extraction` (auto-launches Windows File Explorer).
    - Built-in decryption password field with visibility toggle and auto-focus when archive encryption is detected.
    - Archive summary badge displaying total file count and uncompressed byte size.
    - Keyboard ergonomics: `Enter` to extract immediately, `Escape` to dismiss the modal without closing the workspace.

### 🧹 Clean Architecture & Legacy Code Removal
- **Streamlined Archive Inspector (`ArchiveTreeInspector.tsx`)**:
  - Removed obsolete tab redirects (`onSwitchMode("extract")`), allowing archive files opened from disk to load directly inside the workspace without switching views.
  - Removed artificial `mode === "extract"` checks on the `"No Archive Loaded"` empty state and left-panel `"Open Archive File..."` button.
  - Simplified drag-and-drop staging checks (`if (onStageItem)`) and removed artificial `mode === "compress"` restrictions across context menus and file trees.
  - Removed obsolete workspace extraction sidebars (`WorkspaceExtractSidebar.tsx`) and drawers (`WorkspaceExtractQueue.tsx`).
  - Preserved normal standalone pages (`CompressPage.tsx` and `ExtractPage.tsx`) for classic quick operations.

## [v2.3.0] - 2026-10-25

### ⫴ Instant Dual-Pane Split View & Right-Click Split
- **Side-by-Side Exploration**:
  - Added instant dual-pane split view (`WorkspaceDualPaneView.tsx`) toggled via `⫴` or `Ctrl+\`.
  - Enables viewing local drive contents and staged/archive files simultaneously side-by-side with independent directory navigation.
  - Interactive resizable center split divider with minimum clamped bounds and drag handle.
- **Drag-to-Split & Right-Click Context Menu Integration**:
  - Fixed drag-to-split behavior: dragging any local folder to the right side now immediately opens that specific directory in Pane 2 (instead of erroneously staging it or defaulting to `C:\`).
  - Added dedicated **"⫴ Open in Split View"** action directly inside the right-click context menu for all directory targets.
  - Eliminated redundant `[📁 Disk] [📦 Staged]` toggle buttons from Pane 1 and Pane 2 headers, replacing them with clean breadcrumbs, item counters, ergonomic `Up ⮥` buttons, and a clean `✕ Close` button.
- **Filesystem Hardening & Cross-Volume Moves**:
  - Added cross-volume move fallback in `drives.rs` to handle moves across different Windows drive letters (`C:\` to `D:\`) without error 17 (`EXDEV`).
  - Enforced Windows filename character validation disallowing illegal characters (`< > : " / \ | ? *`) and reserved device names.

### 🖐️ Interactive Drag-to-Move & Cross-Pane Staging
- **Unified Drag & Drop Architecture**:
  - Extracted comprehensive drag-to-move and staging logic into dedicated hook (`useWorkspaceDragDrop.ts`).
  - Supports picking files and folders from table rows or tree nodes and dropping them into target directory rows, tree hierarchy folders, or breadcrumb pills.
  - Full staging feedback with drop targets, hover states, and conflict safeguards.

### 🎛️ Workspace Panel Layout Controls
- **Unified Top-Bar Layout Toggles (`WorkspaceHeader.tsx`)**:
  - Replaced ad-hoc text buttons with a dedicated 5-icon layout cluster:
    - `◧` **Left Explorer / Folder Tree** (`Ctrl+B`): Toggles tree visibility, allowing full-width file view expansion.
    - `⫴` **Dual-Pane Split** (`Ctrl+\`): Toggles instant split view.
    - `⬒` **Bottom Queue Drawer** (`Ctrl+J`): Toggles the staging/extract queue drawer with badge counter.
    - `◨` **Right Options Sidebar**: Toggles the compression/extraction options panel.
    - `🗗` **Compact View** (`Esc`): Restores the compact card view.
- **De-Cluttered Modern Interface**:
  - Cleaned up noisy `[ ✕ Exclude Files ]` and `[ ✕ Exit Workspace ]` buttons across both full-screen Workspace and compact inspector views.
  - Neutralized comments for clean open-source readiness with zero external brand references.

### 🗑️ Native Disk Item Management & Recycle Bin Deletion
- **Windows Recycle Bin Integration**:
  - Fixed right-click context menu "Delete" action on physical disk files and directories to properly invoke Tauri's `delete_to_recycle_bin`.
  - Added native `Delete` / `Del` keyboard shortcut support when focused on disk items.
  - Real-time UI synchronization: deleted items are immediately purged from active table panes (Pane 1 & Pane 2) and unstaged from the compression queue if present.

### 🎯 Disk Row Selection & Keyboard Shortcuts in Dual Panes
- **Interactive Disk Row Selection**:
  - Added click selection with active accent highlight (`bg-[#25394A] border-l-2 border-[#60CDFF]`) across all local disk folders and files in primary and secondary panes.
  - Keyboard interaction parity: enabled `Space` (instant safe preview), `Ctrl+C` (copy absolute path), `F2` (rename), and `Del` (recycle bin deletion).

### 👁️ Safe Direct File Preview for Disk & Staged Files
- **Accurate Path Routing**:
  - Fixed path resolution and routing in `handlePreviewEntry` within `App.tsx`: disk and staged files bypass the archive extraction engine and route directly to `preview_staged_file`.
  - Cleans up synthetic prefixes, preventing "archive not found" errors and displaying accurate file paths.

### 🔄 Context Menu Staging & Exclusion Synchronization
- **Accurate Context Target Detection**:
  - Context menus now accurately differentiate between physical disk items, root staged items, and archive entries.
  - Root staged items display `Remove from Archive` directly in the context menu for immediate unstaging.
  - Left tree staging unstage action now recursively cleans up both the target folder and all nested descendant files.
  - Viewport margin clamping (`Math.max(8, ...)`) prevents context menus from clipping off the top or bottom of the screen.

### 🗂️ Unified Workspace Architecture & Deprecation of Flat View
- **Streamlined Dual-Pane Focus**:
  - Streamlined workspace exploration around the hierarchical tree and dual-pane split view, cleanly removing the redundant and buggy Flat View toggle.
  - Preserves typed address bar navigation inputs without unexpected parent folder truncations.
  - Smooth navigation fallback when encountering inaccessible or locked system directories.

---

## [v2.2.3] - 2026-10-24

### 🏝️ Dynamic Island 1-Click Cancellation HUD
- **Instant Cancel Button on Dynamic Island Pill**:
  - Added a direct 1-click `✕` cancel button directly on the Dynamic Island HUD pill with dedicated hover effects and tooltip.
  - Enforced `data-tauri-drag-region="false"` and `e.stopPropagation()` / `e.preventDefault()` to stop Windows DWM from intercepting clicks as window drag operations.
  - Added optimistic UI state (`Cancelling...`) with responsive visual feedback during compression and extraction aborts.

### 🧩 Decomposed Modular Workspace Architecture
- **Monolithic Refactor & Component Isolation**:
  - Refactored and decomposed the 2500+ line `ArchiveTreeInspector.tsx` into modular components under `src/components/workspace/`:
    - **`WorkspaceContextMenu.tsx`**: Viewport-clamped floating context menu (`z-[130]`) with instant dismiss, supporting disk rows (`+ Stage`, `👁️ Preview`, `✏️ Rename`, `📋 Copy`, `🗑️ Delete to Disk`), archive entries (`✕ Exclude / ↺ Include`, `Preview`, `Rename`, `Copy`), and table background (`📁+ New Folder`, `📥 Paste`).
    - **`WorkspaceExtractQueue.tsx`**: Dedicated resizable bottom drawer (80px–450px) for Extraction Workspace displaying archive payload summary, item counters, and an interactive Excluded Files list with 1-click `↺ Include` buttons.
    - **`WorkspaceExtractSidebar.tsx`**: Standalone Windows 11 Fluent extraction options sidebar (Where to Extract, Extraction Options, Live Summary & Primary Action), mirroring `WorkspaceAdvanceSidebar` 1:1.
    - **`WorkspaceNavBar.tsx`**: Unified navigation bar supporting both filesystem and archive folder browsing with clickable breadcrumbs, editable address bar, history stack, and live search.

### 📁 Ergonomic HCI In-Table Navigation (`.. [Parent Directory]`)
- **Direct Row 0 Navigation**:
  - Implemented the desktop archiver standard (WinRAR, 7-Zip, Total Commander): added an ergonomic `📁 .. [Parent Directory]` row at index 0 of the table when browsing subdirectories.
  - Double-clicking or clicking `..` navigates up one directory directly under the cursor without requiring travel to top-level toolbar buttons.

### ⚡ Native Quick Access & Drive Navigation (Left Panel)
- **Eliminated Duplicate Folder Hierarchy**:
  - Fixed duplicate folder list between left and middle panes by replacing the left subfolder list with native **Quick Access Shortcuts** (Desktop 🖥️, Downloads 📥, Documents 📄, Pictures 🖼️, Music 🎵, Videos 🎬) powered by the new `get_quick_access` Tauri command.
  - System drives with live capacity bars remain accessible, while the left pane in Extract mode is dedicated to the hierarchical archive directory tree.

---

## [v2.2.2] - 2026-10-23

### 🏝️ Dynamic Island Top-Header Progress HUD
- **Centered Header Progress Pill**:
  - Replaced the bulky bottom-right floating toast that previously obscured the file list and status bar with a centered, non-obtrusive titlebar progress pill.
  - Features an energetic pulsing emerald live status indicator, operation title (`Compressing Files...`), mini shimmer gradient progress bar, and bold percentage badge (`2%`).
- **Interactive Slide-Down Telemetry Card**:
  - Clicking the header pill drops down a frosted Fluent card (`bg-[#161616]/95 backdrop-blur-xl border border-emerald-500/40 shadow-2xl`):
    - **Real-time Estimated Time (ETA)**: Live calculation of time remaining (e.g. `~14s left`, `< 5s left`, `Finishing up...`).
    - **Live Throughput Speed**: Instantaneous transfer rate calculation (e.g. `42.5 MB/s`).
    - **Active File Telemetry**: Monospace path display with tooltip and formatted byte counts (`35.95 MB / 738.76 MB`).
    - **Safe Cancellation**: Danger-styled cancel button with busy feedback (`Cancelling operation...`).
    - **Effortless Collapsing**: Auto-collapses on chevron toggle, collapse button, or clicking outside.
- **Ambient Peripheral Progress**:
  - Retained the running emerald glowing progress line across the workspace base to maintain ambient visibility when the header dropdown is closed.

### 🔄 Interactive Workspace Titlebar Mode Switcher
- **Segmented Workspace Mode Pill**:
  - Replaced the static mode text badge in the workspace titlebar (`BAYZIP WORKSPACE`) with an interactive segmented controller:
    `[ 📦 Compress ]` ↔ `[ 📂 Extract ]`.
  - Enables seamless, instantaneous switching between Compression Workspace and Extraction Workspace without leaving full-screen mode.

### 🗑️ Normal Mode Staging, Unstaging & Context Menu Security
- **1-Click On-Row Staged Item Removal**:
  - Added dedicated `[ ✕ Remove ]` buttons directly on rows for top-level staged folders and files in normal Tree View.
  - Added dedicated `[ ✕ Remove ]` button on the active archive banner card in `ExtractPage`.
- **Context Menu (`Right-Click`) Safety Overhaul**:
  - Context menu on staged files prioritizes non-destructive **`✕ Remove from Archive [Unstage]`**.
  - Context menu on archive entries prioritizes **`✕ Exclude from Extraction`** / **`↺ Include in Extraction`**.
  - Strictly restricted destructive *"Delete (Recycle Bin)"* to local physical disk browsing in filesystem mode, safeguarding original source files from accidental deletion.

### 📊 Metric Accuracy & Navigation Enhancements
- **Accurate Exclusion Counters**: Fixed calculation of included vs. excluded files (`N of Total included (X excluded)`), correctly tracking recursive folder payloads.
- **Automatic Orphan Pruning**: Added auto-pruning of excluded path sets in `App.tsx` when staging selections change.
- **Workspace Navigation Bar & Staging Drawer**: Integrated `WorkspaceNavBar` (history back/forward stack, Up folder, refresh, breadcrumbs, editable path field with `Enter` navigation) and `WorkspaceStagedQueue` resizable staging drawer (`↕`).

---

## [v2.2.1] - 2026-10-22

### 🖥️ Full-Screen Workspace Takeover & Competitor-Grade UX
- **Border-Free Full Canvas Mode**:
  - Replaced the constrained modal overlay with a borderless, immersive full-screen takeover workspace (`fixed inset-0 w-screen h-screen bg-[#141414] z-50`).
  - Integrated custom Windows-style caption/titlebar controls: Minimize, Maximize, and Exit Workspace (with `Esc` key shortcut) along with seamless `data-tauri-drag-region` multi-monitor dragging.
- **Dedicated "Workspace" Access Buttons**:
  - Added dedicated glowing **Workspace** buttons across primary views:
    - Top navigation bar next to Compress/Extract mode tabs.
    - Prominent trigger buttons in the central drag-and-drop zone of both Compress and Extract pages.
    - Quick-access buttons in the staged files header and loaded archive header.

### 🧭 System Drives Selector & Multi-Drive Disk Hierarchy Navigator
- **Visual Storage Capacity Gauges (Left Panel)**:
  - Added interactive system drive cards displaying drive letter, volume label, and storage usage gauges with percentage bars and available/total GB.
  - Dual source mode: easily toggle between **Staged / Archive Tree** and **Drive Files**.
- **Asynchronous Disk Directory Browsing**:
  - Implemented async `browse_directory` Tauri command (`tauri::async_runtime::spawn_blocking`) for lightning-fast directory traversal without UI stutter.
  - Interactive breadcrumbs with parent directory jumping and root resets.
  - Double-click folder exploration and zero-disk safe file previewing.
  - **1-Click Direct Staging (`+ Stage`)**: Queue individual files or entire folders from any local drive directly into the archive without leaving the workspace.

### ⚙️ In-Workspace Advanced Options Sidebar (Right Panel)
- **Direct Workspace Configuration**:
  - In-workspace access to the Advance Section drawer without leaving the full-screen view.
  - Real-time archive name editing and destination directory targeting.
  - Format selection (`.bay`, `.bay_gamer`, `.zip`, `.tar.zst`, `.7z`), hardware core allocation slider, solid compression settings, encryption passwords, and smart media bypass.

### 🟢 Dual Real-Time Progress & Loading Indicators
- **High-Visibility Emerald Bottom Progress Line**:
  - Running glowing emerald line across the bottom viewport edge (`h-1.5 bg-emerald-500 shadow-[0_0_12px_rgba(16,185,129,0.7)]`) with smooth `@keyframes shimmerSweep` animation during active operations.
- **Floating Pop-up / Pop-down Progress HUD**:
  - Docked toast card in the bottom-right corner (`fixed bottom-6 right-6`) providing detailed operation feedback:
    - Active operation status, current filename, processed/total byte telemetry, and smooth percentage progress.
    - One-click immediate operation cancellation.

### 🛠️ Stability & Architecture Improvements
- **Resolved Duplicate Tree Inspector Mounting**: Fixed an issue in `CompressPage.tsx` where multiple overlapping instances of the inspector mounted simultaneously when toggling workspace mode.
- **Safe Fallback State Handling**: Added graceful fallback initialization in `ExtractPage.tsx` allowing workspace mode exploration even when no archive has been loaded yet.
- **Cleaned Unused Variables**: Removed dead state variables (`isDriveMenuOpen`) and verified 100% clean builds across TypeScript and Rust.

## [v2.2.0] - 2026-10-22

### 🗂️ Advanced In-App File Management Suite & Live Hierarchy Engine
- **Full Right-Click Context Menu (`contextMenu`)**:
  - Context-aware right-click menu accessible on individual files, folders, or empty workspace canvas across both Tree View and Flat Table.
  - **New Folder (`Ctrl+Shift+N`)**: Instantly creates new directories both in-memory and on disk with dedicated toolbar and tree header shortcuts.
  - **Rename (`F2`)**: In-place inline renaming with instant keybindings (`Enter` to commit, `Escape` to cancel).
  - **Safe Windows Recycle Bin Deletion (`Del`)**:
    - Replaced destructive unrecoverable deletes with native Win32 `SHFileOperationW` (`FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT`).
    - Files and multi-gigabyte directories are transferred to the Windows Recycle Bin in $0\text{ ms}$ via Master File Table (MFT) pointer shifts without physical flash wear or UI lockups, keeping user files 100% recoverable.
  - **Copy & Paste (`Ctrl+C` / `Ctrl+V`)**: Multi-item clipboard copying with automatic duplicate name collision resolution (`- Copy`, `- Copy (2)`, etc.).
  - **Restricted Safe Preview (`Space` / Double Click)**: Direct zero-disk in-memory preview of documents, images, and source code.
- **Inline Naming & Dynamic Left-Bar Hierarchy Synchronization**:
  - Added inline text inputs for immediate naming and renaming of folders and files.
  - Automatic focus and auto-selection of text for rapid keyboard workflows.
  - The left-hand folder tree hierarchy instantly updates and expands in real-time when folders are created, renamed, or deleted, keeping navigation and breadcrumbs completely synchronized.

### 🧭 Non-Cluttering System Drive Switcher
- **Discreet Drive Switcher Badge (`[ C: ▾ ]`)**:
  - Embedded a sleek, modern badge into the Folder Tree hierarchy header rather than crowding the UI with static drive buttons.
- **Dynamic Multi-Partition & Removable Media Detection (`sysinfo::Disks`)**:
  - Automatically queries physical storage mounts, volume names, disk space usage (`Available / Total GB`), and removable USB flash drive flags.
  - Clicking the badge opens an animated Fluent glassmorphic flyout displaying all active drives with live storage capacity bars.
  - 1-click instant mounting: selecting a drive immediately routes the folder tree to the chosen drive root (`C:\`, `D:\`, `E:\`, USBs).
  - Closes automatically upon selection, outside click, or `Escape`, preserving a clean, minimalist workspace.

### 🪟 Draggable Titlebar & Native Window Caption Controls
- **Seamless Multi-Monitor Window Dragging**:
  - Added native `data-tauri-drag-region` to the header of the full-screen expanded dual-pane workspace modal.
  - Integrated native Windows caption controls: Minimize, Maximize/Restore, and Close buttons directly into the titlebar.
  - Upgraded Tauri window bindings to static `getCurrentWindow()` references, eliminating dynamic import overhead and Vite chunk bundle warnings.

### 🎨 Energetic Emerald Green Loading Indicators & Animated "DROP" Watermark
- **Emerald Accent Progress Bars**:
  - Redesigned continuous progress indicators across compression and extraction views to use an energetic emerald green gradient (`from-[#059669] via-[#10B981] to-[#34D399]`) with an ambient emerald glow effect, aligning with Bayzip's refined dark theme.
- **Interactive Pulsing "DROP" Watermark**:
  - Added an animated, semi-transparent pulsing **`DROP`** watermark (`z-[100]`) that activates when dragging files or directories over the Bayzip workspace, providing immediate visual confirmation that the window is ready to accept dropped assets.

### ⚡ Dynamic Real-Time Throughput Engine & Second-by-Second Countdown ETA
- **Eliminated Static ETA Freezes (`useDynamicThroughput`)**:
  - Replaced legacy cumulative lifetime averaging (`bytes / total_elapsed`) with a high-precision sliding-window (2.0s) throughput sampler and an active 200ms tick timer.
  - Solved the issue where an operation stalled on disk write flushes or multi-core worker joins caused the ETA to freeze statically (e.g., `ETA: 3s`).
  - **Second-by-Second Real-Time Countdown**: Actively counts down remaining time (`3s` ➔ `2s` ➔ `1s` ➔ `Finishing...`) second-by-second, maintaining fluid feedback even between asynchronous Tauri IPC events.
  - **Graceful Speed Decay**: Exponential Moving Average (EMA) smoothly decays throughput during physical storage flushes rather than sticking to obsolete read-burst figures.
- **Backend Finalizing Progress Stream (`bay.rs`)**:
  - Added dedicated final-chunk progress emissions and an explicit `"Finalizing archive..."` status event when worker threads are flushing frames and committing the Table of Contents trailer.

## [v2.1.1] - 2026-10-21

### 🪟 Windows 11 Shell Extension Multi-Selection Archiving
- **Unified Multi-Item Context Menu Archiving**:
  - Right-clicking multiple selected files or folders in Windows File Explorer now creates a single comprehensive archive (`ParentFolder.bay`, `.bayg`, `.zip`, `.7z`, or `.tar`) containing all selected items, instead of launching separate standalone archives.
  - Implemented `headless_quick_compress_targets` across `.bay`, `.zip`, `.tar`, and `.7z` engines.
- **Silent Headless Execution (`CREATE_NO_WINDOW`)**:
  - Added the Windows process creation flag `CREATE_NO_WINDOW` (`0x08000000`) before spawning subprocesses from `bayzip_shell.dll`, completely eliminating command prompt window flashes during right-click actions.

### ⚡ Real-Time Throughput Speed, Dynamic ETA & Drag-and-Drop Polish
- **Live Transfer Speed & Dynamic ETA**:
  - Active compression cards and extraction footers now calculate real-time transfer speed (`MB/s`) and dynamic completion time remaining (`ETA: Xs`) using elapsed execution timers and processed byte counts.
- **Interactive Drag-and-Drop Fluent Overlay**:
  - Main window listens to dragging events across child elements, displaying a Fluent dark backdrop with an illuminated cyan border glow and drag drop badge.
- **Subfolder Path Normalization**:
  - Unified subfolder path separators across Windows and cross-platform views using standard forward-slash (`/`) directory hierarchy formatting.

### 🛡️ Calibrated Non-Blocking I/O Streaming & Clean Shell Uninstallation
- **Hardware-Calibrated Storage I/O Boundaries**:
  - Calibrated physical read slices (256 KB for USB, 512 KB for SATA, 1 MB for NVMe) and destination write caps (512 KB for USB, 1 MB for SATA, 2 MB for NVMe).
  - Eliminates the Windows 11 NVMe dynamic SLC controller queue stall and USB Quick-Removal write locks while maintaining a responsive 20 FPS progress bar and instant cancellation.
- **Clean Registry & Shell Association Management**:
  - Consolidated shell association notification into `notify_shell_associations_changed()`, removed duplicate `SHChangeNotify` definitions, removed obsolete `ie4uinit.exe` calls, and added clean `Software\RegisteredApplications` removal during uninstallation.

### 🏷️ UI Precision & Documentation Alignment
- **Explicit `(ZIP)` Indicator for Smart Media Bypass**:
  - Added a cyan `(ZIP)` badge to both `SettingsWindow` and `CompressPage` and refined the descriptions to clearly communicate that zero-CPU Fast Store mode directly applies to `.zip` archives, whereas continuous solid formats (`.bay` and `.7z`) apply dictionary protection grouping to keep sliding LZ77/Zstd dictionaries clean.
- **Clarified Standard Unencrypted RAR Support**:
  - Updated documentation and the supported formats matrix to explicitly clarify that RAR extraction in v2.1.1 supports standard, unencrypted archives via Windows `libarchive`, while password-protected AES-128/256 RAR5 and multi-volume spanning are tracked for v2.2.

## [v2.1.0] - 2026-10-21

### 📦 Windows Installer & Uninstaller Branding (NSIS)
- **Installer & Uninstaller Wizard Branding**:
  - Added official Bayzip branding bitmaps: 150x57 `installer-header.bmp` for wizard page headers and 164x314 `installer-sidebar.bmp` for Welcome and Finish pages.
  - Added `uninstallerIcon` (`icons/icon.ico`) so `uninstall.exe` and the Windows "Installed Apps" / "Apps & Features" uninstall page in Windows Settings display the authentic Bayzip icon.

### 🪟 Native Windows Shell Icon Extraction & WinRAR/Bandizip-Style Architecture
- **Direct Windows Shell Icon Extraction (`SHGetFileInfoW`)**:
  - Implemented authentic system icon extraction via Win32 Shell API (`SHGetFileInfoW`), adopting the proven native architecture used by WinRAR and Bandizip.
  - Automatically queries the Windows shell association cache for each file extension, extracting the user's authentic desktop application icons for **Microsoft Word (`.docx`, `.doc`)**, **Adobe Acrobat / PDF (`.pdf`)**, **Cisco Packet Tracer (`.pkt`, `.pka`)**, **Microsoft Excel (`.xlsx`)**, **PowerPoint (`.pptx`)**, and any registered system file type.
  - Converts native 32-bit Windows `HICON` handles to 32-bit RGBA pixel buffers and encodes them into memory-efficient, transparent base64 PNG data URLs using `flate2`.
  - **Thread-Safe In-Memory Icon Caching (`icon.rs` & `FileFormatIcon.tsx`)**:
    - Dual-layer caching: in-memory `ICON_CACHE` in Rust and `WINDOWS_ICON_CACHE` in React. Each unique extension is extracted from Windows exactly once in microseconds.
    - Added batch prefetching (`get_file_icons`) that preloads all unique file extensions across an entire archive or staged directory tree in a single asynchronous IPC call.
  - **Resilient Fallback Design**:
    - Gracefully falls back to high-fidelity, Fluent dark-mode vector SVG icons during asynchronous loading or when running on non-Windows environments.

### 🛡️ Strict Archive Detection & Prevention of False Extractions
- **Office Document & Folder Filtering (`format.rs`)**:
  - Excluded directories (`path.is_dir()`) and known non-archive file formats (`.docx`, `.xlsx`, `.pptx`, `.odt`, `.ods`, `.odp`, `.pdf`, `.pkt`, `.txt`, `.png`, `.jpg`, `.mp4`, `.exe`, etc.) from being falsely detected as `.zip` archives by raw `PK\x03\x04` magic bytes.
  - Office files like `imp points.docx` or `payment_record.xlsx` are no longer misidentified as ZIP archives, preventing unwanted extraction and exposure of internal XML directory structures (`_rels/`, `word/`, etc.).
- **Folder Error Guard in Inspector (`inspector/mod.rs`)**:
  - Added strict validation rejecting directory paths in `inspect_archive`, returning a clear error if a folder is passed for extraction.
- **Intelligent Windows 11 Context Menu Shell Routing (`bayzip_shell.dll`)**:
  - Updated context menu execution logic: right-clicking folders or non-archive files now executes `--gui-compress` to stage them for compression. Only genuine archive extensions (`.bay`, `.bayg`, `.zip`, `.7z`, `.rar`, `.tar`, `.gz`, `.xz`, `.zst`, `.iso`, `.cab`) invoke `--gui-extract`.
- **Mode-Preserving Drag-and-Drop & CLI Handling (`App.tsx`)**:
  - Dropping files or folders while on the **Compress** tab now always stages them for compression, eliminating unexpected tab switching to Extract mode.
  - Added safety redirection in `selectArchive`: passing non-archive files or folders automatically switches to Compress mode and stages the items.
  - CLI argument router guards `--gui-extract` against non-archive files and directories, preserving the user's intended workflow.

### 🗂️ Staged Items vs Archive Explorer UI Distinction
- **Dynamic Workspace Header & Icons (`ArchiveTreeInspector.tsx` & `CompressPage.tsx`)**:
  - Added `inspectorTitle` property distinguishing between staged compression items (`Staged Items & Folders` with 📁 folder root icon) and archive extraction (`Archive Explorer` with 📦 archive box root icon).
  - Search input placeholder dynamically adjusts between filtering files in the current folder vs searching archive contents.
  - Fixed cross-platform path segment parsing using `split(/[/\\]/).pop()` across tree and flat table views to resolve Windows backslash issues.

### ⚙️ Hardware Slider Alignment, Media Bypass Toggle & Draggable Split Panels
- **Mathematical Slider Tick Centering (`CompressPage.tsx` & `App.css`)**:
  - Fixed numeric tick alignment (`0`, `25`, `50`, `75`, `100`) beneath the Hardware Allocation slider using offset formula `calc(${val}% + ${(50 - val) * 0.16}px)` with `transform: translateX(-50%)`.
  - Added clean Windows 11 circular thumb styling (`16px`) with cyan border hover and glow effects.
- **Global Smart Media Bypass Setting (`SettingsWindow.tsx`, `App.tsx`, `utils.rs`)**:
  - Added persistent global setting and interactive toggle button in the Settings window allowing users to enable or disable automatic media pass-through.
  - Cross-window event synchronization updates compression engines and frontend state immediately.
- **Draggable Split Panels in Workspace Mode (`ArchiveTreeInspector.tsx`)**:
  - Implemented interactive column resizers (`cursor-col-resize`) between Folder Hierarchy, File Explorer, and Action Sidebar for seamless navigation on widescreen displays.
- **Individual File Un-Exclusion Logic (`App.tsx`)**:
  - Fixed path exclusion toggle so un-excluding an individual file inside an excluded folder removes ancestor exclusion while preserving sibling exclusions.

### ⚡ Storage-Aware Streaming I/O Calibration & Zero-Stall Buffer Architecture
- **Eliminated Monolithic 128 MB Write Stall (`hardware.rs`, `mod.rs`, `bay.rs`)**:
  - Resolved critical queue depth stalls where 128 MB synchronous `WriteFile` calls on NVMe SSDs caused latency to spike to 221 ms and locked compression threads for 6–10 seconds.
  - Decoupled physical disk streaming buffer sizes from CPU worker thread counts: CPU slider strictly dictates worker thread count and compression level, while I/O buffer sizing is governed by physical storage bus type.
- **Physical Drive Bus Calibration (`get_optimal_read_slice` & `get_optimal_write_buffer`)**:
  - **External USB / Flash Thumb Drives**: Read slice calibrated to **256 KB**, write buffer capped to **512 KB** (guarantees fluid progress and eliminates multi-second hangs under Windows Quick-Removal policy).
  - **Internal SATA SSD / HDD**: Read slice calibrated to **512 KB**, write buffer capped to **1 MB** (prevents head thrashing on mechanical drives).
  - **Internal PCIe NVMe SSD**: Read slice calibrated to **1 MB**, write buffer capped to **2 MB** (saturates multi-gigabyte PCIe burst line rate while keeping disk response latency under 2 ms).
- **Universal Multi-Algorithm Calibration (`bay.rs`, `zip.rs`, `tar.rs`, `sevenz.rs`)**:
  - Standardized optimal streaming chunks across all supported container formats: Zstandard, LZ4, Brotli, ZIP (Deflate / Bzip2), TAR, and 7-Zip (LZMA2).
- **Optimized Stream Extraction Architecture (`extractor.rs`)**:
  - Standardized decompression transfer buffers to **256 KB** (aligned with `ZSTD_DStreamOutSize`, `flate2`, and CPU L2 cache boundaries).
  - Reduced archive input `BufReader` from 16 MB to **1 MB**.
  - Calibrated extracted file `BufWriter` from 16 MB down to **32 KB** for tiny files ($\le 64\text{ KB}$), **256 KB** for medium files ($\le 1\text{ MB}$), and **512 KB** for large files, completely preventing dirty-page write stalls during multi-gigabyte extractions.

---

## [v2.0.7] - 2026-10-21

### 🚀 Pro Workspace Mode, Quick Format Selector, Authentic v2.0.0 UI & Responsive Width
- **Quick Format Selector Dropdown (`CompressPage.tsx`)**:
  - Replaced the static format badge on the Archive Name card with an interactive format selector dropdown featuring a subtle chevron indicator (`▼`).
  - Allows 1-click switching between `.bay`, `.bayg (Gamer)`, `.zip`, `.7z`, and `.tar` directly from the main view (at 50% Balanced or any slider setting) without opening Advanced Options.
  - Automatically updates the compression algorithm, hardware engine badge, and thread matrix in real time upon selection.
- **Authentic v2.0.0 UI Aesthetic Restoration (`CompressPage.tsx`)**:
  - Restored always-visible Archive Name card featuring the glowing 3D Bayzip file box icon tile, "Saved in source folder" indicator, and input container.
  - Restored the original Hardware Allocation header layout: dynamic multi-core engine badge (`[ ZSTANDARD MULTI-CORE ]`, `[ ZIP ARCHIVE ENGINE ]`, `[ LZMA2 MULTI-CORE ]`) on the top right, active profile name (e.g. `Balanced`) in cyan directly under the title, clean numeric slider ticks (`0`, `25`, `50`, `75`, `100`), and `Target RAM Usage` subcard header.
- **Responsive Full-Screen Width Scaling (`App.tsx`)**:
  - Replaced the rigid `max-w-2xl` body container with responsive width scaling: `w-full max-w-2xl lg:max-w-4xl xl:max-w-5xl mx-auto space-y-6 transition-all duration-200`.
  - Maintains compact static width (~672px) at default window size (800x700), while expanding smoothly to ~1024px on maximized/full-screen widescreen displays (1080p, 1440p, 4K) so dropzones, file trees, and sliders utilize screen width comfortably without empty black voids.
- **Settings-Inspired In-App About Modal (`AboutModal.tsx`)**:
  - Restructured the About modal to adopt the exact dark glassmorphism design language of the Settings interface: dark gradient header bar (`bg-gradient-to-r from-[#262626] to-[#1F1F1F]`), glowing `BayzipLogo` emblem, engine architecture card with active green status pill, 2x2 feature grid, author creator panel, and `Esc` key dismissal.
- **Symmetric Pro Workspace Mode (`CompressPage.tsx` & `ExtractPage.tsx`)**:
  - Full-screen dual-pane workspace with a right-hand Action Sidebar (`w-80`) across both **Compress** and **Extract** workflows.
  - Allows full archive inspection, selective file/folder exclusion, folder tree exploration, and spacebar file previewing while keeping all configuration controls and execution buttons directly within reach.
  - **Compress Workspace Sidebar**: Archive naming with branding tile, format pills (`.bay`, `.bayg`, `.zip`, `.7z`, `.tar`), hardware/compression level slider, media pass-through toggle, encryption password, staging payload summary, and `[ ⚡ Compress Now ]`.
  - **Extract Workspace Sidebar**: Destination folder picker with Browse button, encryption password with Show/Hide toggle, subfolder extraction toggle, completion trigger toggle, selection summary, and `[ 📦 Extract Selected Files ]`.
  - Accessible via the `[ ⤢ Workspace Mode ]` header button in both views or the toolbar expand icon, with keyboard `Esc` shortcut.
- **Authentic Windows 11 Fluent Yellow Folders (`#F9D648`)**:
  - Restored classic Fluent Yellow folder icons across all directory listings, tree inspector nodes, breadcrumb elements, and flat table views, replacing generic blue/tan icons.
- **Resilient Brand Identity (`BayzipLogo.tsx`)**:
  - Created standalone, resilient `BayzipLogo` component with inline vector SVG fallback, ensuring branding assets never fail or hide on image load errors.
  - Integrated into Window Titlebar, About Modal, Compression header, Extraction card, and Workspace sidebars.
- **Compact Default State & Redundancy Cleanup**:
  - Restored the sleek, compact list view as default for staging files (< 190px), preserving the lightweight feel of v2.0.0.
  - Removed redundant spinning hardware specs refresh button from Hardware Allocation settings.

---

## [v2.0.6] - 2026-10-20

### 🛡️ Path Hardening, Directory Traversal Security & Heuristic Fixes
- **CurDir & Leading `./` Component Normalization (`inspector/mod.rs`)**:
  - Enhanced `sanitize_archive_path` to strip leading `./` and `/` without altering relative subfolder hierarchies.
  - Filtered out `Component::CurDir` (`.`) during path normalization so archives created with relative dot-prefixes (common in TAR and Linux tools) resolve cleanly into normal paths rather than breaking single-root directory detection.
  - Returns `None` for entries consisting solely of `.` or `./` to prevent invalid directory node creation.
- **Single-Root Directory Deduplication Hardening (`extractor.rs`)**:
  - Upgraded `.bay` extraction single-root prefix resolution to use `get_single_root_dir` instead of naive `entries[0]` indexing.
  - Added loose top-level file verification to ZIP and 7-Zip candidate root loops: archives containing loose top-level files are never misidentified as wrapped single-folder containers, preventing erroneous folder-stripping.
- **Folder Exclusion Trailing Slash Normalization (`compressor/utils.rs`)**:
  - Fixed a pattern-matching mismatch in `is_path_excluded` where folder exclusions with trailing slashes failed to match directory entries without trailing slashes.
  - Both path and exclusions are normalized via `trim_end_matches('/')` with prefix and exact matching.
- **Frontend Path Alignment (`App.tsx` & `ArchiveTreeInspector.tsx`)**:
  - Replaced naive slash-indexing in `hasSingleRoot` with a multi-entry scan matching the backend's `get_single_root_dir` logic.
  - Cleaned `normalizeArchiveEntryPath` and tree building loops to strip leading `./` and prevent ghost `.` folders.
- **Documentation Modernization (`README.md`)**:
  - Cleaned up legacy historical release notes from `README.md` to highlight only the latest version while directing users to `CHANGELOG.md` for complete release history.

---

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


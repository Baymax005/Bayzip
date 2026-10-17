import { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

import { Titlebar } from "./components/Titlebar";
import { AboutModal } from "./components/AboutModal";
import { SettingsModal } from "./components/SettingsModal";
import { PasswordModal } from "./components/PasswordModal";
import { ResultModals } from "./components/ResultModals";
import { ArchiveTreeInspector } from "./components/ArchiveTreeInspector";
import { FilePreviewModal } from "./components/FilePreviewModal";

import {
  SystemSpecs,
  CompressProgress,
  CompressResult,
  SelectedItemInfo,
  ExtractProgress,
  ExtractResult,
  ArchiveInfo,
  ArchiveEntryPreview,
  StartupCommand,
} from "./types";

const FORMAT_ALGORITHMS: Record<string, string[]> = {
  ".bay": ["Zstandard (Recommended)", "Brotli", "LZ4 (Fastest)"],
  ".bay_gamer": ["Zstd-Fast (Auto)"],
  ".7z": ["LZMA2", "LZMA", "BZip2"],
  ".zip": ["Deflate (Standard)", "None (Store)", "BZip2 (Requires 3rd-party App)"],
  ".tar": ["None (Store)"],
};

const ARCHIVE_EXTS = [
  ".bay",
  ".bay_gamer",
  ".zip",
  ".7z",
  ".tar",
  ".gz",
  ".tgz",
  ".rar",
  ".zst",
  ".bz2",
  ".xz",
];

function getArchiveStemName(filename: string): string {
  if (!filename) return "extracted";
  const lower = filename.toLowerCase();
  const compoundExts = [
    ".tar.gz",
    ".tar.zst",
    ".tar.bz2",
    ".tar.xz",
    ".tar.lz4",
    ".tar.zstd",
    ".tar.br",
    ".bay_gamer",
  ];
  for (const ext of compoundExts) {
    if (lower.endsWith(ext)) {
      return filename.slice(0, -ext.length);
    }
  }
  return filename.replace(/\.[^/.]+$/, "");
}

function isArchiveFile(filePath: string): boolean {
  const lower = filePath.toLowerCase();
  return ARCHIVE_EXTS.some((ext) => lower.endsWith(ext));
}

function App() {
  // Navigation Mode: "compress" or "extract"
  const [activeMode, setActiveMode] = useState<"compress" | "extract">("compress");

  // System Specs
  const [specs, setSpecs] = useState<SystemSpecs | null>(null);
  const [isRefreshingSpecs, setIsRefreshingSpecs] = useState(false);
  const [sliderValue, setSliderValue] = useState<number>(50); // 0, 25, 50, 75, 100
  const [isAdvanced, setIsAdvanced] = useState(false);

  const handleRefreshSpecs = async () => {
    setIsRefreshingSpecs(true);
    try {
      const res = await invoke<SystemSpecs>("get_system_specs");
      setSpecs(res);
    } catch (err) {
      console.error("Specs query error:", err);
    } finally {
      setTimeout(() => setIsRefreshingSpecs(false), 500);
    }
  };

  // Compress Options & Naming
  const [customArchiveName, setCustomArchiveName] = useState<string>("");
  const [selectedFormat, setSelectedFormat] = useState(".bay");
  const [selectedAlgorithm, setSelectedAlgorithm] = useState(FORMAT_ALGORITHMS[".bay"][0]);
  const [selectedEncryption, setSelectedEncryption] = useState("none");
  const [compressPassword, setCompressPassword] = useState<string>("");
  const [showCompressPassword, setShowCompressPassword] = useState<boolean>(false);
  const [archiveToOriginalPath, setArchiveToOriginalPath] = useState(true);
  const [compressDestDir, setCompressDestDir] = useState<string>("");
  const [separateArchives, setSeparateArchives] = useState(false);
  const [selectedFiles, setSelectedFiles] = useState<string[]>([]);
  const [itemsInfo, setItemsInfo] = useState<Record<string, { size: number; is_dir: boolean }>>({});
  const [isAboutOpen, setIsAboutOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [contextMenuStatus, setContextMenuStatus] = useState<string | null>(null);

  // Extraction State & Password
  const [extractPassword, setExtractPassword] = useState<string>("");
  const [showExtractPassword, setShowExtractPassword] = useState<boolean>(false);
  const [isPasswordPromptOpen, setIsPasswordPromptOpen] = useState<boolean>(false);

  // Compression execution state
  const [isCompressing, setIsCompressing] = useState(false);
  const [isCancellingCompress, setIsCancellingCompress] = useState(false);
  const [progress, setProgress] = useState<CompressProgress | null>(null);
  const [compressResult, setCompressResult] = useState<CompressResult | null>(null);
  const [compressError, setCompressError] = useState<string | null>(null);
  const [activeCompressConfig, setActiveCompressConfig] = useState<{
    format: string;
    algorithm: string;
    sliderValue: number;
    archiveName: string;
  } | null>(null);

  // Extraction State
  const [selectedArchive, setSelectedArchive] = useState<string | null>(null);
  const [archiveInfo, setArchiveInfo] = useState<ArchiveInfo | null>(null);
  const [isInspectingArchive, setIsInspectingArchive] = useState<boolean>(false);
  const [extractDest, setExtractDest] = useState<string>("");
  const [extractToSubfolder, setExtractToSubfolder] = useState<boolean>(true);
  const [openFolderOnExtract, setOpenFolderOnExtract] = useState<boolean>(true);
  const [isExtracting, setIsExtracting] = useState<boolean>(false);
  const [isCancellingExtract, setIsCancellingExtract] = useState<boolean>(false);
  const [extractProgress, setExtractProgress] = useState<ExtractProgress | null>(null);
  const [extractResult, setExtractResult] = useState<ExtractResult | null>(null);
  const [extractError, setExtractError] = useState<string | null>(null);
  // Safe in-memory preview state
  const [isPreviewOpen, setIsPreviewOpen] = useState(false);
  const [previewData, setPreviewData] = useState<ArchiveEntryPreview | null>(null);
  const [isPreviewLoading, setIsPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);

  const handlePreviewEntry = async (entryName: string) => {
    if (!selectedArchive) return;
    setIsPreviewOpen(true);
    setIsPreviewLoading(true);
    setPreviewError(null);
    setPreviewData(null);

    try {
      const res = await invoke<ArchiveEntryPreview>("preview_archive_entry", {
        archivePath: selectedArchive,
        entryName,
        password: extractPassword.trim() ? extractPassword.trim() : null,
      });
      setPreviewData(res);
    } catch (err: unknown) {
      setPreviewError(typeof err === "string" ? err : String(err));
    } finally {
      setIsPreviewLoading(false);
    }
  };

  // Query file and folder size metadata when selection changes
  useEffect(() => {
    if (selectedFiles.length === 0) {
      setItemsInfo({});
      return;
    }
    let isCurrent = true;
    invoke<SelectedItemInfo[]>("get_selected_items_info", { paths: selectedFiles })
      .then((infos) => {
        if (!isCurrent) return;
        const map: Record<string, { size: number; is_dir: boolean }> = {};
        infos.forEach((info) => {
          map[info.path] = { size: info.size, is_dir: info.is_dir };
        });
        setItemsInfo(map);
      })
      .catch((err) => {
        if (isCurrent) console.error("Failed to query item sizes:", err);
      });
    return () => {
      isCurrent = false;
    };
  }, [selectedFiles]);

  // Auto-fill archive name from first file stem if currently empty
  useEffect(() => {
    if (selectedFiles.length > 0 && !customArchiveName) {
      const first = selectedFiles[0];
      const filename = first.split(/[/\\]/).pop() || "bayzip_archive";
      const stem = getArchiveStemName(filename);
      setCustomArchiveName(stem);
    }
  }, [selectedFiles]);

  const totalSelectedSize = useMemo(() => {
    return selectedFiles.reduce((acc, path) => acc + (itemsInfo[path]?.size || 0), 0);
  }, [selectedFiles, itemsInfo]);

  // Select Archive Handler (with metadata inquiry)
  const selectArchive = async (filePath: string, pwd?: string) => {
    setSelectedArchive(filePath);
    setExtractResult(null);
    setExtractError(null);
    setExtractProgress(null);

    // Parent directory default
    const parentDir = filePath.replace(/[/\\][^/\\]+$/, "");
    setExtractDest(parentDir || ".");

    const filename = filePath.split(/[/\\]/).pop() || "archive";
    const lower = filename.toLowerCase();
    let format = "Archive";
    if (lower.endsWith(".bay_gamer")) format = "Bayzip Gamer Archive (.bay_gamer)";
    else if (lower.endsWith(".bay")) format = "Bayzip Archive (.bay)";
    else if (lower.endsWith(".zip")) format = "ZIP Archive (.zip)";
    else if (lower.endsWith(".7z")) format = "7-Zip Archive (.7z)";
    else if (lower.endsWith(".rar")) format = "RAR Archive (.rar)";
    else if (lower.endsWith(".tar") || lower.endsWith(".tar.gz") || lower.endsWith(".tgz")) format = "TAR Archive";

    // Immediate synchronous skeleton info so UI renders archive card & dedicated folder name immediately
    setArchiveInfo({
      path: filePath,
      filename,
      size: 0,
      format,
      is_archive: true,
      is_encrypted: false,
      file_count: 0,
      total_uncompressed_size: 0,
      entries: [],
    });
    setIsInspectingArchive(true);

    try {
      const info = await invoke<ArchiveInfo>("get_archive_info", {
        path: filePath,
        password: pwd || (extractPassword.trim() ? extractPassword.trim() : undefined),
      });
      setArchiveInfo(info);
      if (info.is_encrypted && (!pwd || !pwd.trim())) {
        setIsPasswordPromptOpen(true);
      }
    } catch (err) {
      console.error("Failed to inspect archive info:", err);
      const errStr = String(err).toLowerCase();
      const isEnc = errStr.includes("password") || errStr.includes("encrypt");
      setArchiveInfo({
        path: filePath,
        filename,
        size: 0,
        format,
        is_archive: true,
        is_encrypted: isEnc,
        file_count: 0,
        total_uncompressed_size: 0,
        entries: [],
      });
      if (isEnc) {
        setIsPasswordPromptOpen(true);
      }
    } finally {
      setIsInspectingArchive(false);
    }
  };

  useEffect(() => {
    let isMounted = true;
    let unlistenDrop: (() => void) | undefined;
    let unlistenCompress: (() => void) | undefined;
    let unlistenExtract: (() => void) | undefined;

    // 1. Instant hardware specs query
    invoke<SystemSpecs>("get_system_specs")
      .then((res) => {
        if (isMounted) setSpecs(res);
      })
      .catch((err) => console.error("Specs query error:", err));

    // 2. Fetch CLI / Context menu startup arguments
    invoke<StartupCommand>("get_cli_args")
      .then((cmd) => {
        if (isMounted && cmd && cmd.paths && cmd.paths.length > 0) {
          if (cmd.action === "compress") {
            setActiveMode("compress");
            setSelectedFiles((prev) => [...new Set([...prev, ...cmd.paths])]);
          } else if (cmd.action === "extract") {
            setActiveMode("extract");
            selectArchive(cmd.paths[0]);
          } else {
            if (cmd.paths.length === 1 && isArchiveFile(cmd.paths[0])) {
              setActiveMode("extract");
              selectArchive(cmd.paths[0]);
            } else {
              setActiveMode("compress");
              setSelectedFiles((prev) => [...new Set([...prev, ...cmd.paths])]);
            }
          }
        }
      })
      .catch(() => { });

    // 3. Listen to native Tauri file drop events
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          const paths = event.payload.paths;
          if (paths && paths.length > 0) {
            if (paths.length === 1 && isArchiveFile(paths[0])) {
              setActiveMode("extract");
              selectArchive(paths[0]);
            } else {
              setSelectedFiles((prev) => [...new Set([...prev, ...paths])]);
            }
          }
        }
      })
      .then((fn) => {
        if (isMounted) unlistenDrop = fn;
        else fn();
      })
      .catch((err) => {
        console.error("Failed to register drag-drop listener:", err);
      });

    // 4. Listen to live compression progress events from Rust
    listen<CompressProgress>("compress_progress", (event) => {
      if (isMounted) setProgress(event.payload);
    })
      .then((fn) => {
        if (isMounted) unlistenCompress = fn;
        else fn();
      })
      .catch((err) => {
        console.error("Compress progress listener error:", err);
      });

    // 5. Listen to live extraction progress events from Rust
    listen<ExtractProgress>("extract_progress", (event) => {
      if (isMounted) setExtractProgress(event.payload);
    })
      .then((fn) => {
        if (isMounted) unlistenExtract = fn;
        else fn();
      })
      .catch((err) => {
        console.error("Extract progress listener error:", err);
      });

    return () => {
      isMounted = false;
      if (unlistenDrop) unlistenDrop();
      if (unlistenCompress) unlistenCompress();
      if (unlistenExtract) unlistenExtract();
    };
  }, []);

  useEffect(() => {
    setSelectedAlgorithm(FORMAT_ALGORITHMS[selectedFormat][0]);
  }, [selectedFormat]);

  const getEstimatedRam = () => {
    const fmt = isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat;
    const val = isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue;
    if (fmt === ".tar") return "< 0.1";
    if (fmt === ".zip") return val === 0 ? "< 0.1" : "0.3";
    if (val === 0) return "0.1";
    if (!specs) return "0.5";
    const maxSafeRam = Math.max(0.5, specs.free_ram_gb - 1.5);
    return ((val / 100) * maxSafeRam).toFixed(1);
  };

  const getEstimatedCores = () => {
    const fmt = isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat;
    const val = isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue;
    if (fmt === ".tar" || fmt === ".zip") return 1;
    if (val <= 0) return 1;
    if (!specs) return 1;
    if (val === 25) return Math.max(1, Math.floor(specs.cpu_cores / 4));
    if (val === 50) return Math.max(1, Math.floor(specs.cpu_cores / 2));
    if (val === 75) return Math.max(1, Math.floor((specs.cpu_cores * 3) / 4));
    return specs.cpu_cores;
  };

  const handleSelectFiles = async () => {
    try {
      const selected = await open({
        multiple: true,
        title: "Select files to compress",
      });
      if (selected) {
        const newFiles = Array.isArray(selected) ? selected : [selected];
        setSelectedFiles((prev) => [...new Set([...prev, ...newFiles])]);
      }
    } catch (err) {
      console.error("File selection error:", err);
    }
  };

  const handleSelectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: true,
        title: "Select folder to compress",
      });
      if (selected) {
        const newFolders = Array.isArray(selected) ? selected : [selected];
        setSelectedFiles((prev) => [...new Set([...prev, ...newFolders])]);
      }
    } catch (err) {
      console.error("Folder selection error:", err);
    }
  };

  const handleSelectArchive = async () => {
    if (isExtracting) return;
    try {
      const selected = await open({
        multiple: false,
        title: "Select Archive to Extract",
        filters: [
          {
            name: "Supported Archives",
            extensions: ["bay", "bay_gamer", "zip", "7z", "tar", "gz", "tgz", "rar"],
          },
          {
            name: "Bayzip Archives (*.bay, *.bay_gamer)",
            extensions: ["bay", "bay_gamer"],
          },
          {
            name: "ZIP Archives (*.zip)",
            extensions: ["zip"],
          },
          {
            name: "7-Zip Archives (*.7z)",
            extensions: ["7z"],
          },
          {
            name: "RAR Archives (*.rar)",
            extensions: ["rar"],
          },
          {
            name: "TAR Archives (*.tar, *.tar.gz)",
            extensions: ["tar", "gz", "tgz"],
          },
          {
            name: "All Files (*.*)",
            extensions: ["*"],
          },
        ],
      });
      if (selected && typeof selected === "string") {
        selectArchive(selected);
      }
    } catch (err) {
      console.error("Archive selection error:", err);
    }
  };

  const handleSelectDestFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Extraction Destination Folder",
        defaultPath: extractDest || undefined,
      });
      if (selected && typeof selected === "string") {
        setExtractDest(selected);
      }
    } catch (err) {
      console.error("Destination folder selection error:", err);
    }
  };

  const handleSelectCompressDestination = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Output Destination Folder",
        defaultPath: compressDestDir || undefined,
      });
      if (selected && typeof selected === "string") {
        setCompressDestDir(selected);
      }
    } catch (err) {
      console.error("Compress destination folder selection error:", err);
    }
  };

  const [isSyncingRegistry, setIsSyncingRegistry] = useState(false);

  const handleRegisterContextMenu = async () => {
    setIsSyncingRegistry(true);
    try {
      const res = await invoke<string>("register_context_menu");
      setContextMenuStatus(res);
    } catch (e) {
      setContextMenuStatus("Registration failed: " + String(e));
    } finally {
      setIsSyncingRegistry(false);
    }
  };

  const handleOpenSettings = async () => {
    try {
      await invoke("open_settings_window");
    } catch (err) {
      console.warn("Failed to open native settings window, falling back to modal:", err);
      setIsSettingsOpen(true);
    }
  };

  const handleCancelCompress = async () => {
    try {
      setIsCancellingCompress(true);
      await invoke("cancel_compression");
    } catch (e) {
      console.error("Failed to cancel compression:", e);
    }
  };

  const handleCompress = async () => {
    if (selectedFiles.length === 0) {
      await handleSelectFiles();
      return;
    }

    if (selectedEncryption !== "none" && !compressPassword.trim()) {
      setCompressError("Please enter an archive password to enable encryption.");
      return;
    }

    const targetArchiveName = customArchiveName.trim() ||
      (selectedFiles.length === 1 ? selectedFiles[0].split(/[/\\]/).pop() || "archive" : "archive");

    setActiveCompressConfig({
      format: selectedFormat,
      algorithm: selectedAlgorithm,
      sliderValue,
      archiveName: targetArchiveName,
    });
    setIsCompressing(true);
    setIsCancellingCompress(false);
    setCompressError(null);
    setProgress({
      percent: 0,
      current_file: "Initializing compression...",
      bytes_processed: 0,
      total_bytes: 0,
    });

    try {
      if (separateArchives && selectedFiles.length > 1) {
        let lastRes: CompressResult | null = null;
        let totalOrig = 0;
        let totalComp = 0;
        let totalElapsed = 0;

        for (let i = 0; i < selectedFiles.length; i++) {
          const file = selectedFiles[i];
          const res = await invoke<CompressResult>("compress_files", {
            payload: {
              files: [file],
              format: selectedFormat,
              algorithm: selectedAlgorithm,
              slider_value: sliderValue,
              custom_name: undefined, // Each file uses its own name
              encryption: selectedEncryption,
              password: selectedEncryption !== "none" && compressPassword.trim() ? compressPassword.trim() : undefined,
              output_dir: !archiveToOriginalPath && compressDestDir.trim() ? compressDestDir.trim() : undefined,
            },
          });
          totalOrig += res.original_size;
          totalComp += res.compressed_size;
          totalElapsed += res.elapsed_ms;
          lastRes = res;
        }

        if (lastRes) {
          setCompressResult({
            ...lastRes,
            original_size: totalOrig,
            compressed_size: totalComp,
            elapsed_ms: totalElapsed,
          });
        }
      } else {
        const res = await invoke<CompressResult>("compress_files", {
          payload: {
            files: selectedFiles,
            format: selectedFormat,
            algorithm: selectedAlgorithm,
            slider_value: sliderValue,
            custom_name: customArchiveName.trim() || undefined,
            encryption: selectedEncryption,
            password: selectedEncryption !== "none" && compressPassword.trim() ? compressPassword.trim() : undefined,
            output_dir: !archiveToOriginalPath && compressDestDir.trim() ? compressDestDir.trim() : undefined,
          },
        });
        setCompressResult(res);
      }
    } catch (err) {
      const errStr = String(err);
      if (errStr.toLowerCase().includes("cancel")) {
        console.log("Compression cancelled by user.");
        setCompressError(null);
      } else {
        console.error("Compression execution error:", err);
        setCompressError(errStr);
      }
    } finally {
      setIsCompressing(false);
      setIsCancellingCompress(false);
      setActiveCompressConfig(null);
      setProgress(null);
    }
  };

  const handleCancelExtract = async () => {
    try {
      setIsCancellingExtract(true);
      await invoke("cancel_extraction");
    } catch (e) {
      console.error("Failed to cancel extraction:", e);
    }
  };

  const handleExtract = async () => {
    if (!selectedArchive) {
      await handleSelectArchive();
      return;
    }

    if (archiveInfo?.is_encrypted && !extractPassword.trim()) {
      setExtractError("Password required: This archive is encrypted. Please enter the password to extract.");
      setIsPasswordPromptOpen(true);
      return;
    }

    setIsExtracting(true);
    setIsCancellingExtract(false);
    setExtractError(null);
    setExtractProgress({
      percent: 0,
      current_file: "Preparing extraction...",
      bytes_extracted: 0,
    });

    try {
      let finalDest = extractDest;
      if (extractToSubfolder) {
        // Check if all files in the archive already share a single common top-level root folder
        const entries = archiveInfo?.entries || [];
        let hasSingleRoot = false;
        if (entries.length > 0) {
          const firstPath = entries[0].name.replace(/\\/g, "/");
          const firstSlash = firstPath.indexOf("/");
          if (firstSlash !== -1) {
            const rootPrefix = firstPath.substring(0, firstSlash);
            hasSingleRoot = entries.every((e) => {
              const clean = e.name.replace(/\\/g, "/");
              return clean === rootPrefix || clean.startsWith(rootPrefix + "/");
            });
          }
        }

        // Only append subfolder if the archive does not already contain a single root folder
        if (!hasSingleRoot) {
          const fallbackName = selectedArchive ? selectedArchive.split(/[/\\]/).pop() || "archive" : "archive";
          const stem = getArchiveStemName(archiveInfo?.filename || fallbackName);
          finalDest = `${extractDest}\\${stem}`;
        }
      }

      const res = await invoke<ExtractResult>("extract_archive", {
        archivePath: selectedArchive,
        destination: finalDest,
        password: extractPassword.trim() ? extractPassword.trim() : undefined,
        expectedSize: archiveInfo?.total_uncompressed_size || archiveInfo?.size || undefined,
        extractToSubfolder: extractToSubfolder,
      });

      setExtractResult(res);
      setIsPasswordPromptOpen(false);

      if (openFolderOnExtract && res.output_directory) {
        revealItemInDir(res.output_directory).catch(() => { });
      }
    } catch (err) {
      const errMsg = String(err);
      if (errMsg.toLowerCase().includes("cancel")) {
        console.log("Extraction cancelled by user.");
        setExtractError(null);
      } else {
        console.error("Extraction execution error:", err);
        setExtractError(errMsg);
        if (errMsg.toLowerCase().includes("password")) {
          setIsPasswordPromptOpen(true);
        }
      }
    } finally {
      setIsExtracting(false);
      setIsCancellingExtract(false);
      setExtractProgress(null);
    }
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return (bytes / Math.pow(k, i)).toFixed(2) + " " + sizes[i];
  };

  const getSliderLabel = () => {
    const fmt = isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat;
    const val = isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue;

    if (fmt === ".tar") {
      return "Direct Packaging (Store)";
    }
    if (fmt === ".zip") {
      if (val === 0) return "Store (Instant)";
      return "Deflate (Standard)";
    }
    if (fmt === ".7z") {
      if (val === 0) return "Fastest (Level 1)";
      if (val === 25) return "Fast (Level 3)";
      if (val === 50) return "Balanced (Level 5)";
      if (val === 75) return "High (Level 7)";
      return "Maximum (Level 9)";
    }
    if (fmt === ".bay_gamer") {
      if (val === 0) return "Eco (1 Core)";
      if (val === 25) return "Fast";
      if (val === 50) return "Balanced";
      if (val === 75) return "High Performance";
      return "Maximum Power";
    }
    // Default .bay (Zstandard)
    if (val === 0) return "Eco (1 Core)";
    if (val === 25) return "Fast";
    if (val === 50) return "Balanced";
    if (val === 75) return "High Performance";
    return "Maximum Power";
  };

  return (
    <div className="h-screen w-screen bg-[#202020] text-white flex flex-col font-sans select-none overflow-hidden">
      {/* Windows 11 Native Custom Titlebar */}
      <Titlebar />

      {/* Main Scrollable Body */}
      <div className="flex-1 flex flex-col p-6 overflow-y-auto custom-scrollbar">
        <div className="w-full max-w-2xl mx-auto space-y-6">
          {/* Header & Mode Switcher */}
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-3">
              {/* Segmented Mode Control */}
              <div className="flex items-center bg-[#1A1A1A] p-1 rounded-lg border border-[#333333]">
                <button
                  type="button"
                  onClick={() => {
                    setActiveMode("compress");
                    setExtractResult(null);
                  }}
                  className={`flex items-center space-x-2 px-3.5 py-1.5 rounded-md text-xs font-medium transition-all cursor-pointer ${activeMode === "compress"
                      ? "bg-[#2D2D2D] text-white shadow-sm border border-[#3E3E3E]"
                      : "text-[#888888] hover:text-white"
                    }`}
                  title={isCompressing ? "Compression in progress" : undefined}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-[#60CDFF]">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                  </svg>
                  <span>Compress</span>
                  {isCompressing && activeMode !== "compress" && (
                    <span className="inline-flex items-center space-x-1 ml-1 px-1.5 py-0.5 rounded-full bg-[#60CDFF]/15 text-[#60CDFF] text-[10px] font-mono border border-[#60CDFF]/30">
                      <span className="w-1.5 h-1.5 rounded-full bg-[#60CDFF] animate-pulse"></span>
                      <span>{progress ? `${Math.round(progress.percent)}%` : "..."}</span>
                    </span>
                  )}
                </button>
                <button
                  type="button"
                  onClick={() => setActiveMode("extract")}
                  className={`flex items-center space-x-2 px-3.5 py-1.5 rounded-md text-xs font-medium transition-all cursor-pointer ${activeMode === "extract"
                      ? "bg-[#2D2D2D] text-white shadow-sm border border-[#3E3E3E]"
                      : "text-[#888888] hover:text-white"
                    }`}
                  title={isExtracting ? "Extraction in progress" : undefined}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-[#60CDFF]">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" />
                  </svg>
                  <span>Extract</span>
                  {isExtracting && activeMode !== "extract" && (
                    <span className="inline-flex items-center space-x-1 ml-1 px-1.5 py-0.5 rounded-full bg-[#60CDFF]/15 text-[#60CDFF] text-[10px] font-mono border border-[#60CDFF]/30">
                      <span className="w-1.5 h-1.5 rounded-full bg-[#60CDFF] animate-pulse"></span>
                      <span>{extractProgress ? `${Math.round(extractProgress.percent)}%` : "..."}</span>
                    </span>
                  )}
                </button>
              </div>

              <span className="text-xl font-semibold tracking-tight text-white hidden sm:inline">
                {activeMode === "compress" ? "Create Archive" : "Extract Archive"}
              </span>
            </div>

            <div className="flex items-center space-x-2">
              {activeMode === "compress" && (
                <button
                  type="button"
                  onClick={() => setIsAdvanced(!isAdvanced)}
                  className="text-xs font-medium px-3 py-1.5 rounded-md bg-[#252525] hover:bg-[#303030] border border-[#3E3E3E] hover:border-[#555] transition-colors cursor-pointer text-[#C8C8C8] hover:text-white"
                >
                  {isAdvanced ? "Simple Mode" : "Advanced"}
                </button>
              )}
              <button
                type="button"
                onClick={() => setIsAboutOpen(true)}
                className="text-xs font-medium px-2.5 py-1.5 rounded-md text-[#A8A8A8] hover:text-white hover:bg-[#252525] transition-colors cursor-pointer"
              >
                About
              </button>
              <button
                type="button"
                onClick={handleOpenSettings}
                className="p-1.5 text-[#A8A8A8] hover:text-[#60CDFF] transition-colors cursor-pointer group"
                title="Settings"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={2}
                  className="w-4 h-4 transition-transform duration-500 ease-out group-hover:rotate-90"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                  <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
              </button>
            </div>
          </div>

          {/* ======================================================== */}
          {/* COMPRESS MODE CONTENT                                     */}
          {/* ======================================================== */}
          {activeMode === "compress" && (
            <>
              {/* Windows 11 File Drop Zone */}
              {selectedFiles.length > 0 ? (
                <div className="bg-[#2D2D2D] border border-[#3E3E3E] rounded-lg p-4 flex flex-col space-y-3">
                  <div className="flex justify-between items-center text-xs border-b border-[#3E3E3E] pb-2">
                    <div className="flex items-center space-x-2 text-[#A8A8A8]">
                      <span className="text-white font-medium">
                        {selectedFiles.length} item{selectedFiles.length > 1 ? "s" : ""} selected
                      </span>
                      {totalSelectedSize > 0 && (
                        <>
                          <span>•</span>
                          <span className="text-[#60CDFF] font-mono font-medium">
                            Total: {formatBytes(totalSelectedSize)}
                          </span>
                        </>
                      )}
                      {isCompressing && (
                        <span className="text-[10px] text-[#60CDFF] bg-[#60CDFF]/10 border border-[#60CDFF]/30 px-2 py-0.5 rounded font-medium">
                          Staging for Next Archive
                        </span>
                      )}
                    </div>
                    <button
                      type="button"
                      onClick={() => {
                        setSelectedFiles([]);
                        if (!isCompressing) {
                          setCustomArchiveName("");
                        }
                      }}
                      className="text-red-400 hover:text-red-300 transition-colors cursor-pointer"
                    >
                      Clear all
                    </button>
                  </div>

                  <div className="max-h-48 overflow-y-auto flex flex-col space-y-2 custom-scrollbar pr-1">
                    {selectedFiles.map((file) => {
                      const info = itemsInfo[file];
                      const isDir = info?.is_dir;
                      const sizeFormatted = info !== undefined ? formatBytes(info.size) : "...";
                      return (
                        <div
                          key={file}
                          className="flex justify-between items-center bg-[#202020] px-4 py-2.5 rounded-md border border-[#3E3E3E] group hover:border-[#555] transition-colors"
                        >
                          <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                            {isDir ? (
                              <svg
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                className="w-4 h-4 text-[#F9D648] shrink-0"
                              >
                                <path
                                  strokeLinecap="round"
                                  strokeLinejoin="round"
                                  strokeWidth={2}
                                  d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                                />
                              </svg>
                            ) : (
                              <svg
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                className="w-4 h-4 text-[#60CDFF] shrink-0"
                              >
                                <path
                                  strokeLinecap="round"
                                  strokeLinejoin="round"
                                  strokeWidth={2}
                                  d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"
                                />
                              </svg>
                            )}
                            <span className="text-sm truncate text-white" title={file}>
                              {file.split("\\").pop() || file.split("/").pop()}
                            </span>
                            {isDir && (
                              <span className="text-[10px] text-[#A8A8A8] bg-[#2A2A2A] px-1.5 py-0.5 rounded border border-[#3E3E3E] shrink-0">
                                folder
                              </span>
                            )}
                          </div>
                          <div className="flex items-center space-x-3 shrink-0">
                            <span className="text-xs font-mono font-medium text-[#C8C8C8] bg-[#2A2A2A] px-2.5 py-1 rounded border border-[#3E3E3E]">
                              {sizeFormatted}
                            </span>
                            <button
                              type="button"
                              onClick={() => setSelectedFiles((prev) => prev.filter((f) => f !== file))}
                              className="text-[#A8A8A8] hover:text-red-400 p-1 rounded hover:bg-[#333333] transition-colors cursor-pointer shrink-0"
                              title="Remove"
                            >
                              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                              </svg>
                            </button>
                          </div>
                        </div>
                      );
                    })}
                  </div>

                  <div className="flex items-center space-x-3 pt-1">
                    <button
                      type="button"
                      onClick={handleSelectFiles}
                      className="text-xs font-medium text-[#60CDFF] hover:text-[#4FB8E6] transition-colors px-3 py-1.5 bg-[#202020] rounded border border-[#3E3E3E] hover:border-[#60CDFF] cursor-pointer"
                    >
                      + Add more files
                    </button>
                    <button
                      type="button"
                      onClick={handleSelectFolder}
                      className="text-xs font-medium text-[#60CDFF] hover:text-[#4FB8E6] transition-colors px-3 py-1.5 bg-[#202020] rounded border border-[#3E3E3E] hover:border-[#60CDFF] cursor-pointer"
                    >
                      + Add folder
                    </button>
                    {isCompressing && (
                      <span className="text-[11px] text-[#A8A8A8] italic">
                        (Staged for next archive — active compression won't be interrupted)
                      </span>
                    )}
                  </div>
                </div>
              ) : (
                <div className="bg-[#2D2D2D] border border-dashed border-[#3E3E3E] hover:border-[#60CDFF] rounded-lg p-8 text-center transition-colors flex flex-col items-center justify-center space-y-4 group">
                  <div className="w-12 h-12 rounded-full bg-[#202020] border border-[#3E3E3E] group-hover:border-[#60CDFF] group-hover:shadow-[0_0_15px_rgba(96,205,255,0.25)] flex items-center justify-center text-[#60CDFF] transition-all duration-200">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-6 h-6">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={1.8}
                        d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"
                      />
                    </svg>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-white mb-1">Drag & drop files or folders here</p>
                    <p className="text-xs text-[#A8A8A8]">Native Windows Explorer drag-and-drop supported</p>
                  </div>
                  <div className="flex space-x-3 pt-1">
                    <button
                      type="button"
                      onClick={handleSelectFiles}
                      className="text-xs font-medium px-4 py-2 bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#60CDFF] text-white rounded-md transition-colors cursor-pointer"
                    >
                      Browse Files
                    </button>
                    <button
                      type="button"
                      onClick={handleSelectFolder}
                      className="text-xs font-medium px-4 py-2 bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#60CDFF] text-white rounded-md transition-colors cursor-pointer"
                    >
                      Browse Folder
                    </button>
                  </div>
                </div>
              )}

              {/* Archive Name Card */}
              <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-4 flex flex-col space-y-2">
                <div className="flex justify-between items-center text-xs">
                  <span className="font-medium text-[#C8C8C8]">Archive Name</span>
                  <span className="text-[11px] text-[#888888]">Saved in source folder</span>
                </div>
                <div className="flex items-center space-x-2.5">
                  <div className="w-9 h-9 rounded-lg bg-[#202020] border border-[#3E3E3E] flex items-center justify-center shrink-0">
                    <img
                      src="/bayzip-logo.png"
                      alt="Bayzip"
                      className="w-5 h-5 object-contain"
                      onError={(e) => {
                        (e.currentTarget as HTMLElement).style.display = "none";
                      }}
                    />
                  </div>
                  <input
                    type="text"
                    disabled={isCompressing}
                    value={customArchiveName}
                    onChange={(e) => setCustomArchiveName(e.target.value)}
                    placeholder={selectedFiles.length > 0 ? "Archive name..." : "Enter archive name..."}
                    className="flex-1 bg-[#202020] text-sm text-white px-3 py-2 rounded-md border border-[#3E3E3E] focus:border-[#60CDFF] outline-none font-medium placeholder-[#666] disabled:opacity-50 disabled:cursor-not-allowed"
                  />
                  <span className="bg-[#202020] px-3 py-2 rounded-md border border-[#3E3E3E] text-xs font-mono text-[#60CDFF] shrink-0 font-semibold">
                    {isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat}
                  </span>
                </div>
              </div>

              {/* Active Compression Progress Card - High Visibility Placement */}
              {isCompressing && (
                <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-4 shadow-lg space-y-3 animate-fadeIn">
                  <div className="flex justify-between items-center text-xs">
                    <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                      <span className="w-2.5 h-2.5 rounded-full bg-[#60CDFF] animate-pulse shrink-0"></span>
                      <span className="text-[#D0D0D0] truncate font-medium">
                        {isCancellingCompress
                          ? "Cancelling compression..."
                          : progress?.current_file
                            ? `Compressing: ${progress.current_file}`
                            : `Compressing into ${activeCompressConfig?.archiveName || "archive"}${activeCompressConfig?.format || selectedFormat}...`}
                      </span>
                    </div>
                    <div className="flex items-center space-x-2.5 shrink-0">
                      <span className="text-sm font-semibold text-[#60CDFF] font-mono">
                        {progress ? Math.round(progress.percent) : 0}%
                      </span>
                      <button
                        type="button"
                        onClick={handleCancelCompress}
                        disabled={isCancellingCompress}
                        className="w-7 h-7 flex items-center justify-center text-[#FF99A4] hover:text-white bg-red-950/40 hover:bg-red-900/70 border border-red-800/60 hover:border-red-600 rounded-md transition-all cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-sm group"
                        title="Cancel Compression"
                      >
                        {isCancellingCompress ? (
                          <svg className="animate-spin h-3.5 w-3.5 text-[#FF99A4]" fill="none" viewBox="0 0 24 24">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                          </svg>
                        ) : (
                          <svg className="w-3.5 h-3.5 group-hover:scale-110 transition-transform" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
                          </svg>
                        )}
                      </button>
                    </div>
                  </div>

                  {/* Progress Track & Animated Glow Fill */}
                  <div className="w-full h-2 bg-[#202020] rounded-full overflow-hidden border border-[#3E3E3E]">
                    <div
                      className="h-full bg-gradient-to-r from-[#0078D4] to-[#60CDFF] transition-all duration-150 ease-out rounded-full shadow-[0_0_10px_rgba(96,205,255,0.7)]"
                      style={{ width: `${progress ? Math.max(2, progress.percent) : 2}%` }}
                    ></div>
                  </div>

                  <div className="flex justify-between items-center text-[11px] text-[#8A8A8A]">
                    <span>
                      {progress && progress.total_bytes > 0
                        ? `${formatBytes(progress.bytes_processed)} / ${formatBytes(progress.total_bytes)}`
                        : "Hardware Acceleration Active"}
                    </span>
                    <span className="text-[#60CDFF] font-medium">
                      {(activeCompressConfig?.algorithm || selectedAlgorithm)} • {(activeCompressConfig?.format || selectedFormat)}
                    </span>
                  </div>
                </div>
              )}

              {/* Hardware Configuration - Fluent Design Card */}
              <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6">
                <div className="flex justify-between items-center mb-6">
                  <div className="flex items-center space-x-2">
                    <h3 className="text-sm font-medium">Hardware Allocation</h3>
                  </div>
                  <div className="flex items-center space-x-2">
                    <button
                      type="button"
                      onClick={handleRefreshSpecs}
                      disabled={isRefreshingSpecs || isCompressing}
                      title="Refresh available RAM & system hardware specs"
                      className="flex items-center space-x-1.5 text-xs bg-[#202020] hover:bg-[#2A2A2A] border border-[#3E3E3E] hover:border-[#60CDFF] text-[#60CDFF] px-2.5 py-1 rounded-md transition-all duration-200 hover:shadow-[0_0_10px_rgba(96,205,255,0.3)] cursor-pointer group disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      <svg
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        className={`w-3.5 h-3.5 text-[#60CDFF] transition-transform ${isRefreshingSpecs ? "animate-spin" : "group-hover:rotate-180 duration-500"
                          }`}
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2}
                          d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                        />
                      </svg>
                      <span className="text-[11px] font-medium hidden sm:inline">Refresh RAM</span>
                    </button>
                    <span className="text-xs bg-[#3E3E3E] text-[#60CDFF] px-2 py-1 rounded-md font-mono text-[10px] tracking-wider uppercase font-semibold">
                      {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bay" && "Zstandard Multi-Core"}
                      {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bay_gamer" && "Zstd Gamer Capped"}
                      {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".7z" && "LZMA2 Multi-Thread"}
                      {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".zip" && "Zip Single-Thread"}
                      {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar" && "Store Direct I/O"}
                    </span>
                  </div>
                </div>

                {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar" ? (
                  <div className="bg-[#202020] border border-[#3E3E3E] rounded-md p-4 flex items-center space-x-3.5 mb-6">
                    <div className="w-10 h-10 rounded-lg bg-[#60CDFF]/10 border border-[#60CDFF]/30 flex items-center justify-center shrink-0">
                      <svg className="w-5 h-5 text-[#60CDFF]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" />
                      </svg>
                    </div>
                    <div className="min-w-0 flex-1">
                      <h4 className="text-xs font-semibold text-white">Direct Packaging Mode</h4>
                      <p className="text-[11px] text-[#A8A8A8] mt-0.5">
                        TAR files bundle files together without compression. Zero compression latency and minimal CPU overhead.
                      </p>
                    </div>
                  </div>
                ) : (
                  <div className="mb-6">
                    <div className="flex justify-between items-end mb-2">
                      <span className="text-sm font-semibold text-[#60CDFF]">{getSliderLabel()}</span>
                    </div>

                    <div className="relative pt-2 pb-8">
                      <input
                        type="range"
                        min="0"
                        max="100"
                        step="25"
                        disabled={isCompressing}
                        value={isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue}
                        onChange={(e) => setSliderValue(Number(e.target.value))}
                        className={`w-full h-1 bg-[#3E3E3E] rounded-lg appearance-none accent-[#60CDFF] relative z-10 ${isCompressing ? "opacity-50 cursor-not-allowed" : "cursor-pointer"
                          }`}
                      />
                      <div className="absolute w-[calc(100%-16px)] left-[8px] mt-2">
                        <span className="absolute left-[0%] -translate-x-1/2 text-xs text-[#A8A8A8]">0</span>
                        <span className="absolute left-[25%] -translate-x-1/2 text-xs text-[#A8A8A8]">25</span>
                        <span className="absolute left-[50%] -translate-x-1/2 text-xs text-[#A8A8A8]">50</span>
                        <span className="absolute left-[75%] -translate-x-1/2 text-xs text-[#A8A8A8]">75</span>
                        <span className="absolute left-[100%] -translate-x-1/2 text-xs text-[#A8A8A8]">100</span>
                      </div>
                    </div>
                  </div>
                )}

                {/* Metrics */}
                <div className="grid grid-cols-2 gap-4">
                  <div className="bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
                    <div className="flex justify-between items-start mb-1">
                      <p className="text-xs text-[#A8A8A8]">Target RAM Usage</p>
                      {specs && (
                        <span className="text-[10px] text-[#60CDFF] bg-[#1a3847] px-1.5 py-0.5 rounded font-mono">
                          {Math.round(specs.total_ram_gb)} GB Total
                        </span>
                      )}
                    </div>
                    <p className="text-lg font-medium">
                      {getEstimatedRam()} <span className="text-sm font-normal text-[#A8A8A8]">GB</span>
                    </p>
                    <p className="text-[11px] text-[#7A7A7A] mt-1">
                      {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar"
                        ? "Direct stream buffer"
                        : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".zip"
                          ? "Standard Deflate buffer"
                          : `Dynamic safe cap (${specs ? specs.free_ram_gb.toFixed(1) : "?"} GB available)`}
                    </p>
                  </div>

                  <div className="bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
                    <div className="flex justify-between items-start mb-1">
                      <p className="text-xs text-[#A8A8A8]">Target CPU Threads</p>
                      {specs && (
                        <span className="text-[10px] text-[#60CDFF] bg-[#1a3847] px-1.5 py-0.5 rounded font-mono">
                          {specs.cpu_cores} Threads
                        </span>
                      )}
                    </div>
                    <p className="text-lg font-medium">
                      {getEstimatedCores()}{" "}
                      <span className="text-sm font-normal text-[#A8A8A8]">/ {specs?.cpu_cores || "?"} Threads</span>
                    </p>
                    <p className="text-[11px] text-[#7A7A7A] mt-1">
                      {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar"
                        ? "Single-stream sequential I/O"
                        : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".zip"
                          ? "Standard ZIP is single-threaded"
                          : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".7z"
                            ? `${getEstimatedCores()} Threads allocated for LZMA2`
                            : (isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue) === 100
                              ? "100% Full Multithreading"
                              : `${isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue}% Thread Allocation`}
                    </p>
                  </div>
                </div>
              </div>

              {/* Advanced Options Toggle */}
              {isAdvanced && (
                <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6 space-y-5">
                  <div className="grid grid-cols-1 sm:grid-cols-12 gap-3.5">
                    <div className={(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bay_gamer" ? "sm:col-span-6" : "sm:col-span-3"}>
                      <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">Format</label>
                      <div className="relative group">
                        <select
                          disabled={isCompressing}
                          value={isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat}
                          onChange={(e) => {
                            if (isCompressing) return;
                            const newFormat = e.target.value;
                            setSelectedFormat(newFormat);
                            if (FORMAT_ALGORITHMS[newFormat]) {
                              setSelectedAlgorithm(FORMAT_ALGORITHMS[newFormat][0]);
                            }
                          }}
                          className={`w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none text-white truncate transition-colors ${isCompressing ? "opacity-50 cursor-not-allowed" : "cursor-pointer"
                            }`}
                        >
                          <option value=".bay">.bay</option>
                          <option value=".bay_gamer">.bay (Gamer Mod Pack)</option>
                          <option value=".7z">.7z</option>
                          <option value=".zip">.zip</option>
                          <option value=".tar">.tar</option>
                        </select>
                        <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-[#888888] group-hover:text-white transition-colors">
                          <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6l4 4 4-4" />
                          </svg>
                        </div>
                      </div>
                    </div>

                    {/* Hide algorithm dropdown if Gamer Mod Pack is selected */}
                    {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) !== ".bay_gamer" && (
                      <div className="sm:col-span-5">
                        <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">Algorithm</label>
                        <div className="relative group">
                          <select
                            disabled={isCompressing}
                            value={isCompressing && activeCompressConfig ? activeCompressConfig.algorithm : selectedAlgorithm}
                            onChange={(e) => {
                              if (isCompressing) return;
                              setSelectedAlgorithm(e.target.value);
                            }}
                            className={`w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none text-white truncate transition-colors ${isCompressing ? "opacity-50 cursor-not-allowed" : "cursor-pointer"
                              }`}
                          >
                            {FORMAT_ALGORITHMS[isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat]?.map((algo) => (
                              <option key={algo} value={algo}>
                                {algo}
                              </option>
                            ))}
                          </select>
                          <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-[#888888] group-hover:text-white transition-colors">
                            <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6l4 4 4-4" />
                            </svg>
                          </div>
                        </div>
                      </div>
                    )}

                    <div className={(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bay_gamer" ? "sm:col-span-6" : "sm:col-span-4"}>
                      <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">Encryption</label>
                      <div className="relative group">
                        <select
                          disabled={isCompressing}
                          value={selectedEncryption}
                          onChange={(e) => {
                            if (isCompressing) return;
                            setSelectedEncryption(e.target.value);
                          }}
                          className={`w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none text-white truncate transition-colors ${isCompressing ? "opacity-50 cursor-not-allowed" : "cursor-pointer"
                            }`}
                        >
                          <option value="none">None (Unencrypted)</option>
                          <option value="aes-256">AES-256 (Military-Grade)</option>
                          <option value="aes-128">AES-128 (Standard)</option>
                        </select>
                        <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-[#888888] group-hover:text-white transition-colors">
                          <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6l4 4 4-4" />
                          </svg>
                        </div>
                      </div>
                    </div>
                  </div>

                  {/* Windows Explorer Incompatibility Notice for BZip2 in .zip */}
                  {selectedFormat === ".zip" && selectedAlgorithm.toLowerCase().includes("bzip2") && (
                    <div className="p-3 rounded-lg bg-amber-500/10 border border-amber-500/30 flex items-start gap-3 text-xs text-amber-200">
                      <svg className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                      </svg>
                      <div className="space-y-1">
                        <p className="font-semibold text-amber-300">Windows Explorer Incompatibility Notice</p>
                        <p className="text-amber-200/90 leading-relaxed">
                          Windows File Explorer does not support BZip2 inside <code>.zip</code> files and will extract them as empty (0 KB) files.
                        </p>
                        <p className="text-amber-200/80 leading-relaxed">
                          • For standard Windows Explorer compatibility, choose <strong>Deflate (Standard)</strong>.
                          <br />
                          • If you want BZip2 compression, select the <strong>.7z</strong> format instead.
                        </p>
                      </div>
                    </div>
                  )}

                  {selectedEncryption !== "none" && (
                    <div>
                      <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">
                        Archive Password <span className="text-[#60CDFF]">*</span>
                      </label>
                      <div className="relative">
                        <input
                          type={showCompressPassword ? "text" : "password"}
                          disabled={isCompressing}
                          value={compressPassword}
                          onChange={(e) => setCompressPassword(e.target.value)}
                          placeholder="Enter secure password to protect archive..."
                          className="w-full bg-[#202020] rounded-md pl-3 pr-10 py-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none placeholder-[#777] text-white disabled:opacity-50 disabled:cursor-not-allowed"
                        />
                        <button
                          type="button"
                          disabled={isCompressing}
                          onClick={() => setShowCompressPassword(!showCompressPassword)}
                          className="absolute inset-y-0 right-0 pr-3 flex items-center text-[#888] hover:text-white cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                          title={showCompressPassword ? "Hide password" : "Show password"}
                        >
                          {showCompressPassword ? (
                            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l18 18" />
                            </svg>
                          ) : (
                            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                            </svg>
                          )}
                        </button>
                      </div>
                    </div>
                  )}

                  {/* Extra Options Toggles */}
                  <div className="pt-2 border-t border-[#3E3E3E] flex flex-col space-y-4">
                    <label className="flex items-center cursor-pointer group">
                      <div className="relative">
                        <input
                          type="checkbox"
                          disabled={isCompressing}
                          className="sr-only"
                          checked={archiveToOriginalPath}
                          onChange={() => setArchiveToOriginalPath(!archiveToOriginalPath)}
                        />
                        <div
                          className={`block w-10 h-6 rounded-full transition-colors ${archiveToOriginalPath ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                            }`}
                        ></div>
                        <div
                          className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${archiveToOriginalPath ? "transform translate-x-4" : ""
                            }`}
                        ></div>
                      </div>
                      <div className="ml-3 text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                        Archive to original path
                      </div>
                    </label>

                    {!archiveToOriginalPath && (
                      <div className="pl-9 space-y-1.5 animate-fadeIn">
                        <label className="block text-xs font-medium text-[#C8C8C8]">
                          Custom Destination Directory
                        </label>
                        <div className="flex space-x-2">
                          <input
                            type="text"
                            disabled={isCompressing}
                            value={compressDestDir}
                            onChange={(e) => setCompressDestDir(e.target.value)}
                            placeholder="Select output folder (leave blank for source directory)..."
                            className="flex-1 bg-[#202020] text-sm text-white px-3 py-2 rounded-md border border-[#3E3E3E] focus:border-[#60CDFF] outline-none font-mono text-xs disabled:opacity-50 disabled:cursor-not-allowed"
                          />
                          <button
                            type="button"
                            disabled={isCompressing}
                            onClick={handleSelectCompressDestination}
                            className="text-xs font-medium px-4 py-2 bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#60CDFF] text-white rounded-md transition-colors cursor-pointer shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            Browse...
                          </button>
                        </div>
                      </div>
                    )}

                    <label className="flex items-center cursor-pointer group">
                      <div className="relative">
                        <input
                          type="checkbox"
                          disabled={isCompressing}
                          className="sr-only"
                          checked={separateArchives}
                          onChange={() => setSeparateArchives(!separateArchives)}
                        />
                        <div
                          className={`block w-10 h-6 rounded-full transition-colors ${separateArchives ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                            }`}
                        ></div>
                        <div
                          className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${separateArchives ? "transform translate-x-4" : ""
                            }`}
                        ></div>
                      </div>
                      <div className="ml-3 text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                        Add to separate archives
                      </div>
                    </label>
                  </div>
                </div>
              )}

              {/* Error Banner */}
              {compressError && (
                <div className="bg-red-950/50 border border-red-800 text-red-300 p-4 rounded-lg text-sm">
                  <span className="font-semibold">Compression failed: </span>
                  {compressError}
                </div>
              )}

              {/* Execute Button */}
              <div className="flex justify-end pt-2">
                <button
                  type="button"
                  disabled={isCancellingCompress}
                  onClick={isCompressing ? handleCancelCompress : handleCompress}
                  className={`px-8 py-2.5 font-semibold rounded-md shadow-sm transition-all text-sm flex items-center space-x-2 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed ${isCompressing
                      ? "bg-red-950/40 hover:bg-red-900/60 text-[#FF99A4] hover:text-[#FFB3BA] border border-red-800/60 hover:border-red-600"
                      : "bg-[#60CDFF] hover:bg-[#4FB8E6] text-black"
                    }`}
                  title={isCompressing ? "Click to cancel compression" : undefined}
                >
                  {isCompressing && (
                    <svg className="animate-spin -ml-1 mr-2 h-4 w-4 text-[#FF99A4]" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                  )}
                  <span>
                    {isCompressing
                      ? isCancellingCompress
                        ? "Cancelling..."
                        : "Cancel Compression"
                      : selectedFiles.length > 0
                        ? "Compress Now"
                        : "Select Files to Compress"}
                  </span>
                </button>
              </div>
            </>
          )}

          {/* ======================================================== */}
          {/* EXTRACT MODE CONTENT                                      */}
          {/* ======================================================== */}
          {activeMode === "extract" && (
            <div className="space-y-6">
              {/* Archive Selection Area */}
              {selectedArchive && archiveInfo ? (
                <div className="bg-[#2D2D2D] border border-[#3E3E3E] rounded-lg p-5 flex flex-col space-y-4">
                  <div className="flex justify-between items-center text-xs border-b border-[#3E3E3E] pb-3">
                    <span className="text-xs uppercase tracking-wider font-semibold text-[#60CDFF]">
                      Archive Selected
                    </span>
                    <button
                      type="button"
                      disabled={isExtracting}
                      onClick={() => {
                        if (isExtracting) return;
                        setSelectedArchive(null);
                        setArchiveInfo(null);
                        setExtractResult(null);
                      }}
                      className="text-red-400 hover:text-red-300 transition-colors cursor-pointer text-xs disabled:opacity-50 disabled:cursor-not-allowed"
                      title={isExtracting ? "Extraction in progress" : undefined}
                    >
                      Clear
                    </button>
                  </div>

                  <div className="flex items-center space-x-4 bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
                    <div className="relative w-12 h-12 rounded-xl bg-[#142330] border border-[#20495f] flex items-center justify-center shrink-0 shadow">
                      <img
                        src="/bayzip-logo.png"
                        alt="Archive"
                        className="w-7 h-7 object-contain"
                        onError={(e) => {
                          (e.currentTarget as HTMLElement).style.display = "none";
                        }}
                      />
                      <span className="absolute -bottom-1 -right-1 bg-[#60CDFF] text-black text-[8px] font-black font-mono px-1 rounded uppercase">
                        {archiveInfo.format.replace(".", "").toUpperCase() || "ZIP"}
                      </span>
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="text-sm font-semibold text-white truncate" title={selectedArchive}>
                        {archiveInfo.filename}
                      </p>
                      <div className="flex items-center space-x-2 text-xs text-[#A8A8A8] mt-0.5">
                        <span className="font-mono text-[#60CDFF]">{formatBytes(archiveInfo.size)}</span>
                        <span>•</span>
                        <span className="bg-[#2D2D2D] px-2 py-0.5 rounded text-[11px] text-[#C8C8C8] border border-[#3E3E3E]">
                          {archiveInfo.format}
                        </span>
                        {archiveInfo.is_encrypted && (
                          <span className="bg-amber-950/60 text-amber-300 text-[10px] px-2 py-0.5 rounded border border-amber-800/80 font-medium flex items-center space-x-1">
                            <svg className="w-3 h-3 text-amber-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                            </svg>
                            <span>Encrypted</span>
                          </span>
                        )}
                      </div>
                    </div>
                    <button
                      type="button"
                      disabled={isExtracting}
                      onClick={handleSelectArchive}
                      className="text-xs px-3 py-1.5 bg-[#2D2D2D] hover:bg-[#383838] border border-[#3E3E3E] text-white rounded-md transition-colors cursor-pointer shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
                      title={isExtracting ? "Extraction in progress" : undefined}
                    >
                      Change
                    </button>
                  </div>

                  {/* Hierarchical Tree-Type Archive Directory Inspector */}
                  <ArchiveTreeInspector
                    archiveInfo={archiveInfo}
                    selectedArchive={selectedArchive}
                    extractPassword={extractPassword}
                    isLoading={isInspectingArchive}
                    onUnlockPreview={() => selectArchive(selectedArchive, extractPassword.trim())}
                    onPreviewEntry={handlePreviewEntry}
                  />
                </div>
              ) : (
                <div
                  onClick={handleSelectArchive}
                  className="bg-[#2D2D2D] border border-dashed border-[#3E3E3E] hover:border-[#60CDFF] rounded-lg p-8 text-center transition-colors flex flex-col items-center justify-center space-y-4 cursor-pointer group"
                >
                  <div className="w-12 h-12 rounded-full bg-[#202020] border border-[#3E3E3E] group-hover:border-[#60CDFF] group-hover:shadow-[0_0_15px_rgba(96,205,255,0.25)] flex items-center justify-center text-[#60CDFF] transition-all duration-200">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-6 h-6">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={1.8}
                        d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"
                      />
                    </svg>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-white mb-1">Drag & drop archive to uncompress</p>
                    <p className="text-xs text-[#A8A8A8]">
                      Supports <span className="text-[#60CDFF]">.bay</span>, <span className="text-[#60CDFF]">.bay_gamer</span>, <span className="text-[#60CDFF]">.zip</span>, <span className="text-[#60CDFF]">.7z</span>, <span className="text-[#60CDFF]">.rar</span>, and <span className="text-[#60CDFF]">.tar</span>
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleSelectArchive();
                    }}
                    className="text-xs font-medium px-4 py-2 bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#60CDFF] text-white rounded-md transition-colors cursor-pointer"
                  >
                    Browse Archive
                  </button>
                </div>
              )}

              {/* Extraction Destination & Options */}
              <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6 space-y-4">
                <div className="flex justify-between items-center">
                  <h3 className="text-sm font-medium text-white">Extraction Destination</h3>
                </div>

                <div>
                  <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">Destination Directory</label>
                  <div className="flex space-x-2">
                    <input
                      type="text"
                      disabled={isExtracting}
                      value={extractDest}
                      onChange={(e) => setExtractDest(e.target.value)}
                      placeholder="Destination folder path..."
                      className="flex-1 bg-[#202020] text-sm text-white px-3 py-2 rounded-md border border-[#3E3E3E] focus:border-[#60CDFF] outline-none font-mono text-xs disabled:opacity-50 disabled:cursor-not-allowed"
                    />
                    <button
                      type="button"
                      disabled={isExtracting}
                      onClick={handleSelectDestFolder}
                      className="text-xs font-medium px-4 py-2 bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#60CDFF] text-white rounded-md transition-colors cursor-pointer shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      Browse...
                    </button>
                  </div>
                </div>

                <div>
                  <div className="flex items-center justify-between mb-1.5">
                    <label className="text-xs font-medium text-[#C8C8C8]">
                      Archive Password
                    </label>
                    {archiveInfo?.is_encrypted ? (
                      <span className="text-[11px] font-semibold text-amber-400 bg-amber-950/40 border border-amber-800/60 px-2.5 py-0.5 rounded-full flex items-center space-x-1.5">
                        <svg className="w-3 h-3 text-amber-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                        </svg>
                        <span>Password Required</span>
                      </span>
                    ) : (
                      <span className="text-[11px] text-emerald-400 font-normal">
                        Unencrypted (No password needed)
                      </span>
                    )}
                  </div>
                  <div className="relative">
                    <input
                      type={showExtractPassword ? "text" : "password"}
                      disabled={isExtracting}
                      value={extractPassword}
                      onChange={(e) => setExtractPassword(e.target.value)}
                      placeholder={
                        archiveInfo?.is_encrypted
                          ? "Enter password to decrypt..."
                          : "Leave blank (unencrypted)"
                      }
                      className={`w-full bg-[#202020] text-sm text-white pl-3 pr-10 py-2 rounded-md border outline-none font-mono text-xs placeholder-[#666] transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${archiveInfo?.is_encrypted
                          ? "border-amber-500/60 focus:border-amber-400 focus:ring-1 focus:ring-amber-400/30"
                          : "border-[#3E3E3E] focus:border-[#60CDFF]"
                        }`}
                    />
                    <button
                      type="button"
                      disabled={isExtracting}
                      onClick={() => setShowExtractPassword(!showExtractPassword)}
                      className="absolute inset-y-0 right-0 pr-3 flex items-center text-[#888] hover:text-white cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                      title={showExtractPassword ? "Hide password" : "Show password"}
                    >
                      {showExtractPassword ? (
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l18 18" />
                        </svg>
                      ) : (
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                        </svg>
                      )}
                    </button>
                  </div>
                </div>

                <div className="pt-2 border-t border-[#3E3E3E] flex flex-col space-y-3">
                  <label className="flex items-center cursor-pointer group">
                    <div className="relative">
                      <input
                        type="checkbox"
                        disabled={isExtracting}
                        className="sr-only"
                        checked={extractToSubfolder}
                        onChange={() => setExtractToSubfolder(!extractToSubfolder)}
                      />
                      <div
                        className={`block w-10 h-6 rounded-full transition-colors ${extractToSubfolder ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                          }`}
                      ></div>
                      <div
                        className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${extractToSubfolder ? "transform translate-x-4" : ""
                          }`}
                      ></div>
                    </div>
                    <div className="ml-3 text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                      Extract into dedicated folder{" "}
                      <span className="text-[#60CDFF] font-mono text-xs">
                        ({archiveInfo?.filename
                          ? getArchiveStemName(archiveInfo.filename)
                          : selectedArchive
                            ? getArchiveStemName(selectedArchive.split(/[/\\]/).pop() || "")
                            : "folder_name"})
                      </span>
                    </div>
                  </label>

                  <label className="flex items-center cursor-pointer group">
                    <div className="relative">
                      <input
                        type="checkbox"
                        disabled={isExtracting}
                        className="sr-only"
                        checked={openFolderOnExtract}
                        onChange={() => setOpenFolderOnExtract(!openFolderOnExtract)}
                      />
                      <div
                        className={`block w-10 h-6 rounded-full transition-colors ${openFolderOnExtract ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                          }`}
                      ></div>
                      <div
                        className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${openFolderOnExtract ? "transform translate-x-4" : ""
                          }`}
                      ></div>
                    </div>
                    <div className="ml-3 text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                      Open folder when extraction completes
                    </div>
                  </label>
                </div>
              </div>

              {/* Extraction Progress Card */}
              {isExtracting && (
                <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-4 shadow-lg space-y-3">
                  <div className="flex justify-between items-center text-xs">
                    <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                      <span className="w-2.5 h-2.5 rounded-full bg-[#60CDFF] animate-pulse shrink-0"></span>
                      <span className="text-[#D0D0D0] truncate font-medium">
                        {extractProgress?.current_file ? `Extracting: ${extractProgress.current_file}` : "Decompressing archive..."}
                      </span>
                    </div>
                    <div className="flex items-center space-x-2.5 shrink-0">
                      <span className="text-sm font-semibold text-[#60CDFF] font-mono">
                        {extractProgress ? Math.round(extractProgress.percent) : 0}%
                      </span>
                      <button
                        type="button"
                        onClick={handleCancelExtract}
                        disabled={isCancellingExtract}
                        className="w-7 h-7 flex items-center justify-center text-[#FF99A4] hover:text-white bg-red-950/40 hover:bg-red-900/70 border border-red-800/60 hover:border-red-600 rounded-md transition-all cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-sm group"
                        title="Cancel Extraction"
                      >
                        {isCancellingExtract ? (
                          <svg className="animate-spin h-3.5 w-3.5 text-[#FF99A4]" fill="none" viewBox="0 0 24 24">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                          </svg>
                        ) : (
                          <svg className="w-3.5 h-3.5 group-hover:scale-110 transition-transform" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
                          </svg>
                        )}
                      </button>
                    </div>
                  </div>

                  <div className="w-full h-2 bg-[#202020] rounded-full overflow-hidden border border-[#3E3E3E]">
                    <div
                      className="h-full bg-gradient-to-r from-[#0078D4] to-[#60CDFF] transition-all duration-150 ease-out rounded-full shadow-[0_0_10px_rgba(96,205,255,0.7)]"
                      style={{ width: `${extractProgress ? Math.max(2, extractProgress.percent) : 2}%` }}
                    ></div>
                  </div>

                  <div className="flex justify-between items-center text-[11px] text-[#8A8A8A]">
                    <span>
                      {extractProgress && extractProgress.bytes_extracted > 0 ? (
                        archiveInfo && (archiveInfo.total_uncompressed_size ?? 0) > 0 ? (
                          `${formatBytes(extractProgress.bytes_extracted)} / ${formatBytes(archiveInfo.total_uncompressed_size!)}`
                        ) : (
                          `${formatBytes(extractProgress.bytes_extracted)} uncompressed`
                        )
                      ) : (
                        "High-speed decompression active"
                      )}
                    </span>
                    <span className="text-[#60CDFF] font-medium">
                      {archiveInfo?.format || "Archive"}
                    </span>
                  </div>
                </div>
              )}

              {/* Error Banner */}
              {extractError && (
                <div className="bg-red-950/50 border border-red-800 text-red-300 p-4 rounded-lg text-sm">
                  <span className="font-semibold">Extraction failed: </span>
                  {extractError}
                </div>
              )}

              {/* Extract Action Button */}
              <div className="flex justify-end pt-2">
                <button
                  type="button"
                  disabled={isExtracting && isCancellingExtract}
                  onClick={isExtracting ? handleCancelExtract : handleExtract}
                  className={`px-8 py-2.5 font-semibold rounded-md shadow-sm transition-all text-sm flex items-center space-x-2 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed ${isExtracting
                      ? "bg-red-950/40 hover:bg-red-900/60 text-[#FF99A4] hover:text-[#FFB3BA] border border-red-800/60 hover:border-red-600"
                      : "bg-[#60CDFF] hover:bg-[#4FB8E6] text-black"
                    }`}
                  title={isExtracting ? "Click to cancel extraction" : undefined}
                >
                  {isExtracting && (
                    <svg className="animate-spin -ml-1 mr-2 h-4 w-4 text-[#FF99A4]" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                  )}
                  <span>
                    {isExtracting
                      ? isCancellingExtract
                        ? "Cancelling..."
                        : "Cancel Extraction"
                      : selectedArchive
                        ? "Extract Now"
                        : "Select Archive to Extract"}
                  </span>
                </button>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Result Modals (Compression & Extraction) */}
      <ResultModals
        compressResult={compressResult}
        onCloseCompressResult={() => {
          setCompressResult(null);
          setSelectedFiles([]);
          setCustomArchiveName("");
        }}
        extractResult={extractResult}
        archiveInfo={archiveInfo}
        onCloseExtractResult={() => {
          setExtractResult(null);
          setSelectedArchive(null);
          setArchiveInfo(null);
        }}
        formatBytes={formatBytes}
      />

      {/* About Modal */}
      <AboutModal
        isOpen={isAboutOpen}
        onClose={() => setIsAboutOpen(false)}
      />

      {/* Settings Modal (Draggable Floating Window) */}
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        isSyncingRegistry={isSyncingRegistry}
        contextMenuStatus={contextMenuStatus}
        onRegisterContextMenu={handleRegisterContextMenu}
      />

      {/* Password Prompt Modal for Encrypted Archives */}
      <PasswordModal
        isOpen={isPasswordPromptOpen}
        onClose={() => setIsPasswordPromptOpen(false)}
        password={extractPassword}
        setPassword={setExtractPassword}
        showPassword={showExtractPassword}
        setShowPassword={setShowExtractPassword}
        isExtracting={isExtracting}
        extractError={extractError}
        onSubmit={handleExtract}
      />

      {/* Safe In-Memory File Preview Modal */}
      <FilePreviewModal
        isOpen={isPreviewOpen}
        onClose={() => {
          setIsPreviewOpen(false);
          setPreviewData(null);
          setPreviewError(null);
        }}
        preview={previewData}
        isLoading={isPreviewLoading}
        error={previewError}
        archivePath={selectedArchive || ""}
      />
    </div>
  );
}

export default App;

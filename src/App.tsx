import { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface SystemSpecs {
  total_ram_gb: number;
  free_ram_gb: number;
  cpu_cores: number;
}

interface CompressProgress {
  percent: number;
  current_file: string;
  bytes_processed: number;
  total_bytes: number;
}

interface CompressResult {
  success: boolean;
  output_path: string;
  original_size: number;
  compressed_size: number;
  elapsed_ms: number;
  error?: string;
}

interface SelectedItemInfo {
  path: string;
  name: string;
  size: number;
  is_dir: boolean;
}

interface ExtractProgress {
  percent: number;
  current_file: string;
  bytes_extracted: number;
}

interface ExtractResult {
  success: boolean;
  output_directory: string;
  file_count: number;
  elapsed_ms: number;
  error?: string;
}

interface ArchiveInfo {
  path: string;
  filename: string;
  size: number;
  format: string;
  is_archive: boolean;
}

const FORMAT_ALGORITHMS: Record<string, string[]> = {
  ".bay": ["Zstandard (Recommended)", "Brotli", "LZ4 (Fastest)"],
  ".bay_gamer": ["Zstd-Fast (Auto)"],
  ".7z": ["LZMA2", "LZMA", "BZip2", "PPMd"],
  ".zip": ["Deflate", "Deflate64", "BZip2", "LZMA"],
  ".tar": ["None (Store)"],
};

const ARCHIVE_EXTS = [".bay", ".bay_gamer", ".zip", ".7z", ".tar", ".gz", ".tgz", ".rar"];

function isArchiveFile(filePath: string): boolean {
  const lower = filePath.toLowerCase();
  return ARCHIVE_EXTS.some((ext) => lower.endsWith(ext));
}

function App() {
  // Navigation Mode: "compress" or "extract"
  const [activeMode, setActiveMode] = useState<"compress" | "extract">("compress");

  // System Specs
  const [specs, setSpecs] = useState<SystemSpecs | null>(null);
  const [sliderValue, setSliderValue] = useState<number>(50); // 0, 25, 50, 75, 100
  const [isAdvanced, setIsAdvanced] = useState(false);

  // Compress Options & Naming
  const [customArchiveName, setCustomArchiveName] = useState<string>("");
  const [selectedFormat, setSelectedFormat] = useState(".bay");
  const [selectedAlgorithm, setSelectedAlgorithm] = useState(FORMAT_ALGORITHMS[".bay"][0]);
  const [selectedEncryption, setSelectedEncryption] = useState("none");
  const [archiveToOriginalPath, setArchiveToOriginalPath] = useState(true);
  const [separateArchives, setSeparateArchives] = useState(false);
  const [selectedFiles, setSelectedFiles] = useState<string[]>([]);
  const [itemsInfo, setItemsInfo] = useState<Record<string, { size: number; is_dir: boolean }>>({});
  const [isAboutOpen, setIsAboutOpen] = useState(false);
  const [contextMenuStatus, setContextMenuStatus] = useState<string | null>(null);

  // Compression execution state
  const [isCompressing, setIsCompressing] = useState(false);
  const [progress, setProgress] = useState<CompressProgress | null>(null);
  const [compressResult, setCompressResult] = useState<CompressResult | null>(null);
  const [compressError, setCompressError] = useState<string | null>(null);

  // Extraction State
  const [selectedArchive, setSelectedArchive] = useState<string | null>(null);
  const [archiveInfo, setArchiveInfo] = useState<ArchiveInfo | null>(null);
  const [extractDest, setExtractDest] = useState<string>("");
  const [extractToSubfolder, setExtractToSubfolder] = useState<boolean>(true);
  const [openFolderOnExtract, setOpenFolderOnExtract] = useState<boolean>(true);
  const [isExtracting, setIsExtracting] = useState<boolean>(false);
  const [extractProgress, setExtractProgress] = useState<ExtractProgress | null>(null);
  const [extractResult, setExtractResult] = useState<ExtractResult | null>(null);
  const [extractError, setExtractError] = useState<string | null>(null);

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
      const stem = first.split(/[/\\]/).pop()?.replace(/\.[^/.]+$/, "") || "bayzip_archive";
      setCustomArchiveName(stem);
    }
  }, [selectedFiles]);

  const totalSelectedSize = useMemo(() => {
    return selectedFiles.reduce((acc, path) => acc + (itemsInfo[path]?.size || 0), 0);
  }, [selectedFiles, itemsInfo]);

  // Select Archive Handler (with metadata inquiry)
  const selectArchive = async (filePath: string) => {
    setSelectedArchive(filePath);
    setExtractResult(null);
    setExtractError(null);
    setExtractProgress(null);

    // Parent directory default
    const parentDir = filePath.replace(/[/\\][^/\\]+$/, "");
    setExtractDest(parentDir || ".");

    try {
      const info = await invoke<ArchiveInfo>("get_archive_info", { path: filePath });
      setArchiveInfo(info);
    } catch (err) {
      console.error("Failed to inspect archive info:", err);
      const filename = filePath.split(/[/\\]/).pop() || "archive";
      setArchiveInfo({
        path: filePath,
        filename,
        size: 0,
        format: "Archive",
        is_archive: true,
      });
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
    invoke<string[]>("get_cli_args")
      .then((args) => {
        if (isMounted && args && args.length > 0) {
          if (args.length === 1 && isArchiveFile(args[0])) {
            setActiveMode("extract");
            selectArchive(args[0]);
          } else {
            setSelectedFiles((prev) => [...new Set([...prev, ...args])]);
          }
        }
      })
      .catch(() => {});

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
    if (sliderValue === 0) return "0.0";
    if (!specs) return "0.5";
    const maxSafeRam = Math.max(0.5, specs.free_ram_gb - 1.5);
    return ((sliderValue / 100) * maxSafeRam).toFixed(1);
  };

  const getEstimatedCores = () => {
    if (sliderValue === 0) return 1;
    if (!specs) return 1;
    return Math.max(1, Math.round((sliderValue / 100) * specs.cpu_cores));
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
    try {
      const selected = await open({
        multiple: false,
        title: "Select Archive to Extract",
        filters: [
          {
            name: "Supported Archives",
            extensions: ["bay", "zip", "7z", "tar", "gz", "tgz", "rar"],
          },
          {
            name: "Bayzip Archives (*.bay)",
            extensions: ["bay"],
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

  const handleCompress = async () => {
    if (selectedFiles.length === 0) {
      await handleSelectFiles();
      return;
    }

    setIsCompressing(true);
    setCompressError(null);
    setProgress({
      percent: 0,
      current_file: "Initializing compression...",
      bytes_processed: 0,
      total_bytes: 0,
    });

    try {
      const res = await invoke<CompressResult>("compress_files", {
        payload: {
          files: selectedFiles,
          format: selectedFormat,
          algorithm: selectedAlgorithm,
          slider_value: sliderValue,
          custom_name: customArchiveName.trim() || undefined,
        },
      });
      setCompressResult(res);
    } catch (err) {
      console.error("Compression execution error:", err);
      setCompressError(String(err));
    } finally {
      setIsCompressing(false);
      setProgress(null);
    }
  };

  const handleExtract = async () => {
    if (!selectedArchive) {
      await handleSelectArchive();
      return;
    }

    setIsExtracting(true);
    setExtractError(null);
    setExtractProgress({
      percent: 0,
      current_file: "Preparing extraction...",
      bytes_extracted: 0,
    });

    try {
      let finalDest = extractDest;
      if (extractToSubfolder) {
        const stem = archiveInfo?.filename?.replace(/\.[^/.]+$/, "") || "extracted";
        finalDest = `${extractDest}\\${stem}`;
      }

      const res = await invoke<ExtractResult>("extract_archive", {
        archivePath: selectedArchive,
        destination: finalDest,
      });

      setExtractResult(res);

      if (openFolderOnExtract && res.output_directory) {
        revealItemInDir(res.output_directory).catch(() => {});
      }
    } catch (err) {
      console.error("Extraction execution error:", err);
      setExtractError(String(err));
    } finally {
      setIsExtracting(false);
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
    if (sliderValue === 0) return "STORE (No Compression)";
    if (sliderValue === 25) return "FAST";
    if (sliderValue === 50) return "BALANCED";
    if (sliderValue === 75) return "HIGH COMPRESSION";
    return "MAX COMPRESSION";
  };

  return (
    <div className="h-screen w-screen bg-[#202020] text-white flex flex-col font-sans select-none overflow-hidden">
      {/* Windows 11 Native Custom Titlebar - Full Width & Height Grabbable (Native Drag Region) */}
      <div
        data-tauri-drag-region
        onDoubleClick={(e) => {
          if (!(e.target as HTMLElement).closest("button")) {
            getCurrentWindow().toggleMaximize().catch(() => {});
          }
        }}
        className="h-11 w-full flex justify-between items-center bg-[#202020] cursor-default select-none border-b border-[#2D2D2D] shrink-0"
      >
        {/* Left Branding Area - Fully Grabbable */}
        <div data-tauri-drag-region className="flex items-center space-x-3 px-4 cursor-default">
          <div data-tauri-drag-region className="w-4 h-4 text-[#60CDFF] flex items-center justify-center pointer-events-none">
            {/* Simple Zip Icon */}
            <svg viewBox="0 0 24 24" fill="currentColor" className="w-full h-full pointer-events-none">
              <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2h12c1.1 0 2-.9 2-2V8l-6-6zm-2 14h-2v-2h2v2zm0-4h-2v-2h2v2zm0-4h-2V6h2v2z" />
            </svg>
          </div>
          <span data-tauri-drag-region className="text-xs font-normal text-white tracking-wide pointer-events-none">
            Bayzip
          </span>
        </div>

        {/* Dedicated Draggable Center Surface - Covers entire expanse */}
        <div data-tauri-drag-region className="flex-1 h-full cursor-default" />

        {/* Window Controls */}
        <div className="flex h-full" data-tauri-drag-region="false">
          {/* Windows Minimize */}
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              getCurrentWindow().minimize().catch((err) => console.error("Minimize error:", err));
            }}
            className="h-full px-4 hover:bg-[#333333] flex items-center justify-center text-[#A8A8A8] hover:text-white transition-colors cursor-pointer"
            title="Minimize"
          >
            <svg width="10" height="1" viewBox="0 0 10 1">
              <rect width="10" height="1" fill="currentColor" />
            </svg>
          </button>
          {/* Windows Maximize */}
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              getCurrentWindow().toggleMaximize().catch((err) => console.error("Maximize error:", err));
            }}
            className="h-full px-4 hover:bg-[#333333] flex items-center justify-center text-[#A8A8A8] hover:text-white transition-colors cursor-pointer"
            title="Maximize"
          >
            <svg width="10" height="10" viewBox="0 0 10 10">
              <path d="M0,0v10h10V0H0z M9,9H1V1h8V9z" fill="currentColor" />
            </svg>
          </button>
          {/* Windows Close */}
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              getCurrentWindow().close().catch((err) => console.error("Close error:", err));
            }}
            className="h-full px-4 hover:bg-[#E81123] hover:text-white flex items-center justify-center text-[#A8A8A8] transition-colors cursor-pointer"
            title="Close"
          >
            <svg width="10" height="10" viewBox="0 0 10 10">
              <path d="M10,1L9,0L5,4L1,0L0,1l4,4l-4,4l1,1l4-4l4,4l1-1l-4-4L10,1z" fill="currentColor" />
            </svg>
          </button>
        </div>
      </div>

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
                  className={`flex items-center space-x-2 px-3.5 py-1.5 rounded-md text-xs font-medium transition-all cursor-pointer ${
                    activeMode === "compress"
                      ? "bg-[#2D2D2D] text-white shadow-sm border border-[#3E3E3E]"
                      : "text-[#888888] hover:text-white"
                  }`}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-[#60CDFF]">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                  </svg>
                  <span>Compress</span>
                </button>
                <button
                  type="button"
                  onClick={() => setActiveMode("extract")}
                  className={`flex items-center space-x-2 px-3.5 py-1.5 rounded-md text-xs font-medium transition-all cursor-pointer ${
                    activeMode === "extract"
                      ? "bg-[#2D2D2D] text-white shadow-sm border border-[#3E3E3E]"
                      : "text-[#888888] hover:text-white"
                  }`}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-[#60CDFF]">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" />
                  </svg>
                  <span>Extract</span>
                </button>
              </div>

              <span className="text-xl font-semibold tracking-tight text-white hidden sm:inline">
                {activeMode === "compress" ? "Create Archive" : "Extract Archive"}
              </span>
            </div>

            <div className="flex space-x-3">
              <button
                type="button"
                onClick={() => setIsAboutOpen(true)}
                className="text-sm font-medium px-4 py-1.5 rounded text-[#A8A8A8] hover:bg-[#333333] hover:text-white transition-colors cursor-pointer"
              >
                About
              </button>
              {activeMode === "compress" && (
                <button
                  type="button"
                  onClick={() => setIsAdvanced(!isAdvanced)}
                  className="text-sm font-medium px-4 py-1.5 rounded bg-[#333333] hover:bg-[#3E3E3E] border border-[#3E3E3E] transition-colors cursor-pointer"
                >
                  {isAdvanced ? "Simple Mode" : "Advanced Options"}
                </button>
              )}
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
                    </div>
                    <button
                      type="button"
                      onClick={() => {
                        setSelectedFiles([]);
                        setCustomArchiveName("");
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

                  <div className="flex space-x-3 pt-1">
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
                  </div>
                </div>
              ) : (
                <div className="bg-[#2D2D2D] border border-dashed border-[#3E3E3E] hover:border-[#60CDFF] rounded-lg p-8 text-center transition-colors flex flex-col items-center justify-center space-y-4">
                  <div className="w-12 h-12 rounded-full bg-[#202020] border border-[#3E3E3E] flex items-center justify-center text-[#60CDFF]">
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
                <div className="flex items-center space-x-2">
                  <input
                    type="text"
                    value={customArchiveName}
                    onChange={(e) => setCustomArchiveName(e.target.value)}
                    placeholder={selectedFiles.length > 0 ? "Archive name..." : "Enter archive name..."}
                    className="flex-1 bg-[#202020] text-sm text-white px-3 py-2 rounded-md border border-[#3E3E3E] focus:border-[#60CDFF] outline-none font-medium placeholder-[#666]"
                  />
                  <span className="bg-[#202020] px-3 py-2 rounded-md border border-[#3E3E3E] text-xs font-mono text-[#60CDFF] shrink-0 font-semibold">
                    {selectedFormat}
                  </span>
                </div>
              </div>

              {/* Hardware Configuration - Fluent Design Card */}
              <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6">
                <div className="flex justify-between items-center mb-6">
                  <h3 className="text-sm font-medium">Hardware Allocation</h3>
                  <span className="text-xs bg-[#3E3E3E] text-[#A8A8A8] px-2 py-1 rounded-md">
                    HARDWARE PROFILING ACTIVE
                  </span>
                </div>

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
                      value={sliderValue}
                      onChange={(e) => setSliderValue(Number(e.target.value))}
                      className="w-full h-1 bg-[#3E3E3E] rounded-lg appearance-none cursor-pointer accent-[#60CDFF] relative z-10"
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
                      Dynamic safe cap ({specs ? specs.free_ram_gb.toFixed(1) : "?"} GB available)
                    </p>
                  </div>

                  <div className="bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
                    <div className="flex justify-between items-start mb-1">
                      <p className="text-xs text-[#A8A8A8]">Target CPU Threads</p>
                      {specs && (
                        <span className="text-[10px] text-[#60CDFF] bg-[#1a3847] px-1.5 py-0.5 rounded font-mono">
                          {specs.cpu_cores} Cores
                        </span>
                      )}
                    </div>
                    <p className="text-lg font-medium">
                      {getEstimatedCores()}{" "}
                      <span className="text-sm font-normal text-[#A8A8A8]">/ {specs?.cpu_cores || "?"}</span>
                    </p>
                    <p className="text-[11px] text-[#7A7A7A] mt-1">
                      {sliderValue === 100 ? "100% Full Multithreading" : `${sliderValue}% Thread Allocation`}
                    </p>
                  </div>
                </div>
              </div>

              {/* Advanced Options Toggle */}
              {isAdvanced && (
                <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6 space-y-5">
                  <div className="grid grid-cols-1 sm:grid-cols-12 gap-3.5">
                    <div className={selectedFormat === ".bay_gamer" ? "sm:col-span-6" : "sm:col-span-3"}>
                      <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">Format</label>
                      <div className="relative group">
                        <select
                          value={selectedFormat}
                          onChange={(e) => setSelectedFormat(e.target.value)}
                          className="w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none cursor-pointer text-white truncate transition-colors"
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
                    {selectedFormat !== ".bay_gamer" && (
                      <div className="sm:col-span-5">
                        <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">Algorithm</label>
                        <div className="relative group">
                          <select
                            value={selectedAlgorithm}
                            onChange={(e) => setSelectedAlgorithm(e.target.value)}
                            className="w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none cursor-pointer text-white truncate transition-colors"
                          >
                            {FORMAT_ALGORITHMS[selectedFormat].map((algo) => (
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

                    <div className={selectedFormat === ".bay_gamer" ? "sm:col-span-6" : "sm:col-span-4"}>
                      <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">Encryption</label>
                      <div className="relative group">
                        <select
                          value={selectedEncryption}
                          onChange={(e) => setSelectedEncryption(e.target.value)}
                          className="w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none cursor-pointer text-white truncate transition-colors"
                        >
                          <option value="none">None</option>
                          <option value="aes-256">AES-256-GCM</option>
                          <option value="chacha20">ChaCha20-Poly1305</option>
                          <option value="twofish">Twofish</option>
                        </select>
                        <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-[#888888] group-hover:text-white transition-colors">
                          <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6l4 4 4-4" />
                          </svg>
                        </div>
                      </div>
                    </div>
                  </div>

                  {selectedEncryption !== "none" && (
                    <div>
                      <label className="block text-xs font-medium mb-2">Password</label>
                      <input
                        type="password"
                        placeholder="Enter secure password"
                        className="w-full bg-[#202020] rounded-md p-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none placeholder-[#A8A8A8]"
                      />
                    </div>
                  )}

                  {/* Extra Options Toggles */}
                  <div className="pt-2 border-t border-[#3E3E3E] flex flex-col space-y-4">
                    <label className="flex items-center cursor-pointer group">
                      <div className="relative">
                        <input
                          type="checkbox"
                          className="sr-only"
                          checked={archiveToOriginalPath}
                          onChange={() => setArchiveToOriginalPath(!archiveToOriginalPath)}
                        />
                        <div
                          className={`block w-10 h-6 rounded-full transition-colors ${
                            archiveToOriginalPath ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                          }`}
                        ></div>
                        <div
                          className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${
                            archiveToOriginalPath ? "transform translate-x-4" : ""
                          }`}
                        ></div>
                      </div>
                      <div className="ml-3 text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                        Archive to original path
                      </div>
                    </label>

                    <label className="flex items-center cursor-pointer group">
                      <div className="relative">
                        <input
                          type="checkbox"
                          className="sr-only"
                          checked={separateArchives}
                          onChange={() => setSeparateArchives(!separateArchives)}
                        />
                        <div
                          className={`block w-10 h-6 rounded-full transition-colors ${
                            separateArchives ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                          }`}
                        ></div>
                        <div
                          className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${
                            separateArchives ? "transform translate-x-4" : ""
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

              {/* Active Compression Progress Card */}
              {isCompressing && (
                <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-4 shadow-lg space-y-3">
                  <div className="flex justify-between items-center text-xs">
                    <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                      <span className="w-2.5 h-2.5 rounded-full bg-[#60CDFF] animate-pulse shrink-0"></span>
                      <span className="text-[#D0D0D0] truncate font-medium">
                        {progress?.current_file ? `Compressing: ${progress.current_file}` : "Initializing compression..."}
                      </span>
                    </div>
                    <span className="text-sm font-semibold text-[#60CDFF] font-mono shrink-0">
                      {progress ? Math.round(progress.percent) : 0}%
                    </span>
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
                      {selectedAlgorithm} • {selectedFormat}
                    </span>
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
                  disabled={isCompressing}
                  onClick={handleCompress}
                  className={`px-8 py-2.5 bg-[#60CDFF] hover:bg-[#4FB8E6] text-black font-semibold rounded-md shadow-sm transition-all text-sm flex items-center space-x-2 ${
                    isCompressing ? "opacity-75 cursor-not-allowed" : "cursor-pointer"
                  }`}
                >
                  {isCompressing && (
                    <svg className="animate-spin -ml-1 mr-2 h-4 w-4 text-black" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                  )}
                  <span>
                    {isCompressing ? "Compressing..." : selectedFiles.length > 0 ? "Compress Now" : "Select Files to Compress"}
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
                      onClick={() => {
                        setSelectedArchive(null);
                        setArchiveInfo(null);
                        setExtractResult(null);
                      }}
                      className="text-red-400 hover:text-red-300 transition-colors cursor-pointer text-xs"
                    >
                      Clear
                    </button>
                  </div>

                  <div className="flex items-center space-x-4 bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
                    <div className="w-10 h-10 rounded-lg bg-[#1a3847] text-[#60CDFF] flex items-center justify-center shrink-0 border border-[#20495f]">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-5 h-5">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" />
                      </svg>
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
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={handleSelectArchive}
                      className="text-xs px-3 py-1.5 bg-[#2D2D2D] hover:bg-[#383838] border border-[#3E3E3E] text-white rounded-md transition-colors cursor-pointer shrink-0"
                    >
                      Change
                    </button>
                  </div>
                </div>
              ) : (
                <div
                  onClick={handleSelectArchive}
                  className="bg-[#2D2D2D] border border-dashed border-[#3E3E3E] hover:border-[#60CDFF] rounded-lg p-8 text-center transition-colors flex flex-col items-center justify-center space-y-4 cursor-pointer group"
                >
                  <div className="w-12 h-12 rounded-full bg-[#202020] border border-[#3E3E3E] group-hover:border-[#60CDFF] flex items-center justify-center text-[#60CDFF] transition-colors">
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
                      Supports <span className="text-[#60CDFF]">.bay</span>, <span className="text-[#60CDFF]">.zip</span>, <span className="text-[#60CDFF]">.7z</span>, <span className="text-[#60CDFF]">.tar</span>, and <span className="text-[#60CDFF]">.tar.gz</span>
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
                      value={extractDest}
                      onChange={(e) => setExtractDest(e.target.value)}
                      placeholder="Destination folder path..."
                      className="flex-1 bg-[#202020] text-sm text-white px-3 py-2 rounded-md border border-[#3E3E3E] focus:border-[#60CDFF] outline-none font-mono text-xs"
                    />
                    <button
                      type="button"
                      onClick={handleSelectDestFolder}
                      className="text-xs font-medium px-4 py-2 bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#60CDFF] text-white rounded-md transition-colors cursor-pointer shrink-0"
                    >
                      Browse...
                    </button>
                  </div>
                </div>

                <div className="pt-2 border-t border-[#3E3E3E] flex flex-col space-y-3">
                  <label className="flex items-center cursor-pointer group">
                    <div className="relative">
                      <input
                        type="checkbox"
                        className="sr-only"
                        checked={extractToSubfolder}
                        onChange={() => setExtractToSubfolder(!extractToSubfolder)}
                      />
                      <div
                        className={`block w-10 h-6 rounded-full transition-colors ${
                          extractToSubfolder ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                        }`}
                      ></div>
                      <div
                        className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${
                          extractToSubfolder ? "transform translate-x-4" : ""
                        }`}
                      ></div>
                    </div>
                    <div className="ml-3 text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                      Extract into dedicated folder{" "}
                      <span className="text-[#60CDFF] font-mono text-xs">
                        ({archiveInfo?.filename ? archiveInfo.filename.replace(/\.[^/.]+$/, "") : "folder_name"})
                      </span>
                    </div>
                  </label>

                  <label className="flex items-center cursor-pointer group">
                    <div className="relative">
                      <input
                        type="checkbox"
                        className="sr-only"
                        checked={openFolderOnExtract}
                        onChange={() => setOpenFolderOnExtract(!openFolderOnExtract)}
                      />
                      <div
                        className={`block w-10 h-6 rounded-full transition-colors ${
                          openFolderOnExtract ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                        }`}
                      ></div>
                      <div
                        className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${
                          openFolderOnExtract ? "transform translate-x-4" : ""
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
                    <span className="text-sm font-semibold text-[#60CDFF] font-mono shrink-0">
                      {extractProgress ? Math.round(extractProgress.percent) : 0}%
                    </span>
                  </div>

                  <div className="w-full h-2 bg-[#202020] rounded-full overflow-hidden border border-[#3E3E3E]">
                    <div
                      className="h-full bg-gradient-to-r from-[#0078D4] to-[#60CDFF] transition-all duration-150 ease-out rounded-full shadow-[0_0_10px_rgba(96,205,255,0.7)]"
                      style={{ width: `${extractProgress ? Math.max(2, extractProgress.percent) : 2}%` }}
                    ></div>
                  </div>

                  <div className="flex justify-between items-center text-[11px] text-[#8A8A8A]">
                    <span>
                      {extractProgress && extractProgress.bytes_extracted > 0
                        ? `${formatBytes(extractProgress.bytes_extracted)} uncompressed`
                        : "High-speed decompression active"}
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
                  disabled={isExtracting}
                  onClick={handleExtract}
                  className={`px-8 py-2.5 bg-[#60CDFF] hover:bg-[#4FB8E6] text-black font-semibold rounded-md shadow-sm transition-all text-sm flex items-center space-x-2 ${
                    isExtracting ? "opacity-75 cursor-not-allowed" : "cursor-pointer"
                  }`}
                >
                  {isExtracting && (
                    <svg className="animate-spin -ml-1 mr-2 h-4 w-4 text-black" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                  )}
                  <span>
                    {isExtracting
                      ? "Extracting..."
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

      {/* Compression Result Modal */}
      {compressResult && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
          <div className="bg-[#202020] border border-[#3E3E3E] rounded-xl p-8 max-w-md w-full shadow-2xl relative space-y-5">
            <div className="flex items-center space-x-3">
              <div className="w-10 h-10 rounded-full bg-emerald-500/20 text-emerald-400 flex items-center justify-center shrink-0">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-6 h-6">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                </svg>
              </div>
              <div>
                <h3 className="text-lg font-semibold text-white">Archive Created Successfully!</h3>
                <p className="text-xs text-[#A8A8A8]">Completed in {compressResult.elapsed_ms}ms</p>
              </div>
            </div>

            <div className="bg-[#2A2A2A] rounded-lg p-4 border border-[#3E3E3E] space-y-2.5">
              <div className="flex justify-between text-xs">
                <span className="text-[#A8A8A8]">Original Size:</span>
                <span className="font-mono text-white">{formatBytes(compressResult.original_size)}</span>
              </div>
              <div className="flex justify-between text-xs">
                <span className="text-[#A8A8A8]">Compressed Size:</span>
                <span className="font-mono text-[#60CDFF] font-semibold">
                  {formatBytes(compressResult.compressed_size)}
                </span>
              </div>
              {compressResult.original_size > 0 && (
                <div className="flex justify-between text-xs border-t border-[#3E3E3E] pt-2">
                  <span className="text-[#A8A8A8]">Space Saved:</span>
                  <span className="font-semibold text-emerald-400">
                    {Math.max(
                      0,
                      Number(((1 - compressResult.compressed_size / compressResult.original_size) * 100).toFixed(1))
                    )}
                    %
                  </span>
                </div>
              )}
            </div>

            <div className="text-xs bg-[#1A1A1A] p-3 rounded border border-[#333]">
              <span className="text-[#7A7A7A] block mb-1">Archive Path:</span>
              <span className="text-white font-mono break-all">{compressResult.output_path}</span>
            </div>

            <div className="flex space-x-3 justify-end pt-2">
              <button
                type="button"
                onClick={() => revealItemInDir(compressResult.output_path).catch(() => {})}
                className="px-4 py-2 bg-[#333333] hover:bg-[#3E3E3E] border border-[#3E3E3E] text-white text-xs font-medium rounded-md transition-colors cursor-pointer"
              >
                Show in Folder
              </button>
              <button
                type="button"
                onClick={() => {
                  setCompressResult(null);
                  setSelectedFiles([]);
                  setCustomArchiveName("");
                }}
                className="px-6 py-2 bg-[#60CDFF] hover:bg-[#4FB8E6] text-black text-xs font-semibold rounded-md transition-colors cursor-pointer"
              >
                Done
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Extraction Result Modal */}
      {extractResult && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
          <div className="bg-[#202020] border border-[#3E3E3E] rounded-xl p-8 max-w-md w-full shadow-2xl relative space-y-5">
            <div className="flex items-center space-x-3">
              <div className="w-10 h-10 rounded-full bg-emerald-500/20 text-emerald-400 flex items-center justify-center shrink-0">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-6 h-6">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                </svg>
              </div>
              <div>
                <h3 className="text-lg font-semibold text-white">Extraction Complete!</h3>
                <p className="text-xs text-[#A8A8A8]">Completed in {extractResult.elapsed_ms}ms</p>
              </div>
            </div>

            <div className="bg-[#2A2A2A] rounded-lg p-4 border border-[#3E3E3E] space-y-2.5">
              <div className="flex justify-between text-xs">
                <span className="text-[#A8A8A8]">Files Extracted:</span>
                <span className="font-mono text-white font-medium">{extractResult.file_count} item{extractResult.file_count !== 1 ? "s" : ""}</span>
              </div>
              <div className="flex justify-between text-xs">
                <span className="text-[#A8A8A8]">Format:</span>
                <span className="font-mono text-[#60CDFF]">{archiveInfo?.format || "Archive"}</span>
              </div>
            </div>

            <div className="text-xs bg-[#1A1A1A] p-3 rounded border border-[#333]">
              <span className="text-[#7A7A7A] block mb-1">Extracted To:</span>
              <span className="text-white font-mono break-all">{extractResult.output_directory}</span>
            </div>

            <div className="flex space-x-3 justify-end pt-2">
              <button
                type="button"
                onClick={() => revealItemInDir(extractResult.output_directory).catch(() => {})}
                className="px-4 py-2 bg-[#333333] hover:bg-[#3E3E3E] border border-[#3E3E3E] text-white text-xs font-medium rounded-md transition-colors cursor-pointer"
              >
                Open Folder
              </button>
              <button
                type="button"
                onClick={() => {
                  setExtractResult(null);
                  setSelectedArchive(null);
                  setArchiveInfo(null);
                }}
                className="px-6 py-2 bg-[#60CDFF] hover:bg-[#4FB8E6] text-black text-xs font-semibold rounded-md transition-colors cursor-pointer"
              >
                Done
              </button>
            </div>
          </div>
        </div>
      )}

      {/* About Modal - Redesigned Clean Fluent Layout */}
      {isAboutOpen && (
        <div
          onClick={() => setIsAboutOpen(false)}
          className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
        >
          <div
            onClick={(e) => e.stopPropagation()}
            className="bg-[#202020] border border-[#3E3E3E] rounded-xl p-6 sm:p-8 max-w-lg w-full shadow-2xl relative space-y-6"
          >
            <button
              type="button"
              onClick={() => setIsAboutOpen(false)}
              className="absolute top-4 right-4 text-[#A8A8A8] hover:text-white p-1 rounded-md hover:bg-[#333333] transition-colors cursor-pointer"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-5 h-5">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>

            {/* Header */}
            <div className="flex items-center space-x-3.5">
              <div className="w-10 h-10 rounded-lg bg-[#1a3847] text-[#60CDFF] flex items-center justify-center border border-[#20495f] shrink-0">
                <svg viewBox="0 0 24 24" fill="currentColor" className="w-6 h-6">
                  <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2h12c1.1 0 2-.9 2-2V8l-6-6zm-2 14h-2v-2h2v2zm0-4h-2v-2h2v2zm0-4h-2V6h2v2z" />
                </svg>
              </div>
              <div>
                <div className="flex items-center space-x-2">
                  <h2 className="text-xl font-bold tracking-tight text-white">Bayzip</h2>
                  <span className="text-[11px] bg-[#2A2A2A] text-[#60CDFF] px-2 py-0.5 rounded-full border border-[#3E3E3E] font-mono">
                    v0.1.0
                  </span>
                </div>
                <p className="text-xs text-[#A8A8A8] mt-0.5">High-Performance Hardware-Accelerated Archiver</p>
              </div>
            </div>

            <p className="text-xs text-[#C8C8C8] leading-relaxed">
              Engineered with Rust, Zstandard multi-threading, and React. Built to utilize your machine's full CPU cores and RAM for maximum compression and instant extraction.
            </p>

            {/* Creator Card */}
            <div className="bg-[#262626] rounded-lg p-3.5 border border-[#3E3E3E] flex items-center justify-between">
              <div>
                <span className="text-[11px] text-[#60CDFF] font-medium uppercase tracking-wider block">Created By</span>
                <span className="text-sm font-semibold text-white">Made by Baymax</span>
                <span className="text-xs text-[#8A8A8A] block">CEO of DPG (Muhammad Ali)</span>
              </div>
              <div className="text-2xl">⚡</div>
            </div>

            {/* Windows Explorer Integration Card */}
            <div className="bg-[#262626] rounded-lg p-4 border border-[#3E3E3E] space-y-3">
              <div className="flex justify-between items-center">
                <div className="flex items-center space-x-2">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4 text-[#60CDFF]">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16m-7 6h7" />
                  </svg>
                  <span className="text-xs font-semibold text-white">Windows Explorer Integration</span>
                </div>
                <span className="text-[10px] text-emerald-400 bg-emerald-950/50 px-2 py-0.5 rounded border border-emerald-800/60 font-medium">
                  Native Shell
                </span>
              </div>

              <div className="text-[11px] text-[#A8A8A8] space-y-1.5 pl-1">
                <div className="flex items-center space-x-2">
                  <span className="text-[#60CDFF]">•</span>
                  <span><strong>Right-Click:</strong> "Extract to dedicated folder" & "Extract Here"</span>
                </div>
                <div className="flex items-center space-x-2">
                  <span className="text-[#60CDFF]">•</span>
                  <span><strong>Right-Click:</strong> Quick compress to .bay, .zip, .7z, .tar</span>
                </div>
                <div className="flex items-center space-x-2">
                  <span className="text-[#60CDFF]">•</span>
                  <span><strong>File Icon & Open:</strong> Double-click .bay files to open in Bayzip</span>
                </div>
              </div>

              <button
                type="button"
                disabled={isSyncingRegistry}
                onClick={handleRegisterContextMenu}
                className={`w-full py-2 bg-[#2D2D2D] hover:bg-[#383838] border border-[#3E3E3E] hover:border-[#60CDFF] text-[#60CDFF] text-xs font-semibold rounded-md transition-all flex items-center justify-center space-x-2 cursor-pointer ${
                  isSyncingRegistry ? "opacity-75 cursor-not-allowed" : ""
                }`}
              >
                {isSyncingRegistry ? (
                  <>
                    <svg className="animate-spin h-3.5 w-3.5 text-[#60CDFF]" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                    <span>Syncing Windows Registry...</span>
                  </>
                ) : (
                  <>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                    </svg>
                    <span>Sync Windows Explorer Integration</span>
                  </>
                )}
              </button>

              {contextMenuStatus && (
                <div className="text-[11px] text-emerald-400 bg-emerald-950/40 p-2.5 rounded border border-emerald-900/50 flex items-center space-x-2">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4 shrink-0 text-emerald-400">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                  </svg>
                  <span>{contextMenuStatus}</span>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;

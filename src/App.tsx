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

const FORMAT_ALGORITHMS: Record<string, string[]> = {
  ".bay": ["Zstandard (Recommended)", "Brotli", "LZ4 (Fastest)"],
  ".bay_gamer": ["Zstd-Fast (Auto)"],
  ".7z": ["LZMA2", "LZMA", "BZip2", "PPMd"],
  ".zip": ["Deflate", "Deflate64", "BZip2", "LZMA"],
  ".tar": ["None (Store)"],
};

function App() {
  const [specs, setSpecs] = useState<SystemSpecs | null>(null);
  const [sliderValue, setSliderValue] = useState<number>(50); // 0, 25, 50, 75, 100
  const [isAdvanced, setIsAdvanced] = useState(false);

  const [selectedFormat, setSelectedFormat] = useState(".bay");
  const [selectedAlgorithm, setSelectedAlgorithm] = useState(FORMAT_ALGORITHMS[".bay"][0]);
  const [selectedEncryption, setSelectedEncryption] = useState("none");

  // Options
  const [archiveToOriginalPath, setArchiveToOriginalPath] = useState(true);
  const [separateArchives, setSeparateArchives] = useState(false);
  const [selectedFiles, setSelectedFiles] = useState<string[]>([]);
  const [itemsInfo, setItemsInfo] = useState<Record<string, { size: number; is_dir: boolean }>>({});
  const [isAboutOpen, setIsAboutOpen] = useState(false);
  const [contextMenuStatus, setContextMenuStatus] = useState<string | null>(null);

  // Compression execution & progress state
  const [isCompressing, setIsCompressing] = useState(false);
  const [progress, setProgress] = useState<CompressProgress | null>(null);
  const [compressResult, setCompressResult] = useState<CompressResult | null>(null);
  const [compressError, setCompressError] = useState<string | null>(null);

  // Query file and folder size metadata when selection changes
  useEffect(() => {
    if (selectedFiles.length === 0) {
      setItemsInfo({});
      return;
    }
    invoke<SelectedItemInfo[]>("get_selected_items_info", { paths: selectedFiles })
      .then((infos) => {
        const map: Record<string, { size: number; is_dir: boolean }> = {};
        infos.forEach((info) => {
          map[info.path] = { size: info.size, is_dir: info.is_dir };
        });
        setItemsInfo(map);
      })
      .catch((err) => console.error("Failed to query item sizes:", err));
  }, [selectedFiles]);

  const totalSelectedSize = useMemo(() => {
    return selectedFiles.reduce((acc, path) => acc + (itemsInfo[path]?.size || 0), 0);
  }, [selectedFiles, itemsInfo]);

  useEffect(() => {
    // 1. Instant hardware specs query
    invoke<SystemSpecs>("get_system_specs")
      .then((res) => setSpecs(res))
      .catch((err) => console.error("Specs query error:", err));

    // 2. Fetch CLI / Context menu startup arguments
    invoke<string[]>("get_cli_args")
      .then((args) => {
        if (args && args.length > 0) {
          setSelectedFiles((prev) => [...new Set([...prev, ...args])]);
        }
      })
      .catch(() => {});

    // 3. Listen to native Tauri file drop events
    let unlistenDrop: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          const paths = event.payload.paths;
          if (paths && paths.length > 0) {
            setSelectedFiles((prev) => [...new Set([...prev, ...paths])]);
          }
        }
      })
      .then((fn) => {
        unlistenDrop = fn;
      })
      .catch((err) => {
        console.error("Failed to register drag-drop listener:", err);
      });

    // 5. Listen to live compression progress events from Rust
    let unlistenProgress: (() => void) | undefined;
    listen<CompressProgress>("compress_progress", (event) => {
      setProgress(event.payload);
    })
      .then((fn) => {
        unlistenProgress = fn;
      })
      .catch((err) => {
        console.error("Progress listener error:", err);
      });

    return () => {
      if (unlistenDrop) unlistenDrop();
      if (unlistenProgress) unlistenProgress();
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
      {/* Windows 11 Native Custom Titlebar */}
      <div
        data-tauri-drag-region
        onMouseDown={(e) => {
          if (e.button === 0 && (e.target as HTMLElement).closest("button") === null) {
            getCurrentWindow().startDragging().catch(() => {});
          }
        }}
        onDoubleClick={(e) => {
          if ((e.target as HTMLElement).closest("button") === null) {
            getCurrentWindow().toggleMaximize().catch(() => {});
          }
        }}
        className="h-10 w-full flex justify-between items-center bg-[#202020] cursor-default select-none border-b border-[#2D2D2D] shrink-0"
      >
        <div data-tauri-drag-region className="flex items-center space-x-3 px-4 pointer-events-none">
          <div className="w-4 h-4 text-[#60CDFF] flex items-center justify-center">
            {/* Simple Zip Icon */}
            <svg viewBox="0 0 24 24" fill="currentColor" className="w-full h-full">
              <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2h12c1.1 0 2-.9 2-2V8l-6-6zm-2 14h-2v-2h2v2zm0-4h-2v-2h2v2zm0-4h-2V6h2v2z" />
            </svg>
          </div>
          <span className="text-xs font-normal text-white tracking-wide">Bayzip</span>
        </div>

        {/* Dedicated Draggable Center Surface */}
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
          {/* Header */}
          <div className="flex items-center justify-between">
            <h1 className="text-2xl font-semibold tracking-tight">Create Archive</h1>
            <div className="flex space-x-3">
              <button
                type="button"
                onClick={() => setIsAboutOpen(true)}
                className="text-sm font-medium px-4 py-1.5 rounded text-[#A8A8A8] hover:bg-[#333333] hover:text-white transition-colors cursor-pointer"
              >
                About
              </button>
              <button
                type="button"
                onClick={() => setIsAdvanced(!isAdvanced)}
                className="text-sm font-medium px-4 py-1.5 rounded bg-[#333333] hover:bg-[#3E3E3E] border border-[#3E3E3E] transition-colors cursor-pointer"
              >
                {isAdvanced ? "Simple Mode" : "Advanced Options"}
              </button>
            </div>
          </div>

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
                  onClick={() => setSelectedFiles([])}
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
              <div className="grid grid-cols-3 gap-4">
                <div className={selectedFormat === ".bay_gamer" ? "col-span-2" : "col-span-1"}>
                  <label className="block text-xs font-medium mb-2">Format</label>
                  <select
                    value={selectedFormat}
                    onChange={(e) => setSelectedFormat(e.target.value)}
                    className="w-full bg-[#202020] rounded-md p-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none cursor-pointer"
                  >
                    <option value=".bay">.bay</option>
                    <option value=".bay_gamer">.bay (Gamer Mod Pack)</option>
                    <option value=".7z">.7z</option>
                    <option value=".zip">.zip</option>
                    <option value=".tar">.tar</option>
                  </select>
                </div>

                {/* Hide algorithm dropdown if Gamer Mod Pack is selected */}
                {selectedFormat !== ".bay_gamer" && (
                  <div>
                    <label className="block text-xs font-medium mb-2">Algorithm</label>
                    <select
                      value={selectedAlgorithm}
                      onChange={(e) => setSelectedAlgorithm(e.target.value)}
                      className="w-full bg-[#202020] rounded-md p-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none cursor-pointer"
                    >
                      {FORMAT_ALGORITHMS[selectedFormat].map((algo) => (
                        <option key={algo} value={algo}>
                          {algo}
                        </option>
                      ))}
                    </select>
                  </div>
                )}

                <div>
                  <label className="block text-xs font-medium mb-2">Encryption</label>
                  <select
                    value={selectedEncryption}
                    onChange={(e) => setSelectedEncryption(e.target.value)}
                    className="w-full bg-[#202020] rounded-md p-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none cursor-pointer"
                  >
                    <option value="none">None</option>
                    <option value="aes-256">AES-256-GCM</option>
                    <option value="chacha20">ChaCha20-Poly1305</option>
                    <option value="twofish">Twofish</option>
                  </select>
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
              <span>{isCompressing ? "Compressing..." : selectedFiles.length > 0 ? "Compress Now" : "Select Files to Compress"}</span>
            </button>
          </div>
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
                }}
                className="px-6 py-2 bg-[#60CDFF] hover:bg-[#4FB8E6] text-black text-xs font-semibold rounded-md transition-colors cursor-pointer"
              >
                Done
              </button>
            </div>
          </div>
        </div>
      )}

      {/* About Modal */}
      {isAboutOpen && (
        <div
          onClick={() => setIsAboutOpen(false)}
          className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
        >
          <div
            onClick={(e) => e.stopPropagation()}
            className="bg-[#202020] border border-[#3E3E3E] rounded-xl p-8 max-w-sm w-full mx-4 shadow-2xl relative"
          >
            <button
              type="button"
              onClick={() => setIsAboutOpen(false)}
              className="absolute top-4 right-4 text-[#A8A8A8] hover:text-white transition-colors cursor-pointer"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-5 h-5">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>

            <div className="flex items-center space-x-3 mb-6">
              <div className="w-8 h-8 text-[#60CDFF] flex items-center justify-center">
                <svg viewBox="0 0 24 24" fill="currentColor" className="w-full h-full">
                  <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2h12c1.1 0 2-.9 2-2V8l-6-6zm-2 14h-2v-2h2v2zm0-4h-2v-2h2v2zm0-4h-2V6h2v2z" />
                </svg>
              </div>
              <h2 className="text-xl font-semibold">Bayzip</h2>
              <span className="text-xs bg-[#333333] text-[#A8A8A8] px-2 py-0.5 rounded-full border border-[#3E3E3E]">
                v0.1.0
              </span>
            </div>

            <div className="space-y-4 text-sm text-[#D0D0D0]">
              <p>
                A next-generation compression tool built with Rust and React for extreme hardware utilization and modern UI.
              </p>

              <div className="pt-4 border-t border-[#3E3E3E]">
                <p className="font-medium text-[#60CDFF] mb-1">Created By</p>
                <p className="text-white font-medium">Made by Baymax</p>
                <p className="text-xs text-[#A8A8A8] mt-0.5">CEO of DPG (aka Muhammad Ali)</p>
              </div>

              {/* Windows Explorer Integration */}
              <div className="pt-4 border-t border-[#3E3E3E] space-y-2">
                <div className="flex justify-between items-center">
                  <div>
                    <span className="text-xs text-white block">Windows Explorer Integration</span>
                    <span className="text-[10px] text-[#A8A8A8]">Adds "Compress with Bayzip" to right-click menu</span>
                  </div>
                  <button
                    type="button"
                    disabled={isSyncingRegistry}
                    onClick={handleRegisterContextMenu}
                    className={`text-xs text-[#60CDFF] hover:underline cursor-pointer bg-[#2A2A2A] px-2.5 py-1 rounded border border-[#3E3E3E] hover:border-[#60CDFF] transition-colors flex items-center space-x-1.5 ${
                      isSyncingRegistry ? "opacity-75 cursor-not-allowed" : ""
                    }`}
                  >
                    {isSyncingRegistry && (
                      <svg className="animate-spin h-3 w-3 text-[#60CDFF]" fill="none" viewBox="0 0 24 24">
                        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                      </svg>
                    )}
                    <span>{isSyncingRegistry ? "Syncing..." : "Sync Registry"}</span>
                  </button>
                </div>
                {contextMenuStatus && (
                  <p className="text-[11px] text-emerald-400 bg-emerald-950/40 p-2 rounded border border-emerald-900/50">
                    {contextMenuStatus}
                  </p>
                )}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;

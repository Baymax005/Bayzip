import { useMemo } from "react";
import { SystemSpecs, CompressProgress } from "../types";

const FORMAT_ALGORITHMS: Record<string, string[]> = {
  ".bay": ["Zstandard (Recommended)", "Brotli", "LZ4 (Fastest)"],
  ".bayg": ["Zstd-Fast (Auto)"],
  ".7z": ["LZMA2", "LZMA", "BZip2"],
  ".zip": ["Deflate (Standard)", "None (Store)", "BZip2 (Requires 3rd-party App)"],
  ".tar": ["None (Store)"],
};

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export interface CompressPageProps {
  selectedFiles: string[];
  setSelectedFiles: React.Dispatch<React.SetStateAction<string[]>>;
  itemsInfo: Record<string, { size: number; is_dir: boolean }>;
  customArchiveName: string;
  setCustomArchiveName: (name: string) => void;
  selectedFormat: string;
  setSelectedFormat: (fmt: string) => void;
  selectedAlgorithm: string;
  setSelectedAlgorithm: (algo: string) => void;
  selectedEncryption: string;
  setSelectedEncryption: (enc: string) => void;
  compressPassword: string;
  setCompressPassword: (pwd: string) => void;
  showCompressPassword: boolean;
  setShowCompressPassword: (show: boolean) => void;
  archiveToOriginalPath: boolean;
  setArchiveToOriginalPath: (val: boolean) => void;
  compressDestDir: string;
  setCompressDestDir: (dir: string) => void;
  separateArchives: boolean;
  setSeparateArchives: (val: boolean) => void;
  smartMediaBypass: boolean;
  setSmartMediaBypass: (val: boolean) => void;
  isAdvanced: boolean;
  setIsAdvanced: (val: boolean) => void;
  sliderValue: number;
  setSliderValue: (val: number) => void;
  specs: SystemSpecs | null;
  isRefreshingSpecs: boolean;
  handleRefreshSpecs: () => void;
  isCompressing: boolean;
  isCancellingCompress: boolean;
  progress: CompressProgress | null;
  compressError: string | null;
  activeCompressConfig: {
    format: string;
    algorithm: string;
    sliderValue: number;
    archiveName: string;
  } | null;
  handleSelectFiles: () => void;
  handleSelectFolder: () => void;
  handleSelectCompressDestination: () => void;
  handleCompress: () => void;
  handleCancelCompress: () => void;
}

export function CompressPage({
  selectedFiles,
  setSelectedFiles,
  itemsInfo,
  customArchiveName,
  setCustomArchiveName,
  selectedFormat,
  setSelectedFormat,
  selectedAlgorithm,
  setSelectedAlgorithm,
  selectedEncryption,
  setSelectedEncryption,
  compressPassword,
  setCompressPassword,
  showCompressPassword,
  setShowCompressPassword,
  archiveToOriginalPath,
  setArchiveToOriginalPath,
  compressDestDir,
  setCompressDestDir,
  separateArchives,
  setSeparateArchives,
  smartMediaBypass,
  setSmartMediaBypass,
  isAdvanced,
  sliderValue,
  setSliderValue,
  specs,
  isRefreshingSpecs,
  handleRefreshSpecs,
  isCompressing,
  isCancellingCompress,
  progress,
  compressError,
  activeCompressConfig,
  handleSelectFiles,
  handleSelectFolder,
  handleSelectCompressDestination,
  handleCompress,
  handleCancelCompress,
}: CompressPageProps) {
  const totalSelectedSize = useMemo(() => {
    return selectedFiles.reduce((acc, f) => acc + (itemsInfo[f]?.size || 0), 0);
  }, [selectedFiles, itemsInfo]);

  const getEstimatedCores = () => {
    const val = isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue;
    if (val === 0) return 1;
    if (!specs) return 1;
    if (val === 25) return Math.max(1, Math.floor(specs.cpu_cores / 4));
    if (val === 50) return Math.max(1, Math.floor(specs.cpu_cores / 2));
    if (val === 75) return Math.max(1, Math.floor((specs.cpu_cores * 3) / 4));
    return specs.cpu_cores;
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
    if (fmt === ".bayg") {
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
                        className="w-4 h-4 text-[#60CDFF] shrink-0"
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
                        className="w-4 h-4 text-[#A8A8A8] shrink-0"
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2}
                          d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                        />
                      </svg>
                    )}
                    <span className="text-sm truncate text-white" title={file}>
                      {file.split(/[/\\]/).pop()}
                    </span>
                  </div>
                  <div className="flex items-center space-x-3 shrink-0">
                    <span className="text-xs text-[#A8A8A8] font-mono">{sizeFormatted}</span>
                    <button
                      type="button"
                      onClick={() =>
                        setSelectedFiles((prev) => prev.filter((f) => f !== file))
                      }
                      className="text-[#888888] hover:text-red-400 p-1 transition-colors cursor-pointer"
                      title="Remove from selection"
                    >
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2}
                          d="M6 18L18 6M6 6l12 12"
                        />
                      </svg>
                    </button>
                  </div>
                </div>
              );
            })}
          </div>

          <div className="flex justify-between items-center pt-2">
            <div className="flex space-x-2">
              <button
                type="button"
                onClick={handleSelectFiles}
                className="text-xs font-medium px-3 py-1.5 bg-[#202020] hover:bg-[#2A2A2A] text-white border border-[#3E3E3E] rounded-md transition-colors cursor-pointer flex items-center space-x-1.5"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-[#60CDFF]">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
                <span>Add Files</span>
              </button>
              <button
                type="button"
                onClick={handleSelectFolder}
                className="text-xs font-medium px-3 py-1.5 bg-[#202020] hover:bg-[#2A2A2A] text-white border border-[#3E3E3E] rounded-md transition-colors cursor-pointer flex items-center space-x-1.5"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-[#60CDFF]">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M9 13h6m-3-3v6m-9 1V7a2 2 0 012-2h6l2 2h6a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z"
                  />
                </svg>
                <span>Add Folder</span>
              </button>
            </div>
            <span className="text-[11px] text-[#888888]">
              You can drag & drop more files anytime
            </span>
          </div>
        </div>
      ) : (
        <div
          onClick={handleSelectFiles}
          className="bg-[#2D2D2D] border border-dashed border-[#3E3E3E] hover:border-[#60CDFF] rounded-lg p-8 text-center transition-colors flex flex-col items-center justify-center space-y-4 cursor-pointer group"
        >
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
            <p className="text-xs text-[#A8A8A8]">Native hardware-accelerated multithreaded engine</p>
          </div>
          <div className="flex space-x-2">
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                handleSelectFiles();
              }}
              className="text-xs font-medium px-4 py-2 bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#60CDFF] text-white rounded-md transition-colors cursor-pointer"
            >
              Select Files
            </button>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                handleSelectFolder();
              }}
              className="text-xs font-medium px-4 py-2 bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#60CDFF] text-white rounded-md transition-colors cursor-pointer"
            >
              Select Folder
            </button>
          </div>
        </div>
      )}

      {/* Target Archive Name Card */}
      {selectedFiles.length > 0 && (
        <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-4 flex items-center space-x-3">
          <div className="w-8 h-8 rounded-md bg-[#202020] border border-[#3E3E3E] flex items-center justify-center text-[#60CDFF] shrink-0 font-mono text-xs font-bold">
            {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat).toUpperCase()}
          </div>
          <div className="flex-1">
            <label className="block text-[11px] text-[#A8A8A8] mb-1">Archive Name</label>
            <div className="flex items-center bg-[#202020] rounded-md border border-[#3E3E3E] focus-within:border-[#60CDFF] px-2.5 py-1.5 transition-colors">
              <input
                type="text"
                disabled={isCompressing}
                value={isCompressing && activeCompressConfig ? activeCompressConfig.archiveName : customArchiveName}
                onChange={(e) => setCustomArchiveName(e.target.value)}
                placeholder="Enter archive name..."
                className="bg-transparent border-none outline-none text-white text-xs font-mono flex-1 disabled:opacity-50 disabled:cursor-not-allowed"
              />
              <span className="text-xs text-[#60CDFF] font-mono ml-1 font-semibold select-none">
                {isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat}
              </span>
            </div>
          </div>
        </div>
      )}

      {/* Hardware Allocation & Thread Matrix Card */}
      <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6 space-y-5">
        <div className="flex justify-between items-center">
          <div>
            <h3 className="text-sm font-medium text-white">Hardware Allocation</h3>
            <p className="text-xs text-[#A8A8A8] mt-0.5">Control system CPU cores, thread pool & memory limits</p>
          </div>
          <div className="flex items-center space-x-2">
            <span className="text-xs font-medium text-[#60CDFF] bg-[#1a3847] px-2.5 py-1 rounded-md border border-[#23536b]">
              {getSliderLabel()}
            </span>
            <button
              type="button"
              onClick={handleRefreshSpecs}
              disabled={isRefreshingSpecs}
              className="p-1 rounded bg-[#202020] hover:bg-[#333333] border border-[#3E3E3E] text-[#A8A8A8] hover:text-white transition-colors cursor-pointer disabled:opacity-50"
              title="Refresh Hardware Specs"
            >
              <svg
                className={`w-3.5 h-3.5 ${isRefreshingSpecs ? "animate-spin text-[#60CDFF]" : ""}`}
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path strokeLinecap="round" strokeLinejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </button>
          </div>
        </div>

        {/* Dynamic 5-Tier Hardware Allocation Slider */}
        <div className="space-y-2 pt-2">
          <input
            type="range"
            min="0"
            max="100"
            step="25"
            disabled={isCompressing}
            value={isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue}
            onChange={(e) => setSliderValue(Number(e.target.value))}
            className="w-full h-1.5 bg-[#202020] rounded-lg appearance-none cursor-pointer accent-[#60CDFF] disabled:opacity-50 disabled:cursor-not-allowed"
          />
          <div className="flex justify-between text-[11px] text-[#A8A8A8] px-0.5">
            <span>0% (Eco)</span>
            <span>25%</span>
            <span>50% (Balanced)</span>
            <span>75%</span>
            <span>100% (Max)</span>
          </div>
        </div>

        {/* Live Hardware Resource Estimates */}
        <div className="grid grid-cols-2 gap-4 pt-2">
          <div className="bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
            <div className="flex justify-between items-start mb-1">
              <p className="text-xs text-[#A8A8A8]">Encoder RAM Allocation</p>
              {specs && (
                <span className="text-[10px] text-[#60CDFF] bg-[#1a3847] px-1.5 py-0.5 rounded font-mono">
                  {Math.round(specs.total_ram_gb)} GB Total
                </span>
              )}
            </div>
            <p className="text-lg font-medium">
              {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar"
                ? "Streaming (0 MB)"
                : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".zip"
                  ? "Standard (2 MB)"
                  : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".7z"
                    ? (isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue) >= 75 ? "~1.2 GB" : "~350 MB"
                    : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bayg"
                      ? "Fast (64 MB)"
                      : (isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue) === 100
                        ? "~1.8 GB (Deep Matching)"
                        : (isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue) >= 75
                          ? "~450 MB"
                          : "~128 MB"}
            </p>
            <p className="text-[11px] text-[#7A7A7A] mt-1">
              {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar"
                ? "Direct stream buffer"
                : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".zip"
                  ? "Standard Deflate buffer"
                  : `Working buffer & encoder RAM (${specs ? Math.round(specs.free_ram_gb) : "?"} GB free)`}
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

        {/* Ultra Archival Mode Warning for .bay at 100% */}
        {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bay" &&
          (isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue) === 100 &&
          (isCompressing && activeCompressConfig ? activeCompressConfig.algorithm : selectedAlgorithm).toLowerCase().includes("zstandard") && (
            <div className="mt-4 p-3 rounded-lg bg-amber-500/10 border border-amber-500/30 flex items-start gap-3 text-xs text-amber-200 animate-fadeIn">
              <svg className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              <div className="space-y-1">
                <p className="font-semibold text-amber-300">Ultra Archival Mode (Zstandard Level 19)</p>
                <p className="text-amber-200/90 leading-relaxed">
                  Level 19 uses deep binary-tree matching for the smallest possible file size, which requires higher RAM usage and significantly longer compression time. For fast multi-core compression, use 75% (Level 9) or switch to <strong>.bayg</strong>.
                </p>
              </div>
            </div>
          )}
      </div>

      {/* Advanced Options Toggle */}
      {isAdvanced && (
        <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6 space-y-5">
          <div className="grid grid-cols-1 sm:grid-cols-12 gap-3.5">
            <div className={(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bayg" ? "sm:col-span-6" : "sm:col-span-3"}>
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
                  className={`w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none text-white truncate transition-colors ${
                    isCompressing ? "opacity-50 cursor-not-allowed" : "cursor-pointer"
                  }`}
                >
                  <option value=".bay">.bay</option>
                  <option value=".bayg">.bayg (Gamer Mod Pack)</option>
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
            {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) !== ".bayg" && (
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
                    className={`w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none text-white truncate transition-colors ${
                      isCompressing ? "opacity-50 cursor-not-allowed" : "cursor-pointer"
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

            <div className={(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bayg" ? "sm:col-span-6" : "sm:col-span-4"}>
              <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">Encryption</label>
              <div className="relative group">
                <select
                  disabled={isCompressing || (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar"}
                  value={selectedEncryption}
                  onChange={(e) => {
                    if (isCompressing) return;
                    setSelectedEncryption(e.target.value);
                  }}
                  className={`w-full appearance-none bg-[#202020] hover:bg-[#252525] rounded-md pl-3 pr-8 py-2 text-sm border border-[#3E3E3E] hover:border-[#555] focus:border-[#60CDFF] outline-none text-white truncate transition-colors ${
                    isCompressing || (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar"
                      ? "opacity-50 cursor-not-allowed"
                      : "cursor-pointer"
                  }`}
                >
                  <option value="none">None (Fastest)</option>
                  {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) !== ".tar" && (
                    <option value="aes256">AES-256 (Encrypted)</option>
                  )}
                </select>
                <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-[#888888] group-hover:text-white transition-colors">
                  <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6l4 4 4-4" />
                  </svg>
                </div>
              </div>
            </div>
          </div>

          {/* Secure Password Entry when Encryption is enabled */}
          {selectedEncryption !== "none" && (
            <div className="pt-2 animate-fadeIn">
              <label className="block text-xs font-medium mb-1.5 text-[#C8C8C8]">
                Archive Password
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
            <label className={`flex items-center group transition-opacity ${
              isCompressing ? "cursor-not-allowed opacity-50 pointer-events-none" : "cursor-pointer"
            }`}>
              <div className="relative">
                <input
                  type="checkbox"
                  disabled={isCompressing}
                  className="sr-only"
                  checked={archiveToOriginalPath}
                  onChange={() => !isCompressing && setArchiveToOriginalPath(!archiveToOriginalPath)}
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

            <label className={`flex items-center group transition-opacity ${
              isCompressing ? "cursor-not-allowed opacity-50 pointer-events-none" : "cursor-pointer"
            }`}>
              <div className="relative">
                <input
                  type="checkbox"
                  disabled={isCompressing}
                  className="sr-only"
                  checked={separateArchives}
                  onChange={() => !isCompressing && setSeparateArchives(!separateArchives)}
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
                Compress each file / folder into a separate dedicated archive
              </div>
            </label>

            {/* Smart Pre-Compressed Media Bypass (Fast Store Mode) */}
            <label className={`flex items-center group transition-opacity ${
              isCompressing ? "cursor-not-allowed opacity-50 pointer-events-none" : "cursor-pointer"
            }`}>
              <div className="relative">
                <input
                  type="checkbox"
                  disabled={isCompressing}
                  className="sr-only"
                  checked={smartMediaBypass}
                  onChange={() => !isCompressing && setSmartMediaBypass(!smartMediaBypass)}
                />
                <div
                  className={`block w-10 h-6 rounded-full transition-colors ${
                    smartMediaBypass ? "bg-[#60CDFF]" : "bg-[#3E3E3E]"
                  }`}
                ></div>
                <div
                  className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${
                    smartMediaBypass ? "transform translate-x-4" : ""
                  }`}
                ></div>
              </div>
              <div className="ml-3 flex flex-col">
                <div className="flex items-center space-x-2">
                  <span className="text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                    Smart media bypass (Fast Store)
                  </span>
                  <span className="text-[10px] font-semibold uppercase tracking-wider bg-[#142330] text-[#60CDFF] px-2 py-0.5 rounded border border-[#20495f]">
                    Fast Store
                  </span>
                </div>
                <span className="text-[11px] text-[#7A7A7A] mt-0.5">
                  Stores already-compressed media (mp4, mkv, mp3, jpg, png, zip, 7z) with 0% CPU overhead, maximizing NVMe write speeds
                </span>
              </div>
            </label>
          </div>
        </div>
      )}

      {/* Progress Card */}
      {isCompressing && (
        <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-4 shadow-lg space-y-3">
          <div className="flex justify-between items-center text-xs">
            <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
              <span className="w-2.5 h-2.5 rounded-full bg-[#60CDFF] animate-pulse shrink-0"></span>
              <span className="text-[#D0D0D0] truncate font-medium">
                {progress?.current_file ? progress.current_file : "Compressing..."}
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

          <div className="w-full h-2 bg-[#202020] rounded-full overflow-hidden border border-[#3E3E3E]">
            <div
              className="h-full bg-gradient-to-r from-[#0078D4] to-[#60CDFF] transition-all duration-150 ease-out rounded-full shadow-[0_0_10px_rgba(96,205,255,0.7)]"
              style={{ width: `${progress ? Math.max(2, progress.percent) : 2}%` }}
            ></div>
          </div>

          <div className="flex justify-between items-center text-[11px] text-[#8A8A8A]">
            <span>
              {progress && progress.bytes_processed > 0 ? (
                progress.total_bytes > 0 ? (
                  `${formatBytes(progress.bytes_processed)} / ${formatBytes(progress.total_bytes)}`
                ) : (
                  `${formatBytes(progress.bytes_processed)} processed`
                )
              ) : (
                "High-performance multithreading active"
              )}
            </span>
            <span className="text-[#60CDFF] font-medium">
              {activeCompressConfig ? activeCompressConfig.format : selectedFormat}
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
          disabled={isCancellingCompress}
          onClick={isCompressing ? handleCancelCompress : handleCompress}
          className={`px-8 py-2.5 font-semibold rounded-md shadow-sm transition-all text-sm flex items-center space-x-2 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed ${
            isCompressing
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
  );
}

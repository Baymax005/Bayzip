import { useState, useMemo, useEffect } from "react";
import { SystemSpecs, CompressProgress, ArchiveInfo, ArchiveEntryItem } from "../types";
import { ArchiveTreeInspector, normalizeArchiveEntryPath } from "./ArchiveTreeInspector";
import { BayzipLogo } from "./BayzipLogo";
import { FileFormatIcon, prefetchFileIcons } from "./FileFormatIcon";

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

  // Turn 3: Staged Tree Inspection & Exclusions
  compressViewMode: "tree" | "list";
  setCompressViewMode: (mode: "tree" | "list") => void;
  stagedEntries: ArchiveEntryItem[];
  isInspectingStaged: boolean;
  excludedCompressPaths: Set<string>;
  onToggleCompressExclude: (path: string, isDir?: boolean) => void;
  onCompressIncludeAll: () => void;
  onPreviewStagedEntry: (entryName: string) => void;
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
  compressViewMode,
  setCompressViewMode,
  stagedEntries,
  isInspectingStaged,
  excludedCompressPaths,
  onToggleCompressExclude,
  onCompressIncludeAll,
  onPreviewStagedEntry,
}: CompressPageProps) {
  const totalSelectedSize = useMemo(() => {
    return selectedFiles.reduce((acc, f) => acc + (itemsInfo[f]?.size || 0), 0);
  }, [selectedFiles, itemsInfo]);

  const effectiveTotalSize = useMemo(() => {
    if (stagedEntries.length === 0) {
      return totalSelectedSize;
    }
    if (excludedCompressPaths.size === 0) {
      return stagedEntries.reduce((acc, e) => acc + (e.is_dir ? 0 : e.size), 0);
    }
    const excludedArray = Array.from(excludedCompressPaths);
    return stagedEntries.reduce((acc, e) => {
      if (e.is_dir) return acc;
      const norm = normalizeArchiveEntryPath(e.name);
      const isExcluded = excludedArray.some((ex) => {
        if (norm === ex) return true;
        const prefix = ex.endsWith("/") ? ex : ex + "/";
        return norm.startsWith(prefix);
      });
      return isExcluded ? acc : acc + e.size;
    }, 0);
  }, [stagedEntries, excludedCompressPaths, totalSelectedSize]);

  const totalStagedFilesCount = useMemo(() => {
    return stagedEntries.filter((e) => !e.is_dir).length;
  }, [stagedEntries]);

  const includedFilesCount = useMemo(() => {
    if (stagedEntries.length === 0) return selectedFiles.length;
    if (excludedCompressPaths.size === 0) return totalStagedFilesCount;
    const excludedArray = Array.from(excludedCompressPaths);
    let count = 0;
    for (const e of stagedEntries) {
      if (e.is_dir) continue;
      const norm = normalizeArchiveEntryPath(e.name);
      const isExcluded = excludedArray.some((ex) => {
        if (norm === ex) return true;
        const prefix = ex.endsWith("/") ? ex : ex + "/";
        return norm.startsWith(prefix);
      });
      if (!isExcluded) {
        count++;
      }
    }
    return count;
  }, [stagedEntries, excludedCompressPaths, totalStagedFilesCount, selectedFiles.length]);

  const stagedArchiveInfo = useMemo<ArchiveInfo>(() => {
    const targetName =
      customArchiveName.trim() ||
      (selectedFiles.length === 1 ? selectedFiles[0].split(/[/\\]/).pop() || "Archive" : "Archive");
    return {
      path: selectedFiles[0] || "",
      filename: targetName,
      size: effectiveTotalSize,
      format: selectedFormat.toUpperCase(),
      is_archive: false,
      is_encrypted: false,
      file_count: totalStagedFilesCount,
      total_uncompressed_size: effectiveTotalSize,
      entries: stagedEntries,
    };
  }, [selectedFiles, customArchiveName, effectiveTotalSize, selectedFormat, stagedEntries, totalStagedFilesCount]);

  useEffect(() => {
    if (selectedFiles.length > 0) {
      prefetchFileIcons(selectedFiles);
    }
    if (stagedEntries.length > 0) {
      prefetchFileIcons(stagedEntries.map((e) => e.name));
    }
  }, [selectedFiles, stagedEntries]);

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

  const [isWorkspaceOpen, setIsWorkspaceOpen] = useState(false);

  const renderWorkspaceSidebar = () => (
    <div className="space-y-4 text-xs">
      {/* Archive Name Card */}
      <div className="bg-[#1C1C1C] rounded-lg border border-[#333333] p-3 space-y-1.5">
        <label className="text-[11px] font-medium text-[#C8C8C8]">Archive Name</label>
        <div className="flex items-center space-x-2">
          <div className="w-8 h-8 rounded-md bg-[#252525] border border-[#3E3E3E] flex items-center justify-center shrink-0">
            <BayzipLogo className="w-4 h-4" />
          </div>
          <div className="flex-1 flex items-center bg-[#252525] rounded-md border border-[#3E3E3E] focus-within:border-[#60CDFF] px-2 py-1">
            <input
              type="text"
              disabled={isCompressing}
              value={isCompressing && activeCompressConfig ? activeCompressConfig.archiveName : customArchiveName}
              onChange={(e) => setCustomArchiveName(e.target.value)}
              placeholder="Archive name..."
              className="bg-transparent border-none outline-none text-white text-xs font-mono flex-1 disabled:opacity-50"
            />
            <span className="text-[11px] text-[#60CDFF] font-mono ml-1 font-semibold">
              {selectedFormat}
            </span>
          </div>
        </div>
      </div>

      {/* Format Selector Pills */}
      <div className="bg-[#1C1C1C] rounded-lg border border-[#333333] p-3 space-y-2">
        <label className="text-[11px] font-medium text-[#C8C8C8]">Archive Format</label>
        <div className="grid grid-cols-3 gap-1.5">
          {[".bay", ".bayg", ".zip", ".7z", ".tar"].map((fmt) => (
            <button
              key={fmt}
              type="button"
              disabled={isCompressing}
              onClick={() => {
                setSelectedFormat(fmt);
                setSelectedAlgorithm(FORMAT_ALGORITHMS[fmt]?.[0] || "None (Store)");
              }}
              className={`py-1.5 px-2 rounded text-xs font-mono font-semibold transition-all cursor-pointer ${
                selectedFormat === fmt
                  ? "bg-[#60CDFF] text-black shadow-sm"
                  : "bg-[#252525] text-[#A8A8A8] hover:text-white border border-[#383838]"
              }`}
            >
              {fmt}
            </button>
          ))}
        </div>
      </div>

      {/* Compression Level Slider */}
      <div className="bg-[#1C1C1C] rounded-lg border border-[#333333] p-3 space-y-2">
        <div className="flex justify-between items-center">
          <label className="text-[11px] font-medium text-[#C8C8C8]">Compression Profile</label>
          <span className="text-[10px] text-[#60CDFF] font-semibold bg-[#1a3847] px-2 py-0.5 rounded border border-[#23536b]">
            {getSliderLabel()}
          </span>
        </div>
        <div className="relative pt-1">
          <input
            type="range"
            min="0"
            max="100"
            step="25"
            disabled={isCompressing}
            value={sliderValue}
            onChange={(e) => setSliderValue(Number(e.target.value))}
            className="w-full h-1.5 bg-[#333333] rounded-lg appearance-none cursor-pointer accent-[#60CDFF] block"
          />
          <div className="relative w-full h-4 mt-1.5 text-[9px] font-mono select-none">
            {[
              { val: 0, label: "Eco" },
              { val: 25, label: "Fast" },
              { val: 50, label: "Balanced" },
              { val: 75, label: "High" },
              { val: 100, label: "Max" },
            ].map((tier) => {
              const isSelected = sliderValue === tier.val;
              return (
                <span
                  key={tier.val}
                  onClick={() => !isCompressing && setSliderValue(tier.val)}
                  style={{
                    left: `calc(${tier.val}% + ${(50 - tier.val) * 0.16}px)`,
                    transform: "translateX(-50%)",
                  }}
                  className={`absolute transition-colors cursor-pointer whitespace-nowrap ${
                    isSelected ? "text-[#60CDFF] font-bold" : "text-[#777777] hover:text-[#C8C8C8]"
                  }`}
                  title={`Set Compression Profile to ${tier.label}`}
                >
                  {tier.label}
                </span>
              );
            })}
          </div>
        </div>
      </div>

      {/* Smart Media Bypass Toggle */}
      <div className="bg-[#1C1C1C] rounded-lg border border-[#333333] p-3 flex items-center justify-between">
        <div>
          <p className="text-[11px] font-medium text-white">Smart Media Bypass</p>
          <p className="text-[10px] text-[#888888]">Store pre-compressed media directly</p>
        </div>
        <input
          type="checkbox"
          disabled={isCompressing}
          checked={smartMediaBypass}
          onChange={(e) => setSmartMediaBypass(e.target.checked)}
          className="accent-[#60CDFF] w-4 h-4 cursor-pointer"
        />
      </div>

      {/* Password Protection */}
      <div className="bg-[#1C1C1C] rounded-lg border border-[#333333] p-3 space-y-1.5">
        <label className="text-[11px] font-medium text-[#C8C8C8]">Encryption (Optional)</label>
        <div className="flex items-center bg-[#252525] rounded-md border border-[#3E3E3E] focus-within:border-[#60CDFF] px-2 py-1">
          <input
            type={showCompressPassword ? "text" : "password"}
            disabled={isCompressing}
            value={compressPassword}
            onChange={(e) => {
              setCompressPassword(e.target.value);
              if (e.target.value.trim().length > 0 && selectedEncryption === "none") {
                setSelectedEncryption("aes256");
              }
            }}
            placeholder="Password..."
            className="bg-transparent border-none outline-none text-white text-xs font-mono flex-1 disabled:opacity-50"
          />
          <button
            type="button"
            onClick={() => setShowCompressPassword(!showCompressPassword)}
            className="text-[#888] hover:text-white p-0.5 cursor-pointer text-xs"
          >
            {showCompressPassword ? "Hide" : "Show"}
          </button>
        </div>
      </div>

      {/* Live Staging Summary */}
      <div className="bg-[#182633]/60 border border-[#20495f]/70 rounded-lg p-3 space-y-1 text-[#C8C8C8]">
        <div className="flex justify-between text-[11px]">
          <span>Files Selected:</span>
          <span className="font-mono text-white font-semibold">{includedFilesCount} of {totalStagedFilesCount}</span>
        </div>
        {excludedCompressPaths.size > 0 && (
          <div className="flex justify-between text-[11px] text-rose-300">
            <span>Files Excluded:</span>
            <span className="font-mono font-semibold">{excludedCompressPaths.size} skipped</span>
          </div>
        )}
        <div className="flex justify-between text-[11px]">
          <span>Est. Payload:</span>
          <span className="font-mono text-[#60CDFF] font-semibold">{formatBytes(effectiveTotalSize)}</span>
        </div>
      </div>

      {/* Primary Action Button */}
      <div className="pt-1">
        {isCompressing ? (
          <button
            type="button"
            onClick={handleCancelCompress}
            disabled={isCancellingCompress}
            className="w-full py-2.5 bg-red-600 hover:bg-red-500 text-white font-semibold rounded-lg text-xs transition-colors flex items-center justify-center space-x-2 cursor-pointer shadow-lg"
          >
            <span>{isCancellingCompress ? "Cancelling..." : "Cancel Compression"}</span>
          </button>
        ) : (
          <button
            type="button"
            onClick={handleCompress}
            disabled={selectedFiles.length === 0}
            className="w-full py-3 bg-[#60CDFF] hover:bg-[#72D4FF] text-black font-bold rounded-lg text-xs tracking-wide uppercase transition-all flex items-center justify-center space-x-2 cursor-pointer shadow-lg hover:shadow-[0_0_20px_rgba(96,205,255,0.4)] disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <span>⚡</span>
            <span>Compress Now ({selectedFormat})</span>
          </button>
        )}
      </div>
    </div>
  );

  return (
    <>
      {/* Windows 11 File Drop Zone & Staging Inspector */}
      {selectedFiles.length > 0 ? (
        <div className="bg-[#2D2D2D] border border-[#3E3E3E] rounded-lg p-4 flex flex-col space-y-3">
          <div className="flex justify-between items-center text-xs border-b border-[#3E3E3E] pb-2">
            <div className="flex items-center space-x-2 text-[#A8A8A8] flex-wrap">
              <span className="text-white font-medium">
                {selectedFiles.length} item{selectedFiles.length > 1 ? "s" : ""} selected
              </span>
              {effectiveTotalSize > 0 && (
                <>
                  <span>•</span>
                  <span className="text-[#60CDFF] font-mono font-medium">
                    Total: {formatBytes(effectiveTotalSize)}
                  </span>
                </>
              )}
              {excludedCompressPaths.size > 0 && (
                <>
                  <span>•</span>
                  <span className="text-amber-400 bg-amber-500/10 border border-amber-500/30 px-2 py-0.5 rounded text-[11px] font-medium">
                    {includedFilesCount} of {totalStagedFilesCount} files included ({excludedCompressPaths.size} excluded)
                  </span>
                </>
              )}
              {isCompressing && (
                <span className="text-[10px] text-[#60CDFF] bg-[#60CDFF]/10 border border-[#60CDFF]/30 px-2 py-0.5 rounded font-medium">
                  Staging for Next Archive
                </span>
              )}
            </div>

            <div className="flex items-center space-x-2.5">
              {/* Workspace Mode Button */}
              <button
                type="button"
                onClick={() => setIsWorkspaceOpen(true)}
                className="px-2.5 py-1 text-xs font-semibold rounded transition-all flex items-center space-x-1.5 cursor-pointer bg-[#202020] hover:bg-[#282828] text-[#60CDFF] hover:text-[#90DCFF] border border-[#3E3E3E] hover:border-[#60CDFF]/50 shadow-sm"
                title="Open full workspace with folder hierarchy, file selection, and options sidebar"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
                </svg>
                <span>Workspace Mode</span>
              </button>

              {/* View Switcher: Tree vs List */}
              <div className="flex items-center bg-[#202020] p-0.5 rounded-md border border-[#3E3E3E]">
                <button
                  type="button"
                  onClick={() => setCompressViewMode("tree")}
                  className={`px-2.5 py-1 text-xs font-medium rounded transition-colors flex items-center space-x-1.5 cursor-pointer ${
                    compressViewMode === "tree"
                      ? "bg-[#60CDFF]/20 text-[#60CDFF] border border-[#60CDFF]/30 font-semibold shadow-sm"
                      : "text-[#A8A8A8] hover:text-white"
                  }`}
                  title="Hierarchical Tree & Folder Inspector"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                  <span>Tree</span>
                </button>
                <button
                  type="button"
                  onClick={() => setCompressViewMode("list")}
                  className={`px-2.5 py-1 text-xs font-medium rounded transition-colors flex items-center space-x-1.5 cursor-pointer ${
                    compressViewMode === "list"
                      ? "bg-[#60CDFF]/20 text-[#60CDFF] border border-[#60CDFF]/30 font-semibold shadow-sm"
                      : "text-[#A8A8A8] hover:text-white"
                  }`}
                  title="Compact List View"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                  </svg>
                  <span>List</span>
                </button>
              </div>

              <button
                type="button"
                onClick={() => {
                  setSelectedFiles([]);
                  if (!isCompressing) {
                    setCustomArchiveName("");
                  }
                }}
                className="text-red-400 hover:text-red-300 transition-colors cursor-pointer text-xs"
              >
                Clear all
              </button>
            </div>
          </div>

          {compressViewMode === "tree" ? (
            <div className="rounded-md overflow-hidden border border-[#3E3E3E]">
              <ArchiveTreeInspector
                archiveInfo={stagedArchiveInfo}
                selectedArchive={selectedFiles[0] || "staged"}
                isLoading={isInspectingStaged}
                onPreviewEntry={onPreviewStagedEntry}
                excludedPaths={excludedCompressPaths}
                onToggleExclude={onToggleCompressExclude}
                onIncludeAll={onCompressIncludeAll}
                isExpandedModal={isWorkspaceOpen}
                onCloseExpandedModal={() => setIsWorkspaceOpen(false)}
                onOpenExpandedModal={() => setIsWorkspaceOpen(true)}
                sidebar={renderWorkspaceSidebar()}
                sidebarTitle="Compression Options"
                inspectorTitle="Staged Items & Folders"
              />
            </div>
          ) : (
            <div className="max-h-56 overflow-y-auto flex flex-col space-y-2 custom-scrollbar pr-1">
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
                        <FileFormatIcon fileName={file} size="sm" />
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
          )}

          {compressViewMode !== "tree" && isWorkspaceOpen && (
            <ArchiveTreeInspector
              archiveInfo={stagedArchiveInfo}
              selectedArchive={selectedFiles[0] || "staged"}
              isLoading={isInspectingStaged}
              onPreviewEntry={onPreviewStagedEntry}
              excludedPaths={excludedCompressPaths}
              onToggleExclude={onToggleCompressExclude}
              onIncludeAll={onCompressIncludeAll}
              isExpandedModal={true}
              onCloseExpandedModal={() => setIsWorkspaceOpen(false)}
              onOpenExpandedModal={() => setIsWorkspaceOpen(true)}
              sidebar={renderWorkspaceSidebar()}
              sidebarTitle="Compression Options"
              inspectorTitle="Staged Items & Folders"
            />
          )}

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

      {/* Target Archive Name Card - Always visible with glowing Bayzip logo tile */}
      <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-4 flex flex-col space-y-2">
        <div className="flex justify-between items-center text-xs">
          <span className="font-medium text-[#C8C8C8]">Archive Name</span>
          <span className="text-[11px] text-[#888888]">Saved in source folder</span>
        </div>
        <div className="flex items-center space-x-2.5">
          <div className="w-9 h-9 rounded-lg bg-[#202020] border border-[#3E3E3E] flex items-center justify-center shrink-0 shadow-sm">
            <BayzipLogo className="w-5 h-5" />
          </div>
          <input
            type="text"
            disabled={isCompressing}
            value={isCompressing && activeCompressConfig ? activeCompressConfig.archiveName : customArchiveName}
            onChange={(e) => setCustomArchiveName(e.target.value)}
            placeholder={selectedFiles.length > 0 ? "Archive name..." : "Enter archive name..."}
            className="flex-1 bg-[#202020] text-sm text-white px-3 py-2 rounded-md border border-[#3E3E3E] focus:border-[#60CDFF] outline-none font-medium placeholder-[#666] disabled:opacity-50 disabled:cursor-not-allowed"
          />
          {/* Quick Format Dropdown Selector */}
          <div className="relative group shrink-0">
            <select
              disabled={isCompressing}
              value={isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat}
              onChange={(e) => {
                if (isCompressing) return;
                const newFmt = e.target.value;
                setSelectedFormat(newFmt);
                if (FORMAT_ALGORITHMS[newFmt]) {
                  setSelectedAlgorithm(FORMAT_ALGORITHMS[newFmt][0]);
                }
              }}
              className="appearance-none bg-[#202020] hover:bg-[#282828] text-xs font-mono text-[#60CDFF] font-semibold pl-3 pr-7 py-2 rounded-md border border-[#3E3E3E] hover:border-[#60CDFF]/60 focus:border-[#60CDFF] outline-none cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              title="Change archive compression format"
            >
              <option value=".bay" className="bg-[#202020] text-white">.bay</option>
              <option value=".bayg" className="bg-[#202020] text-white">.bayg (Gamer)</option>
              <option value=".zip" className="bg-[#202020] text-white">.zip</option>
              <option value=".7z" className="bg-[#202020] text-white">.7z</option>
              <option value=".tar" className="bg-[#202020] text-white">.tar</option>
            </select>
            <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2 text-[#60CDFF] group-hover:text-white transition-colors">
              <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6l4 4 4-4" />
              </svg>
            </div>
          </div>
        </div>
      </div>

      {/* Hardware Allocation & Thread Matrix Card */}
      <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6 space-y-4">
        <div className="flex justify-between items-center">
          <div>
            <h3 className="text-sm font-medium text-white">Hardware Allocation</h3>
            <div className="text-xs font-semibold text-[#60CDFF] mt-0.5">
              {getSliderLabel()}
            </div>
          </div>
          <span className="text-[10px] font-semibold uppercase tracking-wider bg-[#202020] text-[#60CDFF] px-2.5 py-1 rounded border border-[#3E3E3E]">
            {(isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".bayg"
              ? "BAYGAMER MULTI-CORE"
              : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".7z"
                ? "LZMA2 MULTI-CORE"
                : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".zip"
                  ? "ZIP ARCHIVE ENGINE"
                  : (isCompressing && activeCompressConfig ? activeCompressConfig.format : selectedFormat) === ".tar"
                    ? "TAR STREAM PACKAGING"
                    : "ZSTANDARD MULTI-CORE"}
          </span>
        </div>

        {/* Dynamic 5-Tier Hardware Allocation Slider */}
        <div className="space-y-1.5 pt-1">
          <input
            type="range"
            min="0"
            max="100"
            step="25"
            disabled={isCompressing}
            value={isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue}
            onChange={(e) => setSliderValue(Number(e.target.value))}
            className="w-full h-1.5 bg-[#202020] rounded-lg appearance-none cursor-pointer accent-[#60CDFF] disabled:opacity-50 disabled:cursor-not-allowed block"
          />
          <div className="relative w-full h-4 text-[11px] font-mono select-none">
            {[0, 25, 50, 75, 100].map((val) => {
              const currentVal = isCompressing && activeCompressConfig ? activeCompressConfig.sliderValue : sliderValue;
              const isSelected = currentVal === val;
              return (
                <span
                  key={val}
                  onClick={() => !isCompressing && setSliderValue(val)}
                  style={{
                    left: `calc(${val}% + ${(50 - val) * 0.16}px)`,
                    transform: "translateX(-50%)",
                  }}
                  className={`absolute transition-colors cursor-pointer whitespace-nowrap ${
                    isSelected ? "text-[#60CDFF] font-bold" : "text-[#A8A8A8] hover:text-white"
                  }`}
                  title={`Set Allocation to ${val}%`}
                >
                  {val}
                </span>
              );
            })}
          </div>
        </div>

        {/* Live Hardware Resource Estimates */}
        <div className="grid grid-cols-2 gap-4 pt-1">
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

      {/* Full Workspace Modal with Right-Hand Options Sidebar */}
      {isWorkspaceOpen && (
        <ArchiveTreeInspector
          archiveInfo={stagedArchiveInfo}
          selectedArchive={selectedFiles[0] || "staged"}
          isLoading={isInspectingStaged}
          onPreviewEntry={onPreviewStagedEntry}
          excludedPaths={excludedCompressPaths}
          onToggleExclude={onToggleCompressExclude}
          onIncludeAll={onCompressIncludeAll}
          isExpandedModal={true}
          onCloseExpandedModal={() => setIsWorkspaceOpen(false)}
          sidebar={renderWorkspaceSidebar()}
          sidebarTitle="Compression Options"
        />
      )}
    </>
  );
}

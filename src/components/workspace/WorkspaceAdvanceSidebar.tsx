import React from "react";
import { BayzipLogo } from "../BayzipLogo";
import { SystemSpecs } from "../../types";
import { FORMAT_ALGORITHMS } from "../CompressPage";

export interface WorkspaceAdvanceSidebarProps {
  // Archive Details
  archiveName: string;
  onArchiveNameChange: (name: string) => void;
  selectedFormat: string;
  onFormatChange: (fmt: string) => void;
  selectedAlgorithm?: string;
  onAlgorithmChange?: (algo: string) => void;

  // Destination / "Where to store"
  archiveToOriginalPath: boolean;
  onToggleArchiveToOriginalPath: (val: boolean) => void;
  compressDestDir: string;
  onCompressDestDirChange: (dir: string) => void;
  onSelectDestination: () => void;

  // Compression Profile & Hardware
  sliderValue: number;
  onSliderValueChange: (val: number) => void;
  sliderLabel: string;
  specs?: SystemSpecs | null;
  getEstimatedCores?: () => number;
  smartMediaBypass: boolean;
  onSmartMediaBypassChange: (val: boolean) => void;
  separateArchives?: boolean;
  onSeparateArchivesChange?: (val: boolean) => void;

  // Encryption
  selectedEncryption?: string;
  onEncryptionChange?: (enc: string) => void;
  compressPassword: string;
  onCompressPasswordChange: (pwd: string) => void;
  showCompressPassword: boolean;
  onToggleShowPassword: () => void;

  // Summary
  includedFilesCount: number;
  totalFilesCount: number;
  excludedFilesCount: number;
  effectivePayloadSize: number;

  // Actions
  isCompressing: boolean;
  isCancellingCompress: boolean;
  onCompress: () => void;
  onCancelCompress: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export const WorkspaceAdvanceSidebar: React.FC<WorkspaceAdvanceSidebarProps> = ({
  archiveName,
  onArchiveNameChange,
  selectedFormat,
  onFormatChange,
  selectedAlgorithm,
  onAlgorithmChange,
  archiveToOriginalPath,
  onToggleArchiveToOriginalPath,
  compressDestDir,
  onCompressDestDirChange,
  onSelectDestination,
  sliderValue,
  onSliderValueChange,
  sliderLabel,
  specs,
  getEstimatedCores,
  smartMediaBypass,
  onSmartMediaBypassChange,
  separateArchives = false,
  onSeparateArchivesChange,
  selectedEncryption = "none",
  onEncryptionChange,
  compressPassword,
  onCompressPasswordChange,
  showCompressPassword,
  onToggleShowPassword,
  includedFilesCount,
  totalFilesCount,
  excludedFilesCount,
  effectivePayloadSize,
  isCompressing,
  isCancellingCompress,
  onCompress,
  onCancelCompress,
}) => {
  const algorithmsForFormat = FORMAT_ALGORITHMS[selectedFormat] || [];

  const getEstimatedRAM = () => {
    if (selectedFormat === ".tar") return "Streaming (0 MB)";
    if (selectedFormat === ".zip") return "Standard (2 MB)";
    if (selectedFormat === ".7z") return sliderValue >= 75 ? "~1.2 GB" : "~350 MB";
    if (selectedFormat === ".bayg") return "Fast (64 MB)";
    if (sliderValue === 100) return "~1.8 GB (Deep)";
    if (sliderValue >= 75) return "~450 MB";
    return "~128 MB";
  };

  const estimatedCores = getEstimatedCores ? getEstimatedCores() : 1;

  return (
    <div className="space-y-3.5 text-xs select-none">
      {/* SECTION 1: TARGET ARCHIVE & DESTINATION */}
      <div className="bg-[#1C1C1C] rounded-xl border border-[#2D2D2D] p-3 space-y-2.5 shadow-sm">
        <div className="flex items-center space-x-1.5 text-[11px] font-bold text-[#A8A8A8] uppercase tracking-wider">
          <span className="text-[#60CDFF]">📁</span>
          <span>Target & Destination</span>
        </div>

        {/* Archive Name Input */}
        <div className="space-y-1">
          <label className="text-[11px] font-medium text-[#B8B8B8]">Archive Name</label>
          <div className="flex items-center bg-[#151515] rounded-lg border border-[#333333] focus-within:border-[#60CDFF] px-2.5 py-1.5 transition-all shadow-inner">
            <div className="w-5 h-5 flex items-center justify-center shrink-0 mr-2 text-[#60CDFF]">
              <BayzipLogo className="w-4 h-4" />
            </div>
            <input
              type="text"
              disabled={isCompressing}
              value={archiveName}
              onChange={(e) => onArchiveNameChange(e.target.value)}
              placeholder="Archive name..."
              className="bg-transparent border-none outline-none text-white text-xs font-mono flex-1 disabled:opacity-50 placeholder-[#555]"
            />
            <span className="text-[11px] text-[#60CDFF] font-mono font-bold bg-[#173042] px-2 py-0.5 rounded border border-[#60CDFF]/30 shrink-0 ml-1.5">
              {selectedFormat}
            </span>
          </div>
        </div>

        {/* Destination Folder */}
        <div className="space-y-2 pt-1.5 border-t border-[#282828]">
          <div className="flex items-center justify-between">
            <label className="text-[11px] font-medium text-[#B8B8B8]">Destination</label>
            <label className="flex items-center space-x-1.5 cursor-pointer text-[10px] text-[#A8A8A8] hover:text-white transition-colors">
              <input
                type="checkbox"
                disabled={isCompressing}
                checked={archiveToOriginalPath}
                onChange={(e) => onToggleArchiveToOriginalPath(e.target.checked)}
                className="accent-[#60CDFF] w-3.5 h-3.5 rounded cursor-pointer"
              />
              <span>Source Folder</span>
            </label>
          </div>

          {!archiveToOriginalPath && (
            <div className="flex items-center space-x-1.5 animate-in fade-in duration-150">
              <input
                type="text"
                disabled={isCompressing}
                value={compressDestDir}
                onChange={(e) => onCompressDestDirChange(e.target.value)}
                placeholder="Select custom output folder..."
                className="flex-1 bg-[#151515] text-white px-2.5 py-1.5 rounded-lg border border-[#333333] focus:border-[#60CDFF] outline-none font-mono text-[11px] disabled:opacity-50"
              />
              <button
                type="button"
                disabled={isCompressing}
                onClick={onSelectDestination}
                className="px-2.5 py-1.5 bg-[#252525] hover:bg-[#303030] text-[#60CDFF] hover:text-white rounded-lg border border-[#3E3E3E] hover:border-[#60CDFF]/60 transition-colors cursor-pointer text-[11px] font-medium shrink-0 disabled:opacity-50"
              >
                Browse...
              </button>
            </div>
          )}
        </div>
      </div>

      {/* SECTION 2: FORMAT & COMPRESSION ALGORITHM */}
      <div className="bg-[#1C1C1C] rounded-xl border border-[#2D2D2D] p-3 space-y-2.5 shadow-sm">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-1.5 text-[11px] font-bold text-[#A8A8A8] uppercase tracking-wider">
            <span className="text-[#60CDFF]">📦</span>
            <span>Format & Algorithm</span>
          </div>
          <span className="text-[10px] font-mono text-[#60CDFF] bg-[#162736] px-1.5 py-0.5 rounded border border-[#20495f]">
            {selectedFormat === ".bay"
              ? "Zstandard Engine"
              : selectedFormat === ".bayg"
                ? "Gamer Preset"
                : selectedFormat === ".7z"
                  ? "LZMA2 Multi-Core"
                  : selectedFormat === ".zip"
                    ? "ZIP Deflate"
                    : "TAR Pack"}
          </span>
        </div>

        {/* 5-Format Balanced Selector Grid */}
        <div className="grid grid-cols-5 gap-1 bg-[#151515] p-1 rounded-lg border border-[#2C2C2C]">
          {[
            { fmt: ".bay", label: ".bay", title: "Bayzip Standard (Zstandard Multi-Core)" },
            { fmt: ".bayg", label: ".bayg", title: "BayGamer Fast Decompress Pack" },
            { fmt: ".zip", label: ".zip", title: "Universal ZIP Format" },
            { fmt: ".7z", label: ".7z", title: "7-Zip LZMA2 Format" },
            { fmt: ".tar", label: ".tar", title: "TAR Uncompressed Packaging" },
          ].map((item) => {
            const isSel = selectedFormat === item.fmt;
            return (
              <button
                key={item.fmt}
                type="button"
                disabled={isCompressing}
                onClick={() => onFormatChange(item.fmt)}
                className={`py-1.5 px-1 rounded-md text-[11px] font-mono font-bold transition-all cursor-pointer text-center ${
                  isSel
                    ? "bg-[#60CDFF] text-black shadow-[0_0_12px_rgba(96,205,255,0.4)]"
                    : "text-[#888888] hover:text-white hover:bg-[#222222]"
                }`}
                title={item.title}
              >
                {item.label}
              </button>
            );
          })}
        </div>

        {/* Algorithm Dropdown */}
        {selectedFormat !== ".bayg" && algorithmsForFormat.length > 0 && onAlgorithmChange && (
          <div className="space-y-1 pt-1 border-t border-[#282828]">
            <div className="flex justify-between items-center">
              <label className="text-[11px] font-medium text-[#B8B8B8]">Compression Algorithm</label>
              <span className="text-[10px] text-[#777777] font-mono">{algorithmsForFormat.length} Available</span>
            </div>
            <div className="relative group">
              <select
                disabled={isCompressing}
                value={selectedAlgorithm || algorithmsForFormat[0]}
                onChange={(e) => onAlgorithmChange(e.target.value)}
                className="w-full appearance-none bg-[#151515] hover:bg-[#1E1E1E] text-[#60CDFF] font-semibold text-xs pl-3 pr-8 py-2 rounded-lg border border-[#333333] hover:border-[#4A4A4A] focus:border-[#60CDFF] outline-none cursor-pointer transition-colors disabled:opacity-50"
              >
                {algorithmsForFormat.map((algo) => (
                  <option key={algo} value={algo} className="bg-[#1C1C1C] text-white">
                    {algo}
                  </option>
                ))}
              </select>
              <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-[#60CDFF]">
                <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6l4 4 4-4" />
                </svg>
              </div>
            </div>
          </div>
        )}

        {selectedFormat === ".bayg" && (
          <div className="p-2 rounded-lg bg-[#142330] border border-[#20495f] flex items-center space-x-2 text-[11px] text-[#60CDFF]">
            <span>⚡</span>
            <span>Auto-tuned Level 3 for game assets & ultra-fast decompression</span>
          </div>
        )}
      </div>

      {/* SECTION 3: PERFORMANCE, HARDWARE ALLOCATION & ADVANCED SETTINGS */}
      <div className="bg-[#1C1C1C] rounded-xl border border-[#2D2D2D] p-3 space-y-2.5 shadow-sm">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-1.5 text-[11px] font-bold text-[#A8A8A8] uppercase tracking-wider">
            <span className="text-[#60CDFF]">⚡</span>
            <span>Hardware & Performance</span>
          </div>
          <span className="text-[10px] text-[#60CDFF] font-semibold bg-[#182833] px-2 py-0.5 rounded border border-[#23536b]">
            {sliderLabel}
          </span>
        </div>

        {/* 5-Tier Slider */}
        <div className="relative pt-1">
          <input
            type="range"
            min="0"
            max="100"
            step="25"
            disabled={isCompressing}
            value={sliderValue}
            onChange={(e) => onSliderValueChange(Number(e.target.value))}
            className="w-full h-1.5 bg-[#2B2B2B] rounded-lg appearance-none cursor-pointer accent-[#60CDFF] block"
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
                  onClick={() => !isCompressing && onSliderValueChange(tier.val)}
                  style={{
                    left: `calc(${tier.val}% + ${(50 - tier.val) * 0.16}px)`,
                    transform: "translateX(-50%)",
                  }}
                  className={`absolute transition-colors cursor-pointer whitespace-nowrap ${
                    isSelected ? "text-[#60CDFF] font-bold" : "text-[#777777] hover:text-[#C8C8C8]"
                  }`}
                >
                  {tier.label}
                </span>
              );
            })}
          </div>
        </div>

        {/* Live Resource Estimates Grid */}
        <div className="grid grid-cols-2 gap-2 pt-1">
          <div className="bg-[#151515] p-2 rounded-lg border border-[#2A2A2A] space-y-0.5">
            <div className="flex justify-between items-center text-[10px] text-[#888888]">
              <span>Target RAM</span>
              {specs && <span className="font-mono text-[#60CDFF]">{Math.round(specs.total_ram_gb)} GB</span>}
            </div>
            <p className="text-xs font-bold text-white font-mono">{getEstimatedRAM()}</p>
          </div>

          <div className="bg-[#151515] p-2 rounded-lg border border-[#2A2A2A] space-y-0.5">
            <div className="flex justify-between items-center text-[10px] text-[#888888]">
              <span>CPU Threads</span>
              {specs && <span className="font-mono text-[#60CDFF]">{specs.cpu_cores} Cores</span>}
            </div>
            <p className="text-xs font-bold text-white font-mono">
              {estimatedCores} <span className="text-[10px] font-normal text-[#888888]">/ {specs?.cpu_cores || "?"}</span>
            </p>
          </div>
        </div>

        {/* Ultra Archival Warning */}
        {selectedFormat === ".bay" && sliderValue === 100 && (
          <div className="p-2 rounded-lg bg-amber-500/10 border border-amber-500/30 flex items-start space-x-2 text-[10px] text-amber-200 animate-in fade-in duration-150">
            <span className="text-amber-400 font-bold shrink-0">⚠️</span>
            <span>Ultra Archival (Level 19): maximum compression ratio with higher memory and time allocation.</span>
          </div>
        )}

        {/* Options Switches */}
        <div className="pt-2 border-t border-[#282828] space-y-2">
          {/* Smart Media Bypass */}
          <label className="flex items-center justify-between cursor-pointer group">
            <div>
              <p className="text-[11px] font-medium text-white group-hover:text-[#60CDFF] transition-colors">
                Smart Media Bypass
              </p>
              <p className="text-[10px] text-[#777777]">Fast store pre-compressed media</p>
            </div>
            <input
              type="checkbox"
              disabled={isCompressing}
              checked={smartMediaBypass}
              onChange={(e) => onSmartMediaBypassChange(e.target.checked)}
              className="accent-[#60CDFF] w-4 h-4 rounded cursor-pointer"
            />
          </label>

          {/* Separate Archives */}
          {onSeparateArchivesChange && (
            <label className="flex items-center justify-between cursor-pointer group">
              <div>
                <p className="text-[11px] font-medium text-white group-hover:text-[#60CDFF] transition-colors">
                  Separate Dedicated Archives
                </p>
                <p className="text-[10px] text-[#777777]">Pack each root item independently</p>
              </div>
              <input
                type="checkbox"
                disabled={isCompressing}
                checked={separateArchives}
                onChange={(e) => onSeparateArchivesChange(e.target.checked)}
                className="accent-[#60CDFF] w-4 h-4 rounded cursor-pointer"
              />
            </label>
          )}
        </div>
      </div>

      {/* SECTION 4: SECURITY & ENCRYPTION */}
      <div className="bg-[#1C1C1C] rounded-xl border border-[#2D2D2D] p-3 space-y-2.5 shadow-sm">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-1.5 text-[11px] font-bold text-[#A8A8A8] uppercase tracking-wider">
            <span className="text-[#60CDFF]">🔒</span>
            <span>Security & Encryption</span>
          </div>
          {selectedEncryption !== "none" && (
            <span className="text-[9px] text-emerald-400 font-mono font-bold bg-emerald-950/60 px-1.5 py-0.5 rounded border border-emerald-700/60">
              AES-256
            </span>
          )}
        </div>

        {selectedFormat !== ".tar" ? (
          <div className="space-y-2">
            {onEncryptionChange && (
              <div className="flex items-center justify-between">
                <label className="text-[11px] font-medium text-[#B8B8B8]">Protection</label>
                <select
                  disabled={isCompressing}
                  value={selectedEncryption}
                  onChange={(e) => onEncryptionChange(e.target.value)}
                  className="bg-[#151515] text-[#60CDFF] text-xs font-mono font-semibold px-2 py-1 rounded-lg border border-[#333333] outline-none cursor-pointer"
                >
                  <option value="none" className="bg-[#1C1C1C] text-white">None (Standard)</option>
                  <option value="aes256" className="bg-[#1C1C1C] text-white">AES-256 Encryption</option>
                </select>
              </div>
            )}

            {(selectedEncryption !== "none" || compressPassword.trim().length > 0) && (
              <div className="flex items-center bg-[#151515] rounded-lg border border-[#333333] focus-within:border-[#60CDFF] px-2.5 py-1.5 animate-in fade-in duration-150">
                <input
                  type={showCompressPassword ? "text" : "password"}
                  disabled={isCompressing}
                  value={compressPassword}
                  onChange={(e) => onCompressPasswordChange(e.target.value)}
                  placeholder="Enter archive password..."
                  className="bg-transparent border-none outline-none text-white text-xs font-mono flex-1 disabled:opacity-50 placeholder-[#555]"
                />
                <button
                  type="button"
                  onClick={onToggleShowPassword}
                  className="text-[#888888] hover:text-white p-0.5 cursor-pointer text-[10px] font-semibold shrink-0 ml-1"
                >
                  {showCompressPassword ? "Hide" : "Show"}
                </button>
              </div>
            )}
          </div>
        ) : (
          <p className="text-[10px] text-[#777777]">TAR container does not support native encryption.</p>
        )}
      </div>

      {/* SECTION 5: LIVE STAGING SUMMARY & ACTION */}
      <div className="bg-gradient-to-r from-[#173042]/80 to-[#12222E]/80 border border-[#20495f] rounded-xl p-3 space-y-1.5 shadow-md">
        <div className="flex justify-between text-[11px]">
          <span className="text-[#A8A8A8]">Files Staged:</span>
          <span className="font-mono text-white font-bold">{includedFilesCount} of {totalFilesCount}</span>
        </div>
        {excludedFilesCount > 0 && (
          <div className="flex justify-between text-[11px] text-rose-300">
            <span>Excluded Files:</span>
            <span className="font-mono font-semibold">{excludedFilesCount} skipped</span>
          </div>
        )}
        <div className="flex justify-between text-[11px]">
          <span className="text-[#A8A8A8]">Est. Payload:</span>
          <span className="font-mono text-[#60CDFF] font-bold">{formatBytes(effectivePayloadSize)}</span>
        </div>
      </div>

      {/* Primary Action Button */}
      <div className="pt-1">
        {isCompressing ? (
          <button
            type="button"
            onClick={onCancelCompress}
            disabled={isCancellingCompress}
            className="w-full py-2.5 bg-red-600 hover:bg-red-500 text-white font-bold rounded-xl text-xs transition-colors flex items-center justify-center space-x-2 cursor-pointer shadow-lg"
          >
            <span>{isCancellingCompress ? "Cancelling..." : "Cancel Compression"}</span>
          </button>
        ) : (
          <button
            type="button"
            onClick={onCompress}
            disabled={includedFilesCount === 0}
            className="w-full py-2.5 bg-gradient-to-r from-[#00A3FF] to-[#0077CC] hover:from-[#1AB0FF] hover:to-[#0088EE] text-white font-bold rounded-xl text-xs transition-all flex items-center justify-center space-x-2 cursor-pointer shadow-[0_4px_16px_rgba(0,163,255,0.3)] hover:shadow-[0_4px_22px_rgba(0,163,255,0.5)] active:scale-[0.99] disabled:opacity-40 disabled:cursor-not-allowed"
          >
            <span>⚡ COMPRESS NOW ({selectedFormat.toUpperCase()})</span>
          </button>
        )}
      </div>
    </div>
  );
};

import React from "react";
import { BayzipLogo } from "../BayzipLogo";

export interface WorkspaceAdvanceSidebarProps {
  // Archive Details
  archiveName: string;
  onArchiveNameChange: (name: string) => void;
  selectedFormat: string;
  onFormatChange: (fmt: string) => void;

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
  smartMediaBypass: boolean;
  onSmartMediaBypassChange: (val: boolean) => void;

  // Encryption
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
  archiveToOriginalPath,
  onToggleArchiveToOriginalPath,
  compressDestDir,
  onCompressDestDirChange,
  onSelectDestination,
  sliderValue,
  onSliderValueChange,
  sliderLabel,
  smartMediaBypass,
  onSmartMediaBypassChange,
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
  return (
    <div className="space-y-3.5 text-xs select-none">
      {/* SECTION 1: Destination & Archive Target (Where to store) */}
      <div className="space-y-2">
        <div className="flex items-center space-x-1.5 text-[11px] font-bold text-[#A8A8A8] uppercase tracking-wider">
          <span>📍</span>
          <span>Where to Store & Archive Info</span>
        </div>

        {/* Output Destination Folder Card */}
        <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3 space-y-2">
          <div className="flex items-center justify-between">
            <label className="text-[11px] font-medium text-[#C8C8C8]">Destination Folder</label>
            <label className="flex items-center space-x-1.5 cursor-pointer text-[10px] text-[#A8A8A8] hover:text-white">
              <input
                type="checkbox"
                disabled={isCompressing}
                checked={archiveToOriginalPath}
                onChange={(e) => onToggleArchiveToOriginalPath(e.target.checked)}
                className="accent-[#60CDFF] w-3.5 h-3.5 cursor-pointer"
              />
              <span>Source Folder</span>
            </label>
          </div>

          {!archiveToOriginalPath && (
            <div className="flex items-center space-x-1.5">
              <input
                type="text"
                disabled={isCompressing}
                value={compressDestDir}
                onChange={(e) => onCompressDestDirChange(e.target.value)}
                placeholder="Choose output destination folder..."
                className="flex-1 bg-[#181818] text-white px-2.5 py-1.5 rounded-md border border-[#3A3A3A] focus:border-[#60CDFF] outline-none font-mono text-[11px] disabled:opacity-50"
              />
              <button
                type="button"
                disabled={isCompressing}
                onClick={onSelectDestination}
                className="px-2.5 py-1.5 bg-[#252525] hover:bg-[#323232] text-white rounded-md border border-[#3E3E3E] hover:border-[#60CDFF] transition-colors cursor-pointer text-[11px] font-medium shrink-0 disabled:opacity-50"
              >
                Browse...
              </button>
            </div>
          )}
        </div>

        {/* Archive Name Card */}
        <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3 space-y-1.5">
          <label className="text-[11px] font-medium text-[#C8C8C8]">Archive Name</label>
          <div className="flex items-center space-x-2">
            <div className="w-8 h-8 rounded-md bg-[#252525] border border-[#3E3E3E] flex items-center justify-center shrink-0">
              <BayzipLogo className="w-4 h-4" />
            </div>
            <div className="flex-1 flex items-center bg-[#181818] rounded-md border border-[#3E3E3E] focus-within:border-[#60CDFF] px-2 py-1">
              <input
                type="text"
                disabled={isCompressing}
                value={archiveName}
                onChange={(e) => onArchiveNameChange(e.target.value)}
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
        <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3 space-y-2">
          <label className="text-[11px] font-medium text-[#C8C8C8]">Archive Format</label>
          <div className="grid grid-cols-3 gap-1.5">
            {[".bay", ".bayg", ".zip", ".7z", ".tar"].map((fmt) => (
              <button
                key={fmt}
                type="button"
                disabled={isCompressing}
                onClick={() => onFormatChange(fmt)}
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
      </div>

      {/* SECTION 2: DIVIDER & ADVANCE SECTION */}
      <div className="pt-1">
        <div className="flex items-center space-x-2 my-2">
          <div className="flex-1 h-[1px] bg-[#333333]" />
          <span className="text-[10px] uppercase font-bold tracking-wider text-[#60CDFF] bg-[#1a3847] px-2 py-0.5 rounded border border-[#23536b]">
            ⚙️ Advance Options
          </span>
          <div className="flex-1 h-[1px] bg-[#333333]" />
        </div>

        <div className="space-y-2">
          {/* Compression Level Slider */}
          <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3 space-y-2">
            <div className="flex justify-between items-center">
              <label className="text-[11px] font-medium text-[#C8C8C8]">Compression Profile</label>
              <span className="text-[10px] text-[#60CDFF] font-semibold bg-[#182833] px-2 py-0.5 rounded border border-[#23536b]">
                {sliderLabel}
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
                onChange={(e) => onSliderValueChange(Number(e.target.value))}
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
          </div>

          {/* Smart Media Bypass Toggle */}
          <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3 flex items-center justify-between">
            <div>
              <p className="text-[11px] font-medium text-white">Smart Media Bypass</p>
              <p className="text-[10px] text-[#888888]">Store pre-compressed files directly</p>
            </div>
            <input
              type="checkbox"
              disabled={isCompressing}
              checked={smartMediaBypass}
              onChange={(e) => onSmartMediaBypassChange(e.target.checked)}
              className="accent-[#60CDFF] w-4 h-4 cursor-pointer"
            />
          </div>

          {/* Encryption (Optional) */}
          <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3 space-y-1.5">
            <div className="flex justify-between items-center">
              <label className="text-[11px] font-medium text-[#C8C8C8]">Encryption (Optional)</label>
              {compressPassword.trim().length > 0 && (
                <span className="text-[9px] text-emerald-400 font-mono font-bold bg-emerald-950/60 px-1.5 py-0.5 rounded border border-emerald-700/60">
                  AES-256
                </span>
              )}
            </div>
            <div className="flex items-center bg-[#181818] rounded-md border border-[#3E3E3E] focus-within:border-[#60CDFF] px-2 py-1">
              <input
                type={showCompressPassword ? "text" : "password"}
                disabled={isCompressing}
                value={compressPassword}
                onChange={(e) => onCompressPasswordChange(e.target.value)}
                placeholder="Enter archive password..."
                className="bg-transparent border-none outline-none text-white text-xs font-mono flex-1 disabled:opacity-50"
              />
              <button
                type="button"
                onClick={onToggleShowPassword}
                className="text-[#888888] hover:text-white p-0.5 cursor-pointer text-[10px] font-semibold"
              >
                {showCompressPassword ? "Hide" : "Show"}
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* SECTION 3: Live Staging Summary & Primary Action */}
      <div className="bg-[#172533]/70 border border-[#20495f]/80 rounded-lg p-3 space-y-1.5 text-[#C8C8C8]">
        <div className="flex justify-between text-[11px]">
          <span>Files Selected:</span>
          <span className="font-mono text-white font-semibold">
            {includedFilesCount} of {totalFilesCount}
          </span>
        </div>

        {excludedFilesCount > 0 && (
          <div className="flex justify-between text-[11px] text-rose-300">
            <span>Files Excluded:</span>
            <span className="font-mono font-semibold">{excludedFilesCount} skipped</span>
          </div>
        )}

        <div className="flex justify-between text-[11px]">
          <span>Est. Payload:</span>
          <span className="font-mono text-[#60CDFF] font-semibold">{formatBytes(effectivePayloadSize)}</span>
        </div>
      </div>

      {/* Primary Action Button */}
      <div className="pt-1">
        {isCompressing ? (
          <button
            type="button"
            onClick={onCancelCompress}
            disabled={isCancellingCompress}
            className="w-full py-2.5 bg-red-600 hover:bg-red-500 text-white font-semibold rounded-lg text-xs transition-colors flex items-center justify-center space-x-2 cursor-pointer shadow-lg"
          >
            <span>{isCancellingCompress ? "Cancelling..." : "Cancel Compression"}</span>
          </button>
        ) : (
          <button
            type="button"
            onClick={onCompress}
            disabled={includedFilesCount === 0}
            className="w-full py-2.5 bg-gradient-to-r from-[#00A3FF] to-[#0077CC] hover:from-[#1AB0FF] hover:to-[#0088EE] text-white font-semibold rounded-lg text-xs transition-all flex items-center justify-center space-x-2 cursor-pointer shadow-lg hover:shadow-cyan-500/20 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            <span>⚡ COMPRESS NOW ({selectedFormat.toUpperCase()})</span>
          </button>
        )}
      </div>
    </div>
  );
};

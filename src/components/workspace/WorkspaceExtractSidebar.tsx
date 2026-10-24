import React from "react";
import { BayzipLogo } from "../BayzipLogo";

export interface WorkspaceExtractSidebarProps {
  // Destination
  extractDest: string;
  onExtractDestChange: (dir: string) => void;
  onSelectDestination: () => void;
  extractToSubfolder: boolean;
  onToggleExtractToSubfolder: (val: boolean) => void;
  archiveStemName: string;

  // Options
  openFolderOnExtract: boolean;
  onToggleOpenFolderOnExtract: (val: boolean) => void;
  isEncrypted: boolean;
  extractPassword: string;
  onExtractPasswordChange: (pwd: string) => void;
  showPassword?: boolean;
  onToggleShowPassword?: () => void;

  // Archive details & stats
  archiveFilename: string;
  archiveFormat: string;
  totalFilesCount: number;
  includedFilesCount: number;
  excludedFilesCount: number;
  totalUncompressedSize: number;

  // Actions
  isExtracting: boolean;
  isCancellingExtract: boolean;
  onExtract: () => void;
  onCancelExtract: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export const WorkspaceExtractSidebar: React.FC<WorkspaceExtractSidebarProps> = ({
  extractDest,
  onExtractDestChange,
  onSelectDestination,
  extractToSubfolder,
  onToggleExtractToSubfolder,
  archiveStemName,
  openFolderOnExtract,
  onToggleOpenFolderOnExtract,
  isEncrypted,
  extractPassword,
  onExtractPasswordChange,
  showPassword = false,
  onToggleShowPassword,
  archiveFilename,
  archiveFormat,
  totalFilesCount,
  includedFilesCount,
  excludedFilesCount,
  totalUncompressedSize,
  isExtracting,
  isCancellingExtract,
  onExtract,
  onCancelExtract,
}) => {
  return (
    <div className="space-y-3.5 text-xs select-none">
      {/* SECTION 1: Destination & Target Path (Where to extract) */}
      <div className="space-y-2">
        <div className="flex items-center space-x-1.5 text-[11px] font-bold text-[#A8A8A8] uppercase tracking-wider">
          <span>📍</span>
          <span>Where to Extract</span>
        </div>

        {/* Destination Directory Card */}
        <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3 space-y-2">
          <div className="flex items-center justify-between">
            <label className="text-[11px] font-medium text-[#C8C8C8]">Destination Folder</label>
            <span className="text-[10px] text-[#888888] font-mono">Target Drive</span>
          </div>

          <div className="flex items-center space-x-1.5">
            <input
              type="text"
              disabled={isExtracting}
              value={extractDest}
              onChange={(e) => onExtractDestChange(e.target.value)}
              placeholder="Select folder to extract..."
              className="flex-1 bg-[#181818] text-white px-2.5 py-1.5 rounded-md border border-[#3A3A3A] focus:border-[#60CDFF] outline-none font-mono text-[11px] disabled:opacity-50"
            />
            <button
              type="button"
              disabled={isExtracting}
              onClick={onSelectDestination}
              className="px-2.5 py-1.5 bg-[#252525] hover:bg-[#323232] text-white rounded-md border border-[#3E3E3E] hover:border-[#60CDFF] transition-colors cursor-pointer text-[11px] font-medium shrink-0 disabled:opacity-50"
            >
              Browse...
            </button>
          </div>

          {/* Dedicated Subfolder Toggle */}
          <label className="flex items-center space-x-2 pt-1 cursor-pointer group">
            <input
              type="checkbox"
              disabled={isExtracting}
              checked={extractToSubfolder}
              onChange={(e) => onToggleExtractToSubfolder(e.target.checked)}
              className="accent-[#60CDFF] w-3.5 h-3.5 cursor-pointer"
            />
            <span className="text-[11px] text-[#A8A8A8] group-hover:text-white transition-colors">
              Extract into subfolder{" "}
              {archiveStemName ? (
                <span className="text-[#60CDFF] font-mono text-[10px]">({archiveStemName})</span>
              ) : null}
            </span>
          </label>
        </div>

        {/* Archive Info Header Card */}
        <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3 space-y-1.5">
          <label className="text-[11px] font-medium text-[#C8C8C8]">Loaded Archive</label>
          <div className="flex items-center space-x-2">
            <div className="w-8 h-8 rounded-md bg-[#252525] border border-[#3E3E3E] flex items-center justify-center shrink-0">
              <BayzipLogo className="w-4 h-4" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-white font-mono text-xs font-semibold truncate" title={archiveFilename}>
                {archiveFilename || "No archive loaded"}
              </div>
              <div className="flex items-center space-x-2 text-[10px] text-[#888888]">
                <span className="bg-[#173042] text-[#60CDFF] px-1.5 py-0.2 rounded font-bold uppercase">
                  {archiveFormat || "ARCHIVE"}
                </span>
                <span>{totalFilesCount} files</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* FLUENT DIVIDER LINE */}
      <div className="relative py-0.5">
        <div className="absolute inset-0 flex items-center">
          <div className="w-full border-t border-[#2D2D2D]" />
        </div>
        <div className="relative flex justify-center text-[10px] uppercase font-bold text-[#777777] bg-[#141414] px-2 w-max mx-auto tracking-wider">
          <span>EXTRACTION OPTIONS</span>
        </div>
      </div>

      {/* SECTION 2: Extraction Flags & Decryption */}
      <div className="space-y-2">
        {/* Open Folder on Completion */}
        <div className="bg-[#1F1F1F] rounded-lg border border-[#333333] p-3">
          <label className="flex items-center justify-between cursor-pointer group">
            <div className="flex flex-col pr-2">
              <span className="text-[11px] font-medium text-white group-hover:text-[#60CDFF] transition-colors">
                Auto-Open Folder
              </span>
              <span className="text-[10px] text-[#888888]">
                Open destination in Explorer after decompression
              </span>
            </div>
            <input
              type="checkbox"
              disabled={isExtracting}
              checked={openFolderOnExtract}
              onChange={(e) => onToggleOpenFolderOnExtract(e.target.checked)}
              className="accent-[#60CDFF] w-4 h-4 cursor-pointer"
            />
          </label>
        </div>

        {/* Decryption Password Card (when archive is encrypted or optional) */}
        {isEncrypted && (
          <div className="bg-[#1F1F1F] rounded-lg border border-amber-600/40 p-3 space-y-2">
            <div className="flex items-center justify-between">
              <label className="text-[11px] font-medium text-amber-300 flex items-center space-x-1">
                <span>🔒</span>
                <span>Archive Password Required</span>
              </label>
              <span className="text-[9px] text-amber-400 font-mono font-bold bg-amber-950/60 px-1.5 py-0.5 rounded border border-amber-700/60">
                Encrypted
              </span>
            </div>
            <div className="flex items-center bg-[#181818] rounded-md border border-[#3E3E3E] focus-within:border-amber-400 px-2 py-1">
              <input
                type={showPassword ? "text" : "password"}
                disabled={isExtracting}
                value={extractPassword}
                onChange={(e) => onExtractPasswordChange(e.target.value)}
                placeholder="Enter password to extract..."
                className="bg-transparent border-none outline-none text-white text-xs font-mono flex-1 disabled:opacity-50"
              />
              {onToggleShowPassword && (
                <button
                  type="button"
                  onClick={onToggleShowPassword}
                  className="text-[#888888] hover:text-white p-0.5 cursor-pointer text-[10px] font-semibold"
                >
                  {showPassword ? "Hide" : "Show"}
                </button>
              )}
            </div>
          </div>
        )}
      </div>

      {/* FLUENT DIVIDER LINE */}
      <div className="relative py-0.5">
        <div className="absolute inset-0 flex items-center">
          <div className="w-full border-t border-[#2D2D2D]" />
        </div>
      </div>

      {/* SECTION 3: Live Extraction Summary & Primary Action */}
      <div className="bg-[#172533]/70 border border-[#20495f]/80 rounded-lg p-3 space-y-1.5 text-[#C8C8C8]">
        <div className="flex justify-between text-[11px]">
          <span>Files to Extract:</span>
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
          <span>Uncompressed Size:</span>
          <span className="font-mono text-[#60CDFF] font-semibold">{formatBytes(totalUncompressedSize)}</span>
        </div>
      </div>

      {/* Primary Action Button */}
      <div className="pt-1">
        {isExtracting ? (
          <button
            type="button"
            onClick={onCancelExtract}
            disabled={isCancellingExtract}
            className="w-full py-2.5 bg-red-600 hover:bg-red-500 text-white font-semibold rounded-lg text-xs transition-colors flex items-center justify-center space-x-2 cursor-pointer shadow-lg"
          >
            <span>{isCancellingExtract ? "Cancelling..." : "Cancel Extraction"}</span>
          </button>
        ) : (
          <button
            type="button"
            onClick={onExtract}
            disabled={totalFilesCount === 0 || includedFilesCount === 0}
            className="w-full py-2.5 bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white font-semibold rounded-lg text-xs transition-all flex items-center justify-center space-x-2 cursor-pointer shadow-lg hover:shadow-emerald-500/20 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            <span>
              ⚡ {excludedFilesCount > 0 ? `EXTRACT ${includedFilesCount} FILES` : "EXTRACT ALL NOW"}
            </span>
          </button>
        )}
      </div>
    </div>
  );
};

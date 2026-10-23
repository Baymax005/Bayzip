import { useState } from "react";
import { ArchiveInfo, ExtractProgress } from "../types";
import { ArchiveTreeInspector } from "./ArchiveTreeInspector";
import { BayzipLogo } from "./BayzipLogo";
import { useDynamicThroughput } from "../hooks/useDynamicThroughput";

export interface ExtractPageProps {
  selectedArchive: string | null;
  archiveInfo: ArchiveInfo | null;
  isInspectingArchive: boolean;
  extractDest: string;
  setExtractDest: (dest: string) => void;
  extractToSubfolder: boolean;
  setExtractToSubfolder: (val: boolean) => void;
  openFolderOnExtract: boolean;
  setOpenFolderOnExtract: (val: boolean) => void;
  extractPassword: string;
  setExtractPassword: (pwd: string) => void;
  showExtractPassword: boolean;
  setShowExtractPassword: (val: boolean) => void;
  isExtracting: boolean;
  isCancellingExtract: boolean;
  extractProgress: ExtractProgress | null;
  extractError: string | null;
  excludedPaths: Set<string>;
  includedFilesCount: number;
  handleToggleExclude: (path: string, isDir?: boolean) => void;
  handleIncludeAll: () => void;
  handleSelectArchive: () => void;
  handleSelectDestFolder: () => void;
  handleExtract: () => void;
  handleCancelExtract: () => void;
  onClearArchive: () => void;
  selectArchive: (filePath: string, pwd?: string) => void;
  handlePreviewEntry: (entryName: string) => void;
  isWorkspaceOpen?: boolean;
  setIsWorkspaceOpen?: (open: boolean) => void;
  onSwitchMode?: (newMode: "compress" | "extract") => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

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
    ".bayg",
  ];
  for (const ext of compoundExts) {
    if (lower.endsWith(ext)) {
      return filename.slice(0, -ext.length);
    }
  }
  return filename.replace(/\.[^/.]+$/, "");
}

export function ExtractPage({
  selectedArchive,
  archiveInfo,
  isInspectingArchive,
  extractDest,
  setExtractDest,
  extractToSubfolder,
  setExtractToSubfolder,
  openFolderOnExtract,
  setOpenFolderOnExtract,
  extractPassword,
  setExtractPassword,
  showExtractPassword,
  setShowExtractPassword,
  isExtracting,
  isCancellingExtract,
  extractProgress,
  extractError,
  excludedPaths,
  includedFilesCount,
  handleToggleExclude,
  handleIncludeAll,
  handleSelectArchive,
  handleSelectDestFolder,
  handleExtract,
  handleCancelExtract,
  onClearArchive,
  selectArchive,
  handlePreviewEntry,
  isWorkspaceOpen: isWorkspaceOpenProp,
  setIsWorkspaceOpen: setIsWorkspaceOpenProp,
  onSwitchMode,
}: ExtractPageProps) {
  const [internalWorkspaceOpen, setInternalWorkspaceOpen] = useState(false);
  const isWorkspaceOpen = isWorkspaceOpenProp !== undefined ? isWorkspaceOpenProp : internalWorkspaceOpen;
  const setIsWorkspaceOpen = setIsWorkspaceOpenProp || setInternalWorkspaceOpen;

  const { speedText, etaText } = useDynamicThroughput({
    isActive: isExtracting,
    bytesProcessed: extractProgress?.bytes_extracted ?? 0,
    totalBytes: archiveInfo?.total_uncompressed_size ?? 0,
  });

  const renderExtractSidebar = () => (
    <div className="space-y-4 text-xs">
      {/* Destination Directory */}
      <div className="bg-[#1C1C1C] rounded-lg border border-[#333333] p-3 space-y-1.5">
        <label className="text-[11px] font-medium text-[#C8C8C8]">Destination Folder</label>
        <div className="flex space-x-1.5">
          <input
            type="text"
            disabled={isExtracting}
            value={extractDest}
            onChange={(e) => setExtractDest(e.target.value)}
            placeholder="Destination directory..."
            className="flex-1 bg-[#252525] text-xs text-white px-2 py-1.5 rounded border border-[#3E3E3E] focus:border-[#60CDFF] outline-none font-mono disabled:opacity-50"
          />
          <button
            type="button"
            disabled={isExtracting}
            onClick={handleSelectDestFolder}
            className="px-2.5 py-1.5 bg-[#252525] hover:bg-[#333333] text-white rounded border border-[#3E3E3E] text-xs transition-colors cursor-pointer shrink-0"
          >
            Browse
          </button>
        </div>
      </div>

      {/* Password (if encrypted) */}
      <div className="bg-[#1C1C1C] rounded-lg border border-[#333333] p-3 space-y-1.5">
        <div className="flex justify-between items-center">
          <label className="text-[11px] font-medium text-[#C8C8C8]">Archive Password</label>
          {archiveInfo?.is_encrypted && (
            <span className="text-[10px] text-amber-400 font-semibold">Required</span>
          )}
        </div>
        <div className="flex items-center bg-[#252525] rounded border border-[#3E3E3E] focus-within:border-[#60CDFF] px-2 py-1">
          <input
            type={showExtractPassword ? "text" : "password"}
            disabled={isExtracting}
            value={extractPassword}
            onChange={(e) => setExtractPassword(e.target.value)}
            placeholder={archiveInfo?.is_encrypted ? "Enter archive password..." : "Unencrypted"}
            className="bg-transparent border-none outline-none text-white text-xs font-mono flex-1 disabled:opacity-50"
          />
          <button
            type="button"
            onClick={() => setShowExtractPassword(!showExtractPassword)}
            className="text-[#888] hover:text-white p-0.5 text-xs cursor-pointer"
          >
            {showExtractPassword ? "Hide" : "Show"}
          </button>
        </div>
      </div>

      {/* Extraction Options Toggles */}
      <div className="bg-[#1C1C1C] rounded-lg border border-[#333333] p-3 space-y-2.5">
        <label className="flex items-center justify-between cursor-pointer">
          <span className="text-[11px] text-[#A8A8A8]">Extract into subfolder</span>
          <input
            type="checkbox"
            disabled={isExtracting}
            checked={extractToSubfolder}
            onChange={(e) => setExtractToSubfolder(e.target.checked)}
            className="accent-[#60CDFF] w-4 h-4 cursor-pointer"
          />
        </label>
        <label className="flex items-center justify-between cursor-pointer">
          <span className="text-[11px] text-[#A8A8A8]">Open folder when done</span>
          <input
            type="checkbox"
            disabled={isExtracting}
            checked={openFolderOnExtract}
            onChange={(e) => setOpenFolderOnExtract(e.target.checked)}
            className="accent-[#60CDFF] w-4 h-4 cursor-pointer"
          />
        </label>
      </div>

      {/* Summary */}
      <div className="bg-[#182633]/60 border border-[#20495f]/70 rounded-lg p-3 space-y-1 text-[#C8C8C8]">
        <div className="flex justify-between text-[11px]">
          <span>Files Selected:</span>
          <span className="font-mono text-white font-semibold">
            {archiveInfo ? (archiveInfo.file_count || 0) - excludedPaths.size : 0} of {archiveInfo?.file_count || 0}
          </span>
        </div>
        {excludedPaths.size > 0 && (
          <div className="flex justify-between text-[11px] text-rose-300">
            <span>Files Skipped:</span>
            <span className="font-mono font-semibold">{excludedPaths.size} excluded</span>
          </div>
        )}
        <div className="flex justify-between text-[11px]">
          <span>Archive Size:</span>
          <span className="font-mono text-[#60CDFF] font-semibold">{formatBytes(archiveInfo?.size || 0)}</span>
        </div>
      </div>

      {/* Primary Action Button */}
      <div className="pt-1">
        {isExtracting ? (
          <button
            type="button"
            onClick={handleCancelExtract}
            disabled={isCancellingExtract}
            className="w-full py-2.5 bg-red-600 hover:bg-red-500 text-white font-semibold rounded-lg text-xs transition-colors flex items-center justify-center space-x-2 cursor-pointer shadow-lg"
          >
            <span>{isCancellingExtract ? "Cancelling..." : "Cancel Extraction"}</span>
          </button>
        ) : (
          <button
            type="button"
            onClick={handleExtract}
            disabled={!selectedArchive}
            className="w-full py-3 bg-[#60CDFF] hover:bg-[#72D4FF] text-black font-bold rounded-lg text-xs tracking-wide uppercase transition-all flex items-center justify-center space-x-2 cursor-pointer shadow-lg hover:shadow-[0_0_20px_rgba(96,205,255,0.4)] disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <span>📦</span>
            <span>
              {excludedPaths.size > 0
                ? `Extract (${archiveInfo ? (archiveInfo.file_count || 0) - excludedPaths.size : 0} Files)`
                : "Extract All Files"}
            </span>
          </button>
        )}
      </div>
    </div>
  );

  return (
    <div className="space-y-6">
      {/* Archive Selection Area */}
      {selectedArchive && archiveInfo ? (
        <div className="bg-[#2D2D2D] border border-[#3E3E3E] rounded-lg p-5 flex flex-col space-y-4">
          <div className="flex justify-between items-center text-xs border-b border-[#3E3E3E] pb-3">
            <span className="text-xs uppercase tracking-wider font-semibold text-[#60CDFF]">
              Archive Selected
            </span>
            <div className="flex items-center space-x-2.5">
              <button
                type="button"
                onClick={() => setIsWorkspaceOpen(true)}
                className="px-3 py-1.5 text-xs font-semibold rounded-md transition-all flex items-center space-x-1.5 cursor-pointer bg-gradient-to-r from-[#173042] to-[#12222E] hover:from-[#1F415A] hover:to-[#172D3D] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/40 hover:border-[#60CDFF]/70 shadow-[0_0_10px_rgba(96,205,255,0.15)]"
                title="Open full workspace with folder hierarchy, archive inspection, and extraction options"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
                </svg>
                <span>Workspace</span>
              </button>
              <button
                type="button"
                disabled={isExtracting}
                onClick={() => {
                  if (isExtracting) return;
                  onClearArchive();
                }}
                className="text-red-400 hover:text-red-300 transition-colors cursor-pointer text-xs disabled:opacity-50 disabled:cursor-not-allowed"
                title={isExtracting ? "Extraction in progress" : undefined}
              >
                Clear
              </button>
            </div>
          </div>

          <div className="flex items-center space-x-4 bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
            <div className="relative w-12 h-12 rounded-xl bg-[#142330] border border-[#20495f] flex items-center justify-center shrink-0 shadow">
              <BayzipLogo className="w-7 h-7" />
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
            <div className="flex items-center space-x-2 shrink-0">
              <button
                type="button"
                disabled={isExtracting}
                onClick={handleSelectArchive}
                className="text-xs px-3 py-1.5 bg-[#2D2D2D] hover:bg-[#383838] border border-[#3E3E3E] text-white rounded-md transition-colors cursor-pointer shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
                title={isExtracting ? "Extraction in progress" : undefined}
              >
                Change
              </button>
              <button
                type="button"
                disabled={isExtracting}
                onClick={onClearArchive}
                className="text-xs px-3 py-1.5 bg-rose-950/30 hover:bg-rose-900/60 border border-rose-800/50 hover:border-rose-600 text-rose-300 hover:text-white rounded-md transition-colors cursor-pointer shrink-0 disabled:opacity-50 disabled:cursor-not-allowed flex items-center space-x-1"
                title="Remove / Close this archive"
              >
                <span>✕</span>
                <span>Remove</span>
              </button>
            </div>
          </div>

          {/* Hierarchical Tree-Type Archive Directory Inspector */}
          <ArchiveTreeInspector
            archiveInfo={archiveInfo}
            selectedArchive={selectedArchive}
            extractPassword={extractPassword}
            isLoading={isInspectingArchive}
            onUnlockPreview={() => selectArchive(selectedArchive, extractPassword.trim())}
            onPreviewEntry={handlePreviewEntry}
            excludedPaths={excludedPaths}
            onToggleExclude={handleToggleExclude}
            onIncludeAll={handleIncludeAll}
            isExpandedModal={false}
            onOpenExpandedModal={() => setIsWorkspaceOpen(true)}
            sidebar={renderExtractSidebar()}
            sidebarTitle="Extraction Options"
            inspectorTitle="Archive Explorer & Folders"
            mode="extract"
            onSwitchMode={onSwitchMode}
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
              Supports <span className="text-[#60CDFF]">.bay</span>, <span className="text-[#60CDFF]">.bayg</span>, <span className="text-[#60CDFF]">.zip</span>, <span className="text-[#60CDFF]">.7z</span>, <span className="text-[#60CDFF]">.rar</span>, and <span className="text-[#60CDFF]">.tar</span>
            </p>
          </div>
          <div className="flex flex-wrap items-center justify-center gap-2">
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
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setIsWorkspaceOpen(true);
              }}
              className="text-xs font-semibold px-4 py-2 bg-gradient-to-r from-[#173042] to-[#12222E] hover:from-[#1F415A] hover:to-[#172D3D] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/40 hover:border-[#60CDFF]/70 rounded-md transition-all cursor-pointer flex items-center space-x-1.5 shadow-[0_0_12px_rgba(96,205,255,0.15)]"
              title="Open full workspace to browse drives and inspect archives"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
              </svg>
              <span>Workspace</span>
            </button>
          </div>
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
              className={`w-full bg-[#202020] text-sm text-white pl-3 pr-10 py-2 rounded-md border outline-none font-mono text-xs placeholder-[#666] transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
                archiveInfo?.is_encrypted
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
          <label className={`flex items-center group transition-opacity ${isExtracting ? "cursor-not-allowed opacity-50 pointer-events-none" : "cursor-pointer"}`}>
            <div className="relative">
              <input
                type="checkbox"
                disabled={isExtracting}
                className="sr-only"
                checked={extractToSubfolder}
                onChange={() => !isExtracting && setExtractToSubfolder(!extractToSubfolder)}
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
              {archiveInfo?.filename || selectedArchive ? (
                <span className="text-[#60CDFF] font-mono text-xs">
                  (
                  {archiveInfo?.filename
                    ? getArchiveStemName(archiveInfo.filename)
                    : getArchiveStemName(selectedArchive?.split(/[/\\]/).pop() || "")}
                  )
                </span>
              ) : (
                <span className="text-[#777777] font-mono text-xs">
                  (subfolder)
                </span>
              )}
            </div>
          </label>

          <label className={`flex items-center group transition-opacity ${isExtracting ? "cursor-not-allowed opacity-50 pointer-events-none" : "cursor-pointer"}`}>
            <div className="relative">
              <input
                type="checkbox"
                disabled={isExtracting}
                className="sr-only"
                checked={openFolderOnExtract}
                onChange={() => !isExtracting && setOpenFolderOnExtract(!openFolderOnExtract)}
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
              className="h-full bg-gradient-to-r from-[#059669] via-[#10B981] to-[#34D399] transition-all duration-150 ease-out rounded-full shadow-[0_0_10px_rgba(16,185,129,0.7)]"
              style={{ width: `${extractProgress ? Math.max(2, extractProgress.percent) : 2}%` }}
            ></div>
          </div>

          <div className="flex justify-between items-center text-[11px] text-[#8A8A8A]">
            <div className="flex items-center space-x-2 min-w-0">
              <span className="truncate">
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
              {speedText && (
                <>
                  <span className="text-[#555]">•</span>
                  <span className="font-mono text-[#D0D0D0] font-medium shrink-0">{speedText}</span>
                </>
              )}
            </div>
            <div className="flex items-center space-x-2 shrink-0 ml-2">
              {etaText && (
                <span className="text-amber-400 font-mono font-medium">
                  {etaText}
                </span>
              )}
              <span className="text-[#60CDFF] font-medium">
                {archiveInfo?.format || "Archive"}
              </span>
            </div>
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
      <div className="flex items-center justify-between pt-2">
        <div>
          {selectedArchive && excludedPaths.size > 0 && (
            <button
              type="button"
              disabled={isExtracting}
              onClick={handleIncludeAll}
              className="text-xs text-[#A8A8A8] hover:text-white px-3 py-2 rounded-md bg-[#252525] hover:bg-[#323232] border border-[#3E3E3E] transition-all flex items-center space-x-1.5 cursor-pointer disabled:opacity-50"
              title="Include all excluded files and directories"
            >
              <svg className="w-3.5 h-3.5 text-[#60CDFF]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              <span>Include All ({excludedPaths.size} excluded)</span>
            </button>
          )}
        </div>
        <button
          type="button"
          disabled={isExtracting && isCancellingExtract}
          onClick={isExtracting ? handleCancelExtract : handleExtract}
          className={`px-8 py-2.5 font-semibold rounded-md shadow-sm transition-all text-sm flex items-center space-x-2 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed ${
            isExtracting
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
                ? excludedPaths.size > 0
                  ? `Extract ${includedFilesCount} Files (${excludedPaths.size} Excluded)`
                  : archiveInfo?.file_count
                    ? `Extract All (${archiveInfo.file_count} Files)`
                    : "Extract Now"
                : "Select Archive to Extract"}
          </span>
        </button>
      </div>

      {/* Full Workspace Modal with Right-Hand Options Sidebar */}
      {isWorkspaceOpen && (
        <ArchiveTreeInspector
          archiveInfo={
            archiveInfo || {
              path: "",
              filename: "No Archive Loaded",
              size: 0,
              format: "ZIP",
              is_archive: true,
              is_encrypted: false,
              file_count: 0,
              total_uncompressed_size: 0,
              entries: [],
            }
          }
          selectedArchive={selectedArchive || "archive"}
          extractPassword={extractPassword}
          isLoading={isInspectingArchive}
          onUnlockPreview={() => {
            if (selectedArchive) selectArchive(selectedArchive, extractPassword.trim());
          }}
          onPreviewEntry={handlePreviewEntry}
          excludedPaths={excludedPaths}
          onToggleExclude={handleToggleExclude}
          onIncludeAll={handleIncludeAll}
          isExpandedModal={true}
          onCloseExpandedModal={() => setIsWorkspaceOpen(false)}
          sidebar={renderExtractSidebar()}
          sidebarTitle="Extraction Options"
          inspectorTitle="Archive Explorer & Drives"
          operationProgress={
            isExtracting && extractProgress
              ? {
                  percent: extractProgress.percent,
                  label: "Extracting Archive...",
                  current_file: extractProgress.current_file,
                  bytes_processed: extractProgress.bytes_extracted,
                  onCancel: handleCancelExtract,
                  isCancelling: isCancellingExtract,
                }
              : null
          }
          mode="extract"
          onSwitchMode={onSwitchMode}
        />
      )}
    </div>
  );
}

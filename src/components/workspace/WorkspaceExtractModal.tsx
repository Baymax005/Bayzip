import React, { useEffect, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useModalDismiss } from "../../hooks/useModalDismiss";

export interface WorkspaceExtractModalProps {
  isOpen: boolean;
  onClose: () => void;
  archivePath: string;
  archiveFilename: string;
  archiveFormat: string;
  fileCount: number;
  totalUncompressedSize: number;
  isEncrypted: boolean;

  // Destination Path
  destination: string;
  onDestinationChange: (dest: string) => void;

  // Extraction Options
  extractToSubfolder: boolean;
  onToggleExtractToSubfolder: (val: boolean) => void;
  openFolderOnExtract: boolean;
  onToggleOpenFolderOnExtract: (val: boolean) => void;

  // Password Decryption
  password: string;
  onPasswordChange: (pwd: string) => void;
  showPassword: boolean;
  onToggleShowPassword: () => void;

  // Execution
  isExtracting: boolean;
  extractError: string | null;
  onConfirmExtract: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

function getArchiveStem(filename: string): string {
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

export const WorkspaceExtractModal: React.FC<WorkspaceExtractModalProps> = ({
  isOpen,
  onClose,
  archivePath,
  archiveFilename,
  archiveFormat,
  fileCount,
  totalUncompressedSize,
  isEncrypted,
  destination,
  onDestinationChange,
  extractToSubfolder,
  onToggleExtractToSubfolder,
  openFolderOnExtract,
  onToggleOpenFolderOnExtract,
  password,
  onPasswordChange,
  showPassword,
  onToggleShowPassword,
  isExtracting,
  extractError,
  onConfirmExtract,
}) => {
  const modalContainerRef = useRef<HTMLDivElement>(null);
  const destInputRef = useRef<HTMLInputElement>(null);
  const passwordInputRef = useRef<HTMLInputElement>(null);
  const stem = getArchiveStem(archiveFilename);

  const { handleBackdropClick } = useModalDismiss({
    isOpen,
    onClose,
    disabled: isExtracting,
  });

  // Auto-focus input when modal opens (password if encrypted, destination otherwise)
  useEffect(() => {
    if (!isOpen) return;
    const timer = setTimeout(() => {
      if (isEncrypted && passwordInputRef.current) {
        passwordInputRef.current.focus();
      } else if (destInputRef.current) {
        destInputRef.current.focus();
      }
    }, 80);
    return () => clearTimeout(timer);
  }, [isOpen, isEncrypted]);

  if (!isOpen) return null;

  const handleBrowseFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Destination Folder for Extraction",
        defaultPath: destination || undefined,
      });
      if (selected && typeof selected === "string") {
        onDestinationChange(selected);
      }
    } catch (err) {
      console.error("Failed to select destination folder:", err);
    }
  };

  // Keyboard navigation & submission handler
  const handleKeyDown = (e: React.KeyboardEvent) => {
    // 1. Enter key submission (avoid triggering if focused on a button)
    if (e.key === "Enter" && !isExtracting) {
      const target = e.target as HTMLElement | null;
      if (target?.tagName === "BUTTON") {
        return;
      }
      e.preventDefault();
      onConfirmExtract();
      return;
    }

    // 2. Tab focus trap within modal dialog
    if (e.key === "Tab" && modalContainerRef.current) {
      const focusableElements = modalContainerRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])'
      );
      if (focusableElements.length > 0) {
        const firstEl = focusableElements[0];
        const lastEl = focusableElements[focusableElements.length - 1];

        if (e.shiftKey) {
          if (document.activeElement === firstEl) {
            e.preventDefault();
            lastEl.focus();
          }
        } else {
          if (document.activeElement === lastEl) {
            e.preventDefault();
            firstEl.focus();
          }
        }
      }
    }
  };

  return (
    <div
      onClick={handleBackdropClick}
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm animate-in fade-in duration-150 select-none"
    >
      <div
        ref={modalContainerRef}
        className="bg-[#1E1E1E] border border-[#3E3E3E] rounded-xl shadow-2xl w-full max-w-xl overflow-hidden flex flex-col text-white font-sans animate-in zoom-in-95 duration-150"
        onKeyDown={handleKeyDown}
      >
        {/* Header: Title & Close */}
        <div className="px-5 py-3.5 bg-[#252525] border-b border-[#333333] flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <div className="w-8 h-8 rounded-lg bg-[#173042] border border-[#60CDFF]/40 text-[#60CDFF] flex items-center justify-center text-sm font-bold shadow-[0_0_10px_rgba(96,205,255,0.2)]">
              ⚡
            </div>
            <div>
              <h2 className="text-sm font-bold text-white tracking-wide">
                Extraction Path and Options
              </h2>
              <p className="text-[11px] text-[#A8A8A8] truncate max-w-md font-mono" title={archivePath || archiveFilename}>
                {archiveFilename || "Archive"}
              </p>
            </div>
          </div>
          <button
            type="button"
            disabled={isExtracting}
            onClick={onClose}
            className="p-1 rounded-md text-[#888888] hover:text-white hover:bg-[#333333] transition-colors cursor-pointer disabled:opacity-50"
            title="Cancel & Close (Esc)"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Archive Metadata Bar */}
        <div className="px-5 py-2.5 bg-[#171717] border-b border-[#2B2B2B] flex items-center justify-between text-xs">
          <div className="flex items-center space-x-2">
            <span className="px-2 py-0.5 rounded text-[10px] font-bold font-mono uppercase bg-[#173042] text-[#60CDFF] border border-[#60CDFF]/40">
              {archiveFormat || "ARCHIVE"}
            </span>
            <span className="text-[#888888] font-mono">
              {fileCount} {fileCount === 1 ? "file" : "files"}
            </span>
            <span className="text-[#555555]">•</span>
            <span className="text-[#C8C8C8] font-mono">
              {formatBytes(totalUncompressedSize)}
            </span>
          </div>

          {isEncrypted && (
            <span className="px-2 py-0.5 rounded text-[10px] font-semibold flex items-center space-x-1 bg-amber-950/40 border border-amber-600/40 text-amber-300">
              <svg className="w-3 h-3 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
              </svg>
              <span>AES Encrypted</span>
            </span>
          )}
        </div>

        {/* Modal Body */}
        <div className="p-5 space-y-4 text-xs overflow-y-auto max-h-[70vh]">
          {/* Section 1: Destination Path */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label className="text-xs font-semibold text-[#DEDEDE] flex items-center space-x-1.5">
                <span>Destination Folder</span>
                <span className="text-[10px] font-normal text-[#888888]">(Where to unpack)</span>
              </label>
            </div>
            <div className="flex items-center space-x-2">
              <input
                ref={destInputRef}
                type="text"
                disabled={isExtracting}
                value={destination}
                onChange={(e) => onDestinationChange(e.target.value)}
                placeholder="Choose or enter destination path..."
                className="flex-1 bg-[#141414] text-white px-3 py-2 rounded-lg border border-[#3E3E3E] focus:border-[#60CDFF] focus:ring-1 focus:ring-[#60CDFF]/30 outline-none font-mono text-xs transition-colors disabled:opacity-50"
              />
              <button
                type="button"
                disabled={isExtracting}
                onClick={handleBrowseFolder}
                className="px-3.5 py-2 bg-[#282828] hover:bg-[#333333] text-white rounded-lg border border-[#444444] hover:border-[#60CDFF] transition-all cursor-pointer font-medium text-xs flex items-center space-x-1.5 shrink-0 disabled:opacity-50"
                title="Browse folder via Windows dialog"
              >
                <svg className="w-3.5 h-3.5 text-[#60CDFF]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                </svg>
                <span>Browse...</span>
              </button>
            </div>
          </div>

          {/* Section 2: Extraction Checkbox Options */}
          <div className="bg-[#161616] rounded-lg border border-[#2F2F2F] p-3 space-y-2.5">
            {/* Extract to Subfolder */}
            <label className="flex items-start space-x-2.5 cursor-pointer group">
              <input
                type="checkbox"
                disabled={isExtracting}
                checked={extractToSubfolder}
                onChange={(e) => onToggleExtractToSubfolder(e.target.checked)}
                className="mt-0.5 w-4 h-4 rounded bg-[#1B1B1B] border-[#444444] text-[#60CDFF] focus:ring-0 focus:ring-offset-0 cursor-pointer accent-[#60CDFF]"
              />
              <div className="flex-1 select-none">
                <div className="text-xs font-medium text-[#E0E0E0] group-hover:text-white transition-colors flex items-center space-x-1.5">
                  <span>Extract to subfolder</span>
                  <span className="font-mono text-[11px] text-[#60CDFF] bg-[#173042] px-1.5 py-0.5 rounded">
                    \{stem}\
                  </span>
                </div>
                <p className="text-[10px] text-[#888888] mt-0.5">
                  Creates a dedicated subfolder to avoid cluttering your destination directory.
                </p>
              </div>
            </label>

            {/* Open Destination Folder When Done */}
            <label className="flex items-start space-x-2.5 cursor-pointer group pt-1 border-t border-[#242424]">
              <input
                type="checkbox"
                disabled={isExtracting}
                checked={openFolderOnExtract}
                onChange={(e) => onToggleOpenFolderOnExtract(e.target.checked)}
                className="mt-0.5 w-4 h-4 rounded bg-[#1B1B1B] border-[#444444] text-[#60CDFF] focus:ring-0 focus:ring-offset-0 cursor-pointer accent-[#60CDFF]"
              />
              <div className="flex-1 select-none">
                <div className="text-xs font-medium text-[#E0E0E0] group-hover:text-white transition-colors">
                  Open destination folder after extraction
                </div>
                <p className="text-[10px] text-[#888888] mt-0.5">
                  Automatically launches Windows Explorer displaying the extracted files.
                </p>
              </div>
            </label>
          </div>

          {/* Section 3: Password Decryption */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label className="text-xs font-semibold text-[#DEDEDE] flex items-center space-x-1.5">
                <svg className="w-3.5 h-3.5 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                </svg>
                <span>Password</span>
                <span className={`text-[10px] font-normal ${isEncrypted ? "text-amber-400 font-semibold" : "text-[#888888]"}`}>
                  {isEncrypted ? "(Required for encrypted archive)" : "(Leave empty if none)"}
                </span>
              </label>
            </div>

            <div className="relative">
              <input
                ref={passwordInputRef}
                type={showPassword ? "text" : "password"}
                disabled={isExtracting}
                value={password}
                onChange={(e) => onPasswordChange(e.target.value)}
                placeholder={isEncrypted ? "Enter archive password..." : "Password (optional)..."}
                className={`w-full bg-[#141414] text-white text-xs pl-3 pr-10 py-2 rounded-lg border outline-none font-mono placeholder-[#555555] transition-colors disabled:opacity-50 ${
                  isEncrypted && !password
                    ? "border-amber-500/60 focus:border-amber-400 focus:ring-1 focus:ring-amber-400/30"
                    : "border-[#3E3E3E] focus:border-[#60CDFF] focus:ring-1 focus:ring-[#60CDFF]/30"
                }`}
              />
              <button
                type="button"
                tabIndex={-1}
                onClick={onToggleShowPassword}
                className="absolute inset-y-0 right-0 pr-3 flex items-center text-[#777777] hover:text-white cursor-pointer"
                title={showPassword ? "Hide password" : "Show password"}
              >
                {showPassword ? (
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l18 18" />
                  </svg>
                ) : (
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                  </svg>
                )}
              </button>
            </div>
          </div>

          {/* Error Banner */}
          {extractError && (
            <div className="bg-rose-950/40 border border-rose-800/60 rounded-lg p-3 text-xs text-rose-300 flex items-start space-x-2 animate-in fade-in">
              <svg className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              <div className="flex-1 break-words leading-relaxed font-sans">
                {extractError}
              </div>
            </div>
          )}
        </div>

        {/* Modal Footer */}
        <div className="px-5 py-3.5 bg-[#252525] border-t border-[#333333] flex items-center justify-between">
          <span className="text-[11px] text-[#777777] font-mono hidden sm:inline">
            Press <kbd className="px-1 py-0.5 bg-[#1B1B1B] border border-[#3E3E3E] rounded text-[10px] text-[#A8A8A8]">Enter</kbd> to extract
          </span>

          <div className="flex items-center space-x-2.5 ml-auto">
            <button
              type="button"
              disabled={isExtracting}
              onClick={onClose}
              className="px-4 py-2 rounded-lg bg-[#2A2A2A] hover:bg-[#333333] text-[#CCCCCC] hover:text-white border border-[#3E3E3E] transition-colors cursor-pointer text-xs font-semibold disabled:opacity-50"
            >
              Cancel
            </button>

            <button
              type="button"
              disabled={isExtracting}
              onClick={onConfirmExtract}
              className="px-5 py-2 rounded-lg bg-gradient-to-r from-[#173042] via-[#1B3B52] to-[#12222E] hover:from-[#1F415A] hover:to-[#172D3D] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/60 hover:border-[#60CDFF] shadow-[0_0_15px_rgba(96,205,255,0.25)] transition-all cursor-pointer text-xs font-bold flex items-center space-x-2 disabled:opacity-50"
            >
              {isExtracting ? (
                <>
                  <svg className="w-3.5 h-3.5 animate-spin text-[#60CDFF]" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z"></path>
                  </svg>
                  <span>Extracting...</span>
                </>
              ) : (
                <>
                  <span className="text-sm">⚡</span>
                  <span>Extract Now</span>
                </>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

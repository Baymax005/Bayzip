import { useState, useEffect, useMemo } from "react";
import { ArchiveEntryPreview } from "../types";

interface FilePreviewModalProps {
  isOpen: boolean;
  onClose: () => void;
  preview: ArchiveEntryPreview | null;
  isLoading: boolean;
  error: string | null;
  archivePath: string;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export function FilePreviewModal({
  isOpen,
  onClose,
  preview,
  isLoading,
  error,
  archivePath,
}: FilePreviewModalProps) {
  const [copied, setCopied] = useState(false);
  const [wrapText, setWrapText] = useState(true);

  // Split and cap lines to prevent DOM lockup on large files (>3,000 lines)
  const { renderedLines, totalLineCount, isCapped } = useMemo(() => {
    if (!preview?.content || preview.preview_type === "image") {
      return { renderedLines: [], totalLineCount: 0, isCapped: false };
    }
    const allLines = preview.content.split("\n");
    const MAX_LINES = 3000;
    if (allLines.length > MAX_LINES) {
      return {
        renderedLines: allLines.slice(0, MAX_LINES),
        totalLineCount: allLines.length,
        isCapped: true,
      };
    }
    return {
      renderedLines: allLines,
      totalLineCount: allLines.length,
      isCapped: false,
    };
  }, [preview?.content, preview?.preview_type]);

  // Close on Escape key
  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const handleCopy = () => {
    if (preview?.content && preview.preview_type !== "image") {
      navigator.clipboard.writeText(preview.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const getFileExtension = (name: string): string => {
    const parts = name.split(".");
    return parts.length > 1 ? parts.pop()?.toUpperCase() || "FILE" : "FILE";
  };

  return (
    <div
      onClick={onClose}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-md p-4 animate-in fade-in duration-150"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="bg-[#1E1E1E] border border-[#3E3E3E] rounded-xl shadow-2xl flex flex-col w-full max-w-4xl h-[80vh] overflow-hidden relative"
      >
        {/* Top Header Bar */}
        <div className="px-4 py-3 bg-[#262626] border-b border-[#3E3E3E] flex items-center justify-between shrink-0">
          <div className="flex items-center space-x-3 min-w-0 flex-1 mr-4">
            {/* File Format Icon */}
            <div className="w-8 h-8 rounded-lg bg-[#181818] border border-[#3A3A3A] flex items-center justify-center shrink-0">
              {preview?.is_restricted ? (
                <svg className="w-4 h-4 text-rose-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m0 0v2m0-2h2m-2 0H10m11-5a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
              ) : preview?.preview_type === "image" ? (
                <svg className="w-4 h-4 text-purple-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
                </svg>
              ) : (
                <svg className="w-4 h-4 text-[#60CDFF]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
              )}
            </div>

            <div className="min-w-0">
              <div className="flex items-center space-x-2">
                <h3 className="text-sm font-semibold text-white truncate font-mono" title={preview?.name}>
                  {preview ? preview.name.split("/").pop() : "File Preview"}
                </h3>
                {preview && (
                  <span className="text-[10px] font-mono uppercase bg-[#333333] text-[#A8A8A8] px-2 py-0.5 rounded border border-[#444444]">
                    {getFileExtension(preview.name)}
                  </span>
                )}
                {preview?.is_restricted && (
                  <span className="text-[10px] font-semibold bg-rose-500/20 text-rose-400 border border-rose-500/30 px-2 py-0.5 rounded-full flex items-center space-x-1">
                    <span>🛡️ Execution Blocked</span>
                  </span>
                )}
                {preview?.preview_type === "text" && !preview.is_restricted && (
                  <span className="text-[10px] font-medium bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 px-2 py-0.5 rounded-full">
                    Safe In-Memory Preview
                  </span>
                )}
              </div>
              <p className="text-[11px] text-[#888888] truncate font-mono mt-0.5">
                {preview ? preview.name : archivePath}
              </p>
            </div>
          </div>

          {/* Action Buttons */}
          <div className="flex items-center space-x-2">
            {preview?.preview_type === "text" && (
              <>
                <button
                  type="button"
                  onClick={() => setWrapText(!wrapText)}
                  className={`text-xs px-2.5 py-1 rounded border transition-colors cursor-pointer ${
                    wrapText
                      ? "bg-[#333333] text-[#60CDFF] border-[#444444]"
                      : "bg-[#2A2A2A] text-[#888888] border-[#3A3A3A] hover:text-white"
                  }`}
                  title="Toggle Word Wrap"
                >
                  Wrap
                </button>
                <button
                  type="button"
                  onClick={handleCopy}
                  className="text-xs px-2.5 py-1 bg-[#333333] hover:bg-[#3E3E3E] text-white rounded border border-[#444444] transition-colors flex items-center space-x-1 cursor-pointer"
                  title="Copy content to clipboard"
                >
                  {copied ? (
                    <>
                      <svg className="w-3.5 h-3.5 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                      </svg>
                      <span className="text-emerald-400">Copied</span>
                    </>
                  ) : (
                    <>
                      <svg className="w-3.5 h-3.5 text-[#A8A8A8]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                      </svg>
                      <span>Copy</span>
                    </>
                  )}
                </button>
              </>
            )}

            <button
              type="button"
              onClick={onClose}
              className="text-[#888888] hover:text-white p-1.5 rounded hover:bg-[#333333] transition-colors cursor-pointer"
              title="Close Preview (Esc)"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        {/* Warning / Security Notice Banner */}
        {preview?.warning && (
          <div
            className={`px-4 py-2 border-b text-xs flex items-center space-x-2 ${
              preview.is_restricted
                ? "bg-rose-950/40 border-rose-800/40 text-rose-300"
                : "bg-amber-950/30 border-amber-800/30 text-amber-300"
            }`}
          >
            <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
            <span className="flex-1">{preview.warning}</span>
          </div>
        )}

        {/* Preview Content Area */}
        <div className="flex-1 overflow-auto bg-[#181818] relative custom-scrollbar">
          {isLoading ? (
            <div className="h-full flex flex-col items-center justify-center space-y-3 p-8 text-center">
              <div className="w-9 h-9 border-2 border-[#60CDFF] border-t-transparent rounded-full animate-spin" />
              <p className="text-sm font-medium text-white">Decompressing file from memory...</p>
              <p className="text-xs text-[#888888]">Zero bytes are being written to disk.</p>
            </div>
          ) : error ? (
            <div className="h-full flex flex-col items-center justify-center space-y-3 p-8 text-center">
              <div className="w-12 h-12 rounded-full bg-rose-500/10 text-rose-400 flex items-center justify-center">
                <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
              </div>
              <p className="text-sm font-semibold text-white">Preview Failed</p>
              <p className="text-xs text-rose-400 max-w-md font-mono bg-[#202020] p-3 rounded border border-rose-900/30">
                {error}
              </p>
            </div>
          ) : preview?.is_restricted && !preview.content ? (
            /* Restricted Executable Binary Screen */
            <div className="h-full flex flex-col items-center justify-center space-y-4 p-8 text-center max-w-md mx-auto">
              <div className="w-16 h-16 rounded-2xl bg-rose-500/10 border border-rose-500/30 text-rose-400 flex items-center justify-center shadow-lg">
                <svg className="w-8 h-8" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                </svg>
              </div>
              <div>
                <h4 className="text-base font-semibold text-white">Restricted Binary Execution Protected</h4>
                <p className="text-xs text-[#A8A8A8] mt-1.5 leading-relaxed">
                  Executable binaries (<span className="text-rose-400 font-mono">.exe, .dll, .sys, .com</span>) cannot be launched or executed directly from the archive viewer.
                </p>
              </div>
              <div className="bg-[#222222] border border-[#333333] rounded-lg p-3 w-full text-left space-y-1.5 text-xs font-mono">
                <div className="flex justify-between text-[#888888]">
                  <span>Target Entry:</span>
                  <span className="text-white truncate max-w-[200px]" title={preview.name}>{preview.name}</span>
                </div>
                <div className="flex justify-between text-[#888888]">
                  <span>Status:</span>
                  <span className="text-rose-400 font-semibold">Strict Execution Lockout</span>
                </div>
                <div className="flex justify-between text-[#888888]">
                  <span>Security Layer:</span>
                  <span className="text-emerald-400 font-medium">Memory Isolation Guard</span>
                </div>
              </div>
              <p className="text-[11px] text-[#666666]">
                To run or inspect this executable, extract the archive to a trusted folder on your machine.
              </p>
            </div>
          ) : preview?.preview_type === "image" && preview.content ? (
            /* Image Preview */
            <div className="h-full flex flex-col items-center justify-center p-6 bg-[#141414] select-none">
              <div className="max-w-full max-h-full flex items-center justify-center p-2 rounded-lg border border-[#2A2A2A] bg-[radial-gradient(#262626_1px,transparent_1px)] [background-size:16px_16px]">
                <img
                  src={preview.content}
                  alt={preview.name}
                  className="max-h-[60vh] max-w-full object-contain rounded shadow-lg"
                />
              </div>
              <div className="mt-3 text-xs text-[#888888] font-mono flex items-center space-x-3">
                <span>{preview.mime_type}</span>
                <span>•</span>
                <span>{formatBytes(preview.size)}</span>
              </div>
            </div>
          ) : preview?.content ? (
            /* Text & Code Viewer with Line Numbers */
            <div className="h-full flex flex-col text-xs font-mono">
              <div className="flex-1 overflow-auto custom-scrollbar p-4 bg-[#141414] text-[#E0E0E0] select-text">
                <pre
                  className={`font-mono text-xs leading-5 ${
                    wrapText ? "whitespace-pre-wrap break-words" : "whitespace-pre"
                  }`}
                >
                  {renderedLines.map((line, idx) => (
                    <div key={idx} className="flex hover:bg-[#202020] transition-colors">
                      <span className="w-12 shrink-0 select-none text-[#555555] text-right pr-4">
                        {idx + 1}
                      </span>
                      <span className="flex-1 font-mono">{line || " "}</span>
                    </div>
                  ))}
                </pre>
                {isCapped && (
                  <div className="mt-4 p-3 rounded-lg bg-[#221D12] border border-amber-500/40 text-amber-300 text-xs flex items-center space-x-2.5 shadow-sm">
                    <span className="text-base shrink-0">⚠️</span>
                    <span>
                      Showing first <strong>3,000</strong> lines of{" "}
                      <strong>{totalLineCount.toLocaleString()}</strong> lines in memory preview. Extract the archive to view the full file.
                    </span>
                  </div>
                )}
              </div>
            </div>
          ) : (
            /* Binary Unsupported Screen */
            <div className="h-full flex flex-col items-center justify-center space-y-3 p-8 text-center max-w-md mx-auto">
              <div className="w-14 h-14 rounded-2xl bg-[#262626] border border-[#3E3E3E] flex items-center justify-center text-[#888888]">
                <svg className="w-7 h-7" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
              </div>
              <div>
                <h4 className="text-sm font-semibold text-white">No Direct Preview Available</h4>
                <p className="text-xs text-[#888888] mt-1 leading-relaxed">
                  This file format is binary and cannot be rendered as text or an image.
                </p>
              </div>
              <div className="text-xs text-[#888888] font-mono bg-[#202020] px-3 py-1.5 rounded border border-[#333333]">
                Size: <span className="text-white font-semibold">{preview ? formatBytes(preview.size) : "0 B"}</span>
              </div>
            </div>
          )}
        </div>

        {/* Bottom Status Bar */}
        <div className="px-4 py-2 bg-[#202020] border-t border-[#3E3E3E] flex items-center justify-between text-xs font-mono text-[#888888] shrink-0">
          <div className="flex items-center space-x-3">
            <span>Size: <span className="text-white">{preview ? formatBytes(preview.size) : "0 B"}</span></span>
            <span>•</span>
            <span>Mode: <span className="text-[#60CDFF]">Zero-Disk Streaming</span></span>
          </div>
          <div className="flex items-center space-x-2">
            <span>Press <kbd className="px-1.5 py-0.5 bg-[#333333] border border-[#444444] rounded text-[10px] text-white">Esc</kbd> to exit</span>
          </div>
        </div>
      </div>
    </div>
  );
}

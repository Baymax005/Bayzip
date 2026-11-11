import { useEffect } from "react";
import { ExtractProgress } from "../types";
import { formatBytes } from "../utils/format";
import { useDynamicThroughput } from "../hooks/useDynamicThroughput";
import { useDraggableModal } from "../hooks/useDraggableModal";

export interface ExtractModalProps {
  isOpen: boolean;
  onClose: () => void;
  extractDest: string;
  setExtractDest: (dest: string) => void;
  handleSelectDestFolder: () => void;
  extractToSubfolder: boolean;
  setExtractToSubfolder: (val: boolean) => void;
  openFolderOnExtract: boolean;
  setOpenFolderOnExtract: (val: boolean) => void;
  onStartExtract: () => void;
  isExtracting?: boolean;
  isCancelling?: boolean;
  archiveName?: string;
  extractProgress?: ExtractProgress | null;
  totalUncompressedSize?: number;
  onCancelExtract?: () => void;
  onMinimizeToFooter?: () => void;
  onDetachToDesktop?: () => void;
}

export function ExtractModal({
  isOpen,
  onClose,
  extractDest,
  setExtractDest,
  handleSelectDestFolder,
  extractToSubfolder,
  setExtractToSubfolder,
  openFolderOnExtract,
  setOpenFolderOnExtract,
  onStartExtract,
  isExtracting = false,
  isCancelling = false,
  archiveName,
  extractProgress = null,
  totalUncompressedSize = 0,
  onCancelExtract,
  onMinimizeToFooter,
  onDetachToDesktop,
}: ExtractModalProps) {
  const {
    position,
    isDragging,
    handleMouseDown,
    resetPosition,
    isDisplaced,
  } = useDraggableModal({ isOpen });

  const { speedText, etaText } = useDynamicThroughput({
    isActive: isExtracting,
    bytesProcessed: extractProgress?.bytes_extracted ?? 0,
    totalBytes: totalUncompressedSize,
  });

  const percent = extractProgress?.percent ?? 0;
  const targetArchiveTitle = archiveName || "Archive";

  // Keyboard shortcut: Escape to dismiss configuration modal
  useEffect(() => {
    if (!isOpen || isExtracting) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, isExtracting, onClose]);

  if (!isOpen) return null;

  return (
    // Scoped strictly inside File Viewer bounds with responsive draggable window positioning
    <div
      onClick={(e) => {
        if (e.target === e.currentTarget && !isExtracting) {
          onClose();
        }
      }}
      className={`absolute inset-0 z-40 flex items-center justify-center p-4 select-none ${
        isDisplaced ? "bg-black/10 pointer-events-none" : "bg-black/30"
      }`}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          transform: `translate(${position.x}px, ${position.y}px)`,
        }}
        className={`max-w-[420px] w-full bg-[#1C1C1F] rounded-xl border border-[#383842] p-5 shadow-[0_24px_60px_rgba(0,0,0,0.75)] flex flex-col space-y-4 text-xs text-[#F4F4F6] pointer-events-auto transition-shadow ${
          isDragging ? "shadow-[0_25px_70px_rgba(56,189,248,0.25)] cursor-grabbing" : ""
        }`}
      >
        {/* ================================================================= */}
        {/* MODE A: LIVE IN-PLACE STREAMING PROGRESS VIEW (NO PAUSE BUTTON)   */}
        {/* ================================================================= */}
        {isExtracting ? (
          <div className="space-y-4">
            {/* Header with target name & percentage badge (Draggable) */}
            <div
              onMouseDown={handleMouseDown}
              className="flex items-center justify-between pb-2 border-b border-[#2E2E34] cursor-grab active:cursor-grabbing select-none"
              title="Click and drag to move stream window"
            >
              <div className="flex items-center space-x-2 truncate mr-2">
                <span className="w-2 h-2 rounded-full bg-[#38BDF8] animate-pulse shrink-0"></span>
                <span className="font-semibold text-white tracking-wide truncate">
                  Extracting <span className="text-[#38BDF8]">{targetArchiveTitle}</span>
                </span>
                <span className="text-[9px] text-[#888888] bg-[#252529] px-1.5 py-0.5 rounded border border-[#323238] shrink-0 pointer-events-none">
                  Draggable
                </span>
              </div>
              <div className="flex items-center space-x-2 shrink-0">
                {isDisplaced && (
                  <button
                    type="button"
                    onClick={resetPosition}
                    className="text-[10px] text-[#888888] hover:text-[#38BDF8] px-1.5 py-0.5 rounded hover:bg-[#252529] transition-colors"
                    title="Reset to center"
                  >
                    Center
                  </button>
                )}
                <span className="font-mono text-sm font-bold text-[#38BDF8]">
                  {Math.round(percent)}%
                </span>
              </div>
            </div>

            {/* h-2 smooth progress bar */}
            <div className="w-full h-2 rounded-full bg-[#252529] overflow-hidden">
              <div
                className="h-full bg-[#38BDF8] transition-all duration-300"
                style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
              />
            </div>

            {/* Compact 2x2 Telemetry Grid (tabular-nums font-mono) */}
            <div className="grid grid-cols-2 gap-2.5 bg-[#141416] p-3 rounded-[6px] border border-[#2E2E34] text-[11px] font-mono">
              <div>
                <span className="text-[#9CA3AF] block font-sans text-[10px]">Throughput:</span>
                <span className="text-white font-semibold">{speedText || "Calculating..."}</span>
              </div>
              <div>
                <span className="text-[#9CA3AF] block font-sans text-[10px]">ETA:</span>
                <span className="text-white font-semibold">{etaText || "--:--"}</span>
              </div>
              <div className="col-span-2 truncate">
                <span className="text-[#9CA3AF] block font-sans text-[10px]">Extracting Entry:</span>
                <span className="text-[#38BDF8] truncate block" title={extractProgress?.current_file || ""}>
                  {extractProgress?.current_file ? extractProgress.current_file.split(/[/\\]/).pop() : "Unpacking..."}
                </span>
              </div>
              <div className="col-span-2 pt-1 border-t border-[#232328] flex justify-between">
                <span className="text-[#9CA3AF] font-sans text-[10px]">Extracted Bytes:</span>
                <span className="text-white font-semibold">
                  {formatBytes(extractProgress?.bytes_extracted || 0)} / {formatBytes(totalUncompressedSize)}
                </span>
              </div>
            </div>

            {/* Footer Actions (Minimize to Footer, Detach to Desktop & Cancel) */}
            <div className="flex items-center justify-between pt-2 border-t border-[#2E2E34]">
              <div className="flex items-center space-x-1.5">
                <button
                  type="button"
                  onClick={() => {
                    if (onMinimizeToFooter) onMinimizeToFooter();
                  }}
                  className="px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] hover:text-[#38BDF8] border border-[#323238] font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1"
                  title="Minimize extraction progress to bottom status bar"
                >
                  <span>▾</span>
                  <span>Minimize to Footer</span>
                </button>

                {onDetachToDesktop && (
                  <button
                    type="button"
                    onClick={onDetachToDesktop}
                    className="px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] hover:text-[#38BDF8] border border-[#323238] font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1"
                    title="Detach progress into a standalone floating desktop window"
                  >
                    <span>⧉</span>
                    <span>Detach</span>
                  </button>
                )}
              </div>

              <button
                type="button"
                onClick={onCancelExtract}
                disabled={isCancelling}
                className={`px-3 py-1.5 rounded-[4px] font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1.5 ${
                  isCancelling
                    ? "bg-rose-950/60 border border-rose-700 text-rose-300 opacity-80 cursor-not-allowed"
                    : "bg-[#252529] hover:bg-rose-950/40 text-[#9CA3AF] hover:text-rose-400 border border-[#323238] hover:border-rose-800"
                }`}
                title="Abort extraction and remove partial files"
              >
                {isCancelling ? (
                  <>
                    <svg className="animate-spin w-3 h-3 text-rose-400" viewBox="0 0 24 24" fill="none">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z" />
                    </svg>
                    <span>Cancelling...</span>
                  </>
                ) : (
                  <span>Cancel</span>
                )}
              </button>
            </div>
          </div>
        ) : (
          /* ================================================================= */
          /* MODE B: CONFIGURATION DIALOG                                     */
          /* ================================================================= */
          <>
            {/* 1. Header (Draggable) */}
            <div
              onMouseDown={handleMouseDown}
              className="flex items-center justify-between pb-2 border-b border-[#2E2E34] cursor-grab active:cursor-grabbing select-none"
              title="Click and drag to move window"
            >
              <div className="flex items-center space-x-2">
                <span className="text-sm font-semibold text-white tracking-wide">
                  Extract Archive
                </span>
                <span className="text-[10px] text-[#888888] bg-[#252529] px-1.5 py-0.5 rounded border border-[#323238] pointer-events-none">
                  Draggable
                </span>
                {archiveName && (
                  <span className="text-[11px] font-mono text-[#9CA3AF] truncate max-w-[180px]">
                    ({archiveName})
                  </span>
                )}
              </div>
              <div className="flex items-center space-x-1.5">
                {isDisplaced && (
                  <button
                    type="button"
                    onClick={resetPosition}
                    className="text-[10px] text-[#888888] hover:text-[#38BDF8] px-1.5 py-0.5 rounded hover:bg-[#252529] transition-colors"
                    title="Reset to center"
                  >
                    Center
                  </button>
                )}
                <button
                  type="button"
                  onClick={onClose}
                  className="p-1 text-[#9CA3AF] hover:text-white rounded-[4px] hover:bg-[#252529] transition-colors cursor-pointer"
                  title="Close"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-4 h-4">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </div>

            {/* 2. Destination Path Row */}
            <div className="space-y-1.5">
              <label className="text-[11px] text-[#9CA3AF] font-medium">Destination Folder</label>
              <div className="flex items-center space-x-2">
                <input
                  type="text"
                  value={extractDest}
                  onChange={(e) => setExtractDest(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") onStartExtract();
                  }}
                  placeholder="Select destination folder..."
                  className="flex-1 bg-[#252529] hover:bg-[#2E2E34] focus:bg-[#252529] border border-[#323238] focus:border-[#38BDF8] rounded-[4px] px-2.5 py-1.5 text-xs text-[#F4F4F6] placeholder-[#6B7280] outline-none font-mono transition-colors"
                />
                <button
                  type="button"
                  onClick={handleSelectDestFolder}
                  className="px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] border border-[#323238] font-medium text-xs transition-colors cursor-pointer shrink-0"
                >
                  Browse...
                </button>
              </div>
            </div>

            {/* 3. Toggles */}
            <div className="space-y-2 pt-2 border-t border-[#2E2E34]">
              <label className="flex items-center justify-between text-[11px] text-[#F4F4F6] cursor-pointer">
                <span>Extract to subfolder</span>
                <input
                  type="checkbox"
                  checked={extractToSubfolder}
                  onChange={(e) => setExtractToSubfolder(e.target.checked)}
                  className="accent-[#38BDF8] rounded-[2px] w-3.5 h-3.5 cursor-pointer"
                />
              </label>

              <label className="flex items-center justify-between text-[11px] text-[#F4F4F6] cursor-pointer">
                <span>Open destination folder when finished</span>
                <input
                  type="checkbox"
                  checked={openFolderOnExtract}
                  onChange={(e) => setOpenFolderOnExtract(e.target.checked)}
                  className="accent-[#38BDF8] rounded-[2px] w-3.5 h-3.5 cursor-pointer"
                />
              </label>
            </div>

            {/* 4. Modal Footer */}
            <div className="flex items-center justify-end space-x-2 pt-2 border-t border-[#2E2E34]">
              <button
                type="button"
                disabled={isExtracting}
                onClick={onClose}
                className="px-3.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] border border-[#323238] font-medium text-xs transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={isExtracting}
                onClick={onStartExtract}
                className="px-4 py-1.5 rounded-[4px] bg-[#38BDF8] hover:bg-[#7DD3FC] text-[#0F172A] font-semibold text-xs shadow-md shadow-sky-500/20 active:scale-95 transition-all cursor-pointer flex items-center space-x-1.5 disabled:opacity-50 disabled:cursor-not-allowed disabled:pointer-events-none"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} className="w-3.5 h-3.5">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                </svg>
                <span>Extract Now</span>
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
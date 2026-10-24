import React, { useRef, useEffect } from "react";
import { OperationProgressInfo } from "../../types";

export interface WorkspaceHeaderProgressHUDProps {
  progress: OperationProgressInfo;
  isOpen: boolean;
  onToggleOpen: () => void;
  onClose: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export const WorkspaceHeaderProgressHUD: React.FC<WorkspaceHeaderProgressHUDProps> = ({
  progress,
  isOpen,
  onToggleOpen,
  onClose,
}) => {
  const dropdownRef = useRef<HTMLDivElement>(null);
  const startTimeRef = useRef<number>(Date.now());

  const [localCancelling, setLocalCancelling] = React.useState<boolean>(false);

  // Reset timer and local state on new operation
  useEffect(() => {
    startTimeRef.current = Date.now();
    setLocalCancelling(false);
  }, [progress.label]);

  // Close dropdown on click outside
  useEffect(() => {
    if (!isOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    window.addEventListener("mousedown", handleClickOutside);
    return () => window.removeEventListener("mousedown", handleClickOutside);
  }, [isOpen, onClose]);

  const percent = Math.min(100, Math.max(0, Math.round(progress.percent)));
  const elapsedSec = Math.max(0.2, (Date.now() - startTimeRef.current) / 1000);

  // Dynamic ETA estimation
  let etaText = "Calculating ETA...";
  if (percent >= 100) {
    etaText = "Finishing up...";
  } else if (percent > 2 && elapsedSec > 1) {
    const totalEstSec = elapsedSec / (percent / 100);
    const remSec = Math.max(0, Math.round(totalEstSec - elapsedSec));
    if (remSec < 5) {
      etaText = "< 5s left";
    } else if (remSec < 60) {
      etaText = `~${remSec}s left`;
    } else {
      const mins = Math.floor(remSec / 60);
      const secs = remSec % 60;
      etaText = `~${mins}m ${secs}s left`;
    }
  }

  // Throughput speed estimation
  let speedText: string | null = null;
  if (progress.bytes_processed !== undefined && elapsedSec > 0.8 && percent < 100) {
    const bytesPerSec = progress.bytes_processed / elapsedSec;
    if (bytesPerSec > 0) {
      speedText = `${formatBytes(bytesPerSec)}/s`;
    }
  }

  const isActuallyCancelling = progress.isCancelling || localCancelling;

  const handleCancelClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    if (!isActuallyCancelling && progress.onCancel) {
      setLocalCancelling(true);
      progress.onCancel();
    }
  };

  return (
    <div className="relative flex items-center justify-center select-none" data-tauri-drag-region="false">
      {/* Centered Top Header Progress Pill (Dynamic Island Style) */}
      <div
        data-tauri-drag-region="false"
        className={`px-3 py-1 rounded-full flex items-center space-x-2 border transition-all cursor-pointer ${
          isOpen
            ? "bg-[#183325] border-emerald-500/80 shadow-[0_0_18px_rgba(16,185,129,0.35)]"
            : "bg-[#14231b] hover:bg-[#1a3327] border-emerald-600/40 hover:border-emerald-500/80 shadow-[0_0_10px_rgba(16,185,129,0.15)]"
        }`}
        onClick={(e) => {
          e.stopPropagation();
          onToggleOpen();
        }}
        title="Click to toggle operation progress & ETA details"
      >
        <span className="relative flex h-2 w-2 shrink-0">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
          <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-400"></span>
        </span>

        <span className="text-[11px] font-medium text-white tracking-wide truncate max-w-[120px] sm:max-w-[160px]">
          {isActuallyCancelling ? "Cancelling..." : progress.label || "Processing..."}
        </span>

        {/* Mini progress bar inside titlebar */}
        <div className="w-16 sm:w-24 h-1.5 bg-[#22392d] rounded-full overflow-hidden border border-emerald-900/50 hidden sm:block">
          <div
            className="h-full bg-gradient-to-r from-emerald-500 to-teal-400 rounded-full transition-all duration-150 relative overflow-hidden"
            style={{ width: `${Math.max(percent, 3)}%` }}
          >
            <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/40 to-transparent animate-shimmer" />
          </div>
        </div>

        <span className="font-mono text-[11px] font-bold text-emerald-300 shrink-0">
          {percent}%
        </span>

        {/* Dropdown chevron indicator */}
        <button
          type="button"
          data-tauri-drag-region="false"
          onClick={(e) => {
            e.stopPropagation();
            onToggleOpen();
          }}
          className="text-emerald-400 p-0.5 hover:text-white transition-colors cursor-pointer"
          title={isOpen ? "Collapse Details" : "Show Details"}
        >
          <svg
            className={`w-3 h-3 transition-transform duration-200 shrink-0 ${
              isOpen ? "rotate-180" : ""
            }`}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>

        {/* Direct 1-Click Cancel Button right on the Dynamic Island pill */}
        {progress.onCancel && (
          <button
            type="button"
            data-tauri-drag-region="false"
            onClick={handleCancelClick}
            disabled={isActuallyCancelling}
            className="ml-0.5 p-1 rounded-full hover:bg-rose-950/80 text-rose-300 hover:text-white transition-all cursor-pointer border border-transparent hover:border-rose-600/60 disabled:opacity-50"
            title="Cancel operation immediately"
          >
            {isActuallyCancelling ? (
              <svg className="animate-spin w-3 h-3 text-rose-300" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
            ) : (
              <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
              </svg>
            )}
          </button>
        )}
      </div>

      {/* Dropdown Details HUD (Slides Down Directly from the Top Center) */}
      {isOpen && (
        <div
          ref={dropdownRef}
          data-tauri-drag-region="false"
          onClick={(e) => e.stopPropagation()}
          className="absolute top-9 left-1/2 -translate-x-1/2 z-50 w-88 sm:w-96 bg-[#161616]/95 backdrop-blur-xl border border-emerald-500/40 rounded-xl shadow-[0_16px_50px_rgba(0,0,0,0.85)] p-4 space-y-3 animate-in slide-in-from-top-2 duration-200 text-xs select-none"
        >
          {/* Header Row */}
          <div className="flex items-center justify-between" data-tauri-drag-region="false">
            <div className="flex items-center space-x-2 min-w-0">
              <span className="text-emerald-400 font-bold text-sm">⚡</span>
              <span className="font-semibold text-white tracking-wide text-xs truncate">
                {progress.label}
              </span>
            </div>

            <div className="flex items-center space-x-2 shrink-0">
              <span className="font-mono text-sm font-bold text-[#60CDFF]">
                {percent}%
              </span>
              <button
                type="button"
                data-tauri-drag-region="false"
                onClick={onClose}
                className="p-1 rounded text-[#888888] hover:text-white hover:bg-[#252525] cursor-pointer"
                title="Collapse details"
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 15l7-7 7 7" />
                </svg>
              </button>
            </div>
          </div>

          {/* Large Progress Bar with Shimmer */}
          <div className="w-full h-2 bg-[#252525] rounded-full overflow-hidden border border-[#3A3A3A] relative">
            <div
              className="h-full bg-gradient-to-r from-[#059669] via-[#10B981] to-[#34D399] rounded-full shadow-[0_0_12px_rgba(16,185,129,0.7)] transition-all duration-150 relative overflow-hidden"
              style={{ width: `${Math.max(percent, 2)}%` }}
            >
              <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/40 to-transparent animate-shimmer" />
            </div>
          </div>

          {/* Real-time Telemetry: ETA, Speed, & Bytes */}
          <div className="grid grid-cols-2 gap-2 bg-[#1C1C1C] p-2 rounded-lg border border-[#2D2D2D]">
            <div className="flex flex-col">
              <span className="text-[10px] uppercase font-mono text-[#888888]">Estimated Time</span>
              <span className="text-[11px] font-mono text-emerald-300 font-semibold truncate">
                {etaText}
              </span>
            </div>

            <div className="flex flex-col items-end">
              <span className="text-[10px] uppercase font-mono text-[#888888]">Speed / Throughput</span>
              <span className="text-[11px] font-mono text-[#60CDFF] font-semibold truncate">
                {speedText || "Calculating..."}
              </span>
            </div>
          </div>

          {/* Current File & Total Bytes */}
          <div className="flex items-center justify-between text-[11px] text-[#A8A8A8] pt-0.5">
            <span className="truncate max-w-[200px] font-mono text-[10px] text-white" title={progress.current_file}>
              {progress.current_file || "Finalizing operations..."}
            </span>

            {progress.bytes_processed !== undefined && (
              <span className="font-mono text-[10px] text-emerald-300 font-semibold shrink-0">
                {formatBytes(progress.bytes_processed)}
                {progress.total_bytes ? ` / ${formatBytes(progress.total_bytes)}` : ""}
              </span>
            )}
          </div>

          {/* Action Cancel Button inside Dropdown */}
          {progress.onCancel && (
            <div className="pt-1 border-t border-[#2A2A2A]" data-tauri-drag-region="false">
              <button
                type="button"
                data-tauri-drag-region="false"
                onClick={handleCancelClick}
                disabled={isActuallyCancelling}
                className="w-full py-1.5 text-center text-xs font-semibold text-rose-300 hover:text-white bg-rose-950/40 hover:bg-rose-900/60 border border-rose-800/60 rounded-lg transition-colors cursor-pointer disabled:opacity-50 flex items-center justify-center space-x-1.5"
              >
                {isActuallyCancelling ? (
                  <>
                    <svg className="animate-spin w-3.5 h-3.5 text-rose-300" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                    <span>Cancelling operation...</span>
                  </>
                ) : (
                  <span>Cancel Operation</span>
                )}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
};



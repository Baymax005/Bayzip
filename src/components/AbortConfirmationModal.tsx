import { useState } from "react";

export interface AbortConfirmationModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirmAbortAndExit: () => Promise<void>;
  onMinimizeToBackground?: () => void;
  operationType: "compress" | "extract" | "test";
  targetName?: string;
  percent?: number;
  isHudDetached?: boolean;
}

export function AbortConfirmationModal({
  isOpen,
  onClose,
  onConfirmAbortAndExit,
  onMinimizeToBackground,
  operationType,
  targetName = "Active Task",
  percent = 0,
  isHudDetached = false,
}: AbortConfirmationModalProps) {
  const [isAborting, setIsAborting] = useState(false);

  if (!isOpen) return null;

  const handleAbort = async () => {
    setIsAborting(true);
    await onConfirmAbortAndExit();
  };

  const actionVerb =
    operationType === "compress"
      ? "Compression"
      : operationType === "extract"
      ? "Extraction"
      : "Archive Verification";

  const questionText =
    operationType === "compress"
      ? "Compression is in progress. Abort and delete partial archive?"
      : operationType === "extract"
      ? "Extraction is in progress. Abort and remove partial files?"
      : "Archive verification is in progress. Abort verification?";

  return (
    <div
      onClick={(e) => {
        if (e.target === e.currentTarget && !isAborting) {
          onClose();
        }
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 backdrop-blur-sm p-4 select-none animate-fadeIn"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="max-w-[440px] w-full bg-[#1C1C1F] border border-[#383842] rounded-xl p-5 shadow-[0_24px_60px_rgba(0,0,0,0.85)] flex flex-col space-y-4 text-xs text-[#F4F4F6]"
      >
        {/* Header with Warning Icon */}
        <div className="flex items-start space-x-3 pb-3 border-b border-[#2E2E34]">
          <div className="w-10 h-10 rounded-full bg-amber-500/10 border border-amber-500/30 flex items-center justify-center text-amber-400 shrink-0">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-5 h-5">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
              />
            </svg>
          </div>
          <div className="flex-1 min-w-0">
            <h3 className="text-sm font-semibold text-white tracking-wide">
              {actionVerb} in Progress
            </h3>
            <p className="text-[11px] text-[#9CA3AF] mt-0.5 leading-relaxed">
              {questionText}
            </p>
          </div>
        </div>

        {/* Task Details Card */}
        <div className="bg-[#141416] p-3 rounded-[6px] border border-[#2E2E34] flex flex-col space-y-1 text-[11px] font-mono">
          <div className="flex items-center justify-between text-white font-semibold">
            <span className="truncate max-w-[280px]" title={targetName}>
              {targetName}
            </span>
            <span className="text-[#38BDF8] shrink-0">{Math.round(percent)}%</span>
          </div>
          <p className="text-[10px] font-sans text-[#888888] pt-1 border-t border-[#232328]">
            Your original source files will remain untouched. Any incomplete output will be deleted cleanly from disk.
          </p>
        </div>

        {/* Action Buttons */}
        <div className="flex items-center justify-end space-x-2 pt-2">
          {/* If HUD is detached, offer to minimize main window instead of stopping */}
          {isHudDetached && onMinimizeToBackground && (
            <button
              type="button"
              disabled={isAborting}
              onClick={onMinimizeToBackground}
              className="px-3 py-1.5 rounded-[4px] bg-[#222226] hover:bg-[#2A2A30] text-[#38BDF8] border border-[#323238] hover:border-[#38BDF8]/40 font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1"
              title="Minimize Bayzip and keep the detached progress HUD open"
            >
              <span>⧉</span>
              <span>Minimize (Keep HUD)</span>
            </button>
          )}

          <button
            type="button"
            disabled={isAborting}
            onClick={onClose}
            className="px-3.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] border border-[#323238] font-medium text-xs transition-colors cursor-pointer"
          >
            Continue
          </button>

          <button
            type="button"
            disabled={isAborting}
            onClick={handleAbort}
            className={`px-3.5 py-1.5 rounded-[4px] font-semibold text-xs transition-all cursor-pointer flex items-center space-x-1.5 ${
              isAborting
                ? "bg-rose-950/60 border border-rose-700 text-rose-300 opacity-80 cursor-not-allowed"
                : "bg-rose-900/40 hover:bg-rose-800 text-rose-200 border border-rose-700/60 shadow-sm shadow-rose-950/50"
            }`}
          >
            {isAborting ? (
              <>
                <svg className="animate-spin w-3 h-3 text-rose-300" viewBox="0 0 24 24" fill="none">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z" />
                </svg>
                <span>Aborting & Exiting...</span>
              </>
            ) : (
              <span>Abort & Exit</span>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

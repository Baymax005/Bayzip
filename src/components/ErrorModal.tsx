import { useState, useEffect } from "react";

export interface ErrorModalProps {
  isOpen: boolean;
  title: string;
  message: string;
  details?: string;
  onClose: () => void;
}

export function ErrorModal({
  isOpen,
  title,
  message,
  details,
  onClose,
}: ErrorModalProps) {
  const [copied, setCopied] = useState(false);

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

  const handleCopy = async () => {
    const textToCopy = details ? `${message}\n\nDetails:\n${details}` : message;
    try {
      await navigator.clipboard.writeText(textToCopy);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback if clipboard API is unavailable
    }
  };

  if (!isOpen) return null;

  return (
    <div
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onClose();
        }
      }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm select-none animate-in fade-in duration-150"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="bg-[#1C1C1F] border border-[#3E3E48] rounded-xl p-6 max-w-md w-full shadow-2xl relative space-y-4 animate-in zoom-in-95 duration-150 text-xs text-[#F4F4F6]"
      >
        {/* Header with error badge */}
        <div className="flex items-start space-x-3.5">
          <div className="w-10 h-10 rounded-full bg-rose-500/20 text-rose-400 flex items-center justify-center shrink-0 border border-rose-500/30">
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              className="w-5 h-5"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2.2}
                d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
              />
            </svg>
          </div>
          <div className="min-w-0 flex-1">
            <h3 className="text-sm font-semibold text-white tracking-tight">
              {title}
            </h3>
            <p className="text-xs text-[#A8A8B4] mt-1 break-words leading-relaxed">
              {message}
            </p>
          </div>
        </div>

        {/* Optional scrollable error details */}
        {details && (
          <div className="bg-[#141416] border border-[#2E2E34] rounded-lg p-3 space-y-2">
            <div className="flex items-center justify-between text-[11px] text-[#A8A8B4]">
              <span className="font-mono uppercase tracking-wider text-[10px] text-slate-400">
                Diagnostic Details
              </span>
              <button
                type="button"
                onClick={handleCopy}
                className="text-[11px] text-[#38BDF8] hover:text-[#7DD3FC] flex items-center space-x-1 cursor-pointer transition-colors"
                title="Copy error details to clipboard"
              >
                {copied ? (
                  <>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-emerald-400">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                    <span className="text-emerald-400 font-medium">Copied!</span>
                  </>
                ) : (
                  <>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                      <rect x="9" y="9" width="13" height="13" rx="2" strokeWidth={2} />
                      <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" strokeWidth={2} />
                    </svg>
                    <span>Copy</span>
                  </>
                )}
              </button>
            </div>
            <div className="max-h-36 overflow-y-auto font-mono text-[11px] text-rose-300/90 whitespace-pre-wrap break-all select-text leading-tight bg-[#0F0F12] p-2 rounded border border-[#25252B]">
              {details}
            </div>
          </div>
        )}

        {/* Modal Footer */}
        <div className="flex items-center justify-end space-x-2 pt-2 border-t border-[#2E2E34]">
          <button
            type="button"
            onClick={onClose}
            className="px-5 py-1.5 rounded-[4px] bg-[#2E2E36] hover:bg-[#3A3A44] text-[#F4F4F6] border border-[#3E3E48] font-medium text-xs transition-colors cursor-pointer"
          >
            Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}

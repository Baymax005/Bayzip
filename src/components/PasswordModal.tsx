import { useEffect, useRef } from "react";
import { useDraggableModal } from "../hooks/useDraggableModal";

export interface PasswordModalProps {
  isOpen: boolean;
  onClose: () => void;
  password: string;
  setPassword: (password: string) => void;
  showPassword: boolean;
  setShowPassword: (show: boolean) => void;
  isExtracting: boolean;
  extractError: string | null;
  onSubmit: () => void;
  archiveName?: string;
}

export function PasswordModal({
  isOpen,
  onClose,
  password,
  setPassword,
  showPassword,
  setShowPassword,
  isExtracting,
  extractError,
  onSubmit,
  archiveName,
}: PasswordModalProps) {
  const {
    position,
    isDragging,
    handleMouseDown,
    resetPosition,
    isDisplaced,
  } = useDraggableModal({ isOpen });

  const inputRef = useRef<HTMLInputElement | null>(null);

  // Auto-focus input when modal opens
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 50);
    }
  }, [isOpen]);

  // Close only on explicit Escape key (clicks outside do NOT dismiss to prevent accidental loss)
  useEffect(() => {
    if (!isOpen || isExtracting) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, isExtracting, onClose]);

  if (!isOpen) return null;

  return (
    // Scoped within canvas area with NO backdrop blur to preserve full Titlebar accessibility
    <div
      className={`absolute inset-0 z-40 flex items-center justify-center p-4 select-none ${
        isDisplaced ? "bg-black/10 pointer-events-none" : "bg-black/30"
      }`}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          transform: `translate(${position.x}px, ${position.y}px)`,
        }}
        className={`bg-[#202024] border border-[#383842] rounded-xl w-full max-w-md shadow-[0_24px_60px_rgba(0,0,0,0.85)] p-5 space-y-4 pointer-events-auto transition-shadow ${
          isDragging ? "shadow-[0_25px_70px_rgba(56,189,248,0.25)] cursor-grabbing" : ""
        }`}
      >
        {/* Grabbable Header */}
        <div
          onMouseDown={handleMouseDown}
          className="flex items-center justify-between pb-2.5 border-b border-[#2E2E34] cursor-grab active:cursor-grabbing select-none"
          title="Click and drag to move password dialog"
        >
          <div className="flex items-center space-x-2.5 truncate mr-2">
            <div className="w-8 h-8 rounded-lg bg-[#38BDF8]/15 border border-[#38BDF8]/30 flex items-center justify-center shrink-0">
              <svg className="w-4 h-4 text-[#38BDF8]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
                />
              </svg>
            </div>
            <div className="truncate">
              <div className="flex items-center space-x-2">
                <h3 className="text-xs font-semibold text-white tracking-wide">Password Required</h3>
                {archiveName && (
                  <span className="text-[10px] text-[#38BDF8] font-mono bg-[#18181B] px-1.5 py-0.5 rounded border border-[#2E2E34] truncate max-w-[160px]">
                    {archiveName}
                  </span>
                )}
              </div>
              <p className="text-[11px] text-[#9CA3AF]">
                Archive encrypted with AES. Enter decryption password to unlock.
              </p>
            </div>
          </div>

          <div className="flex items-center space-x-1.5 shrink-0">
            <span className="text-[9px] text-[#888888] bg-[#18181A] px-1.5 py-0.5 rounded border border-[#2E2E34] shrink-0 pointer-events-none">
              Draggable
            </span>
            {isDisplaced && (
              <button
                type="button"
                onClick={resetPosition}
                className="text-[10px] text-[#A8A8A8] hover:text-[#38BDF8] bg-[#2A2A30] hover:bg-[#34343C] px-2 py-0.5 rounded border border-[#3A3A42] transition-colors cursor-pointer"
                title="Reset position to center"
              >
                Center
              </button>
            )}
          </div>
        </div>

        {/* Input Field */}
        <div>
          <label className="block text-[11px] font-medium text-[#C8C8C8] mb-1.5">
            Decryption Password
          </label>
          <div className="relative">
            <input
              ref={inputRef}
              type={showPassword ? "text" : "password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && password.trim() && !isExtracting) {
                  e.preventDefault();
                  onSubmit();
                }
              }}
              placeholder="Enter archive password..."
              className="w-full bg-[#161618] text-white text-xs pl-3 pr-10 py-2.5 rounded-lg border border-[#383842] focus:border-[#38BDF8] outline-none font-mono placeholder-[#666] transition-colors"
            />
            <button
              type="button"
              onClick={() => setShowPassword(!showPassword)}
              className="absolute inset-y-0 right-0 pr-3 flex items-center text-[#888] hover:text-white cursor-pointer"
              title={showPassword ? "Hide password" : "Show password"}
            >
              {showPassword ? (
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

        {extractError && (
          <div className="text-xs text-rose-400 bg-rose-950/40 p-2.5 rounded-lg border border-rose-900/50 flex items-start space-x-2">
            <svg className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
            <div className="flex-1 break-all">{extractError}</div>
          </div>
        )}

        <div className="flex justify-end space-x-2 pt-1">
          <button
            type="button"
            disabled={isExtracting}
            onClick={onClose}
            className="px-4 py-2 bg-[#2A2A30] hover:bg-[#34343C] text-[#D1D5DB] text-xs font-medium rounded-lg border border-[#383842] transition-colors cursor-pointer disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={!password.trim() || isExtracting}
            onClick={onSubmit}
            className="px-4 py-2 bg-[#38BDF8] hover:bg-[#0284C7] disabled:opacity-50 disabled:cursor-not-allowed text-black text-xs font-semibold rounded-lg transition-colors cursor-pointer flex items-center space-x-1.5 shadow-md shadow-sky-500/10"
          >
            {isExtracting ? (
              <>
                <svg className="animate-spin h-3.5 w-3.5 text-black" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                <span>Decrypting...</span>
              </>
            ) : (
              <span>Decrypt & Extract</span>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

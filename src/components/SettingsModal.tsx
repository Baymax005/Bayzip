import { useState, useEffect, useRef } from "react";

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  isSyncingRegistry: boolean;
  contextMenuStatus: string | null;
  onRegisterContextMenu: () => void;
}

export function SettingsModal({
  isOpen,
  onClose,
  isSyncingRegistry,
  contextMenuStatus,
  onRegisterContextMenu,
}: SettingsModalProps) {
  // Draggable floating window coordinates
  const [position, setPosition] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartRef = useRef<{ startX: number; startY: number; posX: number; posY: number }>({
    startX: 0,
    startY: 0,
    posX: 0,
    posY: 0,
  });

  // Reset to center on open
  useEffect(() => {
    if (isOpen) {
      setPosition({ x: 0, y: 0 });
    }
  }, [isOpen]);

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

  // Global mousemove and mouseup listeners for smooth dragging
  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaX = e.clientX - dragStartRef.current.startX;
      const deltaY = e.clientY - dragStartRef.current.startY;
      const newX = dragStartRef.current.posX + deltaX;
      const newY = dragStartRef.current.posY + deltaY;

      // Safe viewport boundary clamping so the modal cannot be dragged off-screen
      const maxOffsetX = Math.max(0, window.innerWidth / 2 - 120);
      const maxOffsetY = Math.max(0, window.innerHeight / 2 - 80);

      setPosition({
        x: Math.max(-maxOffsetX, Math.min(maxOffsetX, newX)),
        y: Math.max(-maxOffsetY, Math.min(maxOffsetY, newY)),
      });
    };

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging]);

  if (!isOpen) return null;

  const handleMouseDown = (e: React.MouseEvent) => {
    // Only drag when clicking the header area, not buttons
    if ((e.target as HTMLElement).closest("button")) return;
    setIsDragging(true);
    dragStartRef.current = {
      startX: e.clientX,
      startY: e.clientY,
      posX: position.x,
      posY: position.y,
    };
  };

  return (
    <div
      onClick={onClose}
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4 animate-fadeIn select-none"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          transform: `translate(${position.x}px, ${position.y}px)`,
        }}
        className={`bg-[#1F1F1F]/95 backdrop-blur-xl border border-[#3A3A3A] rounded-2xl max-w-lg w-full shadow-[0_20px_60px_rgba(0,0,0,0.85),0_0_30px_rgba(96,205,255,0.06)] relative overflow-hidden transition-shadow ${
          isDragging ? "shadow-[0_25px_70px_rgba(96,205,255,0.18)] cursor-grabbing" : ""
        }`}
      >
        {/* Draggable Titlebar Header */}
        <div
          onMouseDown={handleMouseDown}
          className="bg-gradient-to-r from-[#262626] to-[#1F1F1F] px-6 py-4 border-b border-[#333333] flex items-center justify-between cursor-grab active:cursor-grabbing select-none"
          title="Click and drag to move settings window"
        >
          <div className="flex items-center space-x-2.5">
            <div className="w-7 h-7 rounded-lg bg-[#183144] border border-[#60CDFF]/30 flex items-center justify-center text-[#60CDFF] shrink-0">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-4 h-4">
                <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
            </div>
            <div>
              <div className="flex items-center space-x-2">
                <h3 className="text-sm font-semibold text-white">Settings</h3>
                <span className="text-[10px] text-[#888888] bg-[#2A2A2A] px-1.5 py-0.5 rounded border border-[#3E3E3E]">
                  Draggable
                </span>
              </div>
              <p className="text-[11px] text-[#888888]">System integration and application preferences</p>
            </div>
          </div>

          <div className="flex items-center space-x-1.5">
            {/* Reset position button */}
            {(position.x !== 0 || position.y !== 0) && (
              <button
                type="button"
                onClick={() => setPosition({ x: 0, y: 0 })}
                className="text-[10px] text-[#A8A8A8] hover:text-white px-2 py-1 rounded bg-[#2A2A2A] hover:bg-[#333333] border border-[#3E3E3E] transition-colors cursor-pointer"
                title="Reset window to center"
              >
                Center
              </button>
            )}
            <button
              type="button"
              onClick={onClose}
              className="w-7 h-7 rounded-lg bg-[#2A2A2A] hover:bg-[#333333] border border-[#3E3E3E] hover:border-[#555] text-[#A8A8A8] hover:text-white flex items-center justify-center transition-all cursor-pointer shadow-sm"
              title="Close (Esc)"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        {/* Modal Body */}
        <div className="p-6 space-y-5 max-h-[75vh] overflow-y-auto">
          {/* Windows Explorer Integration Card */}
          <div className="bg-[#1A1A1A] rounded-xl p-4 border border-[#333333] space-y-3.5 shadow-inner">
            <div className="flex justify-between items-center">
              <div className="flex items-center space-x-2">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4 text-[#60CDFF]">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16m-7 6h7" />
                </svg>
                <span className="text-xs font-semibold text-white">Windows 11 Explorer Shell Integration</span>
              </div>
              <span className="text-[10px] text-emerald-400 bg-emerald-950/60 px-2 py-0.5 rounded-full border border-emerald-800/70 font-medium flex items-center space-x-1">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400"></span>
                <span>Active</span>
              </span>
            </div>

            <p className="text-xs text-[#9E9E9E] leading-relaxed">
              Registers Bayzip into the Windows 11 context menu so you can right-click any file or folder to compress and extract instantly.
            </p>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-[11px] text-[#A8A8A8] pt-0.5">
              <div className="flex items-start space-x-2">
                <svg className="w-3.5 h-3.5 text-[#60CDFF] shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                </svg>
                <span>Right-click Extract (.bay, .zip, .7z, .rar)</span>
              </div>
              <div className="flex items-start space-x-2">
                <svg className="w-3.5 h-3.5 text-[#60CDFF] shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                </svg>
                <span>Right-click Compress context menu</span>
              </div>
              <div className="flex items-start space-x-2">
                <svg className="w-3.5 h-3.5 text-[#60CDFF] shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                </svg>
                <span>Double-click .bay file association</span>
              </div>
              <div className="flex items-start space-x-2">
                <svg className="w-3.5 h-3.5 text-[#60CDFF] shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                </svg>
                <span>Safe in-memory file & code preview</span>
              </div>
            </div>

            <button
              type="button"
              disabled={isSyncingRegistry}
              onClick={onRegisterContextMenu}
              className={`w-full py-2.5 bg-[#252525] hover:bg-[#2D2D2D] border border-[#3E3E3E] hover:border-[#60CDFF] text-[#60CDFF] text-xs font-semibold rounded-lg transition-all flex items-center justify-center space-x-2 cursor-pointer shadow-sm hover:shadow-[0_0_12px_rgba(96,205,255,0.2)] ${
                isSyncingRegistry ? "opacity-75 cursor-not-allowed" : ""
              }`}
            >
              {isSyncingRegistry ? (
                <>
                  <svg className="animate-spin h-3.5 w-3.5 text-[#60CDFF]" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                  </svg>
                  <span>Syncing Windows Registry...</span>
                </>
              ) : (
                <>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                  </svg>
                  <span>Sync Windows Explorer Integration</span>
                </>
              )}
            </button>

            {contextMenuStatus && (
              <div className="text-[11px] text-emerald-400 bg-emerald-950/40 p-2.5 rounded-lg border border-emerald-900/50 flex items-center space-x-2">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4 shrink-0 text-emerald-400">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                </svg>
                <span>{contextMenuStatus}</span>
              </div>
            )}
          </div>

          {/* Software Updates (Phase 2 Preview Card) */}
          <div className="bg-[#1A1A1A] rounded-xl p-4 border border-[#303030] flex items-center justify-between">
            <div className="flex items-center space-x-3">
              <div className="w-8 h-8 rounded-lg bg-[#252525] border border-[#3E3E3E] flex items-center justify-center text-[#60CDFF] shrink-0">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-4 h-4">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                </svg>
              </div>
              <div>
                <p className="text-xs font-semibold text-white">Application Updates</p>
                <p className="text-[11px] text-[#888888]">Current version v1.1.3 • Up to date</p>
              </div>
            </div>
            <button
              type="button"
              disabled
              className="px-3 py-1.5 rounded-md bg-[#252525] border border-[#383838] text-[11px] font-medium text-[#7A7A7A] cursor-not-allowed"
            >
              Check for Updates
            </button>
          </div>
        </div>

        {/* Footer */}
        <div className="bg-[#181818] px-6 py-3 border-t border-[#2A2A2A] flex items-center justify-between text-[11px] text-[#7A7A7A]">
          <span>Bayzip v1.1.3 • Preferences</span>
          <span className="font-mono text-[#888888]">Esc to close</span>
        </div>
      </div>
    </div>
  );
}

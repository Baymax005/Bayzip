import React, { useEffect, useRef } from "react";

export interface ContextMenuTarget {
  name: string;
  isDir: boolean;
  size?: number;
  diskPath?: string;
  isStaged?: boolean;
  isExcluded?: boolean;
  matchedStagedPath?: string | null;
}

export interface WorkspaceContextMenuProps {
  isOpen: boolean;
  x: number;
  y: number;
  target: ContextMenuTarget | null;
  mode: "compress" | "extract";
  browserSource: "archive" | "filesystem";
  onClose: () => void;
  onPreview?: (name: string) => void;
  onStage?: (path: string) => void;
  onUnstage?: (path: string) => void;
  onToggleExclude?: (name: string, isDir?: boolean) => void;
  onStartRename?: (name: string, isDir: boolean) => void;
  onCopy?: (name: string, isDir: boolean, diskPath?: string) => void;
  onDeleteDisk?: (name: string, isDir: boolean, diskPath?: string) => void;
  onNewFolder?: () => void;
  onPaste?: () => void;
  onLoadArchive?: (path: string) => void;
  clipboardItem?: { relName: string; isDir: boolean; diskPath?: string } | null;
}

export const WorkspaceContextMenu: React.FC<WorkspaceContextMenuProps> = ({
  isOpen,
  x,
  y,
  target,
  mode,
  browserSource,
  onClose,
  onPreview,
  onStage,
  onUnstage,
  onToggleExclude,
  onStartRename,
  onCopy,
  onDeleteDisk,
  onNewFolder,
  onPaste,
  onLoadArchive,
  clipboardItem,
}) => {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  // Viewport clamping
  const clampedX = Math.min(x, window.innerWidth - 240);
  const clampedY = Math.min(y, window.innerHeight - 320);

  return (
    <>
      {/* Invisible backdrop to capture clicks outside */}
      <div
        className="fixed inset-0 z-[125]"
        onClick={onClose}
        onContextMenu={(e) => {
          e.preventDefault();
          onClose();
        }}
      />

      {/* Floating Fluent Context Menu */}
      <div
        ref={menuRef}
        style={{ left: `${clampedX}px`, top: `${clampedY}px` }}
        className="fixed z-[130] min-w-[220px] bg-[#1E1E1E]/95 backdrop-blur-md border border-[#3E3E3E] rounded-lg shadow-2xl p-1 text-xs select-none animate-in fade-in duration-100 font-sans"
        onClick={(e) => e.stopPropagation()}
        data-tauri-drag-region="false"
      >
        {target && (
          <>
            {/* Action 1: In-memory safe preview for files */}
            {!target.isDir && onPreview && (
              <button
                type="button"
                onClick={() => {
                  onPreview(target.name);
                  onClose();
                }}
                className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
              >
                <div className="flex items-center space-x-2">
                  <span className="text-sm">👁️</span>
                  <span>Preview File</span>
                </div>
                <span className="text-[10px] text-[#777777] font-mono">Space</span>
              </button>
            )}

            {/* Action 1b: Open Archive in Inspector if it's an archive file on disk */}
            {onLoadArchive && target.diskPath && !target.isDir && /\.(bay|bayg|zip|7z|tar|gz|bz2|xz|zst|rar)$/i.test(target.name) && (
              <button
                type="button"
                onClick={() => {
                  onLoadArchive(target.diskPath!);
                  onClose();
                }}
                className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#1C3345] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
              >
                <div className="flex items-center space-x-2">
                  <span className="text-sm">📦</span>
                  <span className="font-semibold text-[#60CDFF]">Open Archive</span>
                </div>
                <span className="text-[10px] text-[#60CDFF] font-mono">Inspect</span>
              </button>
            )}

            {/* Action 2: Filesystem disk item -> Stage or Unstage */}
            {browserSource === "filesystem" && (
              <>
                {target.isStaged && onUnstage ? (
                  <button
                    type="button"
                    onClick={() => {
                      onUnstage(target.diskPath || target.name);
                      onClose();
                    }}
                    className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-rose-950/70 hover:text-white text-rose-300 transition-colors cursor-pointer"
                  >
                    <div className="flex items-center space-x-2">
                      <span className="text-sm font-bold text-rose-400">✕</span>
                      <span className="font-semibold">Remove from Staged</span>
                    </div>
                    <span className="text-[10px] text-rose-400/80 font-mono">Unstage</span>
                  </button>
                ) : onStage ? (
                  <button
                    type="button"
                    onClick={() => {
                      onStage(target.diskPath || target.name);
                      onClose();
                    }}
                    className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-emerald-950/70 hover:text-white text-emerald-300 transition-colors cursor-pointer"
                  >
                    <div className="flex items-center space-x-2">
                      <span className="text-sm font-bold text-emerald-400">+</span>
                      <span className="font-semibold">Stage for Archive</span>
                    </div>
                    <span className="text-[10px] text-emerald-400 font-mono">Add</span>
                  </button>
                ) : null}
              </>
            )}

            {/* Action 3: Compress Mode Staged item removal */}
            {mode === "compress" && browserSource === "archive" && target.matchedStagedPath && onUnstage && (
              <button
                type="button"
                onClick={() => {
                  onUnstage(target.matchedStagedPath!);
                  onClose();
                }}
                className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-rose-950/70 hover:text-white text-rose-300 transition-colors cursor-pointer"
              >
                <div className="flex items-center space-x-2">
                  <span className="text-sm font-bold text-rose-400">✕</span>
                  <span className="font-semibold">Remove from Archive</span>
                </div>
                <span className="text-[10px] text-rose-400/80 font-mono">Unstage</span>
              </button>
            )}

            {/* Action 4: Exclusion / Skip Toggle (Only in Extract Mode or in Compress Mode when inspecting Staged Archive Tree) */}
            {onToggleExclude &&
              (mode === "extract" || (mode === "compress" && browserSource === "archive" && !target.matchedStagedPath)) && (
              <button
                type="button"
                onClick={() => {
                  onToggleExclude(target.name, target.isDir);
                  onClose();
                }}
                className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-rose-950/70 hover:text-white text-rose-300 transition-colors cursor-pointer"
              >
                <div className="flex items-center space-x-2">
                  <span className="text-sm font-bold text-rose-400">
                    {target.isExcluded ? "↺" : "✕"}
                  </span>
                  <span className="font-semibold">
                    {target.isExcluded
                      ? target.isDir ? "Include Folder" : mode === "compress" ? "Include in Archive" : "Include in Extraction"
                      : target.isDir ? "Exclude Folder" : mode === "compress" ? "Exclude from Archive" : "Exclude from Extraction"}
                  </span>
                </div>
                <span className="text-[10px] text-rose-400/80 font-mono">
                  {target.isExcluded ? "Restore" : "Skip"}
                </span>
              </button>
            )}

            <div className="my-1 border-t border-[#2E2E2E]" />

            {/* Action 5: Rename */}
            {onStartRename && (
              <button
                type="button"
                onClick={() => {
                  onStartRename(target.name, target.isDir);
                  onClose();
                }}
                className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
              >
                <div className="flex items-center space-x-2">
                  <span className="text-sm">✏️</span>
                  <span>Rename</span>
                </div>
                <span className="text-[10px] text-[#777777] font-mono">F2</span>
              </button>
            )}

            {/* Action 6: Copy */}
            {onCopy && (
              <button
                type="button"
                onClick={() => {
                  onCopy(target.name, target.isDir, target.diskPath);
                  onClose();
                }}
                className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
              >
                <div className="flex items-center space-x-2">
                  <span className="text-sm">📋</span>
                  <span>Copy</span>
                </div>
                <span className="text-[10px] text-[#777777] font-mono">Ctrl+C</span>
              </button>
            )}

            {/* Action 7: Recycle Bin delete for physical disk files */}
            {browserSource === "filesystem" && onDeleteDisk && (
              <>
                <div className="my-1 border-t border-[#2E2E2E]" />
                <button
                  type="button"
                  onClick={() => {
                    onDeleteDisk(target.name, target.isDir, target.diskPath);
                    onClose();
                  }}
                  className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-rose-950/60 hover:text-rose-300 text-rose-400 transition-colors cursor-pointer"
                >
                  <div className="flex items-center space-x-2">
                    <span className="text-sm">🗑️</span>
                    <span>Delete to Recycle Bin</span>
                  </div>
                  <span className="text-[10px] text-rose-400/80 font-mono">Del</span>
                </button>
              </>
            )}

            <div className="my-1 border-t border-[#2E2E2E]" />
          </>
        )}

        {/* Global Action: New Folder */}
        {onNewFolder && (
          <button
            type="button"
            onClick={() => {
              onNewFolder();
              onClose();
            }}
            className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
          >
            <div className="flex items-center space-x-2">
              <span className="text-sm">📁+</span>
              <span>New Folder</span>
            </div>
            <span className="text-[10px] text-[#777777] font-mono">Ctrl+Shift+N</span>
          </button>
        )}

        {/* Global Action: Paste */}
        {clipboardItem && onPaste && (
          <button
            type="button"
            onClick={() => {
              onPaste();
              onClose();
            }}
            className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
          >
            <div className="flex items-center space-x-2 truncate mr-2">
              <span className="text-sm">📥</span>
              <span className="truncate">Paste "{clipboardItem.relName.split("/").filter(Boolean).pop()}"</span>
            </div>
            <span className="text-[10px] text-[#777777] font-mono shrink-0">Ctrl+V</span>
          </button>
        )}
      </div>
    </>
  );
};

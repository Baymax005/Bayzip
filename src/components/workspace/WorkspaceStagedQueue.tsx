import React, { useState } from "react";
import { FileFormatIcon } from "../FileFormatIcon";

export interface WorkspaceStagedQueueProps {
  stagedItems: string[];
  itemsInfo: Record<string, { size: number; is_dir: boolean }>;
  onRemoveItem: (path: string) => void;
  onClearAll: () => void;
  isOpen: boolean;
  onToggleOpen: () => void;
  onPreviewItem?: (path: string) => void;
  height?: number;
  onHeightChange?: (h: number) => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export const WorkspaceStagedQueue: React.FC<WorkspaceStagedQueueProps> = ({
  stagedItems,
  itemsInfo,
  onRemoveItem,
  onClearAll,
  isOpen,
  onToggleOpen,
  onPreviewItem,
  height = 180,
  onHeightChange,
}) => {
  const [isResizing, setIsResizing] = useState(false);

  const totalBytes = stagedItems.reduce((acc, p) => acc + (itemsInfo[p]?.size || 0), 0);

  const handleMouseDownResize = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
    const startY = e.clientY;
    const startH = height;

    const handleMouseMove = (moveEvent: MouseEvent) => {
      const deltaY = startY - moveEvent.clientY;
      const newH = Math.max(90, Math.min(450, startH + deltaY));
      if (onHeightChange) {
        onHeightChange(newH);
      }
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
  };

  return (
    <div
      className={`border-t border-[#2A2A2A] bg-[#161616] flex flex-col shrink-0 select-none transition-all ${
        isResizing ? "cursor-row-resize" : ""
      }`}
      style={{ height: isOpen ? `${height}px` : "34px" }}
    >
      {/* Resizable Divider Header */}
      <div
        onMouseDown={isOpen ? handleMouseDownResize : undefined}
        className="h-8 bg-[#1E1E1E] hover:bg-[#252525] border-b border-[#2C2C2C] px-3 flex items-center justify-between text-xs cursor-row-resize group"
        title={isOpen ? "Drag up or down to resize Staged Queue drawer" : "Click to expand"}
      >
        <div className="flex items-center space-x-2">
          {/* Collapse/Expand Toggle */}
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleOpen();
            }}
            className="p-1 rounded text-[#888888] hover:text-white hover:bg-[#303030] cursor-pointer"
            title={isOpen ? "Collapse Queue" : "Expand Queue"}
          >
            <svg
              className={`w-3.5 h-3.5 transition-transform duration-200 ${isOpen ? "rotate-180" : ""}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 15l7-7 7 7" />
            </svg>
          </button>

          <span className="font-semibold text-white text-[11px] flex items-center space-x-1">
            <span>📦</span>
            <span>Staged Items for Archive</span>
          </span>

          <span className="bg-[#173042] text-[#60CDFF] px-2 py-0.5 rounded-full font-mono text-[10px] font-bold border border-[#60CDFF]/30">
            {stagedItems.length} {stagedItems.length === 1 ? "item" : "items"}
          </span>

          <span className="font-mono text-[11px] text-[#A8A8A8]">
            Total: <span className="text-[#60CDFF] font-semibold">{formatBytes(totalBytes)}</span>
          </span>
        </div>

        {/* Center Resize Grip */}
        <div className="hidden sm:flex items-center space-x-0.5 text-[#555555] group-hover:text-[#60CDFF] transition-colors">
          <div className="w-8 h-1 bg-[#3A3A3A] group-hover:bg-[#60CDFF] rounded-full transition-colors" />
        </div>

        {/* Action Controls */}
        <div className="flex items-center space-x-2">
          {stagedItems.length > 0 && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onClearAll();
              }}
              className="text-[10px] font-semibold text-rose-400 hover:text-rose-300 hover:bg-rose-950/40 px-2 py-0.5 rounded border border-rose-800/60 transition-colors cursor-pointer flex items-center space-x-1"
              title="Clear all staged files"
            >
              <span>🗑️</span>
              <span>Clear All</span>
            </button>
          )}

          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleOpen();
            }}
            className="text-[10px] text-[#888888] hover:text-white cursor-pointer px-1 py-0.5"
          >
            {isOpen ? "Hide Queue" : "Show Queue"}
          </button>
        </div>
      </div>

      {/* Staged Items List Body (Visible when open) */}
      {isOpen && (
        <div className="flex-1 overflow-y-auto custom-scrollbar p-1 divide-y divide-[#222222]">
          {stagedItems.length === 0 ? (
            <div className="h-full flex flex-col items-center justify-center p-4 text-center text-[#777777] space-y-1">
              <span className="text-xl">📥</span>
              <p className="text-xs font-medium text-[#AAAAAA]">No files staged yet</p>
              <p className="text-[11px] max-w-sm">
                Browse drives above and click <span className="text-[#60CDFF] font-semibold">+ Stage</span> on any file or folder to add it to your archive queue.
              </p>
            </div>
          ) : (
            stagedItems.map((itemPath) => {
              const name = itemPath.split(/[/\\]/).pop() || itemPath;
              const info = itemsInfo[itemPath];
              const isDir = info?.is_dir ?? false;
              const size = info?.size ?? 0;

              return (
                <div
                  key={itemPath}
                  onDoubleClick={() => onPreviewItem && onPreviewItem(itemPath)}
                  className="px-3 py-1.5 flex items-center justify-between text-xs hover:bg-[#1F1F1F] transition-colors group cursor-pointer"
                >
                  <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                    {isDir ? (
                      <svg className="w-4 h-4 text-[#F9D648] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                      </svg>
                    ) : (
                      <FileFormatIcon fileName={name} size="sm" />
                    )}

                    <div className="flex flex-col min-w-0">
                      <span className="font-mono text-white text-[11px] truncate font-medium group-hover:text-[#60CDFF] transition-colors" title={itemPath}>
                        {name}
                      </span>
                      <span className="text-[10px] text-[#777777] font-mono truncate" title={itemPath}>
                        {itemPath}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center space-x-3 shrink-0">
                    <span className="font-mono text-[11px] text-[#A8A8A8] w-24 text-right">
                      {isDir ? "Folder" : formatBytes(size)}
                    </span>

                    {/* Remove Action Button */}
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onRemoveItem(itemPath);
                      }}
                      className="p-1 rounded text-[#777777] hover:text-rose-400 hover:bg-rose-950/40 border border-transparent hover:border-rose-800/50 transition-colors cursor-pointer"
                      title={`Remove "${name}" from archive staging`}
                    >
                      <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      )}
    </div>
  );
};

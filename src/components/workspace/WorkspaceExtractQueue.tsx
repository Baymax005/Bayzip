import React, { useState } from "react";

export interface WorkspaceExtractQueueProps {
  totalFilesCount: number;
  totalUncompressedSize: number;
  excludedPaths: Set<string>;
  onToggleExclude: (path: string, isDir?: boolean) => void;
  onIncludeAll?: () => void;
  isOpen: boolean;
  onToggleOpen: () => void;
  height: number;
  onHeightChange: (height: number) => void;
  archiveFormat?: string;
  isEncrypted?: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export const WorkspaceExtractQueue: React.FC<WorkspaceExtractQueueProps> = ({
  totalFilesCount,
  totalUncompressedSize,
  excludedPaths,
  onToggleExclude,
  onIncludeAll,
  isOpen,
  onToggleOpen,
  height,
  onHeightChange,
  archiveFormat = "ARCHIVE",
  isEncrypted = false,
}) => {
  const [isResizing, setIsResizing] = useState(false);

  const handleMouseDownResize = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsResizing(true);
    const startY = e.clientY;
    const startHeight = height;

    const handleMouseMove = (moveEvent: MouseEvent) => {
      const deltaY = startY - moveEvent.clientY;
      const newHeight = Math.max(90, Math.min(450, startHeight + deltaY));
      onHeightChange(newHeight);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
  };

  const excludedList = Array.from(excludedPaths);
  const includedCount = Math.max(0, totalFilesCount - excludedList.length);

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
        title={isOpen ? "Drag up or down to resize Extraction Summary drawer" : "Click to expand"}
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
            title={isOpen ? "Collapse Drawer" : "Expand Drawer"}
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
            <span>📂</span>
            <span>Extraction Queue & Targets</span>
          </span>

          <span className="bg-[#173042] text-[#60CDFF] px-2 py-0.5 rounded-full font-mono text-[10px] font-bold border border-[#60CDFF]/30">
            {includedCount} to extract
          </span>

          {excludedList.length > 0 && (
            <span className="bg-rose-950/80 text-rose-300 px-2 py-0.5 rounded-full font-mono text-[10px] font-bold border border-rose-700/60">
              {excludedList.length} excluded
            </span>
          )}

          <span className="font-mono text-[11px] text-[#A8A8A8] hidden sm:inline">
            Payload: <span className="text-[#60CDFF] font-semibold">{formatBytes(totalUncompressedSize)}</span>
          </span>
        </div>

        {/* Center Resize Grip */}
        <div className="hidden sm:flex items-center space-x-0.5 text-[#555555] group-hover:text-[#60CDFF] transition-colors">
          <div className="w-8 h-1 bg-[#3A3A3A] group-hover:bg-[#60CDFF] rounded-full transition-colors" />
        </div>

        {/* Action Controls */}
        <div className="flex items-center space-x-2">
          {excludedList.length > 0 && onIncludeAll && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onIncludeAll();
              }}
              className="text-[10px] font-semibold text-[#60CDFF] hover:text-white hover:bg-[#20495f] px-2 py-0.5 rounded border border-[#60CDFF]/50 transition-colors cursor-pointer flex items-center space-x-1"
              title="Reset exclusions and extract all files"
            >
              <span>↺</span>
              <span>Include All</span>
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
            {isOpen ? "Hide Drawer" : "Show Drawer"}
          </button>
        </div>
      </div>

      {/* Drawer Body (Visible when open) */}
      {isOpen && (
        <div className="flex-1 overflow-y-auto custom-scrollbar p-2 space-y-2">
          {excludedList.length === 0 ? (
            <div className="h-full flex flex-col items-center justify-center p-4 text-center text-[#777777] space-y-1">
              <span className="text-xl">✅</span>
              <p className="text-xs font-medium text-[#AAAAAA]">All {totalFilesCount} files are included for extraction</p>
              <p className="text-[11px] max-w-md">
                Format: <span className="text-white font-mono">{archiveFormat}</span> • 
                Total size: <span className="text-emerald-400 font-mono">{formatBytes(totalUncompressedSize)}</span>
                {isEncrypted ? " • 🔒 Password Protected" : ""}
              </p>
              <p className="text-[10px] text-[#666666]">
                Tip: Right-click any file above or enable <span className="text-rose-400">Exclude Files</span> to skip unwanted items from being extracted.
              </p>
            </div>
          ) : (
            <div className="space-y-1.5">
              <div className="flex items-center justify-between px-2 text-[10px] uppercase font-bold text-rose-400 tracking-wider">
                <span>Excluded Items ({excludedList.length}) - These will NOT be extracted</span>
                <span className="text-[#888888] font-normal lowercase">click restore to include</span>
              </div>
              <div className="divide-y divide-[#262626] bg-[#1A1A1A] rounded-md border border-[#2E2E2E]">
                {excludedList.map((excPath) => (
                  <div
                    key={excPath}
                    className="px-3 py-1.5 flex items-center justify-between text-xs hover:bg-[#222222] transition-colors"
                  >
                    <div className="flex items-center space-x-2 min-w-0 mr-3">
                      <span className="text-rose-400 font-bold text-xs shrink-0">✕</span>
                      <span className="font-mono text-rose-300 text-[11px] truncate" title={excPath}>
                        {excPath}
                      </span>
                    </div>

                    <button
                      type="button"
                      onClick={() => onToggleExclude(excPath, excPath.endsWith("/"))}
                      className="text-[10px] px-2 py-0.5 rounded bg-[#20495f]/70 hover:bg-[#20495f] text-[#60CDFF] hover:text-white border border-[#60CDFF]/50 transition-colors flex items-center space-x-1 cursor-pointer shrink-0"
                      title="Include this file back into extraction"
                    >
                      <span>↺</span>
                      <span>Include</span>
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

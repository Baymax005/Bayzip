import React, { useState, useEffect, useRef } from "react";

export interface WorkspaceDualPaneViewProps {
  isSplit: boolean;
  onToggleSplit: () => void;
  splitRatio?: number;
  onSplitRatioChange?: (newRatio: number) => void;

  // Primary Pane (Left)
  pane1Header: React.ReactNode;
  pane1Content: React.ReactNode;

  // Secondary Pane (Right)
  pane2Header: React.ReactNode;
  pane2Content: React.ReactNode;
  onClosePane2: () => void;

  // Drag & drop cross-pane events
  onDropFolderToSplit?: (folderPath: string, isDisk?: boolean) => void;
  onDropDiskItemToStage?: (diskPath: string) => void;

  // Bottom drawer (e.g. Staged Items Queue)
  bottomDrawer?: React.ReactNode;
}

export const WorkspaceDualPaneView: React.FC<WorkspaceDualPaneViewProps> = ({
  isSplit,
  onToggleSplit: _onToggleSplit,
  splitRatio = 0.5,
  onSplitRatioChange,
  pane1Header,
  pane1Content,
  pane2Header,
  pane2Content,
  onClosePane2: _onClosePane2,
  onDropFolderToSplit,
  onDropDiskItemToStage,
  bottomDrawer,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [isResizing, setIsResizing] = useState(false);
  const [internalRatio, setInternalRatio] = useState(splitRatio);

  // Drag-to-split hover state in Single Pane mode
  const [isHoveringDropZone, setIsHoveringDropZone] = useState(false);

  // Drag over Pane 2 in Split Mode
  const [isPane2DragOver, setIsPane2DragOver] = useState(false);

  const activeRatio = splitRatio !== undefined ? splitRatio : internalRatio;
  const updateRatio = (r: number) => {
    const clamped = Math.max(0.2, Math.min(0.8, r));
    setInternalRatio(clamped);
    if (onSplitRatioChange) {
      onSplitRatioChange(clamped);
    }
  };

  // Draggable Splitter Mouse Handlers
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsResizing(true);
  };

  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (!containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const relativeX = e.clientX - rect.left;
      const newRatio = relativeX / rect.width;
      updateRatio(newRatio);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isResizing]);

  // Drag-and-drop parsing helper: supports both folders and files
  const handleDropPayload = (e: React.DragEvent) => {
    const folderPath = e.dataTransfer.getData("application/bayzip-folder-path");
    const diskPath = e.dataTransfer.getData("application/bayzip-disk-path");
    const isDir = e.dataTransfer.getData("application/bayzip-is-dir") === "true";
    const archiveEntry = e.dataTransfer.getData("application/bayzip-archive-entry");
    const textData = e.dataTransfer.getData("text/plain");

    let targetFolder = folderPath || (isDir ? diskPath : "");
    let isDisk = Boolean(diskPath) || /^[a-zA-Z]:/.test(targetFolder);

    // If it's a file on physical disk (not directory), open its parent directory in Split View
    if (!targetFolder && diskPath) {
      isDisk = true;
      const clean = diskPath.replace(/\//g, "\\");
      const lastSlash = clean.lastIndexOf("\\");
      if (lastSlash > 0) {
        targetFolder = clean.substring(0, lastSlash + 1);
      } else if (clean.length >= 2 && clean[1] === ":") {
        targetFolder = `${clean.slice(0, 2)}\\`;
      } else {
        targetFolder = clean;
      }
    }

    // If it's an archive entry file, open its parent folder in Split View
    if (!targetFolder && archiveEntry) {
      isDisk = false;
      const clean = archiveEntry.replace(/\\/g, "/");
      const lastSlash = clean.lastIndexOf("/");
      if (lastSlash > 0) {
        targetFolder = clean.substring(0, lastSlash);
      } else {
        targetFolder = ""; // archive root
      }
    }

    // Fallback: textData (from external drag or standard drag)
    if (!targetFolder && textData) {
      const clean = textData.trim();
      const isLikelyDisk = /^[a-zA-Z]:/.test(clean);
      if (clean.endsWith("/") || clean.endsWith("\\") || !clean.includes(".")) {
        targetFolder = clean;
        isDisk = isLikelyDisk;
      } else if (isLikelyDisk) {
        const norm = clean.replace(/\//g, "\\");
        const lastSlash = norm.lastIndexOf("\\");
        targetFolder = lastSlash > 0 ? norm.substring(0, lastSlash + 1) : norm;
        isDisk = true;
      } else {
        const norm = clean.replace(/\\/g, "/");
        const lastSlash = norm.lastIndexOf("/");
        targetFolder = lastSlash > 0 ? norm.substring(0, lastSlash) : "";
        isDisk = false;
      }
    }

    if (onDropFolderToSplit && targetFolder !== undefined && targetFolder !== "") {
      onDropFolderToSplit(targetFolder, isDisk);
    } else if (diskPath && onDropDiskItemToStage) {
      onDropDiskItemToStage(diskPath);
    }
  };

  return (
    <div
      ref={containerRef}
      className="flex-1 bg-[#181818] overflow-hidden flex flex-col min-w-0 relative select-none"
    >
      {/* Main Panes Container */}
      <div className="flex-1 flex overflow-hidden min-h-0 relative">
        {/* PANE 1 (Left / Primary) */}
        <div
          style={{ width: isSplit ? `${activeRatio * 100}%` : "100%" }}
          className="flex flex-col min-w-0 h-full overflow-hidden relative"
        >
          {/* Pane 1 Header Bar */}
          <div className="shrink-0">{pane1Header}</div>

          {/* Pane 1 Scrollable Content */}
          <div
            onDragOver={(e) => {
              if (!isSplit) {
                // Unconditionally prevent default so Chromium never renders the red forbidden cursor
                e.preventDefault();
                const rect = e.currentTarget.getBoundingClientRect();
                const inDropZone = e.clientX >= rect.right - Math.max(160, rect.width * 0.35);
                if (inDropZone) {
                  e.dataTransfer.dropEffect = "copy";
                  if (!isHoveringDropZone) setIsHoveringDropZone(true);
                } else {
                  e.dataTransfer.dropEffect = "move";
                  if (isHoveringDropZone) setIsHoveringDropZone(false);
                }
              }
            }}
            className="flex-1 flex flex-col overflow-hidden min-h-0 relative"
          >
            {pane1Content}
          </div>

          {/* Single Mode: Drag-to-Split Edge Drop Target (Right 35% trigger) */}
          {!isSplit && (
            <div
              onDragOver={(e) => {
                e.preventDefault();
                e.stopPropagation();
                e.dataTransfer.dropEffect = "copy";
                if (!isHoveringDropZone) setIsHoveringDropZone(true);
              }}
              onDragLeave={(e) => {
                e.stopPropagation();
                const related = e.relatedTarget as Node | null;
                if (e.currentTarget.contains(related)) return;
                setIsHoveringDropZone(false);
              }}
              onDrop={(e) => {
                e.preventDefault();
                e.stopPropagation();
                setIsHoveringDropZone(false);
                handleDropPayload(e);
              }}
              className={`absolute top-0 right-0 bottom-0 w-1/3 transition-all duration-150 flex items-center justify-center ${
                isHoveringDropZone
                  ? "bg-[#173042]/90 border-2 border-dashed border-[#60CDFF] shadow-[inset_0_0_35px_rgba(96,205,255,0.35),0_0_25px_rgba(96,205,255,0.25)] z-30 pointer-events-auto"
                  : "bg-transparent pointer-events-none"
              }`}
            >
              {isHoveringDropZone && (
                <div className="p-5 rounded-xl bg-[#12222E]/95 border-2 border-[#60CDFF] shadow-[0_0_30px_rgba(96,205,255,0.35)] flex flex-col items-center space-y-2 text-center animate-in zoom-in-95 duration-150 pointer-events-none select-none">
                  <div className="w-10 h-10 rounded-full bg-[#1C3E56] border border-[#60CDFF]/60 flex items-center justify-center text-xl text-[#60CDFF] shadow-[0_0_15px_rgba(96,205,255,0.3)]">
                    ⫴
                  </div>
                  <p className="text-sm font-bold text-white font-sans tracking-wide">
                    Drop to Open Split View
                  </p>
                  <span className="text-xs text-[#A8A8A8]">
                    Opens side-by-side dual explorer
                  </span>
                </div>
              )}
            </div>
          )}
        </div>

        {/* SPLIT VIEW DIVIDER & PANE 2 */}
        {isSplit && (
          <>
            {/* Draggable Vertical Splitter */}
            <div
              onMouseDown={handleMouseDown}
              className={`w-2 hover:w-2 bg-[#222222] hover:bg-[#60CDFF] active:bg-[#60CDFF] cursor-col-resize transition-colors shrink-0 z-20 flex items-center justify-center group select-none border-x border-[#2A2A2A] ${
                isResizing ? "bg-[#60CDFF]" : ""
              }`}
              title="Drag to resize split panes (Double click to reset 50/50)"
              onDoubleClick={() => updateRatio(0.5)}
            >
              <div className="w-0.5 h-10 bg-[#4A4A4A] group-hover:bg-white rounded-full transition-colors flex flex-col justify-between py-1 items-center">
                <span className="w-0.5 h-0.5 bg-[#888888] group-hover:bg-white rounded-full" />
                <span className="w-0.5 h-0.5 bg-[#888888] group-hover:bg-white rounded-full" />
                <span className="w-0.5 h-0.5 bg-[#888888] group-hover:bg-white rounded-full" />
              </div>
            </div>

            {/* PANE 2 (Right / Secondary) */}
            <div
              style={{ width: `${(1 - activeRatio) * 100}%` }}
              onDragOver={(e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = "copy";
                setIsPane2DragOver(true);
              }}
              onDragLeave={(e) => {
                const related = e.relatedTarget as Node | null;
                if (e.currentTarget.contains(related)) return;
                setIsPane2DragOver(false);
              }}
              onDrop={(e) => {
                e.preventDefault();
                setIsPane2DragOver(false);
                handleDropPayload(e);
              }}
              className="flex flex-col min-w-0 h-full overflow-hidden relative border-l border-[#262626]"
            >
              {/* Pane 2 Header Bar */}
              <div className="shrink-0">{pane2Header}</div>

              {/* Pane 2 Scrollable Content */}
              <div className="flex-1 flex flex-col overflow-hidden min-h-0 relative">
                {pane2Content}
              </div>

              {/* Drop Highlight Overlay for Pane 2 */}
              {isPane2DragOver && (
                <div className="absolute inset-0 bg-[#12222E]/80 border-2 border-dashed border-[#60CDFF] z-30 flex items-center justify-center p-4 pointer-events-none animate-in fade-in duration-100">
                  <div className="p-4 rounded-xl bg-[#173042] border border-[#60CDFF]/80 shadow-2xl flex flex-col items-center space-y-2 text-center">
                    <span className="text-3xl">📥</span>
                    <p className="text-xs font-bold text-white font-sans">
                      Drop to Stage / Open in Pane 2
                    </p>
                    <span className="text-[10px] text-[#60CDFF] font-mono">
                      Items will be placed into this target
                    </span>
                  </div>
                </div>
              )}
            </div>
          </>
        )}
      </div>

      {/* Bottom Drawer (e.g. Staged Items Queue or Extract Queue) */}
      {bottomDrawer && <div className="shrink-0">{bottomDrawer}</div>}
    </div>
  );
};

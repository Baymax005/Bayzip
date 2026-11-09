import { getCurrentWindow } from "@tauri-apps/api/window";
import { BayzipLogo } from "./BayzipLogo";

export interface TitlebarProps {
  activeDocument?: string;
}

export function Titlebar({ activeDocument }: TitlebarProps = {}) {
  return (
    <div
      data-tauri-drag-region
      onDoubleClick={(e) => {
        if (!(e.target as HTMLElement).closest("button")) {
          getCurrentWindow().toggleMaximize().catch(() => {});
        }
      }}
      className="h-8 w-full flex justify-between items-center bg-[#141416] cursor-default select-none border-b border-[#2E2E34] shrink-0"
    >
      {/* Left Branding Area - Fully Grabbable */}
      <div data-tauri-drag-region className="flex items-center space-x-2 px-3 cursor-default">
        <div data-tauri-drag-region className="w-4 h-4 flex items-center justify-center pointer-events-none">
          <BayzipLogo className="w-3.5 h-3.5 rounded-sm pointer-events-none" />
        </div>
        <span data-tauri-drag-region className="text-xs font-semibold text-[#F4F4F6] tracking-wide pointer-events-none">
          Bayzip
        </span>
        {activeDocument && (
          <>
            <span data-tauri-drag-region className="text-slate-600 text-xs pointer-events-none">
              -
            </span>
            <span data-tauri-drag-region className="text-xs text-[#9CA3AF] font-mono truncate max-w-sm pointer-events-none">
              {activeDocument}
            </span>
          </>
        )}
      </div>

      {/* Dedicated Draggable Center Surface */}
      <div data-tauri-drag-region className="flex-1 h-full cursor-default" />

      {/* Window Controls */}
      <div className="flex h-full">
        {/* Minimize */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            getCurrentWindow().minimize().catch((err) => console.error("Minimize error:", err));
          }}
          className="h-full px-3.5 hover:bg-[#252529] flex items-center justify-center text-[#9CA3AF] hover:text-white transition-colors cursor-pointer"
          title="Minimize"
        >
          <svg width="10" height="1" viewBox="0 0 10 1">
            <rect width="10" height="1" fill="currentColor" />
          </svg>
        </button>
        {/* Maximize */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            getCurrentWindow().toggleMaximize().catch((err) => console.error("Maximize error:", err));
          }}
          className="h-full px-3.5 hover:bg-[#252529] flex items-center justify-center text-[#9CA3AF] hover:text-white transition-colors cursor-pointer"
          title="Maximize"
        >
          <svg width="10" height="10" viewBox="0 0 10 10">
            <path d="M0,0v10h10V0H0z M9,9H1V1h8V9z" fill="currentColor" />
          </svg>
        </button>
        {/* Close */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            getCurrentWindow().close().catch((err) => console.error("Close error:", err));
          }}
          className="h-full px-4 hover:bg-[#E81123] hover:text-white flex items-center justify-center text-[#9CA3AF] transition-colors cursor-pointer"
          title="Close"
        >
          <svg width="10" height="10" viewBox="0 0 10 10">
            <path d="M10,1L9,0L5,4L1,0L0,1l4,4l-4,4l1,1l4-4l4,4l1-1l-4-4L10,1z" fill="currentColor" />
          </svg>
        </button>
      </div>
    </div>
  );
}
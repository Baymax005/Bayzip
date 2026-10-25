import React from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { OperationProgressInfo } from "../../types";
import { WorkspaceHeaderProgressHUD } from "./WorkspaceHeaderProgressHUD";

export interface WorkspaceHeaderProps {
  isFullModal?: boolean;
  mode?: "compress" | "extract";
  onSwitchMode?: (newMode: "compress" | "extract") => void;
  onSelectArchive?: () => void;
  onOpenExtractModal?: () => void;
  onAddOrCompress?: () => void;
  onTestArchive?: () => void;
  hasArchiveLoaded?: boolean;
  archiveName?: string;
  inspectorTitle?: string;
  itemCount: number;
  operationProgress?: OperationProgressInfo | null;
  isHeaderHUDOpen: boolean;
  onToggleHeaderHUD: () => void;
  onCloseHeaderHUD: () => void;
  onStartNewFolder?: () => void;
  // Workspace Panel Layout Controls
  isLeftPanelOpen?: boolean;
  onToggleLeftPanel?: () => void;
  isSplitView?: boolean;
  onToggleSplitView?: () => void;
  isBottomPanelOpen?: boolean;
  onToggleBottomPanel?: () => void;
  bottomPanelBadge?: number;
  hasRightPanel?: boolean;
  isRightPanelOpen?: boolean;
  onToggleRightPanel?: () => void;
  rightPanelTitle?: string;
  onToggleCompactView?: () => void;
  onExpandToWorkspace?: () => void;
}

export const WorkspaceHeader: React.FC<WorkspaceHeaderProps> = ({
  isFullModal = false,
  mode = "compress",
  onSwitchMode: _onSwitchMode,
  onSelectArchive,
  onOpenExtractModal,
  onAddOrCompress,
  onTestArchive,
  hasArchiveLoaded = false,
  archiveName,
  inspectorTitle,
  itemCount,
  operationProgress,
  isHeaderHUDOpen,
  onToggleHeaderHUD,
  onCloseHeaderHUD,
  onStartNewFolder,
  isLeftPanelOpen = true,
  onToggleLeftPanel,
  isSplitView = false,
  onToggleSplitView,
  isBottomPanelOpen = false,
  onToggleBottomPanel,
  bottomPanelBadge = 0,
  hasRightPanel = false,
  isRightPanelOpen = false,
  onToggleRightPanel,
  rightPanelTitle = "Options",
  onToggleCompactView,
  onExpandToWorkspace,
}) => {
  return (
    <header
      className={`px-3.5 py-2 ${
        isFullModal
          ? "bg-[#181818] border-b border-[#2C2C2C]"
          : "bg-[#242424] border-b border-[#383838]"
      } flex items-center justify-between gap-2 shrink-0 select-none z-30`}
      data-tauri-drag-region={isFullModal ? "true" : undefined}
    >
      {/* Left Cluster: App Branding & Mode Switching */}
      <div className="flex items-center space-x-2.5 min-w-0" data-tauri-drag-region={isFullModal ? "true" : undefined}>
        {isFullModal ? (
          <div className="flex items-center space-x-2.5 mr-1" data-tauri-drag-region="true">
            <span className="w-2.5 h-2.5 rounded-full bg-[#60CDFF] shadow-[0_0_8px_rgba(96,205,255,0.8)]" />
            <span className="font-bold text-xs tracking-wider text-white uppercase font-sans mr-1">
              BAYZIP
            </span>

            {/* Desktop Archiver Primary Action Toolbar */}
            <div className="flex items-center space-x-1.5" data-tauri-drag-region="false">
              {/* 1. Open Archive Button */}
              {onSelectArchive && (
                <button
                  type="button"
                  data-tauri-drag-region="false"
                  onClick={onSelectArchive}
                  className="px-2.5 py-1 text-xs font-semibold rounded-lg bg-[#202020] hover:bg-[#282828] text-[#D0D0D0] hover:text-white border border-[#3E3E3E] hover:border-[#60CDFF]/50 transition-all cursor-pointer flex items-center space-x-1.5 shadow-sm"
                  title="Open Archive File from PC (.bay, .zip, .7z, .tar, .rar) [Ctrl+O]"
                >
                  <svg className="w-3.5 h-3.5 text-[#60CDFF]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z" />
                  </svg>
                  <span>Open</span>
                </button>
              )}

              {/* 2. Extract Button (Opens Extraction Popup Modal) */}
              {onOpenExtractModal && (
                <button
                  type="button"
                  data-tauri-drag-region="false"
                  onClick={onOpenExtractModal}
                  className={`px-3 py-1 text-xs font-semibold rounded-lg transition-all cursor-pointer flex items-center space-x-1.5 shadow-sm ${
                    hasArchiveLoaded
                      ? "bg-gradient-to-r from-[#173042] to-[#12222E] hover:from-[#1F415A] hover:to-[#172D3D] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/60 hover:border-[#60CDFF] shadow-[0_0_10px_rgba(96,205,255,0.2)]"
                      : "bg-[#202020] hover:bg-[#282828] text-[#888888] hover:text-[#CCCCCC] border border-[#3E3E3E]"
                  }`}
                  title="Extract Archive with Options & Password (Ctrl+E)"
                >
                  <span className="text-[#60CDFF] text-sm leading-none">⚡</span>
                  <span>Extract</span>
                </button>
              )}

              {/* 3. Add / Compress Button */}
              {onAddOrCompress && (
                <button
                  type="button"
                  data-tauri-drag-region="false"
                  onClick={onAddOrCompress}
                  className={`px-2.5 py-1 text-xs font-semibold rounded-lg transition-all cursor-pointer flex items-center space-x-1.5 shadow-sm ${
                    isRightPanelOpen
                      ? "bg-[#25394A] text-[#60CDFF] border border-[#60CDFF]/50"
                      : "bg-[#202020] hover:bg-[#282828] text-[#D0D0D0] hover:text-white border border-[#3E3E3E] hover:border-[#60CDFF]/50"
                  }`}
                  title="Add / Compress Files (Configure algorithm, format & settings)"
                >
                  <span className="text-xs leading-none">📦</span>
                  <span>Add</span>
                </button>
              )}

              {/* 4. Test Archive Button */}
              {onTestArchive && hasArchiveLoaded && (
                <button
                  type="button"
                  data-tauri-drag-region="false"
                  onClick={onTestArchive}
                  className="px-2.5 py-1 text-xs font-semibold rounded-lg bg-[#202020] hover:bg-[#282828] text-[#D0D0D0] hover:text-white border border-[#3E3E3E] hover:border-emerald-500/50 transition-all cursor-pointer flex items-center space-x-1.5 shadow-sm"
                  title="Test Archive Integrity"
                >
                  <svg className="w-3.5 h-3.5 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <span>Test</span>
                </button>
              )}

              {/* Archive Name Badge */}
              {archiveName && hasArchiveLoaded && (
                <span
                  className="hidden xl:inline-flex items-center space-x-1 px-2 py-0.5 rounded bg-[#171717] border border-[#2F2F2F] text-[11px] font-mono text-[#A8A8A8] truncate max-w-[180px]"
                  title={archiveName}
                >
                  <span className="text-[#60CDFF]">📁</span>
                  <span className="truncate">{archiveName}</span>
                </span>
              )}
            </div>
          </div>
        ) : (
          <>
            <svg className="w-4 h-4 text-[#60CDFF]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
            <span className="text-xs font-semibold text-white tracking-wide truncate max-w-xs">
              {inspectorTitle || (mode === "compress" ? "Staged Items Inspector" : "Archive Explorer")}
            </span>
          </>
        )}

        {/* Item Count Badge */}
        <span className="text-[10px] font-mono bg-[#282828] text-[#A8A8A8] px-2 py-0.5 rounded-full border border-[#3E3E3E] shrink-0">
          {itemCount} item{itemCount !== 1 ? "s" : ""}
        </span>
      </div>

      {/* Center: Running Operation HUD Banner */}
      {operationProgress && (
        <div className="flex-1 flex justify-center items-center min-w-0 px-2" data-tauri-drag-region="false">
          <WorkspaceHeaderProgressHUD
            progress={operationProgress}
            isOpen={isHeaderHUDOpen}
            onToggleOpen={onToggleHeaderHUD}
            onClose={onCloseHeaderHUD}
          />
        </div>
      )}

      {/* Right Controls Cluster */}
      <div className="flex items-center space-x-2 shrink-0" data-tauri-drag-region="false">

        {/* + New Folder Button */}
        {onStartNewFolder && (
          <button
            type="button"
            onClick={onStartNewFolder}
            className="px-2 py-1 text-xs rounded-lg font-medium transition-all flex items-center space-x-1 bg-[#25394A] hover:bg-[#2C4459] text-[#60CDFF] border border-[#60CDFF]/30 hover:border-[#60CDFF]/60 cursor-pointer shadow-sm"
            title="Create New Folder (Ctrl+Shift+N)"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 13h6m-3-3v6m-9 1V7a2 2 0 012-2h6l2 2h6a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z" />
            </svg>
            <span className="hidden sm:inline text-[11px]">New Folder</span>
          </button>
        )}

        {/* Workspace Panel Layout Controls (When in Workspace) */}
        {isFullModal ? (
          <>
            <div className="flex items-center space-x-1 bg-[#141414] p-0.5 rounded-lg border border-[#383838]">
              {/* Toggle Left Explorer (Tree) */}
              {onToggleLeftPanel && (
                <button
                  type="button"
                  onClick={onToggleLeftPanel}
                  className={`p-1.5 rounded transition-all cursor-pointer ${
                    isLeftPanelOpen
                      ? "bg-[#242424] text-[#60CDFF]"
                      : "text-[#777777] hover:text-[#CCCCCC] hover:bg-[#202020]"
                  }`}
                  title={isLeftPanelOpen ? "Hide Folder Tree (Ctrl+B)" : "Show Folder Tree (Ctrl+B)"}
                >
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <rect x="3" y="3" width="18" height="18" rx="2" strokeWidth="2" />
                    <line x1="9" y1="3" x2="9" y2="21" strokeWidth="2" />
                  </svg>
                </button>
              )}

              {/* Toggle Dual-Pane Split View */}
              {onToggleSplitView && (
                <button
                  type="button"
                  onClick={onToggleSplitView}
                  className={`p-1.5 rounded transition-all cursor-pointer ${
                    isSplitView
                      ? "bg-[#25394A] text-[#60CDFF] shadow-[0_0_8px_rgba(96,205,255,0.3)]"
                      : "text-[#777777] hover:text-[#CCCCCC] hover:bg-[#202020]"
                  }`}
                  title={isSplitView ? "Close Split View (Ctrl+\\)" : "Instant Dual-Pane Split View (Ctrl+\\)"}
                >
                  <span className="font-mono text-xs font-bold leading-none block px-0.5">⫴</span>
                </button>
              )}

              {/* Toggle Bottom Queue / Drawer */}
              {onToggleBottomPanel && (
                <button
                  type="button"
                  onClick={onToggleBottomPanel}
                  className={`p-1.5 rounded transition-all cursor-pointer relative ${
                    isBottomPanelOpen
                      ? "bg-[#242424] text-[#60CDFF]"
                      : "text-[#777777] hover:text-[#CCCCCC] hover:bg-[#202020]"
                  }`}
                  title={isBottomPanelOpen ? "Hide Queue Drawer (Ctrl+J)" : "Show Queue Drawer (Ctrl+J)"}
                >
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <rect x="3" y="3" width="18" height="18" rx="2" strokeWidth="2" />
                    <line x1="3" y1="15" x2="21" y2="15" strokeWidth="2" />
                  </svg>
                  {bottomPanelBadge > 0 && !isBottomPanelOpen && (
                    <span className="absolute -top-1 -right-1 w-2 h-2 rounded-full bg-[#60CDFF] animate-pulse" />
                  )}
                </button>
              )}

              {/* Toggle Right Options Sidebar */}
              {hasRightPanel && onToggleRightPanel && (
                <button
                  type="button"
                  onClick={onToggleRightPanel}
                  className={`p-1.5 rounded transition-all cursor-pointer ${
                    isRightPanelOpen
                      ? "bg-[#242424] text-[#60CDFF]"
                      : "text-[#777777] hover:text-[#CCCCCC] hover:bg-[#202020]"
                  }`}
                  title={isRightPanelOpen ? `Hide ${rightPanelTitle}` : `Show ${rightPanelTitle}`}
                >
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <rect x="3" y="3" width="18" height="18" rx="2" strokeWidth="2" />
                    <line x1="15" y1="3" x2="15" y2="21" strokeWidth="2" />
                  </svg>
                </button>
              )}

              {/* Restore / Compact View Toggle */}
              {onToggleCompactView && (
                <button
                  type="button"
                  onClick={onToggleCompactView}
                  className="p-1.5 text-[#777777] hover:text-white hover:bg-[#242424] rounded transition-all cursor-pointer border-l border-[#333333] ml-0.5"
                  title="Switch to Compact View (Esc)"
                >
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 14h6m0 0v6m0-6L3 21m17-7h-6m0 0v6m0-6l7 7M10 4v6m0 0H4m6 0L3 3m10 7h6m-6 0V4m0 6l7-7" />
                  </svg>
                </button>
              )}
            </div>

            {/* Native Windows Titlebar Window Controls */}
            <div className="flex items-center h-full ml-1 border-l border-[#333333] pl-1.5 space-x-0.5" data-tauri-drag-region="false">
              {/* Minimize */}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  getCurrentWindow().minimize().catch(() => {});
                }}
                className="p-1.5 hover:bg-[#333333] text-[#A8A8A8] hover:text-white rounded transition-colors cursor-pointer"
                title="Minimize Window"
              >
                <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor" /></svg>
              </button>
              {/* Maximize / Restore */}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  getCurrentWindow().toggleMaximize().catch(() => {});
                }}
                className="p-1.5 hover:bg-[#333333] text-[#A8A8A8] hover:text-white rounded transition-colors cursor-pointer"
                title="Maximize / Restore Window"
              >
                <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0,0v10h10V0H0z M9,9H1V1h8V9z" fill="currentColor" /></svg>
              </button>
              {/* Close */}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  getCurrentWindow().close().catch(() => {});
                }}
                className="p-1.5 hover:bg-[#E81123] text-[#A8A8A8] hover:text-white rounded transition-colors cursor-pointer"
                title="Close Bayzip"
              >
                <svg width="10" height="10" viewBox="0 0 10 10"><path d="M10,1L9,0L5,4L1,0L0,1l4,4l-4,4l1,1l4-4l4,4l1-1l-4-4L10,1z" fill="currentColor" /></svg>
              </button>
            </div>
          </>
        ) : (
          /* Expand into Full Workspace Button (in embedded compact card) */
          onExpandToWorkspace && (
            <button
              type="button"
              onClick={onExpandToWorkspace}
              className="p-1.5 bg-[#2D2D2D] hover:bg-[#383838] text-[#A8A8A8] hover:text-white rounded border border-[#3E3E3E] transition-colors cursor-pointer"
              title="Expand into Full Workspace (⤢)"
            >
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
              </svg>
            </button>
          )
        )}
      </div>
    </header>
  );
};

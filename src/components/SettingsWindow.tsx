import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function SettingsWindow() {
  const [isSyncingRegistry, setIsSyncingRegistry] = useState(false);
  const [contextMenuStatus, setContextMenuStatus] = useState<string | null>(null);

  // Close on Escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        getCurrentWindow().hide().catch(() => {});
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const handleRegisterContextMenu = async () => {
    try {
      setIsSyncingRegistry(true);
      setContextMenuStatus(null);
      const res = await invoke<string>("register_context_menu");
      setContextMenuStatus(res);
    } catch (e) {
      setContextMenuStatus(`Failed to register: ${e}`);
    } finally {
      setIsSyncingRegistry(false);
    }
  };

  const handleClose = () => {
    getCurrentWindow().hide().catch(() => {});
  };

  const handleMinimize = () => {
    getCurrentWindow().minimize().catch(() => {});
  };

  return (
    <div className="h-screen w-screen bg-[#1E1E1E] text-white flex flex-col font-sans select-none overflow-hidden border border-[#3E3E3E] rounded-xl shadow-2xl">
      {/* Native Drag Region Titlebar */}
      <div
        data-tauri-drag-region
        className="h-11 w-full flex justify-between items-center bg-[#252525] border-b border-[#333333] cursor-default select-none shrink-0"
      >
        {/* Left Branding Area - Fully Grabbable */}
        <div data-tauri-drag-region className="flex items-center space-x-2.5 px-4 cursor-default">
          <div data-tauri-drag-region className="w-5 h-5 flex items-center justify-center pointer-events-none">
            <img
              src="/bayzip-logo.png"
              alt="Bayzip"
              className="w-4 h-4 object-contain pointer-events-none rounded-sm"
              onError={(e) => {
                (e.currentTarget as HTMLElement).style.display = "none";
              }}
            />
          </div>
          <span data-tauri-drag-region className="text-xs font-semibold text-white tracking-wide pointer-events-none">
            Bayzip Preferences
          </span>
          <span data-tauri-drag-region className="text-[10px] font-mono text-[#888888] bg-[#1A1A1A] px-2 py-0.5 rounded border border-[#333333]">
            v1.0.7
          </span>
        </div>

        {/* Center Draggable Surface */}
        <div data-tauri-drag-region className="flex-1 h-full cursor-default" />

        {/* Window Controls */}
        <div className="flex h-full" data-tauri-drag-region="false">
          <button
            type="button"
            onClick={handleMinimize}
            className="h-full px-4 hover:bg-[#333333] flex items-center justify-center text-[#A8A8A8] hover:text-white transition-colors cursor-pointer"
            title="Minimize"
          >
            <svg width="10" height="1" viewBox="0 0 10 1">
              <rect width="10" height="1" fill="currentColor" />
            </svg>
          </button>
          <button
            type="button"
            onClick={handleClose}
            className="h-full px-4 hover:bg-[#C42B1C] hover:text-white flex items-center justify-center text-[#A8A8A8] transition-colors cursor-pointer"
            title="Close (Esc)"
          >
            <svg width="10" height="10" viewBox="0 0 10 10">
              <path d="M1 1L9 9M9 1L1 9" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
            </svg>
          </button>
        </div>
      </div>

      {/* Main Settings Body */}
      <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-5 bg-[#1B1B1B]">
        {/* Windows Explorer Integration Card */}
        <div className="bg-[#222222] rounded-xl p-4 border border-[#333333] space-y-3.5 shadow-sm">
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
              <span>Compress to .bay, .zip, .7z, .tar</span>
            </div>
            <div className="flex items-start space-x-2">
              <svg className="w-3.5 h-3.5 text-[#60CDFF] shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
              </svg>
              <span>Instant Extract Here</span>
            </div>
            <div className="flex items-start space-x-2">
              <svg className="w-3.5 h-3.5 text-[#60CDFF] shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
              </svg>
              <span>Extract to Dedicated Subfolder</span>
            </div>
            <div className="flex items-start space-x-2">
              <svg className="w-3.5 h-3.5 text-[#60CDFF] shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
              </svg>
              <span>Open in Bayzip Inspector</span>
            </div>
          </div>

          <div className="pt-2 border-t border-[#2C2C2C] flex items-center justify-between">
            <div className="text-[11px] text-[#7A7A7A]">
              {contextMenuStatus ? (
                <span className="text-[#60CDFF] font-medium">{contextMenuStatus}</span>
              ) : (
                <span>Synced with current Windows user session</span>
              )}
            </div>
            <button
              type="button"
              onClick={handleRegisterContextMenu}
              disabled={isSyncingRegistry}
              className={`px-3 py-1.5 rounded-md text-xs font-medium transition-all flex items-center space-x-1.5 cursor-pointer ${
                isSyncingRegistry
                  ? "bg-[#252525] text-[#7A7A7A] border border-[#383838]"
                  : "bg-[#22394A] hover:bg-[#2A475D] border border-[#60CDFF]/40 text-[#60CDFF] shadow-sm hover:shadow"
              }`}
            >
              {isSyncingRegistry ? (
                <>
                  <div className="w-3 h-3 border-2 border-[#60CDFF] border-t-transparent rounded-full animate-spin"></div>
                  <span>Syncing...</span>
                </>
              ) : (
                <>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-3.5 h-3.5">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                  </svg>
                  <span>Re-Sync Registry</span>
                </>
              )}
            </button>
          </div>
        </div>

        {/* File Associations Card */}
        <div className="bg-[#222222] rounded-xl p-4 border border-[#333333] space-y-3 shadow-sm">
          <div className="flex items-center space-x-2">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4 text-[#E8C488]">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
            </svg>
            <span className="text-xs font-semibold text-white">Default File Associations</span>
          </div>

          <p className="text-xs text-[#9E9E9E] leading-relaxed">
            Windows 11 opens archives with Bayzip when double-clicked in File Explorer.
          </p>

          <div className="flex flex-wrap gap-2 pt-1">
            <div className="flex items-center space-x-1.5 px-2.5 py-1 rounded-md bg-[#181818] border border-[#353535] text-xs">
              <span className="w-2 h-2 rounded-full bg-[#60CDFF]"></span>
              <span className="font-mono text-white text-[11px]">.bay</span>
              <span className="text-[10px] text-[#7A7A7A]">• Bayzip Archive</span>
            </div>
            <div className="flex items-center space-x-1.5 px-2.5 py-1 rounded-md bg-[#181818] border border-[#353535] text-xs">
              <span className="w-2 h-2 rounded-full bg-[#E8C488]"></span>
              <span className="font-mono text-white text-[11px]">.bay_gamer</span>
              <span className="text-[10px] text-[#7A7A7A]">• Gamer Modpack</span>
            </div>
          </div>
        </div>

        {/* Software Updates Card */}
        <div className="bg-[#222222] rounded-xl p-4 border border-[#333333] flex items-center justify-between shadow-sm">
          <div className="flex items-center space-x-3">
            <div className="w-8 h-8 rounded-lg bg-[#181818] border border-[#383838] flex items-center justify-center text-[#60CDFF] shrink-0">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-4 h-4">
                <path strokeLinecap="round" strokeLinejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
              </svg>
            </div>
            <div>
              <p className="text-xs font-semibold text-white">Application Updates</p>
              <p className="text-[11px] text-[#888888]">Current version v1.0.7 • Up to date</p>
            </div>
          </div>
          <button
            type="button"
            disabled
            className="px-3 py-1.5 rounded-md bg-[#181818] border border-[#333333] text-[11px] font-medium text-[#7A7A7A] cursor-not-allowed"
          >
            Check for Updates
          </button>
        </div>
      </div>

      {/* Footer */}
      <div className="bg-[#181818] px-6 py-2.5 border-t border-[#2A2A2A] flex items-center justify-between text-[11px] text-[#7A7A7A] shrink-0">
        <span>Bayzip v1.0.7 • Native Settings Window</span>
        <span className="font-mono text-[#888888]">Press Esc to close</span>
      </div>
    </div>
  );
}

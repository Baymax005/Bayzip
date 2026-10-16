interface AboutModalProps {
  isOpen: boolean;
  onClose: () => void;
  isSyncingRegistry: boolean;
  contextMenuStatus: string | null;
  onRegisterContextMenu: () => void;
}

export function AboutModal({
  isOpen,
  onClose,
  isSyncingRegistry,
  contextMenuStatus,
  onRegisterContextMenu,
}: AboutModalProps) {
  if (!isOpen) return null;

  return (
    <div
      onClick={onClose}
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-md p-4 animate-fadeIn"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="bg-[#1F1F1F] border border-[#3A3A3A] rounded-2xl p-6 sm:p-7 max-w-lg w-full shadow-[0_20px_60px_rgba(0,0,0,0.85),0_0_30px_rgba(96,205,255,0.06)] relative space-y-5"
      >
        {/* Close Button */}
        <button
          type="button"
          onClick={onClose}
          className="absolute top-4 right-4 w-7 h-7 rounded-lg bg-[#262626] hover:bg-[#333333] border border-[#3A3A3A] hover:border-[#555555] text-[#A8A8A8] hover:text-white flex items-center justify-center transition-all cursor-pointer shadow-sm"
          title="Close"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>

        {/* Header Hero */}
        <div className="flex items-center space-x-4 pr-6">
          <div className="w-13 h-13 rounded-2xl bg-gradient-to-br from-[#183144] to-[#0D1C27] border border-[#60CDFF]/40 p-2 shadow-[0_0_20px_rgba(96,205,255,0.25)] flex items-center justify-center shrink-0">
            <img
              src="/bayzip-logo.png"
              alt="Bayzip"
              className="w-full h-full object-contain filter drop-shadow-[0_2px_8px_rgba(96,205,255,0.4)]"
            />
          </div>
          <div>
            <div className="flex items-center space-x-2.5">
              <h2 className="text-xl font-bold tracking-tight text-white font-sans">Bayzip</h2>
              <span className="text-[11px] font-mono font-semibold bg-[#183144] text-[#60CDFF] px-2.5 py-0.5 rounded-full border border-[#60CDFF]/30 flex items-center space-x-1.5 shadow-sm">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span>
                <span>v1.0.2</span>
              </span>
            </div>
            <p className="text-xs text-[#A8A8A8] mt-0.5">Hardware-Accelerated Archiver for Windows 11</p>
          </div>
        </div>

        {/* Architecture & Tech Spec Pills */}
        <div className="grid grid-cols-2 gap-2 text-xs">
          <div className="bg-[#1A1A1A] border border-[#303030] rounded-lg p-2.5 flex items-center space-x-2.5">
            <div className="w-7 h-7 rounded-md bg-[#252525] border border-[#3A3A3A] flex items-center justify-center shrink-0 text-[#60CDFF] text-xs font-bold">
              ⚡
            </div>
            <div className="min-w-0">
              <p className="font-semibold text-white text-[11px] truncate">Zstandard Multi-Core</p>
              <p className="text-[10px] text-[#888888] truncate">Scales to all CPU threads</p>
            </div>
          </div>
          <div className="bg-[#1A1A1A] border border-[#303030] rounded-lg p-2.5 flex items-center space-x-2.5">
            <div className="w-7 h-7 rounded-md bg-[#252525] border border-[#3A3A3A] flex items-center justify-center shrink-0 text-amber-400 text-xs font-bold">
              🎮
            </div>
            <div className="min-w-0">
              <p className="font-semibold text-white text-[11px] truncate">Gamer Mod Pack</p>
              <p className="text-[10px] text-[#888888] truncate">High-throughput level 3</p>
            </div>
          </div>
          <div className="bg-[#1A1A1A] border border-[#303030] rounded-lg p-2.5 flex items-center space-x-2.5">
            <div className="w-7 h-7 rounded-md bg-[#252525] border border-[#3A3A3A] flex items-center justify-center shrink-0 text-emerald-400 text-xs font-bold">
              🛡️
            </div>
            <div className="min-w-0">
              <p className="font-semibold text-white text-[11px] truncate">Zip-Slip Immune</p>
              <p className="text-[10px] text-[#888888] truncate">Sanitized path extraction</p>
            </div>
          </div>
          <div className="bg-[#1A1A1A] border border-[#303030] rounded-lg p-2.5 flex items-center space-x-2.5">
            <div className="w-7 h-7 rounded-md bg-[#252525] border border-[#3A3A3A] flex items-center justify-center shrink-0 text-cyan-400 text-xs font-bold">
              ⚙️
            </div>
            <div className="min-w-0">
              <p className="font-semibold text-white text-[11px] truncate">OS Priority Guard</p>
              <p className="text-[10px] text-[#888888] truncate">100% smooth UI & gaming</p>
            </div>
          </div>
        </div>

        {/* Creator & Executive Card */}
        <div className="bg-gradient-to-r from-[#222222] to-[#1A1A1A] rounded-xl p-3.5 border border-[#383838] border-l-2 border-l-[#60CDFF] flex items-center justify-between shadow-sm">
          <div className="flex items-center space-x-3">
            <div className="w-9 h-9 rounded-full bg-[#183144] border border-[#60CDFF]/40 text-[#60CDFF] flex items-center justify-center font-bold text-sm shrink-0 shadow-inner">
              M
            </div>
            <div>
              <div className="flex items-center space-x-1.5">
                <span className="text-sm font-semibold text-white">Made by Baymax</span>
                <span className="text-[10px] bg-[#60CDFF]/15 text-[#60CDFF] px-1.5 py-0.2 rounded border border-[#60CDFF]/30 font-medium">Author</span>
              </div>
              <p className="text-xs text-[#9E9E9E]">CEO of DPG • Muhammad Ali</p>
            </div>
          </div>
          <div className="w-8 h-8 rounded-full bg-[#282828] border border-[#3E3E3E] flex items-center justify-center text-[#60CDFF] shadow-inner">
            <svg viewBox="0 0 24 24" fill="currentColor" className="w-4 h-4">
              <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
            </svg>
          </div>
        </div>

        {/* Windows Explorer Shell Integration Card */}
        <div className="bg-[#1A1A1A] rounded-xl p-4 border border-[#333333] space-y-3 shadow-inner">
          <div className="flex justify-between items-center">
            <div className="flex items-center space-x-2">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4 text-[#60CDFF]">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16m-7 6h7" />
              </svg>
              <span className="text-xs font-semibold text-white">Windows 11 Explorer Shell Integration</span>
            </div>
            <span className="text-[10px] text-emerald-400 bg-emerald-950/60 px-2 py-0.5 rounded-full border border-emerald-800/70 font-medium flex items-center space-x-1">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400"></span>
              <span>Native Shell</span>
            </span>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-[11px] text-[#A8A8A8] pt-1">
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

        {/* Footer */}
        <div className="pt-1 flex items-center justify-between text-[11px] text-[#7A7A7A] border-t border-[#2A2A2A]">
          <span>© 2026 DPG (Muhammad Ali) • All rights reserved</span>
          <span className="font-mono text-[#888888]">Rust + Tauri v2</span>
        </div>
      </div>
    </div>
  );
}

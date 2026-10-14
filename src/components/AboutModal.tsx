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
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="bg-[#202020] border border-[#3E3E3E] rounded-xl p-6 sm:p-8 max-w-lg w-full shadow-2xl relative space-y-6"
      >
        <button
          type="button"
          onClick={onClose}
          className="absolute top-4 right-4 text-[#A8A8A8] hover:text-white p-1 rounded-md hover:bg-[#333333] transition-colors cursor-pointer"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-5 h-5">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>

        {/* Header */}
        <div className="flex items-center space-x-3.5">
          <div className="w-11 h-11 rounded-xl bg-[#142330] border border-[#20495f] flex items-center justify-center shrink-0 p-1.5 shadow-md">
            <img
              src="/bayzip-logo.png"
              alt="Bayzip"
              className="w-full h-full object-contain"
            />
          </div>
          <div>
            <div className="flex items-center space-x-2">
              <h2 className="text-xl font-bold tracking-tight text-white">Bayzip</h2>
              <span className="text-[11px] bg-[#2A2A2A] text-[#60CDFF] px-2 py-0.5 rounded-full border border-[#3E3E3E] font-mono">
                v0.1.0
              </span>
            </div>
            <p className="text-xs text-[#A8A8A8] mt-0.5">High-Performance Hardware-Accelerated Archiver</p>
          </div>
        </div>

        <p className="text-xs text-[#C8C8C8] leading-relaxed">
          Engineered with Rust, Zstandard multi-threading, and React. Built to utilize your machine's full CPU cores and RAM for maximum compression and instant extraction.
        </p>

        {/* Creator Card */}
        <div className="bg-[#262626] rounded-lg p-3.5 border border-[#3E3E3E] flex items-center justify-between">
          <div>
            <span className="text-[11px] text-[#60CDFF] font-medium uppercase tracking-wider block">Created By</span>
            <span className="text-sm font-semibold text-white">Made by Baymax</span>
            <span className="text-xs text-[#8A8A8A] block">CEO of DPG (Muhammad Ali)</span>
          </div>
          <div className="w-8 h-8 rounded-full bg-[#60CDFF]/15 text-[#60CDFF] flex items-center justify-center border border-[#60CDFF]/30">
            <svg viewBox="0 0 24 24" fill="currentColor" className="w-4 h-4">
              <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
            </svg>
          </div>
        </div>

        {/* Windows Explorer Integration Card */}
        <div className="bg-[#262626] rounded-lg p-4 border border-[#3E3E3E] space-y-3">
          <div className="flex justify-between items-center">
            <div className="flex items-center space-x-2">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4 text-[#60CDFF]">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16m-7 6h7" />
              </svg>
              <span className="text-xs font-semibold text-white">Windows Explorer Integration</span>
            </div>
            <span className="text-[10px] text-emerald-400 bg-emerald-950/50 px-2 py-0.5 rounded border border-emerald-800/60 font-medium">
              Native Shell
            </span>
          </div>

          <div className="text-[11px] text-[#A8A8A8] space-y-1.5 pl-1">
            <div className="flex items-center space-x-2">
              <span className="text-[#60CDFF]">•</span>
              <span><strong>Extract Menu:</strong> Dedicated "Extract with Bayzip" right-click menu</span>
            </div>
            <div className="flex items-center space-x-2">
              <span className="text-[#60CDFF]">•</span>
              <span><strong>Compress Menu:</strong> Dedicated "Add to archive with Bayzip" (.bay, .zip, .7z, .tar)</span>
            </div>
            <div className="flex items-center space-x-2">
              <span className="text-[#60CDFF]">•</span>
              <span><strong>Universal Uncompress:</strong> .bay, .zip, .7z, .rar, .tar, .gz auto-detected</span>
            </div>
            <div className="flex items-center space-x-2">
              <span className="text-[#60CDFF]">•</span>
              <span><strong>File Icon & Open:</strong> Double-click .bay files to open directly in Bayzip</span>
            </div>
          </div>

          <button
            type="button"
            disabled={isSyncingRegistry}
            onClick={onRegisterContextMenu}
            className={`w-full py-2 bg-[#2D2D2D] hover:bg-[#383838] border border-[#3E3E3E] hover:border-[#60CDFF] text-[#60CDFF] text-xs font-semibold rounded-md transition-all flex items-center justify-center space-x-2 cursor-pointer ${
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
            <div className="text-[11px] text-emerald-400 bg-emerald-950/40 p-2.5 rounded border border-emerald-900/50 flex items-center space-x-2">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-4 h-4 shrink-0 text-emerald-400">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
              </svg>
              <span>{contextMenuStatus}</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

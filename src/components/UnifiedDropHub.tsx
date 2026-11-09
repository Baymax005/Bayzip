import { BayzipLogo } from "./BayzipLogo";

export interface UnifiedDropHubProps {
  onSelectArchive: () => void;
  onSelectFiles: () => void;
  onSelectFolder: () => void;
  isDraggingOver?: boolean;
  activeMode?: "compress" | "extract";
  onSwitchMode?: (mode: "compress" | "extract") => void;
  className?: string;
}

/**
 * Centered empty state drop canvas for Bayzip's full-height File Viewer.
 * Adheres strictly to the Fluent Zinc & Sky palette (#1C1C1F, #2E2E34, #252529, #38BDF8, #10B981).
 */
export function UnifiedDropHub({
  onSelectArchive,
  onSelectFiles,
  onSelectFolder,
  isDraggingOver = false,
  className = "",
}: UnifiedDropHubProps) {
  const supportedPills = [
    { ext: ".bayg", label: "Bayzip Gamer (Zstd Fast)", brand: true },
    { ext: ".bay", label: "Bayzip Standard", brand: true },
    { ext: ".zip", label: "Standard ZIP", brand: false },
    { ext: ".7z", label: "7-Zip LZMA2", brand: false },
    { ext: ".rar", label: "WinRAR Archive", brand: false },
    { ext: ".tar", label: "POSIX Tarball", brand: false },
  ];

  return (
    <div
      className={`relative w-full h-full flex flex-col items-center justify-center text-center select-none p-6 sm:p-10 transition-colors ${
        isDraggingOver ? "bg-[#1B2636]/60" : "bg-[#1C1C1F]"
      } ${className}`}
    >
      {/* Central Bayzip Cube Icon */}
      <div className="relative mb-4">
        <div
          className={`w-16 h-16 rounded-xl flex items-center justify-center transition-all ${
            isDraggingOver
              ? "bg-[#25384D] border border-[#38BDF8] scale-105"
              : "bg-[#252529] border border-[#323238]"
          }`}
        >
          <BayzipLogo className="w-8 h-8 transition-transform hover:scale-105" />
        </div>
      </div>

      {/* Central Typography */}
      <h2 className="text-base sm:text-lg font-semibold tracking-tight text-[#F4F4F6] mb-1.5">
        Drop Anything Here to Begin
      </h2>
      <p className="text-xs text-[#9CA3AF] max-w-sm mx-auto leading-relaxed mb-6 font-sans">
        Drop an archive to extract <span className="text-[#38BDF8] mx-1">•</span> Drop files or folders to compress
      </p>

      {/* Action Buttons Row */}
      <div className="flex flex-wrap items-center justify-center gap-2.5 mb-7">
        <button
          type="button"
          onClick={onSelectArchive}
          className="px-3.5 py-2 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] hover:text-white border border-[#323238] hover:border-[#38BDF8] text-xs font-medium transition-colors flex items-center space-x-1.5 cursor-pointer shadow-sm active:scale-95"
        >
          <span className="text-[#38BDF8]">⚡</span>
          <span>Open Archive to Extract</span>
        </button>

        <button
          type="button"
          onClick={onSelectFiles}
          className="px-3.5 py-2 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] hover:text-white border border-[#323238] hover:border-[#38BDF8] text-xs font-medium transition-colors flex items-center space-x-1.5 cursor-pointer shadow-sm active:scale-95"
        >
          <span className="text-slate-400">📦</span>
          <span>Select Files to Compress</span>
        </button>

        <button
          type="button"
          onClick={onSelectFolder}
          className="px-3.5 py-2 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] hover:text-white border border-[#323238] hover:border-[#38BDF8] text-xs font-medium transition-colors flex items-center space-x-1.5 cursor-pointer shadow-sm active:scale-95"
        >
          <span className="text-[#FBBF24]">📁</span>
          <span>Select Folder</span>
        </button>
      </div>

      {/* Supported Format Pills */}
      <div className="flex flex-wrap items-center justify-center gap-1.5 pt-4 border-t border-[#2E2E34] max-w-md w-full">
        <span className="text-[10px] text-[#9CA3AF] uppercase tracking-wider font-semibold mr-1 font-sans">
          Supported:
        </span>
        {supportedPills.map((fmt) => (
          <span
            key={fmt.ext}
            className={`px-2 py-0.5 rounded-[4px] text-[11px] font-mono select-none ${
              fmt.brand
                ? "bg-[#10B981]/15 text-[#10B981] border border-[#10B981]/30 font-semibold"
                : "bg-[#252529] text-[#9CA3AF] border border-[#323238]"
            }`}
            title={`${fmt.ext} (${fmt.label})`}
          >
            {fmt.ext}
          </span>
        ))}
      </div>
    </div>
  );
}
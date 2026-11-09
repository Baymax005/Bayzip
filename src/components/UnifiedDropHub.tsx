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
 * Unified Windows 11 Fluent dark-mode empty state hub for Bayzip.
 * Renders an inviting, glowing drop target that seamlessly handles archives (extract)
 * or files/folders (compress) with quick action buttons and supported format pills.
 */
export function UnifiedDropHub({
  onSelectArchive,
  onSelectFiles,
  onSelectFolder,
  isDraggingOver = false,
  activeMode,
  onSwitchMode,
  className = "",
}: UnifiedDropHubProps) {
  const handleOpenArchive = () => {
    if (onSwitchMode && activeMode !== "extract") {
      onSwitchMode("extract");
    }
    onSelectArchive();
  };

  const handleSelectFiles = () => {
    if (onSwitchMode && activeMode !== "compress") {
      onSwitchMode("compress");
    }
    onSelectFiles();
  };

  const handleSelectFolder = () => {
    if (onSwitchMode && activeMode !== "compress") {
      onSwitchMode("compress");
    }
    onSelectFolder();
  };

  const supportedPills = [
    { ext: ".bayg", label: "Bayzip Gamer (Zstd Fast)", featured: true },
    { ext: ".bay", label: "Bayzip Standard", featured: true },
    { ext: ".zip", label: "Standard ZIP", featured: false },
    { ext: ".7z", label: "7-Zip LZMA2", featured: false },
    { ext: ".rar", label: "WinRAR Archive", featured: false },
    { ext: ".tar", label: "POSIX Tarball", featured: false },
  ];

  return (
    <div
      className={`relative w-full rounded-2xl border transition-all duration-300 p-8 sm:p-12 flex flex-col items-center justify-center text-center select-none group ${
        isDraggingOver
          ? "border-[#60CDFF] bg-[#142330]/80 shadow-[0_0_40px_rgba(96,205,255,0.3)] ring-2 ring-[#60CDFF]/50 scale-[1.01]"
          : "border-dashed border-[#3A3A3A] bg-gradient-to-b from-[#242424]/90 via-[#1E1E1E]/95 to-[#161616] hover:border-[#60CDFF]/60 hover:shadow-[0_0_25px_rgba(96,205,255,0.12)] shadow-xl"
      } ${className}`}
    >
      {/* Background radial accent glow */}
      <div
        className={`absolute inset-0 pointer-events-none transition-opacity duration-300 rounded-2xl ${
          isDraggingOver
            ? "opacity-100 bg-[radial-gradient(circle_at_center,rgba(96,205,255,0.18)_0%,transparent_70%)]"
            : "opacity-40 group-hover:opacity-85 bg-[radial-gradient(circle_at_center,rgba(96,205,255,0.08)_0%,transparent_70%)]"
        }`}
      />

      {/* Central Icon */}
      <div className="relative mb-5">
        <div
          className={`w-20 h-20 sm:w-24 sm:h-24 rounded-2xl flex items-center justify-center transition-all duration-300 ${
            isDraggingOver
              ? "bg-[#25394A] border-2 border-[#60CDFF] shadow-[0_0_30px_rgba(96,205,255,0.4)] scale-110"
              : "bg-[#202020] border border-[#3E3E3E] group-hover:border-[#60CDFF] group-hover:shadow-[0_0_20px_rgba(96,205,255,0.2)] shadow-md"
          }`}
        >
          <div className="relative flex items-center justify-center">
            <BayzipLogo className="w-10 h-10 sm:w-12 sm:h-12 transition-transform duration-300 group-hover:scale-105" />
          </div>
        </div>

        {/* Ambient ping ring while dragging */}
        {isDraggingOver && (
          <span className="absolute -inset-1 rounded-2xl border-2 border-[#60CDFF] animate-ping opacity-40 pointer-events-none" />
        )}
      </div>

      {/* Central Copy */}
      <h2 className="text-xl sm:text-2xl font-bold tracking-tight text-white mb-2">
        Drop Anything Here to Begin
      </h2>
      <p className="text-xs sm:text-sm text-[#A8A8A8] max-w-md mx-auto leading-relaxed mb-7">
        Drop an archive to extract <span className="text-[#60CDFF] mx-1.5">•</span> Drop files or folders to compress
      </p>

      {/* Clean Action Buttons */}
      <div className="flex flex-wrap items-center justify-center gap-3 mb-8">
        <button
          type="button"
          onClick={handleOpenArchive}
          className="px-4 py-2.5 rounded-lg bg-[#60CDFF] hover:bg-[#4FB8E6] text-black font-semibold text-xs sm:text-sm shadow-md shadow-[#60CDFF]/20 hover:shadow-[#60CDFF]/30 active:scale-[0.98] transition-all flex items-center space-x-2 cursor-pointer"
        >
          <span>⚡</span>
          <span>Open Archive to Extract</span>
        </button>

        <button
          type="button"
          onClick={handleSelectFiles}
          className="px-4 py-2.5 rounded-lg bg-[#2D2D2D] hover:bg-[#383838] text-white border border-[#3E3E3E] hover:border-[#60CDFF] font-medium text-xs sm:text-sm shadow-sm active:scale-[0.98] transition-all flex items-center space-x-2 cursor-pointer"
        >
          <span>📦</span>
          <span>Select Files to Compress</span>
        </button>

        <button
          type="button"
          onClick={handleSelectFolder}
          className="px-4 py-2.5 rounded-lg bg-[#242424] hover:bg-[#2F2F2F] text-[#CCCCCC] hover:text-white border border-[#383838] hover:border-[#555555] font-medium text-xs sm:text-sm active:scale-[0.98] transition-all flex items-center space-x-2 cursor-pointer"
        >
          <span>📁</span>
          <span>Select Folder</span>
        </button>
      </div>

      {/* Quick Supported Format Pills */}
      <div className="flex flex-wrap items-center justify-center gap-2 pt-4 border-t border-[#2C2C2C] max-w-lg w-full">
        <span className="text-[11px] text-[#777777] uppercase tracking-wider font-semibold mr-1">
          Supported:
        </span>
        {supportedPills.map((fmt) => (
          <span
            key={fmt.ext}
            className={`px-2.5 py-0.5 rounded-full text-[11px] font-mono border transition-all select-none ${
              fmt.featured
                ? "bg-[#142330] text-[#60CDFF] border-[#20495f] font-semibold hover:border-[#60CDFF]"
                : "bg-[#1C1C1C] text-[#A8A8A8] border-[#333333] hover:text-white hover:border-[#555555]"
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

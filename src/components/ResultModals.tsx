import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { CompressResult, ExtractResult, ArchiveInfo } from "../types";
import { useModalDismiss } from "../hooks/useModalDismiss";

interface ResultModalsProps {
  compressResult: CompressResult | null;
  onCloseCompressResult: () => void;
  extractResult: ExtractResult | null;
  archiveInfo: ArchiveInfo | null;
  onCloseExtractResult: () => void;
  formatBytes: (bytes: number) => string;
}

function formatElapsed(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const totalSeconds = ms / 1000;
  if (totalSeconds < 60) return `${totalSeconds.toFixed(1)}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const remainingSeconds = Math.round(totalSeconds % 60);
  if (minutes < 60) {
    return remainingSeconds > 0 ? `${minutes}m ${remainingSeconds}s` : `${minutes}m`;
  }
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return `${hours}h ${remainingMinutes}m`;
}

export function ResultModals({
  compressResult,
  onCloseCompressResult,
  extractResult,
  archiveInfo,
  onCloseExtractResult,
  formatBytes,
}: ResultModalsProps) {
  const { handleBackdropClick: handleCompressBackdrop } = useModalDismiss({
    isOpen: Boolean(compressResult),
    onClose: onCloseCompressResult,
  });

  const { handleBackdropClick: handleExtractBackdrop } = useModalDismiss({
    isOpen: Boolean(extractResult),
    onClose: onCloseExtractResult,
  });

  const compressSpeed = compressResult && compressResult.elapsed_ms > 0
    ? `${formatBytes(compressResult.original_size / (compressResult.elapsed_ms / 1000))}/s`
    : null;

  return (
    <>
      {/* Compression Result Modal */}
      {compressResult && (
        <div
          onClick={handleCompressBackdrop}
          className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4 select-none animate-in fade-in duration-150"
        >
          <div className="bg-[#202020] border border-[#3E3E3E] rounded-xl p-8 max-w-md w-full shadow-2xl relative space-y-5 animate-in zoom-in-95 duration-150">
            {compressResult.success ? (
              <>
                <div className="flex items-center space-x-3">
                  <div className="w-10 h-10 rounded-full bg-emerald-500/20 text-emerald-400 flex items-center justify-center shrink-0">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-6 h-6">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                    </svg>
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-white">Archive Created Successfully!</h3>
                    <p className="text-xs text-[#A8A8A8]">
                      Completed in {formatElapsed(compressResult.elapsed_ms)}
                      {compressSpeed ? ` • ${compressSpeed}` : ""}
                    </p>
                  </div>
                </div>

                {/* Archive File Preview Card with Bayzip Logo & ZIP Badge */}
                <div className="bg-[#1A2530] border border-[#0078D4]/40 rounded-xl p-3.5 flex items-center space-x-3.5 shadow-inner">
                  <div className="relative w-12 h-12 rounded-xl bg-[#0F1922] border border-[#60CDFF]/40 flex items-center justify-center shrink-0 shadow">
                    <img
                      src="/bayzip-logo.png"
                      alt="Bayzip Archive"
                      className="w-7 h-7 object-contain"
                      onError={(e) => {
                        (e.currentTarget as HTMLElement).style.display = "none";
                      }}
                    />
                    <span className="absolute -bottom-1 -right-1 bg-[#60CDFF] text-black text-[8px] font-black font-mono px-1 rounded uppercase tracking-wider">
                      {compressResult.output_path.split(".").pop()?.toUpperCase() || "ZIP"}
                    </span>
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center space-x-2">
                      <span className="text-[10px] font-bold uppercase tracking-wider text-[#60CDFF] bg-[#60CDFF]/15 px-2 py-0.5 rounded-full border border-[#60CDFF]/30">
                        Compressed Archive
                      </span>
                    </div>
                    <h4 className="text-sm font-semibold text-white truncate mt-1" title={compressResult.output_path}>
                      {compressResult.output_path.split(/[\\/]/).pop()}
                    </h4>
                    <div className="flex items-center space-x-2 text-xs text-[#8BBEE8] mt-0.5">
                      <span className="font-mono font-medium text-white">{formatBytes(compressResult.compressed_size)}</span>
                      <span>•</span>
                      <span>Bayzip Package</span>
                    </div>
                  </div>
                </div>

                <div className="bg-[#2A2A2A] rounded-lg p-4 border border-[#3E3E3E] space-y-2.5">
                  <div className="flex justify-between text-xs">
                    <span className="text-[#A8A8A8]">Original Size:</span>
                    <span className="font-mono text-white">{formatBytes(compressResult.original_size)}</span>
                  </div>
                  <div className="flex justify-between text-xs">
                    <span className="text-[#A8A8A8]">Compressed Size:</span>
                    <span className="font-mono text-[#60CDFF] font-semibold">
                      {formatBytes(compressResult.compressed_size)}
                    </span>
                  </div>
                  {compressResult.original_size > 0 && (
                    <div className="flex justify-between text-xs border-t border-[#3E3E3E] pt-2">
                      <span className="text-[#A8A8A8]">Space Saved:</span>
                      <span className="font-semibold text-emerald-400">
                        {Math.max(
                          0,
                          Number(((1 - compressResult.compressed_size / compressResult.original_size) * 100).toFixed(1))
                        )}
                        %
                      </span>
                    </div>
                  )}
                </div>

                <div className="text-xs bg-[#1A1A1A] p-3 rounded border border-[#333]">
                  <span className="text-[#7A7A7A] block mb-1">Archive Path:</span>
                  <span className="text-white font-mono break-all">{compressResult.output_path}</span>
                </div>

                <div className="flex space-x-3 justify-end pt-2">
                  <button
                    type="button"
                    onClick={() => revealItemInDir(compressResult.output_path).catch(() => {})}
                    className="px-4 py-2 bg-[#333333] hover:bg-[#3E3E3E] border border-[#3E3E3E] text-white text-xs font-medium rounded-md transition-colors cursor-pointer"
                  >
                    Show in Folder
                  </button>
                  <button
                    type="button"
                    onClick={onCloseCompressResult}
                    className="px-6 py-2 bg-[#60CDFF] hover:bg-[#4FB8E6] text-black text-xs font-semibold rounded-md transition-colors cursor-pointer"
                  >
                    Done
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="flex items-center space-x-3">
                  <div className="w-10 h-10 rounded-full bg-rose-500/20 text-rose-400 flex items-center justify-center shrink-0 border border-rose-500/30">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-6 h-6">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-white">Compression Failed</h3>
                    <p className="text-xs text-rose-400/90">
                      Archive creation could not be completed.
                    </p>
                  </div>
                </div>

                <div className="bg-[#141416] rounded-lg p-4 border border-[#3E3E3E] space-y-2">
                  <span className="text-[#A8A8A8] text-xs font-medium block">Error Details:</span>
                  <div className="font-mono text-xs text-rose-300 break-all select-text bg-[#0F0F12] p-2.5 rounded border border-[#2A2A30]">
                    {compressResult.error || "An unknown error occurred during compression."}
                  </div>
                </div>

                <div className="flex justify-end pt-2">
                  <button
                    type="button"
                    onClick={onCloseCompressResult}
                    className="px-6 py-2 bg-[#333333] hover:bg-[#3E3E3E] text-white text-xs font-semibold rounded-md transition-colors cursor-pointer"
                  >
                    Close
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* Extraction Result Modal */}
      {extractResult && (
        <div
          onClick={handleExtractBackdrop}
          className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4 select-none animate-in fade-in duration-150"
        >
          <div className="bg-[#202020] border border-[#3E3E3E] rounded-xl p-8 max-w-md w-full shadow-2xl relative space-y-5 animate-in zoom-in-95 duration-150">
            {extractResult.success ? (
              <>
                <div className="flex items-center space-x-3">
                  <div className="w-10 h-10 rounded-full bg-emerald-500/20 text-emerald-400 flex items-center justify-center shrink-0">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-6 h-6">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                    </svg>
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-white">Extraction Complete!</h3>
                    <p className="text-xs text-[#A8A8A8]">
                      Completed in {formatElapsed(extractResult.elapsed_ms)}
                      {extractResult.elapsed_ms > 0 && ((archiveInfo?.total_uncompressed_size ?? 0) > 0 || (archiveInfo?.size ?? 0) > 0)
                        ? ` • ${(
                            ((archiveInfo?.total_uncompressed_size || archiveInfo?.size || 0) / (1024 * 1024)) /
                            (extractResult.elapsed_ms / 1000)
                          ).toFixed(1)} MB/s`
                        : ""}
                    </p>
                  </div>
                </div>

                {/* Archive Info Card */}
                <div className="bg-[#1A2530] border border-[#3E3E3E] rounded-lg p-3.5 flex items-center space-x-3.5">
                  <div className="relative w-10 h-10 rounded-lg bg-[#0F1922] border border-[#60CDFF]/30 flex items-center justify-center shrink-0 shadow">
                    <img
                      src="/bayzip-logo.png"
                      alt="Bayzip Archive"
                      className="w-6 h-6 object-contain"
                      onError={(e) => {
                        (e.currentTarget as HTMLElement).style.display = "none";
                      }}
                    />
                  </div>
                  <div className="min-w-0 flex-1">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-[#60CDFF]">Source Archive</span>
                    <p className="text-xs font-semibold text-white truncate">{archiveInfo?.filename || "Extracted Archive"}</p>
                  </div>
                </div>

                <div className="bg-[#2A2A2A] rounded-lg p-4 border border-[#3E3E3E] space-y-2.5">
                  <div className="flex justify-between text-xs">
                    <span className="text-[#A8A8A8]">Files Extracted:</span>
                    <span className="font-mono text-white font-medium">{extractResult.file_count} item{extractResult.file_count !== 1 ? "s" : ""}</span>
                  </div>
                  <div className="flex justify-between text-xs">
                    <span className="text-[#A8A8A8]">Format:</span>
                    <span className="font-mono text-[#60CDFF]">{archiveInfo?.format || "Archive"}</span>
                  </div>
                </div>

                <div className="text-xs bg-[#1A1A1A] p-3 rounded border border-[#333]">
                  <span className="text-[#7A7A7A] block mb-1">Extracted To:</span>
                  <span className="text-white font-mono break-all">{extractResult.output_directory}</span>
                </div>

                <div className="flex space-x-3 justify-end pt-2">
                  <button
                    type="button"
                    onClick={() => revealItemInDir(extractResult.output_directory).catch(() => {})}
                    className="px-4 py-2 bg-[#333333] hover:bg-[#3E3E3E] border border-[#3E3E3E] text-white text-xs font-medium rounded-md transition-colors cursor-pointer"
                  >
                    Open Folder
                  </button>
                  <button
                    type="button"
                    onClick={onCloseExtractResult}
                    className="px-6 py-2 bg-[#60CDFF] hover:bg-[#4FB8E6] text-black text-xs font-semibold rounded-md transition-colors cursor-pointer"
                  >
                    Done
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="flex items-center space-x-3">
                  <div className="w-10 h-10 rounded-full bg-rose-500/20 text-rose-400 flex items-center justify-center shrink-0 border border-rose-500/30">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-6 h-6">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-white">Extraction Failed</h3>
                    <p className="text-xs text-rose-400/90">
                      Archive extraction could not be completed.
                    </p>
                  </div>
                </div>

                <div className="bg-[#141416] rounded-lg p-4 border border-[#3E3E3E] space-y-2">
                  <span className="text-[#A8A8A8] text-xs font-medium block">Error Details:</span>
                  <div className="font-mono text-xs text-rose-300 break-all select-text bg-[#0F0F12] p-2.5 rounded border border-[#2A2A30]">
                    {extractResult.error || "An unknown error occurred during extraction."}
                  </div>
                </div>

                <div className="flex justify-end pt-2">
                  <button
                    type="button"
                    onClick={onCloseExtractResult}
                    className="px-6 py-2 bg-[#333333] hover:bg-[#3E3E3E] text-white text-xs font-semibold rounded-md transition-colors cursor-pointer"
                  >
                    Close
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </>
  );
}

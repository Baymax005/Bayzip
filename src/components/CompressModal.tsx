import { useMemo } from "react";
import { BayzipLogo } from "./BayzipLogo";
import { SystemSpecs, CompressProgress } from "../types";
import { formatBytes } from "../utils/format";
import { useDynamicThroughput } from "../hooks/useDynamicThroughput";
import { useDraggableModal } from "../hooks/useDraggableModal";

export interface CompressModalProps {
  isOpen: boolean;
  onClose: () => void;
  customArchiveName: string;
  setCustomArchiveName: (name: string) => void;
  selectedFormat: string;
  setSelectedFormat: (fmt: string) => void;
  selectedAlgorithm: string;
  setSelectedAlgorithm: (algo: string) => void;
  selectedEncryption: string;
  setSelectedEncryption: (enc: string) => void;
  compressPassword: string;
  setCompressPassword: (pwd: string) => void;
  showCompressPassword: boolean;
  setShowCompressPassword: (show: boolean) => void;
  archiveToOriginalPath: boolean;
  setArchiveToOriginalPath: (val: boolean) => void;
  separateArchives: boolean;
  setSeparateArchives: (val: boolean) => void;
  smartMediaBypass: boolean;
  setSmartMediaBypass: (val: boolean) => void;
  sliderValue: number;
  setSliderValue: (val: number) => void;
  specs: SystemSpecs | null;
  formatAlgorithms: Record<string, string[]>;
  onStartCompress: () => void;
  isCompressing?: boolean;
  isCancelling?: boolean;
  progress?: CompressProgress | null;
  onCancelCompress?: () => void;
  onMinimizeToFooter?: () => void;
  onDetachToDesktop?: () => void;
}

export function CompressModal({
  isOpen,
  onClose,
  customArchiveName,
  setCustomArchiveName,
  selectedFormat,
  setSelectedFormat,
  selectedAlgorithm,
  setSelectedAlgorithm,
  selectedEncryption,
  setSelectedEncryption,
  compressPassword,
  setCompressPassword,
  showCompressPassword,
  setShowCompressPassword,
  archiveToOriginalPath,
  setArchiveToOriginalPath,
  separateArchives,
  setSeparateArchives,
  smartMediaBypass,
  setSmartMediaBypass,
  sliderValue,
  setSliderValue,
  specs,
  formatAlgorithms,
  onStartCompress,
  isCompressing = false,
  isCancelling = false,
  progress = null,
  onCancelCompress,
  onMinimizeToFooter,
  onDetachToDesktop,
}: CompressModalProps) {
  const {
    position,
    isDragging,
    handleMouseDown,
    resetPosition,
    isDisplaced,
  } = useDraggableModal({ isOpen });
  const availableAlgorithms = useMemo(() => {
    return formatAlgorithms[selectedFormat] || ["Standard"];
  }, [selectedFormat, formatAlgorithms]);

  const totalCores = specs?.cpu_cores || 8;
  const { threads, estimatedRam, presetName } = useMemo(() => {
    let t = 1;
    let ram = 64;
    let name = "Balanced";

    switch (sliderValue) {
      case 0:
        t = 1;
        ram = 32;
        name = "Store";
        break;
      case 25:
        t = Math.max(1, Math.floor(totalCores / 4));
        ram = 64;
        name = "Fast";
        break;
      case 50:
        t = Math.max(1, Math.floor(totalCores / 2));
        ram = 128;
        name = "Balanced";
        break;
      case 75:
        t = Math.max(1, Math.floor((totalCores * 3) / 4));
        ram = 256;
        name = "High";
        break;
      case 100:
        t = totalCores;
        ram = 512;
        name = "Ultra";
        break;
      default:
        t = Math.max(1, Math.floor(totalCores / 2));
        ram = 128;
        name = "Balanced";
    }

    return { threads: t, estimatedRam: ram, presetName: name };
  }, [sliderValue, totalCores]);

  // Live throughput calculations
  const { speedText, etaText } = useDynamicThroughput({
    isActive: isCompressing,
    bytesProcessed: progress?.bytes_processed ?? 0,
    totalBytes: progress?.total_bytes ?? 0,
  });

  const percent = progress?.percent ?? 0;
  const targetArchiveTitle = `${customArchiveName.trim() || "Archive"}${selectedFormat}`;

  if (!isOpen) return null;

  return (
    // Scoped strictly inside File Viewer bounds with responsive draggable window positioning
    <div
      onClick={(e) => {
        if (e.target === e.currentTarget && !isCompressing) {
          onClose();
        }
      }}
      className={`absolute inset-0 z-40 flex items-center justify-center p-4 select-none ${
        isDisplaced ? "bg-black/10 pointer-events-none" : "bg-black/30"
      }`}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          transform: `translate(${position.x}px, ${position.y}px)`,
        }}
        className={`max-w-[440px] w-full bg-[#1C1C1F] rounded-xl border border-[#383842] p-5 shadow-[0_24px_60px_rgba(0,0,0,0.75)] flex flex-col space-y-4 text-xs text-[#F4F4F6] pointer-events-auto transition-shadow ${
          isDragging ? "shadow-[0_25px_70px_rgba(56,189,248,0.25)] cursor-grabbing" : ""
        }`}
      >
        {/* ================================================================= */}
        {/* MODE A: LIVE IN-PLACE STREAMING PROGRESS VIEW (NO PAUSE BUTTON)   */}
        {/* ================================================================= */}
        {isCompressing ? (
          <div className="space-y-4">
            {/* Header with target name & percentage badge (Draggable) */}
            <div
              onMouseDown={handleMouseDown}
              className="flex items-center justify-between pb-2 border-b border-[#2E2E34] cursor-grab active:cursor-grabbing select-none"
              title="Click and drag to move stream window"
            >
              <div className="flex items-center space-x-2 truncate mr-2">
                <span className="w-2 h-2 rounded-full bg-[#38BDF8] animate-pulse shrink-0"></span>
                <span className="font-semibold text-white tracking-wide truncate">
                  Streaming to <span className="text-[#38BDF8]">{targetArchiveTitle}</span>
                </span>
                <span className="text-[9px] text-[#888888] bg-[#252529] px-1.5 py-0.5 rounded border border-[#323238] shrink-0 pointer-events-none">
                  Draggable
                </span>
              </div>
              <div className="flex items-center space-x-2 shrink-0">
                {isDisplaced && (
                  <button
                    type="button"
                    onClick={resetPosition}
                    className="text-[10px] text-[#888888] hover:text-[#38BDF8] px-1.5 py-0.5 rounded hover:bg-[#252529] transition-colors"
                    title="Reset to center"
                  >
                    Center
                  </button>
                )}
                <span className="font-mono text-sm font-bold text-[#38BDF8]">
                  {Math.round(percent)}%
                </span>
              </div>
            </div>

            {/* h-2 smooth progress bar */}
            <div className="w-full h-2 rounded-full bg-[#252529] overflow-hidden">
              <div
                className="h-full bg-[#38BDF8] transition-all duration-300"
                style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
              />
            </div>

            {/* Compact 2x2 Telemetry Grid (tabular-nums font-mono) */}
            <div className="grid grid-cols-2 gap-2.5 bg-[#141416] p-3 rounded-[6px] border border-[#2E2E34] text-[11px] font-mono">
              <div>
                <span className="text-[#9CA3AF] block font-sans text-[10px]">Throughput:</span>
                <span className="text-white font-semibold">{speedText || "Calculating..."}</span>
              </div>
              <div>
                <span className="text-[#9CA3AF] block font-sans text-[10px]">ETA:</span>
                <span className="text-white font-semibold">{etaText || "--:--"}</span>
              </div>
              <div className="col-span-2 truncate">
                <span className="text-[#9CA3AF] block font-sans text-[10px]">Streaming File:</span>
                <span className="text-[#38BDF8] truncate block" title={progress?.current_file || ""}>
                  {progress?.current_file ? progress.current_file.split(/[/\\]/).pop() : "Preparing stream..."}
                </span>
              </div>
              <div className="col-span-2 pt-1 border-t border-[#232328] flex justify-between">
                <span className="text-[#9CA3AF] font-sans text-[10px]">Streamed Bytes:</span>
                <span className="text-white font-semibold">
                  {formatBytes(progress?.bytes_processed || 0)} / {formatBytes(progress?.total_bytes || 0)}
                </span>
              </div>
            </div>

            {/* Footer Actions (Minimize to Footer, Detach to Desktop & Cancel) */}
            <div className="flex items-center justify-between pt-2 border-t border-[#2E2E34]">
              <div className="flex items-center space-x-1.5">
                <button
                  type="button"
                  onClick={() => {
                    if (onMinimizeToFooter) onMinimizeToFooter();
                  }}
                  className="px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] hover:text-[#38BDF8] border border-[#323238] font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1"
                  title="Minimize streaming progress to bottom status bar"
                >
                  <span>▾</span>
                  <span>Minimize to Footer</span>
                </button>

                {onDetachToDesktop && (
                  <button
                    type="button"
                    onClick={onDetachToDesktop}
                    className="px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] hover:text-[#38BDF8] border border-[#323238] font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1"
                    title="Detach progress into a standalone floating desktop window"
                  >
                    <span>⧉</span>
                    <span>Detach</span>
                  </button>
                )}
              </div>

              <button
                type="button"
                onClick={onCancelCompress}
                disabled={isCancelling}
                className={`px-3 py-1.5 rounded-[4px] font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1.5 ${
                  isCancelling
                    ? "bg-rose-950/60 border border-rose-700 text-rose-300 opacity-80 cursor-not-allowed"
                    : "bg-[#252529] hover:bg-rose-950/40 text-[#9CA3AF] hover:text-rose-400 border border-[#323238] hover:border-rose-800"
                }`}
                title="Abort stream and delete partial output"
              >
                {isCancelling ? (
                  <>
                    <svg className="animate-spin w-3 h-3 text-rose-400" viewBox="0 0 24 24" fill="none">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z" />
                    </svg>
                    <span>Cancelling...</span>
                  </>
                ) : (
                  <span>Cancel</span>
                )}
              </button>
            </div>
          </div>
        ) : (
          /* ================================================================= */
          /* MODE B: CONFIGURATION DIALOG                                     */
          /* ================================================================= */
          <>
            {/* 1. Header (Draggable) */}
            <div
              onMouseDown={handleMouseDown}
              className="flex items-center justify-between pb-2 border-b border-[#2E2E34] cursor-grab active:cursor-grabbing select-none"
              title="Click and drag to move window"
            >
              <div className="flex items-center space-x-2">
                <span className="text-sm font-semibold text-white tracking-wide">
                  Compress Archive
                </span>
                <span className="text-[10px] text-[#888888] bg-[#252529] px-1.5 py-0.5 rounded border border-[#323238] pointer-events-none">
                  Draggable
                </span>
              </div>
              <div className="flex items-center space-x-1.5">
                {isDisplaced && (
                  <button
                    type="button"
                    onClick={resetPosition}
                    className="text-[10px] text-[#888888] hover:text-[#38BDF8] px-1.5 py-0.5 rounded hover:bg-[#252529] transition-colors"
                    title="Reset to center"
                  >
                    Center
                  </button>
                )}
                <button
                  type="button"
                  onClick={onClose}
                  className="p-1 text-[#9CA3AF] hover:text-white rounded-[4px] hover:bg-[#252529] transition-colors cursor-pointer"
                  title="Close"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-4 h-4">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </div>

            {/* 2. Archive Name & Format Row */}
            <div className="flex items-center space-x-2">
              <div className="w-8 h-8 rounded-[4px] bg-[#252529] border border-[#323238] flex items-center justify-center shrink-0">
                <BayzipLogo className="w-4 h-4" />
              </div>
              <input
                type="text"
                value={customArchiveName}
                onChange={(e) => setCustomArchiveName(e.target.value)}
                placeholder="Archive name..."
                className="flex-1 bg-[#252529] hover:bg-[#2E2E34] focus:bg-[#252529] border border-[#323238] focus:border-[#38BDF8] rounded-[4px] px-2.5 py-1.5 text-xs text-[#F4F4F6] placeholder-[#6B7280] outline-none font-sans transition-colors"
              />
              <select
                value={selectedFormat}
                onChange={(e) => {
                  const newFmt = e.target.value;
                  setSelectedFormat(newFmt);
                  const algos = formatAlgorithms[newFmt];
                  if (algos && algos.length > 0) {
                    setSelectedAlgorithm(algos[0]);
                  }
                }}
                className="bg-[#252529] hover:bg-[#2E2E34] border border-[#323238] focus:border-[#38BDF8] rounded-[4px] px-2 py-1.5 text-xs text-[#F4F4F6] font-mono outline-none cursor-pointer transition-colors"
              >
                <option value=".bay">.bay (Ultra)</option>
                <option value=".bayg">.bayg (Gamer)</option>
                <option value=".zip">.zip (Deflate)</option>
                <option value=".7z">.7z (LZMA2)</option>
                <option value=".tar">.tar (Store)</option>
              </select>
            </div>

            {/* 3. Algorithm & Encryption Row */}
            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <label className="text-[11px] text-[#9CA3AF] font-medium">Algorithm</label>
                <select
                  value={selectedAlgorithm}
                  onChange={(e) => setSelectedAlgorithm(e.target.value)}
                  className="w-full bg-[#252529] hover:bg-[#2E2E34] border border-[#323238] focus:border-[#38BDF8] rounded-[4px] px-2 py-1.5 text-[11px] text-[#F4F4F6] outline-none cursor-pointer transition-colors truncate"
                >
                  {availableAlgorithms.map((algo) => (
                    <option key={algo} value={algo}>
                      {algo}
                    </option>
                  ))}
                </select>
              </div>

              <div className="space-y-1">
                <label className="text-[11px] text-[#9CA3AF] font-medium">Encryption</label>
                <select
                  value={selectedEncryption}
                  onChange={(e) => setSelectedEncryption(e.target.value)}
                  className="w-full bg-[#252529] hover:bg-[#2E2E34] border border-[#323238] focus:border-[#38BDF8] rounded-[4px] px-2 py-1.5 text-[11px] text-[#F4F4F6] outline-none cursor-pointer transition-colors"
                >
                  <option value="none">None (Fastest)</option>
                  <option value="aes256">AES-256</option>
                </select>
              </div>
            </div>

            {/* Dedicated Clear Password Section when AES-256 is selected */}
            {selectedEncryption === "aes256" && (
              <div className="space-y-1.5 bg-[#141416] p-2.5 rounded-[4px] border border-[#323238]">
                <label className="text-[11px] text-[#9CA3AF] font-medium flex items-center justify-between">
                  <span className="flex items-center space-x-1.5">
                    <span className="text-[#38BDF8]">🔒</span>
                    <span className="text-white font-semibold">Archive Password (AES-256)</span>
                  </span>
                  <span className="text-[10px] text-amber-400 font-mono font-semibold">Required</span>
                </label>
                <div className="relative">
                  <input
                    type={showCompressPassword ? "text" : "password"}
                    value={compressPassword}
                    onChange={(e) => setCompressPassword(e.target.value)}
                    placeholder="Enter encryption password..."
                    className="w-full bg-[#252529] border border-[#323238] focus:border-[#38BDF8] rounded-[4px] px-2.5 py-1.5 pr-8 text-xs text-[#F4F4F6] placeholder-[#6B7280] outline-none font-mono"
                    autoFocus
                  />
                  <button
                    type="button"
                    onClick={() => setShowCompressPassword(!showCompressPassword)}
                    className="absolute right-2 top-2 text-[#9CA3AF] hover:text-white cursor-pointer"
                    title={showCompressPassword ? "Hide password" : "Show password"}
                  >
                    {showCompressPassword ? "👁️" : "🔒"}
                  </button>
                </div>
              </div>
            )}

            {/* 4. Keka-Style Compact Hardware & Speed Slider */}
            <div className="bg-[#141416] border border-[#2E2E34] rounded-[6px] p-3 space-y-2">
              <div className="flex items-center justify-between text-[11px]">
                <span className="font-semibold text-white">
                  Compression & Hardware: <span className="text-[#38BDF8]">{presetName}</span>
                </span>
                <span className="font-mono tabular-nums text-[10px] px-2 py-0.5 rounded-[4px] bg-[#252529] text-[#9CA3AF] border border-[#323238]">
                  {threads}/{totalCores} Threads • ~{estimatedRam} MB RAM
                </span>
              </div>

              <div className="pt-1">
                <input
                  type="range"
                  min={0}
                  max={100}
                  step={25}
                  value={sliderValue}
                  onChange={(e) => setSliderValue(Number(e.target.value))}
                  className="w-full accent-[#38BDF8] cursor-pointer"
                />
                <div className="flex justify-between text-[10px] text-[#9CA3AF] font-sans mt-1 px-0.5">
                  <span className={sliderValue === 0 ? "text-[#38BDF8] font-bold" : ""}>Store</span>
                  <span className={sliderValue === 25 ? "text-[#38BDF8] font-bold" : ""}>Fast</span>
                  <span className={sliderValue === 50 ? "text-[#38BDF8] font-bold" : ""}>Balanced</span>
                  <span className={sliderValue === 75 ? "text-[#38BDF8] font-bold" : ""}>High</span>
                  <span className={sliderValue === 100 ? "text-[#38BDF8] font-bold" : ""}>Ultra</span>
                </div>
              </div>
            </div>

            {/* 5. Compact Toggles Stack */}
            <div className="space-y-2 pt-1 border-t border-[#2E2E34]">
              <label className="flex items-center justify-between text-[11px] text-[#F4F4F6] cursor-pointer">
                <span>Archive to original path</span>
                <input
                  type="checkbox"
                  checked={archiveToOriginalPath}
                  onChange={(e) => setArchiveToOriginalPath(e.target.checked)}
                  className="accent-[#38BDF8] rounded-[2px] w-3.5 h-3.5 cursor-pointer"
                />
              </label>

              <label className="flex items-center justify-between text-[11px] text-[#F4F4F6] cursor-pointer">
                <span>Compress each item into separate archive</span>
                <input
                  type="checkbox"
                  checked={separateArchives}
                  onChange={(e) => setSeparateArchives(e.target.checked)}
                  className="accent-[#38BDF8] rounded-[2px] w-3.5 h-3.5 cursor-pointer"
                />
              </label>

              <label className="flex items-center justify-between text-[11px] text-[#F4F4F6] cursor-pointer">
                <span className="flex items-center space-x-1.5">
                  <span>Smart media bypass (ZIP)</span>
                  <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-[#10B981]/15 text-[#10B981] border border-[#10B981]/30 font-semibold">
                    FAST STORE
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={smartMediaBypass}
                  onChange={(e) => setSmartMediaBypass(e.target.checked)}
                  className="accent-[#38BDF8] rounded-[2px] w-3.5 h-3.5 cursor-pointer"
                />
              </label>
            </div>

            {/* 6. Modal Footer */}
            <div className="flex items-center justify-end space-x-2 pt-2 border-t border-[#2E2E34]">
              <button
                type="button"
                onClick={onClose}
                className="px-3.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] border border-[#323238] font-medium text-xs transition-colors cursor-pointer"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={onStartCompress}
                className="px-4 py-1.5 rounded-[4px] bg-[#38BDF8] hover:bg-[#7DD3FC] text-[#0F172A] font-semibold text-xs shadow-md shadow-sky-500/20 active:scale-95 transition-all cursor-pointer flex items-center space-x-1.5"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} className="w-3.5 h-3.5">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                </svg>
                <span>Compress Now</span>
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
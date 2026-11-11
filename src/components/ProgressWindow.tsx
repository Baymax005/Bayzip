import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CompressProgress, ExtractProgress, TestArchiveProgress } from "../types";
import { formatBytes } from "../utils/format";
import { APP_VERSION } from "../version";

type StreamType = "compress" | "extract" | "test" | "idle";

export function ProgressWindow() {
  const [streamType, setStreamType] = useState<StreamType>("idle");
  const [targetName, setTargetName] = useState<string>("Active Stream");
  const [percent, setPercent] = useState<number>(0);
  const [currentFile, setCurrentFile] = useState<string>("Initializing stream...");
  const [bytesProcessed, setBytesProcessed] = useState<number>(0);
  const [totalBytes, setTotalBytes] = useState<number>(0);
  const [isCancelling, setIsCancelling] = useState<boolean>(false);
  const [alwaysOnTop, setAlwaysOnTop] = useState<boolean>(false);

  // Speed and ETA calculation refs
  const historyRef = useRef<{ timestamp: number; bytes: number }[]>([]);
  const [speedText, setSpeedText] = useState<string>("");
  const [etaText, setEtaText] = useState<string>("");

  // Update dynamic throughput
  useEffect(() => {
    if (streamType === "idle" || totalBytes === 0) return;

    const now = Date.now();
    historyRef.current.push({ timestamp: now, bytes: bytesProcessed });

    // Keep history within last 1500ms
    const windowStart = now - 1500;
    historyRef.current = historyRef.current.filter((item) => item.timestamp >= windowStart);

    if (historyRef.current.length >= 2) {
      const oldest = historyRef.current[0];
      const deltaBytes = bytesProcessed - oldest.bytes;
      const deltaTimeSec = (now - oldest.timestamp) / 1000;

      if (deltaTimeSec > 0.1 && deltaBytes >= 0) {
        const bytesPerSec = deltaBytes / deltaTimeSec;
        setSpeedText(`${formatBytes(bytesPerSec)}/s`);

        const remainingBytes = Math.max(0, totalBytes - bytesProcessed);
        if (bytesPerSec > 1024 && remainingBytes > 0) {
          const remainingSec = Math.round(remainingBytes / bytesPerSec);
          if (remainingSec < 60) {
            setEtaText(`${remainingSec}s`);
          } else if (remainingSec < 3600) {
            const m = Math.floor(remainingSec / 60);
            const s = remainingSec % 60;
            setEtaText(`${m}m ${s}s`);
          } else {
            const h = Math.floor(remainingSec / 3600);
            const m = Math.floor((remainingSec % 3600) / 60);
            setEtaText(`${h}h ${m}m`);
          }
        } else {
          setEtaText("--:--");
        }
      }
    }
  }, [bytesProcessed, totalBytes, streamType]);

  // Setup Tauri event listeners
  useEffect(() => {
    let isMounted = true;
    let unlistenCompress: (() => void) | undefined;
    let unlistenExtract: (() => void) | undefined;
    let unlistenTest: (() => void) | undefined;

    const setupListeners = async () => {
      const uCompress = await listen<CompressProgress>("compress_progress", (event) => {
        if (!isMounted) return;
        setStreamType("compress");
        setPercent(event.payload.percent);
        setCurrentFile(event.payload.current_file);
        setBytesProcessed(event.payload.bytes_processed);
        setTotalBytes(event.payload.total_bytes);
        if (event.payload.current_file) {
          const name = event.payload.current_file.split(/[/\\]/).pop();
          if (name) setTargetName(name);
        }
      });
      if (!isMounted) {
        uCompress();
      } else {
        unlistenCompress = uCompress;
      }

      const uExtract = await listen<ExtractProgress>("extract_progress", (event) => {
        if (!isMounted) return;
        setStreamType("extract");
        setPercent(event.payload.percent);
        setCurrentFile(event.payload.current_file);
        setBytesProcessed(event.payload.bytes_extracted);
        if (event.payload.percent > 0) {
          setTotalBytes(Math.round((event.payload.bytes_extracted / event.payload.percent) * 100));
        }
        if (event.payload.current_file) {
          const name = event.payload.current_file.split(/[/\\]/).pop();
          if (name) setTargetName(name);
        }
      });
      if (!isMounted) {
        uExtract();
      } else {
        unlistenExtract = uExtract;
      }

      const uTest = await listen<TestArchiveProgress>("test_progress", (event) => {
        if (!isMounted) return;
        setStreamType("test");
        setPercent(event.payload.percent);
        setCurrentFile(event.payload.current_file);
        setBytesProcessed(event.payload.bytes_tested);
        setTotalBytes(event.payload.total_bytes);
      });
      if (!isMounted) {
        uTest();
      } else {
        unlistenTest = uTest;
      }
    };

    setupListeners();

    return () => {
      isMounted = false;
      if (unlistenCompress) unlistenCompress();
      if (unlistenExtract) unlistenExtract();
      if (unlistenTest) unlistenTest();
    };
  }, []);

  const handleCancel = async () => {
    setIsCancelling(true);
    try {
      if (streamType === "compress") {
        await invoke("cancel_compression");
      } else if (streamType === "extract") {
        await invoke("cancel_extraction");
      } else if (streamType === "test") {
        await invoke("cancel_test_archive");
      }
    } catch (err) {
      console.error("Failed to cancel active stream:", err);
    }
  };

  const handleTogglePin = async () => {
    const nextVal = !alwaysOnTop;
    setAlwaysOnTop(nextVal);
    try {
      await invoke("set_progress_always_on_top", { alwaysOnTop: nextVal });
    } catch {
      await getCurrentWindow().setAlwaysOnTop(nextVal).catch(() => {});
    }
  };

  const handleRestoreMainApp = async () => {
    try {
      await invoke("focus_main_window");
    } catch (e) {
      console.error("Failed to focus main window:", e);
    }
  };

  const handleMinimize = async () => {
    try {
      await invoke("minimize_progress_window");
    } catch {
      await getCurrentWindow().minimize().catch(() => {});
    }
  };

  const handleClose = async () => {
    try {
      await invoke("close_progress_window");
    } catch {
      await getCurrentWindow().hide().catch(() => {});
    }
  };

  const typeLabel =
    streamType === "compress"
      ? "Streaming to"
      : streamType === "extract"
      ? "Extracting"
      : streamType === "test"
      ? "Verifying"
      : "Active Stream";

  return (
    <div className="h-screen w-screen bg-[#18181A] text-white flex flex-col font-sans select-none overflow-hidden border border-[#383842] rounded-xl shadow-2xl">
      {/* Draggable Titlebar Header */}
      <div className="h-9 w-full flex justify-between items-center bg-[#202024] border-b border-[#2E2E34] cursor-default select-none shrink-0">
        <div data-tauri-drag-region className="flex items-center space-x-2 px-3 h-full cursor-default">
          <span className="w-2 h-2 rounded-full bg-[#38BDF8] animate-pulse shrink-0"></span>
          <span data-tauri-drag-region className="text-xs font-semibold text-white tracking-wide pointer-events-none">
            Bayzip Stream HUD
          </span>
          <span data-tauri-drag-region className="text-[9px] font-mono text-[#888888] bg-[#141416] px-1.5 py-0.2 rounded border border-[#2E2E34] pointer-events-none">
            v{APP_VERSION}
          </span>
        </div>

        {/* Center Draggable Region */}
        <div data-tauri-drag-region className="flex-1 h-full cursor-default" />

        {/* Window Actions */}
        <div className="flex items-center h-full pointer-events-auto z-10">
          <button
            type="button"
            onClick={handleTogglePin}
            className={`h-full px-2.5 flex items-center justify-center transition-colors cursor-pointer ${
              alwaysOnTop
                ? "text-[#38BDF8] bg-[#282830]"
                : "text-[#888888] hover:text-white hover:bg-[#282830]"
            }`}
            title={alwaysOnTop ? "Always on top: Enabled" : "Pin always on top"}
          >
            <svg viewBox="0 0 24 24" fill={alwaysOnTop ? "currentColor" : "none"} stroke="currentColor" strokeWidth={2} className="w-3.5 h-3.5">
              <path strokeLinecap="round" strokeLinejoin="round" d="M19 13.5l-6-6m2.5-3.5L8 11.5l-5 1 4 4 1-5 7.5-7.5zM12 12l-7 7" />
            </svg>
          </button>

          <button
            type="button"
            onClick={handleRestoreMainApp}
            className="h-full px-2.5 text-[#888888] hover:text-[#38BDF8] hover:bg-[#282830] transition-colors cursor-pointer"
            title="Focus Main Bayzip App"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-3.5 h-3.5">
              <path strokeLinecap="round" strokeLinejoin="round" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
            </svg>
          </button>

          <button
            type="button"
            onClick={handleMinimize}
            className="h-full px-2.5 text-[#888888] hover:text-white hover:bg-[#282830] transition-colors cursor-pointer"
            title="Minimize"
          >
            <svg width="10" height="1" viewBox="0 0 10 1" className="fill-current">
              <rect width="10" height="1" />
            </svg>
          </button>

          <button
            type="button"
            onClick={handleClose}
            className="h-full px-3 text-[#888888] hover:text-white hover:bg-rose-900/60 transition-colors cursor-pointer"
            title="Hide Progress HUD"
          >
            <svg width="10" height="10" viewBox="0 0 10 10" className="fill-current">
              <path d="M1 0L0 1l4 4-4 4 1 1 4-4 4 4 1-1-4-4 4-4-1-1-4 4z" />
            </svg>
          </button>
        </div>
      </div>

      {/* Main Body */}
      <div className="flex-1 p-3.5 flex flex-col justify-between text-xs text-[#F4F4F6]">
        {/* Top: Status & Percent */}
        <div>
          <div className="flex items-center justify-between pb-2 border-b border-[#2E2E34]">
            <div className="flex items-center space-x-2 truncate mr-2">
              <span className="font-semibold text-white tracking-wide truncate">
                {typeLabel} <span className="text-[#38BDF8]">{targetName}</span>
              </span>
            </div>
            <span className="font-mono text-sm font-bold text-[#38BDF8] shrink-0">
              {Math.round(percent)}%
            </span>
          </div>

          {/* Progress Bar */}
          <div className="w-full h-2 rounded-full bg-[#252529] overflow-hidden mt-2.5">
            <div
              className="h-full bg-[#38BDF8] transition-all duration-300"
              style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
            />
          </div>
        </div>

        {/* Telemetry Grid */}
        <div className="grid grid-cols-2 gap-2 bg-[#121214] p-2.5 rounded-[6px] border border-[#2E2E34] text-[11px] font-mono">
          <div>
            <span className="text-[#9CA3AF] block font-sans text-[10px]">Throughput:</span>
            <span className="text-white font-semibold">{speedText || "Streaming..."}</span>
          </div>
          <div>
            <span className="text-[#9CA3AF] block font-sans text-[10px]">ETA:</span>
            <span className="text-white font-semibold">{etaText || "--:--"}</span>
          </div>
          <div className="col-span-2 truncate">
            <span className="text-[#9CA3AF] block font-sans text-[10px]">Current File:</span>
            <span className="text-[#38BDF8] truncate block" title={currentFile}>
              {currentFile ? currentFile.split(/[/\\]/).pop() : "Streaming..."}
            </span>
          </div>
          <div className="col-span-2 pt-1 border-t border-[#232328] flex justify-between">
            <span className="text-[#9CA3AF] font-sans text-[10px]">Bytes:</span>
            <span className="text-white font-semibold">
              {formatBytes(bytesProcessed)} / {formatBytes(totalBytes)}
            </span>
          </div>
        </div>

        {/* Footer Actions */}
        <div className="flex items-center justify-between pt-2 border-t border-[#2E2E34]">
          <button
            type="button"
            onClick={handleRestoreMainApp}
            className="px-2.5 py-1 rounded-[4px] bg-[#222226] hover:bg-[#2A2A30] text-[#9CA3AF] hover:text-[#38BDF8] border border-[#323238] font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1"
          >
            <span>⧉</span>
            <span>Open Bayzip</span>
          </button>

          <button
            type="button"
            onClick={handleCancel}
            disabled={isCancelling}
            className={`px-3 py-1 rounded-[4px] border font-medium text-xs transition-colors cursor-pointer flex items-center space-x-1.5 ${
              isCancelling
                ? "bg-rose-950/60 border-rose-700 text-rose-300 opacity-80 cursor-not-allowed"
                : "bg-[#222226] hover:bg-rose-950/40 text-[#9CA3AF] hover:text-rose-400 border-[#323238] hover:border-rose-800"
            }`}
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
              <span>Cancel Stream</span>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

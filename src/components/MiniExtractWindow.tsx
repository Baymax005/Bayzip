import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { BayzipLogo } from "./BayzipLogo";
import { formatBytes } from "../utils/format";
import { getArchiveStemName } from "../utils/archive";
import { ArchiveInfo, ExtractProgress, ExtractResult, StartupCommand } from "../types";
import { APP_VERSION } from "../version";

type MiniExtractState = "inspecting" | "password" | "extracting" | "done" | "error";

export function MiniExtractWindow() {
  const [state, setState] = useState<MiniExtractState>("inspecting");
  const [archivePath, setArchivePath] = useState<string>("");
  const [archiveFilename, setArchiveFilename] = useState<string>("");
  const [destination, setDestination] = useState<string>("");
  const [extractToSubfolder, setExtractToSubfolder] = useState<boolean>(true);
  const [expectedSize, setExpectedSize] = useState<number | undefined>(undefined);

  // Password Prompt State
  const [password, setPassword] = useState<string>("");
  const [showPassword, setShowPassword] = useState<boolean>(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Live Extraction State
  const [percent, setPercent] = useState<number>(0);
  const [currentFile, setCurrentFile] = useState<string>("Initializing extraction...");
  const [bytesExtracted, setBytesExtracted] = useState<number>(0);
  const [totalBytes, setTotalBytes] = useState<number>(0);
  const [isCancelling, setIsCancelling] = useState<boolean>(false);
  const [speedText, setSpeedText] = useState<string>("");
  const [etaText, setEtaText] = useState<string>("");
  const [outputDirectory, setOutputDirectory] = useState<string>("");

  const historyRef = useRef<{ timestamp: number; bytes: number }[]>([]);

  // Calculate dynamic throughput
  useEffect(() => {
    if (state !== "extracting" || totalBytes === 0) return;

    const now = Date.now();
    historyRef.current.push({ timestamp: now, bytes: bytesExtracted });

    const windowStart = now - 1500;
    historyRef.current = historyRef.current.filter((item) => item.timestamp >= windowStart);

    if (historyRef.current.length >= 2) {
      const oldest = historyRef.current[0];
      const deltaBytes = bytesExtracted - oldest.bytes;
      const deltaTimeSec = (now - oldest.timestamp) / 1000;

      if (deltaTimeSec > 0.1 && deltaBytes >= 0) {
        const bytesPerSec = deltaBytes / deltaTimeSec;
        setSpeedText(`${formatBytes(bytesPerSec)}/s`);

        const remainingBytes = Math.max(0, totalBytes - bytesExtracted);
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
  }, [bytesExtracted, totalBytes, state]);

  // Setup progress listener
  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | undefined;

    const setupListener = async () => {
      const u = await listen<ExtractProgress>("extract_progress", (event) => {
        if (!isMounted) return;
        setPercent(event.payload.percent);
        setCurrentFile(event.payload.current_file || "Extracting...");
        setBytesExtracted(event.payload.bytes_extracted);
        if (event.payload.percent > 0) {
          setTotalBytes(Math.round((event.payload.bytes_extracted / event.payload.percent) * 100));
        }
      });
      if (!isMounted) {
        u();
      } else {
        unlisten = u;
      }
    };

    setupListener();

    return () => {
      isMounted = false;
      if (unlisten) unlisten();
    };
  }, []);

  // Initialize from CLI arguments on mount
  useEffect(() => {
    const initFromCli = async () => {
      try {
        const cmd = await invoke<StartupCommand>("get_cli_args");
        if (!cmd.paths || cmd.paths.length === 0) {
          setErrorMessage("No archive file provided to extract.");
          setState("error");
          return;
        }

        const target = cmd.paths[0];
        setArchivePath(target);
        const fname = target.split(/[/\\]/).pop() || "archive";
        setArchiveFilename(fname);

        const parent = target.replace(/[/\\][^/\\]+$/, "") || ".";
        setDestination(parent);

        // Decide subfolder extraction mode:
        // If action is explicit "extract-here", do NOT create subfolder.
        // Otherwise (e.g. "extract-to-folder" or default context menu), extract into subfolder.
        const shouldSubfolder = cmd.action !== "extract-here";
        setExtractToSubfolder(shouldSubfolder);

        // Inspect archive to check encryption & size
        try {
          const info = await invoke<ArchiveInfo>("get_archive_info", { path: target });
          setExpectedSize(info.total_uncompressed_size || info.size || undefined);

          if (info.is_encrypted) {
            setState("password");
          } else {
            // Unencrypted: begin extraction immediately!
            startExtraction(target, parent, shouldSubfolder, undefined, info.total_uncompressed_size || info.size);
          }
        } catch (inspectErr) {
          const errStr = String(inspectErr).toLowerCase();
          if (errStr.includes("password") || errStr.includes("encrypt")) {
            setState("password");
          } else {
            // Attempt extraction anyway; extractor will return definitive result
            startExtraction(target, parent, shouldSubfolder, undefined);
          }
        }
      } catch (err) {
        setErrorMessage(String(err));
        setState("error");
      }
    };

    initFromCli();
  }, []);

  // Auto-focus input when in password mode
  useEffect(() => {
    if (state === "password") {
      setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 60);
    }
  }, [state]);

  // Main extraction routine
  const startExtraction = async (
    targetArchive: string,
    targetDest: string,
    subfolder: boolean,
    pwd?: string,
    size?: number
  ) => {
    setState("extracting");
    setErrorMessage(null);
    setIsCancelling(false);
    setPercent(0);
    setCurrentFile("Preparing extraction...");
    setBytesExtracted(0);

    let finalDest = targetDest;
    if (subfolder) {
      const stem = getArchiveStemName(targetArchive.split(/[/\\]/).pop() || "archive");
      const cleanDest = targetDest.replace(/[/\\]+$/, "");
      finalDest = `${cleanDest}/${stem}`;
    }

    try {
      const res = await invoke<ExtractResult>("extract_archive", {
        archivePath: targetArchive,
        destination: finalDest,
        password: pwd && pwd.trim() ? pwd.trim() : undefined,
        expectedSize: size || expectedSize,
        extractToSubfolder: subfolder,
      });

      setOutputDirectory(res.output_directory);
      setState("done");

      // Reveal output folder in Windows Explorer
      if (res.output_directory) {
        revealItemInDir(res.output_directory).catch(() => {});
      }

      // Automatically exit after a smooth 1s confirmation
      setTimeout(() => {
        handleExit();
      }, 1100);
    } catch (err) {
      console.error("Mini extract error:", err);
      const errStr = String(err);
      if (errStr.toLowerCase().includes("password") || errStr.toLowerCase().includes("decrypt")) {
        setErrorMessage("Incorrect password. Please try again.");
        setPassword("");
        setState("password");
      } else {
        setErrorMessage(errStr);
        setState("error");
      }
    }
  };

  const handlePasswordSubmit = () => {
    if (!password.trim() || state === "extracting") return;
    startExtraction(archivePath, destination, extractToSubfolder, password);
  };

  const handleCancel = async () => {
    if (state === "extracting") {
      setIsCancelling(true);
      try {
        await invoke("cancel_extraction");
      } catch (e) {
        console.error("Cancel extraction error:", e);
      }
    }
    handleExit();
  };

  const handleExit = async () => {
    try {
      await invoke("exit_app");
    } catch {
      await getCurrentWindow().close().catch(() => {});
    }
  };

  const handleMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (e) {
      console.error("Minimize error:", e);
    }
  };

  return (
    <div className="h-screen w-screen bg-[#18181A] text-white flex flex-col font-sans select-none overflow-hidden border border-[#383842] rounded-xl shadow-2xl">
      {/* Titlebar Header */}
      <div className="h-8 w-full flex justify-between items-center bg-[#141416] border-b border-[#2E2E34] cursor-default select-none shrink-0 relative z-50">
        {/* Left Branding */}
        <div data-tauri-drag-region className="flex items-center space-x-2 px-3 h-full cursor-default">
          <div data-tauri-drag-region className="w-3.5 h-3.5 flex items-center justify-center pointer-events-none">
            <BayzipLogo className="w-3.5 h-3.5 rounded-sm pointer-events-none" />
          </div>
          <span data-tauri-drag-region className="text-xs font-semibold text-[#F4F4F6] tracking-wide pointer-events-none">
            Bayzip Extract
          </span>
          <span data-tauri-drag-region className="text-[9px] font-mono text-[#888888] bg-[#202024] px-1.5 py-0.2 rounded border border-[#2E2E34] pointer-events-none">
            v{APP_VERSION}
          </span>
        </div>

        {/* Center Draggable Surface */}
        <div data-tauri-drag-region className="flex-1 h-full cursor-default" />

        {/* Controls */}
        <div className="flex items-center h-full pointer-events-auto">
          <button
            type="button"
            onClick={handleMinimize}
            className="h-full px-3 flex items-center justify-center text-[#9CA3AF] hover:bg-[#2A2A30] hover:text-white transition-colors cursor-pointer"
            title="Minimize"
          >
            <svg className="w-3.5 h-3.5" viewBox="0 0 12 12" fill="currentColor">
              <rect y="5" width="12" height="1.5" rx="0.5" />
            </svg>
          </button>
          <button
            type="button"
            onClick={handleCancel}
            className="h-full px-3 flex items-center justify-center text-[#9CA3AF] hover:bg-[#E81123] hover:text-white transition-colors cursor-pointer"
            title="Close"
          >
            <svg className="w-3.5 h-3.5" viewBox="0 0 12 12" fill="currentColor">
              <path d="M1.5 1.5l9 9m0-9l-9 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </button>
        </div>
      </div>

      {/* Main Body */}
      <div className="flex-1 p-5 flex flex-col justify-center">
        {/* STATE 1: INSPECTING */}
        {state === "inspecting" && (
          <div className="flex flex-col items-center justify-center space-y-3 py-4">
            <svg className="animate-spin h-6 w-6 text-[#38BDF8]" fill="none" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            <div className="text-xs text-[#9CA3AF]">Analyzing archive container...</div>
          </div>
        )}

        {/* STATE 2: PASSWORD PROMPT */}
        {state === "password" && (
          <div className="space-y-3">
            <div className="flex items-center space-x-2.5">
              <div className="w-8 h-8 rounded-lg bg-[#38BDF8]/15 border border-[#38BDF8]/30 flex items-center justify-center shrink-0">
                <svg className="w-4 h-4 text-[#38BDF8]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                </svg>
              </div>
              <div className="truncate">
                <div className="flex items-center space-x-2">
                  <h3 className="text-xs font-semibold text-white tracking-wide">Password Required</h3>
                  {archiveFilename && (
                    <span className="text-[10px] text-[#38BDF8] font-mono bg-[#161618] px-1.5 py-0.5 rounded border border-[#2E2E34] truncate max-w-[180px]">
                      {archiveFilename}
                    </span>
                  )}
                </div>
                <p className="text-[11px] text-[#9CA3AF]">Enter password to unlock and extract this archive.</p>
              </div>
            </div>

            <div>
              <div className="relative">
                <input
                  ref={inputRef}
                  type={showPassword ? "text" : "password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && password.trim()) {
                      e.preventDefault();
                      handlePasswordSubmit();
                    } else if (e.key === "Escape") {
                      handleCancel();
                    }
                  }}
                  placeholder="Enter archive password..."
                  className="w-full bg-[#141416] text-white text-xs pl-3 pr-10 py-2.5 rounded-lg border border-[#383842] focus:border-[#38BDF8] outline-none font-mono placeholder-[#666] transition-colors"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword(!showPassword)}
                  className="absolute inset-y-0 right-0 pr-3 flex items-center text-[#888] hover:text-white cursor-pointer"
                  title={showPassword ? "Hide password" : "Show password"}
                >
                  {showPassword ? (
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l18 18" />
                    </svg>
                  ) : (
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                    </svg>
                  )}
                </button>
              </div>
            </div>

            {errorMessage && (
              <div className="text-[11px] text-rose-400 bg-rose-950/40 px-2.5 py-1.5 rounded-lg border border-rose-900/50 flex items-center space-x-1.5">
                <svg className="w-3.5 h-3.5 text-rose-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                </svg>
                <span className="truncate">{errorMessage}</span>
              </div>
            )}

            <div className="flex justify-end space-x-2 pt-1">
              <button
                type="button"
                onClick={handleCancel}
                className="px-3.5 py-1.5 bg-[#2A2A30] hover:bg-[#34343C] text-[#D1D5DB] text-xs font-medium rounded-lg border border-[#383842] transition-colors cursor-pointer"
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={!password.trim()}
                onClick={handlePasswordSubmit}
                className="px-4 py-1.5 bg-[#38BDF8] hover:bg-[#0284C7] disabled:opacity-50 disabled:cursor-not-allowed text-black text-xs font-semibold rounded-lg transition-colors cursor-pointer flex items-center space-x-1.5 shadow-md shadow-sky-500/10"
              >
                <span>Decrypt & Extract</span>
              </button>
            </div>
          </div>
        )}

        {/* STATE 3: EXTRACTING LIVE HUD */}
        {state === "extracting" && (
          <div className="space-y-3.5">
            {/* Header: Title & Percentage */}
            <div className="flex items-center justify-between">
              <div className="flex items-center space-x-2 truncate mr-2">
                <span className="w-2 h-2 rounded-full bg-[#38BDF8] animate-pulse shrink-0"></span>
                <span className="font-semibold text-xs text-white tracking-wide truncate">
                  Extracting <span className="text-[#38BDF8]">{archiveFilename}</span>
                </span>
              </div>
              <div className="text-xs font-bold font-mono text-[#38BDF8] shrink-0">
                {percent}%
              </div>
            </div>

            {/* Glowing Smooth Progress Bar */}
            <div className="w-full bg-[#242428] rounded-full h-2 overflow-hidden border border-[#383842]/60 p-0.5">
              <div
                className="bg-gradient-to-r from-[#0284C7] to-[#38BDF8] h-full rounded-full transition-all duration-150 ease-out shadow-[0_0_12px_rgba(56,189,248,0.5)]"
                style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
              />
            </div>

            {/* Telemetry Stats Grid */}
            <div className="bg-[#141416] p-2.5 rounded-lg border border-[#2E2E34] space-y-1 text-[11px]">
              <div className="flex justify-between items-center text-[#9CA3AF]">
                <span className="truncate pr-2">File:</span>
                <span className="font-mono text-white truncate max-w-[280px]" title={currentFile}>
                  {currentFile}
                </span>
              </div>
              <div className="flex justify-between items-center text-[#9CA3AF]">
                <span>Speed: <span className="text-white font-mono">{speedText || "Streaming..."}</span></span>
                <span>ETA: <span className="text-white font-mono">{etaText || "--:--"}</span></span>
                <span>Bytes: <span className="text-white font-mono">{formatBytes(bytesExtracted)}</span></span>
              </div>
            </div>

            {/* Action Footer */}
            <div className="flex justify-end pt-0.5">
              <button
                type="button"
                disabled={isCancelling}
                onClick={handleCancel}
                className="px-3.5 py-1.5 bg-[#2A2A30] hover:bg-[#34343C] text-[#D1D5DB] text-xs font-medium rounded-lg border border-[#383842] transition-colors cursor-pointer flex items-center space-x-1.5"
              >
                {isCancelling ? (
                  <>
                    <svg className="animate-spin h-3.5 w-3.5 text-[#38BDF8]" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                    <span>Cancelling...</span>
                  </>
                ) : (
                  <span>Cancel</span>
                )}
              </button>
            </div>
          </div>
        )}

        {/* STATE 4: DONE */}
        {state === "done" && (
          <div className="flex flex-col items-center justify-center space-y-2.5 py-2">
            <div className="w-10 h-10 rounded-full bg-emerald-500/15 border border-emerald-500/30 flex items-center justify-center text-emerald-400">
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
              </svg>
            </div>
            <div className="text-center">
              <h3 className="text-xs font-semibold text-white">Extraction Complete</h3>
              <p className="text-[11px] text-[#9CA3AF] truncate max-w-sm mt-0.5" title={outputDirectory}>
                {outputDirectory || destination}
              </p>
            </div>
            <div className="text-[10px] text-emerald-400/80 animate-pulse">
              Opening folder in Explorer...
            </div>
          </div>
        )}

        {/* STATE 5: FATAL ERROR */}
        {state === "error" && (
          <div className="space-y-3 py-1">
            <div className="flex items-center space-x-2.5 text-rose-400">
              <svg className="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              <h3 className="text-xs font-semibold text-white">Extraction Failed</h3>
            </div>
            <div className="text-xs text-rose-300 bg-rose-950/40 p-2.5 rounded-lg border border-rose-900/50 break-all max-h-24 overflow-y-auto">
              {errorMessage || "An unexpected error occurred during extraction."}
            </div>
            <div className="flex justify-end">
              <button
                type="button"
                onClick={handleExit}
                className="px-4 py-1.5 bg-[#2A2A30] hover:bg-[#34343C] text-white text-xs font-medium rounded-lg border border-[#383842] transition-colors cursor-pointer"
              >
                Close
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

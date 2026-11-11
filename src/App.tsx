import { formatBytes } from "./utils/format";
import { getArchiveStemName, isArchiveFile } from "./utils/archive";
import { useState, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

import { Titlebar } from "./components/Titlebar";
import { AboutModal } from "./components/AboutModal";
import { SettingsModal } from "./components/SettingsModal";
import { PasswordModal } from "./components/PasswordModal";
import { ResultModals } from "./components/ResultModals";
import { CompressModal } from "./components/CompressModal";
import { ExtractModal } from "./components/ExtractModal";
import { UnifiedDropHub } from "./components/UnifiedDropHub";
import { ArchiveTreeInspector, normalizeArchiveEntryPath, StreamProgressTelemetry } from "./components/ArchiveTreeInspector";
import { FilePreviewModal } from "./components/FilePreviewModal";
import { AbortConfirmationModal } from "./components/AbortConfirmationModal";
import { ErrorModal } from "./components/ErrorModal";

import {
  SystemSpecs,
  CompressProgress,
  CompressResult,
  SelectedItemInfo,
  ExtractProgress,
  ExtractResult,
  ArchiveInfo,
  ArchiveEntryItem,
  ArchiveEntryPreview,
  StartupCommand,
  TestArchiveProgress,
  TestArchiveResponse,
} from "./types";

export const FORMAT_ALGORITHMS: Record<string, string[]> = {
  ".bay": ["Zstandard (Recommended)", "Brotli", "LZ4 (Fastest)"],
  ".bayg": ["Zstd-Fast (Auto)"],
  ".7z": ["LZMA2", "LZMA", "BZip2"],
  ".zip": ["Deflate (Standard)", "None (Store)", "BZip2 (Requires 3rd-party App)"],
  ".tar": ["None (Store)"],
};

export default function App() {
  // System Hardware Specs
  const [specs, setSpecs] = useState<SystemSpecs | null>(null);
  const [sliderValue, setSliderValue] = useState<number>(50); // 0, 25, 50, 75, 100

  // Compress Options & Naming
  const [customArchiveName, setCustomArchiveName] = useState<string>("");
  const [selectedFormat, setSelectedFormat] = useState(".bay");
  const [selectedAlgorithm, setSelectedAlgorithm] = useState(FORMAT_ALGORITHMS[".bay"][0]);
  const [selectedEncryption, setSelectedEncryption] = useState("none");
  const [compressPassword, setCompressPassword] = useState<string>("");
  const [showCompressPassword, setShowCompressPassword] = useState<boolean>(false);
  const [archiveToOriginalPath, setArchiveToOriginalPath] = useState(true);
  const [compressDestDir] = useState<string>("");
  const [separateArchives, setSeparateArchives] = useState(false);
  const [smartMediaBypass, setSmartMediaBypass] = useState(true);
  const [selectedFiles, setSelectedFiles] = useState<string[]>([]);
  const [itemsInfo, setItemsInfo] = useState<Record<string, { size: number; is_dir: boolean }>>({});
  const [stagedEntries, setStagedEntries] = useState<ArchiveEntryItem[]>([]);
  const [isInspectingStaged, setIsInspectingStaged] = useState<boolean>(false);
  const [excludedCompressPaths, setExcludedCompressPaths] = useState<Set<string>>(new Set());
  const stagedCacheRef = useRef<Map<string, { info: SelectedItemInfo; entries: ArchiveEntryItem[] }>>(new Map());

  // Dialog & Modal Visibilities
  const [isCompressModalOpen, setIsCompressModalOpen] = useState(false);
  const [isExtractModalOpen, setIsExtractModalOpen] = useState(false);
  const [isAboutOpen, setIsAboutOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [contextMenuStatus, setContextMenuStatus] = useState<string | null>(null);
  const [isDraggingOver, setIsDraggingOver] = useState(false);

  // Compression Execution State
  const [isCompressing, setIsCompressing] = useState(false);
  const [isCancellingCompress, setIsCancellingCompress] = useState(false);
  const [compressProgress, setCompressProgress] = useState<CompressProgress | null>(null);
  const [isProgressMinimized, setIsProgressMinimized] = useState(false);
  const [compressResult, setCompressResult] = useState<CompressResult | null>(null);
  const [globalError, setGlobalError] = useState<{
    title: string;
    message: string;
    details?: string;
  } | null>(null);
  const [, setActiveCompressConfig] = useState<{
    format: string;
    algorithm: string;
    sliderValue: number;
    archiveName: string;
  } | null>(null);

  // Extraction State & Password
  const [selectedArchive, setSelectedArchive] = useState<string | null>(null);
  const [archiveInfo, setArchiveInfo] = useState<ArchiveInfo | null>(null);
  const [isInspectingArchive, setIsInspectingArchive] = useState<boolean>(false);
  const [extractDest, setExtractDest] = useState<string>("");
  const [extractToSubfolder, setExtractToSubfolder] = useState<boolean>(true);
  const [openFolderOnExtract, setOpenFolderOnExtract] = useState<boolean>(true);
  const [extractPassword, setExtractPassword] = useState<string>("");
  const [showExtractPassword, setShowExtractPassword] = useState<boolean>(false);
  const [isPasswordPromptOpen, setIsPasswordPromptOpen] = useState<boolean>(false);
  const [isExtracting, setIsExtracting] = useState<boolean>(false);
  const [isCancellingExtract, setIsCancellingExtract] = useState<boolean>(false);
  const [extractProgress, setExtractProgress] = useState<ExtractProgress | null>(null);
  const [extractResult, setExtractResult] = useState<ExtractResult | null>(null);
  const [extractError, setExtractError] = useState<string | null>(null);
  const [excludedPaths, setExcludedPaths] = useState<Set<string>>(new Set());

  // In-Place Archive Testing State
  const [testStatus, setTestStatus] = useState<"idle" | "testing" | "verified" | "error">("idle");
  const [testMessage, setTestMessage] = useState<string | null>(null);
  const [testProgress, setTestProgress] = useState<TestArchiveProgress | null>(null);
  const isTestingRef = useRef(false);

  // Close Confirmation & Detached HUD State
  const [isConfirmCloseOpen, setIsConfirmCloseOpen] = useState(false);
  const [isHudDetached, setIsHudDetached] = useState(false);
  const isCompressingRef = useRef(false);
  isCompressingRef.current = isCompressing;
  const isExtractingRef = useRef(false);
  isExtractingRef.current = isExtracting;
  const testStatusRef = useRef(testStatus);
  testStatusRef.current = testStatus;

  // Safe In-Memory File Preview State
  const [isPreviewOpen, setIsPreviewOpen] = useState(false);
  const [previewData, setPreviewData] = useState<ArchiveEntryPreview | null>(null);
  const [isPreviewLoading, setIsPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewTargetDisplayPath, setPreviewTargetDisplayPath] = useState<string>("");

  // Global modal and in-flight operation defense flag
  const isAnyModalOpen =
    isCompressModalOpen ||
    isExtractModalOpen ||
    isAboutOpen ||
    isSettingsOpen ||
    isPasswordPromptOpen ||
    Boolean(globalError) ||
    Boolean(compressResult) ||
    Boolean(extractResult) ||
    isPreviewOpen ||
    isConfirmCloseOpen ||
    isCompressing ||
    isExtracting ||
    testStatus === "testing";

  // Registry Sync State
  const [isSyncingRegistry, setIsSyncingRegistry] = useState(false);

  // Calculations for Staged Files
  const totalSelectedSize = useMemo(() => {
    return selectedFiles.reduce((acc, f) => acc + (itemsInfo[f]?.size || 0), 0);
  }, [selectedFiles, itemsInfo]);

  const effectiveTotalSize = useMemo(() => {
    if (stagedEntries.length === 0) return totalSelectedSize;
    if (excludedCompressPaths.size === 0) {
      return stagedEntries.reduce((acc, e) => acc + (e.is_dir ? 0 : e.size), 0);
    }
    const excludedArray = Array.from(excludedCompressPaths);
    return stagedEntries.reduce((acc, e) => {
      if (e.is_dir) return acc;
      const norm = normalizeArchiveEntryPath(e.name);
      const isExcluded = excludedArray.some((ex) => {
        if (norm === ex) return true;
        const prefix = ex.endsWith("/") ? ex : ex + "/";
        return norm.startsWith(prefix);
      });
      return isExcluded ? acc : acc + e.size;
    }, 0);
  }, [stagedEntries, excludedCompressPaths, totalSelectedSize]);

  const totalStagedFilesCount = useMemo(() => {
    return stagedEntries.filter((e) => !e.is_dir).length;
  }, [stagedEntries]);

  const stagedArchiveInfo = useMemo<ArchiveInfo>(() => {
    const targetName =
      customArchiveName.trim() ||
      (selectedFiles.length === 1 ? selectedFiles[0].split(/[/\\]/).pop() || "Archive" : "Archive");
    return {
      path: selectedFiles[0] || "",
      filename: targetName,
      size: effectiveTotalSize,
      format: selectedFormat.toUpperCase(),
      is_archive: false,
      is_encrypted: false,
      file_count: totalStagedFilesCount,
      total_uncompressed_size: effectiveTotalSize,
      entries: stagedEntries,
    };
  }, [selectedFiles, customArchiveName, effectiveTotalSize, selectedFormat, stagedEntries, totalStagedFilesCount]);

  // Combined Active Archive Info for File Viewer
  const currentArchiveInfo = useMemo<ArchiveInfo>(() => {
    if (selectedArchive && archiveInfo) {
      return archiveInfo;
    }
    if (selectedFiles.length > 0) {
      return stagedArchiveInfo;
    }
    return {
      path: "",
      filename: "",
      size: 0,
      format: "",
      is_archive: false,
      is_encrypted: false,
      file_count: 0,
      total_uncompressed_size: 0,
      entries: [],
    };
  }, [selectedArchive, archiveInfo, selectedFiles.length, stagedArchiveInfo]);

  // Cancellation Handlers
  const handleCancelCompress = async () => {
    try {
      setIsCancellingCompress(true);
      await invoke("cancel_compression");
    } catch (e) {
      console.error("Failed to cancel compression:", e);
    }
  };

  const handleCancelExtract = async () => {
    try {
      setIsCancellingExtract(true);
      await invoke("cancel_extraction");
    } catch (e) {
      console.error("Failed to cancel extraction:", e);
    }
  };

  const handleCancelTest = async () => {
    try {
      await invoke("cancel_test_archive");
      setTestStatus("idle");
      setTestMessage(null);
      setTestProgress(null);
    } catch (e) {
      console.error("Failed to cancel archive test:", e);
    }
  };

  const handleDetachProgress = async () => {
    try {
      await invoke("open_progress_window");
      setIsProgressMinimized(true);
      setIsHudDetached(true);
    } catch (e) {
      console.error("Failed to open desktop progress window:", e);
    }
  };

  // Close Guard: intercept window close when an operation is actively running
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onCloseRequested(async (event) => {
        if (isCompressingRef.current || isExtractingRef.current || testStatusRef.current === "testing") {
          event.preventDefault();
          setIsConfirmCloseOpen(true);
        }
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.error("Error setting onCloseRequested listener:", err));

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleRequestClose = () => {
    if (isCompressing || isExtracting || testStatus === "testing") {
      setIsConfirmCloseOpen(true);
    } else {
      getCurrentWindow().close().catch(() => {});
    }
  };

  const handleConfirmAbortAndExit = async () => {
    try {
      if (isCompressing) {
        await invoke("cancel_compression").catch(() => {});
      }
      if (isExtracting) {
        await invoke("cancel_extraction").catch(() => {});
      }
      if (testStatus === "testing") {
        await invoke("cancel_test_archive").catch(() => {});
      }
      await invoke("close_progress_window").catch(() => {});
      // Short delay ensuring OS unlinks partial files before termination
      await new Promise((r) => setTimeout(r, 200));
      await invoke("exit_app").catch(() => {
        getCurrentWindow().destroy().catch(() => {});
      });
    } catch (e) {
      console.error("Failed to cleanly abort and exit:", e);
      await invoke("exit_app").catch(() => {
        getCurrentWindow().destroy().catch(() => {});
      });
    }
  };

  const handleMinimizeToBackground = () => {
    setIsConfirmCloseOpen(false);
    getCurrentWindow().minimize().catch(() => {});
  };

  // Auto-pop out detached progress window if Bayzip is minimized while stream is running
  useEffect(() => {
    if (!isCompressing && !isExtracting) return;
    const handleVisibilityChange = () => {
      if (document.hidden && (isCompressing || isExtracting)) {
        invoke("open_progress_window").catch(() => {});
      }
    };
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => document.removeEventListener("visibilitychange", handleVisibilityChange);
  }, [isCompressing, isExtracting]);

  // Active Stream Telemetry for Bottom Status Bar
  const activeStreamTelemetry = useMemo<StreamProgressTelemetry | null>(() => {
    if (testStatus === "testing") {
      const fileName = testProgress?.current_file
        ? testProgress.current_file.split(/[/\\]/).pop() || "Verifying..."
        : "Verifying container...";
      const throughputText = testProgress?.total_bytes
        ? `${formatBytes(testProgress.bytes_tested)} / ${formatBytes(testProgress.total_bytes)} (CRC32)`
        : `${formatBytes(testProgress?.bytes_tested || 0)} (CRC32)`;
      return {
        type: "test",
        fileName,
        percent: testProgress?.percent ?? 0,
        throughputText,
        onCancel: handleCancelTest,
      };
    }
    if (isCompressing && isProgressMinimized) {
      const fileName = compressProgress?.current_file
        ? compressProgress.current_file.split(/[/\\]/).pop() || "Streaming..."
        : `${customArchiveName.trim() || "Archive"}${selectedFormat}`;
      return {
        type: "compress",
        fileName,
        percent: compressProgress?.percent ?? 0,
        throughputText: `${formatBytes(compressProgress?.bytes_processed || 0)} / ${formatBytes(compressProgress?.total_bytes || 0)}`,
        isCancelling: isCancellingCompress,
        onCancel: handleCancelCompress,
        onRestoreModal: () => setIsProgressMinimized(false),
        onDetachToDesktop: handleDetachProgress,
      };
    }
    if (isExtracting && isProgressMinimized) {
      const fileName = extractProgress?.current_file
        ? extractProgress.current_file.split(/[/\\]/).pop() || "Extracting..."
        : archiveInfo?.filename || "Archive";
      const totalUncompressed = archiveInfo?.total_uncompressed_size || archiveInfo?.size || 0;
      return {
        type: "extract",
        fileName,
        percent: extractProgress?.percent ?? 0,
        throughputText: `${formatBytes(extractProgress?.bytes_extracted || 0)} / ${formatBytes(totalUncompressed)}`,
        isCancelling: isCancellingExtract,
        onCancel: handleCancelExtract,
        onRestoreModal: () => setIsProgressMinimized(false),
        onDetachToDesktop: handleDetachProgress,
      };
    }
    return null;
  }, [
    testStatus,
    testProgress,
    isCompressing,
    isExtracting,
    isProgressMinimized,
    isCancellingCompress,
    isCancellingExtract,
    compressProgress,
    extractProgress,
    customArchiveName,
    selectedFormat,
    archiveInfo,
  ]);

  // Exclusions Management
  const handleToggleExclude = (rawPath: string, isDir?: boolean) => {
    const norm = normalizeArchiveEntryPath(rawPath).replace(/\/+$/, "");
    const prefix = norm + "/";

    setExcludedPaths((prev) => {
      const next = new Set(prev);
      const allEntries = archiveInfo?.entries || [];
      const isCurrentlyExcluded = next.has(norm);

      if (isCurrentlyExcluded) {
        next.delete(norm);
        next.delete(norm + "/");
        if (isDir) {
          for (const exc of Array.from(next)) {
            const excClean = exc.replace(/\/+$/, "");
            if (excClean === norm || excClean.startsWith(prefix)) {
              next.delete(exc);
            }
          }
        }
      } else {
        next.add(norm);
        if (isDir) {
          for (const entry of allEntries) {
            const entryNorm = normalizeArchiveEntryPath(entry.name).replace(/\/+$/, "");
            if (entryNorm.startsWith(prefix) || entryNorm === norm) {
              next.add(entryNorm);
            }
          }
        }
      }
      return next;
    });
  };

  const handleIncludeAll = () => {
    setExcludedPaths(new Set());
  };

  const handleToggleCompressExclude = (rawPath: string, isDir?: boolean) => {
    const norm = normalizeArchiveEntryPath(rawPath).replace(/\/+$/, "");
    const prefix = norm + "/";

    setExcludedCompressPaths((prev) => {
      const next = new Set(prev);
      const isCurrentlyExcluded = next.has(norm);

      if (isCurrentlyExcluded) {
        next.delete(norm);
        next.delete(norm + "/");
        if (isDir) {
          for (const exc of Array.from(next)) {
            const excClean = exc.replace(/\/+$/, "");
            if (excClean === norm || excClean.startsWith(prefix)) {
              next.delete(exc);
            }
          }
        }
      } else {
        next.add(norm);
        if (isDir) {
          for (const entry of stagedEntries) {
            const entryNorm = normalizeArchiveEntryPath(entry.name).replace(/\/+$/, "");
            if (entryNorm.startsWith(prefix) || entryNorm === norm) {
              next.add(entryNorm);
            }
          }
        }
      }
      return next;
    });
  };

  const handleCompressIncludeAll = () => {
    setExcludedCompressPaths(new Set());
  };

  // Preview Entry
  const handlePreviewEntry = async (entryName: string) => {
    const isDiskFile = /^[a-zA-Z]:[/\\]/.test(entryName) || !selectedArchive;
    setIsPreviewOpen(true);
    setIsPreviewLoading(true);
    setPreviewError(null);
    setPreviewData(null);

    if (isDiskFile) {
      setPreviewTargetDisplayPath(entryName);
      try {
        const res = await invoke<ArchiveEntryPreview>("preview_staged_file", { path: entryName });
        setPreviewData(res);
      } catch (err: unknown) {
        setPreviewError(typeof err === "string" ? err : String(err));
      } finally {
        setIsPreviewLoading(false);
      }
      return;
    }

    const archiveBase = selectedArchive.split(/[/\\]/).pop() || selectedArchive;
    setPreviewTargetDisplayPath(`${archiveBase} ➔ ${entryName}`);

    try {
      const res = await invoke<ArchiveEntryPreview>("preview_archive_entry", {
        archivePath: selectedArchive,
        entryName,
        password: extractPassword.trim() ? extractPassword.trim() : null,
      });
      setPreviewData(res);
    } catch (err: unknown) {
      setPreviewError(typeof err === "string" ? err : String(err));
    } finally {
      setIsPreviewLoading(false);
    }
  };

  // Staged Items Inspection Effect
  useEffect(() => {
    if (selectedFiles.length === 0) {
      stagedCacheRef.current.clear();
      setItemsInfo({});
      setStagedEntries([]);
      setExcludedCompressPaths(new Set());
      setIsInspectingStaged(false);
      if (!isCompressing) setCustomArchiveName("");
      return;
    }

    let isCurrent = true;
    const currentCachedPaths = Array.from(stagedCacheRef.current.keys());
    const pathsRemoved = currentCachedPaths.filter((p) => !selectedFiles.includes(p));
    const pathsToAdd = selectedFiles.filter((p) => !stagedCacheRef.current.has(p));

    if (pathsRemoved.length > 0) {
      pathsRemoved.forEach((p) => stagedCacheRef.current.delete(p));
    }

    if (pathsToAdd.length === 0) {
      const newItemsInfo: Record<string, { size: number; is_dir: boolean }> = {};
      const newStagedEntries: ArchiveEntryItem[] = [];
      for (const [p, data] of stagedCacheRef.current.entries()) {
        newItemsInfo[p] = { size: data.info.size, is_dir: data.info.is_dir };
        newStagedEntries.push(...data.entries);
      }
      setItemsInfo(newItemsInfo);
      setStagedEntries(newStagedEntries);
      setIsInspectingStaged(false);
      return;
    }

    setIsInspectingStaged(true);
    Promise.all([
      invoke<SelectedItemInfo[]>("get_selected_items_info", { paths: pathsToAdd }),
      invoke<ArchiveEntryItem[]>("inspect_staged_items", { paths: pathsToAdd }),
    ])
      .then(([infos, newEntries]) => {
        if (!isCurrent) return;
        infos.forEach((info) => {
          const baseName = info.name;
          const matchingEntries = newEntries.filter((e) => {
            const clean = e.name.replace(/\\/g, "/").replace(/^\/+/, "");
            return clean === baseName || clean === `${baseName}/` || clean.startsWith(`${baseName}/`);
          });
          stagedCacheRef.current.set(info.path, {
            info,
            entries: matchingEntries.length > 0 ? matchingEntries : [{ name: info.name, size: info.size, is_dir: info.is_dir }],
          });
        });

        const aggregatedItemsInfo: Record<string, { size: number; is_dir: boolean }> = {};
        const aggregatedEntries: ArchiveEntryItem[] = [];
        for (const [p, data] of stagedCacheRef.current.entries()) {
          aggregatedItemsInfo[p] = { size: data.info.size, is_dir: data.info.is_dir };
          aggregatedEntries.push(...data.entries);
        }
        setItemsInfo(aggregatedItemsInfo);
        setStagedEntries(aggregatedEntries);
      })
      .catch((err) => {
        if (isCurrent) console.error("Failed to inspect added staged items:", err);
      })
      .finally(() => {
        if (isCurrent) setIsInspectingStaged(false);
      });

    return () => {
      isCurrent = false;
    };
  }, [selectedFiles, isCompressing]);

  // Auto-fill archive name from first file stem
  useEffect(() => {
    if (selectedFiles.length > 0 && !customArchiveName) {
      const first = selectedFiles[0];
      const filename = first.split(/[/\\]/).pop() || "bayzip_archive";
      const stem = getArchiveStemName(filename);
      setCustomArchiveName(stem);
    }
  }, [selectedFiles, customArchiveName]);

  // Select Archive Handler
  const selectArchive = async (filePath: string, pwd?: string) => {
    if (!isArchiveFile(filePath)) {
      setSelectedArchive(null);
      setArchiveInfo(null);
      setSelectedFiles((prev) => [...new Set([...prev, filePath])]);
      return;
    }

    // Switch to Archive View
    setSelectedFiles([]);
    setSelectedArchive(filePath);
    setExcludedPaths(new Set());
    setExtractResult(null);
    setExtractError(null);
    setTestStatus("idle");
    setTestMessage(null);

    const parentDir = filePath.replace(/[/\\][^/\\]+$/, "");
    setExtractDest(parentDir || ".");

    const filename = filePath.split(/[/\\]/).pop() || "archive";
    const lower = filename.toLowerCase();
    let format = "Archive";
    if (lower.endsWith(".bayg")) format = "Bayzip Gamer Archive (.bayg)";
    else if (lower.endsWith(".bay")) format = "Bayzip Archive (.bay)";
    else if (lower.endsWith(".zip")) format = "ZIP Archive (.zip)";
    else if (lower.endsWith(".7z")) format = "7-Zip Archive (.7z)";
    else if (lower.endsWith(".rar")) format = "RAR Archive (.rar)";
    else if (lower.endsWith(".tar") || lower.endsWith(".tar.gz") || lower.endsWith(".tgz")) format = "TAR Archive";

    let initialSize = 0;
    try {
      initialSize = await invoke<number>("get_file_size", { path: filePath });
    } catch {
      initialSize = 0;
    }

    setArchiveInfo({
      path: filePath,
      filename,
      size: initialSize,
      format,
      is_archive: true,
      is_encrypted: false,
      file_count: 0,
      total_uncompressed_size: 0,
      entries: [],
    });
    setIsInspectingArchive(true);

    try {
      const info = await invoke<ArchiveInfo>("get_archive_info", {
        path: filePath,
        password: pwd || (extractPassword.trim() ? extractPassword.trim() : undefined),
      });
      if (info.size === 0 && initialSize > 0) {
        info.size = initialSize;
      }
      setArchiveInfo(info);
      if (info.is_encrypted && (!pwd || !pwd.trim())) {
        setIsPasswordPromptOpen(true);
      }
    } catch (err) {
      console.error("Failed to inspect archive info:", err);
      const errStr = String(err).toLowerCase();
      const isEnc = errStr.includes("password") || errStr.includes("encrypt");
      setArchiveInfo({
        path: filePath,
        filename,
        size: initialSize,
        format,
        is_archive: true,
        is_encrypted: isEnc,
        file_count: 0,
        total_uncompressed_size: 0,
        entries: [],
      });
      if (isEnc) setIsPasswordPromptOpen(true);
    } finally {
      setIsInspectingArchive(false);
    }
  };

  // Global Listeners & Drag Drop
  useEffect(() => {
    let isMounted = true;
    let unlistenDrop: (() => void) | undefined;
    let unlistenCompress: (() => void) | undefined;
    let unlistenExtract: (() => void) | undefined;
    let unlistenTest: (() => void) | undefined;

    invoke<SystemSpecs>("get_system_specs")
      .then((res) => {
        if (isMounted) setSpecs(res);
      })
      .catch((err) => console.error("Specs query error:", err));

    invoke<StartupCommand>("get_cli_args")
      .then((cmd) => {
        if (isMounted && cmd && cmd.paths && cmd.paths.length > 0) {
          const archivePaths = cmd.paths.filter(isArchiveFile);
          if (archivePaths.length > 0) {
            selectArchive(archivePaths[0]);
          } else {
            setSelectedFiles((prev) => [...new Set([...prev, ...cmd.paths])]);
          }
        }
      })
      .catch(() => {});

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          setIsDraggingOver(true);
        } else if (event.payload.type === "leave") {
          setIsDraggingOver(false);
        } else if (event.payload.type === "drop") {
          setIsDraggingOver(false);
          const paths = event.payload.paths;
          if (paths && paths.length > 0) {
            const archiveFile = paths.find(isArchiveFile);
            if (archiveFile) {
              selectArchive(archiveFile);
            } else {
              setSelectedArchive(null);
              setArchiveInfo(null);
              setSelectedFiles((prev) => [...new Set([...prev, ...paths])]);
            }
          }
        }
      })
      .then((fn) => {
        if (isMounted) unlistenDrop = fn;
        else fn();
      })
      .catch(() => {});

    listen<CompressProgress>("compress_progress", (event) => {
      if (isMounted) setCompressProgress(event.payload);
    })
      .then((fn) => {
        if (isMounted) unlistenCompress = fn;
        else fn();
      })
      .catch(() => {});

    listen<ExtractProgress>("extract_progress", (event) => {
      if (isMounted) setExtractProgress(event.payload);
    })
      .then((fn) => {
        if (isMounted) unlistenExtract = fn;
        else fn();
      })
      .catch(() => {});

    listen<TestArchiveProgress>("test_progress", (event) => {
      if (isMounted) setTestProgress(event.payload);
    })
      .then((fn) => {
        if (isMounted) unlistenTest = fn;
        else fn();
      })
      .catch(() => {});

    return () => {
      isMounted = false;
      if (unlistenDrop) unlistenDrop();
      if (unlistenCompress) unlistenCompress();
      if (unlistenExtract) unlistenExtract();
      if (unlistenTest) unlistenTest();
    };
  }, []);

  // Toolbar Actions
  const handleSelectFiles = async () => {
    if (isCompressingRef.current || isExtractingRef.current || isTestingRef.current) return;
    try {
      const selected = await open({
        multiple: true,
        title: "Select files to compress",
      });
      if (selected) {
        const newFiles = Array.isArray(selected) ? selected : [selected];
        const archiveFile = newFiles.find(isArchiveFile);
        if (newFiles.length === 1 && archiveFile) {
          selectArchive(archiveFile);
        } else {
          setSelectedArchive(null);
          setArchiveInfo(null);
          setSelectedFiles((prev) => [...new Set([...prev, ...newFiles])]);
        }
      }
    } catch (err) {
      console.error("File selection error:", err);
    }
  };

  const handleSelectFolder = async () => {
    if (isCompressingRef.current || isExtractingRef.current || isTestingRef.current) return;
    try {
      const selected = await open({
        directory: true,
        multiple: true,
        title: "Select folder to compress",
      });
      if (selected) {
        const newFolders = Array.isArray(selected) ? selected : [selected];
        setSelectedArchive(null);
        setArchiveInfo(null);
        setSelectedFiles((prev) => [...new Set([...prev, ...newFolders])]);
      }
    } catch (err) {
      console.error("Folder selection error:", err);
    }
  };

  const handleSelectArchive = async () => {
    if (isExtractingRef.current || isCompressingRef.current || isTestingRef.current) return;
    try {
      const selected = await open({
        multiple: false,
        title: "Select Archive to Open",
        filters: [
          {
            name: "Supported Archives",
            extensions: ["bay", "bayg", "zip", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "iso", "cab", "rar"],
          },
          { name: "All Files (*.*)", extensions: ["*"] },
        ],
      });
      if (selected && typeof selected === "string") {
        selectArchive(selected);
      }
    } catch (err) {
      console.error("Archive selection error:", err);
    }
  };

  const handleClearSelection = () => {
    if (isCompressingRef.current || isExtractingRef.current || isTestingRef.current) return;
    setSelectedFiles([]);
    setSelectedArchive(null);
    setArchiveInfo(null);
    setCustomArchiveName("");
    setStagedEntries([]);
    setExcludedPaths(new Set());
    setExcludedCompressPaths(new Set());
    setTestStatus("idle");
    setTestMessage(null);
    setIsProgressMinimized(false);
    setIsCompressModalOpen(false);
    setIsExtractModalOpen(false);
  };

  // In-Place Test Action
  const handleInPlaceTest = async () => {
    if (isTestingRef.current || isCompressingRef.current || isExtractingRef.current) return;

    let targetArchive = selectedArchive;

    if (!targetArchive) {
      try {
        const selected = await open({
          multiple: false,
          title: "Select Archive to Test",
          filters: [
            {
              name: "Supported Archives",
              extensions: ["bay", "bayg", "zip", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "iso", "cab", "rar"],
            },
            { name: "All Files (*.*)", extensions: ["*"] },
          ],
        });
        if (selected && typeof selected === "string") {
          targetArchive = selected;
          await selectArchive(selected);
        } else {
          return;
        }
      } catch (err) {
        console.error("Test archive selection error:", err);
        return;
      }
    }

    isTestingRef.current = true;
    setTestStatus("testing");
    setTestMessage("Verifying container integrity & stream checksums...");
    setTestProgress({
      percent: 0,
      current_file: "Initializing stream verifier...",
      bytes_tested: 0,
      total_bytes: archiveInfo?.total_uncompressed_size || 0,
    });

    try {
      const res = await invoke<TestArchiveResponse>("test_archive", {
        archivePath: targetArchive,
        password: extractPassword.trim() ? extractPassword.trim() : undefined,
      });

      if (!res.success && res.error && (res.error.toLowerCase().includes("password") || res.error.toLowerCase().includes("encrypt"))) {
        setTestStatus("error");
        setTestMessage("Password required for encrypted archive");
        setIsPasswordPromptOpen(true);
      } else if (res.is_ok) {
        setTestStatus("verified");
        setTestMessage(`✓ Stream Verified 100% OK (${res.tested_files} items, 0 CRC errors)`);
      } else {
        setTestStatus("error");
        if (res.errors && res.errors.length > 0) {
          const firstErr = res.errors[0];
          setTestMessage(`Verification failed: ${firstErr.file_name} (${firstErr.error})`);
        } else {
          setTestMessage(`Verification failed: ${res.error || "Archive corruption detected"}`);
        }
      }
    } catch (err) {
      const errStr = String(err);
      if (errStr.toLowerCase().includes("password") || errStr.toLowerCase().includes("encrypt")) {
        setTestStatus("error");
        setTestMessage("Password required for encrypted archive");
        setIsPasswordPromptOpen(true);
      } else {
        setTestStatus("error");
        setTestMessage(`Verification failed: ${errStr}`);
      }
    } finally {
      isTestingRef.current = false;
      setTestProgress(null);
      setTimeout(() => {
        setTestStatus("idle");
        setTestMessage(null);
      }, 5000);
    }
  };

  // Compression Execution
  const handleCompress = async () => {
    if (isCompressingRef.current || isExtractingRef.current || isTestingRef.current || selectedFiles.length === 0) return;

    if (selectedEncryption !== "none" && !compressPassword.trim()) {
      setGlobalError({
        title: "Encryption Password Required",
        message: "Please enter an archive password to enable encryption.",
      });
      return;
    }

    isCompressingRef.current = true;
    const targetArchiveName =
      customArchiveName.trim() ||
      (selectedFiles.length === 1 ? selectedFiles[0].split(/[/\\]/).pop() || "archive" : "archive");

    setActiveCompressConfig({
      format: selectedFormat,
      algorithm: selectedAlgorithm,
      sliderValue,
      archiveName: targetArchiveName,
    });
    setIsCompressing(true);
    setIsCompressModalOpen(false);
    setIsProgressMinimized(false);
    setIsCancellingCompress(false);
    setCompressProgress({
      percent: 0,
      current_file: "Initializing compression...",
      bytes_processed: 0,
      total_bytes: 0,
    });

    try {
      if (separateArchives && selectedFiles.length > 1) {
        let lastRes: CompressResult | null = null;
        let totalOrig = 0;
        let totalComp = 0;
        let totalElapsed = 0;

        for (let i = 0; i < selectedFiles.length; i++) {
          const file = selectedFiles[i];
          const res = await invoke<CompressResult>("compress_files", {
            payload: {
              files: [file],
              format: selectedFormat,
              algorithm: selectedAlgorithm,
              slider_value: sliderValue,
              custom_name: undefined,
              encryption: selectedEncryption,
              password: selectedEncryption !== "none" && compressPassword.trim() ? compressPassword.trim() : undefined,
              output_dir: !archiveToOriginalPath && compressDestDir.trim() ? compressDestDir.trim() : undefined,
              smart_bypass: smartMediaBypass,
              excluded_paths: excludedCompressPaths.size > 0 ? Array.from(excludedCompressPaths) : undefined,
            },
          });
          if (!res.success) {
            setGlobalError({
              title: "Compression Failed",
              message: res.error || `Failed to compress ${file}`,
            });
            break;
          }
          totalOrig += res.original_size;
          totalComp += res.compressed_size;
          totalElapsed += res.elapsed_ms;
          lastRes = res;
        }

        if (lastRes && lastRes.success) {
          setCompressResult({
            ...lastRes,
            original_size: totalOrig,
            compressed_size: totalComp,
            elapsed_ms: totalElapsed,
          });
        }
      } else {
        const res = await invoke<CompressResult>("compress_files", {
          payload: {
            files: selectedFiles,
            format: selectedFormat,
            algorithm: selectedAlgorithm,
            slider_value: sliderValue,
            custom_name: customArchiveName.trim() || undefined,
            encryption: selectedEncryption,
            password: selectedEncryption !== "none" && compressPassword.trim() ? compressPassword.trim() : undefined,
            output_dir: !archiveToOriginalPath && compressDestDir.trim() ? compressDestDir.trim() : undefined,
            smart_bypass: smartMediaBypass,
            excluded_paths: excludedCompressPaths.size > 0 ? Array.from(excludedCompressPaths) : undefined,
          },
        });
        setCompressResult(res);
        if (!res.success) {
          setGlobalError({
            title: "Compression Failed",
            message: res.error || "An unexpected error occurred during compression.",
          });
        }
      }
    } catch (err) {
      console.error("Compression execution error:", err);
      setGlobalError({
        title: "Compression Failed",
        message: "Failed to create archive.",
        details: String(err),
      });
    } finally {
      isCompressingRef.current = false;
      setIsCompressing(false);
      setIsCompressModalOpen(false);
      setIsCancellingCompress(false);
      setActiveCompressConfig(null);
      setCompressProgress(null);
      setIsProgressMinimized(false);
      invoke("close_progress_window").catch(() => {});
    }
  };

  // Extraction Execution
  const handleExtract = async (options?: {
    destination?: string;
    extractToSubfolder?: boolean;
    openFolderOnExtract?: boolean;
    password?: string;
  }) => {
    if (isExtractingRef.current || isCompressingRef.current || isTestingRef.current || !selectedArchive) return;

    const effectivePassword = options?.password !== undefined ? options.password : extractPassword;
    const effectiveDest = options?.destination || extractDest;
    const effectiveSubfolder = options?.extractToSubfolder !== undefined ? options.extractToSubfolder : extractToSubfolder;
    const effectiveOpenFolder = options?.openFolderOnExtract !== undefined ? options.openFolderOnExtract : openFolderOnExtract;

    if (archiveInfo?.is_encrypted && !effectivePassword.trim()) {
      setExtractError("Password required to extract encrypted archive.");
      setIsPasswordPromptOpen(true);
      return;
    }

    isExtractingRef.current = true;
    setIsExtracting(true);
    setIsExtractModalOpen(false);
    setIsProgressMinimized(false);
    setIsCancellingExtract(false);
    setExtractError(null);
    setExtractProgress({
      percent: 0,
      current_file: "Preparing extraction...",
      bytes_extracted: 0,
    });

    try {
      let finalDest = effectiveDest;
      if (effectiveSubfolder) {
        const fallbackName = selectedArchive ? selectedArchive.split(/[/\\]/).pop() || "archive" : "archive";
        const stem = getArchiveStemName(archiveInfo?.filename || fallbackName);
        const cleanDest = effectiveDest.replace(/[/\\]+$/, "");
        finalDest = `${cleanDest}/${stem}`;
      }

      const res = await invoke<ExtractResult>("extract_archive", {
        archivePath: selectedArchive,
        destination: finalDest,
        password: effectivePassword.trim() ? effectivePassword.trim() : undefined,
        expectedSize: archiveInfo?.total_uncompressed_size || archiveInfo?.size || undefined,
        extractToSubfolder: effectiveSubfolder,
        excludedPaths: excludedPaths.size > 0 ? Array.from(excludedPaths) : undefined,
      });

      setExtractResult(res);
      setIsPasswordPromptOpen(false);

      if (!res.success) {
        setGlobalError({
          title: "Extraction Failed",
          message: res.error || "An unexpected error occurred during extraction.",
        });
      } else if (effectiveOpenFolder && res.output_directory) {
        revealItemInDir(res.output_directory).catch(() => {});
      }
    } catch (err) {
      console.error("Extraction execution error:", err);
      const errStr = String(err);
      setExtractError(errStr);
      if (errStr.toLowerCase().includes("password")) {
        setIsPasswordPromptOpen(true);
      } else {
        setGlobalError({
          title: "Extraction Failed",
          message: "Failed to extract archive contents.",
          details: errStr,
        });
      }
    } finally {
      isExtractingRef.current = false;
      setIsExtracting(false);
      setIsExtractModalOpen(false);
      setIsCancellingExtract(false);
      setExtractProgress(null);
      setIsProgressMinimized(false);
      invoke("close_progress_window").catch(() => {});
    }
  };

  const handleSelectDestFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Extraction Destination Folder",
        defaultPath: extractDest || undefined,
      });
      if (selected && typeof selected === "string") {
        setExtractDest(selected);
      }
    } catch (err) {
      console.error("Destination folder selection error:", err);
    }
  };

  const handleOpenSettings = async () => {
    try {
      await invoke("open_settings_window");
    } catch {
      setIsSettingsOpen(true);
    }
  };

  const handleRegisterContextMenu = async () => {
    setIsSyncingRegistry(true);
    try {
      const res = await invoke<string>("register_context_menu");
      setContextMenuStatus(res);
    } catch (e) {
      setContextMenuStatus("Registration failed: " + String(e));
    } finally {
      setIsSyncingRegistry(false);
    }
  };

  // Determine active document name for Titlebar
  const activeDocumentName = selectedArchive
    ? selectedArchive.split(/[/\\]/).pop()
    : selectedFiles.length > 0
      ? `${selectedFiles.length} item(s) staged`
      : undefined;

  return (
    <div className="h-screen w-screen overflow-hidden flex flex-col bg-[#141416] text-[#F4F4F6] select-none font-sans">
      {/* 1. Titlebar */}
      <Titlebar activeDocument={activeDocumentName} onClose={handleRequestClose} />

      {/* 2. Unified Top Toolbar */}
      <div className="h-11 bg-[#1C1C1F] border-b border-[#2E2E34] px-3 flex items-center justify-between text-xs select-none shrink-0">
        {/* Left Group (Load / Stage) */}
        <div className="flex items-center space-x-1.5">
          <button
            type="button"
            disabled={isAnyModalOpen}
            onClick={handleSelectArchive}
            className="flex items-center space-x-1.5 px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] border border-[#323238] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none"
            title="Open existing archive to inspect & extract (Ctrl+O)"
          >
            <span className="text-[#38BDF8]">📂</span>
            <span>Open Archive</span>
          </button>

          <button
            type="button"
            disabled={isAnyModalOpen}
            onClick={handleSelectFiles}
            className="flex items-center space-x-1.5 px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] border border-[#323238] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none"
            title="Add files to compress (Ctrl+N)"
          >
            <span className="text-slate-400">➕</span>
            <span>Add Files</span>
          </button>

          <button
            type="button"
            disabled={isAnyModalOpen}
            onClick={handleSelectFolder}
            className="flex items-center space-x-1.5 px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] border border-[#323238] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none"
            title="Add directory to compress"
          >
            <span className="text-[#FBBF24]">📁</span>
            <span>Add Folder</span>
          </button>
        </div>

        {/* Right Group (Actions & Modals) */}
        <div className="flex items-center space-x-1.5">
          {/* [ Compress... ] Button (Active Sky-Blue when files are staged) */}
          <button
            type="button"
            disabled={isAnyModalOpen}
            onClick={() => {
              if (selectedFiles.length === 0) {
                handleSelectFiles();
              } else {
                setIsCompressModalOpen(true);
              }
            }}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-[4px] text-xs transition-all cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none ${
              selectedFiles.length > 0
                ? "bg-[#38BDF8] hover:bg-[#7DD3FC] text-[#0F172A] font-semibold shadow-sm"
                : "bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] border border-[#323238]"
            }`}
            title="Configure and compress staged files into archive"
          >
            <span>📦</span>
            <span>Compress...</span>
          </button>

          {/* [ Extract... ] Button (Active Sky-Blue when archive is open) */}
          <button
            type="button"
            disabled={isAnyModalOpen}
            onClick={() => {
              if (!selectedArchive) {
                handleSelectArchive();
              } else {
                setIsExtractModalOpen(true);
              }
            }}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-[4px] text-xs transition-all cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none ${
              selectedArchive && archiveInfo
                ? "bg-[#38BDF8] hover:bg-[#7DD3FC] text-[#0F172A] font-semibold shadow-sm"
                : "bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] border border-[#323238]"
            }`}
            title="Extract archive contents to destination"
          >
            <span>📤</span>
            <span>Extract...</span>
          </button>

          {/* [ 🛡️ Test ] Button (In-Place Verification) */}
          <button
            type="button"
            disabled={isAnyModalOpen || !selectedArchive || !archiveInfo}
            onClick={handleInPlaceTest}
            className="flex items-center space-x-1.5 px-2.5 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#F4F4F6] border border-[#323238] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none"
            title="Verify archive stream integrity & checksums in-place (Ctrl+T)"
          >
            <span>🛡️</span>
            <span>Test</span>
          </button>

          {/* [ ✕ Clear ] Button */}
          {(selectedArchive || selectedFiles.length > 0) && (
            <button
              type="button"
              disabled={isAnyModalOpen}
              onClick={handleClearSelection}
              className="flex items-center space-x-1 px-2 py-1.5 rounded-[4px] bg-[#252529] hover:bg-rose-950/40 text-[#9CA3AF] hover:text-rose-300 border border-[#323238] hover:border-rose-800 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none"
              title="Clear selection and return to home"
            >
              <span>✕</span>
              <span>Clear</span>
            </button>
          )}

          <div className="w-[1px] h-5 bg-[#2E2E34] mx-1"></div>

          {/* [ About ] */}
          <button
            type="button"
            disabled={isAnyModalOpen}
            onClick={() => setIsAboutOpen(true)}
            className="px-2 py-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] hover:text-white border border-[#323238] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none"
            title="About Bayzip"
          >
            About
          </button>

          {/* [ ⚙️ Settings ] */}
          <button
            type="button"
            disabled={isAnyModalOpen}
            onClick={handleOpenSettings}
            className="p-1.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] hover:text-[#38BDF8] border border-[#323238] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none"
            title="Settings & Shell Integrations"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-3.5 h-3.5">
              <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          </button>
        </div>
      </div>

      {/* 3. Full-Height File Viewer Canvas (flex-1 min-h-0) */}
      <div className="flex-1 min-h-0 flex flex-col p-2.5 overflow-hidden relative">
        <ArchiveTreeInspector
          archiveInfo={currentArchiveInfo}
          selectedArchive={selectedArchive || (selectedFiles[0] || "")}
          isLoading={isInspectingArchive || isInspectingStaged}
          onUnlockPreview={() => setIsPasswordPromptOpen(true)}
          onPreviewEntry={handlePreviewEntry}
          excludedPaths={selectedArchive ? excludedPaths : excludedCompressPaths}
          onToggleExclude={selectedArchive ? handleToggleExclude : handleToggleCompressExclude}
          onIncludeAll={selectedArchive ? handleIncludeAll : handleCompressIncludeAll}
          inspectorTitle={
            selectedArchive
              ? archiveInfo?.filename || "Archive Contents"
              : selectedFiles.length > 0
                ? "Staged Items & Folders"
                : "File Viewer"
          }
          mode={selectedArchive ? "extract" : "compress"}
          stagedItems={selectedFiles}
          itemsInfo={itemsInfo}
          onStageItem={(path) => setSelectedFiles((prev) => (prev.includes(path) ? prev : [...prev, path]))}
          onUnstageItem={(path) => setSelectedFiles((prev) => prev.filter((p) => p !== path))}
          onClearStagedItems={handleClearSelection}
          extractPassword={extractPassword}
          testStatus={testStatus}
          testMessage={testMessage}
          streamProgress={activeStreamTelemetry}
          emptyState={
            <UnifiedDropHub
              onSelectArchive={handleSelectArchive}
              onSelectFiles={handleSelectFiles}
              onSelectFolder={handleSelectFolder}
              isDraggingOver={isDraggingOver}
            />
          }
        />

        {/* 4. Keka-Style Compact Compress Modal (max-w-[440px]) - Draggable Floating Window */}
        <CompressModal
          isOpen={isCompressing ? !isProgressMinimized : isCompressModalOpen}
          onClose={() => {
            setIsCompressModalOpen(false);
            setIsProgressMinimized(false);
          }}
          customArchiveName={customArchiveName}
          setCustomArchiveName={setCustomArchiveName}
          selectedFormat={selectedFormat}
          setSelectedFormat={setSelectedFormat}
          selectedAlgorithm={selectedAlgorithm}
          setSelectedAlgorithm={setSelectedAlgorithm}
          selectedEncryption={selectedEncryption}
          setSelectedEncryption={setSelectedEncryption}
          compressPassword={compressPassword}
          setCompressPassword={setCompressPassword}
          showCompressPassword={showCompressPassword}
          setShowCompressPassword={setShowCompressPassword}
          archiveToOriginalPath={archiveToOriginalPath}
          setArchiveToOriginalPath={setArchiveToOriginalPath}
          separateArchives={separateArchives}
          setSeparateArchives={setSeparateArchives}
          smartMediaBypass={smartMediaBypass}
          setSmartMediaBypass={setSmartMediaBypass}
          sliderValue={sliderValue}
          setSliderValue={setSliderValue}
          specs={specs}
          formatAlgorithms={FORMAT_ALGORITHMS}
          onStartCompress={handleCompress}
          isCompressing={isCompressing}
          isCancelling={isCancellingCompress}
          progress={compressProgress}
          onCancelCompress={handleCancelCompress}
          onMinimizeToFooter={() => setIsProgressMinimized(true)}
          onDetachToDesktop={handleDetachProgress}
        />

        {/* 5. Matching Compact Extract Modal (max-w-[420px]) - Draggable Floating Window */}
        <ExtractModal
          isOpen={isExtracting ? !isProgressMinimized : isExtractModalOpen}
          onClose={() => {
            setIsExtractModalOpen(false);
            setIsProgressMinimized(false);
          }}
          extractDest={extractDest}
          setExtractDest={setExtractDest}
          handleSelectDestFolder={handleSelectDestFolder}
          extractToSubfolder={extractToSubfolder}
          setExtractToSubfolder={setExtractToSubfolder}
          openFolderOnExtract={openFolderOnExtract}
          setOpenFolderOnExtract={setOpenFolderOnExtract}
          onStartExtract={handleExtract}
          isExtracting={isExtracting}
          isCancelling={isCancellingExtract}
          archiveName={archiveInfo?.filename || (selectedArchive ? selectedArchive.split(/[/\\]/).pop() : undefined)}
          extractProgress={extractProgress}
          totalUncompressedSize={archiveInfo?.total_uncompressed_size || archiveInfo?.size || 0}
          onCancelExtract={handleCancelExtract}
          onMinimizeToFooter={() => setIsProgressMinimized(true)}
          onDetachToDesktop={handleDetachProgress}
        />

        {/* 6. In-Canvas Draggable Password Prompt Modal */}
        <PasswordModal
          isOpen={isPasswordPromptOpen}
          onClose={() => setIsPasswordPromptOpen(false)}
          password={extractPassword}
          setPassword={setExtractPassword}
          showPassword={showExtractPassword}
          setShowPassword={setShowExtractPassword}
          isExtracting={isExtracting}
          extractError={extractError}
          onSubmit={handleExtract}
          archiveName={archiveInfo?.filename || (selectedArchive ? selectedArchive.split(/[/\\]/).pop() : undefined)}
        />
      </div>

      {/* Result Modals */}
      <ResultModals
        compressResult={compressResult}
        onCloseCompressResult={() => {
          setCompressResult(null);
          handleClearSelection();
        }}
        extractResult={extractResult}
        archiveInfo={archiveInfo}
        onCloseExtractResult={() => {
          setExtractResult(null);
          handleClearSelection();
        }}
        formatBytes={formatBytes}
      />

      {/* About Modal */}
      <AboutModal
        isOpen={isAboutOpen}
        onClose={() => setIsAboutOpen(false)}
      />

      {/* Settings Modal */}
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        isSyncingRegistry={isSyncingRegistry}
        contextMenuStatus={contextMenuStatus}
        onRegisterContextMenu={handleRegisterContextMenu}
      />

      {/* Safe In-Memory File Preview Modal */}
      <FilePreviewModal
        isOpen={isPreviewOpen}
        onClose={() => {
          setIsPreviewOpen(false);
          setPreviewData(null);
          setPreviewError(null);
          setPreviewTargetDisplayPath("");
        }}
        preview={previewData}
        isLoading={isPreviewLoading}
        error={previewError}
        archivePath={previewTargetDisplayPath || selectedArchive || ""}
      />

      {/* Drag & Drop Visual Indicator Overlay */}
      {isDraggingOver && (
        <div className="fixed inset-0 z-[100] pointer-events-none bg-[#141416]/80 backdrop-blur-[2px] border-2 border-dashed border-[#38BDF8] m-2 rounded-xl flex flex-col items-center justify-center space-y-3">
          <div className="w-14 h-14 rounded-xl bg-[#252529] border border-[#38BDF8] flex items-center justify-center text-[#38BDF8] shadow-2xl">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} className="w-7 h-7 animate-bounce">
              <path strokeLinecap="round" strokeLinejoin="round" d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M9 19l3 3m0 0l3-3m-3 3V10" />
            </svg>
          </div>
          <p className="text-[#F4F4F6] font-semibold text-xs tracking-wide bg-[#1C1C1F] px-4 py-1.5 rounded-full border border-[#2E2E34] shadow-xl">
            Drop archive to inspect or files/folders to compress
          </p>
        </div>
      )}

      {/* Accidental Close Guard on Window Close */}
      <AbortConfirmationModal
        isOpen={isConfirmCloseOpen}
        onClose={() => setIsConfirmCloseOpen(false)}
        onConfirmAbortAndExit={handleConfirmAbortAndExit}
        onMinimizeToBackground={handleMinimizeToBackground}
        operationType={isCompressing ? "compress" : isExtracting ? "extract" : "test"}
        targetName={
          isCompressing
            ? `${customArchiveName.trim() || "Archive"}${selectedFormat}`
            : isExtracting
            ? archiveInfo?.filename || (selectedArchive ? selectedArchive.split(/[/\\]/).pop() : "Archive")
            : testProgress?.current_file
            ? testProgress.current_file.split(/[/\\]/).pop() || "Archive"
            : "Archive"
        }
        percent={
          isCompressing
            ? compressProgress?.percent ?? 0
            : isExtracting
            ? extractProgress?.percent ?? 0
            : testProgress?.percent ?? 0
        }
        isHudDetached={isHudDetached}
      />

      {/* Global Error Modal */}
      <ErrorModal
        isOpen={Boolean(globalError)}
        title={globalError?.title || "Error"}
        message={globalError?.message || ""}
        details={globalError?.details}
        onClose={() => setGlobalError(null)}
      />
    </div>
  );
}

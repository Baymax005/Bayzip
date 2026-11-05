import { useState, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

import { Titlebar } from "./components/Titlebar";
import { AboutModal } from "./components/AboutModal";
import { SettingsModal } from "./components/SettingsModal";
import { PasswordModal } from "./components/PasswordModal";
import { ResultModals } from "./components/ResultModals";
import { ExtractPage } from "./components/ExtractPage";
import { CompressPage } from "./components/CompressPage";
import { normalizeArchiveEntryPath } from "./components/ArchiveTreeInspector";
import { FilePreviewModal } from "./components/FilePreviewModal";

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
} from "./types";

const FORMAT_ALGORITHMS: Record<string, string[]> = {
  ".bay": ["Zstandard (Recommended)", "Brotli", "LZ4 (Fastest)"],
  ".bayg": ["Zstd-Fast (Auto)"],
  ".7z": ["LZMA2", "LZMA", "BZip2"],
  ".zip": ["Deflate (Standard)", "None (Store)", "BZip2 (Requires 3rd-party App)"],
  ".tar": ["None (Store)"],
};

const ARCHIVE_EXTS = [
  ".bay",
  ".bayg",
  ".zip",
  ".7z",
  ".tar",
  ".gz",
  ".tgz",
  ".rar",
  ".zst",
  ".bz2",
  ".xz",
  ".iso",
  ".cab",
];

function getArchiveStemName(filename: string): string {
  if (!filename) return "extracted";
  const lower = filename.toLowerCase();
  const compoundExts = [
    ".tar.gz",
    ".tar.zst",
    ".tar.bz2",
    ".tar.xz",
    ".tar.lz4",
    ".tar.zstd",
    ".tar.br",
    ".bayg",
  ];
  for (const ext of compoundExts) {
    if (lower.endsWith(ext)) {
      return filename.slice(0, -ext.length);
    }
  }
  return filename.replace(/\.[^/.]+$/, "");
}

function isArchiveFile(filePath: string): boolean {
  const lower = filePath.toLowerCase();
  return ARCHIVE_EXTS.some((ext) => lower.endsWith(ext));
}

function App() {
  // Navigation Mode: "compress" or "extract"
  const [activeMode, setActiveMode] = useState<"compress" | "extract">("compress");
  const activeModeRef = useRef<"compress" | "extract">(activeMode);
  useEffect(() => {
    activeModeRef.current = activeMode;
  }, [activeMode]);

  // System Specs
  const [specs, setSpecs] = useState<SystemSpecs | null>(null);
  const [sliderValue, setSliderValue] = useState<number>(50); // 0, 25, 50, 75, 100
  const [isAdvanced, setIsAdvanced] = useState(false);

  // Compress Options & Naming
  const [customArchiveName, setCustomArchiveName] = useState<string>("");
  const [selectedFormat, setSelectedFormat] = useState(".bay");
  const [selectedAlgorithm, setSelectedAlgorithm] = useState(FORMAT_ALGORITHMS[".bay"][0]);
  const [selectedEncryption, setSelectedEncryption] = useState("none");
  const [compressPassword, setCompressPassword] = useState<string>("");
  const [showCompressPassword, setShowCompressPassword] = useState<boolean>(false);
  const [archiveToOriginalPath, setArchiveToOriginalPath] = useState(true);
  const [compressDestDir, setCompressDestDir] = useState<string>("");
  const [separateArchives, setSeparateArchives] = useState(false);
  const [smartMediaBypass, setSmartMediaBypass] = useState(true);
  const [selectedFiles, setSelectedFiles] = useState<string[]>([]);
  const [itemsInfo, setItemsInfo] = useState<Record<string, { size: number; is_dir: boolean }>>({});
  const [compressViewMode, setCompressViewMode] = useState<"tree" | "list">("list");
  const [stagedEntries, setStagedEntries] = useState<ArchiveEntryItem[]>([]);
  const [isInspectingStaged, setIsInspectingStaged] = useState<boolean>(false);
  const [excludedCompressPaths, setExcludedCompressPaths] = useState<Set<string>>(new Set());
  const stagedCacheRef = useRef<Map<string, { info: SelectedItemInfo; entries: ArchiveEntryItem[] }>>(new Map());
  const [isAboutOpen, setIsAboutOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [contextMenuStatus, setContextMenuStatus] = useState<string | null>(null);
  const [isDraggingOver, setIsDraggingOver] = useState(false);
  const [isWorkspaceOpen, setIsWorkspaceOpen] = useState(false);

  // Extraction State & Password
  const [extractPassword, setExtractPassword] = useState<string>("");
  const [showExtractPassword, setShowExtractPassword] = useState<boolean>(false);
  const [isPasswordPromptOpen, setIsPasswordPromptOpen] = useState<boolean>(false);

  // Compression execution state
  const [isCompressing, setIsCompressing] = useState(false);
  const [isCancellingCompress, setIsCancellingCompress] = useState(false);
  const [progress, setProgress] = useState<CompressProgress | null>(null);
  const [compressResult, setCompressResult] = useState<CompressResult | null>(null);
  const [compressError, setCompressError] = useState<string | null>(null);
  const [activeCompressConfig, setActiveCompressConfig] = useState<{
    format: string;
    algorithm: string;
    sliderValue: number;
    archiveName: string;
  } | null>(null);

  // Extraction State
  const [selectedArchive, setSelectedArchive] = useState<string | null>(null);
  const [archiveInfo, setArchiveInfo] = useState<ArchiveInfo | null>(null);
  const [isInspectingArchive, setIsInspectingArchive] = useState<boolean>(false);
  const [extractDest, setExtractDest] = useState<string>("");
  const [extractToSubfolder, setExtractToSubfolder] = useState<boolean>(true);
  const [openFolderOnExtract, setOpenFolderOnExtract] = useState<boolean>(true);
  const [isExtracting, setIsExtracting] = useState<boolean>(false);
  const [excludedPaths, setExcludedPaths] = useState<Set<string>>(new Set());

  const handleToggleExclude = (rawPath: string, isDir?: boolean) => {
    const norm = normalizeArchiveEntryPath(rawPath).replace(/\/+$/, "");
    const prefix = norm + "/";

    setExcludedPaths((prev) => {
      const next = new Set(prev);
      const allEntries = archiveInfo?.entries || [];

      const isDirectlyExcluded = next.has(norm);
      const isAncestorExcluded = Array.from(next).some((exc) => {
        const excClean = exc.replace(/\/+$/, "");
        return norm.startsWith(excClean + "/");
      });
      const isDescendantExcluded = isDir && Array.from(next).some((exc) => {
        const excClean = exc.replace(/\/+$/, "");
        return excClean.startsWith(prefix);
      });

      const isCurrentlyExcluded = isDirectlyExcluded || isAncestorExcluded || isDescendantExcluded;

      if (isCurrentlyExcluded) {
        // User wants to INCLUDE this item back
        next.delete(norm);
        next.delete(norm + "/");

        // If an umbrella ancestor was in next, remove the ancestor and retain sibling items
        for (const exc of Array.from(next)) {
          const excClean = exc.replace(/\/+$/, "");
          if (norm.startsWith(excClean + "/")) {
            next.delete(exc);
            next.delete(excClean);
            const ancPrefix = excClean + "/";
            for (const entry of allEntries) {
              const entryNorm = normalizeArchiveEntryPath(entry.name).replace(/\/+$/, "");
              if (entryNorm.startsWith(ancPrefix) && entryNorm !== norm && !entryNorm.startsWith(prefix)) {
                next.add(entryNorm);
              }
            }
          }
        }

        // If it's a directory, un-exclude all descendants
        if (isDir) {
          for (const exc of Array.from(next)) {
            const excClean = exc.replace(/\/+$/, "");
            if (excClean === norm || excClean.startsWith(prefix)) {
              next.delete(exc);
            }
          }
        }
      } else {
        // User wants to EXCLUDE this item
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

  const includedFilesCount = useMemo(() => {
    if (!archiveInfo?.entries || archiveInfo.entries.length === 0) {
      return Math.max(0, (archiveInfo?.file_count || 0) - excludedPaths.size);
    }
    if (excludedPaths.size === 0) {
      return archiveInfo.file_count || archiveInfo.entries.filter((e) => !e.is_dir).length;
    }
    const excludedArray = Array.from(excludedPaths);
    let count = 0;
    for (const e of archiveInfo.entries) {
      if (e.is_dir) continue;
      const norm = normalizeArchiveEntryPath(e.name);
      const isExcluded = excludedArray.some((ex) => {
        if (norm === ex) return true;
        const prefix = ex.endsWith("/") ? ex : ex + "/";
        return norm.startsWith(prefix);
      });
      if (!isExcluded) {
        count++;
      }
    }
    return count;
  }, [archiveInfo, excludedPaths]);
  const [isCancellingExtract, setIsCancellingExtract] = useState<boolean>(false);
  const [extractProgress, setExtractProgress] = useState<ExtractProgress | null>(null);
  const [extractResult, setExtractResult] = useState<ExtractResult | null>(null);
  const [extractError, setExtractError] = useState<string | null>(null);
  // Safe in-memory preview state
  const [isPreviewOpen, setIsPreviewOpen] = useState(false);
  const [previewData, setPreviewData] = useState<ArchiveEntryPreview | null>(null);
  const [isPreviewLoading, setIsPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewTargetDisplayPath, setPreviewTargetDisplayPath] = useState<string>("");

  const handlePreviewEntry = async (entryName: string) => {
    const isDiskFile = /^[a-zA-Z]:[/\\]/.test(entryName) || activeMode === "compress" || !selectedArchive;

    setIsPreviewOpen(true);
    setIsPreviewLoading(true);
    setPreviewError(null);
    setPreviewData(null);

    if (isDiskFile) {
      setPreviewTargetDisplayPath(entryName);
      try {
        const res = await invoke<ArchiveEntryPreview>("preview_staged_file", {
          path: entryName,
        });
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

  // Query file and folder size metadata and recursive tree incrementally when selection changes
  useEffect(() => {
    if (selectedFiles.length === 0) {
      stagedCacheRef.current.clear();
      setItemsInfo({});
      setStagedEntries([]);
      setExcludedCompressPaths(new Set());
      setIsInspectingStaged(false);
      if (!isCompressing) {
        setCustomArchiveName("");
      }
      return;
    }
    let isCurrent = true;

    // Determine paths added and paths removed
    const currentCachedPaths = Array.from(stagedCacheRef.current.keys());
    const pathsRemoved = currentCachedPaths.filter((p) => !selectedFiles.includes(p));
    const pathsToAdd = selectedFiles.filter((p) => !stagedCacheRef.current.has(p));

    // Prune removed paths from cache immediately
    if (pathsRemoved.length > 0) {
      pathsRemoved.forEach((p) => stagedCacheRef.current.delete(p));
    }

    // Fast path: Only removals occurred (0ms instant rebuild, zero disk I/O, zero loading spinner)
    if (pathsToAdd.length === 0) {
      const newItemsInfo: Record<string, { size: number; is_dir: boolean }> = {};
      const newStagedEntries: ArchiveEntryItem[] = [];
      let hasFolder = false;

      for (const [p, data] of stagedCacheRef.current.entries()) {
        newItemsInfo[p] = { size: data.info.size, is_dir: data.info.is_dir };
        if (data.info.is_dir) hasFolder = true;
        newStagedEntries.push(...data.entries);
      }

      setItemsInfo(newItemsInfo);
      setStagedEntries(newStagedEntries);
      if (hasFolder) {
        setCompressViewMode("tree");
      }

      // Prune orphaned excluded paths that no longer exist in stagedEntries
      setExcludedCompressPaths((prev) => {
        if (prev.size === 0) return prev;
        const entryNorms = new Set(newStagedEntries.map((e) => normalizeArchiveEntryPath(e.name).replace(/\/+$/, "")));
        const pruned = new Set<string>();
        for (const exc of prev) {
          const clean = exc.replace(/\/+$/, "");
          if (entryNorms.has(clean)) {
            pruned.add(exc);
          }
        }
        return pruned;
      });

      setIsInspectingStaged(false);
      return;
    }

    // Slow path: Only inspect the NEWLY ADDED paths from disk!
    setIsInspectingStaged(true);

    Promise.all([
      invoke<SelectedItemInfo[]>("get_selected_items_info", { paths: pathsToAdd }),
      invoke<ArchiveEntryItem[]>("inspect_staged_items", { paths: pathsToAdd }),
    ])
      .then(([infos, newEntries]) => {
        if (!isCurrent) return;

        // Associate new entries with their respective top-level item path
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

        // Reconstruct aggregated states
        const aggregatedItemsInfo: Record<string, { size: number; is_dir: boolean }> = {};
        const aggregatedEntries: ArchiveEntryItem[] = [];
        let hasFolder = false;

        for (const [p, data] of stagedCacheRef.current.entries()) {
          aggregatedItemsInfo[p] = { size: data.info.size, is_dir: data.info.is_dir };
          if (data.info.is_dir) hasFolder = true;
          aggregatedEntries.push(...data.entries);
        }

        setItemsInfo(aggregatedItemsInfo);
        setStagedEntries(aggregatedEntries);
        if (hasFolder) {
          setCompressViewMode("tree");
        }

        // Prune orphaned excluded paths that no longer exist in stagedEntries
        setExcludedCompressPaths((prev) => {
          if (prev.size === 0) return prev;
          const entryNorms = new Set(aggregatedEntries.map((e) => normalizeArchiveEntryPath(e.name).replace(/\/+$/, "")));
          const pruned = new Set<string>();
          for (const exc of prev) {
            const clean = exc.replace(/\/+$/, "");
            if (entryNorms.has(clean)) {
              pruned.add(exc);
            }
          }
          return pruned;
        });
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
  }, [selectedFiles]);

  const handleToggleCompressExclude = (rawPath: string, isDir?: boolean) => {
    const norm = normalizeArchiveEntryPath(rawPath).replace(/\/+$/, "");
    const prefix = norm + "/";

    setExcludedCompressPaths((prev) => {
      const next = new Set(prev);
      const allEntries = stagedEntries;

      const isDirectlyExcluded = next.has(norm);
      const isAncestorExcluded = Array.from(next).some((exc) => {
        const excClean = exc.replace(/\/+$/, "");
        return norm.startsWith(excClean + "/");
      });
      const isDescendantExcluded = isDir && Array.from(next).some((exc) => {
        const excClean = exc.replace(/\/+$/, "");
        return excClean.startsWith(prefix);
      });

      const isCurrentlyExcluded = isDirectlyExcluded || isAncestorExcluded || isDescendantExcluded;

      if (isCurrentlyExcluded) {
        // User wants to INCLUDE this item back
        next.delete(norm);
        next.delete(norm + "/");

        // If an umbrella ancestor was in next, remove the ancestor and retain sibling items
        for (const exc of Array.from(next)) {
          const excClean = exc.replace(/\/+$/, "");
          if (norm.startsWith(excClean + "/")) {
            next.delete(exc);
            next.delete(excClean);
            const ancPrefix = excClean + "/";
            for (const entry of allEntries) {
              const entryNorm = normalizeArchiveEntryPath(entry.name).replace(/\/+$/, "");
              if (entryNorm.startsWith(ancPrefix) && entryNorm !== norm && !entryNorm.startsWith(prefix)) {
                next.add(entryNorm);
              }
            }
          }
        }

        // If it's a directory, un-exclude all descendants
        if (isDir) {
          for (const exc of Array.from(next)) {
            const excClean = exc.replace(/\/+$/, "");
            if (excClean === norm || excClean.startsWith(prefix)) {
              next.delete(exc);
            }
          }
        }
      } else {
        // User wants to EXCLUDE this item
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

  const handleCompressIncludeAll = () => {
    setExcludedCompressPaths(new Set());
  };

  const handlePreviewStagedEntry = async (entryName: string) => {
    let resolvedPath: string | null = null;
    const isAbsolute = /^[a-zA-Z]:[/\\]/.test(entryName) || entryName.startsWith("/");
    if (isAbsolute) {
      resolvedPath = entryName;
    } else {
      const clean = entryName.replace(/\\/g, "/").replace(/^\/+/, "");
      const firstSegment = clean.split("/")[0].toLowerCase();

      for (const filePath of selectedFiles) {
        const normFilePath = filePath.replace(/\\/g, "/");
        const name = normFilePath.split("/").pop() || "";
        if (name.toLowerCase() === firstSegment) {
          if (clean.includes("/")) {
            const sub = clean.substring(clean.indexOf("/") + 1);
            resolvedPath = `${filePath}/${sub}`;
          } else {
            resolvedPath = filePath;
          }
          break;
        }
      }

      if (!resolvedPath && selectedFiles.length === 1) {
        resolvedPath = `${selectedFiles[0]}/${clean}`;
      }
    }

    const displayPath = resolvedPath || entryName;
    setPreviewTargetDisplayPath(displayPath);

    if (!resolvedPath) {
      setPreviewError("Could not locate source file on disk.");
      setIsPreviewOpen(true);
      return;
    }

    setIsPreviewOpen(true);
    setIsPreviewLoading(true);
    setPreviewError(null);
    setPreviewData(null);

    try {
      const res = await invoke<ArchiveEntryPreview>("preview_staged_file", {
        path: resolvedPath,
      });
      setPreviewData(res);
    } catch (err: unknown) {
      setPreviewError(typeof err === "string" ? err : String(err));
    } finally {
      setIsPreviewLoading(false);
    }
  };

  // Auto-fill archive name from first file stem if currently empty
  useEffect(() => {
    if (selectedFiles.length > 0 && !customArchiveName) {
      const first = selectedFiles[0];
      const filename = first.split(/[/\\]/).pop() || "bayzip_archive";
      const stem = getArchiveStemName(filename);
      setCustomArchiveName(stem);
    }
  }, [selectedFiles]);

  // Select Archive Handler (with metadata inquiry)
  const selectArchive = async (filePath: string, pwd?: string) => {
    if (!isArchiveFile(filePath)) {
      setActiveMode("compress");
      setSelectedFiles((prev) => [...new Set([...prev, filePath])]);
      return;
    }

    setSelectedArchive(filePath);
    setExcludedPaths(new Set());
    setExtractResult(null);
    setExtractError(null);
    setExtractProgress(null);

    // Parent directory default
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

    // Immediate query for exact file size so UI never shows "0 B" while indexing
    let initialSize = 0;
    try {
      initialSize = await invoke<number>("get_file_size", { path: filePath });
    } catch {
      initialSize = 0;
    }

    // Immediate skeleton info so UI renders archive card & dedicated folder name immediately with real size
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
      if (isEnc) {
        setIsPasswordPromptOpen(true);
      }
    } finally {
      setIsInspectingArchive(false);
    }
  };

  useEffect(() => {
    let isMounted = true;
    let unlistenDrop: (() => void) | undefined;
    let unlistenCompress: (() => void) | undefined;
    let unlistenExtract: (() => void) | undefined;
    let unlistenBypass: (() => void) | undefined;

    // 0. Query initial smart media bypass global preference
    invoke<boolean>("get_smart_media_bypass")
      .then((enabled) => {
        if (isMounted) setSmartMediaBypass(enabled);
      })
      .catch(() => {});

    // Listen for cross-window smart media bypass updates
    listen<boolean>("smart_media_bypass_changed", (event) => {
      if (isMounted) setSmartMediaBypass(event.payload);
    })
      .then((fn) => {
        if (isMounted) unlistenBypass = fn;
        else fn();
      })
      .catch(() => {});

    // 1. Instant hardware specs query
    invoke<SystemSpecs>("get_system_specs")
      .then((res) => {
        if (isMounted) setSpecs(res);
      })
      .catch((err) => console.error("Specs query error:", err));

    // 2. Fetch CLI / Context menu startup arguments
    invoke<StartupCommand>("get_cli_args")
      .then((cmd) => {
        if (isMounted && cmd && cmd.paths && cmd.paths.length > 0) {
          if (cmd.action === "compress") {
            setActiveMode("compress");
            setSelectedFiles((prev) => [...new Set([...prev, ...cmd.paths])]);
          } else if (cmd.action === "extract") {
            if (cmd.paths.length === 1 && isArchiveFile(cmd.paths[0])) {
              setActiveMode("extract");
              selectArchive(cmd.paths[0]);
            } else {
              setActiveMode("compress");
              setSelectedFiles((prev) => [...new Set([...prev, ...cmd.paths])]);
            }
          } else {
            if (cmd.paths.length === 1 && isArchiveFile(cmd.paths[0])) {
              setActiveMode("extract");
              selectArchive(cmd.paths[0]);
            } else {
              setActiveMode("compress");
              setSelectedFiles((prev) => [...new Set([...prev, ...cmd.paths])]);
            }
          }
        }
      })
      .catch(() => { });

    // 3. Enable window-level drop acceptance for Windows WebView2 & Explorer
    const handleWindowDragOver = (e: DragEvent) => {
      e.preventDefault();
      if (e.dataTransfer) {
        e.dataTransfer.dropEffect = "copy";
      }
    };
    const handleWindowDrop = (e: DragEvent) => {
      e.preventDefault();
      setIsDraggingOver(false);
    };
    window.addEventListener("dragover", handleWindowDragOver);
    window.addEventListener("drop", handleWindowDrop);

    // 4. Listen to native Tauri file drop events
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
            if (activeModeRef.current === "compress") {
              setSelectedFiles((prev) => [...new Set([...prev, ...paths])]);
            } else {
              if (paths.length === 1 && isArchiveFile(paths[0])) {
                selectArchive(paths[0]);
              } else {
                setActiveMode("compress");
                setSelectedFiles((prev) => [...new Set([...prev, ...paths])]);
              }
            }
          }
        }
      })
      .then((fn) => {
        if (isMounted) unlistenDrop = fn;
        else fn();
      })
      .catch((err) => {
        console.error("Failed to register drag-drop listener:", err);
      });

    // 5. Listen to live compression progress events from Rust
    listen<CompressProgress>("compress_progress", (event) => {
      if (isMounted) setProgress(event.payload);
    })
      .then((fn) => {
        if (isMounted) unlistenCompress = fn;
        else fn();
      })
      .catch((err) => {
        console.error("Compress progress listener error:", err);
      });

    // 6. Listen to live extraction progress events from Rust
    listen<ExtractProgress>("extract_progress", (event) => {
      if (isMounted) setExtractProgress(event.payload);
    })
      .then((fn) => {
        if (isMounted) unlistenExtract = fn;
        else fn();
      })
      .catch((err) => {
        console.error("Extract progress listener error:", err);
      });

    return () => {
      isMounted = false;
      window.removeEventListener("dragover", handleWindowDragOver);
      window.removeEventListener("drop", handleWindowDrop);
      if (unlistenDrop) unlistenDrop();
      if (unlistenCompress) unlistenCompress();
      if (unlistenExtract) unlistenExtract();
      if (unlistenBypass) unlistenBypass();
    };
  }, []);

  const handleUpdateSmartMediaBypass = async (val: boolean) => {
    setSmartMediaBypass(val);
    try {
      await invoke("set_smart_media_bypass", { enabled: val });
    } catch (e) {
      console.error("Failed to update smart media bypass:", e);
    }
  };

  useEffect(() => {
    setSelectedAlgorithm(FORMAT_ALGORITHMS[selectedFormat][0]);
  }, [selectedFormat]);


  const handleSelectFiles = async () => {
    try {
      const selected = await open({
        multiple: true,
        title: "Select files to compress",
      });
      if (selected) {
        const newFiles = Array.isArray(selected) ? selected : [selected];
        setSelectedFiles((prev) => [...new Set([...prev, ...newFiles])]);
      }
    } catch (err) {
      console.error("File selection error:", err);
    }
  };

  const handleSelectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: true,
        title: "Select folder to compress",
      });
      if (selected) {
        const newFolders = Array.isArray(selected) ? selected : [selected];
        setSelectedFiles((prev) => [...new Set([...prev, ...newFolders])]);
      }
    } catch (err) {
      console.error("Folder selection error:", err);
    }
  };

  const handleSelectArchive = async () => {
    if (isExtracting) return;
    try {
      const selected = await open({
        multiple: false,
        title: "Select Archive to Extract",
        filters: [
          {
            name: "Supported Archives",
            extensions: ["bay", "bayg", "zip", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "iso", "cab", "rar"],
          },
          {
            name: "Bayzip Archives (*.bay, *.bayg)",
            extensions: ["bay", "bayg"],
          },
          {
            name: "ZIP Archives (*.zip)",
            extensions: ["zip"],
          },
          {
            name: "7-Zip Archives (*.7z)",
            extensions: ["7z"],
          },
          {
            name: "RAR Archives (*.rar)",
            extensions: ["rar"],
          },
          {
            name: "TAR Archives (*.tar, *.gz, *.bz2, *.xz, *.zst)",
            extensions: ["tar", "gz", "tgz", "bz2", "xz", "zst"],
          },
          {
            name: "ISO & Cabinet (*.iso, *.cab)",
            extensions: ["iso", "cab"],
          },
          {
            name: "All Files (*.*)",
            extensions: ["*"],
          },
        ],
      });
      if (selected && typeof selected === "string") {
        selectArchive(selected);
      }
    } catch (err) {
      console.error("Archive selection error:", err);
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

  const handleSelectCompressDestination = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Output Destination Folder",
        defaultPath: compressDestDir || undefined,
      });
      if (selected && typeof selected === "string") {
        setCompressDestDir(selected);
      }
    } catch (err) {
      console.error("Compress destination folder selection error:", err);
    }
  };

  const [isSyncingRegistry, setIsSyncingRegistry] = useState(false);

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

  const handleOpenSettings = async () => {
    try {
      await invoke("open_settings_window");
    } catch (err) {
      console.warn("Failed to open native settings window, falling back to modal:", err);
      setIsSettingsOpen(true);
    }
  };

  const handleCancelCompress = async () => {
    try {
      setIsCancellingCompress(true);
      await invoke("cancel_compression");
    } catch (e) {
      console.error("Failed to cancel compression:", e);
    }
  };

  const handleCompress = async () => {
    if (selectedFiles.length === 0) {
      await handleSelectFiles();
      return;
    }

    if (selectedEncryption !== "none" && !compressPassword.trim()) {
      setCompressError("Please enter an archive password to enable encryption.");
      return;
    }

    const targetArchiveName = customArchiveName.trim() ||
      (selectedFiles.length === 1 ? selectedFiles[0].split(/[/\\]/).pop() || "archive" : "archive");

    setActiveCompressConfig({
      format: selectedFormat,
      algorithm: selectedAlgorithm,
      sliderValue,
      archiveName: targetArchiveName,
    });
    setIsCompressing(true);
    setIsCancellingCompress(false);
    setCompressError(null);
    setProgress({
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
              custom_name: undefined, // Each file uses its own name
              encryption: selectedEncryption,
              password: selectedEncryption !== "none" && compressPassword.trim() ? compressPassword.trim() : undefined,
              output_dir: !archiveToOriginalPath && compressDestDir.trim() ? compressDestDir.trim() : undefined,
              smart_bypass: smartMediaBypass,
              excluded_paths: excludedCompressPaths.size > 0 ? Array.from(excludedCompressPaths) : undefined,
            },
          });
          totalOrig += res.original_size;
          totalComp += res.compressed_size;
          totalElapsed += res.elapsed_ms;
          lastRes = res;
        }

        if (lastRes) {
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
      }
    } catch (err) {
      const errStr = String(err);
      if (errStr.toLowerCase().includes("cancel")) {
        console.log("Compression cancelled by user.");
        setCompressError(null);
      } else {
        console.error("Compression execution error:", err);
        setCompressError(errStr);
      }
    } finally {
      setIsCompressing(false);
      setIsCancellingCompress(false);
      setActiveCompressConfig(null);
      setProgress(null);
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

  const handleExtract = async (options?: {
    destination?: string;
    extractToSubfolder?: boolean;
    openFolderOnExtract?: boolean;
    password?: string;
  }) => {
    if (!selectedArchive) {
      await handleSelectArchive();
      return;
    }

    // Resolve parameters from WorkspaceExtractModal options or fallback to component state (V-INT-01)
    const effectivePassword = options?.password !== undefined ? options.password : extractPassword;
    const effectiveDest = options?.destination !== undefined && options.destination.trim() !== ""
      ? options.destination
      : extractDest;
    const effectiveSubfolder = options?.extractToSubfolder !== undefined
      ? options.extractToSubfolder
      : extractToSubfolder;
    const effectiveOpenFolder = options?.openFolderOnExtract !== undefined
      ? options.openFolderOnExtract
      : openFolderOnExtract;

    // Synchronize parent state with options passed from modal
    if (options?.destination && options.destination.trim() !== "") setExtractDest(options.destination);
    if (options?.password !== undefined) setExtractPassword(options.password);
    if (options?.extractToSubfolder !== undefined) setExtractToSubfolder(options.extractToSubfolder);
    if (options?.openFolderOnExtract !== undefined) setOpenFolderOnExtract(options.openFolderOnExtract);

    if (archiveInfo?.is_encrypted && !effectivePassword.trim()) {
      setExtractError("Password required: This archive is encrypted. Please enter the password to extract.");
      setIsPasswordPromptOpen(true);
      return;
    }

    setIsExtracting(true);
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
        // Check if all files in the archive already share a single common top-level root folder
        const entries = archiveInfo?.entries || [];
        let hasSingleRoot = false;
        if (entries.length > 0) {
          let rootPrefix: string | null = null;
          let isSingle = true;
          for (const e of entries) {
            const clean = e.name.replace(/\\/g, "/").replace(/^\.?\/+/, "").trim();
            if (!clean || clean === ".") continue;
            if (!clean.includes("/") && !e.is_dir) {
              isSingle = false;
              break;
            }
            const firstSegment = clean.split("/")[0];
            if (!firstSegment) continue;
            if (rootPrefix === null) {
              rootPrefix = firstSegment;
            } else if (rootPrefix.toLowerCase() !== firstSegment.toLowerCase()) {
              isSingle = false;
              break;
            }
          }
          if (isSingle && rootPrefix !== null) {
            hasSingleRoot = true;
          }
        }

        // Only append subfolder if the archive does not already contain a single root folder
        if (!hasSingleRoot) {
          const fallbackName = selectedArchive ? selectedArchive.split(/[/\\]/).pop() || "archive" : "archive";
          const stem = getArchiveStemName(archiveInfo?.filename || fallbackName);
          const cleanDest = effectiveDest.replace(/[/\\]+$/, "");
          finalDest = `${cleanDest}/${stem}`;
        }
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

      if (effectiveOpenFolder && res.output_directory) {
        revealItemInDir(res.output_directory).catch(() => { });
      }
    } catch (err) {
      const errMsg = String(err);
      if (errMsg.toLowerCase().includes("cancel")) {
        console.log("Extraction cancelled by user.");
        setExtractError(null);
      } else {
        console.error("Extraction execution error:", err);
        setExtractError(errMsg);
        if (errMsg.toLowerCase().includes("password")) {
          setIsPasswordPromptOpen(true);
        }
      }
    } finally {
      setIsExtracting(false);
      setIsCancellingExtract(false);
      setExtractProgress(null);
    }
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return (bytes / Math.pow(k, i)).toFixed(2) + " " + sizes[i];
  };


  return (
    <div className="h-screen w-screen bg-[#202020] text-white flex flex-col font-sans select-none overflow-hidden">
      {/* Windows 11 Native Custom Titlebar */}
      <Titlebar />

      {/* Main Scrollable Body */}
      <div className="flex-1 flex flex-col p-4 sm:p-6 overflow-y-auto custom-scrollbar">
        <div className="w-full max-w-2xl lg:max-w-4xl xl:max-w-5xl mx-auto space-y-6 transition-all duration-200">
          {/* Header & Mode Switcher */}
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-3">
              {/* Segmented Mode Control */}
              <div className="flex items-center bg-[#1A1A1A] p-1 rounded-lg border border-[#333333]">
                <button
                  type="button"
                  onClick={() => {
                    setActiveMode("compress");
                    setExtractResult(null);
                  }}
                  className={`flex items-center space-x-2 px-3.5 py-1.5 rounded-md text-xs font-medium transition-all cursor-pointer ${activeMode === "compress"
                      ? "bg-[#2D2D2D] text-white shadow-sm border border-[#3E3E3E]"
                      : "text-[#888888] hover:text-white"
                    }`}
                  title={isCompressing ? "Compression in progress" : undefined}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-[#60CDFF]">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                  </svg>
                  <span>Compress</span>
                  {isCompressing && activeMode !== "compress" && (
                    <span className="inline-flex items-center space-x-1 ml-1 px-1.5 py-0.5 rounded-full bg-[#60CDFF]/15 text-[#60CDFF] text-[10px] font-mono border border-[#60CDFF]/30">
                      <span className="w-1.5 h-1.5 rounded-full bg-[#60CDFF] animate-pulse"></span>
                      <span>{progress ? `${Math.round(progress.percent)}%` : "..."}</span>
                    </span>
                  )}
                </button>
                <button
                  type="button"
                  onClick={() => setActiveMode("extract")}
                  className={`flex items-center space-x-2 px-3.5 py-1.5 rounded-md text-xs font-medium transition-all cursor-pointer ${activeMode === "extract"
                      ? "bg-[#2D2D2D] text-white shadow-sm border border-[#3E3E3E]"
                      : "text-[#888888] hover:text-white"
                    }`}
                  title={isExtracting ? "Extraction in progress" : undefined}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5 text-[#60CDFF]">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" />
                  </svg>
                  <span>Extract</span>
                  {isExtracting && activeMode !== "extract" && (
                    <span className="inline-flex items-center space-x-1 ml-1 px-1.5 py-0.5 rounded-full bg-[#60CDFF]/15 text-[#60CDFF] text-[10px] font-mono border border-[#60CDFF]/30">
                      <span className="w-1.5 h-1.5 rounded-full bg-[#60CDFF] animate-pulse"></span>
                      <span>{extractProgress ? `${Math.round(extractProgress.percent)}%` : "..."}</span>
                    </span>
                  )}
                </button>
              </div>

              <span className="text-xl font-semibold tracking-tight text-white hidden md:inline">
                {activeMode === "compress" ? "Create Archive" : "Extract Archive"}
              </span>
            </div>

            <div className="flex items-center space-x-2">
              <button
                type="button"
                onClick={() => setIsWorkspaceOpen(true)}
                className="flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-semibold 
                  bg-gradient-to-r from-[#0E4D6F] to-[#1A3A50] hover:from-[#155B82] hover:to-[#224A62]
                  text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/40 hover:border-[#60CDFF]/70
                  shadow-[0_0_12px_rgba(96,205,255,0.15)] hover:shadow-[0_0_20px_rgba(96,205,255,0.3)]
                  transition-all cursor-pointer"
                title="Open full workspace file manager"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-3.5 h-3.5">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
                </svg>
                <span>Workspace</span>
              </button>

              {activeMode === "compress" && (
                <button
                  type="button"
                  onClick={() => setIsAdvanced(!isAdvanced)}
                  className="text-xs font-medium px-3 py-1.5 rounded-md bg-[#252525] hover:bg-[#303030] border border-[#3E3E3E] hover:border-[#555] transition-colors cursor-pointer text-[#C8C8C8] hover:text-white"
                >
                  <span className="hidden sm:inline">{isAdvanced ? "Simple Mode" : "Advanced"}</span>
                  <span className="sm:hidden">{isAdvanced ? "Simple" : "Adv"}</span>
                </button>
              )}
              <button
                type="button"
                onClick={() => setIsAboutOpen(true)}
                className="text-xs font-medium px-2.5 py-1.5 rounded-md text-[#A8A8A8] hover:text-white hover:bg-[#252525] transition-colors cursor-pointer hidden sm:block"
              >
                About
              </button>
              <button
                type="button"
                onClick={handleOpenSettings}
                className="p-1.5 text-[#A8A8A8] hover:text-[#60CDFF] transition-colors cursor-pointer group"
                title="Settings"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={2}
                  className="w-4 h-4 transition-transform duration-500 ease-out group-hover:rotate-90"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                  <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
              </button>
            </div>
          </div>

          {/* ======================================================== */}
          {/* COMPRESS MODE CONTENT                                     */}
          {/* ======================================================== */}
          {activeMode === "compress" && (
            <CompressPage
              selectedFiles={selectedFiles}
              setSelectedFiles={setSelectedFiles}
              itemsInfo={itemsInfo}
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
              compressDestDir={compressDestDir}
              setCompressDestDir={setCompressDestDir}
              separateArchives={separateArchives}
              setSeparateArchives={setSeparateArchives}
              smartMediaBypass={smartMediaBypass}
              setSmartMediaBypass={handleUpdateSmartMediaBypass}
              isAdvanced={isAdvanced}
              setIsAdvanced={setIsAdvanced}
              sliderValue={sliderValue}
              setSliderValue={setSliderValue}
              specs={specs}
              isCompressing={isCompressing}
              isCancellingCompress={isCancellingCompress}
              progress={progress}
              compressError={compressError}
              activeCompressConfig={activeCompressConfig}
              handleSelectFiles={handleSelectFiles}
              handleSelectFolder={handleSelectFolder}
              handleSelectCompressDestination={handleSelectCompressDestination}
              handleCompress={handleCompress}
              handleCancelCompress={handleCancelCompress}
              compressViewMode={compressViewMode}
              setCompressViewMode={setCompressViewMode}
              stagedEntries={stagedEntries}
              isInspectingStaged={isInspectingStaged}
              excludedCompressPaths={excludedCompressPaths}
              onToggleCompressExclude={handleToggleCompressExclude}
              onCompressIncludeAll={handleCompressIncludeAll}
              onPreviewStagedEntry={handlePreviewStagedEntry}
              isWorkspaceOpen={isWorkspaceOpen}
              setIsWorkspaceOpen={setIsWorkspaceOpen}
              onSwitchMode={(m) => setActiveMode(m)}
              onSelectArchive={handleSelectArchive}
              onLoadArchiveFromFile={selectArchive}
            />
          )}

          {/* ======================================================== */}
          {/* EXTRACT MODE CONTENT                                      */}
          {/* ======================================================== */}
          {activeMode === "extract" && (
            <ExtractPage
              selectedArchive={selectedArchive}
              archiveInfo={archiveInfo}
              isInspectingArchive={isInspectingArchive}
              extractDest={extractDest}
              setExtractDest={setExtractDest}
              extractToSubfolder={extractToSubfolder}
              setExtractToSubfolder={setExtractToSubfolder}
              openFolderOnExtract={openFolderOnExtract}
              setOpenFolderOnExtract={setOpenFolderOnExtract}
              extractPassword={extractPassword}
              setExtractPassword={setExtractPassword}
              showExtractPassword={showExtractPassword}
              setShowExtractPassword={setShowExtractPassword}
              isExtracting={isExtracting}
              isCancellingExtract={isCancellingExtract}
              extractProgress={extractProgress}
              extractError={extractError}
              excludedPaths={excludedPaths}
              includedFilesCount={includedFilesCount}
              handleToggleExclude={handleToggleExclude}
              handleIncludeAll={handleIncludeAll}
              handleSelectArchive={handleSelectArchive}
              handleSelectDestFolder={handleSelectDestFolder}
              handleExtract={handleExtract}
              handleCancelExtract={handleCancelExtract}
              onClearArchive={() => {
                setSelectedArchive(null);
                setArchiveInfo(null);
                setExtractResult(null);
              }}
              selectArchive={selectArchive}
              handlePreviewEntry={handlePreviewEntry}
              isWorkspaceOpen={isWorkspaceOpen}
              setIsWorkspaceOpen={setIsWorkspaceOpen}
              onSwitchMode={(m) => setActiveMode(m)}
            />
          )}
        </div>
      </div>

      {/* Result Modals (Compression & Extraction) */}
      <ResultModals
        compressResult={compressResult}
        onCloseCompressResult={() => {
          setCompressResult(null);
          setSelectedFiles([]);
          setCustomArchiveName("");
          setStagedEntries([]);
          setExcludedCompressPaths(new Set());
        }}
        extractResult={extractResult}
        archiveInfo={archiveInfo}
        onCloseExtractResult={() => {
          setExtractResult(null);
          setSelectedArchive(null);
          setArchiveInfo(null);
        }}
        formatBytes={formatBytes}
      />

      {/* About Modal */}
      <AboutModal
        isOpen={isAboutOpen}
        onClose={() => setIsAboutOpen(false)}
      />

      {/* Settings Modal (Draggable Floating Window) */}
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        isSyncingRegistry={isSyncingRegistry}
        contextMenuStatus={contextMenuStatus}
        onRegisterContextMenu={handleRegisterContextMenu}
      />

      {/* Password Prompt Modal for Encrypted Archives */}
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

      {/* Drag & Drop Visual Indicator Overlay with Blinking Watermark */}
      {isDraggingOver && (
        <div className="fixed inset-0 z-[100] pointer-events-none bg-[#059669]/10 backdrop-blur-[3px] border-2 border-dashed border-[#10B981] m-2 rounded-2xl flex flex-col items-center justify-center space-y-4">
          {/* Semi-transparent blinking DROP watermark */}
          <div className="text-7xl sm:text-9xl font-black tracking-widest text-[#10B981]/40 uppercase animate-pulse select-none drop-shadow-[0_0_35px_rgba(16,185,129,0.6)]">
            DROP
          </div>
          <div className="w-16 h-16 rounded-2xl bg-[#1A2530]/95 border border-[#10B981]/60 flex items-center justify-center shadow-2xl text-[#10B981]">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-8 h-8 animate-bounce">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M9 19l3 3m0 0l3-3m-3 3V10" />
            </svg>
          </div>
          <p className="text-white font-semibold text-xs tracking-wide bg-[#202020]/95 px-5 py-2 rounded-full border border-[#3E3E3E] shadow-2xl">
            Drop files or folders here to {activeMode === "extract" ? "extract" : "compress"}
          </p>
        </div>
      )}
    </div>
  );
}

export default App;

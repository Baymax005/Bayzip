import React, { useState, useMemo, useEffect, useRef } from "react";
import { VirtualScrollList } from "./VirtualScrollList";
import { formatBytes } from "../utils/format";
import { normalizeArchiveEntryPath } from "../utils/archive";
import { ArchiveInfo, ArchiveEntryItem, OperationProgressInfo } from "../types";
import { FileFormatIcon, prefetchFileIcons } from "./FileFormatIcon";

export { normalizeArchiveEntryPath };

export interface StreamProgressTelemetry {
  type: "compress" | "extract" | "test" | "indexing";
  fileName: string;
  percent: number;
  throughputText?: string;
  isCancelling?: boolean;
  onCancel?: () => void;
  onRestoreModal?: () => void;
  onDetachToDesktop?: () => void;
}

export interface ArchiveTreeInspectorProps {
  archiveInfo: ArchiveInfo;
  selectedArchive: string;
  extractPassword?: string;
  isLoading?: boolean;
  onUnlockPreview?: () => void;
  onPreviewEntry: (entryName: string) => void;
  excludedPaths?: Set<string>;
  onToggleExclude?: (path: string, isDir?: boolean) => void;
  onIncludeAll?: () => void;
  isExpandedModal?: boolean;
  onCloseExpandedModal?: () => void;
  onOpenExpandedModal?: () => void;
  sidebar?: React.ReactNode;
  sidebarTitle?: string;
  inspectorTitle?: string;
  operationProgress?: OperationProgressInfo | null;
  onStageItem?: (path: string) => void;
  mode?: "compress" | "extract";
  onSwitchMode?: (newMode: "compress" | "extract") => void;
  stagedItems?: string[];
  itemsInfo?: Record<string, { size: number; is_dir: boolean }>;
  onUnstageItem?: (path: string) => void;
  onClearStagedItems?: () => void;
  onSelectArchive?: () => void;
  onLoadArchiveFromFile?: (path: string) => void;
  extractDest?: string;
  onExtractDestChange?: (dest: string) => void;
  extractToSubfolder?: boolean;
  onToggleExtractToSubfolder?: (val: boolean) => void;
  openFolderOnExtract?: boolean;
  onToggleOpenFolderOnExtract?: (val: boolean) => void;
  onExtractArchive?: (options: {
    destination: string;
    extractToSubfolder: boolean;
    openFolderOnExtract: boolean;
    password?: string;
  }) => Promise<void> | void;
  emptyState?: React.ReactNode;
  testStatus?: "idle" | "testing" | "verified" | "error";
  testMessage?: string | null;
  streamProgress?: StreamProgressTelemetry | null;
}

export interface TreeNode {
  path: string;
  name: string;
  isDir: boolean;
  size: number;
  directFileCount: number;
  totalItemCount: number;
  subfolders: TreeNode[];
  files: ArchiveEntryItem[];
}

type SortKey = "name" | "size";
type SortOrder = "asc" | "desc";

export const ArchiveTreeInspector: React.FC<ArchiveTreeInspectorProps> = ({
  archiveInfo,
  selectedArchive,
  extractPassword = "",
  isLoading = false,
  onUnlockPreview,
  onPreviewEntry,
  excludedPaths = new Set(),
  onToggleExclude,
  onIncludeAll,
  inspectorTitle,
  mode = "extract",
  stagedItems,
  emptyState,
  testStatus = "idle",
  testMessage,
  streamProgress = null,
}) => {
  const [currentFolderPath, setCurrentFolderPath] = useState<string>("");
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [selectedEntryName, setSelectedEntryName] = useState<string | null>(null);
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortOrder, setSortOrder] = useState<SortOrder>("asc");
  const containerRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  // Folder history navigation (Back / Forward)
  const [history, setHistory] = useState<string[]>([""]);
  const [historyIndex, setHistoryIndex] = useState<number>(0);

  const localEntries = useMemo<ArchiveEntryItem[]>(() => {
    if (mode === "compress" && (!stagedItems || stagedItems.length === 0)) {
      return [];
    }
    return archiveInfo.entries || [];
  }, [archiveInfo.entries, stagedItems, mode]);

  // Reset folder navigation when archive path changes
  useEffect(() => {
    setCurrentFolderPath("");
    setHistory([""]);
    setHistoryIndex(0);
    setSearchQuery("");
    setSelectedEntryName(null);
  }, [archiveInfo.path, selectedArchive]);

  // Batch prefetch icons for entries
  useEffect(() => {
    if (localEntries.length > 0) {
      prefetchFileIcons(localEntries.slice(0, 300).map((e) => e.name));
    }
  }, [localEntries]);

  // Exclusion check helpers
  const isFileExcluded = (filePath: string): boolean => {
    if (!excludedPaths || excludedPaths.size === 0) return false;
    const clean = normalizeArchiveEntryPath(filePath).replace(/\/+$/, "");
    if (excludedPaths.has(clean)) return true;
    for (const exc of excludedPaths) {
      const excClean = exc.replace(/\/+$/, "");
      if (clean.startsWith(excClean + "/")) return true;
    }
    return false;
  };

  const isDirExcluded = (dirPath: string): boolean => {
    if (!excludedPaths || excludedPaths.size === 0) return false;
    const cleanDir = normalizeArchiveEntryPath(dirPath).replace(/\/+$/, "");
    if (excludedPaths.has(cleanDir)) return true;
    for (const exc of excludedPaths) {
      const excClean = exc.replace(/\/+$/, "");
      if (cleanDir.startsWith(excClean + "/")) return true;
    }
    return false;
  };

  const excludedCount =
    mode === "compress" && (!stagedItems || stagedItems.length === 0) ? 0 : excludedPaths.size;

  const excludedBytes = useMemo(() => {
    if (!localEntries || excludedPaths.size === 0) return 0;
    return localEntries
      .filter((e) => !e.is_dir && isFileExcluded(e.name))
      .reduce((acc, curr) => acc + (curr.size || 0), 0);
  }, [localEntries, excludedPaths]);

  // Build hierarchical folder tree
  const treeRoot = useMemo<TreeNode>(() => {
    const defaultRootName = mode === "compress" ? "Staged Items" : "Archive Root";
    const rootName =
      archiveInfo.filename && archiveInfo.filename !== "Archive"
        ? archiveInfo.filename
        : defaultRootName;

    const root: TreeNode = {
      path: "",
      name: rootName,
      isDir: true,
      size: 0,
      directFileCount: 0,
      totalItemCount: 0,
      subfolders: [],
      files: [],
    };

    if (!localEntries || localEntries.length === 0) {
      return root;
    }

    const dirMap = new Map<string, TreeNode>();
    dirMap.set("", root);

    const ensureFolder = (folderPath: string): TreeNode => {
      if (dirMap.has(folderPath)) {
        return dirMap.get(folderPath)!;
      }
      const parts = folderPath.split("/");
      const folderName = parts[parts.length - 1];
      const parentPath = parts.slice(0, -1).join("/");
      const parentNode = ensureFolder(parentPath);

      const newNode: TreeNode = {
        path: folderPath,
        name: folderName,
        isDir: true,
        size: 0,
        directFileCount: 0,
        totalItemCount: 0,
        subfolders: [],
        files: [],
      };
      parentNode.subfolders.push(newNode);
      dirMap.set(folderPath, newNode);
      return newNode;
    };

    for (const entry of localEntries) {
      const clean = entry.name.replace(/\\/g, "/").trim().replace(/^\.?\/+/, "");
      if (!clean || clean === ".") continue;

      if (entry.is_dir || clean.endsWith("/")) {
        const folderPath = clean.replace(/\/+$/, "");
        if (folderPath) {
          ensureFolder(folderPath);
        }
      } else {
        const lastSlash = clean.lastIndexOf("/");
        const parentPath = lastSlash === -1 ? "" : clean.substring(0, lastSlash);
        const parentNode = ensureFolder(parentPath);
        parentNode.files.push({
          ...entry,
          name: clean,
        });
        parentNode.directFileCount += 1;
      }
    }

    const computeStats = (node: TreeNode): { size: number; totalCount: number } => {
      node.subfolders.sort((a, b) => a.name.localeCompare(b.name));
      node.files.sort((a, b) => {
        const leafA = a.name.includes("/") ? a.name.slice(a.name.lastIndexOf("/") + 1) : a.name;
        const leafB = b.name.includes("/") ? b.name.slice(b.name.lastIndexOf("/") + 1) : b.name;
        return leafA.localeCompare(leafB);
      });

      let totalSize = node.files.reduce((acc, f) => acc + (f.size || 0), 0);
      let count = node.files.length;

      for (const sub of node.subfolders) {
        const subStat = computeStats(sub);
        totalSize += subStat.size;
        count += subStat.totalCount;
      }

      node.size = totalSize;
      node.totalItemCount = count;
      return { size: totalSize, totalCount: count };
    };

    computeStats(root);
    return root;
  }, [archiveInfo.filename, localEntries, mode]);

  const getNodeByPath = (folderPath: string): TreeNode => {
    if (!folderPath) return treeRoot;
    const parts = folderPath.split("/").filter(Boolean);
    let cur = treeRoot;
    for (const part of parts) {
      const found = cur.subfolders.find((s) => s.name === part);
      if (found) {
        cur = found;
      } else {
        return treeRoot;
      }
    }
    return cur;
  };

  const currentFolderNode = useMemo<TreeNode>(() => {
    return getNodeByPath(currentFolderPath);
  }, [treeRoot, currentFolderPath]);

  // Navigation handlers
  const navigateToFolder = (folderPath: string, pushHistory = true) => {
    setCurrentFolderPath(folderPath);
    if (pushHistory) {
      setHistory((prev) => {
        const next = prev.slice(0, historyIndex + 1);
        next.push(folderPath);
        return next;
      });
      setHistoryIndex((prev) => prev + 1);
    }
  };

  const navigateUp = () => {
    if (!currentFolderPath) return;
    const parts = currentFolderPath.split("/").filter(Boolean);
    parts.pop();
    navigateToFolder(parts.join("/"));
  };

  const handleBack = () => {
    if (historyIndex > 0) {
      const prevFolder = history[historyIndex - 1];
      setHistoryIndex((prev) => prev - 1);
      setCurrentFolderPath(prevFolder);
    }
  };

  const handleForward = () => {
    if (historyIndex < history.length - 1) {
      const nextFolder = history[historyIndex + 1];
      setHistoryIndex((prev) => prev + 1);
      setCurrentFolderPath(nextFolder);
    }
  };

  // Keyboard shortcuts & ergonomics:
  // - Ctrl+F: Focuses search input (searchRef)
  // - Backspace / Alt+ArrowUp: Navigates to parent directory if currentFolderPath is not root
  // - Enter: If selectedEntryName is a folder in currentFolderNode.subfolders, navigates into it
  // - Space: Preview selected entry
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+F / Cmd+F: Global shortcut to jump to search filter
      if ((e.ctrlKey || e.metaKey) && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }

      const target = e.target as HTMLElement | null;
      if (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA") return;

      // Backspace or Alt+ArrowUp: navigate to parent directory if not root
      if (e.key === "Backspace" || (e.altKey && e.key === "ArrowUp")) {
        if (currentFolderPath !== "") {
          e.preventDefault();
          navigateUp();
          return;
        }
      }

      // Enter: if selectedEntryName is a folder in currentFolderNode.subfolders, navigate into it
      if (e.key === "Enter" && selectedEntryName) {
        const targetFolder = currentFolderNode.subfolders.find(
          (f) => f.path === selectedEntryName || f.name === selectedEntryName
        );
        if (targetFolder) {
          e.preventDefault();
          setSelectedEntryName(null);
          navigateToFolder(targetFolder.path);
          return;
        }
      }

      // Space: preview selected entry
      if (e.key === " " && selectedEntryName) {
        e.preventDefault();
        onPreviewEntry(selectedEntryName);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedEntryName, onPreviewEntry, currentFolderPath, currentFolderNode, historyIndex]);

  // Breadcrumbs representation
  const breadcrumbSegments = useMemo(() => {
    if (!currentFolderPath) return [];
    const parts = currentFolderPath.split("/").filter(Boolean);
    const segs: { name: string; path: string }[] = [];
    let acc = "";
    for (const part of parts) {
      acc = acc ? `${acc}/${part}` : part;
      segs.push({ name: part, path: acc });
    }
    return segs;
  }, [currentFolderPath]);

  // Filter & sort files
  type DisplayItem =
    | { kind: "folder"; data: TreeNode }
    | { kind: "file"; data: ArchiveEntryItem };

  const displayItems = useMemo<DisplayItem[]>(() => {
    let subfolders = [...currentFolderNode.subfolders];
    let files = [...currentFolderNode.files];

    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      subfolders = subfolders.filter((s) => s.name.toLowerCase().includes(q));
      files = files.filter((f) => f.name.toLowerCase().includes(q));
    }

    // Sort folders
    subfolders.sort((a, b) => {
      const mult = sortOrder === "asc" ? 1 : -1;
      if (sortKey === "size") return (a.size - b.size) * mult;
      return a.name.localeCompare(b.name) * mult;
    });

    // Sort files
    files.sort((a, b) => {
      const mult = sortOrder === "asc" ? 1 : -1;
      const leafA = a.name.includes("/") ? a.name.slice(a.name.lastIndexOf("/") + 1) : a.name;
      const leafB = b.name.includes("/") ? b.name.slice(b.name.lastIndexOf("/") + 1) : b.name;

      if (sortKey === "size") return ((a.size || 0) - (b.size || 0)) * mult;
      return leafA.localeCompare(leafB) * mult;
    });

    return [
      ...subfolders.map((f) => ({ kind: "folder" as const, data: f })),
      ...files.map((f) => ({ kind: "file" as const, data: f })),
    ];
  }, [currentFolderNode, searchQuery, sortKey, sortOrder]);

  const isEncryptedWithoutEntries =
    archiveInfo.is_encrypted &&
    (!archiveInfo.entries || archiveInfo.entries.length === 0);

  // Render Encrypted Banner
  const renderEncryptedBanner = () => (
    <div className="p-8 text-center space-y-3 bg-[#1A1A1A]">
      <div className="w-12 h-12 rounded-full bg-amber-500/10 text-amber-400 mx-auto flex items-center justify-center">
        <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
          />
        </svg>
      </div>
      <p className="text-sm font-semibold text-white">This archive is password-protected</p>
      <p className="text-xs text-[#888888] max-w-sm mx-auto">
        Enter the archive password below and unlock to inspect directory trees and preview files in memory.
      </p>
      {extractPassword.trim() && onUnlockPreview && (
        <button
          type="button"
          onClick={onUnlockPreview}
          className="mt-2 text-xs px-4 py-2 bg-[#25394A] hover:bg-[#2C4459] text-[#60CDFF] font-medium rounded-md border border-[#60CDFF]/40 transition-colors cursor-pointer shadow-sm"
        >
          Unlock Directory Inspector
        </button>
      )}
    </div>
  );

  return (
    <div
      ref={containerRef}
      className="bg-[#1C1C1F] rounded-lg border border-[#2E2E34] overflow-hidden flex flex-col shadow-sm flex-1 min-h-0 select-none"
    >
      {/* Header Bar */}
      <div className="px-3.5 py-2 bg-[#1C1C1F] border-b border-[#2E2E34] flex items-center justify-between gap-2 select-none shrink-0">
        <div className="flex items-center space-x-2.5 min-w-0">
          <span className="w-2 h-2 rounded-full bg-[#38BDF8] shadow-[0_0_8px_rgba(56,189,248,0.7)]" />
          <span className="font-semibold text-xs text-[#F4F4F6] truncate">
            {inspectorTitle || (archiveInfo.filename ? archiveInfo.filename : "Archive Contents")}
          </span>
          <span className="px-2 py-0.5 text-[10px] font-mono bg-[#252529] text-[#9CA3AF] border border-[#323238] rounded-full">
            {archiveInfo.file_count || localEntries.length || 0} items
          </span>
        </div>

        {/* Right Header: Include All Button & Search Bar */}
        <div className="flex items-center space-x-2">
          {excludedCount > 0 && onIncludeAll && (
            <button
              type="button"
              onClick={onIncludeAll}
              className="px-2 py-1 text-[11px] font-semibold bg-[#10B981]/15 hover:bg-[#10B981]/25 text-[#10B981] border border-[#10B981]/30 rounded transition-colors cursor-pointer"
              title="Include all excluded items"
            >
              Include All ({excludedCount})
            </button>
          )}

          {/* Quick Search Input */}
          <div className="relative">
            <input
              ref={searchRef}
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Filter files..."
              className="w-36 sm:w-48 bg-[#252529] text-xs text-[#F4F4F6] pl-7 pr-6 py-1 rounded-[4px] border border-[#323238] focus:border-[#38BDF8] outline-none font-sans placeholder-[#6B7280]"
            />
            <svg
              className="w-3.5 h-3.5 text-[#6B7280] absolute left-2 top-2 pointer-events-none"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                className="absolute right-2 top-1.5 text-[#9CA3AF] hover:text-white text-xs cursor-pointer"
              >
                ✕
              </button>
            )}
          </div>
        </div>
      </div>

      {emptyState && localEntries.length === 0 && !isLoading ? (
        <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
          {emptyState}
        </div>
      ) : isEncryptedWithoutEntries ? (
        renderEncryptedBanner()
      ) : (
        <>
          {/* Breadcrumbs Navigation */}
          <div className="px-3 py-1.5 bg-[#141416] border-b border-[#2E2E34] flex items-center justify-between text-xs font-mono select-none shrink-0">
            <div className="flex items-center space-x-1.5 overflow-x-auto custom-scrollbar py-0.5">
              {/* Back / Forward History */}
              <button
                type="button"
                onClick={handleBack}
                disabled={historyIndex <= 0}
                className={`p-1 rounded transition-colors ${
                  historyIndex > 0 ? "text-[#CCCCCC] hover:bg-[#252529] cursor-pointer" : "text-[#555555] cursor-not-allowed"
                }`}
                title="Back (Alt+Left)"
              >
                ◀
              </button>
              <button
                type="button"
                onClick={handleForward}
                disabled={historyIndex >= history.length - 1}
                className={`p-1 rounded transition-colors ${
                  historyIndex < history.length - 1 ? "text-[#CCCCCC] hover:bg-[#252529] cursor-pointer" : "text-[#555555] cursor-not-allowed"
                }`}
                title="Forward (Alt+Right)"
              >
                ▶
              </button>

              {/* Up Level Button */}
              <button
                type="button"
                onClick={navigateUp}
                disabled={!currentFolderPath}
                className={`p-1 rounded transition-colors ${
                  currentFolderPath ? "text-[#38BDF8] hover:bg-[#252529] cursor-pointer" : "text-[#555555] cursor-not-allowed"
                }`}
                title="Up one folder level"
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 10l7-7m0 0l7 7m-7-7v18" />
                </svg>
              </button>

              {/* Root breadcrumb */}
              <button
                type="button"
                onClick={() => navigateToFolder("")}
                className={`px-1.5 py-0.5 rounded transition-colors cursor-pointer flex items-center space-x-1 ${
                  currentFolderPath === ""
                    ? "bg-[#252529] text-[#38BDF8] border border-[#323238] font-semibold"
                    : "text-[#9CA3AF] hover:text-[#F4F4F6] hover:bg-[#252529]"
                }`}
              >
                <span>📦</span>
                <span>{treeRoot.name}</span>
              </button>

              {/* Segment breadcrumbs */}
              {breadcrumbSegments.map((seg, idx) => (
                <React.Fragment key={seg.path}>
                  <span className="text-[#555555]">/</span>
                  <button
                    type="button"
                    onClick={() => navigateToFolder(seg.path)}
                    className={`px-1.5 py-0.5 rounded transition-colors cursor-pointer ${
                      idx === breadcrumbSegments.length - 1
                        ? "bg-[#252529] text-[#38BDF8] border border-[#323238] font-semibold"
                        : "text-[#9CA3AF] hover:text-[#F4F4F6] hover:bg-[#252529]"
                    }`}
                  >
                    {seg.name}
                  </button>
                </React.Fragment>
              ))}
            </div>

            {/* Sort Controls */}
            <div className="flex items-center space-x-1 text-[11px] text-[#9CA3AF]">
              <span className="hidden sm:inline">Sort:</span>
              <button
                type="button"
                onClick={() => {
                  if (sortKey === "name") setSortOrder((o) => (o === "asc" ? "desc" : "asc"));
                  else {
                    setSortKey("name");
                    setSortOrder("asc");
                  }
                }}
                className={`px-1.5 py-0.5 rounded cursor-pointer ${
                  sortKey === "name" ? "bg-[#252529] text-white border border-[#323238]" : "hover:text-white"
                }`}
              >
                Name {sortKey === "name" ? (sortOrder === "asc" ? "▲" : "▼") : ""}
              </button>
              <button
                type="button"
                onClick={() => {
                  if (sortKey === "size") setSortOrder((o) => (o === "asc" ? "desc" : "asc"));
                  else {
                    setSortKey("size");
                    setSortOrder("desc");
                  }
                }}
                className={`px-1.5 py-0.5 rounded cursor-pointer ${
                  sortKey === "size" ? "bg-[#252529] text-white border border-[#323238]" : "hover:text-white"
                }`}
              >
                Size {sortKey === "size" ? (sortOrder === "asc" ? "▲" : "▼") : ""}
              </button>
            </div>
          </div>

          {/* Virtual File / Folder List Container */}
          <div className="flex-1 min-h-[280px] flex flex-col overflow-hidden relative">
            {isLoading ? (
              <div className="p-8 text-center space-y-3 m-auto">
                <div className="w-6 h-6 border-2 border-[#38BDF8] border-t-transparent rounded-full animate-spin mx-auto" />
                <p className="text-xs text-[#9CA3AF]">Indexing archive directory tree...</p>
              </div>
            ) : displayItems.length === 0 ? (
              <div className="p-8 text-center text-xs text-[#9CA3AF] flex flex-col items-center justify-center space-y-2 flex-1 h-full">
                <span>{searchQuery ? "No matching files in this folder" : "This folder is empty"}</span>
              </div>
            ) : (
              <div className="flex-1 flex flex-col overflow-hidden h-full">
                {/* Parent Directory Row ("..") */}
                {currentFolderPath !== "" && (
                  <div
                    onDoubleClick={navigateUp}
                    style={{ height: "34px" }}
                    className="flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#232327] select-none text-[#9CA3AF] hover:text-[#38BDF8] hover:bg-[#252529] shrink-0"
                    title="Navigate to parent folder"
                  >
                    <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3 pointer-events-none">
                      <span className="text-base font-bold text-[#FBBF24] shrink-0 leading-none">📁</span>
                      <span className="font-mono text-[12px] font-bold text-white group-hover:text-[#38BDF8]">
                        ..
                      </span>
                      <span className="text-[10px] text-[#777777] italic font-sans">[Parent Directory]</span>
                    </div>
                    <div className="flex items-center space-x-2 shrink-0">
                      <span className="text-[10px] text-[#9CA3AF] font-mono">Up ⮥</span>
                    </div>
                  </div>
                )}

                {/* Virtualized Rows */}
                <VirtualScrollList
                  key={`archive-view-${currentFolderPath}-${searchQuery}-${sortKey}-${sortOrder}`}
                  items={displayItems}
                  rowHeight={34}
                  className="flex-1 h-full custom-scrollbar"
                  renderRow={(item) => {
                    if (item.kind === "folder") {
                      const folder = item.data;
                      const isFolderSkipped = isDirExcluded(folder.path);

                      return (
                        <div
                          key={`folder-${folder.path}`}
                          onDoubleClick={() => navigateToFolder(folder.path)}
                          onClick={() => setSelectedEntryName(folder.path)}
                          style={{ height: "34px" }}
                          className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border select-none border-b border-[#232327] ${
                            selectedEntryName === folder.path
                              ? "bg-[#1B2836] border-l-2 border-[#38BDF8]"
                              : isFolderSkipped
                                ? "opacity-50 hover:opacity-85 bg-rose-950/15 border-l-2 border-rose-500/80"
                                : "hover:bg-[#252529]"
                          }`}
                        >
                          <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                            <span className="text-base text-[#FBBF24] shrink-0">📁</span>
                            <span className="font-sans font-medium text-[#F4F4F6] truncate group-hover:text-[#38BDF8]">
                              {folder.name}
                            </span>
                            <span className="text-[10px] text-[#777777] font-mono shrink-0">
                              ({folder.totalItemCount} {folder.totalItemCount === 1 ? "item" : "items"})
                            </span>
                          </div>

                          <div className="flex items-center space-x-3 shrink-0">
                            <span className="text-[11px] font-mono text-[#9CA3AF]">
                              {formatBytes(folder.size)}
                            </span>

                            {onToggleExclude && (
                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  onToggleExclude(folder.path, true);
                                }}
                                className={`w-4 h-4 rounded border flex items-center justify-center transition-colors cursor-pointer ${
                                  isFolderSkipped
                                    ? "bg-rose-950/60 border-rose-500/80 text-rose-300"
                                    : "bg-[#252529] border-[#323238] text-[#38BDF8] hover:border-[#38BDF8]"
                                }`}
                                title={isFolderSkipped ? "Include folder" : "Exclude folder"}
                              >
                                {isFolderSkipped ? "✕" : "✓"}
                              </button>
                            )}
                          </div>
                        </div>
                      );
                    }

                    const file = item.data;
                    const isSkipped = isFileExcluded(file.name);
                    const leafName = file.name.includes("/")
                      ? file.name.slice(file.name.lastIndexOf("/") + 1)
                      : file.name;

                    return (
                      <div
                        key={`file-${file.name}`}
                        onClick={() => setSelectedEntryName(file.name)}
                        onDoubleClick={() => onPreviewEntry(file.name)}
                        style={{ height: "34px" }}
                        className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border select-none border-b border-[#232327] ${
                          selectedEntryName === file.name
                            ? "bg-[#1B2836] border-l-2 border-[#38BDF8]"
                            : isSkipped
                              ? "opacity-50 hover:opacity-85 bg-rose-950/15 border-l-2 border-rose-500/80"
                              : "hover:bg-[#252529]"
                        }`}
                      >
                        <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                          <FileFormatIcon fileName={file.name} className="w-4 h-4 shrink-0" />
                          <span className={`font-sans truncate ${isSkipped ? "line-through text-rose-400/80" : "text-[#F4F4F6] group-hover:text-white"}`}>
                            {leafName}
                          </span>
                        </div>

                        <div className="flex items-center space-x-3 shrink-0">
                          {testStatus === "verified" && (
                            <span className="text-[10px] font-mono text-[#10B981] flex items-center space-x-1 bg-[#10B981]/10 px-1.5 py-0.5 rounded border border-[#10B981]/20">
                              <span>✓</span>
                              <span>OK</span>
                            </span>
                          )}
                          <span className="text-[11px] font-mono text-[#9CA3AF] min-w-[60px] text-right">
                            {formatBytes(file.size || 0)}
                          </span>

                          {/* Quick Preview Button */}
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              onPreviewEntry(file.name);
                            }}
                            className="p-1 rounded text-[#9CA3AF] hover:text-[#38BDF8] hover:bg-[#252529] transition-colors cursor-pointer"
                            title="Preview file contents (Space)"
                          >
                            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                            </svg>
                          </button>

                          {/* Exclude Checkbox */}
                          {onToggleExclude && (
                            <button
                              type="button"
                              onClick={(e) => {
                                e.stopPropagation();
                                onToggleExclude(file.name, false);
                              }}
                              className={`w-4 h-4 rounded border flex items-center justify-center transition-colors cursor-pointer ${
                                isSkipped
                                  ? "bg-rose-950/60 border-rose-500/80 text-rose-300"
                                  : "bg-[#252529] border-[#323238] text-[#38BDF8] hover:border-[#38BDF8]"
                              }`}
                              title={isSkipped ? "Include file" : "Exclude file"}
                            >
                              {isSkipped ? "✕" : "✓"}
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  }}
                />
              </div>
            )}
          </div>
        </>
      )}

      {/* Loading Progress Line */}
      {isLoading && (
        <div className="w-full h-1 bg-[#141416] overflow-hidden shrink-0">
          <div className="h-full bg-gradient-to-r from-[#0284C7] via-[#38BDF8] to-[#7DD3FC] shadow-[0_0_12px_rgba(56,189,248,0.8)] animate-pulse w-full" />
        </div>
      )}

      {/* Compact Status Footer with live 2px stream line & telemetry */}
      <div className="relative bg-[#141416] border-t border-[#2E2E34] text-[11px] font-mono select-none shrink-0">
        {/* 2px #38BDF8 stream progress line along the top edge */}
        {streamProgress && streamProgress.percent >= 0 && (
          <div className="absolute top-0 left-0 right-0 h-[2px] bg-[#252529] overflow-hidden">
            <div
              className="h-full bg-[#38BDF8] transition-all duration-300"
              style={{ width: `${Math.min(100, Math.max(0, streamProgress.percent))}%` }}
            />
          </div>
        )}

        <div className="px-3.5 py-1.5 flex items-center justify-between text-[#9CA3AF]">
          {/* Left Side: Stream Telemetry / Test Status / Ready */}
          <div className="flex items-center space-x-2 truncate max-w-[540px]">
            {streamProgress ? (
              <div className="flex items-center space-x-2 truncate">
                <span className="text-[#38BDF8] flex items-center space-x-1.5 truncate">
                  <span className="w-1.5 h-1.5 rounded-full bg-[#38BDF8] animate-pulse shrink-0"></span>
                  <span className="font-semibold shrink-0">
                    ⚡ {streamProgress.type === "compress" ? "Streaming to" : streamProgress.type === "extract" ? "Extracting" : streamProgress.type === "indexing" ? "Indexing" : "Verifying stream"}:
                  </span>
                  <span className="text-white truncate max-w-[180px]">{streamProgress.fileName}</span>
                  <span className="text-[#38BDF8] font-bold shrink-0">({Math.round(streamProgress.percent)}%</span>
                  {streamProgress.throughputText && (
                    <span className="text-[#9CA3AF] shrink-0">• {streamProgress.throughputText}</span>
                  )}
                  <span className="text-[#38BDF8] font-bold shrink-0">)</span>
                </span>
                {streamProgress.onRestoreModal && (
                  <button
                    type="button"
                    onClick={streamProgress.onRestoreModal}
                    className="px-1.5 py-0.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#38BDF8] border border-[#323238] text-[10px] cursor-pointer shrink-0 flex items-center space-x-1"
                    title="Restore full progress card"
                  >
                    <span>▴</span>
                    <span>Expand</span>
                  </button>
                )}
                {streamProgress.onDetachToDesktop && (
                  <button
                    type="button"
                    onClick={streamProgress.onDetachToDesktop}
                    className="px-1.5 py-0.5 rounded-[4px] bg-[#252529] hover:bg-[#2E2E34] text-[#9CA3AF] hover:text-[#38BDF8] border border-[#323238] text-[10px] cursor-pointer shrink-0 flex items-center space-x-1"
                    title="Detach progress into a standalone desktop window"
                  >
                    <span>⧉</span>
                    <span>Detach</span>
                  </button>
                )}
                {streamProgress.onCancel && (
                  <button
                    type="button"
                    onClick={streamProgress.onCancel}
                    disabled={streamProgress.isCancelling}
                    className={`px-1.5 py-0.5 rounded-[4px] border text-[10px] cursor-pointer shrink-0 flex items-center space-x-1 ${
                      streamProgress.isCancelling
                        ? "bg-rose-950/60 border-rose-700 text-rose-300 opacity-80 cursor-not-allowed"
                        : "bg-[#252529] hover:bg-rose-950/40 text-[#9CA3AF] hover:text-rose-400 border-[#323238] hover:border-rose-800"
                    }`}
                    title="Cancel active stream"
                  >
                    {streamProgress.isCancelling ? (
                      <>
                        <svg className="animate-spin w-2.5 h-2.5 text-rose-400" viewBox="0 0 24 24" fill="none">
                          <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                          <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z" />
                        </svg>
                        <span>Cancelling...</span>
                      </>
                    ) : (
                      <span>Cancel</span>
                    )}
                  </button>
                )}
              </div>
            ) : testStatus === "verified" ? (
              <span className="text-[#10B981] font-semibold flex items-center space-x-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-[#10B981]"></span>
                <span>{testMessage || "✓ Stream Verified 100% OK (0 CRC errors)"}</span>
              </span>
            ) : testStatus === "testing" ? (
              <span className="text-[#38BDF8] flex items-center space-x-1.5 animate-pulse">
                <span className="w-1.5 h-1.5 rounded-full bg-[#38BDF8]"></span>
                <span>{testMessage || "Verifying stream checksums..."}</span>
              </span>
            ) : testStatus === "error" ? (
              <span className="text-rose-400 font-semibold flex items-center space-x-1.5">
                <span>✕</span>
                <span>{testMessage || "Verification failed"}</span>
              </span>
            ) : (
              <span className="truncate">
                {selectedArchive ? selectedArchive.split(/[/\\]/).pop() : "Ready"}
              </span>
            )}
          </div>

          {/* Right Side: Exclusions & Total Uncompressed Size */}
          <div className="flex items-center space-x-2 shrink-0">
            {excludedCount > 0 && (
              <span className="text-rose-400 font-semibold">
                {excludedCount} excluded ({formatBytes(excludedBytes)} skipped)
              </span>
            )}
            <span className="text-[#F4F4F6]">
              {isLoading
                ? "Indexing tree..."
                : `Uncompressed: ${formatBytes(archiveInfo.total_uncompressed_size || treeRoot.size || 0)}`}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
};

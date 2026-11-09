import React, { useState, useMemo, useEffect, useRef } from "react";
import { VirtualScrollList } from "./VirtualScrollList";
import { formatBytes } from "../utils/format";
import { normalizeArchiveEntryPath } from "../utils/archive";
import { ArchiveInfo, ArchiveEntryItem, OperationProgressInfo } from "../types";
import { FileFormatIcon, prefetchFileIcons } from "./FileFormatIcon";

export { normalizeArchiveEntryPath };

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
}) => {
  const [currentFolderPath, setCurrentFolderPath] = useState<string>("");
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [selectedEntryName, setSelectedEntryName] = useState<string | null>(null);
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortOrder, setSortOrder] = useState<SortOrder>("asc");
  const containerRef = useRef<HTMLDivElement>(null);

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

  // Keyboard shortcut: Space to preview selected entry
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA") return;

      if (e.key === " " && selectedEntryName) {
        e.preventDefault();
        onPreviewEntry(selectedEntryName);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedEntryName, onPreviewEntry]);

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
      className="bg-[#202020] rounded-lg border border-[#3E3E3E] overflow-hidden flex flex-col shadow-sm"
    >
      {/* Header Bar */}
      <div className="px-3.5 py-2.5 bg-[#242424] border-b border-[#333333] flex items-center justify-between gap-2 select-none">
        <div className="flex items-center space-x-2.5 min-w-0">
          <span className="w-2.5 h-2.5 rounded-full bg-[#60CDFF] shadow-[0_0_8px_rgba(96,205,255,0.7)]" />
          <span className="font-semibold text-xs text-white truncate">
            {inspectorTitle || (archiveInfo.filename ? archiveInfo.filename : "Archive Contents")}
          </span>
          <span className="px-2 py-0.5 text-[10px] font-mono bg-[#181818] text-[#888888] border border-[#333333] rounded-full">
            {archiveInfo.file_count || localEntries.length || 0} items
          </span>
        </div>

        {/* Right Header: Include All Button & Search Bar */}
        <div className="flex items-center space-x-2">
          {excludedCount > 0 && onIncludeAll && (
            <button
              type="button"
              onClick={onIncludeAll}
              className="px-2 py-1 text-[11px] font-semibold bg-[#10B981]/20 hover:bg-[#10B981]/30 text-[#10B981] border border-[#10B981]/40 rounded transition-colors cursor-pointer"
              title="Include all excluded items"
            >
              Include All ({excludedCount})
            </button>
          )}

          {/* Quick Search Input */}
          <div className="relative">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Filter files..."
              className="w-36 sm:w-48 bg-[#181818] text-xs text-white pl-7 pr-6 py-1 rounded border border-[#3A3A3A] focus:border-[#60CDFF] outline-none font-sans placeholder-[#666]"
            />
            <svg
              className="w-3.5 h-3.5 text-[#777] absolute left-2 top-2 pointer-events-none"
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
                className="absolute right-2 top-1.5 text-[#888] hover:text-white text-xs cursor-pointer"
              >
                ✕
              </button>
            )}
          </div>
        </div>
      </div>

      {isEncryptedWithoutEntries ? (
        renderEncryptedBanner()
      ) : (
        <>
          {/* Breadcrumbs Navigation */}
          <div className="px-3 py-1.5 bg-[#1C1C1C] border-b border-[#2C2C2C] flex items-center justify-between text-xs font-mono select-none">
            <div className="flex items-center space-x-1.5 overflow-x-auto custom-scrollbar py-0.5">
              {/* Back / Forward History */}
              <button
                type="button"
                onClick={handleBack}
                disabled={historyIndex <= 0}
                className={`p-1 rounded transition-colors ${
                  historyIndex > 0 ? "text-[#CCCCCC] hover:bg-[#333333] cursor-pointer" : "text-[#555555] cursor-not-allowed"
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
                  historyIndex < history.length - 1 ? "text-[#CCCCCC] hover:bg-[#333333] cursor-pointer" : "text-[#555555] cursor-not-allowed"
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
                  currentFolderPath ? "text-[#60CDFF] hover:bg-[#333333] cursor-pointer" : "text-[#555555] cursor-not-allowed"
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
                    ? "bg-[#25394A] text-[#60CDFF] font-semibold"
                    : "text-[#A8A8A8] hover:text-white hover:bg-[#2A2A2A]"
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
                        ? "bg-[#25394A] text-[#60CDFF] font-semibold"
                        : "text-[#A8A8A8] hover:text-white hover:bg-[#2A2A2A]"
                    }`}
                  >
                    {seg.name}
                  </button>
                </React.Fragment>
              ))}
            </div>

            {/* Sort Controls */}
            <div className="flex items-center space-x-1 text-[11px] text-[#888888]">
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
                  sortKey === "name" ? "bg-[#2D2D2D] text-white" : "hover:text-white"
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
                  sortKey === "size" ? "bg-[#2D2D2D] text-white" : "hover:text-white"
                }`}
              >
                Size {sortKey === "size" ? (sortOrder === "asc" ? "▲" : "▼") : ""}
              </button>
            </div>
          </div>

          {/* Virtual File / Folder List Container */}
          <div className="h-72 flex flex-col overflow-hidden relative">
            {isLoading ? (
              <div className="p-8 text-center space-y-3 m-auto">
                <div className="w-6 h-6 border-2 border-[#60CDFF] border-t-transparent rounded-full animate-spin mx-auto" />
                <p className="text-xs text-[#A8A8A8]">Indexing archive directory tree...</p>
              </div>
            ) : displayItems.length === 0 ? (
              <div className="p-8 text-center text-xs text-[#888888] flex flex-col items-center justify-center space-y-2 flex-1 h-full">
                <span>{searchQuery ? "No matching files in this folder" : "This folder is empty"}</span>
              </div>
            ) : (
              <div className="flex-1 flex flex-col overflow-hidden h-full">
                {/* Parent Directory Row ("..") */}
                {currentFolderPath !== "" && (
                  <div
                    onDoubleClick={navigateUp}
                    style={{ height: "34px" }}
                    className="flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#282828] select-none text-[#A8A8A8] hover:text-[#60CDFF] hover:bg-[#252525] shrink-0"
                    title="Navigate to parent folder"
                  >
                    <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3 pointer-events-none">
                      <span className="text-base font-bold text-[#F9D648] shrink-0 leading-none">📁</span>
                      <span className="font-mono text-[12px] font-bold text-white group-hover:text-[#60CDFF]">
                        ..
                      </span>
                      <span className="text-[10px] text-[#777777] italic font-sans">[Parent Directory]</span>
                    </div>
                    <div className="flex items-center space-x-2 shrink-0">
                      <span className="text-[10px] text-[#888888] font-mono">Up ⮥</span>
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
                          className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border select-none ${
                            selectedEntryName === folder.path
                              ? "bg-[#25394A]/80 border-l-2 border-[#60CDFF]"
                              : isFolderSkipped
                                ? "opacity-50 hover:opacity-85 bg-rose-950/15 border-l-2 border-rose-500/80"
                                : "hover:bg-[#252525]"
                          }`}
                        >
                          <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                            <span className="text-base text-[#F9D648] shrink-0">📁</span>
                            <span className="font-sans font-medium text-white truncate group-hover:text-[#60CDFF]">
                              {folder.name}
                            </span>
                            <span className="text-[10px] text-[#777777] font-mono shrink-0">
                              ({folder.totalItemCount} {folder.totalItemCount === 1 ? "item" : "items"})
                            </span>
                          </div>

                          <div className="flex items-center space-x-3 shrink-0">
                            <span className="text-[11px] font-mono text-[#A8A8A8]">
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
                                    : "bg-[#2A2A2A] border-[#444444] text-[#60CDFF] hover:border-[#60CDFF]"
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
                        className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border select-none ${
                          selectedEntryName === file.name
                            ? "bg-[#25394A]/80 border-l-2 border-[#60CDFF]"
                            : isSkipped
                              ? "opacity-50 hover:opacity-85 bg-rose-950/15 border-l-2 border-rose-500/80"
                              : "hover:bg-[#252525]"
                        }`}
                      >
                        <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                          <FileFormatIcon fileName={file.name} className="w-4 h-4 shrink-0" />
                          <span className={`font-sans truncate ${isSkipped ? "line-through text-rose-400/80" : "text-[#E0E0E0] group-hover:text-white"}`}>
                            {leafName}
                          </span>
                        </div>

                        <div className="flex items-center space-x-3 shrink-0">
                          <span className="text-[11px] font-mono text-[#A8A8A8] min-w-[60px] text-right">
                            {formatBytes(file.size || 0)}
                          </span>

                          {/* Quick Preview Button */}
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              onPreviewEntry(file.name);
                            }}
                            className="p-1 rounded text-[#777777] hover:text-[#60CDFF] hover:bg-[#333333] transition-colors cursor-pointer"
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
                                  : "bg-[#2A2A2A] border-[#444444] text-[#60CDFF] hover:border-[#60CDFF]"
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
        <div className="w-full h-1 bg-[#1A1A1A] overflow-hidden">
          <div className="h-full bg-gradient-to-r from-[#059669] via-[#10B981] to-[#34D399] shadow-[0_0_12px_rgba(16,185,129,0.8)] animate-pulse w-full" />
        </div>
      )}

      {/* Compact Status Footer */}
      <div className="px-3.5 py-1.5 bg-[#1C1C1C] border-t border-[#333333] flex items-center justify-between text-[11px] font-mono text-[#888888] select-none">
        <span className="truncate max-w-[240px]">
          {selectedArchive ? selectedArchive.split(/[/\\]/).pop() : "Ready"}
        </span>
        <div className="flex items-center space-x-2">
          {excludedCount > 0 && (
            <span className="text-rose-400 font-semibold">
              {excludedCount} excluded ({formatBytes(excludedBytes)} skipped)
            </span>
          )}
          <span className="text-white">
            {isLoading
              ? "Indexing tree..."
              : `Uncompressed: ${formatBytes(archiveInfo.total_uncompressed_size || treeRoot.size || 0)}`}
          </span>
        </div>
      </div>
    </div>
  );
};

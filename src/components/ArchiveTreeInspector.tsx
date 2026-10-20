import { useState, useMemo, useEffect, useRef } from "react";
import { ArchiveInfo, ArchiveEntryItem } from "../types";
import { FileFormatIcon } from "./FileFormatIcon";

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
}

export function normalizeArchiveEntryPath(p: string): string {
  return p.replace(/\\/g, "/").replace(/^\.?\/+/, "").toLowerCase();
}

export interface TreeNode {
  path: string; // e.g. "src/components" or "" for root
  name: string; // e.g. "components" or archive stem
  isDir: boolean;
  size: number;
  directFileCount: number;
  totalItemCount: number;
  subfolders: TreeNode[];
  files: ArchiveEntryItem[];
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

interface VirtualScrollListProps<T> {
  items: T[];
  rowHeight?: number;
  overscan?: number;
  className?: string;
  renderRow: (item: T, index: number) => React.ReactNode;
}

function VirtualScrollList<T>({
  items,
  rowHeight = 34,
  overscan = 5,
  className = "",
  renderRow,
}: VirtualScrollListProps<T>) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(300);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;

    const onScroll = () => {
      setScrollTop(el.scrollTop);
    };

    const updateHeight = () => {
      if (el) {
        setViewportHeight(el.clientHeight || 300);
      }
    };

    updateHeight();
    el.addEventListener("scroll", onScroll, { passive: true });

    let ro: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver(() => updateHeight());
      ro.observe(el);
    } else {
      window.addEventListener("resize", updateHeight);
    }

    return () => {
      el.removeEventListener("scroll", onScroll);
      if (ro) ro.disconnect();
      else window.removeEventListener("resize", updateHeight);
    };
  }, []);

  const totalHeight = items.length * rowHeight;
  const startIndex = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const endIndex = Math.min(items.length, Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan);
  const visibleItems = items.slice(startIndex, endIndex);
  const offsetY = startIndex * rowHeight;

  return (
    <div
      ref={scrollRef}
      className={`overflow-y-auto custom-scrollbar relative ${className}`}
    >
      <div style={{ height: `${totalHeight}px`, position: "relative", width: "100%" }}>
        <div
          style={{
            transform: `translateY(${offsetY}px)`,
            position: "absolute",
            top: 0,
            left: 0,
            right: 0,
          }}
          className="divide-y divide-[#262626]"
        >
          {visibleItems.map((item, idx) => renderRow(item, startIndex + idx))}
        </div>
      </div>
    </div>
  );
}

export function ArchiveTreeInspector({
  archiveInfo,
  selectedArchive,
  extractPassword = "",
  isLoading = false,
  onUnlockPreview,
  onPreviewEntry,
  excludedPaths = new Set(),
  onToggleExclude,
  onIncludeAll,
  isExpandedModal: isExpandedModalProp,
  onCloseExpandedModal,
  onOpenExpandedModal,
  sidebar,
  sidebarTitle,
}: ArchiveTreeInspectorProps) {
  const [viewMode, setViewMode] = useState<"tree" | "flat">("tree");
  const [internalExpandedModal, setInternalExpandedModal] = useState<boolean>(false);
  const isExpandedModal = isExpandedModalProp !== undefined ? isExpandedModalProp : internalExpandedModal;
  const setIsExpandedModal = (val: boolean) => {
    setInternalExpandedModal(val);
    if (val && onOpenExpandedModal) {
      onOpenExpandedModal();
    } else if (!val && onCloseExpandedModal) {
      onCloseExpandedModal();
    }
  };
  const [currentFolderPath, setCurrentFolderPath] = useState<string>("");
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set([""]));
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [selectedEntryName, setSelectedEntryName] = useState<string | null>(null);
  const [isExclusionMode, setIsExclusionMode] = useState<boolean>(false);

  const excludedCount = excludedPaths.size;

  const isFileExcluded = (filePath: string): boolean => {
    if (!excludedPaths || excludedPaths.size === 0) return false;
    const clean = normalizeArchiveEntryPath(filePath);
    if (excludedPaths.has(clean)) return true;
    for (const exc of Array.from(excludedPaths)) {
      const parentPrefix = exc.endsWith("/") ? exc : exc + "/";
      if (clean.startsWith(parentPrefix)) return true;
    }
    return false;
  };

  const isDirExcluded = (dirPath: string): boolean => {
    if (!excludedPaths || excludedPaths.size === 0) return false;
    const cleanDir = normalizeArchiveEntryPath(dirPath);
    if (excludedPaths.has(cleanDir)) return true;
    for (const exc of Array.from(excludedPaths)) {
      const parentPrefix = exc.endsWith("/") ? exc : exc + "/";
      if (cleanDir.startsWith(parentPrefix)) return true;
    }
    const prefix = cleanDir ? `${cleanDir}/` : "";
    const allEntries = archiveInfo?.entries || [];
    const filesInDir = allEntries.filter((e) => !e.is_dir && normalizeArchiveEntryPath(e.name).startsWith(prefix));
    if (filesInDir.length > 0 && filesInDir.every((f) => isFileExcluded(f.name))) {
      return true;
    }
    return false;
  };

  const excludedBytes = useMemo(() => {
    if (!archiveInfo?.entries || excludedPaths.size === 0) return 0;
    return archiveInfo.entries
      .filter((e) => !e.is_dir && isFileExcluded(e.name))
      .reduce((acc, curr) => acc + (curr.size || 0), 0);
  }, [archiveInfo, excludedPaths]);

  const containerRef = useRef<HTMLDivElement>(null);

  // Close full-screen expand mode on Escape
  useEffect(() => {
    if (!isExpandedModal) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setIsExpandedModal(false);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isExpandedModal]);

  // Spacebar to preview selected entry (safely ignoring text inputs)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === " " && selectedEntryName) {
        const target = e.target as HTMLElement | null;
        const tag = target?.tagName;
        const isEditable = target?.isContentEditable;
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || isEditable) {
          return;
        }
        // Prevent page scroll on Space
        e.preventDefault();
        onPreviewEntry(selectedEntryName);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedEntryName, onPreviewEntry]);

  // Build Hierarchical Tree structure from flat entries
  const treeRoot = useMemo<TreeNode>(() => {
    const root: TreeNode = {
      path: "",
      name: archiveInfo.filename || "Archive Root",
      isDir: true,
      size: 0,
      directFileCount: 0,
      totalItemCount: 0,
      subfolders: [],
      files: [],
    };

    if (!archiveInfo.entries || archiveInfo.entries.length === 0) {
      return root;
    }

    const dirMap = new Map<string, TreeNode>();
    dirMap.set("", root);

    // Helper to ensure parent folder path exists in tree
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

    for (const entry of archiveInfo.entries) {
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

    // Sort subfolders and files alphabetically, and calculate recursive statistics
    const computeStats = (node: TreeNode): { size: number; totalCount: number } => {
      node.subfolders.sort((a, b) => a.name.localeCompare(b.name));
      node.files.sort((a, b) => {
        const nameA = a.name.split("/").pop() || "";
        const nameB = b.name.split("/").pop() || "";
        return nameA.localeCompare(nameB);
      });

      let totalSize = node.files.reduce((acc, f) => acc + f.size, 0);
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
  }, [archiveInfo]);

  // Current folder node based on currentFolderPath
  const currentFolderNode = useMemo<TreeNode>(() => {
    if (!currentFolderPath) return treeRoot;

    const parts = currentFolderPath.split("/");
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
  }, [treeRoot, currentFolderPath]);

  // Breadcrumb parts
  const breadcrumbSegments = useMemo(() => {
    if (!currentFolderPath) return [];
    const parts = currentFolderPath.split("/");
    const segs: { name: string; path: string }[] = [];
    let acc = "";
    for (const part of parts) {
      acc = acc ? `${acc}/${part}` : part;
      segs.push({ name: part, path: acc });
    }
    return segs;
  }, [currentFolderPath]);

  // Toggle folder expansion in tree
  const toggleFolderExpand = (folderPath: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(folderPath)) {
        next.delete(folderPath);
      } else {
        next.add(folderPath);
      }
      return next;
    });
  };

  // Navigate into a folder
  const navigateToFolder = (folderPath: string) => {
    setCurrentFolderPath(folderPath);
    setExpandedFolders((prev) => new Set(prev).add(folderPath));
  };

  // Navigate up one level
  const navigateUp = () => {
    if (!currentFolderPath) return;
    const parts = currentFolderPath.split("/");
    parts.pop();
    setCurrentFolderPath(parts.join("/"));
  };

  // Filtered entries for Flat View
  const filteredFlatEntries = useMemo(() => {
    if (!archiveInfo.entries) return [];
    if (!searchQuery.trim()) return archiveInfo.entries;
    const q = searchQuery.toLowerCase();
    return archiveInfo.entries.filter((entry) => entry.name.toLowerCase().includes(q));
  }, [archiveInfo.entries, searchQuery]);

  // Filtered files in Current Folder for Tree View
  const filteredCurrentFiles = useMemo(() => {
    if (!searchQuery.trim()) return currentFolderNode.files;
    const q = searchQuery.toLowerCase();
    return currentFolderNode.files.filter((f) => f.name.toLowerCase().includes(q));
  }, [currentFolderNode.files, searchQuery]);

  // Recursive Tree Node Renderer for Left Pane
  const renderTreeFolderNode = (node: TreeNode, depth: number = 0) => {
    const isExpanded = expandedFolders.has(node.path);
    const isSelected = currentFolderPath === node.path;
    const hasChildren = node.subfolders.length > 0;
    const isFolderSkipped = isDirExcluded(node.path);

    return (
      <div key={node.path || "root"} className="select-none text-xs">
        <div
          onClick={() => navigateToFolder(node.path)}
          style={{ paddingLeft: `${Math.max(8, depth * 16 + 8)}px` }}
          className={`flex items-center justify-between py-1.5 pr-2 rounded-md transition-colors cursor-pointer group ${
            isSelected
              ? "bg-[#25394A] text-[#60CDFF] font-semibold border-l-2 border-[#60CDFF]"
              : isFolderSkipped
                ? "bg-rose-950/20 text-rose-300/80 hover:bg-rose-950/30"
                : "hover:bg-[#282828] text-[#CCCCCC]"
          }`}
        >
          <div className="flex items-center space-x-1.5 min-w-0 flex-1 mr-2">
            {/* Chevron toggle */}
            {hasChildren ? (
              <button
                type="button"
                onClick={(e) => toggleFolderExpand(node.path, e)}
                className="w-4 h-4 flex items-center justify-center text-[#888888] hover:text-white rounded transition-transform cursor-pointer"
              >
                <svg
                  className={`w-3 h-3 transform transition-transform ${isExpanded ? "rotate-90" : ""}`}
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M9 5l7 7-7 7" />
                </svg>
              </button>
            ) : (
              <span className="w-4" />
            )}

            {/* Folder Icon */}
            <svg
              className={`w-4 h-4 shrink-0 ${isSelected ? "text-[#60CDFF]" : isFolderSkipped ? "text-rose-400" : "text-[#F9D648]"}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d={
                  isExpanded
                    ? "M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z"
                    : "M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                }
              />
            </svg>

            <span
              className={`truncate font-mono text-[11px] ${
                isFolderSkipped ? "text-rose-300 line-through opacity-80" : ""
              }`}
              title={node.name}
            >
              {node.name}
            </span>
          </div>

          {/* Badge */}
          {isFolderSkipped ? (
            <span className="text-[9px] font-semibold text-rose-400 px-1 py-0.2 rounded bg-rose-950/60 border border-rose-800/80 shrink-0">
              ✕
            </span>
          ) : (
            <span className="text-[10px] font-mono text-[#888888] px-1.5 py-0.2 rounded bg-[#2A2A2A] border border-[#3A3A3A] shrink-0">
              {node.totalItemCount}
            </span>
          )}
        </div>

        {/* Render nested subfolders if expanded */}
        {isExpanded && hasChildren && (
          <div className="space-y-0.5 mt-0.5">
            {node.subfolders.map((child) => renderTreeFolderNode(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  // Render File Icon based on category using FileFormatIcon
  const renderFileIcon = (fileName: string) => {
    return <FileFormatIcon fileName={fileName} size="sm" />;
  };

  // Main UI Toolbar Component
  const renderToolbar = (isFullModal: boolean = false) => (
    <div className="px-3.5 py-2.5 bg-[#262626] border-b border-[#3E3E3E] flex flex-wrap items-center justify-between gap-2 shrink-0">
      <div className="flex items-center space-x-2.5">
        <svg className="w-4 h-4 text-[#60CDFF]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
        </svg>
        <span className="text-xs font-semibold text-white tracking-wide">Archive Explorer</span>
        <span className="text-[10px] font-mono bg-[#333333] text-[#A8A8A8] px-2 py-0.5 rounded-full border border-[#444444]">
          {archiveInfo.file_count || 0} item{archiveInfo.file_count !== 1 ? "s" : ""}
        </span>
      </div>

      <div className="flex items-center space-x-2">
        {/* Toggle between Tree View and Flat Table */}
        <div className="bg-[#1C1C1C] p-0.5 rounded-lg border border-[#3E3E3E] flex items-center space-x-1">
          <button
            type="button"
            onClick={() => setViewMode("tree")}
            className={`px-2.5 py-1 text-xs rounded font-medium transition-colors flex items-center space-x-1 cursor-pointer ${
              viewMode === "tree"
                ? "bg-[#333333] text-[#60CDFF] shadow-sm font-semibold"
                : "text-[#888888] hover:text-white"
            }`}
            title="Hierarchical Tree View"
          >
            <span>🗂️</span>
            <span>Tree View</span>
          </button>
          <button
            type="button"
            onClick={() => setViewMode("flat")}
            className={`px-2.5 py-1 text-xs rounded font-medium transition-colors flex items-center space-x-1 cursor-pointer ${
              viewMode === "flat"
                ? "bg-[#333333] text-[#60CDFF] shadow-sm font-semibold"
                : "text-[#888888] hover:text-white"
            }`}
            title="Flat Archive Table"
          >
            <span>📋</span>
            <span>Flat Table</span>
          </button>
        </div>

        {/* Selective Exclusion Mode Button */}
        <button
          type="button"
          onClick={() => setIsExclusionMode(!isExclusionMode)}
          className={`px-2.5 py-1 text-xs rounded-lg font-medium transition-all flex items-center space-x-1.5 cursor-pointer ${
            isExclusionMode
              ? "bg-rose-950/50 text-rose-300 border border-rose-700/80 shadow-[0_0_12px_rgba(244,63,94,0.25)] font-semibold"
              : "bg-[#1C1C1C] hover:bg-[#2A2A2A] text-[#A8A8A8] hover:text-white border border-[#3E3E3E] hover:border-[#555555]"
          }`}
          title={isExclusionMode ? "Exit Exclusion Mode" : "Enable Selective Exclusion: skip unwanted files during extraction"}
        >
          {isExclusionMode && <span className="w-1.5 h-1.5 rounded-full bg-rose-400 animate-pulse"></span>}
          <span className="text-rose-400 font-bold">✕</span>
          <span>{isExclusionMode ? "Exclusion Mode" : "Exclude Files"}</span>
          {excludedCount > 0 && (
            <span className="text-[10px] font-mono bg-rose-900/70 text-white px-1.5 py-0.2 rounded-full border border-rose-600">
              {excludedCount}
            </span>
          )}
        </button>

        {/* Reset / Include All Button */}
        {excludedCount > 0 && onIncludeAll && (
          <button
            type="button"
            onClick={onIncludeAll}
            className="px-2 py-1 text-xs rounded-lg font-medium transition-colors flex items-center space-x-1 bg-[#1C1C1C] hover:bg-[#2A2A2A] text-[#60CDFF] hover:text-[#90DCFF] border border-[#3E3E3E] cursor-pointer"
            title="Include all files (reset exclusions)"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
            <span>Include All</span>
          </button>
        )}

        {/* Expand / Minimize Fullscreen Toggle (⤢) */}
        {!isFullModal ? (
          <button
            type="button"
            onClick={() => setIsExpandedModal(true)}
            className="p-1.5 bg-[#2D2D2D] hover:bg-[#383838] text-[#A8A8A8] hover:text-white rounded border border-[#3E3E3E] transition-colors cursor-pointer"
            title="Expand into full workspace (⤢)"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
            </svg>
          </button>
        ) : (
          <button
            type="button"
            onClick={() => setIsExpandedModal(false)}
            className="px-2.5 py-1 bg-[#2D2D2D] hover:bg-rose-950/40 text-[#A8A8A8] hover:text-rose-300 rounded-md border border-[#3E3E3E] hover:border-rose-700/60 transition-colors cursor-pointer flex items-center space-x-1.5 text-xs font-medium"
            title="Exit Workspace (Esc)"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
            <span>Exit Workspace</span>
          </button>
        )}
      </div>
    </div>
  );

  // Breadcrumb Trail Component
  const renderBreadcrumbs = () => (
    <div className="px-3 py-1.5 bg-[#1C1C1C] border-b border-[#2E2E2E] flex items-center justify-between text-xs font-mono shrink-0">
      <div className="flex items-center space-x-1.5 overflow-x-auto custom-scrollbar py-0.5">
        {/* Up One Level Button */}
        <button
          type="button"
          onClick={navigateUp}
          disabled={!currentFolderPath}
          className={`p-1 rounded transition-colors ${
            currentFolderPath
              ? "text-[#60CDFF] hover:bg-[#333333] cursor-pointer"
              : "text-[#555555] cursor-not-allowed"
          }`}
          title="Up one folder level"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 10l7-7m0 0l7 7m-7-7v18" />
          </svg>
        </button>

        {/* Root archive segment */}
        <button
          type="button"
          onClick={() => navigateToFolder("")}
          className={`px-1.5 py-0.5 rounded transition-colors flex items-center space-x-1 cursor-pointer ${
            currentFolderPath === ""
              ? "bg-[#25394A] text-[#60CDFF] font-semibold"
              : "text-[#A8A8A8] hover:text-white hover:bg-[#2A2A2A]"
          }`}
        >
          <span>📦</span>
          <span>{archiveInfo.filename || "Archive"}</span>
        </button>

        {/* Nested segments */}
        {breadcrumbSegments.map((seg, idx) => (
          <div key={seg.path} className="flex items-center space-x-1.5">
            <span className="text-[#666666]">/</span>
            <button
              type="button"
              onClick={() => navigateToFolder(seg.path)}
              className={`px-1.5 py-0.5 rounded transition-colors flex items-center space-x-1 cursor-pointer ${
                idx === breadcrumbSegments.length - 1
                  ? "bg-[#25394A] text-[#60CDFF] font-semibold"
                  : "text-[#A8A8A8] hover:text-white hover:bg-[#2A2A2A]"
              }`}
            >
              <span>📁</span>
              <span>{seg.name}</span>
            </button>
          </div>
        ))}
      </div>

      <span className="text-[11px] text-[#777777] shrink-0 pl-2">
        {currentFolderNode.subfolders.length} dirs, {currentFolderNode.files.length} files
      </span>
    </div>
  );

  // Search input
  const renderSearchInput = () => (
    <div className="p-2 border-b border-[#2E2E2E] bg-[#1E1E1E] shrink-0">
      <div className="relative">
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder={viewMode === "tree" ? "Filter files in current folder..." : "Search all files in archive..."}
          className="w-full bg-[#161616] text-xs text-white pl-8 pr-7 py-1.5 rounded border border-[#333333] focus:border-[#60CDFF] outline-none font-mono placeholder-[#666]"
        />
        <svg className="w-3.5 h-3.5 text-[#777] absolute left-2.5 top-2" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        {searchQuery && (
          <button
            type="button"
            onClick={() => setSearchQuery("")}
            className="absolute right-2.5 top-1.5 text-[#888] hover:text-white text-xs cursor-pointer"
          >
            ✕
          </button>
        )}
      </div>
    </div>
  );

  // Password unlock banner for encrypted archives
  const renderEncryptedBanner = () => (
    <div className="p-6 text-center space-y-2.5 bg-[#1A1A1A]">
      <div className="w-10 h-10 rounded-full bg-amber-500/10 text-amber-400 mx-auto flex items-center justify-center">
        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
        </svg>
      </div>
      <p className="text-xs font-semibold text-white">This archive is password-protected</p>
      <p className="text-[11px] text-[#888888] max-w-sm mx-auto">
        Enter the archive password below and unlock to inspect directory trees and preview files in memory.
      </p>
      {extractPassword.trim() && onUnlockPreview && (
        <button
          type="button"
          onClick={onUnlockPreview}
          className="mt-2 text-xs px-4 py-1.5 bg-[#2A2A2A] hover:bg-[#333333] text-[#60CDFF] font-medium rounded border border-[#444444] transition-colors cursor-pointer"
        >
          Unlock Directory Inspector
        </button>
      )}
    </div>
  );

  // Right pane / Content table for Current Folder
  const renderCurrentFolderContents = () => {
    if (isLoading) {
      return (
        <div className="p-8 text-center space-y-3">
          <div className="w-5 h-5 border-2 border-[#60CDFF] border-t-transparent rounded-full animate-spin mx-auto"></div>
          <p className="text-xs text-[#A8A8A8]">Indexing archive directory tree...</p>
        </div>
      );
    }

    const hasItems = currentFolderNode.subfolders.length > 0 || filteredCurrentFiles.length > 0;

    if (!hasItems) {
      return (
        <div className="p-8 text-center text-xs text-[#888888]">
          {searchQuery ? "No matching files in this folder" : "This folder is empty"}
        </div>
      );
    }

    type FolderItem =
      | { kind: "folder"; data: TreeNode }
      | { kind: "file"; data: ArchiveEntryItem };

    const folderItems: FolderItem[] = [
      ...currentFolderNode.subfolders.map((node) => ({ kind: "folder" as const, data: node })),
      ...filteredCurrentFiles.map((file) => ({ kind: "file" as const, data: file })),
    ];

    return (
      <VirtualScrollList
        key={`folder-virtual-${currentFolderPath}-${searchQuery}`}
        items={folderItems}
        rowHeight={34}
        className="flex-1 h-full"
        renderRow={(item) => {
          if (item.kind === "folder") {
            const sub = item.data;
            const isFolderSkipped = isDirExcluded(sub.path);
            return (
              <div
                key={`folder-${sub.path}`}
                onDoubleClick={() => navigateToFolder(sub.path)}
                onClick={() => {
                  if (isExclusionMode && onToggleExclude) {
                    onToggleExclude(sub.path, true);
                  } else {
                    setSelectedEntryName(null);
                  }
                }}
                style={{ height: "34px" }}
                className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border ${
                  isFolderSkipped
                    ? "opacity-50 hover:opacity-85 bg-rose-950/15 hover:bg-rose-950/25 border-l-2 border-rose-500/80"
                    : "hover:bg-[#252525]"
                }`}
              >
                <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                  <svg className="w-4 h-4 text-[#F9D648] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                  <span
                    className={`font-mono text-[11px] truncate transition-colors font-medium ${
                      isFolderSkipped ? "text-rose-300/80 line-through" : "text-white group-hover:text-[#60CDFF]"
                    }`}
                  >
                    {sub.name}/
                  </span>
                  {isFolderSkipped && (
                    <span className="text-[10px] font-semibold px-2 py-0.2 rounded-full bg-rose-950/70 border border-rose-800 text-rose-300 shrink-0">
                      ✕ Folder Excluded
                    </span>
                  )}
                </div>
                <div className="flex items-center space-x-2 shrink-0">
                  <span className="text-[#777777] font-mono text-[10px]">
                    {sub.totalItemCount} item{sub.totalItemCount !== 1 ? "s" : ""}
                  </span>
                  {isExclusionMode && onToggleExclude && (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onToggleExclude(sub.path, true);
                      }}
                      className={`text-[10px] px-2 py-0.5 rounded border transition-colors flex items-center space-x-1 cursor-pointer ${
                        isFolderSkipped
                          ? "bg-rose-900/40 hover:bg-rose-900/70 text-rose-300 border-rose-700/60"
                          : "bg-[#2D2D2D] hover:bg-rose-950/50 text-[#A8A8A8] hover:text-rose-300 border-[#3E3E3E] hover:border-rose-800/60"
                      }`}
                      title={isFolderSkipped ? "Include all files in this folder" : "Exclude all files in this folder"}
                    >
                      <span className="text-rose-400 font-bold">{isFolderSkipped ? "↺" : "✕"}</span>
                      <span>{isFolderSkipped ? "Include" : "Skip Dir"}</span>
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      navigateToFolder(sub.path);
                    }}
                    className="text-[10px] text-[#A8A8A8] hover:text-[#60CDFF] px-2 py-0.5 rounded bg-[#2D2D2D] hover:bg-[#383838] border border-[#3E3E3E] cursor-pointer"
                  >
                    Open ➔
                  </button>
                </div>
              </div>
            );
          }

          const file = item.data;
          const isSelected = selectedEntryName === file.name;
          const fileNameOnly = file.name.split("/").pop() || file.name;
          const isExcluded = isFileExcluded(file.name);

          return (
            <div
              key={`file-${file.name}`}
              onClick={() => {
                if (isExclusionMode && onToggleExclude) {
                  onToggleExclude(file.name);
                } else {
                  setSelectedEntryName(file.name);
                }
              }}
              onDoubleClick={() => onPreviewEntry(file.name)}
              style={{ height: "34px" }}
              className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border ${
                isExcluded
                  ? "opacity-50 hover:opacity-85 bg-rose-950/15 hover:bg-rose-950/25 border-l-2 border-rose-500/80"
                  : isSelected
                    ? "bg-[#25394A] text-white"
                    : "hover:bg-[#242424]"
              }`}
            >
              <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                {renderFileIcon(fileNameOnly)}
                <span
                  className={`font-mono text-[11px] truncate transition-colors ${
                    isExcluded ? "text-rose-300/80 line-through" : "text-white group-hover:text-[#60CDFF]"
                  }`}
                  title={`${file.name} (Double-click or Spacebar to Preview${isExclusionMode ? " | Click to Toggle Exclusion" : ""})`}
                >
                  {fileNameOnly}
                </span>
                {isExcluded && (
                  <span className="text-[10px] font-semibold px-2 py-0.2 rounded-full bg-rose-950/70 border border-rose-700/80 text-rose-300 flex items-center space-x-1 shrink-0 shadow-sm">
                    <span>✕</span>
                    <span>Excluded</span>
                  </span>
                )}
              </div>

              <div className="flex items-center space-x-2.5 shrink-0">
                <span className={`font-mono text-[11px] ${isExcluded ? "text-rose-400 font-semibold" : "text-[#888888]"}`}>
                  {isExcluded ? "(Skipped)" : formatBytes(file.size)}
                </span>
                {isExclusionMode && onToggleExclude ? (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onToggleExclude(file.name);
                    }}
                    className={`text-[10px] px-2 py-0.5 rounded border transition-colors flex items-center space-x-1 cursor-pointer ${
                      isExcluded
                        ? "bg-rose-900/40 hover:bg-rose-900/70 text-rose-300 border-rose-700/70"
                        : "bg-[#2E2E2E] hover:bg-rose-950/50 text-[#A8A8A8] hover:text-rose-300 border-[#3E3E3E] hover:border-rose-800/60"
                    }`}
                    title={isExcluded ? "Include this file in extraction" : "Exclude this file from extraction"}
                  >
                    <span className="text-rose-400 font-bold">{isExcluded ? "↺" : "✕"}</span>
                    <span>{isExcluded ? "Include" : "Skip"}</span>
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onPreviewEntry(file.name);
                    }}
                    className="text-[10px] px-2 py-0.5 rounded bg-[#2E2E2E] hover:bg-[#3A3A3A] text-[#60CDFF] border border-[#444444] transition-colors flex items-center space-x-1 cursor-pointer"
                    title="In-memory safe preview (Zero-disk)"
                  >
                    <span>👁️</span>
                    <span>Preview</span>
                  </button>
                )}
              </div>
            </div>
          );
        }}
      />
    );
  };

  // Flat Table Component
  const renderFlatTable = () => {
    if (isLoading) {
      return (
        <div className="p-8 text-center space-y-3">
          <div className="w-5 h-5 border-2 border-[#60CDFF] border-t-transparent rounded-full animate-spin mx-auto"></div>
          <p className="text-xs text-[#A8A8A8]">Indexing archive directory tree...</p>
        </div>
      );
    }

    if (filteredFlatEntries.length === 0) {
      return (
        <div className="p-8 text-center text-xs text-[#888888]">
          {searchQuery ? "No matching files in archive" : "Archive is empty"}
        </div>
      );
    }

    return (
      <VirtualScrollList
        key={`flat-virtual-${searchQuery}`}
        items={filteredFlatEntries}
        rowHeight={34}
        className="flex-1 h-full"
        renderRow={(entry, idx) => {
          const isSelected = selectedEntryName === entry.name;
          const fileNameOnly = entry.name.split("/").pop() || entry.name;
          const isExcluded = !entry.is_dir && isFileExcluded(entry.name);
          const isFolderSkipped = entry.is_dir && isDirExcluded(entry.name);
          const isDimmed = isExcluded || isFolderSkipped;

          return (
            <div
              key={`flat-${entry.name || idx}`}
              onClick={() => {
                if (isExclusionMode && onToggleExclude) {
                  onToggleExclude(entry.name, entry.is_dir);
                } else {
                  setSelectedEntryName(entry.name);
                }
              }}
              onDoubleClick={() => {
                if (!entry.is_dir) onPreviewEntry(entry.name);
              }}
              style={{ height: "34px" }}
              className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border ${
                isDimmed
                  ? "opacity-50 hover:opacity-85 bg-rose-950/15 hover:bg-rose-950/25 border-l-2 border-rose-500/80"
                  : isSelected
                    ? "bg-[#25394A] text-white"
                    : "hover:bg-[#252525]"
              }`}
            >
              <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                {entry.is_dir ? (
                  <svg className="w-4 h-4 text-[#F9D648] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                ) : (
                  renderFileIcon(fileNameOnly)
                )}
                <span
                  className={`truncate font-mono text-[11px] transition-colors ${
                    isDimmed ? "text-rose-300/80 line-through" : "text-white group-hover:text-[#60CDFF]"
                  }`}
                  title={entry.name}
                >
                  {entry.name}
                </span>
                {isExcluded && (
                  <span className="text-[10px] font-semibold px-2 py-0.2 rounded-full bg-rose-950/70 border border-rose-700/80 text-rose-300 flex items-center space-x-1 shrink-0 shadow-sm">
                    <span>✕</span>
                    <span>Excluded</span>
                  </span>
                )}
                {isFolderSkipped && (
                  <span className="text-[10px] font-semibold px-2 py-0.2 rounded-full bg-rose-950/70 border border-rose-800 text-rose-300 shrink-0">
                    ✕ Folder Excluded
                  </span>
                )}
              </div>

              <div className="flex items-center space-x-2.5 shrink-0">
                <span className={`font-mono text-[11px] ${isDimmed ? "text-rose-400 font-semibold" : "text-[#888888]"}`}>
                  {isExcluded
                    ? "(Skipped)"
                    : isFolderSkipped
                      ? "(Folder Skipped)"
                      : entry.is_dir
                        ? "Directory"
                        : formatBytes(entry.size)}
                </span>
                {isExclusionMode && onToggleExclude ? (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onToggleExclude(entry.name, entry.is_dir);
                    }}
                    className={`text-[10px] px-2 py-0.5 rounded border transition-colors flex items-center space-x-1 cursor-pointer ${
                      isDimmed
                        ? "bg-rose-900/40 hover:bg-rose-900/70 text-rose-300 border-rose-700/70"
                        : "bg-[#2E2E2E] hover:bg-rose-950/50 text-[#A8A8A8] hover:text-rose-300 border-[#3E3E3E] hover:border-rose-800/60"
                    }`}
                    title={isDimmed ? "Include in extraction" : "Exclude from extraction"}
                  >
                    <span className="text-rose-400 font-bold">{isDimmed ? "↺" : "✕"}</span>
                    <span>{isDimmed ? "Include" : "Skip"}</span>
                  </button>
                ) : (
                  !entry.is_dir && (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onPreviewEntry(entry.name);
                      }}
                      className="text-[10px] px-2 py-0.5 rounded bg-[#2E2E2E] hover:bg-[#3A3A3A] text-[#60CDFF] border border-[#444444] transition-colors flex items-center space-x-1 cursor-pointer"
                      title="In-memory safe preview"
                    >
                      <span>👁️</span>
                      <span>Preview</span>
                    </button>
                  )
                )}
              </div>
            </div>
          );
        }}
      />
    );
  };

  const isEncryptedWithoutEntries =
    archiveInfo.is_encrypted && (!archiveInfo.entries || archiveInfo.entries.length === 0);

  return (
    <>
      {/* Compact Inspector View (Embedded inside Extract Tab) */}
      <div
        ref={containerRef}
        className="bg-[#202020] rounded-lg border border-[#3E3E3E] overflow-hidden flex flex-col shadow-sm"
      >
        {renderToolbar(false)}

        {isEncryptedWithoutEntries ? (
          renderEncryptedBanner()
        ) : (
          <>
            {viewMode === "tree" ? (
              <>
                {renderBreadcrumbs()}
                {renderSearchInput()}
                <div className="h-64 flex flex-col overflow-hidden">
                  {renderCurrentFolderContents()}
                </div>
              </>
            ) : (
              <>
                {renderSearchInput()}
                <div className="h-64 flex flex-col overflow-hidden">
                  {renderFlatTable()}
                </div>
              </>
            )}
          </>
        )}

        {/* Compact Footer */}
        <div className="px-3.5 py-1.5 bg-[#1C1C1C] border-t border-[#333333] flex items-center justify-between text-[11px] font-mono text-[#888888]">
          <span className="truncate max-w-[240px]">
            {selectedArchive.split(/[/\\]/).pop()}
          </span>
          <div className="flex items-center space-x-2">
            {excludedCount > 0 && (
              <span className="text-rose-400 font-semibold">
                {excludedCount} excluded ({formatBytes(excludedBytes)} skipped)
              </span>
            )}
            <span className="text-white">
              {isLoading ? "Indexing tree..." : `Uncompressed: ${formatBytes(archiveInfo.total_uncompressed_size || 0)}`}
            </span>
          </div>
        </div>
      </div>

      {/* Full-Screen Dual-Pane Expand Modal (⤢) */}
      {isExpandedModal && (
        <div
          onClick={() => setIsExpandedModal(false)}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-md p-4 sm:p-6 animate-in fade-in duration-150"
        >
          <div
            onClick={(e) => e.stopPropagation()}
            className={`bg-[#1E1E1E] border border-[#3E3E3E] rounded-xl shadow-2xl flex flex-col w-full ${
              sidebar ? "max-w-[96vw] h-[92vh]" : "max-w-6xl h-[88vh]"
            } overflow-hidden relative`}
          >
            {renderToolbar(true)}

            {isEncryptedWithoutEntries ? (
              <div className="flex-1 flex items-center justify-center">
                {renderEncryptedBanner()}
              </div>
            ) : (
              <div className="flex-1 flex overflow-hidden">
                <div className="flex-1 flex flex-col overflow-hidden">
                  {viewMode === "tree" ? (
                    <>
                      {renderBreadcrumbs()}
                      {renderSearchInput()}
                      {/* Dual-Pane Layout: Left Folder Tree, Right Folder Files */}
                      <div className="flex-1 flex overflow-hidden">
                        {/* Left Pane: Hierarchical Tree */}
                        <div className="w-72 bg-[#1A1A1A] border-r border-[#333333] overflow-y-auto custom-scrollbar p-2 flex flex-col shrink-0">
                          <div className="text-[10px] font-bold tracking-wider uppercase text-[#888888] px-2 py-1 mb-1">
                            Folder Hierarchy
                          </div>
                          {renderTreeFolderNode(treeRoot)}
                        </div>

                        {/* Right Pane: Files inside selected folder */}
                        <div className="flex-1 bg-[#181818] overflow-hidden flex flex-col">
                          <div className="px-3 py-1.5 bg-[#222222] border-b border-[#2E2E2E] text-[11px] font-mono text-[#A8A8A8] flex justify-between shrink-0">
                            <span>Folder: <span className="text-white font-semibold">/{currentFolderPath || ""}</span></span>
                            <span>Double-click or Spacebar to Preview</span>
                          </div>
                          <div className="flex-1 flex flex-col overflow-hidden">
                            {renderCurrentFolderContents()}
                          </div>
                        </div>
                      </div>
                    </>
                  ) : (
                    <>
                      {renderSearchInput()}
                      <div className="flex-1 overflow-hidden bg-[#181818] flex flex-col">
                        {renderFlatTable()}
                      </div>
                    </>
                  )}
                </div>

                {/* Right Options Sidebar (WinRAR / Pro Archiver style) */}
                {sidebar && (
                  <div className="w-80 bg-[#202020] border-l border-[#333333] flex flex-col shrink-0 overflow-y-auto custom-scrollbar p-4 space-y-4">
                    {sidebarTitle && (
                      <div className="flex items-center justify-between border-b border-[#3E3E3E] pb-2 mb-1 shrink-0">
                        <span className="text-xs uppercase tracking-wider font-semibold text-[#60CDFF]">
                          {sidebarTitle}
                        </span>
                      </div>
                    )}
                    {sidebar}
                  </div>
                )}
              </div>
            )}

            {/* Modal Bottom Status Bar */}
            <div className="px-4 py-2 bg-[#222222] border-t border-[#3E3E3E] flex items-center justify-between text-xs font-mono text-[#888888] shrink-0">
              <div className="flex items-center space-x-3">
                <span>Total Items: <span className="text-white">{archiveInfo.file_count || 0}</span></span>
                <span>•</span>
                <span>Uncompressed: <span className="text-white">{formatBytes(archiveInfo.total_uncompressed_size || 0)}</span></span>
                {excludedCount > 0 && (
                  <>
                    <span>•</span>
                    <span className="text-rose-400 font-semibold">
                      {excludedCount} excluded ({formatBytes(excludedBytes)} skipped)
                    </span>
                  </>
                )}
                {selectedEntryName && (
                  <>
                    <span>•</span>
                    <span className="text-[#60CDFF] truncate max-w-xs">Selected: {selectedEntryName.split("/").pop()}</span>
                  </>
                )}
              </div>
              <div className="flex items-center space-x-2">
                <span>Press <kbd className="px-1.5 py-0.5 bg-[#333333] border border-[#444444] rounded text-[10px] text-white">Space</kbd> to Preview</span>
                <span>•</span>
                <span>Press <kbd className="px-1.5 py-0.5 bg-[#333333] border border-[#444444] rounded text-[10px] text-white">Esc</kbd> to Exit</span>
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

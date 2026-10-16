import { useState, useMemo, useEffect, useRef } from "react";
import { ArchiveInfo, ArchiveEntryItem } from "../types";

interface ArchiveTreeInspectorProps {
  archiveInfo: ArchiveInfo;
  selectedArchive: string;
  extractPassword?: string;
  isLoading?: boolean;
  onUnlockPreview?: () => void;
  onPreviewEntry: (entryName: string) => void;
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

function getFileCategory(name: string): "code" | "image" | "restricted" | "archive" | "media" | "doc" {
  const ext = name.split(".").pop()?.toLowerCase() || "";
  if (["exe", "bat", "cmd", "msi", "dll", "sys", "com", "scr", "vbs", "ps1"].includes(ext)) {
    return "restricted";
  }
  if (["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico", "svg"].includes(ext)) {
    return "image";
  }
  if (["zip", "7z", "bay", "bay_gamer", "tar", "gz", "tgz", "rar", "bz2", "xz"].includes(ext)) {
    return "archive";
  }
  if (["mp3", "wav", "flac", "mp4", "mkv", "avi", "mov", "webm"].includes(ext)) {
    return "media";
  }
  if (
    [
      "js", "ts", "jsx", "tsx", "rs", "py", "c", "cpp", "h", "cs", "java", "go",
      "html", "css", "json", "yaml", "yml", "toml", "xml", "sql", "sh"
    ].includes(ext)
  ) {
    return "code";
  }
  return "doc";
}

export function ArchiveTreeInspector({
  archiveInfo,
  selectedArchive,
  extractPassword = "",
  isLoading = false,
  onUnlockPreview,
  onPreviewEntry,
}: ArchiveTreeInspectorProps) {
  const [viewMode, setViewMode] = useState<"tree" | "flat">("tree");
  const [isExpandedModal, setIsExpandedModal] = useState<boolean>(false);
  const [currentFolderPath, setCurrentFolderPath] = useState<string>("");
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set([""]));
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [selectedEntryName, setSelectedEntryName] = useState<string | null>(null);

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

  // Spacebar to preview selected entry
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === " " && selectedEntryName) {
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
      const clean = entry.name.replace(/\\/g, "/").trim().replace(/^\/+/, "");
      if (!clean) continue;

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

    return (
      <div key={node.path || "root"} className="select-none text-xs">
        <div
          onClick={() => navigateToFolder(node.path)}
          style={{ paddingLeft: `${Math.max(8, depth * 16 + 8)}px` }}
          className={`flex items-center justify-between py-1.5 pr-2 rounded-md transition-colors cursor-pointer group ${
            isSelected
              ? "bg-[#25394A] text-[#60CDFF] font-semibold border-l-2 border-[#60CDFF]"
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
              className={`w-4 h-4 shrink-0 ${isSelected ? "text-[#60CDFF]" : "text-[#E8C488]"}`}
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

            <span className="truncate font-mono text-[11px]" title={node.name}>
              {node.name}
            </span>
          </div>

          {/* Badge */}
          <span className="text-[10px] font-mono text-[#888888] px-1.5 py-0.2 rounded bg-[#2A2A2A] border border-[#3A3A3A] shrink-0">
            {node.totalItemCount}
          </span>
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

  // Render File Icon based on category
  const renderFileIcon = (fileName: string) => {
    const cat = getFileCategory(fileName);
    if (cat === "restricted") {
      return (
        <svg className="w-4 h-4 text-rose-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m0 0v2m0-2h2m-2 0H10m11-5a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
      );
    }
    if (cat === "image") {
      return (
        <svg className="w-4 h-4 text-purple-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      );
    }
    if (cat === "archive") {
      return (
        <svg className="w-4 h-4 text-amber-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" />
        </svg>
      );
    }
    if (cat === "code") {
      return (
        <svg className="w-4 h-4 text-[#60CDFF] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
        </svg>
      );
    }
    return (
      <svg className="w-4 h-4 text-[#A8A8A8] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
      </svg>
    );
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

        {/* Expand / Minimize Fullscreen Toggle (⤢) */}
        {!isFullModal ? (
          <button
            type="button"
            onClick={() => setIsExpandedModal(true)}
            className="p-1.5 bg-[#2D2D2D] hover:bg-[#383838] text-[#A8A8A8] hover:text-white rounded border border-[#3E3E3E] transition-colors cursor-pointer"
            title="Expand into full dual-pane workspace (⤢)"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
            </svg>
          </button>
        ) : (
          <button
            type="button"
            onClick={() => setIsExpandedModal(false)}
            className="p-1.5 bg-[#2D2D2D] hover:bg-[#383838] text-[#A8A8A8] hover:text-white rounded border border-[#3E3E3E] transition-colors cursor-pointer"
            title="Minimize to normal view"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
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

    return (
      <div className="overflow-y-auto custom-scrollbar divide-y divide-[#262626]">
        {/* Subfolders */}
        {currentFolderNode.subfolders.map((sub) => (
          <div
            key={sub.path}
            onDoubleClick={() => navigateToFolder(sub.path)}
            onClick={() => setSelectedEntryName(null)}
            className="flex items-center justify-between px-3 py-2 text-xs hover:bg-[#252525] transition-colors cursor-pointer group"
          >
            <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
              <svg className="w-4 h-4 text-[#E8C488] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
              </svg>
              <span className="text-white font-mono text-[11px] truncate group-hover:text-[#60CDFF] transition-colors font-medium">
                {sub.name}/
              </span>
            </div>
            <div className="flex items-center space-x-3 shrink-0">
              <span className="text-[#777777] font-mono text-[10px]">
                {sub.totalItemCount} item{sub.totalItemCount !== 1 ? "s" : ""}
              </span>
              <button
                type="button"
                onClick={() => navigateToFolder(sub.path)}
                className="text-[10px] text-[#A8A8A8] hover:text-[#60CDFF] px-2 py-0.5 rounded bg-[#2D2D2D] hover:bg-[#383838] border border-[#3E3E3E] cursor-pointer"
              >
                Open ➔
              </button>
            </div>
          </div>
        ))}

        {/* Files */}
        {filteredCurrentFiles.map((file) => {
          const isSelected = selectedEntryName === file.name;
          const fileNameOnly = file.name.split("/").pop() || file.name;

          return (
            <div
              key={file.name}
              onClick={() => setSelectedEntryName(file.name)}
              onDoubleClick={() => onPreviewEntry(file.name)}
              className={`flex items-center justify-between px-3 py-2 text-xs transition-colors cursor-pointer group ${
                isSelected ? "bg-[#25394A] text-white" : "hover:bg-[#242424]"
              }`}
            >
              <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                {renderFileIcon(fileNameOnly)}
                <span
                  className="font-mono text-[11px] truncate text-white group-hover:text-[#60CDFF] transition-colors"
                  title={`${file.name} (Double-click or Spacebar to Preview)`}
                >
                  {fileNameOnly}
                </span>
              </div>

              <div className="flex items-center space-x-3 shrink-0">
                <span className="text-[#888888] font-mono text-[11px]">
                  {formatBytes(file.size)}
                </span>
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
              </div>
            </div>
          );
        })}
      </div>
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
      <div className="overflow-y-auto custom-scrollbar divide-y divide-[#262626]">
        {filteredFlatEntries.map((entry, idx) => {
          const isSelected = selectedEntryName === entry.name;
          const fileNameOnly = entry.name.split("/").pop() || entry.name;

          return (
            <div
              key={idx}
              onClick={() => setSelectedEntryName(entry.name)}
              onDoubleClick={() => {
                if (!entry.is_dir) onPreviewEntry(entry.name);
              }}
              className={`flex items-center justify-between px-3 py-2 text-xs transition-colors cursor-pointer group ${
                isSelected ? "bg-[#25394A] text-white" : "hover:bg-[#252525]"
              }`}
            >
              <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                {entry.is_dir ? (
                  <svg className="w-4 h-4 text-[#E8C488] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                ) : (
                  renderFileIcon(fileNameOnly)
                )}
                <span className="text-white truncate font-mono text-[11px] group-hover:text-[#60CDFF] transition-colors" title={entry.name}>
                  {entry.name}
                </span>
              </div>

              <div className="flex items-center space-x-3 shrink-0">
                <span className="text-[#888888] font-mono text-[11px]">
                  {entry.is_dir ? "Directory" : formatBytes(entry.size)}
                </span>
                {!entry.is_dir && (
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
                )}
              </div>
            </div>
          );
        })}
      </div>
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
                <div className="max-h-64 flex flex-col">
                  {renderCurrentFolderContents()}
                </div>
              </>
            ) : (
              <>
                {renderSearchInput()}
                <div className="max-h-64 flex flex-col">
                  {renderFlatTable()}
                </div>
              </>
            )}
          </>
        )}

        {/* Compact Footer */}
        <div className="px-3.5 py-1.5 bg-[#1C1C1C] border-t border-[#333333] flex items-center justify-between text-[11px] font-mono text-[#888888]">
          <span className="truncate max-w-[280px]">
            {selectedArchive.split(/[/\\]/).pop()}
          </span>
          <span className="text-white">
            {isLoading ? "Indexing tree..." : `Uncompressed: ${formatBytes(archiveInfo.total_uncompressed_size || 0)}`}
          </span>
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
            className="bg-[#1E1E1E] border border-[#3E3E3E] rounded-xl shadow-2xl flex flex-col w-full max-w-6xl h-[88vh] overflow-hidden relative"
          >
            {renderToolbar(true)}

            {isEncryptedWithoutEntries ? (
              <div className="flex-1 flex items-center justify-center">
                {renderEncryptedBanner()}
              </div>
            ) : (
              <div className="flex-1 flex flex-col overflow-hidden">
                {viewMode === "tree" ? (
                  <>
                    {renderBreadcrumbs()}
                    {renderSearchInput()}
                    {/* Dual-Pane Layout: Left Folder Tree, Right Folder Files */}
                    <div className="flex-1 flex overflow-hidden">
                      {/* Left Pane: Hierarchical Tree */}
                      <div className="w-80 bg-[#1A1A1A] border-r border-[#333333] overflow-y-auto custom-scrollbar p-2 flex flex-col shrink-0">
                        <div className="text-[10px] font-bold tracking-wider uppercase text-[#888888] px-2 py-1 mb-1">
                          Folder Hierarchy
                        </div>
                        {renderTreeFolderNode(treeRoot)}
                      </div>

                      {/* Right Pane: Files inside selected folder */}
                      <div className="flex-1 bg-[#181818] overflow-y-auto custom-scrollbar flex flex-col">
                        <div className="px-3 py-1.5 bg-[#222222] border-b border-[#2E2E2E] text-[11px] font-mono text-[#A8A8A8] flex justify-between">
                          <span>Folder: <span className="text-white font-semibold">/{currentFolderPath || ""}</span></span>
                          <span>Double-click or Spacebar to Preview</span>
                        </div>
                        <div className="flex-1">
                          {renderCurrentFolderContents()}
                        </div>
                      </div>
                    </div>
                  </>
                ) : (
                  <>
                    {renderSearchInput()}
                    <div className="flex-1 overflow-y-auto custom-scrollbar bg-[#181818]">
                      {renderFlatTable()}
                    </div>
                  </>
                )}
              </div>
            )}

            {/* Modal Bottom Status Bar */}
            <div className="px-4 py-2 bg-[#222222] border-t border-[#3E3E3E] flex items-center justify-between text-xs font-mono text-[#888888] shrink-0">
              <div className="flex items-center space-x-3">
                <span>Total Items: <span className="text-white">{archiveInfo.file_count || 0}</span></span>
                <span>•</span>
                <span>Uncompressed: <span className="text-white">{formatBytes(archiveInfo.total_uncompressed_size || 0)}</span></span>
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

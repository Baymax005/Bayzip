import { useState, useMemo, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArchiveInfo, ArchiveEntryItem, DriveInfo, DirectoryItem, QuickAccessItem, OperationProgressInfo } from "../types";
import { FileFormatIcon, prefetchFileIcons } from "./FileFormatIcon";
import { WorkspaceNavBar } from "./workspace/WorkspaceNavBar";
import { WorkspaceStagedQueue } from "./workspace/WorkspaceStagedQueue";
import { WorkspaceExtractQueue } from "./workspace/WorkspaceExtractQueue";
import { WorkspaceContextMenu, ContextMenuTarget } from "./workspace/WorkspaceContextMenu";
import { WorkspaceDualPaneView } from "./workspace/WorkspaceDualPaneView";
import { WorkspaceHeader } from "./workspace/WorkspaceHeader";
import { useWorkspaceDragDrop, normalizeArchiveEntryPath } from "./workspace/useWorkspaceDragDrop";
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
  inspectorTitle,
  operationProgress,
  onStageItem,
  mode = "compress",
  onSwitchMode,
  stagedItems,
  itemsInfo,
  onUnstageItem,
  onClearStagedItems,
  onSelectArchive,
  onLoadArchiveFromFile,
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

  // Staged files drawer state
  const [isStagedQueueOpen, setIsStagedQueueOpen] = useState<boolean>(true);
  const [stagedQueueHeight, setStagedQueueHeight] = useState<number>(180);

  // Extract files drawer state
  const [isExtractQueueOpen, setIsExtractQueueOpen] = useState<boolean>(true);
  const [extractQueueHeight, setExtractQueueHeight] = useState<number>(180);

  // Top Titlebar Progress HUD dropdown state
  const [isHeaderHUDOpen, setIsHeaderHUDOpen] = useState<boolean>(false);

  // Dynamic responsive window width tracking
  const [windowWidth, setWindowWidth] = useState<number>(() =>
    typeof window !== "undefined" ? window.innerWidth : 1200
  );
  const [showMobileTree, setShowMobileTree] = useState<boolean>(false);
  const [showMobileSidebar, setShowMobileSidebar] = useState<boolean>(false);
  const [isLeftPanelOpen, setIsLeftPanelOpen] = useState<boolean>(true);

  useEffect(() => {
    const handleResize = () => setWindowWidth(window.innerWidth);
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === "b" || e.key === "B")) {
        e.preventDefault();
        if (windowWidth < 800) {
          setShowMobileTree((prev) => !prev);
        } else {
          setIsLeftPanelOpen((prev) => !prev);
        }
      }
    };
    window.addEventListener("resize", handleResize);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("resize", handleResize);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [windowWidth]);

  // System drives & Filesystem browsing states
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [activeDrive, setActiveDrive] = useState<string>("C:");

  // Local mutable entries state for real-time additions/renames/deletions
  const [localEntries, setLocalEntries] = useState<ArchiveEntryItem[]>(archiveInfo.entries || []);

  useEffect(() => {
    if (mode === "compress" && (!stagedItems || stagedItems.length === 0)) {
      setLocalEntries([]);
      setCurrentFolderPath("");
    } else {
      setLocalEntries(archiveInfo.entries || []);
    }
  }, [archiveInfo.entries, stagedItems?.length, mode]);

  const hasTreeItems = Boolean(
    (localEntries && localEntries.length > 0) ||
    (archiveInfo && archiveInfo.entries && archiveInfo.entries.length > 0) ||
    (stagedItems && stagedItems.length > 0)
  );

  const hasArchiveLoaded = Boolean(
    (archiveInfo &&
      archiveInfo.path &&
      archiveInfo.path !== "" &&
      archiveInfo.path !== "archive" &&
      archiveInfo.entries &&
      archiveInfo.entries.length > 0) ||
    (mode === "compress" && hasTreeItems)
  );

  const [browserSource, setBrowserSource] = useState<"archive" | "filesystem">(() => {
    return hasTreeItems ? "archive" : "filesystem";
  });

  const lastArchivePathRef = useRef<string>(archiveInfo?.path || "");
  const lastItemCountRef = useRef<number>((localEntries?.length || 0) + (stagedItems?.length || 0));

  useEffect(() => {
    if (archiveInfo?.path && archiveInfo.path !== lastArchivePathRef.current) {
      lastArchivePathRef.current = archiveInfo.path;
      setBrowserSource("archive");
    }
  }, [archiveInfo?.path]);

  useEffect(() => {
    const currentCount = (localEntries?.length || 0) + (stagedItems?.length || 0);
    if (lastItemCountRef.current === 0 && currentCount > 0) {
      setBrowserSource("archive");
    }
    lastItemCountRef.current = currentCount;
  }, [localEntries?.length, stagedItems?.length]);

  const [diskCurrentPath, setDiskCurrentPath] = useState<string>("C:\\");
  const [diskEntries, setDiskEntries] = useState<DirectoryItem[]>([]);
  const [isLoadingDisk, setIsLoadingDisk] = useState<boolean>(false);
  const [diskError, setDiskError] = useState<string | null>(null);

  // Disk Navigation History
  const [diskHistory, setDiskHistory] = useState<string[]>(["C:\\"]);
  const [diskHistoryIndex, setDiskHistoryIndex] = useState<number>(0);

  // Archive Folder Navigation History
  const [archiveHistory, setArchiveHistory] = useState<string[]>([""]);
  const [archiveHistoryIndex, setArchiveHistoryIndex] = useState<number>(0);

  const handleArchiveBack = () => {
    if (archiveHistoryIndex > 0) {
      const target = archiveHistory[archiveHistoryIndex - 1];
      setArchiveHistoryIndex((prev) => prev - 1);
      setCurrentFolderPath(target);
      setExpandedFolders((prev) => new Set(prev).add(target));
    }
  };

  const handleArchiveForward = () => {
    if (archiveHistoryIndex < archiveHistory.length - 1) {
      const target = archiveHistory[archiveHistoryIndex + 1];
      setArchiveHistoryIndex((prev) => prev + 1);
      setCurrentFolderPath(target);
      setExpandedFolders((prev) => new Set(prev).add(target));
    }
  };

  // Quick Access Shortcuts
  const [quickAccess, setQuickAccess] = useState<QuickAccessItem[]>([]);
  const [editPathTrigger, setEditPathTrigger] = useState<number>(0);

  // Instant Dual-Pane Split View State
  const [isSplitView, setIsSplitView] = useState<boolean>(false);
  const [splitRatio, setSplitRatio] = useState<number>(0.5);

  // Pane 2 State
  const [pane2Source, setPane2Source] = useState<"archive" | "filesystem">(() => {
    return mode === "compress" ? "archive" : "filesystem";
  });
  const [pane2FolderPath, setPane2FolderPath] = useState<string>("");
  const [pane2DiskPath, setPane2DiskPath] = useState<string>("C:\\");
  const [pane2DiskEntries, setPane2DiskEntries] = useState<DirectoryItem[]>([]);
  const [isLoadingPane2Disk, setIsLoadingPane2Disk] = useState<boolean>(false);
  const [pane2DiskError, setPane2DiskError] = useState<string | null>(null);

  const loadPane2DiskDirectory = async (dirPath: string) => {
    let clean = dirPath.trim();
    if (/^[a-zA-Z]:$/.test(clean)) {
      clean = `${clean}\\`;
    }
    setPane2DiskPath(clean);
    setIsLoadingPane2Disk(true);
    setPane2DiskError(null);
    try {
      const items = await invoke<DirectoryItem[]>("browse_directory", { path: clean });
      setPane2DiskEntries(items || []);
    } catch (err: any) {
      setPane2DiskError(String(err));
    } finally {
      setIsLoadingPane2Disk(false);
    }
  };

  const handlePane2DiskUp = () => {
    const clean = pane2DiskPath.replace(/[/\\]+$/, "");
    const lastSlash = Math.max(clean.lastIndexOf("\\"), clean.lastIndexOf("/"));
    if (lastSlash > 0) {
      const parent = clean.substring(0, lastSlash + 1);
      loadPane2DiskDirectory(parent);
    } else if (clean.length >= 2 && clean[1] === ":") {
      loadPane2DiskDirectory(`${clean.slice(0, 2)}\\`);
    }
  };

  const handlePane2ArchiveUp = () => {
    if (!pane2FolderPath) return;
    const parts = pane2FolderPath.split("/").filter(Boolean);
    parts.pop();
    setPane2FolderPath(parts.join("/"));
  };

  useEffect(() => {
    if (isSplitView && pane2Source === "filesystem" && pane2DiskEntries.length === 0) {
      loadPane2DiskDirectory(diskCurrentPath || "C:\\");
    }
  }, [isSplitView, pane2Source]);

  // Helper to resolve an archive/tree entry to its absolute path in stagedItems (if staged for compression)
  const findStagedItemPath = (entryPath: string): string | null => {
    if (!stagedItems || stagedItems.length === 0) return null;
    const clean = entryPath.replace(/\\/g, "/").replace(/\/+$/, "").trim();
    for (const item of stagedItems) {
      const cleanItem = item.replace(/\\/g, "/").replace(/\/+$/, "").trim();
      if (cleanItem.toLowerCase() === clean.toLowerCase()) {
        return item;
      }
      const base = cleanItem.split("/").pop() || "";
      if (base.toLowerCase() === clean.toLowerCase()) {
        return item;
      }
      const rootSeg = clean.split("/")[0] || "";
      if (base.toLowerCase() === rootSeg.toLowerCase() && clean === rootSeg) {
        return item;
      }
    }
    return null;
  };

  const loadDiskDirectory = async (dirPath: string) => {
    let clean = dirPath.trim();
    if (/^[a-zA-Z]:$/.test(clean)) {
      clean = `${clean}\\`;
    }
    setDiskCurrentPath(clean);
    if (/^[a-zA-Z]:/i.test(clean)) {
      setActiveDrive(clean.slice(0, 2).toUpperCase());
    }
    setIsLoadingDisk(true);
    setDiskError(null);
    try {
      const items = await invoke<DirectoryItem[]>("browse_directory", { path: clean });
      setDiskEntries(items || []);
    } catch (err: any) {
      setDiskError(String(err));
    } finally {
      setIsLoadingDisk(false);
    }
  };

  const navigateToDiskDirectory = (dirPath: string, pushHistory = true) => {
    let clean = dirPath.trim();
    if (/^[a-zA-Z]:$/.test(clean)) {
      clean = `${clean}\\`;
    }

    // Smart Quick Access matching: if user typed "downloads", "desktop", "c:\downloads", etc.
    const lower = clean.toLowerCase().replace(/[\/\\]+$/, "");
    if (quickAccess && quickAccess.length > 0) {
      const match = quickAccess.find((item) => {
        const itemLower = item.name.toLowerCase();
        const itemPathLower = item.path.toLowerCase().replace(/[\/\\]+$/, "");
        return (
          itemLower === lower ||
          `c:\\${itemLower}` === lower ||
          `c:/${itemLower}` === lower ||
          itemPathLower === lower
        );
      });
      if (match) {
        clean = match.path;
      }
    }

    loadDiskDirectory(clean);
    if (pushHistory) {
      setDiskHistory((prev) => {
        const next = prev.slice(0, diskHistoryIndex + 1);
        next.push(clean);
        return next;
      });
      setDiskHistoryIndex((prev) => prev + 1);
    }
  };

  const handleDiskBack = () => {
    if (diskHistoryIndex > 0) {
      const target = diskHistory[diskHistoryIndex - 1];
      setDiskHistoryIndex((prev) => prev - 1);
      loadDiskDirectory(target);
    }
  };

  const handleDiskForward = () => {
    if (diskHistoryIndex < diskHistory.length - 1) {
      const target = diskHistory[diskHistoryIndex + 1];
      setDiskHistoryIndex((prev) => prev + 1);
      loadDiskDirectory(target);
    }
  };

  const handleDiskUp = () => {
    const clean = diskCurrentPath.replace(/[/\\]+$/, "");
    const lastSlash = Math.max(clean.lastIndexOf("\\"), clean.lastIndexOf("/"));
    if (lastSlash > 0) {
      const parent = clean.substring(0, lastSlash + 1);
      navigateToDiskDirectory(parent);
    } else if (clean.length > 2 && clean[1] === ":") {
      navigateToDiskDirectory(`${clean.slice(0, 2)}\\`);
    }
  };

  const handleDiskRefresh = () => {
    loadDiskDirectory(diskCurrentPath);
  };

  useEffect(() => {
    invoke<DriveInfo[]>("get_system_drives")
      .then((res) => {
        if (res && res.length > 0) {
          setDrives(res);
          const initialLetter =
            selectedArchive && /^[a-zA-Z]:/.test(selectedArchive)
              ? selectedArchive.slice(0, 2).toUpperCase()
              : res[0].mount_point.slice(0, 2).toUpperCase();
          setActiveDrive(initialLetter);
          const driveRoot = `${initialLetter}\\`;
          setDiskCurrentPath(driveRoot);
        }
      })
      .catch(() => {});

    invoke<QuickAccessItem[]>("get_quick_access")
      .then((res) => {
        if (res && res.length > 0) setQuickAccess(res);
      })
      .catch(() => {});
  }, []);


  // Inline New Folder State
  const [isCreatingFolder, setIsCreatingFolder] = useState<boolean>(false);
  const [newFolderName, setNewFolderName] = useState<string>("New Folder");

  // Inline Rename State
  const [renamingItem, setRenamingItem] = useState<{ originalName: string; isDir: boolean } | null>(null);
  const [renamingName, setRenamingName] = useState<string>("");

  // Internal Clipboard for Copy & Paste
  const [clipboardItem, setClipboardItem] = useState<{
    relName: string;
    isDir: boolean;
    diskPath?: string;
  } | null>(null);

  // Right-Click Context Menu State
  const [contextMenu, setContextMenu] = useState<{
    isOpen: boolean;
    x: number;
    y: number;
    target: ContextMenuTarget | null;
  }>({
    isOpen: false,
    x: 0,
    y: 0,
    target: null,
  });

  const getDiskPath = (relPath: string): string => {
    if (!selectedArchive) return "";
    const cleanRel = relPath.replace(/\\/g, "/").replace(/^\.?\/+/, "").replace(/\/+$/, "");
    const base = selectedArchive.replace(/\\/g, "/").replace(/\/+$/, "");
    if (!cleanRel) return base;
    return `${base}/${cleanRel}`;
  };

  // Modular Workspace Drag-and-Drop Hook (Encapsulated in useWorkspaceDragDrop.ts)
  const {
    getArchiveDragProps,
    getDiskDragProps,
    getFolderDropProps,
  } = useWorkspaceDragDrop({
    localEntries,
    setLocalEntries,
    getDiskPath,
    setExpandedFolders,
    browserSource,
    diskCurrentPath,
    loadDiskDirectory,
    isSplitView,
    pane2Source,
    pane2DiskPath,
    loadPane2DiskDirectory,
    onStageItem,
  });

  const handleStartNewFolder = () => {
    setIsCreatingFolder(true);
    setNewFolderName("New Folder");
    setRenamingItem(null);
  };

  const handleCancelNewFolder = () => {
    setIsCreatingFolder(false);
    setNewFolderName("New Folder");
  };

  const handleCommitNewFolder = async () => {
    const trimmed = newFolderName.trim();
    if (!trimmed) {
      setIsCreatingFolder(false);
      return;
    }
    const cleanCurrent = currentFolderPath ? currentFolderPath.replace(/\/+$/, "") : "";
    const relFolderPath = cleanCurrent ? `${cleanCurrent}/${trimmed}` : trimmed;
    const folderEntryName = `${relFolderPath}/`;

    setLocalEntries((prev) => {
      const exists = prev.some(
        (e) => normalizeArchiveEntryPath(e.name).replace(/\/+$/, "") === normalizeArchiveEntryPath(relFolderPath)
      );
      if (exists) return prev;
      return [{ name: folderEntryName, size: 0, is_dir: true }, ...prev];
    });

    const diskPath = getDiskPath(relFolderPath);
    if (diskPath) {
      invoke("create_directory", { path: diskPath }).catch((err) =>
        console.error("Directory create error on disk:", err)
      );
    }

    // Automatically expand parent and the new folder in the Left Bar Hierarchy
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (cleanCurrent) next.add(cleanCurrent);
      next.add(relFolderPath);
      return next;
    });

    setIsCreatingFolder(false);
    setNewFolderName("New Folder");
  };

  const handleStartRename = (name: string, isDir: boolean) => {
    const baseName = name.replace(/\/+$/, "").split("/").pop() || "";
    setRenamingItem({ originalName: name, isDir });
    setRenamingName(baseName);
    setIsCreatingFolder(false);
  };

  const handleCancelRename = () => {
    setRenamingItem(null);
    setRenamingName("");
  };

  const handleCommitRename = async () => {
    if (!renamingItem) return;
    const trimmed = renamingName.trim();
    if (!trimmed) {
      setRenamingItem(null);
      return;
    }

    const { originalName, isDir } = renamingItem;
    const cleanOrig = originalName.replace(/\\/g, "/").replace(/^\.?\/+/, "").replace(/\/+$/, "");
    const lastSlash = cleanOrig.lastIndexOf("/");
    const parent = lastSlash === -1 ? "" : cleanOrig.substring(0, lastSlash);
    const newRel = parent ? `${parent}/${trimmed}` : trimmed;
    const newEntryName = isDir ? `${newRel}/` : newRel;

    const oldDisk = getDiskPath(cleanOrig);
    const newDisk = getDiskPath(newRel);
    if (oldDisk && newDisk) {
      invoke("rename_path", { oldPath: oldDisk, newPath: newDisk }).catch((err) =>
        console.error("Rename error on disk:", err)
      );
    }

    setLocalEntries((prev) =>
      prev.map((e) => {
        const eNorm = e.name.replace(/\\/g, "/").replace(/^\.?\/+/, "");
        if (isDir) {
          if (eNorm.replace(/\/+$/, "") === cleanOrig) {
            return { ...e, name: newEntryName };
          }
          if (eNorm.startsWith(cleanOrig + "/")) {
            return { ...e, name: `${newRel}/${eNorm.substring(cleanOrig.length + 1)}` };
          }
        } else {
          if (eNorm === cleanOrig) {
            return { ...e, name: newEntryName };
          }
        }
        return e;
      })
    );

    setRenamingItem(null);
    setRenamingName("");
  };

  const handleDeleteItem = async (name: string, isDir: boolean) => {
    const clean = name.replace(/\\/g, "/").replace(/^\.?\/+/, "").replace(/\/+$/, "");
    const diskPath = getDiskPath(clean);

    if (diskPath) {
      try {
        await invoke("delete_to_recycle_bin", { path: diskPath });
      } catch (err) {
        console.error("Recycle bin delete error:", err);
      }
    }

    setLocalEntries((prev) =>
      prev.filter((e) => {
        const eNorm = e.name.replace(/\\/g, "/").replace(/^\.?\/+/, "").replace(/\/+$/, "");
        if (isDir) {
          return eNorm !== clean && !eNorm.startsWith(clean + "/");
        }
        return eNorm !== clean;
      })
    );

    if (selectedEntryName === name) {
      setSelectedEntryName(null);
    }
  };

  const handlePasteItem = async () => {
    if (!clipboardItem) return;
    const destDirDisk = getDiskPath(currentFolderPath);
    const itemName = clipboardItem.relName.split("/").filter(Boolean).pop() || "item";

    if (clipboardItem.diskPath && destDirDisk) {
      try {
        const createdPath = await invoke<string>("copy_path", {
          src: clipboardItem.diskPath,
          destDir: destDirDisk,
        });
        const actualName = createdPath.replace(/\\/g, "/").split("/").pop() || itemName;
        const newRel = currentFolderPath ? `${currentFolderPath}/${actualName}` : actualName;
        const newEntryName = clipboardItem.isDir ? `${newRel}/` : newRel;

        setLocalEntries((prev) => [
          { name: newEntryName, size: 0, is_dir: clipboardItem.isDir },
          ...prev,
        ]);
      } catch (err) {
        console.error("Paste error:", err);
      }
    } else {
      const newRel = currentFolderPath ? `${currentFolderPath}/${itemName}` : itemName;
      const newEntryName = clipboardItem.isDir ? `${newRel}/` : newRel;
      setLocalEntries((prev) => [
        { name: newEntryName, size: 0, is_dir: clipboardItem.isDir },
        ...prev,
      ]);
    }
  };

  const handleOpenContextMenu = (
    e: React.MouseEvent,
    target?: ContextMenuTarget | null
  ) => {
    e.preventDefault();
    e.stopPropagation();
    const clampedX = Math.min(e.clientX, window.innerWidth - 230);
    const clampedY = Math.min(e.clientY, window.innerHeight - 280);
    setContextMenu({
      isOpen: true,
      x: clampedX,
      y: clampedY,
      target: target || null,
    });
  };

  // Draggable Split Panel Widths for large screens
  const [leftPanelWidth, setLeftPanelWidth] = useState<number>(288); // Default 288px (w-72)
  const [rightPanelWidth, setRightPanelWidth] = useState<number>(320); // Default 320px (w-80)
  const [isDraggingLeft, setIsDraggingLeft] = useState<boolean>(false);
  const [isDraggingRight, setIsDraggingRight] = useState<boolean>(false);

  const dragLeftStartRef = useRef<{ startX: number; startWidth: number }>({ startX: 0, startWidth: 288 });
  const dragRightStartRef = useRef<{ startX: number; startWidth: number }>({ startX: 0, startWidth: 320 });

  const handleMouseDownLeft = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDraggingLeft(true);
    dragLeftStartRef.current = { startX: e.clientX, startWidth: leftPanelWidth };
  };

  const handleMouseDownRight = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDraggingRight(true);
    dragRightStartRef.current = { startX: e.clientX, startWidth: rightPanelWidth };
  };

  useEffect(() => {
    if (!isDraggingLeft && !isDraggingRight) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (isDraggingLeft) {
        const delta = e.clientX - dragLeftStartRef.current.startX;
        const newWidth = Math.max(220, Math.min(650, dragLeftStartRef.current.startWidth + delta));
        setLeftPanelWidth(newWidth);
      } else if (isDraggingRight) {
        const delta = dragRightStartRef.current.startX - e.clientX;
        const newWidth = Math.max(240, Math.min(700, dragRightStartRef.current.startWidth + delta));
        setRightPanelWidth(newWidth);
      }
    };

    const handleMouseUp = () => {
      setIsDraggingLeft(false);
      setIsDraggingRight(false);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDraggingLeft, isDraggingRight]);

  const excludedCount =
    mode === "compress" && (!stagedItems || stagedItems.length === 0) ? 0 : excludedPaths.size;

  const isFileExcluded = (filePath: string): boolean => {
    if (!excludedPaths || excludedPaths.size === 0) return false;
    const clean = normalizeArchiveEntryPath(filePath).replace(/\/+$/, "");
    if (excludedPaths.has(clean)) return true;
    for (const exc of Array.from(excludedPaths)) {
      const excClean = exc.replace(/\/+$/, "");
      if (clean.startsWith(excClean + "/")) return true;
    }
    return false;
  };

  const isDirExcluded = (dirPath: string): boolean => {
    if (!excludedPaths || excludedPaths.size === 0) return false;
    const cleanDir = normalizeArchiveEntryPath(dirPath).replace(/\/+$/, "");
    if (excludedPaths.has(cleanDir)) return true;
    for (const exc of Array.from(excludedPaths)) {
      const excClean = exc.replace(/\/+$/, "");
      if (cleanDir.startsWith(excClean + "/")) return true;
    }
    const prefix = cleanDir ? `${cleanDir}/` : "";
    const allEntries = archiveInfo?.entries || [];
    const filesInDir = allEntries.filter((e) => !e.is_dir && normalizeArchiveEntryPath(e.name).replace(/\/+$/, "").startsWith(prefix));
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

  // Batch prefetch native Windows icons for all entries
  useEffect(() => {
    if (archiveInfo.entries && archiveInfo.entries.length > 0) {
      prefetchFileIcons(archiveInfo.entries.map((e) => e.name));
    }
  }, [archiveInfo.entries]);

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

  // Keyboard shortcuts for File Manager & Preview (safely ignoring text inputs)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      const isEditable = target?.isContentEditable;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || isEditable) {
        return;
      }

      // Escape closes context menu, cancels folder creation or rename
      if (e.key === "Escape") {
        if (contextMenu.isOpen) {
          setContextMenu((prev) => ({ ...prev, isOpen: false }));
          return;
        }
        if (isCreatingFolder) {
          setIsCreatingFolder(false);
          return;
        }
        if (renamingItem) {
          setRenamingItem(null);
          return;
        }
      }

      // Spacebar to preview selected entry
      if (e.key === " " && selectedEntryName) {
        e.preventDefault();
        onPreviewEntry(selectedEntryName);
        return;
      }

      // Ctrl+\: Toggle Dual-Pane Split View
      if (e.ctrlKey && (e.key === "\\" || e.key === "|")) {
        e.preventDefault();
        setIsSplitView((prev) => !prev);
        return;
      }

      // Ctrl+Shift+N: New Folder
      if (e.ctrlKey && e.shiftKey && (e.key === "N" || e.key === "n")) {
        e.preventDefault();
        handleStartNewFolder();
        return;
      }

      // F2: Rename selected entry
      if (e.key === "F2" && selectedEntryName) {
        e.preventDefault();
        const isDir = selectedEntryName.endsWith("/");
        handleStartRename(selectedEntryName, isDir);
        return;
      }

      // Delete: Recycle bin delete selected entry
      if ((e.key === "Delete" || e.key === "Del") && selectedEntryName) {
        e.preventDefault();
        const isDir = selectedEntryName.endsWith("/");
        handleDeleteItem(selectedEntryName, isDir);
        return;
      }

      // Ctrl+C: Copy selected entry
      if (e.ctrlKey && (e.key === "c" || e.key === "C") && selectedEntryName) {
        e.preventDefault();
        const isDir = selectedEntryName.endsWith("/");
        setClipboardItem({
          relName: selectedEntryName,
          isDir,
          diskPath: getDiskPath(selectedEntryName),
        });
        return;
      }

      // Ctrl+V: Paste copied entry into current folder
      if (e.ctrlKey && (e.key === "v" || e.key === "V") && clipboardItem) {
        e.preventDefault();
        handlePasteItem();
        return;
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedEntryName, onPreviewEntry, contextMenu.isOpen, isCreatingFolder, renamingItem, clipboardItem, currentFolderPath]);

  // Build Hierarchical Tree structure from flat entries
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
  }, [archiveInfo.filename, archiveInfo.is_archive, localEntries]);

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

  // Current folder node based on currentFolderPath
  const currentFolderNode = useMemo<TreeNode>(() => {
    return getNodeByPath(currentFolderPath);
  }, [treeRoot, currentFolderPath]);

  // Pane 2 folder node based on pane2FolderPath
  const pane2FolderNode = useMemo<TreeNode>(() => {
    return getNodeByPath(pane2FolderPath);
  }, [treeRoot, pane2FolderPath]);

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
  const navigateToFolder = (folderPath: string, pushHistory = true) => {
    setCurrentFolderPath(folderPath);
    setExpandedFolders((prev) => new Set(prev).add(folderPath));
    if (pushHistory) {
      setArchiveHistory((prev) => {
        const next = prev.slice(0, archiveHistoryIndex + 1);
        next.push(folderPath);
        return next;
      });
      setArchiveHistoryIndex((prev) => prev + 1);
    }
  };

  // Navigate up one level
  const navigateUp = () => {
    if (!currentFolderPath) return;
    const parts = currentFolderPath.split("/");
    parts.pop();
    navigateToFolder(parts.join("/"));
  };

  // Filtered entries for Flat View
  const filteredFlatEntries = useMemo(() => {
    if (!localEntries) return [];
    if (!searchQuery.trim()) return localEntries;
    const q = searchQuery.toLowerCase();
    return localEntries.filter((entry) => entry.name.toLowerCase().includes(q));
  }, [localEntries, searchQuery]);

  // Filtered files in Current Folder for Tree View
  const filteredCurrentFiles = useMemo(() => {
    if (!searchQuery.trim()) return currentFolderNode.files;
    const q = searchQuery.toLowerCase();
    return currentFolderNode.files.filter((f) => f.name.toLowerCase().includes(q));
  }, [currentFolderNode.files, searchQuery]);

  // Quick Access Shortcuts (Desktop, Downloads, Documents, Pictures, etc.)
  const renderQuickAccess = () => {
    if (!quickAccess || quickAccess.length === 0) return null;
    const getIcon = (iconName: string) => {
      switch (iconName) {
        case "desktop": return "🖥️";
        case "downloads": return "📥";
        case "documents": return "📄";
        case "pictures": return "🖼️";
        case "music": return "🎵";
        case "videos": return "🎬";
        default: return "📁";
      }
    };
    return (
      <div className="mb-2 p-2 bg-[#1F1F1F] rounded-lg border border-[#2E2E2E] shrink-0 space-y-1.5">
        <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider text-[#888888] px-0.5">
          <div className="flex items-center space-x-1.5">
            <span className="text-sm">⚡</span>
            <span>Quick Access</span>
          </div>
        </div>
        <div className="grid grid-cols-1 gap-0.5">
          {quickAccess.map((item) => {
            const isCur =
              diskCurrentPath.toLowerCase() === item.path.toLowerCase() ||
              diskCurrentPath.toLowerCase() === `${item.path}\\`.toLowerCase();
            return (
              <button
                key={item.path}
                type="button"
                onClick={() => {
                  navigateToDiskDirectory(item.path);
                  setBrowserSource("filesystem");
                }}
                className={`w-full flex items-center space-x-2 px-2 py-1.5 rounded-md text-xs transition-colors cursor-pointer text-left ${
                  isCur
                    ? "bg-[#25394A] text-[#60CDFF] font-semibold border border-[#60CDFF]/30 shadow-sm"
                    : "hover:bg-[#252525] text-[#D0D0D0] hover:text-white border border-transparent"
                }`}
                title={item.path}
              >
                <span className="text-sm shrink-0">{getIcon(item.icon)}</span>
                <span className="truncate flex-1 text-[11px] font-sans font-medium">{item.name}</span>
              </button>
            );
          })}
        </div>
      </div>
    );
  };

  // Sleek System Drive Selector Panel
  const renderDriveSelector = () => {
    if (!drives || drives.length === 0) return null;

    return (
      <div className="mb-2 p-2 bg-[#1F1F1F] rounded-lg border border-[#2E2E2E] shrink-0 space-y-1.5">
        <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider text-[#888888] px-0.5">
          <div className="flex items-center space-x-1.5">
            <span className="text-sm">🖴</span>
            <span>System Drives</span>
          </div>
          <span className="font-mono text-[#60CDFF]">{drives.length} Drives</span>
        </div>

        <div className="grid grid-cols-1 gap-1 max-h-48 overflow-y-auto custom-scrollbar pr-0.5">
          {drives.map((d) => {
            const driveLetter = d.mount_point.slice(0, 2).toUpperCase();
            const isCur = activeDrive === driveLetter;
            const total = d.total_bytes || 1;
            const free = d.available_bytes || 0;
            const used = Math.max(0, total - free);
            const usedPercent = Math.min(100, Math.max(0, Math.round((used / total) * 100)));

            return (
              <button
                key={d.mount_point}
                type="button"
                onClick={() => {
                  setActiveDrive(driveLetter);
                  const root = d.mount_point.endsWith("\\") ? d.mount_point : `${d.mount_point}\\`;
                  setDiskCurrentPath(root);
                  loadDiskDirectory(root);
                  setBrowserSource("filesystem");
                }}
                className={`w-full text-left p-1.5 rounded-md transition-all cursor-pointer flex flex-col space-y-1 border ${
                  isCur
                    ? "bg-gradient-to-r from-[#173042] to-[#12222E] border-[#60CDFF]/60 shadow-[0_0_12px_rgba(96,205,255,0.15)]"
                    : "bg-[#181818] hover:bg-[#252525] border-[#2A2A2A] text-white"
                }`}
                title={`Browse Drive ${driveLetter} (${d.name})`}
              >
                <div className="flex items-center justify-between gap-1.5">
                  <div className="flex items-center space-x-1.5 shrink-0">
                    <span className="text-xs shrink-0">{d.is_removable ? "🔌" : "🖴"}</span>
                    <span className="font-mono font-bold text-xs text-white bg-[#262626] px-1.5 py-0.5 rounded border border-[#3E3E3E] shrink-0">
                      {driveLetter}
                    </span>
                  </div>
                  {d.name && <span className="text-[10px] text-[#A8A8A8] truncate min-w-0 flex-1">{d.name}</span>}
                  <span className={`text-[10px] font-mono shrink-0 ${isCur ? "text-[#60CDFF] font-semibold" : "text-[#777777]"}`}>
                    {formatBytes(free)} free
                  </span>
                </div>
                <div className="w-full h-1 bg-[#282828] rounded-full overflow-hidden">
                  <div
                    className={`h-full rounded-full transition-all duration-300 ${
                      usedPercent > 90 ? "bg-rose-500" : isCur ? "bg-[#60CDFF]" : "bg-[#4A90E2]"
                    }`}
                    style={{ width: `${usedPercent}%` }}
                  />
                </div>
              </button>
            );
          })}
        </div>
      </div>
    );
  };


  // Center Table for Disk Filesystem Browsing
  const renderDiskFolderContents = () => {
    if (isLoadingDisk) {
      return (
        <div className="flex-1 flex flex-col items-center justify-center space-y-2 p-8 text-[#A8A8A8]">
          <div className="w-6 h-6 border-2 border-[#60CDFF] border-t-transparent rounded-full animate-spin" />
          <span className="text-xs">Reading directory contents...</span>
        </div>
      );
    }

    if (diskError) {
      const targetTerm = diskCurrentPath.replace(/^[a-zA-Z]:[/\\]*/, "").replace(/[/\\]+$/, "").toLowerCase();
      const smartSuggestion = quickAccess.find((qa) => {
        const qaLower = qa.name.toLowerCase();
        return (
          qaLower === targetTerm ||
          (targetTerm.length >= 3 && qaLower.startsWith(targetTerm)) ||
          (targetTerm.length >= 3 && targetTerm.startsWith(qaLower))
        );
      });

      return (
        <div className="flex-1 flex flex-col items-center justify-center p-6 space-y-4 text-center select-none max-w-lg mx-auto">
          <div className="w-12 h-12 rounded-full bg-amber-500/10 border border-amber-500/30 flex items-center justify-center text-2xl shadow-[0_0_20px_rgba(245,158,11,0.15)]">
            ⚠️
          </div>

          <div className="space-y-1">
            <h3 className="text-sm font-bold text-white tracking-wide">Path Not Found</h3>
            <p className="text-xs text-[#A8A8A8] max-w-sm">
              Bayzip could not locate or open the specified directory.
            </p>
          </div>

          <div className="w-full bg-[#1A1A1A] border border-[#2E2E2E] rounded-lg p-2.5 font-mono text-xs text-rose-300 break-all select-all flex items-center space-x-2">
            <span className="text-[#666666] shrink-0 font-sans">Location:</span>
            <span className="flex-1 truncate">{diskCurrentPath}</span>
          </div>

          {smartSuggestion && (
            <div className="w-full bg-gradient-to-r from-[#173042] to-[#12222E] border border-[#60CDFF]/40 rounded-lg p-3 text-left space-y-1.5 shadow-md">
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase tracking-wider text-[#60CDFF]">
                  Did you mean?
                </span>
                <span className="text-[10px] text-[#A8A8A8]">Quick Access</span>
              </div>
              <button
                type="button"
                onClick={() => navigateToDiskDirectory(smartSuggestion.path)}
                className="w-full py-1.5 px-3 bg-[#1C3E56] hover:bg-[#254F6D] text-[#60CDFF] hover:text-[#90DCFF] rounded text-xs font-semibold flex items-center justify-between transition-colors cursor-pointer border border-[#60CDFF]/50"
              >
                <div className="flex items-center space-x-2 truncate mr-2">
                  <span>📁</span>
                  <span className="font-bold">{smartSuggestion.name}</span>
                  <span className="text-[10px] text-[#A8A8A8] font-mono truncate">({smartSuggestion.path})</span>
                </div>
                <span className="text-xs font-bold shrink-0">Open →</span>
              </button>
            </div>
          )}

          {/* Action Row */}
          <div className="flex flex-wrap items-center justify-center gap-2 pt-1">
            <button
              type="button"
              onClick={() => setEditPathTrigger((prev) => prev + 1)}
              className="px-3.5 py-1.5 bg-[#252525] hover:bg-[#333333] text-xs font-medium text-white rounded-md border border-[#3E3E3E] hover:border-[#60CDFF]/60 cursor-pointer flex items-center space-x-1.5 transition-all shadow-sm"
            >
              <span>✏️</span>
              <span>Edit Path</span>
            </button>

            <button
              type="button"
              onClick={() => {
                const root = `${activeDrive || "C:"}\\`;
                navigateToDiskDirectory(root);
              }}
              className="px-3.5 py-1.5 bg-[#252525] hover:bg-[#333333] text-xs font-medium text-white rounded-md border border-[#3E3E3E] hover:border-[#60CDFF]/60 cursor-pointer flex items-center space-x-1.5 transition-all shadow-sm"
            >
              <span>🏠</span>
              <span>Go to {activeDrive || "C:"}\</span>
            </button>

            <button
              type="button"
              onClick={() => loadDiskDirectory(diskCurrentPath)}
              className="px-3.5 py-1.5 bg-[#173042] hover:bg-[#20495f] text-xs font-semibold text-[#60CDFF] rounded-md border border-[#60CDFF]/40 cursor-pointer flex items-center space-x-1.5 transition-all shadow-sm"
            >
              <span>↺</span>
              <span>Retry</span>
            </button>
          </div>

          {/* Quick Access Recovery Chips */}
          {quickAccess && quickAccess.length > 0 && (
            <div className="pt-2 border-t border-[#262626] w-full space-y-2">
              <span className="text-[10px] font-semibold text-[#666666] uppercase tracking-wider block">
                Quick Jump:
              </span>
              <div className="flex flex-wrap justify-center gap-1.5">
                {quickAccess.slice(0, 5).map((qa) => (
                  <button
                    key={qa.path}
                    type="button"
                    onClick={() => navigateToDiskDirectory(qa.path)}
                    className="px-2 py-1 rounded bg-[#202020] hover:bg-[#2A2A2A] text-[11px] text-[#CCCCCC] hover:text-white border border-[#303030] hover:border-[#444444] transition-colors cursor-pointer flex items-center space-x-1"
                  >
                    <span>📁</span>
                    <span>{qa.name}</span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      );
    }

    const filtered = searchQuery.trim()
      ? diskEntries.filter((item) => item.name.toLowerCase().includes(searchQuery.toLowerCase()))
      : diskEntries;

    const isNotRootDrive = !/^[a-zA-Z]:\\?$/.test(diskCurrentPath.trim());

    if (filtered.length === 0 && !isNotRootDrive) {
      return (
        <div
          onContextMenu={(e) => handleOpenContextMenu(e, null)}
          className="flex-1 flex items-center justify-center text-xs text-[#777777] p-8"
        >
          This folder is empty
        </div>
      );
    }

    return (
      <div
        className="flex-1 flex flex-col overflow-hidden relative h-full"
        onContextMenu={(e) => handleOpenContextMenu(e, null)}
      >
        {/* WinRAR / 7-Zip Standard Parent Directory Row */}
        {isNotRootDrive && (() => {
          const diskParentPath = (() => {
            const clean = diskCurrentPath.replace(/\//g, "\\").replace(/\\+$/, "");
            const idx = clean.lastIndexOf("\\");
            if (idx === -1) return clean;
            if (idx === 2 && clean.endsWith(":")) return clean + "\\";
            if (idx <= 2) return clean.substring(0, idx + 1);
            return clean.substring(0, idx);
          })();
          const parentDropProps = getFolderDropProps("parent-disk-up", "", true, diskParentPath);
          return (
            <div
              onDoubleClick={handleDiskUp}
              onClick={handleDiskUp}
              {...parentDropProps.handlers}
              style={{ height: "34px" }}
              className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#222222] select-none text-[#A8A8A8] hover:text-[#60CDFF] shrink-0 ${
                parentDropProps.isDropTarget ? "bg-[#173042] text-[#60CDFF] border-l-2 border-[#60CDFF]" : "hover:bg-[#252525]"
              }`}
              title="Navigate to parent folder (or drop items to move up)"
            >
              <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
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
          );
        })()}

        <VirtualScrollList
          items={filtered}
          rowHeight={34}
          className="flex-1 h-full"
          renderRow={(item) => {
            const isAlreadyStaged = Boolean(stagedItems && stagedItems.includes(item.path));
            const dropProps = item.is_dir ? getFolderDropProps(`disk-${item.path}`, "", true, item.path) : null;
            return (
              <div
                key={item.path}
                {...getDiskDragProps(item.path, item.is_dir)}
                {...(dropProps ? dropProps.handlers : {})}
                onContextMenu={(e) =>
                  handleOpenContextMenu(e, {
                    name: item.name,
                    isDir: item.is_dir,
                    diskPath: item.path,
                    isStaged: isAlreadyStaged,
                    size: item.size,
                  })
                }
                onDoubleClick={() => {
                  if (item.is_dir) {
                    navigateToDiskDirectory(item.path);
                  } else {
                    const isArchive = /\.(bay|bayg|zip|7z|tar|gz|bz2|xz|zst|rar)$/i.test(item.name);
                    if (isArchive && onLoadArchiveFromFile) {
                      onLoadArchiveFromFile(item.path);
                      if (onSwitchMode && mode !== "extract") onSwitchMode("extract");
                      setBrowserSource("archive");
                      return;
                    }
                    if (onPreviewEntry) onPreviewEntry(item.path);
                  }
                }}
                style={{ height: "34px" }}
                className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#222222] ${
                  dropProps?.isDropTarget
                    ? "bg-[#173042] text-[#60CDFF] font-semibold border-l-2 border-[#60CDFF] shadow-[0_0_12px_rgba(96,205,255,0.25)]"
                    : isAlreadyStaged
                      ? "bg-[#14261d]/70 hover:bg-[#1a3327] border-l-2 border-emerald-500/90 text-white"
                      : "hover:bg-[#242424]"
                }`}
              >
                <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                  {item.is_dir ? (
                    <svg className="w-4 h-4 text-[#F9D648] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                    </svg>
                  ) : (
                    <FileFormatIcon fileName={item.name} size="sm" />
                  )}
                  <span className={`font-mono text-[11px] truncate ${isAlreadyStaged ? "text-emerald-300 font-medium" : "text-white"}`} title={item.path}>
                    {item.name}
                  </span>
                  {isAlreadyStaged && (
                    <span className="text-[9px] px-1.5 py-0.2 rounded-full bg-emerald-950/90 border border-emerald-600/80 text-emerald-300 font-bold shrink-0">
                      Staged
                    </span>
                  )}
                </div>

                <div className="flex items-center space-x-3 shrink-0">
                  <span className="font-mono text-[11px] text-[#888888] w-20 text-right">
                    {item.is_dir ? "Folder" : formatBytes(item.size)}
                  </span>

                  {/* Stage / Unstage toggle button in Compress mode */}
                  {onStageItem && (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        if (isAlreadyStaged && onUnstageItem) {
                          onUnstageItem(item.path);
                        } else {
                          onStageItem(item.path);
                        }
                      }}
                      className={`px-2 py-0.5 rounded text-[10px] font-semibold transition-all cursor-pointer flex items-center space-x-1 ${
                        isAlreadyStaged
                          ? "bg-emerald-950/80 hover:bg-rose-950/80 text-emerald-300 hover:text-rose-300 border border-emerald-500/70 hover:border-rose-500/70 shadow-sm"
                          : "bg-[#173042] hover:bg-[#1C3E56] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/30 hover:border-[#60CDFF]/70 opacity-70 group-hover:opacity-100"
                      }`}
                      title={isAlreadyStaged ? `Remove "${item.name}" from archive staging` : `Stage "${item.name}" for archive`}
                    >
                      <span>{isAlreadyStaged ? "✓ Staged" : "+ Stage"}</span>
                    </button>
                  )}

                  {/* Open archive button if it's an archive file on disk */}
                  {!item.is_dir && /\.(bay|bayg|zip|7z|tar|gz|bz2|xz|zst|rar)$/i.test(item.name) && onLoadArchiveFromFile && (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onLoadArchiveFromFile(item.path);
                        if (onSwitchMode && mode !== "extract") onSwitchMode("extract");
                        setBrowserSource("archive");
                      }}
                      className="px-2 py-0.5 rounded text-[10px] font-semibold bg-[#25394A] hover:bg-[#324C63] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/50 transition-all cursor-pointer flex items-center space-x-1 shadow-sm"
                      title={`Open "${item.name}" in Bayzip Extract Inspector`}
                    >
                      <span>📦</span>
                      <span>Open</span>
                    </button>
                  )}
                </div>
              </div>
            );
          }}
        />
      </div>
    );
  };

  // Source Toggle at top of left panel: Staged/Archive Tree vs Drive Files
  const renderSourceToggle = () => {
    const isCompress = mode === "compress";
    const treeTitle = isCompress ? "Staged Tree" : "Archive Tree";
    const totalCount = localEntries?.length || archiveInfo.file_count || stagedItems?.length || 0;

    return (
      <div className="grid grid-cols-2 gap-1 bg-[#141414] p-1 rounded-lg border border-[#2E2E2E] mb-2 shrink-0">
        <button
          type="button"
          onClick={() => setBrowserSource("archive")}
          className={`py-1.5 px-2 text-[11px] rounded font-semibold transition-all flex items-center justify-center space-x-1.5 cursor-pointer ${
            browserSource === "archive"
              ? "bg-[#25394A] text-[#60CDFF] shadow-sm border border-[#60CDFF]/40 font-bold"
              : "text-[#888888] hover:text-white hover:bg-[#202020]"
          }`}
          title={`View ${treeTitle} (${totalCount} items)`}
        >
          <span>📦</span>
          <span className="truncate">{treeTitle}</span>
          {hasTreeItems && (
            <span className="text-[9px] px-1.5 py-0.2 rounded-full bg-[#12222E] text-[#60CDFF] font-mono shrink-0">
              {totalCount > 9999 ? `${Math.round(totalCount / 1000)}k` : totalCount}
            </span>
          )}
        </button>
        <button
          type="button"
          onClick={() => setBrowserSource("filesystem")}
          className={`py-1.5 px-2 text-[11px] rounded font-semibold transition-all flex items-center justify-center space-x-1.5 cursor-pointer ${
            browserSource === "filesystem"
              ? "bg-[#25394A] text-[#60CDFF] shadow-sm border border-[#60CDFF]/40 font-bold"
              : "text-[#888888] hover:text-white hover:bg-[#202020]"
          }`}
          title="Browse System Drives & Quick Access"
        >
          <span>📁</span>
          <span className="truncate">Drive Files</span>
        </button>
      </div>
    );
  };

  // Sleek Subheader for Left Pane Folder Hierarchy
  const renderFolderHierarchyHeader = (isDrawer: boolean = false) => (
    <div
      className={`flex items-center justify-between px-2 py-1 mb-1.5 shrink-0 relative ${
        isDrawer ? "border-b border-[#2E2E2E]" : ""
      }`}
    >
      <div className="flex items-center space-x-1.5">
        <span className="text-xs text-[#F9D648]">📁</span>
        <span className="text-[10px] font-bold tracking-wider uppercase text-[#CCCCCC]">
          {mode === "compress" ? "Staged Folders" : "Archive Folders"}
        </span>
      </div>
      <div className="flex items-center space-x-1.5">
        {/* + New Folder Button on Left Header */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            handleStartNewFolder();
          }}
          className="px-1.5 py-0.5 rounded bg-[#252525] hover:bg-[#303030] border border-[#3E3E3E] hover:border-[#60CDFF]/60 text-[#60CDFF] hover:text-[#90DCFF] text-[10px] font-mono font-semibold flex items-center space-x-0.5 transition-all cursor-pointer shadow-sm"
          title="Create New Folder (Ctrl+Shift+N)"
        >
          <span>📁+</span>
        </button>
        {isDrawer && (
          <button
            type="button"
            onClick={() => setShowMobileTree(false)}
            className="text-[#888888] hover:text-white text-xs px-1.5 py-0.5 rounded hover:bg-[#2A2A2A] cursor-pointer"
            title="Close Folder Hierarchy Drawer"
          >
            ✕
          </button>
        )}
      </div>
    </div>
  );

  // Recursive Tree Node Renderer for Left Pane
  const renderTreeFolderNode = (node: TreeNode, depth: number = 0) => {
    const isExpanded = expandedFolders.has(node.path);
    const isSelected = currentFolderPath === node.path;
    const hasChildren = node.subfolders.length > 0;
    const isFolderSkipped = isDirExcluded(node.path);
    const dropProps = getFolderDropProps(`tree-${node.path}`, node.path);

    return (
      <div key={node.path || "root"} className="select-none text-xs">
        <div
          {...getArchiveDragProps(node.path, true, node.name)}
          {...dropProps.handlers}
          onClick={() => navigateToFolder(node.path)}
          onContextMenu={(e) => handleOpenContextMenu(e, { name: node.path, isDir: true })}
          style={{ paddingLeft: `${Math.max(8, depth * 16 + 8)}px` }}
          className={`flex items-center justify-between py-1.5 pr-2 rounded-md transition-colors cursor-pointer group ${
            dropProps.isDropTarget
              ? "bg-[#173042] text-[#60CDFF] font-semibold border-l-2 border-[#60CDFF] shadow-[0_0_12px_rgba(96,205,255,0.25)]"
              : isSelected
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

          {/* Action & Badge Cluster */}
          <div className="flex items-center space-x-1 shrink-0">
            {/* Direct Unstage Button for Staged Folders */}
            {mode === "compress" && onUnstageItem && (
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  const targetPath =
                    findStagedItemPath(node.path || node.name) ||
                    (node.path === "" && stagedItems && stagedItems.length === 1 ? stagedItems[0] : null) ||
                    node.name;
                  if (targetPath) {
                    onUnstageItem(targetPath);
                  }
                  const cleanPrefix = node.path ? node.path.replace(/\/+$/, "") + "/" : "";
                  setLocalEntries((prev) =>
                    prev.filter((item) => {
                      if (!cleanPrefix) return false;
                      const cleanItem = item.name.replace(/\\/g, "/").replace(/^\/+/, "");
                      return !cleanItem.startsWith(cleanPrefix);
                    })
                  );
                  if (node.path === currentFolderPath) {
                    navigateToFolder("");
                  }
                }}
                className="opacity-0 group-hover:opacity-100 p-0.5 hover:bg-rose-950/80 text-[#888888] hover:text-rose-400 rounded transition-all cursor-pointer mr-0.5"
                title="Remove from staged"
              >
                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            )}

            {/* Badge */}
            {isFolderSkipped && node.path !== "" ? (
              <span className="text-[9px] font-semibold text-rose-400 px-1 py-0.2 rounded bg-rose-950/60 border border-rose-800/80 shrink-0">
                ✕
              </span>
            ) : (
              <span className="text-[10px] font-mono text-[#888888] px-1.5 py-0.2 rounded bg-[#2A2A2A] border border-[#3A3A3A] shrink-0">
                {node.totalItemCount}
              </span>
            )}
          </div>
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
          <span>{archiveInfo.is_archive === false ? "📁" : "📦"}</span>
          <span>{archiveInfo.filename || (archiveInfo.is_archive === false ? "Staged Root" : "Archive")}</span>
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

      <span className="text-[11px] text-[#777777] shrink-0 pl-2 hidden sm:inline">
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
          placeholder={
            viewMode === "tree"
              ? "Filter files in current folder..."
              : archiveInfo.is_archive === false
              ? "Search all staged files..."
              : "Search all files in archive..."
          }
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

  // Pane 1 Subheader
  const renderPane1Header = () => (
    <div className="px-3 py-1.5 bg-[#202020] border-b border-[#2A2A2A] text-[11px] font-mono text-[#A8A8A8] flex items-center justify-between shrink-0 select-none">
      <div className="flex items-center space-x-2 truncate min-w-0 flex-1 mr-2">
        <div className="flex items-center space-x-0.5 bg-[#141414] p-0.5 rounded border border-[#303030] shrink-0">
          <button
            type="button"
            onClick={() => setBrowserSource("filesystem")}
            className={`px-1.5 py-0.5 rounded text-[10px] font-semibold transition-all cursor-pointer ${
              browserSource === "filesystem" ? "bg-[#25394A] text-[#60CDFF]" : "text-[#777] hover:text-white"
            }`}
            title="Browse local disk drives"
          >
            📁 Disk
          </button>
          <button
            type="button"
            onClick={() => setBrowserSource("archive")}
            className={`px-1.5 py-0.5 rounded text-[10px] font-semibold transition-all cursor-pointer ${
              browserSource === "archive" ? "bg-[#25394A] text-[#60CDFF]" : "text-[#777] hover:text-white"
            }`}
            title="View archive contents"
          >
            📦 {mode === "compress" ? "Staged" : "Archive"}
          </button>
        </div>
        {isSplitView && <span className="text-[10px] text-[#666666] shrink-0 font-bold">Pane 1:</span>}
        <span className="truncate text-white font-semibold">
          {browserSource === "filesystem" ? diskCurrentPath : `/${currentFolderPath || ""}`}
        </span>
      </div>
      <div className="flex items-center space-x-2 shrink-0">
        <span className="text-[10px] text-[#60CDFF] font-medium">
          {browserSource === "filesystem" ? `${diskEntries.length} items` : `${currentFolderNode.totalItemCount} items`}
        </span>
      </div>
    </div>
  );

  // Pane 2 Subheader
  const renderPane2Header = () => {
    const p2Node = pane2FolderNode;
    return (
      <div className="px-3 py-1.5 bg-[#202020] border-b border-[#2A2A2A] text-[11px] font-mono text-[#A8A8A8] flex items-center justify-between shrink-0 select-none">
        <div className="flex items-center space-x-2 truncate min-w-0 flex-1 mr-2">
          <div className="flex items-center space-x-0.5 bg-[#141414] p-0.5 rounded border border-[#303030] shrink-0">
            <button
              type="button"
              onClick={() => {
                setPane2Source("filesystem");
                if (pane2DiskEntries.length === 0) {
                  loadPane2DiskDirectory(pane2DiskPath || "C:\\");
                }
              }}
              className={`px-1.5 py-0.5 rounded text-[10px] font-semibold transition-all cursor-pointer ${
                pane2Source === "filesystem" ? "bg-[#25394A] text-[#60CDFF]" : "text-[#777] hover:text-white"
              }`}
              title="Browse local disk drives in Pane 2"
            >
              📁 Disk
            </button>
            <button
              type="button"
              onClick={() => setPane2Source("archive")}
              className={`px-1.5 py-0.5 rounded text-[10px] font-semibold transition-all cursor-pointer ${
                pane2Source === "archive" ? "bg-[#25394A] text-[#60CDFF]" : "text-[#777] hover:text-white"
              }`}
              title="View archive contents in Pane 2"
            >
              📦 {mode === "compress" ? "Staged" : "Archive"}
            </button>
          </div>
          <span className="text-[10px] text-[#666666] shrink-0 font-bold">Pane 2:</span>
          <span className="truncate text-white font-semibold">
            {pane2Source === "filesystem" ? pane2DiskPath : `/${pane2FolderPath || ""}`}
          </span>
        </div>
        <div className="flex items-center space-x-2 shrink-0">
          <span className="text-[10px] text-[#60CDFF] font-medium">
            {pane2Source === "filesystem" ? `${pane2DiskEntries.length} items` : `${p2Node.totalItemCount} items`}
          </span>
          <button
            type="button"
            onClick={() => setIsSplitView(false)}
            className="p-1 rounded text-[#888888] hover:text-white hover:bg-[#333333] transition-colors cursor-pointer text-xs leading-none font-bold"
            title="Close Split View (Return to single pane)"
          >
            ✕
          </button>
        </div>
      </div>
    );
  };

  // Pane 2 Archive Contents Renderer
  const renderPane2ArchiveContents = () => {
    const p2Node = pane2FolderNode;
    const p2Files = searchQuery.trim()
      ? p2Node.files.filter((f) => f.name.toLowerCase().includes(searchQuery.toLowerCase()))
      : p2Node.files;

    type FolderItem =
      | { kind: "folder"; data: TreeNode }
      | { kind: "file"; data: ArchiveEntryItem };

    const items: FolderItem[] = [
      ...p2Node.subfolders.map((node) => ({ kind: "folder" as const, data: node })),
      ...p2Files.map((file) => ({ kind: "file" as const, data: file })),
    ];

    if (items.length === 0 && !isCreatingFolder) {
      return (
        <div
          onContextMenu={(e) => handleOpenContextMenu(e, null)}
          className="p-8 text-center text-xs text-[#888888] flex flex-col items-center justify-center space-y-2 flex-1 h-full"
        >
          <span>{searchQuery ? "No matching files in this folder" : "This folder is empty"}</span>
          <button
            type="button"
            onClick={handleStartNewFolder}
            className="px-3 py-1 bg-[#25394A] hover:bg-[#2C4459] text-[#60CDFF] rounded border border-[#60CDFF]/40 text-xs font-medium cursor-pointer transition-colors shadow-sm"
          >
            + Create New Folder
          </button>
        </div>
      );
    }

    return (
      <div
        className="flex-1 flex flex-col overflow-hidden relative h-full"
        onContextMenu={(e) => handleOpenContextMenu(e, null)}
      >
        {pane2FolderPath !== "" && (() => {
          const p2ParentFolderPath = pane2FolderPath.includes("/")
            ? pane2FolderPath.substring(0, pane2FolderPath.lastIndexOf("/"))
            : "";
          const parentDropProps = getFolderDropProps("p2-parent-archive-up", p2ParentFolderPath);
          return (
            <div
              onDoubleClick={handlePane2ArchiveUp}
              onClick={handlePane2ArchiveUp}
              {...parentDropProps.handlers}
              style={{ height: "34px" }}
              className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#222222] select-none text-[#A8A8A8] hover:text-[#60CDFF] shrink-0 ${
                parentDropProps.isDropTarget ? "bg-[#173042] text-[#60CDFF] border-l-2 border-[#60CDFF]" : "hover:bg-[#252525]"
              }`}
              title="Navigate up to parent folder (or drop items to move up)"
            >
              <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                <span className="text-base font-bold text-[#F9D648] shrink-0 leading-none">📁</span>
                <span className="font-mono text-[12px] font-bold text-white group-hover:text-[#60CDFF]">
                  ..
                </span>
                <span className="text-[10px] text-[#777777] italic font-sans">[Parent Directory]</span>
              </div>
              <span className="text-[10px] text-[#888888] font-mono">Up ⮥</span>
            </div>
          );
        })()}

        <VirtualScrollList
          key={`pane2-archive-${pane2FolderPath}-${searchQuery}`}
          items={items}
          rowHeight={34}
          className="flex-1 h-full"
          renderRow={(item) => {
            if (item.kind === "folder") {
              const sub = item.data;
              const dropProps = getFolderDropProps(`p2-folder-${sub.path}`, sub.path);
              return (
                <div
                  key={`p2-folder-${sub.path}`}
                  {...getArchiveDragProps(sub.path, true, sub.name)}
                  {...dropProps.handlers}
                  onDoubleClick={() => setPane2FolderPath(sub.path)}
                  onContextMenu={(e) => handleOpenContextMenu(e, { name: sub.path, isDir: true })}
                  style={{ height: "34px" }}
                  className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#222222] ${
                    dropProps.isDropTarget
                      ? "bg-[#173042] text-[#60CDFF] font-semibold border-l-2 border-[#60CDFF] shadow-[0_0_12px_rgba(96,205,255,0.25)]"
                      : "hover:bg-[#252525]"
                  }`}
                >
                  <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                    <span className="text-base font-bold text-[#F9D648] shrink-0 leading-none">📁</span>
                    <span className="font-mono text-[11px] truncate text-white group-hover:text-[#60CDFF]">
                      {sub.name}
                    </span>
                  </div>
                  <span className="font-mono text-[11px] text-[#888888] shrink-0">
                    {sub.totalItemCount} items
                  </span>
                </div>
              );
            }
            const file = item.data;
            const fileName = file.name.split("/").pop() || file.name;
            return (
              <div
                key={`p2-file-${file.name}`}
                {...getArchiveDragProps(file.name, false, fileName)}
                onDoubleClick={() => onPreviewEntry(file.name)}
                onContextMenu={(e) => handleOpenContextMenu(e, { name: file.name, isDir: false, size: file.size })}
                style={{ height: "34px" }}
                className="flex items-center justify-between px-3 text-xs hover:bg-[#252525] transition-colors cursor-pointer group box-border border-b border-[#222222]"
              >
                <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                  <FileFormatIcon fileName={fileName} size="sm" />
                  <span className="font-mono text-[11px] truncate text-[#CCCCCC] group-hover:text-white">
                    {fileName}
                  </span>
                </div>
                <span className="font-mono text-[11px] text-[#888888] shrink-0">
                  {formatBytes(file.size)}
                </span>
              </div>
            );
          }}
        />
      </div>
    );
  };

  // Pane 2 Disk Contents Renderer
  const renderPane2DiskContents = () => {
    if (isLoadingPane2Disk) {
      return (
        <div className="flex-1 flex flex-col items-center justify-center space-y-2 p-8 text-[#A8A8A8]">
          <div className="w-6 h-6 border-2 border-[#60CDFF] border-t-transparent rounded-full animate-spin" />
          <span className="text-xs">Reading directory contents...</span>
        </div>
      );
    }

    if (pane2DiskError) {
      const targetTerm = pane2DiskPath.replace(/^[a-zA-Z]:[/\\]*/, "").replace(/[/\\]+$/, "").toLowerCase();
      const smartSuggestion = quickAccess.find((qa) => {
        const qaLower = qa.name.toLowerCase();
        return (
          qaLower === targetTerm ||
          (targetTerm.length >= 3 && qaLower.startsWith(targetTerm)) ||
          (targetTerm.length >= 3 && targetTerm.startsWith(qaLower))
        );
      });

      return (
        <div className="flex-1 flex flex-col items-center justify-center p-6 space-y-4 text-center select-none max-w-lg mx-auto">
          <div className="w-12 h-12 rounded-full bg-amber-500/10 border border-amber-500/30 flex items-center justify-center text-2xl shadow-[0_0_20px_rgba(245,158,11,0.15)]">
            ⚠️
          </div>

          <div className="space-y-1">
            <h3 className="text-sm font-bold text-white tracking-wide">Path Not Found (Pane 2)</h3>
            <p className="text-xs text-[#A8A8A8] max-w-sm">
              Bayzip could not locate or open the specified directory.
            </p>
          </div>

          <div className="w-full bg-[#1A1A1A] border border-[#2E2E2E] rounded-lg p-2.5 font-mono text-xs text-rose-300 break-all select-all flex items-center space-x-2">
            <span className="text-[#666666] shrink-0 font-sans">Location:</span>
            <span className="flex-1 truncate">{pane2DiskPath}</span>
          </div>

          {smartSuggestion && (
            <div className="w-full bg-gradient-to-r from-[#173042] to-[#12222E] border border-[#60CDFF]/40 rounded-lg p-3 text-left space-y-1.5 shadow-md">
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase tracking-wider text-[#60CDFF]">
                  Did you mean?
                </span>
                <span className="text-[10px] text-[#A8A8A8]">Quick Access</span>
              </div>
              <button
                type="button"
                onClick={() => loadPane2DiskDirectory(smartSuggestion.path)}
                className="w-full py-1.5 px-3 bg-[#1C3E56] hover:bg-[#254F6D] text-[#60CDFF] hover:text-[#90DCFF] rounded text-xs font-semibold flex items-center justify-between transition-colors cursor-pointer border border-[#60CDFF]/50"
              >
                <div className="flex items-center space-x-2 truncate mr-2">
                  <span>📁</span>
                  <span className="font-bold">{smartSuggestion.name}</span>
                  <span className="text-[10px] text-[#A8A8A8] font-mono truncate">({smartSuggestion.path})</span>
                </div>
                <span className="text-xs font-bold shrink-0">Open →</span>
              </button>
            </div>
          )}

          {/* Action Row */}
          <div className="flex flex-wrap items-center justify-center gap-2 pt-1">
            <button
              type="button"
              onClick={() => loadPane2DiskDirectory("C:\\")}
              className="px-3.5 py-1.5 bg-[#252525] hover:bg-[#333333] text-xs font-medium text-white rounded-md border border-[#3E3E3E] hover:border-[#60CDFF]/60 cursor-pointer flex items-center space-x-1.5 transition-all shadow-sm"
            >
              <span>🏠</span>
              <span>Go to C:\</span>
            </button>

            <button
              type="button"
              onClick={() => loadPane2DiskDirectory(pane2DiskPath)}
              className="px-3.5 py-1.5 bg-[#173042] hover:bg-[#20495f] text-xs font-semibold text-[#60CDFF] rounded-md border border-[#60CDFF]/40 cursor-pointer flex items-center space-x-1.5 transition-all shadow-sm"
            >
              <span>↺</span>
              <span>Retry</span>
            </button>
          </div>
        </div>
      );
    }

    const filtered = searchQuery.trim()
      ? pane2DiskEntries.filter((e) => e.name.toLowerCase().includes(searchQuery.toLowerCase()))
      : pane2DiskEntries;

    const isNotRoot = !/^[a-zA-Z]:\\?$/.test(pane2DiskPath.trim());

    return (
      <div
        className="flex-1 flex flex-col overflow-hidden relative h-full"
        onContextMenu={(e) => handleOpenContextMenu(e, null)}
      >
        {isNotRoot && (() => {
          const p2DiskParentPath = (() => {
            const clean = pane2DiskPath.replace(/\//g, "\\").replace(/\\+$/, "");
            const idx = clean.lastIndexOf("\\");
            if (idx === -1) return clean;
            if (idx === 2 && clean.endsWith(":")) return clean + "\\";
            if (idx <= 2) return clean.substring(0, idx + 1);
            return clean.substring(0, idx);
          })();
          const parentDropProps = getFolderDropProps("p2-parent-disk-up", "", true, p2DiskParentPath);
          return (
            <div
              onDoubleClick={handlePane2DiskUp}
              onClick={handlePane2DiskUp}
              {...parentDropProps.handlers}
              style={{ height: "34px" }}
              className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#222222] select-none text-[#A8A8A8] hover:text-[#60CDFF] shrink-0 ${
                parentDropProps.isDropTarget ? "bg-[#173042] text-[#60CDFF] border-l-2 border-[#60CDFF]" : "hover:bg-[#252525]"
              }`}
              title="Navigate up to parent folder (or drop items to move up)"
            >
              <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                <span className="text-base font-bold text-[#F9D648] shrink-0 leading-none">📁</span>
                <span className="font-mono text-[12px] font-bold text-white group-hover:text-[#60CDFF]">
                  ..
                </span>
                <span className="text-[10px] text-[#777777] italic font-sans">[Parent Directory]</span>
              </div>
              <span className="text-[10px] text-[#888888] font-mono">Up ⮥</span>
            </div>
          );
        })()}

        <VirtualScrollList
          items={filtered}
          rowHeight={34}
          className="flex-1 h-full"
          renderRow={(item) => {
            const isAlreadyStaged = Boolean(stagedItems && stagedItems.includes(item.path));
            const dropProps = item.is_dir ? getFolderDropProps(`p2-disk-${item.path}`, "", true, item.path) : null;
            return (
              <div
                key={`p2-disk-${item.path}`}
                {...getDiskDragProps(item.path, item.is_dir)}
                {...(dropProps ? dropProps.handlers : {})}
                onContextMenu={(e) =>
                  handleOpenContextMenu(e, {
                    name: item.name,
                    isDir: item.is_dir,
                    diskPath: item.path,
                    isStaged: isAlreadyStaged,
                    size: item.size,
                  })
                }
                onDoubleClick={() => {
                  if (item.is_dir) {
                    loadPane2DiskDirectory(item.path);
                  } else {
                    if (onPreviewEntry) onPreviewEntry(item.path);
                  }
                }}
                style={{ height: "34px" }}
                className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#222222] ${
                  dropProps?.isDropTarget
                    ? "bg-[#173042] text-[#60CDFF] font-semibold border-l-2 border-[#60CDFF] shadow-[0_0_12px_rgba(96,205,255,0.25)]"
                    : isAlreadyStaged
                      ? "bg-[#14261d]/70 hover:bg-[#1a3327] border-l-2 border-emerald-500/90 text-white"
                      : "hover:bg-[#242424]"
                }`}
              >
                <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                  {item.is_dir ? (
                    <svg className="w-4 h-4 text-[#F9D648] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                    </svg>
                  ) : (
                    <FileFormatIcon fileName={item.name} size="sm" />
                  )}
                  <span className={`font-mono text-[11px] truncate ${isAlreadyStaged ? "text-emerald-300 font-medium" : "text-white"}`} title={item.path}>
                    {item.name}
                  </span>
                  {isAlreadyStaged && (
                    <span className="text-[9px] px-1.5 py-0.2 rounded-full bg-emerald-950/90 border border-emerald-600/80 text-emerald-300 font-bold shrink-0">
                      Staged
                    </span>
                  )}
                </div>

                <div className="flex items-center space-x-3 shrink-0">
                  <span className="font-mono text-[11px] text-[#888888] w-20 text-right">
                    {item.is_dir ? "Folder" : formatBytes(item.size)}
                  </span>
                  {onStageItem && (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        if (isAlreadyStaged && onUnstageItem) {
                          onUnstageItem(item.path);
                        } else {
                          onStageItem(item.path);
                        }
                      }}
                      className={`px-2 py-0.5 rounded text-[10px] font-semibold transition-all cursor-pointer flex items-center space-x-1 ${
                        isAlreadyStaged
                          ? "bg-emerald-950/80 hover:bg-rose-950/80 text-emerald-300 hover:text-rose-300 border border-emerald-500/70 hover:border-rose-500/70 shadow-sm"
                          : "bg-[#173042] hover:bg-[#1C3E56] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/30 hover:border-[#60CDFF]/70 opacity-70 group-hover:opacity-100"
                      }`}
                    >
                      <span>{isAlreadyStaged ? "✓ Staged" : "+ Stage"}</span>
                    </button>
                  )}
                </div>
              </div>
            );
          }}
        />
      </div>
    );
  };

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

    if (!hasItems && !isCreatingFolder) {
      if (!hasArchiveLoaded && mode === "extract") {
        return (
          <div className="flex-1 flex flex-col items-center justify-center p-8 text-center space-y-4">
            <div className="w-16 h-16 rounded-2xl bg-[#1C1C1C] border border-[#333333] flex items-center justify-center text-3xl shadow-xl">
              📦
            </div>
            <div className="space-y-1">
              <h3 className="text-base font-bold text-white tracking-wide">No Archive Loaded</h3>
              <p className="text-xs text-[#888888] max-w-sm">
                Select an archive file to inspect and extract contents, or browse your local drives on the left.
              </p>
            </div>
            <div className="flex items-center space-x-3 pt-2">
              {onSelectArchive && (
                <button
                  type="button"
                  onClick={onSelectArchive}
                  className="px-4 py-2 bg-gradient-to-r from-[#173042] to-[#12222E] hover:from-[#1F415A] hover:to-[#172D3D] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/40 hover:border-[#60CDFF]/80 rounded-lg text-xs font-semibold shadow-[0_0_15px_rgba(96,205,255,0.2)] transition-all cursor-pointer flex items-center space-x-2"
                >
                  <span>📂</span>
                  <span>Select Archive to Extract...</span>
                </button>
              )}
              <button
                type="button"
                onClick={() => setBrowserSource("filesystem")}
                className="px-3.5 py-2 bg-[#252525] hover:bg-[#303030] text-white border border-[#404040] rounded-lg text-xs font-medium transition-all cursor-pointer flex items-center space-x-1.5"
              >
                <span>🖥️</span>
                <span>Browse Local Drives</span>
              </button>
            </div>
          </div>
        );
      }

      return (
        <div
          onContextMenu={(e) => handleOpenContextMenu(e, null)}
          onDragOver={(e) => {
            if (mode === "compress") {
              e.preventDefault();
              e.dataTransfer.dropEffect = "copy";
            }
          }}
          onDrop={(e) => {
            if (mode === "compress") {
              e.preventDefault();
              const diskPath =
                e.dataTransfer.getData("application/bayzip-disk-path") ||
                e.dataTransfer.getData("text/plain");
              if (diskPath && onStageItem) {
                onStageItem(diskPath);
              }
            }
          }}
          className="p-8 text-center text-xs text-[#888888] flex flex-col items-center justify-center space-y-2 flex-1 h-full"
        >
          <span>{searchQuery ? "No matching files in this folder" : "This folder is empty"}</span>
          <button
            type="button"
            onClick={handleStartNewFolder}
            className="px-3 py-1 bg-[#25394A] hover:bg-[#2C4459] text-[#60CDFF] rounded border border-[#60CDFF]/40 text-xs font-medium cursor-pointer transition-colors shadow-sm"
          >
            + Create New Folder
          </button>
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
      <div
        className="flex-1 flex flex-col overflow-hidden relative h-full"
        onContextMenu={(e) => handleOpenContextMenu(e, null)}
        onDragOver={(e) => {
          if (mode === "compress") {
            e.preventDefault();
            e.dataTransfer.dropEffect = "copy";
          }
        }}
        onDrop={(e) => {
          if (mode === "compress") {
            e.preventDefault();
            const diskPath =
              e.dataTransfer.getData("application/bayzip-disk-path") ||
              e.dataTransfer.getData("text/plain");
            if (diskPath && onStageItem) {
              onStageItem(diskPath);
            }
          }
        }}
      >
        {/* Inline New Folder Input Row */}
        {isCreatingFolder && (
          <div
            style={{ height: "34px" }}
            className="flex items-center justify-between px-3 text-xs bg-[#25394A]/80 border-l-2 border-[#60CDFF] box-border shrink-0 z-10 animate-in fade-in duration-100"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
              <svg className="w-4 h-4 text-[#F9D648] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
              </svg>
              <input
                type="text"
                autoFocus
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleCommitNewFolder();
                  else if (e.key === "Escape") handleCancelNewFolder();
                }}
                ref={(el) => {
                  if (el) el.select();
                }}
                className="bg-[#141414] text-white border border-[#60CDFF] rounded px-2 py-0.5 font-mono text-xs outline-none w-56 shadow-inner"
                placeholder="Folder name..."
              />
            </div>
            <div className="flex items-center space-x-1.5 shrink-0">
              <button
                type="button"
                onClick={handleCommitNewFolder}
                className="p-1 rounded bg-[#10B981]/20 hover:bg-[#10B981]/40 text-[#10B981] border border-[#10B981]/50 cursor-pointer text-xs"
                title="Create folder (Enter)"
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                </svg>
              </button>
              <button
                type="button"
                onClick={handleCancelNewFolder}
                className="p-1 rounded bg-rose-950/40 hover:bg-rose-900/60 text-rose-300 border border-rose-700/60 cursor-pointer text-xs"
                title="Cancel (Esc)"
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          </div>
        )}
        {/* WinRAR / 7-Zip Standard Parent Directory Row */}
        {currentFolderPath !== "" && (() => {
          const parentFolderPath = currentFolderPath.includes("/")
            ? currentFolderPath.substring(0, currentFolderPath.lastIndexOf("/"))
            : "";
          const parentDropProps = getFolderDropProps("parent-archive-up", parentFolderPath);
          return (
            <div
              onDoubleClick={navigateUp}
              onClick={navigateUp}
              {...parentDropProps.handlers}
              style={{ height: "34px" }}
              className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border border-b border-[#222222] select-none text-[#A8A8A8] hover:text-[#60CDFF] shrink-0 ${
                parentDropProps.isDropTarget ? "bg-[#173042] text-[#60CDFF] border-l-2 border-[#60CDFF]" : "hover:bg-[#252525]"
              }`}
              title="Navigate to parent folder (or drop items to move up)"
            >
              <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
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
          );
        })()}

        <VirtualScrollList
          key={`folder-virtual-${currentFolderPath}-${searchQuery}`}
          items={folderItems}
          rowHeight={34}
          className="flex-1 h-full"
          renderRow={(item) => {
            if (item.kind === "folder") {
              const sub = item.data;
              const isFolderSkipped = isDirExcluded(sub.path);
              const isRenaming = renamingItem && renamingItem.originalName === sub.path;
              const matchedStagedPath = mode === "compress" ? findStagedItemPath(sub.path) : null;
              const dropProps = getFolderDropProps(`folder-${sub.path}`, sub.path);
              return (
                <div
                  key={`folder-${sub.path}`}
                  {...getArchiveDragProps(sub.path, true, sub.name)}
                  {...dropProps.handlers}
                  onDoubleClick={() => navigateToFolder(sub.path)}
                  onContextMenu={(e) => handleOpenContextMenu(e, { name: sub.path, isDir: true })}
                  onClick={() => {
                    setSelectedEntryName(sub.path);
                  }}
                  style={{ height: "34px" }}
                  className={`flex items-center justify-between px-3 text-xs transition-colors cursor-pointer group box-border ${
                    dropProps.isDropTarget
                      ? "bg-[#173042] text-[#60CDFF] font-semibold border-l-2 border-[#60CDFF] shadow-[0_0_12px_rgba(96,205,255,0.25)]"
                      : isFolderSkipped
                        ? "opacity-50 hover:opacity-85 bg-rose-950/15 hover:bg-rose-950/25 border-l-2 border-rose-500/80"
                        : "hover:bg-[#252525]"
                  }`}
                >
                  <div className="flex items-center space-x-2.5 min-w-0 flex-1 mr-3">
                    <svg className="w-4 h-4 text-[#F9D648] shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                    </svg>
                    {isRenaming ? (
                      <div className="flex items-center space-x-1.5 flex-1" onClick={(e) => e.stopPropagation()}>
                        <input
                          type="text"
                          autoFocus
                          value={renamingName}
                          onChange={(e) => setRenamingName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") handleCommitRename();
                            else if (e.key === "Escape") handleCancelRename();
                          }}
                          ref={(el) => {
                            if (el) el.select();
                          }}
                          className="bg-[#141414] text-white border border-[#60CDFF] rounded px-2 py-0.5 font-mono text-xs outline-none w-52 shadow-inner"
                        />
                        <button
                          type="button"
                          onClick={handleCommitRename}
                          className="px-1.5 py-0.5 rounded bg-[#10B981]/20 hover:bg-[#10B981]/40 text-[#10B981] border border-[#10B981]/50 cursor-pointer text-xs"
                        >
                          ✓
                        </button>
                        <button
                          type="button"
                          onClick={handleCancelRename}
                          className="px-1.5 py-0.5 rounded bg-rose-950/40 hover:bg-rose-900/60 text-rose-300 border border-rose-700/60 cursor-pointer text-xs"
                        >
                          ✕
                        </button>
                      </div>
                    ) : (
                      <span
                        className={`font-mono text-[11px] truncate transition-colors font-medium ${
                          isFolderSkipped ? "text-rose-300/80 line-through" : "text-white group-hover:text-[#60CDFF]"
                        }`}
                      >
                        {sub.name}/
                      </span>
                    )}
                    {isFolderSkipped && !isRenaming && (
                      <span className="text-[10px] font-semibold px-2 py-0.2 rounded-full bg-rose-950/70 border border-rose-800 text-rose-300 shrink-0">
                        ✕ Folder Excluded
                      </span>
                    )}
                  </div>
                  <div className="flex items-center space-x-2 shrink-0">
                    <span className="text-[#777777] font-mono text-[10px]">
                      {sub.totalItemCount} item{sub.totalItemCount !== 1 ? "s" : ""}
                    </span>
                    {matchedStagedPath && onUnstageItem ? (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onUnstageItem(matchedStagedPath);
                        }}
                        className="text-[10px] px-2 py-0.5 rounded bg-rose-950/40 hover:bg-rose-900/70 text-rose-300 hover:text-white border border-rose-800/60 hover:border-rose-600 transition-colors flex items-center space-x-1 cursor-pointer"
                        title="Remove this folder from archive selection"
                      >
                        <span className="font-bold">✕</span>
                        <span>Remove</span>
                      </button>
                    ) : onToggleExclude ? (
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
                    ) : null}
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
            const fileNameOnly = file.name.split(/[/\\]/).pop() || file.name;
            const isExcluded = isFileExcluded(file.name);
            const isRenaming = renamingItem && renamingItem.originalName === file.name;
            const matchedStagedFilePath = mode === "compress" ? findStagedItemPath(file.name) : null;

            return (
              <div
                key={`file-${file.name}`}
                {...getArchiveDragProps(file.name, false, fileNameOnly)}
                onContextMenu={(e) => handleOpenContextMenu(e, { name: file.name, isDir: false, size: file.size })}
                onClick={() => {
                  setSelectedEntryName(file.name);
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
                  {isRenaming ? (
                    <div className="flex items-center space-x-1.5 flex-1" onClick={(e) => e.stopPropagation()}>
                      <input
                        type="text"
                        autoFocus
                        value={renamingName}
                        onChange={(e) => setRenamingName(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") handleCommitRename();
                          else if (e.key === "Escape") handleCancelRename();
                        }}
                        ref={(el) => {
                          if (el) el.select();
                        }}
                        className="bg-[#141414] text-white border border-[#60CDFF] rounded px-2 py-0.5 font-mono text-xs outline-none w-52 shadow-inner"
                      />
                      <button
                        type="button"
                        onClick={handleCommitRename}
                        className="px-1.5 py-0.5 rounded bg-[#10B981]/20 hover:bg-[#10B981]/40 text-[#10B981] border border-[#10B981]/50 cursor-pointer text-xs"
                      >
                        ✓
                      </button>
                      <button
                        type="button"
                        onClick={handleCancelRename}
                        className="px-1.5 py-0.5 rounded bg-rose-950/40 hover:bg-rose-900/60 text-rose-300 border border-rose-700/60 cursor-pointer text-xs"
                      >
                        ✕
                      </button>
                    </div>
                  ) : (
                    <span
                      className={`font-mono text-[11px] truncate transition-colors ${
                        isExcluded ? "text-rose-300/80 line-through" : "text-white group-hover:text-[#60CDFF]"
                      }`}
                      title={`${file.name} (Right-click for Menu | Double-click or Spacebar to Preview)`}
                    >
                      {fileNameOnly}
                    </span>
                  )}
                  {isExcluded && !isRenaming && (
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
                  {matchedStagedFilePath && onUnstageItem ? (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onUnstageItem(matchedStagedFilePath);
                      }}
                      className="text-[10px] px-2 py-0.5 rounded bg-rose-950/40 hover:bg-rose-900/70 text-rose-300 hover:text-white border border-rose-800/60 hover:border-rose-600 transition-colors flex items-center space-x-1 cursor-pointer"
                      title="Remove this file from archive selection"
                    >
                      <span className="font-bold">✕</span>
                      <span>Remove</span>
                    </button>
                  ) : onToggleExclude ? (
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
                  ) : null}
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
          }}
        />
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
      <div
        className="flex-1 flex flex-col overflow-hidden relative h-full"
        onContextMenu={(e) => handleOpenContextMenu(e, null)}
      >
        <VirtualScrollList
          key={`flat-virtual-${searchQuery}`}
          items={filteredFlatEntries}
          rowHeight={34}
          className="flex-1 h-full"
          renderRow={(entry, idx) => {
            const isSelected = selectedEntryName === entry.name;
            const fileNameOnly = entry.name.split(/[/\\]/).pop() || entry.name;
            const isExcluded = !entry.is_dir && isFileExcluded(entry.name);
            const isFolderSkipped = entry.is_dir && isDirExcluded(entry.name);
            const isDimmed = isExcluded || isFolderSkipped;
            const matchedStagedEntryPath = mode === "compress" ? findStagedItemPath(entry.name) : null;

            return (
              <div
                key={`flat-${entry.name || idx}`}
                onContextMenu={(e) => handleOpenContextMenu(e, { name: entry.name, isDir: entry.is_dir, size: entry.size })}
                onClick={() => {
                  setSelectedEntryName(entry.name);
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
                {matchedStagedEntryPath && onUnstageItem ? (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onUnstageItem(matchedStagedEntryPath);
                    }}
                    className="text-[10px] px-2 py-0.5 rounded bg-rose-950/40 hover:bg-rose-900/70 text-rose-300 hover:text-white border border-rose-800/60 hover:border-rose-600 transition-colors flex items-center space-x-1 cursor-pointer"
                    title="Remove this item from archive selection"
                  >
                    <span className="font-bold">✕</span>
                    <span>Remove</span>
                  </button>
                ) : onToggleExclude ? (
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
                ) : null}
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
        }}
      />
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
        <WorkspaceHeader
          isFullModal={false}
          mode={mode}
          inspectorTitle={inspectorTitle}
          itemCount={archiveInfo.file_count || 0}
          operationProgress={operationProgress}
          isHeaderHUDOpen={isHeaderHUDOpen}
          onToggleHeaderHUD={() => setIsHeaderHUDOpen((prev) => !prev)}
          onCloseHeaderHUD={() => setIsHeaderHUDOpen(false)}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          onStartNewFolder={handleStartNewFolder}
          onExpandToWorkspace={() => setIsExpandedModal(true)}
        />

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

        {/* Running Bottom Loading Line (Green) */}
        {isLoading && (
          <div className="w-full h-1 bg-[#1A1A1A] overflow-hidden">
            <div className="h-full bg-gradient-to-r from-[#059669] via-[#10B981] to-[#34D399] shadow-[0_0_12px_rgba(16,185,129,0.8)] animate-pulse w-full"></div>
          </div>
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

      {/* True Full-Screen Workspace Takeover Mode */}
      {isExpandedModal && (
        <div
          className={`fixed inset-0 z-50 flex flex-col w-screen h-screen bg-[#141414] overflow-hidden select-none animate-in fade-in duration-150 ${
            isDraggingLeft || isDraggingRight ? "cursor-col-resize select-none" : ""
          }`}
        >
          <div className="flex flex-col w-full h-full overflow-hidden relative">
            <WorkspaceHeader
              isFullModal={true}
              mode={mode}
              onSwitchMode={onSwitchMode}
              onSelectArchive={onSelectArchive}
              itemCount={archiveInfo.file_count || 0}
              operationProgress={operationProgress}
              isHeaderHUDOpen={isHeaderHUDOpen}
              onToggleHeaderHUD={() => setIsHeaderHUDOpen((prev) => !prev)}
              onCloseHeaderHUD={() => setIsHeaderHUDOpen(false)}
              viewMode={viewMode}
              onViewModeChange={setViewMode}
              onStartNewFolder={handleStartNewFolder}
              isLeftPanelOpen={windowWidth >= 800 ? isLeftPanelOpen : showMobileTree}
              onToggleLeftPanel={() => {
                if (windowWidth < 800) {
                  setShowMobileTree((prev) => !prev);
                } else {
                  setIsLeftPanelOpen((prev) => !prev);
                }
              }}
              isSplitView={isSplitView}
              onToggleSplitView={() => setIsSplitView((prev) => !prev)}
              isBottomPanelOpen={mode === "compress" ? isStagedQueueOpen : isExtractQueueOpen}
              onToggleBottomPanel={() => {
                if (mode === "compress") {
                  setIsStagedQueueOpen((prev) => !prev);
                } else {
                  setIsExtractQueueOpen((prev) => !prev);
                }
              }}
              bottomPanelBadge={mode === "compress" ? (stagedItems ? stagedItems.length : 0) : excludedCount}
              hasRightPanel={Boolean(sidebar)}
              isRightPanelOpen={showMobileSidebar}
              onToggleRightPanel={() => setShowMobileSidebar((prev) => !prev)}
              rightPanelTitle={sidebarTitle || "Options"}
              onToggleCompactView={() => setIsExpandedModal(false)}
            />

            {isEncryptedWithoutEntries ? (
              <div className="flex-1 flex items-center justify-center">
                {renderEncryptedBanner()}
              </div>
            ) : (
              <div className="flex-1 flex overflow-hidden relative">
                <div className="flex-1 flex flex-col overflow-hidden min-w-0">
                  {viewMode === "tree" ? (
                    <>
                      <WorkspaceNavBar
                        currentPath={browserSource === "filesystem" ? diskCurrentPath : currentFolderPath}
                        sourceType={browserSource}
                        hasError={browserSource === "filesystem" ? Boolean(diskError) : false}
                        isPathEditingRequested={editPathTrigger}
                        rootLabel={
                          browserSource === "filesystem"
                            ? activeDrive
                            : mode === "compress"
                              ? archiveInfo.filename && archiveInfo.filename !== "Archive"
                                ? archiveInfo.filename
                                : "Staged Root"
                              : archiveInfo.filename || "Archive Root"
                        }
                        isSplitView={isSplitView}
                        onToggleSplitView={() => setIsSplitView((prev) => !prev)}
                        onNavigate={(newPath) => {
                          if (browserSource === "filesystem") {
                            navigateToDiskDirectory(newPath);
                          } else {
                            navigateToFolder(newPath);
                          }
                        }}
                        onRefresh={() => {
                          if (browserSource === "filesystem") {
                            handleDiskRefresh();
                          }
                        }}
                        searchQuery={searchQuery}
                        onSearchChange={setSearchQuery}
                        canGoBack={browserSource === "filesystem" ? diskHistoryIndex > 0 : archiveHistoryIndex > 0}
                        canGoForward={
                          browserSource === "filesystem"
                            ? diskHistoryIndex < diskHistory.length - 1
                            : archiveHistoryIndex < archiveHistory.length - 1
                        }
                        onBack={browserSource === "filesystem" ? handleDiskBack : handleArchiveBack}
                        onForward={browserSource === "filesystem" ? handleDiskForward : handleArchiveForward}
                        canGoUp={
                          browserSource === "filesystem"
                            ? !/^[a-zA-Z]:[/\\]*$/.test(diskCurrentPath.trim())
                            : currentFolderPath !== ""
                        }
                        onUp={browserSource === "filesystem" ? handleDiskUp : navigateUp}
                        isLoading={browserSource === "filesystem" ? isLoadingDisk : isLoading}
                      />
                      {/* Dual-Pane Layout: Left Folder Tree / Quick Access, Right Folder Files */}
                      <div className="flex-1 flex overflow-hidden relative">
                        {/* Left Pane: Hierarchical Tree & Drives */}
                        {windowWidth >= 800 && isLeftPanelOpen && (
                          <>
                            <div
                              style={{ width: `${leftPanelWidth}px` }}
                              className="bg-[#1A1A1A] overflow-y-auto custom-scrollbar p-2 flex flex-col shrink-0 border-r border-[#2A2A2A]"
                            >
                              {renderSourceToggle()}

                              {browserSource === "archive" ? (
                                <>
                                  {renderFolderHierarchyHeader(false)}
                                  <div className="flex-1 overflow-y-auto custom-scrollbar pr-0.5">
                                    {mode === "compress" && localEntries.length === 0 && (!stagedItems || stagedItems.length === 0) ? (
                                      <div className="p-4 text-center text-[#777777] flex flex-col items-center justify-center space-y-2 h-44">
                                        <span className="text-2xl">📁</span>
                                        <p className="text-xs font-semibold text-[#CCCCCC]">No Staged Folders</p>
                                        <p className="text-[11px] text-[#666666] max-w-[190px]">
                                          Browse "Drive Files" or drag items here to stage.
                                        </p>
                                        <button
                                          type="button"
                                          onClick={() => setBrowserSource("filesystem")}
                                          className="mt-1 px-2.5 py-1 text-[11px] font-medium rounded bg-[#252525] hover:bg-[#303030] text-[#60CDFF] border border-[#3E3E3E] transition-colors cursor-pointer"
                                        >
                                          Browse Drive Files
                                        </button>
                                      </div>
                                    ) : (
                                      renderTreeFolderNode(treeRoot)
                                    )}
                                  </div>
                                </>
                              ) : (
                                <>
                                  {!hasArchiveLoaded && onSelectArchive && mode === "extract" && (
                                    <div className="mb-2 px-1">
                                      <button
                                        type="button"
                                        onClick={onSelectArchive}
                                        className="w-full py-1.5 px-3 rounded-lg bg-gradient-to-r from-[#173042] to-[#12222E] hover:from-[#1F415A] hover:to-[#172D3D] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/50 shadow-[0_0_12px_rgba(96,205,255,0.15)] text-xs font-semibold flex items-center justify-center space-x-2 transition-all cursor-pointer"
                                        title="Select an archive file from your computer"
                                      >
                                        <span>📂</span>
                                        <span>Open Archive File...</span>
                                      </button>
                                    </div>
                                  )}
                                  {renderQuickAccess()}
                                  {renderDriveSelector()}
                                </>
                              )}
                            </div>

                            {/* Draggable Resizer: Left Folder Tree */}
                            <div
                              onMouseDown={handleMouseDownLeft}
                              className="w-1.5 hover:w-1.5 bg-[#262626] hover:bg-[#60CDFF] active:bg-[#60CDFF] cursor-col-resize transition-colors shrink-0 z-20 flex items-center justify-center group select-none"
                              title="Drag to resize panel"
                            >
                              <div className="w-0.5 h-8 bg-[#444444] group-hover:bg-white rounded-full transition-colors"></div>
                            </div>
                          </>
                        )}

                        {/* Left Pane: Slide-over Drawer (Compact < 800px) */}
                        {windowWidth < 800 && showMobileTree && (
                          <div className="absolute inset-y-0 left-0 z-30 w-72 max-w-[85%] bg-[#1A1A1A] border-r border-[#3E3E3E] shadow-2xl flex flex-col p-2 animate-in slide-in-from-left duration-150">
                            {renderSourceToggle()}

                            {browserSource === "archive" ? (
                              <>
                                {renderFolderHierarchyHeader(true)}
                                <div className="flex-1 overflow-y-auto custom-scrollbar">
                                  {mode === "compress" && localEntries.length === 0 && (!stagedItems || stagedItems.length === 0) ? (
                                    <div className="p-4 text-center text-[#777777] flex flex-col items-center justify-center space-y-2 h-44">
                                      <span className="text-2xl">📁</span>
                                      <p className="text-xs font-semibold text-[#CCCCCC]">No Staged Folders</p>
                                      <p className="text-[11px] text-[#666666] max-w-[190px]">
                                        Browse "Drive Files" or drag items here to stage.
                                      </p>
                                      <button
                                        type="button"
                                        onClick={() => setBrowserSource("filesystem")}
                                        className="mt-1 px-2.5 py-1 text-[11px] font-medium rounded bg-[#252525] hover:bg-[#303030] text-[#60CDFF] border border-[#3E3E3E] transition-colors cursor-pointer"
                                      >
                                        Browse Drive Files
                                      </button>
                                    </div>
                                  ) : (
                                    renderTreeFolderNode(treeRoot)
                                  )}
                                </div>
                              </>
                            ) : (
                              <>
                                {!hasArchiveLoaded && onSelectArchive && mode === "extract" && (
                                  <div className="mb-2 px-1">
                                    <button
                                      type="button"
                                      onClick={onSelectArchive}
                                      className="w-full py-1.5 px-3 rounded-lg bg-gradient-to-r from-[#173042] to-[#12222E] hover:from-[#1F415A] hover:to-[#172D3D] text-[#60CDFF] hover:text-[#90DCFF] border border-[#60CDFF]/50 shadow-[0_0_12px_rgba(96,205,255,0.15)] text-xs font-semibold flex items-center justify-center space-x-2 transition-all cursor-pointer"
                                      title="Select an archive file from your computer"
                                    >
                                      <span>📂</span>
                                      <span>Open Archive File...</span>
                                    </button>
                                  </div>
                                )}
                                {renderQuickAccess()}
                                {renderDriveSelector()}
                              </>
                            )}
                          </div>
                        )}

                        {/* Dual-Pane Split View Controller (Encapsulated in WorkspaceDualPaneView) */}
                        <WorkspaceDualPaneView
                          isSplit={isSplitView}
                          onToggleSplit={() => setIsSplitView((prev) => !prev)}
                          splitRatio={splitRatio}
                          onSplitRatioChange={setSplitRatio}
                          pane1Header={renderPane1Header()}
                          pane1Content={browserSource === "filesystem" ? renderDiskFolderContents() : renderCurrentFolderContents()}
                          pane2Header={renderPane2Header()}
                          pane2Content={pane2Source === "filesystem" ? renderPane2DiskContents() : renderPane2ArchiveContents()}
                          onClosePane2={() => setIsSplitView(false)}
                          onDropFolderToSplit={(folderPath) => {
                            setIsSplitView(true);
                            setPane2Source("archive");
                            setPane2FolderPath(folderPath);
                          }}
                          onDropDiskItemToStage={(diskPath) => {
                            if (onStageItem) onStageItem(diskPath);
                          }}
                          bottomDrawer={
                            <>
                              {mode === "compress" && stagedItems && (
                                <WorkspaceStagedQueue
                                  stagedItems={stagedItems}
                                  itemsInfo={itemsInfo || {}}
                                  onRemoveItem={(path) => {
                                    if (onUnstageItem) onUnstageItem(path);
                                    const base = path.replace(/\\/g, "/").split("/").pop() || "";
                                    setLocalEntries((prev) =>
                                      prev.filter((item) => {
                                        const cleanItem = item.name.replace(/\\/g, "/").replace(/^\/+/, "");
                                        return !cleanItem.startsWith(base + "/") && cleanItem !== base;
                                      })
                                    );
                                  }}
                                  onDropItem={(path) => onStageItem && onStageItem(path)}
                                  onClearAll={() => {
                                    if (onClearStagedItems) onClearStagedItems();
                                    setLocalEntries([]);
                                    setCurrentFolderPath("");
                                  }}
                                  isOpen={isStagedQueueOpen}
                                  onToggleOpen={() => setIsStagedQueueOpen((prev) => !prev)}
                                  height={stagedQueueHeight}
                                  onHeightChange={(h) => setStagedQueueHeight(h)}
                                  onPreviewItem={(path) => {
                                    if (onPreviewEntry) onPreviewEntry(path);
                                  }}
                                />
                              )}

                              {mode === "extract" && (
                                <WorkspaceExtractQueue
                                  totalFilesCount={archiveInfo.file_count || 0}
                                  totalUncompressedSize={archiveInfo.total_uncompressed_size || 0}
                                  excludedPaths={excludedPaths || new Set()}
                                  onToggleExclude={(path, isDir) => onToggleExclude && onToggleExclude(path, isDir)}
                                  onIncludeAll={onIncludeAll}
                                  isOpen={isExtractQueueOpen}
                                  onToggleOpen={() => setIsExtractQueueOpen((prev) => !prev)}
                                  height={extractQueueHeight}
                                  onHeightChange={(h) => setExtractQueueHeight(h)}
                                  archiveFormat={archiveInfo.format}
                                  isEncrypted={archiveInfo.is_encrypted}
                                />
                              )}
                            </>
                          }
                        />
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

                {/* Right Options Sidebar: Advance Section */}
                {sidebar && (
                  <>
                    {/* Draggable Resizer: Right Sidebar */}
                    <div
                      onMouseDown={handleMouseDownRight}
                      className="w-1.5 hover:w-1.5 bg-[#262626] hover:bg-[#60CDFF] active:bg-[#60CDFF] cursor-col-resize transition-colors shrink-0 z-20 flex items-center justify-center group select-none"
                      title="Drag to resize Advance Options panel"
                    >
                      <div className="w-0.5 h-8 bg-[#444444] group-hover:bg-white rounded-full transition-colors"></div>
                    </div>

                    <div
                      style={{ width: `${rightPanelWidth}px` }}
                      className="bg-[#1C1C1C] flex flex-col shrink-0 overflow-y-auto custom-scrollbar p-4 space-y-4 border-l border-[#2E2E2E]"
                    >
                      {sidebarTitle && (
                        <div className="flex items-center justify-between border-b border-[#3E3E3E] pb-2 mb-1 shrink-0">
                          <div className="flex items-center space-x-2">
                            <span className="w-2 h-2 rounded-full bg-[#60CDFF]" />
                            <span className="text-xs uppercase tracking-wider font-semibold text-[#60CDFF]">
                              {sidebarTitle}
                            </span>
                          </div>
                        </div>
                      )}
                      {sidebar}
                    </div>
                  </>
                )}
              </div>
            )}

            {/* Bottom Running Loading Line: High-Visibility Energetic Green Scheme */}
            {operationProgress ? (
              <div className="w-full h-1.5 bg-[#141414] overflow-hidden shrink-0 border-t border-[#262626]">
                <div
                  className="h-full bg-gradient-to-r from-[#059669] via-[#10B981] to-[#34D399] shadow-[0_0_14px_rgba(16,185,129,0.9)] transition-all duration-200 relative overflow-hidden"
                  style={{ width: `${Math.max(operationProgress.percent, 2)}%` }}
                >
                  <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/40 to-transparent animate-shimmer" />
                </div>
              </div>
            ) : isLoading ? (
              <div className="w-full h-1.5 bg-[#141414] overflow-hidden shrink-0 border-t border-[#262626]">
                <div className="h-full bg-gradient-to-r from-[#059669] via-[#10B981] to-[#34D399] shadow-[0_0_12px_rgba(16,185,129,0.8)] animate-pulse w-full" />
              </div>
            ) : null}

            {/* Modal Bottom Status Bar */}
            <div className="px-4 py-2 bg-[#1A1A1A] border-t border-[#2A2A2A] flex items-center justify-between text-xs font-mono text-[#888888] shrink-0 gap-2">
              <div className="flex items-center space-x-3 overflow-hidden min-w-0">
                <span className="shrink-0">Total Items: <span className="text-white font-semibold">{archiveInfo.file_count || 0}</span></span>
                <span>•</span>
                <span className="shrink-0">Uncompressed: <span className="text-white font-semibold">{formatBytes(archiveInfo.total_uncompressed_size || 0)}</span></span>
                {excludedCount > 0 && (
                  <>
                    <span>•</span>
                    <span className="text-rose-400 font-semibold truncate shrink-0">
                      {excludedCount} excluded ({formatBytes(excludedBytes)} skipped)
                    </span>
                  </>
                )}
                {selectedEntryName && (
                  <>
                    <span className="hidden lg:inline">•</span>
                    <span className="text-[#60CDFF] truncate max-w-xs hidden lg:inline">Selected: {selectedEntryName.split(/[/\\]/).pop()}</span>
                  </>
                )}
              </div>
              <div className="hidden md:flex items-center space-x-2 shrink-0">
                <span>Press <kbd className="px-1.5 py-0.5 bg-[#282828] border border-[#3E3E3E] rounded text-[10px] text-white">Space</kbd> to Preview</span>
                <span>•</span>
                <span>Press <kbd className="px-1.5 py-0.5 bg-[#282828] border border-[#3E3E3E] rounded text-[10px] text-white">Esc</kbd> to Exit Workspace</span>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Sleek Windows 11 Fluent Floating Context Menu */}
      <WorkspaceContextMenu
        isOpen={contextMenu.isOpen}
        x={contextMenu.x}
        y={contextMenu.y}
        target={contextMenu.target}
        mode={mode || "extract"}
        browserSource={browserSource}
        onClose={() => setContextMenu((prev) => ({ ...prev, isOpen: false }))}
        onPreview={(name) => {
          if (browserSource === "filesystem" && contextMenu.target?.diskPath) {
            if (onPreviewEntry) onPreviewEntry(contextMenu.target.diskPath);
          } else {
            onPreviewEntry(name);
          }
        }}
        onStage={(path) => onStageItem && onStageItem(path)}
        onUnstage={(path) => onUnstageItem && onUnstageItem(path)}
        onToggleExclude={(name, isDir) => onToggleExclude && onToggleExclude(name, isDir)}
        onStartRename={(name, isDir) => handleStartRename(name, isDir)}
        onCopy={(name, isDir, diskPath) => {
          setClipboardItem({
            relName: name,
            isDir,
            diskPath: diskPath || getDiskPath(name),
          });
        }}
        onDeleteDisk={(name, isDir) => handleDeleteItem(name, isDir)}
        onNewFolder={handleStartNewFolder}
        onPaste={handlePasteItem}
        onLoadArchive={(diskPath) => {
          if (onLoadArchiveFromFile) {
            onLoadArchiveFromFile(diskPath);
            if (onSwitchMode && mode !== "extract") onSwitchMode("extract");
            setBrowserSource("archive");
          }
        }}
        clipboardItem={clipboardItem}
      />
    </>
  );
}

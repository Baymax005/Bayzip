import { useState, useMemo, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArchiveInfo, ArchiveEntryItem, DriveInfo } from "../types";
import { FileFormatIcon, prefetchFileIcons } from "./FileFormatIcon";

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
  inspectorTitle,
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

  // Dynamic responsive window width tracking
  const [windowWidth, setWindowWidth] = useState<number>(() =>
    typeof window !== "undefined" ? window.innerWidth : 1200
  );
  const [showMobileTree, setShowMobileTree] = useState<boolean>(false);
  const [showMobileSidebar, setShowMobileSidebar] = useState<boolean>(false);

  useEffect(() => {
    const handleResize = () => setWindowWidth(window.innerWidth);
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  // System drives for Drive Switcher
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [isDriveMenuOpen, setIsDriveMenuOpen] = useState(false);
  const [activeDrive, setActiveDrive] = useState<string>("C:");

  useEffect(() => {
    invoke<DriveInfo[]>("get_system_drives")
      .then((res) => {
        if (res && res.length > 0) {
          setDrives(res);
          if (selectedArchive && /^[a-zA-Z]:/.test(selectedArchive)) {
            setActiveDrive(selectedArchive.slice(0, 2).toUpperCase());
          } else {
            setActiveDrive(res[0].mount_point.slice(0, 2).toUpperCase());
          }
        }
      })
      .catch(() => {});
  }, [selectedArchive]);

  // Local mutable entries state for real-time additions/renames/deletions
  const [localEntries, setLocalEntries] = useState<ArchiveEntryItem[]>(archiveInfo.entries || []);

  useEffect(() => {
    setLocalEntries(archiveInfo.entries || []);
  }, [archiveInfo.entries]);

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
    target?: {
      name: string;
      isDir: boolean;
      size?: number;
    } | null;
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
    target?: { name: string; isDir: boolean; size?: number } | null
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
        const newWidth = Math.max(180, Math.min(650, dragLeftStartRef.current.startWidth + delta));
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

  const excludedCount = excludedPaths.size;

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

  // Sleek Drive Switcher Badge & Header
  const renderFolderHierarchyHeader = (isDrawer: boolean = false) => (
    <div
      className={`flex items-center justify-between px-2 py-1 mb-1 shrink-0 relative ${
        isDrawer ? "border-b border-[#2E2E2E]" : ""
      }`}
    >
      <span className="text-[10px] font-bold tracking-wider uppercase text-[#888888]">
        Folder Hierarchy
      </span>
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

        {drives.length > 0 && (
          <div className="relative">
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setIsDriveMenuOpen((prev) => !prev);
              }}
              className="px-2 py-0.5 rounded bg-[#252525] hover:bg-[#303030] border border-[#3E3E3E] hover:border-[#60CDFF]/60 text-[#60CDFF] hover:text-[#90DCFF] text-[10px] font-mono font-semibold flex items-center space-x-1 transition-all cursor-pointer shadow-sm"
              title="Switch System Drive"
            >
              <span>🖴</span>
              <span>{activeDrive}</span>
              <span className="text-[8px] text-[#888888]">▾</span>
            </button>

            {isDriveMenuOpen && (
              <>
                <div
                  className="fixed inset-0 z-40"
                  onClick={(e) => {
                    e.stopPropagation();
                    setIsDriveMenuOpen(false);
                  }}
                />
                <div
                  onClick={(e) => e.stopPropagation()}
                  className="absolute right-0 top-full mt-1.5 z-50 w-56 bg-[#1C1C1C] border border-[#3E3E3E] rounded-lg shadow-2xl p-1.5 space-y-1 animate-in fade-in duration-100"
                >
                  <div className="px-2 py-1 text-[9px] font-bold uppercase tracking-wider text-[#777777] border-b border-[#2A2A2A]">
                    System Drives
                  </div>
                  {drives.map((d) => {
                    const driveLetter = d.mount_point.slice(0, 2).toUpperCase();
                    const isCur = activeDrive === driveLetter;
                    return (
                      <button
                        key={d.mount_point}
                        type="button"
                        onClick={() => {
                          setActiveDrive(driveLetter);
                          setIsDriveMenuOpen(false);
                        }}
                        className={`w-full text-left px-2 py-1.5 rounded flex items-center justify-between text-xs transition-colors cursor-pointer ${
                          isCur
                            ? "bg-[#25394A] text-[#60CDFF] font-semibold border border-[#60CDFF]/40"
                            : "hover:bg-[#282828] text-white"
                        }`}
                      >
                        <div className="flex items-center space-x-2 min-w-0">
                          <span className="text-sm">{d.is_removable ? "🔌" : "🖴"}</span>
                          <span className="font-mono font-bold">{driveLetter}</span>
                          <span className="text-[10px] text-[#888888] truncate">{d.name}</span>
                        </div>
                        <span className="text-[9px] font-mono text-[#777777] shrink-0">
                          {formatBytes(d.available_bytes)} free
                        </span>
                      </button>
                    );
                  })}
                </div>
              </>
            )}
          </div>
        )}
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

    return (
      <div key={node.path || "root"} className="select-none text-xs">
        <div
          onClick={() => navigateToFolder(node.path)}
          onContextMenu={(e) => handleOpenContextMenu(e, { name: node.path, isDir: true })}
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
        <span className="text-xs font-semibold text-white tracking-wide">
          {inspectorTitle || (archiveInfo.is_archive === false ? "Staged Items Inspector" : "Archive Explorer")}
        </span>
        <span className="text-[10px] font-mono bg-[#333333] text-[#A8A8A8] px-2 py-0.5 rounded-full border border-[#444444]">
          {archiveInfo.file_count || 0} item{archiveInfo.file_count !== 1 ? "s" : ""}
        </span>
      </div>

      <div className="flex items-center space-x-2">
        {/* Responsive Folder Tree Toggle (in full modal when window width < 850px) */}
        {isFullModal && viewMode === "tree" && (
          <button
            type="button"
            onClick={() => setShowMobileTree((prev) => !prev)}
            className={`px-2.5 py-1 text-xs rounded-lg font-medium transition-all flex items-center space-x-1.5 cursor-pointer ${
              showMobileTree
                ? "bg-[#60CDFF]/20 text-[#60CDFF] border border-[#60CDFF]/70 shadow-[0_0_10px_rgba(96,205,255,0.2)] font-semibold"
                : "bg-[#1C1C1C] hover:bg-[#2A2A2A] text-[#A8A8A8] hover:text-white border border-[#3E3E3E]"
            } ${windowWidth >= 850 ? "hidden" : "flex"}`}
            title={showMobileTree ? "Hide Folder Tree" : "Show Folder Tree Drawer"}
          >
            <span>📁</span>
            <span>Tree</span>
          </button>
        )}

        {/* Responsive Options Sidebar Toggle (in full modal when window width < 1150px) */}
        {isFullModal && sidebar && (
          <button
            type="button"
            onClick={() => setShowMobileSidebar((prev) => !prev)}
            className={`px-2.5 py-1 text-xs rounded-lg font-medium transition-all flex items-center space-x-1.5 cursor-pointer ${
              showMobileSidebar
                ? "bg-[#60CDFF]/20 text-[#60CDFF] border border-[#60CDFF]/70 shadow-[0_0_10px_rgba(96,205,255,0.2)] font-semibold"
                : "bg-[#1C1C1C] hover:bg-[#2A2A2A] text-[#A8A8A8] hover:text-white border border-[#3E3E3E]"
            } ${windowWidth >= 1150 ? "hidden" : "flex"}`}
            title={showMobileSidebar ? "Hide Options Panel" : "Show Options Panel Drawer"}
          >
            <span>⚙️</span>
            <span>{sidebarTitle || "Options"}</span>
          </button>
        )}

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
            <span className="hidden sm:inline">Tree View</span>
            <span className="sm:hidden">Tree</span>
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
            <span className="hidden sm:inline">Flat Table</span>
            <span className="sm:hidden">Flat</span>
          </button>
        </div>

        {/* + New Folder Button */}
        <button
          type="button"
          onClick={handleStartNewFolder}
          className="px-2.5 py-1 text-xs rounded-lg font-medium transition-all flex items-center space-x-1.5 bg-[#25394A] hover:bg-[#2C4459] text-[#60CDFF] border border-[#60CDFF]/30 hover:border-[#60CDFF]/60 cursor-pointer shadow-sm"
          title="Create New Folder (Ctrl+Shift+N)"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 13h6m-3-3v6m-9 1V7a2 2 0 012-2h6l2 2h6a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z" />
          </svg>
          <span className="hidden sm:inline">New Folder</span>
          <span className="sm:hidden">+Folder</span>
        </button>

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
          <span className="hidden sm:inline">{isExclusionMode ? "Exclusion Mode" : "Exclude Files"}</span>
          <span className="sm:hidden">{isExclusionMode ? "Mode" : "Exclude"}</span>
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
            <span className="hidden sm:inline">Include All</span>
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
          <>
            <button
              type="button"
              onClick={() => setIsExpandedModal(false)}
              className="px-2.5 py-1 bg-[#2D2D2D] hover:bg-rose-950/40 text-[#A8A8A8] hover:text-rose-300 rounded-md border border-[#3E3E3E] hover:border-rose-700/60 transition-colors cursor-pointer flex items-center space-x-1.5 text-xs font-medium"
              title="Exit Workspace (Esc)"
            >
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
              <span className="hidden sm:inline">Exit Workspace</span>
            </button>

            {/* Native Window Controls in Workspace Header */}
            <div className="flex items-center h-full ml-1 border-l border-[#3E3E3E] pl-1.5 space-x-0.5" data-tauri-drag-region="false">
              {/* Minimize */}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  getCurrentWindow().minimize().catch(() => {});
                }}
                className="p-1.5 hover:bg-[#333333] text-[#A8A8A8] hover:text-white rounded transition-colors cursor-pointer"
                title="Minimize"
              >
                <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor" /></svg>
              </button>
              {/* Maximize / Restore */}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  getCurrentWindow().toggleMaximize().catch(() => {});
                }}
                className="p-1.5 hover:bg-[#333333] text-[#A8A8A8] hover:text-white rounded transition-colors cursor-pointer"
                title="Maximize / Restore"
              >
                <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0,0v10h10V0H0z M9,9H1V1h8V9z" fill="currentColor" /></svg>
              </button>
              {/* Close */}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  getCurrentWindow().close().catch(() => {});
                }}
                className="p-1.5 hover:bg-[#E81123] text-[#A8A8A8] hover:text-white rounded transition-colors cursor-pointer"
                title="Close Bayzip"
              >
                <svg width="10" height="10" viewBox="0 0 10 10"><path d="M10,1L9,0L5,4L1,0L0,1l4,4l-4,4l1,1l4-4l4,4l1-1l-4-4L10,1z" fill="currentColor" /></svg>
              </button>
            </div>
          </>
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
              return (
                <div
                  key={`folder-${sub.path}`}
                  onDoubleClick={() => navigateToFolder(sub.path)}
                  onContextMenu={(e) => handleOpenContextMenu(e, { name: sub.path, isDir: true })}
                  onClick={() => {
                    if (isExclusionMode && onToggleExclude) {
                      onToggleExclude(sub.path, true);
                    } else {
                      setSelectedEntryName(sub.path);
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
            const fileNameOnly = file.name.split(/[/\\]/).pop() || file.name;
            const isExcluded = isFileExcluded(file.name);
            const isRenaming = renamingItem && renamingItem.originalName === file.name;

            return (
              <div
                key={`file-${file.name}`}
                onContextMenu={(e) => handleOpenContextMenu(e, { name: file.name, isDir: false, size: file.size })}
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

            return (
              <div
                key={`flat-${entry.name || idx}`}
                onContextMenu={(e) => handleOpenContextMenu(e, { name: entry.name, isDir: entry.is_dir, size: entry.size })}
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
            } overflow-hidden relative ${isDraggingLeft || isDraggingRight ? "cursor-col-resize select-none" : ""}`}
          >
            {renderToolbar(true)}

            {isEncryptedWithoutEntries ? (
              <div className="flex-1 flex items-center justify-center">
                {renderEncryptedBanner()}
              </div>
            ) : (
              <div className="flex-1 flex overflow-hidden relative">
                <div className="flex-1 flex flex-col overflow-hidden min-w-0">
                  {viewMode === "tree" ? (
                    <>
                      {renderBreadcrumbs()}
                      {renderSearchInput()}
                      {/* Dual-Pane Layout: Left Folder Tree, Right Folder Files */}
                      <div className="flex-1 flex overflow-hidden relative">
                        {/* Left Pane: Hierarchical Tree (Desktop >= 850px) */}
                        {windowWidth >= 850 && (
                          <>
                            <div
                              style={{ width: `${leftPanelWidth}px` }}
                              className="bg-[#1A1A1A] overflow-y-auto custom-scrollbar p-2 flex flex-col shrink-0"
                            >
                              {renderFolderHierarchyHeader(false)}
                              {renderTreeFolderNode(treeRoot)}
                            </div>

                            {/* Draggable Resizer: Left Folder Tree */}
                            <div
                              onMouseDown={handleMouseDownLeft}
                              className="w-1.5 hover:w-1.5 bg-[#262626] hover:bg-[#60CDFF] active:bg-[#60CDFF] cursor-col-resize transition-colors shrink-0 z-20 flex items-center justify-center group select-none"
                              title="Drag to resize Folder Hierarchy panel"
                            >
                              <div className="w-0.5 h-8 bg-[#444444] group-hover:bg-white rounded-full transition-colors"></div>
                            </div>
                          </>
                        )}

                        {/* Left Pane: Slide-over Drawer (Compact < 850px) */}
                        {windowWidth < 850 && showMobileTree && (
                          <div className="absolute inset-y-0 left-0 z-30 w-72 max-w-[85%] bg-[#1A1A1A] border-r border-[#3E3E3E] shadow-2xl flex flex-col p-2 animate-in slide-in-from-left duration-150">
                            {renderFolderHierarchyHeader(true)}
                            <div className="flex-1 overflow-y-auto custom-scrollbar">
                              {renderTreeFolderNode(treeRoot)}
                            </div>
                          </div>
                        )}

                        {/* Right Pane: Files inside selected folder */}
                        <div className="flex-1 bg-[#181818] overflow-hidden flex flex-col min-w-0">
                          <div className="px-3 py-1.5 bg-[#222222] border-b border-[#2E2E2E] text-[11px] font-mono text-[#A8A8A8] flex justify-between shrink-0">
                            <span className="truncate mr-2">Folder: <span className="text-white font-semibold">/{currentFolderPath || ""}</span></span>
                            <span className="hidden md:inline shrink-0">Double-click or Spacebar to Preview</span>
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

                {/* Right Options Sidebar: Desktop Inline (>= 1150px) */}
                {sidebar && windowWidth >= 1150 && (
                  <>
                    {/* Draggable Resizer: Right Sidebar */}
                    <div
                      onMouseDown={handleMouseDownRight}
                      className="w-1.5 hover:w-1.5 bg-[#262626] hover:bg-[#60CDFF] active:bg-[#60CDFF] cursor-col-resize transition-colors shrink-0 z-20 flex items-center justify-center group select-none"
                      title="Drag to resize Options panel"
                    >
                      <div className="w-0.5 h-8 bg-[#444444] group-hover:bg-white rounded-full transition-colors"></div>
                    </div>

                    <div
                      style={{ width: `${rightPanelWidth}px` }}
                      className="bg-[#202020] flex flex-col shrink-0 overflow-y-auto custom-scrollbar p-4 space-y-4"
                    >
                      {sidebarTitle && (
                        <div className="flex items-center justify-between border-b border-[#3E3E3E] pb-2 mb-1 shrink-0">
                          <span className="text-xs uppercase tracking-wider font-semibold text-[#60CDFF]">
                            {sidebarTitle}
                          </span>
                        </div>
                      )}
                      {sidebar}
                    </div>
                  </>
                )}

                {/* Right Options Sidebar: Slide-over Drawer (Compact < 1150px) */}
                {sidebar && windowWidth < 1150 && showMobileSidebar && (
                  <div className="absolute inset-y-0 right-0 z-30 w-80 max-w-[85%] bg-[#202020] border-l border-[#3E3E3E] shadow-2xl flex flex-col p-4 space-y-4 animate-in slide-in-from-right duration-150">
                    <div className="flex items-center justify-between border-b border-[#3E3E3E] pb-2 mb-1 shrink-0">
                      <span className="text-xs uppercase tracking-wider font-semibold text-[#60CDFF]">
                        {sidebarTitle || "Options"}
                      </span>
                      <button
                        type="button"
                        onClick={() => setShowMobileSidebar(false)}
                        className="text-[#888888] hover:text-white text-xs px-2 py-0.5 rounded hover:bg-[#2A2A2A] cursor-pointer"
                        title="Close Options Panel"
                      >
                        ✕
                      </button>
                    </div>
                    <div className="flex-1 overflow-y-auto custom-scrollbar">
                      {sidebar}
                    </div>
                  </div>
                )}
              </div>
            )}

            {/* Running Bottom Loading Line (High-Visibility Energetic Green Scheme) */}
            {isLoading && (
              <div className="w-full h-1 bg-[#1A1A1A] overflow-hidden">
                <div className="h-full bg-gradient-to-r from-[#059669] via-[#10B981] to-[#34D399] shadow-[0_0_12px_rgba(16,185,129,0.8)] animate-pulse w-full"></div>
              </div>
            )}

            {/* Modal Bottom Status Bar */}
            <div className="px-4 py-2 bg-[#222222] border-t border-[#3E3E3E] flex items-center justify-between text-xs font-mono text-[#888888] shrink-0 gap-2">
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
                <span>Press <kbd className="px-1.5 py-0.5 bg-[#333333] border border-[#444444] rounded text-[10px] text-white">Space</kbd> to Preview</span>
                <span>•</span>
                <span>Press <kbd className="px-1.5 py-0.5 bg-[#333333] border border-[#444444] rounded text-[10px] text-white">Esc</kbd> to Exit</span>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Sleek Windows 11 Fluent Floating Context Menu */}
      {contextMenu.isOpen && (
        <>
          <div
            className="fixed inset-0 z-[120]"
            onClick={() => setContextMenu((prev) => ({ ...prev, isOpen: false }))}
            onContextMenu={(e) => {
              e.preventDefault();
              setContextMenu((prev) => ({ ...prev, isOpen: false }));
            }}
          />
          <div
            style={{ left: `${contextMenu.x}px`, top: `${contextMenu.y}px` }}
            className="fixed z-[121] min-w-[210px] bg-[#1E1E1E]/95 backdrop-blur-md border border-[#3E3E3E] rounded-lg shadow-2xl p-1 text-xs select-none animate-in fade-in duration-100 font-sans"
            onClick={(e) => e.stopPropagation()}
          >
            {contextMenu.target && (
              <>
                {!contextMenu.target.isDir && (
                  <button
                    type="button"
                    onClick={() => {
                      if (contextMenu.target) onPreviewEntry(contextMenu.target.name);
                      setContextMenu((prev) => ({ ...prev, isOpen: false }));
                    }}
                    className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
                  >
                    <div className="flex items-center space-x-2">
                      <span className="text-sm">👁️</span>
                      <span>Preview</span>
                    </div>
                    <span className="text-[10px] text-[#777777] font-mono">Space</span>
                  </button>
                )}

                <button
                  type="button"
                  onClick={() => {
                    if (contextMenu.target) handleStartRename(contextMenu.target.name, contextMenu.target.isDir);
                    setContextMenu((prev) => ({ ...prev, isOpen: false }));
                  }}
                  className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
                >
                  <div className="flex items-center space-x-2">
                    <span className="text-sm">✏️</span>
                    <span>Rename</span>
                  </div>
                  <span className="text-[10px] text-[#777777] font-mono">F2</span>
                </button>

                <button
                  type="button"
                  onClick={() => {
                    if (contextMenu.target) {
                      setClipboardItem({
                        relName: contextMenu.target.name,
                        isDir: contextMenu.target.isDir,
                        diskPath: getDiskPath(contextMenu.target.name),
                      });
                    }
                    setContextMenu((prev) => ({ ...prev, isOpen: false }));
                  }}
                  className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
                >
                  <div className="flex items-center space-x-2">
                    <span className="text-sm">📋</span>
                    <span>Copy</span>
                  </div>
                  <span className="text-[10px] text-[#777777] font-mono">Ctrl+C</span>
                </button>

                <div className="my-1 border-t border-[#2E2E2E]" />

                <button
                  type="button"
                  onClick={() => {
                    if (contextMenu.target) handleDeleteItem(contextMenu.target.name, contextMenu.target.isDir);
                    setContextMenu((prev) => ({ ...prev, isOpen: false }));
                  }}
                  className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-rose-950/60 hover:text-rose-300 text-rose-400 transition-colors cursor-pointer"
                >
                  <div className="flex items-center space-x-2">
                    <span className="text-sm">🗑️</span>
                    <span>Delete (Recycle Bin)</span>
                  </div>
                  <span className="text-[10px] text-rose-400/80 font-mono">Del</span>
                </button>

                <div className="my-1 border-t border-[#2E2E2E]" />
              </>
            )}

            <button
              type="button"
              onClick={() => {
                handleStartNewFolder();
                setContextMenu((prev) => ({ ...prev, isOpen: false }));
              }}
              className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
            >
              <div className="flex items-center space-x-2">
                <span className="text-sm">📁+</span>
                <span>New Folder</span>
              </div>
              <span className="text-[10px] text-[#777777] font-mono">Ctrl+Shift+N</span>
            </button>

            {clipboardItem && (
              <button
                type="button"
                onClick={() => {
                  handlePasteItem();
                  setContextMenu((prev) => ({ ...prev, isOpen: false }));
                }}
                className="w-full text-left px-2.5 py-1.5 rounded flex items-center justify-between hover:bg-[#25394A] hover:text-[#60CDFF] text-white transition-colors cursor-pointer"
              >
                <div className="flex items-center space-x-2 truncate mr-2">
                  <span className="text-sm">📥</span>
                  <span className="truncate">Paste "{clipboardItem.relName.split("/").filter(Boolean).pop()}"</span>
                </div>
                <span className="text-[10px] text-[#777777] font-mono shrink-0">Ctrl+V</span>
              </button>
            )}
          </div>
        </>
      )}
    </>
  );
}

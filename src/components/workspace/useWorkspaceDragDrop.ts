import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArchiveEntryItem } from "../../types";

export function normalizeArchiveEntryPath(p: string): string {
  return p.replace(/\\/g, "/").replace(/^\.?\/+/, "").toLowerCase();
}

export interface UseWorkspaceDragDropOptions {
  localEntries: ArchiveEntryItem[];
  setLocalEntries: React.Dispatch<React.SetStateAction<ArchiveEntryItem[]>>;
  getDiskPath: (relPath: string) => string;
  setExpandedFolders: React.Dispatch<React.SetStateAction<Set<string>>>;
  browserSource: "filesystem" | "archive";
  diskCurrentPath: string;
  loadDiskDirectory: (path: string) => void;
  isSplitView: boolean;
  pane2Source: "filesystem" | "archive";
  pane2DiskPath: string;
  loadPane2DiskDirectory: (path: string) => void;
  onStageItem?: (path: string) => void;
}

export interface FolderDropProps {
  isDropTarget: boolean;
  handlers: {
    onDragOver: (e: React.DragEvent) => void;
    onDragLeave: (e: React.DragEvent) => void;
    onDrop: (e: React.DragEvent) => void;
  };
}

export interface ItemDragProps {
  draggable: true;
  onDragStart: (e: React.DragEvent) => void;
}

export function useWorkspaceDragDrop({
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
}: UseWorkspaceDragDropOptions) {
  const [dragOverTargetKey, setDragOverTargetKey] = useState<string | null>(null);

  // Move an archive entry (file or folder) to a new destination folder inside the archive/stage container
  const handleMoveArchiveEntry = async (sourcePath: string, isDir: boolean, targetFolderPath: string) => {
    const cleanSource = sourcePath.replace(/\\/g, "/").replace(/^\.?\/+/, "").replace(/\/+$/, "");
    const cleanTarget = targetFolderPath.replace(/\\/g, "/").replace(/^\.?\/+/, "").replace(/\/+$/, "");

    // Prevent moving into itself
    if (cleanSource === cleanTarget) return;

    // Prevent moving a folder into its own child/descendant
    if (isDir && (cleanTarget + "/").startsWith(cleanSource + "/")) return;

    // Check if source is already inside targetFolderPath
    const lastSlash = cleanSource.lastIndexOf("/");
    const sourceParent = lastSlash === -1 ? "" : cleanSource.substring(0, lastSlash);
    if (sourceParent === cleanTarget) return;

    const itemName = cleanSource.split("/").pop() || cleanSource;
    const baseNewRel = cleanTarget ? `${cleanTarget}/${itemName}` : itemName;

    // Resolve any name collisions in target folder
    let newRel = baseNewRel;
    let collisionCounter = 1;
    while (
      localEntries.some(
        (e) =>
          normalizeArchiveEntryPath(e.name).replace(/\/+$/, "") === normalizeArchiveEntryPath(newRel) &&
          normalizeArchiveEntryPath(e.name).replace(/\/+$/, "") !== normalizeArchiveEntryPath(cleanSource)
      )
    ) {
      const extIdx = itemName.lastIndexOf(".");
      if (isDir || extIdx === -1) {
        newRel = cleanTarget ? `${cleanTarget}/${itemName}_${collisionCounter}` : `${itemName}_${collisionCounter}`;
      } else {
        const stem = itemName.substring(0, extIdx);
        const ext = itemName.substring(extIdx);
        newRel = cleanTarget ? `${cleanTarget}/${stem}_${collisionCounter}${ext}` : `${stem}_${collisionCounter}${ext}`;
      }
      collisionCounter++;
    }

    const newEntryName = isDir ? `${newRel}/` : newRel;

    // If backing physical disk staging exists, move on disk as well
    const oldDisk = getDiskPath(cleanSource);
    const newDisk = getDiskPath(newRel);
    if (oldDisk && newDisk) {
      try {
        await invoke("rename_path", { oldPath: oldDisk, newPath: newDisk });
      } catch (err) {
        console.warn("Disk move skipped or failed:", err);
      }
    }

    // Update localEntries state
    setLocalEntries((prev) =>
      prev.map((e) => {
        const eNorm = e.name.replace(/\\/g, "/").replace(/^\.?\/+/, "");
        if (isDir) {
          if (eNorm.replace(/\/+$/, "") === cleanSource) {
            return { ...e, name: newEntryName };
          }
          if (eNorm.startsWith(cleanSource + "/")) {
            return { ...e, name: `${newRel}/${eNorm.substring(cleanSource.length + 1)}` };
          }
        } else {
          if (eNorm === cleanSource) {
            return { ...e, name: newEntryName };
          }
        }
        return e;
      })
    );

    // Auto-expand target folder in tree so user immediately sees their moved item
    if (cleanTarget) {
      setExpandedFolders((prev) => new Set([...prev, cleanTarget]));
    }
  };

  // Move a disk file or folder on physical disk via Tauri rename_path
  const handleMoveDiskItem = async (sourceDiskPath: string, targetDiskFolderPath: string) => {
    const normSource = sourceDiskPath.replace(/\//g, "\\").replace(/\\+$/, "");
    const normTarget = targetDiskFolderPath.replace(/\//g, "\\").replace(/\\+$/, "");

    if (normSource.toLowerCase() === normTarget.toLowerCase()) return;
    // Prevent moving directory into itself or child
    if (normTarget.toLowerCase().startsWith(normSource.toLowerCase() + "\\")) return;

    const fileName = normSource.split("\\").pop() || "";
    if (!fileName) return;

    const lastBackslash = normSource.lastIndexOf("\\");
    const sourceParent = lastBackslash === -1 ? "" : normSource.substring(0, lastBackslash);
    if (sourceParent.toLowerCase() === normTarget.toLowerCase()) return;

    const destPath = `${normTarget}\\${fileName}`;
    try {
      await invoke("rename_path", { oldPath: normSource, newPath: destPath });
      // Refresh active disk views
      if (browserSource === "filesystem") {
        loadDiskDirectory(diskCurrentPath);
      }
      if (isSplitView && pane2Source === "filesystem") {
        loadPane2DiskDirectory(pane2DiskPath);
      }
    } catch (err) {
      console.error("Failed to move disk item:", err);
    }
  };

  // Unified drop handler for moving/staging onto any folder or parent row
  const handleItemDropOnFolder = async (
    e: React.DragEvent,
    targetFolderPath: string,
    isDiskTarget?: boolean,
    targetDiskPath?: string
  ) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOverTargetKey(null);

    const archiveEntry = e.dataTransfer.getData("application/bayzip-archive-entry");
    const isDirStr = e.dataTransfer.getData("application/bayzip-is-dir");
    const isDir = isDirStr === "true";
    const diskPath = e.dataTransfer.getData("application/bayzip-disk-path");
    const folderPath = e.dataTransfer.getData("application/bayzip-folder-path");

    if (isDiskTarget && targetDiskPath) {
      // Target is a physical disk folder
      if (diskPath) {
        await handleMoveDiskItem(diskPath, targetDiskPath);
      }
      return;
    }

    // Target is an archive folder
    if (archiveEntry || folderPath) {
      const src = archiveEntry || folderPath;
      await handleMoveArchiveEntry(src, isDir || Boolean(folderPath), targetFolderPath);
    } else if (diskPath) {
      // Dragged from disk to archive folder: stage it!
      if (onStageItem) {
        onStageItem(diskPath);
      }
    }
  };

  // Helper to generate drag props for an archive entry (file or folder)
  const getArchiveDragProps = (entryPath: string, isDir: boolean, name?: string): ItemDragProps => ({
    draggable: true,
    onDragStart: (e: React.DragEvent) => {
      e.dataTransfer.setData("application/bayzip-archive-entry", entryPath);
      e.dataTransfer.setData("application/bayzip-is-dir", isDir ? "true" : "false");
      if (isDir) {
        e.dataTransfer.setData("application/bayzip-folder-path", entryPath);
        e.dataTransfer.setData("application/bayzip-folder-name", name || entryPath.split("/").pop() || entryPath);
      }
      e.dataTransfer.setData("text/plain", entryPath);
      e.dataTransfer.effectAllowed = "copyMove";
    },
  });

  // Helper to generate drag props for a disk entry (file or folder)
  const getDiskDragProps = (diskPath: string, isDir: boolean): ItemDragProps => ({
    draggable: true,
    onDragStart: (e: React.DragEvent) => {
      e.dataTransfer.setData("application/bayzip-disk-path", diskPath);
      e.dataTransfer.setData("application/bayzip-is-dir", isDir ? "true" : "false");
      if (isDir) {
        e.dataTransfer.setData("application/bayzip-folder-path", diskPath);
        e.dataTransfer.setData("application/bayzip-folder-name", diskPath.split(/[/\\]/).pop() || diskPath);
      }
      e.dataTransfer.setData("text/plain", diskPath);
      e.dataTransfer.effectAllowed = "copyMove";
    },
  });

  // Helper to generate drop target props and highlight state for any folder
  const getFolderDropProps = (
    targetKey: string,
    targetFolderPath: string,
    isDiskTarget?: boolean,
    targetDiskPath?: string
  ): FolderDropProps => ({
    isDropTarget: dragOverTargetKey === targetKey,
    handlers: {
      onDragOver: (e: React.DragEvent) => {
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = "move";
        if (dragOverTargetKey !== targetKey) {
          setDragOverTargetKey(targetKey);
        }
      },
      onDragLeave: (e: React.DragEvent) => {
        e.stopPropagation();
        if (dragOverTargetKey === targetKey) {
          setDragOverTargetKey(null);
        }
      },
      onDrop: (e: React.DragEvent) => {
        handleItemDropOnFolder(e, targetFolderPath, isDiskTarget, targetDiskPath);
      },
    },
  });

  return {
    dragOverTargetKey,
    setDragOverTargetKey,
    handleMoveArchiveEntry,
    handleMoveDiskItem,
    handleItemDropOnFolder,
    getArchiveDragProps,
    getDiskDragProps,
    getFolderDropProps,
  };
}

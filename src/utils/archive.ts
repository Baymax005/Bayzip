/**
 * Supported archive extensions recognized by Bayzip
 */
export const ARCHIVE_EXTS = [
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

/**
 * Determines whether a file path corresponds to a supported archive format
 */
export function isArchiveFile(filePath: string): boolean {
  const lower = filePath.toLowerCase();
  return ARCHIVE_EXTS.some((ext) => lower.endsWith(ext));
}

/**
 * Utility helper for archive filename stem extraction
 */
export function getArchiveStemName(filename: string): string {
  const clean = filename.replace(/\\/g, "/").split("/").pop() || "";
  const lower = clean.toLowerCase();
  const multiExts = [
    ".tar.gz",
    ".tar.bz2",
    ".tar.xz",
    ".tar.zst",
    ".tar.br",
    ".tar.lz4",
  ];
  for (const ext of multiExts) {
    if (lower.endsWith(ext)) {
      return clean.slice(0, clean.length - ext.length);
    }
  }
  const dotIdx = clean.lastIndexOf(".");
  return dotIdx > 0 ? clean.slice(0, dotIdx) : clean;
}

/**
 * Normalizes archive entry paths for consistent hierarchy and matching
 */
export function normalizeArchiveEntryPath(p: string): string {
  return p.replace(/\\/g, "/").replace(/^\.?\/+/, "").toLowerCase();
}

/**
 * Toggles exclusion state for a file or folder while safely expanding any
 * excluded ancestor directories when re-including a nested child item.
 */
export function togglePathExclusion(
  prev: Set<string>,
  rawPath: string,
  isDir: boolean | undefined,
  allEntries: { name: string; is_dir?: boolean }[]
): Set<string> {
  const norm = normalizeArchiveEntryPath(rawPath).replace(/\/+$/, "");
  if (!norm) return new Set(prev);

  const prefix = norm + "/";
  const next = new Set(prev);

  const matchingAncestors: string[] = [];
  for (const exc of next) {
    const anc = exc.replace(/\/+$/, "");
    if (anc && norm.startsWith(anc + "/")) {
      matchingAncestors.push(anc);
    }
  }

  const isCurrentlyExcluded = next.has(norm) || next.has(prefix) || matchingAncestors.length > 0;

  if (isCurrentlyExcluded) {
    for (const anc of matchingAncestors) {
      const ancPrefix = anc + "/";
      for (const entry of allEntries) {
        const entryNorm = normalizeArchiveEntryPath(entry.name).replace(/\/+$/, "");
        if (!entryNorm || !entryNorm.startsWith(ancPrefix)) continue;
        // Skip the item being re-included and any ancestor/descendant along its path
        const isOnTargetChain =
          entryNorm === norm ||
          norm.startsWith(entryNorm + "/") ||
          (isDir && entryNorm.startsWith(prefix));
        if (!isOnTargetChain) {
          next.add(entryNorm);
        }
      }
      next.delete(anc);
      next.delete(ancPrefix);
    }

    next.delete(norm);
    next.delete(prefix);
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
}

/**
 * Resolves a relative staged tree entry path back to its full disk path from `selectedFiles`.
 */
export function resolveStagedDiskPath(entryName: string, selectedFiles: string[]): string {
  if (/^[a-zA-Z]:[/\\]/.test(entryName) || entryName.startsWith("\\\\")) {
    return entryName;
  }

  const cleanEntry = entryName.replace(/\\/g, "/").replace(/^\.?\/+/, "");
  const cleanEntryLower = cleanEntry.toLowerCase();

  for (const stagedPath of selectedFiles) {
    const trimmed = stagedPath.replace(/[\\/]+$/, "");
    const baseName = trimmed.split(/[/\\]/).pop() || "";
    if (!baseName) continue;
    const baseLower = baseName.toLowerCase();

    if (cleanEntryLower === baseLower) {
      return trimmed;
    }
    if (cleanEntryLower.startsWith(baseLower + "/")) {
      const remainder = cleanEntry.slice(baseName.length + 1).replace(/\//g, "\\");
      return `${trimmed}\\${remainder}`;
    }
  }

  return entryName;
}


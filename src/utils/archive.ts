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
 * Cleans an archive or staged path while preserving original casing:
 * converts backslashes, trims outer whitespace, and collapses empty or `.` segments.
 */
export function cleanArchiveDisplayPath(p: string): string {
  const clean = (p.includes("\\") ? p.replace(/\\/g, "/") : p).trim();
  if (!clean) return "";
  const parts = clean.split("/");
  const kept: string[] = [];
  for (let i = 0; i < parts.length; i++) {
    const seg = parts[i];
    if (!seg || seg === ".") continue;
    kept.push(seg);
  }
  return kept.join("/");
}

/**
 * Normalizes archive entry paths for consistent hierarchy and matching
 */
export function normalizeArchiveEntryPath(p: string): string {
  return p.replace(/\\/g, "/").replace(/^\.?\/+/, "").toLowerCase();
}

/**
 * Normalizes an exclusion path key to lowercase forward-slash format,
 * collapsing empty/`./` segments so `/./` and `//` variants cannot bypass rules.
 */
export function normalizeExclusionKey(p: string): string {
  return cleanArchiveDisplayPath(p).toLowerCase();
}

export interface ExclusionEntryInput {
  name: string;
  size?: number;
  is_dir?: boolean;
}

export interface EntryExclusionIndex {
  directChildren: Map<string, Set<string>>;
  dirSizeMap: Map<string, number>;
  fileSizeMap: Map<string, number>;
  knownDirs: Set<string>;
}

const EMPTY_EXCLUSION_INDEX: EntryExclusionIndex = {
  directChildren: new Map(),
  dirSizeMap: new Map(),
  fileSizeMap: new Map(),
  knownDirs: new Set(),
};

const EMPTY_NORMALIZED_SET: ReadonlySet<string> = new Set();

const entryIndexCache = new WeakMap<readonly ExclusionEntryInput[], EntryExclusionIndex>();
const normalizedSetCache = new WeakMap<ReadonlySet<string>, ReadonlySet<string>>();

function addDirectChild(
  directChildren: Map<string, Set<string>>,
  parent: string,
  child: string
): void {
  let set = directChildren.get(parent);
  if (!set) {
    set = new Set<string>();
    directChildren.set(parent, set);
  }
  set.add(child);
}

/**
 * Builds an O(1)-lookup tree index mapping each normalized folder to its direct
 * children and cumulative subtree byte sizes. Cached by entry array reference.
 */
export function getEntryExclusionIndex(
  entries?: readonly ExclusionEntryInput[] | null
): EntryExclusionIndex {
  if (!entries || entries.length === 0) {
    return EMPTY_EXCLUSION_INDEX;
  }
  const cached = entryIndexCache.get(entries);
  if (cached) {
    return cached;
  }

  const directChildren = new Map<string, Set<string>>();
  const dirSizeMap = new Map<string, number>();
  const fileSizeMap = new Map<string, number>();
  const knownDirs = new Set<string>();

  const ensureDirChain = (dirPath: string) => {
    let cur = dirPath;
    while (cur && !knownDirs.has(cur)) {
      knownDirs.add(cur);
      const lastSlash = cur.lastIndexOf("/");
      const parent = lastSlash === -1 ? "" : cur.slice(0, lastSlash);
      addDirectChild(directChildren, parent, cur);
      cur = parent;
    }
  };

  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i];
    const trimmedName = entry.name.trim();
    const clean = normalizeExclusionKey(trimmedName);
    if (!clean) continue;

    const isDirEntry =
      Boolean(entry.is_dir) || trimmedName.endsWith("/") || trimmedName.endsWith("\\");

    if (isDirEntry) {
      ensureDirChain(clean);
      continue;
    }

    const lastSlash = clean.lastIndexOf("/");
    const parent = lastSlash === -1 ? "" : clean.slice(0, lastSlash);
    addDirectChild(directChildren, parent, clean);
    if (parent) {
      ensureDirChain(parent);
    }

    const size = entry.size || 0;
    fileSizeMap.set(clean, (fileSizeMap.get(clean) || 0) + size);

    if (size > 0 && parent) {
      let anc = parent;
      while (anc) {
        dirSizeMap.set(anc, (dirSizeMap.get(anc) || 0) + size);
        const ancSlash = anc.lastIndexOf("/");
        anc = ancSlash === -1 ? "" : anc.slice(0, ancSlash);
      }
    }
  }

  const index: EntryExclusionIndex = {
    directChildren,
    dirSizeMap,
    fileSizeMap,
    knownDirs,
  };
  entryIndexCache.set(entries, index);
  return index;
}

/**
 * Normalizes every key inside an exclusion set once into canonical lowercase,
 * forward-slash format without trailing slashes. Cached by Set reference (do not mutate return value).
 */
export function normalizeExcludedSet(excludedPaths?: ReadonlySet<string> | null): ReadonlySet<string> {
  if (!excludedPaths || excludedPaths.size === 0) return EMPTY_NORMALIZED_SET;
  const cached = normalizedSetCache.get(excludedPaths);
  if (cached) return cached;

  const out = new Set<string>();
  for (const raw of excludedPaths) {
    const key = normalizeExclusionKey(raw);
    if (key) out.add(key);
  }
  normalizedSetCache.set(excludedPaths, out);
  normalizedSetCache.set(out, out);
  return out;
}

/**
 * Checks whether an already-normalized path (no trailing slash) or any of its
 * ancestor directories exists in an already-normalized exclusion set in O(depth).
 */
export function isNormalizedPathExcluded(
  normNoTrailingSlash: string,
  excludedSet: ReadonlySet<string>
): boolean {
  if (!normNoTrailingSlash || excludedSet.size === 0) return false;
  if (excludedSet.has(normNoTrailingSlash)) return true;

  let idx = normNoTrailingSlash.lastIndexOf("/");
  while (idx > 0) {
    if (excludedSet.has(normNoTrailingSlash.slice(0, idx))) {
      return true;
    }
    idx = normNoTrailingSlash.lastIndexOf("/", idx - 1);
  }
  return false;
}

/**
 * Checks whether any strict ancestor of an already-normalized path is excluded.
 */
function hasExcludedAncestor(
  normNoTrailingSlash: string,
  excludedSet: ReadonlySet<string>
): boolean {
  let idx = normNoTrailingSlash.lastIndexOf("/");
  while (idx > 0) {
    if (excludedSet.has(normNoTrailingSlash.slice(0, idx))) {
      return true;
    }
    idx = normNoTrailingSlash.lastIndexOf("/", idx - 1);
  }
  return false;
}

/**
 * Checks whether a raw path or any of its ancestor directories is excluded in O(depth).
 * Handles unnormalized `excludedSet` callers in O(1) amortized time via WeakMap caching.
 */
export function isPathExcluded(rawPath: string, excludedSet?: ReadonlySet<string> | null): boolean {
  if (!excludedSet || excludedSet.size === 0) return false;
  const normalized = normalizeExcludedSet(excludedSet);
  const clean = normalizeExclusionKey(rawPath);
  return isNormalizedPathExcluded(clean, normalized);
}

/**
 * Computes total skipped byte size for an exclusion set without scanning all archive entries.
 */
export function computeExcludedBytes(
  excludedSet?: ReadonlySet<string> | null,
  entries?: readonly ExclusionEntryInput[] | null
): number {
  if (!excludedSet || excludedSet.size === 0 || !entries || entries.length === 0) {
    return 0;
  }
  const index = getEntryExclusionIndex(entries);
  const normalized = normalizeExcludedSet(excludedSet);
  let total = 0;

  for (const key of normalized) {
    if (hasExcludedAncestor(key, normalized)) {
      continue;
    }
    total += (index.dirSizeMap.get(key) || 0) + (index.fileSizeMap.get(key) || 0);
  }

  return total;
}

/**
 * Toggles exclusion state for a file or folder in O(depth + direct siblings)
 * while preserving prefix-minimal exclusions (folders never expand all descendants).
 */
export function togglePathExclusion(
  prev: Set<string>,
  rawPath: string,
  isDir: boolean | undefined,
  allEntries: readonly ExclusionEntryInput[]
): Set<string> {
  const norm = normalizeExclusionKey(rawPath);
  const next = new Set(normalizeExcludedSet(prev));
  if (!norm) return next;

  const index = getEntryExclusionIndex(allEntries);

  // Find any strict excluded ancestors ordered shortest-to-longest
  const matchingAncestors: string[] = [];
  let slashIdx = norm.indexOf("/");
  while (slashIdx !== -1) {
    const anc = norm.slice(0, slashIdx);
    if (next.has(anc)) {
      matchingAncestors.push(anc);
    }
    slashIdx = norm.indexOf("/", slashIdx + 1);
  }

  const isCurrentlyExcluded = next.has(norm) || matchingAncestors.length > 0;
  const treatAsDir = Boolean(isDir) || index.knownDirs.has(norm);

  if (isCurrentlyExcluded) {
    for (const anc of matchingAncestors) {
      // Walk strictly down the single chain from `anc` toward `norm`, adding only
      // direct sibling entries at each level instead of walking every subtree file.
      const remainder = norm.slice(anc.length + 1);
      const parts = remainder.split("/");
      let curParent = anc;

      for (const part of parts) {
        const keepChild = `${curParent}/${part}`;
        const children = index.directChildren.get(curParent);
        if (children) {
          for (const child of children) {
            if (child !== keepChild) {
              next.add(child);
            }
          }
        }
        curParent = keepChild;
      }

      next.delete(anc);
    }

    next.delete(norm);
    if (treatAsDir) {
      const prefix = norm + "/";
      for (const exc of next) {
        if (exc.startsWith(prefix)) {
          next.delete(exc);
        }
      }
    }
  } else {
    next.add(norm);
    if (treatAsDir) {
      // Collapse any deeper exclusions subsumed under this excluded folder
      const prefix = norm + "/";
      for (const exc of next) {
        if (exc !== norm && exc.startsWith(prefix)) {
          next.delete(exc);
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


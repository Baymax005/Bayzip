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


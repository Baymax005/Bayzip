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

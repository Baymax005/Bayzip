import React from "react";

export type FileCategory =
  | "pdf"
  | "word"
  | "excel"
  | "powerpoint"
  | "image"
  | "code"
  | "archive"
  | "media"
  | "restricted"
  | "text"
  | "doc";

export function getFileCategory(name: string): FileCategory {
  if (!name) return "doc";
  const ext = name.split(".").pop()?.toLowerCase() || "";

  // 1. Security restricted binaries & dangerous scripts
  if (
    [
      "exe", "bat", "cmd", "msi", "dll", "sys", "com", "scr", "vbs", "ps1",
      "pif", "cpl", "hta", "drv", "ocx", "vbe", "wsf", "wsh", "reg"
    ].includes(ext)
  ) {
    return "restricted";
  }

  // 2. Adobe Acrobat & PDF Documents
  if (ext === "pdf") {
    return "pdf";
  }

  // 3. Microsoft Word Documents
  if (["doc", "docx", "docm", "dot", "dotx", "rtf", "odt", "wps"].includes(ext)) {
    return "word";
  }

  // 4. Microsoft Excel Spreadsheets
  if (["xls", "xlsx", "xlsm", "xlsb", "xlt", "xltx", "csv", "tsv", "ods"].includes(ext)) {
    return "excel";
  }

  // 5. Microsoft PowerPoint Presentations
  if (["ppt", "pptx", "pptm", "pot", "potx", "pps", "ppsx", "odp"].includes(ext)) {
    return "powerpoint";
  }

  // 6. Visual Media / Images
  if (
    [
      "png", "jpg", "jpeg", "webp", "gif", "bmp", "ico", "svg",
      "tiff", "tif", "avif", "heic", "raw", "psd"
    ].includes(ext)
  ) {
    return "image";
  }

  // 7. Archives & Compressed Packages
  if (
    [
      "zip", "7z", "bay", "bayg", "tar", "gz", "tgz", "rar",
      "bz2", "xz", "iso", "cab", "zst", "lz4", "dmg", "apk"
    ].includes(ext)
  ) {
    return "archive";
  }

  // 8. Audio / Video Media
  if (
    [
      "mp3", "wav", "flac", "ogg", "m4a", "aac", "wma",
      "mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "m4v"
    ].includes(ext)
  ) {
    return "media";
  }

  // 9. Source Code, Markup & Configs
  if (
    [
      "js", "ts", "jsx", "tsx", "rs", "py", "c", "cpp", "cc", "cxx", "h", "hpp", "cs",
      "java", "go", "html", "htm", "css", "scss", "sass", "json", "yaml", "yml", "toml",
      "xml", "sql", "sh", "bash", "zsh", "lua", "r", "dart", "swift", "kt", "kts", "rb", "php"
    ].includes(ext)
  ) {
    return "code";
  }

  // 10. Plain text / Markdown / Notes
  if (
    [
      "txt", "md", "markdown", "log", "ini", "cfg", "conf", "env",
      "gitignore", "editorconfig", "properties"
    ].includes(ext)
  ) {
    return "text";
  }

  return "doc";
}

export interface FileFormatIconProps {
  fileName?: string;
  category?: FileCategory;
  size?: "sm" | "md" | "lg" | "xl";
  className?: string;
}

export const FileFormatIcon: React.FC<FileFormatIconProps> = ({
  fileName = "",
  category,
  size = "sm",
  className = "",
}) => {
  const cat = category || getFileCategory(fileName);

  // Large / Hero View (Used for Preview modal centers & detailed cards)
  if (size === "lg" || size === "xl") {
    const iconDim = size === "xl" ? "w-20 h-20" : "w-14 h-14";

    if (cat === "pdf") {
      return (
        <div className={`relative flex items-center justify-center shrink-0 ${className}`}>
          <div className="absolute w-24 h-24 bg-red-600/25 rounded-full blur-2xl pointer-events-none" />
          <svg viewBox="0 0 64 64" className={`${iconDim} drop-shadow-xl shrink-0`} fill="none">
            <defs>
              <linearGradient id="pdfGradLg" x1="0" y1="0" x2="64" y2="64" gradientUnits="userSpaceOnUse">
                <stop offset="0%" stopColor="#FF333B" />
                <stop offset="60%" stopColor="#D9141E" />
                <stop offset="100%" stopColor="#8F0B11" />
              </linearGradient>
              <linearGradient id="pdfFoldGrad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#FF9B9F" />
                <stop offset="100%" stopColor="#E55056" />
              </linearGradient>
            </defs>
            {/* Document sheet */}
            <path d="M14 6a6 6 0 0 1 6-6h22l16 16v40a6 6 0 0 1-6 6H20a6 6 0 0 1-6-6V6z" fill="url(#pdfGradLg)" />
            {/* Corner fold */}
            <path d="M42 0l16 16H46a4 4 0 0 1-4-4V0z" fill="url(#pdfFoldGrad)" />
            {/* Subtle page lines */}
            <rect x="22" y="24" width="18" height="2.5" rx="1.25" fill="rgba(255,255,255,0.35)" />
            <rect x="22" y="30" width="12" height="2.5" rx="1.25" fill="rgba(255,255,255,0.35)" />
            {/* Authentic PDF Badge pill */}
            <g transform="translate(18, 38)">
              <rect x="0" y="0" width="30" height="17" rx="3.5" fill="#FFFFFF" />
              <text
                x="15"
                y="12.5"
                textAnchor="middle"
                fill="#D9141E"
                fontSize="11.5"
                fontWeight="900"
                fontFamily="Segoe UI, Inter, Arial, sans-serif"
                letterSpacing="0.2"
              >
                PDF
              </text>
            </g>
          </svg>
        </div>
      );
    }

    if (cat === "word") {
      return (
        <div className={`relative flex items-center justify-center shrink-0 ${className}`}>
          <div className="absolute w-24 h-24 bg-blue-600/25 rounded-full blur-2xl pointer-events-none" />
          <svg viewBox="0 0 64 64" className={`${iconDim} drop-shadow-xl shrink-0`} fill="none">
            <defs>
              <linearGradient id="wordGradLg" x1="0" y1="0" x2="64" y2="64" gradientUnits="userSpaceOnUse">
                <stop offset="0%" stopColor="#2B7CD3" />
                <stop offset="55%" stopColor="#185ABD" />
                <stop offset="100%" stopColor="#0E3D82" />
              </linearGradient>
              <linearGradient id="wordFoldGrad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#96C2FF" />
                <stop offset="100%" stopColor="#4A8DFF" />
              </linearGradient>
              <linearGradient id="wordTileGrad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#1C6CE0" />
                <stop offset="100%" stopColor="#0B4294" />
              </linearGradient>
            </defs>
            <path d="M14 6a6 6 0 0 1 6-6h22l16 16v40a6 6 0 0 1-6 6H20a6 6 0 0 1-6-6V6z" fill="url(#wordGradLg)" />
            <path d="M42 0l16 16H46a4 4 0 0 1-4-4V0z" fill="url(#wordFoldGrad)" />
            <rect x="22" y="22" width="20" height="2.5" rx="1.25" fill="rgba(255,255,255,0.35)" />
            <rect x="22" y="28" width="14" height="2.5" rx="1.25" fill="rgba(255,255,255,0.35)" />
            {/* Distinctive Microsoft Word 3D Tile with 'W' */}
            <g transform="translate(18, 33)">
              <rect x="0" y="0" width="30" height="23" rx="4.5" fill="url(#wordTileGrad)" stroke="rgba(255,255,255,0.25)" strokeWidth="1" />
              <text
                x="15"
                y="17"
                textAnchor="middle"
                fill="#FFFFFF"
                fontSize="15"
                fontWeight="900"
                fontFamily="Segoe UI, Inter, Arial, sans-serif"
              >
                W
              </text>
            </g>
          </svg>
        </div>
      );
    }

    if (cat === "excel") {
      return (
        <div className={`relative flex items-center justify-center shrink-0 ${className}`}>
          <div className="absolute w-24 h-24 bg-emerald-600/25 rounded-full blur-2xl pointer-events-none" />
          <svg viewBox="0 0 64 64" className={`${iconDim} drop-shadow-xl shrink-0`} fill="none">
            <defs>
              <linearGradient id="excelGradLg" x1="0" y1="0" x2="64" y2="64" gradientUnits="userSpaceOnUse">
                <stop offset="0%" stopColor="#1E9B56" />
                <stop offset="55%" stopColor="#107C41" />
                <stop offset="100%" stopColor="#0B522B" />
              </linearGradient>
              <linearGradient id="excelFoldGrad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#8DF2BA" />
                <stop offset="100%" stopColor="#34D399" />
              </linearGradient>
              <linearGradient id="excelTileGrad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#16A34A" />
                <stop offset="100%" stopColor="#065F46" />
              </linearGradient>
            </defs>
            <path d="M14 6a6 6 0 0 1 6-6h22l16 16v40a6 6 0 0 1-6 6H20a6 6 0 0 1-6-6V6z" fill="url(#excelGradLg)" />
            <path d="M42 0l16 16H46a4 4 0 0 1-4-4V0z" fill="url(#excelFoldGrad)" />
            <rect x="22" y="22" width="20" height="2.5" rx="1.25" fill="rgba(255,255,255,0.35)" />
            <rect x="22" y="28" width="14" height="2.5" rx="1.25" fill="rgba(255,255,255,0.35)" />
            <g transform="translate(18, 33)">
              <rect x="0" y="0" width="30" height="23" rx="4.5" fill="url(#excelTileGrad)" stroke="rgba(255,255,255,0.25)" strokeWidth="1" />
              <text
                x="15"
                y="17"
                textAnchor="middle"
                fill="#FFFFFF"
                fontSize="15"
                fontWeight="900"
                fontFamily="Segoe UI, Inter, Arial, sans-serif"
              >
                X
              </text>
            </g>
          </svg>
        </div>
      );
    }

    if (cat === "powerpoint") {
      return (
        <div className={`relative flex items-center justify-center shrink-0 ${className}`}>
          <div className="absolute w-24 h-24 bg-orange-600/25 rounded-full blur-2xl pointer-events-none" />
          <svg viewBox="0 0 64 64" className={`${iconDim} drop-shadow-xl shrink-0`} fill="none">
            <defs>
              <linearGradient id="pptGradLg" x1="0" y1="0" x2="64" y2="64" gradientUnits="userSpaceOnUse">
                <stop offset="0%" stopColor="#F96332" />
                <stop offset="55%" stopColor="#D24726" />
                <stop offset="100%" stopColor="#9C2710" />
              </linearGradient>
              <linearGradient id="pptFoldGrad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#FFB394" />
                <stop offset="100%" stopColor="#FB923C" />
              </linearGradient>
              <linearGradient id="pptTileGrad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#EA580C" />
                <stop offset="100%" stopColor="#9A3412" />
              </linearGradient>
            </defs>
            <path d="M14 6a6 6 0 0 1 6-6h22l16 16v40a6 6 0 0 1-6 6H20a6 6 0 0 1-6-6V6z" fill="url(#pptGradLg)" />
            <path d="M42 0l16 16H46a4 4 0 0 1-4-4V0z" fill="url(#pptFoldGrad)" />
            <rect x="22" y="22" width="20" height="2.5" rx="1.25" fill="rgba(255,255,255,0.35)" />
            <rect x="22" y="28" width="14" height="2.5" rx="1.25" fill="rgba(255,255,255,0.35)" />
            <g transform="translate(18, 33)">
              <rect x="0" y="0" width="30" height="23" rx="4.5" fill="url(#pptTileGrad)" stroke="rgba(255,255,255,0.25)" strokeWidth="1" />
              <text
                x="15"
                y="17"
                textAnchor="middle"
                fill="#FFFFFF"
                fontSize="15"
                fontWeight="900"
                fontFamily="Segoe UI, Inter, Arial, sans-serif"
              >
                P
              </text>
            </g>
          </svg>
        </div>
      );
    }
  }

  // Small (16x16 / w-4 h-4) & Medium (24x24 / w-6 h-6 or 32x32)
  const sizeClasses =
    size === "md"
      ? "w-6 h-6 shrink-0"
      : size === "lg"
      ? "w-12 h-12 shrink-0"
      : size === "xl"
      ? "w-16 h-16 shrink-0"
      : "w-4 h-4 shrink-0";

  switch (cat) {
    case "pdf":
      return (
        <svg
          viewBox="0 0 24 24"
          fill="none"
          className={`${sizeClasses} ${className}`}
          xmlns="http://www.w3.org/2000/svg"
        >
          {/* Main Document Body */}
          <path
            d="M4 4a2 2 0 0 1 2-2h8.5L20 7.5V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4z"
            fill="#E5252A"
          />
          {/* Folded Corner Tab */}
          <path d="M14.5 2v5.5H20L14.5 2z" fill="#FFA4A7" />
          {/* Crisp White PDF Emblem */}
          <text
            x="12"
            y="17.2"
            textAnchor="middle"
            fill="#FFFFFF"
            fontSize="6.6"
            fontWeight="900"
            fontFamily="Segoe UI, Inter, -apple-system, sans-serif"
            letterSpacing="-0.3"
          >
            PDF
          </text>
        </svg>
      );

    case "word":
      return (
        <svg
          viewBox="0 0 24 24"
          fill="none"
          className={`${sizeClasses} ${className}`}
          xmlns="http://www.w3.org/2000/svg"
        >
          {/* Main Document Body */}
          <path
            d="M4 4a2 2 0 0 1 2-2h8.5L20 7.5V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4z"
            fill="#185ABD"
          />
          {/* Folded Corner Tab */}
          <path d="M14.5 2v5.5H20L14.5 2z" fill="#84B6FF" />
          {/* Crisp White Word 'W' */}
          <text
            x="12"
            y="17.2"
            textAnchor="middle"
            fill="#FFFFFF"
            fontSize="9"
            fontWeight="900"
            fontFamily="Segoe UI, Inter, -apple-system, sans-serif"
          >
            W
          </text>
        </svg>
      );

    case "excel":
      return (
        <svg
          viewBox="0 0 24 24"
          fill="none"
          className={`${sizeClasses} ${className}`}
          xmlns="http://www.w3.org/2000/svg"
        >
          <path
            d="M4 4a2 2 0 0 1 2-2h8.5L20 7.5V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4z"
            fill="#107C41"
          />
          <path d="M14.5 2v5.5H20L14.5 2z" fill="#86EFAC" />
          <text
            x="12"
            y="17.2"
            textAnchor="middle"
            fill="#FFFFFF"
            fontSize="9"
            fontWeight="900"
            fontFamily="Segoe UI, Inter, -apple-system, sans-serif"
          >
            X
          </text>
        </svg>
      );

    case "powerpoint":
      return (
        <svg
          viewBox="0 0 24 24"
          fill="none"
          className={`${sizeClasses} ${className}`}
          xmlns="http://www.w3.org/2000/svg"
        >
          <path
            d="M4 4a2 2 0 0 1 2-2h8.5L20 7.5V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4z"
            fill="#D24726"
          />
          <path d="M14.5 2v5.5H20L14.5 2z" fill="#FDBA74" />
          <text
            x="12"
            y="17.2"
            textAnchor="middle"
            fill="#FFFFFF"
            fontSize="9"
            fontWeight="900"
            fontFamily="Segoe UI, Inter, -apple-system, sans-serif"
          >
            P
          </text>
        </svg>
      );

    case "image":
      return (
        <svg
          className={`${sizeClasses} text-purple-400 ${className}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.8}
            d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
          />
        </svg>
      );

    case "archive":
      return (
        <svg
          className={`${sizeClasses} text-amber-400 ${className}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.8}
            d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4"
          />
        </svg>
      );

    case "code":
      return (
        <svg
          className={`${sizeClasses} text-[#60CDFF] ${className}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.8}
            d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
          />
        </svg>
      );

    case "media":
      return (
        <svg
          className={`${sizeClasses} text-pink-400 ${className}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.8}
            d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"
          />
        </svg>
      );

    case "restricted":
      return (
        <svg
          className={`${sizeClasses} text-rose-400 ${className}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.8}
            d="M12 15v2m0 0v2m0-2h2m-2 0H10m11-5a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
      );

    case "text":
    case "doc":
    default:
      return (
        <svg
          className={`${sizeClasses} text-[#A8A8A8] ${className}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.8}
            d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"
          />
        </svg>
      );
  }
};

import React, { useState, useEffect, useRef } from "react";
import { DriveInfo } from "../../types";

export interface WorkspaceNavBarProps {
  currentPath: string;
  onNavigate: (path: string) => void;
  onRefresh?: () => void;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  drives?: DriveInfo[];
  activeDrive?: string;
  canGoBack: boolean;
  canGoForward: boolean;
  canGoUp?: boolean;
  onBack: () => void;
  onForward: () => void;
  onUp: () => void;
  isLoading?: boolean;
  hasError?: boolean;
  isPathEditingRequested?: number;
  sourceType?: "filesystem" | "archive";
  rootLabel?: string;
  isSplitView?: boolean;
  onToggleSplitView?: () => void;
}

export const WorkspaceNavBar: React.FC<WorkspaceNavBarProps> = ({
  currentPath,
  onNavigate,
  onRefresh,
  searchQuery,
  onSearchChange,
  canGoBack,
  canGoForward,
  canGoUp = true,
  onBack,
  onForward,
  onUp,
  isLoading = false,
  hasError = false,
  isPathEditingRequested,
  sourceType = "filesystem",
  rootLabel,
  isSplitView = false,
  onToggleSplitView,
}) => {
  const [isEditingPath, setIsEditingPath] = useState(false);
  const [inputPath, setInputPath] = useState(currentPath);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setInputPath(currentPath);
  }, [currentPath]);

  useEffect(() => {
    if (isPathEditingRequested) {
      setIsEditingPath(true);
    }
  }, [isPathEditingRequested]);

  useEffect(() => {
    if (isEditingPath && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditingPath]);

  const handleCommitPath = () => {
    let trimmed = inputPath.trim();
    if (trimmed) {
      if (/^[a-zA-Z]:$/.test(trimmed)) {
        trimmed = `${trimmed}\\`;
      }
      onNavigate(trimmed);
    }
    setIsEditingPath(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      handleCommitPath();
    } else if (e.key === "Escape") {
      setInputPath(currentPath);
      setIsEditingPath(false);
    }
  };

  // Parse path segments for clickable breadcrumbs
  const normalized = currentPath.replace(/\\/g, "/");
  const segments = normalized.split("/").filter(Boolean);

  return (
    <div className="bg-[#1C1C1C] border-b border-[#2C2C2C] px-3 py-1.5 flex items-center space-x-2 text-xs shrink-0 select-none">
      {/* HCI Navigation Cluster (Back, Forward, Up, Refresh) */}
      <div className="flex items-center space-x-1 shrink-0 bg-[#242424] p-0.5 rounded-md border border-[#333333]">
        <button
          type="button"
          onClick={onBack}
          disabled={!canGoBack}
          className="p-1 rounded text-[#A8A8A8] hover:text-white hover:bg-[#323232] disabled:opacity-30 disabled:hover:bg-transparent disabled:hover:text-[#A8A8A8] transition-colors cursor-pointer disabled:cursor-not-allowed"
          title="Back (Alt+Left)"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M15 19l-7-7 7-7" />
          </svg>
        </button>

        <button
          type="button"
          onClick={onForward}
          disabled={!canGoForward}
          className="p-1 rounded text-[#A8A8A8] hover:text-white hover:bg-[#323232] disabled:opacity-30 disabled:hover:bg-transparent disabled:hover:text-[#A8A8A8] transition-colors cursor-pointer disabled:cursor-not-allowed"
          title="Forward (Alt+Right)"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M9 5l7 7-7 7" />
          </svg>
        </button>

        <div className="w-[1px] h-3.5 bg-[#3A3A3A] my-auto" />

        <button
          type="button"
          onClick={onUp}
          disabled={!canGoUp}
          className="p-1 rounded text-[#60CDFF] hover:text-white hover:bg-[#323232] disabled:opacity-30 disabled:hover:bg-transparent disabled:text-[#777777] disabled:hover:text-[#777777] transition-colors cursor-pointer disabled:cursor-not-allowed flex items-center space-x-1"
          title={canGoUp ? "Up to Parent Directory (Alt+Up)" : "Already at Root (No Parent Directory)"}
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M5 10l7-7m0 0l7 7m-7-7v18" />
          </svg>
          <span className="text-[10px] font-semibold pr-0.5">Up</span>
        </button>

        {onRefresh && (
          <button
            type="button"
            onClick={onRefresh}
            className={`p-1 rounded text-[#A8A8A8] hover:text-white hover:bg-[#323232] transition-colors cursor-pointer ${
              isLoading ? "animate-spin text-[#60CDFF]" : ""
            }`}
            title="Refresh Directory (F5)"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </button>
        )}
      </div>

      {/* Address Bar: Breadcrumb Display or Custom Editable Path Input */}
      <div
        className={`flex-1 min-w-0 flex items-center rounded-md border px-2 py-1 transition-all ${
          hasError
            ? "border-amber-500/70 bg-[#251A1D] focus-within:border-amber-400"
            : "border-[#333333] bg-[#222222] hover:bg-[#262626] focus-within:bg-[#1E1E1E] focus-within:border-[#60CDFF]"
        }`}
      >
        <span
          className={`text-sm mr-1.5 shrink-0 ${hasError ? "text-amber-400" : "text-[#60CDFF]"}`}
          title={hasError ? "Path not found or inaccessible" : undefined}
        >
          {hasError ? "⚠️" : sourceType === "archive" ? "📦" : "📁"}
        </span>

        {isEditingPath ? (
          <div className="flex-1 flex items-center min-w-0">
            <input
              ref={inputRef}
              type="text"
              value={inputPath}
              onChange={(e) => setInputPath(e.target.value)}
              onKeyDown={handleKeyDown}
              onBlur={handleCommitPath}
              placeholder={
                sourceType === "archive"
                  ? "Enter archive folder path (e.g. assets/textures)..."
                  : "Enter drive or folder path (e.g. C:\\Users\\... or downloads)"
              }
              className="w-full bg-transparent text-white font-mono text-xs outline-none border-none p-0"
            />
            {inputPath && (
              <button
                type="button"
                onClick={() => setInputPath("")}
                className="text-[#666666] hover:text-white px-1 text-xs cursor-pointer shrink-0"
                title="Clear input"
              >
                ✕
              </button>
            )}
            <button
              type="button"
              onClick={handleCommitPath}
              className="ml-1 text-[10px] px-2 py-0.5 bg-[#173042] text-[#60CDFF] hover:bg-[#20495f] rounded border border-[#60CDFF]/40 cursor-pointer shrink-0 font-semibold"
            >
              Go
            </button>
          </div>
        ) : (
          <div
            onClick={() => setIsEditingPath(true)}
            className="flex-1 min-w-0 flex items-center space-x-1 font-mono text-xs text-[#A8A8A8] overflow-x-auto custom-scrollbar cursor-text py-0.5"
            title="Click to enter or edit custom path"
          >
            {sourceType === "archive" ? (
              <>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onNavigate("");
                  }}
                  className={`hover:text-white px-1.5 py-0.5 rounded transition-colors cursor-pointer flex items-center space-x-1 shrink-0 ${
                    segments.length === 0
                      ? "text-white font-bold bg-[#2E2E2E]"
                      : "hover:bg-[#303030] text-[#CCCCCC]"
                  }`}
                  title="Archive Root"
                >
                  <span>{rootLabel || "Archive Root"}</span>
                </button>
                {segments.map((seg, idx) => {
                  const subPath = segments.slice(0, idx + 1).join("/");
                  const isLast = idx === segments.length - 1;
                  return (
                    <React.Fragment key={`${seg}-${idx}`}>
                      <span className="text-[#555555] shrink-0">/</span>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onNavigate(subPath);
                        }}
                        className={`hover:text-white px-1 py-0.5 rounded transition-colors cursor-pointer truncate shrink-0 ${
                          isLast
                            ? "text-white font-bold bg-[#2E2E2E]"
                            : "hover:bg-[#303030] text-[#CCCCCC]"
                        }`}
                        title={subPath}
                      >
                        {seg}
                      </button>
                    </React.Fragment>
                  );
                })}
              </>
            ) : segments.length === 0 ? (
              <span className="text-white font-semibold">{rootLabel || "This PC"}</span>
            ) : (
              segments.map((seg, idx) => {
                const subPath =
                  segments
                    .slice(0, idx + 1)
                    .join("\\") + (idx === 0 && seg.includes(":") ? "\\" : "");
                const isLast = idx === segments.length - 1;
                const displayLabel = idx === 0 && seg.includes(":") ? `${seg}\\` : seg;

                return (
                  <React.Fragment key={`${seg}-${idx}`}>
                    {idx > 0 && <span className="text-[#555555] shrink-0">›</span>}
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onNavigate(subPath);
                      }}
                      className={`hover:text-white px-1 py-0.5 rounded transition-colors cursor-pointer truncate shrink-0 ${
                        isLast
                          ? hasError
                            ? "text-amber-300 font-bold bg-[#331C1F] border border-amber-600/40"
                            : "text-white font-bold bg-[#2E2E2E]"
                          : "hover:bg-[#303030] text-[#CCCCCC]"
                      }`}
                      title={subPath}
                    >
                      {displayLabel}
                    </button>
                  </React.Fragment>
                );
              })
            )}

            {/* Quick Actions Cluster: Copy Path & Edit Path */}
            <div className="ml-auto flex items-center space-x-1 shrink-0">
              <button
                type="button"
                onClick={async (e) => {
                  e.stopPropagation();
                  try {
                    await navigator.clipboard.writeText(currentPath);
                  } catch {}
                }}
                className="text-[#666666] hover:text-[#60CDFF] p-0.5 cursor-pointer rounded hover:bg-[#303030]"
                title="Copy path to clipboard"
              >
                <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3" />
                </svg>
              </button>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  setIsEditingPath(true);
                }}
                className="text-[#666666] hover:text-[#60CDFF] p-0.5 cursor-pointer rounded hover:bg-[#303030]"
                title="Edit Path (Click to enter custom path)"
              >
                <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                </svg>
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Dual-Pane Split View Toggle Button */}
      {onToggleSplitView && (
        <button
          type="button"
          onClick={onToggleSplitView}
          className={`px-2 py-1 rounded-md text-xs font-semibold flex items-center space-x-1.5 transition-all cursor-pointer shrink-0 border ${
            isSplitView
              ? "bg-[#25394A] text-[#60CDFF] border-[#60CDFF]/60 shadow-[0_0_10px_rgba(96,205,255,0.25)]"
              : "bg-[#242424] hover:bg-[#2F2F2F] text-[#A8A8A8] hover:text-white border-[#383838]"
          }`}
          title={isSplitView ? "Close Split View (Single Pane)" : "Instant Dual-Pane Split View (⫴)"}
        >
          <span className="font-mono text-sm leading-none font-bold">⫴</span>
          <span className="hidden sm:inline text-[11px] font-sans">
            {isSplitView ? "Dual-Pane" : "Split View"}
          </span>
        </button>
      )}

      {/* Integrated Search Box */}
      <div className="w-48 sm:w-56 shrink-0 flex items-center bg-[#222222] rounded-md border border-[#333333] focus-within:border-[#60CDFF] px-2 py-1 transition-all">
        <svg className="w-3.5 h-3.5 text-[#777777] mr-1.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={sourceType === "archive" ? "Search archive contents..." : "Search folder contents..."}
          className="w-full bg-transparent text-white font-mono text-xs outline-none border-none p-0"
        />
        {searchQuery && (
          <button
            type="button"
            onClick={() => onSearchChange("")}
            className="text-[#888888] hover:text-white ml-1 p-0.5 cursor-pointer shrink-0 text-xs font-bold"
            title="Clear search"
          >
            ✕
          </button>
        )}
      </div>
    </div>
  );
};

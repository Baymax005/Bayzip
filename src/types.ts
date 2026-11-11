export interface SystemSpecs {
  total_ram_gb: number;
  free_ram_gb: number;
  cpu_cores: number;
}

export interface CompressProgress {
  percent: number;
  current_file: string;
  bytes_processed: number;
  total_bytes: number;
}

export interface CompressResult {
  success: boolean;
  output_path: string;
  original_size: number;
  compressed_size: number;
  elapsed_ms: number;
  error?: string;
}

export interface SelectedItemInfo {
  path: string;
  name: string;
  size: number;
  is_dir: boolean;
}

export interface ExtractProgress {
  percent: number;
  current_file: string;
  bytes_extracted: number;
}

export interface ExtractResult {
  success: boolean;
  output_directory: string;
  file_count: number;
  elapsed_ms: number;
  error?: string;
}

export interface ArchiveEntryItem {
  name: string;
  size: number;
  is_dir: boolean;
  leafName?: string;
}

export interface ArchiveEntryPreview {
  name: string;
  size: number;
  preview_type: "text" | "image" | "binary" | "restricted" | "unsupported" | "error";
  content?: string;
  mime_type?: string;
  is_restricted: boolean;
  warning?: string;
}

export interface ArchiveInfo {
  path: string;
  filename: string;
  size: number;
  format: string;
  is_archive: boolean;
  is_encrypted: boolean;
  file_count?: number;
  total_uncompressed_size?: number;
  entries?: ArchiveEntryItem[];
}

export interface StartupCommand {
  action?: "compress" | "extract" | "extract-here" | "extract-to-folder" | "mini-extract" | null;
  paths: string[];
}

export interface OperationProgressInfo {
  percent: number;
  label: string;
  current_file?: string;
  bytes_processed?: number;
  total_bytes?: number;
  onCancel?: () => void;
  isCancelling?: boolean;
}

export interface TestArchiveProgress {
  percent: number;
  current_file: string;
  bytes_tested: number;
  total_bytes: number;
}

export interface TestArchiveErrorItem {
  file_name: string;
  error: string;
}

export interface TestArchiveResponse {
  success: boolean;
  is_ok: boolean;
  total_files: number;
  tested_files: number;
  total_bytes: number;
  errors: TestArchiveErrorItem[];
  elapsed_ms: number;
  error?: string | null;
}

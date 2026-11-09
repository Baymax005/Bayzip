use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ArchiveFormat {
    Bay,
    Zip,
    SevenZip,
    Tar,
    TarGz,
    Rar,
    Bzip2,
    Xz,
    Zstd,
    Iso,
    Cab,
    Unknown,
}

#[derive(Serialize, Clone, Debug)]
pub struct SystemSpecs {
    pub total_ram_gb: f64,
    pub free_ram_gb: f64,
    pub cpu_cores: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct StartupCommand {
    pub action: Option<String>,
    pub paths: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SelectedItemInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArchiveEntryItem {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct ArchiveEntryPreview {
    pub name: String,
    pub size: u64,
    pub preview_type: String,
    pub content: Option<String>,
    pub mime_type: Option<String>,
    pub is_restricted: bool,
    pub warning: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ArchiveInfo {
    pub path: String,
    pub filename: String,
    pub size: u64,
    pub format: String,
    pub is_archive: bool,
    pub is_encrypted: bool,
    pub file_count: usize,
    pub total_uncompressed_size: u64,
    pub entries: Vec<ArchiveEntryItem>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CompressPayload {
    pub files: Vec<String>,
    pub format: String,
    pub algorithm: String,
    pub slider_value: i32,
    pub custom_name: Option<String>,
    pub encryption: Option<String>,
    pub password: Option<String>,
    pub output_dir: Option<String>,
    pub smart_bypass: Option<bool>,
    pub excluded_paths: Option<Vec<String>>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CompressProgress {
    pub percent: f64,
    pub current_file: String,
    pub bytes_processed: u64,
    pub total_bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct CompressResponse {
    pub success: bool,
    pub output_path: String,
    pub original_size: u64,
    pub compressed_size: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ExtractProgress {
    pub percent: f64,
    pub current_file: String,
    pub bytes_extracted: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct ExtractResponse {
    pub success: bool,
    pub output_directory: String,
    pub file_count: usize,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TestArchiveProgress {
    pub percent: f64,
    pub current_file: String,
    pub bytes_tested: u64,
    pub total_bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct TestArchiveErrorItem {
    pub file_name: String,
    pub error: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct TestArchiveResponse {
    pub success: bool,
    pub is_ok: bool,
    pub total_files: usize,
    pub tested_files: usize,
    pub total_bytes: u64,
    pub errors: Vec<TestArchiveErrorItem>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

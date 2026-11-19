use std::path::Path;

/// Hardware bus classification of a storage volume
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveBusKind {
    UsbRemovable, // External USB / Flash Thumb Drives / SD Cards / Removable Media
    Sata,         // Internal SATA SSD / HDD / SCSI / SAS / RAID
    Nvme,         // Internal PCIe NVMe SSD / SCM
    Unknown,      // Network / UNC / Virtual / Unknown
}

/// Query the physical drive bus kind and seek penalty for a given path.
/// Automatically detects and caches drive letters A through Z via Win32 IOCTL_STORAGE_QUERY_PROPERTY.
#[cfg(target_os = "windows")]
fn get_drive_profile(path: &Path) -> (DriveBusKind, bool) {
    use std::sync::atomic::{AtomicU8, Ordering};

    // Fast static cache for drive letters A through Z (26 slots)
    // Bus: 0 = unqueried, 1 = UsbRemovable, 2 = Sata, 3 = Nvme, 4 = Unknown
    static DRIVE_BUS_CACHE: [AtomicU8; 26] = [
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0),
    ];
    // Seek penalty: 0 = unqueried, 1 = no seek penalty (SSD/NVMe), 2 = incurs seek penalty (HDD/conservative)
    static DRIVE_SEEK_PENALTY_CACHE: [AtomicU8; 26] = [
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0),
    ];

    fn find_drive_letter(s: &str) -> Option<char> {
        let chars: Vec<char> = s.chars().collect();
        for i in 1..chars.len() {
            if chars[i] == ':' && chars[i - 1].is_ascii_alphabetic() {
                return Some(chars[i - 1].to_ascii_uppercase());
            }
        }
        None
    }

    let path_str = path.to_string_lossy();
    let drive_letter = match find_drive_letter(&path_str) {
        Some(dl) => dl,
        None => {
            let canon = path.canonicalize()
                .or_else(|_| {
                    if let Some(parent) = path.parent() {
                        parent.canonicalize()
                    } else {
                        Err(std::io::Error::new(std::io::ErrorKind::NotFound, ""))
                    }
                })
                .or_else(|_| std::env::current_dir());
            if let Ok(canon_path) = canon {
                match find_drive_letter(&canon_path.to_string_lossy()) {
                    Some(dl) => dl,
                    None => return (DriveBusKind::Unknown, true),
                }
            } else {
                return (DriveBusKind::Unknown, true);
            }
        }
    };

    let cache_idx = (drive_letter as u8).saturating_sub(b'A') as usize;
    if cache_idx < 26 {
        let cached_bus = DRIVE_BUS_CACHE[cache_idx].load(Ordering::Relaxed);
        let cached_seek = DRIVE_SEEK_PENALTY_CACHE[cache_idx].load(Ordering::Relaxed);
        if cached_bus != 0 && cached_seek != 0 {
            let bus = match cached_bus {
                1 => DriveBusKind::UsbRemovable,
                2 => DriveBusKind::Sata,
                3 => DriveBusKind::Nvme,
                _ => DriveBusKind::Unknown,
            };
            return (bus, cached_seek == 2);
        }
    }

    let (bus_kind, incurs_seek_penalty) = detect_drive_profile(drive_letter);

    if cache_idx < 26 {
        let bus_val = match bus_kind {
            DriveBusKind::UsbRemovable => 1,
            DriveBusKind::Sata => 2,
            DriveBusKind::Nvme => 3,
            DriveBusKind::Unknown => 4,
        };
        let seek_val = if incurs_seek_penalty { 2 } else { 1 };
        DRIVE_BUS_CACHE[cache_idx].store(bus_val, Ordering::Relaxed);
        DRIVE_SEEK_PENALTY_CACHE[cache_idx].store(seek_val, Ordering::Relaxed);
    }

    (bus_kind, incurs_seek_penalty)
}

#[cfg(target_os = "windows")]
pub fn get_drive_bus_kind(path: &Path) -> DriveBusKind {
    get_drive_profile(path).0
}

#[cfg(target_os = "windows")]
pub fn get_drive_seek_penalty(path: &Path) -> bool {
    get_drive_profile(path).1
}

#[cfg(target_os = "windows")]
fn detect_drive_profile(drive_letter: char) -> (DriveBusKind, bool) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDriveTypeW(lpRootPathName: *const u16) -> u32;
        fn CreateFileW(
            lpFileName: *const u16,
            dwDesiredAccess: u32,
            dwShareMode: u32,
            lpSecurityAttributes: *const std::ffi::c_void,
            dwCreationDisposition: u32,
            dwFlagsAndAttributes: u32,
            hTemplateFile: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        fn DeviceIoControl(
            hDevice: *mut std::ffi::c_void,
            dwIoControlCode: u32,
            lpInBuffer: *const std::ffi::c_void,
            nInBufferSize: u32,
            lpOutBuffer: *mut std::ffi::c_void,
            nOutBufferSize: u32,
            lpBytesReturned: *mut u32,
            lpOverlapped: *mut std::ffi::c_void,
        ) -> i32;
        fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
    }

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_CDROM: u32 = 5;

    let root_path = format!("{}:\\", drive_letter);
    let root_wide = to_wide(&root_path);
    let drive_type = unsafe { GetDriveTypeW(root_wide.as_ptr()) };
    if drive_type == DRIVE_REMOVABLE || drive_type == DRIVE_CDROM {
        return (DriveBusKind::UsbRemovable, true);
    }

    let volume_path = format!("\\\\.\\{}:", drive_letter);
    let volume_wide = to_wide(&volume_path);

    const FILE_SHARE_READ: u32 = 0x00000001;
    const FILE_SHARE_WRITE: u32 = 0x00000002;
    const FILE_SHARE_DELETE: u32 = 0x00000004;
    const OPEN_EXISTING: u32 = 3;
    const INVALID_HANDLE_VALUE: isize = -1;
    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D1400;

    let handle = unsafe {
        CreateFileW(
            volume_wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };

    if handle as isize == INVALID_HANDLE_VALUE || handle.is_null() {
        return (DriveBusKind::Unknown, true);
    }

    let query: [u8; 12] = [0u8; 12];
    let mut out_buf: [u8; 1024] = [0u8; 1024];
    let mut bytes_returned: u32 = 0;

    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            query.as_ptr() as *const _,
            query.len() as u32,
            out_buf.as_mut_ptr() as *mut _,
            out_buf.len() as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };

    let mut bus_kind = DriveBusKind::Unknown;
    if ok != 0 && bytes_returned >= 32 {
        let removable_media = out_buf[10] != 0;
        let bus_type = u32::from_le_bytes([
            out_buf[28],
            out_buf[29],
            out_buf[30],
            out_buf[31],
        ]);

        // BusTypeUsb = 7, BusTypeSd = 12 (0x0C), BusTypeMmc = 13 (0x0D), or marked RemovableMedia
        if bus_type == 7 || bus_type == 12 || bus_type == 13 || removable_media {
            bus_kind = DriveBusKind::UsbRemovable;
        } else if bus_type == 17 || bus_type == 18 {
            // BusTypeNvme = 17 (0x11), BusTypeScm = 18 (0x12)
            bus_kind = DriveBusKind::Nvme;
        } else if bus_type == 11 || bus_type == 3 || bus_type == 2 || bus_type == 1 || bus_type == 10 || bus_type == 8 {
            // BusTypeSata = 11, BusTypeAta = 3, BusTypeAtapi = 2, BusTypeScsi = 1, BusTypeSas = 10, BusTypeRAID = 8
            bus_kind = DriveBusKind::Sata;
        }
    }

    // Query StorageDeviceSeekPenaltyProperty (PropertyId = 7) to distinguish SSD vs. spinning HDD
    let mut seek_query: [u8; 12] = [0u8; 12];
    seek_query[0..4].copy_from_slice(&7u32.to_le_bytes());
    let mut seek_out: [u8; 16] = [0u8; 16];
    let mut seek_returned: u32 = 0;

    let seek_ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            seek_query.as_ptr() as *const _,
            seek_query.len() as u32,
            seek_out.as_mut_ptr() as *mut _,
            seek_out.len() as u32,
            &mut seek_returned,
            std::ptr::null_mut(),
        )
    };

    unsafe {
        CloseHandle(handle);
    }

    let incurs_seek_penalty = if bus_kind == DriveBusKind::Nvme {
        false
    } else if seek_ok != 0 && seek_returned >= 9 {
        seek_out[8] != 0
    } else {
        true
    };

    (bus_kind, incurs_seek_penalty)
}

/// Computes optimal concurrent extraction writer threads for a destination drive profile.
/// Caps threads to avoid NTFS `$MFT` `ERESOURCE` lock convoys on SSDs/NVMe and keeps
/// spinning HDDs / USB flash drives strictly sequential (`1` writer thread).
pub fn calculate_extract_writer_threads(
    bus: DriveBusKind,
    incurs_seek_penalty: bool,
    total_cores: usize,
) -> usize {
    match bus {
        DriveBusKind::Nvme => {
            if total_cores <= 1 {
                1
            } else {
                (total_cores / 2).clamp(2, 6)
            }
        }
        DriveBusKind::Sata if !incurs_seek_penalty => {
            if total_cores <= 1 {
                1
            } else {
                (total_cores / 2).clamp(2, 4)
            }
        }
        DriveBusKind::Sata | DriveBusKind::UsbRemovable | DriveBusKind::Unknown => 1,
    }
}

/// Computes the bounded in-flight memory budget for async extraction file writes.
pub fn calculate_extract_inflight_budget(bus: DriveBusKind, incurs_seek_penalty: bool) -> usize {
    match bus {
        DriveBusKind::Nvme => 48 * 1024 * 1024,
        DriveBusKind::Sata if !incurs_seek_penalty => 32 * 1024 * 1024,
        DriveBusKind::Sata | DriveBusKind::UsbRemovable | DriveBusKind::Unknown => 16 * 1024 * 1024,
    }
}

/// Computes the Windows Cache Manager (256 KB VACB-aligned) chunk size for sequential extraction streaming.
pub fn calculate_extract_chunk_size(bus: DriveBusKind) -> usize {
    match bus {
        DriveBusKind::Nvme => 1024 * 1024,
        DriveBusKind::Sata => 512 * 1024,
        DriveBusKind::UsbRemovable | DriveBusKind::Unknown => 256 * 1024,
    }
}

pub fn get_optimal_extract_writer_threads(dest_dir: &Path) -> usize {
    let total_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    calculate_extract_writer_threads(
        get_drive_bus_kind(dest_dir),
        get_drive_seek_penalty(dest_dir),
        total_cores,
    )
}

pub fn get_optimal_extract_inflight_budget(dest_dir: &Path) -> usize {
    calculate_extract_inflight_budget(
        get_drive_bus_kind(dest_dir),
        get_drive_seek_penalty(dest_dir),
    )
}

pub fn get_optimal_extract_chunk_size(dest_dir: &Path) -> usize {
    calculate_extract_chunk_size(get_drive_bus_kind(dest_dir))
}

/// Query the optimal sequential read slice size for a given file path based on the underlying physical drive bus.
/// - External USB / Flash Thumb Drives / Ext HDDs: Fixed 256 KB (prevents USB controller saturation and high latency)
/// - Internal SATA (SSD / HDD): Fixed 512 KB (smooth OS throughput without disk head thrashing)
/// - Internal PCIe NVMe SSD: Fixed 1 MB (saturates multi-gigabyte PCIe burst line rate without cache pollution)
/// - Fallback / Unknown / Network share: Fixed 256 KB
#[cfg(target_os = "windows")]
pub fn get_optimal_read_slice(path: &Path) -> usize {
    match get_drive_bus_kind(path) {
        DriveBusKind::UsbRemovable => 256 * 1024,      // 256 KB: External USB / Flash Thumb Drives
        DriveBusKind::Sata => 512 * 1024,              // 512 KB: Internal SATA (SSD / HDD)
        DriveBusKind::Nvme => 1024 * 1024,             // 1 MB: Internal PCIe NVMe SSD
        DriveBusKind::Unknown => 256 * 1024,          // 256 KB: Network / UNC / Fallback
    }
}

/// Query the optimal sequential write buffer size for a target output archive based on the destination drive bus.
/// Prevents slow USB thumb drives from locking in multi-second synchronous write stalls under Windows Quick-Removal policy,
/// and eliminates the 221ms queue depth latency spikes on NVMe drives caused by massive 128 MB monolithic flushes.
/// - External USB / Flash Thumb Drives: Capped at 512 KB max (instant cancellation response & smooth progress updates)
/// - Internal SATA SSD / HDD: Capped at 1 MB max (smooth sequential flushing without head thrashing)
/// - Internal PCIe NVMe SSD: Capped at 2 MB max (optimal line rate without dirty-page stalls or latency spikes)
/// - Fallback / Unknown / Network share: Capped at 512 KB max
#[cfg(target_os = "windows")]
pub fn get_optimal_write_buffer(output_path: &Path, requested_buffer: usize) -> usize {
    let cap = match get_drive_bus_kind(output_path) {
        DriveBusKind::UsbRemovable => 512 * 1024,       // 512 KB max
        DriveBusKind::Sata => 1024 * 1024,              // 1 MB max
        DriveBusKind::Nvme => 2 * 1024 * 1024,          // 2 MB max
        DriveBusKind::Unknown => 512 * 1024,            // 512 KB max
    };
    requested_buffer.min(cap).max(64 * 1024)
}

#[cfg(not(target_os = "windows"))]
pub fn get_drive_bus_kind(_path: &Path) -> DriveBusKind {
    DriveBusKind::Unknown
}

#[cfg(not(target_os = "windows"))]
pub fn get_optimal_read_slice(_path: &Path) -> usize {
    512 * 1024
}

#[cfg(not(target_os = "windows"))]
pub fn get_drive_seek_penalty(_path: &Path) -> bool {
    true
}

#[cfg(not(target_os = "windows"))]
pub fn get_optimal_write_buffer(_output_path: &Path, requested_buffer: usize) -> usize {
    requested_buffer.min(2 * 1024 * 1024).max(64 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_extract_writer_threads_by_drive_profile() {
        // NVMe scales between 2 and 6 writers (1 on single-core)
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Nvme, false, 1), 1);
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Nvme, false, 2), 2);
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Nvme, false, 8), 4);
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Nvme, false, 16), 6);

        // SATA SSD (no seek penalty) scales between 2 and 4 writers
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Sata, false, 1), 1);
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Sata, false, 4), 2);
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Sata, false, 16), 4);

        // Mechanical SATA HDD (incurs seek penalty), USB, and Unknown serialize to 1 writer
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Sata, true, 16), 1);
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::UsbRemovable, true, 16), 1);
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::UsbRemovable, false, 16), 1);
        assert_eq!(calculate_extract_writer_threads(DriveBusKind::Unknown, true, 16), 1);
    }

    #[test]
    fn test_calculate_extract_inflight_budget_and_chunk_size() {
        assert_eq!(calculate_extract_inflight_budget(DriveBusKind::Nvme, false), 48 * 1024 * 1024);
        assert_eq!(calculate_extract_inflight_budget(DriveBusKind::Sata, false), 32 * 1024 * 1024);
        assert_eq!(calculate_extract_inflight_budget(DriveBusKind::Sata, true), 16 * 1024 * 1024);
        assert_eq!(calculate_extract_inflight_budget(DriveBusKind::UsbRemovable, true), 16 * 1024 * 1024);
        assert_eq!(calculate_extract_inflight_budget(DriveBusKind::Unknown, false), 16 * 1024 * 1024);

        assert_eq!(calculate_extract_chunk_size(DriveBusKind::Nvme), 1024 * 1024);
        assert_eq!(calculate_extract_chunk_size(DriveBusKind::Sata), 512 * 1024);
        assert_eq!(calculate_extract_chunk_size(DriveBusKind::UsbRemovable), 256 * 1024);
        assert_eq!(calculate_extract_chunk_size(DriveBusKind::Unknown), 256 * 1024);
    }
}

use std::path::Path;

/// Hardware bus classification of a storage volume
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveBusKind {
    UsbRemovable, // External USB / Flash Thumb Drives / SD Cards / Removable Media
    Sata,         // Internal SATA SSD / HDD / SCSI / SAS / RAID
    Nvme,         // Internal PCIe NVMe SSD / SCM
    Unknown,      // Network / UNC / Virtual / Unknown
}

/// Query the physical drive bus kind for a given path.
/// Automatically detects and caches drive letters A through Z via Win32 IOCTL_STORAGE_QUERY_PROPERTY.
#[cfg(target_os = "windows")]
pub fn get_drive_bus_kind(path: &Path) -> DriveBusKind {
    use std::sync::atomic::{AtomicU8, Ordering};

    // Fast static cache for drive letters A through Z (26 slots)
    // 0 = unqueried, 1 = UsbRemovable, 2 = Sata, 3 = Nvme, 4 = Unknown
    static DRIVE_BUS_CACHE: [AtomicU8; 26] = [
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
        AtomicU8::new(0), AtomicU8::new(0),
    ];

    let path_str = path.to_string_lossy();
    let mut drive_letter_opt = None;
    let chars: Vec<char> = path_str.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == ':' && i > 0 && chars[i - 1].is_ascii_alphabetic() {
            drive_letter_opt = Some(chars[i - 1].to_ascii_uppercase());
            break;
        }
    }

    let drive_letter = match drive_letter_opt {
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
                let canon_str = canon_path.to_string_lossy();
                let c_chars: Vec<char> = canon_str.chars().collect();
                let mut found = None;
                for i in 0..c_chars.len() {
                    if c_chars[i] == ':' && i > 0 && c_chars[i - 1].is_ascii_alphabetic() {
                        found = Some(c_chars[i - 1].to_ascii_uppercase());
                        break;
                    }
                }
                match found {
                    Some(dl) => dl,
                    None => return DriveBusKind::Unknown,
                }
            } else {
                return DriveBusKind::Unknown;
            }
        }
    };

    let cache_idx = (drive_letter as u8).saturating_sub(b'A') as usize;
    if cache_idx < 26 {
        let cached = DRIVE_BUS_CACHE[cache_idx].load(Ordering::Relaxed);
        match cached {
            1 => return DriveBusKind::UsbRemovable,
            2 => return DriveBusKind::Sata,
            3 => return DriveBusKind::Nvme,
            4 => return DriveBusKind::Unknown,
            _ => {}
        }
    }

    let bus_kind = detect_drive_bus(drive_letter);

    if cache_idx < 26 {
        let val = match bus_kind {
            DriveBusKind::UsbRemovable => 1,
            DriveBusKind::Sata => 2,
            DriveBusKind::Nvme => 3,
            DriveBusKind::Unknown => 4,
        };
        DRIVE_BUS_CACHE[cache_idx].store(val, Ordering::Relaxed);
    }

    bus_kind
}

#[cfg(target_os = "windows")]
fn detect_drive_bus(drive_letter: char) -> DriveBusKind {
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
        return DriveBusKind::UsbRemovable;
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
        return DriveBusKind::Unknown;
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

    unsafe {
        CloseHandle(handle);
    }

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
            return DriveBusKind::UsbRemovable;
        }

        // BusTypeNvme = 17 (0x11), BusTypeScm = 18 (0x12)
        if bus_type == 17 || bus_type == 18 {
            return DriveBusKind::Nvme;
        }

        // BusTypeSata = 11, BusTypeAta = 3, BusTypeAtapi = 2, BusTypeScsi = 1, BusTypeSas = 10, BusTypeRAID = 8
        if bus_type == 11 || bus_type == 3 || bus_type == 2 || bus_type == 1 || bus_type == 10 || bus_type == 8 {
            return DriveBusKind::Sata;
        }
    }

    DriveBusKind::Unknown
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
pub fn get_optimal_write_buffer(_output_path: &Path, requested_buffer: usize) -> usize {
    requested_buffer.min(2 * 1024 * 1024).max(64 * 1024)
}

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::LazyLock;

static ICON_CACHE: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let type_start = out.len();
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let crc = crc32_ieee(&out[type_start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

pub fn encode_rgba_to_png(width: u32, height: u32, rgba_pixels: &[u8]) -> Vec<u8> {
    use std::io::Write;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    let mut ihdr_data = Vec::new();
    ihdr_data.extend_from_slice(&width.to_be_bytes());
    ihdr_data.extend_from_slice(&height.to_be_bytes());
    ihdr_data.push(8); // 8-bit depth
    ihdr_data.push(6); // RGBA color type
    ihdr_data.push(0); // compression
    ihdr_data.push(0); // filter
    ihdr_data.push(0); // interlace
    write_png_chunk(&mut png, b"IHDR", &ihdr_data);

    let row_len = (width * 4) as usize;
    let mut raw_scanlines = Vec::with_capacity((1 + row_len) * height as usize);
    for y in 0..height as usize {
        raw_scanlines.push(0); // filter type 0: None
        let start = y * row_len;
        if start + row_len <= rgba_pixels.len() {
            raw_scanlines.extend_from_slice(&rgba_pixels[start..start + row_len]);
        }
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    let _ = encoder.write_all(&raw_scanlines);
    let compressed = encoder.finish().unwrap_or_default();
    write_png_chunk(&mut png, b"IDAT", &compressed);

    write_png_chunk(&mut png, b"IEND", &[]);
    png
}

#[cfg(windows)]
pub fn extract_windows_icon_base64(extension_or_name: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
    };
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW,
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HGDIOBJ,
    };
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;

    let clean = extension_or_name.trim().trim_start_matches('.');
    if clean.is_empty() {
        return None;
    }

    let fake_path = format!("dummy.{}", clean);
    let wide: Vec<u16> = fake_path.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut shfi = SHFILEINFOW::default();
        let flags = SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES;
        let res = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0x80), // FILE_ATTRIBUTE_NORMAL
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            flags,
        );

        if res == 0 || shfi.hIcon.is_invalid() {
            return None;
        }

        let hicon = shfi.hIcon;
        let mut icon_info = std::mem::zeroed();
        if GetIconInfo(hicon, &mut icon_info).is_err() {
            let _ = DestroyIcon(hicon);
            return None;
        }

        let hbm_color = icon_info.hbmColor;
        let hbm_mask = icon_info.hbmMask;

        if hbm_color.is_invalid() {
            if !hbm_mask.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(hbm_mask.0));
            }
            let _ = DestroyIcon(hicon);
            return None;
        }

        let mut bm: BITMAP = std::mem::zeroed();
        let get_obj_res = GetObjectW(
            HGDIOBJ(hbm_color.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut _),
        );

        if get_obj_res == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            let _ = DeleteObject(HGDIOBJ(hbm_color.0));
            if !hbm_mask.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(hbm_mask.0));
            }
            let _ = DestroyIcon(hicon);
            return None;
        }

        let width = bm.bmWidth;
        let height = bm.bmHeight;

        let hdc = CreateCompatibleDC(HDC::default());
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // negative for top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bgra_pixels = vec![0u8; (width * height * 4) as usize];
        let lines = GetDIBits(
            hdc,
            hbm_color,
            0,
            height as u32,
            Some(bgra_pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = DeleteDC(hdc);
        let _ = DeleteObject(HGDIOBJ(hbm_color.0));
        if !hbm_mask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(hbm_mask.0));
        }
        let _ = DestroyIcon(hicon);

        if lines == 0 {
            return None;
        }

        let mut rgba_pixels = vec![0u8; bgra_pixels.len()];
        let mut has_non_zero_alpha = false;
        for i in (0..bgra_pixels.len()).step_by(4) {
            let b = bgra_pixels[i];
            let g = bgra_pixels[i + 1];
            let r = bgra_pixels[i + 2];
            let a = bgra_pixels[i + 3];

            if a > 0 {
                has_non_zero_alpha = true;
            }

            rgba_pixels[i] = r;
            rgba_pixels[i + 1] = g;
            rgba_pixels[i + 2] = b;
            rgba_pixels[i + 3] = a;
        }

        if !has_non_zero_alpha {
            for i in (3..rgba_pixels.len()).step_by(4) {
                rgba_pixels[i] = 255;
            }
        }

        let png_bytes = encode_rgba_to_png(width as u32, height as u32, &rgba_pixels);
        let b64 = BASE64.encode(&png_bytes);
        Some(format!("data:image/png;base64,{}", b64))
    }
}

#[cfg(not(windows))]
pub fn extract_windows_icon_base64(_extension_or_name: &str) -> Option<String> {
    None
}

pub fn get_cached_icon(ext: &str) -> Option<String> {
    let normalized = ext.trim().trim_start_matches('.').to_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if let Ok(cache) = ICON_CACHE.lock() {
        if let Some(data) = cache.get(&normalized) {
            return Some(data.clone());
        }
    }

    let b64 = extract_windows_icon_base64(&normalized)?;
    if let Ok(mut cache) = ICON_CACHE.lock() {
        cache.insert(normalized, b64.clone());
    }
    Some(b64)
}

pub fn get_cached_icons_batch(exts: Vec<String>) -> HashMap<String, String> {
    let mut results = HashMap::new();
    for ext in exts {
        let normalized = ext.trim().trim_start_matches('.').to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if let Some(b64) = get_cached_icon(&normalized) {
            results.insert(normalized, b64);
        }
    }
    results
}

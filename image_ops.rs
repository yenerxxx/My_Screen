use crate::paths::{default_save_folder, log_error};
use crate::settings_store::AppSettings;
use chrono::Local;
use image::{ImageFormat, RgbaImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CF_DIB_FORMAT: u32 = 8;

/// Tüm monitörleri (sanal masaüstü) doğrudan Win32 GDI BitBlt ile yakalar.
pub fn capture_screen() -> Option<(RgbaImage, i32, i32)> {
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if vw <= 0 || vh <= 0 {
            return None;
        }

        let hdc_screen = GetDC(HWND::default());
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbm = CreateCompatibleBitmap(hdc_screen, vw, vh);
        let old_obj = SelectObject(hdc_mem, HGDIOBJ(hbm.0));

        let _ = BitBlt(
            hdc_mem,
            0,
            0,
            vw,
            vh,
            hdc_screen,
            vx,
            vy,
            SRCCOPY | CAPTUREBLT,
        );

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: vw,
                biHeight: -vh,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bgra = vec![0u8; (vw * vh * 4) as usize];
        GetDIBits(
            hdc_mem,
            hbm,
            0,
            vh as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old_obj);
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(HWND::default(), hdc_screen);

        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }

        RgbaImage::from_raw(vw as u32, vh as u32, bgra).map(|img| (img, vx, vy))
    }
}

/// Görüntüyü CF_DIB (8) formatında doğrudan Windows panosuna kopyalar.
pub fn copy_image_to_clipboard(img: &RgbaImage) {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
    let row_bytes = ((w * 3 + 3) & !3) as usize;
    let pixel_bytes = row_bytes * (h as usize);
    let total_size = header_size + pixel_bytes;

    unsafe {
        let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, total_size) else {
            return;
        };
        let ptr = GlobalLock(hmem) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(hmem);
            return;
        }

        let header = BITMAPINFOHEADER {
            biSize: header_size as u32,
            biWidth: w,
            biHeight: h,
            biPlanes: 1,
            biBitCount: 24,
            biCompression: BI_RGB.0,
            biSizeImage: pixel_bytes as u32,
            ..Default::default()
        };
        ptr::copy_nonoverlapping(&header as *const _ as *const u8, ptr, header_size);

        let dst = std::slice::from_raw_parts_mut(ptr.add(header_size), pixel_bytes);
        for y in 0..h {
            let src_y = (h - 1 - y) as u32;
            let row_offset = (y as usize) * row_bytes;
            for x in 0..w as u32 {
                let p = img.get_pixel(x, src_y);
                let idx = row_offset + (x as usize) * 3;
                dst[idx] = p[2];
                dst[idx + 1] = p[1];
                dst[idx + 2] = p[0];
            }
        }

        let _ = GlobalUnlock(hmem);
        if OpenClipboard(HWND::default()).is_ok() {
            let _ = EmptyClipboard();
            if SetClipboardData(CF_DIB_FORMAT, HANDLE(hmem.0)).is_err() {
                let _ = GlobalFree(hmem);
            }
            let _ = CloseClipboard();
        } else {
            let _ = GlobalFree(hmem);
        }
    }
}

fn unique_path(base_dir: &Path, filename: &str) -> PathBuf {
    let initial = base_dir.join(filename);
    if !initial.exists() {
        return initial;
    }
    let stem = initial
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("capture");
    let ext = initial
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("png");
    let mut n = 1;
    loop {
        let candidate = base_dir.join(format!("{stem}_{n}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

pub fn save_capture_file(
    img: &RgbaImage,
    settings: &AppSettings,
    force_dialog: bool,
) -> Option<PathBuf> {
    let save_dir = if settings.save_folder.is_empty() {
        default_save_folder()
    } else {
        PathBuf::from(&settings.save_folder)
    };
    let _ = fs::create_dir_all(&save_dir);

    let ts = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let is_jpg = settings.image_format.eq_ignore_ascii_case("JPG");
    let ext = if is_jpg { "jpg" } else { "png" };
    let default_name = format!("{}{}.{}", settings.file_prefix, ts, ext);
    let mut target = unique_path(&save_dir, &default_name);

    if settings.ask_every_time || force_dialog {
        let picked = rfd::FileDialog::new()
            .set_directory(&save_dir)
            .set_file_name(target.file_name()?.to_str()?)
            .add_filter("PNG Dosyası", &["png"])
            .add_filter("JPEG Dosyası", &["jpg", "jpeg"])
            .save_file()?;
        target = picked;
    }

    save_image_to_explicit_path(img, &target, settings.jpeg_quality).then_some(target)
}

pub fn save_image_to_explicit_path(img: &RgbaImage, target: &Path, jpeg_quality: u8) -> bool {
    let is_jpg = target
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("jpg") || s.eq_ignore_ascii_case("jpeg"))
        .unwrap_or(false);

    let res = if is_jpg {
        let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, jpeg_quality);
        enc.encode_image(&rgb)
            .map_err(|e| e.to_string())
            .and_then(|_| fs::write(target, out).map_err(|e| e.to_string()))
    } else {
        img.save_with_format(target, ImageFormat::Png)
            .map_err(|e| e.to_string())
    };

    if let Err(e) = res {
        log_error("Dosya kaydedilemedi", &e);
        false
    } else {
        true
    }
}

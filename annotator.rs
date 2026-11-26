use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
use image::{imageops, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolKind {
    Rect,
    Ellipse,
    Arrow,
    Pen,
    Blur,
    Text,
    Crop,
    Marker,
    Highlight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawItem {
    pub kind: ToolKind,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub color: [u8; 3],
    pub width: f32,
    pub points: Vec<[f32; 2]>,
    pub text: String,
    pub font_size: f32,
    pub marker_num: u32,
}

impl DrawItem {
    pub fn bbox(&self) -> (f32, f32, f32, f32) {
        match self.kind {
            ToolKind::Rect
            | ToolKind::Ellipse
            | ToolKind::Arrow
            | ToolKind::Highlight
            | ToolKind::Blur => (
                self.x1.min(self.x2),
                self.y1.min(self.y2),
                self.x1.max(self.x2),
                self.y1.max(self.y2),
            ),
            ToolKind::Marker => {
                let r = (self.width * 2.0 + 9.0).max(12.0);
                (self.x1 - r, self.y1 - r, self.x1 + r, self.y1 + r)
            }
            ToolKind::Pen => {
                if self.points.is_empty() {
                    (self.x1, self.y1, self.x2, self.y2)
                } else {
                    let mut min_x = f32::MAX;
                    let mut min_y = f32::MAX;
                    let mut max_x = f32::MIN;
                    let mut max_y = f32::MIN;
                    for p in &self.points {
                        min_x = min_x.min(p[0]);
                        min_y = min_y.min(p[1]);
                        max_x = max_x.max(p[0]);
                        max_y = max_y.max(p[1]);
                    }
                    (min_x, min_y, max_x, max_y)
                }
            }
            ToolKind::Text => {
                let w = (self.text.chars().count() as f32 * self.font_size * 0.58).max(40.0);
                let h = self.font_size * 1.35;
                (self.x1, self.y1, self.x1 + w, self.y1 + h)
            }
            ToolKind::Crop => (0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn contains(&self, px: f32, py: f32, tol: f32) -> bool {
        let (x1, y1, x2, y2) = self.bbox();
        px >= x1 - tol && px <= x2 + tol && py >= y1 - tol && py <= y2 + tol
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.x1 += dx;
        self.y1 += dy;
        self.x2 += dx;
        self.y2 += dy;
        for p in &mut self.points {
            p[0] += dx;
            p[1] += dy;
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SidecarData {
    bg_file: String,
    items: Vec<DrawItem>,
}

pub fn sidecar_path(img_path: &Path) -> String {
    format!("{}.myscreen.json", img_path.display())
}

pub fn save_layer_sidecar(img_path: &Path, bg_img: &RgbaImage, items: &[DrawItem]) {
    let meta_path = sidecar_path(img_path);
    let bg_path = format!("{}.bg.png", img_path.display());
    if bg_img.save(&bg_path).is_ok() {
        let data = SidecarData {
            bg_file: Path::new(&bg_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
            items: items.to_vec(),
        };
        if let Ok(json) = serde_json::to_string(&data) {
            let _ = fs::write(meta_path, json);
        }
    }
}

pub fn load_layer_sidecar(img_path: &Path) -> Option<(RgbaImage, Vec<DrawItem>)> {
    let meta_path = sidecar_path(img_path);
    let raw = fs::read_to_string(meta_path).ok()?;
    let data: SidecarData = serde_json::from_str(&raw).ok()?;
    let bg_full = img_path.parent()?.join(data.bg_file);
    let bg_img = image::open(bg_full).ok()?.to_rgba8();
    Some((bg_img, data.items))
}

fn load_system_font() -> Option<FontVec> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    for name in ["segoeuib.ttf", "segoeui.ttf", "arialbd.ttf", "arial.ttf"] {
        let path = Path::new(&windir).join("Fonts").join(name);
        if let Ok(bytes) = fs::read(path) {
            if let Ok(font) = FontVec::try_from_vec(bytes) {
                return Some(font);
            }
        }
    }
    None
}

fn draw_text_onto(
    img: &mut RgbaImage,
    font: &FontVec,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    col: [u8; 3],
    centered: bool,
) {
    let scale = PxScale::from(size.max(8.0));
    let scaled = font.as_scaled(scale);
    let total_w: f32 = text
        .chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)))
        .sum();
    let ascent = scaled.ascent();
    let start_x = if centered { x - total_w * 0.5 } else { x };
    let baseline_y = if centered {
        y + ascent * 0.38
    } else {
        y + ascent
    };

    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let mut cursor_x = start_x;
    for ch in text.chars() {
        let gid = scaled.glyph_id(ch);
        let glyph = gid.with_scale_and_position(scale, point(cursor_x, baseline_y));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0 && px < iw && py >= 0 && py < ih && cov > 0.01 {
                    let a = cov.clamp(0.0, 1.0);
                    let dst = img.get_pixel_mut(px as u32, py as u32);
                    dst[0] = (dst[0] as f32 * (1.0 - a) + col[0] as f32 * a) as u8;
                    dst[1] = (dst[1] as f32 * (1.0 - a) + col[1] as f32 * a) as u8;
                    dst[2] = (dst[2] as f32 * (1.0 - a) + col[2] as f32 * a) as u8;
                }
            });
        }
        cursor_x += scaled.h_advance(gid);
    }
}

fn draw_line_thick(
    img: &mut RgbaImage,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    width: f32,
    col: Rgba<u8>,
) {
    let dist = (x1 - x0).hypot(y1 - y0);
    let steps = (dist.ceil() as i32).max(1);
    let r = (width / 2.0).max(0.5);
    let (w, h) = (img.width() as i32, img.height() as i32);

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let cx = x0 + (x1 - x0) * t;
        let cy = y0 + (y1 - y0) * t;
        let min_x = ((cx - r).floor() as i32).max(0);
        let max_x = ((cx + r).ceil() as i32).min(w - 1);
        let min_y = ((cy - r).floor() as i32).max(0);
        let max_y = ((cy + r).ceil() as i32).min(h - 1);

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                if (px as f32 - cx).hypot(py as f32 - cy) <= r {
                    img.put_pixel(px as u32, py as u32, col);
                }
            }
        }
    }
}

/// Tüm katmanları (metin ve numaralı işaretler dahil) nihai görüntü üzerine işler.
pub fn compose_image(bg: &RgbaImage, items: &[DrawItem]) -> RgbaImage {
    let mut out = bg.clone();
    let (w, h) = out.dimensions();
    let font = load_system_font();

    for it in items {
        let col = Rgba([it.color[0], it.color[1], it.color[2], 255]);
        let rx1 = it.x1.min(it.x2).max(0.0) as u32;
        let ry1 = it.y1.min(it.y2).max(0.0) as u32;
        let rx2 = it.x1.max(it.x2).min(w as f32) as u32;
        let ry2 = it.y1.max(it.y2).min(h as f32) as u32;

        match it.kind {
            ToolKind::Blur if rx2 > rx1 && ry2 > ry1 => {
                let sub = imageops::crop_imm(&out, rx1, ry1, rx2 - rx1, ry2 - ry1).to_image();
                let blurred = imageops::blur(&sub, 9.0);
                imageops::overlay(&mut out, &blurred, rx1 as i64, ry1 as i64);
            }
            ToolKind::Highlight if rx2 > rx1 && ry2 > ry1 => {
                for y in ry1..ry2 {
                    for x in rx1..rx2 {
                        let p = out.get_pixel_mut(x, y);
                        p[0] = ((p[0] as f32 * 0.62) + (it.color[0] as f32 * 0.38)) as u8;
                        p[1] = ((p[1] as f32 * 0.62) + (it.color[1] as f32 * 0.38)) as u8;
                        p[2] = ((p[2] as f32 * 0.62) + (it.color[2] as f32 * 0.38)) as u8;
                    }
                }
            }
            ToolKind::Rect if rx2 > rx1 && ry2 > ry1 => {
                let fx1 = rx1 as f32;
                let fy1 = ry1 as f32;
                let fx2 = rx2 as f32;
                let fy2 = ry2 as f32;
                draw_line_thick(&mut out, fx1, fy1, fx2, fy1, it.width, col);
                draw_line_thick(&mut out, fx2, fy1, fx2, fy2, it.width, col);
                draw_line_thick(&mut out, fx2, fy2, fx1, fy2, it.width, col);
                draw_line_thick(&mut out, fx1, fy2, fx1, fy1, it.width, col);
            }
            ToolKind::Ellipse if rx2 > rx1 && ry2 > ry1 => {
                let cx = (rx1 + rx2) as f32 / 2.0;
                let cy = (ry1 + ry2) as f32 / 2.0;
                let rx = (rx2 - rx1) as f32 / 2.0;
                let ry = (ry2 - ry1) as f32 / 2.0;
                let steps = 180;
                for i in 0..steps {
                    let a0 = (i as f32 / steps as f32) * std::f32::consts::TAU;
                    let a1 = ((i + 1) as f32 / steps as f32) * std::f32::consts::TAU;
                    draw_line_thick(
                        &mut out,
                        cx + rx * a0.cos(),
                        cy + ry * a0.sin(),
                        cx + rx * a1.cos(),
                        cy + ry * a1.sin(),
                        it.width,
                        col,
                    );
                }
            }
            ToolKind::Arrow => {
                draw_line_thick(&mut out, it.x1, it.y1, it.x2, it.y2, it.width, col);
                let ang = (it.y2 - it.y1).atan2(it.x2 - it.x1);
                let sz = (it.width * 4.0).max(14.0);
                let a1x = it.x2 - sz * (ang - 0.42).cos();
                let a1y = it.y2 - sz * (ang - 0.42).sin();
                let a2x = it.x2 - sz * (ang + 0.42).cos();
                let a2y = it.y2 - sz * (ang + 0.42).sin();
                draw_line_thick(&mut out, it.x2, it.y2, a1x, a1y, it.width, col);
                draw_line_thick(&mut out, it.x2, it.y2, a2x, a2y, it.width, col);
            }
            ToolKind::Pen => {
                for pair in it.points.windows(2) {
                    draw_line_thick(
                        &mut out,
                        pair[0][0],
                        pair[0][1],
                        pair[1][0],
                        pair[1][1],
                        it.width,
                        col,
                    );
                }
            }
            ToolKind::Marker => {
                let r = (it.width * 2.0 + 9.0).max(12.0);
                let min_x = ((it.x1 - r - 2.0).floor() as i32).max(0);
                let max_x = ((it.x1 + r + 2.0).ceil() as i32).min(w as i32 - 1);
                let min_y = ((it.y1 - r - 2.0).floor() as i32).max(0);
                let max_y = ((it.y1 + r + 2.0).ceil() as i32).min(h as i32 - 1);
                for py in min_y..=max_y {
                    for px in min_x..=max_x {
                        let d = (px as f32 - it.x1).hypot(py as f32 - it.y1);
                        if d <= r {
                            out.put_pixel(px as u32, py as u32, col);
                        } else if d <= r + 2.0 {
                            out.put_pixel(px as u32, py as u32, Rgba([255, 255, 255, 255]));
                        }
                    }
                }
                if let Some(f) = &font {
                    let lum = 0.299 * it.color[0] as f32
                        + 0.587 * it.color[1] as f32
                        + 0.114 * it.color[2] as f32;
                    let text_col = if lum > 186.0 { [0, 0, 0] } else { [255, 255, 255] };
                    draw_text_onto(
                        &mut out,
                        f,
                        &it.marker_num.to_string(),
                        it.x1,
                        it.y1,
                        r * 1.25,
                        text_col,
                        true,
                    );
                }
            }
            ToolKind::Text => {
                if let Some(f) = &font {
                    draw_text_onto(
                        &mut out,
                        f,
                        &it.text,
                        it.x1,
                        it.y1,
                        it.font_size,
                        it.color,
                        false,
                    );
                }
            }
            _ => {}
        }
    }
    out
}

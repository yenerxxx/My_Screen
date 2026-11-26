use std::fs;
use std::path::Path;
use tiny_skia::*;

fn generate_icon_rgba(size: u32) -> Vec<u8> {
    let mut pix = Pixmap::new(size, size).unwrap();
    let s = size as f32 / 64.0;
    let grad = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(0.0, size as f32),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(96, 165, 250, 255)),
            GradientStop::new(1.0, Color::from_rgba8(37, 99, 235, 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    let paint = Paint {
        shader: grad,
        anti_alias: true,
        ..Default::default()
    };
    let m = 1.5 * s;
    let rect = Rect::from_xywh(m, m, size as f32 - m * 2.0, size as f32 - m * 2.0).unwrap();
    pix.fill_rect(rect, &paint, Transform::identity(), None);

    let mut sp = Paint::default();
    sp.set_color(Color::WHITE);
    sp.anti_alias = true;
    let stroke = Stroke {
        width: 2.8 * s,
        line_cap: LineCap::Round,
        ..Default::default()
    };
    let c_len = 9.5 * s;
    let (x1, y1, x2, y2) = (
        16.0 * s,
        16.0 * s,
        size as f32 - 16.0 * s,
        size as f32 - 16.0 * s,
    );
    let mut pb = PathBuilder::new();
    for (ax, ay, bx, by) in [
        (x1, y1, x1 + c_len, y1),
        (x1, y1, x1, y1 + c_len),
        (x2, y1, x2 - c_len, y1),
        (x2, y1, x2, y1 + c_len),
        (x1, y2, x1 + c_len, y2),
        (x1, y2, x1, y2 - c_len),
        (x2, y2, x2 - c_len, y2),
        (x2, y2, x2, y2 - c_len),
    ] {
        pb.move_to(ax, ay);
        pb.line_to(bx, by);
    }
    if let Some(p) = pb.finish() {
        pix.stroke_path(&p, &sp, &stroke, Transform::identity(), None);
    }
    if let Some(c) = PathBuilder::from_circle(size as f32 / 2.0, size as f32 / 2.0, 4.5 * s) {
        pix.fill_path(&c, &sp, FillRule::Winding, Transform::identity(), None);
    }
    pix.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ico_path = Path::new(&out_dir).join("logo.ico");
    let rgba = generate_icon_rgba(256);
    if let Some(img) = image::RgbaImage::from_raw(256, 256, rgba) {
        let _ = img.save_with_format(&ico_path, image::ImageFormat::Ico);
    }
    let _ = fs::create_dir_all("resources");
    if ico_path.exists() {
        let _ = fs::copy(&ico_path, "resources/logo.ico");
    }

    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        if ico_path.exists() {
            res.set_icon(ico_path.to_str().unwrap());
        }
        let _ = res.compile();
    }
}

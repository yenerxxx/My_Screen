use tiny_skia::*;

pub fn generate_app_rgba(size: u32) -> Vec<u8> {
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
    let margin = 1.5 * s;
    let rect = Rect::from_xywh(
        margin,
        margin,
        size as f32 - margin * 2.0,
        size as f32 - margin * 2.0,
    )
    .unwrap();
    pix.fill_rect(rect, &paint, Transform::identity(), None);

    let mut stroke_paint = Paint::default();
    stroke_paint.set_color(Color::WHITE);
    stroke_paint.anti_alias = true;
    let stroke = Stroke {
        width: 2.8 * s,
        line_cap: LineCap::Round,
        ..Default::default()
    };

    let c_len = 9.5 * s;
    let x1 = 16.0 * s;
    let y1 = 16.0 * s;
    let x2 = size as f32 - 16.0 * s;
    let y2 = size as f32 - 16.0 * s;

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
    if let Some(path) = pb.finish() {
        pix.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    if let Some(circle) = PathBuilder::from_circle(size as f32 / 2.0, size as f32 / 2.0, 4.5 * s)
    {
        pix.fill_path(
            &circle,
            &stroke_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    pix.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

pub fn generate_caps_rgba(size: u32, is_on: bool) -> Vec<u8> {
    let mut pix = Pixmap::new(size, size).unwrap();
    let s = size as f32 / 64.0;

    let mut bg_paint = Paint::default();
    bg_paint.anti_alias = true;
    bg_paint.set_color(if is_on {
        Color::from_rgba8(0, 230, 118, 255)
    } else {
        Color::from_rgba8(225, 29, 72, 255)
    });

    let m = 1.5 * s;
    let rect = Rect::from_xywh(m, m, size as f32 - m * 2.0, size as f32 - m * 2.0).unwrap();
    pix.fill_rect(rect, &bg_paint, Transform::identity(), None);

    let mut border_paint = Paint::default();
    border_paint.set_color(Color::WHITE);
    border_paint.anti_alias = true;
    let border_stroke = Stroke {
        width: (2.5 * s).max(1.5),
        ..Default::default()
    };
    let mut bpb = PathBuilder::new();
    bpb.push_rect(rect);
    if let Some(bp) = bpb.finish() {
        pix.stroke_path(&bp, &border_paint, &border_stroke, Transform::identity(), None);
    }

    let mut glyph_paint = Paint::default();
    glyph_paint.anti_alias = true;
    glyph_paint.set_color(if is_on { Color::BLACK } else { Color::WHITE });
    let glyph_stroke = Stroke {
        width: (6.0 * s).max(3.0),
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };

    let mut gpb = PathBuilder::new();
    if is_on {
        gpb.move_to(20.0 * s, 48.0 * s);
        gpb.line_to(32.0 * s, 16.0 * s);
        gpb.line_to(44.0 * s, 48.0 * s);
        gpb.move_to(24.0 * s, 38.0 * s);
        gpb.line_to(40.0 * s, 38.0 * s);
    } else {
        if let Some(c) = PathBuilder::from_circle(31.0 * s, 37.0 * s, 10.0 * s) {
            pix.stroke_path(&c, &glyph_paint, &glyph_stroke, Transform::identity(), None);
        }
        gpb.move_to(41.0 * s, 26.0 * s);
        gpb.line_to(41.0 * s, 47.0 * s);
    }
    if let Some(gp) = gpb.finish() {
        pix.stroke_path(&gp, &glyph_paint, &glyph_stroke, Transform::identity(), None);
    }

    pix.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

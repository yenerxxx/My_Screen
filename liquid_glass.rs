use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Vec2};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GlassMaterial {
    Clear,
    Tinted,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GlassSize {
    Sm, // 32px
    Md, // 40px
    Lg, // 48px
    Xl, // 64px
}

impl GlassSize {
    pub fn height(self) -> f32 {
        match self {
            GlassSize::Sm => 32.0,
            GlassSize::Md => 40.0,
            GlassSize::Lg => 48.0,
            GlassSize::Xl => 64.0,
        }
    }

    pub fn font_size(self) -> f32 {
        match self {
            GlassSize::Sm => 12.0,
            GlassSize::Md => 13.5,
            GlassSize::Lg => 15.0,
            GlassSize::Xl => 17.5,
        }
    }

    pub fn padding_x(self) -> f32 {
        match self {
            GlassSize::Sm => 12.0,
            GlassSize::Md => 16.0,
            GlassSize::Lg => 20.0,
            GlassSize::Xl => 26.0,
        }
    }
}

#[derive(Clone)]
struct GlassPhysicsState {
    offset: Vec2,
    velocity: Vec2,
    glow: f32,
    cohesion_light: f32,
    is_pressed: bool,
    last_frame: Instant,
}

impl Default for GlassPhysicsState {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            velocity: Vec2::ZERO,
            glow: 0.0,
            cohesion_light: 0.0,
            is_pressed: false,
            last_frame: Instant::now(),
        }
    }
}

/// Liquid Glass UI fizik ve kohezyon motoru.
/// - `pull`: Basılı tutup sürüklerken lastik/jöle esnemesi ve bırakınca yaylanma.
/// - `cohesion`: Birbirine yaklaşan cam butonların yalnızca aradaki boşlukta sıvı köprü kurması.
#[derive(Default)]
pub struct LiquidGlassEngine {
    states: HashMap<egui::Id, GlassPhysicsState>,
    frame_buttons: Vec<(egui::Id, Rect, Color32, f32)>,
}

impl LiquidGlassEngine {
    pub fn begin_frame(&mut self) {
        self.frame_buttons.clear();
    }

    /// Yalnızca komşu cam butonların birbirine bakan kenarları arasındaki boşlukta sıvı köprü çizer.
    pub fn end_frame(&mut self, painter: &egui::Painter) {
        let count = self.frame_buttons.len();
        if count < 2 {
            return;
        }

        let snapshot = self.frame_buttons.clone();
        for i in 0..count {
            for j in (i + 1)..count {
                let (id_a, rect_a, col_a, glow_a) = snapshot[i];
                let (id_b, rect_b, col_b, glow_b) = snapshot[j];

                let dist_x = (rect_a.center().x - rect_b.center().x).abs()
                    - (rect_a.width() + rect_b.width()) * 0.5;
                let dist_y = (rect_a.center().y - rect_b.center().y).abs()
                    - (rect_a.height() + rect_b.height()) * 0.5;
                let gap = dist_x.max(dist_y);

                let threshold = 24.0;
                if gap < threshold && gap > -6.0 {
                    let t = (1.0 - (gap.max(0.0) / threshold)).clamp(0.0, 1.0);
                    let bridge_alpha = (t * t * 150.0) as u8;

                    if let Some(st_b) = self.states.get_mut(&id_b) {
                        st_b.cohesion_light = st_b.cohesion_light.max(glow_a * t * 0.45);
                    }
                    if let Some(st_a) = self.states.get_mut(&id_a) {
                        st_a.cohesion_light = st_a.cohesion_light.max(glow_b * t * 0.45);
                    }

                    // Metnin üstünü kapatmamak için yalnızca bakan kenar noktalarını birleştir
                    let (edge_a, edge_b) = if dist_x >= dist_y {
                        let cy = (rect_a.center().y + rect_b.center().y) * 0.5;
                        if rect_a.center().x < rect_b.center().x {
                            (Pos2::new(rect_a.max.x, cy), Pos2::new(rect_b.min.x, cy))
                        } else {
                            (Pos2::new(rect_a.min.x, cy), Pos2::new(rect_b.max.x, cy))
                        }
                    } else {
                        let cx = (rect_a.center().x + rect_b.center().x) * 0.5;
                        if rect_a.center().y < rect_b.center().y {
                            (Pos2::new(cx, rect_a.max.y), Pos2::new(cx, rect_b.min.y))
                        } else {
                            (Pos2::new(cx, rect_a.min.y), Pos2::new(cx, rect_b.max.y))
                        }
                    };

                    let neck_thickness =
                        (rect_a.height().min(rect_b.height()) * 0.36 * t).max(3.0);
                    let mix_r = ((col_a.r() as u16 + col_b.r() as u16) / 2) as u8;
                    let mix_g = ((col_a.g() as u16 + col_b.g() as u16) / 2) as u8;
                    let mix_b = ((col_a.b() as u16 + col_b.b() as u16) / 2) as u8;

                    let bridge_col =
                        Color32::from_rgba_unmultiplied(mix_r, mix_g, mix_b, bridge_alpha);
                    painter.line_segment([edge_a, edge_b], Stroke::new(neck_thickness, bridge_col));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &mut self,
        ui: &mut egui::Ui,
        id_salt: &str,
        label: &str,
        icon: Option<&str>,
        material: GlassMaterial,
        size: GlassSize,
        tint_color: Color32,
        tint_strength: f32,
        pull: bool,
        min_width: Option<f32>,
    ) -> Response {
        let id = ui.make_persistent_id(id_salt);
        let h = size.height();
        let font_id = egui::FontId::proportional(size.font_size());

        let display_text = match icon {
            Some(ic) if !label.is_empty() => format!("{ic}  {label}"),
            Some(ic) => ic.to_string(),
            None => label.to_string(),
        };

        let galley = ui
            .painter()
            .layout_no_wrap(display_text, font_id, Color32::WHITE);
        let desired_w = (galley.size().x + size.padding_x() * 2.0).max(min_width.unwrap_or(h));
        let (base_rect, response) =
            ui.allocate_exact_size(Vec2::new(desired_w, h), Sense::click_and_drag());

        let state = self.states.entry(id).or_default();
        let dt = state.last_frame.elapsed().as_secs_f32().clamp(0.001, 0.05);
        state.last_frame = Instant::now();

        if pull && response.dragged() {
            state.is_pressed = true;
            let delta = response.drag_delta();
            let resistance = if desired_w <= 72.0 { 0.85 } else { 0.55 };
            state.offset += delta * resistance;
            let max_pull = 22.0;
            if state.offset.length() > max_pull {
                state.offset = state.offset.normalized() * max_pull;
            }
            state.velocity = Vec2::ZERO;
        } else {
            state.is_pressed = response.is_pointer_button_down_on();
            let stiffness = 260.0;
            let damping = 18.0;
            let accel = -stiffness * state.offset - damping * state.velocity;
            state.velocity += accel * dt;
            state.offset += state.velocity * dt;
            if state.offset.length() < 0.05 && state.velocity.length() < 0.5 {
                state.offset = Vec2::ZERO;
                state.velocity = Vec2::ZERO;
            }
        }

        let target_glow = if state.is_pressed {
            1.0
        } else if response.hovered() {
            0.45
        } else {
            0.0
        };
        state.glow += (target_glow - state.glow) * (14.0 * dt).min(1.0);
        state.cohesion_light *= (1.0 - 8.0 * dt).max(0.0);

        let pull_mag = state.offset.length();
        let stretch_x = (state.offset.x.abs() * 0.25).min(7.0);
        let stretch_y = (state.offset.y.abs() * 0.25).min(7.0);
        let press_scale = if state.is_pressed {
            -1.0
        } else if response.hovered() {
            0.8
        } else {
            0.0
        };

        let dynamic_rect = Rect::from_center_size(
            base_rect.center() + state.offset,
            Vec2::new(
                (base_rect.width() + stretch_x - stretch_y * 0.3 + press_scale * 2.0).max(24.0),
                (base_rect.height() + stretch_y - stretch_x * 0.3 + press_scale * 2.0).max(24.0),
            ),
        );

        let rounding = (dynamic_rect.height() * 0.46).min(16.0);
        let painter = ui.painter();

        // 1. Yumuşak gölge
        let shadow_rect = dynamic_rect.translate(Vec2::new(0.0, 2.5)).expand(1.2);
        painter.rect_filled(
            shadow_rect,
            rounding + 1.2,
            Color32::from_rgba_unmultiplied(0, 0, 0, 70),
        );

        // 2. Cam gövde dolgusu
        let total_light = (state.glow + state.cohesion_light).clamp(0.0, 1.2);
        let body_color = match material {
            GlassMaterial::Clear => {
                let alpha = (28.0 + tint_strength * 42.0 + total_light * 45.0).min(180.0) as u8;
                Color32::from_rgba_unmultiplied(tint_color.r(), tint_color.g(), tint_color.b(), alpha)
            }
            GlassMaterial::Tinted => {
                let alpha = (175.0 + total_light * 65.0).min(248.0) as u8;
                Color32::from_rgba_unmultiplied(tint_color.r(), tint_color.g(), tint_color.b(), alpha)
            }
        };
        painter.rect_filled(dynamic_rect, rounding, body_color);

        // 3. Üst speküler cam kırılma parlaması
        let top_glare = Rect::from_min_max(
            Pos2::new(dynamic_rect.min.x + 3.0, dynamic_rect.min.y + 1.5),
            Pos2::new(
                dynamic_rect.max.x - 3.0,
                dynamic_rect.min.y + dynamic_rect.height() * 0.45,
            ),
        );
        painter.rect_filled(
            top_glare,
            (rounding - 2.0).max(2.0),
            Color32::from_rgba_unmultiplied(255, 255, 255, (24.0 + total_light * 40.0) as u8),
        );

        // 4. Alt iç yansıma
        let bottom_rim = Rect::from_min_max(
            Pos2::new(
                dynamic_rect.min.x + 6.0,
                dynamic_rect.max.y - dynamic_rect.height() * 0.22,
            ),
            Pos2::new(dynamic_rect.max.x - 6.0, dynamic_rect.max.y - 1.5),
        );
        painter.rect_filled(
            bottom_rim,
            (rounding * 0.5).max(2.0),
            Color32::from_rgba_unmultiplied(255, 255, 255, (10.0 + total_light * 26.0) as u8),
        );

        // 5. Fresnel kenar çizgisi
        let border_alpha = (110.0 + total_light * 125.0).min(255.0) as u8;
        painter.rect_stroke(
            dynamic_rect,
            rounding,
            Stroke::new(
                if pull_mag > 1.0 { 1.5 } else { 1.1 },
                Color32::from_rgba_unmultiplied(255, 255, 255, border_alpha),
            ),
        );

        // 6. Metin / İkon
        let text_pos = dynamic_rect.center() - galley.size() * 0.5;
        painter.galley(text_pos, galley, Color32::WHITE);

        self.frame_buttons
            .push((id, dynamic_rect, body_color, state.glow));

        if pull_mag > 0.05 || (target_glow - state.glow).abs() > 0.01 || state.cohesion_light > 0.01
        {
            ui.ctx().request_repaint();
        }

        response
    }
}

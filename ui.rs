use crate::annotator::{
    compose_image, load_layer_sidecar, save_layer_sidecar, sidecar_path, DrawItem, ToolKind,
};
use crate::caps_lock::is_caps_lock_on;
use crate::constants::*;
use crate::hotkeys::{spawn_hotkey_listener, HotkeyControl};
use crate::icon_helper::{generate_app_rgba, generate_caps_rgba};
use crate::image_ops::{
    capture_screen, copy_image_to_clipboard, save_capture_file, save_image_to_explicit_path,
};
use crate::liquid_glass::{GlassMaterial, GlassSize, LiquidGlassEngine};
use crate::overlay::{get_visible_window_rects, WinRect};
use crate::paths::{default_save_folder, open_folder};
use crate::settings_store::{load_settings, save_settings, set_windows_startup, AppSettings};
use eframe::egui;
use image::RgbaImage;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(PartialEq, Eq)]
enum ActiveView {
    Hidden,
    Launcher,
    Overlay,
    Preview,
    Settings,
}

struct HistoryEntry {
    path: PathBuf,
    name: String,
    has_layers: bool,
}

pub struct MyScreenRustApp {
    settings: AppSettings,
    rx: Receiver<String>,
    hotkey_ctrl: Sender<HotkeyControl>,
    _app_tray: TrayIcon,
    caps_tray: TrayIcon,
    last_caps: bool,
    last_caps_check: Instant,
    view: ActiveView,
    glass: LiquidGlassEngine,
    // Ekran yakalama ve akıllı pencere algılama
    raw_screen: Option<RgbaImage>,
    window_rects: Vec<WinRect>,
    hovered_win: Option<WinRect>,
    sel_start: Option<egui::Pos2>,
    sel_end: Option<egui::Pos2>,
    // Editör ve katman durumu
    bg_img: Option<RgbaImage>,
    items: Vec<DrawItem>,
    undo_stack: Vec<(RgbaImage, Vec<DrawItem>)>,
    selected_idx: Option<usize>,
    active_tool: ToolKind,
    active_color: [u8; 3],
    pen_width: f32,
    pending_text: String,
    pending_font_size: f32,
    text_popup_pos: Option<[f32; 2]>,
    active_draw_start: Option<[f32; 2]>,
    active_draw_curr: Option<[f32; 2]>,
    active_pen_pts: Vec<[f32; 2]>,
    drag_last_img_pt: Option<[f32; 2]>,
    // Paneller ve geçmiş
    tools_open: bool,
    history_open: bool,
    history_list: Vec<HistoryEntry>,
    current_path: Option<PathBuf>,
    preview_tex: Option<egui::TextureHandle>,
    toast_msg: Option<(String, String, egui::Color32, Instant)>,
    busy: bool,
}

impl MyScreenRustApp {
    fn new(tx: Sender<String>, rx: Receiver<String>) -> Self {
        let settings = load_settings();
        set_windows_startup(settings.start_with_windows);

        let hotkey_ctrl = spawn_hotkey_listener(
            settings.hotkey_free.clone(),
            settings.hotkey_full.clone(),
            tx,
        );

        let menu = Menu::new();
        let _ = menu.append(&MenuItem::with_id("free", "▣ Serbest Yakala", true, None));
        let _ = menu.append(&MenuItem::with_id("full", "▭ Tam Ekran Yakala", true, None));
        let _ = menu.append(&MenuItem::with_id(
            "folder",
            "📁 Kayıt Klasörünü Aç",
            true,
            None,
        ));
        let _ = menu.append(&MenuItem::with_id("settings", "⚙ Ayarlar", true, None));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id("exit", "✕ Programı Kapat", true, None));

        let app_icon = Icon::from_rgba(generate_app_rgba(64), 64, 64).unwrap();
        let app_tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(APP_NAME)
            .with_icon(app_icon)
            .build()
            .unwrap();

        let last_caps = is_caps_lock_on();
        let caps_icon = Icon::from_rgba(generate_caps_rgba(64, last_caps), 64, 64).unwrap();
        let caps_tray = TrayIconBuilder::new()
            .with_tooltip(if last_caps {
                "Caps Lock AÇIK\nSol Tık: Serbest | Çift Tık: Tam Ekran"
            } else {
                "Caps Lock KAPALI\nSol Tık: Serbest | Çift Tık: Tam Ekran"
            })
            .with_icon(caps_icon)
            .build()
            .unwrap();
        let _ = caps_tray.set_visible(settings.caps_notify);

        Self {
            settings,
            rx,
            hotkey_ctrl,
            _app_tray: app_tray,
            caps_tray,
            last_caps,
            last_caps_check: Instant::now(),
            view: ActiveView::Launcher,
            glass: LiquidGlassEngine::default(),
            raw_screen: None,
            window_rects: Vec::new(),
            hovered_win: None,
            sel_start: None,
            sel_end: None,
            bg_img: None,
            items: Vec::new(),
            undo_stack: Vec::new(),
            selected_idx: None,
            active_tool: ToolKind::Rect,
            active_color: [255, 59, 48],
            pen_width: 4.0,
            pending_text: String::new(),
            pending_font_size: 24.0,
            text_popup_pos: None,
            active_draw_start: None,
            active_draw_curr: None,
            active_pen_pts: Vec::new(),
            drag_last_img_pt: None,
            tools_open: true,
            history_open: false,
            history_list: Vec::new(),
            current_path: None,
            preview_tex: None,
            toast_msg: None,
            busy: false,
        }
    }

    fn show_toast(&mut self, title: impl Into<String>, sub: impl Into<String>, color: egui::Color32) {
        if self.settings.notify {
            self.toast_msg = Some((title.into(), sub.into(), color, Instant::now()));
        }
    }

    fn push_undo(&mut self) {
        if let Some(bg) = &self.bg_img {
            self.undo_stack.push((bg.clone(), self.items.clone()));
            if self.undo_stack.len() > 30 {
                self.undo_stack.remove(0);
            }
        }
    }

    fn undo(&mut self, ctx: &egui::Context) {
        if let Some((bg, items)) = self.undo_stack.pop() {
            let ci = egui::ColorImage::from_rgba_unmultiplied(
                [bg.width() as usize, bg.height() as usize],
                bg.as_raw(),
            );
            self.preview_tex = Some(ctx.load_texture("preview", ci, egui::TextureOptions::LINEAR));
            self.bg_img = Some(bg);
            self.items = items;
            self.selected_idx = None;
        }
    }

    fn start_free_capture(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.view = ActiveView::Hidden;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        std::thread::sleep(Duration::from_millis(140));

        if let Some((img, vx, vy)) = capture_screen() {
            let ppp = ctx.pixels_per_point().max(1.0);
            let vw = img.width() as f32 / ppp;
            let vh = img.height() as f32 / ppp;
            let color_img = egui::ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                img.as_raw(),
            );
            self.preview_tex =
                Some(ctx.load_texture("screen_cap", color_img, egui::TextureOptions::LINEAR));
            self.raw_screen = Some(img);
            self.window_rects = get_visible_window_rects();
            self.hovered_win = None;
            self.sel_start = None;
            self.sel_end = None;
            self.view = ActiveView::Overlay;

            // Çoklu monitör sanal masaüstünün tamamını kapla
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                vx as f32 / ppp,
                vy as f32 / ppp,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(vw, vh)));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        } else {
            self.busy = false;
        }
    }

    fn start_full_capture(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.view = ActiveView::Hidden;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        std::thread::sleep(Duration::from_millis(140));

        if let Some((img, _, _)) = capture_screen() {
            self.finish_capture(ctx, img);
        } else {
            self.busy = false;
        }
    }

    fn finish_capture(&mut self, ctx: &egui::Context, img: RgbaImage) {
        self.busy = false;
        match self.settings.after_capture.as_str() {
            "copy" => {
                copy_image_to_clipboard(&img);
                self.show_toast("Panoya kopyalandı", "Görüntü hazır", ACCENT2);
                self.view = ActiveView::Hidden;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                return;
            }
            "save" => {
                let saved = save_capture_file(&img, &self.settings, self.settings.ask_every_time);
                if saved.is_some() {
                    self.show_toast("Kaydedildi", "Ekran görüntüsü alındı", GREEN);
                } else {
                    self.show_toast("İşlem tamamlanmadı", "Kaydetme iptal edildi", RED);
                }
                self.view = ActiveView::Hidden;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                return;
            }
            _ => {}
        }

        let mut saved_path = None;
        if self.settings.auto_save {
            saved_path = save_capture_file(&img, &self.settings, self.settings.ask_every_time);
            if saved_path.is_none() {
                self.show_toast("İşlem tamamlanmadı", "Kaydetme iptal edildi", RED);
                self.view = ActiveView::Hidden;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                return;
            }
        }
        if self.settings.auto_copy {
            copy_image_to_clipboard(&img);
        }

        let color_img = egui::ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.as_raw(),
        );
        self.preview_tex =
            Some(ctx.load_texture("preview", color_img, egui::TextureOptions::LINEAR));
        self.bg_img = Some(img);
        self.items.clear();
        self.undo_stack.clear();
        self.selected_idx = None;
        self.current_path = saved_path;
        self.refresh_history();
        self.view = ActiveView::Preview;

        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(880.0, 580.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(120.0, 90.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);

        if self.settings.auto_save && self.current_path.is_some() {
            self.show_toast("Kaydedildi", "Ekran görüntüsü alındı", GREEN);
        } else if self.settings.auto_copy {
            self.show_toast("Panoya kopyalandı", "Görüntü hazır", ACCENT2);
        }
    }

    fn refresh_history(&mut self) {
        self.history_list.clear();
        let folder = if self.settings.save_folder.is_empty() {
            default_save_folder()
        } else {
            PathBuf::from(&self.settings.save_folder)
        };
        let Ok(entries) = fs::read_dir(folder) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if fname.ends_with(".bg.png") {
                continue;
            }
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "webp") {
                if let Ok(meta) = p.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        files.push((mtime, p));
                    }
                }
            }
        }
        files.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, p) in files.into_iter().take(25) {
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let has_layers = PathBuf::from(sidecar_path(&p)).exists();
            self.history_list.push(HistoryEntry {
                path: p,
                name,
                has_layers,
            });
        }
    }

    fn load_history_item(&mut self, ctx: &egui::Context, path: PathBuf) {
        if let Some((bg, items)) = load_layer_sidecar(&path) {
            let color_img = egui::ColorImage::from_rgba_unmultiplied(
                [bg.width() as usize, bg.height() as usize],
                bg.as_raw(),
            );
            self.preview_tex =
                Some(ctx.load_texture("preview", color_img, egui::TextureOptions::LINEAR));
            self.bg_img = Some(bg);
            self.items = items;
            self.undo_stack.clear();
            self.selected_idx = None;
            self.current_path = Some(path);
            self.show_toast("Görüntü yüklendi", "Katmanlı kayıt açıldı", GREEN);
        } else if let Ok(dyn_img) = image::open(&path) {
            let rgba = dyn_img.to_rgba8();
            let color_img = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            );
            self.preview_tex =
                Some(ctx.load_texture("preview", color_img, egui::TextureOptions::LINEAR));
            self.bg_img = Some(rgba);
            self.items.clear();
            self.undo_stack.clear();
            self.selected_idx = None;
            self.current_path = Some(path);
            self.show_toast("Görüntü yüklendi", "Düzenlemeye hazır", ACCENT2);
        }
    }
}

impl eframe::App for MyScreenRustApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.glass.begin_frame();

        // 1. Komutlar (TCP / CLI / Global Hotkey)
        while let Ok(cmd) = self.rx.try_recv() {
            match cmd.as_str() {
                "SHOW" => {
                    self.view = ActiveView::Launcher;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(330.0, 68.0)));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                "HOTKEY_FREE" => self.start_free_capture(ctx),
                "HOTKEY_FULL" => self.start_full_capture(ctx),
                "CAPTURE" => {
                    if self.settings.allow_cli {
                        self.start_free_capture(ctx);
                    } else {
                        self.show_toast("Komut satırı kapalı", "Ayarlar'dan izin verin", AMBER);
                    }
                }
                "CAPTURE_FULL" => {
                    if self.settings.allow_cli {
                        self.start_full_capture(ctx);
                    } else {
                        self.show_toast("Komut satırı kapalı", "Ayarlar'dan izin verin", AMBER);
                    }
                }
                _ => {}
            }
        }

        // 2. Sistem tepsisi tıklama olayları (sadece MouseButtonState::Up)
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            let is_caps = ev.id() == self.caps_tray.id();
            match ev {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    if is_caps {
                        if self.settings.caps_tray_single_click {
                            self.start_free_capture(ctx);
                        }
                    } else if self.settings.app_tray_left_click {
                        self.view = ActiveView::Launcher;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            330.0, 68.0,
                        )));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                }
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => {
                    if is_caps && self.settings.caps_tray_double_click {
                        self.start_full_capture(ctx);
                    }
                }
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    ..
                } if is_caps && self.settings.caps_tray_right_click => {
                    let p = if self.settings.save_folder.is_empty() {
                        default_save_folder()
                    } else {
                        PathBuf::from(&self.settings.save_folder)
                    };
                    open_folder(&p);
                }
                _ => {}
            }
        }

        // 3. Sistem tepsisi sağ tık menü öğeleri
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.0.as_str() {
                "free" => self.start_free_capture(ctx),
                "full" => self.start_full_capture(ctx),
                "folder" => {
                    let p = if self.settings.save_folder.is_empty() {
                        default_save_folder()
                    } else {
                        PathBuf::from(&self.settings.save_folder)
                    };
                    open_folder(&p);
                }
                "settings" => {
                    self.view = ActiveView::Settings;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        390.0, 630.0,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                "exit" => {
                    let _ = self.hotkey_ctrl.send(HotkeyControl::Shutdown);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
        }

        // 4. Caps Lock durum kontrolü (150 ms)
        if self.last_caps_check.elapsed() >= Duration::from_millis(150) {
            self.last_caps_check = Instant::now();
            let now = is_caps_lock_on();
            if now != self.last_caps {
                self.last_caps = now;
                let icon = Icon::from_rgba(generate_caps_rgba(64, now), 64, 64).unwrap();
                let _ = self.caps_tray.set_icon(Some(icon));
                let _ = self.caps_tray.set_tooltip(Some(if now {
                    "Caps Lock AÇIK"
                } else {
                    "Caps Lock KAPALI"
                }));
                self.show_toast(
                    "Caps Lock",
                    if now { "Açık" } else { "Kapalı" },
                    if now { ACCENT } else { MUTED },
                );
            }
        }

        // 5. Görünüm çizimi (Liquid Glass UI entegreli)
        match self.view {
            ActiveView::Hidden => {}

            ActiveView::Launcher => {
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgba_unmultiplied(7, 15, 26, 232))
                            .stroke(egui::Stroke::new(
                                1.2,
                                egui::Color32::from_rgba_unmultiplied(120, 160, 210, 140),
                            ))
                            .rounding(16.0)
                            .inner_margin(12.0),
                    )
                    .show(ctx, |ui| {
                        if ui
                            .interact(
                                ui.max_rect(),
                                egui::Id::new("drag_launcher"),
                                egui::Sense::drag(),
                            )
                            .dragged()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }

                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;

                            if self
                                .glass
                                .button(
                                    ui,
                                    "launcher_free",
                                    "Serbest",
                                    Some("▣"),
                                    GlassMaterial::Tinted,
                                    GlassSize::Md,
                                    ACCENT,
                                    0.8,
                                    true,
                                    Some(108.0),
                                )
                                .clicked()
                            {
                                self.start_free_capture(ctx);
                            }

                            if self
                                .glass
                                .button(
                                    ui,
                                    "launcher_full",
                                    "Tam Ekran",
                                    Some("▭"),
                                    GlassMaterial::Clear,
                                    GlassSize::Md,
                                    ACCENT2,
                                    0.6,
                                    true,
                                    Some(114.0),
                                )
                                .clicked()
                            {
                                self.start_full_capture(ctx);
                            }

                            if self
                                .glass
                                .button(
                                    ui,
                                    "launcher_close",
                                    "",
                                    Some("✕"),
                                    GlassMaterial::Clear,
                                    GlassSize::Md,
                                    RED,
                                    0.45,
                                    true,
                                    Some(44.0),
                                )
                                .clicked()
                            {
                                self.view = ActiveView::Hidden;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                            }
                        });

                        self.glass.end_frame(ui.painter());

                        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.view = ActiveView::Hidden;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        }
                    });
            }

            ActiveView::Overlay => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none())
                    .show(ctx, |ui| {
                        let rect = ui.max_rect();
                        if let Some(tex) = &self.preview_tex {
                            ui.image((tex.id(), rect.size()));
                        }
                        let painter = ui.painter();
                        painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(85));

                        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                        if let (Some(hover_pos), Some(raw)) = (resp.hover_pos(), &self.raw_screen) {
                            if self.sel_start.is_none() {
                                let sx = raw.width() as f32 / rect.width().max(1.0);
                                let sy = raw.height() as f32 / rect.height().max(1.0);
                                let rx = (hover_pos.x * sx) as i32;
                                let ry = (hover_pos.y * sy) as i32;
                                self.hovered_win = self
                                    .window_rects
                                    .iter()
                                    .filter(|w| w.contains(rx, ry))
                                    .min_by_key(|w| (w.w as i64) * (w.h as i64))
                                    .copied();
                            }
                        }

                        if resp.drag_started() {
                            self.sel_start = resp.interact_pointer_pos();
                            self.sel_end = self.sel_start;
                        }
                        if resp.dragged() {
                            self.sel_end = resp.interact_pointer_pos();
                        }

                        if let (Some(p1), Some(p2)) = (self.sel_start, self.sel_end) {
                            let sel = egui::Rect::from_two_pos(p1, p2);
                            painter.rect_filled(
                                sel,
                                0.0,
                                egui::Color32::from_rgba_unmultiplied(47, 128, 237, 40),
                            );
                            painter.rect_stroke(sel, 0.0, egui::Stroke::new(2.2, ACCENT2));
                            painter.text(
                                egui::pos2(sel.min.x + 6.0, (sel.min.y - 22.0).max(8.0)),
                                egui::Align2::LEFT_TOP,
                                format!("{} × {}", sel.width() as i32, sel.height() as i32),
                                egui::FontId::proportional(13.0),
                                WHITE,
                            );
                        } else if let (Some(w), Some(raw)) = (self.hovered_win, &self.raw_screen) {
                            let sx = rect.width() / raw.width() as f32;
                            let sy = rect.height() / raw.height() as f32;
                            let wrect = egui::Rect::from_min_size(
                                egui::pos2(w.x as f32 * sx, w.y as f32 * sy),
                                egui::vec2(w.w as f32 * sx, w.h as f32 * sy),
                            );
                            painter.rect_stroke(wrect, 2.0, egui::Stroke::new(2.6, ACCENT2));
                            painter.text(
                                egui::pos2(wrect.min.x + 8.0, (wrect.min.y - 24.0).max(8.0)),
                                egui::Align2::LEFT_TOP,
                                format!("Pencere: {} × {} (Tıkla ve Yakala)", w.w, w.h),
                                egui::FontId::proportional(13.0),
                                WHITE,
                            );
                        }

                        if resp.clicked() && self.sel_start.is_none() {
                            if let (Some(w), Some(raw)) = (self.hovered_win, self.raw_screen.take())
                            {
                                let x = w.x.max(0) as u32;
                                let y = w.y.max(0) as u32;
                                let cw = (w.w.max(1) as u32).min(raw.width().saturating_sub(x));
                                let ch = (w.h.max(1) as u32).min(raw.height().saturating_sub(y));
                                let cropped =
                                    image::imageops::crop_imm(&raw, x, y, cw, ch).to_image();
                                self.finish_capture(ctx, cropped);
                            }
                        }

                        if resp.drag_stopped() {
                            if let (Some(p1), Some(p2), Some(raw)) =
                                (self.sel_start, self.sel_end, self.raw_screen.take())
                            {
                                let sel = egui::Rect::from_two_pos(p1, p2);
                                if sel.width() >= 10.0 && sel.height() >= 10.0 {
                                    let sx = raw.width() as f32 / rect.width();
                                    let sy = raw.height() as f32 / rect.height();
                                    let x = (sel.min.x * sx) as u32;
                                    let y = (sel.min.y * sy) as u32;
                                    let w = (sel.width() * sx)
                                        .min((raw.width() - x) as f32)
                                        as u32;
                                    let h = (sel.height() * sy)
                                        .min((raw.height() - y) as f32)
                                        as u32;
                                    let cropped =
                                        image::imageops::crop_imm(&raw, x, y, w, h).to_image();
                                    self.finish_capture(ctx, cropped);
                                } else {
                                    self.busy = false;
                                    self.view = ActiveView::Hidden;
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                                }
                            }
                        }

                        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.busy = false;
                            self.raw_screen = None;
                            self.view = ActiveView::Hidden;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        }
                    });
            }

            ActiveView::Preview => {
                if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z)) {
                    self.undo(ctx);
                }
                if ctx.input(|i| {
                    i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                }) && self.text_popup_pos.is_none()
                {
                    if let Some(idx) = self.selected_idx.take() {
                        if idx < self.items.len() {
                            self.push_undo();
                            self.items.remove(idx);
                        }
                    }
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    if self.selected_idx.is_some() {
                        self.selected_idx = None;
                    } else {
                        self.view = ActiveView::Hidden;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    }
                }

                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgba_unmultiplied(7, 13, 22, 244))
                            .stroke(egui::Stroke::new(
                                1.1,
                                egui::Color32::from_rgba_unmultiplied(110, 145, 190, 135),
                            ))
                            .rounding(14.0)
                            .inner_margin(10.0),
                    )
                    .show(ctx, |ui| {
                        // Üst başlık çubuğu
                        ui.horizontal(|ui| {
                            if ui
                                .interact(
                                    egui::Rect::from_min_size(
                                        ui.min_rect().min,
                                        egui::vec2(280.0, 32.0),
                                    ),
                                    egui::Id::new("preview_drag"),
                                    egui::Sense::drag(),
                                )
                                .dragged()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }
                            ui.label(
                                egui::RichText::new("MY Screen 7.1 · Ön İzleme")
                                    .color(WHITE)
                                    .strong(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "prev_close",
                                            "",
                                            Some("✕"),
                                            GlassMaterial::Clear,
                                            GlassSize::Sm,
                                            RED,
                                            0.4,
                                            true,
                                            Some(34.0),
                                        )
                                        .clicked()
                                    {
                                        self.view = ActiveView::Hidden;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                                    }

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "prev_settings",
                                            "",
                                            Some("⚙"),
                                            GlassMaterial::Clear,
                                            GlassSize::Sm,
                                            ACCENT2,
                                            0.45,
                                            true,
                                            Some(34.0),
                                        )
                                        .clicked()
                                    {
                                        self.view = ActiveView::Settings;
                                    }

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "prev_history",
                                            "Geçmiş",
                                            Some("🕘"),
                                            if self.history_open {
                                                GlassMaterial::Tinted
                                            } else {
                                                GlassMaterial::Clear
                                            },
                                            GlassSize::Sm,
                                            ACCENT,
                                            0.55,
                                            true,
                                            None,
                                        )
                                        .clicked()
                                    {
                                        self.history_open = !self.history_open;
                                        if self.history_open {
                                            self.refresh_history();
                                        }
                                    }

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "prev_tools",
                                            "Araçlar",
                                            Some("✎"),
                                            if self.tools_open {
                                                GlassMaterial::Tinted
                                            } else {
                                                GlassMaterial::Clear
                                            },
                                            GlassSize::Sm,
                                            ACCENT,
                                            0.55,
                                            true,
                                            None,
                                        )
                                        .clicked()
                                    {
                                        self.tools_open = !self.tools_open;
                                    }
                                },
                            );
                        });
                        ui.separator();

                        // Orta çalışma alanı: Sol Araçlar | Tuval | Sağ Geçmiş
                        ui.horizontal(|ui| {
                            let body_h = (ui.available_height() - 48.0).max(160.0);

                            if self.tools_open {
                                egui::Frame::none()
                                    .fill(PANEL2)
                                    .stroke(egui::Stroke::new(1.0, BORDER))
                                    .rounding(10.0)
                                    .inner_margin(6.0)
                                    .show(ui, |ui| {
                                        ui.set_height(body_h);
                                        ui.vertical(|ui| {
                                            for (label, tool) in [
                                                ("▭", ToolKind::Rect),
                                                ("◯", ToolKind::Ellipse),
                                                ("╱", ToolKind::Arrow),
                                                ("✎", ToolKind::Pen),
                                                ("▦", ToolKind::Blur),
                                                ("A", ToolKind::Text),
                                                ("⛶", ToolKind::Crop),
                                                ("①", ToolKind::Marker),
                                                ("▬", ToolKind::Highlight),
                                            ] {
                                                let active = self.active_tool == tool;
                                                if ui
                                                    .add_sized(
                                                        [32.0, 28.0],
                                                        egui::SelectableLabel::new(active, label),
                                                    )
                                                    .clicked()
                                                {
                                                    self.active_tool = tool;
                                                }
                                            }
                                            ui.separator();
                                            for col in [
                                                [255, 59, 48],
                                                [255, 159, 10],
                                                [52, 199, 89],
                                                [10, 132, 255],
                                                [255, 214, 10],
                                                [255, 255, 255],
                                            ] {
                                                let c32 =
                                                    egui::Color32::from_rgb(col[0], col[1], col[2]);
                                                let (r, resp) = ui.allocate_exact_size(
                                                    egui::vec2(28.0, 16.0),
                                                    egui::Sense::click(),
                                                );
                                                ui.painter().rect_filled(r.shrink(2.0), 4.0, c32);
                                                if resp.clicked() {
                                                    self.active_color = col;
                                                    if let Some(idx) = self.selected_idx {
                                                        if let Some(it) = self.items.get_mut(idx) {
                                                            it.color = col;
                                                        }
                                                    }
                                                }
                                            }
                                            ui.separator();
                                            for w in [2.0, 4.0, 8.0, 12.0] {
                                                if ui
                                                    .selectable_label(
                                                        (self.pen_width - w).abs() < 0.1,
                                                        format!("{}px", w as i32),
                                                    )
                                                    .clicked()
                                                {
                                                    self.pen_width = w;
                                                }
                                            }
                                            if ui.button("↩").clicked() {
                                                self.undo(ctx);
                                            }
                                        });
                                    });
                            }

                            let right_reserve = if self.history_open { 156.0 } else { 0.0 };
                            let canvas_w = (ui.available_width() - right_reserve).max(160.0);
                            egui::Frame::none()
                                .fill(egui::Color32::from_rgb(6, 11, 18))
                                .stroke(egui::Stroke::new(1.0, BORDER))
                                .rounding(10.0)
                                .show(ui, |ui| {
                                    ui.set_size(egui::vec2(canvas_w, body_h));
                                    let tex_meta = match (&self.preview_tex, &self.bg_img) {
                                        (Some(t), Some(b)) => {
                                            Some((t.id(), b.width() as f32, b.height() as f32))
                                        }
                                        _ => None,
                                    };

                                    if let Some((tex_id, iw, ih)) = tex_meta {
                                        let scale = (canvas_w / iw).min(body_h / ih);
                                        let disp_w = iw * scale;
                                        let disp_h = ih * scale;
                                        let off_x = ui.min_rect().min.x + (canvas_w - disp_w) / 2.0;
                                        let off_y = ui.min_rect().min.y + (body_h - disp_h) / 2.0;
                                        let img_rect = egui::Rect::from_min_size(
                                            egui::pos2(off_x, off_y),
                                            egui::vec2(disp_w, disp_h),
                                        );

                                        ui.painter().image(
                                            tex_id,
                                            img_rect,
                                            egui::Rect::from_min_max(
                                                egui::pos2(0.0, 0.0),
                                                egui::pos2(1.0, 1.0),
                                            ),
                                            WHITE,
                                        );

                                        let to_img = |p: egui::Pos2| -> [f32; 2] {
                                            [
                                                ((p.x - img_rect.min.x) / scale).clamp(0.0, iw),
                                                ((p.y - img_rect.min.y) / scale).clamp(0.0, ih),
                                            ]
                                        };
                                        let to_scr = |pt: [f32; 2]| -> egui::Pos2 {
                                            egui::pos2(
                                                img_rect.min.x + pt[0] * scale,
                                                img_rect.min.y + pt[1] * scale,
                                            )
                                        };

                                        let painter = ui.painter().with_clip_rect(img_rect);
                                        for (idx, it) in self.items.iter().enumerate() {
                                            let c32 = egui::Color32::from_rgb(
                                                it.color[0],
                                                it.color[1],
                                                it.color[2],
                                            );
                                            let stroke =
                                                egui::Stroke::new((it.width * scale).max(1.0), c32);
                                            match it.kind {
                                                ToolKind::Rect => {
                                                    let r = egui::Rect::from_two_pos(
                                                        to_scr([it.x1, it.y1]),
                                                        to_scr([it.x2, it.y2]),
                                                    );
                                                    painter.rect_stroke(r, 0.0, stroke);
                                                }
                                                ToolKind::Ellipse => {
                                                    let r = egui::Rect::from_two_pos(
                                                        to_scr([it.x1, it.y1]),
                                                        to_scr([it.x2, it.y2]),
                                                    );
                                                    painter.add(egui::epaint::EllipseShape::stroke(
                                                        r.center(),
                                                        r.size() * 0.5,
                                                        stroke,
                                                    ));
                                                }
                                                ToolKind::Arrow => {
                                                    let a = to_scr([it.x1, it.y1]);
                                                    let b = to_scr([it.x2, it.y2]);
                                                    painter.line_segment([a, b], stroke);
                                                    let ang = (b.y - a.y).atan2(b.x - a.x);
                                                    let sz = (it.width * scale * 4.0).max(12.0);
                                                    painter.line_segment(
                                                        [
                                                            b,
                                                            egui::pos2(
                                                                b.x - sz * (ang - 0.4).cos(),
                                                                b.y - sz * (ang - 0.4).sin(),
                                                            ),
                                                        ],
                                                        stroke,
                                                    );
                                                    painter.line_segment(
                                                        [
                                                            b,
                                                            egui::pos2(
                                                                b.x - sz * (ang + 0.4).cos(),
                                                                b.y - sz * (ang + 0.4).sin(),
                                                            ),
                                                        ],
                                                        stroke,
                                                    );
                                                }
                                                ToolKind::Pen => {
                                                    let pts: Vec<egui::Pos2> =
                                                        it.points.iter().map(|&p| to_scr(p)).collect();
                                                    if pts.len() > 1 {
                                                        painter.add(egui::Shape::line(pts, stroke));
                                                    }
                                                }
                                                ToolKind::Highlight => {
                                                    let r = egui::Rect::from_two_pos(
                                                        to_scr([it.x1, it.y1]),
                                                        to_scr([it.x2, it.y2]),
                                                    );
                                                    painter.rect_filled(
                                                        r,
                                                        0.0,
                                                        egui::Color32::from_rgba_unmultiplied(
                                                            it.color[0],
                                                            it.color[1],
                                                            it.color[2],
                                                            100,
                                                        ),
                                                    );
                                                }
                                                ToolKind::Blur => {
                                                    let r = egui::Rect::from_two_pos(
                                                        to_scr([it.x1, it.y1]),
                                                        to_scr([it.x2, it.y2]),
                                                    );
                                                    painter.rect_filled(
                                                        r,
                                                        2.0,
                                                        egui::Color32::from_rgba_unmultiplied(
                                                            120, 140, 165, 160,
                                                        ),
                                                    );
                                                }
                                                ToolKind::Marker => {
                                                    let c = to_scr([it.x1, it.y1]);
                                                    let rad =
                                                        ((it.width * 2.0 + 9.0) * scale).max(10.0);
                                                    painter.circle_filled(c, rad, c32);
                                                    painter.circle_stroke(
                                                        c,
                                                        rad,
                                                        egui::Stroke::new(1.5, WHITE),
                                                    );
                                                    painter.text(
                                                        c,
                                                        egui::Align2::CENTER_CENTER,
                                                        it.marker_num.to_string(),
                                                        egui::FontId::proportional(rad * 1.15),
                                                        WHITE,
                                                    );
                                                }
                                                ToolKind::Text => {
                                                    let p = to_scr([it.x1, it.y1]);
                                                    painter.text(
                                                        p,
                                                        egui::Align2::LEFT_TOP,
                                                        &it.text,
                                                        egui::FontId::proportional(
                                                            (it.font_size * scale).max(10.0),
                                                        ),
                                                        c32,
                                                    );
                                                }
                                                ToolKind::Crop => {}
                                            }

                                            if self.selected_idx == Some(idx) {
                                                let (bx1, by1, bx2, by2) = it.bbox();
                                                let sr = egui::Rect::from_two_pos(
                                                    to_scr([bx1, by1]),
                                                    to_scr([bx2, by2]),
                                                )
                                                .expand(4.0);
                                                painter.rect_stroke(
                                                    sr,
                                                    2.0,
                                                    egui::Stroke::new(1.5, ACCENT2),
                                                );
                                            }
                                        }

                                        if let (Some(s), Some(c)) =
                                            (self.active_draw_start, self.active_draw_curr)
                                        {
                                            let r =
                                                egui::Rect::from_two_pos(to_scr(s), to_scr(c));
                                            painter.rect_stroke(
                                                r,
                                                0.0,
                                                egui::Stroke::new(1.5, ACCENT2),
                                            );
                                        }

                                        let resp = ui.allocate_rect(
                                            img_rect,
                                            egui::Sense::click_and_drag(),
                                        );
                                        if resp.clicked() {
                                            if let Some(pos) = resp.interact_pointer_pos() {
                                                let ipt = to_img(pos);
                                                if let Some(hit) = self
                                                    .items
                                                    .iter()
                                                    .rposition(|it| it.contains(ipt[0], ipt[1], 10.0))
                                                {
                                                    self.selected_idx = Some(hit);
                                                } else if self.active_tool == ToolKind::Marker {
                                                    self.push_undo();
                                                    let next_n = self
                                                        .items
                                                        .iter()
                                                        .filter(|i| i.kind == ToolKind::Marker)
                                                        .map(|i| i.marker_num)
                                                        .max()
                                                        .unwrap_or(0)
                                                        + 1;
                                                    self.items.push(DrawItem {
                                                        kind: ToolKind::Marker,
                                                        x1: ipt[0],
                                                        y1: ipt[1],
                                                        x2: ipt[0],
                                                        y2: ipt[1],
                                                        color: self.active_color,
                                                        width: self.pen_width,
                                                        points: Vec::new(),
                                                        text: String::new(),
                                                        font_size: 16.0,
                                                        marker_num: next_n,
                                                    });
                                                    self.selected_idx = Some(self.items.len() - 1);
                                                } else if self.active_tool == ToolKind::Text {
                                                    self.text_popup_pos = Some(ipt);
                                                    self.pending_text.clear();
                                                } else {
                                                    self.selected_idx = None;
                                                }
                                            }
                                        }

                                        if resp.drag_started() {
                                            if let Some(pos) = resp.interact_pointer_pos() {
                                                let ipt = to_img(pos);
                                                if let Some(hit) = self
                                                    .items
                                                    .iter()
                                                    .rposition(|it| it.contains(ipt[0], ipt[1], 10.0))
                                                {
                                                    self.selected_idx = Some(hit);
                                                    self.push_undo();
                                                    self.drag_last_img_pt = Some(ipt);
                                                } else {
                                                    self.selected_idx = None;
                                                    self.active_draw_start = Some(ipt);
                                                    self.active_draw_curr = Some(ipt);
                                                    self.active_pen_pts = vec![ipt];
                                                }
                                            }
                                        }

                                        if resp.dragged() {
                                            if let Some(pos) = resp.interact_pointer_pos() {
                                                let ipt = to_img(pos);
                                                if let (Some(idx), Some(last)) =
                                                    (self.selected_idx, self.drag_last_img_pt)
                                                {
                                                    if let Some(it) = self.items.get_mut(idx) {
                                                        it.translate(
                                                            ipt[0] - last[0],
                                                            ipt[1] - last[1],
                                                        );
                                                    }
                                                    self.drag_last_img_pt = Some(ipt);
                                                } else if self.active_draw_start.is_some() {
                                                    self.active_draw_curr = Some(ipt);
                                                    if self.active_tool == ToolKind::Pen {
                                                        self.active_pen_pts.push(ipt);
                                                    }
                                                }
                                            }
                                        }

                                        if resp.drag_stopped() {
                                            self.drag_last_img_pt = None;
                                            if let (Some(s), Some(c)) = (
                                                self.active_draw_start.take(),
                                                self.active_draw_curr.take(),
                                            ) {
                                                if self.active_tool == ToolKind::Crop {
                                                    let x1 = s[0].min(c[0]) as u32;
                                                    let y1 = s[1].min(c[1]) as u32;
                                                    let x2 = s[0].max(c[0]) as u32;
                                                    let y2 = s[1].max(c[1]) as u32;
                                                    if x2 > x1 + 8 && y2 > y1 + 8 {
                                                        self.push_undo();
                                                        let cropped_opt =
                                                            self.bg_img.as_ref().map(|bg| {
                                                                let comp =
                                                                    compose_image(bg, &self.items);
                                                                image::imageops::crop_imm(
                                                                    &comp,
                                                                    x1,
                                                                    y1,
                                                                    x2 - x1,
                                                                    y2 - y1,
                                                                )
                                                                .to_image()
                                                            });
                                                        if let Some(cropped) = cropped_opt {
                                                            let ci =
                                                                egui::ColorImage::from_rgba_unmultiplied(
                                                                    [
                                                                        cropped.width() as usize,
                                                                        cropped.height() as usize,
                                                                    ],
                                                                    cropped.as_raw(),
                                                                );
                                                            self.preview_tex = Some(
                                                                ctx.load_texture(
                                                                    "preview",
                                                                    ci,
                                                                    egui::TextureOptions::LINEAR,
                                                                ),
                                                            );
                                                            self.bg_img = Some(cropped);
                                                            self.items.clear();
                                                        }
                                                    }
                                                } else if (c[0] - s[0]).hypot(c[1] - s[1]) >= 4.0 {
                                                    self.push_undo();
                                                    self.items.push(DrawItem {
                                                        kind: self.active_tool,
                                                        x1: s[0],
                                                        y1: s[1],
                                                        x2: c[0],
                                                        y2: c[1],
                                                        color: self.active_color,
                                                        width: self.pen_width,
                                                        points: std::mem::take(
                                                            &mut self.active_pen_pts,
                                                        ),
                                                        text: String::new(),
                                                        font_size: 16.0,
                                                        marker_num: 0,
                                                    });
                                                    self.selected_idx = Some(self.items.len() - 1);
                                                }
                                            }
                                        }
                                    }
                                });

                            // Sağ Geçmiş Paneli
                            if self.history_open {
                                egui::Frame::none()
                                    .fill(PANEL2)
                                    .stroke(egui::Stroke::new(1.0, BORDER))
                                    .rounding(10.0)
                                    .inner_margin(6.0)
                                    .show(ui, |ui| {
                                        ui.set_size(egui::vec2(142.0, body_h));
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Geçmiş")
                                                    .color(ACCENT2)
                                                    .strong(),
                                            );
                                            if ui.small_button("↻").clicked() {
                                                self.refresh_history();
                                            }
                                        });
                                        ui.separator();
                                        egui::ScrollArea::vertical().show(ui, |ui| {
                                            let paths: Vec<(PathBuf, String, bool)> = self
                                                .history_list
                                                .iter()
                                                .map(|h| {
                                                    (
                                                        h.path.clone(),
                                                        h.name.clone(),
                                                        h.has_layers,
                                                    )
                                                })
                                                .collect();
                                            for (p, name, has_l) in paths {
                                                let tag = if has_l { "● " } else { "" };
                                                let short = if name.len() > 16 {
                                                    format!("{}{}...", tag, &name[..13])
                                                } else {
                                                    format!("{}{}", tag, name)
                                                };
                                                if ui.selectable_label(false, short).clicked() {
                                                    self.load_history_item(ctx, p);
                                                }
                                            }
                                        });
                                    });
                            }
                        });

                        // Metin ekleme modal penceresi
                        if let Some(pos) = self.text_popup_pos {
                            egui::Window::new("Yazı Ekle")
                                .collapsible(false)
                                .resizable(false)
                                .show(ctx, |ui| {
                                    ui.text_edit_singleline(&mut self.pending_text);
                                    ui.add(
                                        egui::Slider::new(&mut self.pending_font_size, 12.0..=96.0)
                                            .text("Boyut"),
                                    );
                                    ui.horizontal(|ui| {
                                        if ui.button("Uygula").clicked() {
                                            if !self.pending_text.trim().is_empty() {
                                                self.push_undo();
                                                self.items.push(DrawItem {
                                                    kind: ToolKind::Text,
                                                    x1: pos[0],
                                                    y1: pos[1],
                                                    x2: pos[0],
                                                    y2: pos[1],
                                                    color: self.active_color,
                                                    width: self.pen_width,
                                                    points: Vec::new(),
                                                    text: self.pending_text.trim().to_string(),
                                                    font_size: self.pending_font_size,
                                                    marker_num: 0,
                                                });
                                                self.selected_idx = Some(self.items.len() - 1);
                                            }
                                            self.text_popup_pos = None;
                                        }
                                        if ui.button("İptal").clicked() {
                                            self.text_popup_pos = None;
                                        }
                                    });
                                });
                        }

                        ui.separator();

                        // Alt Liquid Glass İşlem Çubuğu
                        ui.horizontal(|ui| {
                            if let Some(bg) = &self.bg_img {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} × {} px  |  {} katman",
                                        bg.width(),
                                        bg.height(),
                                        self.items.len()
                                    ))
                                    .color(ACCENT2),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 10.0;

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "act_close",
                                            "Kapat",
                                            Some("✕"),
                                            GlassMaterial::Clear,
                                            GlassSize::Sm,
                                            MUTED,
                                            0.45,
                                            true,
                                            None,
                                        )
                                        .clicked()
                                    {
                                        self.view = ActiveView::Hidden;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                                    }

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "act_folder",
                                            "Klasör",
                                            Some("📁"),
                                            GlassMaterial::Clear,
                                            GlassSize::Sm,
                                            AMBER,
                                            0.55,
                                            true,
                                            None,
                                        )
                                        .clicked()
                                    {
                                        let folder = if self.settings.save_folder.is_empty() {
                                            default_save_folder()
                                        } else {
                                            PathBuf::from(&self.settings.save_folder)
                                        };
                                        open_folder(&folder);
                                        self.view = ActiveView::Hidden;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                                    }

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "act_copy",
                                            "Kopyala",
                                            Some("📋"),
                                            GlassMaterial::Tinted,
                                            GlassSize::Sm,
                                            ACCENT,
                                            0.75,
                                            true,
                                            None,
                                        )
                                        .clicked()
                                    {
                                        let comp_opt = self
                                            .bg_img
                                            .as_ref()
                                            .map(|bg| compose_image(bg, &self.items));
                                        if let Some(comp) = comp_opt {
                                            copy_image_to_clipboard(&comp);
                                            self.show_toast(
                                                "Panoya kopyalandı",
                                                "Görüntü hazır",
                                                ACCENT2,
                                            );
                                            self.view = ActiveView::Hidden;
                                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(
                                                false,
                                            ));
                                        }
                                    }

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "act_delete",
                                            "Sil",
                                            Some("🗑"),
                                            GlassMaterial::Clear,
                                            GlassSize::Sm,
                                            RED,
                                            0.65,
                                            true,
                                            None,
                                        )
                                        .clicked()
                                    {
                                        if let Some(p) = self.current_path.take() {
                                            let _ = fs::remove_file(&p);
                                            let _ = fs::remove_file(sidecar_path(&p));
                                            let _ =
                                                fs::remove_file(format!("{}.bg.png", p.display()));
                                            self.show_toast("Silindi", "Dosya kaldırıldı", RED);
                                        }
                                        self.view = ActiveView::Hidden;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                                    }

                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "act_save",
                                            "Kaydet",
                                            Some("💾"),
                                            GlassMaterial::Tinted,
                                            GlassSize::Sm,
                                            GREEN,
                                            0.85,
                                            true,
                                            None,
                                        )
                                        .clicked()
                                    {
                                        let bg_clone = self.bg_img.clone();
                                        if let Some(bg) = bg_clone {
                                            let comp = compose_image(&bg, &self.items);
                                            let mut overwritten = false;
                                            if let Some(p) = self.current_path.clone() {
                                                if !self.settings.ask_every_time
                                                    && save_image_to_explicit_path(
                                                        &comp,
                                                        &p,
                                                        self.settings.jpeg_quality,
                                                    )
                                                {
                                                    save_layer_sidecar(&p, &bg, &self.items);
                                                    self.show_toast(
                                                        "Kaydedildi",
                                                        "Dosya güncellendi",
                                                        GREEN,
                                                    );
                                                    self.view = ActiveView::Hidden;
                                                    ctx.send_viewport_cmd(
                                                        egui::ViewportCommand::Visible(false),
                                                    );
                                                    overwritten = true;
                                                }
                                            }
                                            if !overwritten {
                                                if let Some(new_p) = save_capture_file(
                                                    &comp,
                                                    &self.settings,
                                                    self.settings.ask_every_time,
                                                ) {
                                                    save_layer_sidecar(&new_p, &bg, &self.items);
                                                    self.current_path = Some(new_p);
                                                    self.show_toast(
                                                        "Kaydedildi",
                                                        "Ekran görüntüsü alındı",
                                                        GREEN,
                                                    );
                                                    self.view = ActiveView::Hidden;
                                                    ctx.send_viewport_cmd(
                                                        egui::ViewportCommand::Visible(false),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                },
                            );
                        });

                        self.glass.end_frame(ui.painter());
                    });
            }

            ActiveView::Settings => {
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgba_unmultiplied(11, 20, 32, 245))
                            .stroke(egui::Stroke::new(
                                1.1,
                                egui::Color32::from_rgba_unmultiplied(110, 145, 190, 135),
                            ))
                            .rounding(14.0)
                            .inner_margin(14.0),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .interact(
                                    egui::Rect::from_min_size(
                                        ui.min_rect().min,
                                        egui::vec2(260.0, 28.0),
                                    ),
                                    egui::Id::new("settings_drag"),
                                    egui::Sense::drag(),
                                )
                                .dragged()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }
                            ui.heading(egui::RichText::new("Ayarlar").color(WHITE));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if self
                                        .glass
                                        .button(
                                            ui,
                                            "set_close",
                                            "",
                                            Some("✕"),
                                            GlassMaterial::Clear,
                                            GlassSize::Sm,
                                            RED,
                                            0.45,
                                            true,
                                            Some(34.0),
                                        )
                                        .clicked()
                                    {
                                        self.view = ActiveView::Hidden;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                                    }
                                },
                            );
                        });
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let mut changed = false;
                            let mut hotkeys_changed = false;

                            changed |= ui
                                .checkbox(&mut self.settings.auto_save, "Otomatik kaydet")
                                .changed();
                            changed |= ui
                                .checkbox(&mut self.settings.auto_copy, "Panoya kopyala")
                                .changed();
                            if ui
                                .checkbox(
                                    &mut self.settings.start_with_windows,
                                    "Windows ile başlat",
                                )
                                .changed()
                            {
                                set_windows_startup(self.settings.start_with_windows);
                                changed = true;
                            }
                            ui.separator();
                            changed |= ui
                                .checkbox(
                                    &mut self.settings.app_tray_left_click,
                                    "Ana simge sol / çift tık",
                                )
                                .changed();
                            changed |= ui
                                .checkbox(
                                    &mut self.settings.caps_tray_single_click,
                                    "Caps Lock sol tık (Serbest)",
                                )
                                .changed();
                            changed |= ui
                                .checkbox(
                                    &mut self.settings.caps_tray_double_click,
                                    "Caps Lock çift tık (Tam)",
                                )
                                .changed();
                            ui.separator();
                            changed |= ui
                                .checkbox(&mut self.settings.notify, "İşlem bildirimi")
                                .changed();
                            if ui
                                .checkbox(&mut self.settings.caps_notify, "Caps Lock bildirimi")
                                .changed()
                            {
                                let _ = self.caps_tray.set_visible(self.settings.caps_notify);
                                changed = true;
                            }
                            changed |= ui
                                .checkbox(
                                    &mut self.settings.ask_every_time,
                                    "Her kayıtta konumu sor",
                                )
                                .changed();
                            changed |= ui
                                .checkbox(
                                    &mut self.settings.allow_cli,
                                    "Komut satırı tetikleyicileri (--capture)",
                                )
                                .changed();
                            ui.separator();

                            ui.horizontal(|ui| {
                                ui.label("Yakalama sonrası:");
                                egui::ComboBox::from_id_salt("after_cap")
                                    .selected_text(match self.settings.after_capture.as_str() {
                                        "copy" => "Yalnızca kopyala",
                                        "save" => "Yalnızca kaydet",
                                        _ => "Önizleme aç",
                                    })
                                    .show_ui(ui, |ui| {
                                        changed |= ui
                                            .selectable_value(
                                                &mut self.settings.after_capture,
                                                "preview".into(),
                                                "Önizleme aç",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut self.settings.after_capture,
                                                "copy".into(),
                                                "Yalnızca kopyala",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut self.settings.after_capture,
                                                "save".into(),
                                                "Yalnızca kaydet",
                                            )
                                            .changed();
                                    });
                            });

                            ui.horizontal(|ui| {
                                ui.label("Dosya türü:");
                                egui::ComboBox::from_id_salt("img_fmt")
                                    .selected_text(&self.settings.image_format)
                                    .show_ui(ui, |ui| {
                                        changed |= ui
                                            .selectable_value(
                                                &mut self.settings.image_format,
                                                "PNG".into(),
                                                "PNG",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut self.settings.image_format,
                                                "JPG".into(),
                                                "JPG",
                                            )
                                            .changed();
                                    });
                            });

                            ui.horizontal(|ui| {
                                ui.label("Serbest kısayol:");
                                if ui
                                    .text_edit_singleline(&mut self.settings.hotkey_free)
                                    .changed()
                                {
                                    changed = true;
                                    hotkeys_changed = true;
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label("Tam ekran kısayol:");
                                if ui
                                    .text_edit_singleline(&mut self.settings.hotkey_full)
                                    .changed()
                                {
                                    changed = true;
                                    hotkeys_changed = true;
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label("Dosya öneki:");
                                changed |= ui
                                    .text_edit_singleline(&mut self.settings.file_prefix)
                                    .changed();
                            });

                            ui.add_space(6.0);
                            if self
                                .glass
                                .button(
                                    ui,
                                    "set_folder",
                                    "Kayıt Klasörü Seç",
                                    Some("📁"),
                                    GlassMaterial::Tinted,
                                    GlassSize::Sm,
                                    ACCENT,
                                    0.75,
                                    true,
                                    None,
                                )
                                .clicked()
                            {
                                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                    self.settings.save_folder = dir.display().to_string();
                                    changed = true;
                                }
                            }

                            if hotkeys_changed {
                                let _ = self.hotkey_ctrl.send(HotkeyControl::Update {
                                    free: self.settings.hotkey_free.clone(),
                                    full: self.settings.hotkey_full.clone(),
                                });
                            }

                            if changed {
                                save_settings(&self.settings);
                                self.show_toast("Kaydedildi", "Ayarlar güncellendi", GREEN);
                            }
                        });

                        self.glass.end_frame(ui.painter());
                    });
            }
        }

        // 6. Sağ alt köşe Toast bildirimi
        if let Some((title, sub, col, at)) = &self.toast_msg {
            if at.elapsed() < Duration::from_millis(2200) {
                egui::Area::new(egui::Id::new("toast_area"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-14.0, -14.0))
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgba_unmultiplied(16, 29, 45, 242))
                            .stroke(egui::Stroke::new(1.2, *col))
                            .rounding(12.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(title).color(WHITE).strong());
                                    ui.label(egui::RichText::new(sub).color(MUTED).small());
                                });
                            });
                    });
            } else {
                self.toast_msg = None;
            }
        }

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

pub fn run_app(tx: Sender<String>, rx: Receiver<String>) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([330.0, 68.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |_cc| Ok(Box::new(MyScreenRustApp::new(tx, rx)))),
    )
}

use crate::paths::{log_error, settings_path};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;

const APP_REG_NAME: &str = "MY Screen 7.1 Professional";
const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub save_folder: String,
    pub auto_save: bool,
    pub auto_copy: bool,
    pub notify: bool,
    pub caps_notify: bool,
    pub app_tray_left_click: bool,
    pub app_tray_right_click: bool,
    pub caps_tray_single_click: bool,
    pub caps_tray_double_click: bool,
    pub caps_tray_right_click: bool,
    pub start_with_windows: bool,
    pub image_format: String,
    pub jpeg_quality: u8,
    pub ask_every_time: bool,
    pub file_prefix: String,
    pub capture_btn_free_pos: [f32; 2],
    pub capture_btn_full_pos: [f32; 2],
    pub show_capture_btn_free: bool,
    pub show_capture_btn_full: bool,
    pub show_desktop_btns: bool,
    pub desktop_btns_pos: Option<[i32; 2]>,
    pub hotkey_free: String,
    pub hotkey_full: String,
    pub after_capture: String,
    pub allow_cli: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            save_folder: String::new(),
            auto_save: true,
            auto_copy: true,
            notify: true,
            caps_notify: true,
            app_tray_left_click: true,
            app_tray_right_click: true,
            caps_tray_single_click: true,
            caps_tray_double_click: true,
            caps_tray_right_click: true,
            start_with_windows: false,
            image_format: "PNG".into(),
            jpeg_quality: 90,
            ask_every_time: false,
            file_prefix: "MY_Screen_".into(),
            capture_btn_free_pos: [0.02, 0.15],
            capture_btn_full_pos: [0.02, 0.25],
            show_capture_btn_free: true,
            show_capture_btn_full: true,
            show_desktop_btns: true,
            desktop_btns_pos: None,
            hotkey_free: "Print Screen".into(),
            hotkey_full: String::new(),
            after_capture: "preview".into(),
            allow_cli: false,
        }
    }
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(s) = serde_json::from_str::<AppSettings>(&content) {
            return s;
        }
    }
    AppSettings::default()
}

pub fn save_settings(settings: &AppSettings) {
    let path = settings_path();
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        if let Err(e) = fs::write(&path, json) {
            log_error("Ayarlar kaydedilemedi", &e.to_string());
        }
    }
}

fn clean_startup_folder() {
    let Ok(appdata) = env::var("APPDATA") else { return };
    let startup_dir = PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");
    let Ok(entries) = fs::read_dir(startup_dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase()
            .replace('_', " ");
        if stem == APP_REG_NAME.to_lowercase() || stem.starts_with("my screen") {
            let _ = fs::remove_file(path);
        }
    }
}

pub fn set_windows_startup(enable: bool) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((key, _)) = hkcu.create_subkey(RUN_KEY_PATH) {
        if enable {
            if let Ok(exe) = env::current_exe() {
                let val = format!("\"{}\"", exe.display());
                let _ = key.set_value(APP_REG_NAME, &val);
            }
        } else {
            let _ = key.delete_value(APP_REG_NAME);
            clean_startup_folder();
        }
    }
}

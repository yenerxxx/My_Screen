use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY};

pub const HK_FREE: i32 = 701;
pub const HK_FULL: i32 = 702;

pub enum HotkeyControl {
    Update { free: String, full: String },
    Shutdown,
}

pub fn parse_hotkey(text: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    if text.trim().is_empty() {
        return None;
    }
    let mut mods = MOD_NOREPEAT;
    let mut vk: Option<u32> = None;

    for part in text.split('+').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" | "windows" => mods |= MOD_WIN,
            other => {
                if vk.is_some() {
                    return None;
                }
                vk = match other {
                    "print screen" => Some(0x2C),
                    "pause" => Some(0x13),
                    "insert" => Some(0x2D),
                    "delete" => Some(0x2E),
                    "home" => Some(0x24),
                    "end" => Some(0x23),
                    "page up" => Some(0x21),
                    "page down" => Some(0x22),
                    "space" => Some(0x20),
                    "left" => Some(0x25),
                    "up" => Some(0x26),
                    "right" => Some(0x27),
                    "down" => Some(0x28),
                    s if s.len() == 1 && s.chars().next().unwrap().is_ascii_alphanumeric() => {
                        Some(s.chars().next().unwrap().to_ascii_uppercase() as u32)
                    }
                    s if s.starts_with('f') => s[1..]
                        .parse::<u32>()
                        .ok()
                        .filter(|&n| (1..=24).contains(&n))
                        .map(|n| 0x70 + n - 1),
                    _ => None,
                };
                if vk.is_none() {
                    return None;
                }
            }
        }
    }
    vk.map(|k| (mods, k))
}

unsafe fn apply_bindings(hwnd: HWND, free_combo: &str, full_combo: &str) {
    let _ = UnregisterHotKey(hwnd, HK_FREE);
    let _ = UnregisterHotKey(hwnd, HK_FULL);
    if let Some((mods, vk)) = parse_hotkey(free_combo) {
        let _ = RegisterHotKey(hwnd, HK_FREE, mods, vk);
    }
    if let Some((mods, vk)) = parse_hotkey(full_combo) {
        let _ = RegisterHotKey(hwnd, HK_FULL, mods, vk);
    }
}

/// Dinamik olarak güncellenebilen Win32 global kısayol dinleyicisi.
pub fn spawn_hotkey_listener(
    free_combo: String,
    full_combo: String,
    tx: Sender<String>,
) -> Sender<HotkeyControl> {
    let (ctrl_tx, ctrl_rx) = mpsc::channel::<HotkeyControl>();
    thread::spawn(move || unsafe {
        let hwnd = HWND::default();
        apply_bindings(hwnd, &free_combo, &full_combo);

        let mut msg = MSG::default();
        loop {
            while let Ok(ctrl) = ctrl_rx.try_recv() {
                match ctrl {
                    HotkeyControl::Update { free, full } => {
                        apply_bindings(hwnd, &free, &full);
                    }
                    HotkeyControl::Shutdown => {
                        let _ = UnregisterHotKey(hwnd, HK_FREE);
                        let _ = UnregisterHotKey(hwnd, HK_FULL);
                        return;
                    }
                }
            }

            while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_HOTKEY {
                    match msg.wParam.0 as i32 {
                        HK_FREE => {
                            let _ = tx.send("HOTKEY_FREE".into());
                        }
                        HK_FULL => {
                            let _ = tx.send("HOTKEY_FULL".into());
                        }
                        _ => {}
                    }
                }
            }

            thread::sleep(Duration::from_millis(25));
        }
    });
    ctrl_tx
}

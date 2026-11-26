use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CAPITAL};

pub fn is_caps_lock_on() -> bool {
    unsafe { (GetKeyState(VK_CAPITAL.0 as i32) & 1) != 0 }
}

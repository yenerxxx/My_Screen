use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, Copy)]
pub struct WinRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl WinRect {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

pub fn get_visible_window_rects() -> Vec<WinRect> {
    let mut rects: Vec<WinRect> = Vec::new();
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let scr_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let scr_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let prim_w = GetSystemMetrics(SM_CXSCREEN);
        let prim_h = GetSystemMetrics(SM_CYSCREEN);
        let shell = GetShellWindow();

        struct Ctx<'a> {
            rects: &'a mut Vec<WinRect>,
            vx: i32,
            vy: i32,
            scr_w: i32,
            scr_h: i32,
            prim_w: i32,
            prim_h: i32,
            shell: HWND,
        }

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut Ctx);
            if hwnd == ctx.shell || !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
                return BOOL(1);
            }
            let mut r = RECT::default();
            let got_dwm = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut r as *mut _ as *mut _,
                std::mem::size_of::<RECT>() as u32,
            )
            .is_ok();

            if got_dwm || GetWindowRect(hwnd, &mut r).is_ok() {
                let w = r.right - r.left;
                let h = r.bottom - r.top;
                if w > 30
                    && w <= ctx.scr_w
                    && h > 30
                    && h <= ctx.scr_h
                    && !((w == ctx.prim_w && h == ctx.prim_h) || (w == ctx.scr_w && h == ctx.scr_h))
                {
                    ctx.rects.push(WinRect {
                        x: r.left - ctx.vx,
                        y: r.top - ctx.vy,
                        w,
                        h,
                    });
                }
            }
            BOOL(1)
        }

        let mut ctx = Ctx {
            rects: &mut rects,
            vx,
            vy,
            scr_w,
            scr_h,
            prim_w,
            prim_h,
            shell,
        };
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    }
    rects
}

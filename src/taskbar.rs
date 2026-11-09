use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::MapWindowPoints;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClientRect, GetWindowRect, SetParent, SetWindowLongPtrW, SetWindowPos,
    GWL_EXSTYLE, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOPARENTNOTIFY, WS_VISIBLE,
};

#[derive(Clone, Copy, Debug)]
pub struct Taskbar {
    pub hwnd: HWND,
    pub width: i32,
    pub height: i32,
    pub dpi: u32,
    pub tray_left: Option<i32>,
}

pub fn find() -> Option<Taskbar> {
    unsafe {
        let hwnd = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect).ok()?;
        let dpi = match GetDpiForWindow(hwnd) {
            0 => 96,
            dpi => dpi,
        };
        let tray = FindWindowExW(hwnd, HWND::default(), w!("TrayNotifyWnd"), PCWSTR::null()).ok();
        let tray_left = tray.and_then(|tray| child_rect(hwnd, tray).map(|r| r.left));
        Some(Taskbar {
            hwnd,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
            dpi,
            tray_left,
        })
    }
}

fn child_rect(parent: HWND, child: HWND) -> Option<RECT> {
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(child, &mut rect).ok()?;
        let mut points = [
            POINT {
                x: rect.left,
                y: rect.top,
            },
            POINT {
                x: rect.right,
                y: rect.bottom,
            },
        ];
        MapWindowPoints(None, parent, &mut points);
        Some(RECT {
            left: points[0].x,
            top: points[0].y,
            right: points[1].x,
            bottom: points[1].y,
        })
    }
}

pub fn attach(strip: HWND, taskbar: &Taskbar) -> bool {
    unsafe {
        SetWindowLongPtrW(
            strip,
            GWL_STYLE,
            (WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS).0 as isize,
        );
        SetWindowLongPtrW(
            strip,
            GWL_EXSTYLE,
            (WS_EX_LAYERED | WS_EX_NOPARENTNOTIFY).0 as isize,
        );
        if SetParent(strip, taskbar.hwnd).is_err() {
            return false;
        }
        SetWindowPos(
            strip,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
        .is_ok()
    }
}

pub fn place(
    strip: HWND,
    taskbar: &Taskbar,
    width_dip: i32,
    position: &str,
    offset_dip: i32,
) -> (i32, i32, i32, i32) {
    let scale = taskbar.dpi as f32 / 96.0;
    let width = ((width_dip as f32) * scale).round() as i32;
    let offset = ((offset_dip as f32) * scale).round() as i32;
    let x = match position {
        "left" => offset,
        _ => taskbar
            .tray_left
            .map(|left| left - width - offset)
            .unwrap_or(taskbar.width - width - offset),
    };
    let x = x.clamp(0, (taskbar.width - width).max(0));
    let height = taskbar.height;
    unsafe {
        let _ = SetWindowPos(
            strip,
            HWND_TOP,
            x,
            0,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    (x, 0, width, height)
}

#[allow(dead_code)]
pub fn class_name() -> PCWSTR {
    w!("Shell_TrayWnd")
}

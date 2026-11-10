use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::MapWindowPoints;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, FindWindowExW, FindWindowW, GetClientRect, GetWindowRect, IsWindowVisible,
    SetParent, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, WS_CHILD, WS_CLIPSIBLINGS,
    WS_EX_LAYERED, WS_EX_NOPARENTNOTIFY, WS_VISIBLE,
};

#[derive(Clone, Copy, Debug)]
pub struct Taskbar {
    pub hwnd: HWND,
    pub width: i32,
    pub height: i32,
    pub dpi: u32,
    pub tray_left: Option<i32>,
    tray: Option<HWND>,
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
            tray,
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
    let gap = ((6.0 * scale).round() as i32).max(2);
    let bound = taskbar.tray_left.unwrap_or(taskbar.width);
    let occupied = inflate(occupied_ranges(taskbar, strip), gap, 0, bound.max(0));
    let free = free_gaps(&occupied, bound.max(0));

    let x = if position == "left" {
        let desired = offset.max(0);
        free.iter()
            .filter_map(|&(left, right)| {
                let candidate = left.max(desired);
                (candidate + width <= right).then_some(candidate)
            })
            .min()
            .unwrap_or(desired)
    } else {
        let desired_right = (bound - offset).min(bound);
        free.iter()
            .filter_map(|&(left, right)| {
                let candidate = right.min(desired_right) - width;
                (candidate >= left).then_some(candidate)
            })
            .max()
            .unwrap_or(bound - width - offset)
    };

    let x = x.clamp(0, (bound - width).max(0));
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

struct OccupancyContext {
    taskbar: HWND,
    exclude: HWND,
    tray: Option<HWND>,
    taskbar_width: i32,
    ranges: Vec<(i32, i32)>,
}

unsafe extern "system" fn occupancy_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let context = unsafe { &mut *(lparam.0 as *mut OccupancyContext) };
    if hwnd == context.exclude || context.tray == Some(hwnd) {
        return BOOL(1);
    }
    if !unsafe { IsWindowVisible(hwnd).as_bool() } {
        return BOOL(1);
    }
    let Some(rect) = child_rect(context.taskbar, hwnd) else {
        return BOOL(1);
    };
    let width = rect.right - rect.left;
    if width <= 0 || width >= context.taskbar_width - 4 {
        return BOOL(1);
    }
    context.ranges.push((rect.left, rect.right));
    BOOL(1)
}

fn occupied_ranges(taskbar: &Taskbar, strip: HWND) -> Vec<(i32, i32)> {
    let mut context = OccupancyContext {
        taskbar: taskbar.hwnd,
        exclude: strip,
        tray: taskbar.tray,
        taskbar_width: taskbar.width,
        ranges: Vec::new(),
    };
    unsafe {
        let _ = EnumChildWindows(
            taskbar.hwnd,
            Some(occupancy_proc),
            LPARAM(&mut context as *mut _ as isize),
        );
    }
    let mut ranges: Vec<(i32, i32)> = context
        .ranges
        .into_iter()
        .filter(|(left, right)| right > left)
        .collect();
    ranges.sort_unstable();
    merge(ranges)
}

fn inflate(ranges: Vec<(i32, i32)>, gap: i32, min: i32, max: i32) -> Vec<(i32, i32)> {
    let expanded = ranges
        .into_iter()
        .filter_map(|(left, right)| {
            let left = (left - gap).max(min);
            let right = (right + gap).min(max);
            (right > left).then_some((left, right))
        })
        .collect();
    merge(expanded)
}

fn merge(mut ranges: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    ranges.sort_unstable();
    let mut merged: Vec<(i32, i32)> = Vec::new();
    for (left, right) in ranges {
        if let Some(last) = merged.last_mut() {
            if left <= last.1 {
                last.1 = last.1.max(right);
                continue;
            }
        }
        merged.push((left, right));
    }
    merged
}

fn free_gaps(ranges: &[(i32, i32)], bound: i32) -> Vec<(i32, i32)> {
    let mut gaps = Vec::new();
    let mut cursor = 0;
    for &(left, right) in ranges {
        if left > cursor {
            gaps.push((cursor, left.min(bound)));
        }
        cursor = cursor.max(right);
        if cursor >= bound {
            break;
        }
    }
    if cursor < bound {
        gaps.push((cursor, bound));
    }
    gaps
}

#[allow(dead_code)]
pub fn class_name() -> PCWSTR {
    w!("Shell_TrayWnd")
}

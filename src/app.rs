use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;
use std::time::Instant;

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, GetMonitorInfoW,
    InvalidateRect, MonitorFromWindow, SetBkColor, SetTextColor, ValidateRect, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, FF_DONTCARE, FW_NORMAL, HBRUSH, HDC,
    HFONT, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ, RRF_RT_REG_DWORD,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE,
    VK_RETURN,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture;
use crate::config::{Config, TodoItem};
use crate::logging;
use crate::metrics::{Sampler, Snapshot};
use crate::pomodoro::{self, Pomodoro};
use crate::render::Renderer;
use crate::taskbar::{self, Taskbar};
use crate::ui;

const HOST_CLASS: PCWSTR = w!("TaskbarStatsHost");
const STRIP_CLASS: PCWSTR = w!("TaskbarStatsStrip");
const POPUP_CLASS: PCWSTR = w!("TaskbarStatsPopup");
const TODO_CLASS: PCWSTR = w!("TaskbarStatsTodo");

const TIMER_UPDATE: usize = 1;
const TIMER_ATTACH: usize = 2;
const TIMER_HOVER: usize = 3;
const TIMER_HIDE: usize = 4;
const TIMER_ANIM: usize = 5;
const TIMER_POMODORO: usize = 6;
const TIMER_DISPLAY: usize = 7;
const WM_TRAY: u32 = WM_APP + 1;
const NIN_BALLOONUSERCLICK: u32 = 0x0400 + 5;
const TRAY_ID: u32 = 1;
const TODO_EDIT_ID: usize = 100;
const WM_TODO_ADD: u32 = WM_APP + 2;
const WM_TODO_CANCEL: u32 = WM_APP + 3;
const WM_DISPLAY_SYNC: u32 = WM_APP + 4;
const PANEL_SHOW_MS: u64 = 140;
const PANEL_HIDE_MS: u64 = 110;

const MUTEX_NAME: PCWSTR = w!("Local\\TaskbarStats.SingleInstance");
const WM_MOUSELEAVE_MSG: u32 = 0x02A3;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static TODO_EDIT_PROC: Cell<isize> = const { Cell::new(0) };
}

pub fn run() -> Result<()> {
    unsafe {
        let _mutex = match CreateMutexW(None, BOOL(1), MUTEX_NAME) {
            Ok(handle) => handle,
            Err(error) => {
                logging::write(&format!("CreateMutexW failed: {error}"));
                return Ok(());
            }
        };
        if GetLastError().0 == 183 {
            logging::write("another instance is already running");
            return Ok(());
        }

        let module = GetModuleHandleW(None)?;
        let hinstance = HINSTANCE(module.0);
        register_classes(hinstance);

        let hidden = create_window(WS_EX_TOOLWINDOW, HOST_CLASS, WS_OVERLAPPED, None, hinstance)?;
        let strip = create_window(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            STRIP_CLASS,
            WS_POPUP,
            None,
            hinstance,
        )?;
        let popup = create_window(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            POPUP_CLASS,
            WS_POPUP,
            None,
            hinstance,
        )?;

        let (config, config_path) = Config::load();
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
        let app = App {
            hinstance,
            hidden,
            strip,
            popup,
            pomodoro: Pomodoro::new(&config),
            config,
            config_path,
            taskbar: taskbar::find(),
            sampler: Sampler::new(),
            snapshot: Snapshot::default(),
            strip_renderer: Renderer::new()?,
            popup_renderer: Renderer::new()?,
            strip_w_px: 0,
            strip_h_px: 0,
            strip_dpi: 96,
            popup_visible: false,
            pinned: false,
            panel_tab: ui::PanelTab::Overview,
            panel_hover: None,
            hover_timer: false,
            hide_timer: false,
            taskbar_created,
            light: false,
            backdrop: None,
            backdrop_w: 0,
            backdrop_h: 0,
            popup_alpha: 1.0,
            fade: None,
            fade_timer: false,
            drag: None,
            tray_icon: None,
            tray_added: false,
            todo_dialog: None,
            todo_edit: HWND::default(),
            todo_font: HFONT::default(),
            todo_brush: HBRUSH::default(),
            todo_renderer: Renderer::new()?,
            todo_hover: None,
            todo_tracking: false,
            todo_dpi: 96,
            todo_page: 0,
        };

        APP.with(|cell| *cell.borrow_mut() = Some(app));
        with_app(|app| app.start());

        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0);
            if result.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| {
        cell.try_borrow_mut()
            .ok()
            .and_then(|mut guard| guard.as_mut().map(f))
    })
}

struct App {
    hinstance: HINSTANCE,
    hidden: HWND,
    strip: HWND,
    popup: HWND,
    config: Config,
    config_path: PathBuf,
    taskbar: Option<Taskbar>,
    sampler: Sampler,
    snapshot: Snapshot,
    strip_renderer: Renderer,
    popup_renderer: Renderer,
    strip_w_px: i32,
    strip_h_px: i32,
    strip_dpi: u32,
    pomodoro: Pomodoro,
    popup_visible: bool,
    pinned: bool,
    panel_tab: ui::PanelTab,
    panel_hover: Option<ui::PanelAction>,
    hover_timer: bool,
    hide_timer: bool,
    taskbar_created: u32,
    light: bool,
    backdrop: Option<Vec<u8>>,
    backdrop_w: u32,
    backdrop_h: u32,
    popup_alpha: f32,
    fade: Option<Fade>,
    fade_timer: bool,
    drag: Option<StripDrag>,
    tray_icon: Option<HICON>,
    tray_added: bool,
    todo_dialog: Option<HWND>,
    todo_edit: HWND,
    todo_font: HFONT,
    todo_brush: HBRUSH,
    todo_renderer: Renderer,
    todo_hover: Option<ui::TodoDialogButton>,
    todo_tracking: bool,
    todo_dpi: u32,
    todo_page: usize,
}

struct Fade {
    start: Instant,
    from: f32,
    to: f32,
    duration_ms: u64,
    hiding: bool,
}

struct StripDrag {
    cursor_start: i32,
    strip_start: i32,
    moved: bool,
}

impl App {
    fn start(&mut self) {
        self.refresh_theme();
        if !sync_startup(self.config.launch_at_startup) {
            logging::write("could not update startup registration");
        }
        self.tray_icon = load_app_icon(self.hinstance);
        self.add_tray_icon();
        self.taskbar = taskbar::find();
        if let Some(tb) = self.taskbar {
            if taskbar::attach(self.strip, &tb) {
                self.position_strip();
                self.show_strip();
                self.sample_and_render();
            } else {
                logging::write("failed to attach strip to taskbar");
            }
        } else {
            logging::write("taskbar not found; waiting");
            unsafe {
                SetTimer(self.hidden, TIMER_ATTACH, 2000, None);
            }
        }
        unsafe {
            SetTimer(self.hidden, TIMER_UPDATE, self.config.update_ms, None);
            SetTimer(self.hidden, TIMER_POMODORO, 1000, None);
        }
    }

    fn on_host(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == self.taskbar_created {
            self.reattach();
            self.tray_added = false;
            self.add_tray_icon();
            return LRESULT(0);
        }
        if msg == WM_TRAY {
            self.on_tray(lparam);
            return LRESULT(0);
        }
        if msg == WM_DISPLAY_SYNC {
            self.sync_taskbar();
            self.refresh_layout();
            self.render_strip();
            return LRESULT(0);
        }
        match msg {
            WM_TIMER => {
                match wparam.0 {
                    TIMER_UPDATE => self.sample_and_render(),
                    TIMER_ATTACH => self.reattach(),
                    TIMER_ANIM => self.tick_fade(),
                    TIMER_POMODORO => self.tick_pomodoro(),
                    TIMER_DISPLAY => {
                        unsafe {
                            let _ = KillTimer(self.hidden, TIMER_DISPLAY);
                        }
                        if self.sync_taskbar() {
                            self.refresh_layout();
                            self.render_strip();
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DPICHANGED | WM_DPICHANGED_AFTERPARENT => {
                self.sync_taskbar();
                self.refresh_layout();
                self.render_strip();
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                unsafe {
                    let _ = PostMessageW(self.hidden, WM_DISPLAY_SYNC, WPARAM(0), LPARAM(0));
                }
                self.schedule_display_retry();
                LRESULT(0)
            }
            WM_SETTINGCHANGE | WM_THEMECHANGED => {
                self.refresh_theme();
                if wparam.0 as u32 == SPI_SETWORKAREA.0 {
                    unsafe {
                        let _ = PostMessageW(self.hidden, WM_DISPLAY_SYNC, WPARAM(0), LPARAM(0));
                    }
                    self.schedule_display_retry();
                }
                self.render_strip();
                if self.popup_visible {
                    self.render_popup();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                self.remove_tray_icon();
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn on_strip(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_LBUTTONDOWN => {
                let Some(tb) = self.taskbar else {
                    return LRESULT(0);
                };
                let mut point = POINT::default();
                let mut rect = RECT::default();
                let mut taskbar_rect = RECT::default();
                unsafe {
                    let _ = GetCursorPos(&mut point);
                    let _ = GetWindowRect(self.strip, &mut rect);
                    let _ = GetWindowRect(tb.hwnd, &mut taskbar_rect);
                    let _ = SetCapture(hwnd);
                }
                if self.hover_timer {
                    unsafe {
                        let _ = KillTimer(self.strip, TIMER_HOVER);
                    }
                    self.hover_timer = false;
                }
                self.drag = Some(StripDrag {
                    cursor_start: point.x,
                    strip_start: rect.left - taskbar_rect.left,
                    moved: false,
                });
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if self.update_strip_drag() {
                    return LRESULT(0);
                }
                self.track_leave(hwnd);
                if !self.popup_visible && !self.pinned && !self.hover_timer {
                    unsafe {
                        SetTimer(self.strip, TIMER_HOVER, self.config.hover_delay_ms, None);
                    }
                    self.hover_timer = true;
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                self.drag = None;
                LRESULT(0)
            }
            WM_MOUSELEAVE_MSG => {
                if self.hover_timer {
                    unsafe {
                        let _ = KillTimer(self.strip, TIMER_HOVER);
                    }
                    self.hover_timer = false;
                }
                if !self.pinned && self.popup_visible {
                    self.start_hide_timer();
                }
                LRESULT(0)
            }
            WM_TIMER => {
                match wparam.0 {
                    TIMER_HOVER => {
                        unsafe {
                            let _ = KillTimer(self.strip, TIMER_HOVER);
                        }
                        self.hover_timer = false;
                        if self.cursor_over(self.strip) {
                            self.show_popup(false);
                        }
                    }
                    TIMER_HIDE => {
                        self.hide_timer = false;
                        if !self.pinned
                            && !self.cursor_over(self.strip)
                            && !self.cursor_over(self.popup)
                        {
                            self.hide_popup();
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let drag = self.drag.take();
                unsafe {
                    let _ = ReleaseCapture();
                }
                if drag.is_some_and(|drag| drag.moved) {
                    self.finish_strip_drag();
                } else if self.fade.as_ref().is_some_and(|fade| fade.hiding) {
                    self.cancel_fade();
                    self.popup_alpha = 1.0;
                    self.render_popup();
                } else if self.popup_visible {
                    self.hide_popup();
                } else {
                    self.show_popup(true);
                }
                LRESULT(0)
            }
            WM_MBUTTONUP => {
                self.panel_tab = ui::PanelTab::Settings;
                self.show_popup(true);
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                self.show_menu();
                LRESULT(0)
            }
            WM_PAINT => {
                let _ = unsafe { ValidateRect(hwnd, None) };
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DPICHANGED | WM_DPICHANGED_AFTERPARENT => {
                unsafe {
                    let _ = PostMessageW(self.hidden, WM_DISPLAY_SYNC, WPARAM(0), LPARAM(0));
                }
                self.schedule_display_retry();
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn on_popup(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_MOUSEMOVE => {
                if self.fade.as_ref().is_some_and(|fade| fade.hiding) {
                    self.cancel_fade();
                    self.popup_alpha = 1.0;
                    self.render_popup();
                }
                self.track_leave(hwnd);
                if self.hide_timer {
                    unsafe {
                        let _ = KillTimer(self.strip, TIMER_HIDE);
                    }
                    self.hide_timer = false;
                }
                let (x, y) = self.panel_point(lparam);
                let hover = ui::action_at(
                    self.panel_tab,
                    x,
                    y,
                    self.sampler.gpu_names().len(),
                    &ui::Layout::new(self.config.font_scale),
                );
                if hover != self.panel_hover {
                    self.panel_hover = hover;
                    self.render_popup();
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE_MSG => {
                if self.panel_hover.is_some() {
                    self.panel_hover = None;
                    self.render_popup();
                }
                if !self.pinned {
                    self.start_hide_timer();
                }
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_LBUTTONUP => {
                let (x, y) = self.panel_point(lparam);
                if let Some(action) = ui::action_at(
                    self.panel_tab,
                    x,
                    y,
                    self.sampler.gpu_names().len(),
                    &ui::Layout::new(self.config.font_scale),
                ) {
                    self.handle_panel_action(action);
                } else if self.pinned {
                    self.hide_popup();
                }
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                self.hide_popup();
                LRESULT(0)
            }
            WM_PAINT => {
                let _ = unsafe { ValidateRect(hwnd, None) };
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_DPICHANGED | WM_DPICHANGED_AFTERPARENT => {
                unsafe {
                    let _ = PostMessageW(self.hidden, WM_DISPLAY_SYNC, WPARAM(0), LPARAM(0));
                }
                self.schedule_display_retry();
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn panel_point(&self, lparam: LPARAM) -> (f32, f32) {
        let (x, y) = client_point(lparam);
        let dpi = self.popup_dpi().max(96) as f32;
        let scale = 96.0 / dpi;
        (x * scale - ui::PANEL_MARGIN, y * scale)
    }

    fn filter_snapshot(&self, snapshot: &mut Snapshot) {
        if !self.config.widgets.iter().any(|widget| widget == "gpu") {
            snapshot.gpus.clear();
        } else if !self.config.gpu_visible.is_empty() {
            snapshot.gpus.retain(|gpu| {
                self.config
                    .gpu_visible
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&gpu.name))
            });
        }
    }

    fn refresh_snapshot(&mut self) {
        let mut snapshot = self.sampler.sample();
        self.filter_snapshot(&mut snapshot);
        self.snapshot = snapshot;
    }

    fn visible_gpu_count(&self) -> usize {
        if !self.config.widgets.iter().any(|widget| widget == "gpu") {
            return 0;
        }
        let names = self.sampler.gpu_names();
        if self.config.gpu_visible.is_empty() {
            names.len()
        } else {
            names
                .iter()
                .filter(|name| self.config.gpu_enabled(name))
                .count()
        }
    }

    fn capture_backdrop(&mut self, x: i32, y: i32, width: i32, height: i32, dpi: u32) {
        self.backdrop = None;
        if self.config.style != "blur" || width <= 0 || height <= 0 {
            return;
        }
        let margin = (ui::PANEL_MARGIN * dpi as f32 / 96.0).round() as i32;
        let content_width = width - 2 * margin;
        if content_width <= 0 {
            return;
        }
        let Some(mut frame) = capture::screen_bgra(x + margin, y, content_width, height) else {
            return;
        };
        let blur_radius = ((14.0 * dpi as f32 / 96.0).round() as usize).max(1);
        capture::blur(
            &mut frame,
            content_width as usize,
            height as usize,
            blur_radius,
        );
        let corner_radius = ((18.0 * dpi as f32 / 96.0).round() as usize)
            .min(content_width as usize / 2)
            .min(height as usize / 2)
            .max(1);
        capture::round_corners(
            &mut frame,
            content_width as usize,
            height as usize,
            corner_radius,
        );
        self.backdrop = Some(frame);
        self.backdrop_w = content_width as u32;
        self.backdrop_h = height as u32;
    }

    fn sample_and_render(&mut self) {
        let changed = self.sync_taskbar();
        self.refresh_snapshot();
        if changed {
            self.refresh_layout();
        }
        self.render_strip();
        if self.popup_visible {
            self.render_popup();
        }
    }

    fn sync_taskbar(&mut self) -> bool {
        let found = taskbar::find();
        let changed = match (self.taskbar, found) {
            (Some(current), Some(found)) => {
                current.hwnd != found.hwnd
                    || current.dpi != found.dpi
                    || current.width != found.width
                    || current.height != found.height
            }
            (None, None) => false,
            _ => true,
        };
        self.taskbar = found;
        if changed {
            if let Some(tb) = self.taskbar {
                logging::write(&format!(
                    "taskbar changed: hwnd={:?} w={} h={} dpi={}",
                    tb.hwnd, tb.width, tb.height, tb.dpi
                ));
            } else {
                logging::write("taskbar lost");
            }
        }
        changed
    }

    fn schedule_display_retry(&self) {
        unsafe {
            SetTimer(self.hidden, TIMER_DISPLAY, 700, None);
        }
    }

    fn refresh_layout(&mut self) {
        self.backdrop = None;
        self.close_todo_dialog();
        let Some(tb) = self.taskbar else {
            self.reattach();
            return;
        };
        let attached = unsafe {
            IsWindow(self.strip).as_bool()
                && GetParent(self.strip)
                    .map(|parent| parent == tb.hwnd)
                    .unwrap_or(false)
        };
        if !attached {
            self.reattach();
            return;
        }
        self.position_strip();
        if self.popup_visible {
            self.show_popup(self.pinned);
        }
    }

    fn render_strip(&mut self) {
        if self.strip_w_px <= 0 || self.strip_h_px <= 0 {
            return;
        }
        if !unsafe { IsWindow(self.strip).as_bool() } {
            return;
        }
        if self
            .strip_renderer
            .resize(self.strip_w_px, self.strip_h_px, self.strip_dpi)
            .is_err()
        {
            return;
        }
        let width = self.strip_w_px as f32 * 96.0 / self.strip_dpi as f32;
        let height = self.strip_h_px as f32 * 96.0 / self.strip_dpi as f32;
        self.strip_renderer
            .set_font_scale((self.config.font_scale as f32).min(1.0));
        if let Err(error) = ui::draw_strip(
            &self.strip_renderer,
            self.strip,
            &self.snapshot,
            &self.config,
            self.light,
            width,
            height,
        ) {
            logging::write(&format!("strip render failed: {error}"));
        }
    }

    fn render_popup(&mut self) {
        let dpi = self.popup_dpi().max(96);
        let width = ((ui::PANEL_WIDTH + 2.0 * ui::PANEL_MARGIN) * dpi as f32 / 96.0).round() as i32;
        let height = (ui::panel_height(self.panel_tab, &ui::Layout::new(self.config.font_scale))
            * dpi as f32
            / 96.0)
            .round() as i32;
        if self.popup_renderer.resize(width, height, dpi).is_err() {
            return;
        }
        self.popup_renderer.set_alpha(self.popup_alpha);
        self.popup_renderer
            .set_font_scale(self.config.font_scale as f32);
        let gpu_names = self.sampler.gpu_names();
        if let Err(error) = ui::draw_panel(
            &self.popup_renderer,
            self.popup,
            &self.snapshot,
            &self.config,
            self.light,
            ui::PANEL_WIDTH,
            ui::panel_height(self.panel_tab, &ui::Layout::new(self.config.font_scale)),
            self.panel_tab,
            self.panel_hover,
            &self.pomodoro,
            self.todo_page,
            &gpu_names,
            self.backdrop.as_deref(),
            self.backdrop_w,
            self.backdrop_h,
        ) {
            logging::write(&format!("popup render failed: {error}"));
        }
    }

    fn position_strip(&mut self) {
        let Some(tb) = self.taskbar else { return };
        if !unsafe { IsWindow(self.strip).as_bool() } {
            return;
        }
        let width_dip = ui::strip_width_dip(&self.config, self.visible_gpu_count());
        let (x, _, width, height) = taskbar::place(
            self.strip,
            &tb,
            width_dip,
            &self.config.position,
            self.config.offset,
        );
        self.strip_w_px = width;
        self.strip_h_px = height;
        self.strip_dpi = tb.dpi;
        logging::write(&format!(
            "strip placed: x={} y=0 w={} h={} dpi={} taskbar_w={}",
            x, width, height, tb.dpi, tb.width
        ));
    }

    fn show_strip(&self) {
        unsafe {
            let _ = ShowWindow(self.strip, SW_SHOWNOACTIVATE);
        }
    }

    fn popup_dpi(&self) -> u32 {
        for window in [self.strip, self.popup] {
            let scale = unsafe { GetDpiForWindow(window) };
            if scale > 0 {
                return scale;
            }
        }
        self.taskbar.map(|tb| tb.dpi).unwrap_or(96)
    }

    fn show_popup(&mut self, pin: bool) {
        self.sync_taskbar();
        let Some(tb) = self.taskbar else { return };
        let was_visible = self.popup_visible;
        self.cancel_fade();
        if !was_visible {
            self.popup_alpha = 0.0;
        }
        self.pinned = pin;
        self.panel_hover = None;
        if self.hover_timer {
            unsafe {
                let _ = KillTimer(self.strip, TIMER_HOVER);
            }
            self.hover_timer = false;
        }
        if self.hide_timer {
            unsafe {
                let _ = KillTimer(self.strip, TIMER_HIDE);
            }
            self.hide_timer = false;
        }
        let dpi = self.popup_dpi().max(96);
        let width = ((ui::PANEL_WIDTH + 2.0 * ui::PANEL_MARGIN) * dpi as f32 / 96.0).round() as i32;
        let height = (ui::panel_height(self.panel_tab, &ui::Layout::new(self.config.font_scale))
            * dpi as f32
            / 96.0)
            .round() as i32;

        let mut strip_rect = RECT::default();
        let mut taskbar_rect = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            let _ = GetWindowRect(self.strip, &mut strip_rect);
            let _ = GetWindowRect(tb.hwnd, &mut taskbar_rect);
            let monitor_handle = MonitorFromWindow(tb.hwnd, MONITOR_DEFAULTTONEAREST);
            let _ = GetMonitorInfoW(monitor_handle, &mut monitor);
        }
        let work = monitor.rcWork;
        let gap = (6.0 * dpi as f32 / 96.0).round() as i32;
        let center = (strip_rect.left + strip_rect.right) / 2;
        let x = (center - width / 2).clamp(work.left, (work.right - width).max(work.left));
        let midpoint = work.top + (work.bottom - work.top) / 2;
        let y = if taskbar_rect.top > midpoint {
            taskbar_rect.top - height - gap
        } else {
            taskbar_rect.bottom + gap
        };
        let needs_capture =
            self.config.style == "blur" && (!was_visible || self.backdrop.is_none());
        if needs_capture {
            if was_visible {
                unsafe {
                    let _ = ShowWindow(self.popup, SW_HIDE);
                }
            }
            self.capture_backdrop(x, y, width, height, dpi);
        }
        unsafe {
            let _ = SetWindowPos(
                self.popup,
                HWND_TOPMOST,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = ShowWindow(self.popup, SW_SHOWNOACTIVATE);
        }
        self.popup_visible = true;
        self.render_popup();
        if self.popup_alpha < 1.0 {
            self.start_fade(1.0, PANEL_SHOW_MS, false);
        }
    }

    fn hide_popup(&mut self) {
        if !self.popup_visible {
            return;
        }
        if self.fade.as_ref().is_some_and(|fade| fade.hiding) {
            return;
        }
        if self.hide_timer {
            unsafe {
                let _ = KillTimer(self.strip, TIMER_HIDE);
            }
            self.hide_timer = false;
        }
        self.pinned = false;
        self.start_fade(0.0, PANEL_HIDE_MS, true);
    }

    fn finish_hide_popup(&mut self) {
        if self.fade_timer {
            unsafe {
                let _ = KillTimer(self.hidden, TIMER_ANIM);
            }
            self.fade_timer = false;
        }
        self.fade = None;
        self.popup_visible = false;
        self.pinned = false;
        self.panel_tab = ui::PanelTab::Overview;
        self.panel_hover = None;
        self.popup_alpha = 1.0;
        unsafe {
            let _ = ShowWindow(self.popup, SW_HIDE);
        }
    }

    fn cancel_fade(&mut self) {
        if self.fade_timer {
            unsafe {
                let _ = KillTimer(self.hidden, TIMER_ANIM);
            }
            self.fade_timer = false;
        }
        self.fade = None;
    }

    fn start_fade(&mut self, to: f32, duration_ms: u64, hiding: bool) {
        if !animations_enabled() || duration_ms == 0 {
            self.popup_alpha = to;
            if hiding {
                self.finish_hide_popup();
            } else {
                self.popup_renderer.set_alpha(self.popup_alpha);
                self.render_popup();
            }
            return;
        }
        self.fade = Some(Fade {
            start: Instant::now(),
            from: self.popup_alpha,
            to,
            duration_ms,
            hiding,
        });
        if !self.fade_timer {
            unsafe {
                SetTimer(self.hidden, TIMER_ANIM, 15, None);
            }
            self.fade_timer = true;
        }
    }

    fn tick_fade(&mut self) {
        let Some(fade) = self.fade.as_ref() else {
            return;
        };
        let elapsed = fade.start.elapsed().as_secs_f32() * 1000.0;
        let t = (elapsed / fade.duration_ms as f32).clamp(0.0, 1.0);
        let eased = if fade.hiding {
            t * t
        } else {
            1.0 - (1.0 - t).powi(3)
        };
        self.popup_alpha = fade.from + (fade.to - fade.from) * eased;
        if self.popup_visible {
            self.popup_renderer.set_alpha(self.popup_alpha);
            let _ = self.popup_renderer.present(self.popup);
        }
        if t >= 1.0 {
            let hiding = fade.hiding;
            self.popup_alpha = fade.to;
            self.cancel_fade();
            if hiding {
                self.finish_hide_popup();
            }
        }
    }

    fn tick_pomodoro(&mut self) {
        if let Some(event) = self.pomodoro.tick(&self.config) {
            match event {
                pomodoro::Event::FocusComplete => self.notify(
                    "Focus complete",
                    &format!(
                        "Nice work. Take a {} minute break.",
                        self.config.pomodoro_break_min
                    ),
                ),
                pomodoro::Event::BreakComplete => self.notify(
                    "Break complete",
                    &format!(
                        "Ready for another {} minute focus session?",
                        self.config.pomodoro_focus_min
                    ),
                ),
            }
        }
        if self.popup_visible && self.panel_tab == ui::PanelTab::Pomodoro {
            self.render_popup();
        }
    }

    fn on_tray(&mut self, lparam: LPARAM) {
        let event = lparam.0 as u32 & 0xFFFF;
        match event {
            WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                self.panel_tab = ui::PanelTab::Overview;
                self.show_popup(true);
            }
            NIN_BALLOONUSERCLICK => {
                self.panel_tab = ui::PanelTab::Pomodoro;
                self.show_popup(true);
            }
            WM_RBUTTONUP => self.show_menu(),
            _ => {}
        }
    }

    fn add_tray_icon(&mut self) {
        if self.tray_added {
            return;
        }
        let Some(icon) = self.tray_icon else {
            return;
        };
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hidden,
            uID: TRAY_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: WM_TRAY,
            hIcon: icon,
            ..Default::default()
        };
        fill_wide("Taskbar Stats", &mut data.szTip);
        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
            data.Anonymous.uVersion = 4;
            let _ = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) };
            self.tray_added = true;
            logging::write("tray icon added");
        } else {
            logging::write("tray icon add failed");
        }
    }

    fn remove_tray_icon(&mut self) {
        if !self.tray_added {
            return;
        }
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hidden,
            uID: TRAY_ID,
            ..Default::default()
        };
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        self.tray_added = false;
    }

    fn notify(&self, title: &str, body: &str) {
        if !self.tray_added {
            return;
        }
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hidden,
            uID: TRAY_ID,
            uFlags: NIF_INFO,
            dwInfoFlags: NIIF_INFO,
            ..Default::default()
        };
        fill_wide(title, &mut data.szInfoTitle);
        fill_wide(body, &mut data.szInfo);
        data.Anonymous.uTimeout = 10_000;
        logging::write(&format!("notification: {title} - {body}"));
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }

    fn open_todo_dialog(&mut self) {
        if let Some(dialog) = self.todo_dialog {
            unsafe {
                let _ = SetForegroundWindow(dialog);
                let _ = SetFocus(self.todo_edit);
            }
            return;
        }
        self.pinned = true;
        let dpi = self.taskbar.map(|tb| tb.dpi).unwrap_or(96).max(96);
        let scale = dpi as f32 / 96.0;
        let width = (ui::TODO_DIALOG_W * scale).round() as i32;
        let height = (ui::TODO_DIALOG_H * scale).round() as i32;
        let mut popup_rect = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.popup, &mut popup_rect);
        }
        let x = (popup_rect.left + popup_rect.right - width) / 2;
        let y = (popup_rect.top + popup_rect.bottom - height) / 2;
        let dialog = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                TODO_CLASS,
                w!("New task"),
                WS_POPUP | WS_CLIPCHILDREN,
                x,
                y,
                width,
                height,
                self.hidden,
                HMENU::default(),
                self.hinstance,
                None,
            )
        };
        let Ok(dialog) = dialog else {
            logging::write("could not create todo dialog");
            return;
        };
        let preference = DWMWCP_ROUND;
        let border = DWMWA_COLOR_NONE;
        unsafe {
            let _ = DwmSetWindowAttribute(
                dialog,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &preference as *const _ as *const c_void,
                size_of::<i32>() as u32,
            );
            let _ = DwmSetWindowAttribute(
                dialog,
                DWMWA_BORDER_COLOR,
                &border as *const _ as *const c_void,
                size_of::<u32>() as u32,
            );
        }
        let (edit_x, edit_y, edit_w, edit_h) = ui::todo_dialog_edit_rect();
        let edit = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                (edit_x * scale).round() as i32,
                (edit_y * scale).round() as i32,
                (edit_w * scale).round() as i32,
                (edit_h * scale).round() as i32,
                dialog,
                HMENU(TODO_EDIT_ID as *mut c_void),
                self.hinstance,
                None,
            )
        };
        let Ok(edit) = edit else {
            unsafe {
                let _ = DestroyWindow(dialog);
            }
            logging::write("could not create todo input");
            return;
        };
        let colors = ui::todo_dialog_colors(&self.config, self.light);
        let font = unsafe {
            CreateFontW(
                -(14.0 * scale * self.config.font_scale as f32).round() as i32,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                w!("Segoe UI"),
            )
        };
        let brush = unsafe { CreateSolidBrush(COLORREF(colors.field)) };
        self.todo_font = font;
        self.todo_brush = brush;
        self.todo_dpi = dpi;
        self.todo_hover = None;
        self.todo_tracking = false;
        self.todo_dialog = Some(dialog);
        self.todo_edit = edit;
        unsafe {
            SendMessageW(edit, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            let previous =
                SetWindowLongPtrW(edit, GWLP_WNDPROC, todo_edit_proc as *const () as isize);
            TODO_EDIT_PROC.with(|cell| cell.set(previous));
            let _ = ShowWindow(dialog, SW_SHOW);
            let _ = SetForegroundWindow(dialog);
            let _ = SetFocus(edit);
        }
    }

    fn commit_todo_dialog(&mut self) {
        if self.todo_dialog.is_none() {
            return;
        }
        let mut buffer = [0u16; 256];
        let length = unsafe { GetWindowTextW(self.todo_edit, &mut buffer) } as usize;
        let text = String::from_utf16_lossy(&buffer[..length.min(buffer.len())]);
        let text = text.trim();
        if !text.is_empty() {
            self.config.todos.push(TodoItem {
                text: text.to_string(),
                done: false,
            });
            self.config.validate();
            self.todo_page = self
                .config
                .todos
                .len()
                .saturating_sub(1)
                .div_euclid(ui::TODO_ROWS);
            if let Err(error) = self.config.save(&self.config_path) {
                logging::write(&format!("config save failed: {error}"));
            }
            self.render_popup();
        }
        self.close_todo_dialog();
    }

    fn close_todo_dialog(&mut self) {
        if let Some(dialog) = self.todo_dialog.take() {
            unsafe {
                let _ = DestroyWindow(dialog);
            }
        }
    }

    fn paint_todo(&mut self, hwnd: HWND) {
        let mut paint = PAINTSTRUCT::default();
        unsafe {
            let hdc = BeginPaint(hwnd, &mut paint);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            if self
                .todo_renderer
                .resize(rect.right.max(1), rect.bottom.max(1), self.todo_dpi)
                .is_ok()
            {
                self.todo_renderer
                    .set_font_scale(self.config.font_scale as f32);
                if self.todo_renderer.begin().is_ok() {
                    ui::draw_todo_dialog(
                        &self.todo_renderer,
                        &self.config,
                        self.light,
                        self.todo_hover,
                    );
                    let _ = self.todo_renderer.end_draw();
                    self.todo_renderer.blit(hdc);
                }
            }
            let _ = EndPaint(hwnd, &paint);
        }
    }

    fn update_todo_hover(&mut self, hwnd: HWND, lparam: LPARAM) {
        let x = (lparam.0 & 0xFFFF) as i16 as f32 * 96.0 / self.todo_dpi as f32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 * 96.0 / self.todo_dpi as f32;
        let hover = ui::todo_dialog_hit(x, y);
        if hover != self.todo_hover {
            self.todo_hover = hover;
            unsafe {
                let _ = InvalidateRect(hwnd, None, false);
            }
        }
    }

    fn on_todo(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_PAINT => {
                self.paint_todo(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_CTLCOLOREDIT => {
                let colors = ui::todo_dialog_colors(&self.config, self.light);
                unsafe {
                    let hdc = HDC(wparam.0 as *mut c_void);
                    SetBkColor(hdc, COLORREF(colors.field));
                    SetTextColor(hdc, COLORREF(colors.text));
                }
                LRESULT(self.todo_brush.0 as isize)
            }
            WM_MOUSEMOVE => {
                self.update_todo_hover(hwnd, lparam);
                if !self.todo_tracking {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        ..Default::default()
                    };
                    unsafe {
                        let _ = TrackMouseEvent(&mut track);
                    }
                    self.todo_tracking = true;
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE_MSG => {
                self.todo_tracking = false;
                if self.todo_hover.take().is_some() {
                    unsafe {
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                self.update_todo_hover(hwnd, lparam);
                match self.todo_hover {
                    Some(ui::TodoDialogButton::Add) => self.commit_todo_dialog(),
                    Some(ui::TodoDialogButton::Cancel) => self.close_todo_dialog(),
                    None => {}
                }
                LRESULT(0)
            }
            WM_TODO_ADD => {
                self.commit_todo_dialog();
                LRESULT(0)
            }
            WM_TODO_CANCEL | WM_CLOSE => {
                self.close_todo_dialog();
                LRESULT(0)
            }
            WM_KEYDOWN if wparam.0 as u16 == VK_ESCAPE.0 => {
                self.close_todo_dialog();
                LRESULT(0)
            }
            WM_DESTROY => {
                if self.todo_dialog == Some(hwnd) {
                    self.todo_dialog = None;
                    self.todo_edit = HWND::default();
                    self.todo_hover = None;
                    self.todo_tracking = false;
                    unsafe {
                        if !self.todo_font.is_invalid() {
                            let _ = DeleteObject(HGDIOBJ(self.todo_font.0));
                        }
                        if !self.todo_brush.is_invalid() {
                            let _ = DeleteObject(HGDIOBJ(self.todo_brush.0));
                        }
                    }
                    self.todo_font = HFONT::default();
                    self.todo_brush = HBRUSH::default();
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn start_hide_timer(&mut self) {
        if self.hide_timer {
            return;
        }
        unsafe {
            SetTimer(self.strip, TIMER_HIDE, 260, None);
        }
        self.hide_timer = true;
    }

    fn update_strip_drag(&mut self) -> bool {
        let Some(drag) = self.drag.as_mut() else {
            return false;
        };
        let Some(tb) = self.taskbar else {
            return true;
        };
        let mut point = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut point);
        }
        let dx = point.x - drag.cursor_start;
        let mut started = false;
        if !drag.moved && dx.abs() > 4 {
            drag.moved = true;
            started = true;
        }
        if !drag.moved {
            return true;
        }
        let strip_start = drag.strip_start;
        if started {
            self.pinned = false;
            if self.popup_visible {
                self.hide_popup();
            }
        }
        let width = self.strip_w_px.max(1);
        let bound = tb.tray_left.unwrap_or(tb.width).max(width);
        let x = (strip_start + dx).clamp(0, (bound - width).max(0));
        unsafe {
            let _ = SetWindowPos(
                self.strip,
                HWND_TOP,
                x,
                0,
                width,
                self.strip_h_px,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
        true
    }

    fn finish_strip_drag(&mut self) {
        let Some(tb) = self.taskbar else {
            return;
        };
        let mut rect = RECT::default();
        let mut taskbar_rect = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.strip, &mut rect);
            let _ = GetWindowRect(tb.hwnd, &mut taskbar_rect);
        }
        let x = rect.left - taskbar_rect.left;
        let width = (rect.right - rect.left).max(1);
        let bound = tb.tray_left.unwrap_or(tb.width).max(width);
        let position = if x + width / 2 <= bound / 2 {
            "left"
        } else {
            "right"
        };
        let offset_px = if position == "left" {
            x.max(0)
        } else {
            (bound - (x + width)).max(0)
        };
        self.config.position = position.to_string();
        self.config.offset = (offset_px as f32 * 96.0 / tb.dpi as f32).round() as i32;
        self.config.validate();
        if let Err(error) = self.config.save(&self.config_path) {
            logging::write(&format!("config save failed: {error}"));
        }
        self.position_strip();
        self.render_strip();
    }

    fn track_leave(&self, hwnd: HWND) {
        let mut track = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        unsafe {
            let _ = TrackMouseEvent(&mut track);
        }
    }

    fn cursor_over(&self, hwnd: HWND) -> bool {
        if !unsafe { IsWindow(hwnd).as_bool() } {
            return false;
        }
        let mut rect = RECT::default();
        let mut point = POINT::default();
        unsafe {
            if GetWindowRect(hwnd, &mut rect).is_err() || GetCursorPos(&mut point).is_err() {
                return false;
            }
        }
        point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
    }

    fn reattach(&mut self) {
        self.taskbar = taskbar::find();
        let Some(tb) = self.taskbar else {
            unsafe {
                let _ = ShowWindow(self.strip, SW_HIDE);
                SetTimer(self.hidden, TIMER_ATTACH, 2000, None);
            }
            return;
        };
        let attached = unsafe {
            GetParent(self.strip)
                .ok()
                .map(|parent| parent == tb.hwnd)
                .unwrap_or(false)
        };
        if !attached || !unsafe { IsWindow(self.strip).as_bool() } {
            unsafe {
                if IsWindow(self.strip).as_bool() {
                    let _ = DestroyWindow(self.strip);
                }
            }
            match create_window(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                STRIP_CLASS,
                WS_POPUP,
                None,
                self.hinstance,
            ) {
                Ok(strip) => {
                    self.strip = strip;
                    if !taskbar::attach(self.strip, &tb) {
                        logging::write("reattach failed");
                        return;
                    }
                }
                Err(error) => {
                    logging::write(&format!("create strip failed: {error}"));
                    return;
                }
            }
        }
        self.position_strip();
        self.show_strip();
        self.sample_and_render();
    }

    fn refresh_theme(&mut self) {
        self.light = match self.config.theme.as_str() {
            "light" | "sand" => true,
            "dark" | "amoled" | "slate" => false,
            _ => system_uses_light(),
        };
        if self.config.style != "blur" {
            self.backdrop = None;
        }
    }

    fn switch_tab(&mut self, tab: ui::PanelTab) {
        self.panel_tab = tab;
        self.show_popup(self.pinned);
    }

    fn handle_panel_action(&mut self, action: ui::PanelAction) {
        match action {
            ui::PanelAction::TabOverview => {
                self.switch_tab(ui::PanelTab::Overview);
                return;
            }
            ui::PanelAction::TabSettings => {
                self.switch_tab(ui::PanelTab::Settings);
                return;
            }
            ui::PanelAction::TabAppearance => {
                self.switch_tab(ui::PanelTab::Appearance);
                return;
            }
            ui::PanelAction::TabPomodoro => {
                self.switch_tab(ui::PanelTab::Pomodoro);
                return;
            }
            ui::PanelAction::TabTodo => {
                self.switch_tab(ui::PanelTab::Todo);
                return;
            }
            ui::PanelAction::TodoAdd => {
                self.open_todo_dialog();
                return;
            }
            _ => {}
        }

        let before = self.config.clone();
        match action {
            ui::PanelAction::PositionLeft => self.config.position = "left".into(),
            ui::PanelAction::PositionRight => self.config.position = "right".into(),
            ui::PanelAction::ThemeSystem => self.config.theme = "system".into(),
            ui::PanelAction::ThemeDark => self.config.theme = "dark".into(),
            ui::PanelAction::ThemeLight => self.config.theme = "light".into(),
            ui::PanelAction::ThemeAmoled => self.config.theme = "amoled".into(),
            ui::PanelAction::ThemeSlate => self.config.theme = "slate".into(),
            ui::PanelAction::ThemeSand => self.config.theme = "sand".into(),
            ui::PanelAction::StyleSolid => self.config.style = "solid".into(),
            ui::PanelAction::StyleBlur => self.config.style = "blur".into(),
            ui::PanelAction::StyleClear => self.config.style = "clear".into(),
            ui::PanelAction::MemoryGb => self.config.mem_mode = "gb".into(),
            ui::PanelAction::MemoryPercent => self.config.mem_mode = "percent".into(),
            ui::PanelAction::DiskGb => self.config.disk_mode = "gb".into(),
            ui::PanelAction::DiskPercent => self.config.disk_mode = "percent".into(),
            ui::PanelAction::Refresh500 => self.config.update_ms = 500,
            ui::PanelAction::Refresh1000 => self.config.update_ms = 1000,
            ui::PanelAction::Refresh2000 => self.config.update_ms = 2000,
            ui::PanelAction::Hover0 => self.config.hover_delay_ms = 0,
            ui::PanelAction::Hover150 => self.config.hover_delay_ms = 150,
            ui::PanelAction::Hover220 => self.config.hover_delay_ms = 220,
            ui::PanelAction::Hover400 => self.config.hover_delay_ms = 400,
            ui::PanelAction::OffsetMinus => {
                self.config.offset = (self.config.offset - 2).clamp(-400, 400);
            }
            ui::PanelAction::OffsetPlus => {
                self.config.offset = (self.config.offset + 2).clamp(-400, 400);
            }
            ui::PanelAction::Accent(index) => {
                if let Some(hex) = ui::ACCENTS.get(index) {
                    self.config.accent = (*hex).to_string();
                }
            }
            ui::PanelAction::WidgetCpu => toggle_widget(&mut self.config, "cpu"),
            ui::PanelAction::WidgetMem => toggle_widget(&mut self.config, "mem"),
            ui::PanelAction::WidgetDisk => toggle_widget(&mut self.config, "disk"),
            ui::PanelAction::WidgetNet => toggle_widget(&mut self.config, "net"),
            ui::PanelAction::Gpu(index) => {
                let names = self.sampler.gpu_names();
                if let Some(name) = names.get(index) {
                    toggle_gpu(&mut self.config, name, &names);
                }
            }
            ui::PanelAction::LaunchAtStartup => {
                self.config.launch_at_startup = !self.config.launch_at_startup;
            }
            ui::PanelAction::FontCompact => self.config.font_scale = 0.9,
            ui::PanelAction::FontNormal => self.config.font_scale = 1.0,
            ui::PanelAction::FontLarge => self.config.font_scale = 1.15,
            ui::PanelAction::PomodoroToggle => self.pomodoro.toggle(),
            ui::PanelAction::PomodoroReset => self.pomodoro.reset(&self.config),
            ui::PanelAction::PomodoroSkip => self.pomodoro.skip(&self.config),
            ui::PanelAction::PomodoroFocus25 => self.config.pomodoro_focus_min = 25,
            ui::PanelAction::PomodoroFocus50 => self.config.pomodoro_focus_min = 50,
            ui::PanelAction::PomodoroBreak5 => self.config.pomodoro_break_min = 5,
            ui::PanelAction::PomodoroBreak10 => self.config.pomodoro_break_min = 10,
            ui::PanelAction::TodoPrevPage => {
                self.todo_page = self.todo_page.saturating_sub(1);
            }
            ui::PanelAction::TodoNextPage => {
                let pages = self.config.todos.len().div_ceil(ui::TODO_ROWS).max(1);
                self.todo_page = (self.todo_page + 1).min(pages - 1);
            }
            ui::PanelAction::TodoToggle(row) => {
                let index = self.todo_page * ui::TODO_ROWS + row;
                if let Some(todo) = self.config.todos.get_mut(index) {
                    todo.done = !todo.done;
                }
            }
            ui::PanelAction::TodoDelete(row) => {
                let index = self.todo_page * ui::TODO_ROWS + row;
                if index < self.config.todos.len() {
                    self.config.todos.remove(index);
                    let pages = self.config.todos.len().div_ceil(ui::TODO_ROWS).max(1);
                    self.todo_page = self.todo_page.min(pages - 1);
                }
            }
            ui::PanelAction::TodoClearDone => {
                self.config.todos.retain(|todo| !todo.done);
                let pages = self.config.todos.len().div_ceil(ui::TODO_ROWS).max(1);
                self.todo_page = self.todo_page.min(pages - 1);
            }
            ui::PanelAction::Reset => {
                self.config = Config::default();
                self.todo_page = 0;
            }
            _ => {}
        }

        self.config.validate();
        if let Err(error) = self.config.save(&self.config_path) {
            logging::write(&format!("config save failed: {error}"));
        }
        self.refresh_theme();
        if before.launch_at_startup != self.config.launch_at_startup
            && !sync_startup(self.config.launch_at_startup)
        {
            logging::write("could not update startup registration");
        }
        if before.pomodoro_focus_min != self.config.pomodoro_focus_min
            || before.pomodoro_break_min != self.config.pomodoro_break_min
        {
            self.pomodoro.apply_duration(&self.config);
        }
        if before.update_ms != self.config.update_ms {
            unsafe {
                SetTimer(self.hidden, TIMER_UPDATE, self.config.update_ms, None);
            }
        }

        let layout_changed = before.position != self.config.position
            || before.offset != self.config.offset
            || before.widgets != self.config.widgets
            || before.gpu_visible != self.config.gpu_visible
            || before.font_scale != self.config.font_scale;
        let theme_changed = before.theme != self.config.theme || before.style != self.config.style;
        self.refresh_snapshot();
        if layout_changed {
            self.position_strip();
        }
        self.render_strip();
        if self.popup_visible {
            if layout_changed || theme_changed {
                self.show_popup(self.pinned);
            } else {
                self.render_popup();
            }
        }
    }

    fn show_menu(&mut self) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            let _ = AppendMenuW(menu, MF_STRING, 1, w!("Settings"));
            let _ = AppendMenuW(menu, MF_STRING, 2, w!("Reload config"));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(menu, MF_STRING, 3, w!("Exit"));
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let _ = SetForegroundWindow(self.hidden);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                self.hidden,
                None,
            );
            let _ = DestroyMenu(menu);
            match command.0 as usize {
                1 => {
                    self.panel_tab = ui::PanelTab::Settings;
                    self.pinned = true;
                    self.show_popup(true);
                }
                2 => self.reload_config(),
                3 => {
                    self.remove_tray_icon();
                    PostQuitMessage(0);
                }
                _ => {}
            }
        }
    }

    fn reload_config(&mut self) {
        let (config, path) = Config::load();
        self.config = config;
        self.config_path = path;
        self.refresh_theme();
        self.pomodoro.apply_duration(&self.config);
        let pages = self.config.todos.len().div_ceil(ui::TODO_ROWS).max(1);
        self.todo_page = self.todo_page.min(pages - 1);
        let _ = sync_startup(self.config.launch_at_startup);
        self.position_strip();
        unsafe {
            SetTimer(self.hidden, TIMER_UPDATE, self.config.update_ms, None);
        }
        self.sample_and_render();
    }
}

fn client_point(lparam: LPARAM) -> (f32, f32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
    (x, y)
}

fn ordered_widgets(widgets: &[String]) -> Vec<String> {
    let order = ["cpu", "gpu", "mem", "disk", "net"];
    let mut next = Vec::new();
    for item in order {
        if widgets.iter().any(|widget| widget == item) {
            next.push(item.to_string());
        }
    }
    for item in widgets {
        if !order.contains(&item.as_str()) && !next.contains(item) {
            next.push(item.clone());
        }
    }
    next
}

fn toggle_gpu(config: &mut Config, name: &str, names: &[String]) {
    if config.gpu_enabled(name) {
        if config.gpu_visible.is_empty() {
            config.gpu_visible = names
                .iter()
                .filter(|candidate| !candidate.eq_ignore_ascii_case(name))
                .cloned()
                .collect();
        } else {
            config
                .gpu_visible
                .retain(|candidate| !candidate.eq_ignore_ascii_case(name));
        }
        if config.gpu_visible.is_empty() {
            config.widgets.retain(|widget| widget != "gpu");
        }
    } else {
        if !config.widgets.iter().any(|widget| widget == "gpu") {
            config.widgets.push("gpu".to_string());
        }
        if config.gpu_visible.is_empty() {
            config.gpu_visible = vec![name.to_string()];
        } else if !config
            .gpu_visible
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name))
        {
            config.gpu_visible.push(name.to_string());
        }
    }
    config.widgets = ordered_widgets(&config.widgets);
}

fn toggle_widget(config: &mut Config, widget: &str) {
    let mut widgets = config.widgets.clone();
    if let Some(index) = widgets.iter().position(|item| item == widget) {
        if widgets.len() == 1 {
            return;
        }
        widgets.remove(index);
    } else {
        widgets.push(widget.to_string());
    }

    let order = ["cpu", "gpu", "mem", "disk", "net"];
    let mut next = Vec::new();
    for item in order {
        if widgets.iter().any(|widget| widget == item) {
            next.push(item.to_string());
        }
    }
    for item in widgets {
        if !order.contains(&item.as_str()) && !next.contains(&item) {
            next.push(item);
        }
    }
    if next.is_empty() {
        next.push("cpu".to_string());
    }
    config.widgets = next;
}

fn system_uses_light() -> bool {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast::<c_void>()),
            Some(&mut size),
        )
    };
    status.0 == 0 && value == 1
}

fn animations_enabled() -> bool {
    let mut enabled = BOOL(0);
    let status = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut BOOL).cast::<c_void>()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if status.is_err() {
        return true;
    }
    enabled.as_bool()
}

fn register_classes(hinstance: HINSTANCE) {
    let cursor = unsafe { LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default() };
    type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;
    let classes: [(PCWSTR, WndProc, WNDCLASS_STYLES); 4] = [
        (HOST_CLASS, host_wndproc as WndProc, WNDCLASS_STYLES(0)),
        (
            STRIP_CLASS,
            strip_wndproc as WndProc,
            CS_HREDRAW | CS_VREDRAW,
        ),
        (
            POPUP_CLASS,
            popup_wndproc as WndProc,
            CS_HREDRAW | CS_VREDRAW,
        ),
        (TODO_CLASS, todo_wndproc as WndProc, CS_HREDRAW | CS_VREDRAW),
    ];
    for (class, proc, style) in classes {
        let window_class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style,
            lpfnWndProc: Some(proc),
            hInstance: hinstance,
            hCursor: cursor,
            lpszClassName: class,
            ..Default::default()
        };
        let registered = unsafe { RegisterClassExW(&window_class) };
        if registered == 0 {
            let error = unsafe { GetLastError() };
            if error.0 != 1410 {
                logging::write(&format!("RegisterClassExW failed: {}", error.0));
            }
        }
    }
}

fn create_window(
    ex_style: WINDOW_EX_STYLE,
    class: PCWSTR,
    style: WINDOW_STYLE,
    parent: Option<HWND>,
    hinstance: HINSTANCE,
) -> Result<HWND> {
    unsafe {
        CreateWindowExW(
            ex_style,
            class,
            w!("Taskbar Stats"),
            style,
            0,
            0,
            1,
            1,
            parent.unwrap_or_default(),
            HMENU::default(),
            hinstance,
            None,
        )
    }
}

fn fill_wide<const N: usize>(text: &str, buffer: &mut [u16; N]) {
    let mut index = 0;
    for unit in text.encode_utf16().take(N.saturating_sub(1)) {
        buffer[index] = unit;
        index += 1;
    }
    if index < N {
        buffer[index] = 0;
    }
}

#[allow(clippy::manual_dangling_ptr)]
fn load_app_icon(hinstance: HINSTANCE) -> Option<HICON> {
    unsafe {
        LoadIconW(hinstance, PCWSTR(1 as *const u16))
            .ok()
            .or_else(|| LoadIconW(HINSTANCE::default(), IDI_APPLICATION).ok())
    }
}

fn sync_startup(enabled: bool) -> bool {
    unsafe {
        let mut key = windows::Win32::System::Registry::HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            0,
            KEY_SET_VALUE,
            &mut key,
        );
        if status.0 != 0 {
            return !enabled;
        }
        let result = if enabled {
            let Ok(executable) = std::env::current_exe() else {
                let _ = RegCloseKey(key);
                return false;
            };
            let value = format!("\"{}\"", executable.display());
            let mut wide: Vec<u16> = value.encode_utf16().collect();
            wide.push(0);
            let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
            RegSetValueExW(key, w!("TaskbarStats"), 0, REG_SZ, Some(bytes)).0 == 0
        } else {
            let status = RegDeleteValueW(key, w!("TaskbarStats"));
            status.0 == 0 || status.0 == 2
        };
        let _ = RegCloseKey(key);
        result
    }
}

unsafe extern "system" fn host_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    APP.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                return app.on_host(hwnd, msg, wparam, lparam);
            }
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    })
}

unsafe extern "system" fn strip_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    APP.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                return app.on_strip(hwnd, msg, wparam, lparam);
            }
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    })
}

unsafe extern "system" fn popup_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    APP.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                return app.on_popup(hwnd, msg, wparam, lparam);
            }
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    })
}

unsafe extern "system" fn todo_edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CHAR && matches!(wparam.0 as u16, 13 | 27) {
        return LRESULT(0);
    }
    if msg == WM_KEYDOWN {
        let key = wparam.0 as u16;
        if key == VK_RETURN.0 {
            if let Ok(parent) = unsafe { GetParent(hwnd) } {
                let _ = unsafe { PostMessageW(parent, WM_TODO_ADD, WPARAM(0), LPARAM(0)) };
            }
            return LRESULT(0);
        }
        if key == VK_ESCAPE.0 {
            if let Ok(parent) = unsafe { GetParent(hwnd) } {
                let _ = unsafe { PostMessageW(parent, WM_TODO_CANCEL, WPARAM(0), LPARAM(0)) };
            }
            return LRESULT(0);
        }
    }
    let previous = TODO_EDIT_PROC.with(|cell| cell.get());
    let proc: WNDPROC = unsafe { std::mem::transmute(previous) };
    unsafe { CallWindowProcW(proc, hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn todo_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    APP.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                return app.on_todo(hwnd, msg, wparam, lparam);
            }
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    })
}

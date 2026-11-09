use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;

use windows::core::{w, PCWSTR, Result};
use windows::Win32::Foundation::{
    GetLastError, BOOL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, ValidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::Config;
use crate::logging;
use crate::metrics::{Sampler, Snapshot};
use crate::render::Renderer;
use crate::taskbar::{self, Taskbar};
use crate::ui;

const HOST_CLASS: PCWSTR = w!("TaskbarStatsHost");
const STRIP_CLASS: PCWSTR = w!("TaskbarStatsStrip");
const POPUP_CLASS: PCWSTR = w!("TaskbarStatsPopup");

const TIMER_UPDATE: usize = 1;
const TIMER_ATTACH: usize = 2;
const TIMER_HOVER: usize = 3;
const TIMER_HIDE: usize = 4;

const MUTEX_NAME: PCWSTR = w!("Local\\TaskbarStats.SingleInstance");
const WM_MOUSELEAVE_MSG: u32 = 0x02A3;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
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

        let hidden = create_window(
            WS_EX_TOOLWINDOW,
            HOST_CLASS,
            WS_OVERLAPPED,
            None,
            hinstance,
        )?;
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
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut guard| guard.as_mut().map(f)))
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
    popup_visible: bool,
    pinned: bool,
    panel_tab: ui::PanelTab,
    panel_hover: Option<ui::PanelAction>,
    hover_timer: bool,
    hide_timer: bool,
    taskbar_created: u32,
    light: bool,
}

impl App {
    fn start(&mut self) {
        self.refresh_theme();
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
            SetTimer(
                self.hidden,
                TIMER_UPDATE,
                self.config.update_ms,
                None,
            );
        }
    }

    fn on_host(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == self.taskbar_created {
            self.reattach();
            return LRESULT(0);
        }
        match msg {
            WM_TIMER => {
                match wparam.0 {
                    TIMER_UPDATE => self.sample_and_render(),
                    TIMER_ATTACH => self.reattach(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                self.taskbar = taskbar::find();
                self.position_strip();
                self.render_strip();
                LRESULT(0)
            }
            WM_SETTINGCHANGE | WM_THEMECHANGED => {
                self.refresh_theme();
                self.render_strip();
                if self.popup_visible {
                    self.render_popup();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn on_strip(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_MOUSEMOVE => {
                self.track_leave(hwnd);
                if !self.popup_visible && !self.pinned && !self.hover_timer {
                    unsafe {
                        SetTimer(
                            self.strip,
                            TIMER_HOVER,
                            self.config.hover_delay_ms,
                            None,
                        );
                    }
                    self.hover_timer = true;
                }
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
                if self.popup_visible && self.pinned {
                    self.hide_popup();
                } else {
                    self.show_popup(true);
                }
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
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DPICHANGED => {
                self.position_strip();
                self.render_strip();
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn on_popup(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_MOUSEMOVE => {
                self.track_leave(hwnd);
                if self.hide_timer {
                    unsafe {
                        let _ = KillTimer(self.strip, TIMER_HIDE);
                    }
                    self.hide_timer = false;
                }
                let (x, y) = self.panel_point(lparam);
                let hover = ui::action_at(self.panel_tab, x, y);
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
                if let Some(action) = ui::action_at(self.panel_tab, x, y) {
                    self.handle_panel_action(action);
                } else if self.panel_tab == ui::PanelTab::Settings {
                    // Keep the settings surface open when a click lands between controls.
                } else if self.pinned {
                    self.hide_popup();
                } else {
                    self.pinned = true;
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
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn panel_point(&self, lparam: LPARAM) -> (f32, f32) {
        let (x, y) = client_point(lparam);
        let dpi = self.taskbar.map(|tb| tb.dpi).unwrap_or(96).max(96) as f32;
        let scale = 96.0 / dpi;
        (x * scale, y * scale)
    }

    fn sample_and_render(&mut self) {
        self.snapshot = self.sampler.sample();
        self.render_strip();
        if self.popup_visible {
            self.render_popup();
        }
    }

    fn render_strip(&mut self) {
        if self.strip_w_px <= 0 || self.strip_h_px <= 0 {
            return;
        }
        if self.strip_renderer.resize(
            self.strip_w_px,
            self.strip_h_px,
            self.strip_dpi,
        ).is_err()
        {
            return;
        }
        let width = self.strip_w_px as f32 * 96.0 / self.strip_dpi as f32;
        let height = self.strip_h_px as f32 * 96.0 / self.strip_dpi as f32;
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
        let Some(tb) = self.taskbar else { return };
        let dpi = tb.dpi.max(96);
        let width = (ui::PANEL_WIDTH * dpi as f32 / 96.0).round() as i32;
        let height = (ui::panel_height(self.panel_tab) * dpi as f32 / 96.0).round() as i32;
        if self.popup_renderer.resize(width, height, dpi).is_err() {
            return;
        }
        if let Err(error) = ui::draw_panel(
            &self.popup_renderer,
            self.popup,
            &self.snapshot,
            &self.config,
            self.light,
            ui::PANEL_WIDTH,
            ui::panel_height(self.panel_tab),
            self.panel_tab,
            self.panel_hover,
        ) {
            logging::write(&format!("popup render failed: {error}"));
        }
    }

    fn position_strip(&mut self) {
        let Some(tb) = self.taskbar else { return };
        if !unsafe { IsWindow(self.strip).as_bool() } {
            return;
        }
        let width_dip = ui::strip_width_dip(&self.config, self.sampler.gpu_count());
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
            x,
            width,
            height,
            tb.dpi,
            tb.width
        ));
    }

    fn show_strip(&self) {
        unsafe {
            let _ = ShowWindow(self.strip, SW_SHOWNOACTIVATE);
        }
    }

    fn show_popup(&mut self, pin: bool) {
        let Some(tb) = self.taskbar else { return };
        self.pinned = pin;
        self.panel_hover = None;
        if self.hide_timer {
            unsafe {
                let _ = KillTimer(self.strip, TIMER_HIDE);
            }
            self.hide_timer = false;
        }
        let dpi = tb.dpi.max(96);
        let width = (ui::PANEL_WIDTH * dpi as f32 / 96.0).round() as i32;
        let height = (ui::panel_height(self.panel_tab) * dpi as f32 / 96.0).round() as i32;

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
    }

    fn hide_popup(&mut self) {
        self.pinned = false;
        self.popup_visible = false;
        self.panel_tab = ui::PanelTab::Overview;
        self.panel_hover = None;
        unsafe {
            let _ = ShowWindow(self.popup, SW_HIDE);
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
        point.x >= rect.left
            && point.x < rect.right
            && point.y >= rect.top
            && point.y < rect.bottom
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
            "light" => true,
            "dark" => false,
            _ => system_uses_light(),
        };
    }

    fn handle_panel_action(&mut self, action: ui::PanelAction) {
        match action {
            ui::PanelAction::TabOverview => {
                self.panel_tab = ui::PanelTab::Overview;
                self.pinned = true;
                self.show_popup(true);
                return;
            }
            ui::PanelAction::TabSettings => {
                self.panel_tab = ui::PanelTab::Settings;
                self.pinned = true;
                self.show_popup(true);
                return;
            }
            ui::PanelAction::Done => {
                self.hide_popup();
                return;
            }
            _ => {}
        }

        self.pinned = true;
        let before = self.config.clone();
        match action {
            ui::PanelAction::PositionLeft => self.config.position = "left".into(),
            ui::PanelAction::PositionRight => self.config.position = "right".into(),
            ui::PanelAction::ThemeSystem => self.config.theme = "system".into(),
            ui::PanelAction::ThemeDark => self.config.theme = "dark".into(),
            ui::PanelAction::ThemeLight => self.config.theme = "light".into(),
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
            ui::PanelAction::WidgetGpu => toggle_widget(&mut self.config, "gpu"),
            ui::PanelAction::WidgetMem => toggle_widget(&mut self.config, "mem"),
            ui::PanelAction::WidgetDisk => toggle_widget(&mut self.config, "disk"),
            ui::PanelAction::Reset => self.config = Config::default(),
            _ => {}
        }

        self.config.validate();
        if let Err(error) = self.config.save(&self.config_path) {
            logging::write(&format!("config save failed: {error}"));
        }
        self.refresh_theme();
        if before.update_ms != self.config.update_ms {
            unsafe {
                SetTimer(
                    self.hidden,
                    TIMER_UPDATE,
                    self.config.update_ms,
                    None,
                );
            }
        }

        let layout_changed = before.position != self.config.position
            || before.offset != self.config.offset
            || before.widgets != self.config.widgets;
        if layout_changed {
            self.position_strip();
        }
        self.render_strip();
        if self.popup_visible {
            if layout_changed {
                self.show_popup(true);
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
                3 => PostQuitMessage(0),
                _ => {}
            }
        }
    }

    fn reload_config(&mut self) {
        let (config, path) = Config::load();
        self.config = config;
        self.config_path = path;
        self.refresh_theme();
        self.position_strip();
        unsafe {
            SetTimer(
                self.hidden,
                TIMER_UPDATE,
                self.config.update_ms,
                None,
            );
        }
        self.sample_and_render();
    }
}

fn client_point(lparam: LPARAM) -> (f32, f32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
    (x, y)
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

    let order = ["cpu", "gpu", "mem", "disk"];
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

fn register_classes(hinstance: HINSTANCE) {
    let cursor = unsafe {
        LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default()
    };
    type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;
    let classes: [(PCWSTR, WndProc, WNDCLASS_STYLES); 3] = [
        (HOST_CLASS, host_wndproc as WndProc, WNDCLASS_STYLES(0)),
        (STRIP_CLASS, strip_wndproc as WndProc, CS_HREDRAW | CS_VREDRAW),
        (POPUP_CLASS, popup_wndproc as WndProc, CS_HREDRAW | CS_VREDRAW),
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

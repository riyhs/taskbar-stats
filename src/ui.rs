use windows::core::Result;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_TRAILING,
};

use crate::config::Config;
use crate::gpu::GpuInfo;
use crate::metrics::Snapshot;
use crate::pomodoro::Pomodoro;
use crate::render::Renderer;

const STRIP_PAD: f32 = 4.0;
const STRIP_INNER: f32 = 6.0;
const STRIP_GAP: f32 = 10.0;
const STRIP_RADIUS: f32 = 12.0;
const STRIP_CELL_W: f32 = 72.0;
const LABEL_SIZE: f32 = 9.0;
const TEMP_SIZE: f32 = 9.0;

pub const PANEL_WIDTH: f32 = 400.0;
pub const PANEL_MARGIN: f32 = 6.0;
const SETTINGS_HEIGHT: f32 = 332.0;
const SETTINGS_PAD: f32 = 16.0;
const SETTINGS_COL_W: f32 = 179.0;
const SETTINGS_COL2: f32 = SETTINGS_PAD + SETTINGS_COL_W + 10.0;
const SETTINGS_ROW1: f32 = 16.0;
const SETTINGS_ROW2: f32 = 64.0;
const SETTINGS_ROW3: f32 = 112.0;
const SETTINGS_WIDGETS: f32 = 160.0;
const SETTINGS_GPUS: f32 = 208.0;
const SETTINGS_FOOTER: f32 = 256.0;
const APPEARANCE_HEIGHT: f32 = 204.0;
const APPEARANCE_ROW1: f32 = 16.0;
const APPEARANCE_ROW2: f32 = 64.0;
const APPEARANCE_ROW3: f32 = 112.0;
const POMODORO_HEIGHT: f32 = 360.0;
const TODO_HEIGHT: f32 = 320.0;
pub const TODO_ROWS: usize = 5;
const TAB_W: f32 = 58.0;
const TAB_GAP: f32 = 5.0;
const TAB_H: f32 = 28.0;
const TAB_BOTTOM_PAD: f32 = 8.0;
const VERSION: &str = env!("TASKBAR_STATS_VERSION");
const TAB_BAND: f32 = TAB_H + TAB_BOTTOM_PAD;

pub const ACCENTS: [&str; 6] = [
    "#5B9CFF", "#4CC87E", "#F5A641", "#F55C5C", "#B07CFF", "#50C8D8",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelTab {
    Overview,
    Pomodoro,
    Todo,
    Settings,
    Appearance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelAction {
    TabOverview,
    TabPomodoro,
    TabTodo,
    TabSettings,
    TabAppearance,
    PositionLeft,
    PositionRight,
    ThemeSystem,
    ThemeDark,
    ThemeLight,
    ThemeAmoled,
    ThemeSlate,
    ThemeSand,
    StyleSolid,
    StyleBlur,
    StyleClear,
    MemoryGb,
    MemoryPercent,
    DiskGb,
    DiskPercent,
    Refresh500,
    Refresh1000,
    Refresh2000,
    Hover0,
    Hover150,
    Hover220,
    Hover400,
    OffsetMinus,
    OffsetPlus,
    Accent(usize),
    WidgetCpu,
    WidgetMem,
    WidgetDisk,
    WidgetNet,
    Gpu(usize),
    LaunchAtStartup,
    FontCompact,
    FontNormal,
    FontLarge,
    PomodoroToggle,
    PomodoroReset,
    PomodoroSkip,
    PomodoroFocus25,
    PomodoroFocus50,
    PomodoroBreak5,
    PomodoroBreak10,
    TodoAdd,
    TodoClearDone,
    TodoPrevPage,
    TodoNextPage,
    TodoToggle(usize),
    TodoDelete(usize),
    Reset,
}

pub fn panel_height(tab: PanelTab) -> f32 {
    match tab {
        PanelTab::Overview => 280.0,
        PanelTab::Pomodoro => POMODORO_HEIGHT,
        PanelTab::Todo => TODO_HEIGHT,
        PanelTab::Settings => SETTINGS_HEIGHT,
        PanelTab::Appearance => APPEARANCE_HEIGHT,
    }
}

type Rect = (f32, f32, f32, f32);

fn contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.0 && x < rect.0 + rect.2 && y >= rect.1 && y < rect.1 + rect.3
}

fn tab_rect(tab: PanelTab, height: f32) -> Rect {
    let index = match tab {
        PanelTab::Overview => 0.0,
        PanelTab::Pomodoro => 1.0,
        PanelTab::Todo => 2.0,
        PanelTab::Settings => 3.0,
        PanelTab::Appearance => 4.0,
    };
    (
        SETTINGS_PAD + index * (TAB_W + TAB_GAP),
        height - TAB_H - TAB_BOTTOM_PAD,
        TAB_W,
        TAB_H,
    )
}

fn segment_rect(column_x: f32, row_y: f32, count: usize, index: usize) -> Rect {
    let gap = 4.0;
    let width = (SETTINGS_COL_W - (count as f32 - 1.0) * gap) / count as f32;
    (
        column_x + index as f32 * (width + gap),
        row_y + 16.0,
        width,
        24.0,
    )
}

fn full_row_rect(index: usize, count: usize, row_y: f32) -> Rect {
    let gap = 6.0;
    let width = (PANEL_WIDTH - 2.0 * SETTINGS_PAD - (count as f32 - 1.0) * gap) / count as f32;
    (
        SETTINGS_PAD + index as f32 * (width + gap),
        row_y + 14.0,
        width,
        24.0,
    )
}

fn settings_rect(action: PanelAction, gpu_count: usize) -> Option<Rect> {
    match action {
        PanelAction::PositionLeft => Some(segment_rect(SETTINGS_PAD, SETTINGS_ROW1, 2, 0)),
        PanelAction::PositionRight => Some(segment_rect(SETTINGS_PAD, SETTINGS_ROW1, 2, 1)),
        PanelAction::OffsetMinus => Some((SETTINGS_COL2, SETTINGS_ROW1 + 16.0, 30.0, 24.0)),
        PanelAction::OffsetPlus => Some((
            SETTINGS_COL2 + SETTINGS_COL_W - 30.0,
            SETTINGS_ROW1 + 16.0,
            30.0,
            24.0,
        )),
        PanelAction::MemoryGb => Some(segment_rect(SETTINGS_PAD, SETTINGS_ROW2, 2, 0)),
        PanelAction::MemoryPercent => Some(segment_rect(SETTINGS_PAD, SETTINGS_ROW2, 2, 1)),
        PanelAction::DiskGb => Some(segment_rect(SETTINGS_COL2, SETTINGS_ROW2, 2, 0)),
        PanelAction::DiskPercent => Some(segment_rect(SETTINGS_COL2, SETTINGS_ROW2, 2, 1)),
        PanelAction::Refresh500 => Some(segment_rect(SETTINGS_PAD, SETTINGS_ROW3, 3, 0)),
        PanelAction::Refresh1000 => Some(segment_rect(SETTINGS_PAD, SETTINGS_ROW3, 3, 1)),
        PanelAction::Refresh2000 => Some(segment_rect(SETTINGS_PAD, SETTINGS_ROW3, 3, 2)),
        PanelAction::Hover0 => Some(segment_rect(SETTINGS_COL2, SETTINGS_ROW3, 4, 0)),
        PanelAction::Hover150 => Some(segment_rect(SETTINGS_COL2, SETTINGS_ROW3, 4, 1)),
        PanelAction::Hover220 => Some(segment_rect(SETTINGS_COL2, SETTINGS_ROW3, 4, 2)),
        PanelAction::Hover400 => Some(segment_rect(SETTINGS_COL2, SETTINGS_ROW3, 4, 3)),
        PanelAction::WidgetCpu => Some(full_row_rect(0, 4, SETTINGS_WIDGETS)),
        PanelAction::WidgetMem => Some(full_row_rect(1, 4, SETTINGS_WIDGETS)),
        PanelAction::WidgetDisk => Some(full_row_rect(2, 4, SETTINGS_WIDGETS)),
        PanelAction::WidgetNet => Some(full_row_rect(3, 4, SETTINGS_WIDGETS)),
        PanelAction::Gpu(index) if index < gpu_count => {
            Some(full_row_rect(index, gpu_count, SETTINGS_GPUS))
        }
        PanelAction::LaunchAtStartup => Some((SETTINGS_PAD, SETTINGS_FOOTER, 150.0, 24.0)),
        PanelAction::FontCompact => Some(segment_rect(SETTINGS_PAD, APPEARANCE_ROW3, 3, 0)),
        PanelAction::FontNormal => Some(segment_rect(SETTINGS_PAD, APPEARANCE_ROW3, 3, 1)),
        PanelAction::FontLarge => Some(segment_rect(SETTINGS_PAD, APPEARANCE_ROW3, 3, 2)),
        PanelAction::PomodoroToggle => Some((SETTINGS_PAD, 176.0, 160.0, 28.0)),
        PanelAction::PomodoroReset => Some((186.0, 176.0, 90.0, 28.0)),
        PanelAction::PomodoroSkip => Some((286.0, 176.0, 98.0, 28.0)),
        PanelAction::PomodoroFocus25 => Some(segment_rect(SETTINGS_PAD, 228.0, 2, 0)),
        PanelAction::PomodoroFocus50 => Some(segment_rect(SETTINGS_PAD, 228.0, 2, 1)),
        PanelAction::PomodoroBreak5 => Some(segment_rect(SETTINGS_PAD, 276.0, 2, 0)),
        PanelAction::PomodoroBreak10 => Some(segment_rect(SETTINGS_PAD, 276.0, 2, 1)),
        PanelAction::TodoAdd => Some((SETTINGS_PAD, 216.0, 112.0, 26.0)),
        PanelAction::TodoClearDone => Some((140.0, 216.0, 130.0, 26.0)),
        PanelAction::TodoPrevPage => Some((280.0, 216.0, 30.0, 24.0)),
        PanelAction::TodoNextPage => Some((352.0, 216.0, 30.0, 24.0)),
        PanelAction::TodoToggle(index) if index < TODO_ROWS => {
            Some((SETTINGS_PAD, 40.0 + index as f32 * 34.0, 300.0, 28.0))
        }
        PanelAction::TodoDelete(index) if index < TODO_ROWS => {
            Some((354.0, 40.0 + index as f32 * 34.0, 30.0, 28.0))
        }
        PanelAction::Reset => Some((
            PANEL_WIDTH - SETTINGS_PAD - 70.0,
            SETTINGS_FOOTER,
            70.0,
            24.0,
        )),
        PanelAction::ThemeSystem => Some(full_row_rect(0, 6, APPEARANCE_ROW1)),
        PanelAction::ThemeDark => Some(full_row_rect(1, 6, APPEARANCE_ROW1)),
        PanelAction::ThemeLight => Some(full_row_rect(2, 6, APPEARANCE_ROW1)),
        PanelAction::ThemeAmoled => Some(full_row_rect(3, 6, APPEARANCE_ROW1)),
        PanelAction::ThemeSlate => Some(full_row_rect(4, 6, APPEARANCE_ROW1)),
        PanelAction::ThemeSand => Some(full_row_rect(5, 6, APPEARANCE_ROW1)),
        PanelAction::StyleSolid => Some(segment_rect(SETTINGS_PAD, APPEARANCE_ROW2, 3, 0)),
        PanelAction::StyleBlur => Some(segment_rect(SETTINGS_PAD, APPEARANCE_ROW2, 3, 1)),
        PanelAction::StyleClear => Some(segment_rect(SETTINGS_PAD, APPEARANCE_ROW2, 3, 2)),
        PanelAction::Accent(index) => {
            if index >= ACCENTS.len() {
                return None;
            }
            let total = ACCENTS.len() as f32 * 20.0 + (ACCENTS.len() as f32 - 1.0) * 5.0;
            let start = SETTINGS_COL2 + (SETTINGS_COL_W - total) / 2.0;
            Some((
                start + index as f32 * 25.0,
                APPEARANCE_ROW2 + 16.0,
                20.0,
                20.0,
            ))
        }
        _ => None,
    }
}

const SETTINGS_ACTIONS: &[PanelAction] = &[
    PanelAction::PositionLeft,
    PanelAction::PositionRight,
    PanelAction::OffsetMinus,
    PanelAction::OffsetPlus,
    PanelAction::MemoryGb,
    PanelAction::MemoryPercent,
    PanelAction::DiskGb,
    PanelAction::DiskPercent,
    PanelAction::Refresh500,
    PanelAction::Refresh1000,
    PanelAction::Refresh2000,
    PanelAction::Hover0,
    PanelAction::Hover150,
    PanelAction::Hover220,
    PanelAction::Hover400,
    PanelAction::WidgetCpu,
    PanelAction::WidgetMem,
    PanelAction::WidgetDisk,
    PanelAction::WidgetNet,
    PanelAction::LaunchAtStartup,
    PanelAction::Reset,
];

const APPEARANCE_ACTIONS: &[PanelAction] = &[
    PanelAction::ThemeSystem,
    PanelAction::ThemeDark,
    PanelAction::ThemeLight,
    PanelAction::ThemeAmoled,
    PanelAction::ThemeSlate,
    PanelAction::ThemeSand,
    PanelAction::StyleSolid,
    PanelAction::StyleBlur,
    PanelAction::StyleClear,
    PanelAction::Accent(0),
    PanelAction::Accent(1),
    PanelAction::Accent(2),
    PanelAction::Accent(3),
    PanelAction::Accent(4),
    PanelAction::Accent(5),
    PanelAction::FontCompact,
    PanelAction::FontNormal,
    PanelAction::FontLarge,
];

const POMODORO_ACTIONS: &[PanelAction] = &[
    PanelAction::PomodoroToggle,
    PanelAction::PomodoroReset,
    PanelAction::PomodoroSkip,
    PanelAction::PomodoroFocus25,
    PanelAction::PomodoroFocus50,
    PanelAction::PomodoroBreak5,
    PanelAction::PomodoroBreak10,
];

const TODO_ACTIONS: &[PanelAction] = &[
    PanelAction::TodoAdd,
    PanelAction::TodoClearDone,
    PanelAction::TodoPrevPage,
    PanelAction::TodoNextPage,
    PanelAction::TodoToggle(0),
    PanelAction::TodoToggle(1),
    PanelAction::TodoToggle(2),
    PanelAction::TodoToggle(3),
    PanelAction::TodoToggle(4),
    PanelAction::TodoToggle(5),
    PanelAction::TodoDelete(0),
    PanelAction::TodoDelete(1),
    PanelAction::TodoDelete(2),
    PanelAction::TodoDelete(3),
    PanelAction::TodoDelete(4),
    PanelAction::TodoDelete(5),
];

pub fn action_at(tab: PanelTab, x: f32, y: f32, gpu_count: usize) -> Option<PanelAction> {
    let height = panel_height(tab);
    for (item, action) in [
        (PanelTab::Overview, PanelAction::TabOverview),
        (PanelTab::Pomodoro, PanelAction::TabPomodoro),
        (PanelTab::Todo, PanelAction::TabTodo),
        (PanelTab::Settings, PanelAction::TabSettings),
        (PanelTab::Appearance, PanelAction::TabAppearance),
    ] {
        if contains(tab_rect(item, height), x, y) {
            return Some(action);
        }
    }
    if tab == PanelTab::Settings {
        return SETTINGS_ACTIONS
            .iter()
            .copied()
            .find(|action| {
                settings_rect(*action, gpu_count).is_some_and(|rect| contains(rect, x, y))
            })
            .or_else(|| {
                (0..gpu_count).find_map(|index| {
                    let action = PanelAction::Gpu(index);
                    settings_rect(action, gpu_count)
                        .is_some_and(|rect| contains(rect, x, y))
                        .then_some(action)
                })
            });
    }
    if tab == PanelTab::Appearance {
        return APPEARANCE_ACTIONS.iter().copied().find(|action| {
            settings_rect(*action, gpu_count).is_some_and(|rect| contains(rect, x, y))
        });
    }
    if tab == PanelTab::Pomodoro {
        return POMODORO_ACTIONS.iter().copied().find(|action| {
            settings_rect(*action, gpu_count).is_some_and(|rect| contains(rect, x, y))
        });
    }
    if tab == PanelTab::Todo {
        return TODO_ACTIONS
            .iter()
            .copied()
            .find(|action| settings_rect(*action, gpu_count).is_some_and(|r| contains(r, x, y)));
    }
    None
}

pub fn strip_width_dip(config: &Config, gpu_count: usize) -> i32 {
    let count = strip_cell_count(config, gpu_count) as f32;
    (2.0 * STRIP_PAD + 2.0 * STRIP_INNER + count * STRIP_CELL_W + (count - 1.0) * STRIP_GAP).round()
        as i32
}

fn strip_cell_count(config: &Config, gpu_count: usize) -> usize {
    config
        .widgets
        .iter()
        .map(|widget| if widget == "gpu" { gpu_count } else { 1 })
        .sum::<usize>()
        .max(1)
}

struct Palette {
    bg: D2D1_COLOR_F,
    card: D2D1_COLOR_F,
    border: D2D1_COLOR_F,
    track: D2D1_COLOR_F,
    muted: D2D1_COLOR_F,
    secondary: D2D1_COLOR_F,
    text: D2D1_COLOR_F,
    accent: D2D1_COLOR_F,
    warn: D2D1_COLOR_F,
    bad: D2D1_COLOR_F,
    highlight: D2D1_COLOR_F,
}

struct StripCell {
    label: String,
    value: String,
    detail: Option<(String, D2D1_COLOR_F)>,
    value_color: D2D1_COLOR_F,
    bar_color: D2D1_COLOR_F,
    fraction: f32,
    show_bar: bool,
}

pub fn draw_strip(
    renderer: &Renderer,
    hwnd: HWND,
    snapshot: &Snapshot,
    config: &Config,
    light: bool,
    width: f32,
    height: f32,
) -> Result<()> {
    let p = palette(light, &config.theme, &config.style, &config.accent);
    renderer.begin()?;

    renderer.fill_round(
        STRIP_PAD,
        STRIP_PAD,
        width - 2.0 * STRIP_PAD,
        height - 2.0 * STRIP_PAD,
        STRIP_RADIUS,
        p.bg,
    );
    renderer.stroke_round(
        STRIP_PAD + 0.5,
        STRIP_PAD + 0.5,
        width - 2.0 * STRIP_PAD - 1.0,
        height - 2.0 * STRIP_PAD - 1.0,
        STRIP_RADIUS - 0.5,
        p.border,
        1.0,
    );

    let cells = build_strip_cells(snapshot, config, &p);
    let count = cells.len().max(1) as f32;
    let content_width = width - 2.0 * STRIP_PAD - 2.0 * STRIP_INNER;
    let cell_width = (content_width - (count - 1.0) * STRIP_GAP) / count;
    let value_size = (height * 0.36).clamp(13.0, 17.0);
    let label_size = (height * 0.19).clamp(8.0, LABEL_SIZE);
    let label_y = (height * 0.16).max(6.0);
    let value_y = (height * 0.38).max(15.0);
    let bar_y = height - 8.5;
    let bar_h = 2.5;

    for (index, cell) in cells.iter().enumerate() {
        let x = STRIP_PAD + STRIP_INNER + index as f32 * (cell_width + STRIP_GAP);
        renderer.text_left_weight(
            &cell.label,
            x,
            label_y,
            cell_width,
            10.0,
            label_size,
            p.muted,
            DWRITE_FONT_WEIGHT_NORMAL,
        );

        if let Some((detail, detail_color)) = &cell.detail {
            renderer.text_weight(
                detail,
                x,
                label_y + 0.5,
                cell_width,
                10.0,
                TEMP_SIZE,
                *detail_color,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            );
        }

        renderer.text_left_weight(
            &cell.value,
            x,
            value_y,
            cell_width,
            value_size + 4.0,
            value_size,
            cell.value_color,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
        );

        if cell.show_bar {
            renderer.fill_round(x, bar_y, cell_width, bar_h, bar_h * 0.5, p.track);
            if cell.fraction > 0.0 {
                let fill = (cell_width * cell.fraction.clamp(0.0, 1.0)).max(bar_h);
                renderer.fill_round(x, bar_y, fill, bar_h, bar_h * 0.5, cell.bar_color);
            }
        }
    }

    renderer.end(hwnd)
}

fn build_strip_cells(snapshot: &Snapshot, config: &Config, p: &Palette) -> Vec<StripCell> {
    let mut cells = Vec::new();
    for widget in &config.widgets {
        match widget.as_str() {
            "cpu" => cells.push(StripCell {
                label: "CPU".into(),
                value: format!("{:.0}%", snapshot.cpu_load),
                detail: Some(temp_detail(snapshot.cpu_temp, p)),
                value_color: load_text_color(snapshot.cpu_load, p),
                bar_color: load_fill_color(snapshot.cpu_load, p),
                fraction: snapshot.cpu_load / 100.0,
                show_bar: true,
            }),
            "gpu" => {
                for gpu in &snapshot.gpus {
                    let load = gpu.load_pct.unwrap_or(0.0);
                    cells.push(StripCell {
                        label: short_name(&gpu.name),
                        value: if gpu.load_pct.is_some() {
                            format!("{load:.0}%")
                        } else {
                            "--".into()
                        },
                        detail: Some(temp_detail(gpu.temp_c, p)),
                        value_color: if gpu.load_pct.is_some() {
                            load_text_color(load, p)
                        } else {
                            p.muted
                        },
                        bar_color: load_fill_color(load, p),
                        fraction: load / 100.0,
                        show_bar: true,
                    });
                }
            }
            "temp" => {
                let value = snapshot
                    .cpu_temp
                    .map(|temp| format!("{temp:.0}\u{00B0}"))
                    .unwrap_or_else(|| "--".to_string());
                let color = snapshot
                    .cpu_temp
                    .map(|temp| temperature_color(temp, p))
                    .unwrap_or(p.muted);
                cells.push(StripCell {
                    label: "TEMP".into(),
                    value,
                    detail: None,
                    value_color: color,
                    bar_color: color,
                    fraction: snapshot.cpu_temp.unwrap_or(0.0) / 100.0,
                    show_bar: true,
                });
            }
            "gputemp" => {
                let gpu = snapshot.gpus.first();
                let value = gpu
                    .and_then(|gpu| gpu.temp_c)
                    .map(|temp| format!("{temp:.0}\u{00B0}"))
                    .unwrap_or_else(|| "--".to_string());
                let color = gpu
                    .and_then(|gpu| gpu.temp_c)
                    .map(|temp| temperature_color(temp, p))
                    .unwrap_or(p.muted);
                cells.push(StripCell {
                    label: "GPU".into(),
                    value,
                    detail: None,
                    value_color: color,
                    bar_color: color,
                    fraction: 0.0,
                    show_bar: false,
                });
            }
            "mem" => {
                let fraction = if snapshot.mem_total > 0 {
                    snapshot.mem_used as f32 / snapshot.mem_total as f32
                } else {
                    0.0
                };
                cells.push(StripCell {
                    label: "MEM".into(),
                    value: if config.mem_mode == "percent" {
                        format!("{:.0}%", fraction * 100.0)
                    } else {
                        format!("{:.1}G", gib(snapshot.mem_used))
                    },
                    detail: None,
                    value_color: load_text_color(fraction * 100.0, p),
                    bar_color: load_fill_color(fraction * 100.0, p),
                    fraction,
                    show_bar: true,
                });
            }
            "disk" => {
                let fraction = if snapshot.disk_total > 0 {
                    snapshot.disk_used as f32 / snapshot.disk_total as f32
                } else {
                    0.0
                };
                cells.push(StripCell {
                    label: "DISK".into(),
                    value: if config.disk_mode == "percent" {
                        format!("{:.0}%", fraction * 100.0)
                    } else {
                        format!("{:.0}G", gib(snapshot.disk_used))
                    },
                    detail: Some(temp_detail(snapshot.disk_temp, p)),
                    value_color: load_text_color(fraction * 100.0, p),
                    bar_color: load_fill_color(fraction * 100.0, p),
                    fraction,
                    show_bar: true,
                });
            }
            "net" => cells.push(StripCell {
                label: "NET".into(),
                value: format!("\u{2193}{}", format_rate(snapshot.net_down_bps)),
                detail: Some((
                    format!("\u{2191}{}", format_rate(snapshot.net_up_bps)),
                    p.secondary,
                )),
                value_color: p.text,
                bar_color: p.accent,
                fraction: 0.0,
                show_bar: false,
            }),
            _ => {}
        }
    }
    if cells.is_empty() {
        cells.push(StripCell {
            label: "CPU".into(),
            value: format!("{:.0}%", snapshot.cpu_load),
            detail: Some(temp_detail(snapshot.cpu_temp, p)),
            value_color: load_text_color(snapshot.cpu_load, p),
            bar_color: load_fill_color(snapshot.cpu_load, p),
            fraction: snapshot.cpu_load / 100.0,
            show_bar: true,
        });
    }
    cells
}

fn temp_detail(temp: Option<f32>, p: &Palette) -> (String, D2D1_COLOR_F) {
    match temp {
        Some(temp) => (format!("{temp:.0}\u{00B0}"), temperature_color(temp, p)),
        None => ("--\u{00B0}".to_string(), p.muted),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_panel(
    renderer: &Renderer,
    hwnd: HWND,
    snapshot: &Snapshot,
    config: &Config,
    light: bool,
    width: f32,
    height: f32,
    tab: PanelTab,
    hover: Option<PanelAction>,
    pomodoro: &Pomodoro,
    todo_page: usize,
    gpu_names: &[String],
    background: Option<&[u8]>,
    background_width: u32,
    background_height: u32,
) -> Result<()> {
    let p = palette(light, &config.theme, &config.style, &config.accent);
    renderer.begin()?;
    renderer.translate(PANEL_MARGIN, 0.0);
    if let Some(pixels) = background {
        renderer.draw_background(
            pixels,
            background_width,
            background_height,
            0.0,
            0.0,
            width,
            height,
        )?;
    }
    renderer.fill_round(0.0, 0.0, width, height, 18.0, p.bg);
    renderer.stroke_round(0.5, 0.5, width - 1.0, height - 1.0, 17.5, p.border, 1.0);
    renderer.fill_round(18.0, 1.0, width - 36.0, 1.0, 0.5, p.highlight);

    let pad = 16.0;
    draw_tabs(renderer, tab, height, hover, &p);
    renderer.text_weight(
        VERSION,
        width - pad - 58.0,
        height - TAB_H - 2.0,
        58.0,
        14.0,
        8.5,
        p.secondary,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
    );
    match tab {
        PanelTab::Settings => {
            draw_settings(renderer, config, hover, gpu_names, &p);
            return renderer.end(hwnd);
        }
        PanelTab::Appearance => {
            draw_appearance(renderer, config, hover, &p);
            return renderer.end(hwnd);
        }
        PanelTab::Pomodoro => {
            draw_pomodoro(renderer, pomodoro, config, hover, &p);
            return renderer.end(hwnd);
        }
        PanelTab::Todo => {
            draw_todo(renderer, config, todo_page, hover, &p);
            return renderer.end(hwnd);
        }
        PanelTab::Overview => {}
    }

    let gap = 10.0;
    let top = 16.0;
    let card_width = (width - 2.0 * pad - gap) / 2.0;
    let card_height = (height - top - TAB_BAND - pad - gap) / 2.0;

    let cpu_card = (pad, top, card_width, card_height);
    let mem_card = (pad + card_width + gap, top, card_width, card_height);
    let disk_card = (pad, top + card_height + gap, card_width, card_height);
    let gpu_card = (
        pad + card_width + gap,
        top + card_height + gap,
        card_width,
        card_height,
    );

    draw_card(renderer, cpu_card, &p);
    draw_card(renderer, mem_card, &p);
    draw_card(renderer, disk_card, &p);
    draw_card(renderer, gpu_card, &p);

    let cpu_temp = snapshot
        .cpu_temp
        .map(|temp| format!("{temp:.0}\u{00B0}C"))
        .unwrap_or_else(|| "temp --".to_string());
    let cpu_sub = format!("{cpu_temp} \u{00B7} {} threads", snapshot.cores.len());
    let cpu_sub_color = snapshot
        .cpu_temp
        .map(|temp| temperature_color(temp, &p))
        .unwrap_or(p.secondary);
    draw_card_content(
        renderer,
        cpu_card,
        "CPU",
        &format!("{:.0}%", snapshot.cpu_load),
        &cpu_sub,
        load_text_color(snapshot.cpu_load, &p),
        cpu_sub_color,
        None,
        &p,
    );

    let mem_pct = if snapshot.mem_total > 0 {
        snapshot.mem_used as f32 / snapshot.mem_total as f32
    } else {
        0.0
    };
    let mem_value = if config.mem_mode == "percent" {
        format!("{:.0}%", mem_pct * 100.0)
    } else {
        format!("{:.1}G", gib(snapshot.mem_used))
    };
    let mem_sub = if config.mem_mode == "percent" {
        format!(
            "{:.1}G of {:.1}G",
            gib(snapshot.mem_used),
            gib(snapshot.mem_total)
        )
    } else {
        format!(
            "of {:.1}G \u{00B7} {:.0}% used",
            gib(snapshot.mem_total),
            mem_pct * 100.0
        )
    };
    draw_card_content(
        renderer,
        mem_card,
        "MEMORY",
        &mem_value,
        &mem_sub,
        load_text_color(mem_pct * 100.0, &p),
        p.secondary,
        Some((mem_pct, load_fill_color(mem_pct * 100.0, &p))),
        &p,
    );

    let disk_pct = if snapshot.disk_total > 0 {
        snapshot.disk_used as f32 / snapshot.disk_total as f32
    } else {
        0.0
    };
    let free = snapshot.disk_total.saturating_sub(snapshot.disk_used);
    let disk_value = if config.disk_mode == "percent" {
        format!("{:.0}%", disk_pct * 100.0)
    } else {
        format!("{:.0}G", gib(snapshot.disk_used))
    };
    let disk_temp = snapshot
        .disk_temp
        .map(|temp| format!("{temp:.0}\u{00B0}C"))
        .unwrap_or_else(|| "temp --".to_string());
    let disk_sub_color = snapshot
        .disk_temp
        .map(|temp| temperature_color(temp, &p))
        .unwrap_or(p.secondary);
    draw_card_content(
        renderer,
        disk_card,
        "DISK",
        &disk_value,
        &format!("{:.0}G free \u{00B7} {disk_temp}", gib(free)),
        load_text_color(disk_pct * 100.0, &p),
        disk_sub_color,
        Some((disk_pct, load_fill_color(disk_pct * 100.0, &p))),
        &p,
    );

    if snapshot.gpus.is_empty() {
        draw_card_content(
            renderer,
            gpu_card,
            "GPU",
            "--",
            "not detected",
            p.muted,
            p.secondary,
            None,
            &p,
        );
    } else if snapshot.gpus.len() == 1 {
        let gpu = &snapshot.gpus[0];
        let load = gpu.load_pct.unwrap_or(0.0);
        let gpu_temp = gpu
            .temp_c
            .map(|temp| format!("{temp:.0}\u{00B0}C"))
            .unwrap_or_else(|| "temp --".to_string());
        let sub_color = gpu
            .temp_c
            .map(|temp| temperature_color(temp, &p))
            .unwrap_or(p.secondary);
        let load_text = if gpu.load_pct.is_some() {
            format!("{load:.0}%")
        } else {
            "--".to_string()
        };
        let bar = gpu
            .load_pct
            .map(|load| (load / 100.0, load_fill_color(load, &p)));
        draw_card_content(
            renderer,
            gpu_card,
            &short_name(&gpu.name),
            &load_text,
            &gpu_temp,
            if gpu.load_pct.is_some() {
                load_text_color(load, &p)
            } else {
                p.muted
            },
            sub_color,
            bar,
            &p,
        );
    } else {
        draw_gpu_rows(renderer, gpu_card, &snapshot.gpus, &p);
    }

    draw_network_footer(renderer, cpu_card, snapshot, &p);

    renderer.end(hwnd)
}

fn draw_tabs(
    renderer: &Renderer,
    tab: PanelTab,
    height: f32,
    hover: Option<PanelAction>,
    p: &Palette,
) {
    for (item, action, label) in [
        (PanelTab::Overview, PanelAction::TabOverview, "Overview"),
        (PanelTab::Pomodoro, PanelAction::TabPomodoro, "Pomodoro"),
        (PanelTab::Todo, PanelAction::TabTodo, "Todo"),
        (PanelTab::Settings, PanelAction::TabSettings, "Settings"),
        (
            PanelTab::Appearance,
            PanelAction::TabAppearance,
            "Appearance",
        ),
    ] {
        let (x, y, w, h) = tab_rect(item, height);
        let selected = tab == item;
        let hovered = hover == Some(action);
        if selected || hovered {
            renderer.fill_round(x, y, w, h, 7.0, p.card);
        }
        let color = if selected || hovered { p.text } else { p.muted };
        let weight = if selected {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        renderer.text_weight(
            label,
            x,
            y + 6.0,
            w,
            14.0,
            9.5,
            color,
            weight,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        );
        if selected {
            renderer.fill_round(x + 8.0, y + h - 4.0, w - 16.0, 2.0, 1.0, p.accent);
        }
    }
}

fn draw_settings(
    renderer: &Renderer,
    config: &Config,
    hover: Option<PanelAction>,
    gpu_names: &[String],
    p: &Palette,
) {
    draw_group(renderer, SETTINGS_PAD, SETTINGS_ROW1, "POSITION", p);
    draw_group(renderer, SETTINGS_COL2, SETTINGS_ROW1, "OFFSET", p);
    draw_group(renderer, SETTINGS_PAD, SETTINGS_ROW2, "MEMORY", p);
    draw_group(renderer, SETTINGS_COL2, SETTINGS_ROW2, "DISK", p);
    draw_group(renderer, SETTINGS_PAD, SETTINGS_ROW3, "REFRESH", p);
    draw_group(renderer, SETTINGS_COL2, SETTINGS_ROW3, "HOVER DELAY", p);
    draw_group(renderer, SETTINGS_PAD, SETTINGS_WIDGETS, "WIDGETS", p);
    draw_group(renderer, SETTINGS_PAD, SETTINGS_GPUS, "GPUS", p);

    let position = if config.position == "left" {
        Some(0)
    } else {
        Some(1)
    };
    draw_segments(
        renderer,
        &[PanelAction::PositionLeft, PanelAction::PositionRight],
        &["Left", "Right"],
        position,
        hover,
        gpu_names.len(),
        p,
    );

    let memory = if config.mem_mode == "percent" {
        Some(1)
    } else {
        Some(0)
    };
    draw_segments(
        renderer,
        &[PanelAction::MemoryGb, PanelAction::MemoryPercent],
        &["GB", "%"],
        memory,
        hover,
        gpu_names.len(),
        p,
    );

    let disk = if config.disk_mode == "percent" {
        Some(1)
    } else {
        Some(0)
    };
    draw_segments(
        renderer,
        &[PanelAction::DiskGb, PanelAction::DiskPercent],
        &["GB", "%"],
        disk,
        hover,
        gpu_names.len(),
        p,
    );

    let refresh = match config.update_ms {
        500 => Some(0),
        1000 => Some(1),
        2000 => Some(2),
        _ => None,
    };
    draw_segments(
        renderer,
        &[
            PanelAction::Refresh500,
            PanelAction::Refresh1000,
            PanelAction::Refresh2000,
        ],
        &["0.5s", "1s", "2s"],
        refresh,
        hover,
        gpu_names.len(),
        p,
    );

    let delay = match config.hover_delay_ms {
        0 => Some(0),
        150 => Some(1),
        220 => Some(2),
        400 => Some(3),
        _ => None,
    };
    draw_segments(
        renderer,
        &[
            PanelAction::Hover0,
            PanelAction::Hover150,
            PanelAction::Hover220,
            PanelAction::Hover400,
        ],
        &["0", "150", "220", "400"],
        delay,
        hover,
        gpu_names.len(),
        p,
    );

    if let Some(rect) = settings_rect(PanelAction::OffsetMinus, gpu_names.len()) {
        draw_chip(
            renderer,
            rect,
            "\u{2212}",
            false,
            hover == Some(PanelAction::OffsetMinus),
            p,
            12.0,
        );
    }
    if let Some(rect) = settings_rect(PanelAction::OffsetPlus, gpu_names.len()) {
        draw_chip(
            renderer,
            rect,
            "+",
            false,
            hover == Some(PanelAction::OffsetPlus),
            p,
            12.0,
        );
    }
    renderer.text_weight(
        &format!("{} DIP", config.offset),
        SETTINGS_COL2 + 34.0,
        SETTINGS_ROW1 + 20.0,
        SETTINGS_COL_W - 68.0,
        14.0,
        9.5,
        p.secondary,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_CENTER,
    );

    for (action, label, widget) in [
        (PanelAction::WidgetCpu, "CPU", "cpu"),
        (PanelAction::WidgetMem, "MEM", "mem"),
        (PanelAction::WidgetDisk, "DISK", "disk"),
        (PanelAction::WidgetNet, "NET", "net"),
    ] {
        let Some(rect) = settings_rect(action, gpu_names.len()) else {
            continue;
        };
        let selected = config.widgets.iter().any(|item| item == widget);
        draw_chip(
            renderer,
            rect,
            label,
            selected,
            hover == Some(action),
            p,
            9.0,
        );
    }

    if gpu_names.is_empty() {
        renderer.text_left_weight(
            "No GPUs detected",
            SETTINGS_PAD,
            SETTINGS_GPUS + 18.0,
            160.0,
            14.0,
            9.0,
            p.muted,
            DWRITE_FONT_WEIGHT_NORMAL,
        );
    } else {
        for (index, name) in gpu_names.iter().enumerate() {
            let Some(rect) = settings_rect(PanelAction::Gpu(index), gpu_names.len()) else {
                continue;
            };
            let selected = config.gpu_enabled(name);
            draw_chip(
                renderer,
                rect,
                &short_name(name),
                selected,
                hover == Some(PanelAction::Gpu(index)),
                p,
                9.0,
            );
        }
    }

    if let Some(rect) = settings_rect(PanelAction::Reset, gpu_names.len()) {
        draw_chip(
            renderer,
            rect,
            "Reset defaults",
            false,
            hover == Some(PanelAction::Reset),
            p,
            8.5,
        );
    }
    if let Some(rect) = settings_rect(PanelAction::LaunchAtStartup, gpu_names.len()) {
        draw_chip(
            renderer,
            rect,
            "Start with Windows",
            config.launch_at_startup,
            hover == Some(PanelAction::LaunchAtStartup),
            p,
            8.5,
        );
    }
}

fn draw_appearance(renderer: &Renderer, config: &Config, hover: Option<PanelAction>, p: &Palette) {
    draw_group(renderer, SETTINGS_PAD, APPEARANCE_ROW1, "THEME", p);
    draw_group(renderer, SETTINGS_PAD, APPEARANCE_ROW2, "STYLE", p);
    draw_group(renderer, SETTINGS_COL2, APPEARANCE_ROW2, "ACCENT", p);
    draw_group(renderer, SETTINGS_PAD, APPEARANCE_ROW3, "TEXT SIZE", p);

    let theme = match config.theme.as_str() {
        "system" => Some(0),
        "dark" => Some(1),
        "light" => Some(2),
        "amoled" => Some(3),
        "slate" => Some(4),
        "sand" => Some(5),
        _ => None,
    };
    draw_segments(
        renderer,
        &[
            PanelAction::ThemeSystem,
            PanelAction::ThemeDark,
            PanelAction::ThemeLight,
            PanelAction::ThemeAmoled,
            PanelAction::ThemeSlate,
            PanelAction::ThemeSand,
        ],
        &["System", "Dark", "Light", "AMOLED", "Slate", "Sand"],
        theme,
        hover,
        0,
        p,
    );

    let style = match config.style.as_str() {
        "blur" => Some(1),
        "clear" => Some(2),
        _ => Some(0),
    };
    draw_segments(
        renderer,
        &[
            PanelAction::StyleSolid,
            PanelAction::StyleBlur,
            PanelAction::StyleClear,
        ],
        &["Solid", "Blur", "Clear"],
        style,
        hover,
        0,
        p,
    );

    for (index, hex) in ACCENTS.iter().enumerate() {
        let Some((x, y, w, h)) = settings_rect(PanelAction::Accent(index), 0) else {
            continue;
        };
        let selected = config.accent.eq_ignore_ascii_case(hex);
        let hovered = hover == Some(PanelAction::Accent(index));
        renderer.fill_round(x, y, w, h, 5.0, accent_color(hex));
        if selected || hovered {
            renderer.stroke_round(
                x - 1.0,
                y - 1.0,
                w + 2.0,
                h + 2.0,
                6.0,
                if selected { p.text } else { p.accent },
                1.0,
            );
        }
    }

    let size = if (config.font_scale - 0.9).abs() < 0.01 {
        Some(0)
    } else if (config.font_scale - 1.15).abs() < 0.01 {
        Some(2)
    } else {
        Some(1)
    };
    draw_segments(
        renderer,
        &[
            PanelAction::FontCompact,
            PanelAction::FontNormal,
            PanelAction::FontLarge,
        ],
        &["Compact", "Normal", "Large"],
        size,
        hover,
        0,
        p,
    );
}

fn draw_group(renderer: &Renderer, x: f32, y: f32, label: &str, p: &Palette) {
    renderer.text_left_weight(
        label,
        x,
        y,
        160.0,
        12.0,
        9.0,
        p.muted,
        DWRITE_FONT_WEIGHT_NORMAL,
    );
}

fn draw_segments(
    renderer: &Renderer,
    actions: &[PanelAction],
    labels: &[&str],
    selected: Option<usize>,
    hover: Option<PanelAction>,
    gpu_count: usize,
    p: &Palette,
) {
    for (index, (action, label)) in actions.iter().zip(labels.iter()).enumerate() {
        let Some(rect) = settings_rect(*action, gpu_count) else {
            continue;
        };
        draw_chip(
            renderer,
            rect,
            label,
            selected == Some(index),
            hover == Some(*action),
            p,
            9.0,
        );
    }
}

fn draw_chip(
    renderer: &Renderer,
    rect: Rect,
    label: &str,
    selected: bool,
    hovered: bool,
    p: &Palette,
    size: f32,
) {
    let (x, y, w, h) = rect;
    let fill = if selected || hovered { p.card } else { p.track };
    renderer.fill_round(x, y, w, h, 6.0, fill);
    renderer.stroke_round(x + 0.5, y + 0.5, w - 1.0, h - 1.0, 5.5, p.border, 1.0);
    renderer.text_weight(
        label,
        x,
        y + 6.0,
        w,
        12.0,
        size,
        if selected || hovered {
            p.text
        } else {
            p.secondary
        },
        if selected {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        },
        DWRITE_TEXT_ALIGNMENT_CENTER,
    );
    if selected {
        renderer.fill_round(x + 9.0, y + h - 3.0, w - 18.0, 2.0, 1.0, p.accent);
    }
}

type Card = (f32, f32, f32, f32);

fn draw_card(renderer: &Renderer, card: Card, p: &Palette) {
    renderer.fill_round(card.0, card.1, card.2, card.3, 12.0, p.card);
    renderer.stroke_round(
        card.0 + 0.5,
        card.1 + 0.5,
        card.2 - 1.0,
        card.3 - 1.0,
        11.5,
        p.border,
        1.0,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_card_content(
    renderer: &Renderer,
    card: Card,
    label: &str,
    value: &str,
    sub: &str,
    value_color: D2D1_COLOR_F,
    sub_color: D2D1_COLOR_F,
    bar: Option<(f32, D2D1_COLOR_F)>,
    p: &Palette,
) {
    let x = card.0 + 14.0;
    let y = card.1 + 11.0;
    let width = card.2 - 28.0;
    renderer.text_left_weight(
        label,
        x,
        y,
        width,
        12.0,
        9.0,
        p.muted,
        DWRITE_FONT_WEIGHT_NORMAL,
    );
    renderer.text_left_weight(
        value,
        x,
        y + 15.0,
        width,
        30.0,
        24.0,
        value_color,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
    );
    renderer.text_left_weight(
        sub,
        x,
        y + 47.0,
        width,
        13.0,
        9.5,
        sub_color,
        DWRITE_FONT_WEIGHT_NORMAL,
    );

    if let Some((fraction, bar_color)) = bar {
        let bar_y = card.1 + card.3 - 14.0;
        let bar_w = card.2 - 28.0;
        renderer.fill_round(x, bar_y, bar_w, 3.0, 1.5, p.track);
        let fill = (bar_w * fraction.clamp(0.0, 1.0)).max(3.0);
        renderer.fill_round(x, bar_y, fill, 3.0, 1.5, bar_color);
    }
}

fn draw_gpu_rows(renderer: &Renderer, card: Card, gpus: &[GpuInfo], p: &Palette) {
    let x = card.0 + 14.0;
    let y = card.1 + 11.0;
    let width = card.2 - 28.0;
    renderer.text_left_weight(
        "GRAPHICS",
        x,
        y,
        width,
        12.0,
        9.0,
        p.muted,
        DWRITE_FONT_WEIGHT_NORMAL,
    );

    for (index, gpu) in gpus.iter().take(3).enumerate() {
        let row_y = y + 20.0 + index as f32 * 18.0;
        let load = gpu.load_pct.unwrap_or(0.0);
        renderer.text_left_weight(
            &short_name(&gpu.name),
            x,
            row_y,
            width - 68.0,
            11.0,
            9.5,
            p.secondary,
            DWRITE_FONT_WEIGHT_NORMAL,
        );
        let load_text = if gpu.load_pct.is_some() {
            format!("{load:.0}%")
        } else {
            "--".to_string()
        };
        renderer.text_weight(
            &load_text,
            x + width - 68.0,
            row_y,
            34.0,
            11.0,
            10.0,
            if gpu.load_pct.is_some() {
                load_text_color(load, p)
            } else {
                p.muted
            },
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        );
        let temp = gpu
            .temp_c
            .map(|temp| format!("{temp:.0}\u{00B0}"))
            .unwrap_or_else(|| "--\u{00B0}".to_string());
        let temp_color = gpu
            .temp_c
            .map(|temp| temperature_color(temp, p))
            .unwrap_or(p.muted);
        renderer.text_weight(
            &temp,
            x + width - 30.0,
            row_y,
            30.0,
            11.0,
            9.5,
            temp_color,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        );

        let bar_y = row_y + 12.0;
        renderer.fill_round(x, bar_y, width, 2.0, 1.0, p.track);
        if gpu.load_pct.is_some() {
            let fill = (width * (load / 100.0).clamp(0.0, 1.0)).max(2.0);
            renderer.fill_round(x, bar_y, fill, 2.0, 1.0, load_fill_color(load, p));
        }
    }

    let hottest = gpus
        .iter()
        .filter_map(|gpu| gpu.temp_c)
        .fold(None, |hottest, temp| {
            Some(hottest.map_or(temp, |value: f32| value.max(temp)))
        });
    let summary = match hottest {
        Some(temp) => format!("{} adapters \u{00B7} max {temp:.0}\u{00B0}", gpus.len()),
        None => format!("{} adapters", gpus.len()),
    };
    renderer.text_left_weight(
        &summary,
        x,
        card.1 + card.3 - 16.0,
        width,
        10.0,
        9.0,
        p.muted,
        DWRITE_FONT_WEIGHT_NORMAL,
    );
}

fn draw_network_footer(renderer: &Renderer, card: Card, snapshot: &Snapshot, p: &Palette) {
    let x = card.0 + 14.0;
    let y = card.1 + card.3 - 17.0;
    let width = card.2 - 28.0;
    renderer.text_left_weight(
        "NET",
        x,
        y,
        width,
        11.0,
        8.5,
        p.muted,
        DWRITE_FONT_WEIGHT_NORMAL,
    );
    renderer.text_weight(
        &format!(
            "\u{2193} {}  \u{2191} {}",
            format_rate(snapshot.net_down_bps),
            format_rate(snapshot.net_up_bps)
        ),
        x + 26.0,
        y,
        width - 26.0,
        11.0,
        9.0,
        p.secondary,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
    );
}

fn draw_pomodoro(
    renderer: &Renderer,
    pomodoro: &Pomodoro,
    config: &Config,
    hover: Option<PanelAction>,
    p: &Palette,
) {
    draw_group(renderer, SETTINGS_PAD, 16.0, "POMODORO", p);
    draw_card(renderer, (16.0, 30.0, 368.0, 96.0), p);

    renderer.text_weight(
        pomodoro.phase_label(),
        16.0,
        38.0,
        368.0,
        14.0,
        9.0,
        if pomodoro.phase == crate::pomodoro::Phase::Focus {
            p.accent
        } else {
            p.warn
        },
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_CENTER,
    );
    let time = format_time(pomodoro.remaining_secs());
    renderer.text_weight(
        &time,
        16.0,
        52.0,
        368.0,
        58.0,
        42.0,
        p.text,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_CENTER,
    );
    let sub = if pomodoro.running {
        "Running \u{00B7} notifications on"
    } else {
        "Ready when you are"
    };
    renderer.text_weight(
        sub,
        16.0,
        106.0,
        368.0,
        13.0,
        8.5,
        p.secondary,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_CENTER,
    );

    let progress = pomodoro.progress(config);
    let bar_x = 16.0;
    let bar_y = 140.0;
    let bar_w = 368.0;
    renderer.fill_round(bar_x, bar_y, bar_w, 6.0, 3.0, p.track);
    if progress > 0.0 {
        renderer.fill_round(
            bar_x,
            bar_y,
            (bar_w * progress).max(6.0),
            6.0,
            3.0,
            if pomodoro.phase == crate::pomodoro::Phase::Focus {
                p.accent
            } else {
                p.warn
            },
        );
    }

    for (action, label, selected) in [
        (
            PanelAction::PomodoroToggle,
            if pomodoro.running { "Pause" } else { "Start" },
            pomodoro.running,
        ),
        (PanelAction::PomodoroReset, "Reset", false),
        (PanelAction::PomodoroSkip, "Skip", false),
    ] {
        if let Some(rect) = settings_rect(action, 0) {
            draw_chip(
                renderer,
                rect,
                label,
                selected,
                hover == Some(action),
                p,
                9.0,
            );
        }
    }

    renderer.text_left_weight(
        &format!(
            "{} completed \u{00B7} {} min focus \u{00B7} {} min break",
            pomodoro.completed, config.pomodoro_focus_min, config.pomodoro_break_min
        ),
        SETTINGS_PAD,
        210.0,
        368.0,
        13.0,
        8.5,
        p.muted,
        DWRITE_FONT_WEIGHT_NORMAL,
    );

    draw_group(renderer, SETTINGS_PAD, 228.0, "FOCUS LENGTH", p);
    draw_segments(
        renderer,
        &[PanelAction::PomodoroFocus25, PanelAction::PomodoroFocus50],
        &["25 min", "50 min"],
        match config.pomodoro_focus_min {
            25 => Some(0),
            50 => Some(1),
            _ => None,
        },
        hover,
        0,
        p,
    );

    draw_group(renderer, SETTINGS_PAD, 276.0, "BREAK LENGTH", p);
    draw_segments(
        renderer,
        &[PanelAction::PomodoroBreak5, PanelAction::PomodoroBreak10],
        &["5 min", "10 min"],
        match config.pomodoro_break_min {
            5 => Some(0),
            10 => Some(1),
            _ => None,
        },
        hover,
        0,
        p,
    );
}

fn draw_todo(
    renderer: &Renderer,
    config: &Config,
    page: usize,
    hover: Option<PanelAction>,
    p: &Palette,
) {
    draw_group(renderer, SETTINGS_PAD, 16.0, "TASKS", p);
    let open = config.todos.iter().filter(|todo| !todo.done).count();
    renderer.text_weight(
        &format!("{open} open \u{00B7} {} total", config.todos.len()),
        240.0,
        16.0,
        144.0,
        12.0,
        8.5,
        p.muted,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
    );

    let pages = config.todos.len().div_ceil(TODO_ROWS).max(1);
    let page = page.min(pages - 1);
    let start = page * TODO_ROWS;

    if config.todos.is_empty() {
        renderer.text_left_weight(
            "No tasks yet. Add one to keep your focus.",
            SETTINGS_PAD,
            44.0,
            368.0,
            14.0,
            9.5,
            p.secondary,
            DWRITE_FONT_WEIGHT_NORMAL,
        );
    }

    for (index, todo) in config.todos.iter().skip(start).take(TODO_ROWS).enumerate() {
        let y = 40.0 + index as f32 * 34.0;
        let hovered = hover == Some(PanelAction::TodoToggle(index));
        if hovered {
            renderer.fill_round(SETTINGS_PAD, y, 368.0, 28.0, 6.0, p.card);
        }
        let box_color = if todo.done { p.accent } else { p.border };
        renderer.fill_round(SETTINGS_PAD, y + 7.0, 16.0, 16.0, 4.0, box_color);
        if todo.done {
            renderer.text_weight(
                "\u{2713}",
                SETTINGS_PAD,
                y + 7.0,
                16.0,
                16.0,
                10.0,
                p.bg,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
            );
        }
        let text = if todo.text.chars().count() > 46 {
            todo.text.chars().take(45).collect::<String>() + "\u{2026}"
        } else {
            todo.text.clone()
        };
        renderer.text_left_weight(
            &text,
            SETTINGS_PAD + 24.0,
            y + 7.0,
            294.0,
            16.0,
            9.5,
            if todo.done { p.muted } else { p.text },
            DWRITE_FONT_WEIGHT_NORMAL,
        );
        renderer.text_weight(
            "\u{00D7}",
            354.0,
            y + 4.0,
            30.0,
            20.0,
            14.0,
            if hover == Some(PanelAction::TodoDelete(index)) {
                p.bad
            } else {
                p.muted
            },
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        );
    }

    if let Some(rect) = settings_rect(PanelAction::TodoAdd, 0) {
        draw_chip(
            renderer,
            rect,
            "Add task",
            false,
            hover == Some(PanelAction::TodoAdd),
            p,
            8.5,
        );
    }
    if let Some(rect) = settings_rect(PanelAction::TodoClearDone, 0) {
        draw_chip(
            renderer,
            rect,
            "Clear completed",
            false,
            hover == Some(PanelAction::TodoClearDone),
            p,
            8.5,
        );
    }
    if pages > 1 {
        if let Some(rect) = settings_rect(PanelAction::TodoPrevPage, 0) {
            draw_chip(
                renderer,
                rect,
                "\u{2039}",
                false,
                hover == Some(PanelAction::TodoPrevPage),
                p,
                12.0,
            );
        }
        renderer.text_weight(
            &format!("{}/{}", page + 1, pages),
            314.0,
            218.0,
            34.0,
            20.0,
            8.5,
            p.secondary,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        );
        if let Some(rect) = settings_rect(PanelAction::TodoNextPage, 0) {
            draw_chip(
                renderer,
                rect,
                "\u{203A}",
                false,
                hover == Some(PanelAction::TodoNextPage),
                p,
                12.0,
            );
        }
    }
}

fn format_time(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn format_rate(bytes_per_second: f64) -> String {
    let value = bytes_per_second.max(0.0);
    if value < 1_000.0 {
        format!("{value:.0}B")
    } else if value < 1_000_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else if value < 1_000_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else {
        format!("{:.1}G", value / 1_000_000_000.0)
    }
}

fn palette(light: bool, theme: &str, style: &str, accent_hex: &str) -> Palette {
    let accent = accent_color(accent_hex);
    let mut p = match theme {
        "amoled" => Palette {
            bg: rgb(0, 0, 0, 0.99),
            card: rgb(255, 255, 255, 0.055),
            border: rgb(255, 255, 255, 0.06),
            track: rgb(255, 255, 255, 0.08),
            muted: rgb(150, 150, 158, 1.0),
            secondary: rgb(195, 195, 202, 1.0),
            text: rgb(255, 255, 255, 1.0),
            accent,
            warn: rgb(250, 170, 64, 1.0),
            bad: rgb(250, 94, 94, 1.0),
            highlight: rgb(255, 255, 255, 0.07),
        },
        "slate" => Palette {
            bg: rgb(24, 28, 36, 0.985),
            card: rgb(255, 255, 255, 0.045),
            border: rgb(255, 255, 255, 0.05),
            track: rgb(255, 255, 255, 0.075),
            muted: rgb(138, 148, 162, 1.0),
            secondary: rgb(184, 193, 207, 1.0),
            text: rgb(240, 244, 250, 1.0),
            accent,
            warn: rgb(245, 166, 65, 1.0),
            bad: rgb(245, 92, 92, 1.0),
            highlight: rgb(255, 255, 255, 0.06),
        },
        "sand" => Palette {
            bg: rgb(247, 242, 232, 0.98),
            card: rgb(0, 0, 0, 0.035),
            border: rgb(0, 0, 0, 0.055),
            track: rgb(0, 0, 0, 0.07),
            muted: rgb(116, 106, 94, 1.0),
            secondary: rgb(82, 72, 60, 1.0),
            text: rgb(30, 25, 20, 1.0),
            accent,
            warn: rgb(190, 110, 18, 1.0),
            bad: rgb(190, 45, 45, 1.0),
            highlight: rgb(255, 255, 255, 0.9),
        },
        _ if light => Palette {
            bg: rgb(250, 250, 252, 0.98),
            card: rgb(0, 0, 0, 0.035),
            border: rgb(0, 0, 0, 0.055),
            track: rgb(0, 0, 0, 0.07),
            muted: rgb(105, 105, 115, 1.0),
            secondary: rgb(70, 70, 80, 1.0),
            text: rgb(24, 24, 28, 1.0),
            accent,
            warn: rgb(196, 118, 20, 1.0),
            bad: rgb(196, 48, 48, 1.0),
            highlight: rgb(255, 255, 255, 0.9),
        },
        _ => Palette {
            bg: rgb(19, 19, 23, 0.985),
            card: rgb(255, 255, 255, 0.04),
            border: rgb(255, 255, 255, 0.05),
            track: rgb(255, 255, 255, 0.07),
            muted: rgb(145, 145, 155, 1.0),
            secondary: rgb(185, 185, 195, 1.0),
            text: rgb(244, 244, 248, 1.0),
            accent,
            warn: rgb(245, 166, 65, 1.0),
            bad: rgb(245, 92, 92, 1.0),
            highlight: rgb(255, 255, 255, 0.055),
        },
    };

    match style {
        "blur" => {
            p.bg.a = 0.62;
            if light {
                p.card = rgb(255, 255, 255, 0.30);
                p.border = rgb(0, 0, 0, 0.075);
                p.track = rgb(0, 0, 0, 0.10);
                p.highlight = rgb(255, 255, 255, 0.70);
            } else {
                p.card = rgb(255, 255, 255, 0.075);
                p.border = rgb(255, 255, 255, 0.05);
                p.track = rgb(255, 255, 255, 0.10);
                p.highlight = rgb(255, 255, 255, 0.12);
            }
        }
        "clear" => {
            p.bg.a = if light { 0.80 } else { 0.78 };
            p.card = if light {
                rgb(0, 0, 0, 0.10)
            } else {
                rgb(255, 255, 255, 0.09)
            };
            p.border = if light {
                rgb(0, 0, 0, 0.08)
            } else {
                rgb(255, 255, 255, 0.08)
            };
            p.track = if light {
                rgb(0, 0, 0, 0.09)
            } else {
                rgb(255, 255, 255, 0.10)
            };
        }
        _ => {}
    }
    p
}

fn temperature_color(temp: f32, p: &Palette) -> D2D1_COLOR_F {
    if temp >= 90.0 {
        p.bad
    } else if temp >= 80.0 {
        p.warn
    } else {
        p.secondary
    }
}

fn load_text_color(value: f32, p: &Palette) -> D2D1_COLOR_F {
    if value >= 95.0 {
        p.bad
    } else if value >= 85.0 {
        p.warn
    } else {
        p.text
    }
}

fn load_fill_color(value: f32, p: &Palette) -> D2D1_COLOR_F {
    if value >= 95.0 {
        p.bad
    } else if value >= 85.0 {
        p.warn
    } else {
        p.accent
    }
}

fn rgb(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a,
    }
}

fn accent_color(hex: &str) -> D2D1_COLOR_F {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return rgb(91, 156, 255, 1.0);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(91);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(156);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    rgb(r, g, b, 1.0)
}

fn short_name(name: &str) -> String {
    let mut name = name.trim().to_string();
    loop {
        let mut changed = false;
        for prefix in [
            "NVIDIA ",
            "GeForce ",
            "AMD ",
            "Radeon ",
            "Intel(R) ",
            "Intel ",
        ] {
            if let Some(rest) = name.strip_prefix(prefix) {
                name = rest.to_string();
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    name = name.replace("(R)", "").replace("(TM)", "");
    name = name.replace(" Graphics", "");
    let name = name.trim();
    if name.chars().count() > 16 {
        name.chars().take(15).collect::<String>() + "\u{2026}"
    } else {
        name.to_string()
    }
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

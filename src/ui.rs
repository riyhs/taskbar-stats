use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_TRAILING,
};
use windows::Win32::Foundation::HWND;
use windows::core::Result;

use crate::config::Config;
use crate::gpu::GpuInfo;
use crate::metrics::Snapshot;
use crate::render::Renderer;

const STRIP_PAD: f32 = 4.0;
const STRIP_INNER: f32 = 6.0;
const STRIP_GAP: f32 = 10.0;
const STRIP_RADIUS: f32 = 12.0;
const STRIP_CELL_W: f32 = 62.0;
const LABEL_SIZE: f32 = 9.0;
const TEMP_SIZE: f32 = 9.0;

pub fn strip_width_dip(config: &Config, gpu_count: usize) -> i32 {
    let count = strip_cell_count(config, gpu_count) as f32;
    (2.0 * STRIP_PAD
        + 2.0 * STRIP_INNER
        + count * STRIP_CELL_W
        + (count - 1.0) * STRIP_GAP)
        .round() as i32
}

fn strip_cell_count(config: &Config, gpu_count: usize) -> usize {
    config
        .widgets
        .iter()
        .map(|widget| {
            if widget == "gpu" {
                gpu_count.max(1)
            } else {
                1
            }
        })
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
    good: D2D1_COLOR_F,
    warn: D2D1_COLOR_F,
    bad: D2D1_COLOR_F,
    highlight: D2D1_COLOR_F,
}

struct StripCell {
    label: String,
    value: String,
    temp: Option<f32>,
    value_color: D2D1_COLOR_F,
    bar_color: D2D1_COLOR_F,
    fraction: f32,
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
    let p = palette(light, &config.accent);
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

        let temp_text = cell
            .temp
            .map(|temp| format!("{temp:.0}\u{00B0}"))
            .unwrap_or_else(|| "--\u{00B0}".to_string());
        let temp_color = cell
            .temp
            .map(|temp| temperature_color(temp, &p))
            .unwrap_or(p.muted);
        let temp_x = x + label_advance(&cell.label, label_size) + 4.0;
        renderer.text_left_weight(
            &temp_text,
            temp_x,
            label_y + 0.5,
            (x + cell_width - temp_x).max(0.0),
            10.0,
            TEMP_SIZE,
            temp_color,
            DWRITE_FONT_WEIGHT_NORMAL,
        );

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

        renderer.fill_round(x, bar_y, cell_width, bar_h, bar_h * 0.5, p.track);
        if cell.fraction > 0.0 {
            let fill = (cell_width * cell.fraction.clamp(0.0, 1.0)).max(bar_h);
            renderer.fill_round(x, bar_y, fill, bar_h, bar_h * 0.5, cell.bar_color);
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
                temp: snapshot.cpu_temp,
                value_color: load_text_color(snapshot.cpu_load, p),
                bar_color: load_fill_color(snapshot.cpu_load, p),
                fraction: snapshot.cpu_load / 100.0,
            }),
            "gpu" => {
                if snapshot.gpus.is_empty() {
                    cells.push(StripCell {
                        label: "GPU".into(),
                        value: "--".into(),
                        temp: None,
                        value_color: p.muted,
                        bar_color: p.muted,
                        fraction: 0.0,
                    });
                } else {
                    let multiple = snapshot.gpus.len() > 1;
                    for gpu in &snapshot.gpus {
                        let load = gpu.load_pct.unwrap_or(0.0);
                        cells.push(StripCell {
                            label: if multiple {
                                gpu.vendor.clone()
                            } else {
                                "GPU".into()
                            },
                            value: if gpu.load_pct.is_some() {
                                format!("{load:.0}%")
                            } else {
                                "--".into()
                            },
                            temp: gpu.temp_c,
                            value_color: if gpu.load_pct.is_some() {
                                load_text_color(load, p)
                            } else {
                                p.muted
                            },
                            bar_color: load_fill_color(load, p),
                            fraction: load / 100.0,
                        });
                    }
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
                    temp: snapshot.cpu_temp,
                    value_color: color,
                    bar_color: color,
                    fraction: snapshot.cpu_temp.unwrap_or(0.0) / 100.0,
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
                    temp: gpu.and_then(|gpu| gpu.temp_c),
                    value_color: color,
                    bar_color: color,
                    fraction: 0.0,
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
                    temp: snapshot.mem_temp,
                    value_color: load_text_color(fraction * 100.0, p),
                    bar_color: load_fill_color(fraction * 100.0, p),
                    fraction,
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
                    temp: snapshot.disk_temp,
                    value_color: load_text_color(fraction * 100.0, p),
                    bar_color: load_fill_color(fraction * 100.0, p),
                    fraction,
                });
            }
            _ => {}
        }
    }
    if cells.is_empty() {
        cells.push(StripCell {
            label: "CPU".into(),
            value: format!("{:.0}%", snapshot.cpu_load),
            temp: snapshot.cpu_temp,
            value_color: load_text_color(snapshot.cpu_load, p),
            bar_color: load_fill_color(snapshot.cpu_load, p),
            fraction: snapshot.cpu_load / 100.0,
        });
    }
    cells
}

pub fn draw_panel(
    renderer: &Renderer,
    hwnd: HWND,
    snapshot: &Snapshot,
    config: &Config,
    light: bool,
    width: f32,
    height: f32,
) -> Result<()> {
    let p = palette(light, &config.accent);
    renderer.begin()?;

    renderer.fill_round(0.0, 0.0, width, height, 18.0, p.bg);
    renderer.stroke_round(0.5, 0.5, width - 1.0, height - 1.0, 17.5, p.border, 1.0);
    renderer.fill_round(18.0, 1.0, width - 36.0, 1.0, 0.5, p.highlight);

    let (days, hours, minutes) = uptime(snapshot.uptime_secs);
    let uptime_text = if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    };
    let pad = 16.0;
    renderer.text_left_weight(
        "SYSTEM",
        pad,
        13.0,
        120.0,
        14.0,
        9.5,
        p.muted,
        DWRITE_FONT_WEIGHT_NORMAL,
    );
    renderer.text_weight(
        &format!("UP {uptime_text}"),
        width - pad - 96.0,
        13.0,
        96.0,
        14.0,
        9.0,
        p.secondary,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
    );
    let live_x = width - pad - 132.0;
    renderer.fill_circle(live_x - 7.0, 19.5, 2.0, p.good);
    renderer.text_left_weight(
        "LIVE",
        live_x,
        13.0,
        30.0,
        14.0,
        8.5,
        p.good,
        DWRITE_FONT_WEIGHT_NORMAL,
    );

    let gap = 10.0;
    let top = 44.0;
    let card_width = (width - 2.0 * pad - gap) / 2.0;
    let card_height = (height - top - pad - gap) / 2.0;

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
        "DISK C:",
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
            "GPU",
            &load_text,
            &format!("{gpu_temp} \u{00B7} {}", short_name(&gpu.name)),
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

    if !snapshot.cores.is_empty() {
        draw_core_bars(renderer, cpu_card, &snapshot.cores, &p);
    }

    renderer.end(hwnd)
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

fn draw_core_bars(renderer: &Renderer, card: Card, cores: &[f32], p: &Palette) {
    let x = card.0 + 14.0;
    let y = card.1 + card.3 - 14.0;
    let total_w = card.2 - 28.0;
    let n = cores.len().min(16);
    if n == 0 {
        return;
    }
    let gap = 2.0;
    let bar_w = ((total_w - gap * (n as f32 - 1.0)) / n as f32).max(1.0);
    for (i, load) in cores.iter().take(n).enumerate() {
        let bx = x + i as f32 * (bar_w + gap);
        renderer.fill_round(bx, y, bar_w, 4.0, 1.5, p.track);
        let h = (4.0 * (load / 100.0).clamp(0.0, 1.0)).max(1.0);
        renderer.fill_round(
            bx,
            y + 4.0 - h,
            bar_w,
            h,
            1.5,
            load_fill_color(*load, p),
        );
    }
}

fn palette(light: bool, accent_hex: &str) -> Palette {
    let accent = accent_color(accent_hex);
    if light {
        Palette {
            bg: rgb(250, 250, 252, 0.98),
            card: rgb(0, 0, 0, 0.035),
            border: rgb(0, 0, 0, 0.09),
            track: rgb(0, 0, 0, 0.07),
            muted: rgb(105, 105, 115, 1.0),
            secondary: rgb(70, 70, 80, 1.0),
            text: rgb(24, 24, 28, 1.0),
            accent,
            good: rgb(28, 150, 88, 1.0),
            warn: rgb(196, 118, 20, 1.0),
            bad: rgb(196, 48, 48, 1.0),
            highlight: rgb(255, 255, 255, 0.9),
        }
    } else {
        Palette {
            bg: rgb(19, 19, 23, 0.985),
            card: rgb(255, 255, 255, 0.04),
            border: rgb(255, 255, 255, 0.085),
            track: rgb(255, 255, 255, 0.07),
            muted: rgb(145, 145, 155, 1.0),
            secondary: rgb(185, 185, 195, 1.0),
            text: rgb(244, 244, 248, 1.0),
            accent,
            good: rgb(76, 200, 126, 1.0),
            warn: rgb(245, 166, 65, 1.0),
            bad: rgb(245, 92, 92, 1.0),
            highlight: rgb(255, 255, 255, 0.055),
        }
    }
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

fn label_advance(label: &str, size: f32) -> f32 {
    let units = match label {
        "CPU" => 2.05,
        "GPU" => 2.10,
        "MEM" => 2.45,
        "DISK" => 2.30,
        "TEMP" => 2.70,
        "INTEL" => 2.75,
        "NVIDIA" => 3.45,
        "AMD" => 2.15,
        _ => label.chars().count() as f32 * 0.68,
    };
    units * size
}

fn short_name(name: &str) -> String {
    let mut name = name.trim().to_string();
    for prefix in ["NVIDIA ", "GeForce ", "AMD ", "Radeon ", "Intel(R) ", "Intel "] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.to_string();
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

fn uptime(seconds: u64) -> (u64, u64, u64) {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    (days, hours, minutes)
}

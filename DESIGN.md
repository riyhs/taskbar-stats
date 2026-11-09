# Design System

## Intent

Taskbar Stats should feel like a precise native instrument, not a miniature
dashboard pasted onto the taskbar. The strip is for fast scanning; the hover
panel is for confirmation and detail. Calm typography, neutral normal values,
and semantic color matter more than decoration.

Principles:

1. **Glance first.** The largest element in each strip cell is the live value.
2. **One metric, one group.** Keep label, temperature, value, and progress bar
   visually bound to the same cell.
3. **Color earns its place.** Normal values are neutral. Color indicates load,
   heat, capacity, or live state.
4. **Space before lines.** Use gaps and surfaces for grouping. Avoid table-like
   separators in the strip.
5. **Stay inside the taskbar.** The strip is embedded, not floating. The panel
   opens on the inner side of the taskbar and never reserves desktop space.

## Layout

### Taskbar strip

At 96 DPI:

| Token | Value |
| --- | --- |
| Outer inset | 4 px |
| Outer radius | 12 px |
| Inner horizontal padding | 6 px |
| Cell width | 62 px |
| Cell gap | 10 px |
| Track bar | 2.5 px high, full cell width |
| Label baseline area | 9 px text at y ≈ 7.7 px |
| Primary value | 17 px semibold at y ≈ 18.2 px |
| Temperature | 9 px normal, same row as label |
| Strip height | Taskbar height (typically 48 px) |

Strip width is calculated from the number of cells:

```text
width = 2 * 4 + 2 * 6 + cells * 62 + (cells - 1) * 10
```

`gpu` expands to one cell per physical adapter, so five visible cells produce a
370 px strip. Widths scale with the taskbar DPI.

Cell anatomy:

```text
CPU   --°          <- label and temperature, 9 px normal
53%                <- primary value, 17 px semibold
[========------]   <- utilization/capacity bar, 2.5 px
```

The temperature follows the label, separated by a 4 px optical gap. Its color is
secondary text under normal conditions and warning/bad when hot.

### Hover panel

| Token | Value |
| --- | --- |
| Panel size | 400 × 272 DIP |
| Outer radius | 18 px |
| Border | 1 px |
| Header height | 44 px |
| Padding | 16 px |
| Card gap | 10 px |
| Card size | 179 × 101 px |
| Card radius | 12 px |
| Card padding | 14 px |

Layout:

```text
SYSTEM                         ● LIVE   UP 7h 38m

CPU                         MEMORY
53%                         10.9G
temp -- · 8 threads         of 15.8G · 69% used
[core bars]                 [====================]

DISK C:                     GRAPHICS
322G                        Iris Plus   14%   --°
47G free · 49°C             MX350        0%   52°
[====================]      2 adapters · max 52°
```

The panel is centered on the strip, clamped to the monitor work area, and
placed 6 DIP away from the taskbar. On a bottom taskbar it opens above; on a top
taskbar it opens below.

## Typography

Use `Segoe UI` through DirectWrite.

| Role | Size | Weight |
| --- | --- | --- |
| Strip label | 9.0 | 400 |
| Strip temperature | 9.0 | 400 |
| Strip value | 17.0 | 600 |
| Panel header label | 9.5 | 400 |
| Panel live state | 8.5 | 400 |
| Panel uptime | 9.0 | 400 |
| Card label | 9.0 | 400 |
| Card value | 24.0 | 600 |
| Card secondary | 9.5 | 400 |
| GPU row name | 9.5 | 400 |
| GPU row value | 10.0 | 600 |
| GPU row temperature | 9.5 | 400 |
| GPU footer | 9.0 | 400 |

Rules:

- Uppercase is reserved for compact labels (`CPU`, `MEMORY`, `GRAPHICS`).
- Do not use weights below 400 for UI text.
- Keep changing values left-aligned in fixed-width cells; the cell itself
  prevents layout shift.
- Use `·` as the separator in secondary text. Use `--` for unavailable data and
  `--°` where a temperature is expected.
- Display memory and disk in GiB with one decimal or whole numbers as
  appropriate; `G` is an intentional compact unit in the strip.
- Use `°C` in the panel and `°` in the strip.

## Color

Colors are stored as premultiplied Direct2D RGBA values in `src/ui.rs`.
`accent` is the user-configurable `#RRGGBB` value; it defaults to `#5B9CFF`.

### Dark

| Role | Value |
| --- | --- |
| Background | `rgb(19, 19, 23)` at 98.5% |
| Card | white 4% |
| Border | white 8.5% |
| Track | white 7% |
| Muted label | `rgb(145, 145, 155)` |
| Secondary | `rgb(185, 185, 195)` |
| Primary text | `rgb(244, 244, 248)` |
| Good / live | `rgb(76, 200, 126)` |
| Warning | `rgb(245, 166, 65)` |
| Bad | `rgb(245, 92, 92)` |
| Top highlight | white 5.5% |

### Light

| Role | Value |
| --- | --- |
| Background | `rgb(250, 250, 252)` at 98% |
| Card | black 3.5% |
| Border | black 9% |
| Track | black 7% |
| Muted label | `rgb(105, 105, 115)` |
| Secondary | `rgb(70, 70, 80)` |
| Primary text | `rgb(24, 24, 28)` |
| Good / live | `rgb(28, 150, 88)` |
| Warning | `rgb(196, 118, 20)` |
| Bad | `rgb(196, 48, 48)` |
| Top highlight | white 90% |

### Semantic thresholds

| Signal | Normal | Warning | Bad |
| --- | --- | --- | --- |
| Utilization | `< 85%` | `85–94%` | `≥ 95%` |
| Temperature | `< 80°C` | `80–89°C` | `≥ 90°C` |
| Disk capacity | follows utilization thresholds | | |

- Normal load values use primary text.
- Progress fills use accent until the warning threshold.
- Temperatures use secondary text until the warning threshold.
- A high disk fill turns both the value and the bar warning-colored.
- Missing sensors use muted text.
- The live dot uses the good color; `LIVE` is the only green text.

## Components

### Strip cell

- Outer strip surface: background, 1 px border, 12 px radius.
- Label + temperature on the first line.
- Large value on the second line.
- Track bar at the bottom; fill represents load/capacity.
- No per-cell background; spacing provides grouping.
- Multiple GPUs use the adapter vendor as the label (`INTEL`, `NVIDIA`, `AMD`).
- A single GPU uses `GPU`.

### Panel header

- `SYSTEM` on the leading edge, muted.
- `LIVE` with a 2 px good-colored dot on the trailing edge.
- `UP <duration>` right-aligned, secondary.
- No divider; the 44 px header band supplies separation.

### Cards

- Label in muted 9 px.
- Primary value in 24 px semibold, neutral unless warning/bad.
- Secondary line in 9.5 px: temperature, total, free space, or adapter name.
- Memory and disk cards use a 3 px bar at the bottom.
- The CPU card uses one 4 px bar per thread at the bottom instead of a single
  utilization bar.
- The GPU card uses one row per adapter, with name, utilization, temperature,
  and a 2 px mini bar. If there are multiple adapters, a footer summarizes the
  count and highest available temperature.

### Missing data

- Strip temperature: `--°`.
- Panel temperature: `temp --`.
- GPU row temperature: `--°`.
- No GPUs: value `--`, secondary `not detected`.
- No cores or no GPU load: omit the fill; keep the track.

## Interaction

- Hover delay: `hover_delay_ms`, default 220 ms.
- Panel remains while the pointer is over the strip or panel.
- Leaving both starts a 260 ms hide timer.
- Clicking the strip pins the panel; clicking again closes it.
- Clicking the panel pins it; clicking a pinned panel closes it.
- Right-clicking the strip opens `Reload config`, `Open config`, and `Exit`.
- The panel never takes focus (`WS_EX_NOACTIVATE`).
- Current motion: none. State changes are instantaneous. Do not add entrance
  animations to the one-second data refresh; if panel motion is added later,
  keep it short, interruptible, and disabled for reduced-motion preferences
  where the platform exposes them.

## Adaptation and accessibility

- All sizes are DIPs; the renderer receives the taskbar DPI and scales the
  backing surface.
- The panel is clamped to the work area so it cannot run off-screen.
- Text contrast must be checked in both themes. Muted labels are intentionally
  quieter than values but must remain readable at 9 px.
- The strip is a mouse-first taskbar surface; there is currently no keyboard
  focus model or screen-reader representation. Do not remove the right-click
  menu, since it is the only exit/configuration path.
- Keep the right-click target the full strip, not an individual cell.
- Do not reduce the strip below the 48 px taskbar height; the two text lines and
  bar need the full content band.

## Verification checklist

1. Strip at 100%: labels, temperatures, values, and bars align; no cell text
   touches its neighbor.
2. Five-cell layout at 370 px; strip remains clear of the notification area.
3. Hover panel at 400 × 272; all cards fit without clipping.
4. Dark and light themes both retain hierarchy and contrast.
5. `mem_mode` and `disk_mode` in `gb` and `percent` do not overflow cells.
6. Two-GPU layout shows both adapters, mini bars, and the summary footer.
7. Missing temperatures render as `--°`/`temp --`, never as fabricated values.
8. Right-click menu, click-to-pin, and leave-to-hide still work after layout
   changes.

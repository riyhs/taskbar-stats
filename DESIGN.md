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
| Cell width | 72 px |
| Cell gap | 10 px |
| Track bar | 2.5 px high, full cell width |
| Label baseline area | 9 px base text at y ≈ 7 px |
| Primary value | 17 px base semibold at y ≈ 17 px |
| Temperature | 9 px base normal, same row as label |
| Strip height | Taskbar height (typically 48 px) |

Strip width is calculated from the number of cells:

```text
width = 2 * 4 + 2 * 6 + cells * 72 + (cells - 1) * 10
```

`gpu` expands to one cell per physical adapter, so five visible cells produce a
420 px strip. Widths scale with the taskbar DPI.

Cell anatomy:

```text
CPU              --°   <- label left; temperature right, 9 px normal
53%                    <- primary value, 17 px semibold
[========------]       <- utilization/capacity bar, 2.5 px
```

The label is left-aligned. Temperature, upload rate, or other secondary detail
is right-aligned in the cell so large text sizes cannot collide. The detail is
secondary text under normal conditions and warning/bad when hot. The strip text
sizes derive from the taskbar height, and the strip caps the text-size setting
at Normal so the fixed taskbar row always fits; Compact still shrinks it.

### Hover panel

| Token | Value |
| --- | --- |
| Panel width | 400 DIP |
| Side margin | 6 DIP left and right |
| Overview height | 322 DIP at Normal (313 Compact, 336 Large) |
| Pomodoro height | 352 DIP at Normal (332 Compact, 382 Large) |
| Todo height | 290 DIP at Normal (284 Compact, 299 Large) |
| Settings height | 352 DIP at Normal (332 Compact, 382 Large) |
| Appearance height | 208 DIP at Normal (198 Compact, 223 Large) |
| Outer radius | 18 px |
| Border | 1 px |
| Bottom tab bar height | 38 px (30 px tabs plus 8 px bottom padding) |
| Padding | 16 px on every page edge |
| Row step | 48 px at Normal (45 Compact, 53 Large) |
| Card gap | 10 px |
| Card size | 179 × 101 px at Normal (96.5 Compact, 107.75 Large) |
| Card radius | 12 px |
| Card padding | 14 px |

Page height and row geometry scale with the text-size setting through
`Layout`:

```text
t          = font_scale - 1.0
chip_top   = 18 + 13t      chip_h    = 24 + 20t
row_step   = chip_top + chip_h + 6
card_h     = 101 + 45t     todo_step = 34 + 8t
todo_card  = 104 + 28t
```

The strip stays at its base size; the panel honors the full setting.

Layout:

```text
CPU                         MEMORY
53%                         10.9G
temp -- · 8 threads         of 15.8G · 69% used
[============--------]      [====================]

DISK                        GRAPHICS
322G                        Iris Plus   14%   --°
47G free · 49°C             MX350        0%   52°
[====================]      2 adapters · max 52°

NETWORK                              ↓ 12.4M   ↑ 1.2M

Overview  Pomodoro  Todo  Settings  Appearance   v0.1.0
```

The panel is centered on the strip, clamped to the monitor work area, and
placed 6 DIP away from the taskbar. On a bottom taskbar it opens above; on a top
taskbar it opens below.

The strip is a layered taskbar child, not a taskbar band. It does not reserve
layout space, but placement scans visible taskbar siblings, excludes the
notification area and full-width composition surfaces, and chooses the nearest
free gap with a 6 px collision margin. The configured offset is the desired
edge gap; collision avoidance may place the strip farther inward.

## Typography

Use `Segoe UI` through DirectWrite.

| Role | Size at Normal | Weight |
| --- | --- | --- |
| Strip label | 11.0 | 400 |
| Strip detail (temp/network) | 11.0 | 400 |
| Strip value | 19.0 | 600 |
| Panel tab | 11.5 | 400/600 selected |
| Pomodoro time | 42.0 | 600 |
| Todo text | 11.5 | 400 |
| Settings group label | 11.0 | 400 |
| Settings control | 11.0 | 400/600 selected |
| Settings footer | 10.5 | 400 |
| Panel version | 10.5 | 400 |
| Card label | 11.0 | 400 |
| Card value | 26.0 | 600 |
| Card secondary | 11.5 | 400 |
| GPU row name | 11.5 | 400 |
| GPU row value | 12.0 | 600 |
| GPU row temperature | 11.5 | 400 |
| GPU footer | 11.0 | 400 |

The values above already include a +2 DIP global boost applied inside
`Renderer`. The Compact and Large settings multiply every size by 0.9 or 1.15
on top of that.

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
- The Compact / Normal / Large text-size choices scale all text through the
  renderer and scale the panel geometry through `Layout`; do not hard-code
  per-widget sizes.
- Every label passes through `text_fit` (shrinks to a floor) or `truncate`
  (ellipsis). Text must never clip a chip, card, row, or tab; prefer shrinking
  the label over widening the layout.
- Network throughput uses compact `K`, `M`, and `G` suffixes and arrows:
  `↓` download, `↑` upload.

## Color

Colors are stored as premultiplied Direct2D RGBA values in `src/ui.rs`.
`accent` is the user-configurable `#RRGGBB` value; it defaults to `#5B9CFF`.

### Themes

`System` resolves to the Windows light or dark appearance. All themes share the
same typography, semantic thresholds, and sizing.

| Theme | Background | Primary | Secondary | Muted |
| --- | --- | --- | --- | --- |
| Dark | `rgb(19, 19, 23)` at 98.5% | `rgb(244, 244, 248)` | `rgb(185, 185, 195)` | `rgb(145, 145, 155)` |
| Light | `rgb(250, 250, 252)` at 98% | `rgb(24, 24, 28)` | `rgb(70, 70, 80)` | `rgb(105, 105, 115)` |
| AMOLED | `rgb(0, 0, 0)` at 99% | `rgb(255, 255, 255)` | `rgb(195, 195, 202)` | `rgb(150, 150, 158)` |
| Slate | `rgb(24, 28, 36)` at 98.5% | `rgb(240, 244, 250)` | `rgb(184, 193, 207)` | `rgb(138, 148, 162)` |
| Sand | `rgb(247, 242, 232)` at 98% | `rgb(30, 25, 20)` | `rgb(82, 72, 60)` | `rgb(116, 106, 94)` |

Base card, border, and track tokens:

| Theme | Card | Border | Track | Highlight |
| --- | --- | --- | --- | --- |
| Dark | white 4% | white 5% | white 7% | white 5.5% |
| Light | black 3.5% | black 5.5% | black 7% | white 90% |
| AMOLED | white 5.5% | white 6% | white 8% | white 7% |
| Slate | white 4.5% | white 5% | white 7.5% | white 6% |
| Sand | black 3.5% | black 5.5% | black 7% | white 90% |

### Surface styles

Style is independent of theme, so every theme can be Solid, Blur, or Clear.

| Style | Background | Surfaces | Behavior |
| --- | --- | --- | --- |
| Solid | theme background at 98–99% | theme card, border, and track | no desktop capture |
| Blur | theme background at 62% | light: card white 30%, border black 7.5%, track black 10%; dark: card white 7.5%, border white 5%, track white 10% | captures and blurs the desktop behind the panel |
| Clear | theme background at 78–80% | light: card black 10%, border black 8%, track black 9%; dark: card white 9%, border white 8%, track white 10% | translucent, no blur |

Blur uses the theme's own background tint, so AMOLED + Blur stays black,
Slate + Blur stays blue-gray, and Sand + Blur keeps its warm light tint.

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

## Components

### Strip cell

- Outer strip surface: background, 1 px border, 12 px radius.
- Label + temperature on the first line.
- Large value on the second line.
- Track bar at the bottom; fill represents load/capacity.
- No per-cell background; spacing provides grouping.
- GPU cells use the shortened adapter name (`Iris Plus`, `MX350`), matching
  the names in the panel and Settings. Each adapter gets its own cell. A single
  visible adapter uses its name as the card label and its temperature as the
  secondary line.

### Panel tab bar

- `Overview`, `Pomodoro`, `Todo`, `Settings`, and `Appearance` tabs on the
  bottom leading edge. Tabs allocate the available width proportionally
  (weights 8 / 8 / 4 / 8 / 10) with a 44 px minimum and 5 px gaps; each label is
  fitted inside its tab so a long row never clips or pushes the version badge.
- The selected tab uses primary text, a subtle card fill, and a 2 px accent
  underline. Hovering an unselected tab applies the same card fill and primary
  text.
- The current version, such as `v0.1.0`, is right-aligned in the same bottom
  bar, secondary, in a reserved 48 px slot.
- No divider; the 36 px bottom bar supplies separation.
- The bottom anchor keeps the tab strip under the pointer across panel height
  changes.

### Cards

- Label in muted 9 px.
- Primary value in 24 px semibold, neutral unless warning/bad.
- Secondary line in 9.5 px: temperature, total, free space, or adapter name.
- Every card uses a 3 px bar at the bottom.
- The CPU card shows overall load, temperature, thread count, and an overall
  load bar. It does not use per-core mini bars; they were unreadable at card
  scale.
- Network throughput lives in its own 368 × 30 px row below the card grid, with
  a `NETWORK` label and right-aligned `↓`/`↑` rates. It never shares a card with
  another metric.
- The GPU card uses one row per adapter, with name, utilization, temperature,
  and a 2 px mini bar. If there are multiple adapters, a footer summarizes the
  count and highest available temperature.
- Memory never shows a temperature detail.

### Settings tab

The settings surface is a two-column form with a 10 px gap between columns. The
panel is 352 px while Settings is active at Normal text size.

| Row | Left column | Right column |
| --- | --- | --- |
| 1 | Position: Left / Right | Offset: − / value / + |
| 2 | Memory: GB / % | Disk: GB / % |
| 3 | Refresh: 0.5s / 1s / 2s | Hover delay: 0 / 150 / 220 / 400 ms |
| 4 | Widgets: CPU / MEM / DISK / NET | |
| 5 | GPUs: one chip per detected adapter | |
| Footer | Start with Windows | Reset defaults |

- Group labels are muted uppercase at 11 px base.
- Controls are 24 px high with a 6 px radius at Normal, and grow with the text
  setting.
- Unselected controls use the track fill, soft card border, and secondary text.
- Selected controls use a card fill, primary semibold text, and a 2 px accent
  underline; hovered controls use the card fill and primary text.
- Offset changes by 2 DIP per press and is clamped to −4096…4096.
- Every change applies immediately and writes the config file.
- Each GPU chip toggles that adapter independently; deselecting every adapter
  removes the GPU widget, and selecting one restores it.
- The footer toggle writes or removes `TaskbarStats` in the current user's Run
  key.
- `Reset defaults` restores the default config and updates the UI immediately.

### Appearance tab

The appearance surface keeps visual choices separate from behavior settings. The
panel is 208 px while Appearance is active at Normal text size.

| Row | Left column | Right column |
| --- | --- | --- |
| 1 | Theme: System / Dark / Light / AMOLED / Slate / Sand (full width) | |
| 2 | Style: Solid / Blur / Clear | Accent: six swatches |
| 3 | Text size: Compact / Normal / Large (full width) | |

- Theme controls the color palette; style controls Solid, Blur, or Clear
  surfaces.
- Theme and style are independent selections.
- Accent swatches update progress fills immediately.
- Accent swatches are 20 px; the selected swatch gets a 1 px primary ring,
  hovered swatches get an accent ring.
- Text size scales all renderer text and every panel metric, and writes
  `font_scale` (`0.9`, `1.0`, or `1.15`). Long labels shrink or ellipsize
  instead of overflowing their control.
- Every change applies immediately and writes the config file.

### Pomodoro tab

- Large `MM:SS` countdown in a 368 × 104 card at y 32 (101-108 px as text
  scales), with phase, running state, and a 6 px progress bar directly below
  the card.
- `Start/Pause`, `Reset`, and `Skip` controls are 24 px high at Normal and share
  the 48 px control grid.
- Focus and break presets are separate two-chip rows.
- The completed-session count is secondary text below the controls.
- At phase end, switch phases and send a tray balloon notification. The break
  starts automatically; the next focus session starts paused.

### Todo tab

- Five 30 px rows at a 34-35 px pitch from y 36; each row has a 16 px checkbox,
  truncated task text (ellipsis measured against the row), and a delete `×`.
- `Add task` opens a themed 360 × 164 dialog: rounded surface, muted `NEW TASK`
  label, inset text field, hint text, and Cancel/Add controls. The dialog uses
  the active theme palette and the same accent/contrast rules as the panel. Its
  text scales with the text setting; the hint and buttons fit instead of
  clipping.
- `Clear completed` removes done tasks.
- Page controls appear only when more than one page is needed.
- Up to 20 tasks are stored; empty text is discarded by config validation.

### Missing data

- Strip temperature: `--°`.
- Panel temperature: `temp --`.
- GPU row temperature: `--°`.
- Memory: no temperature detail at all.
- Network: `0B` when no traffic or no counter is available.
- No GPUs: value `--`, secondary `not detected`.
- No GPU load: omit the fill; keep the track.

## Interaction

- Hover delay: `hover_delay_ms`, default 220 ms.
- Panel remains while the pointer is over the strip or panel.
- Leaving both starts a 260 ms hide timer.
- Clicking the strip toggles the panel: hidden opens it pinned, visible closes
  it.
- Middle-clicking the strip opens Settings pinned.
- Dragging the strip horizontally repositions it; on release the position and
  offset are saved and snapped into the nearest free taskbar gap.
- Opening any tab, including Settings, Appearance, Pomodoro, or Todo, does not
  pin a hover-opened panel.
- Tabs stay at the bottom, so switching tabs never moves the tab bar out from
  under the pointer even when the panel height changes.
- An open panel reuses its current blurred backdrop when resizing; do not hide
  and recapture on tab changes.
- Clicking non-button panel space does not pin a hover-opened panel; it keeps
  the panel unpinned so leaving still starts the hide timer. On a pinned panel,
  the same click closes it.
- Leaving an unpinned panel, including Settings, Appearance, Pomodoro, or Todo,
  starts the hide timer.
- Settings and Appearance controls apply changes immediately and save automatically.
- Right-clicking the strip or tray icon opens `Settings`, `Reload config`, and
  `Exit`.
- The tray icon stays present while the app runs. Left-click opens the panel.
- The panel never takes focus (`WS_EX_NOACTIVATE`) except for the short-lived
  Add task dialog, which is a normal top-level window.
- Pomodoro notifications use the tray icon and a 10 second balloon timeout.
- The panel fades in over 140 ms and fades out over 110 ms, with an ease-out
  curve. The hide timer is 260 ms. Motion is skipped when the Windows
  `SPI_GETCLIENTAREAANIMATION` setting is disabled. Do not animate the
  one-second data refresh.

## Adaptation and accessibility

- All sizes are DIPs; the renderer receives the taskbar DPI and scales the
  backing surface.
- The panel is clamped to the work area so it cannot run off-screen.
- Text contrast must be checked in every theme. Muted labels are intentionally
  quieter than values but must remain readable at 9 px.
- The strip is a mouse-first taskbar surface; there is currently no keyboard
  focus model or screen-reader representation. Keep the right-click menu and
  hover-out close available as exit paths.
- Keep the right-click target the full strip, not an individual cell.
- Do not reduce the strip below the 48 px taskbar height; the two text lines and
  bar need the full content band.

## Verification checklist

1. Strip at 100%: labels, temperatures, values, and bars align; no cell text
   touches its neighbor.
2. Five-cell layout at 420 px; strip remains clear of the notification area.
3. Overview at 400 × 322, Pomodoro at 400 × 352, Todo at 400 × 290, Settings at
   400 × 352, and Appearance at 400 × 208 at Normal; every page also fits at
   Compact and Large without clipping or a clipped tab,
   with 6 DIP side margins and 16 px top/bottom padding above the tab bar.
4. System, Dark, Light, AMOLED, Slate, and Sand themes retain hierarchy and
   contrast in Solid, Blur, and Clear styles.
5. `mem_mode` and `disk_mode` in `gb` and `percent` do not overflow cells.
6. Two-GPU layout shows both adapters as separate cells/chips, mini bars, and
   the summary footer.
7. Missing temperatures render as `--°`/`temp --`, never as fabricated values.
8. Right-click menu, click-to-toggle, and leave-to-hide still work after layout
   changes.
9. Settings and Appearance controls apply live, save automatically, and
   survive close/reopen.
10. Position, offset, widget visibility (including NET), per-GPU visibility,
    theme, style, accent, text size, refresh, hover delay, memory mode, disk
    mode, startup, reset, GPU toggles, Pomodoro controls, and Todo controls
    work.

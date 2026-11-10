# Architecture

## Overview

Taskbar Stats is a single-process Windows desktop application written in Rust.
It hosts a layered strip inside the Explorer taskbar and a layered detail panel
above the taskbar. All sensors, rendering, configuration, and input handling run
in the same process on the UI thread.

There is no Windows service, kernel driver, DLL injection, COM server, network
listener, or elevation requirement.

## Process startup

`src/main.rs`:

1. Initializes the append-only log.
2. Installs a panic hook that records the panic before the default handler.
3. Enables per-monitor DPI awareness V2.
4. Initializes COM as apartment-threaded for the Win32 APIs in use.
5. Calls `app::run()`.

`app::run()`:

1. Creates the `Local\TaskbarStats.SingleInstance` mutex. A second instance
   exits immediately.
2. Registers the host, strip, and popup window classes.
3. Creates:
   - a hidden host window for timers and top-level messages,
   - a layered strip window,
   - a layered topmost popup window.
4. Loads config from `%APPDATA%\TaskbarStats\config.toml`.
5. Initializes the sampler and two Direct2D renderers.
6. Finds and attaches to `Shell_TrayWnd`.
7. Starts the update timer and enters the message loop.

## Window model

```text
Shell_TrayWnd (Explorer taskbar)
└── strip window
    WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS
    WS_EX_LAYERED | WS_EX_NOPARENTNOTIFY

hidden host window
    receives update, attach, theme, and display-change messages

popup window
    WS_POPUP | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE
```

The strip is attached with `SetParent(strip, Shell_TrayWnd)`. It overlays the
taskbar rather than reserving layout space, so other taskbar buttons are not
pushed aside. The popup is independent so it can extend above the taskbar
without being clipped by the taskbar parent.

## Placement

`src/taskbar.rs` locates:

- `Shell_TrayWnd` and its client rectangle,
- the taskbar DPI,
- `TrayNotifyWnd` and its left edge.

Strip placement:

- Enumerate visible child windows of `Shell_TrayWnd`.
- Exclude the strip itself, `TrayNotifyWnd`, and full-width composition surfaces.
- Merge the remaining rectangles, inflate them by a 6 px collision gap, and
  compute free gaps.
- `position = "right"`: choose the free gap whose right edge is closest to
  `tray_left - offset`.
- `position = "left"`: choose the free gap whose left edge is closest to
  `offset`.
- If no gap fits, fall back to the edge-anchored position.
- The result is clamped to the taskbar width.
- The strip height is the taskbar height; the UI draws inside a 4 px inset.

This still overlays the taskbar rather than reserving layout space, but it no
longer covers visible sibling widgets.

Strip width is calculated from the widget list and GPU count. A `gpu` widget
contributes one cell per physical adapter.

Popup placement:

- Content width 400 DIP plus 6 DIP transparent side margins; height 272 DIP
  for Overview, 344 DIP for Settings, and 176 DIP for Appearance, scaled by
  taskbar DPI.
- Horizontally centered on the strip and clamped to the monitor work area.
- Placed 6 DIP from the taskbar on its inner side: above a bottom taskbar,
  below a top taskbar.
- Fades use `UpdateLayeredWindow` with a changing `SourceConstantAlpha`;
  `TIMER_ANIM` runs at 15 ms while a fade is active. Reduced-motion preference
  skips the fade.
- `WS_EX_NOACTIVATE` and `WM_MOUSEACTIVATE → MA_NOACTIVATE` keep focus on the
  foreground application.

## Rendering

Each of the strip and popup owns a `Renderer`:

1. Create a Direct2D factory and a shared DirectWrite factory.
2. Create a DC render target with premultiplied alpha and the target DPI.
3. Resize a top-down 32-bit DIB section to the window pixel size.
4. Bind the DIB's memory DC to the render target.
5. Clear to transparent, draw rounded rectangles, text, bars, and dots.
6. `EndDraw`, then present with `UpdateLayeredWindow`.

For the Blur style, `show_popup` briefly hides the popup, captures the desktop
region behind its content with `BitBlt`, applies a separable box blur and a
rounded-corner alpha mask, and stores the result. `draw_panel` draws that
captured backdrop beneath the translucent tint and cards. The blur is sampled
when the panel is shown, resized, or switched to Blur, not on every metric
refresh. Solid and Clear never capture a backdrop.

The UI works in DIPs. `app.rs` converts pixel dimensions back to DIPs before
calling `ui::draw_strip`/`ui::draw_panel`, and converts popup mouse coordinates
back to DIPs before hit testing, so layout constants remain stable across DPI
scales.

UI composition lives in `src/ui.rs`:

- `strip_width_dip` and `strip_cell_count` calculate the strip width.
- `draw_strip` builds cells and draws labels, temperatures, values, and bars.
- `draw_panel` draws the tabbed header and dispatches to the overview cards,
  the settings form, or the appearance form.
- `draw_settings`, `draw_appearance`, `action_at`, and `settings_rect` share
  one fixed control geometry for drawing and hit testing.
- `palette` resolves a color theme (system/dark/light/amoled/slate/sand) and
  then applies a surface style (solid/blur/clear); `palette` and threshold
  helpers are the only places that decide semantic colors.
- `src/render.rs` provides weighted text, rounded fills/strokes, circles, and
  layered-window presentation.

## Timers and input

| Timer | Owner | Purpose |
| --- | --- | --- |
| `TIMER_UPDATE` | hidden host | Sample metrics and redraw the strip/panel |
| `TIMER_ATTACH` | hidden host | Retry attaching if Explorer/taskbar is unavailable |
| `TIMER_HOVER` | strip | Delay opening the hover panel |
| `TIMER_HIDE` | strip | Delay closing the panel after pointer leave |
| `TIMER_ANIM` | hidden host | Drive the 140/110 ms panel fade |

Strip input:

- `WM_MOUSEMOVE`: tracks `WM_MOUSELEAVE` and starts the hover timer.
- Hover timer: opens the panel if the cursor is still over the strip.
- `WM_LBUTTONDOWN` starts a drag; movement past 4 px moves the strip and
  suppresses hover. `WM_LBUTTONUP` saves the dropped position and offset, or
  toggles the panel when the movement stayed below the threshold.
- `WM_MBUTTONUP`: opens Settings pinned.
- `WM_LBUTTONUP`: toggles open/pinned or close.
- `WM_RBUTTONUP`: opens the native context menu.
- `WM_MOUSELEAVE`: starts the hide timer unless pinned.

Popup input:

- Mouse movement cancels a pending hide and updates the hovered control.
- Mouse leave clears hover and starts a hide timer unless pinned.
- Left click resolves `PanelAction` through `ui::action_at`.
  - Tab clicks switch the panel without pinning a hover-opened popup. A short
    settle timer keeps a shrinking panel open long enough for the pointer to
    enter the new tab layout.
  - Settings and Appearance controls mutate the config, save it, and apply
    immediately.
  - Empty panel space leaves an unpinned panel unpinned (so mouse leave still
    hides it) and closes a pinned panel.
- Right click closes.
- The popup never activates.

## Settings and Appearance flow

Settings and Appearance are control surfaces rendered inside the same popup.
The panel height is 272 DIP for Overview, 344 DIP for Settings, and 176 DIP for
Appearance.

1. A click arrives at the popup in physical pixels.
2. `App::panel_point` converts the point to DIPs.
3. `ui::action_at` maps the point to a `PanelAction` using the same geometry as
   the renderer.
4. `App::handle_panel_action` updates the in-memory config.
5. `Config::validate` normalizes the result.
6. `Config::save` writes the TOML file.
7. Theme refresh, strip placement, update timer, strip redraw, and popup redraw
   run as needed. Layout-affecting changes re-center the popup.

Settings include position, offset, memory/disk display mode, refresh interval,
hover delay, visible widgets, per-GPU visibility, and reset. Appearance includes
theme, surface style, and accent. The config file is an implementation detail;
manual editing is not required.

`TaskbarCreated` triggers reattachment after Explorer restarts. Display, DPI,
theme, and settings changes trigger re-placement and redraw. Switching tabs
  uses the same geometry and reruns `show_popup`, so height and blur capture stay
  correct.

## Sampling

`src/metrics.rs` owns a `Sampler` and a `Snapshot`.

```text
Snapshot
├── cpu_load, cpu_temp, cores
├── mem_used, mem_total, mem_temp
├── disk_used, disk_total, disk_temp
├── uptime_secs
└── gpus: Vec<GpuInfo>
```

Sample interval: `update_ms`, default 1000 ms, clamped to 250–5000 ms.

| Metric | Source |
| --- | --- |
| System CPU load | `GetSystemTimes` deltas |
| Per-thread load | `NtQuerySystemInformation(SystemProcessorPerformanceInformation)` deltas |
| Memory | `GlobalMemoryStatusEx` |
| Local disk capacity | Sum `GetDiskFreeSpaceExW` across fixed NTFS/ReFS volumes that report physical disk extents |
| Uptime | `GetTickCount64` |
| CPU temperature | PDH `\Thermal Zone Information(*)\High Precision Temperature`, then `Temperature` |
| GPU enumeration | DXGI `IDXGIFactory1::EnumAdapters1` |
| GPU load | PDH `\GPU Engine(*)\Utilization Percentage` grouped by adapter LUID; NVML preferred for NVIDIA |
| GPU temperature | NVML for NVIDIA; `D3DKMTQueryAdapterInfo(KMTQAITYPE_ADAPTERPERFDATA)` fallback |
| Disk temperature | `IOCTL_STORAGE_QUERY_PROPERTY(StorageDeviceTemperatureProperty)` on the system volume's physical disk |
| Memory temperature | No standard user-mode source; remains `None` |

CPU temperature accepts only plausible values, filters the known 27.8 °C
sentinel, converts tenths-Kelvin when required, and returns the highest valid
thermal zone. GPU and disk temperatures are likewise range-checked; unavailable
values remain `None` rather than defaulting to zero.

`GpuSampler` loads `nvml.dll` only from `System32`
(`LOAD_LIBRARY_SEARCH_SYSTEM32`), resolves function pointers dynamically, and
shuts NVML down before freeing the library.

## Configuration

`src/config.rs` loads `%APPDATA%\TaskbarStats\config.toml`. On first run it
writes defaults. Referenced fields:

```toml
position = "right"
offset = 6
widgets = ["cpu", "gpu", "mem", "disk"]
theme = "system"
style = "solid"
accent = "#5B9CFF"
hover_delay_ms = 220
update_ms = 1000
mem_mode = "gb"
disk_mode = "gb"
gpu_visible = []
```

Validation:

- `position`: `left` or `right`.
- `theme`: `system`, `dark`, `light`, `amoled`, `slate`, or `sand`.
- `style`: `solid`, `blur`, or `clear`.
- `offset`: −4096 to 4096 DIP.
- `hover_delay_ms`: 0–2000 ms.
- `update_ms`: 250–5000 ms.
- `mem_mode`, `disk_mode`: `gb` or `percent`.
- `gpu_visible`: adapter names, empty to show all; duplicate names removed.
- `widgets`: known widgets only, duplicates removed, fallback to the default
  list when empty.
- `accent`: six hex digits, normalized to uppercase.
- The legacy default `["cpu", "temp", "mem", "disk"]` migrates to the GPU-aware
  default.
- The legacy `theme = "glass"` migrates to `theme = "system"` and
  `style = "blur"`.

`Reload config` re-reads the file, re-validates it, repositions the strip if the
widget/GPU count changes, and redraws. The normal configuration path is the
Settings and Appearance tabs, which write the file automatically after every
change.

## Logging

`src/logging.rs` appends timestamped lines to
`%APPDATA%\TaskbarStats\taskbar-stats.log` using a process-wide `Mutex<File>`.
The startup line and each successful strip placement are logged. Render errors,
attachment failures, and fatal errors are also recorded. The panic hook writes
the panic message before the default hook runs.

## Build

- Rust edition 2021, Windows x64 MSVC target.
- `windows` crate 0.58 with explicit Win32/WDK feature gates.
- `serde` + `toml` for configuration.
- `build.rs` embeds `app.manifest` into the executable and strips the `\\?\`
  prefix from the manifest path.
- Release profile enables size optimization, LTO, one codegen unit, abort on
  panic, and symbol stripping.
- `scripts/build.bat` locates Visual Studio, loads the C++ environment, adds
  the local `.toolchain` SDK libraries/build tools when available, then invokes
  Cargo.

## Limitations

- Primary horizontal taskbar only; vertical and secondary-taskbar placement are
  not implemented.
- Free-space detection relies on visible taskbar child rectangles; UIA-only
  widgets and full-screen overlays are not detected.
- The strip overlays the taskbar and does not reserve button space.
- CPU package temperature depends on an ACPI thermal zone or a vendor driver.
  Many laptops expose neither to normal user-mode applications; reading the
  package sensor directly requires elevation and a kernel driver.
- Intel iGPU temperature is unavailable on hardware/driver combinations that
  expose no thermal sensor through WDDM or Level Zero.
- GPU sampling is DXGI-based; NVIDIA benefits from NVML, while other vendors
  depend on WDDM performance data.
- Memory temperature has no standard Windows provider.
- The widget is mouse-first; there is no keyboard navigation or screen-reader
  surface.

## Extension points

- Add a widget in `ui.rs` by extending `StripCell`/`build_strip_cells` and the
  config validation list; update `DESIGN.md`.
- Add a sensor by extending `Snapshot`, populating it in `Sampler::sample`, and
  rendering it through existing card/row primitives.
- Add a new theme or surface style by extending `Palette`, the Settings
  actions, and the validation lists.
- Add placement modes in `taskbar::place` while preserving the child-window
  model and work-area clamping.

# Taskbar Stats

A compact system monitor that lives inside the Windows taskbar. It shows a
configurable strip of live metrics and expands into a detailed panel on hover.
The panel floats above the taskbar, so it never pushes content into the
desktop below. See [DESIGN.md](DESIGN.md) for the visual system and
[ARCHITECTURE.md](ARCHITECTURE.md) for the runtime design.

Current version: `v0.1.0`.

## Build

Requirements:

- Windows 10 or 11, x64
- Rust with the `x86_64-pc-windows-msvc` target
- Visual Studio Build Tools with the C++ workload
- Windows SDK

Build from the repository root:

```bat
scripts\build.bat
```

Release build:

```bat
scripts\build.bat --release
```

The executable is written to `target\debug\taskbar-stats.exe` or
`target\release\taskbar-stats.exe`.

The build script uses `vswhere` when available and falls back to the BuildTools
installation path. If the project-local `.toolchain` directory exists, its
Windows SDK import libraries and build tools are used automatically.

## Run

```bat
target\debug\taskbar-stats.exe
```

The app starts with Windows only if you add it to your startup folder or create
a shortcut in the Startup apps list. It does not install a service or a driver.

## Configuration

Settings are managed from the hover panel. Hover the strip, open the
**Settings** tab, and change values in place. Every change applies immediately
and is saved automatically; no manual file editing is required.

The settings are persisted in:

```text
%APPDATA%\TaskbarStats\config.toml
```

The file is created on first launch and is managed by the app. The settings tab
includes position, offset, theme, accent color, memory/disk display mode,
refresh interval, hover delay, visible widgets, and a reset action.

```toml
position = "right"
offset = 6
widgets = ["cpu", "gpu", "mem", "disk"]
theme = "system"
accent = "#5B9CFF"
hover_delay_ms = 220
update_ms = 1000
mem_mode = "gb"
disk_mode = "gb"
```

- `position`: `"left"` or `"right"`. `"right"` anchors the strip just before
  the notification area.
- `offset`: gap from the chosen edge or from the notification area, in DIPs.
- `widgets`: ordered list. Available widgets:
  - `cpu` — CPU utilization, with CPU temperature beside the label
  - `gpu` — expands to one cell per physical GPU; each cell shows GPU
    utilization and that adapter's temperature
  - `mem` — physical memory usage
  - `disk` — system drive usage, with drive temperature beside the label
  - `temp` / `gputemp` — legacy single-temperature cells, kept for existing
    configs
- `mem_mode` and `disk_mode`: `"gb"` (default) or `"percent"`. The main strip
  number follows this setting; the bar still represents the percentage.
- `theme`: `"system"`, `"dark"`, or `"light"`.
- `accent`: six-digit hex color used for progress fills and the live indicator.
- `hover_delay_ms`: delay before the detail panel appears.
- `update_ms`: sampling interval, clamped to 250–5000 ms.

## Behavior

- Hover the strip to show the detail panel.
- Use the **Overview** and **Settings** tabs in the panel.
- Settings apply live to the strip and panel and save automatically.
- Click **Done** in Settings (or click the strip) to close the panel.
- Right-click the strip for **Settings**, **Reload config**, and **Exit**.
- The strip reattaches after Explorer restarts.
- The app uses only user-mode APIs and does not require administrator rights.

## Sensor notes

- CPU utilization uses system-wide kernel/user/idle counters, with per-core
  counters for the detail panel.
- CPU temperature depends on an ACPI thermal zone exposed by the firmware. Some
  laptops and virtual machines do not expose one; the UI shows `--` rather than
  inventing a value.
- GPUs are enumerated through DXGI. Per-adapter utilization is read from the
  Windows `GPU Engine` performance counters; NVIDIA adapters also use NVML for
  utilization and temperature. Temperature for other adapters uses the WDDM
  adapter performance data when the driver exposes it; unsupported adapters
  show `--`.
- Disk temperature is read through the standard storage temperature property
  (no administrator rights required on tested NVMe hardware). Memory temperature
  is shown as `--` because Windows exposes no standard memory temperature API.
- Memory, disk, and uptime come from standard Windows APIs.

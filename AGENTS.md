# Taskbar Stats — Agent Guide

## Scope

This repository contains a native Windows taskbar system monitor written in
Rust. It uses Win32, Direct2D, DirectWrite, DXGI, PDH, NVML, and storage IOCTLs.
The application runs as a normal user process with no service, kernel driver,
process injection, or administrator requirement.

Read [DESIGN.md](DESIGN.md) before changing the strip or hover panel. Read
[ARCHITECTURE.md](ARCHITECTURE.md) before changing windowing, placement, or
sampling.

## Build

Stop the running executable first. It locks the output binary.

```bat
taskkill /F /IM taskbar-stats.exe
scripts\build.bat --release
```

From Git Bash or MSYS, escape the taskkill switches:

```bash
taskkill //F //IM taskbar-stats.exe
cmd //c 'scripts\build.bat --release'
```

`scripts\build.bat` locates Visual Studio with `vswhere`, loads `vcvars64`, and
uses the project-local `.toolchain` SDK/build tools when present. A plain
`cargo build` may fail to link on machines where the Git `link.exe` shadows the
MSVC linker or the Windows SDK import libraries are not installed.

Output:

- `target\debug\taskbar-stats.exe`
- `target\release\taskbar-stats.exe`

## Run and inspect

```bat
target\release\taskbar-stats.exe
```

Runtime files:

- Configuration: `%APPDATA%\TaskbarStats\config.toml`
- Log: `%APPDATA%\TaskbarStats\taskbar-stats.log`

The app exits through the strip context menu: right-click the strip and choose
**Exit**. Only one instance runs per user session.

## Source map

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Process setup: logging, panic hook, per-monitor DPI, COM |
| `src/app.rs` | Windows, message loop, timers, hover/pin behavior, settings actions, rendering orchestration |
| `src/taskbar.rs` | Find `Shell_TrayWnd`, attach the strip, place it around the tray |
| `src/render.rs` | Direct2D/DirectWrite renderer, DIB, bitmap backdrops, layered-window presentation |
| `src/capture.rs` | Desktop capture, box blur, and rounded-corner mask for the Glass theme |
| `src/ui.rs` | Strip and tabbed panel composition, settings controls/hit testing, palette, text formatting |
| `src/metrics.rs` | Sampling loop and snapshot data |
| `src/gpu.rs` | DXGI adapter enumeration, GPU load and temperature providers |
| `src/disk.rs` | Volume extent checks and physical-disk temperature through storage IOCTLs |
| `src/config.rs` | TOML config load/save, defaults, validation, legacy migration |
| `src/logging.rs` | Append-only runtime log |
| `build.rs` | Embeds the application manifest |
| `scripts/build.bat` | Visual Studio/SDK bootstrap and Cargo invocation |

## Invariants

1. Keep the strip inside the taskbar. It is a layered child of
   `Shell_TrayWnd`; do not replace it with a floating top-level window.
2. Keep the detail panel above the taskbar on a bottom taskbar and inside the
   monitor work area. It must never push or overlay content below the taskbar.
3. Do not introduce a service, kernel driver, DLL injection, or elevation
   requirement. All current sensors are user-mode.
4. Never fabricate a sensor value. Missing CPU, GPU, memory, or disk temperature
   must remain `None` and render as `--` or `--°`.
5. Keep sampling off the UI critical path where practical. The current design
   samples on the UI thread on a timer; new blocking work should not stall the
   message loop.
6. Preserve the single-instance mutex behavior and Explorer-restart recovery.
7. Keep config backward compatible. New fields need serde defaults and
   validation.
8. Keep the strip width deterministic from the configured widgets and visible
   GPU count so placement remains stable.
9. Placement must avoid visible taskbar siblings. Use the occupancy logic in
   `src/taskbar.rs`; keep the 6 px collision gap and do not fall back to
   `tray_left` alone.
10. The Settings tab is the supported configuration path. Every control change
    must validate, save, and apply immediately; do not reintroduce a
    manual-edit-only workflow.

## UI rules

- Follow the tokens and layout values in `DESIGN.md` instead of adding one-off
  colors or font sizes.
- Normal values are neutral. Reserve warning colors for high utilization, hot
  temperatures, and full disks; reserve accent color for progress fills and
  live state.
- Use `Segoe UI`. Labels and secondary text are normal weight; primary values
  are semibold.
- Use 9 px labels and temperatures, 17 px strip values, and 24 px panel values
  at 96 DPI.
- Keep the strip at a 4 px inset, 12 px outer radius, 72 px cells, and 10 px
  inter-cell gaps unless the documented composition changes.
- Keep motion brief and interruptible: the panel fades in over 140 ms and out
  over 110 ms; respect `SPI_GETCLIENTAREAANIMATION`. Do not animate the
  one-second data refresh.
- Verify System, Dark, Light, and Glass palettes, plus `mem_mode`/`disk_mode`
  in both `gb` and `percent` modes.
- Settings controls use the 24 px chip geometry, 6 px radius, card fill,
  accent selected border, and primary semibold selected text.
- Keep the Settings panel at 400 × 392 DIP and the Overview panel at 400 × 272
  DIP unless both layout and hit-testing constants are updated together.
- The popup adds 6 DIP transparent side margins; subtract them before hit
  testing.
- Convert popup mouse coordinates from pixels to DIPs before hit testing.

## Verification

Before calling a change complete:

1. Build with `scripts\build.bat --release`; fix all errors and warnings.
2. Confirm the log contains a successful `strip placed:` line.
3. Capture the strip and the hover panel at 100% scaling.
4. Check System, Dark, Light, and Glass themes.
5. Check multi-GPU layout and per-GPU toggles when more than one adapter is
   present.
6. Check `gb` and `percent` display modes, and confirm disk totals exclude
   cloud-drive letters.
7. Confirm hover show, fade, leave-hide, click toggle, middle-click Settings,
   strip drag/snap, and right-click menu behavior.
8. Confirm left and right placement avoid existing taskbar widgets with a
   6 px gap.
9. Open Settings from the hover tab and from the right-click menu.
10. Exercise every settings control: position, offset, theme, accent, memory
   mode, disk mode, refresh, hover delay, widget toggles, GPU toggles, reset,
   and Reset.
11. Confirm settings survive closing/reopening the panel and that the strip and
    panel update immediately.
12. Remove temporary screenshots, scripts, and logs from the repository root.

## Git hygiene

- `target/`, `.toolchain/`, and `*.log` are ignored.
- Do not commit build products, local toolchain extracts, runtime logs, or
  temporary screenshots.
- Keep commits focused; do not mix unrelated refactors with UI or sensor work.

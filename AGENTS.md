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
| `src/app.rs` | Windows, message loop, timers, hover/pin behavior, tray icon, startup registration, Pomodoro/Todo state, settings actions, rendering orchestration |
| `src/taskbar.rs` | Find `Shell_TrayWnd`, attach the strip, place it around the tray |
| `src/render.rs` | Direct2D/DirectWrite renderer, DIB, bitmap backdrops, layered-window presentation |
| `src/capture.rs` | Desktop capture, box blur, and rounded-corner mask for the Blur style |
| `src/ui.rs` | Strip and five-tab panel composition, Pomodoro/Todo views, Settings/Appearance controls, hit testing, palette, text formatting |
| `src/metrics.rs` | Sampling loop and snapshot data, including CPU, memory, disk, and network counters |
| `src/pomodoro.rs` | Pomodoro phase state, countdown, and completion events |
| `src/gpu.rs` | DXGI adapter enumeration, GPU load and temperature providers |
| `src/disk.rs` | Volume extent checks and physical-disk temperature through storage IOCTLs |
| `src/config.rs` | TOML config load/save, defaults, validation, legacy migration, todos and timer settings |
| `src/logging.rs` | Append-only runtime log |
| `build.rs` | Embeds the application manifest and compiles the icon resource |
| `taskbar-stats.rc`, `assets/icon.ico` | Windows executable icon resource |
| `scripts/build.bat` | Visual Studio/SDK bootstrap and Cargo invocation |
| `scripts/release_notes.py` | Generates grouped release notes from conventional commits |
| `scripts/bump_version.py` | Rewrites the package version in `Cargo.toml` and `Cargo.lock` |
| `scripts/build-installer.ps1` | Inno Setup bootstrap and installer build |
| `installer/taskbar-stats.iss` | Per-user installer and uninstaller definition |
| `.github/workflows/ci.yml` | Format, clippy, test, and release-build checks |
| `.github/workflows/prepare-release.yml` | Bumps the version and pushes the tag after a merge to `main` |
| `.github/workflows/release.yml` | Tag-driven Windows zip/exe/setup release with checksums |

## Invariants

1. Keep the strip inside the taskbar. It is a layered child of
   `Shell_TrayWnd`; do not replace it with a floating top-level window.
2. Keep the detail panel above the taskbar on a bottom taskbar and inside the
   monitor work area. It must never push or overlay content below the taskbar.
3. Do not introduce a service, kernel driver, DLL injection, or elevation
   requirement. All current sensors are user-mode.
4. Never fabricate a sensor value. Missing CPU, GPU, or disk temperature must
   remain `None` and render as `--` or `--°`. Memory has no temperature reading
   and must not display one.
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
10. The Settings and Appearance tabs are the supported configuration path.
    Every control change must validate, save, and apply immediately; do not
    reintroduce a manual-edit-only workflow.
11. Keep the tray icon and Start with Windows toggle per-user. Do not require
    elevation and do not leave a stale startup entry on uninstall.
12. Pomodoro notifications use the tray icon; do not replace them with a
    blocking dialog or a top-level notification window.
13. Never cache DPI across a display change. On `WM_DISPLAYCHANGE`,
    `WM_SETTINGCHANGE`/`SPI_SETWORKAREA`, `WM_DPICHANGED`, and
    `WM_DPICHANGED_AFTERPARENT`, re-read `Shell_TrayWnd` through
    `taskbar::find()` before laying anything out. The strip is a child window
    and only receives `WM_DPICHANGED_AFTERPARENT`; the popup is top-level and
    receives both. `App::popup_dpi` queries `GetDpiForWindow` instead of
    remembering a value.

## UI rules

- Follow the tokens and layout values in `DESIGN.md` instead of adding one-off
  colors or font sizes.
- Normal values are neutral. Reserve warning colors for high utilization, hot
  temperatures, and full disks; reserve accent color for progress fills and
  selected controls.
- Use `Segoe UI`. Labels and secondary text are normal weight; primary values
  are semibold.
- Text carries a +2 DIP global boost in `Renderer`; the documented sizes in
  `DESIGN.md` are the resulting Normal sizes at 96 DPI (11 px labels, 19 px
  strip values, 26 px panel values).
- Keep the strip at a 4 px inset, 12 px outer radius, 72 px cells, and 10 px
  inter-cell gaps unless the documented composition changes.
- Keep motion brief and interruptible: the panel fades in over 140 ms and out
  over 110 ms; respect `SPI_GETCLIENTAREAANIMATION`. Do not animate the
  one-second data refresh.
- Verify System, Dark, Light, AMOLED, Slate, and Sand themes in Solid, Blur,
  and Clear styles, plus `mem_mode`/`disk_mode` in both `gb` and `percent`
  modes.
- Settings and Appearance controls use the 24 px chip base geometry, 6 px
  radius, soft card border, card fill, 2 px accent selected underline, and
  primary semibold selected text.
- Keep the Overview panel at 400 × 322 DIP, Pomodoro at 400 × 352 DIP, Todo at
  400 × 290 DIP, Settings at 400 × 352 DIP, and Appearance at 400 × 208 DIP at
  Normal text size; `Layout` derives every height, row step, chip height, and
  card height from `font_scale`, so drawing and hit testing must share it.
  Every page uses the shared `PAD` and `LABEL_H` tokens so the top and bottom
  padding stay at 16 px. Tabs belong on the bottom bar so switching tabs does
  not move them out from under the pointer.
- Keep the five bottom tabs proportional (weights 8/8/4/8/10, 44 px minimum,
  5 px gaps), fit each label inside its tab, and reserve a 48 px slot at the
  right for the version label.
- The font size chips scale text through `Renderer::set_font_scale` and layout
  through `ui::Layout`; do not hard-code per-widget font sizes or geometry for
  the Compact/Normal/Large setting. The strip caps the setting at Normal so it
  fits the fixed taskbar row.
- Every label must pass through `text_fit` or `truncate`; text may shrink or
  ellipsize, but never clip a chip, card, row, or tab.
- Reuse the current blurred backdrop when resizing an already-open panel. Do
  not hide the popup on tab changes; that produces the flash the bottom-tab
  layout is designed to avoid.
- The popup adds 6 DIP transparent side margins; subtract them before hit
  testing.
- Convert popup mouse coordinates from pixels to DIPs before hit testing.

## Verification

Before calling a change complete:

1. Build with `scripts\build.bat --release`; fix all errors and warnings.
2. Confirm the log contains a successful `strip placed:` line.
3. Capture the strip and the hover panel at 100% scaling.
4. Check System, Dark, Light, AMOLED, Slate, and Sand themes in Solid, Blur,
   and Clear styles.
5. Check multi-GPU layout and per-GPU toggles when more than one adapter is
   present.
6. Check `gb` and `percent` display modes, and confirm disk totals exclude
   cloud-drive letters.
7. Confirm hover show, fade, leave-hide, click toggle, middle-click Settings,
   tab switching across all five resized panels without a visible flash,
   strip drag/snap, and right-click menu behavior.
8. Confirm Pomodoro start/pause/reset/skip, preset changes, and tray notification
   behavior. Confirm Todo add/complete/delete/clear and paging.
9. Confirm the tray icon appears, left-click opens the panel, and right-click
   opens the menu. Confirm Start with Windows adds and removes the Run entry.
10. Confirm left and right placement avoid existing taskbar widgets with a
    6 px gap.
11. Open Settings and Appearance from the hover tabs, and Settings from the
    right-click menu.
12. Exercise every Settings and Appearance control: position, offset, theme,
    style, accent, text size, memory mode, disk mode, refresh, hover delay,
    widget toggles, GPU toggles, startup, reset, and Reset.
13. Confirm settings survive closing/reopening the panel and that the strip and
    panel update immediately.
14. Build and install the Inno Setup package; confirm the uninstaller removes the
    app, shortcuts, and startup registration while preserving user config.
15. Remove temporary screenshots, scripts, and logs from the repository root.

## Git hygiene

- `target/`, `.toolchain/`, and `*.log` are ignored.
- Do not commit build products, local toolchain extracts, runtime logs, or
  temporary screenshots.
- Keep commits focused; do not mix unrelated refactors with UI or sensor work.

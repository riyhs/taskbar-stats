# Releasing

Releases are built and published by `.github/workflows/release.yml` on
`windows-latest`.

## Versioning

Use `MAJOR.MINOR.PATCH` for the Rust package version and a matching `v` tag.

1. Update `version` in `Cargo.toml`.
2. Run the local checks:

   ```bat
   scripts\build.bat --release
   cargo fmt -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test --all-targets
   ```

3. Commit the version bump.
4. Create and push an annotated tag:

   ```bat
   git tag -a v0.1.0 -m "Taskbar Stats v0.1.0"
   git push origin main --tags
   ```

The release workflow rejects a tag that does not match `Cargo.toml`. It exports
`TASKBAR_STATS_VERSION=v<version>` before compiling, so the version in the panel
footer comes from the release tag. Local builds fall back to the Cargo package
version.

## Artifacts

The workflow builds `target/release/taskbar-stats.exe`, compiles the Inno Setup
installer, and publishes:

```text
TaskbarStats-<version>-windows-x64.zip
TaskbarStats-<version>-windows-x64.exe
TaskbarStats-<version>-windows-x64-setup.exe
TaskbarStats-<version>-windows-x64.sha256
```

The zip contains the executable. The setup executable installs the app and
generates an uninstaller automatically. The checksum file covers both the zip and
the setup executable using the standard two-space format:

```text
<sha256>  TaskbarStats-<version>-windows-x64.zip
<sha256>  TaskbarStats-<version>-windows-x64-setup.exe
```

## Installer

The installer source is `installer/taskbar-stats.iss`. It targets the current
user, installs under `%LOCALAPPDATA%\Programs\TaskbarStats`, and does not require
elevation.

To build it locally, install Inno Setup 6, build the release executable, then run:

```powershell
.\scripts\build-installer.ps1 -Version 0.1.0
```

The result is written to `dist/TaskbarStats-0.1.0-windows-x64-setup.exe`.

## Signing

Windows builds are currently **unsigned**. There are no signing secrets to
configure, and SmartScreen may warn on first run.

If code signing is added later, sign both the executable and the setup executable
before packaging them and update the verification step in `release.yml`. Do not
publish a signed label until the signatures have been verified.

## Local release build

```bat
scripts\build.bat --release
```

The local executable is written to `target\release\taskbar-stats.exe`. The icon
resource is compiled from `taskbar-stats.rc` and `assets/icon.ico` by `build.rs`;
the Windows SDK `rc.exe` must be available.

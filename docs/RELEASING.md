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

The release workflow rejects a tag that does not match `Cargo.toml`.

## Artifacts

The workflow builds `target/release/taskbar-stats.exe` and publishes:

```text
TaskbarStats-<version>-windows-x64.zip
TaskbarStats-<version>-windows-x64.exe
TaskbarStats-<version>-windows-x64.zip.sha256
```

The zip contains the executable. The checksum file uses the standard two-space
format:

```text
<sha256>  TaskbarStats-<version>-windows-x64.zip
```

## Signing

Windows builds are currently **unsigned**. There are no signing secrets to
configure, and SmartScreen may warn on first run.

If code signing is added later, sign the executable before packaging it and
update the verification step in `release.yml`. Do not publish a signed label
until the signature has been verified.

## Local release build

```bat
scripts\build.bat --release
```

The local executable is written to `target\release\taskbar-stats.exe`. The icon
resource is compiled from `taskbar-stats.rc` and `assets/icon.ico` by `build.rs`;
the Windows SDK `rc.exe` must be available.

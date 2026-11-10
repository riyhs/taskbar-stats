use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_rc() -> Option<PathBuf> {
    if let Some(path) = env::var_os("RC") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(path) = find_in_path("rc.exe") {
        return Some(path);
    }

    if let Some(kits) = env::var_os("ProgramFiles(x86)") {
        let bin = PathBuf::from(kits).join("Windows Kits/10/bin");
        if let Ok(entries) = std::fs::read_dir(bin) {
            let mut versions: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect();
            versions.sort();
            for version in versions.iter().rev() {
                let candidate = version.join("x64/rc.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join(".toolchain/buildtools/extracted/bin");
    if let Ok(entries) = std::fs::read_dir(local) {
        let mut versions: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        versions.sort();
        for version in versions.iter().rev() {
            let candidate = version.join("x64/rc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=taskbar-stats.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-env-changed=TASKBAR_STATS_VERSION");

    let version = env::var("TASKBAR_STATS_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "v{}",
                env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into())
            )
        });
    println!("cargo:rustc-env=TASKBAR_STATS_VERSION={version}");

    let manifest = std::fs::canonicalize("app.manifest").expect("app.manifest");
    let mut manifest = manifest.to_string_lossy().into_owned();
    if let Some(stripped) = manifest.strip_prefix(r"\?\") {
        manifest = stripped.to_string();
    }
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{manifest}");

    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe not found; building without an application icon");
        return;
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("taskbar-stats.res");
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&out)
        .arg("taskbar-stats.rc")
        .status()
        .expect("failed to run rc.exe");
    assert!(status.success(), "rc.exe failed");
    println!("cargo:rustc-link-arg={}", out.display());
}

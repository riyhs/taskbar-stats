use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG: OnceLock<Mutex<Option<File>>> = OnceLock::new();

pub fn init() {
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TaskbarStats");
    let _ = std::fs::create_dir_all(&dir);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("taskbar-stats.log"))
        .ok();
    let _ = LOG.set(Mutex::new(file));
    write("startup");
}

pub fn write(message: &str) {
    let Some(lock) = LOG.get() else { return };
    let Ok(mut guard) = lock.lock() else { return };
    let Some(file) = guard.as_mut() else { return };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = writeln!(file, "[{secs}] {message}");
    let _ = file.flush();
}

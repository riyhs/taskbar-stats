use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub position: String,
    pub offset: i32,
    pub widgets: Vec<String>,
    pub theme: String,
    pub accent: String,
    pub hover_delay_ms: u32,
    pub update_ms: u32,
    #[serde(default = "default_gb")]
    pub mem_mode: String,
    #[serde(default = "default_gb")]
    pub disk_mode: String,
}

fn default_gb() -> String {
    "gb".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            position: "right".into(),
            offset: 6,
            widgets: vec!["cpu".into(), "gpu".into(), "mem".into(), "disk".into()],
            theme: "system".into(),
            accent: "#5B9CFF".into(),
            hover_delay_ms: 220,
            update_ms: 1000,
            mem_mode: default_gb(),
            disk_mode: default_gb(),
        }
    }
}

impl Config {
    pub fn load() -> (Self, PathBuf) {
        let dir = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("TaskbarStats");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");

        if !path.exists() {
            let body = toml::to_string_pretty(&Self::default()).unwrap_or_default();
            let _ = std::fs::write(&path, body);
        }

        let mut cfg = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str::<Self>(&s).ok())
            .unwrap_or_default();
        if cfg.widgets == ["cpu", "temp", "mem", "disk"] {
            cfg.widgets = vec!["cpu".into(), "gpu".into(), "mem".into(), "disk".into()];
        }
        cfg.validate();
        (cfg, path)
    }

    pub fn validate(&mut self) {
        if self.position != "left" && self.position != "right" {
            self.position = "right".into();
        }
        if self.theme != "system" && self.theme != "dark" && self.theme != "light" {
            self.theme = "system".into();
        }
        self.offset = self.offset.clamp(-400, 400);
        self.hover_delay_ms = self.hover_delay_ms.clamp(0, 2000);
        self.update_ms = self.update_ms.clamp(250, 5000);
        if self.mem_mode != "percent" {
            self.mem_mode = "gb".into();
        }
        if self.disk_mode != "percent" {
            self.disk_mode = "gb".into();
        }

        let known = ["cpu", "temp", "gputemp", "gpu", "mem", "disk"];
        let mut seen = Vec::new();
        for w in self.widgets.drain(..) {
            if known.contains(&w.as_str()) && !seen.contains(&w) {
                seen.push(w);
            }
        }
        if seen.is_empty() {
            seen = vec!["cpu".into(), "gpu".into(), "mem".into(), "disk".into()];
        }
        self.widgets = seen;

        let hex = self.accent.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            self.accent = "#5B9CFF".into();
        } else {
            self.accent = format!("#{}", hex.to_ascii_uppercase());
        }
    }
}

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub position: String,
    pub offset: i32,
    pub widgets: Vec<String>,
    pub theme: String,
    #[serde(default = "default_style")]
    pub style: String,
    pub accent: String,
    pub hover_delay_ms: u32,
    pub update_ms: u32,
    #[serde(default = "default_gb")]
    pub mem_mode: String,
    #[serde(default = "default_gb")]
    pub disk_mode: String,
    #[serde(default)]
    pub gpu_visible: Vec<String>,
    #[serde(default = "default_font_scale")]
    pub font_scale: f64,
    #[serde(default)]
    pub launch_at_startup: bool,
    #[serde(default = "default_focus_min")]
    pub pomodoro_focus_min: u32,
    #[serde(default = "default_break_min")]
    pub pomodoro_break_min: u32,
    #[serde(default)]
    pub todos: Vec<TodoItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TodoItem {
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

fn default_gb() -> String {
    "gb".into()
}

fn default_style() -> String {
    "solid".into()
}

fn default_font_scale() -> f64 {
    1.0
}

fn default_focus_min() -> u32 {
    25
}

fn default_break_min() -> u32 {
    5
}

impl Default for Config {
    fn default() -> Self {
        Self {
            position: "right".into(),
            offset: 6,
            widgets: vec![
                "cpu".into(),
                "gpu".into(),
                "mem".into(),
                "disk".into(),
                "net".into(),
            ],
            theme: "system".into(),
            style: default_style(),
            accent: "#5B9CFF".into(),
            hover_delay_ms: 220,
            update_ms: 1000,
            mem_mode: default_gb(),
            disk_mode: default_gb(),
            gpu_visible: Vec::new(),
            font_scale: default_font_scale(),
            launch_at_startup: false,
            pomodoro_focus_min: default_focus_min(),
            pomodoro_break_min: default_break_min(),
            todos: Vec::new(),
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
        let mut migrated = false;
        if cfg.theme == "glass" {
            cfg.theme = "system".into();
            cfg.style = "blur".into();
            migrated = true;
        }
        if cfg.widgets == ["cpu", "temp", "mem", "disk"] {
            cfg.widgets = vec![
                "cpu".into(),
                "gpu".into(),
                "mem".into(),
                "disk".into(),
                "net".into(),
            ];
        }
        cfg.validate();
        if migrated {
            let _ = cfg.save(&path);
        }
        (cfg, path)
    }

    pub fn gpu_enabled(&self, name: &str) -> bool {
        self.widgets.iter().any(|widget| widget == "gpu")
            && (self.gpu_visible.is_empty()
                || self
                    .gpu_visible
                    .iter()
                    .any(|visible| visible.eq_ignore_ascii_case(name)))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let body = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, body)
    }

    pub fn validate(&mut self) {
        if self.position != "left" && self.position != "right" {
            self.position = "right".into();
        }
        if !matches!(
            self.theme.as_str(),
            "system" | "dark" | "light" | "amoled" | "slate" | "sand"
        ) {
            self.theme = "system".into();
        }
        if !matches!(self.style.as_str(), "solid" | "blur" | "clear") {
            self.style = "solid".into();
        }
        self.offset = self.offset.clamp(-4096, 4096);
        self.hover_delay_ms = self.hover_delay_ms.clamp(0, 2000);
        self.update_ms = self.update_ms.clamp(250, 5000);
        if self.mem_mode != "percent" {
            self.mem_mode = "gb".into();
        }
        if self.disk_mode != "percent" {
            self.disk_mode = "gb".into();
        }
        if !self.font_scale.is_finite() {
            self.font_scale = 1.0;
        }
        self.font_scale = self.font_scale.clamp(0.85, 1.15);
        self.pomodoro_focus_min = self.pomodoro_focus_min.clamp(5, 120);
        self.pomodoro_break_min = self.pomodoro_break_min.clamp(1, 60);
        self.todos.retain(|todo| !todo.text.trim().is_empty());
        self.todos.truncate(20);
        for todo in &mut self.todos {
            todo.text = todo.text.trim().chars().take(160).collect();
        }
        self.gpu_visible.retain(|name| !name.trim().is_empty());
        let mut seen = Vec::new();
        self.gpu_visible.retain(|name| {
            let key = name.to_ascii_lowercase();
            if seen.contains(&key) {
                false
            } else {
                seen.push(key);
                true
            }
        });

        let known = ["cpu", "temp", "gputemp", "gpu", "mem", "disk", "net"];
        let mut seen = Vec::new();
        for w in self.widgets.drain(..) {
            if known.contains(&w.as_str()) && !seen.contains(&w) {
                seen.push(w);
            }
        }
        if seen.is_empty() {
            seen = vec![
                "cpu".into(),
                "gpu".into(),
                "mem".into(),
                "disk".into(),
                "net".into(),
            ];
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

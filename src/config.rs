use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::theme::{self, known};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    #[serde(default = "default_num_ctx")]
    pub num_ctx: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default = "default_keep_alive")]
    pub keep_alive: String,
    #[serde(default = "default_true")]
    pub show_token_stats: bool,
    #[serde(default)]
    pub insecure_hf_pulls: bool,
    #[serde(default)]
    pub autostart_ollama: bool,
    #[serde(default)]
    pub stop_ollama_on_quit: bool,
}

fn default_theme() -> String {
    "catppuccin-mocha".into()
}
fn default_host() -> String {
    "http://127.0.0.1:11434".into()
}
fn default_temperature() -> f32 {
    0.8
}
fn default_top_p() -> f32 {
    0.9
}
fn default_num_ctx() -> u32 {
    8192
}
fn default_keep_alive() -> String {
    "5m".into()
}
fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            host: default_host(),
            default_model: String::new(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            num_ctx: default_num_ctx(),
            seed: None,
            keep_alive: default_keep_alive(),
            show_token_stats: true,
            insecure_hf_pulls: false,
            autostart_ollama: false,
            stop_ollama_on_quit: false,
        }
    }
}

impl Config {
    pub fn load(path: &Path, omarchy_installed: bool) -> Self {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_default(),
            Err(_) => {
                let mut fresh = Config::default();
                fresh.theme = theme::fallback_id(omarchy_installed).into();
                fresh
            }
        };
        cfg.normalize();
        cfg
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }

    fn normalize(&mut self) {
        self.host = normalize_host(&self.host);
        if self.theme.trim().is_empty() || !known(&self.theme) {
            self.theme = "catppuccin-mocha".into();
        }
        self.temperature = self.temperature.clamp(0.0, 2.0);
        self.top_p = self.top_p.clamp(0.0, 1.0);
        if self.num_ctx < 128 {
            self.num_ctx = 128;
        }
        if self.keep_alive.trim().is_empty() {
            self.keep_alive = default_keep_alive();
        }
    }
}

pub fn normalize_host(host: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    if host.is_empty() {
        return default_host();
    }
    if host.starts_with("http://") || host.starts_with("https://") {
        host.to_string()
    } else {
        format!("http://{host}")
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ollatui")
        .join("config.toml")
}

pub fn config_dir() -> PathBuf {
    config_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn open_config_dir() -> anyhow::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let status = std::process::Command::new("xdg-open").arg(&dir).spawn();
    match status {
        Ok(_) => Ok(()),
        Err(err) => anyhow::bail!("could not open {dir}: {err}", dir = dir.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_launch_follows_omarchy_only_when_installed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let with = Config::load(&path, true);
        assert_eq!(with.theme, "omarchy");
        let without = Config::load(&path, false);
        assert_eq!(without.theme, "catppuccin-mocha");
        assert!(!path.exists());
    }

    #[test]
    fn roundtrip_keeps_an_explicit_theme() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::load(&path, false);
        cfg.theme = "dracula".into();
        cfg.temperature = 0.2;
        cfg.seed = Some(7);
        cfg.host = "127.0.0.1:11434".into();
        cfg.save(&path).unwrap();
        let loaded = Config::load(&path, true);
        assert_eq!(loaded.theme, "dracula");
        assert!((loaded.temperature - 0.2).abs() < f32::EPSILON);
        assert_eq!(loaded.seed, Some(7));
        assert_eq!(loaded.host, "http://127.0.0.1:11434");
    }
}

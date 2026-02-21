// config.rs — Safe TOML config loading with XDG-compliant directory resolution.
// Interactive setup wizard for first-run. No manual file editing required.

use anyhow::{Context, Result};
use dialoguer::{Input, Select, theme::ColorfulTheme};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

/// Application configuration persisted to disk.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_model() -> String {
    "models/gemini-2.0-flash".into()
}

fn default_hotkey() -> String {
    "ctrl+shift+space".into()
}

fn default_timeout_secs() -> u64 {
    10
}

impl Config {
    /// Build the REST API URL for Gemini generateContent endpoint.
    pub fn api_url(&self) -> String {
        // Model format in config: "models/gemini-2.0-flash"
        // API URL format: https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key=KEY
        let model_name = self.model.strip_prefix("models/").unwrap_or(&self.model);
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model_name, self.api_key
        )
    }
}

// ── Paths ──────────────────────────────────────────────────

/// Resolve the config directory path.
/// Linux:   ~/.config/g-type/
/// macOS:   ~/Library/Application Support/g-type/
/// Windows: %APPDATA%\g-type\
fn config_dir() -> Result<PathBuf> {
    let proj = ProjectDirs::from("", "", "g-type")
        .context("Cannot determine home directory for config")?;
    Ok(proj.config_dir().to_path_buf())
}

/// Full path to config.toml.
pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

// ── Load ───────────────────────────────────────────────────

/// Load configuration from disk.
/// If the file doesn't exist or the key is missing, run the interactive wizard.
pub fn load() -> Result<Config> {
    let path = config_path()?;

    if !path.exists() {
        eprintln!();
        eprintln!("  No config found. Starting first-time setup...");
        eprintln!();
        return interactive_setup(&path);
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config at {}", path.display()))?;

    let cfg: Config = toml::from_str(&raw)
        .with_context(|| format!("Failed to parse config at {}", path.display()))?;

    if cfg.api_key.is_empty() || cfg.api_key == "YOUR_GEMINI_API_KEY_HERE" {
        eprintln!();
        eprintln!("  API key not set. Re-running setup...");
        eprintln!();
        return interactive_setup(&path);
    }

    debug!(path = %path.display(), "Config loaded");
    Ok(cfg)
}

// ── Interactive Setup Wizard ───────────────────────────────

/// Run the interactive first-time setup. Prompts for API key in the terminal.
/// Called automatically on first run, or explicitly via `g-type setup`.
pub fn interactive_setup(path: &PathBuf) -> Result<Config> {
    println!("\x1b[36m╔══════════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[36m║         G-Type — First Time Setup            ║\x1b[0m");
    println!("\x1b[36m╚══════════════════════════════════════════════╝\x1b[0m");
    println!();
    println!("  G-Type needs a Google Gemini API key to work.");
    println!("  Get one free at: \x1b[4mhttps://aistudio.google.com/apikey\x1b[0m");
    println!();

    let theme = ColorfulTheme::default();

    // API Key
    let api_key: String = Input::with_theme(&theme)
        .with_prompt("Gemini API Key")
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                Err("API Key cannot be empty")
            } else if !input.starts_with("AIzaSy") {
                Err("Invalid API Key format. Gemini keys usually start with 'AIzaSy'")
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    // Model Selection
    let models = vec![
        "models/gemini-2.0-flash",
        "models/gemini-2.0-flash-lite",
        "models/gemini-2.0-pro-exp",
        "models/gemini-1.5-pro",
        "models/gemini-1.5-flash",
    ];
    
    let model_idx = Select::with_theme(&theme)
        .with_prompt("Select Gemini Model")
        .default(0)
        .items(&models)
        .interact()?;
    let model = models[model_idx].to_string();

    // Hotkey
    let hotkey: String = Input::with_theme(&theme)
        .with_prompt("Hotkey")
        .default(default_hotkey())
        .interact_text()?;

    let cfg = Config {
        api_key,
        model,
        hotkey,
        timeout_secs: default_timeout_secs(),
    };

    save(&cfg, path)?;

    println!();
    println!("  \x1b[32m✔ Config saved to {}\x1b[0m", path.display());
    println!("  \x1b[32m✔ You can re-run setup anytime with: g-type setup\x1b[0m");
    println!();

    Ok(cfg)
}

// ── Save / Update ──────────────────────────────────────────

/// Persist config to disk.
fn save(cfg: &Config, path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create config directory {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(cfg)
        .context("Failed to serialize config")?;

    fs::write(path, &content)
        .with_context(|| format!("Failed to write config to {}", path.display()))?;

    info!(path = %path.display(), "Config saved");
    Ok(())
}

/// Update just the API key in an existing config.
pub fn set_api_key(key: &str) -> Result<()> {
    let path = config_path()?;
    let mut cfg = if path.exists() {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config at {}", path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("Failed to parse config at {}", path.display()))?
    } else {
        Config {
            api_key: String::new(),
            model: default_model(),
            hotkey: default_hotkey(),
            timeout_secs: default_timeout_secs(),
        }
    };

    cfg.api_key = key.to_string();
    save(&cfg, &path)?;
    println!("  \x1b[32m✔ API key updated.\x1b[0m");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_url_construction() {
        let cfg = Config {
            api_key: "test-key-123".into(),
            model: default_model(),
            hotkey: default_hotkey(),
            timeout_secs: 3,
        };
        let url = cfg.api_url();
        assert!(url.contains("test-key-123"));
        assert!(url.contains("gemini-2.0-flash"));
        assert!(url.contains("generateContent"));
        assert!(url.starts_with("https://"));
    }

    #[test]
    fn test_defaults() {
        let raw = r#"api_key = "abc""#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.model, "models/gemini-2.0-flash");
        assert_eq!(cfg.hotkey, "ctrl+shift+space");
    }
}

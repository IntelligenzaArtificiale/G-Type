// config.rs — Safe TOML config loading with XDG-compliant directory resolution.
// Interactive setup wizard for first-run. No manual file editing required.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use tracing::info;

/// Application configuration persisted to disk.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_injection_threshold")]
    pub injection_threshold: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_model() -> String {
    "models/gemini-2.0-flash".into()
}

fn default_hotkey() -> String {
    "ctrl+t".into()
}

fn default_injection_threshold() -> usize {
    80
}

fn default_timeout_secs() -> u64 {
    3
}

impl Config {
    /// Build the WebSocket URL from the stored API key and model.
    pub fn ws_url(&self) -> String {
        format!(
            "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent?key={}",
            self.api_key
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

    info!(path = %path.display(), "Config loaded");
    Ok(cfg)
}

// ── Interactive Setup Wizard ───────────────────────────────

/// Run the interactive first-time setup. Prompts for API key in the terminal.
/// Called automatically on first run, or explicitly via `g-type setup`.
pub fn interactive_setup(path: &PathBuf) -> Result<Config> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();

    println!("\x1b[36m╔══════════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[36m║         G-Type — First Time Setup            ║\x1b[0m");
    println!("\x1b[36m╚══════════════════════════════════════════════╝\x1b[0m");
    println!();
    println!("  G-Type needs a Google Gemini API key to work.");
    println!("  Get one free at: \x1b[4mhttps://aistudio.google.com/apikey\x1b[0m");
    println!();

    // API Key
    let api_key = prompt_required(&mut reader, "  Gemini API Key: ")?;

    // Model (with default)
    let model = prompt_with_default(
        &mut reader,
        "  Model",
        &default_model(),
    )?;

    // Hotkey (with default)
    let hotkey = prompt_with_default(
        &mut reader,
        "  Hotkey",
        &default_hotkey(),
    )?;

    let cfg = Config {
        api_key,
        model,
        hotkey,
        injection_threshold: default_injection_threshold(),
        timeout_secs: default_timeout_secs(),
    };

    save(&cfg, path)?;

    println!();
    println!("  \x1b[32m✔ Config saved to {}\x1b[0m", path.display());
    println!("  \x1b[32m✔ You can re-run setup anytime with: g-type setup\x1b[0m");
    println!();

    Ok(cfg)
}

/// Prompt for a required value (cannot be empty).
fn prompt_required(reader: &mut impl BufRead, prompt: &str) -> Result<String> {
    loop {
        print!("{}", prompt);
        io::stdout().flush().context("Failed to flush stdout")?;

        let mut input = String::new();
        reader.read_line(&mut input).context("Failed to read input")?;
        let trimmed = input.trim().to_string();

        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
        println!("  \x1b[33m⚠ This field is required.\x1b[0m");
    }
}

/// Prompt with a default value shown in brackets. Enter accepts the default.
fn prompt_with_default(
    reader: &mut impl BufRead,
    label: &str,
    default: &str,
) -> Result<String> {
    print!("{} [{}]: ", label, default);
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut input = String::new();
    reader.read_line(&mut input).context("Failed to read input")?;
    let trimmed = input.trim();

    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
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
            injection_threshold: default_injection_threshold(),
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
    fn test_ws_url_construction() {
        let cfg = Config {
            api_key: "test-key-123".into(),
            model: default_model(),
            hotkey: default_hotkey(),
            injection_threshold: 80,
            timeout_secs: 3,
        };
        let url = cfg.ws_url();
        assert!(url.contains("test-key-123"));
        assert!(url.starts_with("wss://"));
    }

    #[test]
    fn test_defaults() {
        let raw = r#"api_key = "abc""#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.model, "models/gemini-2.0-flash");
        assert_eq!(cfg.injection_threshold, 80);
    }

    #[test]
    fn test_prompt_with_default_empty() {
        let input = b"\n";
        let mut reader = &input[..];
        let result = prompt_with_default(&mut reader, "Test", "default_val");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "default_val");
    }

    #[test]
    fn test_prompt_with_default_custom() {
        let input = b"custom_val\n";
        let mut reader = &input[..];
        let result = prompt_with_default(&mut reader, "Test", "default_val");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "custom_val");
    }

    #[test]
    fn test_prompt_required() {
        let input = b"my-api-key\n";
        let mut reader = &input[..];
        let result = prompt_required(&mut reader, "Key: ");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "my-api-key");
    }
}

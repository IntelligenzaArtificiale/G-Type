// config.rs — Safe TOML config loading with XDG-compliant directory resolution.
// Interactive setup wizard for first-run. No manual file editing required.

use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use directories::ProjectDirs;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
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
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_sound_enabled")]
    pub sound_enabled: bool,
    #[serde(default = "default_currency")]
    pub currency: String,
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

fn default_language() -> String {
    "auto".into()
}

fn default_sound_enabled() -> bool {
    true
}

fn default_currency() -> String {
    "USD".into()
}

/// Available languages for transcription.
const LANGUAGES: &[(&str, &str)] = &[
    ("auto", "Auto-detect"),
    ("it", "Italiano"),
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("pt", "Português"),
    ("ja", "日本語"),
    ("zh", "中文"),
    ("ko", "한국어"),
    ("ar", "العربية"),
    ("ru", "Русский"),
    ("hi", "हिन्दी"),
];

/// Get the transcription prompt for the configured language.
pub fn transcription_prompt(language: &str) -> String {
    let lang_instruction = match language {
        "auto" | "" => String::new(),
        code => {
            let name = LANGUAGES
                .iter()
                .find(|(c, _)| *c == code)
                .map(|(_, n)| *n)
                .unwrap_or(code);
            format!(" The audio is in {name} ({code}). Transcribe in that language.")
        }
    };
    format!(
        "Trascrivi esattamente ciò che viene detto in questo audio, parola per parola. \
         Non aggiungere commenti, non rispondere a domande, non inventare punteggiatura. \
         Restituisci SOLO il testo dettato. Se l'audio è silenzioso o incomprensibile, \
         rispondi con una stringa vuota.{lang_instruction}"
    )
}

impl Config {
    /// Build the REST API URL for Gemini generateContent endpoint.
    /// The API key is NOT included in the URL — it is sent via the
    /// `x-goog-api-key` HTTP header (see `network::transcribe`).
    pub fn api_url(&self) -> String {
        let model_name = self.model.strip_prefix("models/").unwrap_or(&self.model);
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            model_name
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
    println!();
    println!(
        "{}",
        "╔══════════════════════════════════════════════╗"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        "║         G-Type — First Time Setup            ║"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════╝"
            .cyan()
            .bold()
    );
    println!();
    println!("  G-Type needs a Google Gemini API key to work.");
    println!(
        "  Get one free at: {}",
        "https://aistudio.google.com/apikey".underline()
    );
    println!();

    let theme = ColorfulTheme::default();

    // ── Step 1: API Key ────────────────────────────────────
    let api_key: String = Input::with_theme(&theme)
        .with_prompt("🔑 Gemini API Key")
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                Err("API Key cannot be empty")
            } else if !input.starts_with("AIzaSy") {
                Err("Invalid format — Gemini keys start with 'AIzaSy'")
            } else if input.len() < 30 {
                Err("API Key seems too short")
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    // ── Step 2: Verify API key with a real call ────────────
    verify_api_key_spinner(&api_key)?;

    // ── Step 3: Model Selection ────────────────────────────
    let models = vec![
        "models/gemini-2.0-flash",
        "models/gemini-2.5-flash",
        "models/gemini-2.5-flash-lite",
        "models/gemini-2.5-pro",
        "models/gemini-3-flash-preview",
        "models/gemini-3-pro-preview",
        "models/gemini-3.1-pro-preview",
    ];

    let model_idx = Select::with_theme(&theme)
        .with_prompt("🤖 Select Gemini Model")
        .default(0)
        .items(&models)
        .interact()?;
    let model = models[model_idx].to_string();

    // ── Step 4: Language ───────────────────────────────────
    let lang_labels: Vec<String> = LANGUAGES
        .iter()
        .map(|(code, name)| format!("{name}  ({code})"))
        .collect();

    let lang_idx = Select::with_theme(&theme)
        .with_prompt("🌍 Transcription Language")
        .default(0)
        .items(&lang_labels)
        .interact()?;
    let language = LANGUAGES[lang_idx].0.to_string();

    // ── Step 5: Sound feedback ─────────────────────────────
    let sound_options = ["Yes — play beeps on start/stop", "No — silent mode"];
    let sound_idx = Select::with_theme(&theme)
        .with_prompt("🔊 Enable sound feedback?")
        .default(0)
        .items(sound_options)
        .interact()?;
    let sound_enabled = sound_idx == 0;

    // ── Step 6: Currency ───────────────────────────────────
    let currency_labels: Vec<String> = crate::tracking::CURRENCIES
        .iter()
        .map(|(code, sym, _)| format!("{code}  ({sym})"))
        .collect();

    let currency_idx = Select::with_theme(&theme)
        .with_prompt("💱 Display currency for cost tracking")
        .default(0)
        .items(&currency_labels)
        .interact()?;
    let currency = crate::tracking::CURRENCIES[currency_idx].0.to_string();

    // ── Step 7: Hotkey — interactive capture ───────────────
    println!();
    println!(
        "  {} Press your desired hotkey combo (e.g. hold Ctrl+Shift+Space)...",
        "⌨️".bold()
    );
    println!(
        "  {}",
        "(or press Enter to use the default: ctrl+shift+space)".dimmed()
    );

    let hotkey = capture_hotkey_interactive().unwrap_or_else(|| {
        println!("  Using default hotkey: {}", "ctrl+shift+space".green());
        default_hotkey()
    });

    let cfg = Config {
        api_key,
        model,
        hotkey,
        timeout_secs: default_timeout_secs(),
        language,
        sound_enabled,
        currency,
    };

    save(&cfg, path)?;

    println!();
    println!(
        "  {} Config saved to {}",
        "✔".green().bold(),
        path.display()
    );
    println!(
        "  {} Re-run anytime with: {}",
        "✔".green().bold(),
        "g-type setup".bold()
    );
    println!();

    Ok(cfg)
}

/// Verify the API key by making a lightweight test call to the Gemini models.list endpoint.
/// Shows a spinner while the request is in flight.
fn verify_api_key_spinner(api_key: &str) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("invalid spinner template"),
    );
    spinner.set_message("Verifying API key...");
    spinner.enable_steady_tick(Duration::from_millis(80));

    let result = verify_api_key_sync(api_key);

    match &result {
        Ok(()) => {
            spinner.finish_with_message(format!("{}", "✔ API key is valid!".green().bold()));
        }
        Err(e) => {
            spinner.finish_with_message(format!("{} {}", "✘".red().bold(), e));
        }
    }

    result
}

/// Synchronous API key verification — calls Gemini models.list endpoint.
fn verify_api_key_sync(api_key: &str) -> Result<()> {
    let url = "https://generativelanguage.googleapis.com/v1beta/models";

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("Failed to build HTTP client for verification")?;

    let response = client
        .get(url)
        .header("x-goog-api-key", api_key)
        .send()
        .context("Network error — check your internet connection")?;

    let status = response.status();
    if status.is_success() {
        Ok(())
    } else if status.as_u16() == 400 || status.as_u16() == 403 {
        anyhow::bail!("API key is invalid or has insufficient permissions (HTTP {}). Double-check it at https://aistudio.google.com/apikey", status)
    } else {
        anyhow::bail!(
            "Unexpected response from Gemini API (HTTP {}). Try again later.",
            status
        )
    }
}

/// Attempt to interactively capture a hotkey combo by listening to keyboard events.
/// Returns None if capture fails or times out (e.g. no display, CI environment).
fn capture_hotkey_interactive() -> Option<String> {
    use std::sync::{Arc, Mutex};

    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let modifiers: Arc<Mutex<std::collections::HashSet<String>>> =
        Arc::new(Mutex::new(std::collections::HashSet::new()));
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let captured_clone = captured.clone();
    let modifiers_clone = modifiers.clone();
    let done_clone = done.clone();

    // Spawn rdev listener on a separate thread
    let listener_handle = std::thread::spawn(move || {
        let _ = rdev::listen(move |event: rdev::Event| {
            if done_clone.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            match event.event_type {
                rdev::EventType::KeyPress(key) => {
                    let mut mods = modifiers_clone.lock().unwrap();

                    // Track modifiers
                    match key {
                        rdev::Key::ControlLeft | rdev::Key::ControlRight => {
                            mods.insert("ctrl".to_string());
                        }
                        rdev::Key::ShiftLeft | rdev::Key::ShiftRight => {
                            mods.insert("shift".to_string());
                        }
                        rdev::Key::Alt | rdev::Key::AltGr => {
                            mods.insert("alt".to_string());
                        }
                        rdev::Key::MetaLeft | rdev::Key::MetaRight => {
                            mods.insert("super".to_string());
                        }
                        rdev::Key::Return => {
                            // Enter = accept default, signal done
                            done_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        _ => {
                            // Non-modifier key pressed — this is the trigger
                            if !mods.is_empty() {
                                let trigger = rdev_key_to_str(key);
                                // Build combo string: sort modifiers for consistency
                                let mut parts: Vec<String> = mods.iter().cloned().collect();
                                parts.sort();
                                parts.push(trigger);
                                let combo = parts.join("+");

                                *captured_clone.lock().unwrap() = Some(combo);
                                done_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                }
                rdev::EventType::KeyRelease(key) => {
                    let mut mods = modifiers_clone.lock().unwrap();
                    match key {
                        rdev::Key::ControlLeft | rdev::Key::ControlRight => {
                            mods.remove("ctrl");
                        }
                        rdev::Key::ShiftLeft | rdev::Key::ShiftRight => {
                            mods.remove("shift");
                        }
                        rdev::Key::Alt | rdev::Key::AltGr => {
                            mods.remove("alt");
                        }
                        rdev::Key::MetaLeft | rdev::Key::MetaRight => {
                            mods.remove("super");
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        });
    });

    // Wait up to 10 seconds for the user to press a combo
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if done.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Signal done to stop the listener (it will exit when the thread drops)
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(listener_handle); // detach — rdev::listen can't be cleanly stopped

    let result = captured.lock().unwrap().clone();
    if let Some(ref combo) = result {
        println!("  Captured hotkey: {}", combo.green().bold());
    }
    result
}

/// Convert an rdev::Key to a lowercase string for hotkey config.
fn rdev_key_to_str(key: rdev::Key) -> String {
    match key {
        rdev::Key::Space => "space".to_string(),
        rdev::Key::Return => "enter".to_string(),
        rdev::Key::Tab => "tab".to_string(),
        rdev::Key::Escape => "escape".to_string(),
        rdev::Key::Backspace => "backspace".to_string(),
        rdev::Key::Delete => "delete".to_string(),
        rdev::Key::F1 => "f1".to_string(),
        rdev::Key::F2 => "f2".to_string(),
        rdev::Key::F3 => "f3".to_string(),
        rdev::Key::F4 => "f4".to_string(),
        rdev::Key::F5 => "f5".to_string(),
        rdev::Key::F6 => "f6".to_string(),
        rdev::Key::F7 => "f7".to_string(),
        rdev::Key::F8 => "f8".to_string(),
        rdev::Key::F9 => "f9".to_string(),
        rdev::Key::F10 => "f10".to_string(),
        rdev::Key::F11 => "f11".to_string(),
        rdev::Key::F12 => "f12".to_string(),
        rdev::Key::KeyA => "a".to_string(),
        rdev::Key::KeyB => "b".to_string(),
        rdev::Key::KeyC => "c".to_string(),
        rdev::Key::KeyD => "d".to_string(),
        rdev::Key::KeyE => "e".to_string(),
        rdev::Key::KeyF => "f".to_string(),
        rdev::Key::KeyG => "g".to_string(),
        rdev::Key::KeyH => "h".to_string(),
        rdev::Key::KeyI => "i".to_string(),
        rdev::Key::KeyJ => "j".to_string(),
        rdev::Key::KeyK => "k".to_string(),
        rdev::Key::KeyL => "l".to_string(),
        rdev::Key::KeyM => "m".to_string(),
        rdev::Key::KeyN => "n".to_string(),
        rdev::Key::KeyO => "o".to_string(),
        rdev::Key::KeyP => "p".to_string(),
        rdev::Key::KeyQ => "q".to_string(),
        rdev::Key::KeyR => "r".to_string(),
        rdev::Key::KeyS => "s".to_string(),
        rdev::Key::KeyT => "t".to_string(),
        rdev::Key::KeyU => "u".to_string(),
        rdev::Key::KeyV => "v".to_string(),
        rdev::Key::KeyW => "w".to_string(),
        rdev::Key::KeyX => "x".to_string(),
        rdev::Key::KeyY => "y".to_string(),
        rdev::Key::KeyZ => "z".to_string(),
        rdev::Key::Num0 => "0".to_string(),
        rdev::Key::Num1 => "1".to_string(),
        rdev::Key::Num2 => "2".to_string(),
        rdev::Key::Num3 => "3".to_string(),
        rdev::Key::Num4 => "4".to_string(),
        rdev::Key::Num5 => "5".to_string(),
        rdev::Key::Num6 => "6".to_string(),
        rdev::Key::Num7 => "7".to_string(),
        rdev::Key::Num8 => "8".to_string(),
        rdev::Key::Num9 => "9".to_string(),
        other => format!("{:?}", other).to_lowercase(),
    }
}

// ── Save / Update ──────────────────────────────────────────

/// Persist config to disk.
fn save(cfg: &Config, path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create config directory {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(cfg).context("Failed to serialize config")?;

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
            language: default_language(),
            sound_enabled: default_sound_enabled(),
            currency: default_currency(),
        }
    };

    cfg.api_key = key.to_string();
    save(&cfg, &path)?;
    println!("  {} API key updated.", "✔".green().bold());
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
            language: default_language(),
            sound_enabled: default_sound_enabled(),
            currency: default_currency(),
        };
        let url = cfg.api_url();
        // API key must NOT appear in the URL (sent via header).
        assert!(!url.contains("test-key-123"), "API key must not be in URL");
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
        assert_eq!(cfg.timeout_secs, 10);
        assert_eq!(cfg.language, "auto");
        assert!(cfg.sound_enabled);
        assert_eq!(cfg.currency, "USD");
    }

    #[test]
    fn test_full_config_roundtrip() {
        let raw = r#"
api_key = "AIzaSyTest"
model = "models/gemini-1.5-pro"
hotkey = "alt+f9"
timeout_secs = 30
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.api_key, "AIzaSyTest");
        assert_eq!(cfg.model, "models/gemini-1.5-pro");
        assert_eq!(cfg.hotkey, "alt+f9");
        assert_eq!(cfg.timeout_secs, 30);
    }

    #[test]
    fn test_api_url_no_key_leak() {
        let cfg = Config {
            api_key: "AIzaSySECRET".into(),
            model: "models/gemini-2.0-flash".into(),
            hotkey: default_hotkey(),
            timeout_secs: 10,
            language: default_language(),
            sound_enabled: default_sound_enabled(),
            currency: default_currency(),
        };
        let url = cfg.api_url();
        assert!(!url.contains("SECRET"), "API key must not appear in URL");
        assert!(!url.contains("key="), "No key= querystring allowed");
    }

    #[test]
    fn test_api_url_strips_model_prefix() {
        let cfg = Config {
            api_key: "k".into(),
            model: "models/gemini-2.5-flash".into(),
            hotkey: default_hotkey(),
            timeout_secs: 10,
            language: default_language(),
            sound_enabled: default_sound_enabled(),
            currency: default_currency(),
        };
        assert!(cfg.api_url().contains("gemini-2.5-flash:generateContent"));
    }

    #[test]
    fn test_api_url_without_prefix() {
        let cfg = Config {
            api_key: "k".into(),
            model: "gemini-2.0-pro".into(),
            hotkey: default_hotkey(),
            timeout_secs: 10,
            language: default_language(),
            sound_enabled: default_sound_enabled(),
            currency: default_currency(),
        };
        assert!(cfg.api_url().contains("gemini-2.0-pro:generateContent"));
    }
}

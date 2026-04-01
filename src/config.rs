// config.rs — V2 Configuration System. Zero Fluff, Auto-Migrating Data Layer.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

// ── V2 Structures ────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigV2 {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub keys: HashMap<String, String>,
}

impl Default for ConfigV2 {
    fn default() -> Self {
        Self {
            global: Default::default(),
            profiles: vec![Profile::default()],
            keys: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GlobalConfig {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default = "default_sound_enabled")]
    pub sound_enabled: bool,
    #[serde(default = "default_tray_enabled")]
    pub tray_enabled: bool,
    pub audio_device: Option<String>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            language: default_language(),
            currency: default_currency(),
            sound_enabled: default_sound_enabled(),
            tray_enabled: default_tray_enabled(),
            audio_device: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    pub name: String,
    pub hotkey: String,
    pub provider: String,
    pub model: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub transforms: Vec<TransformConfig>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "dictation".to_string(),
            hotkey: default_hotkey(),
            provider: "gemini".to_string(),
            model: default_model(),
            timeout_secs: default_timeout_secs(),
            transforms: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum TransformConfig {
    Cleanup,
    AiRewrite {
        prompt: String,
        context: String,
        model: String,
    },
    Template {
        template: String,
    },
}

// ── Default Values ───────────────────────────────────────

fn default_model() -> String { "models/gemini-2.0-flash".into() }
fn default_hotkey() -> String { "ctrl+shift+space".into() }
fn default_timeout_secs() -> u64 { 10 }
fn default_language() -> String { "auto".into() }
fn default_sound_enabled() -> bool { true }
fn default_tray_enabled() -> bool { true }
fn default_currency() -> String { "USD".into() }

// ── Legacy V1 Structure (strictly for migration) ─────────

#[derive(Deserialize)]
struct ConfigV1 {
    api_key: String,
    #[serde(default = "default_model")]
    model: String,
    #[serde(default = "default_hotkey")]
    hotkey: String,
    #[serde(default = "default_timeout_secs")]
    timeout_secs: u64,
    #[serde(default = "default_language")]
    language: String,
    #[serde(default = "default_sound_enabled")]
    sound_enabled: bool,
    #[serde(default = "default_currency")]
    currency: String,
}

// ── Migration & Loading ──────────────────────────────────

fn config_dir() -> Result<PathBuf> {
    let proj = ProjectDirs::from("", "", "g-type")
        .context("Cannot determine home directory for config")?;
    Ok(proj.config_dir().to_path_buf())
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn load() -> Result<ConfigV2> {
    let path = config_path()?;

    if !path.exists() {
        info!("No config found. Creating default V2 config.");
        let default_config = ConfigV2::default();
        save(&default_config, &path)?;
        return Ok(default_config);
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config at {}", path.display()))?;

    // Try V2 format first
    if let Ok(v2) = toml::from_str::<ConfigV2>(&raw) {
        return Ok(v2);
    }

    // Fallback: Try V1 and auto-migrate
    info!("Detected legacy V1 config format. Running auto-migration to V2...");
    match toml::from_str::<ConfigV1>(&raw) {
        Ok(v1) => {
            let mut v2 = ConfigV2::default();
            
            v2.global.language = v1.language;
            v2.global.sound_enabled = v1.sound_enabled;
            v2.global.currency = v1.currency;
            
            if !v1.api_key.is_empty() && v1.api_key != "YOUR_GEMINI_API_KEY_HERE" {
                v2.keys.insert("gemini".to_string(), v1.api_key);
            }

            v2.profiles[0].hotkey = v1.hotkey;
            v2.profiles[0].model = v1.model;
            v2.profiles[0].timeout_secs = v1.timeout_secs;

            // Save immediately. Zero user intervention.
            save(&v2, &path)?;
            info!("Successfully migrated to V2 config format.");
            Ok(v2)
        }
        Err(e) => {
            warn!("Config parsing failed. Cannot read as V2 or V1: {}", e);
            anyhow::bail!("Unreadable config file syntax at {}. Please check it or delete it.", path.display());
        }
    }
}

// ── Save ─────────────────────────────────────────────────

pub fn save(cfg: &ConfigV2, path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create config directory {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(cfg).context("Failed to serialize config")?;
    fs::write(path, &content)
        .with_context(|| format!("Failed to write config to {}", path.display()))?;
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────

pub const LANGUAGES: &[(&str, &str)] = &[
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

pub fn set_api_key(key: &str) -> Result<()> {
    let path = config_path()?;
    let mut cfg = load().unwrap_or_default();
    cfg.keys.insert("gemini".to_string(), key.to_string());
    save(&cfg, &path)?;
    println!("  {} API key updated.", "✔".to_string() /* remove colored */);
    Ok(())
}

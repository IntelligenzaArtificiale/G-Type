// config.rs — V2 configuration system with crash-safe persistence and migration.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
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
            custom_prompt: None,
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
fn default_tray_enabled() -> bool {
    true
}
fn default_currency() -> String {
    "USD".into()
}

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

fn config_dir() -> Result<PathBuf> {
    let proj = ProjectDirs::from("", "", "g-type")
        .context("Cannot determine home directory for config")?;
    Ok(proj.config_dir().to_path_buf())
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("toml.bak")
}

fn parse_config(raw: &str) -> Result<ConfigV2> {
    if let Ok(v2) = toml::from_str::<ConfigV2>(raw) {
        return Ok(v2);
    }

    let v1 = toml::from_str::<ConfigV1>(raw).context("config is neither valid V2 nor legacy V1")?;
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
    Ok(v2)
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

    match parse_config(&raw) {
        Ok(config) => {
            if toml::from_str::<ConfigV2>(&raw).is_err() {
                info!("Detected legacy V1 config. Migrating to V2.");
                save(&config, &path)?;
            }
            Ok(config)
        }
        Err(primary_error) => {
            let backup = backup_path(&path);
            if backup.exists() {
                warn!(
                    error = %primary_error,
                    backup = %backup.display(),
                    "Primary config is unreadable; trying backup"
                );
                if let Ok(backup_raw) = fs::read_to_string(&backup) {
                    if let Ok(recovered) = parse_config(&backup_raw) {
                        save(&recovered, &path)?;
                        warn!("Recovered configuration from backup");
                        return Ok(recovered);
                    }
                }
            }

            anyhow::bail!(
                "Unreadable config file at {} and no valid backup is available: {}",
                path.display(),
                primary_error
            )
        }
    }
}

pub fn save(cfg: &ConfigV2, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create config directory {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(cfg).context("Failed to serialize config")?;
    atomic_write_with_backup(path, content.as_bytes())?;
    Ok(())
}

fn atomic_write_with_backup(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("toml.tmp");
    let backup = backup_path(path);

    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)
            .with_context(|| format!("Cannot create temporary config {}", tmp.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("Cannot write temporary config {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("Cannot sync temporary config {}", tmp.display()))?;
    }

    if path.exists() {
        fs::copy(path, &backup)
            .with_context(|| format!("Cannot create config backup {}", backup.display()))?;
        if let Ok(file) = fs::OpenOptions::new().read(true).open(&backup) {
            let _ = file.sync_all();
        }
    }

    if let Err(error) = replace_file(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        if !path.exists() && backup.exists() {
            let _ = fs::copy(&backup, path);
        }
        return Err(error).with_context(|| format!("Cannot replace config {}", path.display()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

#[cfg(unix)]
fn replace_file(tmp: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(tmp, destination)
}

#[cfg(windows)]
fn replace_file(tmp: &Path, destination: &Path) -> std::io::Result<()> {
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(tmp, destination)
}

#[cfg(not(any(unix, windows)))]
fn replace_file(tmp: &Path, destination: &Path) -> std::io::Result<()> {
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(tmp, destination)
}

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
                .find(|(candidate, _)| *candidate == code)
                .map(|(_, name)| *name)
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
    println!("  ✔ API key updated.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_keeps_backup_and_latest_content() {
        let root = std::env::temp_dir().join(format!(
            "g-type-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.toml");

        atomic_write_with_backup(&path, b"first").unwrap();
        atomic_write_with_backup(&path, b"second").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), "first");
        let _ = fs::remove_dir_all(root);
    }
}

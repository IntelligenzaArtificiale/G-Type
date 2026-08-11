// input.rs — Global keyboard hook using rdev.
// Runs on a dedicated OS thread (rdev::listen is blocking).
// Detects configurable hotkey combos and sends signals via tokio mpsc.

use anyhow::{Context, Result};
use rdev::{Event, EventType, Key};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSignal {
    Start(String),
    Stop,
}

pub type InputTx = mpsc::Sender<InputSignal>;
pub type InputRx = mpsc::Receiver<InputSignal>;

const DEBOUNCE_MS: u64 = 200;

#[derive(Debug, Clone)]
pub struct Hotkey {
    pub modifiers: HashSet<Modifier>,
    pub trigger: Key,
    #[allow(dead_code)]
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Meta,
}

pub struct SharedHotkeys {
    profiles: RwLock<Vec<(Hotkey, String)>>,
}

impl SharedHotkeys {
    pub fn new(profiles: Vec<(Hotkey, String)>) -> Arc<Self> {
        Arc::new(Self {
            profiles: RwLock::new(profiles),
        })
    }

    pub fn update(&self, new_profiles: Vec<(Hotkey, String)>) {
        let mut lock = self
            .profiles
            .write()
            .expect("SharedHotkeys write lock poisoned");
        *lock = new_profiles;
        info!(count = lock.len(), "Hotkey profiles updated");
    }

    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, Vec<(Hotkey, String)>> {
        self.profiles
            .read()
            .expect("SharedHotkeys read lock poisoned")
    }
}

pub fn parse_hotkey(raw: &str) -> Result<Hotkey> {
    let parts: Vec<String> = raw
        .split('+')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        anyhow::bail!("Hotkey string is empty");
    }

    let mut modifiers = HashSet::new();
    let mut trigger: Option<Key> = None;

    for part in &parts {
        match part.as_str() {
            "ctrl" | "control" => {
                modifiers.insert(Modifier::Ctrl);
            }
            "shift" => {
                modifiers.insert(Modifier::Shift);
            }
            "alt" | "option" => {
                modifiers.insert(Modifier::Alt);
            }
            "meta" | "super" | "win" | "cmd" | "command" => {
                modifiers.insert(Modifier::Meta);
            }
            _ => {
                if trigger.is_some() {
                    anyhow::bail!(
                        "Multiple non-modifier keys in hotkey: '{}'. Use format like 'ctrl+shift+space'",
                        raw
                    );
                }
                trigger = Some(
                    str_to_rdev_key(part)
                        .with_context(|| format!("Unknown key '{}' in hotkey '{}'", part, raw))?,
                );
            }
        }
    }

    let trigger = trigger.with_context(|| {
        format!(
            "No trigger key found in hotkey '{}'. Need at least one non-modifier key.",
            raw
        )
    })?;

    Ok(Hotkey {
        modifiers,
        trigger,
        label: raw.to_string(),
    })
}

fn str_to_rdev_key(name: &str) -> Result<Key> {
    let key = match name {
        "a" => Key::KeyA,
        "b" => Key::KeyB,
        "c" => Key::KeyC,
        "d" => Key::KeyD,
        "e" => Key::KeyE,
        "f" => Key::KeyF,
        "g" => Key::KeyG,
        "h" => Key::KeyH,
        "i" => Key::KeyI,
        "j" => Key::KeyJ,
        "k" => Key::KeyK,
        "l" => Key::KeyL,
        "m" => Key::KeyM,
        "n" => Key::KeyN,
        "o" => Key::KeyO,
        "p" => Key::KeyP,
        "q" => Key::KeyQ,
        "r" => Key::KeyR,
        "s" => Key::KeyS,
        "t" => Key::KeyT,
        "u" => Key::KeyU,
        "v" => Key::KeyV,
        "w" => Key::KeyW,
        "x" => Key::KeyX,
        "y" => Key::KeyY,
        "z" => Key::KeyZ,
        "0" => Key::Num0,
        "1" => Key::Num1,
        "2" => Key::Num2,
        "3" => Key::Num3,
        "4" => Key::Num4,
        "5" => Key::Num5,
        "6" => Key::Num6,
        "7" => Key::Num7,
        "8" => Key::Num8,
        "9" => Key::Num9,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        "space" | "spacebar" => Key::Space,
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" | "ins" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" | "pgdown" => Key::PageDown,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "capslock" | "caps" => Key::CapsLock,
        "printscreen" | "prtsc" => Key::PrintScreen,
        "scrolllock" => Key::ScrollLock,
        "pause" => Key::Pause,
        "`" | "grave" | "backtick" => Key::BackQuote,
        "-" | "minus" => Key::Minus,
        "=" | "equal" | "equals" => Key::Equal,
        "[" | "bracketleft" => Key::LeftBracket,
        "]" | "bracketright" => Key::RightBracket,
        "\\" | "backslash" => Key::BackSlash,
        ";" | "semicolon" => Key::SemiColon,
        "'" | "quote" | "apostrophe" => Key::Quote,
        "," | "comma" => Key::Comma,
        "." | "period" | "dot" => Key::Dot,
        "/" | "slash" => Key::Slash,
        other => anyhow::bail!("Unknown key: '{}'", other),
    };
    Ok(key)
}

struct HookState {
    held_modifiers: HashSet<Modifier>,
    held_triggers: HashSet<Key>,
    recording: bool,
    active_profile: Option<String>,
    last_trigger: Instant,
    shared_hotkeys: Arc<SharedHotkeys>,
    tx: InputTx,
}

impl HookState {
    fn new(tx: InputTx, shared_hotkeys: Arc<SharedHotkeys>) -> Self {
        Self {
            held_modifiers: HashSet::new(),
            held_triggers: HashSet::new(),
            recording: false,
            active_profile: None,
            last_trigger: Instant::now() - std::time::Duration::from_secs(10),
            shared_hotkeys,
            tx,
        }
    }

    fn handle_event(&mut self, event: &Event) {
        match event.event_type {
            EventType::KeyPress(key) => {
                if let Some(modifier) = key_to_modifier(key) {
                    self.held_modifiers.insert(modifier);
                } else {
                    self.held_triggers.insert(key);
                }
                self.check_combo();
            }
            EventType::KeyRelease(key) => {
                if let Some(modifier) = key_to_modifier(key) {
                    self.held_modifiers.remove(&modifier);
                } else {
                    self.held_triggers.remove(&key);
                }
                self.check_release();
            }
            _ => {}
        }
    }

    fn check_combo(&mut self) {
        if self.recording {
            return;
        }

        let profiles = self.shared_hotkeys.read();
        let mut best_match: Option<&(Hotkey, String)> = None;
        let mut best_mod_count = 0;

        for profile in profiles.iter() {
            let (hotkey, _) = profile;
            let all_mods = hotkey
                .modifiers
                .iter()
                .all(|modifier| self.held_modifiers.contains(modifier));
            let trigger_held = self.held_triggers.contains(&hotkey.trigger);

            if all_mods && trigger_held {
                let mod_count = hotkey.modifiers.len();
                if mod_count > best_mod_count || best_match.is_none() {
                    best_match = Some(profile);
                    best_mod_count = mod_count;
                }
            }
        }

        if let Some((_hotkey, profile_name)) = best_match {
            let now = Instant::now();
            if now.duration_since(self.last_trigger).as_millis() < DEBOUNCE_MS as u128 {
                return;
            }
            self.last_trigger = now;
            self.recording = true;
            self.active_profile = Some(profile_name.clone());
            info!(profile = %profile_name, "Hotkey pressed — START recording");
            if self
                .tx
                .blocking_send(InputSignal::Start(profile_name.clone()))
                .is_err()
            {
                error!("Input channel closed, cannot send Start signal");
            }
        }
    }

    fn check_release(&mut self) {
        if !self.recording {
            return;
        }

        if let Some(ref profile_name) = self.active_profile {
            let profiles = self.shared_hotkeys.read();
            if let Some((hotkey, _)) = profiles.iter().find(|(_, name)| name == profile_name) {
                let all_mods = hotkey
                    .modifiers
                    .iter()
                    .all(|modifier| self.held_modifiers.contains(modifier));
                let trigger_held = self.held_triggers.contains(&hotkey.trigger);

                if !trigger_held || !all_mods {
                    self.recording = false;
                    self.active_profile = None;
                    debug!("Hotkey released — STOP");
                    if self.tx.blocking_send(InputSignal::Stop).is_err() {
                        error!("Input channel closed, cannot send Stop signal");
                    }
                }
            } else {
                self.recording = false;
                self.active_profile = None;
                let _ = self.tx.blocking_send(InputSignal::Stop);
            }
        }
    }
}

fn key_to_modifier(key: Key) -> Option<Modifier> {
    match key {
        Key::ControlLeft | Key::ControlRight => Some(Modifier::Ctrl),
        Key::ShiftLeft | Key::ShiftRight => Some(Modifier::Shift),
        Key::Alt | Key::AltGr => Some(Modifier::Alt),
        Key::MetaLeft | Key::MetaRight => Some(Modifier::Meta),
        _ => None,
    }
}

/// Spawn the platform-appropriate global keyboard listener.
///
/// Important: use compile-time cfg blocks here rather than `cfg!()`. `cfg!()`
/// only evaluates to a boolean and still type-checks both branches, which made
/// macOS/Windows builds reference the Linux-only evdev function.
pub fn spawn_listener(
    tx: InputTx,
    shutdown: Arc<AtomicBool>,
    shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    #[cfg(target_os = "linux")]
    {
        if is_wayland() {
            return spawn_evdev_listener(tx, shutdown, shared_hotkeys);
        }
    }

    spawn_rdev_listener(tx, shutdown, shared_hotkeys)
}

pub fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
}

fn spawn_rdev_listener(
    tx: InputTx,
    shutdown: Arc<AtomicBool>,
    shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("g-type-input".into())
        .spawn(move || {
            debug!("Global keyboard listener started");
            let state = Arc::new(std::sync::Mutex::new(HookState::new(tx, shared_hotkeys)));

            let callback = move |event: Event| {
                if shutdown.load(Ordering::Relaxed) {
                    return;
                }
                if let Ok(mut state) = state.lock() {
                    state.handle_event(&event);
                }
            };

            if let Err(err) = rdev::listen(callback) {
                error!(?err, "Global keyboard listener crashed");
            }
        })
        .context("Failed to spawn input listener thread")?;

    Ok(handle)
}

#[cfg(target_os = "linux")]
fn spawn_evdev_listener(
    _tx: InputTx,
    _shutdown: Arc<AtomicBool>,
    _shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("g-type-input-evdev".into())
        .spawn(move || {
            let devices = find_keyboard_devices();
            if devices.is_empty() {
                error!("No keyboard devices found. Is user in 'input' group?");
                return;
            }

            info!(
                devices = devices.len(),
                "evdev keyboard listener placeholder started (Wayland)"
            );

            std::thread::sleep(std::time::Duration::from_secs(u64::MAX));
        })
        .context("Failed to spawn evdev listener thread")?;

    Ok(handle)
}

#[cfg(target_os = "linux")]
fn find_keyboard_devices() -> Vec<std::path::PathBuf> {
    let mut keyboards = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev/input/") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                if name.starts_with("event") {
                    let cap_path = format!("/sys/class/input/{}/device/capabilities/ev", name);
                    if let Ok(caps) = std::fs::read_to_string(&cap_path) {
                        if let Ok(value) = u64::from_str_radix(caps.trim(), 16) {
                            if value & (1 << 1) != 0 {
                                keyboards.push(path);
                            }
                        }
                    }
                }
            }
        }
    }
    keyboards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_modifier() {
        assert_eq!(key_to_modifier(Key::ControlLeft), Some(Modifier::Ctrl));
        assert_eq!(key_to_modifier(Key::ControlRight), Some(Modifier::Ctrl));
        assert_eq!(key_to_modifier(Key::ShiftLeft), Some(Modifier::Shift));
        assert_eq!(key_to_modifier(Key::Alt), Some(Modifier::Alt));
        assert_eq!(key_to_modifier(Key::MetaLeft), Some(Modifier::Meta));
        assert_eq!(key_to_modifier(Key::KeyT), None);
        assert_eq!(key_to_modifier(Key::Space), None);
    }

    #[test]
    fn test_parse_hotkey_ctrl_shift_space() {
        let hotkey = parse_hotkey("ctrl+shift+space").unwrap();
        assert!(hotkey.modifiers.contains(&Modifier::Ctrl));
        assert!(hotkey.modifiers.contains(&Modifier::Shift));
        assert_eq!(hotkey.trigger, Key::Space);
    }

    #[test]
    fn test_parse_hotkey_ctrl_t() {
        let hotkey = parse_hotkey("ctrl+t").unwrap();
        assert!(hotkey.modifiers.contains(&Modifier::Ctrl));
        assert!(!hotkey.modifiers.contains(&Modifier::Shift));
        assert_eq!(hotkey.trigger, Key::KeyT);
    }

    #[test]
    fn test_parse_hotkey_alt_f9() {
        let hotkey = parse_hotkey("alt+f9").unwrap();
        assert!(hotkey.modifiers.contains(&Modifier::Alt));
        assert_eq!(hotkey.trigger, Key::F9);
    }

    #[test]
    fn test_parse_hotkey_invalid() {
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("ctrl+shift+badkey123").is_err());
    }

    #[test]
    fn test_hook_state_combo() {
        let handle = std::thread::spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let hotkey = parse_hotkey("ctrl+shift+space").unwrap();
            let shared_hotkeys = SharedHotkeys::new(vec![(hotkey, "dictation".to_string())]);
            let (tx, mut rx) = mpsc::channel(16);
            let mut state = HookState::new(tx, shared_hotkeys);

            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::ControlLeft),
            });
            assert!(state.held_modifiers.contains(&Modifier::Ctrl));
            assert!(!state.recording);

            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::ShiftLeft),
            });
            assert!(!state.recording);

            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::Space),
            });
            assert!(state.recording);

            let signal = runtime.block_on(async { rx.recv().await });
            assert_eq!(signal, Some(InputSignal::Start("dictation".to_string())));

            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyRelease(Key::Space),
            });
            assert!(!state.recording);

            let signal = runtime.block_on(async { rx.recv().await });
            assert_eq!(signal, Some(InputSignal::Stop));
        });

        handle.join().expect("Test thread panicked");
    }
}

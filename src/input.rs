// input.rs — Global keyboard hook using rdev.
// Runs on a dedicated OS thread (rdev::listen is blocking).
// Detects a configurable hotkey combo and sends signals via tokio mpsc.

use anyhow::{Context, Result};
use rdev::{Event, EventType, Key};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Signals sent from the input thread to the main event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSignal {
    /// Hotkey pressed — start recording. Carries the profile name.
    Start(String),
    /// Hotkey released — stop recording.
    Stop,
}

/// Sender type for input signals.
pub type InputTx = mpsc::Sender<InputSignal>;
/// Receiver type for input signals.
pub type InputRx = mpsc::Receiver<InputSignal>;

/// Minimum time between Start signals to prevent bouncing (ms).
const DEBOUNCE_MS: u64 = 200;

/// A parsed hotkey definition: modifier keys + one trigger key.
#[derive(Debug, Clone)]
pub struct Hotkey {
    /// Modifier keys that must all be held (ctrl, shift, alt, meta/super).
    pub modifiers: HashSet<Modifier>,
    /// The main trigger key (the non-modifier key in the combo).
    pub trigger: Key,
    /// Human-readable label (stored but currently unused, intentionally kept for debugging).
    #[allow(dead_code)]
    pub label: String,
}

/// Supported modifier types (we track left/right variants together).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Meta, // Super/Win/Cmd
}

/// Shared hotkey configuration that can be updated at runtime.
/// The listener thread reads this on every keyboard event.
pub struct SharedHotkeys {
    profiles: RwLock<Vec<(Hotkey, String)>>,
}

impl SharedHotkeys {
    pub fn new(profiles: Vec<(Hotkey, String)>) -> Arc<Self> {
        Arc::new(Self {
            profiles: RwLock::new(profiles),
        })
    }

    /// Update hotkeys at runtime (called when config changes).
    /// The listener thread will pick up the change on the next keyboard event.
    #[allow(dead_code)]
    pub fn update(&self, new_profiles: Vec<(Hotkey, String)>) {
        let mut lock = self.profiles.write().expect("SharedHotkeys write lock poisoned");
        *lock = new_profiles;
        tracing::info!(count = lock.len(), "Hotkey profiles updated");
    }

    /// Read current profiles (called by listener on every event).
    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, Vec<(Hotkey, String)>> {
        self.profiles.read().expect("SharedHotkeys read lock poisoned")
    }
}

/// Parse a hotkey string like "ctrl+shift+space" into a Hotkey struct.
pub fn parse_hotkey(raw: &str) -> Result<Hotkey> {
    let parts: Vec<String> = raw.split('+').map(|s| s.trim().to_lowercase()).collect();

    if parts.is_empty() {
        anyhow::bail!("Hotkey string is empty");
    }

    let mut modifiers = HashSet::new();
    let mut trigger: Option<Key> = None;

    for part in parts.iter() {
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
                // This should be the trigger key (last part typically)
                if trigger.is_some() {
                    anyhow::bail!("Multiple non-modifier keys in hotkey: '{}'. Use format like 'ctrl+shift+space'", raw);
                }
                trigger = Some(
                    str_to_rdev_key(part)
                        .with_context(|| format!("Unknown key '{}' in hotkey '{}'", part, raw))?,
                );
            }
        }
    }

    let trigger = trigger.context(format!(
        "No trigger key found in hotkey '{}'. Need at least one non-modifier key.",
        raw
    ))?;

    Ok(Hotkey {
        modifiers,
        trigger,
        label: raw.to_string(),
    })
}

/// Map a lowercase key name to an rdev::Key.
fn str_to_rdev_key(name: &str) -> Result<Key> {
    let key = match name {
        // Letters
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
        // Numbers
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
        // Function keys
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
        // Special keys
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
        // Punctuation
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

/// State tracked inside the keyboard hook callback.
struct HookState {
    /// Currently held modifier keys.
    held_modifiers: HashSet<Modifier>,
    /// Currently held non-modifier keys (track ALL, not just one trigger).
    held_triggers: HashSet<Key>,
    /// Whether we are currently recording.
    recording: bool,
    /// Which profile is currently active (if recording).
    active_profile: Option<String>,
    /// Last trigger time for debouncing.
    last_trigger: Instant,
    /// Shared hotkey profiles (read on every event).
    shared_hotkeys: Arc<SharedHotkeys>,
    /// Channel sender.
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
                if let Some(m) = key_to_modifier(key) {
                    self.held_modifiers.insert(m);
                } else {
                    self.held_triggers.insert(key);
                }
                self.check_combo();
            }
            EventType::KeyRelease(key) => {
                if let Some(m) = key_to_modifier(key) {
                    self.held_modifiers.remove(&m);
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
            return; // Already recording, ignore new combos
        }

        let profiles = self.shared_hotkeys.read();
        
        // Find the first matching profile
        // Priority: more modifiers = higher priority (prevents false matches)
        let mut best_match: Option<&(Hotkey, String)> = None;
        let mut best_mod_count = 0;

        for profile in profiles.iter() {
            let (hotkey, _) = profile;
            let all_mods = hotkey.modifiers.iter()
                .all(|m| self.held_modifiers.contains(m));
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
            if self.tx.blocking_send(InputSignal::Start(profile_name.clone())).is_err() {
                error!("Input channel closed, cannot send Start signal");
            }
        }
    }

    fn check_release(&mut self) {
        if !self.recording {
            return;
        }

        // Stop when the active profile's hotkey is no longer fully held
        if let Some(ref profile_name) = self.active_profile {
            let profiles = self.shared_hotkeys.read();
            if let Some((hotkey, _)) = profiles.iter()
                .find(|(_, name)| name == profile_name)
            {
                let all_mods = hotkey.modifiers.iter()
                    .all(|m| self.held_modifiers.contains(m));
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
                // Profile was removed while recording — stop
                self.recording = false;
                self.active_profile = None;
                let _ = self.tx.blocking_send(InputSignal::Stop);
            }
        }
    }
}

/// Map an rdev Key to a Modifier, if it is one.
fn key_to_modifier(key: Key) -> Option<Modifier> {
    match key {
        Key::ControlLeft | Key::ControlRight => Some(Modifier::Ctrl),
        Key::ShiftLeft | Key::ShiftRight => Some(Modifier::Shift),
        Key::Alt | Key::AltGr => Some(Modifier::Alt),
        Key::MetaLeft | Key::MetaRight => Some(Modifier::Meta),
        _ => None,
    }
}

/// Spawn a dedicated OS thread that listens for global keyboard events.
///
/// This function returns immediately. The thread runs until `shutdown` is set to true
/// or the process exits.
///
/// `tx` — channel for sending Start/Stop signals to the async event loop.
/// `shared_hotkeys` — the parsed hotkey combos to listen for.
pub fn spawn_listener(
    tx: InputTx,
    shutdown: Arc<AtomicBool>,
    shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    if cfg!(target_os = "linux") && is_wayland() {
        spawn_evdev_listener(tx, shutdown, shared_hotkeys)
    } else {
        spawn_rdev_listener(tx, shutdown, shared_hotkeys)
    }
}

/// Detect if running on Wayland
pub fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
}

/// rdev-based listener (X11, macOS, Windows)
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
                if let Ok(mut s) = state.lock() {
                    s.handle_event(&event);
                }
            };

            if let Err(e) = rdev::listen(callback) {
                error!(?e, "Global keyboard listener crashed");
            }
        })
        .context("Failed to spawn input listener thread")?;

    Ok(handle)
}

/// evdev-based listener (Wayland Linux)
/// Requires user in 'input' group: sudo usermod -aG input $USER
#[cfg(target_os = "linux")]
fn spawn_evdev_listener(
    _tx: InputTx,
    _shutdown: Arc<AtomicBool>,
    _shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    // Placeholder for actual evdev implementation
    let handle = std::thread::Builder::new()
        .name("g-type-input-evdev".into())
        .spawn(move || {
            let devices = find_keyboard_devices();
            if devices.is_empty() {
                error!("No keyboard devices found. Is user in 'input' group?");
                return;
            }

            info!(devices = devices.len(), "evdev keyboard listener placeholder started (Wayland)");
            
            // TODO: implement actual evdev listening
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
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("event") {
                    let cap_path = format!("/sys/class/input/{}/device/capabilities/ev", name);
                    if let Ok(caps) = std::fs::read_to_string(&cap_path) {
                        let caps = caps.trim();
                        if let Ok(val) = u64::from_str_radix(caps, 16) {
                            if val & (1 << 1) != 0 { // EV_KEY bit
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
        let hk = parse_hotkey("ctrl+shift+space").unwrap();
        assert!(hk.modifiers.contains(&Modifier::Ctrl));
        assert!(hk.modifiers.contains(&Modifier::Shift));
        assert_eq!(hk.trigger, Key::Space);
    }

    #[test]
    fn test_parse_hotkey_ctrl_t() {
        let hk = parse_hotkey("ctrl+t").unwrap();
        assert!(hk.modifiers.contains(&Modifier::Ctrl));
        assert!(!hk.modifiers.contains(&Modifier::Shift));
        assert_eq!(hk.trigger, Key::KeyT);
    }

    #[test]
    fn test_parse_hotkey_alt_f9() {
        let hk = parse_hotkey("alt+f9").unwrap();
        assert!(hk.modifiers.contains(&Modifier::Alt));
        assert_eq!(hk.trigger, Key::F9);
    }

    #[test]
    fn test_parse_hotkey_invalid() {
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("ctrl+shift+badkey123").is_err());
    }

    #[test]
    fn test_hook_state_combo() {
        // Run inside a standalone thread to avoid tokio runtime blocking conflict.
        let handle = std::thread::spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let hotkey = parse_hotkey("ctrl+shift+space").unwrap();
            let shared_hotkeys = SharedHotkeys::new(vec![(hotkey, "dictation".to_string())]);
            let (tx, mut rx) = mpsc::channel(16);
            let mut state = HookState::new(tx, shared_hotkeys);

            // Press Ctrl
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::ControlLeft),
            });
            assert!(state.held_modifiers.contains(&Modifier::Ctrl));
            assert!(!state.recording);

            // Press Shift
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::ShiftLeft),
            });
            assert!(!state.recording);

            // Press Space
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::Space),
            });
            assert!(state.recording);

            let signal = rt.block_on(async { rx.recv().await });
            assert_eq!(signal, Some(InputSignal::Start("dictation".to_string())));

            // Release Space
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyRelease(Key::Space),
            });
            assert!(!state.recording);

            let signal = rt.block_on(async { rx.recv().await });
            assert_eq!(signal, Some(InputSignal::Stop));
        });

        handle.join().expect("Test thread panicked");
    }
}

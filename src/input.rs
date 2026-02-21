// input.rs — Global keyboard hook using rdev.
// Runs on a dedicated OS thread (rdev::listen is blocking).
// Detects a configurable hotkey combo and sends signals via tokio mpsc.

use anyhow::{Context, Result};
use rdev::{Event, EventType, Key};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Signals sent from the input thread to the main event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSignal {
    /// Hotkey pressed — start recording.
    Start,
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
    /// Human-readable label for log messages.
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
    /// Whether the trigger key is currently held.
    trigger_held: bool,
    /// Whether we are currently recording.
    recording: bool,
    /// Last trigger time for debouncing.
    last_trigger: Instant,
    /// The hotkey definition.
    hotkey: Hotkey,
    /// Channel sender.
    tx: InputTx,
}

impl HookState {
    fn new(tx: InputTx, hotkey: Hotkey) -> Self {
        Self {
            held_modifiers: HashSet::new(),
            trigger_held: false,
            recording: false,
            last_trigger: Instant::now() - std::time::Duration::from_secs(10),
            hotkey,
            tx,
        }
    }

    fn handle_event(&mut self, event: &Event) {
        match event.event_type {
            EventType::KeyPress(key) => {
                if let Some(m) = key_to_modifier(key) {
                    self.held_modifiers.insert(m);
                }
                if key == self.hotkey.trigger {
                    self.trigger_held = true;
                }
                self.check_combo();
            }
            EventType::KeyRelease(key) => {
                if let Some(m) = key_to_modifier(key) {
                    self.held_modifiers.remove(&m);
                }
                if key == self.hotkey.trigger {
                    self.trigger_held = false;
                }
                self.check_release();
            }
            _ => {}
        }
    }

    fn check_combo(&mut self) {
        // All required modifiers must be held AND the trigger key
        let all_mods = self
            .hotkey
            .modifiers
            .iter()
            .all(|m| self.held_modifiers.contains(m));
        if all_mods && self.trigger_held && !self.recording {
            let now = Instant::now();
            if now.duration_since(self.last_trigger).as_millis() < DEBOUNCE_MS as u128 {
                debug!(hotkey = %self.hotkey.label, "Hotkey debounced");
                return;
            }
            self.last_trigger = now;
            self.recording = true;
            info!(hotkey = %self.hotkey.label, "Hotkey pressed — START recording");
            if self.tx.blocking_send(InputSignal::Start).is_err() {
                error!("Input channel closed, cannot send Start signal");
            }
        }
    }

    fn check_release(&mut self) {
        if self.recording {
            // Stop when trigger is released OR any required modifier is released
            let all_mods = self
                .hotkey
                .modifiers
                .iter()
                .all(|m| self.held_modifiers.contains(m));
            if !self.trigger_held || !all_mods {
                self.recording = false;
                debug!(hotkey = %self.hotkey.label, "Hotkey released");
                if self.tx.blocking_send(InputSignal::Stop).is_err() {
                    error!("Input channel closed, cannot send Stop signal");
                }
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
/// `hotkey` — the parsed hotkey combo to listen for.
pub fn spawn_listener(
    tx: InputTx,
    shutdown: Arc<AtomicBool>,
    hotkey: Hotkey,
) -> Result<std::thread::JoinHandle<()>> {
    let label = hotkey.label.clone();
    let handle = std::thread::Builder::new()
        .name("g-type-input".into())
        .spawn(move || {
            debug!(hotkey = %label, "Global keyboard listener started");
            let state = Arc::new(std::sync::Mutex::new(HookState::new(tx, hotkey)));

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
            let (tx, mut rx) = mpsc::channel(16);
            let mut state = HookState::new(tx, hotkey);

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
            assert_eq!(signal, Some(InputSignal::Start));

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

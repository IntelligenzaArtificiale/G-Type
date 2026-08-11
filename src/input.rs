// input.rs — Cross-platform global hotkey listener.
// X11/macOS/Windows use rdev. Native Linux Wayland reads evdev directly.

use anyhow::{Context, Result};
use rdev::{Event, EventType, Key};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

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
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
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
            debug!("rdev global keyboard listener started");
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
#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxInputEvent {
    time: libc::timeval,
    event_type: u16,
    code: u16,
    value: i32,
}

#[cfg(target_os = "linux")]
fn spawn_evdev_listener(
    tx: InputTx,
    shutdown: Arc<AtomicBool>,
    shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("g-type-input-evdev".into())
        .spawn(move || {
            let state = Arc::new(std::sync::Mutex::new(HookState::new(tx, shared_hotkeys)));
            let devices = find_keyboard_devices();
            let mut readers = Vec::new();

            for path in devices {
                match std::fs::OpenOptions::new().read(true).open(&path) {
                    Ok(file) => {
                        if let Err(error) = set_nonblocking(&file) {
                            warn!(device = %path.display(), %error, "Could not set evdev device non-blocking");
                            continue;
                        }

                        let state = state.clone();
                        let shutdown = shutdown.clone();
                        let device_name = path.display().to_string();
                        readers.push(std::thread::spawn(move || {
                            evdev_reader_loop(file, &device_name, state, shutdown)
                        }));
                    }
                    Err(error) => {
                        debug!(device = %path.display(), %error, "Cannot open evdev input device");
                    }
                }
            }

            if readers.is_empty() {
                warn!(
                    "Wayland detected but no readable /dev/input/event* keyboard was found. Add the user to the 'input' group and log in again; falling back to rdev/XWayland."
                );
                let state = state.clone();
                let callback = move |event: Event| {
                    if shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut state) = state.lock() {
                        state.handle_event(&event);
                    }
                };
                if let Err(error) = rdev::listen(callback) {
                    error!(?error, "Wayland fallback listener failed");
                }
                return;
            }

            info!(devices = readers.len(), "Wayland evdev keyboard listener started");
            for reader in readers {
                let _ = reader.join();
            }
        })
        .context("Failed to spawn evdev listener thread")?;

    Ok(handle)
}

#[cfg(target_os = "linux")]
fn set_nonblocking(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn evdev_reader_loop(
    file: std::fs::File,
    device_name: &str,
    state: Arc<std::sync::Mutex<HookState>>,
    shutdown: Arc<AtomicBool>,
) {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();

    while !shutdown.load(Ordering::Relaxed) {
        let mut event = std::mem::MaybeUninit::<LinuxInputEvent>::uninit();
        let size = std::mem::size_of::<LinuxInputEvent>();
        let read = unsafe { libc::read(fd, event.as_mut_ptr().cast::<libc::c_void>(), size) };

        if read == size as isize {
            let event = unsafe { event.assume_init() };
            if event.event_type != 1 || event.value == 2 {
                continue;
            }
            if let Some(key) = linux_keycode_to_rdev(event.code) {
                let event_type = if event.value == 0 {
                    EventType::KeyRelease(key)
                } else if event.value == 1 {
                    EventType::KeyPress(key)
                } else {
                    continue;
                };
                let event = Event {
                    time: std::time::SystemTime::now(),
                    name: None,
                    event_type,
                };
                if let Ok(mut state) = state.lock() {
                    state.handle_event(&event);
                }
            }
            continue;
        }

        if read < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::WouldBlock {
                warn!(device = device_name, %error, "evdev read failed");
                break;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(target_os = "linux")]
fn linux_keycode_to_rdev(code: u16) -> Option<Key> {
    Some(match code {
        1 => Key::Escape,
        2 => Key::Num1,
        3 => Key::Num2,
        4 => Key::Num3,
        5 => Key::Num4,
        6 => Key::Num5,
        7 => Key::Num6,
        8 => Key::Num7,
        9 => Key::Num8,
        10 => Key::Num9,
        11 => Key::Num0,
        12 => Key::Minus,
        13 => Key::Equal,
        14 => Key::Backspace,
        15 => Key::Tab,
        16 => Key::KeyQ,
        17 => Key::KeyW,
        18 => Key::KeyE,
        19 => Key::KeyR,
        20 => Key::KeyT,
        21 => Key::KeyY,
        22 => Key::KeyU,
        23 => Key::KeyI,
        24 => Key::KeyO,
        25 => Key::KeyP,
        26 => Key::LeftBracket,
        27 => Key::RightBracket,
        28 => Key::Return,
        29 => Key::ControlLeft,
        30 => Key::KeyA,
        31 => Key::KeyS,
        32 => Key::KeyD,
        33 => Key::KeyF,
        34 => Key::KeyG,
        35 => Key::KeyH,
        36 => Key::KeyJ,
        37 => Key::KeyK,
        38 => Key::KeyL,
        39 => Key::SemiColon,
        40 => Key::Quote,
        41 => Key::BackQuote,
        42 => Key::ShiftLeft,
        43 => Key::BackSlash,
        44 => Key::KeyZ,
        45 => Key::KeyX,
        46 => Key::KeyC,
        47 => Key::KeyV,
        48 => Key::KeyB,
        49 => Key::KeyN,
        50 => Key::KeyM,
        51 => Key::Comma,
        52 => Key::Dot,
        53 => Key::Slash,
        54 => Key::ShiftRight,
        56 => Key::Alt,
        57 => Key::Space,
        58 => Key::CapsLock,
        59 => Key::F1,
        60 => Key::F2,
        61 => Key::F3,
        62 => Key::F4,
        63 => Key::F5,
        64 => Key::F6,
        65 => Key::F7,
        66 => Key::F8,
        67 => Key::F9,
        68 => Key::F10,
        70 => Key::ScrollLock,
        87 => Key::F11,
        88 => Key::F12,
        97 => Key::ControlRight,
        99 => Key::PrintScreen,
        100 => Key::AltGr,
        102 => Key::Home,
        103 => Key::UpArrow,
        104 => Key::PageUp,
        105 => Key::LeftArrow,
        106 => Key::RightArrow,
        107 => Key::End,
        108 => Key::DownArrow,
        109 => Key::PageDown,
        110 => Key::Insert,
        111 => Key::Delete,
        119 => Key::Pause,
        125 => Key::MetaLeft,
        126 => Key::MetaRight,
        _ => return None,
    })
}

#[cfg(target_os = "linux")]
fn find_keyboard_devices() -> Vec<std::path::PathBuf> {
    let mut devices = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev/input/") {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with("event") {
                continue;
            }

            let cap_path = format!("/sys/class/input/{}/device/capabilities/ev", name);
            if let Ok(caps) = std::fs::read_to_string(&cap_path) {
                if let Ok(value) = u64::from_str_radix(caps.trim(), 16) {
                    if value & (1 << 1) != 0 {
                        devices.push(path);
                    }
                }
            }
        }
    }
    devices
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
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::ShiftLeft),
            });
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::Space),
            });

            assert_eq!(
                runtime.block_on(async { rx.recv().await }),
                Some(InputSignal::Start("dictation".to_string()))
            );

            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyRelease(Key::Space),
            });
            assert_eq!(
                runtime.block_on(async { rx.recv().await }),
                Some(InputSignal::Stop)
            );
        });

        handle.join().expect("Test thread panicked");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_keycodes_cover_default_hotkey() {
        assert_eq!(linux_keycode_to_rdev(29), Some(Key::ControlLeft));
        assert_eq!(linux_keycode_to_rdev(42), Some(Key::ShiftLeft));
        assert_eq!(linux_keycode_to_rdev(57), Some(Key::Space));
    }
}

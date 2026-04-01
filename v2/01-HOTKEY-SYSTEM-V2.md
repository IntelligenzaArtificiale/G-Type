# 01 — HOTKEY SYSTEM V2: Fix + Multi-Hotkey + Profili

## Stato Attuale e Bug

### File coinvolti
- `src/input.rs` (355 righe) — hook globale tastiera
- `src/app.rs` (301 righe) — FSM del daemon
- `src/config.rs` (660 righe) — config TOML + wizard

### BUG CRITICO: Cambio hotkey non funziona a runtime

**Root cause**: La hotkey viene parsata UNA VOLTA al boot in `app.rs:53`:
```rust
// app.rs:53 — ATTUALE
let hotkey = input::parse_hotkey(&config.hotkey).context("Invalid hotkey in config")?;
```

Poi viene passata come valore owned a `spawn_listener()` in `app.rs:61`:
```rust
// app.rs:61 — ATTUALE  
let _input_handle = crate::input::spawn_listener(input_tx, shutdown_clone, hotkey)
```

Dentro `spawn_listener()` (`input.rs:303-331`), la hotkey diventa parte di `HookState`:
```rust
// input.rs:313 — ATTUALE
let state = Arc::new(std::sync::Mutex::new(HookState::new(tx, hotkey)));
```

`HookState` possiede la hotkey come campo owned (`input.rs:203`):
```rust
// input.rs:192-206 — ATTUALE
struct HookState {
    held_modifiers: HashSet<Modifier>,
    trigger_held: bool,
    recording: bool,
    last_trigger: Instant,
    hotkey: Hotkey,      // ← OWNED, immutabile dopo creazione
    tx: InputTx,
}
```

`rdev::listen()` (`input.rs:324`) è una funzione BLOCCANTE che non ritorna mai. Non esiste modo di:
1. Terminare il listener thread
2. Passare una nuova hotkey al thread
3. Ricaricare la config

### SECONDO PROBLEMA: Un solo hotkey = un solo profilo

La struttura attuale supporta una sola hotkey. Per il sistema a profili (dictation, email, code, etc.) servono N hotkey contemporanee, ognuna legata a un profilo.

---

## Soluzione: SharedHotkeys con Arc<RwLock>

### Concetto

Invece di passare una `Hotkey` owned al listener, passiamo un `Arc<RwLock<Vec<(Hotkey, String)>>>`. Il callback `rdev::listen` legge il RwLock ad ogni evento di tastiera (costo: ~nanoseconds per read lock). Quando la config cambia, aggiorniamo il RwLock dall'esterno. Il thread listener non va mai riavviato.

### Nuova struttura InputSignal

```rust
// input.rs — NUOVO InputSignal
/// Signals sent from the input thread to the main event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSignal {
    /// Hotkey pressed — start recording. Carries the profile name.
    Start(String),
    /// Hotkey released — stop recording.
    Stop,
}
```

**Riferimento attuale** (`input.rs:14-21`): oggi `InputSignal::Start` non porta dati. Va modificato.

### Nuova struttura SharedHotkeys

```rust
// input.rs — NUOVO
use std::sync::RwLock;

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
```

### Nuovo HookState

```rust
// input.rs — NUOVO HookState (sostituisce l'attuale a riga 192-206)
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
            let (hotkey, _name) = profile;
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
```

### Nuovo spawn_listener

```rust
// input.rs — NUOVO spawn_listener (sostituisce l'attuale a riga 303-331)
pub fn spawn_listener(
    tx: InputTx,
    shutdown: Arc<AtomicBool>,
    shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("g-type-input".into())
        .spawn(move || {
            debug!("Global keyboard listener started");
            let state = Arc::new(std::sync::Mutex::new(
                HookState::new(tx, shared_hotkeys)
            ));

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
```

### Modifiche a app.rs

```rust
// app.rs — MODIFICHE alla funzione run()

pub async fn run(config: ConfigV2) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));

    // Build shared hotkeys from all profiles
    let hotkey_profiles: Vec<(input::Hotkey, String)> = config.profiles.iter()
        .filter_map(|p| {
            input::parse_hotkey(&p.hotkey).ok().map(|hk| (hk, p.name.clone()))
        })
        .collect();

    let shared_hotkeys = input::SharedHotkeys::new(hotkey_profiles);

    // Channel for keyboard input signals
    let (input_tx, mut input_rx): (input::InputTx, input::InputRx) = mpsc::channel(32);

    // Spawn listener with shared hotkeys (instead of single hotkey)
    let shutdown_clone = shutdown.clone();
    let _input_handle = input::spawn_listener(input_tx, shutdown_clone, shared_hotkeys.clone())
        .context("Failed to spawn keyboard listener")?;

    // ... signal handler unchanged ...

    info!(profiles = config.profiles.len(), "Ready — hold hotkey to dictate.");

    loop {
        if shutdown.load(Ordering::SeqCst) {
            info!("Shutting down gracefully.");
            return Ok(());
        }

        // Wait for Start signal with profile name
        match input_rx.recv().await {
            Some(InputSignal::Start(profile_name)) => {
                // Find the matching profile
                let profile = config.profiles.iter()
                    .find(|p| p.name == profile_name);

                if let Some(profile) = profile {
                    info!(profile = %profile_name, "🎤 Recording...");
                    if config.global.sound_enabled {
                        crate::audio_feedback::play_start_beep();
                    }
                    // Run the recording→transcribe→inject pipeline
                    state_recording(&config, profile, &mut input_rx).await;
                } else {
                    warn!(profile = %profile_name, "Unknown profile triggered");
                }
            }
            Some(InputSignal::Stop) => {
                // Spurious stop while idle, ignore
                continue;
            }
            None => {
                error!("Input channel closed unexpectedly");
                return Ok(());
            }
        }
    }
}
```

### Modifiche a state_recording in app.rs

La firma cambia per accettare il profilo:

```rust
// app.rs — NUOVA firma state_recording
async fn state_recording(
    config: &ConfigV2,
    profile: &Profile,      // ← NUOVO: quale profilo ha triggerato
    input_rx: &mut InputRx,
) {
    // Audio capture — INVARIATO (stesso codice attuale da riga 149-210)
    // ...

    // Transcription — USA IL PROVIDER DEL PROFILO
    let provider = providers::create_provider(profile, &config.keys)
        .expect("Failed to create provider");

    let result = provider.transcribe(&all_samples, &profile.language).await;

    let (raw_text, usage) = match result {
        Ok(r) => (r.text, r.usage),
        Err(e) => {
            error!(%e, "Transcription failed");
            if config.global.sound_enabled {
                crate::audio_feedback::play_error_beep();
            }
            return;
        }
    };

    if raw_text.is_empty() {
        warn!("Empty transcription, skipping");
        return;
    }

    // Transform pipeline — NUOVO
    let final_text = if !profile.transforms.is_empty() {
        let ctx = transforms::TransformContext {
            language: profile.language.clone(),
            profile_name: profile.name.clone(),
            api_keys: config.keys.clone(),
            audio_duration_secs: duration,
        };
        match transforms::run_pipeline(&profile.transforms, &raw_text, &ctx).await {
            Ok(text) => text,
            Err(e) => {
                warn!(%e, "Transform pipeline failed, using raw text");
                raw_text.clone()
            }
        }
    } else {
        raw_text.clone()
    };

    // Tracking — aggiunge campo text e profile
    // ...

    // Injection — INVARIATO
    let text = final_text.clone();
    let inject_result = tokio::task::spawn_blocking(move || injector::inject(&text)).await;
    // ... error handling invariato ...
}
```

### Config v2 — Struttura Profile

```rust
// config.rs — NUOVE strutture

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigV2 {
    pub global: GlobalConfig,
    pub keys: HashMap<String, String>,  // provider_name → api_key
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GlobalConfig {
    #[serde(default = "default_sound_enabled")]
    pub sound_enabled: bool,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default = "default_language")]
    pub default_language: String,
    #[serde(default = "default_true")]
    pub tray_enabled: bool,
    #[serde(default = "default_true")]
    pub save_transcriptions: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    pub name: String,
    pub hotkey: String,
    pub provider: String,           // "gemini", "openai", "deepgram", "local"
    pub model: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub transforms: Vec<TransformConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum TransformConfig {
    #[serde(rename = "cleanup")]
    Cleanup,
    #[serde(rename = "ai_rewrite")]
    AiRewrite {
        prompt: String,
        #[serde(default)]
        context: String,
        #[serde(default = "default_rewrite_model")]
        model: String,
    },
    #[serde(rename = "template")]
    Template { template: String },
}

fn default_rewrite_model() -> String { "gemini-2.0-flash".into() }
fn default_true() -> bool { true }
```

### Migrazione v1 → v2

```rust
// config.rs — Migrazione automatica

pub fn load() -> Result<ConfigV2> {
    let path = config_path()?;

    if !path.exists() {
        eprintln!("  No config found. Starting first-time setup...");
        return interactive_setup_v2(&path);
    }

    let raw = fs::read_to_string(&path)?;

    // Try v2 format first
    if let Ok(v2) = toml::from_str::<ConfigV2>(&raw) {
        if !v2.profiles.is_empty() {
            return Ok(v2);
        }
    }

    // Fallback: parse v1 format
    if let Ok(v1) = toml::from_str::<ConfigV1>(&raw) {
        info!("Migrating config from v1 to v2");
        let v2 = ConfigV2 {
            global: GlobalConfig {
                sound_enabled: v1.sound_enabled,
                currency: v1.currency.clone(),
                default_language: v1.language.clone(),
                tray_enabled: true,
                save_transcriptions: true,
            },
            keys: {
                let mut m = HashMap::new();
                m.insert("gemini".to_string(), v1.api_key.clone());
                m
            },
            profiles: vec![Profile {
                name: "dictation".to_string(),
                hotkey: v1.hotkey.clone(),
                provider: "gemini".to_string(),
                model: v1.model.clone(),
                language: v1.language.clone(),
                transforms: vec![],
            }],
        };
        save_v2(&v2, &path)?;
        return Ok(v2);
    }

    anyhow::bail!("Invalid config format at {}", path.display())
}

// Alias for backward compat during transition
type ConfigV1 = Config; // la struct Config attuale
```

### Wayland Support per Hotkey

```rust
// input.rs — Aggiunta in fondo al file

/// Detect if running on Wayland
pub fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
}

/// Spawn listener with platform-appropriate backend
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

/// rdev-based listener (X11, macOS, Windows) — il codice attuale refactored
fn spawn_rdev_listener(
    tx: InputTx,
    shutdown: Arc<AtomicBool>,
    shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    // ... codice attuale di spawn_listener, identico ...
}

/// evdev-based listener (Wayland Linux)
/// Requires user in 'input' group: sudo usermod -aG input $USER
#[cfg(target_os = "linux")]
fn spawn_evdev_listener(
    tx: InputTx,
    shutdown: Arc<AtomicBool>,
    shared_hotkeys: Arc<SharedHotkeys>,
) -> Result<std::thread::JoinHandle<()>> {
    use std::fs;

    let handle = std::thread::Builder::new()
        .name("g-type-input-evdev".into())
        .spawn(move || {
            // Find keyboard devices in /dev/input/
            let devices = find_keyboard_devices();
            if devices.is_empty() {
                error!("No keyboard devices found. Is user in 'input' group?");
                return;
            }

            info!(devices = devices.len(), "evdev keyboard listener started (Wayland)");

            // Open all keyboard devices and poll them
            // Use epoll or select to monitor multiple fds
            // Map evdev keycodes to rdev::Key equivalents
            // Feed events into the same HookState logic

            // This is the evdev implementation.
            // Key mapping: evdev KEY_LEFTCTRL → Modifier::Ctrl, etc.
            // The HookState logic is IDENTICAL — only the event source changes.
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
                    // Check if device has EV_KEY capability
                    // Read /sys/class/input/{name}/device/capabilities/ev
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
```

### Test da scrivere

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_hotkeys_update() {
        let profiles = vec![
            (parse_hotkey("ctrl+shift+space").unwrap(), "dictation".to_string()),
        ];
        let shared = SharedHotkeys::new(profiles);

        assert_eq!(shared.read().len(), 1);

        shared.update(vec![
            (parse_hotkey("ctrl+shift+space").unwrap(), "dictation".to_string()),
            (parse_hotkey("ctrl+shift+e").unwrap(), "email".to_string()),
        ]);

        assert_eq!(shared.read().len(), 2);
    }

    #[test]
    fn test_multi_hotkey_priority() {
        // ctrl+shift+space should not trigger when ctrl+shift+e is pressed
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all().build().unwrap();

        let profiles = vec![
            (parse_hotkey("ctrl+shift+space").unwrap(), "dictation".to_string()),
            (parse_hotkey("ctrl+shift+e").unwrap(), "email".to_string()),
        ];
        let shared = SharedHotkeys::new(profiles);
        let (tx, mut rx) = mpsc::channel(16);
        let mut state = HookState::new(tx, shared);

        // Press Ctrl+Shift+E
        state.handle_event(&make_event(EventType::KeyPress(Key::ControlLeft)));
        state.handle_event(&make_event(EventType::KeyPress(Key::ShiftLeft)));
        state.handle_event(&make_event(EventType::KeyPress(Key::KeyE)));

        let signal = rt.block_on(async { rx.recv().await });
        assert_eq!(signal, Some(InputSignal::Start("email".to_string())));
    }

    fn make_event(event_type: EventType) -> Event {
        Event {
            time: std::time::SystemTime::now(),
            name: None,
            event_type,
        }
    }
}
```

---

## Checklist di Implementazione

- [ ] Creare `InputSignal::Start(String)` con nome profilo
- [ ] Creare struct `SharedHotkeys` con `Arc<RwLock>`
- [ ] Modificare `HookState` per tracciare multipli trigger e usare SharedHotkeys
- [ ] Modificare `check_combo()` per matchare N profili con priorità
- [ ] Modificare `check_release()` per tracciare quale profilo è attivo
- [ ] Modificare `spawn_listener()` per accettare `Arc<SharedHotkeys>`
- [ ] Aggiungere detection Wayland + stub per evdev listener
- [ ] Modificare `app.rs::run()` per costruire SharedHotkeys dai profili
- [ ] Modificare `app.rs` loop per ricevere `InputSignal::Start(profile_name)`
- [ ] Modificare `state_recording()` per accettare `&Profile`
- [ ] Creare strutture `ConfigV2`, `GlobalConfig`, `Profile`, `TransformConfig`
- [ ] Implementare migrazione v1 → v2 in `config::load()`
- [ ] Scrivere test per multi-hotkey, priorità, update runtime
- [ ] Aggiornare wizard per setup v2 (profili multipli)

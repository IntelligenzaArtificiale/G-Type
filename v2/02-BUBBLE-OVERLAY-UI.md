# 02 — BUBBLE OVERLAY UI: Sistema Visivo Floating

## Obiettivo

Creare un sistema di bubble/pill floating che appare sopra tutte le app, esattamente come Wispr Flow. Di default c'è una bubble principale "dictation". Ogni profilo aggiuntivo aggiunge una bubble a fianco. Le bubble si chiudono/nascondono dinamicamente in base allo stato.

## Architettura UI a 3 Layer

### Layer 1 — Tray Icon (sempre visibile)

**Crate**: `tray-icon` (v0.19+) + `muda` (v0.15+)

```toml
# Cargo.toml — nuove dipendenze
tray-icon = "0.19"
muda = "0.15"
```

**File**: `src/tray.rs` (nuovo)

```rust
// tray.rs — System tray icon + menu

use muda::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};
use std::sync::Arc;
use tokio::sync::watch;

/// State broadcasted from the daemon to the tray
#[derive(Debug, Clone)]
pub enum DaemonState {
    Idle,
    Recording { profile: String, duration_secs: f32 },
    Processing { profile: String },
    Error(String),
}

pub struct TrayManager {
    _icon: TrayIcon,
}

impl TrayManager {
    pub fn new(
        profiles: &[String],
        state_rx: watch::Receiver<DaemonState>,
    ) -> anyhow::Result<Self> {
        let menu = Menu::new();

        // Status item (disabled, just shows state)
        let status = MenuItem::new("G-Type: Ready", false, None);
        menu.append(&status)?;
        menu.append(&PredefinedMenuItem::separator())?;

        // Profile submenu
        if profiles.len() > 1 {
            let profiles_menu = Submenu::new("Profiles", true);
            for name in profiles {
                profiles_menu.append(&MenuItem::new(name, true, None))?;
            }
            menu.append(&profiles_menu)?;
            menu.append(&PredefinedMenuItem::separator())?;
        }

        // Actions
        let settings = MenuItem::new("Settings...", true, None);
        let stats = MenuItem::new("Stats", true, None);
        let quit = MenuItem::new("Quit G-Type", true, None);

        menu.append(&settings)?;
        menu.append(&stats)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit)?;

        // Icon — 32x32 PNG embedded at compile time
        // Genera con: convert -size 32x32 xc:transparent -fill '#4ade80' 
        //             -draw 'circle 16,16 16,4' icon_idle.png
        let icon_rgba = include_bytes!("../assets/icon_idle_32x32.rgba");
        let icon = tray_icon::Icon::from_rgba(icon_rgba.to_vec(), 32, 32)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip("G-Type — Voice Dictation")
            .build()?;

        // Menu event handler
        let status_clone = status.clone();
        let settings_clone = settings.clone();
        let quit_clone = quit.clone();

        // Handle menu events in a separate task
        tokio::spawn(async move {
            let mut rx = state_rx;
            loop {
                if rx.changed().await.is_err() { break; }
                let state = rx.borrow().clone();
                let text = match state {
                    DaemonState::Idle => "G-Type: Ready".to_string(),
                    DaemonState::Recording { ref profile, duration_secs } =>
                        format!("🔴 Recording [{}] {:.0}s", profile, duration_secs),
                    DaemonState::Processing { ref profile } =>
                        format!("⏳ Transcribing [{}]...", profile),
                    DaemonState::Error(ref msg) =>
                        format!("❌ {}", msg),
                };
                status_clone.set_text(&text);
            }
        });

        Ok(Self { _icon: tray })
    }
}
```

### Layer 2 — Floating Pill Overlay (appare durante recording/processing)

**Crate**: `wry` (webview nativo, ~2MB overhead) oppure `winit` + `softbuffer` (zero overhead ma più codice)

**Scelta raccomandata**: `wry` per la prima versione (rapidità di sviluppo, CSS per lo styling), poi eventualmente migrare a `winit` + `tiny-skia` per zero dipendenze webview.

```toml
# Cargo.toml
wry = "0.50"   # check latest version
winit = "0.30"
```

**File**: `src/overlay.rs` (nuovo)

```rust
// overlay.rs — Floating pill overlay

use std::sync::Arc;
use tokio::sync::watch;
use anyhow::Result;

/// HTML/CSS/JS per il pill overlay — embedded nel binario
const PILL_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { margin:0; padding:0; box-sizing:border-box; }
  body { 
    background: transparent; 
    -webkit-app-region: drag;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    user-select: none;
  }
  
  .container {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 16px;
    background: rgba(15, 15, 15, 0.92);
    border-radius: 28px;
    backdrop-filter: blur(24px) saturate(180%);
    -webkit-backdrop-filter: blur(24px) saturate(180%);
    border: 1px solid rgba(255,255,255,0.08);
    box-shadow: 0 8px 32px rgba(0,0,0,0.4), 0 0 0 1px rgba(255,255,255,0.05);
    transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  }
  
  .container.recording {
    border-color: rgba(239, 68, 68, 0.3);
    box-shadow: 0 8px 32px rgba(239, 68, 68, 0.15), 0 0 0 1px rgba(239, 68, 68, 0.2);
  }
  
  .container.processing {
    border-color: rgba(59, 130, 246, 0.3);
  }
  
  .dot {
    width: 10px; height: 10px;
    border-radius: 50%;
    background: #4ade80;
    flex-shrink: 0;
    transition: background 0.3s;
  }
  .dot.recording {
    background: #ef4444;
    animation: pulse 1.2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }
  .dot.processing {
    background: #3b82f6;
    animation: spin-dot 1s linear infinite;
  }
  
  .label {
    color: #e5e5e5;
    font-size: 13px;
    font-weight: 500;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }
  
  .time {
    color: #737373;
    font-size: 12px;
    font-family: 'SF Mono', 'Cascadia Code', 'JetBrains Mono', monospace;
    font-variant-numeric: tabular-nums;
    min-width: 32px;
    text-align: right;
  }
  
  .profiles {
    display: flex;
    gap: 4px;
    margin-left: 4px;
  }
  
  .chip {
    padding: 3px 10px;
    border-radius: 12px;
    font-size: 11px;
    font-weight: 500;
    background: rgba(255,255,255,0.07);
    color: #a3a3a3;
    border: 1px solid rgba(255,255,255,0.06);
    cursor: pointer;
    -webkit-app-region: no-drag;
    transition: all 0.2s;
    white-space: nowrap;
  }
  .chip:hover {
    background: rgba(255,255,255,0.12);
    color: #d4d4d4;
  }
  .chip.active {
    background: rgba(59, 130, 246, 0.25);
    color: #93c5fd;
    border-color: rgba(59, 130, 246, 0.3);
  }
  
  /* Hidden state — pill collapses */
  .container.hidden {
    opacity: 0;
    transform: scale(0.8) translateY(-10px);
    pointer-events: none;
  }
  
  /* Minimal state — only dot visible */
  .container.minimal .label,
  .container.minimal .time,
  .container.minimal .profiles {
    display: none;
  }
  .container.minimal {
    padding: 8px;
  }
  
  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.6; transform: scale(0.85); }
  }
  
  @keyframes spin-dot {
    0% { box-shadow: 0 0 0 0 rgba(59,130,246,0.4); }
    100% { box-shadow: 0 0 0 8px rgba(59,130,246,0); }
  }
</style>
</head>
<body>
<div class="container" id="pill">
  <div class="dot" id="dot"></div>
  <span class="label" id="label">Ready</span>
  <div class="profiles" id="profiles"></div>
  <span class="time" id="time"></span>
</div>

<script>
  // ═══ State Management ═══
  let state = { mode: 'idle', profile: '', time: 0, profiles: [] };
  let timer = null;
  
  function setState(newState) {
    state = { ...state, ...newState };
    render();
  }
  
  function render() {
    const pill = document.getElementById('pill');
    const dot = document.getElementById('dot');
    const label = document.getElementById('label');
    const time = document.getElementById('time');
    
    // Reset classes
    pill.className = 'container';
    dot.className = 'dot';
    
    switch(state.mode) {
      case 'idle':
        pill.classList.add('minimal');
        break;
      case 'recording':
        pill.classList.add('recording');
        dot.classList.add('recording');
        label.textContent = state.profile || 'Recording';
        time.textContent = formatTime(state.time);
        break;
      case 'processing':
        pill.classList.add('processing');
        dot.classList.add('processing');
        label.textContent = 'Transcribing...';
        time.textContent = '';
        break;
      case 'hidden':
        pill.classList.add('hidden');
        break;
    }
    
    renderProfiles();
  }
  
  function renderProfiles() {
    const el = document.getElementById('profiles');
    if (state.mode === 'idle' || state.profiles.length <= 1) {
      el.innerHTML = '';
      return;
    }
    el.innerHTML = state.profiles.map(p =>
      '<button class="chip' + (p.active ? ' active' : '') + 
      '" onclick="selectProfile(\'' + p.name + '\')">' + p.name + '</button>'
    ).join('');
  }
  
  function formatTime(secs) {
    if (secs < 0.1) return '';
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return m > 0 ? m + ':' + String(s).padStart(2,'0') : s + 's';
  }
  
  function selectProfile(name) {
    // Send to Rust via IPC
    window.ipc.postMessage('profile:' + name);
  }
  
  // ═══ Timer for recording duration ═══
  function startTimer() {
    state.time = 0;
    timer = setInterval(() => {
      state.time += 0.1;
      document.getElementById('time').textContent = formatTime(state.time);
    }, 100);
  }
  
  function stopTimer() {
    if (timer) { clearInterval(timer); timer = null; }
  }
  
  // ═══ API called from Rust ═══
  window.gtype = {
    startRecording: (profileName) => {
      setState({ mode: 'recording', profile: profileName });
      startTimer();
    },
    stopRecording: () => {
      stopTimer();
      setState({ mode: 'processing' });
    },
    done: () => {
      setState({ mode: 'idle', time: 0 });
    },
    hide: () => {
      setState({ mode: 'hidden' });
    },
    setProfiles: (profiles) => {
      setState({ profiles: profiles });
    },
    error: (msg) => {
      document.getElementById('label').textContent = '❌ ' + msg;
      setTimeout(() => setState({ mode: 'idle' }), 3000);
    }
  };
  
  // Start in idle/minimal mode
  render();
</script>
</body>
</html>"##;

/// Position of the overlay pill on screen
#[derive(Debug, Clone, Copy)]
pub enum PillPosition {
    TopCenter,
    TopRight,
    BottomCenter,
}

pub struct OverlayManager {
    // wry webview handle for calling JS
    eval_fn: Box<dyn Fn(&str) + Send>,
}

impl OverlayManager {
    /// Create the overlay window.
    /// IMPORTANT: On macOS, this MUST be called from the main thread.
    pub fn new(
        event_loop: &winit::event_loop::EventLoop<()>,
        position: PillPosition,
    ) -> Result<Self> {
        use winit::dpi::{LogicalSize, LogicalPosition};

        // Get screen size for positioning
        let monitor = event_loop.primary_monitor()
            .or_else(|| event_loop.available_monitors().next())
            .context("No monitor found")?;
        let screen = monitor.size();
        let scale = monitor.scale_factor();

        let pill_width = 320.0;
        let pill_height = 56.0;

        let (x, y) = match position {
            PillPosition::TopCenter => (
                (screen.width as f64 / scale - pill_width) / 2.0,
                20.0,
            ),
            PillPosition::TopRight => (
                screen.width as f64 / scale - pill_width - 20.0,
                20.0,
            ),
            PillPosition::BottomCenter => (
                (screen.width as f64 / scale - pill_width) / 2.0,
                screen.height as f64 / scale - pill_height - 80.0,
            ),
        };

        let window = event_loop.create_window(
            winit::window::WindowAttributes::default()
                .with_title("G-Type Overlay")
                .with_inner_size(LogicalSize::new(pill_width, pill_height))
                .with_position(LogicalPosition::new(x, y))
                .with_decorations(false)
                .with_transparent(true)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                .with_resizable(false)
                // macOS: hide from dock and cmd+tab
                // Linux: set window type to utility/notification
        )?;

        let webview = wry::WebViewBuilder::new()
            .with_html(PILL_HTML)
            .with_transparent(true)
            .with_ipc_handler(|msg| {
                // Handle messages from JS
                if let Some(profile) = msg.body().strip_prefix("profile:") {
                    tracing::info!(profile, "Profile selected from overlay");
                    // TODO: send to app.rs via channel
                }
            })
            .build(&window)?;

        // Create eval closure that can call JS from any thread
        let proxy = webview.clone(); // wry supports Clone for eval
        let eval_fn = Box::new(move |js: &str| {
            let _ = proxy.evaluate_script(js);
        });

        Ok(Self { eval_fn })
    }

    /// Show recording state
    pub fn start_recording(&self, profile_name: &str) {
        let js = format!("window.gtype.startRecording('{}')", 
            profile_name.replace('\'', "\\'"));
        (self.eval_fn)(&js);
    }

    /// Show processing state
    pub fn stop_recording(&self) {
        (self.eval_fn)("window.gtype.stopRecording()");
    }

    /// Return to idle
    pub fn done(&self) {
        (self.eval_fn)("window.gtype.done()");
    }

    /// Update profile chips
    pub fn set_profiles(&self, profiles: &[(String, bool)]) {
        let json: Vec<String> = profiles.iter().map(|(name, active)| {
            format!("{{name:'{}',active:{}}}", 
                name.replace('\'', "\\'"), active)
        }).collect();
        let js = format!("window.gtype.setProfiles([{}])", json.join(","));
        (self.eval_fn)(&js);
    }

    /// Show error briefly
    pub fn show_error(&self, msg: &str) {
        let js = format!("window.gtype.error('{}')", 
            msg.replace('\'', "\\'"));
        (self.eval_fn)(&js);
    }
}
```

### Integrazione in app.rs

```rust
// app.rs — Integrazione overlay nel loop principale

pub async fn run(config: ConfigV2) -> Result<()> {
    // ... setup hotkeys, channels, etc ...

    // Tray state channel
    let (tray_tx, tray_rx) = tokio::sync::watch::channel(tray::DaemonState::Idle);

    // Overlay reference (created on main thread)
    // Note: su macOS il webview DEVE girare sul main thread
    // Soluzione: crea l'overlay prima di entrare nel loop async
    let overlay = if config.global.tray_enabled {
        // TODO: overlay creation depends on platform
        // For now, Option<OverlayManager>
        None // placeholder
    } else {
        None
    };

    // Nel loop, quando inizia recording:
    // if let Some(ref ov) = overlay {
    //     ov.start_recording(&profile.name);
    // }
    // tray_tx.send(DaemonState::Recording { 
    //     profile: profile.name.clone(), duration_secs: 0.0 
    // })?;

    // Quando transcription completa:
    // if let Some(ref ov) = overlay { ov.done(); }
    // tray_tx.send(DaemonState::Idle)?;
}
```

### Gestione Dinamica delle Bubble per Profili

La logica è:
- **1 profilo** → mostra solo il pill con dot + "Ready"/"Recording"
- **2+ profili** → mostra il pill con i chip dei profili a fianco. Il chip attivo è evidenziato.
- **Aggiunta profilo** → dal settings web, si aggiorna la config, il daemon rileva il cambio, chiama `overlay.set_profiles()` che aggiorna i chip via JS
- **Rimozione profilo** → stessa logica inversa

Il pill si auto-nasconde dopo 3 secondi di idle (diventa "minimal" → solo dot verde visibile). Al recording si espande. Al processing mostra spinner. Al completamento, si ri-minimizza.

### Platform-Specific Notes

**macOS**: il webview deve girare sul main thread (requisito NSView). Il `tokio::main` non è il main thread di macOS. Soluzione: usa `objc` per fare `[NSApp run]` sul main thread e tokio su un thread separato. Oppure usa il pattern Tauri dove l'event loop è sul main thread.

**Linux X11**: `wry` usa WebKitGTK. Richiede `libwebkit2gtk-4.1-dev` installato. L'utente deve installarlo: `sudo apt install libwebkit2gtk-4.1-dev`.

**Linux Wayland**: WebKitGTK funziona su Wayland nativamente. La finestra `winit` con `with_decorations(false)` funziona ma potrebbe non avere always-on-top su tutti i compositor. Fallback: usa `layer-shell` protocol via `gtk-layer-shell` per vero overlay.

**Windows**: `wry` usa WebView2 (basato su Chromium Edge, preinstallato su Windows 10+). Funziona out of the box.

### Alternativa Leggera: No WebView

Se vuoi evitare la dipendenza WebView (che su Linux richiede WebKitGTK da installare), puoi fare il pill con `winit` + `softbuffer` + `tiny-skia`:

```rust
// overlay_native.rs — Rendering pill senza webview
// Usa tiny-skia per disegnare il pill in un buffer RGBA
// Poi softbuffer per blittare il buffer nella finestra winit

use tiny_skia::{Pixmap, Paint, PathBuilder, Transform, FillRule, Color};

fn render_pill(
    width: u32, height: u32,
    state: &PillState,
) -> Pixmap {
    let mut pixmap = Pixmap::new(width, height).unwrap();

    // Background rounded rect
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(15, 15, 15, 235));
    
    let path = {
        let mut pb = PathBuilder::new();
        let r = height as f32 / 2.0; // fully rounded
        pb.move_to(r, 0.0);
        pb.line_to(width as f32 - r, 0.0);
        pb.cubic_to(width as f32, 0.0, width as f32, height as f32, 
                    width as f32 - r, height as f32);
        pb.line_to(r, height as f32);
        pb.cubic_to(0.0, height as f32, 0.0, 0.0, r, 0.0);
        pb.close();
        pb.finish().unwrap()
    };
    
    pixmap.fill_path(&path, &paint, FillRule::Winding, 
                     Transform::identity(), None);

    // Dot
    let dot_color = match state {
        PillState::Idle => Color::from_rgba8(74, 222, 128, 255),
        PillState::Recording { .. } => Color::from_rgba8(239, 68, 68, 255),
        PillState::Processing => Color::from_rgba8(59, 130, 246, 255),
    };
    // ... draw circle for dot ...

    // Text rendering requires a font library (fontdue, ab_glyph, etc.)
    // This adds complexity but avoids the WebView dependency entirely

    pixmap
}
```

**Trade-off**: 
- WebView (`wry`): più facile da stilare (CSS), richiede WebKitGTK su Linux, ~2MB overhead
- Native (`tiny-skia`): zero dipendenze esterne, ma il rendering testo richiede font loading manuale

**Raccomandazione**: Inizia con `wry`, migra a nativo se la dipendenza WebKitGTK è un problema per gli utenti.

---

## Checklist

- [ ] Aggiungere dipendenze: `tray-icon`, `muda`, `wry`, `winit`
- [ ] Creare `src/tray.rs` con TrayManager
- [ ] Creare `assets/icon_idle_32x32.rgba` (icona tray)
- [ ] Creare `src/overlay.rs` con OverlayManager
- [ ] Embedded HTML/CSS/JS per il pill
- [ ] IPC bidirezionale: Rust→JS (eval_script) e JS→Rust (ipc_handler)
- [ ] Watch channel `DaemonState` per sincronizzare tray + overlay
- [ ] Integrazione nel loop `app.rs`: update overlay su start/stop/done
- [ ] Set profiles dinamici: quando config cambia, aggiorna chip
- [ ] Platform testing: macOS main thread, Linux WebKitGTK, Windows WebView2
- [ ] Auto-hide pill dopo idle timeout
- [ ] Drag del pill (CSS `-webkit-app-region: drag`)

# 05 — GUI INTERFACE: Web Settings Dashboard

## Architettura

La GUI è un server HTTP locale embedded nel binario. L'utente clicca "Settings" dal tray icon, si apre il browser su `http://127.0.0.1:{port}`. Zero dipendenze GUI — il browser è già installato su ogni OS.

### Perché Browser e Non GUI Nativa

1. **CSS > qualsiasi widget toolkit Rust** — puoi fare un'interfaccia che sembra una SaaS premium
2. **Zero compilazione extra** — no WebKitGTK da installare, no framework GUI
3. **Manutenibilità** — HTML/JS embedded si modifica senza ricompilare (in dev mode)
4. **Dimensione binario** — l'HTML/CSS/JS aggiunge ~50KB embedded, vs ~10MB per iced/egui

### Stack Tecnico

- **Server**: `axum` (lightweight async HTTP)
- **Frontend**: HTML + Tailwind CSS (CDN) + vanilla JS
- **Embedding**: `include_str!()` per files HTML/CSS/JS
- **API**: REST JSON endpoints per lettura/scrittura config
- **Auth**: nessuna — il server ascolta SOLO su 127.0.0.1

```toml
# Cargo.toml — nuove dipendenze
axum = "0.8"
tower-http = { version = "0.6", features = ["cors"] }
portpicker = "0.1"    # trova porta libera
open = "5"             # apre URL nel browser default
```

### File Structure

```
src/
└── settings/
    ├── mod.rs           # Server setup, routes
    ├── api.rs           # REST API handlers
    └── assets.rs        # include_str! per tutti gli asset

assets/
├── settings.html        # Single-page app
├── settings.css         # Styles (o Tailwind inline)
└── settings.js          # App logic vanilla JS
```

### Server Setup

```rust
// settings/mod.rs

use axum::{Router, routing::{get, post, put, delete}, Json};
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

mod api;

pub struct SettingsState {
    pub config: Arc<RwLock<crate::config::ConfigV2>>,
    pub config_path: std::path::PathBuf,
    /// Callback per notificare il daemon che la config è cambiata
    pub on_config_changed: tokio::sync::mpsc::Sender<()>,
}

/// Avvia il server settings e ritorna la porta
pub async fn start_server(state: SettingsState) -> Result<u16> {
    let port = portpicker::pick_unused_port().unwrap_or(9741);
    let state = Arc::new(state);

    let app = Router::new()
        // Pages
        .route("/", get(serve_html))
        .route("/settings.css", get(serve_css))
        .route("/settings.js", get(serve_js))
        
        // API endpoints
        .route("/api/config", get(api::get_config))
        .route("/api/config/global", put(api::update_global))
        .route("/api/config/keys", put(api::update_keys))
        .route("/api/profiles", get(api::list_profiles).post(api::create_profile))
        .route("/api/profiles/:name", 
            get(api::get_profile)
            .put(api::update_profile)
            .delete(api::delete_profile))
        .route("/api/stats", get(api::get_stats))
        .route("/api/stats/history", get(api::get_history))
        .route("/api/providers/verify", post(api::verify_provider_key))
        .route("/api/audio/devices", get(api::list_audio_devices))
        .route("/api/local/hardware", get(api::get_hardware_info))
        .route("/api/local/models", get(api::list_local_models))
        .route("/api/local/download", post(api::download_model))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(
        format!("127.0.0.1:{}", port)
    ).await?;

    tracing::info!(port, "Settings server started");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Ok(port)
}

// Asset serving
async fn serve_html() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../assets/settings.html"))
}
async fn serve_css() -> ([(axum::http::header::HeaderName, &'static str); 1], &'static str) {
    ([(axum::http::header::CONTENT_TYPE, "text/css")], 
     include_str!("../../assets/settings.css"))
}
async fn serve_js() -> ([(axum::http::header::HeaderName, &'static str); 1], &'static str) {
    ([(axum::http::header::CONTENT_TYPE, "application/javascript")], 
     include_str!("../../assets/settings.js"))
}
```

### API Handlers

```rust
// settings/api.rs

use axum::{extract::{State, Path, Query}, Json};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

type AppState = Arc<super::SettingsState>;

// ═══ Config ═══

pub async fn get_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    Json(serde_json::to_value(&*config).unwrap())
}

pub async fn update_global(
    State(state): State<AppState>,
    Json(global): Json<crate::config::GlobalConfig>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let mut config = state.config.write().await;
    config.global = global;
    save_and_notify(&config, &state).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn update_keys(
    State(state): State<AppState>,
    Json(keys): Json<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let mut config = state.config.write().await;
    config.keys = keys;
    save_and_notify(&config, &state).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

// ═══ Profiles ═══

pub async fn list_profiles(State(state): State<AppState>) -> Json<Vec<crate::config::Profile>> {
    let config = state.config.read().await;
    Json(config.profiles.clone())
}

pub async fn create_profile(
    State(state): State<AppState>,
    Json(profile): Json<crate::config::Profile>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let mut config = state.config.write().await;
    
    // Check duplicate name
    if config.profiles.iter().any(|p| p.name == profile.name) {
        return Err(axum::http::StatusCode::CONFLICT);
    }
    
    config.profiles.push(profile);
    save_and_notify(&config, &state).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn update_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(profile): Json<crate::config::Profile>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let mut config = state.config.write().await;
    if let Some(existing) = config.profiles.iter_mut().find(|p| p.name == name) {
        *existing = profile;
        save_and_notify(&config, &state).await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(serde_json::json!({"ok": true})))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

pub async fn delete_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let mut config = state.config.write().await;
    let before = config.profiles.len();
    config.profiles.retain(|p| p.name != name);
    if config.profiles.len() < before {
        save_and_notify(&config, &state).await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(serde_json::json!({"ok": true})))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

pub async fn get_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<crate::config::Profile>, axum::http::StatusCode> {
    let config = state.config.read().await;
    config.profiles.iter().find(|p| p.name == name).cloned()
        .map(Json)
        .ok_or(axum::http::StatusCode::NOT_FOUND)
}

// ═══ Stats ═══

pub async fn get_stats() -> Json<serde_json::Value> {
    let records = crate::tracking::load_records().unwrap_or_default();
    let today = crate::tracking::today_prefix();
    let today_records = crate::tracking::filter_records_by_date(&records, &today);
    let week_records = crate::tracking::filter_records_this_week(&records);

    Json(serde_json::json!({
        "today": crate::tracking::Stats::from_records(&today_records),
        "week": crate::tracking::Stats::from_records(&week_records),
        "total": crate::tracking::Stats::from_records(&records),
        "record_count": records.len(),
    }))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    search: String,
    #[serde(default)]
    profile: String,
}
fn default_limit() -> usize { 50 }

pub async fn get_history(Query(q): Query<HistoryQuery>) -> Json<serde_json::Value> {
    let records = crate::tracking::load_records().unwrap_or_default();
    let filtered: Vec<_> = records.into_iter().rev()
        .filter(|r| {
            if !q.search.is_empty() {
                // Richiede campo `text` nel TranscriptionRecord
                // r.text.to_lowercase().contains(&q.search.to_lowercase())
                true // placeholder fino a quando text non è nel record
            } else { true }
        })
        .take(q.limit)
        .collect();
    Json(serde_json::to_value(&filtered).unwrap())
}

// ═══ Audio ═══

pub async fn list_audio_devices() -> Json<serde_json::Value> {
    match crate::audio::list_input_devices() {
        Ok(devices) => Json(serde_json::to_value(&devices).unwrap()),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}

// ═══ Provider Verification ═══

#[derive(Deserialize)]
pub struct VerifyRequest {
    provider: String,
    key: String,
}

pub async fn verify_provider_key(
    Json(req): Json<VerifyRequest>,
) -> Json<serde_json::Value> {
    // Esegui su blocking thread (usa reqwest::blocking)
    let result = tokio::task::spawn_blocking(move || {
        match req.provider.as_str() {
            "gemini" => crate::config::verify_gemini_key(&req.key),
            "openai" => crate::config::verify_openai_key(&req.key),
            "deepgram" => crate::config::verify_deepgram_key(&req.key),
            _ => anyhow::bail!("Unknown provider"),
        }
    }).await;

    match result {
        Ok(Ok(())) => Json(serde_json::json!({"valid": true})),
        Ok(Err(e)) => Json(serde_json::json!({"valid": false, "error": e.to_string()})),
        Err(e) => Json(serde_json::json!({"valid": false, "error": e.to_string()})),
    }
}

// ═══ Local Models ═══

pub async fn get_hardware_info() -> Json<serde_json::Value> {
    let info = crate::providers::local::detect_hardware();
    Json(serde_json::json!({
        "ram_gb": info.ram_gb,
        "is_apple_silicon": info.is_apple_silicon,
        "cpu_cores": info.cpu_cores,
        "recommended_model": info.recommended_model,
        "recommended_size_mb": info.recommended_size_mb,
    }))
}

// ═══ Utility ═══

async fn save_and_notify(
    config: &crate::config::ConfigV2,
    state: &super::SettingsState,
) -> anyhow::Result<()> {
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&state.config_path, content)?;
    let _ = state.on_config_changed.send(()).await;
    Ok(())
}
```

### Frontend HTML (scheletro)

Il file `assets/settings.html` è una single-page app. Qui il design system:

- **Font**: Inter (Google Fonts CDN)
- **Colors**: dark theme con accenti blu (#3b82f6)  
- **Layout**: sidebar left + content right
- **Framework CSS**: Tailwind via CDN (zero build step)

Le sezioni della dashboard:

1. **Profiles** — lista card, crea/edita/elimina, hotkey capture
2. **API Keys** — input per ogni provider con bottone "Verify"
3. **Stats** — numeri big + grafici (Chart.js via CDN)
4. **History** — tabella con search, filtro per profilo/data
5. **Local Models** — hardware info, modelli scaricati, download con progress
6. **Audio** — lista device, selezione device preferito
7. **About** — versione, check update, links

L'HTML completo è ~400 righe. Il JS è ~300 righe. Tutto vanilla, zero framework.

### Integrazione in app.rs

```rust
// app.rs — Avvio settings server nel daemon

pub async fn run(config: ConfigV2) -> Result<()> {
    let config_arc = Arc::new(RwLock::new(config.clone()));
    let (config_changed_tx, mut config_changed_rx) = tokio::sync::mpsc::channel(1);

    // Start settings server
    if config.global.tray_enabled {
        let settings_state = settings::SettingsState {
            config: config_arc.clone(),
            config_path: crate::config::config_path()?,
            on_config_changed: config_changed_tx,
        };
        let port = settings::start_server(settings_state).await?;
        info!(port, "Settings dashboard at http://127.0.0.1:{}", port);
        
        // Il tray menu ha "Settings" che apre il browser:
        // open::that(format!("http://127.0.0.1:{}", port))?;
    }

    // Config change listener — aggiorna hotkeys quando la config cambia
    let shared_hotkeys_clone = shared_hotkeys.clone();
    let config_arc_clone = config_arc.clone();
    tokio::spawn(async move {
        while config_changed_rx.recv().await.is_some() {
            let config = config_arc_clone.read().await;
            let new_profiles: Vec<_> = config.profiles.iter()
                .filter_map(|p| {
                    crate::input::parse_hotkey(&p.hotkey).ok()
                        .map(|hk| (hk, p.name.clone()))
                })
                .collect();
            shared_hotkeys_clone.update(new_profiles);
            info!("Config reloaded, hotkeys updated");
        }
    });

    // ... rest of daemon loop ...
}
```

---

## Checklist

- [ ] Aggiungere dipendenze: `axum`, `tower-http`, `portpicker`, `open`
- [ ] Creare `src/settings/mod.rs` — server setup + asset serving
- [ ] Creare `src/settings/api.rs` — tutti gli endpoint REST
- [ ] Creare `assets/settings.html` — dashboard single-page
- [ ] Creare `assets/settings.js` — logica frontend vanilla
- [ ] API: GET/PUT config, CRUD profiles, GET stats/history
- [ ] API: POST verify provider key (blocking)
- [ ] API: GET audio devices, GET hardware info
- [ ] Integrazione: avvio server in app.rs, config change listener
- [ ] Tray menu: "Settings" apre `open::that(url)`
- [ ] Config change notification: mpsc channel → update SharedHotkeys
- [ ] Test: ogni endpoint API con curl
- [ ] Security: server solo su 127.0.0.1 (nessun accesso remoto)

// settings.rs — Local Axum web server for the G-Type settings dashboard.
// Routes:
//   GET  /          → settings dashboard (redirects to /setup if no key)
//   GET  /setup     → initial setup wizard
//   POST /setup     → submit API key + first profile
//   GET  /api/state → current config as JSON
//   GET  /api/history → last 100 transcriptions (newest first)
//   POST /api/open_config → open config.toml in the system editor
//   POST /api/profiles    → create a new profile
//   DELETE /api/profiles/:name → delete a profile by name

use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect},
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::ConfigV2;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<ConfigV2>>,
}

pub async fn start_server_with_listener(
    listener: tokio::net::TcpListener,
    config: Arc<RwLock<ConfigV2>>,
) -> Result<()> {
    let state = AppState { config };

    let app = Router::new()
        .route("/",                      get(serve_index))
        .route("/setup",                 get(serve_setup))
        .route("/setup",                 post(api_setup))
        .route("/api/state",             get(api_state))
        .route("/api/history",           get(api_history))
        .route("/api/open_config",       post(api_open_config))
        .route("/api/profiles",          post(api_create_profile))
        .route("/api/profiles/{name}",    delete(api_delete_profile))
        .with_state(state);

    tracing::info!("Settings server: http://{}", listener.local_addr()?);

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Axum server error: {}", e))
}

// ── HTML pages ───────────────────────────────────────────────

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    if config.keys.is_empty() {
        return Redirect::to("/setup").into_response();
    }
    Html(include_str!("settings_ui.html")).into_response()
}

async fn serve_setup() -> impl IntoResponse {
    Html(include_str!("setup_ui.html")).into_response()
}

// ── POST /setup ──────────────────────────────────────────────

#[derive(Deserialize)]
struct SetupPayload {
    api_key: String,
    model: String,
    hotkey: String,
}

async fn api_setup(
    State(state): State<AppState>,
    Json(payload): Json<SetupPayload>,
) -> impl IntoResponse {
    let mut config = state.config.write().await;

    config.keys.insert("gemini".to_string(), payload.api_key);
    if let Some(profile) = config.profiles.get_mut(0) {
        profile.model  = payload.model;
        profile.hotkey = payload.hotkey;
    } else {
        let mut p = crate::config::Profile::default();
        p.model  = payload.model;
        p.hotkey = payload.hotkey;
        config.profiles.push(p);
    }

    save_config(&config)
}

// ── GET /api/state ───────────────────────────────────────────

async fn api_state(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    let records = crate::tracking::load_records().unwrap_or_default();
    let stats   = crate::tracking::Stats::from_records(&records);

    (StatusCode::OK, Json(json!({
        "config": *config,
        "stats": {
            "total_words":     stats.total_words,
            "total_cost_usd":  stats.total_cost_usd,
            "time_saved_secs": stats.time_saved_secs,
            "count":           stats.count,
        }
    })))
}

// ── GET /api/history ─────────────────────────────────────────

async fn api_history() -> impl IntoResponse {
    match crate::tracking::load_recent_records(100) {
        Ok(records) => (StatusCode::OK, Json(json!(records))).into_response(),
        Err(e) => {
            tracing::error!("Failed to load history: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── POST /api/open_config ────────────────────────────────────

async fn api_open_config() -> impl IntoResponse {
    match crate::config::config_path() {
        Ok(path) => {
            if open::that(&path).is_ok() { StatusCode::OK } else { StatusCode::INTERNAL_SERVER_ERROR }
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── POST /api/profiles ───────────────────────────────────────

#[derive(Deserialize)]
struct CreateProfilePayload {
    name:     String,
    hotkey:   String,
    model:    String,
    provider: Option<String>,
}

async fn api_create_profile(
    State(state): State<AppState>,
    Json(payload): Json<CreateProfilePayload>,
) -> impl IntoResponse {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Profile name cannot be empty"}))).into_response();
    }
    let hotkey = payload.hotkey.trim().to_string();
    if hotkey.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Hotkey cannot be empty"}))).into_response();
    }

    let mut config = state.config.write().await;

    // Reject duplicate names
    if config.profiles.iter().any(|p| p.name == name) {
        return (StatusCode::CONFLICT, Json(json!({"error": "A profile with this name already exists"}))).into_response();
    }
    // Reject duplicate hotkeys
    if config.profiles.iter().any(|p| p.hotkey == hotkey) {
        return (StatusCode::CONFLICT, Json(json!({"error": "This hotkey is already used by another profile"}))).into_response();
    }

    let profile = crate::config::Profile {
        name:         name.clone(),
        hotkey:       hotkey,
        provider:     payload.provider.unwrap_or_else(|| "gemini".to_string()),
        model:        payload.model,
        timeout_secs: 10,
        transforms:   vec![],
    };
    config.profiles.push(profile);

    match save_config(&config) {
        StatusCode::OK => (StatusCode::OK, Json(json!({"ok": true, "name": name}))).into_response(),
        code           => code.into_response(),
    }
}

// ── DELETE /api/profiles/:name ───────────────────────────────

async fn api_delete_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut config = state.config.write().await;

    if config.profiles.len() <= 1 {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Cannot delete the last profile"}))).into_response();
    }

    let before = config.profiles.len();
    config.profiles.retain(|p| p.name != name);

    if config.profiles.len() == before {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "Profile not found"}))).into_response();
    }

    match save_config(&config) {
        StatusCode::OK => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        code           => code.into_response(),
    }
}

// ── Helpers ──────────────────────────────────────────────────

fn save_config(config: &ConfigV2) -> StatusCode {
    match crate::config::config_path() {
        Ok(path) => match crate::config::save(config, &path) {
            Ok(()) => StatusCode::OK,
            Err(e) => {
                tracing::error!("Failed to save config: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        },
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

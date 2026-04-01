use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::ConfigV2;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<ConfigV2>>,
}

// Data payload for setup
#[derive(Deserialize)]
struct SetupPayload {
    api_key: String,
    model: String,
    hotkey: String,
}

pub async fn start_server(config: Arc<RwLock<ConfigV2>>) -> Result<()> {
    let state = AppState { config };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/setup", get(serve_setup))
        .route("/setup", post(api_setup))
        .route("/api/state", get(api_state))
        .route("/api/open_config", post(api_open_config))
        .with_state(state);

    let port = 9741;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    
    tracing::info!("UI Settings Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Axum server error: {}", e))
}

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    if config.keys.is_empty() {
        return Redirect::to("/setup").into_response();
    }
    Html(include_str!("settings_ui.html")).into_response()
}

async fn serve_setup(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    if !config.keys.is_empty() {
        return Redirect::to("/").into_response();
    }
    Html(include_str!("setup_ui.html")).into_response()
}

async fn api_setup(
    State(state): State<AppState>,
    Json(payload): Json<SetupPayload>,
) -> impl IntoResponse {
    let mut config = state.config.write().await;
    
    config.keys.insert("gemini".to_string(), payload.api_key.clone());
    if let Some(profile) = config.profiles.get_mut(0) {
        profile.model = payload.model.clone();
        profile.hotkey = payload.hotkey.clone();
    } else {
        // Fallback default profile
        let mut p = crate::config::Profile::default();
        p.model = payload.model;
        p.hotkey = payload.hotkey;
        config.profiles.push(p);
    }
    
    if let Ok(path) = crate::config::config_path() {
        if let Err(e) = crate::config::save(&config, &path) {
            tracing::error!("Failed to save config during setup: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
        tracing::info!("Config saved with new setup settings.");
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn api_state(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    
    // In futuro prenderemo anche tracking stats. Per ora passiamo config grezzo
    let payload = json!({
        "config": *config,
    });
    
    (StatusCode::OK, Json(payload))
}

async fn api_open_config() -> impl IntoResponse {
    match crate::config::config_path() {
        Ok(path) => {
            if open::that(&path).is_ok() {
                StatusCode::OK
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

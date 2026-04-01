use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::ConfigV2;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<ConfigV2>>,
}

pub async fn start_server(config: Arc<RwLock<ConfigV2>>) -> Result<()> {
    let state = AppState { config };

    let app = Router::new()
        .route("/", get(serve_index))
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

async fn serve_index() -> Html<&'static str> {
    Html(include_str!("settings_ui.html"))
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

// settings.rs — Local Axum web server for the G-Type dashboard.

use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect},
    routing::{delete, get, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::ConfigV2;
use crate::providers::model_catalog;

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
        .route("/", get(serve_index))
        .route("/recovery", get(serve_recovery))
        .route("/setup", get(serve_setup))
        .route("/setup", post(api_setup))
        .route("/api/state", get(api_state))
        .route("/api/models", get(api_models))
        .route("/api/history", get(api_history))
        .route("/api/statistics", get(api_statistics))
        .route("/api/recovery", get(api_recovery))
        .route("/api/recovery/{id}/retry", post(api_retry_recovery))
        .route("/api/recovery/{id}/open", post(api_open_recovery))
        .route("/api/open_config", post(api_open_config))
        .route("/api/keys/gemini", put(api_update_gemini_key))
        .route("/api/profiles", post(api_create_profile))
        .route("/api/profiles/{name}", delete(api_delete_profile))
        .route("/api/profiles/{name}", put(api_update_profile))
        .with_state(state);

    tracing::info!("Settings server: http://{}", listener.local_addr()?);

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Axum server error: {}", e))
}

fn dashboard_model_options() -> Vec<Value> {
    model_catalog::selectable_models()
        .map(|spec| {
            json!([
                spec.normalized_id(),
                format!(
                    "{} · audio ${:.2}/M · output ${:.2}/M{}",
                    spec.label,
                    spec.input_audio_per_m,
                    spec.output_per_m,
                    if spec.status == "preview" { " · preview" } else { "" }
                )
            ])
        })
        .collect()
}

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    if config.keys.is_empty() {
        return Redirect::to("/setup").into_response();
    }
    drop(config);

    let recovery_count = crate::app::recovery::list()
        .map(|items| items.len())
        .unwrap_or(0);
    let mut html = include_str!("settings_ui.html").to_string();

    // Keep the dashboard HTML lightweight while making the Rust model catalog
    // the single source of truth for all profile selectors.
    let old_models = "const MODELS=[['models/gemini-2.0-flash','gemini-2.0-flash'],['models/gemini-2.5-flash','gemini-2.5-flash'],['models/gemini-2.5-flash-lite','gemini-2.5-flash-lite'],['models/gemini-2.5-pro','gemini-2.5-pro']];";
    if let Ok(models_json) = serde_json::to_string(&dashboard_model_options()) {
        html = html.replacen(old_models, &format!("const MODELS={models_json};"), 1);
    }

    if recovery_count > 0 {
        let banner = format!(
            "</nav><a href=\"/recovery\" style=\"display:flex;align-items:center;justify-content:space-between;gap:14px;margin:-10px 0 20px;padding:12px 14px;border:1px solid rgba(245,158,11,.28);background:rgba(245,158,11,.08);border-radius:11px;color:#fcd34d;text-decoration:none;font-size:12px\"><span><strong>{recovery_count} audio da recuperare</strong> · le registrazioni sono al sicuro su disco</span><span>Apri recupero →</span></a>"
        );
        html = html.replacen("</nav>", &banner, 1);
    }
    Html(html).into_response()
}

async fn serve_recovery(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    if config.keys.is_empty() {
        return Redirect::to("/setup").into_response();
    }
    Html(include_str!("recovery_ui.html")).into_response()
}

async fn serve_setup() -> impl IntoResponse {
    Html(include_str!("setup_ui.html")).into_response()
}

async fn api_models() -> impl IntoResponse {
    let models: Vec<_> = model_catalog::selectable_models().copied().collect();
    (
        StatusCode::OK,
        Json(json!({
            "pricing_reviewed_at": model_catalog::PRICING_REVIEWED_AT,
            "recommended": model_catalog::recommended_model(),
            "models": models,
            "live_audio_models": model_catalog::LIVE_AUDIO_MODELS,
        })),
    )
}

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
    if let Err(e) = crate::input::parse_hotkey(payload.hotkey.trim()) {
        return bad_request(format!("Hotkey non valida: {e}"));
    }
    if !model_catalog::is_selectable(&payload.model) {
        return bad_request("Modello Gemini non disponibile per la trascrizione".into());
    }

    let api_key = payload.api_key.trim();
    if api_key.is_empty() {
        return bad_request("La API key non può essere vuota".into());
    }

    let mut config = state.config.write().await;
    config
        .keys
        .insert("gemini".to_string(), api_key.to_string());
    if let Some(profile) = config.profiles.get_mut(0) {
        profile.model = normalized_model(&payload.model);
        profile.hotkey = payload.hotkey.trim().to_string();
    } else {
        let p = crate::config::Profile {
            model: normalized_model(&payload.model),
            hotkey: payload.hotkey.trim().to_string(),
            ..crate::config::Profile::default()
        };
        config.profiles.push(p);
    }

    save_config(&config).into_response()
}

async fn api_state(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    let records = crate::tracking::load_records().unwrap_or_default();
    let stats = crate::tracking::Stats::from_records(&records);
    let recovery_count = crate::app::recovery::list()
        .map(|items| items.len())
        .unwrap_or(0);

    let mut public_config = config.clone();
    public_config.keys.clear();
    let gemini_key = config.keys.get("gemini").map(String::as_str).unwrap_or("");

    (
        StatusCode::OK,
        Json(json!({
            "config": public_config,
            "providers": {
                "gemini": {
                    "configured": !gemini_key.is_empty(),
                    "masked_key": mask_secret(gemini_key)
                }
            },
            "stats": {
                "total_words": stats.total_words,
                "total_cost_usd": stats.total_cost_usd,
                "time_saved_secs": stats.time_saved_secs,
                "count": stats.count,
            },
            "recovery_count": recovery_count,
            "runtime": {
                "live_profile_reload": true,
                "recovery_spool": true,
                "model_fallback": true,
                "pricing_reviewed_at": model_catalog::PRICING_REVIEWED_AT,
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
    )
}

async fn api_history() -> impl IntoResponse {
    match crate::tracking::load_recent_records(100) {
        Ok(records) => (StatusCode::OK, Json(json!(records))).into_response(),
        Err(e) => {
            tracing::error!("Failed to load history: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn api_recovery() -> impl IntoResponse {
    match crate::app::recovery::list() {
        Ok(items) => (StatusCode::OK, Json(json!(items))).into_response(),
        Err(error) => {
            tracing::error!(%error, "Failed to load recovery queue");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Impossibile leggere gli audio da recuperare"})),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct RetryRecoveryPayload {
    model: Option<String>,
}

async fn api_retry_recovery(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<RetryRecoveryPayload>,
) -> impl IntoResponse {
    let (item, samples) = match crate::app::recovery::load(&id) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, id, "Recovery item not found or unreadable");
            return not_found("Audio di recupero non trovato".into());
        }
    };

    let config = state.config.read().await.clone();
    let requested_model = payload
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if model_catalog::is_selectable(&item.model) {
                item.model.clone()
            } else {
                model_catalog::recommended_model().to_string()
            }
        });

    if !model_catalog::is_selectable(&requested_model) {
        return bad_request("Il modello scelto non supporta questa trascrizione".into());
    }

    let mut profile = config
        .profiles
        .iter()
        .find(|profile| profile.name == item.profile)
        .cloned()
        .unwrap_or_else(|| crate::config::Profile {
            name: item.profile.clone(),
            model: normalized_model(&requested_model),
            ..crate::config::Profile::default()
        });
    profile.model = normalized_model(&requested_model);

    let language = if item.language.trim().is_empty() {
        config.global.language.as_str()
    } else {
        item.language.as_str()
    };

    let outcome = match crate::providers::transcribe_exact(
        &profile,
        &config.keys,
        &requested_model,
        &samples,
        language,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            let message = format!(
                "{}: {}",
                model_catalog::normalize_model_id(&requested_model),
                error
            );
            let _ = crate::app::recovery::mark_failure(&id, &message);
            tracing::warn!(%error, model = %requested_model, id, "Recovery retry failed; WAV kept on disk");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": error.to_string(),
                    "model": requested_model,
                    "preserved": true
                })),
            )
                .into_response();
        }
    };

    if outcome.text.trim().is_empty() {
        let message = format!(
            "{}: Gemini ha restituito una trascrizione vuota",
            model_catalog::normalize_model_id(&requested_model)
        );
        let _ = crate::app::recovery::mark_failure(&id, &message);
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": message, "preserved": true})),
        )
            .into_response();
    }

    let final_text = if profile.transforms.is_empty() {
        outcome.text
    } else {
        crate::transforms::run_pipeline(&profile.transforms, &outcome.text, language).await
    };

    if final_text.trim().is_empty() {
        let message = "La pipeline ha prodotto testo vuoto";
        let _ = crate::app::recovery::mark_failure(&id, message);
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": message, "preserved": true})),
        )
            .into_response();
    }

    let record = crate::tracking::build_record(
        &outcome.model_used,
        item.duration_secs,
        &outcome.usage,
        &final_text,
    );
    if let Err(error) = crate::tracking::append_record(&record) {
        let message = format!("Trascrizione riuscita ma salvataggio cronologia fallito: {error}");
        let _ = crate::app::recovery::mark_failure(&id, &message);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": message, "preserved": true})),
        )
            .into_response();
    }

    if let Err(error) = crate::app::recovery::remove(&id) {
        tracing::warn!(%error, id, "Recovered transcription saved but spool cleanup failed");
    }

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "text": final_text,
            "duration_secs": item.duration_secs,
            "model_used": outcome.model_used
        })),
    )
        .into_response()
}

async fn api_open_recovery(Path(id): Path<String>) -> impl IntoResponse {
    match crate::app::recovery::open_audio(&id) {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(error) => {
            tracing::warn!(%error, id, "Failed to open recovery audio");
            not_found("Audio di recupero non trovato".into())
        }
    }
}

async fn api_statistics() -> impl IntoResponse {
    let records = match crate::tracking::load_records() {
        Ok(records) => records,
        Err(e) => {
            tracing::error!("Failed to load statistics: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let stats = crate::tracking::Stats::from_records(&records);
    let mut by_day: BTreeMap<String, (u64, u64, f64, f64)> = BTreeMap::new();
    let mut by_model: BTreeMap<String, (u64, u64, f64, f64)> = BTreeMap::new();

    for r in &records {
        let day = r.timestamp.split('T').next().unwrap_or("unknown").to_string();
        let d = by_day.entry(day).or_insert((0, 0, 0.0, 0.0));
        d.0 += 1;
        d.1 += r.word_count as u64;
        d.2 += r.total_cost_usd;
        d.3 += r.audio_duration_secs;

        let model = r.model.strip_prefix("models/").unwrap_or(&r.model).to_string();
        let m = by_model.entry(model).or_insert((0, 0, 0.0, 0.0));
        m.0 += 1;
        m.1 += r.word_count as u64;
        m.2 += r.total_cost_usd;
        m.3 += r.audio_duration_secs;
    }

    let mut days = by_day.into_iter().rev().take(14).collect::<Vec<_>>();
    days.reverse();

    let daily = days
        .into_iter()
        .map(|(date, (count, words, cost, audio_secs))| {
            json!({
                "date": date,
                "count": count,
                "words": words,
                "cost_usd": cost,
                "audio_secs": audio_secs
            })
        })
        .collect::<Vec<_>>();

    let models = by_model
        .into_iter()
        .map(|(model, (count, words, cost, audio_secs))| {
            json!({
                "model": model,
                "count": count,
                "words": words,
                "cost_usd": cost,
                "audio_secs": audio_secs
            })
        })
        .collect::<Vec<_>>();

    let count = stats.count as f64;
    let avg_words = if count > 0.0 {
        stats.total_words as f64 / count
    } else {
        0.0
    };
    let avg_duration_secs = if count > 0.0 {
        stats.total_audio_secs / count
    } else {
        0.0
    };
    let speaking_wpm = if stats.total_audio_secs > 0.0 {
        stats.total_words as f64 / (stats.total_audio_secs / 60.0)
    } else {
        0.0
    };
    let cost_per_1000_words = if stats.total_words > 0 {
        stats.total_cost_usd / stats.total_words as f64 * 1000.0
    } else {
        0.0
    };

    (
        StatusCode::OK,
        Json(json!({
            "totals": {
                "count": stats.count,
                "words": stats.total_words,
                "chars": stats.total_chars,
                "audio_secs": stats.total_audio_secs,
                "input_tokens": stats.total_input_tokens,
                "output_tokens": stats.total_output_tokens,
                "total_tokens": stats.total_input_tokens + stats.total_output_tokens,
                "cost_usd": stats.total_cost_usd,
                "time_saved_secs": stats.time_saved_secs
            },
            "averages": {
                "words_per_transcription": avg_words,
                "duration_secs": avg_duration_secs,
                "speaking_wpm": speaking_wpm,
                "cost_per_1000_words_usd": cost_per_1000_words
            },
            "daily": daily,
            "models": models
        })),
    )
        .into_response()
}

async fn api_open_config() -> impl IntoResponse {
    match crate::config::config_path() {
        Ok(path) if open::that(&path).is_ok() => StatusCode::OK,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Deserialize)]
struct ApiKeyPayload {
    api_key: String,
}

async fn api_update_gemini_key(
    State(state): State<AppState>,
    Json(payload): Json<ApiKeyPayload>,
) -> impl IntoResponse {
    let api_key = payload.api_key.trim();
    if api_key.is_empty() {
        return bad_request("La API key non può essere vuota".into());
    }

    let mut config = state.config.write().await;
    config
        .keys
        .insert("gemini".to_string(), api_key.to_string());
    match save_config(&config) {
        StatusCode::OK => (
            StatusCode::OK,
            Json(json!({"ok": true, "provider": "gemini", "live": true})),
        )
            .into_response(),
        code => code.into_response(),
    }
}

#[derive(Deserialize)]
struct CreateProfilePayload {
    name: String,
    hotkey: String,
    model: String,
    provider: Option<String>,
    custom_prompt: Option<String>,
}

async fn api_create_profile(
    State(state): State<AppState>,
    Json(payload): Json<CreateProfilePayload>,
) -> impl IntoResponse {
    let name = payload.name.trim().to_string();
    let hotkey = payload.hotkey.trim().to_string();

    if name.is_empty() {
        return bad_request("Il nome del profilo non può essere vuoto".into());
    }
    if hotkey.is_empty() {
        return bad_request("La hotkey non può essere vuota".into());
    }
    if let Err(e) = crate::input::parse_hotkey(&hotkey) {
        return bad_request(format!("Hotkey non valida: {e}"));
    }
    if !model_catalog::is_selectable(&payload.model) {
        return bad_request("Modello Gemini non disponibile per la trascrizione".into());
    }

    let mut config = state.config.write().await;
    if config.profiles.iter().any(|p| p.name == name) {
        return conflict("Esiste già un profilo con questo nome".into());
    }
    if config.profiles.iter().any(|p| p.hotkey == hotkey) {
        return conflict("Questa hotkey è già usata da un altro profilo".into());
    }

    let profile = crate::config::Profile {
        name: name.clone(),
        hotkey,
        provider: payload.provider.unwrap_or_else(|| "gemini".to_string()),
        model: normalized_model(&payload.model),
        timeout_secs: 10,
        transforms: vec![],
        custom_prompt: clean_prompt(payload.custom_prompt),
    };
    config.profiles.push(profile);

    match save_config(&config) {
        StatusCode::OK => (
            StatusCode::OK,
            Json(json!({"ok": true, "name": name, "live": true})),
        )
            .into_response(),
        code => code.into_response(),
    }
}

async fn api_delete_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut config = state.config.write().await;
    if config.profiles.len() <= 1 {
        return bad_request("Non puoi eliminare l'ultimo profilo".into());
    }
    let before = config.profiles.len();
    config.profiles.retain(|p| p.name != name);
    if config.profiles.len() == before {
        return not_found("Profilo non trovato".into());
    }

    match save_config(&config) {
        StatusCode::OK => (
            StatusCode::OK,
            Json(json!({"ok": true, "live": true})),
        )
            .into_response(),
        code => code.into_response(),
    }
}

#[derive(Deserialize)]
struct UpdateProfilePayload {
    name: Option<String>,
    hotkey: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    custom_prompt: Option<String>,
}

async fn api_update_profile(
    State(state): State<AppState>,
    Path(current_name): Path<String>,
    Json(payload): Json<UpdateProfilePayload>,
) -> impl IntoResponse {
    let mut config = state.config.write().await;
    let Some(index) = config.profiles.iter().position(|p| p.name == current_name) else {
        return not_found("Profilo non trovato".into());
    };

    let new_name = payload
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&config.profiles[index].name)
        .to_string();
    let new_hotkey = payload
        .hotkey
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&config.profiles[index].hotkey)
        .to_string();

    if let Err(e) = crate::input::parse_hotkey(&new_hotkey) {
        return bad_request(format!("Hotkey non valida: {e}"));
    }
    if config
        .profiles
        .iter()
        .enumerate()
        .any(|(i, p)| i != index && p.name == new_name)
    {
        return conflict("Esiste già un profilo con questo nome".into());
    }
    if config
        .profiles
        .iter()
        .enumerate()
        .any(|(i, p)| i != index && p.hotkey == new_hotkey)
    {
        return conflict("Questa hotkey è già usata da un altro profilo".into());
    }

    if let Some(ref model) = payload.model {
        if !model_catalog::is_selectable(model) {
            return bad_request("Modello Gemini non disponibile per la trascrizione".into());
        }
    }

    let p = &mut config.profiles[index];
    p.name = new_name.clone();
    p.hotkey = new_hotkey;
    if let Some(model) = payload.model.filter(|s| !s.trim().is_empty()) {
        p.model = normalized_model(&model);
    }
    if let Some(provider) = payload.provider.filter(|s| !s.trim().is_empty()) {
        p.provider = provider;
    }
    p.custom_prompt = clean_prompt(payload.custom_prompt);

    match save_config(&config) {
        StatusCode::OK => (
            StatusCode::OK,
            Json(json!({"ok": true, "name": new_name, "live": true})),
        )
            .into_response(),
        code => code.into_response(),
    }
}

fn normalized_model(model: &str) -> String {
    format!("models/{}", model_catalog::normalize_model_id(model))
}

fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let suffix: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("••••••••{}", suffix)
}

fn clean_prompt(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

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

fn bad_request(message: String) -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Json(json!({"error": message}))).into_response()
}

fn conflict(message: String) -> axum::response::Response {
    (StatusCode::CONFLICT, Json(json!({"error": message}))).into_response()
}

fn not_found(message: String) -> axum::response::Response {
    (StatusCode::NOT_FOUND, Json(json!({"error": message}))).into_response()
}

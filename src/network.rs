// network.rs — WebSocket client for Gemini 2.0 Flash BidiGenerateContent Live API.
// Handles TLS handshake, setup message, audio streaming, and response parsing.

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::audio::AudioChunk;
use crate::config::Config;

/// Connect to Gemini Live API, stream audio chunks, collect transcription.
///
/// - `config`: App config with API key and model.
/// - `audio_rx`: Receives PCM i16 audio chunks from the capture thread.
/// - `stop_rx`: Receives a signal when recording stops (CTRL+T released).
///
/// Returns the accumulated transcription text.
pub async fn transcribe(
    config: &Config,
    mut audio_rx: mpsc::Receiver<AudioChunk>,
    mut stop_rx: mpsc::Receiver<()>,
) -> Result<String> {
    let url = config.ws_url();
    info!("Connecting to Gemini Live API...");

    // Connect with TLS
    let (ws_stream, response) = tokio_tungstenite::connect_async(&url)
        .await
        .context("WebSocket connection to Gemini failed")?;

    debug!(status = ?response.status(), "WebSocket connected");

    let (mut sink, mut stream) = ws_stream.split();

    // Step 1: Send setup message
    let setup_msg = build_setup_message(config);
    sink.send(Message::Text(setup_msg.to_string()))
        .await
        .context("Failed to send setup message")?;

    info!("Setup message sent, waiting for confirmation...");

    // Wait for setup confirmation from server
    match timeout(Duration::from_secs(5), stream.next()).await {
        Ok(Some(Ok(msg))) => {
            debug!(msg = %msg, "Setup response received");
        }
        Ok(Some(Err(e))) => {
            bail!("Error receiving setup response: {}", e);
        }
        Ok(None) => {
            bail!("WebSocket closed before setup confirmation");
        }
        Err(_) => {
            bail!("Timeout waiting for setup confirmation (5s)");
        }
    }

    // Step 2: Stream audio chunks until stop signal
    let mut recording = true;

    while recording {
        tokio::select! {
            // Check for stop signal
            _ = stop_rx.recv() => {
                info!("Stop signal received, finishing audio stream");
                recording = false;
            }
            // Send audio chunks as they arrive
            chunk = audio_rx.recv() => {
                match chunk {
                    Some(pcm_data) => {
                        let msg = encode_audio_chunk(&pcm_data);
                        if let Err(e) = sink.send(Message::Text(msg.to_string())).await {
                            error!(%e, "Failed to send audio chunk");
                            bail!("WebSocket send error: {}", e);
                        }
                    }
                    None => {
                        debug!("Audio channel closed");
                        recording = false;
                    }
                }
            }
        }
    }

    // Drain any remaining chunks in the channel
    while let Ok(chunk) = audio_rx.try_recv() {
        let msg = encode_audio_chunk(&chunk);
        if let Err(e) = sink.send(Message::Text(msg.to_string())).await {
            warn!(%e, "Failed to send trailing audio chunk");
            break;
        }
    }

    // Step 3: Send turnComplete signal
    let turn_complete = json!({
        "clientContent": {
            "turnComplete": true
        }
    });
    sink.send(Message::Text(turn_complete.to_string()))
        .await
        .context("Failed to send turnComplete")?;

    info!("turnComplete sent, awaiting transcription...");

    // Step 4: Accumulate text response until server sends turnComplete
    let mut transcription = String::new();
    let deadline = Duration::from_secs(config.timeout_secs.max(3));

    loop {
        match timeout(deadline, stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                match parse_server_message(&text) {
                    ServerEvent::TextDelta(delta) => {
                        transcription.push_str(&delta);
                        debug!(delta = %delta, "Text fragment received");
                    }
                    ServerEvent::TurnComplete => {
                        info!(len = transcription.len(), "Transcription complete");
                        break;
                    }
                    ServerEvent::SetupComplete => {
                        debug!("Late setup complete message (ignored)");
                    }
                    ServerEvent::Unknown(raw) => {
                        debug!(msg = %raw, "Unknown server message");
                    }
                }
            }
            Ok(Some(Ok(Message::Close(_)))) => {
                warn!("WebSocket closed by server before turnComplete");
                break;
            }
            Ok(Some(Ok(_))) => {
                // Binary or other message types — skip
                continue;
            }
            Ok(Some(Err(e))) => {
                error!(%e, "WebSocket receive error");
                break;
            }
            Ok(None) => {
                warn!("WebSocket stream ended");
                break;
            }
            Err(_) => {
                warn!(timeout_secs = config.timeout_secs, "Timeout waiting for transcription");
                break;
            }
        }
    }

    // Close the WebSocket gracefully
    if let Err(e) = sink.close().await {
        debug!(%e, "WebSocket close error (non-fatal)");
    }

    Ok(transcription.trim().to_string())
}

/// Server events we care about.
enum ServerEvent {
    TextDelta(String),
    TurnComplete,
    SetupComplete,
    Unknown(String),
}

/// Parse a server JSON message into a structured event.
fn parse_server_message(raw: &str) -> ServerEvent {
    let parsed: Result<Value, _> = serde_json::from_str(raw);
    let value = match parsed {
        Ok(v) => v,
        Err(_) => return ServerEvent::Unknown(raw.to_string()),
    };

    // Check for setupComplete
    if value.get("setupComplete").is_some() {
        return ServerEvent::SetupComplete;
    }

    // Check for serverContent
    if let Some(server_content) = value.get("serverContent") {
        // Check turnComplete
        if server_content.get("turnComplete").and_then(|v| v.as_bool()) == Some(true) {
            return ServerEvent::TurnComplete;
        }

        // Extract text from modelTurn.parts[].text
        if let Some(model_turn) = server_content.get("modelTurn") {
            if let Some(parts) = model_turn.get("parts").and_then(|p| p.as_array()) {
                let mut text = String::new();
                for part in parts {
                    if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
                if !text.is_empty() {
                    return ServerEvent::TextDelta(text);
                }
            }
        }
    }

    ServerEvent::Unknown(raw.to_string())
}

/// Build the JSON setup message for Gemini Live API.
fn build_setup_message(config: &Config) -> Value {
    json!({
        "setup": {
            "model": config.model,
            "generationConfig": {
                "responseModalities": ["TEXT"],
                "temperature": 0.0
            },
            "systemInstruction": {
                "parts": [{
                    "text": "Sei un motore di dettatura passivo. Il tuo unico compito è trascrivere esattamente ciò che l'utente dice nell'audio, parola per parola, senza aggiungere punteggiatura inventata, senza commentare e senza rispondere alle domande. Restituisci solo la trascrizione pura."
                }]
            }
        }
    })
}

/// Encode a PCM i16 chunk to base64 and wrap in the realtimeInput JSON envelope.
fn encode_audio_chunk(samples: &[i16]) -> Value {
    // Convert i16 samples to raw bytes (little-endian)
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    let b64 = BASE64.encode(&bytes);

    json!({
        "realtimeInput": {
            "mediaChunks": [{
                "mimeType": "audio/pcm;rate=16000",
                "data": b64
            }]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_setup_message() {
        let config = Config {
            api_key: "test".into(),
            model: "models/gemini-2.0-flash".into(),
            hotkey: "ctrl+shift+space".into(),
            injection_threshold: 80,
            timeout_secs: 3,
        };
        let msg = build_setup_message(&config);
        assert_eq!(msg["setup"]["model"], "models/gemini-2.0-flash");
        assert_eq!(msg["setup"]["generationConfig"]["responseModalities"][0], "TEXT");
    }

    #[test]
    fn test_encode_audio_chunk() {
        let samples: Vec<i16> = vec![0, 100, -100, i16::MAX, i16::MIN];
        let msg = encode_audio_chunk(&samples);
        assert!(msg["realtimeInput"]["mediaChunks"][0]["data"].is_string());
        assert_eq!(
            msg["realtimeInput"]["mediaChunks"][0]["mimeType"],
            "audio/pcm;rate=16000"
        );
    }

    #[test]
    fn test_parse_text_delta() {
        let raw = r#"{"serverContent":{"modelTurn":{"parts":[{"text":"hello world"}]}}}"#;
        match parse_server_message(raw) {
            ServerEvent::TextDelta(t) => assert_eq!(t, "hello world"),
            _ => panic!("Expected TextDelta"),
        }
    }

    #[test]
    fn test_parse_turn_complete() {
        let raw = r#"{"serverContent":{"turnComplete":true}}"#;
        match parse_server_message(raw) {
            ServerEvent::TurnComplete => {}
            _ => panic!("Expected TurnComplete"),
        }
    }

    #[test]
    fn test_parse_setup_complete() {
        let raw = r#"{"setupComplete":{}}"#;
        match parse_server_message(raw) {
            ServerEvent::SetupComplete => {}
            _ => panic!("Expected SetupComplete"),
        }
    }
}

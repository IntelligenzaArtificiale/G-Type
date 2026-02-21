// audio.rs — Microphone capture with cpal, real-time downsampling to 16kHz i16 mono.
// Pushes PCM chunks (100ms = 1600 samples) through a tokio mpsc channel.

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SampleRate, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// A chunk of 16kHz i16 mono PCM data (100ms = 1600 samples).
pub type AudioChunk = Vec<i16>;

/// Sender half for audio chunks.
pub type AudioTx = mpsc::Sender<AudioChunk>;

/// Target sample rate for Gemini API.
const TARGET_RATE: u32 = 16_000;
/// Chunk duration in milliseconds.
const CHUNK_MS: u32 = 100;
/// Samples per chunk at target rate.
const SAMPLES_PER_CHUNK: usize = (TARGET_RATE * CHUNK_MS / 1000) as usize;

/// Get the default input device or bail.
fn default_input_device() -> Result<Device> {
    let host = cpal::default_host();
    host.default_input_device()
        .context("No audio input device found. Is a microphone connected?")
}

/// Pick a supported input config, preferring mono 16kHz.
fn pick_input_config(device: &Device) -> Result<(StreamConfig, SampleFormat)> {
    let supported = device
        .supported_input_configs()
        .context("Failed to query supported audio input configs")?;

    // Collect all supported configs
    let configs: Vec<_> = supported.collect();
    if configs.is_empty() {
        bail!("Audio device supports no input configurations");
    }

    // Try to find one that supports 16kHz natively
    for cfg in &configs {
        if cfg.min_sample_rate().0 <= TARGET_RATE && cfg.max_sample_rate().0 >= TARGET_RATE {
            let config = cfg.with_sample_rate(SampleRate(TARGET_RATE)).config();
            let fmt = cfg.sample_format();
            info!(
                rate = TARGET_RATE,
                channels = config.channels,
                format = ?fmt,
                "Using native 16kHz config"
            );
            return Ok((config, fmt));
        }
    }

    // Fallback: use the default config, we'll resample in software
    let fallback = configs[0].with_max_sample_rate().config();
    let fmt = configs[0].sample_format();
    warn!(
        rate = fallback.sample_rate.0,
        channels = fallback.channels,
        "No native 16kHz support, will resample in software"
    );
    Ok((fallback, fmt))
}

/// Simple linear downsampler: picks nearest sample for rate conversion.
struct Downsampler {
    source_rate: u32,
    source_channels: u16,
    buffer: Vec<i16>,
}

impl Downsampler {
    fn new(source_rate: u32, source_channels: u16) -> Self {
        Self {
            source_rate,
            source_channels,
            buffer: Vec::with_capacity(SAMPLES_PER_CHUNK * 2),
        }
    }

    /// Feed raw i16 samples (possibly multi-channel, possibly different rate).
    /// Returns complete chunks of SAMPLES_PER_CHUNK mono 16kHz samples.
    fn feed(&mut self, samples: &[i16]) -> Vec<AudioChunk> {
        let ratio = self.source_rate as f64 / TARGET_RATE as f64;
        let ch = self.source_channels as usize;

        // Mix to mono and downsample
        let frames = samples.len() / ch;
        for frame_idx in 0..frames {
            let mono: i32 = (0..ch)
                .map(|c| samples[frame_idx * ch + c] as i32)
                .sum::<i32>()
                / ch as i32;

            // Accumulate position tracking via buffer length
            let target_sample_idx = (self.buffer.len() as f64 * ratio) as usize;
            let source_sample_idx = (frame_idx as f64) + (self.buffer.len() as f64 * ratio)
                - (self.buffer.len() as f64 * ratio);
            let _ = (target_sample_idx, source_sample_idx); // suppress warnings

            self.buffer.push(mono as i16);
        }

        // If source rate differs, resample the buffer
        if self.source_rate != TARGET_RATE {
            let resampled = resample_linear(&self.buffer, self.source_rate, TARGET_RATE);
            self.buffer.clear();
            self.buffer = resampled;
        }

        // Extract complete chunks
        let mut chunks = Vec::new();
        while self.buffer.len() >= SAMPLES_PER_CHUNK {
            let chunk: Vec<i16> = self.buffer.drain(..SAMPLES_PER_CHUNK).collect();
            chunks.push(chunk);
        }
        chunks
    }
}

/// Linear interpolation resampling.
fn resample_linear(input: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if input.is_empty() {
        return Vec::new();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;

        if idx + 1 < input.len() {
            let sample = input[idx] as f64 * (1.0 - frac) + input[idx + 1] as f64 * frac;
            output.push(sample as i16);
        } else if idx < input.len() {
            output.push(input[idx]);
        }
    }
    output
}

/// Convert f32 sample to i16 with clamping.
fn f32_to_i16(s: f32) -> i16 {
    let clamped = s.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32) as i16
}

/// Convert u8 sample (unsigned, center at 128) to i16.
fn u8_to_i16(s: u8) -> i16 {
    // U8 audio: 0..255, 128 = silence
    ((s as i32 - 128) * 256) as i16
}

/// Convert i32 sample to i16 (shift right 16 bits).
fn i32_to_i16(s: i32) -> i16 {
    (s >> 16) as i16
}

/// Start audio capture on a dedicated OS thread.
/// Returns immediately. Audio chunks flow through `tx`.
/// Set `running` to false to stop capture.
pub fn start_capture(
    tx: AudioTx,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let device = default_input_device()?;
    let (config, sample_format) = pick_input_config(&device)?;

    let source_rate = config.sample_rate.0;
    let source_channels = config.channels;

    info!(
        device = device.name().unwrap_or_else(|_| "unknown".into()),
        rate = source_rate,
        channels = source_channels,
        format = ?sample_format,
        "Starting audio capture"
    );

    std::thread::spawn(move || {
        let downsampler = Arc::new(std::sync::Mutex::new(
            Downsampler::new(source_rate, source_channels),
        ));
        let tx_clone = tx.clone();
        let ds_clone = downsampler.clone();
        let running_clone = running.clone();

        let err_callback = |err: cpal::StreamError| {
            error!(%err, "Audio stream error");
        };

        let stream_result = match sample_format {
            SampleFormat::I16 => {
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !running_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(mut ds) = ds_clone.lock() {
                            for chunk in ds.feed(data) {
                                if tx_clone.blocking_send(chunk).is_err() {
                                    return;
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::F32 => {
                let ds_clone2 = downsampler.clone();
                let tx_clone2 = tx.clone();
                let running_clone2 = running.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !running_clone2.load(Ordering::Relaxed) {
                            return;
                        }
                        let i16_data: Vec<i16> = data.iter().map(|&s| f32_to_i16(s)).collect();
                        if let Ok(mut ds) = ds_clone2.lock() {
                            for chunk in ds.feed(&i16_data) {
                                if tx_clone2.blocking_send(chunk).is_err() {
                                    return;
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::U8 => {
                let ds_clone3 = downsampler.clone();
                let tx_clone3 = tx.clone();
                let running_clone3 = running.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[u8], _: &cpal::InputCallbackInfo| {
                        if !running_clone3.load(Ordering::Relaxed) {
                            return;
                        }
                        let i16_data: Vec<i16> = data.iter().map(|&s| u8_to_i16(s)).collect();
                        if let Ok(mut ds) = ds_clone3.lock() {
                            for chunk in ds.feed(&i16_data) {
                                if tx_clone3.blocking_send(chunk).is_err() {
                                    return;
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::I32 => {
                let ds_clone4 = downsampler.clone();
                let tx_clone4 = tx.clone();
                let running_clone4 = running.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i32], _: &cpal::InputCallbackInfo| {
                        if !running_clone4.load(Ordering::Relaxed) {
                            return;
                        }
                        let i16_data: Vec<i16> = data.iter().map(|&s| i32_to_i16(s)).collect();
                        if let Ok(mut ds) = ds_clone4.lock() {
                            for chunk in ds.feed(&i16_data) {
                                if tx_clone4.blocking_send(chunk).is_err() {
                                    return;
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            other => {
                error!(?other, "Unsupported sample format");
                return;
            }
        };

        match stream_result {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    error!(%e, "Failed to start audio stream");
                    return;
                }
                debug!("Audio stream playing");
                // Keep thread alive while recording
                while running.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                debug!("Audio capture stopped");
                drop(stream);
            }
            Err(e) => {
                error!(%e, "Failed to build audio input stream");
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_identity() {
        let input: Vec<i16> = (0..1600).collect();
        let output = resample_linear(&input, 16000, 16000);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn test_resample_downsample() {
        let input: Vec<i16> = (0..4800).collect();
        let output = resample_linear(&input, 48000, 16000);
        // Should be roughly 1/3 the size
        assert!(output.len() > 1500 && output.len() < 1700);
    }

    #[test]
    fn test_f32_to_i16_clamp() {
        assert_eq!(f32_to_i16(1.0), i16::MAX);
        assert_eq!(f32_to_i16(-1.0), -i16::MAX);
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(2.0), i16::MAX); // clamp
    }

    #[test]
    fn test_u8_to_i16() {
        assert_eq!(u8_to_i16(128), 0);       // silence
        assert_eq!(u8_to_i16(0), -32768);    // min
        assert_eq!(u8_to_i16(255), 32512);   // near max
    }

    #[test]
    fn test_i32_to_i16() {
        assert_eq!(i32_to_i16(0), 0);
        assert_eq!(i32_to_i16(i32::MAX), i16::MAX);
        assert_eq!(i32_to_i16(i32::MIN), i16::MIN);
    }
}

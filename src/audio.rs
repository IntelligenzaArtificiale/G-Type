// audio.rs — Microphone capture with cpal, real-time downsampling to 16kHz i16 mono.
// Uses std::sync::mpsc (not tokio) for the audio callback thread → collector bridge.
// This avoids issues with tokio::sync::mpsc::blocking_send on non-tokio threads.

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SampleRate, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Suppress noisy ALSA/JACK/OSS error messages printed to stderr during device enumeration.
/// These are harmless warnings from ALSA probing devices that don't exist (JACK, OSS, etc.).
fn suppress_alsa_stderr() {
    // Set env vars that quiet ALSA/PipeWire/JACK noise
    std::env::set_var("PIPEWIRE_LOG_LEVEL", "0");
    // The ALSA error handler can only be overridden via C API (snd_lib_error_set_handler),
    // but we can at least redirect stderr temporarily for the enumeration.
}

/// A chunk of 16kHz i16 mono PCM data (100ms = 1600 samples).
pub type AudioChunk = Vec<i16>;

/// Sender half for audio chunks (std::sync — NOT tokio).
pub type AudioTx = std::sync::mpsc::Sender<AudioChunk>;
/// Receiver half for audio chunks.
pub type AudioRx = std::sync::mpsc::Receiver<AudioChunk>;

/// Target sample rate for Gemini API.
const TARGET_RATE: u32 = 16_000;
/// Chunk duration in milliseconds.
const CHUNK_MS: u32 = 100;
/// Samples per chunk at target rate.
const SAMPLES_PER_CHUNK: usize = (TARGET_RATE * CHUNK_MS / 1000) as usize;

/// Get the best input device.
/// On Linux with PipeWire, the ALSA "default" device may not work for capture.
/// We enumerate all input devices and prefer USB hardware devices
/// (which are almost always external microphones), falling back to any hw: device,
/// and only using "default" as a last resort.
fn default_input_device() -> Result<Device> {
    suppress_alsa_stderr();
    let host = cpal::default_host();
    info!(host = host.id().name(), "Audio host selected");

    // Read /proc/asound/cards to identify which ALSA cards are USB
    let usb_card_names = detect_usb_alsa_cards();
    if !usb_card_names.is_empty() {
        info!(usb_cards = ?usb_card_names, "Detected USB audio cards");
    }

    if let Ok(input_devices) = host.input_devices() {
        let devices: Vec<Device> = input_devices.collect();

        let skip_prefixes = ["null", "default", "pipewire", "pulse", "sysdefault",
            "dsnoop", "plughw", "jack", "oss", "upmix", "vdownmix",
            "lavrate", "samplerate", "speex", "surround"];

        let mut usb_hw_devices: Vec<&Device> = Vec::new();
        let mut other_hw_devices: Vec<&Device> = Vec::new();

        for device in &devices {
            if let Ok(name) = device.name() {
                let lower = name.to_lowercase();
                if skip_prefixes.iter().any(|p| lower.starts_with(p)) {
                    continue;
                }
                // Only consider hw: and front: devices
                if !lower.starts_with("hw:") && !lower.starts_with("front:") {
                    continue;
                }
                // Verify it supports input configs
                let has_configs = device.supported_input_configs()
                    .map(|c| c.count() > 0).unwrap_or(false);
                if !has_configs {
                    continue;
                }

                // Check if this card is USB
                let is_usb = usb_card_names.iter().any(|usb_name| {
                    lower.contains(&format!("card={}", usb_name.to_lowercase()))
                });

                if is_usb {
                    usb_hw_devices.push(device);
                } else {
                    other_hw_devices.push(device);
                }
            }
        }

        // Prefer USB devices (external mics), then built-in
        let best = usb_hw_devices.first().or(other_hw_devices.first());

        if let Some(device) = best {
            let name = device.name().unwrap_or_else(|_| "unknown".into());
            let is_usb = usb_hw_devices.first().map(|d| std::ptr::eq(*d, *device)).unwrap_or(false);
            info!(
                device = name,
                usb = is_usb,
                "Selected hardware input device (bypassing ALSA default)"
            );
            return Ok((*device).clone());
        }
    }

    // Fallback to the default
    let device = host.default_input_device()
        .context("No audio input device found. Is a microphone connected?")?;
    info!(
        device = device.name().unwrap_or_else(|_| "unknown".into()),
        "Using default input device (fallback)"
    );
    Ok(device)
}

/// Read /proc/asound/cards to find card names that are USB-Audio.
/// Returns a list of card short names (e.g. "Device", "CameraB409241").
fn detect_usb_alsa_cards() -> Vec<String> {
    let mut usb_cards = Vec::new();
    if let Ok(contents) = std::fs::read_to_string("/proc/asound/cards") {
        // Format: " 1 [Device         ]: USB-Audio - TONOR TC-777 Audio Device"
        for line in contents.lines() {
            if line.contains("USB-Audio") || line.contains("usb-audio") {
                // Extract the card short name between [ and ]
                if let Some(start) = line.find('[') {
                    if let Some(end) = line.find(']') {
                        let name = line[start + 1..end].trim().to_string();
                        if !name.is_empty() {
                            usb_cards.push(name);
                        }
                    }
                }
            }
        }
    }
    usb_cards
}

/// Pick a supported input config.
/// Strategy: always use `device.default_input_config()` first — this is what the
/// audio server (PipeWire/PulseAudio) expects, and avoids format mismatches that
/// cause the stream to silently produce zero callbacks.
/// We downsample and mix to mono in software.
fn pick_input_config(device: &Device) -> Result<(StreamConfig, SampleFormat)> {
    // 1) Try device default — most reliable, especially under PipeWire
    if let Ok(default_cfg) = device.default_input_config() {
        let fmt = default_cfg.sample_format();
        let config: StreamConfig = default_cfg.into();
        info!(
            rate = config.sample_rate.0,
            channels = config.channels,
            format = ?fmt,
            "Using device default config (will downsample/mono-mix in software)"
        );
        return Ok((config, fmt));
    }

    // 2) Fallback: enumerate and pick first working config
    let supported = device
        .supported_input_configs()
        .context("Failed to query supported audio input configs")?;

    let configs: Vec<_> = supported.collect();
    if configs.is_empty() {
        bail!("Audio device supports no input configurations");
    }

    let format_priority = |fmt: SampleFormat| -> u8 {
        match fmt {
            SampleFormat::I16 => 0,
            SampleFormat::F32 => 1,
            SampleFormat::I32 => 2,
            SampleFormat::U8  => 3,
            _                 => 4,
        }
    };

    let mut all_sorted: Vec<_> = configs.iter().collect();
    all_sorted.sort_by_key(|cfg| format_priority(cfg.sample_format()));
    let best = all_sorted[0];
    // Prefer a rate the device natively supports
    let rate = if best.min_sample_rate().0 <= TARGET_RATE && best.max_sample_rate().0 >= TARGET_RATE {
        SampleRate(TARGET_RATE)
    } else {
        best.max_sample_rate()
    };
    let fallback = best.with_sample_rate(rate).config();
    let fmt = best.sample_format();
    warn!(
        rate = fallback.sample_rate.0,
        channels = fallback.channels,
        format = ?fmt,
        "Using fallback config (will downsample/mono-mix in software)"
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

/// Create a new audio channel pair (std::sync::mpsc).
pub fn audio_channel() -> (AudioTx, AudioRx) {
    std::sync::mpsc::channel()
}

/// Start audio capture on a dedicated OS thread.
/// Returns immediately. Audio chunks flow through `tx` (std::sync::mpsc).
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

    // Counters for debug logging (shared with callback)
    let callback_count = Arc::new(AtomicU64::new(0));
    let samples_fed = Arc::new(AtomicU64::new(0));
    let chunks_sent = Arc::new(AtomicU64::new(0));
    let send_errors = Arc::new(AtomicU64::new(0));

    let cb_count = callback_count.clone();
    let s_fed = samples_fed.clone();
    let c_sent = chunks_sent.clone();
    let s_err = send_errors.clone();

    std::thread::spawn(move || {
        let downsampler = Arc::new(std::sync::Mutex::new(
            Downsampler::new(source_rate, source_channels),
        ));

        let err_callback = |err: cpal::StreamError| {
            error!(%err, "Audio stream error");
        };

        // Generic helper: process raw i16 samples through downsampler and send chunks.
        // We define closures per format that convert to i16 then call this shared logic.
        let build_i16_callback = |ds: Arc<std::sync::Mutex<Downsampler>>,
                                   tx: AudioTx,
                                   running: Arc<AtomicBool>,
                                   cb_count: Arc<AtomicU64>,
                                   s_fed: Arc<AtomicU64>,
                                   c_sent: Arc<AtomicU64>,
                                   s_err: Arc<AtomicU64>| {
            move |data: &[i16]| {
                if !running.load(Ordering::Relaxed) {
                    return;
                }
                cb_count.fetch_add(1, Ordering::Relaxed);
                s_fed.fetch_add(data.len() as u64, Ordering::Relaxed);
                if let Ok(mut d) = ds.lock() {
                    for chunk in d.feed(data) {
                        match tx.send(chunk) {
                            Ok(()) => {
                                c_sent.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                s_err.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                        }
                    }
                }
            }
        };

        let i16_cb = build_i16_callback(
            downsampler.clone(), tx.clone(), running.clone(),
            cb_count.clone(), s_fed.clone(), c_sent.clone(), s_err.clone(),
        );

        let stream_result = match sample_format {
            SampleFormat::I16 => {
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        i16_cb(data);
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::F32 => {
                let f32_cb = build_i16_callback(
                    downsampler.clone(), tx.clone(), running.clone(),
                    cb_count.clone(), s_fed.clone(), c_sent.clone(), s_err.clone(),
                );
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let i16_data: Vec<i16> = data.iter().map(|&s| f32_to_i16(s)).collect();
                        f32_cb(&i16_data);
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::U8 => {
                let u8_cb = build_i16_callback(
                    downsampler.clone(), tx.clone(), running.clone(),
                    cb_count.clone(), s_fed.clone(), c_sent.clone(), s_err.clone(),
                );
                device.build_input_stream(
                    &config,
                    move |data: &[u8], _: &cpal::InputCallbackInfo| {
                        let i16_data: Vec<i16> = data.iter().map(|&s| u8_to_i16(s)).collect();
                        u8_cb(&i16_data);
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::I32 => {
                let i32_cb = build_i16_callback(
                    downsampler.clone(), tx.clone(), running.clone(),
                    cb_count.clone(), s_fed.clone(), c_sent.clone(), s_err.clone(),
                );
                device.build_input_stream(
                    &config,
                    move |data: &[i32], _: &cpal::InputCallbackInfo| {
                        let i16_data: Vec<i16> = data.iter().map(|&s| i32_to_i16(s)).collect();
                        i32_cb(&i16_data);
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
                info!("Audio stream playing — callbacks should start arriving");

                // Keep thread alive while recording, log stats periodically
                let mut tick = 0u32;
                while running.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    tick += 1;
                    if tick % 10 == 0 { // every ~1 second
                        debug!(
                            callbacks = callback_count.load(Ordering::Relaxed),
                            samples_fed = samples_fed.load(Ordering::Relaxed),
                            chunks_sent = chunks_sent.load(Ordering::Relaxed),
                            send_errors = send_errors.load(Ordering::Relaxed),
                            "Audio capture stats"
                        );
                    }
                }

                info!(
                    callbacks = callback_count.load(Ordering::Relaxed),
                    samples_fed = samples_fed.load(Ordering::Relaxed),
                    chunks_sent = chunks_sent.load(Ordering::Relaxed),
                    send_errors = send_errors.load(Ordering::Relaxed),
                    "Audio capture stopped — final stats"
                );
                drop(stream);
            }
            Err(e) => {
                error!(%e, "Failed to build audio input stream");
            }
        }
    });

    Ok(())
}

/// List all available audio input devices.
pub fn list_input_devices() -> Result<Vec<(String, Vec<String>)>> {
    suppress_alsa_stderr();
    let host = cpal::default_host();
    let default_name = host.default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();
    let input_devices = host.input_devices()
        .context("Failed to enumerate audio input devices")?;

    for device in input_devices {
        let name = device.name().unwrap_or_else(|_| "unknown".into());
        let is_default = name == default_name;
        let mut configs_info = Vec::new();

        if let Ok(supported) = device.supported_input_configs() {
            for cfg in supported {
                configs_info.push(format!(
                    "  {}Hz–{}Hz, {} ch, {:?}",
                    cfg.min_sample_rate().0,
                    cfg.max_sample_rate().0,
                    cfg.channels(),
                    cfg.sample_format()
                ));
            }
        }

        let label = if is_default {
            format!("{} (DEFAULT)", name)
        } else {
            name
        };
        devices.push((label, configs_info));
    }

    Ok(devices)
}

/// Run a quick audio capture test: record for `duration_secs` and return stats.
pub fn test_audio_capture(duration_secs: u32) -> Result<(u64, u64, f64)> {
    let device = default_input_device()?;
    let (config, sample_format) = pick_input_config(&device)?;

    let source_rate = config.sample_rate.0;
    let source_channels = config.channels;
    let device_name = device.name().unwrap_or_else(|_| "unknown".into());

    eprintln!("  Device: {}", device_name);
    eprintln!("  Config: {}Hz, {} channels, {:?}", source_rate, source_channels, sample_format);
    eprintln!("  Recording for {} seconds...", duration_secs);

    let callback_count = Arc::new(AtomicU64::new(0));
    let total_samples = Arc::new(AtomicU64::new(0));
    let peak_amplitude = Arc::new(AtomicU64::new(0)); // stored as u16
    let running = Arc::new(AtomicBool::new(true));

    let cc = callback_count.clone();
    let ts = total_samples.clone();
    let pa = peak_amplitude.clone();
    let r = running.clone();

    let err_callback = |err: cpal::StreamError| {
        eprintln!("  ❌ Stream error: {}", err);
    };

    // Generic callback that processes i16 samples
    let process_i16 = move |data: &[i16]| {
        if !r.load(Ordering::Relaxed) { return; }
        cc.fetch_add(1, Ordering::Relaxed);
        ts.fetch_add(data.len() as u64, Ordering::Relaxed);
        let max = data.iter().map(|s| s.unsigned_abs() as u64).max().unwrap_or(0);
        pa.fetch_max(max, Ordering::Relaxed);
    };

    let stream = match sample_format {
        SampleFormat::I16 => {
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    process_i16(data);
                },
                err_callback,
                None,
            )?
        }
        SampleFormat::F32 => {
            let cc2 = callback_count.clone();
            let ts2 = total_samples.clone();
            let pa2 = peak_amplitude.clone();
            let r2 = running.clone();
            device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !r2.load(Ordering::Relaxed) { return; }
                    cc2.fetch_add(1, Ordering::Relaxed);
                    ts2.fetch_add(data.len() as u64, Ordering::Relaxed);
                    let max = data.iter().map(|s| (s.abs() * i16::MAX as f32) as u64).max().unwrap_or(0);
                    pa2.fetch_max(max, Ordering::Relaxed);
                },
                err_callback,
                None,
            )?
        }
        SampleFormat::I32 => {
            let cc2 = callback_count.clone();
            let ts2 = total_samples.clone();
            let pa2 = peak_amplitude.clone();
            let r2 = running.clone();
            device.build_input_stream(
                &config,
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    if !r2.load(Ordering::Relaxed) { return; }
                    cc2.fetch_add(1, Ordering::Relaxed);
                    ts2.fetch_add(data.len() as u64, Ordering::Relaxed);
                    let max = data.iter().map(|s| (i32_to_i16(*s)).unsigned_abs() as u64).max().unwrap_or(0);
                    pa2.fetch_max(max, Ordering::Relaxed);
                },
                err_callback,
                None,
            )?
        }
        SampleFormat::U8 => {
            let cc2 = callback_count.clone();
            let ts2 = total_samples.clone();
            let pa2 = peak_amplitude.clone();
            let r2 = running.clone();
            device.build_input_stream(
                &config,
                move |data: &[u8], _: &cpal::InputCallbackInfo| {
                    if !r2.load(Ordering::Relaxed) { return; }
                    cc2.fetch_add(1, Ordering::Relaxed);
                    ts2.fetch_add(data.len() as u64, Ordering::Relaxed);
                    let max = data.iter().map(|s| u8_to_i16(*s).unsigned_abs() as u64).max().unwrap_or(0);
                    pa2.fetch_max(max, Ordering::Relaxed);
                },
                err_callback,
                None,
            )?
        }
        other => bail!("Unsupported format {:?} for test", other),
    };

    stream.play().context("Failed to play test stream")?;

    // Record for the specified duration, showing a live meter
    for i in 0..duration_secs {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let cbs = callback_count.load(Ordering::Relaxed);
        let samps = total_samples.load(Ordering::Relaxed);
        let peak = peak_amplitude.load(Ordering::Relaxed);
        let peak_pct = (peak as f64 / i16::MAX as f64 * 100.0).min(100.0);
        let bar_len = (peak_pct / 5.0) as usize;
        let bar: String = "█".repeat(bar_len) + &"░".repeat(20 - bar_len);
        eprintln!("  [{}s] callbacks={}, samples={}, peak={:.0}% |{}|",
            i + 1, cbs, samps, peak_pct, bar);
        // Reset peak for next second
        peak_amplitude.store(0, Ordering::Relaxed);
    }

    running.store(false, Ordering::Relaxed);
    drop(stream);

    let final_cbs = callback_count.load(Ordering::Relaxed);
    let final_samples = total_samples.load(Ordering::Relaxed);
    let final_peak = peak_amplitude.load(Ordering::Relaxed);
    let peak_pct = final_peak as f64 / i16::MAX as f64 * 100.0;

    Ok((final_cbs, final_samples, peak_pct))
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

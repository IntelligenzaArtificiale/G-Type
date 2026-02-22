// tracking.rs — Persistent cost and usage tracking with JSON-lines storage.
// Cross-platform: uses the same config directory as config.rs (XDG / AppData).
// Each transcription event is appended as a single JSON line — resilient to
// crashes and concurrent access. No external dependencies beyond serde_json.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

// ── Pricing Tables ─────────────────────────────────────────

/// Per-model pricing in USD per 1M tokens.
/// Audio input has its own rate; text output has a separate rate.
/// Some models have tiered pricing (prompt ≤ 200k vs > 200k tokens).
/// For simplicity we use the lower tier (≤ 200k) which covers 99%+ of
/// voice dictation use cases (a 60s recording ≈ 1,920 audio tokens).
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    /// USD per 1M input tokens (audio).
    pub input_audio_per_m: f64,
    /// USD per 1M input tokens (text — for the prompt).
    #[allow(dead_code)]
    pub input_text_per_m: f64,
    /// USD per 1M output tokens (including thinking tokens).
    pub output_per_m: f64,
}

/// Look up pricing for a model identifier.
/// Returns None for unknown models (cost will be logged as 0).
pub fn model_pricing(model: &str) -> Option<ModelPricing> {
    // Strip the "models/" prefix if present.
    let name = model.strip_prefix("models/").unwrap_or(model);

    match name {
        // ── Gemini 3.x (preview) ───────────────────────────
        "gemini-3.1-pro-preview" | "gemini-3.1-pro-preview-customtools" => Some(ModelPricing {
            input_audio_per_m: 2.0, // same as text for this model
            input_text_per_m: 2.0,
            output_per_m: 12.0,
        }),
        "gemini-3-pro-preview" => Some(ModelPricing {
            input_audio_per_m: 2.0,
            input_text_per_m: 2.0,
            output_per_m: 12.0,
        }),
        "gemini-3-flash-preview" => Some(ModelPricing {
            input_audio_per_m: 1.0,
            input_text_per_m: 0.50,
            output_per_m: 3.0,
        }),

        // ── Gemini 2.5 ────────────────────────────────────
        "gemini-2.5-pro" => Some(ModelPricing {
            input_audio_per_m: 1.25, // audio not separately listed; same tier
            input_text_per_m: 1.25,
            output_per_m: 10.0,
        }),
        "gemini-2.5-flash" => Some(ModelPricing {
            input_audio_per_m: 1.0,
            input_text_per_m: 0.30,
            output_per_m: 2.50,
        }),
        "gemini-2.5-flash-lite" | "gemini-2.5-flash-lite-preview-09-2025" => Some(ModelPricing {
            input_audio_per_m: 0.30,
            input_text_per_m: 0.10,
            output_per_m: 0.40,
        }),

        // ── Gemini 2.0 ────────────────────────────────────
        "gemini-2.0-flash" => Some(ModelPricing {
            input_audio_per_m: 0.70,
            input_text_per_m: 0.10,
            output_per_m: 0.40,
        }),

        _ => None,
    }
}

// ── Currency ───────────────────────────────────────────────

/// Supported display currencies with static exchange rates.
/// Rates are approximate and baked at compile time. All internal
/// calculations use USD; display-only conversion happens in stats output.
pub const CURRENCIES: &[(&str, &str, f64)] = &[
    ("USD", "$", 1.0),
    ("EUR", "€", 0.92),
    ("GBP", "£", 0.79),
    ("JPY", "¥", 149.0),
    ("INR", "₹", 83.0),
    ("BRL", "R$", 5.0),
    ("CNY", "¥", 7.25),
    ("KRW", "₩", 1330.0),
];

/// Get the display symbol for a currency code.
pub fn currency_symbol(code: &str) -> &'static str {
    CURRENCIES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, sym, _)| *sym)
        .unwrap_or("$")
}

/// Get the exchange rate (from USD) for a currency code.
pub fn exchange_rate(code: &str) -> f64 {
    CURRENCIES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, _, rate)| *rate)
        .unwrap_or(1.0)
}

/// Format a USD amount in the user's chosen currency.
pub fn format_cost(usd: f64, currency: &str) -> String {
    let sym = currency_symbol(currency);
    let rate = exchange_rate(currency);
    let converted = usd * rate;

    // Use appropriate decimal places based on currency magnitude.
    if rate > 100.0 {
        // JPY, KRW — no decimals needed
        format!("{}{:.0}", sym, converted)
    } else {
        format!("{}{:.6}", sym, converted)
    }
}

// ── Storage ────────────────────────────────────────────────

/// A single transcription event persisted to disk.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptionRecord {
    /// ISO 8601 timestamp (UTC).
    pub timestamp: String,
    /// Model used for this transcription.
    pub model: String,
    /// Audio duration in seconds.
    pub audio_duration_secs: f64,
    /// Number of input tokens (from usageMetadata).
    pub input_tokens: u64,
    /// Number of output tokens (from usageMetadata).
    pub output_tokens: u64,
    /// Cost of input in USD.
    pub input_cost_usd: f64,
    /// Cost of output in USD.
    pub output_cost_usd: f64,
    /// Total cost in USD.
    pub total_cost_usd: f64,
    /// Word count of the transcribed text.
    pub word_count: u32,
    /// Character count of the transcribed text.
    pub char_count: u32,
}

/// Token usage returned from the Gemini API response.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub candidates_tokens: u64,
    #[allow(dead_code)]
    pub total_tokens: u64,
}

/// Calculate cost from token usage.
pub fn calculate_cost(model: &str, usage: &TokenUsage) -> (f64, f64, f64) {
    match model_pricing(model) {
        Some(pricing) => {
            // For voice dictation, the vast majority of input tokens are audio.
            // The text prompt is ~50-80 tokens. We attribute all input tokens
            // to the audio rate for simplicity (the text portion is negligible).
            let input_cost = usage.prompt_tokens as f64 * pricing.input_audio_per_m / 1_000_000.0;
            let output_cost = usage.candidates_tokens as f64 * pricing.output_per_m / 1_000_000.0;
            let total = input_cost + output_cost;
            (input_cost, output_cost, total)
        }
        None => (0.0, 0.0, 0.0),
    }
}

/// Build a TranscriptionRecord from the transcription result.
pub fn build_record(
    model: &str,
    audio_duration_secs: f64,
    usage: &TokenUsage,
    transcription: &str,
) -> TranscriptionRecord {
    let (input_cost, output_cost, total_cost) = calculate_cost(model, usage);
    let word_count = transcription.split_whitespace().count() as u32;
    let char_count = transcription.chars().count() as u32;

    TranscriptionRecord {
        timestamp: chrono_now_utc(),
        model: model.to_string(),
        audio_duration_secs,
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.candidates_tokens,
        input_cost_usd: input_cost,
        output_cost_usd: output_cost,
        total_cost_usd: total_cost,
        word_count,
        char_count,
    }
}

/// Get current UTC timestamp as ISO 8601 string without external crate.
fn chrono_now_utc() -> String {
    // Use std::time to get Unix epoch seconds, then format manually.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let secs = now.as_secs();

    // Convert to date components (simplified — no leap second handling).
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01 → Y/M/D using the civil calendar algorithm.
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day).
/// Uses the algorithm from Howard Hinnant's `chrono`-compatible date library.
fn days_to_ymd(days: u64) -> (i32, u32, u32) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m, d)
}

/// Resolve the tracking data directory.
fn tracking_dir() -> Result<PathBuf> {
    let proj = ProjectDirs::from("", "", "g-type")
        .context("Cannot determine home directory for tracking data")?;
    Ok(proj.data_dir().to_path_buf())
}

/// Full path to the tracking JSONL file.
pub fn tracking_file_path() -> Result<PathBuf> {
    Ok(tracking_dir()?.join("usage.jsonl"))
}

/// Append a transcription record to the tracking file.
pub fn append_record(record: &TranscriptionRecord) -> Result<()> {
    let path = tracking_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create tracking directory {}", parent.display()))?;
    }

    let line = serde_json::to_string(record).context("Failed to serialize tracking record")?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Cannot open tracking file {}", path.display()))?;

    writeln!(file, "{}", line).context("Failed to write tracking record")?;

    Ok(())
}

/// Load all transcription records from disk.
pub fn load_records() -> Result<Vec<TranscriptionRecord>> {
    let path = tracking_file_path()?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&path)
        .with_context(|| format!("Cannot open tracking file {}", path.display()))?;

    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line.context("Failed to read line from tracking file")?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<TranscriptionRecord>(trimmed) {
            Ok(record) => records.push(record),
            Err(e) => {
                // Skip corrupted lines — don't fail the whole stats command.
                tracing::warn!(line = line_num + 1, %e, "Skipping corrupted tracking record");
            }
        }
    }

    Ok(records)
}

// ── Statistics ─────────────────────────────────────────────

/// Average typing speed: words per minute for an average typist.
/// Source: Various ergonomics studies cite 38-40 WPM for average typists.
const AVG_TYPING_WPM: f64 = 40.0;

/// Aggregated statistics over a set of records.
#[derive(Debug, Default)]
pub struct Stats {
    pub count: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_input_cost_usd: f64,
    pub total_output_cost_usd: f64,
    pub total_cost_usd: f64,
    pub total_words: u64,
    pub total_chars: u64,
    pub total_audio_secs: f64,
    /// Estimated time saved in seconds (voice vs keyboard).
    pub time_saved_secs: f64,
}

impl Stats {
    /// Compute stats from a slice of records.
    pub fn from_records(records: &[TranscriptionRecord]) -> Self {
        let mut s = Stats::default();
        for r in records {
            s.count += 1;
            s.total_input_tokens += r.input_tokens;
            s.total_output_tokens += r.output_tokens;
            s.total_input_cost_usd += r.input_cost_usd;
            s.total_output_cost_usd += r.output_cost_usd;
            s.total_cost_usd += r.total_cost_usd;
            s.total_words += r.word_count as u64;
            s.total_chars += r.char_count as u64;
            s.total_audio_secs += r.audio_duration_secs;
        }

        // Time saved: how long it would have taken to TYPE these words
        // minus how long it took to SPEAK them.
        let typing_time_secs = s.total_words as f64 / AVG_TYPING_WPM * 60.0;
        s.time_saved_secs = (typing_time_secs - s.total_audio_secs).max(0.0);

        s
    }
}

/// Filter records by date range (comparing ISO 8601 timestamp prefix).
pub fn filter_records_by_date(
    records: &[TranscriptionRecord],
    date_prefix: &str,
) -> Vec<TranscriptionRecord> {
    records
        .iter()
        .filter(|r| r.timestamp.starts_with(date_prefix))
        .cloned()
        .collect()
}

/// Get today's date as YYYY-MM-DD string.
pub fn today_prefix() -> String {
    let now = chrono_now_utc();
    now[..10].to_string()
}

/// Get this week's date range (Monday through Sunday).
/// Returns (start_date, end_date) as YYYY-MM-DD strings.
pub fn this_week_range() -> (String, String) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let days = now.as_secs() / 86400;

    // Day of week: 0 = Thursday (1970-01-01 was Thursday).
    // Convert so 0 = Monday.
    let dow = (days + 3) % 7; // 0=Mon, 6=Sun

    let monday = days - dow;
    let sunday = monday + 6;

    let (y1, m1, d1) = days_to_ymd(monday);
    let (y2, m2, d2) = days_to_ymd(sunday);

    (
        format!("{:04}-{:02}-{:02}", y1, m1, d1),
        format!("{:04}-{:02}-{:02}", y2, m2, d2),
    )
}

/// Filter records to this week (Monday–Sunday).
pub fn filter_records_this_week(records: &[TranscriptionRecord]) -> Vec<TranscriptionRecord> {
    let (start, end) = this_week_range();
    records
        .iter()
        .filter(|r| {
            let date = &r.timestamp[..10];
            date >= start.as_str() && date <= end.as_str()
        })
        .cloned()
        .collect()
}

/// Format a duration in seconds as a human-readable string.
pub fn format_duration(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else if secs < 3600.0 {
        let mins = secs / 60.0;
        format!("{:.1}min", mins)
    } else {
        let hours = secs / 3600.0;
        format!("{:.1}h", hours)
    }
}

/// Print full statistics report to stdout.
pub fn print_stats(currency: &str) -> Result<()> {
    let records = load_records()?;

    if records.is_empty() {
        println!();
        println!("  No transcription data yet. Start using G-Type to see stats!");
        println!();
        return Ok(());
    }

    let today = today_prefix();
    let today_records = filter_records_by_date(&records, &today);
    let week_records = filter_records_this_week(&records);

    let today_stats = Stats::from_records(&today_records);
    let week_stats = Stats::from_records(&week_records);
    let total_stats = Stats::from_records(&records);

    println!();
    println!("  \x1b[36m╔══════════════════════════════════════════════╗\x1b[0m");
    println!("  \x1b[36m║           G-Type Usage Statistics            ║\x1b[0m");
    println!("  \x1b[36m╚══════════════════════════════════════════════╝\x1b[0m");
    println!();

    // ── Today ──────────────────────────────────────────
    println!("  \x1b[1m📅 Today ({}):\x1b[0m", today);
    print_stats_section(&today_stats, currency);

    // ── This Week ──────────────────────────────────────
    let (week_start, week_end) = this_week_range();
    println!(
        "  \x1b[1m📆 This Week ({} → {}):\x1b[0m",
        week_start, week_end
    );
    print_stats_section(&week_stats, currency);

    // ── All Time ───────────────────────────────────────
    println!("  \x1b[1m📊 All Time:\x1b[0m");
    print_stats_section(&total_stats, currency);

    // ── Data file location ─────────────────────────────
    if let Ok(path) = tracking_file_path() {
        println!("  \x1b[2mData: {}\x1b[0m", path.display());
        println!();
    }

    Ok(())
}

/// Print a single stats section (today / week / total).
fn print_stats_section(stats: &Stats, currency: &str) {
    if stats.count == 0 {
        println!("     No transcriptions in this period.");
        println!();
        return;
    }

    println!("     Transcriptions:  {}", stats.count);
    println!("     Words dictated:  {}", stats.total_words);
    println!(
        "     Audio recorded:  {}",
        format_duration(stats.total_audio_secs)
    );
    println!(
        "     Input cost:      {}",
        format_cost(stats.total_input_cost_usd, currency)
    );
    println!(
        "     Output cost:     {}",
        format_cost(stats.total_output_cost_usd, currency)
    );
    println!(
        "     \x1b[1mTotal cost:       {}\x1b[0m",
        format_cost(stats.total_cost_usd, currency)
    );
    println!(
        "     ⏱️  Time saved:    {} (vs typing at {}wpm)",
        format_duration(stats.time_saved_secs),
        AVG_TYPING_WPM as u32
    );
    println!();
}

/// Format a single-line cost summary for the daemon log after each transcription.
pub fn format_log_line(record: &TranscriptionRecord, currency: &str) -> String {
    format!(
        "💰 Cost: {} (in: {}, out: {}) | {} words, {:.1}s audio | ⏱️ ~{} saved",
        format_cost(record.total_cost_usd, currency),
        format_cost(record.input_cost_usd, currency),
        format_cost(record.output_cost_usd, currency),
        record.word_count,
        record.audio_duration_secs,
        format_duration(estimated_time_saved(record)),
    )
}

/// Estimate time saved for a single transcription.
fn estimated_time_saved(record: &TranscriptionRecord) -> f64 {
    let typing_time = record.word_count as f64 / AVG_TYPING_WPM * 60.0;
    (typing_time - record.audio_duration_secs).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_pricing_known() {
        let p = model_pricing("models/gemini-2.0-flash").unwrap();
        assert!((p.input_audio_per_m - 0.70).abs() < 0.001);
        assert!((p.output_per_m - 0.40).abs() < 0.001);
    }

    #[test]
    fn test_model_pricing_stripped_prefix() {
        let p = model_pricing("gemini-2.5-flash").unwrap();
        assert!((p.input_audio_per_m - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_model_pricing_unknown() {
        assert!(model_pricing("models/gemini-99-turbo").is_none());
    }

    #[test]
    fn test_calculate_cost() {
        let usage = TokenUsage {
            prompt_tokens: 1_000_000,
            candidates_tokens: 1_000_000,
            total_tokens: 2_000_000,
        };
        let (input, output, total) = calculate_cost("models/gemini-2.0-flash", &usage);
        assert!((input - 0.70).abs() < 0.001); // 1M tokens × $0.70/M
        assert!((output - 0.40).abs() < 0.001); // 1M tokens × $0.40/M
        assert!((total - 1.10).abs() < 0.001);
    }

    #[test]
    fn test_calculate_cost_unknown_model() {
        let usage = TokenUsage {
            prompt_tokens: 1000,
            candidates_tokens: 500,
            total_tokens: 1500,
        };
        let (input, output, total) = calculate_cost("models/unknown", &usage);
        assert_eq!(input, 0.0);
        assert_eq!(output, 0.0);
        assert_eq!(total, 0.0);
    }

    #[test]
    fn test_format_cost_usd() {
        let s = format_cost(0.001234, "USD");
        assert!(s.starts_with('$'));
    }

    #[test]
    fn test_format_cost_eur() {
        let s = format_cost(1.0, "EUR");
        assert!(s.starts_with('€'));
    }

    #[test]
    fn test_format_cost_jpy() {
        let s = format_cost(1.0, "JPY");
        assert!(s.starts_with('¥'));
        // JPY should have no decimals
        assert!(!s.contains('.'));
    }

    #[test]
    fn test_currency_symbol() {
        assert_eq!(currency_symbol("USD"), "$");
        assert_eq!(currency_symbol("EUR"), "€");
        assert_eq!(currency_symbol("GBP"), "£");
        assert_eq!(currency_symbol("UNKNOWN"), "$"); // fallback
    }

    #[test]
    fn test_exchange_rate() {
        assert_eq!(exchange_rate("USD"), 1.0);
        assert!(exchange_rate("EUR") > 0.5 && exchange_rate("EUR") < 1.5);
        assert_eq!(exchange_rate("UNKNOWN"), 1.0); // fallback
    }

    #[test]
    fn test_build_record() {
        let usage = TokenUsage {
            prompt_tokens: 100,
            candidates_tokens: 50,
            total_tokens: 150,
        };
        let r = build_record("models/gemini-2.0-flash", 3.5, &usage, "ciao mondo test");
        assert_eq!(r.word_count, 3);
        assert_eq!(r.char_count, 15);
        assert!((r.audio_duration_secs - 3.5).abs() < 0.001);
        assert!(r.total_cost_usd > 0.0);
    }

    #[test]
    fn test_stats_from_records() {
        let records = vec![
            TranscriptionRecord {
                timestamp: "2025-01-15T10:00:00Z".into(),
                model: "models/gemini-2.0-flash".into(),
                audio_duration_secs: 5.0,
                input_tokens: 160,
                output_tokens: 30,
                input_cost_usd: 0.000112,
                output_cost_usd: 0.000012,
                total_cost_usd: 0.000124,
                word_count: 20,
                char_count: 100,
            },
            TranscriptionRecord {
                timestamp: "2025-01-15T11:00:00Z".into(),
                model: "models/gemini-2.0-flash".into(),
                audio_duration_secs: 3.0,
                input_tokens: 96,
                output_tokens: 15,
                input_cost_usd: 0.0000672,
                output_cost_usd: 0.000006,
                total_cost_usd: 0.0000732,
                word_count: 10,
                char_count: 50,
            },
        ];
        let stats = Stats::from_records(&records);
        assert_eq!(stats.count, 2);
        assert_eq!(stats.total_words, 30);
        assert!((stats.total_audio_secs - 8.0).abs() < 0.001);
        // 30 words at 40 WPM = 45 seconds typing. Minus 8s audio = 37s saved.
        assert!(stats.time_saved_secs > 30.0);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30.0), "30s");
        assert_eq!(format_duration(90.0), "1.5min");
        assert_eq!(format_duration(7200.0), "2.0h");
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_known_date() {
        // 2024-01-01 = day 19723 since epoch
        let (y, m, d) = days_to_ymd(19723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    #[test]
    fn test_chrono_now_utc_format() {
        let ts = chrono_now_utc();
        // Should match YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn test_filter_records_by_date() {
        let records = vec![
            TranscriptionRecord {
                timestamp: "2025-01-15T10:00:00Z".into(),
                model: "m".into(),
                audio_duration_secs: 1.0,
                input_tokens: 10,
                output_tokens: 5,
                input_cost_usd: 0.0,
                output_cost_usd: 0.0,
                total_cost_usd: 0.0,
                word_count: 5,
                char_count: 20,
            },
            TranscriptionRecord {
                timestamp: "2025-01-16T10:00:00Z".into(),
                model: "m".into(),
                audio_duration_secs: 1.0,
                input_tokens: 10,
                output_tokens: 5,
                input_cost_usd: 0.0,
                output_cost_usd: 0.0,
                total_cost_usd: 0.0,
                word_count: 5,
                char_count: 20,
            },
        ];
        let filtered = filter_records_by_date(&records, "2025-01-15");
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_record_serialization_roundtrip() {
        let record = TranscriptionRecord {
            timestamp: "2025-01-15T10:30:00Z".into(),
            model: "models/gemini-2.0-flash".into(),
            audio_duration_secs: 4.2,
            input_tokens: 134,
            output_tokens: 25,
            input_cost_usd: 0.0000938,
            output_cost_usd: 0.00001,
            total_cost_usd: 0.0001038,
            word_count: 12,
            char_count: 60,
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: TranscriptionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, record.model);
        assert_eq!(deserialized.word_count, record.word_count);
        assert!((deserialized.total_cost_usd - record.total_cost_usd).abs() < 1e-10);
    }
}

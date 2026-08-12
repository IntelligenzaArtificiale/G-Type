// tracking.rs — Persistent cost and usage tracking with JSON-lines storage.
// Pricing is centralized in providers::model_catalog and reviewed against the
// official Google Gemini Developer API pricing page.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub input_audio_per_m: f64,
    pub input_text_per_m: f64,
    pub output_per_m: f64,
}

pub fn model_pricing(model: &str) -> Option<ModelPricing> {
    let spec = crate::providers::model_catalog::find(model)?;
    let (text, audio, output) = spec.pricing_for_prompt_tokens(0);
    Some(ModelPricing {
        input_audio_per_m: audio,
        input_text_per_m: text,
        output_per_m: output,
    })
}

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

pub fn currency_symbol(code: &str) -> &'static str {
    CURRENCIES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, sym, _)| *sym)
        .unwrap_or("$")
}

pub fn exchange_rate(code: &str) -> f64 {
    CURRENCIES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, _, rate)| *rate)
        .unwrap_or(1.0)
}

pub fn format_cost(usd: f64, currency: &str) -> String {
    let sym = currency_symbol(currency);
    let rate = exchange_rate(currency);
    let converted = usd * rate;
    if rate > 100.0 {
        format!("{}{:.0}", sym, converted)
    } else {
        format!("{}{:.6}", sym, converted)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptionRecord {
    pub timestamp: String,
    pub model: String,
    pub audio_duration_secs: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub input_cost_usd: f64,
    pub output_cost_usd: f64,
    pub total_cost_usd: f64,
    pub word_count: u32,
    pub char_count: u32,
    #[serde(default)]
    pub text: String,
}

/// Token usage returned by Gemini. New modality fields are deliberately kept in
/// memory only: persisted records remain backward compatible while cost is
/// calculated accurately at creation time.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub candidates_tokens: u64,
    pub thoughts_tokens: u64,
    pub total_tokens: u64,
    pub audio_input_tokens: u64,
    pub text_input_tokens: u64,
}

impl TokenUsage {
    pub fn billable_output_tokens(&self) -> u64 {
        self.candidates_tokens.saturating_add(self.thoughts_tokens)
    }
}

pub fn calculate_cost(model: &str, usage: &TokenUsage) -> (f64, f64, f64) {
    let Some(spec) = crate::providers::model_catalog::find(model) else {
        return (0.0, 0.0, 0.0);
    };

    let (text_rate, audio_rate, output_rate) =
        spec.pricing_for_prompt_tokens(usage.prompt_tokens);

    let detailed_input = usage
        .audio_input_tokens
        .saturating_add(usage.text_input_tokens);

    let input_cost = if detailed_input > 0 {
        let other_tokens = usage.prompt_tokens.saturating_sub(detailed_input);
        // Unknown/non-audio modalities are charged at the general text/image
        // rate. G-Type itself currently sends only text + audio.
        ((usage.audio_input_tokens as f64 * audio_rate)
            + ((usage.text_input_tokens + other_tokens) as f64 * text_rate))
            / 1_000_000.0
    } else {
        // Older API responses may not include promptTokensDetails. Treating all
        // prompt tokens as audio is conservative and preserves historical
        // behavior rather than under-reporting cost.
        usage.prompt_tokens as f64 * audio_rate / 1_000_000.0
    };

    // Google bills thinking tokens at the output rate as well.
    let output_cost =
        usage.billable_output_tokens() as f64 * output_rate / 1_000_000.0;
    let total = input_cost + output_cost;
    (input_cost, output_cost, total)
}

pub fn build_record(
    model: &str,
    audio_duration_secs: f64,
    usage: &TokenUsage,
    transcription: &str,
) -> TranscriptionRecord {
    let (input_cost, output_cost, total_cost) = calculate_cost(model, usage);
    TranscriptionRecord {
        timestamp: chrono_now_utc(),
        model: model.to_string(),
        audio_duration_secs,
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.billable_output_tokens(),
        input_cost_usd: input_cost,
        output_cost_usd: output_cost,
        total_cost_usd: total_cost,
        word_count: transcription.split_whitespace().count() as u32,
        char_count: transcription.chars().count() as u32,
        text: transcription.to_string(),
    }
}

pub fn load_recent_records(n: usize) -> Result<Vec<TranscriptionRecord>> {
    let mut all = load_records()?;
    all.reverse();
    all.truncate(n);
    Ok(all)
}

fn chrono_now_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

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

fn tracking_dir() -> Result<PathBuf> {
    let proj = ProjectDirs::from("", "", "g-type")
        .context("Cannot determine home directory for tracking data")?;
    Ok(proj.data_dir().to_path_buf())
}

pub fn tracking_file_path() -> Result<PathBuf> {
    Ok(tracking_dir()?.join("usage.jsonl"))
}

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
            Err(e) => tracing::warn!(line = line_num + 1, %e, "Skipping corrupted tracking record"),
        }
    }
    Ok(records)
}

const AVG_TYPING_WPM: f64 = 40.0;

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
    pub time_saved_secs: f64,
}

impl Stats {
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
        let typing_time_secs = s.total_words as f64 / AVG_TYPING_WPM * 60.0;
        s.time_saved_secs = (typing_time_secs - s.total_audio_secs).max(0.0);
        s
    }
}

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

pub fn today_prefix() -> String {
    let now = chrono_now_utc();
    now[..10].to_string()
}

pub fn this_week_range() -> (String, String) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let days = now.as_secs() / 86400;
    let dow = (days + 3) % 7;
    let monday = days - dow;
    let sunday = monday + 6;
    let (y1, m1, d1) = days_to_ymd(monday);
    let (y2, m2, d2) = days_to_ymd(sunday);
    (
        format!("{:04}-{:02}-{:02}", y1, m1, d1),
        format!("{:04}-{:02}-{:02}", y2, m2, d2),
    )
}

pub fn filter_records_this_week(records: &[TranscriptionRecord]) -> Vec<TranscriptionRecord> {
    let (start, end) = this_week_range();
    records
        .iter()
        .filter(|r| {
            let date = r.timestamp.get(..10).unwrap_or("");
            date >= start.as_str() && date <= end.as_str()
        })
        .cloned()
        .collect()
}

pub fn format_duration(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else if secs < 3600.0 {
        format!("{:.1}min", secs / 60.0)
    } else {
        format!("{:.1}h", secs / 3600.0)
    }
}

pub fn print_stats(currency: &str) -> Result<()> {
    let records = load_records()?;
    if records.is_empty() {
        println!("\n  No transcription data yet. Start using G-Type to see stats!\n");
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
    println!("  \x1b[1m📅 Today ({}):\x1b[0m", today);
    print_stats_section(&today_stats, currency);
    let (week_start, week_end) = this_week_range();
    println!("  \x1b[1m📆 This Week ({} → {}):\x1b[0m", week_start, week_end);
    print_stats_section(&week_stats, currency);
    println!("  \x1b[1m📊 All Time:\x1b[0m");
    print_stats_section(&total_stats, currency);
    if let Ok(path) = tracking_file_path() {
        println!("  \x1b[2mData: {}\x1b[0m\n", path.display());
    }
    Ok(())
}

fn print_stats_section(stats: &Stats, currency: &str) {
    if stats.count == 0 {
        println!("     No transcriptions in this period.\n");
        return;
    }
    println!("     Transcriptions:  {}", stats.count);
    println!("     Words dictated:  {}", stats.total_words);
    println!("     Audio recorded:  {}", format_duration(stats.total_audio_secs));
    println!("     Input cost:      {}", format_cost(stats.total_input_cost_usd, currency));
    println!("     Output cost:     {}", format_cost(stats.total_output_cost_usd, currency));
    println!("     \x1b[1mTotal cost:       {}\x1b[0m", format_cost(stats.total_cost_usd, currency));
    println!("     ⏱️  Time saved:    {} (vs typing at {}wpm)\n", format_duration(stats.time_saved_secs), AVG_TYPING_WPM as u32);
}

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

fn estimated_time_saved(record: &TranscriptionRecord) -> f64 {
    let typing_time = record.word_count as f64 / AVG_TYPING_WPM * 60.0;
    (typing_time - record.audio_duration_secs).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(prompt: u64, output: u64) -> TokenUsage {
        TokenUsage {
            prompt_tokens: prompt,
            candidates_tokens: output,
            total_tokens: prompt + output,
            ..TokenUsage::default()
        }
    }

    #[test]
    fn current_prices_are_available() {
        assert!((model_pricing("gemini-3.6-flash").unwrap().output_per_m - 7.50).abs() < 0.001);
        assert!((model_pricing("gemini-3.5-flash-lite").unwrap().input_audio_per_m - 0.30).abs() < 0.001);
        assert!((model_pricing("gemini-3.1-flash-lite").unwrap().input_audio_per_m - 0.50).abs() < 0.001);
        assert!((model_pricing("gemini-2.5-flash-lite").unwrap().output_per_m - 0.40).abs() < 0.001);
    }

    #[test]
    fn modality_cost_is_exact_when_details_are_present() {
        let usage = TokenUsage {
            prompt_tokens: 1_000_000,
            candidates_tokens: 1_000_000,
            audio_input_tokens: 900_000,
            text_input_tokens: 100_000,
            ..TokenUsage::default()
        };
        let (input, output, total) = calculate_cost("gemini-3.1-flash-lite", &usage);
        assert!((input - 0.475).abs() < 0.000001);
        assert!((output - 1.50).abs() < 0.000001);
        assert!((total - 1.975).abs() < 0.000001);
    }

    #[test]
    fn thought_tokens_are_billed_as_output() {
        let usage = TokenUsage {
            prompt_tokens: 0,
            candidates_tokens: 100,
            thoughts_tokens: 100,
            ..TokenUsage::default()
        };
        let (_, output, _) = calculate_cost("gemini-3.6-flash", &usage);
        assert!((output - 0.0015).abs() < 0.000001);
    }

    #[test]
    fn tiered_pro_pricing_switches_above_200k() {
        let low = calculate_cost("gemini-3.1-pro-preview", &usage(200_000, 1000));
        let high = calculate_cost("gemini-3.1-pro-preview", &usage(200_001, 1000));
        assert!(high.0 > low.0);
        assert!(high.1 > low.1);
    }

    #[test]
    fn unknown_model_cost_is_zero() {
        assert_eq!(calculate_cost("unknown", &usage(100, 50)), (0.0, 0.0, 0.0));
    }

    #[test]
    fn record_and_stats_still_work() {
        let r = build_record("gemini-3.5-flash-lite", 3.5, &usage(100, 50), "ciao mondo test");
        assert_eq!(r.word_count, 3);
        assert!(r.total_cost_usd > 0.0);
        let stats = Stats::from_records(&[r]);
        assert_eq!(stats.count, 1);
        assert_eq!(stats.total_words, 3);
    }

    #[test]
    fn date_helpers_are_safe() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        let ts = chrono_now_utc();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
    }
}

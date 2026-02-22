// main.rs — Entry point for the G-Type daemon.
// Supports CLI subcommands for zero-friction user experience:
//   g-type          → run daemon (auto-setup on first run)
//   g-type setup    → interactive config wizard
//   g-type set-key  → update API key without full setup
//   g-type config   → print config file path

mod app;
mod audio;
mod audio_feedback;
mod config;
mod injector;
mod input;
mod network;
mod tracking;

use anyhow::Result;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

fn print_usage() {
    eprintln!("Usage: g-type [command]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  (none)        Start the dictation daemon");
    eprintln!("  setup         Run interactive setup wizard");
    eprintln!("  set-key       Update your Gemini API key");
    eprintln!("  config        Show config file location");
    eprintln!("  stats         Show cost & usage statistics");
    eprintln!("  test-audio    Test microphone capture (3 seconds)");
    eprintln!("  list-devices  List all audio input devices");
    eprintln!("  help          Show this message");
    eprintln!();
    eprintln!("Hold your hotkey (default: CTRL+SHIFT+SPACE) to dictate anywhere.");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str());

    // Handle non-daemon commands before initializing logger
    match command {
        Some("help") | Some("--help") | Some("-h") => {
            print_usage();
            return Ok(());
        }
        Some("config") => {
            match config::config_path() {
                Ok(p) => println!("{}", p.display()),
                Err(e) => {
                    eprintln!("❌ {e}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        Some("setup") => {
            let path = config::config_path()?;
            if let Err(e) = config::interactive_setup(&path) {
                eprintln!("\n❌ Setup failed: {e}\n");
                std::process::exit(1);
            }
            return Ok(());
        }
        Some("stats") => {
            // Load config for currency preference (fallback to USD if no config).
            let currency = config::config_path()
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|raw| toml::from_str::<config::Config>(&raw).ok())
                .map(|c| c.currency)
                .unwrap_or_else(|| "USD".to_string());

            if let Err(e) = tracking::print_stats(&currency) {
                eprintln!("\n❌ Failed to load stats: {e}\n");
                std::process::exit(1);
            }
            return Ok(());
        }
        Some("set-key") => {
            let key = args.get(2).map(|s| s.as_str());
            match key {
                Some(k) => {
                    if let Err(e) = config::set_api_key(k) {
                        eprintln!("\n❌ {e}\n");
                        std::process::exit(1);
                    }
                }
                None => {
                    eprintln!("Usage: g-type set-key <YOUR_API_KEY>");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        Some("test-audio") => {
            eprintln!();
            eprintln!("  \x1b[36m🎤 G-Type Audio Test\x1b[0m");
            eprintln!();
            match audio::test_audio_capture(3) {
                Ok((callbacks, samples, _peak)) => {
                    eprintln!();
                    if callbacks == 0 {
                        eprintln!("  \x1b[31m❌ FAIL: No audio callbacks received!\x1b[0m");
                        eprintln!("     Your audio device is not sending data.");
                        eprintln!("     Try: g-type list-devices");
                    } else if samples == 0 {
                        eprintln!("  \x1b[31m❌ FAIL: Callbacks fired but no samples!\x1b[0m");
                    } else {
                        eprintln!("  \x1b[32m✔ PASS: Audio capture working!\x1b[0m");
                        eprintln!("    {} callbacks, {} total samples", callbacks, samples);
                    }
                    eprintln!();
                }
                Err(e) => {
                    eprintln!("  \x1b[31m❌ Audio test failed: {}\x1b[0m", e);
                    eprintln!();
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        Some("list-devices") => {
            eprintln!();
            eprintln!("  \x1b[36m🔊 Audio Input Devices\x1b[0m");
            eprintln!();
            match audio::list_input_devices() {
                Ok(devices) => {
                    if devices.is_empty() {
                        eprintln!("  No audio input devices found!");
                    } else {
                        for (name, configs) in &devices {
                            eprintln!("  • {}", name);
                            for cfg in configs {
                                eprintln!("   {}", cfg);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  \x1b[31m❌ Failed to list devices: {}\x1b[0m", e);
                }
            }
            eprintln!();
            return Ok(());
        }
        Some(unknown) => {
            eprintln!("Unknown command: {}", unknown);
            eprintln!();
            print_usage();
            std::process::exit(1);
        }
        None => {} // default: run daemon
    }

    // ── Daemon mode ────────────────────────────────────────

    // Initialize structured logging with env filter.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("g_type=info,warn")),
        )
        .with_target(true)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .compact()
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "G-Type daemon starting"
    );

    // Load config (auto-triggers interactive setup if missing)
    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            error!(%e, "Configuration error");
            eprintln!("\n❌ {e}\n");
            std::process::exit(1);
        }
    };

    debug!(
        model = %cfg.model,
        hotkey = %cfg.hotkey,
        "Configuration loaded"
    );

    // Run the main event loop (never returns under normal operation)
    if let Err(e) = app::run(cfg).await {
        error!(%e, "Fatal error in main loop");
        eprintln!("\n❌ Fatal: {e}\n");
        std::process::exit(1);
    }

    Ok(())
}

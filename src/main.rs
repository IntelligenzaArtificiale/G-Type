// main.rs — Entry point for the G-Type daemon.
// Supports CLI subcommands for zero-friction user experience:
//   g-type          → run daemon (auto-setup on first run)
//   g-type setup    → interactive config wizard
//   g-type set-key  → update API key without full setup
//   g-type config   → print config file path

mod app;
mod audio;
mod config;
mod injector;
mod input;
mod network;

use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn print_usage() {
    eprintln!("Usage: g-type [command]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  (none)      Start the dictation daemon");
    eprintln!("  setup       Run interactive setup wizard");
    eprintln!("  set-key     Update your Gemini API key");
    eprintln!("  config      Show config file location");
    eprintln!("  help        Show this message");
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

    info!(
        model = %cfg.model,
        hotkey = %cfg.hotkey,
        threshold = cfg.injection_threshold,
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

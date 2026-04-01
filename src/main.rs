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
mod overlay;
mod providers;
mod settings;
mod tracking;
mod transforms;
mod tray;
mod ui_bridge;
mod upgrade;

use anyhow::Result;
use tracing::{debug, error, info, warn};
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
    eprintln!("  upgrade       Self-update to latest release");
    eprintln!("  version       Show current version");
    eprintln!("  test-audio    Test microphone capture (3 seconds)");
    eprintln!("  list-devices  List all audio input devices");
    eprintln!("  help          Show this message");
    eprintln!();
    eprintln!("Hold your hotkey (default: CTRL+SHIFT+SPACE) to dictate anywhere.");
}

fn main() -> Result<()> {
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
                let _path = config::config_path().unwrap_or_default();
            if let Err(e) = config::load() {
                eprintln!("\n❌ Setup failed: {e}\n");
                std::process::exit(1);
            }
            println!("Ready. Use the web settings to configure further.");
            return Ok(());
        }
        Some("stats") => {
            // Load config for currency preference (fallback to USD if no config).
            let currency = config::config_path()
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|raw| toml::from_str::<config::ConfigV2>(&raw).ok())
                .map(|c| c.global.currency)
                .unwrap_or_else(|| "USD".to_string());

            if let Err(e) = tracking::print_stats(&currency) {
                eprintln!("\n❌ Failed to load stats: {e}\n");
                std::process::exit(1);
            }
            return Ok(());
        }
        Some("upgrade") | Some("update") => {
            if let Err(e) = upgrade::run_upgrade() {
                eprintln!("\n❌ Upgrade failed: {e}\n");
                std::process::exit(1);
            }
            return Ok(());
        }
        Some("version") | Some("--version") | Some("-V") => {
            println!("g-type {}", env!("CARGO_PKG_VERSION"));
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

    let mut cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            error!(%e, "Configuration error");
            eprintln!("\n❌ {e}\n");
            std::process::exit(1);
        }
    };

    // --- Web Onboarding Flow ---
    // If there are no API keys configured, we assume first run.
    let is_first_run = cfg.keys.is_empty();
    
    // We start the tokio runtime here because we need it for the settings server anyway
    let rt = tokio::runtime::Runtime::new()?;

    let cfg_shared = std::sync::Arc::new(tokio::sync::RwLock::new(cfg.clone()));

    // Spawn Settings Dashboard over HTTP (always running for settings & setup)
    let cfg_server_clone = cfg_shared.clone();
    rt.spawn(async move {
        if let Err(e) = settings::start_server(cfg_server_clone).await {
            error!("Settings server failed: {}", e);
        }
    });

    if is_first_run {
        info!("No API keys found. Launching initial web setup...");
        
        let setup_url = "http://127.0.0.1:9741/setup";
        if let Err(e) = open::that(setup_url) {
            warn!("Could not open browser automatically: {}", e);
        }

        // Wait in a loop until the API key is populated
        rt.block_on(async {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                let current_cfg = cfg_shared.read().await;
                if !current_cfg.keys.is_empty() {
                    // Update our local synchronous config
                    cfg = current_cfg.clone();
                    info!("Setup complete! Proceeding to background daemon.");
                    break;
                }
            }
        });
    }

    let p0 = cfg.profiles.get(0).expect("At least 1 profile must exist");
    debug!(
        model = %p0.model,
        hotkey = %p0.hotkey,
        "Configuration loaded"
    );

    // ANTIREX: We split the thread models here!
    // -> OS Thread: winit event loop (required for GUI components like UI floating pill)
    // -> Spawned Thread: tokio background runtime (Settings API, audio logic, hotkeys)

    // Set up cross-thread communication for Winit <-> Tokio
    let (ui_tx, ui_rx) = std::sync::mpsc::channel::<ui_bridge::UiCommand>();

    // Create the native OS Main Window loop (Winit)
    use winit::event_loop::{ControlFlow, EventLoop};
    let event_loop = winit::event_loop::EventLoop::<ui_bridge::DaemonEvent>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let ui_proxy = event_loop.create_proxy();

    // Spawn Background Daemon Thread
    std::thread::spawn(move || {
        rt.block_on(async move {
            // Run the main event loop (never returns under normal operation)
            if let Err(e) = app::run_with_ui(cfg, ui_proxy, ui_rx).await {
                error!(%e, "Fatal error in main tokio loop");
                eprintln!("\n❌ Fatal: {e}\n");
                std::process::exit(1);
            }
        });
    });    // We instantiate TrayManager and OverlayManager once the event loop is running.
    // However, in winit 0.30+, `run` is deprecated in favor of `run_app`. We'll just suppress
    // the deprecation warning and use the closure for simplicity of porting existing logic.
    #[allow(deprecated)]
    let _ = event_loop.run(move |event, elwt| {
        // UI Managers will be instantiated asynchronously on Resumed or via standard event matching
        use winit::event::{Event, WindowEvent};
        use crate::ui_bridge::{DaemonEvent, DaemonState};
        
        // Static references to managers
        // (Due to closure borrowing rules, we can use static or Option variables moved in)
        // For simplicity, handle state here via local options.
        static mut TRAY_MGR: Option<tray::TrayManager> = None;
        static mut OVERLAY_MGR: Option<overlay::OverlayManager> = None;

        match event {
            Event::Resumed => {
                // Initialize Tray
                if unsafe { TRAY_MGR.is_none() } {
                    let tm = tray::TrayManager::new(ui_tx.clone()).expect("Failed to create TrayManager");
                    unsafe { TRAY_MGR = Some(tm); }
                }

                // Initialize Overlay
                if unsafe { OVERLAY_MGR.is_none() } {
                    use winit::window::Window;
                    let window = elwt.create_window(
                        Window::default_attributes()
                        .with_title("G-Type Overlay")
                        .with_decorations(false)
                        .with_transparent(true)
                        .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                        .with_visible(false) // hidden until recording
                    ).expect("Failed to build window");
                        
                    let om = overlay::OverlayManager::new(&window, ui_tx.clone()).expect("Failed to create OverlayManager");
                    unsafe { OVERLAY_MGR = Some(om); }
                }
            }
            Event::UserEvent(daemon_event) => {
                match daemon_event {
                    DaemonEvent::StateChanged(state) => {
                        let tray_ref = unsafe { TRAY_MGR.as_ref() };
                        let overlay_ref = unsafe { OVERLAY_MGR.as_ref() };

                        match state {
                            DaemonState::Idle => {
                                if let Some(t) = tray_ref { let _ = t.set_idle(); }
                                if let Some(o) = overlay_ref { let _ = o.set_idle(); }
                            }
                            DaemonState::Recording { profile } => {
                                if let Some(t) = tray_ref { let _ = t.set_recording(&profile); }
                                if let Some(o) = overlay_ref { let _ = o.set_recording(); }
                            }
                            DaemonState::Processing { profile } => {
                                if let Some(t) = tray_ref { let _ = t.set_processing(); }
                                if let Some(o) = overlay_ref { let _ = o.set_processing(); }
                            }
                        }
                    }
                    DaemonEvent::ProfileActivated(info) => {
                        let tray_ref = unsafe { TRAY_MGR.as_ref() };
                        if let Some(t) = tray_ref {
                            // Can add tooltips later
                            let _ = t;
                        }
                    }
                    DaemonEvent::ProfilesUpdated(_) => {
                        // Can refresh tray menu items later
                    }
                    DaemonEvent::Error(err) => {
                        error!("UI received error event: {}", err);
                    }
                    DaemonEvent::Quit => {
                        elwt.exit();
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                elwt.exit();
            }
            _ => {}
        }
    });

    Ok(())
}

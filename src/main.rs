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
            // Check if daemon is already running by probing the port
            if std::net::TcpStream::connect("127.0.0.1:9741").is_ok() {
                println!("G-Type è già in esecuzione. Apertura pagina di setup nel browser...");
                let _ = open::that("http://127.0.0.1:9741/setup");
                return Ok(());
            }

            println!("Il demone non è in esecuzione. Avvio di G-Type in background...");
            // Just launch the daemon detached so it binds the port and serves the setup
            std::process::Command::new(std::env::current_exe()?)
                .spawn()?;
            
            // Give it half a second to bind
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = open::that("http://127.0.0.1:9741/setup");
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

    // Singleton check: try to bind the port.
    // If binding fails, the port is held by another process — but we must verify
    // it is actually RESPONSIVE (not a zombie/stopped process holding a dead socket).
    // A stopped process can hold a TCP socket open but never reply to HTTP requests.
    let listener = match rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:9741")) {
        Ok(l) => l,
        Err(_) => {
            // Port is taken. Probe with a real HTTP GET to confirm liveness.
            let alive = rt.block_on(async {
                use tokio::io::AsyncWriteExt;
                let mut stream = match tokio::net::TcpStream::connect("127.0.0.1:9741").await {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                let _ = stream.write_all(b"GET /api/state HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").await;
                // Give it 1 second to send back at least 1 byte.
                let mut buf = [0u8; 1];
                match tokio::time::timeout(
                    std::time::Duration::from_secs(1),
                    tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
                ).await {
                    Ok(Ok(n)) => n > 0,
                    _ => false,
                }
            });

            if alive {
                eprintln!("\n❌ G-Type è già in esecuzione in background!");
                eprintln!("💡 Controlla l'icona nella barra delle applicazioni (vicino all'orologio).");
                eprintln!("   Per configurare G-Type vai su: http://127.0.0.1:9741/\n");
                std::process::exit(1);
            }

            // Port is held by a dead/stopped process. Force-reclaim it with SO_REUSEADDR.
            warn!("Port 9741 held by an unresponsive process — reclaiming with SO_REUSEADDR.");
            let sock = socket2::Socket::new(
                socket2::Domain::IPV4,
                socket2::Type::STREAM,
                None,
            )?;
            sock.set_reuse_address(true)?;
            sock.bind(&"127.0.0.1:9741".parse::<std::net::SocketAddr>()?.into())?;
            sock.listen(128)?;
            sock.set_nonblocking(true)?;
            let std_listener: std::net::TcpListener = sock.into();
            tokio::net::TcpListener::from_std(std_listener)?
        }
    };

    // Spawn Settings Dashboard over HTTP (always running for settings & setup)
    let cfg_server_clone = cfg_shared.clone();
    rt.spawn(async move {
        if let Err(e) = settings::start_server_with_listener(listener, cfg_server_clone).await {
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

    // ── Thread Architecture ────────────────────────────────────────────────
    // Main thread  : winit event loop (tray + overlay). EventLoop is !Send
    //                on Linux (contains X11 raw pointers) — MUST stay here.
    // Background   : tokio runtime (HTTP server + FSM). Runs on a spawned
    //                OS thread so the event loop can block the main thread.
    // ──────────────────────────────────────────────────────────────────────

    #[cfg(target_os = "linux")]
    if let Err(e) = gtk::init() {
        warn!("Failed to initialize GTK: {:?}", e);
    }

    // Cross-thread channel: GUI → daemon (UiCommand)
    let (ui_tx, ui_rx) = std::sync::mpsc::channel::<ui_bridge::UiCommand>();

    // Build the winit event loop on the main thread (mandatory — !Send on Linux).
    use winit::event_loop::ControlFlow;
    let event_loop = match winit::event_loop::EventLoop::<ui_bridge::DaemonEvent>::with_user_event().build() {
        Ok(el) => el,
        Err(e) => {
            warn!("Could not create GUI event loop: {} — daemon runs without overlay/tray", e);
            // Build a throw-away proxy synchronously (EventLoop is !Send — must
            // NOT be created inside an async block or moved to another thread).
            let dummy_proxy = winit::event_loop::EventLoop::<ui_bridge::DaemonEvent>
                ::with_user_event().build().unwrap().create_proxy();
            // Block main thread on the FSM; HTTP server is already spawned above.
            let _ = rt.block_on(app::run_with_ui(cfg, dummy_proxy, ui_rx));
            return Ok(());
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);

    let ui_proxy = event_loop.create_proxy();

    // Spawn the app FSM as a tokio task.
    rt.spawn(async move {
        if let Err(e) = app::run_with_ui(cfg, ui_proxy, ui_rx).await {
            error!(%e, "Fatal daemon error");
            std::process::exit(1);
        }
    });

    // Move the tokio runtime to a background thread so it keeps running
    // while the winit event loop blocks the main thread below.
    std::thread::Builder::new()
        .name("g-type-rt".into())
        .spawn(move || {
            rt.block_on(std::future::pending::<()>());
        })
        .expect("failed to spawn tokio runtime thread");

    // ── GUI event loop (main thread, blocking) ─────────────────────────────
    // Wrapped in catch_unwind so a wry/tray-icon/WebKitGTK panic does NOT
    // kill the process — the tokio background thread keeps running.
    let ui_tx_gui = ui_tx;
    let mut tray_mgr:    Option<tray::TrayManager>       = None;
    let mut overlay_mgr: Option<overlay::OverlayManager> = None;

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        #[allow(deprecated)]
        let _ = event_loop.run(move |event, elwt| {
            use winit::event::{Event, StartCause, WindowEvent};
            use crate::ui_bridge::{DaemonEvent, DaemonState};

            match event {
                Event::NewEvents(StartCause::Init) | Event::Resumed => {
                    if tray_mgr.is_none() {
                        match tray::TrayManager::new(ui_tx_gui.clone()) {
                            Ok(tm)  => { info!("Tray icon initialized"); tray_mgr = Some(tm); }
                            Err(e)  => error!("TrayManager init failed: {}", e),
                        }
                    }
                    if overlay_mgr.is_none() {
                        let attrs = winit::window::Window::default_attributes()
                            .with_title("G-Type Overlay")
                            .with_inner_size(winit::dpi::LogicalSize::new(320.0_f64, 56.0_f64))
                            .with_decorations(false)
                            .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                            .with_resizable(false)
                            .with_visible(false);
                        match elwt.create_window(attrs) {
                            Ok(window) => {
                                match overlay::OverlayManager::new(window, ui_tx_gui.clone()) {
                                    Ok(om)  => { info!("Overlay initialized"); overlay_mgr = Some(om); }
                                    Err(e)  => error!("OverlayManager init failed: {}", e),
                                }
                            }
                            Err(e) => error!("Overlay window creation failed: {}", e),
                        }
                    }
                }

                Event::AboutToWait => {
                    // Pump GTK so tray-icon renders on Linux.
                    #[cfg(target_os = "linux")]
                    while gtk::events_pending() {
                        gtk::main_iteration_do(false);
                    }
                }

                Event::UserEvent(daemon_event) => {
                    match daemon_event {
                        DaemonEvent::StateChanged(state) => {
                            match state {
                                DaemonState::Idle => {
                                    if let Some(t) = &tray_mgr    { t.set_idle(); }
                                    if let Some(o) = &overlay_mgr { let _ = o.set_idle(); }
                                }
                                DaemonState::Recording { profile } => {
                                    if let Some(t) = &tray_mgr    { t.set_recording(&profile); }
                                    if let Some(o) = &overlay_mgr { let _ = o.set_recording(); }
                                }
                                DaemonState::Processing { profile: _ } => {
                                    if let Some(t) = &tray_mgr    { t.set_processing(); }
                                    if let Some(o) = &overlay_mgr { let _ = o.set_processing(); }
                                }
                            }
                        }
                        DaemonEvent::ProfileActivated(_) => {}
                        DaemonEvent::ProfilesUpdated(_)  => {}
                        DaemonEvent::Error(err) => {
                            error!("UI error: {}", err);
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
    }));

    // If the event loop exits or panics, the tokio runtime thread keeps the
    // process alive. Park main thread indefinitely.
    std::thread::park();

    Ok(())
}

//! OpenSplit Tauri backend library entry point.
//!
//! Exposes `run()` which configures Tauri, registers PTY/IPC commands, parses
//! CLI args (for selecting a default profile), and opens the main window.

use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;

mod config;
mod ipc;
mod pty;
mod session;

use ipc::AppState;

/// CLI arguments understood by the `opensplit` binary.
#[derive(Debug, Parser)]
#[command(
    name = "opensplit",
    version,
    about = "Cross-platform terminal harness with right-click splits."
)]
struct Cli {
    /// Profile name to launch in the initial pane.
    ///
    /// Defaults to the config's `default_profile` (out of the box: `opencode`).
    profile: Option<String>,

    /// Run a raw command in the initial pane instead of using a profile.
    ///
    /// Everything after `--` is treated as the command + args.
    #[arg(last = true)]
    raw: Vec<String>,
}

/// Entry point invoked by `main.rs`.
pub fn run() {
    // Initialize structured logging. Honor `OPENSPLIT_LOG` (or `RUST_LOG`).
    let filter = EnvFilter::try_from_env("OPENSPLIT_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info,opensplit=debug"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();

    // Parse CLI. On a fresh GUI launch this is normally empty.
    let cli = Cli::parse();

    // Load or create the user config. On error, fall back to defaults so the
    // app still opens; the user will see a notice in-app eventually.
    let config = match config::Config::load_or_create() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("failed to load config, using defaults: {e:#}");
            config::Config::defaults()
        }
    };

    let initial_launch = config::resolve_initial_launch(&config, cli.profile.as_deref(), &cli.raw);
    tracing::info!(?initial_launch, "resolved initial launch spec");

    let state = Arc::new(AppState::new(config, initial_launch));

    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            ipc::get_initial_launch,
            ipc::list_profiles,
            ipc::spawn_pane,
            ipc::write_pane,
            ipc::resize_pane,
            ipc::close_pane,
            ipc::pane_foreground_info,
            ipc::resolve_split_spec,
        ])
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running OpenSplit");
}

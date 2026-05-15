//! Tauri command handlers + shared `AppState`.
//!
//! Frontend talks to backend exclusively through these `#[tauri::command]`
//! functions. PTY output flows the other way as `pane:data` events emitted
//! from the reader thread (see `pty.rs`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::{
    config::{self, Config, LaunchSpec},
    pty::{self, PaneRegistry},
    session,
};

/// Process-wide state managed by Tauri.
pub struct AppState {
    pub config: parking_lot::RwLock<Config>,
    pub initial_launch: LaunchSpec,
    pub panes: PaneRegistry,
}

impl AppState {
    pub fn new(config: Config, initial_launch: LaunchSpec) -> Self {
        Self {
            config: parking_lot::RwLock::new(config),
            initial_launch,
            panes: PaneRegistry::default(),
        }
    }
}

/// Wrapper to convert anyhow errors into something Tauri/serde-friendly.
fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------------------------------------------------------------------------
// Read-only / metadata commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_initial_launch(state: State<'_, Arc<AppState>>) -> LaunchSpec {
    state.initial_launch.clone()
}

#[derive(Debug, Serialize)]
pub struct ProfileSummary {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

#[tauri::command]
pub fn list_profiles(state: State<'_, Arc<AppState>>) -> Vec<ProfileSummary> {
    let cfg = state.config.read();
    let mut out: Vec<ProfileSummary> = cfg
        .profiles
        .iter()
        .map(|(name, p)| ProfileSummary {
            name: name.clone(),
            command: p.command.clone(),
            args: p.args.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

// ---------------------------------------------------------------------------
// PTY lifecycle
// ---------------------------------------------------------------------------

/// Arguments for `spawn_pane`.
#[derive(Debug, Deserialize)]
pub struct SpawnPaneArgs {
    /// One of:
    /// - `{ kind: "profile", name }`     → look up profile by name
    /// - `{ kind: "spec", spec }`        → use a pre-resolved spec (used for SSH inheritance)
    /// - `{ kind: "initial" }`           → use the spec resolved from CLI at startup
    pub source: SpawnSource,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpawnSource {
    Initial,
    Profile { name: String },
    Spec { spec: LaunchSpec },
}

#[derive(Debug, Serialize)]
pub struct SpawnPaneResult {
    pub pane_id: String,
    /// Echoed back so the frontend can label the pane.
    pub spec: LaunchSpec,
}

#[tauri::command]
pub fn spawn_pane(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    args: SpawnPaneArgs,
) -> Result<SpawnPaneResult, String> {
    let spec = match args.source {
        SpawnSource::Initial => state.initial_launch.clone(),
        SpawnSource::Profile { name } => {
            let cfg = state.config.read();
            config::profile_to_spec(&cfg, &name)
                .ok_or_else(|| format!("no such profile: {name}"))?
        }
        SpawnSource::Spec { spec } => spec,
    };
    let pane = pty::spawn(&app, spec.clone(), args.cols.max(1), args.rows.max(1)).map_err(err)?;
    let id = pane.id.clone();
    state.panes.insert(pane);
    Ok(SpawnPaneResult { pane_id: id, spec })
}

#[derive(Debug, Deserialize)]
pub struct WritePaneArgs {
    pub pane_id: String,
    /// Raw bytes from xterm.js. Treated as UTF-8.
    pub data: String,
}

#[tauri::command]
pub fn write_pane(
    state: State<'_, Arc<AppState>>,
    args: WritePaneArgs,
) -> Result<(), String> {
    let pane = pty::require(&state.panes, &args.pane_id).map_err(err)?;
    pane.write(args.data.as_bytes()).map_err(err)
}

#[derive(Debug, Deserialize)]
pub struct ResizePaneArgs {
    pub pane_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[tauri::command]
pub fn resize_pane(
    state: State<'_, Arc<AppState>>,
    args: ResizePaneArgs,
) -> Result<(), String> {
    let pane = pty::require(&state.panes, &args.pane_id).map_err(err)?;
    pane.resize(args.cols.max(1), args.rows.max(1)).map_err(err)
}

#[derive(Debug, Deserialize)]
pub struct ClosePaneArgs {
    pub pane_id: String,
}

#[tauri::command]
pub fn close_pane(
    state: State<'_, Arc<AppState>>,
    args: ClosePaneArgs,
) -> Result<(), String> {
    if let Some(pane) = state.panes.remove(&args.pane_id) {
        let _ = pane.kill();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SSH inheritance helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PaneForegroundArgs {
    pub pane_id: String,
}

#[tauri::command]
pub fn pane_foreground_info(
    state: State<'_, Arc<AppState>>,
    args: PaneForegroundArgs,
) -> Result<Option<session::ForegroundInfo>, String> {
    let pane = pty::require(&state.panes, &args.pane_id).map_err(err)?;
    let pid = match pane.child_pid() {
        Some(p) => p,
        None => return Ok(None),
    };
    Ok(session::foreground(pid))
}

#[derive(Debug, Deserialize)]
pub struct ResolveSplitSpecArgs {
    /// The pane being split. Used to detect a live SSH session.
    pub source_pane_id: String,
    /// Optional profile to fall back to if SSH isn't detected (or inherit is off).
    pub fallback_profile: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolveSplitSpecResult {
    pub spec: LaunchSpec,
    /// True if we ended up inheriting SSH from the source.
    pub inherited_ssh: bool,
    /// Foreground info we used to decide, for UI display.
    pub source_foreground: Option<session::ForegroundInfo>,
}

#[tauri::command]
pub fn resolve_split_spec(
    state: State<'_, Arc<AppState>>,
    args: ResolveSplitSpecArgs,
) -> Result<ResolveSplitSpecResult, String> {
    let cfg = state.config.read();
    let inherit_enabled = cfg.ssh_inherit;

    // Build the fallback spec first.
    let fallback = if let Some(name) = args.fallback_profile.as_deref() {
        config::profile_to_spec(&cfg, name).unwrap_or_else(|| state.initial_launch.clone())
    } else {
        // Try to reuse the source pane's spec; else initial.
        state
            .panes
            .get(&args.source_pane_id)
            .map(|p| p.spec().clone())
            .unwrap_or_else(|| state.initial_launch.clone())
    };

    if !inherit_enabled {
        return Ok(ResolveSplitSpecResult {
            spec: fallback,
            inherited_ssh: false,
            source_foreground: None,
        });
    }

    // Detect foreground process in source.
    let fg = state
        .panes
        .get(&args.source_pane_id)
        .and_then(|p| p.child_pid())
        .and_then(session::foreground);

    let Some(fg) = fg else {
        return Ok(ResolveSplitSpecResult {
            spec: fallback,
            inherited_ssh: false,
            source_foreground: None,
        });
    };

    let inherited = fg.is_ssh;
    let spec = session::build_split_spec(&fg, fallback);
    Ok(ResolveSplitSpecResult {
        spec,
        inherited_ssh: inherited,
        source_foreground: Some(fg),
    })
}

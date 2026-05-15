//! PTY management.
//!
//! One `Pane` per terminal. We use `portable-pty` so the same code drives
//! Windows ConPTY and Unix PTY. Each pane runs a reader thread that streams
//! bytes back to the frontend via a Tauri event.

use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::Arc,
    thread,
};

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::config::LaunchSpec;

/// Payload emitted to the frontend with PTY output for one pane.
#[derive(Debug, Clone, Serialize)]
struct PaneDataEvent {
    pane_id: String,
    /// UTF-8 lossy decoded chunk (xterm.js consumes strings).
    chunk: String,
}

/// Payload emitted when a pane's child process exits.
#[derive(Debug, Clone, Serialize)]
struct PaneExitEvent {
    pane_id: String,
    code: Option<i32>,
}

/// Owns the master side of a PTY and the child process. Writer is shared
/// because both the frontend (keystrokes) and resize can write to it.
pub struct Pane {
    pub id: String,
    /// `dyn MasterPty + Send` is not `Sync`; wrap in a `Mutex` so the whole
    /// `Pane` (and thus `AppState`) can be `Sync` for Tauri's `State<T>`.
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    /// PID of the spawned child, used for foreground-process detection.
    child_pid: Option<u32>,
    spec: LaunchSpec,
}

impl Pane {
    /// Send raw input bytes to the PTY (typically from a keystroke).
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut w = self.writer.lock();
        w.write_all(data).context("writing to pty")?;
        w.flush().ok();
        Ok(())
    }

    /// Inform the PTY about a new terminal size.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master
            .lock()
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("resizing pty")?;
        Ok(())
    }

    /// Kill the underlying child process.
    pub fn kill(&self) -> Result<()> {
        let mut c = self.child.lock();
        let _ = c.kill();
        Ok(())
    }

    /// PID of the immediate child (the shell or tool we spawned).
    pub fn child_pid(&self) -> Option<u32> {
        self.child_pid
    }

    pub fn spec(&self) -> &LaunchSpec {
        &self.spec
    }
}

/// Registry of all live panes, keyed by pane id (uuid v4 string).
#[derive(Default)]
pub struct PaneRegistry {
    inner: Mutex<HashMap<String, Arc<Pane>>>,
}

impl PaneRegistry {
    pub fn get(&self, id: &str) -> Option<Arc<Pane>> {
        self.inner.lock().get(id).cloned()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Pane>> {
        self.inner.lock().remove(id)
    }

    pub fn insert(&self, pane: Arc<Pane>) {
        self.inner.lock().insert(pane.id.clone(), pane);
    }
}

/// Spawn a brand-new PTY-backed process from `spec`.
///
/// Wires up a background reader thread that emits `pane:data` events to the
/// frontend, plus a child-waiter that emits `pane:exit` when the process ends.
pub fn spawn(app: &AppHandle, spec: LaunchSpec, cols: u16, rows: u16) -> Result<Arc<Pane>> {
    let pty_sys = native_pty_system();
    let pair = pty_sys
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("openpty")?;

    let mut cmd = CommandBuilder::new(&spec.command);
    cmd.args(&spec.args);
    if let Some(cwd) = spec
        .cwd
        .clone()
        .or_else(|| dirs::home_dir().map(|p| p.display().to_string()))
    {
        cmd.cwd(cwd);
    }
    // Pass through current env, then layer profile-specified vars.
    for (k, v) in std::env::vars() {
        cmd.env(k, v);
    }
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    // Hint to terminal apps that we're a real xterm-256color terminal.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let child = pair
        .slave
        .spawn_command(cmd)
        .with_context(|| format!("spawning `{}`", spec.command))?;
    let child_pid = child.process_id();

    let mut reader = pair.master.try_clone_reader().context("clone reader")?;
    let writer = pair.master.take_writer().context("take writer")?;

    let id = Uuid::new_v4().to_string();
    let pane = Arc::new(Pane {
        id: id.clone(),
        master: Mutex::new(pair.master),
        writer: Arc::new(Mutex::new(writer)),
        child: Arc::new(Mutex::new(child)),
        child_pid,
        spec,
    });

    // Reader thread: drain master output → emit `pane:data`.
    {
        let app = app.clone();
        let pane_id = id.clone();
        thread::Builder::new()
            .name(format!("opensplit-pty-{}", &id[..8]))
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            // xterm.js wants a string. Use lossy UTF-8 so partial
                            // multi-byte sequences don't tank the stream; xterm
                            // handles incremental escape parsing on its end.
                            let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                            let _ = app.emit(
                                "pane:data",
                                PaneDataEvent {
                                    pane_id: pane_id.clone(),
                                    chunk,
                                },
                            );
                        }
                        Err(e) => {
                            tracing::debug!(pane = %pane_id, "pty reader error: {e}");
                            break;
                        }
                    }
                }
                tracing::debug!(pane = %pane_id, "pty reader exited");
            })
            .ok();
    }

    // Child waiter thread: emit `pane:exit` when process dies.
    {
        let app = app.clone();
        let pane_id = id.clone();
        let child_handle = pane.child.clone();
        thread::Builder::new()
            .name(format!("opensplit-wait-{}", &id[..8]))
            .spawn(move || {
                // `wait()` borrows mutably; we briefly own the lock for it.
                // portable-pty's `wait()` blocks until exit.
                let status = {
                    let mut guard = child_handle.lock();
                    guard.wait()
                };
                let code = status.ok().map(|s| s.exit_code() as i32);
                let _ = app.emit(
                    "pane:exit",
                    PaneExitEvent {
                        pane_id: pane_id.clone(),
                        code,
                    },
                );
                tracing::debug!(pane = %pane_id, ?code, "pty child exited");
            })
            .ok();
    }

    Ok(pane)
}

/// Convenience for callers that already have a `pane_id` and want to look up
/// or surface a typed error.
pub fn require<'a>(reg: &'a PaneRegistry, pane_id: &str) -> Result<Arc<Pane>> {
    reg.get(pane_id)
        .ok_or_else(|| anyhow!("unknown pane id: {pane_id}"))
}

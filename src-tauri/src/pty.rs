//! PTY management.
//!
//! One `Pane` per terminal. We use `portable-pty` so the same code drives
//! Windows ConPTY and Unix PTY. Each pane runs a reader thread that streams
//! bytes back to the frontend via a Tauri event.

use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use portable_pty::{
    native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize,
};
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
    /// Independent killer that does NOT share a lock with the child-waiter
    /// thread. Required to avoid a deadlock between `close_pane → kill()` and
    /// the waiter thread which holds `child` locked for the entire duration
    /// of `child.wait()`.
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
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
    ///
    /// Uses the independent `killer` handle (not `child`) because the waiter
    /// thread is permanently holding `child` locked inside `wait()`.
    pub fn kill(&self) -> Result<()> {
        let _ = self.killer.lock().kill();
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

    // Resolve the command against PATH (+PATHEXT on Windows) and, on Windows,
    // wrap batch files through `cmd.exe /c` since CreateProcess can't execute
    // .cmd/.bat directly.
    let (resolved_program, resolved_args) = resolve_program(&spec.command, &spec.args)
        .with_context(|| format!("resolving `{}`", spec.command))?;

    let mut cmd = CommandBuilder::new(&resolved_program);
    cmd.args(&resolved_args);
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

    tracing::debug!(
        program = %resolved_program,
        args = ?resolved_args,
        "spawning pty child"
    );

    let child = pair
        .slave
        .spawn_command(cmd)
        .with_context(|| {
            format!(
                "spawning `{}` (resolved to `{}`)",
                spec.command, resolved_program
            )
        })?;
    let child_pid = child.process_id();
    // Independent kill handle that doesn't share a lock with the waiter.
    let killer = child.clone_killer();

    let mut reader = pair.master.try_clone_reader().context("clone reader")?;
    let writer = pair.master.take_writer().context("take writer")?;

    let id = Uuid::new_v4().to_string();
    let pane = Arc::new(Pane {
        id: id.clone(),
        master: Mutex::new(pair.master),
        writer: Arc::new(Mutex::new(writer)),
        child: Arc::new(Mutex::new(child)),
        killer: Mutex::new(killer),
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

// ---------------------------------------------------------------------------
// Program resolution
// ---------------------------------------------------------------------------

/// Resolve `program` against the current `PATH`, returning a tuple of
/// `(program_to_invoke, full_args)`.
///
/// On Windows:
///   - Honors `PATHEXT` so bare names like `opencode` find `opencode.exe`,
///     `opencode.cmd`, etc.
///   - If the resolved file is a batch script (`.cmd` / `.bat`), wraps it via
///     `cmd.exe /c <path> <args...>` because `CreateProcess` (and therefore
///     `portable-pty`) cannot execute batch files directly.
///   - Also augments PATH with well-known per-user install locations that
///     interactive shells typically pick up via the user profile but that
///     GUI-launched processes may miss (e.g. `%APPDATA%\npm`).
///
/// On Unix: returns `(program, args)` unchanged — execve handles PATH for us.
fn resolve_program(program: &str, args: &[String]) -> Result<(String, Vec<String>)> {
    // Absolute / explicit-relative paths bypass PATH lookup entirely.
    let p = Path::new(program);
    if p.is_absolute() || program.contains('/') || program.contains('\\') {
        #[cfg(windows)]
        {
            if let Some(found) = which_windows(program) {
                return Ok(wrap_if_batch(found, args));
            }
        }
        return Ok((program.to_string(), args.to_vec()));
    }

    #[cfg(windows)]
    {
        if let Some(found) = which_windows(program) {
            return Ok(wrap_if_batch(found, args));
        }
        return Err(anyhow!(
            "could not find `{program}` on PATH (searched with PATHEXT). \
             Tip: ensure the directory containing it is on your system PATH, \
             not just your PowerShell profile — GUI apps don't see profile-added PATH entries."
        ));
    }

    #[cfg(not(windows))]
    {
        // execve will handle PATH lookup; just sanity-check it exists.
        if which_unix(program).is_some() {
            return Ok((program.to_string(), args.to_vec()));
        }
        return Err(anyhow!(
            "could not find `{program}` on PATH. \
             Tip: ensure the directory containing it is on PATH for GUI-launched processes."
        ));
    }
}

#[cfg(windows)]
fn wrap_if_batch(resolved: PathBuf, args: &[String]) -> (String, Vec<String>) {
    let is_batch = resolved
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        .unwrap_or(false);

    if is_batch {
        // Spawn: cmd.exe /d /c "<resolved>" <args...>
        // - /d skips per-user AutoRun (faster, more predictable)
        // - /c runs and exits
        let mut wrapped_args: Vec<String> =
            vec!["/d".into(), "/c".into(), resolved.display().to_string()];
        wrapped_args.extend(args.iter().cloned());
        let cmd_exe = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
        (cmd_exe, wrapped_args)
    } else {
        (resolved.display().to_string(), args.to_vec())
    }
}

/// Windows PATH+PATHEXT lookup.
///
/// Search order:
///   1. Each dir in `PATH`
///   2. Plus a curated list of common per-user install dirs that may not be
///      on the GUI-process PATH:
///        - `%APPDATA%\npm`      (npm global bin shims: .cmd / .ps1 / no-ext)
///        - `%USERPROFILE%\.bun\bin`
///        - `%USERPROFILE%\.cargo\bin`
///        - `%USERPROFILE%\scoop\shims`
///        - `%LOCALAPPDATA%\Programs\<program>` (some installers)
///
/// In each dir, try `<name>` as-is first, then each extension in `PATHEXT`.
#[cfg(windows)]
fn which_windows(program: &str) -> Option<PathBuf> {
    let pathext = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let exts: Vec<String> = pathext
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(path_var) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path_var));
    }
    // Augment with well-known user dirs (de-duplicated below).
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".bun").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
        dirs.push(home.join("scoop").join("shims"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Programs").join(program));
    }

    let mut seen = std::collections::HashSet::new();
    for dir in dirs {
        if !seen.insert(dir.clone()) {
            continue;
        }
        // Try as-is (program already has an extension, e.g. user typed
        // "opencode.exe") -- but only if it has a dotted extension.
        if Path::new(program).extension().is_some() {
            let cand = dir.join(program);
            if cand.is_file() {
                return Some(cand);
            }
        }
        // Try each PATHEXT extension.
        for ext in &exts {
            let cand = dir.join(format!("{program}{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn which_unix(program: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(program);
        if cand.is_file() {
            // Best-effort exec-bit check.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&cand) {
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Some(cand);
                    }
                    continue;
                }
            }
            return Some(cand);
        }
    }
    None
}

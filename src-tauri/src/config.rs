//! Config loading + profile resolution.
//!
//! Config lives at the platform-conventional config dir under `opensplit/`:
//!   - Windows: `%APPDATA%\opensplit\config.toml`
//!   - Linux:   `~/.config/opensplit/config.toml`
//!   - macOS:   `~/Library/Application Support/opensplit/config.toml`
//!
//! On first launch we write a sensible default file the user can edit.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Top-level config schema.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Profile to use when `opensplit` is launched with no arguments.
    #[serde(default = "default_profile_name")]
    pub default_profile: String,

    /// Inherit SSH session into newly-split panes when the source pane is in
    /// an `ssh` session.
    #[serde(default = "default_true")]
    pub ssh_inherit: bool,

    /// Named profiles.
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
}

fn default_profile_name() -> String {
    "opencode".to_string()
}

fn default_true() -> bool {
    true
}

/// One launchable command preset.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Profile {
    /// Executable to run. Resolved against PATH if not absolute.
    pub command: String,
    /// Arguments passed to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional working directory; defaults to user's home.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Extra environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Config {
    /// Reasonable defaults shipped on first run.
    pub fn defaults() -> Self {
        let mut profiles = HashMap::new();

        profiles.insert(
            "opencode".to_string(),
            Profile {
                command: "opencode".to_string(),
                args: vec![],
                cwd: None,
                env: HashMap::new(),
            },
        );
        profiles.insert(
            "codex".to_string(),
            Profile {
                command: "codex".to_string(),
                args: vec![],
                cwd: None,
                env: HashMap::new(),
            },
        );
        profiles.insert(
            "claude".to_string(),
            Profile {
                command: "claude".to_string(),
                args: vec![],
                cwd: None,
                env: HashMap::new(),
            },
        );
        profiles.insert(
            "shell".to_string(),
            Profile {
                command: default_shell(),
                args: default_shell_args(),
                cwd: None,
                env: HashMap::new(),
            },
        );

        Self {
            default_profile: "opencode".to_string(),
            ssh_inherit: true,
            profiles,
        }
    }

    /// Load from disk, creating a default file if none exists.
    pub fn load_or_create() -> Result<Self> {
        let path = config_path().context("could not determine config dir")?;
        if !path.exists() {
            let defaults = Self::defaults();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).ok();
            }
            let toml = toml::to_string_pretty(&defaults)
                .context("failed to serialize default config")?;
            let header = "# OpenSplit configuration.\n\
                          # Generated on first launch; edit freely.\n\
                          # See https://github.com/anomalyco/opensplit for docs.\n\n";
            fs::write(&path, format!("{header}{toml}"))
                .with_context(|| format!("writing default config to {}", path.display()))?;
            tracing::info!(path = %path.display(), "wrote default config");
            return Ok(defaults);
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let parsed: Config = toml::from_str(&text)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(parsed)
    }
}

/// Resolved spec ready to hand to the PTY layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: HashMap<String, String>,
    /// Profile name this was derived from, or `null` for raw commands.
    pub profile: Option<String>,
}

/// Decide what to launch in the very first pane based on CLI args + config.
pub fn resolve_initial_launch(
    config: &Config,
    cli_profile: Option<&str>,
    raw: &[String],
) -> LaunchSpec {
    if !raw.is_empty() {
        return LaunchSpec {
            command: raw[0].clone(),
            args: raw[1..].to_vec(),
            cwd: None,
            env: HashMap::new(),
            profile: None,
        };
    }
    let name = cli_profile.unwrap_or(&config.default_profile);
    if let Some(p) = config.profiles.get(name) {
        return LaunchSpec {
            command: p.command.clone(),
            args: p.args.clone(),
            cwd: p.cwd.clone(),
            env: p.env.clone(),
            profile: Some(name.to_string()),
        };
    }
    // Profile name didn't match; treat it as a raw command.
    LaunchSpec {
        command: name.to_string(),
        args: vec![],
        cwd: None,
        env: HashMap::new(),
        profile: None,
    }
}

/// Look up a profile by name and convert to a `LaunchSpec`. Returns `None` if
/// no such profile exists.
pub fn profile_to_spec(config: &Config, name: &str) -> Option<LaunchSpec> {
    config.profiles.get(name).map(|p| LaunchSpec {
        command: p.command.clone(),
        args: p.args.clone(),
        cwd: p.cwd.clone(),
        env: p.env.clone(),
        profile: Some(name.to_string()),
    })
}

fn config_path() -> Option<PathBuf> {
    let dir = dirs::config_dir()?;
    Some(dir.join("opensplit").join("config.toml"))
}

#[cfg(windows)]
fn default_shell() -> String {
    // Prefer PowerShell 7 if present, else Windows PowerShell, else cmd.
    if which("pwsh.exe").is_some() {
        "pwsh.exe".into()
    } else if which("powershell.exe").is_some() {
        "powershell.exe".into()
    } else {
        "cmd.exe".into()
    }
}

#[cfg(windows)]
fn default_shell_args() -> Vec<String> {
    Vec::new()
}

#[cfg(not(windows))]
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into())
}

#[cfg(not(windows))]
fn default_shell_args() -> Vec<String> {
    vec!["-l".into()]
}

#[cfg(windows)]
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[allow(dead_code)]
fn _unused(_: &Path) {}

//! wayclick — low-latency input sound engine (Rust rewrite).
//!
//! Subcommands:
//! - `run`  (default): launcher behavior — permission/config checks, then start
//!   the input listener. This is what `nix run .#wayclick` launches.
//! - `check`: headless self-check — load config, decode audio, list sounds.
//!   Proves "it really runs" without capturing input or needing root.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod app;
mod audio;
mod backend;
mod config;
mod domain;
mod pipeline;

#[derive(Parser)]
#[command(name = "wayclick", version, about = "Low-latency input sound engine")]
struct Cli {
    /// Config directory (defaults to the platform config dir).
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,

    /// Also play on trackpad/touchpad input.
    #[arg(long, global = true)]
    enable_trackpads: bool,

    /// Fixed audio buffer size (frames) for lower latency.
    #[arg(long, global = true)]
    buffer_frames: Option<u32>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the input listener (default).
    Run,
    /// Headless self-check: load config and decode audio.
    Check,
}

fn resolve_config_dir() -> PathBuf {
    let user = user_config_dir();
    // Fall back to the bundled defaults when the user has no config.json yet.
    if user.join("config.json").is_file() {
        user
    } else {
        default_config_dir().unwrap_or(user)
    }
}

fn user_config_dir() -> PathBuf {
    // Mirror the Python `platform_paths.config_dir`: mirrors the launcher's
    // platform-specific resolution. Keep in sync with src/platform_paths.py.
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home {
            return home
                .join("Library")
                .join("Application Support")
                .join("wayclick");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(home) = home {
            return home.join(".config").join("wayclick");
        }
    }
    PathBuf::from("config")
}

/// Locate the bundled default config dir (config.json + wavs). Checks the dev
/// `assets/default` next to the CWD, then the nix-installed `share/wayclick/config`
/// relative to the executable. Returns the first that has a `config.json`.
fn default_config_dir() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("assets/default")];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        candidates.push(dir.join("../share/wayclick/config"));
        candidates.push(dir.join("assets/default"));
    }
    candidates
        .into_iter()
        .find(|c| c.join("config.json").is_file())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config_dir = cli.config_dir.clone().unwrap_or_else(resolve_config_dir);

    let app = app::App::new(config_dir, cli.enable_trackpads, cli.buffer_frames);

    let code = match cli.command.unwrap_or(Command::Run) {
        Command::Run => {
            // Launcher-style guards (mirror wayclick.py): refuse root, require
            // input group on Linux, require config.json.
            if let Err(msg) = launcher_checks(&app) {
                tracing::error!("{msg}");
                eprintln!("wayclick: {msg}");
                1
            } else {
                app.run()
            }
        }
        Command::Check => app.check(),
    };

    std::process::exit(code);
}

/// Permission/config checks before starting the listener (launcher behavior).
#[cfg(target_os = "linux")]
fn launcher_checks(app: &app::App) -> Result<(), String> {
    let _ = app;
    if unsafe { libc::geteuid() } == 0 {
        return Err("do not run as root".into());
    }
    if !in_input_group() {
        return Err("user not in input group".into());
    }
    Ok(())
}

/// Check `input` group membership via `getgroups(2)` — no subprocess, works in
/// minimal containers where `groups`(1) may be absent. evdev's own open() probe
/// is the final authority on permission; this is a fast pre-flight guard.
#[cfg(target_os = "linux")]
fn in_input_group() -> bool {
    let gid = unsafe { libc::getgid() };
    let input_gid = match get_group_gid("input") {
        Some(g) => g,
        None => return false,
    };
    // The primary group counts.
    if gid == input_gid {
        return true;
    }
    // Supplementary groups.
    let mut count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
    if count <= 0 {
        return false;
    }
    let mut groups: Vec<libc::gid_t> = vec![0; count as usize];
    count = unsafe { libc::getgroups(count, groups.as_mut_ptr()) };
    groups.truncate(count.max(0) as usize);
    groups.contains(&input_gid)
}

#[cfg(target_os = "linux")]
fn get_group_gid(name: &str) -> Option<libc::gid_t> {
    let cname = std::ffi::CString::new(name).ok()?;
    unsafe {
        let ptr = libc::getgrnam(cname.as_ptr());
        if ptr.is_null() {
            return None;
        }
        Some((*ptr).gr_gid)
    }
}

#[cfg(not(target_os = "linux"))]
fn launcher_checks(_app: &app::App) -> Result<(), String> {
    Ok(())
}

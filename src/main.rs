//! wayclick CLI. `run` (default) starts the listener; `check` is a headless
//! self-test that decodes audio without touching the input devices.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use directories::ProjectDirs;

use wayclick::app;

#[derive(Parser)]
#[command(name = "wayclick", version, about = "Low-latency input sound engine")]
struct Cli {
    /// Config directory (defaults to the platform config dir).
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,

    /// Also play on trackpad/touchpad input (Linux only).
    #[arg(long, global = true)]
    enable_trackpads: bool,

    /// Fixed audio buffer size in frames for lower latency; 0 uses the
    /// device's own default buffer size.
    #[arg(long, global = true)]
    buffer_frames: Option<u32>,

    /// Output device to use, matched by (case-insensitive) name substring;
    /// defaults to the system output device.
    #[arg(long, global = true)]
    audio_device: Option<String>,

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

/// User config directory via the platform's standard location (XDG config /
/// Application Support / AppData).
fn user_config_dir() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("dev", "wayclick", "wayclick")?;
    Some(dirs.config_dir().to_path_buf())
}

/// Locate the bundled default config dir (dev `assets/default`, then the
/// nix-installed `share/wayclick/config` next to the executable).
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

fn resolve_config_dir() -> PathBuf {
    let user = user_config_dir();
    if let Some(user) = &user
        && user.join("config.json").is_file()
    {
        return user.clone();
    }
    default_config_dir()
        .or(user)
        .unwrap_or_else(|| PathBuf::from("config"))
}

/// Refuse to run as root; device access is decided by the actual open, not by
/// group membership.
#[cfg(target_os = "linux")]
fn launcher_checks() -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 {
        return Err("do not run as root".into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn launcher_checks() -> Result<(), String> {
    Ok(())
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config_dir = cli.config_dir.clone().unwrap_or_else(resolve_config_dir);
    let app = app::App::new(
        config_dir,
        cli.enable_trackpads,
        cli.buffer_frames,
        cli.audio_device,
    );

    let result = match cli.command.unwrap_or(Command::Run) {
        Command::Run => {
            if let Err(msg) = launcher_checks() {
                tracing::error!("{msg}");
                eprintln!("wayclick: {msg}");
                return ExitCode::FAILURE;
            }
            app.run()
        }
        Command::Check => app.check(),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "application failed");
            ExitCode::FAILURE
        }
    }
}

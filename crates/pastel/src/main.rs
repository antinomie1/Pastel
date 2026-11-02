//! The `pastel` command line.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use luminaria::backend::BackendOptions;
use pastel::app;
use pastel_config::{Action, Config};

const USAGE: &str = "\
usage: pastel [--config PATH] [--check] [--list-actions]

  --config PATH    read this file instead of ~/.config/pastel/config.toml
  --check          validate the config and exit
  --list-actions   print every action a key can be bound to

The backend comes from the environment: nested when inside a Wayland session, DRM
from a free VT; LUMINARIA_BACKEND=nested|drm|headless forces one.";

struct Args {
    config: Option<PathBuf>,
    check: bool,
}

/// `None` when the arguments already said everything there was to do.
fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args {
        config: None,
        check: false,
    };
    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--config" => args.config = Some(argv.next().ok_or("--config needs a path")?.into()),
            "--check" => args.check = true,
            "--list-actions" => {
                Action::names().for_each(|name| println!("{name}"));
                return Ok(None);
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(None);
            }
            other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
        }
    }
    Ok(Some(args))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pastel: {e}");
            return ExitCode::FAILURE;
        }
    };
    let path = args.config.or_else(Config::default_path);
    let loaded = match &path {
        Some(path) => Config::load(path),
        None => Config::parse(""),
    };
    if args.check {
        return match loaded {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pastel: {e}");
                ExitCode::FAILURE
            }
        };
    }
    // A broken config must not leave someone at a blank screen on login.
    let config = loaded.unwrap_or_else(|e| {
        eprintln!("pastel: {e}\npastel: using the defaults");
        Config::parse("").expect("the defaults are valid")
    });
    let deadline = std::env::var("PASTEL_EXIT_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map(|ms| Instant::now() + Duration::from_millis(ms));
    let options = match BackendOptions::from_env(None) {
        Ok(options) => options,
        Err(e) => {
            eprintln!("pastel: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ran = app::run(options, config, path, |state, display| {
        app::main_loop(state, display, deadline)
    });
    match ran {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pastel: {e}");
            ExitCode::FAILURE
        }
    }
}

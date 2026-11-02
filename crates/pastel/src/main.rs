//! The `pastel` command line.

use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use luminaria::backend::BackendOptions;
use pastel::app;
use pastel_config::{Action, Config};

const USAGE: &str = "\
usage: pastel [--config PATH] [--check] [--list-actions]
       pastel msg (ACTION... | --state | --subscribe)

  --config PATH    read this file instead of ~/.config/pastel/config.toml
  --check          validate the config and exit
  --list-actions   print every action a key can be bound to
  msg              talk to the running Pastel: run an action (`pastel msg
                   focus-workspace 2`), or print its state as JSON, once or
                   after every change

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
    if std::env::args().nth(1).as_deref() == Some("msg") {
        let words: Vec<String> = argv.skip(1).collect();
        message(&words)?;
        return Ok(None);
    }
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

/// `pastel msg`: one request to the running compositor, or a subscription.
fn message(words: &[String]) -> Result<(), String> {
    use pastel_ipc::{Client, Reply, Request, encode};
    let mut client = Client::connect().map_err(|e| format!("connecting to Pastel: {e}"))?;
    let request = match words {
        [] => return Err(format!("msg needs an action\n\n{USAGE}")),
        [flag] if flag == "--subscribe" => {
            let states = client.subscribe().map_err(|e| e.to_string())?;
            for state in states {
                print!("{}", encode(&state.map_err(|e| e.to_string())?));
            }
            return Ok(());
        }
        [flag] if flag == "--state" => Request::State,
        words => Request::Action(words.join(" ")),
    };
    match client.request(&request).map_err(|e| e.to_string())? {
        Reply::Ok => Ok(()),
        Reply::State(state) => {
            print!("{}", encode(&state));
            Ok(())
        }
        Reply::Error(e) => Err(e),
    }
}

/// fastfetch and the like name the WM after argv[0] of whoever owns the Wayland
/// socket, and only prettify the compositors they know. The command stays lowercase;
/// the name it shows is the proper one. Rust can't rewrite argv, so exec once with it.
fn present_as_pastel() {
    const NAME: &str = "Pastel";
    let mut argv = std::env::args_os();
    if argv.next().is_some_and(|arg0| arg0 == NAME) {
        return;
    }
    // The real path, not /proc/self/exe: comm comes from it, and `pkill pastel` reads comm.
    let e = match std::env::current_exe() {
        Ok(exe) => Command::new(exe).arg0(NAME).args(argv).exec(),
        Err(e) => e,
    };
    eprintln!("pastel: keeping the lowercase process name: {e}");
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
    if !args.check {
        present_as_pastel();
    }
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

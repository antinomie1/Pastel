//! X11 on demand, with a script standing in for xwayland-satellite: what it's given
//! and when it's started is what Pastel is responsible for.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use luminaria::backend::{BackendKind, BackendOptions};
use luminaria::core::Display;
use pastel::app;
use pastel::state::State;
use pastel_config::Config;

const STAYS: &str = "exec sleep 30";
const QUITS: &str = "exit 0";

/// A directory of its own, gone with the test.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("pastel-x11-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("scratch");
        Self(dir)
    }

    /// A satellite that notes each start, with whether its fds really arrived.
    fn satellite(&self, then: &str) -> PathBuf {
        let log = self.log();
        let script = format!(
            "#!/bin/sh\n\
             [ \"$2\" = --test-listenfd-support ] && exit 0\n\
             inherited=yes\n\
             [ -e /proc/$$/fd/$3 ] && [ -e /proc/$$/fd/$5 ] || inherited=no\n\
             echo \"$1 $2 $4 inherited=$inherited wayland=$WAYLAND_DISPLAY\" >> {}\n\
             {then}\n",
            log.display()
        );
        let path = self.0.join("satellite");
        fs::write(&path, script).expect("script");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    fn log(&self) -> PathBuf {
        self.0.join("started")
    }

    fn starts(&self) -> Vec<String> {
        let text = fs::read_to_string(self.log()).unwrap_or_default();
        text.lines().map(str::to_owned).collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// One at a time: a display number one test let go of is the next one's, and these
/// tests look at whether the sockets are there.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn run(
    config_text: &str,
    config_path: Option<PathBuf>,
    body: impl for<'a> FnOnce(&mut State<'a>, &'a Display),
) {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let config = Config::parse(config_text).expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, config_path, |state, display| {
        body(state, display);
        Ok(())
    })
    .expect("compositor runs");
}

fn with_satellite(path: &Path) -> String {
    format!(
        "[overlay]\nat-startup = false\n[xwayland]\npath = \"{}\"\n",
        path.display()
    )
}

fn turn(state: &mut State<'_>, display: &Display, ms: u64) {
    let until = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < until {
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

fn settle(state: &mut State<'_>, display: &Display, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

fn knock(name: &str) -> UnixStream {
    let number = name.trim_start_matches(':');
    UnixStream::connect(format!("/tmp/.X11-unix/X{number}")).expect("connect")
}

#[test]
fn turned_off_there_is_no_display_at_all() {
    let scratch = Scratch::new();
    let satellite = scratch.satellite(STAYS);
    let text = with_satellite(&satellite) + "enabled = false\n";
    run(&text, None, |state, _| {
        assert_eq!(state.x11_display(), None);
    });
    assert!(scratch.starts().is_empty());
}

#[test]
fn without_the_program_the_desktop_still_comes_up() {
    run(
        "[overlay]\nat-startup = false\n[xwayland]\npath = \"/nonexistent/satellite\"",
        None,
        |state, _| assert_eq!(state.x11_display(), None),
    );
}

#[test]
fn it_starts_with_the_first_client_and_gets_the_sockets() {
    let scratch = Scratch::new();
    let satellite = scratch.satellite(STAYS);
    run(&with_satellite(&satellite), None, |state, display| {
        let name = state.x11_display().expect("a display");
        turn(state, display, 100);
        assert!(scratch.starts().is_empty(), "nothing before a client");

        let _client = knock(&name);
        settle(state, display, || !scratch.starts().is_empty());
        let start = &scratch.starts()[0];
        assert!(
            start.starts_with(&format!("{name} -listenfd -listenfd")),
            "{start}"
        );
        assert!(start.contains("inherited=yes"), "{start}");
        assert!(
            start.ends_with(&format!("wayland={}", state.socket())),
            "{start}"
        );

        let _second = knock(&name);
        turn(state, display, 100);
        assert_eq!(scratch.starts().len(), 1, "one is enough");
    });
}

#[test]
fn programs_are_told_the_display() {
    let scratch = Scratch::new();
    let satellite = scratch.satellite(STAYS);
    let out = scratch.0.join("display");
    let text = format!(
        "autostart = [\"echo \\\"$DISPLAY\\\" > {}\"]\n{}",
        out.display(),
        with_satellite(&satellite)
    );
    run(&text, None, |state, display| {
        // Not just the file: the shell makes it before it writes to it.
        let written = || fs::read_to_string(&out).is_ok_and(|text| text.ends_with('\n'));
        settle(state, display, written);
        let seen = fs::read_to_string(&out).expect("output");
        assert_eq!(Some(seen.trim().to_owned()), state.x11_display());
    });
}

/// A connection stays in the backlog, so a satellite that can't start would
/// otherwise be started again at once, for ever.
#[test]
fn one_that_keeps_quitting_is_left_alone() {
    let scratch = Scratch::new();
    let satellite = scratch.satellite(QUITS);
    run(&with_satellite(&satellite), None, |state, display| {
        let name = state.x11_display().expect("a display");
        let _client = knock(&name);
        settle(state, display, || scratch.starts().len() >= 3);
        turn(state, display, 200);
        assert_eq!(scratch.starts().len(), 3);
    });
}

#[test]
fn a_reload_switches_it_and_cleans_up() {
    let scratch = Scratch::new();
    let satellite = scratch.satellite(STAYS);
    let path = scratch.0.join("config.toml");
    let on = with_satellite(&satellite);
    fs::write(&path, &on).expect("config");
    run(&on, Some(path.clone()), |state, display| {
        let name = state.x11_display().expect("a display");
        let number = name.trim_start_matches(':');
        let socket = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
        let lock = PathBuf::from(format!("/tmp/.X{number}-lock"));
        assert!(socket.exists() && lock.exists());

        fs::write(&path, on.clone() + "enabled = false\n").expect("config");
        settle(state, display, || !socket.exists());
        assert_eq!(state.x11_display(), None);
        assert!(!lock.exists());

        fs::write(&path, &on).expect("config");
        settle(state, display, || lock.exists());
        assert!(state.x11_display().is_some());
    });
}

#[test]
fn leaving_takes_the_sockets_along() {
    let scratch = Scratch::new();
    let satellite = scratch.satellite(STAYS);
    let mut socket = PathBuf::new();
    run(&with_satellite(&satellite), None, |state, _| {
        let name = state.x11_display().expect("a display");
        socket = format!("/tmp/.X11-unix/X{}", name.trim_start_matches(':')).into();
        assert!(socket.exists());
    });
    assert!(!socket.exists());
}

/// Just enough of the X11 protocol to put a window on screen.
fn x11_window(stream: &mut UnixStream) {
    // Little-endian, protocol 11.0, no authorisation.
    stream
        .write_all(&[b'l', 0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        .expect("setup");
    let mut head = [0u8; 8];
    stream.read_exact(&mut head).expect("setup reply");
    assert_eq!(head[0], 1, "the server refused");
    let words = u16::from_le_bytes([head[6], head[7]]) as usize;
    let mut body = vec![0u8; words * 4];
    stream.read_exact(&mut body).expect("setup body");
    let word = |at: usize| u32::from_le_bytes(body[at..at + 4].try_into().expect("four bytes"));
    let id_base = word(4);
    let vendor = u16::from_le_bytes([body[16], body[17]]) as usize;
    let formats = body[21] as usize;
    let screen = 32 + vendor.div_ceil(4) * 4 + formats * 8;
    let root = word(screen);
    let black = word(screen + 12);

    let mut create = vec![1u8, 0];
    create.extend_from_slice(&9u16.to_le_bytes());
    create.extend_from_slice(&id_base.to_le_bytes());
    create.extend_from_slice(&root.to_le_bytes());
    create.extend_from_slice(&[0, 0, 0, 0]); // x, y
    create.extend_from_slice(&200u16.to_le_bytes());
    create.extend_from_slice(&100u16.to_le_bytes());
    create.extend_from_slice(&[0, 0]); // no border
    create.extend_from_slice(&1u16.to_le_bytes()); // InputOutput
    create.extend_from_slice(&0u32.to_le_bytes()); // the parent's visual
    create.extend_from_slice(&2u32.to_le_bytes()); // CWBackPixel
    create.extend_from_slice(&black.to_le_bytes());
    stream.write_all(&create).expect("CreateWindow");
    let mut map = vec![8u8, 0, 2, 0];
    map.extend_from_slice(&id_base.to_le_bytes());
    stream.write_all(&map).expect("MapWindow");
}

/// The real thing, where it's installed: an X client's window has to arrive as a
/// window here. Set PASTEL_SATELLITE to use one that isn't on the PATH.
#[test]
fn a_real_x_client_gets_a_window() {
    let path = std::env::var("PASTEL_SATELLITE").unwrap_or("xwayland-satellite".into());
    let found = Command::new(&path)
        .args([":0", "--test-listenfd-support"])
        .status()
        .is_ok_and(|status| status.success());
    if !found {
        eprintln!("skipped: no {path}");
        return;
    }
    let text = format!("[overlay]\nat-startup = false\n[xwayland]\npath = \"{path}\"\n");
    run(&text, None, |state, display| {
        turn(state, display, 50); // the output has to arrive
        let name = state.x11_display().expect("a display");
        assert_eq!(state.windows_mapped(), 0);
        let mut stream = knock(&name);
        // The answer only comes once Xwayland is up, and that needs this loop.
        let (sender, arrived) = channel();
        let client = std::thread::spawn(move || {
            x11_window(&mut stream);
            sender.send(()).expect("the test is waiting");
            stream // stays open: a closed connection takes its windows along
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        while state.windows_mapped() == 0 {
            assert!(
                Instant::now() < deadline,
                "no window: {:?}",
                arrived.try_recv()
            );
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        }
        // Only an X server that answered the handshake gets this far.
        assert!(arrived.try_recv().is_ok(), "a window with no X client?");
        assert_eq!(state.windows_mapped(), 1);
        drop(client);
    });
}

//! X11, through xwayland-satellite: it speaks X to the left and is one more Wayland
//! client to the right, so its windows need nothing special here.
//!
//! On demand, as niri does: Pastel owns the display's sockets and waits on them, so
//! a session with no X client runs no Xwayland at all. The first connection starts
//! the satellite and hands it the sockets, connection still pending. Its exit comes
//! from a pidfd rather than a timer, so that idle stays idle.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, RawFd};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{SocketAddr, UnixListener};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use luminaria::core::{Display, FdWatch};
use pastel_config::XwaylandConfig;

/// A client knocked on one of the sockets.
pub const SOCKET_TAG: u64 = 2;
/// The satellite exited.
pub const EXIT_TAG: u64 = 3;

const SOCKET_DIR: &str = "/tmp/.X11-unix";
const MAX_DISPLAY: u32 = 64;
/// Dying sooner than this counts as failing to start. With a connection left in the
/// backlog, starting it again at once would go on for ever.
const SHORT_LIFE: Duration = Duration::from_secs(2);
const MAX_SHORT_LIVES: u32 = 3;

struct Running<'d> {
    _exit: FdWatch<'d>,
    _pidfd: File,
    child: Child,
    since: Instant,
}

pub struct Xwayland<'d> {
    waiting: Vec<FdWatch<'d>>,
    running: Option<Running<'d>>,
    display: &'d Display,
    path: String,
    socket: String,
    number: u32,
    unix: UnixListener,
    abstracted: UnixListener,
    short_lives: u32,
}

fn lock_path(number: u32) -> PathBuf {
    format!("/tmp/.X{number}-lock").into()
}

fn socket_path(number: u32) -> PathBuf {
    format!("{SOCKET_DIR}/X{number}").into()
}

/// The X convention: a file naming the owner's pid, in ten columns.
fn take_lock(number: u32) -> io::Result<()> {
    let path = lock_path(number);
    let created = OpenOptions::new().write(true).create_new(true).open(&path);
    let mut file = match created {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let owner = fs::read_to_string(&path)?;
            let pid: i32 = owner.trim().parse().unwrap_or(0);
            // SAFETY: signal 0 only asks whether the process exists.
            let alive = pid > 0
                && (unsafe { libc::kill(pid, 0) } == 0
                    || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM));
            if alive {
                return Err(e);
            }
            fs::remove_file(&path)?;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
        }
        other => other?,
    };
    writeln!(file, "{:>10}", std::process::id())
}

/// Both of a display's sockets, or nothing: X clients try the abstract one first.
fn claim(number: u32) -> io::Result<(UnixListener, UnixListener)> {
    take_lock(number)?;
    let bound = (|| {
        let name = format!("{SOCKET_DIR}/X{number}");
        let abstracted = UnixListener::bind_addr(&SocketAddr::from_abstract_name(&name)?)?;
        if fs::create_dir(SOCKET_DIR).is_ok() {
            fs::set_permissions(SOCKET_DIR, fs::Permissions::from_mode(0o1777))?;
        }
        // We hold the lock and the abstract name, so a file here is a dead server's.
        let _ = fs::remove_file(socket_path(number));
        Ok((UnixListener::bind(socket_path(number))?, abstracted))
    })();
    if bound.is_err() {
        let _ = fs::remove_file(lock_path(number));
    }
    bound
}

fn supports_listenfd(path: &str) -> bool {
    let status = Command::new(path)
        .args([":0", "--test-listenfd-support"])
        .env_remove("DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => true,
        Ok(_) => {
            eprintln!("pastel: {path} is too old to start on demand; no X11");
            false
        }
        Err(e) => {
            eprintln!("pastel: no X11 without {path}: {e}");
            false
        }
    }
}

impl<'d> Xwayland<'d> {
    /// Claim a display and wait for its first client. `None` costs only X11.
    pub fn start(display: &'d Display, config: &XwaylandConfig, socket: &str) -> Option<Self> {
        if !config.enabled || !supports_listenfd(&config.path) {
            return None;
        }
        let (number, (unix, abstracted)) =
            (0..MAX_DISPLAY).find_map(|n| claim(n).ok().map(|sockets| (n, sockets)))?;
        let mut this = Self {
            waiting: Vec::new(),
            running: None,
            display,
            path: config.path.clone(),
            socket: socket.to_owned(),
            number,
            unix,
            abstracted,
            short_lives: 0,
        };
        this.wait_for_client();
        Some(this)
    }

    /// What goes in `DISPLAY`.
    pub fn display_name(&self) -> String {
        format!(":{}", self.number)
    }

    fn wait_for_client(&mut self) {
        let fds = [self.unix.as_fd(), self.abstracted.as_fd()];
        let watches = fds.map(|fd| FdWatch::new(self.display, fd, SOCKET_TAG));
        self.waiting = watches.into_iter().filter_map(Result::ok).collect();
    }

    /// Someone connected. Stop looking, or the pending connection fires for ever.
    pub fn client_arrived(&mut self) {
        self.waiting.clear();
        if self.running.is_some() {
            return;
        }
        match self.launch() {
            Ok(running) => self.running = Some(running),
            Err(e) => eprintln!("pastel: {} failed to start: {e}", self.path),
        }
    }

    fn launch(&self) -> io::Result<Running<'d>> {
        let fds: [RawFd; 2] = [self.unix.as_raw_fd(), self.abstracted.as_raw_fd()];
        let mut command = Command::new(&self.path);
        command
            .arg(self.display_name())
            .args(["-listenfd", &fds[0].to_string()])
            .args(["-listenfd", &fds[1].to_string()])
            .env("WAYLAND_DISPLAY", &self.socket)
            .env_remove("DISPLAY")
            .stdin(Stdio::null());
        // SAFETY: only async-signal-safe calls, on fds this process owns.
        unsafe {
            command.pre_exec(move || {
                for fd in fds {
                    if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        let child = command.spawn()?;
        // SAFETY: no pointers are involved, and the result is checked.
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id() as libc::pid_t, 0) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: pidfd_open just returned this fd, and nothing else owns it.
        let pidfd = unsafe { File::from_raw_fd(raw as RawFd) };
        let exit = FdWatch::new(self.display, pidfd.as_fd(), EXIT_TAG)
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Running {
            _exit: exit,
            _pidfd: pidfd,
            child,
            since: Instant::now(),
        })
    }

    /// It went. Back to waiting, unless it keeps going as soon as it comes.
    pub fn exited(&mut self) {
        let Some(mut running) = self.running.take() else {
            return;
        };
        let _ = running.child.wait();
        if running.since.elapsed() >= SHORT_LIFE {
            self.short_lives = 0;
        } else {
            self.short_lives += 1;
        }
        if self.short_lives < MAX_SHORT_LIVES {
            self.wait_for_client();
        } else {
            eprintln!("pastel: {} keeps exiting; no X11 until a reload", self.path);
        }
    }
}

impl Drop for Xwayland<'_> {
    fn drop(&mut self) {
        if let Some(running) = &mut self.running {
            let _ = running.child.kill();
            let _ = running.child.wait();
        }
        let _ = fs::remove_file(socket_path(self.number));
        let _ = fs::remove_file(lock_path(self.number));
    }
}

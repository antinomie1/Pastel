//! Reloads the config when it changes on disk. inotify rather than a timer: idle
//! means idle, and a file nobody touches must not wake anything.

use std::ffi::{CString, OsStr};
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::os::fd::{AsFd, AsRawFd, FromRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use luminaria::core::{Display, Error, FdWatch};

/// The tag of the only `FdWatch` that reads a config.
pub const CONFIG_TAG: u64 = 1;

/// Finished writes only. `IN_MODIFY` would reload half a file, and rename and
/// create cover editors that replace the file rather than write into it.
const MASK: u32 = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE;
const HEADER: usize = size_of::<libc::inotify_event>();

pub struct ConfigWatch<'d> {
    _watch: FdWatch<'d>,
    inotify: File,
    /// What a change is called: the name, and what a symlink at it points to.
    names: Vec<PathBuf>,
}

fn add_watch(inotify: &File, dir: &Path) -> bool {
    let Ok(dir) = CString::new(dir.as_os_str().to_owned().into_vec()) else {
        return false;
    };
    // SAFETY: the fd is open and `dir` is a NUL-terminated path that outlives the call.
    unsafe { libc::inotify_add_watch(inotify.as_raw_fd(), dir.as_ptr(), MASK) >= 0 }
}

impl<'d> ConfigWatch<'d> {
    /// Watches the directories, not the file: an editor replacing it would leave
    /// a watch on the file following the deleted one. `None` if `path` has no
    /// directory to watch yet, which costs only the automatic reload.
    pub fn new(display: &'d Display, path: &Path) -> Result<Option<Self>, Error> {
        // SAFETY: no pointers; a negative result is checked below.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            return Ok(None);
        }
        // SAFETY: inotify_init1 just returned this fd, and nothing else owns it.
        let inotify = unsafe { File::from_raw_fd(fd) };
        let mut names = Vec::new();
        // Dotfile managers link the config to somewhere else entirely.
        for path in [Some(path.to_owned()), path.canonicalize().ok()]
            .into_iter()
            .flatten()
        {
            let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty());
            if add_watch(&inotify, dir.unwrap_or(Path::new("."))) {
                names.extend(path.file_name().map(PathBuf::from));
            }
        }
        if names.is_empty() {
            return Ok(None);
        }
        let watch = FdWatch::new(display, inotify.as_fd(), CONFIG_TAG)?;
        Ok(Some(Self {
            _watch: watch,
            inotify,
            names,
        }))
    }

    /// Everything that arrived, and whether any of it was the config. It has to
    /// be drained either way: the watch fires for as long as there is more.
    pub fn changed(&mut self) -> bool {
        let mut changed = false;
        let mut buffer = [0u8; 4096];
        loop {
            match self.inotify.read(&mut buffer) {
                Ok(0) => return changed,
                Ok(read) => changed |= self.mentions_config(&buffer[..read]),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => return changed, // WouldBlock: all of it
            }
        }
    }

    fn mentions_config(&self, mut events: &[u8]) -> bool {
        let mut found = false;
        while events.len() >= HEADER {
            // The kernel writes whole events, but with no promise of alignment here.
            // SAFETY: at least HEADER bytes remain, and every bit pattern is valid.
            let event: libc::inotify_event = unsafe {
                events
                    .as_ptr()
                    .cast::<libc::inotify_event>()
                    .read_unaligned()
            };
            let end = (HEADER + event.len as usize).min(events.len());
            let name = &events[HEADER..end];
            let name = name.split(|byte| *byte == 0).next().unwrap_or_default();
            found |= self
                .names
                .iter()
                .any(|known| known.as_os_str() == OsStr::from_bytes(name));
            events = &events[end..];
        }
        found
    }
}

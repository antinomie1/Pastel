//! A blocking client: one connection, one request at a time.

use crate::{Reply, Request, State, decode, encode, socket_path};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

/// A connection to Pastel.
pub struct Client {
    reader: BufReader<UnixStream>,
    line: String,
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

impl Client {
    /// Connects to [`socket_path`].
    pub fn connect() -> io::Result<Self> {
        let path = socket_path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no Pastel socket in the environment",
            )
        })?;
        Self::connect_to(&path)
    }

    /// Connects to the socket at `path`.
    pub fn connect_to(path: &Path) -> io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        Ok(Self {
            reader: BufReader::new(stream),
            line: String::new(),
        })
    }

    /// Sends `request` and waits for its reply.
    pub fn request(&mut self, request: &Request) -> io::Result<Reply> {
        self.reader
            .get_mut()
            .write_all(encode(request).as_bytes())?;
        self.read()
    }

    /// Subscribes; every [`Self::next_state`] then blocks until something changes.
    pub fn subscribe(mut self) -> io::Result<Subscription> {
        self.reader
            .get_mut()
            .write_all(encode(&Request::Subscribe).as_bytes())?;
        Ok(Subscription(self))
    }

    fn read(&mut self) -> io::Result<Reply> {
        self.line.clear();
        if self.reader.read_line(&mut self.line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        decode(&self.line).map_err(invalid)
    }
}

/// A connection that receives the state after every change.
pub struct Subscription(Client);

impl Subscription {
    /// Blocks for the next state. The first arrives at once.
    pub fn next_state(&mut self) -> io::Result<State> {
        match self.0.read()? {
            Reply::State(state) => Ok(state),
            other => Err(invalid(format!("expected a state, got {other:?}"))),
        }
    }
}

impl Iterator for Subscription {
    type Item = io::Result<State>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.next_state() {
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => None,
            other => Some(other),
        }
    }
}

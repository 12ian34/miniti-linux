//! Blocking client for the control socket, used by the CLI and by the
//! single-instance check at launch. Deliberately synchronous: the CLI has no
//! runtime and the launch path runs before Tauri exists.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use super::protocol::{Request, Response};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
/// Starting a managed meeting talks to the backend; give commands room.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

fn connect_path(path: &Path) -> std::io::Result<UnixStream> {
    let stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(CONNECT_TIMEOUT))?;
    stream.set_write_timeout(Some(CONNECT_TIMEOUT))?;
    Ok(stream)
}

/// Round-trip a `ping`; Ok only if a live miniti answers on `path`.
pub fn ping(path: &Path) -> std::io::Result<()> {
    let mut client = Client::from_stream(connect_path(path)?)?;
    let r = client.call_with_timeout(&Request::Ping, CONNECT_TIMEOUT)?;
    if r.ok {
        Ok(())
    } else {
        Err(std::io::Error::other(r.error.unwrap_or_default()))
    }
}

/// Connect to the running instance, if any. A connection that does not
/// answer a ping promptly counts as no instance (see `call_with_timeout`).
pub fn connect() -> std::io::Result<Client> {
    let stream = connect_path(&super::socket_path())?;
    let mut client = Client::from_stream(stream)?;
    let r = client.call_with_timeout(&Request::Ping, CONNECT_TIMEOUT)?;
    if !r.ok {
        return Err(std::io::Error::other(r.error.unwrap_or_default()));
    }
    Ok(client)
}

pub fn is_running() -> bool {
    ping(&super::socket_path()).is_ok()
}

impl Client {
    fn from_stream(stream: UnixStream) -> std::io::Result<Self> {
        let writer = stream.try_clone()?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
        })
    }

    pub fn call(&mut self, req: &Request) -> std::io::Result<Response> {
        self.call_with_timeout(req, CALL_TIMEOUT)
    }

    /// A wedged instance can accept the connection (the kernel does that) and
    /// never answer; callers that only probe use a short deadline.
    pub fn call_with_timeout(&mut self, req: &Request, timeout: Duration) -> std::io::Result<Response> {
        let mut line = serde_json::to_string(req).map_err(std::io::Error::other)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()?;
        self.reader.get_ref().set_read_timeout(Some(timeout))?;
        let mut reply = String::new();
        if self.reader.read_line(&mut reply)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "miniti closed the connection",
            ));
        }
        serde_json::from_str(reply.trim()).map_err(std::io::Error::other)
    }

    /// Stream snapshots until the server goes away or `on_line` returns false.
    pub fn subscribe(mut self, mut on_line: impl FnMut(&str) -> bool) -> std::io::Result<()> {
        let mut line = serde_json::to_string(&Request::Subscribe).map_err(std::io::Error::other)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()?;
        self.reader.get_ref().set_read_timeout(None)?;
        let mut buf = String::new();
        loop {
            buf.clear();
            if self.reader.read_line(&mut buf)? == 0 {
                return Ok(());
            }
            if !on_line(buf.trim()) {
                return Ok(());
            }
        }
    }
}

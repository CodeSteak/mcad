//! Local IPC between `mcad` processes of one user (desktop UI ↔ CLI, CLI daemon).
//!
//! * **Endpoints** are Unix sockets in a per-user directory ([`runtime_dir`]):
//!   `$MCAD_IPC_DIR` (old name `$CAD_IPC_DIR`, see [`env_var`]), else `$XDG_RUNTIME_DIR/mcad`,
//!   else `$TMPDIR/mcad-<uid>`. The directory is created with mode 0700 and must belong
//!   to the current user; sockets get mode 0600. An endpoint has a name (`ui-<pid>`,
//!   `daemon`), the socket is `<dir>/<name>.sock`.
//! * **Long paths:** a Unix socket path has at most [`MAX_SOCKET_PATH`] bytes (`sun_path`).
//!   If `<dir>/<name>.sock` is longer, the socket goes to the short substitute directory
//!   `/tmp/mcad-<uid>` (same checks: created 0700, must be a directory of the current user,
//!   no symlink) as `<hash>-<name>.sock`, `<hash>` = FNV-1a 64 of the canonical runtime
//!   directory (several long directories do not collide). Lock and log files stay in the
//!   runtime directory. [`socket_path_in`] gives the path, [`is_substitute`] tells which.
//! * **Frames**: `[u32 LE header length][u32 LE data length][header JSON][data bytes]`. The
//!   header is a [`Header`] with the protocol version `v` ([`IPC_VERSION`]), a message id, the
//!   kind (request, response, event), a method name, a JSON body and an optional error. The
//!   data part carries binary payloads (PNG screenshots, meshes) without base64. The header
//!   is padded with spaces to a multiple of 4 bytes, so the data part starts 4-byte aligned.
//! * **Request/response**: [`Conn::request`] sends a request and waits for the response with
//!   the same id; events that arrive meanwhile are kept for [`Conn::next_event`]. A server
//!   answers with [`Conn::respond`]/[`Conn::respond_err`] and may push [`Conn::send_event`] at
//!   any time.
//! * **Stale sockets**: a socket file whose server is gone (connect refused) is removed by
//!   [`Server::bind`], [`connect`] and [`list`]; binding a name that a live server holds fails
//!   with [`Error::InUse`]. A server binds under a temporary name and moves the socket to its
//!   path only once it listens, so a refused connection really means a stale file (and only
//!   the file that refused is removed).
//!
//! * **Lock files** ([`LockFile`]): an exclusive `flock` on `<dir>/<name>.lock` makes sure
//!   only one process serves an endpoint (CLI daemon: several CLI calls may start one at the
//!   same time). The file is removed when the lock is dropped.
//!
//! Only Unix is implemented; on other platforms every call returns [`Error::Unsupported`].

use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use cad_i18n::{tr, trs};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// Environment variable `MCAD_<name>`, else the old name `CAD_<name>` (before the rename to
/// `mcad`, still read); empty values count as unset. `name` without prefix, e.g. `"IPC_DIR"`.
pub fn env_var_os(name: &str) -> Option<std::ffi::OsString> {
    [format!("MCAD_{name}"), format!("CAD_{name}")]
        .into_iter()
        .find_map(|k| std::env::var_os(k).filter(|v| !v.is_empty()))
}

/// [`env_var_os`] as a string (not valid Unicode: unset).
pub fn env_var(name: &str) -> Option<String> {
    env_var_os(name).and_then(|v| v.into_string().ok())
}

/// Version of the frame and header format. A peer with another version gets an error.
pub const IPC_VERSION: u32 = 1;

/// Largest header accepted (bytes).
pub const MAX_HEADER: usize = 16 << 20;
/// Largest data part accepted (bytes).
pub const MAX_DATA: usize = 512 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Request,
    Response,
    Event,
}

/// Header of a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    /// Protocol version ([`IPC_VERSION`]).
    pub v: u32,
    /// Request id (a response repeats it; events: 0).
    pub id: u64,
    pub kind: Kind,
    /// Method (request) or event name; responses repeat the request's method.
    pub method: String,
    #[serde(default)]
    pub body: Json,
    /// Set on a failed request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One message: header and binary data.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub header: Header,
    pub data: Vec<u8>,
}

impl Frame {
    pub fn request(id: u64, method: &str, body: Json) -> Frame {
        Frame {
            header: Header { v: IPC_VERSION, id, kind: Kind::Request, method: method.into(), body, error: None },
            data: Vec::new(),
        }
    }

    pub fn event(method: &str, body: Json) -> Frame {
        Frame {
            header: Header { v: IPC_VERSION, id: 0, kind: Kind::Event, method: method.into(), body, error: None },
            data: Vec::new(),
        }
    }

    pub fn response_to(req: &Header, body: Json) -> Frame {
        Frame {
            header: Header {
                v: IPC_VERSION,
                id: req.id,
                kind: Kind::Response,
                method: req.method.clone(),
                body,
                error: None,
            },
            data: Vec::new(),
        }
    }

    pub fn with_data(mut self, data: Vec<u8>) -> Frame {
        self.data = data;
        self
    }

    pub fn body(&self) -> &Json {
        &self.header.body
    }
}

#[derive(Debug)]
pub enum Error {
    /// No server at this endpoint (missing or stale socket, the stale file was removed).
    NotRunning(PathBuf),
    /// A live server holds the endpoint.
    InUse(PathBuf),
    /// The peer speaks another protocol version.
    Version {
        ours: u32,
        theirs: u32,
    },
    /// Malformed frame.
    Protocol(String),
    /// The server answered with an error.
    Remote(String),
    /// No answer within the timeout.
    Timeout,
    /// The socket directory is not safe (owner, permissions).
    Unsafe(String),
    Unsupported,
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotRunning(p) => write!(f, "{}", tr!("kein Prozess an {}", "no process at {}", p.display())),
            Error::InUse(p) => write!(f, "{}", tr!("{} ist belegt", "{} is in use", p.display())),
            Error::Version { ours, theirs } => {
                write!(f, "{}", tr!("IPC-Version {theirs}, erwartet {ours}", "IPC version {theirs}, expected {ours}"))
            }
            Error::Protocol(m) => write!(f, "{}", tr!("IPC-Protokoll: {m}", "IPC protocol: {m}")),
            Error::Remote(m) => write!(f, "{m}"),
            Error::Timeout => f.write_str(trs!("keine Antwort (Zeitüberschreitung)", "no answer (timeout)")),
            Error::Unsafe(m) => write!(f, "{}", tr!("unsicheres IPC-Verzeichnis: {m}", "unsafe IPC directory: {m}")),
            Error::Unsupported => f.write_str(trs!(
                "lokale IPC ist auf dieser Plattform nicht umgesetzt",
                "local IPC is not implemented on this platform"
            )),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
            Error::Timeout
        } else {
            Error::Io(e)
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// ------------------------------------------------------------------------------------------
// frames
// ------------------------------------------------------------------------------------------

/// Writes one frame.
pub fn write_frame(w: &mut impl Write, f: &Frame) -> Result<()> {
    let mut header = serde_json::to_vec(&f.header).map_err(|e| Error::Protocol(e.to_string()))?;
    // Pad with spaces so the data part starts 4-byte aligned (typed array views in JS).
    while !header.len().is_multiple_of(4) {
        header.push(b' ');
    }
    if header.len() > MAX_HEADER || f.data.len() > MAX_DATA {
        return Err(Error::Protocol(tr!("Nachricht zu groß", "message too large")));
    }
    let mut buf = Vec::with_capacity(8 + header.len() + f.data.len());
    buf.extend_from_slice(&(header.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(f.data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&header);
    buf.extend_from_slice(&f.data);
    w.write_all(&buf)?;
    w.flush()?;
    Ok(())
}

/// Reads one frame. `Ok(None)`: the peer closed the connection before a new frame.
pub fn read_frame(r: &mut impl Read) -> Result<Option<Frame>> {
    let mut len = [0u8; 8];
    let mut got = 0;
    while got < 8 {
        let n = r.read(&mut len[got..])?;
        if n == 0 {
            if got == 0 {
                return Ok(None);
            }
            return Err(Error::Protocol(tr!(
                "Verbindung mitten im Rahmen beendet",
                "connection closed in the middle of a frame"
            )));
        }
        got += n;
    }
    let hl = u32::from_le_bytes(len[0..4].try_into().unwrap()) as usize;
    let dl = u32::from_le_bytes(len[4..8].try_into().unwrap()) as usize;
    if hl > MAX_HEADER || dl > MAX_DATA {
        return Err(Error::Protocol(tr!("Rahmen zu groß ({hl} + {dl} Bytes)", "frame too large ({hl} + {dl} bytes)")));
    }
    let mut header = vec![0u8; hl];
    r.read_exact(&mut header)?;
    let mut data = vec![0u8; dl];
    r.read_exact(&mut data)?;
    // Check the version before the rest of the header (newer peers may add fields).
    let raw: Json = serde_json::from_slice(&header).map_err(|e| Error::Protocol(tr!("Kopf: {e}", "header: {e}")))?;
    let v = raw
        .get("v")
        .and_then(Json::as_u64)
        .ok_or_else(|| Error::Protocol(tr!("Kopf ohne Version", "header without version")))? as u32;
    if v != IPC_VERSION {
        return Err(Error::Version { ours: IPC_VERSION, theirs: v });
    }
    let header: Header = serde_json::from_value(raw).map_err(|e| Error::Protocol(tr!("Kopf: {e}", "header: {e}")))?;
    Ok(Some(Frame { header, data }))
}

// ------------------------------------------------------------------------------------------
// paths
// ------------------------------------------------------------------------------------------

/// Per-user socket directory (created with mode 0700; must belong to the current user).
pub fn runtime_dir() -> Result<PathBuf> {
    imp::runtime_dir()
}

/// Longest Unix socket path in bytes (`sizeof(sun_path)` minus the terminating NUL: 108 on
/// Linux, 104 on macOS and the BSDs).
#[cfg(target_os = "linux")]
pub const MAX_SOCKET_PATH: usize = 107;
#[cfg(not(target_os = "linux"))]
pub const MAX_SOCKET_PATH: usize = 103;

fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err(Error::Protocol(tr!("ungültiger Endpunktname `{name}`", "invalid endpoint name `{name}`")));
    }
    Ok(())
}

/// Socket path of endpoint `name` (`[A-Za-z0-9._-]`), see [`socket_path_in`].
pub fn endpoint_path(name: &str) -> Result<PathBuf> {
    socket_path_in(&runtime_dir()?, name)
}

/// FNV-1a 64 of the canonical directory: prefix of substitute socket names.
fn dir_hash(dir: &Path) -> String {
    let canon = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in canon.as_os_str().as_encoded_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Socket path of endpoint `name` for runtime directory `dir`: `<dir>/<name>.sock`, or, if
/// that is longer than [`MAX_SOCKET_PATH`], `/tmp/mcad-<uid>/<hash>-<name>.sock` (see the
/// crate docs).
pub fn socket_path_in(dir: &Path, name: &str) -> Result<PathBuf> {
    check_name(name)?;
    let p = dir.join(format!("{name}.sock"));
    if p.as_os_str().len() <= MAX_SOCKET_PATH {
        return Ok(p);
    }
    let short = imp::short_dir()?;
    let q = short.join(format!("{}-{name}.sock", dir_hash(dir)));
    if q.as_os_str().len() > MAX_SOCKET_PATH {
        return Err(Error::Protocol(tr!(
            "Socketpfad zu lang ({} Byte, höchstens {MAX_SOCKET_PATH}): {}",
            "socket path too long ({} bytes, at most {MAX_SOCKET_PATH}): {}",
            p.as_os_str().len(),
            p.display()
        )));
    }
    Ok(q)
}

/// `true` if the socket of `name` in `dir` uses the short substitute path.
pub fn is_substitute(dir: &Path, name: &str) -> bool {
    dir.join(format!("{name}.sock")).as_os_str().len() > MAX_SOCKET_PATH
}

/// Live endpoints whose name starts with `prefix` (name, path), sorted by name. Stale sockets
/// are removed on the way.
pub fn list(prefix: &str) -> Result<Vec<(String, PathBuf)>> {
    list_in(&runtime_dir()?, prefix)
}

/// [`list`] in a given directory (and its substitute sockets, see the crate docs).
pub fn list_in(dir: &Path, prefix: &str) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    scan(dir, "", prefix, &mut out)?;
    // endpoints whose path would be too long live in the short directory
    if let Ok(short) = imp::short_dir()
        && short != dir
    {
        scan(&short, &format!("{}-", dir_hash(dir)), prefix, &mut out)?;
    }
    out.sort();
    Ok(out)
}

fn scan(dir: &Path, strip: &str, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for e in rd.flatten() {
        let p = e.path();
        let Some(name) = p
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".sock"))
            .and_then(|n| n.strip_prefix(strip))
        else {
            continue;
        };
        if !name.starts_with(prefix) {
            continue;
        }
        if imp::alive(&p) {
            out.push((name.to_string(), p));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------
// connections
// ------------------------------------------------------------------------------------------

/// A connection (either side).
pub struct Conn {
    stream: imp::Stream,
    next_id: u64,
    events: std::collections::VecDeque<Frame>,
}

impl Conn {
    /// Sets the read timeout (`None`: block).
    pub fn set_timeout(&self, t: Option<Duration>) -> Result<()> {
        imp::set_timeout(&self.stream, t)
    }

    /// A second handle on the same connection (e.g. a writer for events).
    pub fn try_clone(&self) -> Result<Conn> {
        Ok(Conn { stream: imp::try_clone(&self.stream)?, next_id: self.next_id, events: Default::default() })
    }

    pub fn send(&mut self, f: &Frame) -> Result<()> {
        write_frame(&mut self.stream, f)
    }

    /// Next frame (`Ok(None)`: closed).
    pub fn recv(&mut self) -> Result<Option<Frame>> {
        read_frame(&mut self.stream)
    }

    /// Sends a request and waits for its response (events meanwhile are queued).
    pub fn request(&mut self, method: &str, body: Json) -> Result<Frame> {
        self.request_with(method, body, Vec::new())
    }

    pub fn request_with(&mut self, method: &str, body: Json, data: Vec<u8>) -> Result<Frame> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&Frame::request(id, method, body).with_data(data))?;
        loop {
            let Some(f) = self.recv()? else {
                return Err(Error::Protocol(tr!(
                    "Verbindung ohne Antwort beendet",
                    "connection closed without an answer"
                )));
            };
            match f.header.kind {
                Kind::Event => self.events.push_back(f),
                Kind::Response if f.header.id == id => {
                    return match &f.header.error {
                        Some(e) => Err(Error::Remote(e.clone())),
                        None => Ok(f),
                    };
                }
                _ => {}
            }
        }
    }

    /// Next event (queued ones first).
    pub fn next_event(&mut self) -> Result<Option<Frame>> {
        if let Some(f) = self.events.pop_front() {
            return Ok(Some(f));
        }
        loop {
            match self.recv()? {
                None => return Ok(None),
                Some(f) if f.header.kind == Kind::Event => return Ok(Some(f)),
                Some(_) => {}
            }
        }
    }

    pub fn respond(&mut self, req: &Header, body: Json, data: Vec<u8>) -> Result<()> {
        self.send(&Frame::response_to(req, body).with_data(data))
    }

    pub fn respond_err(&mut self, req: &Header, msg: &str) -> Result<()> {
        let mut f = Frame::response_to(req, Json::Null);
        f.header.error = Some(msg.to_string());
        self.send(&f)
    }

    pub fn send_event(&mut self, method: &str, body: Json) -> Result<()> {
        self.send(&Frame::event(method, body))
    }
}

/// Connects to a socket path. A missing or stale socket gives [`Error::NotRunning`] (a stale
/// file is removed).
pub fn connect_path(path: &Path) -> Result<Conn> {
    Ok(Conn { stream: imp::connect(path)?, next_id: 0, events: Default::default() })
}

/// Connects to endpoint `name`.
pub fn connect(name: &str) -> Result<Conn> {
    connect_path(&endpoint_path(name)?)
}

/// A listening endpoint; the socket file is removed on drop.
pub struct Server {
    listener: imp::Listener,
    path: PathBuf,
}

impl Server {
    /// Binds endpoint `name` (removes a stale socket; a live one gives [`Error::InUse`]).
    pub fn bind(name: &str) -> Result<Server> {
        Server::bind_path(&endpoint_path(name)?)
    }

    pub fn bind_path(path: &Path) -> Result<Server> {
        Ok(Server { listener: imp::bind(path)?, path: path.to_path_buf() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Waits for the next client.
    pub fn accept(&self) -> Result<Conn> {
        Ok(Conn { stream: imp::accept(&self.listener)?, next_id: 0, events: Default::default() })
    }

    /// Serves every connection in its own thread: each request goes to `handler`, which
    /// answers with (body, data) or an error message. Runs until the listener fails.
    pub fn serve<F>(self, handler: F) -> Result<()>
    where
        F: Fn(&Frame) -> std::result::Result<(Json, Vec<u8>), String> + Send + Sync + 'static,
    {
        let handler = std::sync::Arc::new(handler);
        loop {
            let mut conn = self.accept()?;
            let h = handler.clone();
            std::thread::spawn(move || {
                while let Ok(Some(f)) = conn.recv() {
                    if f.header.kind != Kind::Request {
                        continue;
                    }
                    let res = match h(&f) {
                        Ok((body, data)) => conn.respond(&f.header, body, data),
                        Err(e) => conn.respond_err(&f.header, &e),
                    };
                    if res.is_err() {
                        break;
                    }
                }
            });
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// An exclusive lock on a file (`flock`), held until drop; the file is removed on drop.
pub struct LockFile {
    file: std::fs::File,
    path: PathBuf,
}

impl LockFile {
    /// Takes the lock on `<runtime dir>/<name>.lock` if no other process holds it
    /// (`Ok(None)`: held by another process).
    pub fn try_lock(name: &str) -> Result<Option<LockFile>> {
        check_name(name)?;
        LockFile::try_lock_path(&runtime_dir()?.join(format!("{name}.lock")))
    }

    /// [`LockFile::try_lock`] on a path (created with mode 0600).
    pub fn try_lock_path(path: &Path) -> Result<Option<LockFile>> {
        Ok(imp::try_lock(path)?.map(|file| LockFile { file, path: path.to_path_buf() }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Replaces the file's content (e.g. the owner's pid).
    pub fn write_info(&mut self, text: &str) -> Result<()> {
        use std::io::Seek;
        self.file.set_len(0)?;
        self.file.seek(io::SeekFrom::Start(0))?;
        self.file.write_all(text.as_bytes())?;
        Ok(())
    }

    /// Content of a lock file (whoever holds it), if readable.
    pub fn read_info(path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        // Remove while still holding the lock; a process that opened the old file before
        // notices the changed inode after taking the lock and retries (see `imp::try_lock`).
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};

    pub type Stream = UnixStream;
    pub type Listener = UnixListener;

    fn uid() -> u32 {
        // SAFETY: getuid has no preconditions and cannot fail.
        unsafe { libc::getuid() }
    }

    pub fn runtime_dir() -> Result<PathBuf> {
        let dir = if let Some(d) = env_var_os("IPC_DIR") {
            PathBuf::from(d)
        } else if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
            PathBuf::from(d).join("mcad")
        } else {
            std::env::temp_dir().join(format!("mcad-{}", uid()))
        };
        private_dir(dir)
    }

    /// Short directory for sockets whose path in the runtime directory is too long.
    pub fn short_dir() -> Result<PathBuf> {
        private_dir(PathBuf::from(format!("/tmp/mcad-{}", uid())))
    }

    /// Creates `dir` (0700) and checks it: a directory (no symlink) of the current user.
    fn private_dir(dir: PathBuf) -> Result<PathBuf> {
        if !dir.exists() {
            use std::os::unix::fs::DirBuilderExt;
            match std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        match std::fs::create_dir_all(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let md = std::fs::symlink_metadata(&dir)?;
        if !md.is_dir() {
            return Err(Error::Unsafe(tr!("{} ist kein Verzeichnis", "{} is not a directory", dir.display())));
        }
        if md.uid() != uid() {
            return Err(Error::Unsafe(tr!(
                "{} gehört einem anderen Benutzer",
                "{} belongs to another user",
                dir.display()
            )));
        }
        if md.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(dir)
    }

    fn refused(e: &io::Error) -> bool {
        e.kind() == io::ErrorKind::ConnectionRefused
    }

    /// Inode of the file at `path` (not following a symlink).
    fn ino(path: &Path) -> Option<u64> {
        std::fs::symlink_metadata(path).ok().map(|m| m.ino())
    }

    /// Connects; a refused connection removes the socket file, but only the file that
    /// refused: a server that has meanwhile put its socket at `path` (see [`bind`]) keeps it.
    fn connect_or_remove(path: &Path) -> io::Result<UnixStream> {
        let before = ino(path);
        let r = UnixStream::connect(path);
        if let Err(e) = &r
            && refused(e)
            && before.is_some()
            && ino(path) == before
        {
            let _ = std::fs::remove_file(path);
        }
        r
    }

    pub fn alive(path: &Path) -> bool {
        connect_or_remove(path).is_ok()
    }

    pub fn connect(path: &Path) -> Result<UnixStream> {
        match connect_or_remove(path) {
            Ok(s) => Ok(s),
            Err(e) if refused(&e) => Err(Error::NotRunning(path.to_path_buf())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(Error::NotRunning(path.to_path_buf())),
            Err(e) => Err(e.into()),
        }
    }

    /// Binds the socket under a temporary name in the same directory, sets mode 0600 and
    /// only then moves it to `path`: the socket appears there when it already listens.
    /// Bound at `path` directly, a client connecting between `bind` and `listen` was refused,
    /// took the new socket for a stale one and removed it (the daemon then failed to start).
    pub fn bind(path: &Path) -> Result<UnixListener> {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        if path.exists() && alive(path) {
            return Err(Error::InUse(path.to_path_buf()));
        }
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = path.with_file_name(format!(".{}-{seq}.tmp", std::process::id()));
        if tmp.as_os_str().len() > MAX_SOCKET_PATH {
            // no room for a temporary name: bind in place
            let _ = std::fs::remove_file(path);
            let l = UnixListener::bind(path)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            return Ok(l);
        }
        let _ = std::fs::remove_file(&tmp);
        let l = UnixListener::bind(&tmp)?;
        let placed = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).and_then(|()| {
            // a new name: link, so that a server that came first keeps its socket
            match std::fs::hard_link(&tmp, path) {
                Ok(()) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if alive(path) {
                        Ok(false)
                    } else {
                        // stale socket: replace it atomically
                        std::fs::rename(&tmp, path).map(|()| true)
                    }
                }
                Err(e) => Err(e),
            }
        });
        let _ = std::fs::remove_file(&tmp);
        match placed {
            Ok(true) => Ok(l),
            Ok(false) => Err(Error::InUse(path.to_path_buf())),
            Err(e) => Err(e.into()),
        }
    }

    pub fn accept(l: &UnixListener) -> Result<UnixStream> {
        let (s, _) = l.accept()?;
        // Only the own user (the directory is 0700 anyway).
        let ok = peer_uid(&s).is_none_or(|u| u == uid());
        if !ok {
            return Err(Error::Unsafe(tr!("Verbindung eines anderen Benutzers", "connection of another user")));
        }
        Ok(s)
    }

    #[cfg(target_os = "linux")]
    fn peer_uid(s: &UnixStream) -> Option<u32> {
        use std::os::fd::AsRawFd;
        let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: valid fd, `cred` and `len` point to properly sized memory.
        let r = unsafe {
            libc::getsockopt(
                s.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut len,
            )
        };
        (r == 0).then_some(cred.uid)
    }

    #[cfg(not(target_os = "linux"))]
    fn peer_uid(_s: &UnixStream) -> Option<u32> {
        None
    }

    pub fn set_timeout(s: &UnixStream, t: Option<Duration>) -> Result<()> {
        s.set_read_timeout(t)?;
        Ok(())
    }

    pub fn try_clone(s: &UnixStream) -> Result<UnixStream> {
        Ok(s.try_clone()?)
    }

    pub fn try_lock(path: &Path) -> Result<Option<std::fs::File>> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::OpenOptionsExt;
        for _ in 0..16 {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(path)?;
            // SAFETY: valid fd.
            let r = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if r != 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
                    return Ok(None);
                }
                return Err(e.into());
            }
            // The holder before us may have removed the file after we opened it: then our
            // lock is on a deleted inode, try again with the current file.
            let ours = f.metadata()?;
            match std::fs::metadata(path) {
                Ok(m) if m.ino() == ours.ino() && m.dev() == ours.dev() => return Ok(Some(f)),
                _ => continue,
            }
        }
        Err(Error::Protocol(tr!("{}: Sperre wechselt ständig", "{}: lock keeps changing", path.display())))
    }
}

#[cfg(not(unix))]
mod imp {
    use super::*;

    pub struct Stream;
    pub struct Listener;

    impl Read for Stream {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("unsupported"))
        }
    }
    impl Write for Stream {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("unsupported"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    pub fn runtime_dir() -> Result<PathBuf> {
        Err(Error::Unsupported)
    }
    pub fn short_dir() -> Result<PathBuf> {
        Err(Error::Unsupported)
    }
    pub fn alive(_: &Path) -> bool {
        false
    }
    pub fn connect(_: &Path) -> Result<Stream> {
        Err(Error::Unsupported)
    }
    pub fn bind(_: &Path) -> Result<Listener> {
        Err(Error::Unsupported)
    }
    pub fn accept(_: &Listener) -> Result<Stream> {
        Err(Error::Unsupported)
    }
    pub fn set_timeout(_: &Stream, _: Option<Duration>) -> Result<()> {
        Err(Error::Unsupported)
    }
    pub fn try_clone(_: &Stream) -> Result<Stream> {
        Err(Error::Unsupported)
    }
    pub fn try_lock(_: &Path) -> Result<Option<std::fs::File>> {
        Err(Error::Unsupported)
    }
}

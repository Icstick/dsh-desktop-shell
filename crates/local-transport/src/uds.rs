//! Unix domain socket carrier (ADR-0022).
//!
//! UDS is the Unix twin of the Windows named pipe: it lets the server ask
//! the kernel which process connected (`SO_PEERCRED` on Linux,
//! `LOCAL_PEERPID` on macOS - see [crate::peer]). Framing, handshake,
//! credential supervision and deadlines are shared with the TCP carrier
//! through [crate::carrier]; this module only provides the endpoint.
//!
//! Design notes:
//! - No acceptor thread: unlike the Windows named pipe, `UnixListener`
//!   honours non-blocking accept, so the generic polling accept loop drives
//!   it directly and the accepted `UnixStream` is the carrier stream.
//! - The socket file is created `0600`: only the owning user can connect,
//!   so "same user" is the widest set of processes that can even reach the
//!   endpoint. The kernel identity is what narrows that down to the expected
//!   Shell binary. A parent directory this call creates is `0700` as well;
//!   an existing one is left alone (see [ensure_parent]).
//! - The socket file is removed on drop, and a stale socket left behind by a
//!   crashed daemon is replaced on bind. A non-socket file at the same path
//!   is never removed - replacing it would be a destructive surprise.
#![cfg(unix)]

use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use crate::carrier::{CarrierListener, PeerDesc};
use crate::limits::Limits;
use crate::peer::PeerIdentity;

/// Longest socket path this carrier accepts.
///
/// `sockaddr_un.sun_path` is 108 bytes on Linux and 104 on macOS, both
/// including the terminating NUL; the limit stays below both so a too-long
/// path fails early and explicitly instead of being truncated by the
/// kernel.
pub const MAX_SOCKET_PATH_BYTES: usize = 100;

/// Owner-only parent directory mode: nobody but the owning user can even
/// reach the socket.
const DIR_MODE: u32 = 0o700;

/// Owner-only socket mode: the filesystem gate in front of the kernel
/// identity check.
const SOCKET_MODE: u32 = 0o600;

/// Server-side Unix domain socket endpoint.
#[derive(Debug)]
pub struct UdsListener {
    listener: UnixListener,
    path: PathBuf,
}

impl UdsListener {
    /// Bind (start listening) on a filesystem socket path.
    ///
    /// Replaces a stale socket file (never a real file), restricts the socket
    /// to `0600` and puts the listener into non-blocking accept mode - the
    /// mode the generic accept loop polls. A missing parent directory is
    /// created owner-only; an existing one is used as it is (see
    /// [ensure_parent]).
    pub fn bind(path: impl AsRef<Path>, _limits: &Limits) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        ensure_path_fits(&path)?;
        ensure_parent(&path)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_socket() => {
                // Stale socket from a crashed daemon: bind would fail with
                // AddrInUse, so clear the endpoint we are about to own.
                fs::remove_file(&path)?;
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "refusing to replace {} with a socket: it is not a socket file",
                        path.display()
                    ),
                ));
            }
            Err(_) => {}
        }
        let listener = UnixListener::bind(&path)?;
        restrict(&path, SOCKET_MODE)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, path })
    }

    /// The bound socket path (published in the credential file).
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for UdsListener {
    fn drop(&mut self) {
        // The daemon publishes this path in the credential file; leaving the
        // file behind would advertise a dead endpoint to the next Shell.
        let _ = fs::remove_file(&self.path);
    }
}

impl CarrierListener for UdsListener {
    type Stream = UnixStream;

    fn set_nonblocking(&self) -> io::Result<()> {
        self.listener.set_nonblocking(true)
    }

    fn accept(&self) -> io::Result<(UnixStream, PeerDesc)> {
        let (stream, _addr) = self.listener.accept()?;
        Ok((
            stream,
            PeerDesc::UnixSocket(self.path.display().to_string()),
        ))
    }

    /// Kernel identity of the peer: `SO_PEERCRED` on Linux, `LOCAL_PEERPID`
    /// plus `proc_pidpath` on macOS.
    ///
    /// A platform that cannot report an identity yields `None`, which makes
    /// the connection identity-less: the daemon's policy layer then fails
    /// closed for control-plane authority (ADR-0022 decision 3). It is
    /// never silently treated as trusted.
    fn peer_identity(&self, stream: &UnixStream) -> io::Result<Option<PeerIdentity>> {
        Ok(crate::peer::peer_identity_of_unix_stream(stream).ok())
    }

    fn local_desc(&self) -> String {
        format!("uds:{}", self.path.display())
    }
}

/// Connect to a Unix domain socket as a client (the Shell side and tests).
pub fn connect(path: impl AsRef<Path>) -> io::Result<UnixStream> {
    UnixStream::connect(path)
}

fn restrict(path: &Path, mode: u32) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

/// Make sure the socket has a parent directory to live in.
///
/// A directory this call creates is owner-only (`0700`). An existing
/// directory is used as-is: its mode is not ours to change. The daemon data
/// directory is already `0700` by its own policy, while a shared directory
/// such as `/tmp` must never be re-moded by a library call - for a normal
/// user that fails outright, and as root it would quietly wreck someone
/// else's filesystem. The socket file itself is always `0600`, and that is
/// what actually decides who may connect.
fn ensure_parent(path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() || parent.exists() {
        return Ok(());
    }
    fs::create_dir_all(parent)?;
    restrict(parent, DIR_MODE)
}

/// Fail early on a path the kernel cannot represent in `sockaddr_un`.
fn ensure_path_fits(path: &Path) -> io::Result<()> {
    let len = path.as_os_str().as_bytes().len();
    if len > MAX_SOCKET_PATH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "socket path is {len} bytes, over the {MAX_SOCKET_PATH_BYTES}-byte carrier limit: {}",
                path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::thread;
    use std::time::{Duration, Instant};

    /// Unique socket path per test (temp dir + pid + tag), mirroring the
    /// named-pipe tests' naming.
    fn socket_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dsh-lt-uds-{}-{tag}.sock", std::process::id()))
    }

    fn connect_with_retry(path: &Path) -> UnixStream {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match connect(path) {
                Ok(stream) => return stream,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("client connect failed: {error}"),
            }
        }
    }

    fn accept_with_retry(listener: &UdsListener) -> UnixStream {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, desc)) => {
                    assert!(matches!(desc, PeerDesc::UnixSocket(_)));
                    return stream;
                }
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("accept failed: {error}"),
            }
        }
    }

    #[test]
    fn uds_round_trip_and_peer_identity() {
        let path = socket_path("round-trip");
        let _ = fs::remove_file(&path);
        let listener = UdsListener::bind(&path, &Limits::default()).expect("bind uds listener");

        // This test process is the client, so the kernel must report our own
        // pid - the point is that the OS reports a pid at all and that it
        // resolves to an image path (which is what the strict policy gates).
        let client = thread::spawn({
            let path = path.clone();
            move || connect_with_retry(&path)
        });
        let mut server_stream = accept_with_retry(&listener);
        let mut client_stream = client.join().expect("client thread");

        let identity = listener
            .peer_identity(&server_stream)
            .expect("identity probe")
            .expect("UDS must provide a peer identity");
        assert_eq!(identity.pid, std::process::id());
        assert!(
            identity.image_path.is_some(),
            "the peer image path must resolve on this platform"
        );

        client_stream
            .write_all(&crate::framing::encode_frame(b"hello-uds"))
            .expect("client write");
        let mut header = [0u8; 4];
        server_stream
            .read_exact(&mut header)
            .expect("server header");
        let len = u32::from_le_bytes(header) as usize;
        let mut payload = vec![0u8; len];
        server_stream
            .read_exact(&mut payload)
            .expect("server payload");
        assert_eq!(&payload, b"hello-uds");

        server_stream
            .write_all(&crate::framing::encode_frame(b"pong"))
            .expect("server write");
        client_stream
            .read_exact(&mut header)
            .expect("client header");
        let len = u32::from_le_bytes(header) as usize;
        let mut reply = vec![0u8; len];
        client_stream.read_exact(&mut reply).expect("client reply");
        assert_eq!(&reply, b"pong");
    }

    #[test]
    fn read_deadline_expires_as_would_block() {
        let path = socket_path("deadline");
        let _ = fs::remove_file(&path);
        let listener = UdsListener::bind(&path, &Limits::default()).expect("bind uds listener");
        let client = thread::spawn({
            let path = path.clone();
            move || connect_with_retry(&path)
        });
        let mut server_stream = accept_with_retry(&listener);
        server_stream
            .set_read_timeout(Some(Duration::from_millis(120)))
            .expect("set timeout");
        let started = Instant::now();
        let mut buf = [0u8; 8];
        let error = server_stream.read(&mut buf).expect_err("deadline expires");
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(started.elapsed() >= Duration::from_millis(100));
        drop(client);
    }

    /// Regression parity with the named pipe (2026-09-13 live QA): a closed
    /// peer must surface as EOF, never as "no data yet". The disconnect
    /// paths (credential re-issue, lease revocation, ownership release) all
    /// hang off the worker loop observing EOF.
    #[test]
    fn peer_close_surfaces_as_eof() {
        let path = socket_path("eof");
        let _ = fs::remove_file(&path);
        let listener = UdsListener::bind(&path, &Limits::default()).expect("bind uds listener");
        let client = connect_with_retry(&path);
        let mut server_stream = accept_with_retry(&listener);
        drop(client);

        server_stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set timeout");
        let mut buf = [0u8; 8];
        let read = server_stream.read(&mut buf).expect("read must not error");
        assert_eq!(read, 0, "a closed peer must surface as EOF, not a stall");
    }

    #[test]
    fn stale_socket_is_replaced_but_real_files_are_not() {
        let path = socket_path("stale");
        let _ = fs::remove_file(&path);
        // A leftover socket file (crashed daemon) is replaced.
        let first = UdsListener::bind(&path, &Limits::default()).expect("first bind");
        drop(first); // Drop removes the socket file; recreate a stale one.
        let stale = UnixListener::bind(&path).expect("stale socket");
        drop(stale);
        assert!(path.exists(), "the stale socket file is still on disk");
        let second =
            UdsListener::bind(&path, &Limits::default()).expect("rebind over the stale socket");
        drop(second);

        // A real file at the socket path is never destroyed.
        fs::write(&path, b"not a socket").expect("write decoy file");
        let error = UdsListener::bind(&path, &Limits::default()).expect_err("must refuse");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).expect("decoy survives"), b"not a socket");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn socket_is_owner_only_and_created_dirs_are_private() {
        let dir = std::env::temp_dir().join(format!("dsh-lt-uds-dir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("daemon.sock");
        let listener = UdsListener::bind(&path, &Limits::default()).expect("bind uds listener");
        let mode = |p: &Path| fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600, "the socket must be owner-only");
        assert_eq!(
            mode(path.parent().expect("parent")),
            0o700,
            "a directory this call creates must be owner-only"
        );
        drop(listener);
        assert!(!path.exists(), "drop removes the socket file");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Regression (2026-09-13, caught by the Linux test run): binding inside a
    /// shared, already-existing directory (the temp dir) must not try to
    /// re-mode that directory. Doing so fails with EPERM for a normal user,
    /// and as root it would quietly wreck a shared directory.
    #[test]
    fn an_existing_parent_directory_is_left_alone() {
        let parent = std::env::temp_dir().join(format!("dsh-lt-uds-shared-{}", std::process::id()));
        fs::create_dir_all(&parent).expect("create shared parent");
        restrict(&parent, 0o755).expect("relax the parent on purpose");
        let before = fs::metadata(&parent)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;

        let path = parent.join("daemon.sock");
        let listener = UdsListener::bind(&path, &Limits::default()).expect("bind in a shared dir");
        let after = fs::metadata(&parent)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(after, before, "the parent directory mode must not change");
        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
            0o600,
            "the socket stays owner-only even in a shared directory"
        );
        drop(listener);
        let _ = fs::remove_dir_all(&parent);
    }

    #[test]
    fn overlong_socket_path_is_rejected() {
        let long = format!("{}/{}", std::env::temp_dir().display(), "x".repeat(120));
        let error = UdsListener::bind(&long, &Limits::default()).expect_err("must reject");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}

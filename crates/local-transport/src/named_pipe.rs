//! Windows named pipe carrier (ADR-0022).
//!
//! Named pipes are the carrier that lets the server ask the kernel which
//! process connected (GetNamedPipeClientProcessId). The framing/handshake/
//! credential supervision is shared with the TCP carrier through
//! [crate::carrier] - this module only provides the endpoint.
//!
//! Design notes:
//! - One pipe INSTANCE per connection (the native model); the acceptor
//!   thread creates a fresh instance, blocks in ConnectNamedPipe, and hands
//!   the connected stream to a queue the generic accept loop polls.
//! - Shutdown wakes the blocking ConnectNamedPipe with a poison connection
//!   (the acceptor checks the shutdown flag and discards it) - no OVERLAPPED
//!   or CancelSynchronousIo machinery is needed.
//! - Read deadlines are applied inside PipeStream::read with PeekNamedPipe
//!   polling, surfacing WouldBlock exactly like a socket deadline expires;
//!   the supervision loop already treats that as a deadline expiry.
#![cfg(windows)]
// SAFETY CONTRACT (ADR-0022): the named-pipe endpoint is raw Win32 by
// nature. Handles are owned by PipeStream/NamedPipeListener for their whole
// lifetime, each unsafe block is a single documented call on such a handle,
// and no foreign pointer escapes the module.
#![allow(unsafe_code)]

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::carrier::{CarrierListener, CarrierStream, PeerDesc};
use crate::peer::PeerIdentity;

/// Raw Win32 handle value. Stored as isize (not a pointer) so streams and
/// listeners stay Send: Windows handles are process-wide and safe to move
/// between threads (only the owning module touches them).
type Handle = isize;
type Bool = i32;
type Dword = u32;

const INVALID_HANDLE_VALUE: Handle = -1isize;
const PIPE_ACCESS_DUPLEX: Dword = 0x0000_0003;
const PIPE_TYPE_BYTE: Dword = 0x0000_0000;
const PIPE_READMODE_BYTE: Dword = 0x0000_0000;
const PIPE_WAIT: Dword = 0x0000_0000;
const PIPE_UNLIMITED_INSTANCES: Dword = 255;
const GENERIC_READ: Dword = 0x8000_0000;
const GENERIC_WRITE: Dword = 0x4000_0000;
const OPEN_EXISTING: Dword = 3;
const ERROR_PIPE_CONNECTED: Dword = 535;
const ERROR_BROKEN_PIPE: Dword = 109;
const ERROR_PIPE_NOT_CONNECTED: Dword = 233;
const ERROR_NO_DATA: Dword = 232;

/// Poll interval while waiting for data inside a read deadline.
const READ_POLL_INTERVAL: Duration = Duration::from_millis(5);

unsafe extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: Dword,
        pipe_mode: Dword,
        max_instances: Dword,
        out_buffer: Dword,
        in_buffer: Dword,
        timeout: Dword,
        security: *mut std::ffi::c_void,
    ) -> Handle;
    fn ConnectNamedPipe(handle: Handle, overlapped: *mut std::ffi::c_void) -> Bool;
    fn DisconnectNamedPipe(handle: Handle) -> Bool;
    fn PeekNamedPipe(
        handle: Handle,
        buffer: *mut std::ffi::c_void,
        buffer_size: Dword,
        bytes_read: *mut Dword,
        total_available: *mut Dword,
        bytes_left: *mut Dword,
    ) -> Bool;
    fn ReadFile(
        handle: Handle,
        buffer: *mut u8,
        to_read: Dword,
        read: *mut Dword,
        overlapped: *mut std::ffi::c_void,
    ) -> Bool;
    fn WriteFile(
        handle: Handle,
        buffer: *const u8,
        to_write: Dword,
        written: *mut Dword,
        overlapped: *mut std::ffi::c_void,
    ) -> Bool;
    fn CreateFileW(
        name: *const u16,
        access: Dword,
        share: Dword,
        security: *mut std::ffi::c_void,
        disposition: Dword,
        flags: Dword,
        template: Handle,
    ) -> Handle;
    fn CloseHandle(handle: Handle) -> Bool;
    fn GetLastError() -> Dword;
}

fn wide(text: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

/// The pipe path form used by clients (\\.\pipe\<name>).
pub fn pipe_path(name: &str) -> String {
    format!("\\\\.\\pipe\\{name}")
}

/// One connected named-pipe instance, seen as a carrier stream.
#[derive(Debug)]
pub struct PipeStream {
    handle: Handle,
    read_timeout: Mutex<Option<Duration>>,
    write_timeout: Mutex<Option<Duration>>,
    peer: PeerDesc,
    identity: Option<PeerIdentity>,
}

impl PipeStream {
    fn new(handle: Handle, name: &str, identity: Option<PeerIdentity>) -> Self {
        Self {
            handle,
            read_timeout: Mutex::new(None),
            write_timeout: Mutex::new(None),
            peer: PeerDesc::NamedPipe(name.to_string()),
            identity,
        }
    }

    /// Where this stream came from (used by the listener queue).
    fn desc(&self) -> PeerDesc {
        self.peer.clone()
    }

    /// Kernel-provided client identity captured at accept time.
    fn taken_identity(&self) -> Option<PeerIdentity> {
        self.identity.clone()
    }

    /// Bytes ready to read, or `None` once the peer has closed.
    ///
    /// The distinction matters: a closed pipe must surface as EOF
    /// (Read::read == Ok(0)) so the supervision loop ends the connection -
    /// treating it as "no data yet" keeps the worker alive forever, which
    /// leaks the connection and suppresses the disconnect paths (credential
    /// re-issue, lease revocation; found by live QA on 2026-09-13).
    fn available_bytes(&self) -> io::Result<Option<Dword>> {
        let mut available: Dword = 0;
        let ok = unsafe {
            PeekNamedPipe(
                self.handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            if code == ERROR_BROKEN_PIPE
                || code == ERROR_PIPE_NOT_CONNECTED
                || code == ERROR_NO_DATA
            {
                return Ok(None);
            }
            return Err(last_error());
        }
        Ok(Some(available))
    }
}

impl Drop for PipeStream {
    fn drop(&mut self) {
        unsafe {
            DisconnectNamedPipe(self.handle);
            CloseHandle(self.handle);
        }
    }
}

impl Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let timeout = *self.read_timeout.lock().unwrap();
        let started = Instant::now();
        loop {
            let available = match self.available_bytes()? {
                // Peer closed: EOF, exactly like a socket read of length 0.
                None => return Ok(0),
                Some(available) => available,
            };
            if available == 0 {
                if let Some(limit) = timeout
                    && started.elapsed() >= limit
                {
                    // Deadline expiry, mirroring a socket read timeout.
                    return Err(io::Error::from(io::ErrorKind::WouldBlock));
                }
                thread::sleep(READ_POLL_INTERVAL);
                continue;
            }
            let want = (available as usize).min(buf.len()) as Dword;
            let mut got: Dword = 0;
            let ok = unsafe {
                ReadFile(
                    self.handle,
                    buf.as_mut_ptr(),
                    want,
                    &mut got,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(last_error());
            }
            if got == 0 {
                return Ok(0);
            }
            return Ok(got as usize);
        }
    }
}

impl Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let timeout = *self.write_timeout.lock().unwrap();
        let started = Instant::now();
        loop {
            let mut written: Dword = 0;
            let ok = unsafe {
                WriteFile(
                    self.handle,
                    buf.as_ptr(),
                    buf.len() as Dword,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok != 0 {
                return Ok(written as usize);
            }
            // Backpressure: retry until the write deadline expires.
            match timeout {
                Some(limit) if started.elapsed() >= limit => {
                    return Err(io::Error::from(io::ErrorKind::WouldBlock));
                }
                Some(_) => thread::sleep(READ_POLL_INTERVAL),
                None => return Err(last_error()),
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl CarrierStream for PipeStream {
    fn prepare_accepted(&mut self) -> io::Result<()> {
        // The instance is fully connected; nothing to restore.
        Ok(())
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        *self.read_timeout.lock().unwrap() = timeout;
        Ok(())
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        *self.write_timeout.lock().unwrap() = timeout;
        Ok(())
    }
}

/// Listener over a named pipe: an acceptor thread creates instances, blocks
/// in ConnectNamedPipe, and queues connected streams for the generic accept
/// loop. Shutdown is a poison connection that the acceptor discards.
pub struct NamedPipeListener {
    name: String,
    rx: mpsc::Receiver<(PipeStream, PeerDesc)>,
    shutdown: Arc<AtomicBool>,
    acceptor: Option<thread::JoinHandle<()>>,
}

impl NamedPipeListener {
    /// Bind (start accepting) on the pipe name (without the \\.\pipe\ prefix).
    pub fn bind(name: impl Into<String>, _limits: &crate::limits::Limits) -> io::Result<Self> {
        let name = name.into();
        let full = pipe_path(&name);
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let acceptor = {
            let shutdown = Arc::clone(&shutdown);
            let full = full.clone();
            let name = name.clone();
            thread::spawn(move || acceptor_loop(shutdown, full, name, tx))
        };
        Ok(Self {
            name,
            rx,
            shutdown,
            acceptor: Some(acceptor),
        })
    }

    /// The pipe name (without prefix).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Stop accepting: set the flag and wake the blocked ConnectNamedPipe
    /// with a poison connection. Idempotent.
    pub fn stop(&mut self) {
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return;
        }
        // Poison pill: connect once so the acceptor's blocking call returns
        // and it observes the shutdown flag.
        unsafe {
            let handle = CreateFileW(
                wide(&pipe_path(&self.name)).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                0,
            );
            if handle != INVALID_HANDLE_VALUE {
                CloseHandle(handle);
            }
        }
        if let Some(handle) = self.acceptor.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for NamedPipeListener {
    fn drop(&mut self) {
        self.stop();
    }
}

fn acceptor_loop(
    shutdown: Arc<AtomicBool>,
    full_name: String,
    name: String,
    tx: mpsc::Sender<(PipeStream, PeerDesc)>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        let handle = unsafe {
            CreateNamedPipeW(
                wide(&full_name).as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            thread::sleep(Duration::from_millis(20));
            continue;
        }
        let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) };
        let ok = connected != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if !ok {
            unsafe {
                CloseHandle(handle);
            }
            continue;
        }
        if shutdown.load(Ordering::SeqCst) {
            // Poison connection used to wake the acceptor.
            unsafe {
                DisconnectNamedPipe(handle);
                CloseHandle(handle);
            }
            return;
        }
        let identity = crate::peer::peer_identity_of_raw_handle(handle).ok();
        let stream = PipeStream::new(handle, &name, identity);
        let desc = stream.desc();
        if tx.send((stream, desc)).is_err() {
            return;
        }
    }
}

impl CarrierListener for NamedPipeListener {
    type Stream = PipeStream;

    fn set_nonblocking(&self) -> io::Result<()> {
        // The acceptor thread owns the blocking ConnectNamedPipe; accept()
        // itself polls the queue, so no listener flag is involved.
        Ok(())
    }

    fn accept(&self) -> io::Result<(PipeStream, PeerDesc)> {
        match self.rx.recv_timeout(Duration::from_millis(5)) {
            Ok(item) => Ok(item),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(io::Error::from(io::ErrorKind::WouldBlock)),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(io::Error::from(io::ErrorKind::NotConnected))
            }
        }
    }

    fn peer_identity(&self, stream: &PipeStream) -> io::Result<Option<PeerIdentity>> {
        // Captured at accept time (the kernel call must run on the connected
        // server handle before any I/O reorders the picture).
        Ok(stream.taken_identity())
    }

    fn local_desc(&self) -> String {
        format!("pipe:{}", self.name)
    }
}

/// Connect to a named pipe as a client (used by tests and the Shell side).
pub fn connect(name: &str) -> io::Result<PipeStream> {
    let handle = unsafe {
        CreateFileW(
            wide(&pipe_path(name)).as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    Ok(PipeStream::new(handle, name, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_round_trip_and_peer_identity() {
        let name = format!("dsh-lt-test-{}-{}", std::process::id(), 1);
        let listener = NamedPipeListener::bind(&name, &crate::limits::Limits::default())
            .expect("bind pipe listener");

        let client = thread::spawn({
            let name = name.clone();
            move || {
                // The acceptor may not have created the instance yet; retry.
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    match connect(&name) {
                        Ok(stream) => return stream,
                        Err(_) if Instant::now() < deadline => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) => panic!("client connect failed: {error}"),
                    }
                }
            }
        });

        // Accept the connection (poll until the acceptor queues it).
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut server_stream, desc) = loop {
            match listener.accept() {
                Ok(item) => break item,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("accept failed: {error}"),
            }
        };
        assert!(matches!(desc, PeerDesc::NamedPipe(_)));

        // Kernel identity: the client is this very process (the test client
        // runs on a thread, so the PID equals ours) - the important part is
        // that the OS reports a PID at all and it resolves to an image path.
        let identity = listener
            .peer_identity(&server_stream)
            .expect("identity probe")
            .expect("named pipe must provide a peer identity");
        assert_eq!(identity.pid, std::process::id());
        assert!(identity.image_path.is_some(), "image path resolves");

        // Framing round trip (the same length-prefixed frames TCP uses).
        let mut client_stream = client.join().expect("client thread");
        client_stream
            .write_all(&crate::framing::encode_frame(b"hello-pipe"))
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
        assert_eq!(&payload, b"hello-pipe");

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
        let name = format!("dsh-lt-test-{}-{}", std::process::id(), 2);
        let listener = NamedPipeListener::bind(&name, &crate::limits::Limits::default())
            .expect("bind pipe listener");
        let client = thread::spawn({
            let name = name.clone();
            move || {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    match connect(&name) {
                        Ok(stream) => return stream,
                        Err(_) if Instant::now() < deadline => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) => panic!("client connect failed: {error}"),
                    }
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut server_stream, _) = loop {
            match listener.accept() {
                Ok(item) => break item,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("accept failed: {error}"),
            }
        };
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

    /// Regression (2026-09-13 live QA): a closed peer must surface as EOF
    /// (read == Ok(0)), not as "no data yet" - otherwise the supervision
    /// loop never ends the connection and the daemon misses every
    /// disconnect path (credential re-issue, lease revocation).
    #[test]
    fn peer_close_surfaces_as_eof() {
        let name = format!("dsh-lt-test-{}-{}", std::process::id(), 4);
        let limits = crate::limits::Limits::default();
        let listener = NamedPipeListener::bind(&name, &limits).expect("bind pipe listener");
        // Client first (the acceptor creates the instance asynchronously),
        // then accept the connection, then close the client.
        let client = {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match connect(&name) {
                    Ok(stream) => break stream,
                    Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                    Err(error) => panic!("client connect failed: {error}"),
                }
            }
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut server_stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("accept failed: {error}"),
            }
        };
        drop(client);

        server_stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set timeout");
        let mut buf = [0u8; 8];
        let read = server_stream.read(&mut buf).expect("read must not error");
        assert_eq!(read, 0, "a closed peer must surface as EOF, not a stall");
    }

    #[test]
    fn stop_wakes_blocked_acceptor() {
        let name = format!("dsh-lt-test-{}-{}", std::process::id(), 3);
        let mut listener = NamedPipeListener::bind(&name, &crate::limits::Limits::default())
            .expect("bind pipe listener");
        // No client ever connects; stop must return promptly via the poison
        // connection instead of hanging in ConnectNamedPipe.
        let started = Instant::now();
        listener.stop();
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}

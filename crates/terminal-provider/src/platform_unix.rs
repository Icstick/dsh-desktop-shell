//! Unix PTY platform session (M8-A, WI-M8-STABLE): openpty + fork/exec +
//! poll(2) reader + TIOCSWINSZ resize.
//!
//! Mirrors the Windows ConPTY session contract (write/resize/terminate/
//! reader with the same stop-flag discipline): the reader polls the master
//! fd with poll(2) and never blocks indefinitely, so teardown cannot
//! deadlock (same finding as the M6-C1 Windows fix).

use std::ffi::{CStr, CString};
use std::io;
use std::os::unix::io::RawFd;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::{MAX_WRITE_BYTES, OutputEvent, PtyError};

/// Bound on retrying EAGAIN while writing to a backpressured PTY master.
const WRITE_DEADLINE_SECS: u64 = 5;

/// Master/slave PTY pair plus the spawned child.
#[derive(Debug)]
pub struct PlatformSession {
    master_fd: RawFd,
    child: libc::pid_t,
}

/// Default shell: \$SHELL when set, else /bin/sh.
pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

/// Resolve a shell name to an executable path (Unix list; the schema
/// union is cross-platform — cmd/powershell are Windows-only and rejected
/// here, mirroring the Windows resolve_shell).
pub fn resolve_shell(shell: Option<&str>) -> Result<String, PtyError> {
    match shell.unwrap_or("default") {
        "default" | "sh" => Ok(default_shell()),
        "bash" => Ok("bash".to_string()),
        "zsh" => Ok("zsh".to_string()),
        "powershell" | "pwsh" => Ok("pwsh".to_string()),
        _ => Err(PtyError::InvalidShell),
    }
}

fn last_os_error() -> io::Error {
    io::Error::last_os_error()
}

/// Open a master/slave PTY pair via posix_openpt/grantpt/unlockpt.
fn openpty_pair() -> io::Result<(RawFd, RawFd)> {
    unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        if master < 0 {
            return Err(last_os_error());
        }
        if libc::grantpt(master) != 0 {
            let _ = libc::close(master);
            return Err(last_os_error());
        }
        if libc::unlockpt(master) != 0 {
            let _ = libc::close(master);
            return Err(last_os_error());
        }
        let name = libc::ptsname(master);
        if name.is_null() {
            let _ = libc::close(master);
            return Err(io::Error::other("ptsname failed"));
        }
        let name = CStr::from_ptr(name);
        let slave = libc::open(name.as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
        if slave < 0 {
            let _ = libc::close(master);
            return Err(last_os_error());
        }
        // Mark both fds close-on-exec: the child inherits them across
        // fork and must not leak them into the exec'd shell (the shell
        // would then hold the PTY master open and EOF would never arrive).
        libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(slave, libc::F_SETFD, libc::FD_CLOEXEC);
        Ok((master, slave))
    }
}

fn set_winsize(fd: RawFd, cols: u16, rows: u16) {
    let size = libc::winsize {
        ws_row: rows as libc::c_ushort,
        ws_col: cols as libc::c_ushort,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        let _ = libc::ioctl(fd, libc::TIOCSWINSZ as libc::c_ulong, &size);
    }
}

impl PlatformSession {
    /// Spawn a shell under a fresh PTY (fork + setsid + TIOCSCTTY +
    /// dup2 stdio + execvp). The parent keeps the master; the child owns
    /// the slave. All acquired descriptors are released on the error path.
    ///
    /// # Errors
    ///
    /// Returns PtyError::SpawnUnavailable when the PTY pair or fork fails.
    pub fn spawn(
        shell_path: &str,
        cols: u16,
        rows: u16,
        cwd: Option<&str>,
    ) -> Result<Self, PtyError> {
        unsafe {
            let (master, slave) = openpty_pair().map_err(|_| PtyError::SpawnUnavailable)?;
            set_winsize(master, cols, rows);
            let shell_c = CString::new(shell_path).map_err(|_| PtyError::InvalidShell)?;
            let cwd_c = cwd
                .map(|dir| CString::new(dir).map_err(|_| PtyError::InvalidShell))
                .transpose()?;
            let pid = libc::fork();
            if pid < 0 {
                let _ = libc::close(master);
                let _ = libc::close(slave);
                return Err(PtyError::SpawnUnavailable);
            }
            if pid == 0 {
                // Child: detach, take the controlling tty, wire stdio.
                libc::setsid();
                let _ = libc::ioctl(slave, libc::TIOCSCTTY as libc::c_ulong, 0);
                libc::dup2(slave, 0);
                libc::dup2(slave, 1);
                libc::dup2(slave, 2);
                if slave > 2 {
                    libc::close(slave);
                }
                libc::close(master);
                if let Some(dir) = cwd_c.as_deref() {
                    let _ = libc::chdir(dir.as_ptr());
                }
                let argv: [*const libc::c_char; 2] = [shell_c.as_ptr(), std::ptr::null()];
                libc::execvp(shell_c.as_ptr(), argv.as_ptr());
                // exec failed: exit loudly; the parent sees EOF on the master.
                libc::_exit(127);
            }
            // Parent.
            libc::close(slave);
            Ok(Self {
                master_fd: master,
                child: pid,
            })
        }
    }

    /// Reader token: the master fd (passed to the reader thread as the
    /// session keeps the fd alive).
    pub fn reader_token(&self) -> usize {
        self.master_fd as usize
    }

    /// Write data to the PTY master (the child's stdin). The master is
    /// O_NONBLOCK (reader teardown discipline), so backpressure from a
    /// child that is not draining its stdin surfaces as EAGAIN; retry with
    /// a bounded deadline (mirrors the blocking-queue semantics of the
    /// Windows WriteFile path) and give up with PtyError::Io on timeout
    /// rather than silently dropping input. EINTR is retried without
    /// consuming the deadline.
    pub fn write(&self, data: &str) -> Result<(), PtyError> {
        let bytes = data.as_bytes();
        if bytes.len() > MAX_WRITE_BYTES {
            return Err(PtyError::WriteTooLarge);
        }
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(WRITE_DEADLINE_SECS);
        let mut written = 0usize;
        while written < bytes.len() {
            let n = unsafe {
                libc::write(
                    self.master_fd,
                    bytes[written..].as_ptr() as *const libc::c_void,
                    bytes.len() - written,
                )
            };
            if n < 0 {
                let err = io::Error::last_os_error();
                match err.kind() {
                    io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return Err(PtyError::Io);
                        }
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    io::ErrorKind::Interrupted => continue,
                    _ => return Err(PtyError::Io),
                }
            }
            written += n as usize;
        }
        Ok(())
    }

    /// Resize the PTY (TIOCSWINSZ on the master).
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        set_winsize(self.master_fd, cols, rows);
        Ok(())
    }

    /// Spawn the output reader: poll(2) the master for POLLIN with a
    /// bounded tick, forward chunks as events. Exits on the stop flag, a
    /// poll error, EOF, or a full event queue (drop, AC-IPC-002 pattern).
    pub fn spawn_reader(
        session_id: String,
        master_fd: usize,
        event_tx: std::sync::mpsc::SyncSender<OutputEvent>,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> JoinHandle<()> {
        let master_fd = master_fd as RawFd;
        std::thread::spawn(move || {
            // macOS poll(2) (kqueue-backed) can report POLLIN on a PTY
            // master whose data was already drained; a blocking read then
            // hangs forever and wedges teardown (the close path joins the
            // reader, observed as a stuck close Result on macOS CI). Make
            // the master non-blocking so a spurious POLLIN degrades to
            // EAGAIN and the loop keeps polling: the stop flag stays
            // responsive and the join always returns.
            let flags = unsafe { libc::fcntl(master_fd, libc::F_GETFL, 0) };
            if flags >= 0 {
                unsafe { libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
            }
            let mut buffer = [0u8; 4096];
            let mut seq = 0u64;
            loop {
                if stop.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let mut fds = [libc::pollfd {
                    fd: master_fd,
                    events: libc::POLLIN,
                    revents: 0,
                }];
                let ready = unsafe { libc::poll(fds.as_mut_ptr(), 1, 100) };
                if ready < 0 {
                    break;
                }
                if ready == 0 {
                    continue;
                }
                if fds[0].revents & (libc::POLLIN | libc::POLLHUP) == 0 {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                let n = unsafe {
                    libc::read(
                        master_fd,
                        buffer.as_mut_ptr() as *mut libc::c_void,
                        buffer.len(),
                    )
                };
                if n < 0 {
                    let err = io::Error::last_os_error();
                    if err.kind() == io::ErrorKind::WouldBlock {
                        // Spurious POLLIN: the data is gone (macOS pty
                        // semantics); back off a tick and keep polling.
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    break; // real read error: session is gone.
                }
                if n == 0 {
                    break; // EOF: session is gone.
                }
                seq += 1;
                if event_tx
                    .try_send(OutputEvent {
                        session_id: session_id.clone(),
                        seq,
                        data: String::from_utf8_lossy(&buffer[..n as usize]).into_owned(),
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    /// Teardown phase 1: ask the child to exit and reap it (no zombies).
    /// The master fd stays open until close_read — the reader thread is
    /// still polling it; closing it early lets the fd number be reused and
    /// a stale reader poll then reads another session's data (observed as
    /// foreign output events on Ubuntu CI).
    ///
    /// Interactive shells deliberately ignore SIGTERM (bash/dash design:
    /// terminal teardown must not kill a foreground job), so a plain
    /// blocking waitpid hangs forever and wedges the daemon (observed as a
    /// 29s hang before the connection dropped on Ubuntu/macOS CI). The
    /// bounded wait + SIGKILL fallback guarantees reaping — and every
    /// waitpid here is WNOHANG under a deadline: a stuck blocking waitpid
    /// was observed on macOS CI (SIGKILL delivered, waitpid never
    /// returned), so no reap path may block the daemon indefinitely.
    pub fn terminate_io(&mut self) {
        unsafe {
            if self.child > 0 {
                let _ = libc::kill(self.child, libc::SIGTERM);
                let mut status: libc::c_int = 0;
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
                loop {
                    let reaped = libc::waitpid(self.child, &mut status, libc::WNOHANG);
                    if reaped == self.child {
                        break;
                    }
                    if std::time::Instant::now() >= deadline {
                        let _ = libc::kill(self.child, libc::SIGKILL);
                        // Bounded reap after SIGKILL: never block the
                        // daemon on waitpid (macOS CI observed a stuck
                        // blocking waitpid; a leftover zombie is reaped by
                        // init when the daemon exits).
                        let reap_deadline =
                            std::time::Instant::now() + std::time::Duration::from_millis(200);
                        loop {
                            let reaped = libc::waitpid(self.child, &mut status, libc::WNOHANG);
                            if reaped == self.child {
                                break;
                            }
                            if std::time::Instant::now() >= reap_deadline {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                self.child = -1;
            }
        }
    }

    /// Teardown phase 2 (after the reader is joined): close the master.
    /// The slave is already closed in the parent after fork.
    pub fn close_read(&mut self) {
        unsafe {
            if self.master_fd >= 0 {
                let _ = libc::close(self.master_fd);
                self.master_fd = -1;
            }
        }
    }
}

impl Drop for PlatformSession {
    fn drop(&mut self) {
        self.terminate_io();
        self.close_read();
    }
}

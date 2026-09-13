//! Kernel-provided peer identity (ADR-0022).
//!
//! The credential proves *possession* of a one-time token; it says nothing
//! about *which process* is on the other end. Carriers with kernel support
//! (Windows named pipes, Unix domain sockets) let the server ask the OS for
//! the connecting process, which is the only mechanism that can distinguish
//! "the Shell" from "another same-user process that read the token file".
//!
//! Loopback TCP has no such mechanism: [crate::carrier::CarrierListener::peer_identity]
//! returns `None` there, and callers must treat those connections as
//! identity-less (fail closed for control-plane authority).

/// Identity of the process on the other end of a carrier connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIdentity {
    /// Process id as reported by the kernel.
    pub pid: u32,
    /// Executable image path, when the platform can resolve it.
    pub image_path: Option<String>,
}

impl PeerIdentity {
    /// Build an identity record.
    pub fn new(pid: u32, image_path: Option<String>) -> Self {
        Self { pid, image_path }
    }

    /// Whether the peer image resolves to exactly the expected path.
    ///
    /// Case is folded on the platforms whose default filesystem does not
    /// preserve it (Windows, macOS/APFS): there a case difference cannot
    /// name a different file, so folding only removes false negatives and
    /// never admits a different binary. Everywhere else the comparison is
    /// exact.
    pub fn image_matches(&self, expected: &str) -> bool {
        let Some(image) = self.image_path.as_deref() else {
            return false;
        };
        if cfg!(any(windows, target_os = "macos")) {
            image.eq_ignore_ascii_case(expected)
        } else {
            image == expected
        }
    }
}

#[cfg(windows)]
// SAFETY CONTRACT (ADR-0022): the Windows half of the crate's unsafe
// exemption. It calls only three read-only Win32 probes on handles the
// caller owns; no memory is retained, no foreign pointers escape, and the
// PID/path are treated as untrusted input by every caller.
#[allow(unsafe_code)]
mod windows_impl {
    use super::PeerIdentity;
    use std::io;

    /// Raw Win32 handle value (isize, not a pointer: handles must stay
    /// Send when they travel with a carrier stream).
    type Handle = isize;
    type Bool = i32;
    type Dword = u32;

    const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x1000;

    unsafe extern "system" {
        fn GetNamedPipeClientProcessId(pipe: Handle, pid: *mut Dword) -> Bool;
        fn OpenProcess(access: Dword, inherit: Bool, pid: Dword) -> Handle;
        fn QueryFullProcessImageNameW(
            handle: Handle,
            flags: Dword,
            buffer: *mut u16,
            size: *mut Dword,
        ) -> Bool;
        fn CloseHandle(handle: Handle) -> Bool;
    }

    /// Identity of the client connected to a named pipe instance (the raw
    /// handle of a connected server-side instance).
    ///
    /// Verified by the 2026-09-12 spike (docs/research/SPIKE-PEER-IDENTITY-20260912.md):
    /// the PID is the real client process and resolves to its image path.
    pub(crate) fn peer_identity_of_raw_handle(handle: Handle) -> io::Result<PeerIdentity> {
        let mut pid: Dword = 0;
        let ok = unsafe { GetNamedPipeClientProcessId(handle, &mut pid) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(PeerIdentity::new(pid, image_path_of(pid)))
    }

    fn image_path_of(pid: Dword) -> Option<String> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle == 0 {
            return None;
        }
        let mut buffer = vec![0u16; 1024];
        let mut size = buffer.len() as Dword;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    }
}

#[cfg(windows)]
pub(crate) use windows_impl::peer_identity_of_raw_handle;

#[cfg(target_os = "linux")]
mod unix_impl {
    use super::PeerIdentity;
    use std::io;

    /// Identity of the peer connected to a Unix domain socket stream
    /// (Linux: SO_PEERCRED via nix + /proc/<pid>/exe for the image path).
    ///
    /// nix wraps the getsockopt call safely, so this branch - like the rest
    /// of the crate - is unsafe-free: Linux is the platform where the crate
    /// keeps its no-unsafe property end to end.
    pub(crate) fn peer_identity_of_unix_stream(
        stream: &std::os::unix::net::UnixStream,
    ) -> io::Result<PeerIdentity> {
        let credentials =
            nix::sys::socket::getsockopt(stream, nix::sys::socket::sockopt::PeerCredentials)
                .map_err(|error| io::Error::other(format!("SO_PEERCRED failed: {error}")))?;
        let pid = credentials.pid();
        if pid <= 0 {
            return Err(io::Error::other("SO_PEERCRED returned no pid"));
        }
        Ok(PeerIdentity::new(pid as u32, image_path_of(pid as u32)))
    }

    fn image_path_of(pid: u32) -> Option<String> {
        std::fs::read_link(format!("/proc/{pid}/exe"))
            .ok()
            .map(|path| path.to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "macos")]
// SAFETY CONTRACT (ADR-0022): macOS exposes the peer pid only through
// `getsockopt(LOCAL_PEERPID)` and the peer image only through
// `proc_pidpath`; neither has a safe wrapper in std, nix or libc. This
// module performs exactly those two read-only calls on a socket the caller
// owns, copies the results into owned memory, and lets nothing else escape.
// It is the second - and last - unsafe exemption of this crate; the first is
// the Windows named-pipe FFI above.
#[allow(unsafe_code)]
mod apple_impl {
    use super::PeerIdentity;
    use std::io;
    use std::os::unix::io::AsRawFd;

    /// `SOL_LOCAL`/`LOCAL_PEERPID` (sys/un.h): the socket-local
    /// level and the peer-pid option. libc defines both for apple targets;
    /// they are repeated here with their canonical values so the call site
    /// reads as one unit.
    const SOL_LOCAL: libc::c_int = 0;
    const LOCAL_PEERPID: libc::c_int = 0x002;

    /// Identity of the peer connected to a Unix domain socket stream.
    ///
    /// Verified on the CI matrix: the pid comes from the kernel (a same-user
    /// impostor cannot fake it) and `proc_pidpath` resolves the image
    /// of our own test process, which is what the strict policy compares
    /// against the expected Shell binary.
    pub(crate) fn peer_identity_of_unix_stream(
        stream: &std::os::unix::net::UnixStream,
    ) -> io::Result<PeerIdentity> {
        let mut pid: libc::pid_t = 0;
        let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
        // SAFETY: read-only getsockopt on a live socket fd; `pid`/`len`
        // are stack locals and the kernel writes at most `len` bytes.
        let rc = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                SOL_LOCAL,
                LOCAL_PEERPID,
                std::ptr::addr_of_mut!(pid).cast(),
                &mut len,
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        if pid <= 0 {
            return Err(io::Error::other("LOCAL_PEERPID returned no pid"));
        }
        let pid = pid as u32;
        Ok(PeerIdentity::new(pid, image_path_of(pid)))
    }

    /// Executable path of `pid` via libproc. Returns `None` when the
    /// kernel refuses (different user, protected process) - callers treat a
    /// missing image as a failed match, never as a match.
    fn image_path_of(pid: u32) -> Option<String> {
        let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: proc_pidpath writes into our buffer, bounded by its length.
        let written = unsafe {
            libc::proc_pidpath(
                pid as libc::c_int,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
            )
        };
        if written <= 0 {
            return None;
        }
        buffer.truncate(written as usize);
        String::from_utf8(buffer).ok()
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
mod other_unix_impl {
    use super::PeerIdentity;
    use std::io;

    /// Other Unix (BSD family): the carrier works, but kernel PID extraction
    /// has neither a verification path nor a CI target here. Fail closed
    /// with a typed error instead of pretending the connection is
    /// identity-less.
    pub(crate) fn peer_identity_of_unix_stream(
        _stream: &std::os::unix::net::UnixStream,
    ) -> io::Result<PeerIdentity> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "peer identity extraction is not implemented on this platform",
        ))
    }
}

#[cfg(target_os = "macos")]
pub(crate) use apple_impl::peer_identity_of_unix_stream;
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
pub(crate) use other_unix_impl::peer_identity_of_unix_stream;
#[cfg(target_os = "linux")]
pub(crate) use unix_impl::peer_identity_of_unix_stream;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_matches_is_strict_without_path() {
        let identity = PeerIdentity::new(42, None);
        assert!(!identity.image_matches("C:\\anywhere.exe"));
    }

    #[test]
    fn image_matches_folds_case_only_where_the_filesystem_does() {
        let identity = PeerIdentity::new(42, Some("/opt/dsh/shell".to_string()));
        assert!(identity.image_matches("/opt/dsh/shell"));
        assert!(!identity.image_matches("/opt/dsh/other"));
        if cfg!(any(windows, target_os = "macos")) {
            assert!(identity.image_matches("/OPT/DSH/SHELL"));
        } else {
            assert!(!identity.image_matches("/OPT/DSH/SHELL"));
        }
    }
}

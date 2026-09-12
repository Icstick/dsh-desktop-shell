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

    /// Whether the peer image resolves to exactly the expected path
    /// (case-insensitive on Windows path semantics, exact elsewhere).
    pub fn image_matches(&self, expected: &str) -> bool {
        let Some(image) = self.image_path.as_deref() else {
            return false;
        };
        if cfg!(windows) {
            image.eq_ignore_ascii_case(expected)
        } else {
            image == expected
        }
    }
}

#[cfg(windows)]
// SAFETY CONTRACT (ADR-0022): this module is the crate single unsafe
// exemption. It calls only three read-only Win32 probes on handles the
// caller owns; no memory is retained, no foreign pointers escape, and the
// PID/path are treated as untrusted input by every caller.
#[allow(unsafe_code)]
#[allow(dead_code)] // FFI surfaces consumed by the named-pipe carrier (next slice)
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
#[allow(dead_code)] // consumed by the UDS carrier (WI-M13-UNIX-UDS-CARRIER)
mod unix_impl {
    use super::PeerIdentity;
    use std::io;

    /// Identity of the peer connected to a Unix domain socket stream
    /// (Linux: SO_PEERCRED via nix + /proc/<pid>/exe for the image path).
    ///
    /// nix wraps the getsockopt call safely, so this branch - like the rest
    /// of the crate - is unsafe-free. Only verifiable on the CI matrix: the
    /// local development machine is Windows (ADR-0022 spike limitation).
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

#[cfg(all(unix, not(target_os = "linux")))]
#[allow(dead_code)] // consumed by the UDS carrier (WI-M13-UNIX-UDS-CARRIER)
mod unix_impl {
    use super::PeerIdentity;
    use std::io;

    /// macOS/BSD: the carrier exists, but PID extraction needs LOCAL_PEERPID
    /// (macOS) and has not been verified yet - it is part of the CI-matrix
    /// slice of ADR-0022. Fail closed with a typed error instead of
    /// pretending the connection is identity-less.
    pub(crate) fn peer_identity_of_unix_stream(
        _stream: &std::os::unix::net::UnixStream,
    ) -> io::Result<PeerIdentity> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "peer identity extraction is not implemented on this platform yet (ADR-0022 CI slice)",
        ))
    }
}

#[cfg(unix)]
#[allow(unused_imports)] // consumed by the UDS carrier (WI-M13-UNIX-UDS-CARRIER)
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
    fn image_matches_handles_case_on_windows() {
        let identity = PeerIdentity::new(42, Some("C:\\App\\shell.exe".to_string()));
        if cfg!(windows) {
            assert!(identity.image_matches("c:\\app\\SHELL.EXE"));
            assert!(!identity.image_matches("C:\\App\\other.exe"));
        } else {
            assert!(identity.image_matches("C:\\App\\shell.exe"));
        }
    }
}

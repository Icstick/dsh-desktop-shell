//! Platform dispatch for the PTY session (M8-A): Windows ConPTY and
//! Unix openpty share the same session contract.

#[cfg(unix)]
#[path = "platform_unix.rs"]
mod unix;
#[cfg(windows)]
#[path = "platform_windows.rs"]
mod windows;

#[cfg(unix)]
pub use unix::{PlatformSession, resolve_shell};
#[cfg(windows)]
pub use windows::{PlatformSession, resolve_shell};

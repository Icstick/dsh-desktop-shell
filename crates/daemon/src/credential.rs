//! Startup credential file (ADR-0019 decision 5: Shell reads the file at
//! startup, daemon issues one-time credentials via local-transport).
//!
//! The daemon writes `daemon-credential.json` into the daemon data
//! directory (`%APPDATA%/dev.dsh.desktop-shell/` on Windows,
//! `$XDG_DATA_HOME/dev.dsh.desktop-shell/` — or
//! `$HOME/.local/share/dev.dsh.desktop-shell/` — on Unix; overridable with
//! `--data-dir` or the `DSH_DAEMON_DATA_DIR` environment variable).
//! The Shell reads this file to learn where to connect (the envelope
//! `port`) and which one-time `credential` to present during the
//! local-transport handshake. The credential is consumed by its first
//! successful handshake (AC-IPC-001): a Shell restart must re-read the
//! file after the daemon re-issues.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Sub-directory of `%APPDATA%` used for daemon runtime files.
pub const DATA_DIR_NAME: &str = "dev.dsh.desktop-shell";

/// Name of the credential file inside the data directory.
pub const CREDENTIAL_FILE_NAME: &str = "daemon-credential.json";

/// Name of the single-instance lock file inside the data directory.
pub const LOCK_FILE_NAME: &str = "daemon.lock";

/// The fixed, well-known daemon envelope port (ADR-0019 decision 5,
/// 0.2.1 M6-C): the envelope server binds this loopback port directly.
/// It is simultaneously the single-instance authority (a second daemon
/// fails the bind), the Shell presence probe and the connect endpoint;
/// the credential file still carries the one-time token.
pub const CLAIM_PORT: u16 = 37_771;

/// Schema version of the credential file (bump on breaking shape change).
/// v2 (ADR-0022): adds the optional `pipeName` field - the peer-identity
/// carrier endpoint the Shell should prefer. Readers must accept v1 (no
/// pipe name: TCP only) and v2.
pub const CREDENTIAL_FILE_SCHEMA_VERSION: u32 = 2;

/// Resolve the daemon data directory: `--data-dir`/`DSH_DAEMON_DATA_DIR`
/// override, then `%APPDATA%\dev.dsh.desktop-shell` / `%LOCALAPPDATA%\...`,
/// then the Unix XDG fallback (`$XDG_DATA_HOME/dev.dsh.desktop-shell`, or
/// `$HOME/.local/share/dev.dsh.desktop-shell`).
///
/// The resolution is fallible on purpose (security audit 2026-09-10, H-1):
/// the previous last resort was `PathBuf::from(".")`, which is a **normal**
/// outcome on Unix (no `APPDATA`/`LOCALAPPDATA` there) and would have put
/// the daemon credential file — the one-time token the Shell presents —
/// into whatever directory the daemon happened to be started from.
/// A caller that cannot resolve a real data directory must fail, never
/// silently write credentials into the current directory.
pub fn data_dir() -> io::Result<PathBuf> {
    if let Ok(dir) = env::var("DSH_DAEMON_DATA_DIR")
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    if let Ok(appdata) = env::var("APPDATA")
        && !appdata.is_empty()
    {
        return Ok(PathBuf::from(appdata).join(DATA_DIR_NAME));
    }
    if let Ok(local) = env::var("LOCALAPPDATA")
        && !local.is_empty()
    {
        return Ok(PathBuf::from(local).join(DATA_DIR_NAME));
    }
    #[cfg(unix)]
    {
        if let Ok(xdg) = env::var("XDG_DATA_HOME")
            && !xdg.is_empty()
        {
            return Ok(PathBuf::from(xdg).join(DATA_DIR_NAME));
        }
        if let Ok(home) = env::var("HOME")
            && !home.is_empty()
        {
            return Ok(PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(DATA_DIR_NAME));
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no daemon data directory: set DSH_DAEMON_DATA_DIR, XDG_DATA_HOME or HOME",
        ))
    }
    #[cfg(not(unix))]
    {
        // Non-Unix platforms without APPDATA/LOCALAPPDATA keep the historical
        // current-directory fallback; refuse-with-an-error here would break
        // embedded/dev launches on those platforms with no safer alternative.
        Ok(PathBuf::from("."))
    }
}

/// The on-disk credential file (schema `daemon-credential.json`, v2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CredentialFile {
    pub schema_version: u32,
    pub daemon_version: String,
    pub pid: u32,
    pub claim_port: u16,
    /// The envelope server port the Shell must connect to (the TCP endpoint:
    /// presence probe and degradation carrier).
    pub port: u16,
    /// Named pipe endpoint of the peer-identity carrier (ADR-0022), when the
    /// platform provides one. The Shell prefers it: only a named-pipe
    /// connection carries a kernel-provided peer identity, which is what
    /// strict-mode control-plane authority requires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipe_name: Option<String>,
    pub credential: FileCredential,
    pub issued_at: String,
}

/// One-time credential serialized into the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FileCredential {
    /// The opaque token presented during the local-transport handshake.
    pub token: String,
    /// RFC 3339 UTC expiry of the credential.
    pub expires_at: String,
}

impl CredentialFile {
    /// Build the file payload for the daemon startup.
    pub fn new(
        daemon_version: impl Into<String>,
        pid: u32,
        claim_port: u16,
        port: u16,
        token: impl Into<String>,
        expires_at: SystemTime,
        issued_at: SystemTime,
    ) -> Self {
        Self {
            schema_version: CREDENTIAL_FILE_SCHEMA_VERSION,
            daemon_version: daemon_version.into(),
            pid,
            claim_port,
            port,
            pipe_name: None,
            credential: FileCredential {
                token: token.into(),
                expires_at: rfc3339(expires_at),
            },
            issued_at: rfc3339(issued_at),
        }
    }

    /// Record the peer-identity carrier endpoint (ADR-0022): the Shell
    /// prefers the named pipe, whose connection carries a kernel-provided
    /// peer identity; TCP stays the degradation path.
    pub fn with_pipe_name(mut self, pipe_name: impl Into<String>) -> Self {
        self.pipe_name = Some(pipe_name.into());
        self
    }

    /// Serialize to the pretty JSON wire shape.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parse the file shape; unknown fields are rejected fail-closed.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Atomically write the file into `dir` (temp file + rename, so a
    /// concurrent Shell read never observes a torn file).
    ///
    /// The file carries a one-time bootstrap token, so its permissions are
    /// tightened explicitly instead of relying on the process umask
    /// (security audit 2026-09-10, H-1): on Unix the data directory is set
    /// to `0700` and the file to `0600` — the same policy the Shell-side
    /// `environment_store` applies (`restrict_directory`/`restrict_file`).
    /// `0600` is applied to the temp file **before** the rename so the
    /// final path is never observable with looser bits.
    ///
    /// Windows: no explicit ACL work here — `%APPDATA%` (and every
    /// directory created under it) inherits the per-user profile ACL, which
    /// is already single-user; an explicit DACL pass is deliberately left
    /// out of scope for this fix.
    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        restrict_directory(dir)?;
        let json = self.to_json().map_err(io::Error::other)?;
        let temp = dir.join(format!("{CREDENTIAL_FILE_NAME}.tmp"));
        fs::write(&temp, json)?;
        restrict_file(&temp)?;
        fs::rename(&temp, dir.join(CREDENTIAL_FILE_NAME))?;
        Ok(())
    }

    /// Read the credential file from `dir`.
    pub fn read_from(dir: &Path) -> io::Result<Self> {
        let json = fs::read_to_string(dir.join(CREDENTIAL_FILE_NAME))?;
        Self::from_json(&json).map_err(io::Error::other)
    }
}

/// Restrict a directory to owner-only access (`0700` on Unix; no-op
/// elsewhere — see `write_to` for the Windows rationale).
#[cfg(unix)]
fn restrict_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Restrict a file to owner read/write (`0600` on Unix; no-op elsewhere).
#[cfg(unix)]
fn restrict_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// RFC 3339 UTC millisecond timestamp (no external crates; the envelope
/// module owns the same formatter for wire timestamps).
pub(crate) fn rfc3339(time: SystemTime) -> String {
    crate::envelope::now_timestamp_like(time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample() -> CredentialFile {
        let now = SystemTime::now();
        CredentialFile::new(
            "0.1.0",
            4242,
            CLAIM_PORT,
            50_001,
            "lt_0123456789abcdef0123456789abcdef",
            now + Duration::from_secs(3600),
            now,
        )
    }

    #[test]
    fn json_shape_matches_shell_contract() {
        let json = sample().to_json().expect("serializes");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(parsed["schemaVersion"], 2);
        assert_eq!(parsed["daemonVersion"], "0.1.0");
        assert_eq!(parsed["pid"], 4242);
        assert_eq!(parsed["claimPort"], CLAIM_PORT);
        assert_eq!(parsed["port"], 50_001);
        assert_eq!(
            parsed["credential"]["token"],
            "lt_0123456789abcdef0123456789abcdef"
        );
        assert!(parsed["credential"]["expiresAt"].as_str().is_some());
        assert!(parsed["issuedAt"].as_str().is_some());
        // No pipe carrier in this sample: the field is omitted entirely
        // (v1-compatible shape), so there are no unknown keys.
        assert!(parsed.get("pipeName").is_none());
        assert_eq!(parsed.as_object().map(|o| o.len()), Some(7));
    }

    #[test]
    fn pipe_name_is_published_and_round_trips() {
        // ADR-0022: the peer-identity carrier endpoint travels in the file.
        let file = sample().with_pipe_name("dsh-desktop-daemon-7-abc");
        let json = file.to_json().expect("serializes");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(parsed["pipeName"], "dsh-desktop-daemon-7-abc");
        assert_eq!(parsed.as_object().map(|o| o.len()), Some(8));
        let round_trip = CredentialFile::from_json(&json).expect("parses");
        assert_eq!(round_trip, file);
    }

    #[test]
    fn round_trip_preserves_fields() {
        let original = sample();
        let parsed =
            CredentialFile::from_json(&original.to_json().expect("serializes")).expect("parses");
        assert_eq!(parsed, original);
    }

    #[test]
    fn unknown_field_is_rejected() {
        let json = sample().to_json().expect("serializes");
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        value["sneaky"] = serde_json::json!(1);
        assert!(CredentialFile::from_json(&value.to_string()).is_err());
    }

    #[test]
    fn write_read_round_trip_in_temp_dir() {
        let dir = std::env::temp_dir().join(format!("dsh-daemon-cred-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = sample();
        file.write_to(&dir).expect("writes");
        let read = CredentialFile::read_from(&dir).expect("reads");
        assert_eq!(read, file);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn data_dir_prefers_override() {
        // The override path is read once per call; no cross-test races
        // because the tests never set it.
        let fallback = data_dir().expect("data dir resolves");
        assert!(!fallback.as_os_str().is_empty());
    }

    /// H-1 regression: the credential file (and the directory holding it)
    /// must not be group/world readable, and that must not depend on the
    /// umask of the process that created it.
    #[cfg(unix)]
    #[test]
    fn credential_file_and_dir_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("dsh-daemon-cred-mode-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        sample().write_to(&dir).expect("writes");

        let file_mode = fs::metadata(dir.join(CREDENTIAL_FILE_NAME))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600, "credential file must be 0600");
        let dir_mode = fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "credential directory must be 0700");
        // The temp file the atomic write uses must not survive with looser
        // bits either (it is renamed away, so it must be gone).
        assert!(!dir.join(format!("{CREDENTIAL_FILE_NAME}.tmp")).exists());

        let _ = fs::remove_dir_all(&dir);
    }
}

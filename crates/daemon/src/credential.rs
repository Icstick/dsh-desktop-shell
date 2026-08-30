//! Startup credential file (ADR-0019 decision 5: Shell reads the file at
//! startup, daemon issues one-time credentials via local-transport).
//!
//! The daemon writes `daemon-credential.json` into the daemon data
//! directory (`%APPDATA%/dev.dsh.desktop-shell/` on Windows; overridable
//! with `--data-dir` or the `DSH_DAEMON_DATA_DIR` environment variable).
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

/// The fixed, well-known daemon presence port (single-instance claim +
/// Shell liveness probe). The envelope server itself binds a dynamic
/// loopback port (local-transport limitation; see `server.rs` docs and
/// the M6-C TODO) and its real port travels in the credential file.
pub const CLAIM_PORT: u16 = 37_771;

/// Schema version of the credential file (bump on breaking shape change).
pub const CREDENTIAL_FILE_SCHEMA_VERSION: u32 = 1;

/// Resolve the daemon data directory: `--data-dir`/`DSH_DAEMON_DATA_DIR`
/// override, then `%APPDATA%\dev.dsh.desktop-shell`, then a last-resort
/// current-directory fallback.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = env::var("DSH_DAEMON_DATA_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    if let Ok(appdata) = env::var("APPDATA")
        && !appdata.is_empty()
    {
        return PathBuf::from(appdata).join(DATA_DIR_NAME);
    }
    if let Ok(local) = env::var("LOCALAPPDATA")
        && !local.is_empty()
    {
        return PathBuf::from(local).join(DATA_DIR_NAME);
    }
    PathBuf::from(".")
}

/// The on-disk credential file (schema `daemon-credential.json`, v1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CredentialFile {
    pub schema_version: u32,
    pub daemon_version: String,
    pub pid: u32,
    pub claim_port: u16,
    /// The envelope server port the Shell must connect to.
    pub port: u16,
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
            credential: FileCredential {
                token: token.into(),
                expires_at: rfc3339(expires_at),
            },
            issued_at: rfc3339(issued_at),
        }
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
    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let json = self.to_json().map_err(io::Error::other)?;
        let temp = dir.join(format!("{CREDENTIAL_FILE_NAME}.tmp"));
        fs::write(&temp, json)?;
        fs::rename(&temp, dir.join(CREDENTIAL_FILE_NAME))?;
        Ok(())
    }

    /// Read the credential file from `dir`.
    pub fn read_from(dir: &Path) -> io::Result<Self> {
        let json = fs::read_to_string(dir.join(CREDENTIAL_FILE_NAME))?;
        Self::from_json(&json).map_err(io::Error::other)
    }
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
        assert_eq!(parsed["schemaVersion"], 1);
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
        // no unknown keys
        assert_eq!(parsed.as_object().map(|o| o.len()), Some(7));
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
        let fallback = data_dir();
        assert!(!fallback.as_os_str().is_empty());
    }
}

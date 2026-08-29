//! Ephemeral one-time credentials.
//!
//! Tokens are 16 random bytes from the OS CSPRNG (getrandom), hex-encoded
//! under a fixed prefix. A local process that observes one token cannot
//! predict or forge later tokens (FH-1, AC-IPC-001).

use std::time::{Duration, SystemTime};

/// Prefix of every well-formed token.
pub const TOKEN_PREFIX: &str = "lt_";

/// Number of hex characters after the prefix.
pub const TOKEN_HEX_CHARS: usize = 32;

/// A one-time ephemeral credential issued by a [`CredentialIssuer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    token: String,
    expires_at: SystemTime,
}

impl Credential {
    /// Build a credential with an explicit token and expiry (used by
    /// tests and by future carriers that mint their own tokens).
    pub fn new(token: String, expires_at: SystemTime) -> Self {
        Self { token, expires_at }
    }

    /// The opaque token clients must present during the handshake.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The point in time after which the credential is stale.
    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }

    /// Whether the credential is expired at `now` (inclusive).
    pub fn is_expired_at(&self, now: SystemTime) -> bool {
        self.expires_at <= now
    }
}

/// Issues unique, time-limited ephemeral credentials backed by the OS CSPRNG.
#[derive(Debug, Default)]
pub struct CredentialIssuer;

impl CredentialIssuer {
    /// Create a new issuer.
    pub fn new() -> Self {
        Self
    }

    /// Issue a fresh credential valid for `ttl`; `Duration::ZERO` means
    /// already expired (used by tests to exercise the stale path).
    ///
    /// # Panics
    ///
    /// Panics when the OS CSPRNG is unavailable; on Windows/Linux the
    /// getrandom backend always succeeds, so this is a hard invariant.
    pub fn issue(&self, ttl: Duration) -> Credential {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("OS CSPRNG unavailable");
        let now = SystemTime::now();
        let token = format!("{TOKEN_PREFIX}{}", hex(&bytes));
        Credential {
            token,
            expires_at: now + ttl,
        }
    }

    /// Structural validation of a token: prefix + exactly 32 hex chars.
    pub fn is_valid_format(token: &str) -> bool {
        let Some(rest) = token.strip_prefix(TOKEN_PREFIX) else {
            return false;
        };
        rest.len() == TOKEN_HEX_CHARS && rest.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

/// Why a credential was rejected (AC-IPC-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    /// Unknown token (never issued, or consumed and removed).
    Invalid,
    /// Valid token that was already consumed by a previous handshake.
    Replay,
    /// Valid token whose expiry timestamp has passed.
    Stale,
    /// Handshake payload that is not a well-formed hello.
    Malformed,
}

/// Lowercase hex encoding of `bytes` (2 chars per byte).
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_credentials_are_unique_and_well_formed() {
        let issuer = CredentialIssuer::new();
        let a = issuer.issue(Duration::from_secs(60));
        let b = issuer.issue(Duration::from_secs(60));
        assert_ne!(a.token(), b.token());
        for c in [&a, &b] {
            assert!(CredentialIssuer::is_valid_format(c.token()));
            assert_eq!(c.token().len(), TOKEN_PREFIX.len() + TOKEN_HEX_CHARS);
            assert!(!c.is_expired_at(SystemTime::now()));
        }
    }

    #[test]
    fn zero_ttl_is_immediately_expired() {
        let issuer = CredentialIssuer::new();
        let c = issuer.issue(Duration::ZERO);
        assert!(c.is_expired_at(SystemTime::now()));
    }

    #[test]
    fn short_ttl_expires() {
        let issuer = CredentialIssuer::new();
        let c = issuer.issue(Duration::from_millis(10));
        assert!(!c.is_expired_at(SystemTime::now()));
        std::thread::sleep(Duration::from_millis(50));
        assert!(c.is_expired_at(SystemTime::now()));
    }

    #[test]
    fn format_validation_rejects_garbage() {
        let valid = format!("{TOKEN_PREFIX}{}", "a".repeat(TOKEN_HEX_CHARS));
        assert!(CredentialIssuer::is_valid_format(&valid));
        let bad: Vec<String> = vec![
            String::new(),
            "lt_".to_string(),
            "lt_abc".to_string(),
            format!("{TOKEN_PREFIX}{}", "a".repeat(TOKEN_HEX_CHARS - 1)),
            format!("{TOKEN_PREFIX}{}!", "a".repeat(TOKEN_HEX_CHARS)),
            format!("{TOKEN_PREFIX}{}", "z".repeat(TOKEN_HEX_CHARS)),
            "A".repeat(TOKEN_PREFIX.len() + TOKEN_HEX_CHARS),
        ];
        for b in &bad {
            assert!(!CredentialIssuer::is_valid_format(b), "should reject {b:?}");
        }
    }

    #[test]
    fn issuers_differ_across_instances() {
        let i1 = CredentialIssuer::new();
        let i2 = CredentialIssuer::new();
        assert_ne!(
            i1.issue(Duration::ZERO).token(),
            i2.issue(Duration::ZERO).token()
        );
    }
}

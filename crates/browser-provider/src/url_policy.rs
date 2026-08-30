//! Navigation URL policy for the shared browser surface (ADR-0017 decision 3).
//!
//! Only `http`/`https` URLs are navigable, credentials in the authority
//! (userinfo) are rejected, and the URL length is capped at
//! [`MAX_URL_LEN`] characters.

/// Upper bound on navigable URL length in characters (mirrors the report
/// schema `currentUrl` `maxLength: 2048`).
pub const MAX_URL_LEN: usize = 2048;

/// Reasons a candidate URL is rejected by the navigation policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlError {
    /// The URL is the empty string.
    Empty,
    /// The URL has no `scheme://` prefix or the scheme is not http/https.
    UnsupportedScheme,
    /// The authority part contains `@` (userinfo / embedded credentials).
    UserinfoNotAllowed,
    /// The URL is longer than [`MAX_URL_LEN`] characters.
    TooLong,
}

impl std::fmt::Display for UrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Empty => "url must not be empty",
            Self::UnsupportedScheme => "url scheme must be http or https",
            Self::UserinfoNotAllowed => "url must not contain userinfo (credentials)",
            Self::TooLong => "url exceeds the 2048 character limit",
        };
        write!(f, "{message}")
    }
}

impl std::error::Error for UrlError {}

/// HTTP(S)-only navigation policy (stateless validator).
pub struct UrlPolicy;

impl UrlPolicy {
    /// Validate a candidate navigation URL and return it unchanged on success.
    ///
    /// Checks, in order: non-empty, at most [`MAX_URL_LEN`] characters,
    /// `http`/`https` scheme (case-insensitive), and no userinfo
    /// (`@`) in the authority.
    ///
    /// # Errors
    ///
    /// Returns [`UrlError::Empty`], [`UrlError::TooLong`],
    /// [`UrlError::UnsupportedScheme`] or [`UrlError::UserinfoNotAllowed`]
    /// when the URL violates the policy.
    pub fn validate(url: &str) -> Result<String, UrlError> {
        if url.is_empty() {
            return Err(UrlError::Empty);
        }
        if url.chars().count() > MAX_URL_LEN {
            return Err(UrlError::TooLong);
        }
        let rest = match strip_http_scheme(url) {
            Some(rest) => rest,
            None => return Err(UrlError::UnsupportedScheme),
        };
        if authority_has_userinfo(rest) {
            return Err(UrlError::UserinfoNotAllowed);
        }
        Ok(url.to_string())
    }
}

/// Split a `scheme://` prefix; only http/https (case-insensitive) qualify.
fn strip_http_scheme(url: &str) -> Option<&str> {
    let separator = url.find("://")?;
    let scheme = &url[..separator];
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        Some(&url[separator + 3..])
    } else {
        None
    }
}

/// The authority runs from after `://` to the first `/`, `?` or `#`.
/// Any `@` inside it is a userinfo (credential) attempt.
fn authority_has_userinfo(rest: &str) -> bool {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority.contains('@')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_http_and_https() {
        assert_eq!(
            UrlPolicy::validate("https://example.com").unwrap(),
            "https://example.com"
        );
        assert_eq!(
            UrlPolicy::validate("http://example.com/path?q=1#frag").unwrap(),
            "http://example.com/path?q=1#frag"
        );
    }

    #[test]
    fn scheme_is_case_insensitive() {
        for url in [
            "HTTP://EXAMPLE.COM",
            "HtTpS://example.com",
            "https://example.com",
            "http://example.com",
        ] {
            assert!(UrlPolicy::validate(url).is_ok(), "rejected {url}");
        }
    }

    #[test]
    fn rejects_empty_url() {
        assert_eq!(UrlPolicy::validate(""), Err(UrlError::Empty));
    }

    #[test]
    fn rejects_non_http_schemes() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com/file",
        ] {
            assert_eq!(
                UrlPolicy::validate(url),
                Err(UrlError::UnsupportedScheme),
                "accepted {url}"
            );
        }
    }

    #[test]
    fn rejects_urls_without_scheme() {
        for url in ["example.com", "//example.com", "example.com/path"] {
            assert_eq!(
                UrlPolicy::validate(url),
                Err(UrlError::UnsupportedScheme),
                "accepted {url}"
            );
        }
    }

    #[test]
    fn rejects_userinfo_in_authority() {
        for url in [
            "https://user@example.com",
            "https://user:pass@example.com",
            "https://@example.com",
            "http://user@example.com:8080/",
        ] {
            assert_eq!(
                UrlPolicy::validate(url),
                Err(UrlError::UserinfoNotAllowed),
                "accepted {url}"
            );
        }
    }

    #[test]
    fn allows_at_in_path_query_and_fragment() {
        for url in [
            "https://example.com/a@b",
            "https://example.com/path?q=a@b",
            "https://example.com/path#a@b",
        ] {
            assert!(UrlPolicy::validate(url).is_ok(), "rejected {url}");
        }
    }

    #[test]
    fn enforces_length_boundary() {
        let scheme = "https://";
        let at_limit = format!("{scheme}{}", "a".repeat(MAX_URL_LEN - scheme.len()));
        assert_eq!(at_limit.chars().count(), MAX_URL_LEN);
        assert!(UrlPolicy::validate(&at_limit).is_ok());

        let over_limit = format!("{scheme}{}", "a".repeat(MAX_URL_LEN - scheme.len() + 1));
        assert_eq!(over_limit.chars().count(), MAX_URL_LEN + 1);
        assert_eq!(UrlPolicy::validate(&over_limit), Err(UrlError::TooLong));
    }
}

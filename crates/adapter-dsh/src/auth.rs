//! Launch-token authentication (pure): token -> cookie.
//!
//! Verified DSH surface (2026-08-30, D:\deepseek-harness):
//! GET /?token=<base64url 32B> -> 303 Location:/ + Set-Cookie
//! dsh-auth-<base64url(sha256(authority))>=v1.<payload>.<hmac>;
//! HttpOnly; SameSite=Strict; Path=/; Max-Age (30 days default); no Secure.
//! The adapter echoes the cookie verbatim on later requests; signature
//! verification is DSH's job.

use crate::error::AdapterError;
use crate::http::{HttpRequest, HttpResponse};

/// Cookie-name prefix required for the launch cookie (fail-closed check).
pub const LAUNCH_COOKIE_PREFIX: &str = "dsh-auth-";
const MAX_COOKIE_NAME_CHARS: usize = 128;

/// Outcome of a successful launch-token exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthResult {
    /// Cookie header value (name=value) to send on later requests.
    pub cookie: String,
    /// Raw Set-Cookie header from the redirect response (diagnostics).
    pub set_cookie_raw: String,
    /// Redirect Location from the 3xx (usually the app root).
    pub redirect: String,
}

/// Build the token exchange request: GET /?token=<encoded>.
pub fn build_token_request(base_url: &str, token: &str) -> Result<HttpRequest, AdapterError> {
    let path = base_path(base_url)?;
    if token.is_empty() || token.len() > 256 {
        return Err(AdapterError::Auth("launch token out of bounds".to_string()));
    }
    Ok(HttpRequest::get(format!(
        "{path}/?token={}",
        percent_encode(token)
    )))
}

/// Parse the redirect response and extract the launch cookie (pure).
pub fn parse_auth_response(response: &HttpResponse) -> Result<AuthResult, AdapterError> {
    if !(300..400).contains(&response.status) {
        return Err(AdapterError::Auth(format!(
            "expected 3xx redirect, got {}",
            response.status
        )));
    }
    let raw = response
        .set_cookie()
        .ok_or_else(|| AdapterError::Auth("no Set-Cookie on redirect".to_string()))?;
    let name_value = raw.split(';').next().unwrap_or("").trim();
    let (name, value) = name_value
        .split_once('=')
        .ok_or_else(|| AdapterError::Auth("malformed Set-Cookie".to_string()))?;
    if !name.starts_with(LAUNCH_COOKIE_PREFIX)
        || name.len() > MAX_COOKIE_NAME_CHARS
        || value.is_empty()
        || value.len() > 512
    {
        return Err(AdapterError::Auth(format!(
            "unexpected launch cookie name {name:?}"
        )));
    }
    Ok(AuthResult {
        cookie: format!("{name}={value}"),
        set_cookie_raw: raw.to_string(),
        redirect: response.location().unwrap_or("").to_string(),
    })
}

/// Path prefix of a base URL (http://127.0.0.1:6800[/prefix]) -> "" or "/prefix".
pub(crate) fn base_path(base_url: &str) -> Result<String, AdapterError> {
    let rest = base_url
        .strip_prefix("http://")
        .ok_or_else(|| AdapterError::Auth("base_url must use http:// (loopback)".to_string()))?;
    let path = match rest.split_once('/') {
        Some((_, "")) => String::new(),
        Some((_, path)) => format!("/{}", path.trim_end_matches('/')),
        None => String::new(),
    };
    Ok(path)
}

/// RFC 3986 percent-encoding for a URL query value (unreserved kept).
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, set_cookie: Option<&str>, location: Option<&str>) -> HttpResponse {
        let mut headers = Vec::new();
        if let Some(value) = set_cookie {
            headers.push(("Set-Cookie".to_string(), value.to_string()));
        }
        if let Some(value) = location {
            headers.push(("Location".to_string(), value.to_string()));
        }
        HttpResponse {
            status,
            reason: String::new(),
            headers,
            body: Vec::new(),
        }
    }

    #[test]
    fn parses_303_with_launch_cookie() {
        let response = response(
            303,
            Some("dsh-auth-abc=xyz; HttpOnly; SameSite=Strict; Path=/; Max-Age=2592000"),
            Some("/"),
        );
        let result = parse_auth_response(&response).expect("auth");
        assert_eq!(result.cookie, "dsh-auth-abc=xyz");
        assert_eq!(result.redirect, "/");
        assert!(result.set_cookie_raw.contains("Max-Age"));
    }

    #[test]
    fn rejects_non_redirect_and_missing_or_wrong_cookie() {
        let ok = response(200, Some("dsh-auth-a=b"), None);
        assert!(parse_auth_response(&ok).is_err());

        let no_cookie = response(303, None, Some("/"));
        assert!(parse_auth_response(&no_cookie).is_err());

        let wrong_name = response(303, Some("session=abc"), Some("/"));
        assert!(parse_auth_response(&wrong_name).is_err());

        let empty_value = response(303, Some("dsh-auth-a="), Some("/"));
        assert!(parse_auth_response(&empty_value).is_err());
    }

    #[test]
    fn builds_token_request_with_percent_encoding() {
        let request = build_token_request("http://127.0.0.1:6800", "abc-_~./=").expect("request");
        assert_eq!(request.path, "/?token=abc-_~.%2F%3D");
        let request = build_token_request("http://127.0.0.1:6800/prefix", "t").expect("request");
        assert_eq!(request.path, "/prefix/?token=t");
    }

    #[test]
    fn percent_encode_keeps_unreserved_and_encodes_rest() {
        assert_eq!(percent_encode("aZ09-._~"), "aZ09-._~");
        assert_eq!(percent_encode("a b/c"), "a%20b%2Fc");
        assert_eq!(percent_encode("你好"), "%E4%BD%A0%E5%A5%BD");
    }

    #[test]
    fn base_path_extracts_prefix() {
        assert_eq!(base_path("http://127.0.0.1:6800").expect("path"), "");
        assert_eq!(base_path("http://127.0.0.1:6800/").expect("path"), "");
        assert_eq!(
            base_path("http://127.0.0.1:6800/dsh/").expect("path"),
            "/dsh"
        );
        assert!(base_path("https://127.0.0.1:6800").is_err());
        assert!(base_path("127.0.0.1:6800").is_err());
    }
}

//! Minimal loopback HTTP/1.1 client (pure wire codec + transport).
//!
//! DSH is a loopback-only HTTP/WS server; the adapter needs a tiny, sync,
//! dependency-free HTTP client for the launch-token redirect (303) and the
//! /api JSON-RPC calls. The wire codec (request_to_wire, response_from_wire,
//! decode_chunked) is pure and unit-tested without sockets; TcpHttpTransport
//! opens one TCP connection per request and closes it (no keep-alive, no
//! pipelining). HTTPS is out of scope: the DSH surface is loopback HTTP only.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::error::AdapterError;

/// Cap for a decoded response body (defensive; DSH /api bodies are small).
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// One HTTP/1.1 request (client to server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    /// Path and query, e.g. /?token=abc or /api.
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl HttpRequest {
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            method: "GET".to_string(),
            path: path.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn post_json(path: impl Into<String>, body: &serde_json::Value) -> Self {
        Self {
            method: "POST".to_string(),
            path: path.into(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: Some(serde_json::to_vec(body).unwrap_or_default()),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// One HTTP/1.1 response (server to client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn set_cookie(&self) -> Option<&str> {
        self.header("set-cookie")
    }

    pub fn location(&self) -> Option<&str> {
        self.header("location")
    }
}

/// Serialize a request to wire bytes (pure). The transport injects the Host
/// header when the caller did not provide one.
pub fn request_to_wire(request: &HttpRequest) -> Vec<u8> {
    let mut out = format!("{} {} HTTP/1.1\r\n", request.method, request.path);
    for (name, value) in &request.headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = &request.body {
        out.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    out.push_str("Connection: close\r\n\r\n");
    let mut bytes = out.into_bytes();
    if let Some(body) = &request.body {
        bytes.extend_from_slice(body);
    }
    bytes
}

/// Parse a response from wire bytes (pure). Supports Content-Length and
/// chunked transfer encodings; close-delimited bodies are taken as-is.
pub fn response_from_wire(input: &[u8]) -> Result<HttpResponse, AdapterError> {
    let head_end = find_bytes(input, b"\r\n\r\n")
        .ok_or_else(|| AdapterError::Protocol("http: truncated head".to_string()))?;
    let head = std::str::from_utf8(&input[..head_end])
        .map_err(|_| AdapterError::Protocol("http: non-utf8 head".to_string()))?;
    let mut lines = head.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| AdapterError::Protocol("http: empty head".to_string()))?;
    let mut parts = status_line.splitn(3, ' ');
    let _version = parts.next();
    let status: u16 = parts
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| AdapterError::Protocol("http: bad status line".to_string()))?;
    let reason = parts.next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    let body_start = head_end + 4;
    let body = if input.len() < body_start {
        Vec::new()
    } else {
        let rest = &input[body_start..];
        if is_chunked(&headers) {
            decode_chunked(rest)?
        } else {
            match content_length(&headers) {
                Some(length) => rest.get(..length).unwrap_or(rest).to_vec(),
                None => rest.to_vec(),
            }
        }
    };
    Ok(HttpResponse {
        status,
        reason,
        headers,
        body,
    })
}

/// Decode a chunked transfer-encoding body (pure). Trailers are ignored.
pub fn decode_chunked(input: &[u8]) -> Result<Vec<u8>, AdapterError> {
    let mut out = Vec::new();
    let mut rest = input;
    loop {
        let line_end = find_bytes(rest, b"\r\n")
            .ok_or_else(|| AdapterError::Protocol("http: chunked size line missing".to_string()))?;
        let size_line = std::str::from_utf8(&rest[..line_end])
            .map_err(|_| AdapterError::Protocol("http: chunked size not utf8".to_string()))?;
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let size: u64 = u64::from_str_radix(size_hex, 16)
            .map_err(|_| AdapterError::Protocol("http: bad chunk size".to_string()))?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        let size = usize::try_from(size)
            .map_err(|_| AdapterError::Protocol("http: chunk size overflow".to_string()))?;
        if rest.len() < size + 2 {
            return Err(AdapterError::Protocol("http: truncated chunk".to_string()));
        }
        if out.len() + size > MAX_BODY_BYTES {
            return Err(AdapterError::Protocol("http: body exceeds cap".to_string()));
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

fn is_chunked(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding")
            && value.to_ascii_lowercase().contains("chunked")
    })
}

fn content_length(headers: &[(String, String)]) -> Option<usize> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// One request per connection, Connection: close (loopback only).
pub struct TcpHttpTransport {
    addr: SocketAddr,
    timeout: Duration,
}

impl TcpHttpTransport {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            timeout: Duration::from_secs(10),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn roundtrip_once(&self, request: &HttpRequest) -> Result<HttpResponse, AdapterError> {
        let mut stream = TcpStream::connect_timeout(&self.addr, self.timeout)
            .map_err(|e| AdapterError::Transport(format!("tcp connect: {e}")))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|e| AdapterError::Transport(format!("tcp timeout: {e}")))?;
        let mut request = request.clone();
        if !request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("host"))
        {
            request
                .headers
                .push(("Host".to_string(), self.addr.to_string()));
        }
        let wire = request_to_wire(&request);
        stream
            .write_all(&wire)
            .map_err(|e| AdapterError::Transport(format!("tcp write: {e}")))?;
        let mut bytes = Vec::new();
        stream
            .read_to_end(&mut bytes)
            .map_err(|e| AdapterError::Transport(format!("tcp read: {e}")))?;
        response_from_wire(&bytes)
    }
}

/// Pluggable HTTP transport; tests use an in-memory scripted fake.
pub trait HttpTransport {
    fn roundtrip(&mut self, request: &HttpRequest) -> Result<HttpResponse, AdapterError>;
}

impl HttpTransport for TcpHttpTransport {
    fn roundtrip(&mut self, request: &HttpRequest) -> Result<HttpResponse, AdapterError> {
        self.roundtrip_once(request)
    }
}

impl<T: HttpTransport + ?Sized> HttpTransport for &mut T {
    fn roundtrip(&mut self, request: &HttpRequest) -> Result<HttpResponse, AdapterError> {
        (**self).roundtrip(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_to_wire_get_includes_headers() {
        let request = HttpRequest::get("/?token=abc").with_header("Host", "127.0.0.1:6800");
        let wire = request_to_wire(&request);
        let text = String::from_utf8(wire).expect("utf8");
        assert!(text.starts_with("GET /?token=abc HTTP/1.1\r\n"));
        assert!(text.contains("Host: 127.0.0.1:6800\r\n"));
        assert!(text.ends_with("Connection: close\r\n\r\n"));
    }

    #[test]
    fn request_to_wire_post_json_sets_content_type_and_length() {
        let request = HttpRequest::post_json("/api", &serde_json::json!({"a": 1}));
        let wire = request_to_wire(&request);
        let text = String::from_utf8(wire).expect("utf8");
        assert!(text.starts_with("POST /api HTTP/1.1\r\n"));
        assert!(text.contains("Content-Type: application/json\r\n"));
        assert!(text.contains("Content-Length: 7\r\n"));
        assert!(text.ends_with("{\"a\":1}"));
    }

    #[test]
    fn response_from_wire_303_with_set_cookie() {
        let wire = b"HTTP/1.1 303 See Other\r\nLocation: /\r\nSet-Cookie: dsh-auth-abc=xyz; HttpOnly; Path=/\r\nContent-Length: 0\r\n\r\n";
        let response = response_from_wire(wire).expect("parse");
        assert_eq!(response.status, 303);
        assert_eq!(
            response.set_cookie(),
            Some("dsh-auth-abc=xyz; HttpOnly; Path=/")
        );
        assert_eq!(response.location(), Some("/"));
        assert!(response.body.is_empty());
    }

    #[test]
    fn response_from_wire_content_length_body() {
        let body = b"{\"ok\":true}";
        let mut wire = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n".to_vec();
        wire.extend_from_slice(body);
        let response = response_from_wire(&wire).expect("parse");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, body);
    }

    #[test]
    fn response_from_wire_chunked_body() {
        let wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
        let response = response_from_wire(wire).expect("parse");
        assert_eq!(response.body, b"Wikipedia");
    }

    #[test]
    fn decode_chunked_handles_extensions_and_trailers() {
        let input = b"4;ext=1\r\nWiki\r\n5\r\npedia\r\n0\r\nX-Trailer: yes\r\n\r\n";
        assert_eq!(decode_chunked(input).expect("decode"), b"Wikipedia");
    }

    #[test]
    fn decode_chunked_rejects_malformed_input() {
        assert!(decode_chunked(b"zz\r\n").is_err());
        assert!(decode_chunked(b"10\r\nabc").is_err());
        assert!(decode_chunked(b"0").is_err());
    }

    #[test]
    fn response_from_wire_rejects_truncated_head() {
        assert!(response_from_wire(b"HTTP/1.1 200").is_err());
        assert!(response_from_wire(b"GARBAGE\r\n\r\n").is_err());
    }

    #[test]
    fn response_from_wire_tolerates_extra_headers() {
        let wire = b"HTTP/1.1 200 OK\r\nX-Extra: 1\r\nX-More: 2\r\nContent-Length: 0\r\n\r\n";
        let response = response_from_wire(wire).expect("parse");
        assert_eq!(response.status, 200);
        assert_eq!(response.header("x-extra"), Some("1"));
    }
}

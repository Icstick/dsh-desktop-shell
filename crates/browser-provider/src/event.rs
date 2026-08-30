//! Browser lifecycle events (pushed to the surface as `browser://event`).
//!
//! The bridge layer serializes these with serde on its side; this crate
//! provides a dependency-free JSON encoding for tests and lightweight use.

/// Kinds of browser events (M4C-CONTRACT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserEventKind {
    /// A navigation was accepted and the session moved to `loading`.
    NavigationChanged,
    /// The current navigation failed and the session moved to `error`.
    LoadFailed,
    /// The session was closed.
    Closed,
}

impl BrowserEventKind {
    /// Stable wire name for the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NavigationChanged => "navigation_changed",
            Self::LoadFailed => "load_failed",
            Self::Closed => "closed",
        }
    }
}

/// One auditable browser event (ADR-0017 decision 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserEvent {
    /// Opaque session id this event belongs to.
    pub session_id: String,
    /// What happened.
    pub kind: BrowserEventKind,
    /// When it happened (unix ms).
    pub occurred_at_unix_ms: u64,
    /// The URL involved; `None` for `closed` events.
    pub url: Option<String>,
}

impl BrowserEvent {
    /// Minimal JSON encoding (camelCase, matching the report schemas).
    ///
    /// Field names mirror `browser-report.schema.json` conventions so the
    /// bridge can forward events without re-mapping.
    pub fn to_json(&self) -> String {
        let url = match &self.url {
            Some(url) => format!("\"{}\"", json_escape(url)),
            None => "null".to_string(),
        };
        format!(
            "{{\"sessionId\":\"{}\",\"kind\":\"{}\",\"occurredAtUnixMs\":{},\"url\":{url}}}",
            json_escape(&self.session_id),
            self.kind.as_str(),
            self.occurred_at_unix_ms
        )
    }
}

/// Escape a string for embedding in JSON (quotes, backslash, control chars).
fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let hex = format!("{:04x}", c as u32);
                out.push_str("\\u");
                out.push_str(&hex);
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_wire_names_are_stable() {
        assert_eq!(
            BrowserEventKind::NavigationChanged.as_str(),
            "navigation_changed"
        );
        assert_eq!(BrowserEventKind::LoadFailed.as_str(), "load_failed");
        assert_eq!(BrowserEventKind::Closed.as_str(), "closed");
    }

    #[test]
    fn serializes_navigation_event() {
        let event = BrowserEvent {
            session_id: "brw-1000-1".to_string(),
            kind: BrowserEventKind::NavigationChanged,
            occurred_at_unix_ms: 1234,
            url: Some("https://example.com".to_string()),
        };
        assert_eq!(
            event.to_json(),
            "{\"sessionId\":\"brw-1000-1\",\"kind\":\"navigation_changed\",\"occurredAtUnixMs\":1234,\"url\":\"https://example.com\"}"
        );
    }

    #[test]
    fn serializes_closed_event_with_null_url() {
        let event = BrowserEvent {
            session_id: "brw-1000-1".to_string(),
            kind: BrowserEventKind::Closed,
            occurred_at_unix_ms: 5678,
            url: None,
        };
        assert_eq!(
            event.to_json(),
            "{\"sessionId\":\"brw-1000-1\",\"kind\":\"closed\",\"occurredAtUnixMs\":5678,\"url\":null}"
        );
    }

    #[test]
    fn escapes_quotes_and_backslashes_in_url() {
        let event = BrowserEvent {
            session_id: "brw-1-2".to_string(),
            kind: BrowserEventKind::NavigationChanged,
            occurred_at_unix_ms: 1,
            url: Some("https://example.com/?q=\"a\\b\"".to_string()),
        };
        let json = event.to_json();
        assert!(json.contains("?q=\\\"a\\\\b\\\""));
        assert!(!json.contains("?q=\"a"));
    }
}

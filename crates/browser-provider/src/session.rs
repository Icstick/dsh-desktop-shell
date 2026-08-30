//! Browser session state machine and registry (MOD-BROWSER-PROVIDER, ADR-0017).
//!
//! Sessions follow: `create -> loading -> ready`, `close -> closed` and
//! `load failure -> error`. An `error` session can recover via a new
//! navigation; a `closed` session rejects every operation. Unknown ids
//! yield [`BrowserError::NotFound`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::event::{BrowserEvent, BrowserEventKind};
use crate::url_policy::{UrlError, UrlPolicy};

/// Session lifecycle states (mirror `browser-report.schema.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Session created; no navigation yet.
    Created,
    /// A navigation is in flight.
    Loading,
    /// The current page finished loading.
    Ready,
    /// The session was closed; no further operations are accepted.
    Closed,
    /// The current navigation failed; a new navigation can recover.
    Error,
}

impl SessionState {
    /// Stable wire name for the state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Closed => "closed",
            Self::Error => "error",
        }
    }
}

/// Opaque session handle; never exposes profile paths or process details
/// (ADR-0017 decision 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSession {
    /// Opaque id, `brw-<unix_ms>-<seq>`.
    pub session_id: String,
    /// Current lifecycle state.
    pub state: SessionState,
    /// Last accepted navigation URL, if any.
    pub current_url: Option<String>,
    /// Creation time (unix ms).
    pub created_at_unix_ms: u64,
    /// Last state-changing activity (unix ms).
    pub last_activity_unix_ms: Option<u64>,
    /// Last load failure message, if the session is in `error`.
    pub error: Option<String>,
}

/// Errors returned by the browser provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserError {
    /// The session id is unknown to the registry.
    NotFound,
    /// The URL violates the navigation policy (see [`UrlPolicy`]).
    InvalidUrl(UrlError),
    /// The session is closed.
    Closed,
    /// Internal state is unavailable (e.g. poisoned lock).
    Other,
}

impl std::fmt::Display for BrowserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NotFound => "unknown browser session",
            Self::InvalidUrl(cause) => {
                return write!(f, "url rejected by navigation policy: {cause}");
            }
            Self::Closed => "browser session is closed",
            Self::Other => "browser provider state unavailable",
        };
        write!(f, "{message}")
    }
}

impl std::error::Error for BrowserError {}

/// Provider contract for the shared browser surface (M4C-CONTRACT, ADR-0017
/// decision 4). The host picks the profile before creating a provider; the
/// actual webview/process work happens behind this trait.
pub trait BrowserProvider {
    /// Create a new session; the profile is chosen by the host.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::Other`] when the registry state is
    /// unavailable.
    fn create(&mut self) -> Result<BrowserSession, BrowserError>;

    /// Navigate an existing session to `url` (validated by
    /// [`UrlPolicy`]) and move it to `loading`.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids,
    /// [`BrowserError::Closed`] for closed sessions and
    /// [`BrowserError::InvalidUrl`] when the URL violates the policy.
    fn navigate(&mut self, session_id: &str, url: &str) -> Result<BrowserSession, BrowserError>;

    /// Return the current page text snapshot for a session.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids and
    /// [`BrowserError::Closed`] for closed sessions.
    fn snapshot_text(&mut self, session_id: &str) -> Result<String, BrowserError>;

    /// Close a session and move it to `closed`.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids and
    /// [`BrowserError::Closed`] when the session is already closed.
    fn close(&mut self, session_id: &str) -> Result<BrowserSession, BrowserError>;
}

/// Owns every browser session; enforces the state machine and records the
/// audit event history. Thread-safe: all methods take `&self`.
pub struct SessionRegistry {
    next_seq: AtomicU64,
    sessions: Mutex<HashMap<String, Session>>,
    events: Mutex<Vec<BrowserEvent>>,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            next_seq: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
        }
    }

    /// Create a new session in `created` state.
    ///
    /// The id is opaque: `brw-<unix_ms>-<seq>` (ADR-0017 decision 5).
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::Other`] when the registry state is
    /// unavailable.
    pub fn create(&self) -> Result<BrowserSession, BrowserError> {
        let now = unix_ms();
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let id = format!("brw-{now}-{seq}");
        let session = Session {
            id: id.clone(),
            state: SessionState::Created,
            current_url: None,
            created_at_unix_ms: now,
            last_activity_unix_ms: None,
            error: None,
            snapshot: None,
        };
        self.sessions_guard()?.insert(id.clone(), session);
        Ok(BrowserSession {
            session_id: id,
            state: SessionState::Created,
            current_url: None,
            created_at_unix_ms: now,
            last_activity_unix_ms: None,
            error: None,
        })
    }

    /// Validate and accept a navigation: `-> loading` with the URL
    /// recorded and a `navigation_changed` event pushed.
    ///
    /// An `error` session recovers through a new navigation.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids,
    /// [`BrowserError::Closed`] for closed sessions and
    /// [`BrowserError::InvalidUrl`] when the URL violates the policy
    /// (the session state is left untouched in that case).
    pub fn navigate(&self, session_id: &str, url: &str) -> Result<BrowserSession, BrowserError> {
        let url = UrlPolicy::validate(url).map_err(BrowserError::InvalidUrl)?;
        let report = {
            let mut guard = self.sessions_guard()?;
            let session = Self::live_session(&mut guard, session_id)?;
            session.state = SessionState::Loading;
            session.current_url = Some(url.clone());
            session.error = None;
            session.snapshot = None;
            touch(session);
            session.report()
        };
        self.push_event(BrowserEvent {
            session_id: session_id.to_string(),
            kind: BrowserEventKind::NavigationChanged,
            occurred_at_unix_ms: unix_ms(),
            url: Some(url),
        });
        Ok(report)
    }

    /// Mark a `loading` session as `ready` (host calls this when
    /// the webview reports navigation completed). No-op for sessions that
    /// are not loading.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids.
    pub fn mark_ready(&self, session_id: &str) -> Result<BrowserSession, BrowserError> {
        let mut guard = self.sessions_guard()?;
        let session = Self::live_session(&mut guard, session_id)?;
        if session.state == SessionState::Loading {
            session.state = SessionState::Ready;
            touch(session);
        }
        Ok(session.report())
    }

    /// Mark the current navigation as failed: `-> error` with the
    /// message recorded (truncated to 256 characters, mirroring the report
    /// schema `error` `maxLength: 256`) and a `load_failed`
    /// event pushed.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids and
    /// [`BrowserError::Closed`] for closed sessions.
    pub fn mark_load_failed(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<BrowserSession, BrowserError> {
        let truncated: String = message.chars().take(MAX_ERROR_LEN).collect();
        let url = {
            let mut guard = self.sessions_guard()?;
            let session = Self::live_session(&mut guard, session_id)?;
            session.state = SessionState::Error;
            session.error = Some(truncated.clone());
            touch(session);
            session.current_url.clone()
        };
        self.push_event(BrowserEvent {
            session_id: session_id.to_string(),
            kind: BrowserEventKind::LoadFailed,
            occurred_at_unix_ms: unix_ms(),
            url,
        });
        self.get(session_id)
    }

    /// Record the current page text snapshot for a session (host writes the
    /// extracted page text; [`Self::snapshot_text`] reads it).
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids and
    /// [`BrowserError::Closed`] for closed sessions.
    pub fn set_snapshot(
        &self,
        session_id: &str,
        text: &str,
    ) -> Result<BrowserSession, BrowserError> {
        let mut guard = self.sessions_guard()?;
        let session = Self::live_session(&mut guard, session_id)?;
        session.snapshot = Some(text.to_string());
        touch(session);
        Ok(session.report())
    }

    /// Return the current page text snapshot (empty until the host records
    /// one).
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids and
    /// [`BrowserError::Closed`] for closed sessions.
    pub fn snapshot_text(&self, session_id: &str) -> Result<String, BrowserError> {
        let mut guard = self.sessions_guard()?;
        let session = Self::live_session(&mut guard, session_id)?;
        Ok(session.snapshot.clone().unwrap_or_default())
    }

    /// Close a session: `-> closed` with a `closed` event
    /// pushed. The session stays in the registry so its final state can be
    /// queried; a second close returns [`BrowserError::Closed`].
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids and
    /// [`BrowserError::Closed`] when the session is already closed.
    pub fn close(&self, session_id: &str) -> Result<BrowserSession, BrowserError> {
        let report = {
            let mut guard = self.sessions_guard()?;
            let session = Self::live_session(&mut guard, session_id)?;
            session.state = SessionState::Closed;
            touch(session);
            session.report()
        };
        self.push_event(BrowserEvent {
            session_id: session_id.to_string(),
            kind: BrowserEventKind::Closed,
            occurred_at_unix_ms: unix_ms(),
            url: None,
        });
        Ok(report)
    }

    /// Latest report for one session.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NotFound`] for unknown ids.
    pub fn get(&self, session_id: &str) -> Result<BrowserSession, BrowserError> {
        let guard = self.sessions_guard()?;
        let session = guard.get(session_id).ok_or(BrowserError::NotFound)?;
        Ok(session.report())
    }

    /// Reports for all sessions (surface restore/list).
    pub fn list(&self) -> Vec<BrowserSession> {
        let guard = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<BrowserSession> = guard.values().map(Session::report).collect();
        out.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        out
    }

    /// All recorded events, oldest first (clone; never drains).
    pub fn events(&self) -> Vec<BrowserEvent> {
        self.events
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// All recorded events, oldest first, and clears the history.
    pub fn drain_events(&self) -> Vec<BrowserEvent> {
        self.events
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default()
    }

    /// Borrow the session map.
    fn sessions_guard(&self) -> Result<MutexGuard<'_, HashMap<String, Session>>, BrowserError> {
        self.sessions.lock().map_err(|_| BrowserError::Other)
    }

    /// Fetch a session that is not closed.
    fn live_session<'a>(
        guard: &'a mut HashMap<String, Session>,
        session_id: &str,
    ) -> Result<&'a mut Session, BrowserError> {
        let session = guard.get_mut(session_id).ok_or(BrowserError::NotFound)?;
        if session.state == SessionState::Closed {
            return Err(BrowserError::Closed);
        }
        Ok(session)
    }

    /// Append one event to the audit history.
    fn push_event(&self, event: BrowserEvent) {
        if let Ok(mut guard) = self.events.lock() {
            guard.push(event);
        }
    }
}

impl BrowserProvider for SessionRegistry {
    fn create(&mut self) -> Result<BrowserSession, BrowserError> {
        SessionRegistry::create(self)
    }

    fn navigate(&mut self, session_id: &str, url: &str) -> Result<BrowserSession, BrowserError> {
        SessionRegistry::navigate(self, session_id, url)
    }

    fn snapshot_text(&mut self, session_id: &str) -> Result<String, BrowserError> {
        SessionRegistry::snapshot_text(self, session_id)
    }

    fn close(&mut self, session_id: &str) -> Result<BrowserSession, BrowserError> {
        SessionRegistry::close(self, session_id)
    }
}

/// Upper bound on the `error` message length (report schema
/// `maxLength: 256`).
pub const MAX_ERROR_LEN: usize = 256;

/// Internal registry entry: public report plus the page text snapshot.
#[derive(Debug)]
struct Session {
    id: String,
    state: SessionState,
    current_url: Option<String>,
    created_at_unix_ms: u64,
    last_activity_unix_ms: Option<u64>,
    error: Option<String>,
    snapshot: Option<String>,
}

impl Session {
    fn report(&self) -> BrowserSession {
        BrowserSession {
            session_id: self.id.clone(),
            state: self.state,
            current_url: self.current_url.clone(),
            created_at_unix_ms: self.created_at_unix_ms,
            last_activity_unix_ms: self.last_activity_unix_ms,
            error: self.error.clone(),
        }
    }
}

fn touch(session: &mut Session) {
    session.last_activity_unix_ms = Some(unix_ms());
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Validate the opaque id shape (ADR-0017 decision 5).
    fn assert_opaque_id(id: &str) {
        assert!(id.starts_with("brw-"), "id must start with brw-: {id}");
        let rest = &id[4..];
        let mut parts = rest.split('-');
        let ms = parts.next().expect("ms part");
        let seq = parts.next().expect("seq part");
        assert!(
            parts.next().is_none(),
            "id must have exactly two parts: {id}"
        );
        assert!(
            !ms.is_empty() && ms.chars().all(|c| c.is_ascii_digit()),
            "ms must be digits: {id}"
        );
        assert!(
            !seq.is_empty() && seq.chars().all(|c| c.is_ascii_digit()),
            "seq must be digits: {id}"
        );
    }

    #[test]
    fn create_returns_opaque_id_in_created_state() {
        let registry = SessionRegistry::new();
        let session = registry.create().unwrap();
        assert_opaque_id(&session.session_id);
        assert_eq!(session.state, SessionState::Created);
        assert_eq!(session.current_url, None);
        assert_eq!(session.error, None);
        assert_eq!(session.last_activity_unix_ms, None);
        assert!(session.created_at_unix_ms > 0);
    }

    #[test]
    fn ids_are_unique_and_seq_increases() {
        let registry = SessionRegistry::new();
        let a = registry.create().unwrap();
        let b = registry.create().unwrap();
        assert_ne!(a.session_id, b.session_id);
        let seq_of = |id: &str| id.rsplit('-').next().unwrap().parse::<u64>().unwrap();
        assert!(seq_of(&b.session_id) > seq_of(&a.session_id));
    }

    #[test]
    fn state_machine_create_loading_ready() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        let loading = registry
            .navigate(&created.session_id, "https://example.com")
            .unwrap();
        assert_eq!(loading.state, SessionState::Loading);
        assert_eq!(loading.current_url.as_deref(), Some("https://example.com"));
        let ready = registry.mark_ready(&created.session_id).unwrap();
        assert_eq!(ready.state, SessionState::Ready);
    }

    #[test]
    fn navigate_updates_url_and_activity() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        let navigated = registry
            .navigate(&created.session_id, "https://example.com/page")
            .unwrap();
        assert_eq!(
            navigated.current_url.as_deref(),
            Some("https://example.com/page")
        );
        assert!(navigated.last_activity_unix_ms.is_some());
        assert!(navigated.last_activity_unix_ms.unwrap() >= navigated.created_at_unix_ms);
    }

    #[test]
    fn mark_load_failed_moves_to_error_with_message() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        registry
            .navigate(&created.session_id, "https://example.com")
            .unwrap();
        let failed = registry
            .mark_load_failed(&created.session_id, "connection reset")
            .unwrap();
        assert_eq!(failed.state, SessionState::Error);
        assert_eq!(failed.error.as_deref(), Some("connection reset"));
        assert_eq!(failed.current_url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn error_message_is_truncated_to_256_chars() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        let long = "x".repeat(300);
        let failed = registry
            .mark_load_failed(&created.session_id, &long)
            .unwrap();
        assert_eq!(
            failed.error.as_deref().unwrap().chars().count(),
            MAX_ERROR_LEN
        );
    }

    #[test]
    fn error_session_recovers_via_new_navigation() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        registry
            .navigate(&created.session_id, "https://example.com")
            .unwrap();
        registry
            .mark_load_failed(&created.session_id, "boom")
            .unwrap();
        let recovered = registry
            .navigate(&created.session_id, "https://example.org")
            .unwrap();
        assert_eq!(recovered.state, SessionState::Loading);
        assert_eq!(recovered.error, None);
    }

    #[test]
    fn close_moves_to_closed_and_rejects_further_operations() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        let closed = registry.close(&created.session_id).unwrap();
        assert_eq!(closed.state, SessionState::Closed);
        assert_eq!(
            registry.navigate(&created.session_id, "https://example.com"),
            Err(BrowserError::Closed)
        );
        assert_eq!(
            registry.snapshot_text(&created.session_id),
            Err(BrowserError::Closed)
        );
        assert_eq!(
            registry.mark_ready(&created.session_id),
            Err(BrowserError::Closed)
        );
        assert_eq!(
            registry.close(&created.session_id),
            Err(BrowserError::Closed)
        );
    }

    #[test]
    fn closed_session_stays_queryable() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        registry.close(&created.session_id).unwrap();
        let report = registry.get(&created.session_id).unwrap();
        assert_eq!(report.state, SessionState::Closed);
        assert!(
            registry
                .list()
                .iter()
                .any(|s| s.state == SessionState::Closed)
        );
    }

    #[test]
    fn unknown_session_returns_not_found() {
        let registry = SessionRegistry::new();
        assert_eq!(registry.get("brw-0-0"), Err(BrowserError::NotFound));
        assert_eq!(
            registry.navigate("brw-0-0", "https://example.com"),
            Err(BrowserError::NotFound)
        );
        assert_eq!(
            registry.snapshot_text("brw-0-0"),
            Err(BrowserError::NotFound)
        );
        assert_eq!(registry.close("brw-0-0"), Err(BrowserError::NotFound));
        assert_eq!(registry.mark_ready("brw-0-0"), Err(BrowserError::NotFound));
        assert_eq!(
            registry.mark_load_failed("brw-0-0", "boom"),
            Err(BrowserError::NotFound)
        );
    }

    #[test]
    fn navigate_rejects_invalid_url_without_state_change() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        assert_eq!(
            registry.navigate(&created.session_id, "file:///etc/passwd"),
            Err(BrowserError::InvalidUrl(UrlError::UnsupportedScheme))
        );
        assert_eq!(
            registry.navigate(&created.session_id, "https://user@example.com"),
            Err(BrowserError::InvalidUrl(UrlError::UserinfoNotAllowed))
        );
        let report = registry.get(&created.session_id).unwrap();
        assert_eq!(report.state, SessionState::Created);
        assert_eq!(report.current_url, None);
    }

    #[test]
    fn snapshot_roundtrip_and_initial_empty() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        assert_eq!(registry.snapshot_text(&created.session_id).unwrap(), "");
        registry
            .set_snapshot(&created.session_id, "hello browser")
            .unwrap();
        assert_eq!(
            registry.snapshot_text(&created.session_id).unwrap(),
            "hello browser"
        );
    }

    #[test]
    fn events_recorded_in_order_with_kinds_and_urls() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        registry
            .navigate(&created.session_id, "https://a.example")
            .unwrap();
        registry
            .navigate(&created.session_id, "https://b.example")
            .unwrap();
        registry
            .mark_load_failed(&created.session_id, "boom")
            .unwrap();
        registry.close(&created.session_id).unwrap();

        let events = registry.events();
        let kinds: Vec<BrowserEventKind> = events.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BrowserEventKind::NavigationChanged,
                BrowserEventKind::NavigationChanged,
                BrowserEventKind::LoadFailed,
                BrowserEventKind::Closed,
            ]
        );
        assert_eq!(events[0].url.as_deref(), Some("https://a.example"));
        assert_eq!(events[1].url.as_deref(), Some("https://b.example"));
        assert_eq!(events[2].url.as_deref(), Some("https://b.example"));
        assert_eq!(events[3].url, None);
        assert!(events.iter().all(|e| e.session_id == created.session_id));
        let mut times: Vec<u64> = events.iter().map(|e| e.occurred_at_unix_ms).collect();
        times.sort_unstable();
        assert_eq!(
            times,
            events
                .iter()
                .map(|e| e.occurred_at_unix_ms)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn drain_events_clears_history() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        registry
            .navigate(&created.session_id, "https://example.com")
            .unwrap();
        assert_eq!(registry.drain_events().len(), 1);
        assert!(registry.events().is_empty());
    }

    #[test]
    fn mark_ready_is_noop_when_not_loading() {
        let registry = SessionRegistry::new();
        let created = registry.create().unwrap();
        let report = registry.mark_ready(&created.session_id).unwrap();
        assert_eq!(report.state, SessionState::Created);
        registry
            .navigate(&created.session_id, "https://example.com")
            .unwrap();
        registry.mark_ready(&created.session_id).unwrap();
        let again = registry.mark_ready(&created.session_id).unwrap();
        assert_eq!(again.state, SessionState::Ready);
    }

    #[test]
    fn list_sorts_by_session_id() {
        let registry = SessionRegistry::new();
        let a = registry.create().unwrap();
        let b = registry.create().unwrap();
        let all = registry.list();
        assert_eq!(all.len(), 2);
        assert!(all[0].session_id < all[1].session_id);
        assert!(all.iter().any(|s| s.session_id == a.session_id));
        assert!(all.iter().any(|s| s.session_id == b.session_id));
    }

    #[test]
    fn provider_trait_dispatch_works() {
        let mut provider: Box<dyn BrowserProvider> = Box::new(SessionRegistry::new());
        let created = provider.create().unwrap();
        let navigated = provider
            .navigate(&created.session_id, "https://example.com")
            .unwrap();
        assert_eq!(navigated.state, SessionState::Loading);
        assert_eq!(provider.snapshot_text(&created.session_id).unwrap(), "");
        let closed = provider.close(&created.session_id).unwrap();
        assert_eq!(closed.state, SessionState::Closed);
    }

    #[test]
    fn state_and_error_display_messages() {
        assert_eq!(SessionState::Created.as_str(), "created");
        assert_eq!(SessionState::Loading.as_str(), "loading");
        assert_eq!(SessionState::Ready.as_str(), "ready");
        assert_eq!(SessionState::Closed.as_str(), "closed");
        assert_eq!(SessionState::Error.as_str(), "error");
        assert_eq!(
            BrowserError::NotFound.to_string(),
            "unknown browser session"
        );
        assert_eq!(
            BrowserError::Closed.to_string(),
            "browser session is closed"
        );
    }
}

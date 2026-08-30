//! Pure-logic shared browser surface provider (MOD-BROWSER-PROVIDER, ADR-0017).
//!
//! This crate owns browser session lifecycle bookkeeping and the navigation
//! URL policy only. It never touches WebView2, CDP, profiles or any browser
//! data: the host (apps/desktop) implements the actual webview behind the
//! [`BrowserProvider`] trait (ADR-0017 decision 4).
//!
//! Session ids are opaque (`brw-<unix_ms>-<seq>`) and never expose profile
//! paths or process details (ADR-0017 decision 5, AC-BRW-001). Navigation is
//! HTTP(S)-only, userinfo is rejected and URLs are capped at 2048 characters
//! (ADR-0017 decision 3).

pub mod event;
pub mod session;
pub mod url_policy;

pub use event::{BrowserEvent, BrowserEventKind};
pub use session::{BrowserError, BrowserProvider, BrowserSession, SessionRegistry, SessionState};
pub use url_policy::{UrlError, UrlPolicy};

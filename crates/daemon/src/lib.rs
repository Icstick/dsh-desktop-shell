//! # dsh-daemon
//!
//! The dsh-desktop-shell **daemon** (ADR-0019 decision 1): a standalone,
//! UI-less process that owns the shared resources (DSH process tree, PTY
//! registry, browser session state, broker authority — migrated in M6-C)
//! and exposes them through the **unified external API**: a
//! `dsh-local-transport` envelope server (ADR-0019 decision 5).
//!
//! M6-B1 scope (this crate):
//!
//! - [`server`] — the envelope server: `serve_connection` /
//!   `handle_envelope` + per-connection [`SessionState`], ported from
//!   `crates/external-api-example` with the same test semantics, but the
//!   static `GrantPolicy` is replaced by the **broker-driven** chain
//!   (ADR-0018 decision 7 / M5-E1): every Hello maps into
//!   `dsh_supervisor::Broker::broker_grant_from_negotiation` (grants +
//!   bounded leases), every Invocation passes the ADR-0014 dispatch gate
//!   (`Broker::enforce_dispatch`) before the capability handler runs.
//! - [`capabilities`] — the capability surface: `system.ping`,
//!   `daemon.status`, `browser.*` (real since M6-C3), `terminal.*`
//!   (real since M6-C1), `runtime.*` (real since M6-C2),
//!   `scheduler.wake`/`scheduler.cancel` (M6-D).
//! - [`credential`] — the one-time credential + the credential file the
//!   Shell reads at startup (under `%APPDATA%/dev.dsh.desktop-shell/`,
//!   ADR-0019 decision 5).
//! - [`singleton`] — single-instance guard: fixed claim port 37771
//!   ownership + a start lock file (ADR-0019 decision 4; named-mutex and
//!   split-brain tests are M6-D).
//! - [`events`] — daemon-internal event routing (M6-C1): terminal
//!   output and browser lifecycle events are routed by session id to the
//!   subscriber connection.
//! - [`terminal`] — M6-C1: the real terminal capability (PTY hosting,
//!   envelope methods, output events).
//! - [`browser`] — M6-C3: the browser session state authority
//!   (SessionRegistry + lifecycle events; rendering stays in the Shell,
//!   ADR-0019 decision 2).
//! - [`runtime`] — M6-C2: the Managed DSH runtime authority (DSH
//!   process tree + environment catalog; envelope methods
//!   `runtime.start`/`runtime.status`/`runtime.stop`/`runtime.restart`).
//!
//! The daemon does not depend on tauri; the Shell stays the only tauri
//! process (ADR-0019 decision 2/3).

#![forbid(unsafe_code)]

pub mod browser;
pub mod capabilities;
pub mod credential;
pub mod envelope;
pub mod events;
pub mod runtime;
pub mod scheduler;
pub mod server;
pub mod singleton;
pub mod terminal;

pub use server::{DaemonServer, SessionState};

/// Version of the daemon executable, from the crate manifest.
pub const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");

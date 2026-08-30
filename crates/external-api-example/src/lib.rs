//! # dsh-external-api-example
//!
//! Reference implementation of the **unified external API loop**
//! (ADR-0018 decision 5, PLAN-M5 "统一外源 API interface"):
//!
//! - **carrier**: `dsh_local_transport` — loopback TCP with one-time
//!   ephemeral credentials, framing and deadlines (AC-IPC-001/002);
//! - **contract**: the interop envelope
//!   (`specs/protocol/envelope.schema.json`), validated with the same
//!   frame-level semantics as `packages/capability-contracts` (TS);
//! - **authorization**: activation-scoped grants — an Invocation is only
//!   accepted under an Agreement negotiated on the same connection, for a
//!   capability in the activation's `granted` set (minimal grant).
//!
//! The crate is intentionally standalone: it depends on neither tauri nor
//! the desktop shell, so it doubles as the runnable spec for how an
//! external tool talks to Desktop capabilities.
//!
//! Layout:
//! - [`envelope`]: wire types + frame validation (TS `validate.ts` port)
//! - [`server`]: negotiation + dispatch + authorization (TS `semantics.ts`
//!   counterpart on the serving side)
//! - [`client`]: Hello → Agreement → Invocation → Result loop with
//!   correlation checks (TS `semantics.ts` counterpart on the calling side)
//! - [`catalog`]: example capability coordinates shared by both sides

#![forbid(unsafe_code)]

pub mod catalog;
pub mod client;
pub mod envelope;
pub mod server;

pub use client::{ClientError, ExampleClient};
pub use envelope::{
    AgreementPayload, Envelope, EnvelopeKind, ErrorCode, HelloPayload, Participant,
    ProtocolCoordinate, ProtocolError, UnavailableCapability, UnavailableReason, ValidationIssue,
    new_activation_id, new_message_id, now_timestamp, validate_envelope,
};
pub use server::{Activation, ExampleServer, GrantPolicy, SessionState};

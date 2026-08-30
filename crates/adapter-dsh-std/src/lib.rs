//! # dsh-adapter-dsh-std
//!
//! Optional dsh-std adapter (module `MOD-ADAPTER-DSH-STD`, milestone M5
//! slice M5-D, compatibility ladder L2). Per ADR-0018 the adapter is the
//! change absorption point for dsh-std alpha churn: dsh-std alpha types
//! stop at the adapter boundary and never cross into Desktop internals
//! (ADR-0018 decision 3).
//!
//! ## L2 semantics boundary
//!
//! - Only *known* dsh-std versions are represented, via a conformance
//!   declaration that binds package version + Git commit + artifact
//!   integrity ([`conformance`]). Floating tags (`latest`/`rc`) are never
//!   accepted as declarations (ADR-0018 decision 2).
//! - The unstable dsh-std wire is **not adopted**. The adapter only models
//!   negotiation/facets/conformance semantics locally: the negotiation
//!   state machine here is a Rust port of `packages/capability-contracts`
//!   `negotiate.ts`, which remains the wire/shape authority (M5-B).
//! - Legacy (L1) / L0 fallback is never skipped: any adapter failure
//!   degrades to the L0 baseline ([`degrade`], ADR-0018 decision 4).
//! - Every activation negotiates independently; an Agreement is never
//!   cached across activations ([`negotiate`], ADR-0018 decision 1).
//!
//! ## Modules
//!
//! - `conformance`: tri-state (absent/known/unknown) declaration evaluation
//!   against the pinned external baseline.
//! - `negotiate`: per-activation negotiation state machine.
//! - `facets`: minimal (inferred) dsh-std facet model.
//! - `degrade`: L0 degradation path (additive compatibility).
//! - `time`: UTC RFC 3339 timestamps (std only).

#![forbid(unsafe_code)]

pub mod conformance;
pub mod degrade;
pub mod facets;
pub mod negotiate;
mod time;

pub use conformance::{
    ConformanceDeclaration, ConformanceError, ConformanceLog, ConformanceLogEntry,
    ConformanceRecord, ConformanceState, KNOWN_COMMIT, KNOWN_INTEGRITY, KNOWN_PACKAGE,
    KNOWN_VERSION, conforms,
};
pub use degrade::{DegradeEntry, DegradeLog, DegradeReason, L0Fallback, degrade_to_l0};
pub use facets::{Facet, FacetCatalog, FacetKind};
pub use negotiate::{
    Activation, AgreementDecision, AgreementMessage, AgreementPayload, HelloMessage, HelloPayload,
    LeaseConstraints, NegotiationError, NegotiationErrorCode, NegotiationEvent,
    NegotiationEventKind, NegotiationPhase, NegotiationResult, NegotiationSession, Participant,
    ProtocolCoordinate, Rejection, RejectionReason, Requirement, UnavailableCapability,
    UnavailableReason,
};

//! Negotiation state machine (Rust port of packages/capability-contracts
//! src/negotiate.ts semantics, ADR-0018 decision 1: activation ownership).
//!
//! Every capability activation completes an independent
//! Hello -> Agreement -> active cycle; a previous Agreement is never reused
//! as a fact for a new generation. This crate never caches an Agreement:
//! each activation gets a fresh `NegotiationSession` and the broker must
//! create one per activation.
//!
//! ```text
//! proposed --receive_hello--> proposed
//! proposed --issue_agreement--> agreed
//! agreed   --activate--> active
//! proposed|agreed --reject--> rejected (terminal)
//! ```
//!
//! Degrade path: `issue_agreement` with a non-empty `unavailable` list still
//! reaches agreed (partial grant); the session is flagged `degraded` and the
//! activation records exactly which capabilities were not granted and why.
//!
//! Errors are `Result`-based (`NegotiationError` with a code), never panics;
//! repeated operations are idempotent where well-defined (same Hello id,
//! activate when active, reject when rejected). Frame-level structural
//! checks (id lengths, uniqueness) mirror the envelope schema rules that
//! capability-contracts `validateEnvelope` enforces on the TS side.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::time;

/// Phase of one activation negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiationPhase {
    Proposed,
    Agreed,
    Active,
    Rejected,
}

impl NegotiationPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Agreed => "agreed",
            Self::Active => "active",
            Self::Rejected => "rejected",
        }
    }
}

/// Rejection reasons: protocol reasons plus peer-side rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    Unavailable,
    UnsupportedVersion,
    PolicyDenied,
    ProviderFailed,
    PeerRejected,
}

impl RejectionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::UnsupportedVersion => "unsupported_version",
            Self::PolicyDenied => "policy_denied",
            Self::ProviderFailed => "provider_failed",
            Self::PeerRejected => "peer_rejected",
        }
    }
}

/// Outcome codes for state-machine operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiationErrorCode {
    Conflict,
    InvalidState,
    MalformedMessage,
}

impl NegotiationErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conflict => "CONFLICT",
            Self::InvalidState => "INVALID_STATE",
            Self::MalformedMessage => "MALFORMED_MESSAGE",
        }
    }
}

/// Structured state-machine error (mirrors the TS discriminated
/// `{ ok: false, code, message }` results).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiationError {
    pub code: NegotiationErrorCode,
    pub message: String,
}

impl NegotiationError {
    fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: NegotiationErrorCode::Conflict,
            message: message.into(),
        }
    }

    fn invalid_state(message: impl Into<String>) -> Self {
        Self {
            code: NegotiationErrorCode::InvalidState,
            message: message.into(),
        }
    }

    fn malformed(message: impl Into<String>) -> Self {
        Self {
            code: NegotiationErrorCode::MalformedMessage,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for NegotiationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for NegotiationError {}

pub type NegotiationResult<T> = Result<T, NegotiationError>;
/// Capability coordinate (protocol-coordinate.schema.json shape).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolCoordinate {
    pub api_version: String,
    pub kind: String,
}

impl ProtocolCoordinate {
    pub fn new(api_version: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            api_version: api_version.into(),
            kind: kind.into(),
        }
    }
}

/// One entry of `Hello.payload.requires`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub coordinate: ProtocolCoordinate,
    pub required: bool,
}

impl Requirement {
    pub fn new(coordinate: ProtocolCoordinate, required: bool) -> Self {
        Self {
            coordinate,
            required,
        }
    }
}

/// Reason a requested capability was not granted
/// (agreementPayload.unavailable[].reason enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    Unavailable,
    UnsupportedVersion,
    PolicyDenied,
    ProviderFailed,
}

impl UnavailableReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::UnsupportedVersion => "unsupported_version",
            Self::PolicyDenied => "policy_denied",
            Self::ProviderFailed => "provider_failed",
        }
    }
}

/// One entry of `Agreement.payload.unavailable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableCapability {
    pub coordinate: ProtocolCoordinate,
    pub reason: UnavailableReason,
}

impl UnavailableCapability {
    pub fn new(coordinate: ProtocolCoordinate, reason: UnavailableReason) -> Self {
        Self { coordinate, reason }
    }
}

/// Lease constraints offered inside an Agreement
/// (agreementPayload.leaseConstraints).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseConstraints {
    pub max_seconds: Option<u64>,
    pub approval_required: Option<bool>,
}

impl LeaseConstraints {
    pub fn new(max_seconds: u64) -> Self {
        Self {
            max_seconds: Some(max_seconds),
            approval_required: None,
        }
    }
}

/// Envelope sender identity (envelope.schema.json `participant`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub component: String,
    pub facet: String,
    pub activation_id: Option<String>,
}

impl Participant {
    pub fn new(component: impl Into<String>, facet: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            facet: facet.into(),
            activation_id: None,
        }
    }
}

/// `Hello` envelope (envelope.schema.json hello shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloMessage {
    pub id: String,
    pub generation: u64,
    pub participant: Participant,
    pub payload: HelloPayload,
}

impl HelloMessage {
    pub fn new(
        id: impl Into<String>,
        generation: u64,
        participant: Participant,
        payload: HelloPayload,
    ) -> Self {
        Self {
            id: id.into(),
            generation,
            participant,
            payload,
        }
    }
}

/// `Hello` payload (envelope.schema.json helloPayload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloPayload {
    /// Peer instance identifier, 8..128 chars.
    pub instance_id: String,
    /// Capabilities the peer can provide; unique array.
    pub supports: Vec<ProtocolCoordinate>,
    /// Capabilities the peer requires of us; unique array.
    pub requires: Vec<Requirement>,
}

impl HelloPayload {
    pub fn new(
        instance_id: impl Into<String>,
        supports: Vec<ProtocolCoordinate>,
        requires: Vec<Requirement>,
    ) -> Self {
        Self {
            instance_id: instance_id.into(),
            supports,
            requires,
        }
    }
}

/// `Agreement` envelope (envelope.schema.json agreement shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementMessage {
    pub id: String,
    pub reply_to: String,
    pub generation: u64,
    pub participant: Participant,
    pub payload: AgreementPayload,
}

/// `Agreement` payload (envelope.schema.json agreementPayload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementPayload {
    pub activation_id: String,
    /// Capabilities granted to the peer; unique array.
    pub granted: Vec<ProtocolCoordinate>,
    /// Requested capabilities that could not be granted; unique array.
    pub unavailable: Vec<UnavailableCapability>,
    pub lease_constraints: Option<LeaseConstraints>,
}

/// Recorded activation (ADR-0018: broker registers one per negotiation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub activation_id: String,
    pub granted: Vec<ProtocolCoordinate>,
    pub unavailable: Vec<UnavailableCapability>,
    pub lease_constraints: Option<LeaseConstraints>,
    pub created_at: String,
    /// True when at least one requested capability was not granted.
    pub degraded: bool,
}

/// Rejection record (terminal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub reason: String,
    pub message: Option<String>,
    pub at: String,
}

/// Observable timeline entry for audit/observability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiationEvent {
    pub kind: NegotiationEventKind,
    pub at: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiationEventKind {
    Hello,
    Agreement,
    Activation,
    Reject,
    Degrade,
}

/// Decision input for `issue_agreement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementDecision {
    pub activation_id: String,
    pub granted: Vec<ProtocolCoordinate>,
    pub unavailable: Vec<UnavailableCapability>,
    pub lease_constraints: Option<LeaseConstraints>,
    pub id: Option<String>,
    pub generation: Option<u64>,
    pub timestamp: Option<String>,
    pub participant: Option<Participant>,
}

impl AgreementDecision {
    pub fn new(activation_id: impl Into<String>) -> Self {
        Self {
            activation_id: activation_id.into(),
            granted: Vec::new(),
            unavailable: Vec::new(),
            lease_constraints: None,
            id: None,
            generation: None,
            timestamp: None,
            participant: None,
        }
    }

    pub fn with_granted(mut self, granted: Vec<ProtocolCoordinate>) -> Self {
        self.granted = granted;
        self
    }

    pub fn with_unavailable(mut self, unavailable: Vec<UnavailableCapability>) -> Self {
        self.unavailable = unavailable;
        self
    }

    pub fn with_lease(mut self, lease: LeaseConstraints) -> Self {
        self.lease_constraints = Some(lease);
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

static MSG_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_message_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = MSG_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("msg-{nanos:x}-{n}")
}

fn includes_coordinate(list: &[ProtocolCoordinate], target: &ProtocolCoordinate) -> bool {
    list.iter().any(|c| c == target)
}

fn all_unique_coordinates(coords: &[ProtocolCoordinate]) -> bool {
    let mut seen: HashSet<&ProtocolCoordinate> = HashSet::new();
    coords.iter().all(|c| seen.insert(c))
}

fn all_unique_unavailable(list: &[UnavailableCapability]) -> bool {
    let mut seen: HashSet<&ProtocolCoordinate> = HashSet::new();
    list.iter().all(|u| seen.insert(&u.coordinate))
}
/// One activation's negotiation session. Created `Proposed`; drives
/// Hello -> Agreement -> active, with reject and degrade paths.
///
/// Nothing is cached across sessions: two activations always get
/// independent state (ADR-0018 decision 1 - no Agreement reuse).
pub struct NegotiationSession {
    pub session_id: String,
    pub phase: NegotiationPhase,
    /// Last validated Hello (None until received).
    pub hello: Option<HelloMessage>,
    /// Issued Agreement (None until issued).
    pub agreement: Option<AgreementMessage>,
    /// Recorded activation (None until `activate()`).
    pub activation: Option<Activation>,
    /// Rejection record (None unless rejected).
    pub rejection: Option<Rejection>,
    /// True once an agreement contained at least one unavailable capability.
    pub degraded: bool,
    /// Append-only timeline.
    pub history: Vec<NegotiationEvent>,
    now: fn() -> String,
}

impl NegotiationSession {
    /// Start a fresh session (no state carried over from any previous
    /// activation, even for the same session id).
    pub fn begin(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            phase: NegotiationPhase::Proposed,
            hello: None,
            agreement: None,
            activation: None,
            rejection: None,
            degraded: false,
            history: Vec::new(),
            now: time::now_rfc3339,
        }
    }

    /// Test/observability hook: inject a clock.
    pub fn with_clock(session_id: impl Into<String>, now: fn() -> String) -> Self {
        Self {
            now,
            ..Self::begin(session_id)
        }
    }

    /// Receive a Hello. Idempotent for the same Hello id; CONFLICT otherwise.
    pub fn receive_hello(&mut self, hello: HelloMessage) -> NegotiationResult<HelloMessage> {
        if self.phase == NegotiationPhase::Rejected {
            return Err(NegotiationError::invalid_state("session is rejected"));
        }
        if self.phase != NegotiationPhase::Proposed {
            return Err(NegotiationError::conflict(format!(
                "cannot receive Hello in phase {:?}",
                self.phase
            )));
        }
        if let Some(existing) = &self.hello {
            if existing.id == hello.id {
                return Ok(existing.clone());
            }
            return Err(NegotiationError::conflict("Hello already received"));
        }
        // Frame-level structural checks mirroring envelope.schema.json:
        // id and instanceId are 8..128 chars, supports/requires are unique.
        if !(8..=128).contains(&hello.id.len()) {
            return Err(NegotiationError::malformed("Hello id must be 8..128 chars"));
        }
        if !(8..=128).contains(&hello.payload.instance_id.len()) {
            return Err(NegotiationError::malformed(
                "Hello payload.instanceId must be 8..128 chars",
            ));
        }
        if !all_unique_coordinates(&hello.payload.supports) {
            return Err(NegotiationError::malformed("Hello supports must be unique"));
        }
        if hello.payload.requires.is_empty() && hello.payload.supports.is_empty() {
            return Err(NegotiationError::malformed(
                "Hello must declare at least one support or requirement",
            ));
        }
        self.hello = Some(hello);
        let id = self.hello.as_ref().expect("just set").id.clone();
        let at = (self.now)();
        self.history.push(NegotiationEvent {
            kind: NegotiationEventKind::Hello,
            at: at.clone(),
            detail: id,
        });
        Ok(self.hello.as_ref().expect("just set").clone())
    }

    /// Broker decision: grant a subset of the requested capabilities.
    /// Enforces: granted subset of Hello.supports and granted intersect
    /// unavailable = empty. A non-empty `unavailable` marks the session
    /// degraded (still agreed).
    pub fn issue_agreement(
        &mut self,
        decision: AgreementDecision,
    ) -> NegotiationResult<&AgreementMessage> {
        if self.phase == NegotiationPhase::Rejected {
            return Err(NegotiationError::invalid_state("session is rejected"));
        }
        if self.phase != NegotiationPhase::Proposed {
            return Err(NegotiationError::conflict(format!(
                "cannot issue Agreement in phase {:?}",
                self.phase
            )));
        }
        let hello = self
            .hello
            .as_ref()
            .ok_or_else(|| NegotiationError::invalid_state("no Hello received yet"))?;
        for c in &decision.granted {
            if !includes_coordinate(&hello.payload.supports, c) {
                return Err(NegotiationError::conflict(format!(
                    "granted capability {}/{} is not in Hello.supports",
                    c.api_version, c.kind
                )));
            }
        }
        for u in &decision.unavailable {
            if includes_coordinate(&decision.granted, &u.coordinate) {
                return Err(NegotiationError::conflict(format!(
                    "capability {}/{} is both granted and unavailable",
                    u.coordinate.api_version, u.coordinate.kind
                )));
            }
        }
        if decision.activation_id.is_empty() || decision.activation_id.len() > 128 {
            return Err(NegotiationError::malformed(
                "activationId must be 1..128 chars",
            ));
        }
        if !all_unique_coordinates(&decision.granted) {
            return Err(NegotiationError::malformed("granted must be unique"));
        }
        if !all_unique_unavailable(&decision.unavailable) {
            return Err(NegotiationError::malformed("unavailable must be unique"));
        }
        let agreement = AgreementMessage {
            id: decision.id.clone().unwrap_or_else(new_message_id),
            reply_to: hello.id.clone(),
            generation: decision.generation.unwrap_or(hello.generation),
            participant: decision
                .participant
                .clone()
                .unwrap_or_else(|| hello.participant.clone()),
            payload: AgreementPayload {
                activation_id: decision.activation_id.clone(),
                granted: decision.granted.clone(),
                unavailable: decision.unavailable.clone(),
                lease_constraints: decision.lease_constraints.clone(),
            },
        };
        // Frame check mirroring validateEnvelope(Agreement): id 8..128.
        if !(8..=128).contains(&agreement.id.len()) {
            return Err(NegotiationError::malformed(
                "constructed Agreement id must be 8..128 chars",
            ));
        }
        self.agreement = Some(agreement);
        self.phase = NegotiationPhase::Agreed;
        if !decision.unavailable.is_empty() {
            self.degraded = true;
            let at = (self.now)();
            let n = decision.unavailable.len();
            self.history.push(NegotiationEvent {
                kind: NegotiationEventKind::Degrade,
                at: at.clone(),
                detail: format!("{n} capability(ies) unavailable"),
            });
        }
        let id = self.agreement.as_ref().expect("just set").id.clone();
        let at = (self.now)();
        self.history.push(NegotiationEvent {
            kind: NegotiationEventKind::Agreement,
            at,
            detail: id,
        });
        Ok(self.agreement.as_ref().expect("just set"))
    }

    /// Move agreed -> active and record the Activation. Idempotent when active.
    pub fn activate(&mut self) -> NegotiationResult<&Activation> {
        if self.phase == NegotiationPhase::Rejected {
            return Err(NegotiationError::invalid_state("session is rejected"));
        }
        if self.phase == NegotiationPhase::Active {
            return self.activation.as_ref().ok_or_else(|| {
                NegotiationError::invalid_state("active session without activation record")
            });
        }
        if self.phase != NegotiationPhase::Agreed {
            return Err(NegotiationError::invalid_state(format!(
                "cannot activate from phase {:?}",
                self.phase
            )));
        }
        let agreement = self.agreement.as_ref().ok_or_else(|| {
            NegotiationError::invalid_state("agreed session without agreement record")
        })?;
        let created_at = (self.now)();
        let activation = Activation {
            activation_id: agreement.payload.activation_id.clone(),
            granted: agreement.payload.granted.clone(),
            unavailable: agreement.payload.unavailable.clone(),
            lease_constraints: agreement.payload.lease_constraints.clone(),
            created_at: created_at.clone(),
            degraded: self.degraded,
        };
        let detail = activation.activation_id.clone();
        self.activation = Some(activation);
        self.phase = NegotiationPhase::Active;
        self.history.push(NegotiationEvent {
            kind: NegotiationEventKind::Activation,
            at: created_at,
            detail,
        });
        Ok(self.activation.as_ref().expect("just set"))
    }

    /// Reject the negotiation (terminal). Idempotent when already rejected.
    pub fn reject(
        &mut self,
        reason: RejectionReason,
        message: Option<&str>,
    ) -> NegotiationResult<&Rejection> {
        if self.phase == NegotiationPhase::Rejected {
            return self.rejection.as_ref().ok_or_else(|| {
                NegotiationError::invalid_state("rejected session without rejection record")
            });
        }
        if self.phase == NegotiationPhase::Active {
            return Err(NegotiationError::conflict(
                "cannot reject an active session",
            ));
        }
        let at = (self.now)();
        let rejection = Rejection {
            reason: reason.as_str().to_owned(),
            message: message.map(str::to_owned),
            at: at.clone(),
        };
        self.phase = NegotiationPhase::Rejected;
        let detail = match &rejection.message {
            Some(m) => format!("{}: {m}", reason.as_str()),
            None => reason.as_str().to_owned(),
        };
        self.rejection = Some(rejection);
        self.history.push(NegotiationEvent {
            kind: NegotiationEventKind::Reject,
            at,
            detail,
        });
        Ok(self.rejection.as_ref().expect("just set"))
    }

    /// Required Hello requirements that are not covered by `granted`.
    pub fn unsatisfied_requirements(
        &self,
        granted: &[ProtocolCoordinate],
    ) -> Vec<ProtocolCoordinate> {
        let Some(hello) = &self.hello else {
            return Vec::new();
        };
        hello
            .payload
            .requires
            .iter()
            .filter(|r| r.required)
            .map(|r| r.coordinate.clone())
            .filter(|c| !includes_coordinate(granted, c))
            .collect()
    }

    /// Convenience: requirements unsatisfied by the current agreement.
    pub fn unsatisfied_after_agreement(&self) -> Vec<ProtocolCoordinate> {
        let granted = self
            .agreement
            .as_ref()
            .map(|a| a.payload.granted.as_slice())
            .unwrap_or_default();
        self.unsatisfied_requirements(granted)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn terminal() -> ProtocolCoordinate {
        ProtocolCoordinate::new("terminal.dsh-desktop.local/v1alpha1", "Terminal")
    }

    fn browser() -> ProtocolCoordinate {
        ProtocolCoordinate::new("browser.dsh-desktop.local/v1alpha1", "Browser")
    }

    fn runtime() -> ProtocolCoordinate {
        ProtocolCoordinate::new("runtime.dsh-desktop.local/v1alpha1", "Runtime")
    }

    fn hello_msg() -> HelloMessage {
        HelloMessage::new(
            "msg-hk8sj2k3l4m5n6p7",
            1,
            Participant::new("dsh-desktop-shell", "agent"),
            HelloPayload::new(
                "instance-dsh-0001",
                vec![terminal(), browser()],
                vec![Requirement::new(runtime(), true)],
            ),
        )
    }

    #[test]
    fn happy_path_proposed_to_active() {
        let mut s = NegotiationSession::begin("act-7f3a9c2e");
        assert_eq!(s.phase, NegotiationPhase::Proposed);

        s.receive_hello(hello_msg()).expect("hello accepted");
        assert_eq!(s.phase, NegotiationPhase::Proposed);
        assert!(s.hello.is_some());

        let decision = AgreementDecision::new("act-7f3a9c2e")
            .with_granted(vec![terminal(), browser()])
            .with_lease(LeaseConstraints::new(3600));
        s.issue_agreement(decision).expect("agreement issued");
        assert_eq!(s.phase, NegotiationPhase::Agreed);
        assert!(!s.degraded);

        let activation = s.activate().expect("activated").clone();
        assert_eq!(s.phase, NegotiationPhase::Active);
        assert_eq!(activation.activation_id, "act-7f3a9c2e");
        assert_eq!(activation.granted, vec![terminal(), browser()]);
        assert!(activation.unavailable.is_empty());
        assert!(!activation.degraded);
        assert_eq!(
            activation.lease_constraints,
            Some(LeaseConstraints::new(3600))
        );

        let kinds: Vec<NegotiationEventKind> = s.history.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                NegotiationEventKind::Hello,
                NegotiationEventKind::Agreement,
                NegotiationEventKind::Activation,
            ]
        );
    }

    #[test]
    fn granted_not_in_supports_conflict() {
        let mut s = NegotiationSession::begin("act-1");
        s.receive_hello(hello_msg()).expect("hello accepted");
        let decision = AgreementDecision::new("act-1").with_granted(vec![runtime()]);
        let err = s.issue_agreement(decision).expect_err("must conflict");
        assert_eq!(err.code, NegotiationErrorCode::Conflict);
        assert!(err.message.contains("not in Hello.supports"));
        assert_eq!(s.phase, NegotiationPhase::Proposed);
    }

    #[test]
    fn granted_and_unavailable_overlap_conflict() {
        let mut s = NegotiationSession::begin("act-2");
        s.receive_hello(hello_msg()).expect("hello accepted");
        let decision = AgreementDecision::new("act-2")
            .with_granted(vec![terminal()])
            .with_unavailable(vec![UnavailableCapability::new(
                terminal(),
                UnavailableReason::Unavailable,
            )]);
        let err = s.issue_agreement(decision).expect_err("must conflict");
        assert_eq!(err.code, NegotiationErrorCode::Conflict);
        assert!(err.message.contains("both granted and unavailable"));
    }

    #[test]
    fn issue_agreement_before_hello_invalid() {
        let mut s = NegotiationSession::begin("act-3");
        let decision = AgreementDecision::new("act-3").with_granted(vec![terminal()]);
        let err = s.issue_agreement(decision).expect_err("must fail");
        assert_eq!(err.code, NegotiationErrorCode::InvalidState);
        assert!(err.message.contains("no Hello"));
    }

    #[test]
    fn hello_after_agreed_conflict() {
        let mut s = NegotiationSession::begin("act-4");
        s.receive_hello(hello_msg()).expect("hello accepted");
        s.issue_agreement(AgreementDecision::new("act-4").with_granted(vec![terminal()]))
            .expect("agreement issued");
        let err = s.receive_hello(hello_msg()).expect_err("must conflict");
        assert_eq!(err.code, NegotiationErrorCode::Conflict);
    }

    #[test]
    fn duplicate_hello_id_idempotent() {
        let mut s = NegotiationSession::begin("act-5");
        s.receive_hello(hello_msg()).expect("first hello");
        s.receive_hello(hello_msg())
            .expect("same hello id is idempotent");
        assert_eq!(s.history.len(), 1);
    }

    #[test]
    fn different_hello_id_conflict() {
        let mut s = NegotiationSession::begin("act-6");
        s.receive_hello(hello_msg()).expect("first hello");
        let other = HelloMessage {
            id: "msg-other-00000000001".to_owned(),
            ..hello_msg()
        };
        let err = s.receive_hello(other).expect_err("must conflict");
        assert_eq!(err.code, NegotiationErrorCode::Conflict);
    }

    #[test]
    fn activate_before_agreed_invalid() {
        let mut s = NegotiationSession::begin("act-7");
        let err = s.activate().expect_err("must fail");
        assert_eq!(err.code, NegotiationErrorCode::InvalidState);
    }

    #[test]
    fn activate_idempotent() {
        let mut s = NegotiationSession::begin("act-8");
        s.receive_hello(hello_msg()).expect("hello");
        s.issue_agreement(AgreementDecision::new("act-8").with_granted(vec![terminal()]))
            .expect("agreement");
        let first = s.activate().expect("first activate").clone();
        let second = s.activate().expect("activate is idempotent").clone();
        assert_eq!(first, second);
        assert_eq!(s.history.len(), 3);
    }

    #[test]
    fn reject_from_proposed_terminal_and_idempotent() {
        let mut s = NegotiationSession::begin("act-9");
        s.reject(RejectionReason::PolicyDenied, Some("policy forbids peer"))
            .expect("rejected");
        assert_eq!(s.phase, NegotiationPhase::Rejected);
        assert_eq!(s.rejection.as_ref().unwrap().reason, "policy_denied");
        // idempotent
        s.reject(RejectionReason::Unavailable, None)
            .expect("idempotent");
        assert_eq!(s.rejection.as_ref().unwrap().reason, "policy_denied");
        assert_eq!(s.history.len(), 1);
        // terminal: no hello, no agreement, no activation possible
        let err = s.receive_hello(hello_msg()).expect_err("rejected session");
        assert_eq!(err.code, NegotiationErrorCode::InvalidState);
    }

    #[test]
    fn reject_after_active_conflict() {
        let mut s = NegotiationSession::begin("act-10");
        s.receive_hello(hello_msg()).expect("hello");
        s.issue_agreement(AgreementDecision::new("act-10").with_granted(vec![terminal()]))
            .expect("agreement");
        s.activate().expect("activate");
        let err = s
            .reject(RejectionReason::PeerRejected, None)
            .expect_err("must conflict");
        assert_eq!(err.code, NegotiationErrorCode::Conflict);
    }

    #[test]
    fn degrade_partial_grant_marks_degraded() {
        let mut s = NegotiationSession::begin("act-degraded-01");
        s.receive_hello(hello_msg()).expect("hello");
        let decision = AgreementDecision::new("act-degraded-01")
            .with_granted(vec![terminal()])
            .with_unavailable(vec![
                UnavailableCapability::new(browser(), UnavailableReason::UnsupportedVersion),
                UnavailableCapability::new(runtime(), UnavailableReason::PolicyDenied),
            ]);
        let agreement = s
            .issue_agreement(decision)
            .expect("partial grant still agreed")
            .clone();
        assert_eq!(s.phase, NegotiationPhase::Agreed);
        assert!(s.degraded);
        assert_eq!(agreement.payload.unavailable.len(), 2);
        let activation = s.activate().expect("activate");
        assert!(activation.degraded);
        assert_eq!(activation.granted, vec![terminal()]);
        assert_eq!(activation.unavailable.len(), 2);
        assert_eq!(
            activation.unavailable[0].reason,
            UnavailableReason::UnsupportedVersion,
        );
        let kinds: Vec<NegotiationEventKind> = s.history.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                NegotiationEventKind::Hello,
                NegotiationEventKind::Degrade,
                NegotiationEventKind::Agreement,
                NegotiationEventKind::Activation,
            ]
        );
    }

    #[test]
    fn unsatisfied_requirements_filtering() {
        let mut s = NegotiationSession::begin("act-11");
        s.receive_hello(hello_msg()).expect("hello");
        // runtime() is required but not granted yet
        assert_eq!(s.unsatisfied_requirements(&[terminal()]), vec![runtime()]);
        assert_eq!(s.unsatisfied_requirements(&[terminal(), runtime()]), vec![]);
        // after a full grant the convenience accessor agrees
        s.issue_agreement(
            AgreementDecision::new("act-11").with_granted(vec![terminal(), browser()]),
        )
        .expect("agreement");
        assert_eq!(s.unsatisfied_after_agreement(), vec![runtime()]);
    }

    #[test]
    fn optional_requirements_are_not_unsatisfied() {
        let mut s = NegotiationSession::begin("act-12");
        let mut hello = hello_msg();
        hello
            .payload
            .requires
            .push(Requirement::new(browser(), false));
        s.receive_hello(hello).expect("hello");
        assert_eq!(s.unsatisfied_requirements(&[]), vec![runtime()]);
    }

    #[test]
    fn no_agreement_caching_across_sessions() {
        // Two activations with identical inputs are fully independent:
        // nothing from the first leaks into the second.
        let mut first = NegotiationSession::begin("act-same");
        first.receive_hello(hello_msg()).expect("hello");
        first
            .issue_agreement(
                AgreementDecision::new("act-first-000001")
                    .with_granted(vec![terminal()])
                    .with_id("msg-first-agreement"),
            )
            .expect("agreement");
        let a1 = first.activate().expect("activate").clone();

        // Fresh session with the SAME session id starts empty.
        let mut second = NegotiationSession::begin("act-same");
        assert_eq!(second.phase, NegotiationPhase::Proposed);
        assert!(second.hello.is_none());
        assert!(second.agreement.is_none());
        assert!(second.activation.is_none());
        assert!(second.history.is_empty());

        second.receive_hello(hello_msg()).expect("hello");
        second
            .issue_agreement(
                AgreementDecision::new("act-second-0001")
                    .with_granted(vec![browser()])
                    .with_id("msg-second-agreement"),
            )
            .expect("agreement");
        let a2 = second.activate().expect("activate").clone();

        assert_ne!(a1, a2);
        assert_eq!(a1.activation_id, "act-first-000001");
        assert_eq!(a2.activation_id, "act-second-0001");
        assert_ne!(
            first.agreement.as_ref().unwrap().id,
            second.agreement.as_ref().unwrap().id
        );
    }

    #[test]
    fn malformed_hello_id_rejected() {
        let mut s = NegotiationSession::begin("act-13");
        let bad = HelloMessage {
            id: "short".to_owned(),
            ..hello_msg()
        };
        let err = s.receive_hello(bad).expect_err("must fail");
        assert_eq!(err.code, NegotiationErrorCode::MalformedMessage);
        assert!(s.hello.is_none());
    }

    #[test]
    fn malformed_activation_id_rejected() {
        let mut s = NegotiationSession::begin("act-14");
        s.receive_hello(hello_msg()).expect("hello");
        let decision = AgreementDecision::new("").with_granted(vec![terminal()]);
        let err = s.issue_agreement(decision).expect_err("must fail");
        assert_eq!(err.code, NegotiationErrorCode::MalformedMessage);
        assert_eq!(s.phase, NegotiationPhase::Proposed);
    }

    #[test]
    fn agreement_frame_defaults_follow_hello() {
        let mut s = NegotiationSession::begin("act-15");
        s.receive_hello(hello_msg()).expect("hello");
        let agreement = s
            .issue_agreement(AgreementDecision::new("act-15").with_granted(vec![terminal()]))
            .expect("agreement");
        assert_eq!(agreement.reply_to, "msg-hk8sj2k3l4m5n6p7");
        assert_eq!(agreement.generation, 1);
        assert_eq!(agreement.participant.component, "dsh-desktop-shell");
        assert!(!agreement.id.is_empty());
        assert!((8..=128).contains(&agreement.id.len()));
    }

    #[test]
    fn degrade_reason_mapping_covers_agreement_reasons() {
        // Every protocol unavailable-reason maps to a string (used when the
        // degrade path records why an L2 activation is partial).
        for r in [
            UnavailableReason::Unavailable,
            UnavailableReason::UnsupportedVersion,
            UnavailableReason::PolicyDenied,
            UnavailableReason::ProviderFailed,
        ] {
            assert!(!r.as_str().is_empty());
        }
    }
}

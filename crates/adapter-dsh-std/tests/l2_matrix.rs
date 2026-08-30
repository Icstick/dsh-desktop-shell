//! L2 conformance matrix (WI-M5-INTEROP acceptance): absent/known/unknown
//! x degrade. Fixture-driven: declarations come from the fixture JSON
//! files under crates/adapter-dsh-std/fixtures/.

use dsh_adapter_dsh_std::conformance::{
    ConformanceDeclaration, ConformanceLog, ConformanceState, conforms,
};
use dsh_adapter_dsh_std::degrade::{DegradeLog, DegradeReason, degrade_to_l0};
use dsh_adapter_dsh_std::negotiate::{
    AgreementDecision, HelloMessage, HelloPayload, NegotiationPhase, NegotiationSession,
    Participant, ProtocolCoordinate, Requirement,
};

const KNOWN_FIXTURE: &str = include_str!("../fixtures/known-dsh-std.json");
const VERSION_DRIFT: &str = include_str!("../fixtures/unknown-version-drift.json");
const COMMIT_DRIFT: &str = include_str!("../fixtures/unknown-commit-drift.json");
const INTEGRITY_DRIFT: &str = include_str!("../fixtures/unknown-integrity-drift.json");
const BAD_INTEGRITY: &str = include_str!("../fixtures/invalid-integrity.json");

fn terminal() -> ProtocolCoordinate {
    ProtocolCoordinate::new("terminal.dsh-desktop.local/v1alpha1", "Terminal")
}

fn hello() -> HelloMessage {
    HelloMessage::new(
        "msg-hk8sj2k3l4m5n6p7",
        1,
        Participant::new("dsh-desktop-shell", "agent"),
        HelloPayload::new(
            "instance-dsh-0001",
            vec![terminal()],
            vec![Requirement::new(terminal(), true)],
        ),
    )
}

#[test]
fn absent_keeps_l0_l1_unchanged() {
    // No declaration: the dsh-std domain protocol is not implemented.
    let mut clog = ConformanceLog::new();
    assert_eq!(conforms(None, &mut clog), ConformanceState::Absent);
    assert_eq!(clog.len(), 1);

    // Recording the absent state keeps the audit trail but never claims L2.
    let mut dlog = DegradeLog::new();
    let fb = degrade_to_l0(
        DegradeReason::ConformanceAbsent,
        "no dsh-std declaration; staying on L0/L1",
        &mut dlog,
    );
    assert!(fb.baseline_intact());
    assert!(!fb.claims_l2());
}

#[test]
fn known_grants_l2_and_still_degrades_safely() {
    let decl = ConformanceDeclaration::from_json(KNOWN_FIXTURE).expect("fixture parses");
    let mut clog = ConformanceLog::new();
    let state = conforms(Some(&decl), &mut clog);
    assert_eq!(state, ConformanceState::Known);
    assert!(state.is_l2());
    assert_eq!(
        clog.last().unwrap().matched.as_deref(),
        Some("@dsh-std/core@0.1.1-rc.1")
    );

    // L2 granted: a negotiation can reach active.
    let mut session = NegotiationSession::begin("act-matrix-known");
    session.receive_hello(hello()).expect("hello");
    session
        .issue_agreement(AgreementDecision::new("act-matrix-known").with_granted(vec![terminal()]))
        .expect("agreement");
    let activation = session.activate().expect("activate").clone();
    assert_eq!(session.phase, NegotiationPhase::Active);
    assert!(!activation.degraded);

    // Even with conformance known, a runtime adapter failure degrades to L0
    // instead of panicking or blocking (ADR-0018 decision 4).
    let mut dlog = DegradeLog::new();
    let fb = degrade_to_l0(
        DegradeReason::InvocationFailed,
        "known adapter failed at runtime; falling back to L0",
        &mut dlog,
    );
    assert!(fb.baseline_intact());
    assert!(!fb.claims_l2());
    assert_eq!(dlog.len(), 1);
}

#[test]
fn drift_fixtures_fail_closed() {
    for (name, json) in [
        ("version", VERSION_DRIFT),
        ("commit", COMMIT_DRIFT),
        ("integrity", INTEGRITY_DRIFT),
        ("bad-integrity", BAD_INTEGRITY),
    ] {
        let decl = ConformanceDeclaration::from_json(json)
            .unwrap_or_else(|e| panic!("{name} fixture must parse: {e}"));
        let mut clog = ConformanceLog::new();
        let state = conforms(Some(&decl), &mut clog);
        assert_eq!(state, ConformanceState::Unknown, "fixture {name}");
        assert!(!state.is_l2());
        assert_eq!(clog.len(), 1);
        let reason = &clog.last().unwrap().reason;
        assert!(
            reason.contains("fail-closed")
                || reason.contains("drift")
                || reason.contains("format validation"),
            "unexpected reason: {reason}"
        );
    }

    // Unknown always degrades to L0 (recorded, not blocked).
    let mut dlog = DegradeLog::new();
    let fb = degrade_to_l0(
        DegradeReason::ConformanceUnknown,
        "declared coordinates drifted; fail-closed to L0",
        &mut dlog,
    );
    assert!(fb.baseline_intact());
    assert!(!fb.claims_l2());
}

#[test]
fn absent_unknown_known_full_matrix() {
    // The whole tri-state matrix in one pass.
    let mut clog = ConformanceLog::new();

    let absent = conforms(None, &mut clog);
    let known = conforms(
        Some(&ConformanceDeclaration::from_json(KNOWN_FIXTURE).unwrap()),
        &mut clog,
    );
    let unknown = conforms(
        Some(&ConformanceDeclaration::from_json(VERSION_DRIFT).unwrap()),
        &mut clog,
    );

    assert_eq!(absent, ConformanceState::Absent);
    assert_eq!(known, ConformanceState::Known);
    assert_eq!(unknown, ConformanceState::Unknown);
    assert_eq!(
        clog.states(),
        vec![
            ConformanceState::Absent,
            ConformanceState::Known,
            ConformanceState::Unknown
        ],
    );
    // Ordering is stable: absent -> known -> unknown per evaluation order.
    assert_eq!(clog.len(), 3);
}

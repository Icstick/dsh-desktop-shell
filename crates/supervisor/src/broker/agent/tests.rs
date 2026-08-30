//! Agent authorization bridge tests: negotiation -> grant/lease ->
//! dispatch chain, activation ownership (ADR-0018 decision 1), human
//! takeover (AC-BRW-002) and the fail-closed input matrix.

use std::cell::Cell;
use std::rc::Rc;

use super::super::{
    Broker, BrokerError, CapabilityId, Clock, Invocation, InvocationResult, LeaseRevocationReason,
    Provider, Scope,
};
use super::*;

const API_VERSION: &str = "interop.dsh-desktop.local/v1alpha1";
const AGENT: &str = "agent-1";
const AGENT2: &str = "agent-2";
const T0: u64 = 1_000_000;

fn terminal() -> CapabilityId {
    CapabilityId::new(API_VERSION, "Terminal")
}

fn browser() -> CapabilityId {
    CapabilityId::new(API_VERSION, "Browser")
}

fn full_scope() -> Scope {
    Scope {
        session_id: Some("session-1".into()),
        workspace: Some("ws-a".into()),
        ..Default::default()
    }
}

fn other_workspace() -> Scope {
    Scope {
        workspace: Some("ws-b".into()),
        ..Default::default()
    }
}

/// A negotiated (agreed, Known, 60s lease) result for one capability.
fn known_result(activation_id: &str, granted: Vec<CapabilityId>) -> AgentNegotiationResult {
    AgentNegotiationResult {
        activation_id: activation_id.into(),
        agreed: true,
        granted,
        conformance: AgentConformanceState::Known,
        lease_constraints: Some(AgentLeaseConstraints::new(60)),
        scope: full_scope(),
    }
}

fn invoke(capability: &CapabilityId, owner: &str, generation: u64, scope: &Scope) -> Invocation {
    Invocation {
        capability: capability.clone(),
        method: "mutate".into(),
        owner: owner.into(),
        generation,
        scope: scope.clone(),
        payload: serde_json::json!({ "method": "mutate" }),
    }
}

fn echo_provider(id: &str, capability: &CapabilityId) -> Provider {
    Provider::new(id.to_string(), capability.clone(), |inv| InvocationResult {
        payload: inv.payload.clone(),
    })
}

/// Deterministic clock (same pattern as broker/tests.rs).
#[derive(Clone)]
struct FakeClock {
    now: Rc<Cell<u64>>,
}

impl FakeClock {
    fn new(start: u64) -> Self {
        Self {
            now: Rc::new(Cell::new(start)),
        }
    }

    fn advance(&self, ms: u64) {
        self.now.set(self.now.get() + ms);
    }
}

impl Clock for FakeClock {
    fn now_unix_ms(&self) -> u64 {
        self.now.get()
    }
}

// ----------------------------------------------------------------------
// Happy path: negotiation -> grant -> lease -> dispatch
// ----------------------------------------------------------------------

#[test]
fn negotiation_grant_lease_dispatch_succeeds() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();
    broker
        .register_provider(echo_provider("terminal-provider", &capability))
        .unwrap();

    let grant = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-0001", vec![capability.clone()]))
        .unwrap();

    // grant summary carries the facts the surface needs for dispatch
    assert_eq!(grant.agent_id, AGENT);
    assert_eq!(grant.activation_id, "act-0001");
    assert_eq!(grant.generation, 1);
    assert_eq!(grant.capabilities, vec![capability.clone()]);
    assert_eq!(grant.expires_at_unix_ms, T0 + 60_000);
    assert_eq!(grant.granted_at_unix_ms, T0);

    // one grant + one lease were created via the existing mechanism
    assert_eq!(broker.grant_count(), 1);
    assert_eq!(broker.lease_count(), 1);
    assert_eq!(broker.agent_generation(AGENT, "act-0001"), Some(1));
    assert_eq!(broker.agent_activation_count(), 1);

    // dispatch through the existing gate: agent id as owner
    let result = broker
        .dispatch(
            "terminal-provider",
            &invoke(&capability, AGENT, grant.generation, &grant.scope),
        )
        .unwrap();
    assert_eq!(result.payload, serde_json::json!({ "method": "mutate" }));

    // replaying the same result is an idempotent no-op (same generation,
    // no extra state)
    let replay = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-0001", vec![capability]))
        .unwrap();
    assert_eq!(replay.generation, 1);
    assert_eq!(broker.grant_count(), 1);
    assert_eq!(broker.lease_count(), 1);
    assert_eq!(broker.agent_activation_count(), 1);
}

#[test]
fn multi_capability_activation_grants_each_capability() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let terminal = terminal();
    let browser = browser();
    broker
        .register_provider(echo_provider("terminal-provider", &terminal))
        .unwrap();
    broker
        .register_provider(echo_provider("browser-provider", &browser))
        .unwrap();

    // one negotiation granting two capabilities -> one grant + one lease
    // per capability, all under the same activation generation
    let grant = broker
        .broker_grant_from_negotiation(
            AGENT,
            known_result("act-0001", vec![terminal.clone(), browser.clone()]),
        )
        .unwrap();
    assert_eq!(grant.generation, 1);
    assert_eq!(grant.capabilities, vec![terminal.clone(), browser.clone()]);
    assert_eq!(broker.grant_count(), 2);
    assert_eq!(broker.lease_count(), 2);

    // both capabilities dispatch through the existing gate
    assert!(
        broker
            .dispatch(
                "terminal-provider",
                &invoke(&terminal, AGENT, grant.generation, &grant.scope),
            )
            .is_ok()
    );
    assert!(
        broker
            .dispatch(
                "browser-provider",
                &invoke(&browser, AGENT, grant.generation, &grant.scope),
            )
            .is_ok()
    );
}

// ----------------------------------------------------------------------
// No negotiation / unknown activation
// ----------------------------------------------------------------------

#[test]
fn no_negotiation_is_rejected() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();

    // a result that never reached agreement is refused and creates no state
    let result = AgentNegotiationResult {
        agreed: false,
        ..known_result("act-0001", vec![capability.clone()])
    };
    let err = broker
        .broker_grant_from_negotiation(AGENT, result)
        .unwrap_err();
    assert_eq!(err, AgentBridgeError::NotNegotiated);
    assert_eq!(err.protocol_code(), "UNAUTHORIZED");
    assert!(!err.retryable());
    assert_eq!(broker.grant_count(), 0);
    assert_eq!(broker.lease_count(), 0);
    assert_eq!(broker.agent_activation_count(), 0);

    // and dispatch without any negotiation/grant is rejected by the gate
    broker
        .register_provider(echo_provider("terminal-provider", &capability))
        .unwrap();
    let err = broker
        .dispatch(
            "terminal-provider",
            &invoke(&capability, AGENT, 1, &full_scope()),
        )
        .unwrap_err();
    assert_eq!(err, BrokerError::UnknownCapability);
    assert_eq!(err.protocol_code(), "UNAVAILABLE");
}

// ----------------------------------------------------------------------
// Lease expiry
// ----------------------------------------------------------------------

#[test]
fn expired_lease_blocks_dispatch() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock.clone());
    let capability = terminal();
    broker
        .register_provider(echo_provider("terminal-provider", &capability))
        .unwrap();
    let grant = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-0001", vec![capability.clone()]))
        .unwrap();

    // valid while the lease is live
    assert!(
        broker
            .dispatch(
                "terminal-provider",
                &invoke(&capability, AGENT, grant.generation, &grant.scope),
            )
            .is_ok()
    );

    // past expiry the gate rejects with the protocol code
    clock.advance(60_001);
    let err = broker
        .dispatch(
            "terminal-provider",
            &invoke(&capability, AGENT, grant.generation, &grant.scope),
        )
        .unwrap_err();
    assert_eq!(err, BrokerError::LeaseExpired);
    assert_eq!(err.protocol_code(), "UNAUTHORIZED");

    // durable expiry revocation record (AC-LEASE-001)
    let lease_id = broker.leases_for(&capability)[0].id.clone();
    broker
        .revoke(&lease_id, LeaseRevocationReason::Expiry)
        .unwrap();
    let revocation = broker
        .get_lease(&lease_id)
        .unwrap()
        .revoked
        .clone()
        .unwrap();
    assert_eq!(revocation.reason, LeaseRevocationReason::Expiry);
    assert_eq!(revocation.at_unix_ms, T0 + 60_001);
}

// ----------------------------------------------------------------------
// Human takeover (AC-BRW-002)
// ----------------------------------------------------------------------

#[test]
fn human_takeover_revokes_activation_and_blocks_replay() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock.clone());
    let capability = terminal();
    broker
        .register_provider(echo_provider("terminal-provider", &capability))
        .unwrap();
    let grant = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-0001", vec![capability.clone()]))
        .unwrap();

    // takeover revokes the activation leases with human_takeover
    assert_eq!(broker.revoke_agent_grants("act-0001"), 1);
    let lease = &broker.leases_for(&capability)[0];
    assert_eq!(
        lease.revoked.as_ref().unwrap().reason,
        LeaseRevocationReason::HumanTakeover
    );
    assert_eq!(lease.revoked.as_ref().unwrap().at_unix_ms, T0);
    assert!(broker.agent_activation_revoked(AGENT, "act-0001"));

    // mutation dispatch is now rejected (fail-closed)
    let err = broker
        .dispatch(
            "terminal-provider",
            &invoke(&capability, AGENT, grant.generation, &grant.scope),
        )
        .unwrap_err();
    assert_eq!(err, BrokerError::LeaseRevoked);
    assert_eq!(err.protocol_code(), "UNAUTHORIZED");

    // replaying the same negotiation result is refused forever
    let err = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-0001", vec![capability.clone()]))
        .unwrap_err();
    assert_eq!(err, AgentBridgeError::ActivationRevoked);
    assert_eq!(err.protocol_code(), "UNAUTHORIZED");
    assert_eq!(broker.grant_count(), 1);
    assert_eq!(broker.lease_count(), 1);

    // revoking again is an idempotent no-op; unknown ids are a no-op
    assert_eq!(broker.revoke_agent_grants("act-0001"), 0);
    assert_eq!(broker.revoke_agent_grants("act-9999"), 0);
}

// ----------------------------------------------------------------------
// Activation ownership (ADR-0018 decision 1)
// ----------------------------------------------------------------------

#[test]
fn new_activation_supersedes_previous_and_blocks_stale_replay() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();
    broker
        .register_provider(echo_provider("terminal-provider", &capability))
        .unwrap();

    // activation A at generation 1
    let grant_a = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-a", vec![capability.clone()]))
        .unwrap();
    assert_eq!(grant_a.generation, 1);
    assert!(
        broker
            .dispatch(
                "terminal-provider",
                &invoke(&capability, AGENT, grant_a.generation, &grant_a.scope),
            )
            .is_ok()
    );

    // activation B: independent, fresh generation
    let grant_b = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-b", vec![capability.clone()]))
        .unwrap();
    assert_eq!(grant_b.generation, 2);
    assert_eq!(broker.agent_generation(AGENT, "act-a"), Some(1));
    assert_eq!(broker.agent_generation(AGENT, "act-b"), Some(2));

    // A was superseded: old lease revoked (generation_change), stale
    // dispatch rejected with STALE_GENERATION
    let leases = broker.leases_for(&capability);
    let lease_a = leases
        .iter()
        .find(|l| l.owner == AGENT && l.generation == 1)
        .unwrap();
    assert_eq!(
        lease_a.revoked.as_ref().unwrap().reason,
        LeaseRevocationReason::GenerationChange
    );
    let err = broker
        .dispatch(
            "terminal-provider",
            &invoke(&capability, AGENT, grant_a.generation, &grant_a.scope),
        )
        .unwrap_err();
    assert_eq!(err, BrokerError::GenerationMismatch);
    assert_eq!(err.protocol_code(), "STALE_GENERATION");

    // replaying activation A is refused (not current)
    let err = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-a", vec![capability.clone()]))
        .unwrap_err();
    assert_eq!(err, AgentBridgeError::StaleActivation);
    assert_eq!(err.protocol_code(), "STALE_GENERATION");

    // B dispatches normally
    assert!(
        broker
            .dispatch(
                "terminal-provider",
                &invoke(&capability, AGENT, grant_b.generation, &grant_b.scope),
            )
            .is_ok()
    );
}

#[test]
fn distinct_agents_are_independent() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let terminal = terminal();
    let browser = browser();
    broker
        .register_provider(echo_provider("terminal-provider", &terminal))
        .unwrap();
    broker
        .register_provider(echo_provider("browser-provider", &browser))
        .unwrap();

    let grant_1 = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-1", vec![terminal.clone()]))
        .unwrap();
    assert_eq!(grant_1.generation, 1);
    assert!(
        broker
            .dispatch(
                "terminal-provider",
                &invoke(&terminal, AGENT, 1, &grant_1.scope),
            )
            .is_ok()
    );

    // The ADR-0014 grant map is single-owner per capability: a second
    // agent activating the SAME capability at the same generation
    // conflicts deterministically (fail-closed; no state harm).
    let err = broker
        .broker_grant_from_negotiation(AGENT2, known_result("act-2", vec![terminal.clone()]))
        .unwrap_err();
    assert_eq!(err, AgentBridgeError::Broker(BrokerError::Conflict));
    assert_eq!(broker.agent_generation(AGENT2, "act-2"), Some(1));
    assert!(
        broker
            .dispatch(
                "terminal-provider",
                &invoke(&terminal, AGENT, 1, &grant_1.scope),
            )
            .is_ok()
    );

    // A different capability is fully independent: agent2 gets its own
    // grant/lease (its generation counter kept advancing monotonically).
    let grant_2 = broker
        .broker_grant_from_negotiation(AGENT2, known_result("act-3", vec![browser.clone()]))
        .unwrap();
    assert_eq!(grant_2.generation, 2);
    assert_eq!(broker.grant_count(), 2);
    assert_eq!(broker.lease_count(), 2);
    assert!(
        broker
            .dispatch(
                "browser-provider",
                &invoke(&browser, AGENT2, grant_2.generation, &grant_2.scope),
            )
            .is_ok()
    );

    // Cross-owner identity on the other agent capability is rejected.
    let err = broker
        .dispatch(
            "browser-provider",
            &invoke(&browser, AGENT, grant_2.generation, &grant_2.scope),
        )
        .unwrap_err();
    assert_eq!(err, BrokerError::NotGranted);

    // ADR-0014 d4 supersession is capability-wide: agent2's next terminal
    // activation at a fresh generation replaces agent1's grant and
    // revokes its leases; agent1's dispatch is then rejected by the fixed
    // gate order (owner mismatch fires before generation) - fail-closed,
    // documented inherited boundary.
    let grant_3 = broker
        .broker_grant_from_negotiation(AGENT2, known_result("act-4", vec![terminal.clone()]))
        .unwrap();
    assert_eq!(grant_3.generation, 3);
    let err = broker
        .dispatch(
            "terminal-provider",
            &invoke(&terminal, AGENT, grant_1.generation, &grant_1.scope),
        )
        .unwrap_err();
    assert_eq!(err, BrokerError::NotGranted);
    assert!(
        broker
            .dispatch(
                "terminal-provider",
                &invoke(&terminal, AGENT2, grant_3.generation, &grant_3.scope),
            )
            .is_ok()
    );
}

// ----------------------------------------------------------------------
// Conformance tri-state (ADR-0018 decision 2): Known is required
// ----------------------------------------------------------------------

#[test]
fn conformance_known_required_for_grant() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();

    for state in [
        AgentConformanceState::Absent,
        AgentConformanceState::Unknown,
    ] {
        let result = AgentNegotiationResult {
            conformance: state,
            ..known_result("act-0001", vec![capability.clone()])
        };
        let err = broker
            .broker_grant_from_negotiation(AGENT, result)
            .unwrap_err();
        assert_eq!(
            err,
            AgentBridgeError::ConformanceNotKnown,
            "state {:?}",
            state
        );
        assert_eq!(err.protocol_code(), "UNAVAILABLE");
        assert!(err.retryable());
        assert!(!state.is_l2());
        assert_eq!(
            state.as_str(),
            if state == AgentConformanceState::Absent {
                "absent"
            } else {
                "unknown"
            }
        );
    }
    // fail-closed: no grant, no lease, no register entry
    assert_eq!(broker.grant_count(), 0);
    assert_eq!(broker.lease_count(), 0);
    assert_eq!(broker.agent_activation_count(), 0);

    // Known proceeds
    let grant = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-0001", vec![capability]))
        .unwrap();
    assert_eq!(grant.generation, 1);
    assert_eq!(broker.grant_count(), 1);
}

// ----------------------------------------------------------------------
// Fail-closed input matrix
// ----------------------------------------------------------------------

#[test]
fn empty_grant_and_missing_lease_policy_fail_closed() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();

    // agreement with nothing granted
    let result = AgentNegotiationResult {
        granted: vec![],
        ..known_result("act-0001", vec![capability.clone()])
    };
    let err = broker
        .broker_grant_from_negotiation(AGENT, result)
        .unwrap_err();
    assert_eq!(err, AgentBridgeError::NothingGranted);
    assert_eq!(err.protocol_code(), "UNAUTHORIZED");

    // no lease policy at all
    let result = AgentNegotiationResult {
        lease_constraints: None,
        ..known_result("act-0001", vec![capability.clone()])
    };
    let err = broker
        .broker_grant_from_negotiation(AGENT, result)
        .unwrap_err();
    assert_eq!(err, AgentBridgeError::NoLeasePolicy);

    // zero max seconds is not a bounded policy
    let result = AgentNegotiationResult {
        lease_constraints: Some(AgentLeaseConstraints::new(0)),
        ..known_result("act-0001", vec![capability.clone()])
    };
    let err = broker
        .broker_grant_from_negotiation(AGENT, result)
        .unwrap_err();
    assert_eq!(err, AgentBridgeError::NoLeasePolicy);

    assert_eq!(broker.grant_count(), 0);
    assert_eq!(broker.lease_count(), 0);
    assert_eq!(broker.agent_activation_count(), 0);
}

// ----------------------------------------------------------------------
// Error contract
// ----------------------------------------------------------------------

#[test]
fn agent_bridge_errors_are_static_and_machine_readable() {
    let cases = [
        (AgentBridgeError::NotNegotiated, "UNAUTHORIZED"),
        (AgentBridgeError::NothingGranted, "UNAUTHORIZED"),
        (AgentBridgeError::NoLeasePolicy, "UNAUTHORIZED"),
        (AgentBridgeError::ConformanceNotKnown, "UNAVAILABLE"),
        (AgentBridgeError::ActivationRevoked, "UNAUTHORIZED"),
        (AgentBridgeError::StaleActivation, "STALE_GENERATION"),
        (AgentBridgeError::Broker(BrokerError::Conflict), "CONFLICT"),
    ];
    for (error, code) in cases {
        assert_eq!(error.protocol_code(), code);
        let message = error.to_string();
        assert!(message.contains(code));
        // no user-controlled values appear in the message
        assert!(!message.contains(AGENT));
        assert!(!message.contains("act-"));
        assert!(!message.contains("ws-a"));
        assert!(!message.contains("agent-1"));
    }

    // retryable only when the outcome can change (conformance / provider)
    assert!(AgentBridgeError::ConformanceNotKnown.retryable());
    assert!(AgentBridgeError::Broker(BrokerError::UnknownCapability).retryable());
    for error in [
        AgentBridgeError::NotNegotiated,
        AgentBridgeError::NothingGranted,
        AgentBridgeError::NoLeasePolicy,
        AgentBridgeError::ActivationRevoked,
        AgentBridgeError::StaleActivation,
        AgentBridgeError::Broker(BrokerError::LeaseRevoked),
    ] {
        assert!(!error.retryable(), "{:?}", error);
    }
}

#[test]
fn broker_conflict_passthrough_is_deterministic() {
    // A surface-side grant conflict (same capability/owner/generation,
    // divergent fields) surfaces as Broker(Conflict); the register entry
    // is reserved so a retry follows the same deterministic path.
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();
    broker
        .grant(super::super::CapabilityGrant {
            capability_id: capability.clone(),
            version: "v1alpha1".into(),
            scope: other_workspace(),
            owner: AGENT.into(),
            generation: 1,
            created_at_unix_ms: T0,
        })
        .unwrap();

    let err = broker
        .broker_grant_from_negotiation(AGENT, known_result("act-0001", vec![capability]))
        .unwrap_err();
    assert_eq!(err, AgentBridgeError::Broker(BrokerError::Conflict));
    assert_eq!(err.protocol_code(), "CONFLICT");
    assert_eq!(broker.agent_activation_count(), 1); // register reserved
    assert_eq!(broker.lease_count(), 0);
}

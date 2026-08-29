//! Broker tests: dispatch gate, AC-LEASE-001 revocation matrix, idempotency
//! and CONFLICT semantics (ADR-0014 verification gates).

use std::cell::Cell;
use std::rc::Rc;

use super::*;

const API_VERSION: &str = "interop.dsh-desktop.local/v1alpha1";
const OWNER: &str = "participant-a";
const T0: u64 = 1_000_000;

fn terminal() -> CapabilityId {
    CapabilityId::new(API_VERSION, "Terminal")
}

fn browser() -> CapabilityId {
    CapabilityId::new(API_VERSION, "Browser")
}

fn grant(
    capability: &CapabilityId,
    owner: &str,
    generation: u64,
    scope: &Scope,
    at: u64,
) -> CapabilityGrant {
    CapabilityGrant {
        capability_id: capability.clone(),
        version: "v1alpha1".into(),
        scope: scope.clone(),
        owner: owner.into(),
        generation,
        created_at_unix_ms: at,
    }
}

fn lease(
    id: &str,
    capability: &CapabilityId,
    owner: &str,
    generation: u64,
    scope: &Scope,
    expires_at: u64,
) -> Lease {
    Lease {
        id: id.into(),
        capability_id: capability.clone(),
        owner: owner.into(),
        generation,
        scope: scope.clone(),
        expires_at_unix_ms: expires_at,
        revoked: None,
    }
}

fn full_scope() -> Scope {
    Scope {
        session_id: Some("session-1".into()),
        workspace: Some("ws-a".into()),
        ..Default::default()
    }
}

fn workspace(workspace: &str) -> Scope {
    Scope {
        workspace: Some(workspace.into()),
        ..Default::default()
    }
}

fn invoke(
    capability: &CapabilityId,
    method: &str,
    owner: &str,
    generation: u64,
    scope: &Scope,
) -> Invocation {
    Invocation {
        capability: capability.clone(),
        method: method.into(),
        owner: owner.into(),
        generation,
        scope: scope.clone(),
        payload: serde_json::json!({ "method": method }),
    }
}

fn echo_provider(id: &str, capability: &CapabilityId) -> Provider {
    Provider::new(id.to_string(), capability.clone(), |inv| InvocationResult {
        payload: inv.payload.clone(),
    })
}

/// Broker with a granted capability, a registered provider and one valid
/// lease (`lease-0001`), all at generation 1 / OWNER / full scope.
fn granted_broker_with_lease(clock: FakeClock) -> (Broker<FakeClock>, CapabilityId, Scope) {
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();
    let scope = full_scope();
    broker
        .grant(grant(&capability, OWNER, 1, &scope, T0))
        .unwrap();
    broker
        .register_provider(echo_provider("terminal-provider", &capability))
        .unwrap();
    broker
        .lease(lease(
            "lease-0001",
            &capability,
            OWNER,
            1,
            &scope,
            T0 + 60_000,
        ))
        .unwrap();
    (broker, capability, scope)
}

/// Deterministic clock: advance without sleeping (ADR-0014 decision 8).
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
// Happy path
// ----------------------------------------------------------------------

#[test]
fn grant_lease_dispatch_succeeds() {
    let clock = FakeClock::new(T0);
    let (broker, capability, scope) = granted_broker_with_lease(clock);

    let invocation = invoke(&capability, "spawn", OWNER, 1, &scope);
    let result = broker.dispatch("terminal-provider", &invocation).unwrap();
    assert_eq!(result.payload, invocation.payload);

    // state is observable
    assert_eq!(broker.grant_count(), 1);
    assert_eq!(broker.lease_count(), 1);
    assert!(broker.get_lease("lease-0001").is_some());
    assert_eq!(broker.leases_for(&capability).len(), 1);
    assert!(broker.provider("terminal-provider").is_some());
}

// ----------------------------------------------------------------------
// Unknown capability / provider
// ----------------------------------------------------------------------

#[test]
fn unknown_capability_is_rejected() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();
    let scope = full_scope();
    broker
        .register_provider(echo_provider("terminal-provider", &capability))
        .unwrap();

    // no grant exists: dispatch and enforcement reject
    let invocation = invoke(&capability, "spawn", OWNER, 1, &scope);
    let err = broker
        .dispatch("terminal-provider", &invocation)
        .unwrap_err();
    assert_eq!(err, BrokerError::UnknownCapability);
    assert_eq!(err.protocol_code(), "UNAVAILABLE");
    assert!(err.retryable());

    // leasing an un-granted capability is rejected too
    let err = broker
        .lease(lease(
            "lease-0001",
            &capability,
            OWNER,
            1,
            &scope,
            T0 + 60_000,
        ))
        .unwrap_err();
    assert_eq!(err, BrokerError::UnknownCapability);

    let err = broker
        .enforce_dispatch(&capability, OWNER, 1, &scope)
        .unwrap_err();
    assert_eq!(err, BrokerError::UnknownCapability);
}

#[test]
fn unknown_provider_and_capability_mismatch_are_rejected() {
    let clock = FakeClock::new(T0);
    let (mut broker, capability, scope) = granted_broker_with_lease(clock);

    let invocation = invoke(&capability, "spawn", OWNER, 1, &scope);
    let err = broker
        .dispatch("missing-provider", &invocation)
        .unwrap_err();
    assert_eq!(err, BrokerError::UnknownProvider);

    // duplicate registration conflicts
    assert_eq!(
        broker.register_provider(echo_provider("terminal-provider", &capability)),
        Err(BrokerError::Conflict)
    );

    // a provider serving a different capability rejects the invocation
    let other = browser();
    broker
        .register_provider(echo_provider("browser-provider", &other))
        .unwrap();
    let err = broker
        .dispatch("browser-provider", &invocation)
        .unwrap_err();
    assert_eq!(err, BrokerError::UnknownCapability);
}

// ----------------------------------------------------------------------
// AC-LEASE-001: expiry
// ----------------------------------------------------------------------

#[test]
fn expired_lease_blocks_dispatch_and_expiry_revocation_is_recorded() {
    let clock = FakeClock::new(T0);
    let (mut broker, capability, scope) = granted_broker_with_lease(clock.clone());

    let invocation = invoke(&capability, "spawn", OWNER, 1, &scope);
    // valid before expiry
    assert!(broker.dispatch("terminal-provider", &invocation).is_ok());

    // advance past expiry: dispatch is rejected
    clock.advance(60_001);
    let err = broker
        .dispatch("terminal-provider", &invocation)
        .unwrap_err();
    assert_eq!(err, BrokerError::LeaseExpired);
    assert_eq!(err.protocol_code(), "UNAUTHORIZED");
    assert!(!err.retryable());

    // durable expiry revocation record
    broker
        .revoke("lease-0001", LeaseRevocationReason::Expiry)
        .unwrap();
    let revocation = broker
        .get_lease("lease-0001")
        .unwrap()
        .revoked
        .clone()
        .unwrap();
    assert_eq!(revocation.reason, LeaseRevocationReason::Expiry);
    assert_eq!(revocation.at_unix_ms, T0 + 60_001);

    // revoked takes precedence over expiry in error reporting
    let err = broker
        .dispatch("terminal-provider", &invocation)
        .unwrap_err();
    assert_eq!(err, BrokerError::LeaseRevoked);
}

#[test]
fn already_expired_lease_cannot_be_issued() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();
    let scope = full_scope();
    broker
        .grant(grant(&capability, OWNER, 1, &scope, T0))
        .unwrap();

    // constructing a lease with a past expiry is rejected at issue time
    let err = broker
        .lease(lease("lease-0001", &capability, OWNER, 1, &scope, T0 - 1))
        .unwrap_err();
    assert_eq!(err, BrokerError::LeaseExpired);
}

// ----------------------------------------------------------------------
// AC-LEASE-001: disconnect / unload / human takeover
// ----------------------------------------------------------------------

fn assert_revoked_dispatch_blocked(reason: LeaseRevocationReason) {
    let clock = FakeClock::new(T0);
    let (mut broker, capability, scope) = granted_broker_with_lease(clock);
    broker.revoke("lease-0001", reason).unwrap();

    let invocation = invoke(&capability, "spawn", OWNER, 1, &scope);
    let err = broker
        .dispatch("terminal-provider", &invocation)
        .unwrap_err();
    assert_eq!(err, BrokerError::LeaseRevoked);
    assert_eq!(err.protocol_code(), "UNAUTHORIZED");

    let revocation = broker
        .get_lease("lease-0001")
        .unwrap()
        .revoked
        .clone()
        .unwrap();
    assert_eq!(revocation.reason, reason);
    assert_eq!(revocation.at_unix_ms, T0);
}

#[test]
fn disconnect_revokes_lease() {
    assert_revoked_dispatch_blocked(LeaseRevocationReason::Disconnect);
}

#[test]
fn unload_revokes_lease() {
    assert_revoked_dispatch_blocked(LeaseRevocationReason::Unload);
}

#[test]
fn human_takeover_revokes_lease() {
    assert_revoked_dispatch_blocked(LeaseRevocationReason::HumanTakeover);
}

// ----------------------------------------------------------------------
// AC-LEASE-001: generation change
// ----------------------------------------------------------------------

#[test]
fn generation_change_revokes_leases_and_blocks_stale_dispatch() {
    let clock = FakeClock::new(T0);
    let (mut broker, capability, scope) = granted_broker_with_lease(clock.clone());

    // time passes, then a new grant for the same capability at a new generation
    clock.advance(1_000);
    broker
        .grant(grant(&capability, OWNER, 2, &scope, T0 + 1_000))
        .unwrap();

    // the old lease was auto-revoked with reason generation_change
    let revocation = broker
        .get_lease("lease-0001")
        .unwrap()
        .revoked
        .clone()
        .unwrap();
    assert_eq!(revocation.reason, LeaseRevocationReason::GenerationChange);
    assert_eq!(revocation.at_unix_ms, T0 + 1_000);

    // stale generation is rejected with the protocol code
    let stale = invoke(&capability, "spawn", OWNER, 1, &scope);
    let err = broker.dispatch("terminal-provider", &stale).unwrap_err();
    assert_eq!(err, BrokerError::GenerationMismatch);
    assert_eq!(err.protocol_code(), "STALE_GENERATION");

    // current generation without a lease is not granted
    let current = invoke(&capability, "spawn", OWNER, 2, &scope);
    let err = broker.dispatch("terminal-provider", &current).unwrap_err();
    assert_eq!(err, BrokerError::NotGranted);

    // a fresh lease under the new generation dispatches
    broker
        .lease(lease(
            "lease-0002",
            &capability,
            OWNER,
            2,
            &scope,
            T0 + 61_000,
        ))
        .unwrap();
    assert!(broker.dispatch("terminal-provider", &current).is_ok());
}

// ----------------------------------------------------------------------
// Idempotency / CONFLICT
// ----------------------------------------------------------------------

#[test]
fn duplicate_grant_is_idempotent_and_divergent_grants_conflict() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();
    let scope = full_scope();
    let original = grant(&capability, OWNER, 1, &scope, T0);

    broker.grant(original.clone()).unwrap();
    broker.grant(original.clone()).unwrap(); // identical re-grant: idempotent
    assert_eq!(broker.grant_count(), 1);

    // same capability + same generation, divergent scope: CONFLICT
    let divergent_scope = grant(&capability, OWNER, 1, &workspace("ws-b"), T0);
    assert_eq!(broker.grant(divergent_scope), Err(BrokerError::Conflict));

    // same capability + same generation, divergent owner: CONFLICT
    let divergent_owner = grant(&capability, "participant-b", 1, &scope, T0);
    assert_eq!(broker.grant(divergent_owner), Err(BrokerError::Conflict));
    assert_eq!(broker.grant_count(), 1);

    // identical re-lease is idempotent too
    let lease = lease("lease-0001", &capability, OWNER, 1, &scope, T0 + 60_000);
    broker.lease(lease.clone()).unwrap();
    broker.lease(lease.clone()).unwrap();
    assert_eq!(broker.lease_count(), 1);
}

#[test]
fn revoked_lease_slot_conflicts_and_error_is_deterministic() {
    let clock = FakeClock::new(T0);
    let (mut broker, capability, scope) = granted_broker_with_lease(clock);
    broker
        .revoke("lease-0001", LeaseRevocationReason::Disconnect)
        .unwrap();

    let invocation = invoke(&capability, "spawn", OWNER, 1, &scope);
    // deterministic, idempotent error: every retry reports the same result
    assert_eq!(
        broker.dispatch("terminal-provider", &invocation),
        Err(BrokerError::LeaseRevoked)
    );
    assert_eq!(
        broker.dispatch("terminal-provider", &invocation),
        Err(BrokerError::LeaseRevoked)
    );

    // re-revoke is an idempotent no-op and keeps the first record
    broker
        .revoke("lease-0001", LeaseRevocationReason::HumanTakeover)
        .unwrap();
    let revocation = broker
        .get_lease("lease-0001")
        .unwrap()
        .revoked
        .clone()
        .unwrap();
    assert_eq!(revocation.reason, LeaseRevocationReason::Disconnect);

    // the revoked lease id is never reusable: re-issue conflicts
    let err = broker
        .lease(lease(
            "lease-0001",
            &capability,
            OWNER,
            1,
            &scope,
            T0 + 60_000,
        ))
        .unwrap_err();
    assert_eq!(err, BrokerError::Conflict);
    assert_eq!(err.protocol_code(), "CONFLICT");

    // revoking an unknown id is an idempotent no-op
    assert!(
        broker
            .revoke("lease-9999", LeaseRevocationReason::Unload)
            .is_ok()
    );
}

// ----------------------------------------------------------------------
// Owner / generation / scope gates
// ----------------------------------------------------------------------

#[test]
fn owner_and_generation_mismatches_are_rejected() {
    let clock = FakeClock::new(T0);
    let (mut broker, capability, scope) = granted_broker_with_lease(clock);

    let wrong_owner = invoke(&capability, "spawn", "participant-b", 1, &scope);
    let err = broker
        .dispatch("terminal-provider", &wrong_owner)
        .unwrap_err();
    assert_eq!(err, BrokerError::NotGranted);

    let wrong_generation = invoke(&capability, "spawn", OWNER, 99, &scope);
    let err = broker
        .dispatch("terminal-provider", &wrong_generation)
        .unwrap_err();
    assert_eq!(err, BrokerError::GenerationMismatch);

    // lease-level mismatches are rejected at issue time
    let err = broker
        .lease(lease(
            "lease-0010",
            &capability,
            "participant-b",
            1,
            &scope,
            T0 + 60_000,
        ))
        .unwrap_err();
    assert_eq!(err, BrokerError::NotGranted);
    let err = broker
        .lease(lease(
            "lease-0011",
            &capability,
            OWNER,
            99,
            &scope,
            T0 + 60_000,
        ))
        .unwrap_err();
    assert_eq!(err, BrokerError::GenerationMismatch);
}

#[test]
fn scope_not_covered_is_rejected() {
    let clock = FakeClock::new(T0);
    let (mut broker, capability, _) = granted_broker_with_lease(clock);

    // request outside the granted/leased scope
    let outside = invoke(&capability, "spawn", OWNER, 1, &workspace("ws-b"));
    let err = broker.dispatch("terminal-provider", &outside).unwrap_err();
    assert_eq!(err, BrokerError::ScopeMismatch);
    assert_eq!(err.protocol_code(), "UNAUTHORIZED");

    // a lease whose scope the grant does not cover is rejected at issue time
    let err = broker
        .lease(lease(
            "lease-0009",
            &capability,
            OWNER,
            1,
            &workspace("ws-b"),
            T0 + 60_000,
        ))
        .unwrap_err();
    assert_eq!(err, BrokerError::ScopeMismatch);
}

#[test]
fn lease_scope_narrower_than_grant_is_rejected_at_dispatch() {
    let clock = FakeClock::new(T0);
    let mut broker = Broker::with_clock(clock);
    let capability = terminal();
    // the grant is session-wide; the lease additionally pins a workspace
    let grant_scope = Scope {
        session_id: Some("session-1".into()),
        ..Default::default()
    };
    let lease_scope = Scope {
        session_id: Some("session-1".into()),
        workspace: Some("ws-a".into()),
        ..Default::default()
    };
    broker
        .grant(grant(&capability, OWNER, 1, &grant_scope, T0))
        .unwrap();
    broker
        .register_provider(echo_provider("terminal-provider", &capability))
        .unwrap();
    broker
        .lease(lease(
            "lease-0001",
            &capability,
            OWNER,
            1,
            &lease_scope,
            T0 + 60_000,
        ))
        .unwrap();

    // within the grant but outside the lease scope: rejected
    let outside_lease = Scope {
        session_id: Some("session-1".into()),
        workspace: Some("ws-b".into()),
        ..Default::default()
    };
    let invocation = invoke(&capability, "spawn", OWNER, 1, &outside_lease);
    let err = broker
        .dispatch("terminal-provider", &invocation)
        .unwrap_err();
    assert_eq!(err, BrokerError::ScopeMismatch);

    // inside the lease scope: dispatches
    let invocation = invoke(&capability, "spawn", OWNER, 1, &lease_scope);
    assert!(broker.dispatch("terminal-provider", &invocation).is_ok());
}

// ----------------------------------------------------------------------
// Error contract
// ----------------------------------------------------------------------

#[test]
fn error_messages_are_static_and_machine_readable() {
    // every error carries a protocol code, a static message and no user data
    let cases = [
        (BrokerError::UnknownCapability, "UNAVAILABLE"),
        (BrokerError::UnknownProvider, "UNAVAILABLE"),
        (BrokerError::NotGranted, "UNAUTHORIZED"),
        (BrokerError::LeaseExpired, "UNAUTHORIZED"),
        (BrokerError::LeaseRevoked, "UNAUTHORIZED"),
        (BrokerError::ScopeMismatch, "UNAUTHORIZED"),
        (BrokerError::GenerationMismatch, "STALE_GENERATION"),
        (BrokerError::Conflict, "CONFLICT"),
    ];
    for (error, code) in cases {
        assert_eq!(error.protocol_code(), code);
        let message = error.to_string();
        assert!(message.contains(code));
        // no user-controlled values appear in the message
        assert!(!message.contains(OWNER));
        assert!(!message.contains("ws-a"));
        assert!(!message.contains("lease-0001"));
        assert!(!message.contains("token"));
    }
}

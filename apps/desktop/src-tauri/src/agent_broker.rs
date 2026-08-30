//! Shared Capability Broker surface state (M5-E authorization chain,
//! ADR-0018 decision 7, AC-BRW-002 / AC-TERM-001).
//!
//! Wraps the P0 broker (crates/supervisor, ADR-0014) for the tauri app:
//! a negotiation result is mapped into broker grants/leases through the
//! broker/agent.rs bridge, and every agent mutation (browser interact,
//! terminal agent write once M5-E2 wires it) enforces the ADR-0014
//! dispatch gate before executing. The broker itself stays DSH-neutral;
//! this module is the surface-side mapping layer (broker/agent.rs module
//! docs).
//!
//! Agent identity (protocol layering): the wire envelope identifies a
//! participant by component/facet/activationId, while the request payload
//! schemas carry only session-scoped fields. The surface keeps two
//! registers established here:
//! - activation -> agent id (set at grant time, so a mutation command
//!   only needs to carry the activation id);
//! - session -> bound activation ids (from the grant scope session_id,
//!   plus every session an interact is actually authorized on), so a
//!   human takeover (AC-BRW-002) knows exactly which agent activations
//!   to revoke through Broker::revoke_agent_grants.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use dsh_supervisor::{
    AgentBridgeError, AgentGrant, AgentNegotiationResult, Broker, BrokerError, CapabilityId, Scope,
    SystemClock,
};

/// Browser capability coordinate (specs/protocol/fixtures/envelope.agreement.valid.json).
/// browser.dsh-desktop.local/v1alpha1 + Browser — the coordinate the
/// IF-NEGOTIATION Agreement grants (cf. terminal.rs terminal_capability()).
/// The browser bridge (browser.rs) keeps its own copy next to the interact
/// gate; this one serves the register helpers and their tests.
#[cfg_attr(not(test), allow(dead_code))]
pub fn browser_capability() -> CapabilityId {
    CapabilityId::new("browser.dsh-desktop.local/v1alpha1", "Browser")
}

/// Broker surface errors mapped to protocol codes (docs/protocol/ERROR_MODEL.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerSurfaceError {
    /// The activation id is unknown to the surface: never granted, or
    /// already revoked/taken over.
    UnknownActivation,
    /// The ADR-0014 dispatch gate rejected the mutation.
    Broker(BrokerError),
}

impl BrokerSurfaceError {
    pub fn protocol_code(self) -> &'static str {
        match self {
            Self::UnknownActivation => "UNAUTHORIZED",
            Self::Broker(error) => error.protocol_code(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn retryable(self) -> bool {
        match self {
            Self::UnknownActivation => false,
            Self::Broker(error) => error.retryable(),
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::UnknownActivation => "agent activation is unknown or taken over",
            Self::Broker(error) => match error {
                BrokerError::UnknownCapability => "browser capability is not granted",
                BrokerError::UnknownProvider => "browser provider is not registered",
                BrokerError::NotGranted => "not granted for this agent",
                BrokerError::LeaseExpired => "agent lease expired",
                BrokerError::LeaseRevoked => "agent lease revoked",
                BrokerError::ScopeMismatch => "agent scope does not cover this session",
                BrokerError::GenerationMismatch => "stale generation",
                BrokerError::Conflict => "broker state conflict",
            },
        }
    }
}

impl fmt::Display for BrokerSurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message(), self.protocol_code())
    }
}

impl std::error::Error for BrokerSurfaceError {}

/// App-managed broker state: the P0 broker plus the surface-side agent
/// ownership registers (activation -> agent, session -> activations).
///
/// The register methods (grant_from_negotiation / enforce_mutation /
/// revoke_for_session) are the surface entry points for the M5-E browser
/// interact chain; the browser bridge currently enforces through the raw
/// broker handle (browser.rs authorize_interact), and the M5-E negotiation
/// surface lands on these helpers. Kept until then (dead-code allowed
/// outside tests).
#[derive(Clone, Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct BrokerState {
    inner: Arc<Mutex<Broker<SystemClock>>>,
    /// activation_id -> agent_id, established at grant time.
    activation_owners: Arc<Mutex<HashMap<String, String>>>,
    /// session_id -> bound activation ids (grant scope + authorized
    /// mutations); consumed by human takeover (AC-BRW-002).
    session_bindings: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl BrokerState {
    /// Maps one negotiated activation into broker grants/leases and
    /// records the surface ownership registers. Fail-closed checks live
    /// in the bridge (broker/agent.rs); nothing is recorded on error.
    pub fn grant_from_negotiation(
        &self,
        agent_id: &str,
        result: AgentNegotiationResult,
    ) -> Result<AgentGrant, AgentBridgeError> {
        let grant = {
            let mut broker = self
                .inner
                .lock()
                .map_err(|_| AgentBridgeError::Broker(BrokerError::Conflict))?;
            broker.broker_grant_from_negotiation(agent_id, result)?
        };
        {
            let mut owners = self
                .activation_owners
                .lock()
                .map_err(|_| AgentBridgeError::Broker(BrokerError::Conflict))?;
            owners.insert(grant.activation_id.clone(), agent_id.to_owned());
        }
        if let Some(session_id) = grant.scope.session_id.as_deref() {
            let mut bindings = self
                .session_bindings
                .lock()
                .map_err(|_| AgentBridgeError::Broker(BrokerError::Conflict))?;
            let list = bindings.entry(session_id.to_owned()).or_default();
            if !list.contains(&grant.activation_id) {
                list.push(grant.activation_id.clone());
            }
        }
        Ok(grant)
    }

    /// The ADR-0014 dispatch gate for one agent mutation: capability
    /// granted, owner matches the activation, generation matches, grant
    /// scope covers the session, and a valid (unexpired, unrevoked)
    /// lease exists. On success the session binding is recorded (dedup)
    /// so a later human takeover revokes exactly this activation.
    pub fn enforce_mutation(
        &self,
        activation_id: &str,
        capability: &CapabilityId,
        session_id: &str,
    ) -> Result<(), BrokerSurfaceError> {
        {
            let broker = self
                .inner
                .lock()
                .map_err(|_| BrokerSurfaceError::Broker(BrokerError::Conflict))?;
            let agent_id = self
                .activation_owners
                .lock()
                .map_err(|_| BrokerSurfaceError::Broker(BrokerError::Conflict))?
                .get(activation_id)
                .cloned()
                .ok_or(BrokerSurfaceError::UnknownActivation)?;
            let generation = broker
                .agent_generation(&agent_id, activation_id)
                .ok_or(BrokerSurfaceError::UnknownActivation)?;
            let scope = Scope {
                session_id: Some(session_id.to_owned()),
                workspace: None,
                domains: Vec::new(),
                resources: Vec::new(),
            };
            broker
                .enforce_dispatch(capability, &agent_id, generation, &scope)
                .map_err(BrokerSurfaceError::Broker)?;
        }
        let mut bindings = self
            .session_bindings
            .lock()
            .map_err(|_| BrokerSurfaceError::Broker(BrokerError::Conflict))?;
        let list = bindings.entry(session_id.to_owned()).or_default();
        if !list.iter().any(|id| id == activation_id) {
            list.push(activation_id.to_owned());
        }
        Ok(())
    }

    /// Human takeover (AC-BRW-002): revokes every lease of every agent
    /// activation bound to the session (Broker::revoke_agent_grants,
    /// durable revocation) and clears the surface ownership registers so
    /// the same activation ids can never authorize a mutation again.
    ///
    /// Idempotent: an unknown session or already-taken-over session
    /// revokes nothing and returns 0.
    pub fn revoke_for_session(&self, session_id: &str) -> usize {
        let activations: Vec<String> = self
            .session_bindings
            .lock()
            .map(|bindings| bindings.get(session_id).cloned().unwrap_or_default())
            .unwrap_or_default();
        let mut revoked = 0;
        for activation_id in &activations {
            let count = {
                let mut broker = match self.inner.lock() {
                    Ok(broker) => broker,
                    Err(_) => return revoked,
                };
                broker.revoke_agent_grants(activation_id)
            };
            revoked += count;
            if let Ok(mut owners) = self.activation_owners.lock() {
                owners.remove(activation_id);
            }
        }
        if let Ok(mut bindings) = self.session_bindings.lock() {
            bindings.remove(session_id);
        }
        revoked
    }

    /// Whether the activation was durably revoked (observability/tests).
    pub fn activation_revoked(&self, agent_id: &str, activation_id: &str) -> bool {
        self.inner
            .lock()
            .map(|broker| broker.agent_activation_revoked(agent_id, activation_id))
            .unwrap_or(false)
    }

    /// Shared handle for broker consumers that dispatch through the gate
    /// (the terminal provider registers into the broker at TerminalState
    /// construction; M5-E2). One broker instance serves every surface.
    pub fn inner(&self) -> Arc<Mutex<Broker<SystemClock>>> {
        Arc::clone(&self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn test_negotiation(
        activation_id: &str,
        session_id: &str,
        granted: Vec<CapabilityId>,
    ) -> AgentNegotiationResult {
        AgentNegotiationResult {
            activation_id: activation_id.to_owned(),
            agreed: true,
            granted,
            conformance: dsh_supervisor::AgentConformanceState::Known,
            lease_constraints: Some(dsh_supervisor::AgentLeaseConstraints::new(3600)),
            scope: Scope {
                session_id: Some(session_id.to_owned()),
                workspace: None,
                domains: Vec::new(),
                resources: Vec::new(),
            },
        }
    }

    #[test]
    fn grant_records_owner_and_session_binding() {
        let broker = BrokerState::default();
        let grant = broker
            .grant_from_negotiation(
                "agent-1",
                test_negotiation("act-1", "brw-1", vec![browser_capability()]),
            )
            .expect("grant");
        assert_eq!(grant.generation, 1);
        assert!(
            broker
                .enforce_mutation("act-1", &browser_capability(), "brw-1")
                .is_ok()
        );
    }

    #[test]
    fn enforce_requires_granted_activation() {
        let broker = BrokerState::default();
        let error = broker
            .enforce_mutation("act-unknown", &browser_capability(), "brw-1")
            .expect_err("unknown activation");
        assert_eq!(error.protocol_code(), "UNAUTHORIZED");
        assert!(!error.retryable());
    }

    #[test]
    fn enforce_rejects_session_outside_grant_scope() {
        let broker = BrokerState::default();
        broker
            .grant_from_negotiation(
                "agent-1",
                test_negotiation("act-1", "brw-1", vec![browser_capability()]),
            )
            .expect("grant");
        let error = broker
            .enforce_mutation("act-1", &browser_capability(), "brw-2")
            .expect_err("scope mismatch");
        assert_eq!(error.protocol_code(), "UNAUTHORIZED");
        assert_eq!(error.message(), "agent scope does not cover this session");
    }

    #[test]
    fn enforce_rejects_wrong_capability() {
        let broker = BrokerState::default();
        broker
            .grant_from_negotiation(
                "agent-1",
                test_negotiation("act-1", "brw-1", vec![browser_capability()]),
            )
            .expect("grant");
        let terminal = CapabilityId::new("terminal.dsh-desktop.local/v1alpha1", "Terminal");
        let error = broker
            .enforce_mutation("act-1", &terminal, "brw-1")
            .expect_err("wrong capability");
        assert_eq!(error.protocol_code(), "UNAVAILABLE");
    }

    #[test]
    fn revoke_for_session_revokes_and_blocks() {
        let broker = BrokerState::default();
        broker
            .grant_from_negotiation(
                "agent-1",
                test_negotiation("act-1", "brw-1", vec![browser_capability()]),
            )
            .expect("grant");
        assert!(
            broker
                .enforce_mutation("act-1", &browser_capability(), "brw-1")
                .is_ok()
        );
        let revoked = broker.revoke_for_session("brw-1");
        assert_eq!(revoked, 1);
        assert!(broker.activation_revoked("agent-1", "act-1"));
        // The same activation id can never authorize again (durable).
        let error = broker
            .enforce_mutation("act-1", &browser_capability(), "brw-1")
            .expect_err("revoked activation");
        assert_eq!(error, BrokerSurfaceError::UnknownActivation);
    }

    #[test]
    fn revoke_for_session_is_idempotent() {
        let broker = BrokerState::default();
        broker
            .grant_from_negotiation(
                "agent-1",
                test_negotiation("act-1", "brw-1", vec![browser_capability()]),
            )
            .expect("grant");
        assert_eq!(broker.revoke_for_session("brw-1"), 1);
        assert_eq!(broker.revoke_for_session("brw-1"), 0);
        assert_eq!(broker.revoke_for_session("brw-unknown"), 0);
    }
}

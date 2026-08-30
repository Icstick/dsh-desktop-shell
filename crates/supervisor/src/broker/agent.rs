//! Agent authorization bridge: negotiation -> grant/lease -> dispatch
//! (ADR-0018 decision 7, AC-TERM-001 / AC-BRW-002).
//!
//! The agent_automation path (terminal agent mode, browser interact /
//! take_over) follows the same authorization chain as every other
//! capability consumer: an IF-NEGOTIATION result (adapter-dsh-std
//! `negotiate`) is mapped into broker grants and leases here, and every
//! subsequent mutation goes through the existing ADR-0014 dispatch gate
//! with the **agent id as grant owner** - no gate change is needed.
//!
//! This module is the *bridge*: it keeps the P0 broker DSH-neutral
//! (ADR-0014: `dsh-supervisor` depends only on serde/serde_json) by
//! defining its own input view [`AgentNegotiationResult`]. The surface
//! layer (tauri commands) maps adapter-dsh-std types onto that view:
//! `Activation`/agreement -> activation_id + granted,
//! `ConformanceState` -> [`AgentConformanceState`],
//! `LeaseConstraints` -> [`AgentLeaseConstraints`].
//!
//! Semantics (ADR-0018 decision 1 - activation ownership):
//! - Every activation is independent: a new activation of an agent is
//!   issued at a fresh broker generation and **supersedes** the previous
//!   one via the existing generation-change mechanism (old grant replaced,
//!   old leases revoked with `generation_change`). Nothing from a
//!   previous Agreement is reused as a fact for the new generation.
//! - A replay of the *current* activation is an idempotent retry; any
//!   other activation id is a new activation or a stale/revoked one and
//!   is refused (fail-closed).
//! - Human takeover (AC-BRW-002): [`Broker::revoke_agent_grants`] revokes
//!   every lease of an activation with `human_takeover` and durably
//!   marks the activation revoked so the same result can never be
//!   re-issued.
//! - Fail-closed: no agreement, conformance not `Known`, nothing
//!   granted or no bounded lease policy never creates broker state.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{
    Broker, BrokerError, CapabilityGrant, CapabilityId, Clock, Lease, LeaseRevocation,
    LeaseRevocationReason, Scope,
};

/// dsh-std conformance tri-state view (ADR-0018 decision 2).
///
/// Mirrors `dsh_adapter_dsh_std::conformance::ConformanceState` so the
/// P0 broker stays dependency-free; the surface layer maps the adapter
/// state onto this enum. Only `Known` opens the L2 grant path; `Absent`
/// and `Unknown` fail closed (unknown = coordinate drift, no L2 promise).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentConformanceState {
    Known,
    Absent,
    Unknown,
}

impl AgentConformanceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Known => "known",
            Self::Absent => "absent",
            Self::Unknown => "unknown",
        }
    }

    /// True only for `Known` (L2 capability).
    pub fn is_l2(self) -> bool {
        self == Self::Known
    }
}

/// Lease constraints of one agent activation
/// (agreementPayload.leaseConstraints, envelope.schema.json).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLeaseConstraints {
    /// Maximum lease duration in seconds (>= 1). The bridge derives
    /// `expires_at = now + max_seconds`; 0/missing fails closed.
    pub max_seconds: u64,
    /// Surface policy flag (e.g. require a user gesture before a
    /// mutation). Carried for observability; enforcement belongs to the
    /// surface layer (USER_GESTURE_REQUIRED), not the broker.
    pub approval_required: Option<bool>,
}

impl AgentLeaseConstraints {
    pub fn new(max_seconds: u64) -> Self {
        Self {
            max_seconds,
            approval_required: None,
        }
    }
}

/// Bridge input: the negotiated facts of ONE activation (ADR-0018
/// decision 1 - activation ownership).
///
/// Field provenance:
/// - `activation_id`, `agreed`, `granted`, `lease_constraints`
///   come from the adapter-dsh-std negotiation result (`Activation` /
///   `AgreementPayload`); granted `ProtocolCoordinate`s map field-wise
///   to [`CapabilityId`] (api_version + kind).
/// - `conformance` maps from `ConformanceState` (only Known grants).
/// - `scope` is surface policy (session/workspace context of the agent
///   request); the negotiation itself carries no scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentNegotiationResult {
    pub activation_id: String,
    pub agreed: bool,
    pub granted: Vec<CapabilityId>,
    pub conformance: AgentConformanceState,
    pub lease_constraints: Option<AgentLeaseConstraints>,
    pub scope: Scope,
}

/// One granted agent activation: the summary record returned by
/// [`Broker::broker_grant_from_negotiation`].
///
/// `generation` is the broker-issued generation for this activation;
/// surface mutations must dispatch with `owner = agent_id` and exactly
/// this generation and scope, which the existing ADR-0014 gate enforces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGrant {
    pub agent_id: String,
    pub activation_id: String,
    pub generation: u64,
    pub capabilities: Vec<CapabilityId>,
    pub scope: Scope,
    pub lease_constraints: Option<AgentLeaseConstraints>,
    pub expires_at_unix_ms: u64,
    pub granted_at_unix_ms: u64,
}

/// Bridge errors. The ADR-0014 `BrokerError` set stays frozen; this is
/// the extension surface for the agent path. Messages are static,
/// machine-readable strings and carry no secrets, paths, commands or user
/// data (DEVELOPMENT.md error contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentBridgeError {
    /// The negotiation did not reach an agreement (rejected/no Hello).
    NotNegotiated,
    /// The agreement granted zero capabilities.
    NothingGranted,
    /// No bounded lease policy (maxSeconds) - a lease cannot be derived.
    NoLeasePolicy,
    /// dsh-std conformance is not `Known` (absent/unknown): L2
    /// fail-closed, nothing is granted.
    ConformanceNotKnown,
    /// The activation was revoked by human takeover; its result can never
    /// be re-issued.
    ActivationRevoked,
    /// Replay of an activation that is no longer the agent current one.
    StaleActivation,
    /// Underlying broker rejection (delegated).
    Broker(BrokerError),
}

impl AgentBridgeError {
    /// Protocol error code per docs/protocol/ERROR_MODEL.md.
    pub fn protocol_code(&self) -> &'static str {
        match self {
            Self::Broker(error) => error.protocol_code(),
            Self::StaleActivation => "STALE_GENERATION",
            Self::ConformanceNotKnown => "UNAVAILABLE",
            Self::NotNegotiated
            | Self::NothingGranted
            | Self::NoLeasePolicy
            | Self::ActivationRevoked => "UNAUTHORIZED",
        }
    }

    /// Whether the caller may retry the same request as-is.
    pub fn retryable(&self) -> bool {
        match self {
            Self::Broker(error) => error.retryable(),
            Self::ConformanceNotKnown => true,
            Self::NotNegotiated
            | Self::NothingGranted
            | Self::NoLeasePolicy
            | Self::ActivationRevoked
            | Self::StaleActivation => false,
        }
    }
}

impl fmt::Display for AgentBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotNegotiated => "negotiation did not reach agreement",
            Self::NothingGranted => "agreement granted no capabilities",
            Self::NoLeasePolicy => "lease constraints missing max seconds",
            Self::ConformanceNotKnown => "dsh-std conformance is not known",
            Self::ActivationRevoked => "activation revoked by human takeover",
            Self::StaleActivation => "activation is not current",
            Self::Broker(error) => {
                return write!(f, "{} ({})", error, error.protocol_code());
            }
        };
        write!(f, "{} ({})", message, self.protocol_code())
    }
}

impl std::error::Error for AgentBridgeError {}

/// Per-(agent, activation) register state (internal; visible to the
/// broker module which owns the register maps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AgentActivationState {
    generation: u64,
    revoked: bool,
}

/// Contract version of a grant, derived from the negotiated coordinate:
/// the apiVersion suffix after the last `/` (e.g.
/// `interop.dsh-desktop.local/v1alpha1` -> `v1alpha1`); the full
/// apiVersion is used when no separator exists.
fn contract_version(api_version: &str) -> String {
    api_version
        .rsplit('/')
        .next()
        .unwrap_or(api_version)
        .to_owned()
}

/// Deterministic lease id for an agent activation, so re-issuing the same
/// activation is an idempotent no-op (DEVELOPMENT.md idempotency).
fn agent_lease_id(agent_id: &str, activation_id: &str, capability: &CapabilityId) -> String {
    format!(
        "agent-{agent_id}-{activation_id}-{}-{}",
        capability.api_version, capability.kind
    )
}

impl<C: Clock> Broker<C> {
    /// Maps one negotiated activation into broker grants + leases
    /// (ADR-0018 decision 7 chain: negotiation -> grant -> lease).
    ///
    /// Fail-closed checks, in order: agreement reached, conformance
    /// `Known`, non-empty granted list, bounded lease policy. None of
    /// these create broker state.
    ///
    /// Activation ownership (ADR-0018 decision 1):
    /// - first activation of an agent -> generation 1;
    /// - a *new* activation id -> next generation; it supersedes the
    ///   previous activation (old grants replaced, old leases revoked
    ///   with `generation_change` by the existing grant mechanism);
    /// - replaying the *current* activation id -> idempotent re-issue of
    ///   the same grants/leases (same generation);
    /// - replaying a revoked or superseded activation id -> refused.
    pub fn broker_grant_from_negotiation(
        &mut self,
        agent_id: &str,
        result: AgentNegotiationResult,
    ) -> Result<AgentGrant, AgentBridgeError> {
        let AgentNegotiationResult {
            activation_id,
            agreed,
            granted,
            conformance,
            lease_constraints,
            scope,
        } = result;

        if !agreed {
            return Err(AgentBridgeError::NotNegotiated);
        }
        if conformance != AgentConformanceState::Known {
            return Err(AgentBridgeError::ConformanceNotKnown);
        }
        if granted.is_empty() {
            return Err(AgentBridgeError::NothingGranted);
        }
        let constraints = lease_constraints.ok_or(AgentBridgeError::NoLeasePolicy)?;
        if constraints.max_seconds == 0 {
            return Err(AgentBridgeError::NoLeasePolicy);
        }

        let now = self.now_unix_ms();
        let agent_key = agent_id.to_owned();
        let states = self.agent_activations.entry(agent_key.clone()).or_default();
        let current = self.current_activation.get(&agent_key).cloned();
        let generation = match states.get(&activation_id).copied() {
            Some(state) if state.revoked => return Err(AgentBridgeError::ActivationRevoked),
            Some(state) if current.as_deref() == Some(activation_id.as_str()) => state.generation,
            Some(_) => return Err(AgentBridgeError::StaleActivation),
            None => {
                let counter = self.next_generation.entry(agent_key.clone()).or_insert(0);
                *counter += 1;
                states.insert(
                    activation_id.clone(),
                    AgentActivationState {
                        generation: *counter,
                        revoked: false,
                    },
                );
                self.current_activation
                    .insert(agent_key.clone(), activation_id.clone());
                *counter
            }
        };

        // Grants (owner = agent id; existing grant mechanism, ADR-0014 d2).
        for capability in &granted {
            let grant = CapabilityGrant {
                capability_id: capability.clone(),
                version: contract_version(&capability.api_version),
                scope: scope.clone(),
                owner: agent_key.clone(),
                generation,
                created_at_unix_ms: now,
            };
            self.grant(grant).map_err(AgentBridgeError::Broker)?;
        }

        // Leases (deterministic ids; same scope as the grant, bounded by
        // the negotiated maxSeconds).
        let expires_at = now.saturating_add(constraints.max_seconds.saturating_mul(1000));
        for capability in &granted {
            let lease = Lease {
                id: agent_lease_id(&agent_key, &activation_id, capability),
                capability_id: capability.clone(),
                owner: agent_key.clone(),
                generation,
                scope: scope.clone(),
                expires_at_unix_ms: expires_at,
                revoked: None,
            };
            self.lease(lease).map_err(AgentBridgeError::Broker)?;
        }

        Ok(AgentGrant {
            agent_id: agent_key,
            activation_id,
            generation,
            capabilities: granted,
            scope,
            lease_constraints: Some(constraints),
            expires_at_unix_ms: expires_at,
            granted_at_unix_ms: now,
        })
    }

    /// Human takeover (AC-BRW-002): revokes every lease of an activation
    /// with `human_takeover` and durably marks the activation revoked,
    /// so replaying the same negotiation result is refused forever.
    ///
    /// Idempotent: revoking again returns 0. Unknown activation ids are a
    /// no-op. Grants are left in place - without a valid lease the
    /// dispatch gate still rejects (fail-closed), and a *new* activation
    /// requires a fresh negotiation anyway (ADR-0018 decision 1).
    ///
    /// Returns the number of leases revoked (observability).
    pub fn revoke_agent_grants(&mut self, activation_id: &str) -> usize {
        let at_unix_ms = self.now_unix_ms();
        let targets: Vec<(String, u64)> = self
            .agent_activations
            .iter()
            .filter_map(|(agent, states)| {
                states
                    .get(activation_id)
                    .map(|state| (agent.clone(), state.generation))
            })
            .collect();

        let mut revoked = 0;
        for (agent, generation) in &targets {
            for lease in self.leases.values_mut() {
                if lease.owner == *agent
                    && lease.generation == *generation
                    && lease.revoked.is_none()
                {
                    lease.revoked = Some(LeaseRevocation {
                        reason: LeaseRevocationReason::HumanTakeover,
                        at_unix_ms,
                    });
                    revoked += 1;
                }
            }
            if let Some(states) = self.agent_activations.get_mut(agent)
                && let Some(state) = states.get_mut(activation_id)
            {
                state.revoked = true;
            }
            if self.current_activation.get(agent).map(String::as_str) == Some(activation_id) {
                self.current_activation.remove(agent);
            }
        }
        revoked
    }

    // ------------------------------------------------------------------
    // Agent bridge observability
    // ------------------------------------------------------------------

    /// Broker generation issued for an (agent, activation) pair.
    pub fn agent_generation(&self, agent_id: &str, activation_id: &str) -> Option<u64> {
        self.agent_activations
            .get(agent_id)?
            .get(activation_id)
            .map(|state| state.generation)
    }

    /// Whether the activation was revoked by human takeover.
    pub fn agent_activation_revoked(&self, agent_id: &str, activation_id: &str) -> bool {
        self.agent_activations
            .get(agent_id)
            .and_then(|states| states.get(activation_id))
            .is_some_and(|state| state.revoked)
    }

    /// Total registered (agent, activation) pairs (observability).
    pub fn agent_activation_count(&self) -> usize {
        self.agent_activations.values().map(HashMap::len).sum()
    }
}

#[cfg(test)]
mod tests;

//! P0 Capability Broker: grant/lease/scope/generation enforcement and
//! provider dispatch (ADR-0014, AC-LEASE-001).
//!
//! An invocation may reach a provider only when, at dispatch time, ALL of the
//! following hold, validated in this fixed order:
//!
//! 1. the capability is granted (a grant must originate from an
//!    IF-NEGOTIATION Agreement and is the Desktop-level explicit grant);
//! 2. the owner matches the grant owner;
//! 3. the generation matches the grant generation;
//! 4. the requested scope is covered by the grant scope;
//! 5. a valid lease exists: matching capability/owner/generation, scope
//!    covered, not expired, not revoked.
//!
//! The agent_automation path (ADR-0018 decision 7) funnels into the same
//! gate: the agent authorization bridge (broker/agent.rs) maps negotiation
//! results into grants/leases whose owner is the agent id, and every
//! mutation dispatches through this exact gate.
//!
//! Errors are static, machine-readable strings: no secrets, paths, commands
//! or user data cross the boundary (DEVELOPMENT.md error contract).

use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Abstract time source so lease expiry is testable without sleeping.
pub trait Clock {
    /// Current unix time in milliseconds.
    fn now_unix_ms(&self) -> u64;
}

/// Wall-clock implementation used in production.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Identity of a capability contract: independent `apiVersion` + `kind`
/// (ADR-0005), mirroring `protocol-coordinate.schema.json`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityId {
    pub api_version: String,
    pub kind: String,
}

impl CapabilityId {
    pub fn new(api_version: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            api_version: api_version.into(),
            kind: kind.into(),
        }
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.api_version, self.kind)
    }
}

/// Dispatch scope, mirroring the `scope` object of
/// `capability-lease.schema.json` (sessionId/workspace/domains/resources).
///
/// The broker enforces coverage semantics; wire-level shape validation
/// (minProperties >= 1 etc.) belongs to the contract layer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub session_id: Option<String>,
    pub workspace: Option<String>,
    pub domains: Vec<String>,
    pub resources: Vec<String>,
}

impl Scope {
    /// Returns `true` when `self` (the grant/lease scope) covers `requested`.
    ///
    /// Rules per dimension, fail-closed:
    /// - `session_id` / `workspace`: when `self` pins a value, `requested`
    ///   must carry the exact same value; an unset `self` dimension imposes
    ///   no constraint.
    /// - `domains` / `resources`: when `self` lists entries, every entry of
    ///   `requested` must be contained in `self`; an empty `self` list
    ///   imposes no constraint.
    pub fn covers(&self, requested: &Scope) -> bool {
        if let Some(session) = &self.session_id
            && requested.session_id.as_ref() != Some(session)
        {
            return false;
        }
        if let Some(workspace) = &self.workspace
            && requested.workspace.as_ref() != Some(workspace)
        {
            return false;
        }
        if !self.domains.is_empty()
            && !requested
                .domains
                .iter()
                .all(|domain| self.domains.contains(domain))
        {
            return false;
        }
        if !self.resources.is_empty()
            && !requested
                .resources
                .iter()
                .all(|resource| self.resources.contains(resource))
        {
            return false;
        }
        true
    }
}

/// Reason a lease was revoked (AC-LEASE-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseRevocationReason {
    /// The owning participant disconnected.
    Disconnect,
    /// The owning capability/participant was unloaded.
    Unload,
    /// The lease reached its expiry.
    Expiry,
    /// A human took over the session/resource.
    HumanTakeover,
    /// A new grant generation superseded the lease.
    GenerationChange,
}

/// Durable revocation record. The first record wins and is never overwritten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRevocation {
    pub reason: LeaseRevocationReason,
    pub at_unix_ms: u64,
}

/// Desktop-level grant for one capability (ADR-0014 decision 2).
///
/// `version` is the negotiated capability contract version from the
/// IF-NEGOTIATION Agreement (e.g. `v1alpha1`); identity and enforcement use
/// the full [`CapabilityId`] coordinate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub capability_id: CapabilityId,
    pub version: String,
    pub scope: Scope,
    pub owner: String,
    pub generation: u64,
    pub created_at_unix_ms: u64,
}

/// An authorization lease bound to a capability/owner/generation/scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub id: String,
    pub capability_id: CapabilityId,
    pub owner: String,
    pub generation: u64,
    pub scope: Scope,
    pub expires_at_unix_ms: u64,
    pub revoked: Option<LeaseRevocation>,
}

impl Lease {
    pub fn is_revoked(&self) -> bool {
        self.revoked.is_some()
    }

    pub fn is_expired(&self, now_unix_ms: u64) -> bool {
        self.expires_at_unix_ms <= now_unix_ms
    }
}

/// A provider dispatch request (DSH-neutral skeleton; wire framing belongs
/// to the local-transport layer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invocation {
    pub capability: CapabilityId,
    pub method: String,
    pub owner: String,
    pub generation: u64,
    pub scope: Scope,
    pub payload: serde_json::Value,
}

/// Result produced by a provider handler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvocationResult {
    pub payload: serde_json::Value,
}

/// Registered provider: one capability per provider (ADR-0014 decision 7).
///
/// The handler is Send + Sync: the broker is shared across threads (tauri
/// managed state, background drain tasks), so the whole broker must be
/// thread-safe once the clock is.
pub struct Provider {
    pub id: String,
    pub capability: CapabilityId,
    handler: Box<dyn Fn(&Invocation) -> InvocationResult + Send + Sync>,
}

impl Provider {
    pub fn new(
        id: impl Into<String>,
        capability: CapabilityId,
        handler: impl Fn(&Invocation) -> InvocationResult + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            capability,
            handler: Box::new(handler),
        }
    }
}

/// Broker enforcement errors (ADR-0014 decision 6). Messages are static and
/// carry no secrets, paths, commands or user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    /// The capability is not granted.
    UnknownCapability,
    /// The provider is not registered.
    UnknownProvider,
    /// No grant/lease for the requesting owner.
    NotGranted,
    /// The lease expired.
    LeaseExpired,
    /// The lease was revoked.
    LeaseRevoked,
    /// The requested scope is not covered.
    ScopeMismatch,
    /// The request carries a stale generation.
    GenerationMismatch,
    /// Idempotent retry is impossible: state conflict.
    Conflict,
}

impl BrokerError {
    /// Protocol error code per docs/protocol/ERROR_MODEL.md.
    pub fn protocol_code(&self) -> &'static str {
        match self {
            BrokerError::UnknownCapability | BrokerError::UnknownProvider => "UNAVAILABLE",
            BrokerError::NotGranted
            | BrokerError::LeaseExpired
            | BrokerError::LeaseRevoked
            | BrokerError::ScopeMismatch => "UNAUTHORIZED",
            BrokerError::GenerationMismatch => "STALE_GENERATION",
            BrokerError::Conflict => "CONFLICT",
        }
    }

    /// Whether the caller may retry the same request as-is.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            BrokerError::UnknownCapability | BrokerError::UnknownProvider
        )
    }
}

impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            BrokerError::UnknownCapability => "capability is not granted",
            BrokerError::UnknownProvider => "provider is not registered",
            BrokerError::NotGranted => "not granted for this owner",
            BrokerError::LeaseExpired => "lease expired",
            BrokerError::LeaseRevoked => "lease revoked",
            BrokerError::ScopeMismatch => "scope not covered",
            BrokerError::GenerationMismatch => "stale generation",
            BrokerError::Conflict => "state conflict",
        };
        write!(f, "{} ({})", message, self.protocol_code())
    }
}

impl std::error::Error for BrokerError {}

/// The P0 Capability Broker (ADR-0014).
///
/// State is observable through the `grant_for`/`get_lease`/`leases_for`/
/// `provider` accessors; mutation operations are idempotent or return
/// [`BrokerError::Conflict`] (DEVELOPMENT.md state contract).
pub struct Broker<C = SystemClock> {
    grants: HashMap<CapabilityId, CapabilityGrant>,
    leases: HashMap<String, Lease>,
    providers: HashMap<String, Provider>,
    /// Agent authorization bridge register (broker/agent.rs,
    /// ADR-0018 decision 7). Append-only per (agent, activation): a
    /// revoked activation can never be re-issued, a superseded one can
    /// never be replayed.
    agent_activations: HashMap<String, HashMap<String, agent::AgentActivationState>>,
    /// The agent's current activation id (replay is only allowed for it).
    current_activation: HashMap<String, String>,
    /// Per-agent monotonic generation counter for new activations.
    next_generation: HashMap<String, u64>,
    clock: C,
}

impl Default for Broker<SystemClock> {
    fn default() -> Self {
        Self::new()
    }
}

impl Broker<SystemClock> {
    pub fn new() -> Self {
        Self::with_clock(SystemClock)
    }
}

impl<C: Clock> Broker<C> {
    pub fn with_clock(clock: C) -> Self {
        Self {
            grants: HashMap::new(),
            leases: HashMap::new(),
            providers: HashMap::new(),
            agent_activations: HashMap::new(),
            current_activation: HashMap::new(),
            next_generation: HashMap::new(),
            clock,
        }
    }

    /// Current time from the injected clock, in unix milliseconds.
    pub fn now_unix_ms(&self) -> u64 {
        self.clock.now_unix_ms()
    }

    // ------------------------------------------------------------------
    // Registration
    // ------------------------------------------------------------------

    /// Registers a provider. Duplicate ids conflict.
    pub fn register_provider(&mut self, provider: Provider) -> Result<(), BrokerError> {
        if self.providers.contains_key(&provider.id) {
            return Err(BrokerError::Conflict);
        }
        self.providers.insert(provider.id.clone(), provider);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Grants and leases
    // ------------------------------------------------------------------

    /// Grants a capability (ADR-0014 decision 5).
    ///
    /// - An identical re-grant is an idempotent no-op (`Ok`).
    /// - A re-grant of the same capability with a different generation
    ///   supersedes the old grant and revokes every outstanding lease of the
    ///   capability with [`LeaseRevocationReason::GenerationChange`].
    /// - Any other re-grant of the same capability conflicts.
    pub fn grant(&mut self, grant: CapabilityGrant) -> Result<(), BrokerError> {
        if let Some(existing) = self.grants.get(&grant.capability_id) {
            if existing == &grant {
                return Ok(());
            }
            if existing.generation != grant.generation {
                self.revoke_all_for_capability(
                    &grant.capability_id,
                    LeaseRevocationReason::GenerationChange,
                );
                self.grants.insert(grant.capability_id.clone(), grant);
                return Ok(());
            }
            return Err(BrokerError::Conflict);
        }
        self.grants.insert(grant.capability_id.clone(), grant);
        Ok(())
    }

    /// Issues a lease for a granted capability (ADR-0014 decision 2/5).
    ///
    /// The lease must be consistent with the grant (owner, generation, scope
    /// coverage) and must not already be expired. An identical re-issue is an
    /// idempotent no-op; re-using an existing lease id otherwise conflicts
    /// (a revoked lease id is never reusable).
    pub fn lease(&mut self, lease: Lease) -> Result<(), BrokerError> {
        let grant = self
            .grants
            .get(&lease.capability_id)
            .ok_or(BrokerError::UnknownCapability)?;
        if grant.owner != lease.owner {
            return Err(BrokerError::NotGranted);
        }
        if grant.generation != lease.generation {
            return Err(BrokerError::GenerationMismatch);
        }
        if !grant.scope.covers(&lease.scope) {
            return Err(BrokerError::ScopeMismatch);
        }
        if lease.expires_at_unix_ms <= self.now_unix_ms() {
            return Err(BrokerError::LeaseExpired);
        }
        if let Some(existing) = self.leases.get(&lease.id) {
            if existing == &lease {
                return Ok(());
            }
            return Err(BrokerError::Conflict);
        }
        self.leases.insert(lease.id.clone(), lease);
        Ok(())
    }

    /// Revokes a lease (AC-LEASE-001), recording the reason and timestamp.
    ///
    /// Idempotent: revoking an unknown or already-revoked lease id is a
    /// no-op, and the first revocation record is never overwritten.
    pub fn revoke(
        &mut self,
        lease_id: &str,
        reason: LeaseRevocationReason,
    ) -> Result<(), BrokerError> {
        let at_unix_ms = self.now_unix_ms();
        if let Some(lease) = self.leases.get_mut(lease_id)
            && lease.revoked.is_none()
        {
            lease.revoked = Some(LeaseRevocation { reason, at_unix_ms });
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Enforcement and dispatch
    // ------------------------------------------------------------------

    /// The dispatch gate (ADR-0014 decision 1), validated in fixed order:
    /// capability granted, owner match, generation match, grant scope
    /// coverage, valid lease (covering scope, not expired, not revoked).
    // Intentional defense-in-depth (ADR-0014): lease validity is checked
    // again at dispatch time even though lease() validated the same inputs.
    // This double-check is the security boundary; do not simplify it away.
    pub fn enforce_dispatch(
        &self,
        capability: &CapabilityId,
        owner: &str,
        generation: u64,
        scope: &Scope,
    ) -> Result<(), BrokerError> {
        let grant = self
            .grants
            .get(capability)
            .ok_or(BrokerError::UnknownCapability)?;
        if grant.owner != owner {
            return Err(BrokerError::NotGranted);
        }
        if grant.generation != generation {
            return Err(BrokerError::GenerationMismatch);
        }
        if !grant.scope.covers(scope) {
            return Err(BrokerError::ScopeMismatch);
        }
        self.enforce_valid_lease(capability, owner, generation, scope)
    }

    /// Dispatches an invocation to a provider after enforcement
    /// (ADR-0014 decision 7). Unknown provider or capability mismatch is
    /// rejected before any enforcement state is consulted.
    pub fn dispatch(
        &self,
        provider_id: &str,
        invocation: &Invocation,
    ) -> Result<InvocationResult, BrokerError> {
        let provider = self
            .providers
            .get(provider_id)
            .ok_or(BrokerError::UnknownProvider)?;
        if provider.capability != invocation.capability {
            return Err(BrokerError::UnknownCapability);
        }
        self.enforce_dispatch(
            &invocation.capability,
            &invocation.owner,
            invocation.generation,
            &invocation.scope,
        )?;
        Ok((provider.handler)(invocation))
    }

    // ------------------------------------------------------------------
    // Observability
    // ------------------------------------------------------------------

    pub fn grant_for(&self, capability: &CapabilityId) -> Option<&CapabilityGrant> {
        self.grants.get(capability)
    }

    pub fn get_lease(&self, lease_id: &str) -> Option<&Lease> {
        self.leases.get(lease_id)
    }

    /// All leases of a capability (any owner/generation), for observability.
    pub fn leases_for(&self, capability: &CapabilityId) -> Vec<&Lease> {
        self.leases
            .values()
            .filter(|lease| &lease.capability_id == capability)
            .collect()
    }

    pub fn provider(&self, provider_id: &str) -> Option<&Provider> {
        self.providers.get(provider_id)
    }

    pub fn grant_count(&self) -> usize {
        self.grants.len()
    }

    pub fn lease_count(&self) -> usize {
        self.leases.len()
    }

    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    fn enforce_valid_lease(
        &self,
        capability: &CapabilityId,
        owner: &str,
        generation: u64,
        scope: &Scope,
    ) -> Result<(), BrokerError> {
        let now = self.now_unix_ms();
        let mut matched = false; // any lease for (capability, owner, generation)
        let mut covering = false; // matched and scope-covered
        let mut revoked = false; // covering and revoked
        let mut expired = false; // covering and expired
        for lease in self.leases.values() {
            if &lease.capability_id != capability
                || lease.owner != owner
                || lease.generation != generation
            {
                continue;
            }
            matched = true;
            if !lease.scope.covers(scope) {
                continue;
            }
            covering = true;
            if lease.is_revoked() {
                revoked = true;
                continue;
            }
            if lease.is_expired(now) {
                expired = true;
                continue;
            }
            return Ok(());
        }
        if revoked {
            Err(BrokerError::LeaseRevoked)
        } else if expired {
            Err(BrokerError::LeaseExpired)
        } else if covering {
            // Unreachable: a valid covering lease returns above. Kept as a
            // defensive guard so the match arms stay exhaustive.
            Err(BrokerError::ScopeMismatch)
        } else if matched {
            Err(BrokerError::ScopeMismatch)
        } else {
            Err(BrokerError::NotGranted)
        }
    }

    fn revoke_all_for_capability(
        &mut self,
        capability: &CapabilityId,
        reason: LeaseRevocationReason,
    ) {
        let at_unix_ms = self.now_unix_ms();
        for lease in self.leases.values_mut() {
            if &lease.capability_id == capability && lease.revoked.is_none() {
                lease.revoked = Some(LeaseRevocation { reason, at_unix_ms });
            }
        }
    }
}

pub mod agent;

#[cfg(test)]
mod tests;

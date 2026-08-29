//! dsh-supervisor: P0 supervisor control-plane core (MOD-SUPERVISOR, M2).
//!
//! This crate is DSH-neutral: it has no tauri, DSH, or platform dependency,
//! and no DSH-specific type crosses its boundary (ADR-0005/0008, ADR-0014).
//! It owns the P0 Capability Broker: grant/lease/scope/generation enforcement
//! and provider dispatch (AC-LEASE-001, TM-PLG-001).

pub mod broker;

pub use broker::{
    Broker, BrokerError, CapabilityGrant, CapabilityId, Clock, Invocation, InvocationResult, Lease,
    LeaseRevocation, LeaseRevocationReason, Provider, Scope, SystemClock,
};

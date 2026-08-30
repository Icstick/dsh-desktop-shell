//! Degradation path (ADR-0018 decision 4: additive compatibility).
//!
//! L2/L1 adapter failures must be able to fall back to the L0 baseline
//! (DSH process + HTTP Web UI: Surface/health/Managed lifecycle). This
//! module records the failure and returns L0 semantics; it never panics and
//! never blocks L0 behavior.

use crate::time;

/// Why an adapter path degraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradeReason {
    /// Declared coordinates drifted or the conformance fixture check failed:
    /// fail-closed, no L2 promise.
    ConformanceUnknown,
    /// dsh-std domain protocol not implemented (normal absent state; the
    /// record keeps the audit trail, behavior stays L0/L1).
    ConformanceAbsent,
    /// Negotiation was rejected or could not reach Agreement.
    NegotiationRejected,
    /// A known adapter failed during invocation.
    InvocationFailed,
}

impl DegradeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConformanceUnknown => "conformance_unknown",
            Self::ConformanceAbsent => "conformance_absent",
            Self::NegotiationRejected => "negotiation_rejected",
            Self::InvocationFailed => "invocation_failed",
        }
    }
}

/// L0 baseline semantics: DSH process + HTTP Web UI (Surface/health/Managed
/// lifecycle). The adapter claims no L2 capability through a fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L0Fallback {
    pub reason: DegradeReason,
    pub detail: String,
    pub at: String,
}

impl L0Fallback {
    /// L0/L1 remain available; this fallback never claims L2.
    pub fn claims_l2(&self) -> bool {
        false
    }

    /// Baseline semantics are intact by construction (additive compatibility).
    pub fn baseline_intact(&self) -> bool {
        true
    }
}

/// One recorded degradation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DegradeEntry {
    pub at: String,
    pub reason: DegradeReason,
    pub detail: String,
    /// Always true: L0 baseline is preserved by construction.
    pub l0_preserved: bool,
}

/// Append-only degradation log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DegradeLog {
    entries: Vec<DegradeEntry>,
}

impl DegradeLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[DegradeEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn last(&self) -> Option<&DegradeEntry> {
        self.entries.last()
    }
}

/// Record an adapter failure and return L0 semantics. Total: never panics.
pub fn degrade_to_l0(reason: DegradeReason, detail: &str, log: &mut DegradeLog) -> L0Fallback {
    let at = time::now_rfc3339();
    let fallback = L0Fallback {
        reason,
        detail: detail.to_owned(),
        at: at.clone(),
    };
    log.entries.push(DegradeEntry {
        at,
        reason,
        detail: fallback.detail.clone(),
        l0_preserved: true,
    });
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_adapter_failure_degrades_to_l0_without_panic() {
        let mut log = DegradeLog::new();
        let fallback = degrade_to_l0(
            DegradeReason::InvocationFailed,
            "std invocation of list_browsers failed after conformance was known",
            &mut log,
        );
        assert!(!fallback.claims_l2());
        assert!(fallback.baseline_intact());
        assert_eq!(fallback.reason, DegradeReason::InvocationFailed);
        assert_eq!(log.len(), 1);
        assert!(log.last().unwrap().l0_preserved);
    }

    #[test]
    fn unknown_conformance_fails_closed_and_records() {
        let mut log = DegradeLog::new();
        let fallback = degrade_to_l0(
            DegradeReason::ConformanceUnknown,
            "declared core version drifted; not adopting L2",
            &mut log,
        );
        assert!(!fallback.claims_l2());
        assert_eq!(log.len(), 1);
        assert_eq!(
            log.last().unwrap().reason,
            DegradeReason::ConformanceUnknown
        );
    }

    #[test]
    fn absent_conformance_is_a_normal_path_not_an_error() {
        let mut log = DegradeLog::new();
        let fallback = degrade_to_l0(
            DegradeReason::ConformanceAbsent,
            "no dsh-std declaration; staying on L0/L1",
            &mut log,
        );
        assert!(fallback.baseline_intact());
        assert!(!fallback.claims_l2());
        // recorded for the audit trail, but it is not a failure state
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn failures_accumulate_in_order() {
        let mut log = DegradeLog::new();
        degrade_to_l0(
            DegradeReason::NegotiationRejected,
            "peer rejected hello",
            &mut log,
        );
        degrade_to_l0(DegradeReason::InvocationFailed, "second failure", &mut log);
        assert_eq!(log.len(), 2);
        assert_eq!(log.entries()[0].reason, DegradeReason::NegotiationRejected);
        assert_eq!(log.entries()[1].reason, DegradeReason::InvocationFailed);
    }

    #[test]
    fn degrade_never_panics_on_odd_input() {
        let mut log = DegradeLog::new();
        let fallback = degrade_to_l0(DegradeReason::InvocationFailed, "", &mut log);
        assert!(fallback.detail.is_empty());
        assert_eq!(log.len(), 1);
    }
}

//! dsh-std conformance tri-state (ADR-0018 decision 2).
//!
//! A conformance declaration binds the exact coordinates of a known dsh-std
//! package: `package` (npm name), `version` (exact version), `commit` (Git
//! commit) and `integrity` (SRI artifact integrity). Tri-state semantics:
//!
//! - `Known` - declaration matches the pinned baseline (EXTERNAL_BASELINE:
//!   dsh-std main@3df0543, @dsh-std/core@0.1.1-rc.1, SRI from
//!   SOURCE_REGISTER SRC-DSH-STD) and passes local fixture
//!   validation, so L2 capability.
//! - `Absent` - no declaration: the dsh-std domain protocol is not
//!   implemented, so L0/L1 behavior unchanged.
//! - `Unknown` - coordinate drift or fixture failure: fail-closed and
//!   recorded; the adapter must not auto-promise L2.
//!
//! Every evaluation is appended to a `ConformanceLog`; `conforms` is total
//! (it never panics, never blocks L0/L1).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::time;

/// Pinned dsh-std baseline package (docs/research/EXTERNAL_BASELINE.md, 2026-08-30).
pub const KNOWN_PACKAGE: &str = "@dsh-std/core";
/// @dsh-std/core dist-tag `rc`; core unchanged since 2026-08-23.
pub const KNOWN_VERSION: &str = "0.1.1-rc.1";
/// dsh-std `main` HEAD short commit (2026-08-29: fix(connection) x2 + merge).
pub const KNOWN_COMMIT: &str = "3df0543";
/// SRI integrity of @dsh-std/core@0.1.1-rc.1
/// (docs/compliance/SOURCE_REGISTER.yaml, SRC-DSH-STD.distribution.rc).
pub const KNOWN_INTEGRITY: &str = "sha512-I2tZEYk0V5QjlvkLvqKkAj/Fsvz0bTe0oDF2YFzL8EpmOS9KNcILrLBGSdVrt1qZnlBZP+9Zzh2byJzG0dJiOQ==";

/// One declared dsh-std package coordinate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceRecord {
    /// npm package name, e.g. `@dsh-std/core`.
    pub package: String,
    /// Exact package version (floating tags such as `latest`/`rc` are never
    /// accepted as a declaration).
    pub version: String,
    /// Git commit, short or full lowercase hex (7..=40 chars).
    pub commit: String,
    /// SRI artifact integrity, `sha512-<base64>`.
    pub integrity: String,
}

impl ConformanceRecord {
    /// The pinned known baseline record (EXTERNAL_BASELINE + SOURCE_REGISTER).
    pub fn known_baseline() -> Self {
        Self {
            package: KNOWN_PACKAGE.to_owned(),
            version: KNOWN_VERSION.to_owned(),
            commit: KNOWN_COMMIT.to_owned(),
            integrity: KNOWN_INTEGRITY.to_owned(),
        }
    }

    /// Structural validation of the declaration fields.
    pub fn validate_format(&self) -> Result<(), ConformanceError> {
        if self.package.is_empty() {
            return Err(ConformanceError::EmptyPackage);
        }
        if !is_valid_package_name(&self.package) {
            return Err(ConformanceError::InvalidPackageName);
        }
        if self.version.is_empty() {
            return Err(ConformanceError::EmptyVersion);
        }
        if !is_valid_version(&self.version) {
            return Err(ConformanceError::InvalidVersion);
        }
        if !is_valid_commit(&self.commit) {
            return Err(ConformanceError::InvalidCommit);
        }
        if !is_valid_integrity(&self.integrity) {
            return Err(ConformanceError::InvalidIntegrity);
        }
        Ok(())
    }

    /// Exact coordinate match against the pinned baseline (version + commit
    /// + integrity must all bind, per ADR-0018 decision 2).
    pub fn matches_known_baseline(&self) -> bool {
        self.package == KNOWN_PACKAGE
            && self.version == KNOWN_VERSION
            && self.commit == KNOWN_COMMIT
            && self.integrity == KNOWN_INTEGRITY
    }
}
/// Why a record failed format validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConformanceError {
    EmptyPackage,
    InvalidPackageName,
    EmptyVersion,
    InvalidVersion,
    InvalidCommit,
    InvalidIntegrity,
}

impl fmt::Display for ConformanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::EmptyPackage => "package is empty",
            Self::InvalidPackageName => "package is not a valid npm package name",
            Self::EmptyVersion => "version is empty",
            Self::InvalidVersion => "version is not a valid exact version",
            Self::InvalidCommit => "commit is not 7..=40 hex characters",
            Self::InvalidIntegrity => "integrity is not a valid SRI value (sha512-<base64>)",
        };
        f.write_str(msg)
    }
}

/// The set of dsh-std package coordinates a peer (or local config) declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConformanceDeclaration {
    pub records: Vec<ConformanceRecord>,
}

impl ConformanceDeclaration {
    pub fn new(records: Vec<ConformanceRecord>) -> Self {
        Self { records }
    }

    /// Parse a declaration from JSON (fixture-driven conformance checks).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Tri-state conformance result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConformanceState {
    /// Declared coordinates match the pinned baseline and pass fixture
    /// validation: L2 capability is available.
    Known,
    /// No declaration: dsh-std domain protocol not implemented: L0/L1
    /// behavior unchanged.
    Absent,
    /// Coordinate drift or fixture failure: fail-closed; recorded, no L2
    /// promise.
    Unknown,
}

impl ConformanceState {
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

/// One recorded evaluation (append-only audit trail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceLogEntry {
    pub at: String,
    pub state: ConformanceState,
    pub reason: String,
    /// Matched coordinate (`package@version`) when `Known`.
    pub matched: Option<String>,
}

/// Append-only conformance evaluation log. `conforms` records every
/// evaluation here; the log never fails and never panics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConformanceLog {
    entries: Vec<ConformanceLogEntry>,
}

impl ConformanceLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[ConformanceLogEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn last(&self) -> Option<&ConformanceLogEntry> {
        self.entries.last()
    }

    /// States observed so far, in order.
    pub fn states(&self) -> Vec<ConformanceState> {
        self.entries.iter().map(|e| e.state).collect()
    }

    fn record(
        &mut self,
        at: String,
        state: ConformanceState,
        reason: impl Into<String>,
        matched: Option<String>,
    ) {
        self.entries.push(ConformanceLogEntry {
            at,
            state,
            reason: reason.into(),
            matched,
        });
    }
}

/// Tri-state conformance evaluation. Total: never panics.
///
/// - `None`: `Absent` (recorded; L0/L1 unchanged).
/// - Declaration with no records: `Absent`.
/// - Any record failing format validation: `Unknown` (fail-closed).
/// - A record exactly matching the pinned baseline: `Known`.
/// - Otherwise: `Unknown` (coordinate drift).
pub fn conforms(
    declaration: Option<&ConformanceDeclaration>,
    log: &mut ConformanceLog,
) -> ConformanceState {
    let at = time::now_rfc3339();
    let Some(decl) = declaration else {
        log.record(
            at,
            ConformanceState::Absent,
            "no conformance declaration; dsh-std domain protocol not implemented (L0/L1 unchanged)",
            None,
        );
        return ConformanceState::Absent;
    };

    if decl.records.is_empty() {
        log.record(
            at,
            ConformanceState::Absent,
            "declaration present but empty; no dsh-std package coordinates declared (L0/L1 unchanged)",
            None,
        );
        return ConformanceState::Absent;
    }

    for record in &decl.records {
        if let Err(e) = record.validate_format() {
            log.record(
                at,
                ConformanceState::Unknown,
                format!(
                    "conformance fixture failed format validation for {}: {e}",
                    record.package
                ),
                None,
            );
            return ConformanceState::Unknown;
        }
    }

    for record in &decl.records {
        if record.matches_known_baseline() {
            log.record(
                at,
                ConformanceState::Known,
                "declared coordinates match the pinned baseline (EXTERNAL_BASELINE)",
                Some(format!("{}@{}", record.package, record.version)),
            );
            return ConformanceState::Known;
        }
    }

    log.record(
        at,
        ConformanceState::Unknown,
        "no declared record matches the pinned baseline (coordinate drift); fail-closed",
        None,
    );
    ConformanceState::Unknown
}
fn is_valid_package_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    if let Some(rest) = name.strip_prefix('@') {
        let Some((scope, pkg)) = rest.split_once('/') else {
            return false;
        };
        is_package_segment(scope) && is_package_segment(pkg)
    } else {
        is_package_segment(name)
    }
}

fn is_package_segment(seg: &str) -> bool {
    !seg.is_empty()
        && seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Exact-version check: `major.minor.patch[-prerelease][+build]` (npm shape).
fn is_valid_version(v: &str) -> bool {
    let (core, _build) = match v.split_once('+') {
        Some((c, _b)) => (c, ()),
        None => (v, ()),
    };
    let (core, prerelease) = match core.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core, None),
    };
    let mut parts = core.split('.');
    let (Some(major), Some(minor), Some(patch), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if !major.chars().all(|c| c.is_ascii_digit())
        || !minor.chars().all(|c| c.is_ascii_digit())
        || !patch.chars().all(|c| c.is_ascii_digit())
    {
        return false;
    }
    if let Some(p) = prerelease {
        if p.is_empty() || p.split('.').any(|s| s.is_empty()) {
            return false;
        }
        if !p
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
        {
            return false;
        }
    }
    true
}

fn is_valid_commit(c: &str) -> bool {
    (7..=40).contains(&c.len()) && c.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// SRI shape: `sha256|sha384|sha512` + base64 (npm registry emits sha512).
fn is_valid_integrity(i: &str) -> bool {
    let Some((algo, b64)) = i.split_once('-') else {
        return false;
    };
    if !matches!(algo, "sha256" | "sha384" | "sha512") {
        return false;
    }
    !b64.is_empty()
        && b64.len() % 4 == 0
        && b64
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '+' | '='))
}
#[cfg(test)]
mod tests {
    use super::*;

    fn log() -> ConformanceLog {
        ConformanceLog::new()
    }

    fn decl(records: Vec<ConformanceRecord>) -> ConformanceDeclaration {
        ConformanceDeclaration::new(records)
    }

    // ---- format validation ----

    #[test]
    fn known_baseline_passes_format_validation() {
        let r = ConformanceRecord::known_baseline();
        assert_eq!(r.validate_format(), Ok(()));
    }

    #[test]
    fn integrity_format_rejects_non_sri() {
        let r = ConformanceRecord {
            integrity: "sha512-not-base64!!".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(r.validate_format(), Err(ConformanceError::InvalidIntegrity));
    }

    #[test]
    fn integrity_format_rejects_wrong_algo() {
        let r = ConformanceRecord {
            integrity: "md5-deadbeef".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(r.validate_format(), Err(ConformanceError::InvalidIntegrity));
    }

    #[test]
    fn commit_format_rejects_non_hex() {
        let r = ConformanceRecord {
            commit: "xyz1234".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(r.validate_format(), Err(ConformanceError::InvalidCommit));
    }

    #[test]
    fn commit_format_rejects_too_short() {
        let r = ConformanceRecord {
            commit: "3df054".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(r.validate_format(), Err(ConformanceError::InvalidCommit));
    }

    #[test]
    fn version_format_rejects_floating_tag() {
        let r = ConformanceRecord {
            version: "rc".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(r.validate_format(), Err(ConformanceError::InvalidVersion));
    }

    #[test]
    fn version_format_accepts_prerelease() {
        let r = ConformanceRecord {
            version: "0.1.1-rc.1".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(r.validate_format(), Ok(()));
    }

    #[test]
    fn package_format_rejects_unscoped_garbage() {
        let r = ConformanceRecord {
            package: "not a package!".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(
            r.validate_format(),
            Err(ConformanceError::InvalidPackageName)
        );
    }

    #[test]
    fn package_format_accepts_scoped_and_unscoped() {
        for pkg in ["@dsh-std/core", "dsh-std-core", "@a/b"] {
            let r = ConformanceRecord {
                package: pkg.to_owned(),
                ..ConformanceRecord::known_baseline()
            };
            assert_eq!(r.validate_format(), Ok(()), "package {pkg}");
        }
    }

    // ---- tri-state matrix ----

    #[test]
    fn absent_no_declaration() {
        let mut l = log();
        let state = conforms(None, &mut l);
        assert_eq!(state, ConformanceState::Absent);
        assert!(!state.is_l2());
        assert_eq!(l.len(), 1);
        assert_eq!(l.last().unwrap().state, ConformanceState::Absent);
    }

    #[test]
    fn absent_empty_declaration() {
        let mut l = log();
        let state = conforms(Some(&decl(vec![])), &mut l);
        assert_eq!(state, ConformanceState::Absent);
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn known_coordinates_pass() {
        let mut l = log();
        let state = conforms(
            Some(&decl(vec![ConformanceRecord::known_baseline()])),
            &mut l,
        );
        assert_eq!(state, ConformanceState::Known);
        assert!(state.is_l2());
        assert_eq!(
            l.last().unwrap().matched.as_deref(),
            Some("@dsh-std/core@0.1.1-rc.1")
        );
    }

    #[test]
    fn full_commit_spelling_is_exact_match_only() {
        // matches_known_baseline is an exact string compare: a full-commit
        // spelling of the same revision is a different string and therefore
        // Unknown. This locks the no-prefix-leniency semantics.
        let r = ConformanceRecord {
            commit: "3df0543".to_owned() + &"0".repeat(33),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(r.validate_format(), Ok(()));
        let mut l = log();
        assert_eq!(
            conforms(Some(&decl(vec![r])), &mut l),
            ConformanceState::Unknown
        );
    }

    #[test]
    fn unknown_version_drift_rejected() {
        let r = ConformanceRecord {
            version: "0.2.0-rc.1".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        let mut l = log();
        let state = conforms(Some(&decl(vec![r])), &mut l);
        assert_eq!(state, ConformanceState::Unknown);
        assert!(!state.is_l2());
        assert_eq!(l.len(), 1);
        assert!(l.last().unwrap().reason.contains("drift"));
    }

    #[test]
    fn unknown_commit_drift_rejected() {
        let r = ConformanceRecord {
            commit: "deadbeef".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        let mut l = log();
        let state = conforms(Some(&decl(vec![r])), &mut l);
        assert_eq!(state, ConformanceState::Unknown);
    }

    #[test]
    fn unknown_integrity_drift_rejected() {
        let r = ConformanceRecord {
            integrity: "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
                .to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        assert_eq!(r.validate_format(), Ok(()), "format still valid");
        let mut l = log();
        let state = conforms(Some(&decl(vec![r])), &mut l);
        assert_eq!(state, ConformanceState::Unknown);
    }

    #[test]
    fn unknown_bad_integrity_format_fails_closed() {
        let r = ConformanceRecord {
            integrity: "sha512-not-base64!!".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        let mut l = log();
        let state = conforms(Some(&decl(vec![r])), &mut l);
        assert_eq!(state, ConformanceState::Unknown);
        assert!(l.last().unwrap().reason.contains("format validation"));
    }

    #[test]
    fn unknown_when_any_record_malformed() {
        // Fail-closed: one malformed record poisons the whole declaration.
        let good = ConformanceRecord::known_baseline();
        let bad = ConformanceRecord {
            integrity: "garbage".to_owned(),
            ..ConformanceRecord::known_baseline()
        };
        let mut l = log();
        let state = conforms(Some(&decl(vec![good, bad])), &mut l);
        assert_eq!(state, ConformanceState::Unknown);
    }

    #[test]
    fn known_with_extra_records_still_known() {
        let extra = ConformanceRecord {
            package: "@dsh-std/connection".to_owned(),
            version: "0.1.1-rc.2".to_owned(),
            commit: "3df0543".to_owned(),
            integrity: "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
                .to_owned(),
        };
        let mut l = log();
        let state = conforms(
            Some(&decl(vec![extra, ConformanceRecord::known_baseline()])),
            &mut l,
        );
        assert_eq!(state, ConformanceState::Known);
    }

    #[test]
    fn every_evaluation_is_recorded() {
        let mut l = log();
        conforms(None, &mut l);
        conforms(
            Some(&decl(vec![ConformanceRecord::known_baseline()])),
            &mut l,
        );
        conforms(
            Some(&decl(vec![ConformanceRecord {
                version: "9.9.9".to_owned(),
                ..ConformanceRecord::known_baseline()
            }])),
            &mut l,
        );
        assert_eq!(l.len(), 3);
        assert_eq!(
            l.states(),
            vec![
                ConformanceState::Absent,
                ConformanceState::Known,
                ConformanceState::Unknown
            ]
        );
    }

    // ---- fixture-driven parsing ----

    #[test]
    fn parses_known_fixture_and_is_known() {
        let json = include_str!("../fixtures/known-dsh-std.json");
        let d = ConformanceDeclaration::from_json(json).expect("fixture must parse");
        assert_eq!(d.records.len(), 1);
        assert_eq!(d.records[0], ConformanceRecord::known_baseline());
        let mut l = log();
        assert_eq!(conforms(Some(&d), &mut l), ConformanceState::Known);
    }

    #[test]
    fn parses_drift_fixtures_as_unknown() {
        for (name, json) in [
            (
                "version-drift",
                include_str!("../fixtures/unknown-version-drift.json"),
            ),
            (
                "commit-drift",
                include_str!("../fixtures/unknown-commit-drift.json"),
            ),
            (
                "integrity-drift",
                include_str!("../fixtures/unknown-integrity-drift.json"),
            ),
            (
                "bad-integrity",
                include_str!("../fixtures/invalid-integrity.json"),
            ),
        ] {
            let d = ConformanceDeclaration::from_json(json)
                .unwrap_or_else(|e| panic!("fixture {name} must parse: {e}"));
            let mut l = log();
            let state = conforms(Some(&d), &mut l);
            assert_eq!(state, ConformanceState::Unknown, "fixture {name}");
        }
    }
}

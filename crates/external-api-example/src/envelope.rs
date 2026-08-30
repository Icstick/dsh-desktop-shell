//! Interop envelope wire types and frame-level validation.
//!
//! Field-for-field mirror of `specs/protocol/envelope.schema.json` and
//! `specs/protocol/protocol-coordinate.schema.json`, with validation
//! semantics ported from `packages/capability-contracts/src/validate.ts`:
//! protocol const, id bounds, participant shape, timestamp format,
//! generation ≥ 0 (enforced by `u64`), per-kind required/forbidden
//! fields and the Result success/error oneOf. `additionalProperties:
//! false` is enforced by `#[serde(deny_unknown_fields)]` at
//! deserialization time — same fail-closed behavior as the TS checker.
//!
//! Cross-message semantics (replyTo/correlation matching, granted ⊆
//! Hello.supports, invocation ⊆ granted) live in `server`/`client` —
//! the same split as `validate.ts` (frame) vs `semantics.ts`
//! (sequence).

use std::collections::HashSet;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Wire protocol identifier (envelope.schema.json `protocol` const).
pub const PROTOCOL: &str = "interop.dsh-desktop.local/v1alpha1";

/// Envelope id bounds (schema `minLength`/`maxLength`).
pub const ID_MIN_LEN: usize = 8;
pub const ID_MAX_LEN: usize = 128;

/// `error.message` bound (schema `maxLength`).
pub const ERROR_MESSAGE_MAX_LEN: usize = 512;

/// Envelope kind discriminator (schema `kind` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeKind {
    Hello,
    Agreement,
    Invocation,
    Result,
    Event,
}

/// Structured error codes (schema `error.code` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Unavailable,
    Unauthorized,
    UnsupportedVersion,
    NotProcessOwner,
    UserGestureRequired,
    UserDenied,
    StaleGeneration,
    MalformedMessage,
    Conflict,
    Timeout,
    SafeStop,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wire = match self {
            Self::Unavailable => "UNAVAILABLE",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::UnsupportedVersion => "UNSUPPORTED_VERSION",
            Self::NotProcessOwner => "NOT_PROCESS_OWNER",
            Self::UserGestureRequired => "USER_GESTURE_REQUIRED",
            Self::UserDenied => "USER_DENIED",
            Self::StaleGeneration => "STALE_GENERATION",
            Self::MalformedMessage => "MALFORMED_MESSAGE",
            Self::Conflict => "CONFLICT",
            Self::Timeout => "TIMEOUT",
            Self::SafeStop => "SAFE_STOP",
        };
        f.write_str(wire)
    }
}

/// Reason a requested capability was not granted
/// (`agreementPayload.unavailable[].reason` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    Unavailable,
    UnsupportedVersion,
    PolicyDenied,
    ProviderFailed,
}

/// Capability coordinate (protocol-coordinate.schema.json).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolCoordinate {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
}

/// Envelope sender identity (schema `participant`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Participant {
    pub component: String,
    pub facet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_id: Option<String>,
}

/// Structured protocol error (schema `error`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(rename = "correlationId")]
    pub correlation_id: String,
}

/// One entry of `Hello.payload.requires` (schema `requirement`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub coordinate: ProtocolCoordinate,
    pub required: bool,
}

/// `Hello` payload (schema `helloPayload`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HelloPayload {
    pub instance_id: String,
    pub supports: Vec<ProtocolCoordinate>,
    pub requires: Vec<Requirement>,
}

/// One entry of `Agreement.payload.unavailable`
/// (schema `unavailableCapability`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnavailableCapability {
    pub coordinate: ProtocolCoordinate,
    pub reason: UnavailableReason,
}

/// Lease constraints offered inside an Agreement
/// (schema `agreementPayload.leaseConstraints`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LeaseConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_required: Option<bool>,
}

/// `Agreement` payload (schema `agreementPayload`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgreementPayload {
    pub activation_id: String,
    pub granted: Vec<ProtocolCoordinate>,
    pub unavailable: Vec<UnavailableCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_constraints: Option<LeaseConstraints>,
}

/// The interop envelope (schema root).
///
/// `kind`-specific field rules are enforced by `validate_envelope`;
/// unknown fields anywhere are rejected at deserialization
/// (`deny_unknown_fields`, mirroring `additionalProperties: false`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub protocol: String,
    pub id: String,
    pub kind: EnvelopeKind,
    #[serde(rename = "replyTo", skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub participant: Participant,
    pub timestamp: String,
    pub generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<ProtocolCoordinate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
}

/// One validation finding, with a JSON-pointer-ish path — mirrors
/// `ValidationIssue` in validate.ts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

impl ValidationIssue {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// Frame-level validation — the Rust counterpart of `validateEnvelope`
/// (packages/capability-contracts/src/validate.ts).
pub fn validate_envelope(env: &Envelope) -> Result<(), Vec<ValidationIssue>> {
    let mut issues = Vec::new();

    if env.protocol != PROTOCOL {
        issues.push(ValidationIssue::new(
            "envelope.protocol",
            format!("expected const \"${PROTOCOL}\", got \"{}\"", env.protocol),
        ));
    }
    if !(ID_MIN_LEN..=ID_MAX_LEN).contains(&env.id.len()) {
        issues.push(ValidationIssue::new(
            "envelope.id",
            format!(
                "id length {} outside ${ID_MIN_LEN}..=${ID_MAX_LEN}",
                env.id.len()
            ),
        ));
    }
    if env.participant.component.is_empty() {
        issues.push(ValidationIssue::new(
            "envelope.participant.component",
            "minLength 1",
        ));
    }
    if env.participant.facet.is_empty() {
        issues.push(ValidationIssue::new(
            "envelope.participant.facet",
            "minLength 1",
        ));
    }
    if let Some(activation_id) = &env.participant.activation_id
        && (activation_id.is_empty() || activation_id.len() > 128)
    {
        issues.push(ValidationIssue::new(
            "envelope.participant.activationId",
            "length outside 1..=128",
        ));
    }
    if !is_rfc3339_timestamp(&env.timestamp) {
        issues.push(ValidationIssue::new(
            "envelope.timestamp",
            format!("\"{}\" is not a valid date-time", env.timestamp),
        ));
    }

    match env.kind {
        EnvelopeKind::Hello => validate_hello(env, &mut issues),
        EnvelopeKind::Agreement => validate_agreement(env, &mut issues),
        EnvelopeKind::Invocation => validate_invocation(env, &mut issues),
        EnvelopeKind::Result => validate_result(env, &mut issues),
        EnvelopeKind::Event => validate_event(env, &mut issues),
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

fn validate_hello(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    forbid(
        env.reply_to.is_some(),
        "envelope.replyTo",
        "Hello must not carry replyTo",
        issues,
    );
    forbid(
        env.capability.is_some(),
        "envelope.capability",
        "Hello must not carry capability",
        issues,
    );
    forbid(
        env.method.is_some(),
        "envelope.method",
        "Hello must not carry method",
        issues,
    );
    forbid(
        env.error.is_some(),
        "envelope.error",
        "Hello must not carry error",
        issues,
    );

    let Some(payload) = parse_payload::<HelloPayload>(env, issues, "envelope.payload") else {
        return;
    };
    if !(8..=128).contains(&payload.instance_id.len()) {
        issues.push(ValidationIssue::new(
            "envelope.payload.instanceId",
            format!("length {} outside 8..=128", payload.instance_id.len()),
        ));
    }
    validate_coordinates(&payload.supports, "envelope.payload.supports", issues);
    if !unique_serialized(&payload.supports) {
        issues.push(ValidationIssue::new(
            "envelope.payload.supports",
            "items must be unique",
        ));
    }
    for (i, requirement) in payload.requires.iter().enumerate() {
        validate_coordinate(
            &requirement.coordinate,
            &format!("envelope.payload.requires[{i}].coordinate"),
            issues,
        );
    }
    if !unique_serialized(&payload.requires) {
        issues.push(ValidationIssue::new(
            "envelope.payload.requires",
            "items must be unique",
        ));
    }
}

fn validate_agreement(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    forbid(
        env.capability.is_some(),
        "envelope.capability",
        "Agreement must not carry capability",
        issues,
    );
    forbid(
        env.method.is_some(),
        "envelope.method",
        "Agreement must not carry method",
        issues,
    );
    forbid(
        env.error.is_some(),
        "envelope.error",
        "Agreement must not carry error",
        issues,
    );
    validate_reply_to(env, issues);

    let Some(payload) = parse_payload::<AgreementPayload>(env, issues, "envelope.payload") else {
        return;
    };
    if payload.activation_id.is_empty() || payload.activation_id.len() > 128 {
        issues.push(ValidationIssue::new(
            "envelope.payload.activationId",
            "length outside 1..=128",
        ));
    }
    validate_coordinates(&payload.granted, "envelope.payload.granted", issues);
    if !unique_serialized(&payload.granted) {
        issues.push(ValidationIssue::new(
            "envelope.payload.granted",
            "items must be unique",
        ));
    }
    for (i, unavailable) in payload.unavailable.iter().enumerate() {
        validate_coordinate(
            &unavailable.coordinate,
            &format!("envelope.payload.unavailable[{i}].coordinate"),
            issues,
        );
    }
    if !unique_serialized(&payload.unavailable) {
        issues.push(ValidationIssue::new(
            "envelope.payload.unavailable",
            "items must be unique",
        ));
    }
    if let Some(constraints) = &payload.lease_constraints
        && let Some(max_seconds) = constraints.max_seconds
        && max_seconds < 1
    {
        issues.push(ValidationIssue::new(
            "envelope.payload.leaseConstraints.maxSeconds",
            "minimum 1",
        ));
    }
}

fn validate_invocation(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    forbid(
        env.reply_to.is_some(),
        "envelope.replyTo",
        "Invocation must not carry replyTo",
        issues,
    );
    forbid(
        env.error.is_some(),
        "envelope.error",
        "Invocation must not carry error",
        issues,
    );
    validate_capability(env, issues);
    validate_method(env, issues);
    validate_payload_object(env, issues);
}

fn validate_event(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    forbid(
        env.error.is_some(),
        "envelope.error",
        "Event must not carry error",
        issues,
    );
    validate_capability(env, issues);
    validate_method(env, issues);
    validate_payload_object(env, issues);
}

fn validate_result(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    validate_reply_to(env, issues);
    validate_capability(env, issues);
    validate_method(env, issues);
    match (env.payload.as_ref(), env.error.as_ref()) {
        (Some(_), Some(_)) => issues.push(ValidationIssue::new(
            "envelope",
            "oneOf matched 2 branches (expected exactly 1): payload and error are mutually exclusive",
        )),
        (None, None) => issues.push(ValidationIssue::new(
            "envelope",
            "oneOf matched 0 branches (expected exactly 1): Result requires payload or error",
        )),
        (Some(payload), None) => {
            if !payload.is_object() {
                issues.push(ValidationIssue::new("envelope.payload", "payload must be an object"));
            }
        }
        (None, Some(error)) => {
            if error.message.chars().count() > ERROR_MESSAGE_MAX_LEN {
                issues.push(ValidationIssue::new(
                    "envelope.error.message",
                    format!("longer than maxLength ${ERROR_MESSAGE_MAX_LEN}"),
                ));
            }
            if !(ID_MIN_LEN..=ID_MAX_LEN).contains(&error.correlation_id.len()) {
                issues.push(ValidationIssue::new(
                    "envelope.error.correlationId",
                    format!("length {} outside ${ID_MIN_LEN}..=${ID_MAX_LEN}", error.correlation_id.len()),
                ));
            }
        }
    }
}

fn forbid(condition: bool, path: &str, message: &str, issues: &mut Vec<ValidationIssue>) {
    if condition {
        issues.push(ValidationIssue::new(path, message));
    }
}

fn validate_reply_to(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    match env.reply_to.as_ref() {
        Some(reply_to) if (ID_MIN_LEN..=ID_MAX_LEN).contains(&reply_to.len()) => {}
        Some(reply_to) => issues.push(ValidationIssue::new(
            "envelope.replyTo",
            format!(
                "length {} outside ${ID_MIN_LEN}..=${ID_MAX_LEN}",
                reply_to.len()
            ),
        )),
        None => issues.push(ValidationIssue::new(
            "envelope.replyTo",
            "missing required \"replyTo\"",
        )),
    }
}

fn validate_capability(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    match env.capability.as_ref() {
        Some(coordinate) => validate_coordinate(coordinate, "envelope.capability", issues),
        None => issues.push(ValidationIssue::new(
            "envelope.capability",
            "missing required \"capability\"",
        )),
    }
}

fn validate_method(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    match env.method.as_ref() {
        Some(method) if valid_method(method) => {}
        Some(method) => issues.push(ValidationIssue::new(
            "envelope.method",
            format!("\"{method}\" does not match ^[a-z][a-z0-9._-]+$"),
        )),
        None => issues.push(ValidationIssue::new(
            "envelope.method",
            "missing required \"method\"",
        )),
    }
}

fn validate_payload_object(env: &Envelope, issues: &mut Vec<ValidationIssue>) {
    match env.payload.as_ref() {
        Some(payload) if payload.is_object() => {}
        Some(_) => issues.push(ValidationIssue::new(
            "envelope.payload",
            "payload must be an object",
        )),
        None => issues.push(ValidationIssue::new(
            "envelope.payload",
            "missing required \"payload\"",
        )),
    }
}

fn validate_coordinate(
    coordinate: &ProtocolCoordinate,
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    if !valid_api_version(&coordinate.api_version) {
        issues.push(ValidationIssue::new(
            path,
            format!(
                "\"{}\" does not match ^[a-z0-9.-]+/v[0-9]+(alpha[0-9]+|beta[0-9]+)?$",
                coordinate.api_version
            ),
        ));
    }
    if !valid_coordinate_kind(&coordinate.kind) {
        issues.push(ValidationIssue::new(
            path,
            format!("\"{}\" does not match ^[A-Z][A-Za-z0-9]+$", coordinate.kind),
        ));
    }
}

fn validate_coordinates(
    list: &[ProtocolCoordinate],
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    for coordinate in list {
        validate_coordinate(coordinate, path, issues);
    }
}

/// Parse a typed payload; pushes a shape issue on failure (mirrors the
/// `$defs` refs of the schema).
fn parse_payload<T: DeserializeOwned>(
    env: &Envelope,
    issues: &mut Vec<ValidationIssue>,
    path: &str,
) -> Option<T> {
    let Some(value) = env.payload.as_ref() else {
        issues.push(ValidationIssue::new(path, "missing required \"payload\""));
        return None;
    };
    match serde_json::from_value::<T>(value.clone()) {
        Ok(parsed) => Some(parsed),
        Err(error) => {
            issues.push(ValidationIssue::new(
                path,
                format!("payload does not match shape: {error}"),
            ));
            None
        }
    }
}

/// Deep-equality uniqueness (schema `uniqueItems`).
fn unique_serialized<T: Serialize>(items: &[T]) -> bool {
    let mut seen = HashSet::new();
    items
        .iter()
        .all(|item| seen.insert(serde_json::to_string(item).unwrap_or_default()))
}

/// `^[a-z][a-z0-9._-]+$` (schema `method` pattern).
fn valid_method(method: &str) -> bool {
    let mut chars = method.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    method.len() >= 2
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

/// `^[a-z0-9.-]+/v[0-9]+(alpha[0-9]+|beta[0-9]+)?$`
/// (protocol-coordinate.schema.json `apiVersion` pattern).
fn valid_api_version(value: &str) -> bool {
    let Some((namespace, version)) = value.rsplit_once('/') else {
        return false;
    };
    if namespace.is_empty()
        || !namespace
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-'))
    {
        return false;
    }
    let bytes = version.as_bytes();
    if bytes.first() != Some(&b'v') {
        return false;
    }
    let mut i = 1;
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return false;
    }
    if i < bytes.len() {
        let suffix = &version[i..];
        let stage_rest = suffix
            .strip_prefix("alpha")
            .or_else(|| suffix.strip_prefix("beta"));
        match stage_rest {
            Some(rest) if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) => {}
            _ => return false,
        }
    }
    true
}

/// `^[A-Z][A-Za-z0-9]+$` (protocol-coordinate.schema.json `kind` pattern).
fn valid_coordinate_kind(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {}
        _ => return false,
    }
    value.len() >= 2 && chars.all(|c| c.is_ascii_alphanumeric())
}

/// Minimal RFC 3339 date-time check (the `format: date-time` keyword;
/// TS uses `Date.parse`). Accepts `2026-08-31T09:30:00.000Z`,
/// fractional seconds and numeric offsets.
fn is_rfc3339_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return false;
    }
    // YYYY-MM-DD
    if !bytes[0..4].iter().all(|b| b.is_ascii_digit())
        || bytes[4] != b'-'
        || !bytes[5..7].iter().all(|b| b.is_ascii_digit())
        || bytes[7] != b'-'
        || !bytes[8..10].iter().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let month = (bytes[5] - b'0') * 10 + (bytes[6] - b'0');
    let day = (bytes[8] - b'0') * 10 + (bytes[9] - b'0');
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    // THH:MM:SS
    if !matches!(bytes[10], b'T' | b't')
        || !bytes[11..13].iter().all(|b| b.is_ascii_digit())
        || bytes[13] != b':'
        || !bytes[14..16].iter().all(|b| b.is_ascii_digit())
        || bytes[16] != b':'
        || !bytes[17..19].iter().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let hour = (bytes[11] - b'0') * 10 + (bytes[12] - b'0');
    let minute = (bytes[14] - b'0') * 10 + (bytes[15] - b'0');
    let second = (bytes[17] - b'0') * 10 + (bytes[18] - b'0');
    if hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let mut i = 19;
    // optional fraction
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let fraction_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == fraction_start {
            return false;
        }
    }
    // Z or ±HH:MM
    match bytes.get(i) {
        Some(b'Z') | Some(b'z') => i + 1 == bytes.len(),
        Some(b'+') | Some(b'-') => {
            if bytes.len() != i + 6
                || !bytes[i + 1..i + 3].iter().all(|b| b.is_ascii_digit())
                || bytes[i + 3] != b':'
                || !bytes[i + 4..i + 6].iter().all(|b| b.is_ascii_digit())
            {
                return false;
            }
            let offset_hour = (bytes[i + 1] - b'0') * 10 + (bytes[i + 2] - b'0');
            let offset_minute = (bytes[i + 4] - b'0') * 10 + (bytes[i + 5] - b'0');
            offset_hour <= 23 && offset_minute <= 59
        }
        _ => false,
    }
}

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Unique message id with the `msg-` prefix (≥ 8 chars, the TS
/// `newMessageId` equivalent).
pub fn new_message_id() -> String {
    unique_token("msg")
}

/// Unique activation id with the `act-` prefix.
pub fn new_activation_id() -> String {
    unique_token("act")
}

fn unique_token(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{prefix}-{nanos:016x}-{:04x}",
        ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Current time as an RFC 3339 UTC timestamp with millisecond precision
/// (no external crates; civil-date conversion after Hinnant).
pub fn now_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (now.as_secs() / 86_400) as i64;
    let rem = now.as_secs() % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60,
        now.subsec_millis()
    )
}

/// Days since 1970-01-01 → (year, month, day), Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(kind: EnvelopeKind) -> Envelope {
        Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind,
            reply_to: None,
            participant: Participant {
                component: "dsh-desktop-shell".into(),
                facet: "test".into(),
                activation_id: None,
            },
            timestamp: now_timestamp(),
            generation: 0,
            capability: None,
            method: None,
            payload: None,
            error: None,
        }
    }

    fn coordinate() -> ProtocolCoordinate {
        ProtocolCoordinate {
            api_version: "system.dsh-desktop.local/v1alpha1".into(),
            kind: "System".into(),
        }
    }

    #[test]
    fn fixture_shaped_hello_passes() {
        let mut env = envelope(EnvelopeKind::Hello);
        env.payload = Some(serde_json::json!({
            "instanceId": "ext-tool-7f3a9c2e",
            "supports": [{ "apiVersion": "system.dsh-desktop.local/v1alpha1", "kind": "System" }],
            "requires": [],
        }));
        assert_eq!(validate_envelope(&env), Ok(()));
    }

    #[test]
    fn fixture_shaped_agreement_passes() {
        let mut env = envelope(EnvelopeKind::Agreement);
        env.reply_to = Some(new_message_id());
        env.payload = Some(serde_json::json!({
            "activationId": "act-7f3a9c2e",
            "granted": [{ "apiVersion": "system.dsh-desktop.local/v1alpha1", "kind": "System" }],
            "unavailable": [{
                "coordinate": { "apiVersion": "browser.dsh-desktop.local/v1alpha1", "kind": "Browser" },
                "reason": "policy_denied"
            }],
        }));
        assert_eq!(validate_envelope(&env), Ok(()));
    }

    #[test]
    fn wrong_protocol_fails() {
        let mut env = envelope(EnvelopeKind::Hello);
        env.protocol = "interop.dsh-desktop.local/v2".into();
        env.payload = Some(serde_json::json!({
            "instanceId": "ext-tool-7f3a9c2e", "supports": [], "requires": [],
        }));
        let err = validate_envelope(&env).unwrap_err();
        assert!(err.iter().any(|i| i.path == "envelope.protocol"));
    }

    #[test]
    fn short_id_fails() {
        let mut env = envelope(EnvelopeKind::Hello);
        env.id = "short".into();
        env.payload = Some(serde_json::json!({
            "instanceId": "ext-tool-7f3a9c2e", "supports": [], "requires": [],
        }));
        let err = validate_envelope(&env).unwrap_err();
        assert!(err.iter().any(|i| i.path == "envelope.id"));
    }

    #[test]
    fn invocation_requires_capability_method_payload() {
        let env = envelope(EnvelopeKind::Invocation);
        let err = validate_envelope(&env).unwrap_err();
        let paths: Vec<_> = err.iter().map(|i| i.path.as_str()).collect();
        assert!(paths.contains(&"envelope.capability"));
        assert!(paths.contains(&"envelope.method"));
        assert!(paths.contains(&"envelope.payload"));
    }

    #[test]
    fn invocation_bad_method_pattern_fails() {
        let mut env = envelope(EnvelopeKind::Invocation);
        env.capability = Some(coordinate());
        env.method = Some("Ping".into());
        env.payload = Some(serde_json::json!({}));
        let err = validate_envelope(&env).unwrap_err();
        assert!(err.iter().any(|i| i.path == "envelope.method"));
    }

    #[test]
    fn bad_coordinate_fails() {
        let mut env = envelope(EnvelopeKind::Invocation);
        env.capability = Some(ProtocolCoordinate {
            api_version: "system/1".into(),
            kind: "lower".into(),
        });
        env.method = Some("ping".into());
        env.payload = Some(serde_json::json!({}));
        let err = validate_envelope(&env).unwrap_err();
        assert!(err.iter().any(|i| i.path == "envelope.capability"));
    }

    #[test]
    fn result_with_both_payload_and_error_fails() {
        let mut env = envelope(EnvelopeKind::Result);
        env.reply_to = Some(new_message_id());
        env.capability = Some(coordinate());
        env.method = Some("ping".into());
        env.payload = Some(serde_json::json!({ "pong": true }));
        env.error = Some(ProtocolError {
            code: ErrorCode::Unauthorized,
            message: "nope".into(),
            retryable: false,
            correlation_id: new_message_id(),
        });
        let err = validate_envelope(&env).unwrap_err();
        assert!(
            err.iter()
                .any(|i| i.message.contains("oneOf matched 2 branches"))
        );
    }

    #[test]
    fn result_with_neither_payload_nor_error_fails() {
        let mut env = envelope(EnvelopeKind::Result);
        env.reply_to = Some(new_message_id());
        env.capability = Some(coordinate());
        env.method = Some("ping".into());
        let err = validate_envelope(&env).unwrap_err();
        assert!(
            err.iter()
                .any(|i| i.message.contains("oneOf matched 0 branches"))
        );
    }

    #[test]
    fn hello_with_forbidden_reply_to_fails() {
        let mut env = envelope(EnvelopeKind::Hello);
        env.reply_to = Some(new_message_id());
        env.payload = Some(serde_json::json!({
            "instanceId": "ext-tool-7f3a9c2e", "supports": [], "requires": [],
        }));
        let err = validate_envelope(&env).unwrap_err();
        assert!(err.iter().any(|i| i.path == "envelope.replyTo"));
    }

    #[test]
    fn event_with_error_fails() {
        let mut env = envelope(EnvelopeKind::Event);
        env.capability = Some(coordinate());
        env.method = Some("output".into());
        env.payload = Some(serde_json::json!({}));
        env.error = Some(ProtocolError {
            code: ErrorCode::Timeout,
            message: "boom".into(),
            retryable: true,
            correlation_id: new_message_id(),
        });
        let err = validate_envelope(&env).unwrap_err();
        assert!(err.iter().any(|i| i.path == "envelope.error"));
    }

    #[test]
    fn unknown_root_field_rejected_at_deserialization() {
        let mut env = envelope(EnvelopeKind::Hello);
        env.payload = Some(serde_json::json!({
            "instanceId": "ext-tool-7f3a9c2e", "supports": [], "requires": [],
        }));
        let mut value = serde_json::to_value(&env).unwrap();
        value["sneaky"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Envelope>(value).is_err());
    }

    #[test]
    fn negative_generation_rejected_at_deserialization() {
        let mut env = envelope(EnvelopeKind::Hello);
        env.payload = Some(serde_json::json!({
            "instanceId": "ext-tool-7f3a9c2e", "supports": [], "requires": [],
        }));
        let mut value = serde_json::to_value(&env).unwrap();
        value["generation"] = serde_json::json!(-1);
        assert!(serde_json::from_value::<Envelope>(value).is_err());
    }

    #[test]
    fn unknown_payload_field_rejected() {
        let mut env = envelope(EnvelopeKind::Hello);
        env.payload = Some(serde_json::json!({
            "instanceId": "ext-tool-7f3a9c2e",
            "supports": [],
            "requires": [],
            "sneaky": 1,
        }));
        let err = validate_envelope(&env).unwrap_err();
        assert!(err.iter().any(|i| i.path == "envelope.payload"));
    }

    #[test]
    fn rfc3339_valid_and_invalid() {
        assert!(is_rfc3339_timestamp("2026-08-31T09:30:00.000Z"));
        assert!(is_rfc3339_timestamp("2026-08-31T09:30:00Z"));
        assert!(is_rfc3339_timestamp("2026-08-31T09:30:00+08:00"));
        assert!(is_rfc3339_timestamp("2026-08-31t09:30:00z"));
        assert!(!is_rfc3339_timestamp("2026-13-31T09:30:00Z"));
        assert!(!is_rfc3339_timestamp("2026-08-31 09:30:00"));
        assert!(!is_rfc3339_timestamp("not-a-date"));
        assert!(is_rfc3339_timestamp(&now_timestamp()));
    }

    #[test]
    fn civil_from_days_known_epochs() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(20_468), (2026, 1, 15));
    }

    #[test]
    fn api_version_patterns() {
        assert!(valid_api_version("system.dsh-desktop.local/v1alpha1"));
        assert!(valid_api_version("core/v1"));
        assert!(valid_api_version("a.b-c/v2beta3"));
        assert!(!valid_api_version("System/v1"));
        assert!(!valid_api_version("system/v"));
        assert!(!valid_api_version("system/v1x"));
        assert!(!valid_api_version("system"));
    }

    #[test]
    fn coordinate_kind_patterns() {
        assert!(valid_coordinate_kind("System"));
        assert!(valid_coordinate_kind("Browser2"));
        assert!(!valid_coordinate_kind("system"));
        assert!(!valid_coordinate_kind("S"));
        assert!(!valid_coordinate_kind(""));
    }
}

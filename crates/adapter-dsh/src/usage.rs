//! Token-usage aggregation from DSH session flows (M5-C, ADR-0018 decision 6).
//!
//! Verified shape (D:\deepseek-harness llm/llm/src/types.ts): TokenUsage =
//! {inputTokens, outputTokens, totalTokens?, cacheReadTokens?,
//! cacheWriteTokens?, reasoningTokens?}. Usage appears on assistant/message
//! events at data.usage and in ask-flow chunks at {"type":"usage","usage":{...}}.
//! The $events allowlist does NOT carry usage - acquisition goes through a
//! session flow (session/follow seam, see crate::client::SessionFlow).
//!
//! Output follows the frozen desktop contract
//! (specs/usage/usage-record.schema.json + usage-snapshot.schema.json):
//! records are per-session aggregates over the observed window, always
//! flagged isEstimate (DSH usage numbers are best-effort, not billing).

use std::collections::HashMap;

use serde::Serialize;

use crate::events::{EventMessage, allowlist};

/// Source label for records produced from DSH session flows.
pub const USAGE_SOURCE: &str = "dsh";
const UNKNOWN_SESSION: &str = "<unknown>";

/// One extracted usage sample from a session-flow value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSample {
    pub session_id: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub at_unix_ms: Option<u64>,
}

/// Extract a usage sample from a raw flow value (pure, fail-closed: no
/// recognizable usage object means None, never a zeroed guess).
pub fn extract_sample(value: &serde_json::Value) -> Option<UsageSample> {
    let usage = find_usage(value)?;
    let input = first_u64(
        usage,
        &[
            "inputTokens",
            "input_tokens",
            "promptTokens",
            "prompt_tokens",
        ],
    )?;
    let output = first_u64(
        usage,
        &[
            "outputTokens",
            "output_tokens",
            "completionTokens",
            "completion_tokens",
        ],
    )?;
    let cache_read = first_u64(usage, &["cacheReadTokens", "cache_read_tokens"]).unwrap_or(0);
    let session_id = first_string(value, &["sessionId", "session_id"])
        .or_else(|| descend(value, &["session", "id"]).and_then(serde_json::Value::as_str))
        .map(str::to_string);
    let at_unix_ms = first_u64(
        value,
        &["createdAtUnixMs", "timestampUnixMs", "timestampMs", "ts"],
    );
    Some(UsageSample {
        session_id,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        at_unix_ms,
    })
}

/// Per-session aggregate (pure accumulator).
#[derive(Debug, Clone, Default)]
struct SessionAccum {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    first_at_unix_ms: Option<u64>,
    last_at_unix_ms: Option<u64>,
}

/// Aggregates usage samples per session.
#[derive(Debug, Clone, Default)]
pub struct UsageAggregator {
    sessions: HashMap<String, SessionAccum>,
    order: Vec<String>,
    /// Flow values that yielded a usable usage sample.
    pub usage_events: u64,
    /// Values consumed without a recognizable usage object.
    pub skipped_events: u64,
}

impl UsageAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingest one raw session-flow value (assistant/message, usage chunk...).
    pub fn ingest_value(&mut self, session_id: Option<&str>, value: &serde_json::Value) {
        let Some(sample) = extract_sample(value) else {
            self.skipped_events += 1;
            return;
        };
        self.usage_events += 1;
        let key = sample
            .session_id
            .or_else(|| session_id.map(str::to_string))
            .unwrap_or_else(|| UNKNOWN_SESSION.to_string());
        let entry = self.sessions.entry(key.clone()).or_insert_with(|| {
            self.order.push(key.clone());
            SessionAccum::default()
        });
        entry.input_tokens = entry.input_tokens.saturating_add(sample.input_tokens);
        entry.output_tokens = entry.output_tokens.saturating_add(sample.output_tokens);
        entry.cache_read_tokens = entry
            .cache_read_tokens
            .saturating_add(sample.cache_read_tokens);
        entry.first_at_unix_ms = match (entry.first_at_unix_ms, sample.at_unix_ms) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        entry.last_at_unix_ms = match (entry.last_at_unix_ms, sample.at_unix_ms) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
    }

    /// Ingest an $events message (drift net: if DSH ever embeds usage in
    /// api-session events, it lands here too).
    pub fn ingest_message(&mut self, message: &EventMessage) {
        if allowlist(message).is_some() {
            let payload = crate::events::payload_of(message);
            let session_id = crate::events::session_id_of(message);
            self.ingest_value(session_id.as_deref(), &payload);
        }
    }

    /// Snapshot as schema-aligned records, in first-seen session order.
    pub fn snapshot(&self, now_unix_ms: u64) -> Vec<UsageRecord> {
        self.order
            .iter()
            .filter_map(|id| {
                let accum = self.sessions.get(id)?;
                let start = accum
                    .first_at_unix_ms
                    .or(accum.last_at_unix_ms)
                    .unwrap_or(now_unix_ms);
                let end = accum.last_at_unix_ms.unwrap_or(start);
                Some(UsageRecord {
                    schema_version: 1,
                    source: USAGE_SOURCE.to_string(),
                    period: UsagePeriod {
                        start: unix_ms_to_rfc3339(start),
                        end: unix_ms_to_rfc3339(end),
                    },
                    input_tokens: accum.input_tokens,
                    output_tokens: accum.output_tokens,
                    cache_read_tokens: (accum.cache_read_tokens > 0)
                        .then_some(accum.cache_read_tokens),
                    cost: None,
                    currency: None,
                    is_estimate: true,
                    recorded_at_unix_ms: now_unix_ms,
                })
            })
            .collect()
    }

    /// Totals across all sessions (usage-snapshot totals member).
    pub fn totals(&self) -> UsageTotals {
        let mut input = 0u64;
        let mut output = 0u64;
        for accum in self.sessions.values() {
            input = input.saturating_add(accum.input_tokens);
            output = output.saturating_add(accum.output_tokens);
        }
        UsageTotals {
            input_tokens: input,
            output_tokens: output,
            cost: None,
            currency: None,
            estimate_count: self.sessions.len() as u64,
        }
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

/// usage-record.schema.json (frozen, camelCase on the wire).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub schema_version: u8,
    pub source: String,
    pub period: UsagePeriod,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    pub is_estimate: bool,
    pub recorded_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePeriod {
    pub start: String,
    pub end: String,
}

/// usage-snapshot.schema.json totals member.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    pub estimate_count: u64,
}

/// Unix milliseconds -> RFC 3339 UTC (Howard Hinnant civil-from-days; pure,
/// deterministic, no chrono dependency).
pub fn unix_ms_to_rfc3339(unix_ms: u64) -> String {
    let days = unix_ms / 86_400_000;
    let rem_ms = unix_ms % 86_400_000;
    let (year, month, day) = civil_from_days(i64::try_from(days).unwrap_or(0));
    let hour = rem_ms / 3_600_000;
    let minute = (rem_ms / 60_000) % 60;
    let second = (rem_ms / 1000) % 60;
    let millis = rem_ms % 1000;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn find_usage(value: &serde_json::Value) -> Option<&serde_json::Value> {
    for path in [
        &["usage"][..],
        &["data", "usage"][..],
        &["session", "usage"][..],
        &["payload", "usage"][..],
        &["data", "payload", "usage"][..],
    ] {
        if let Some(found) = descend(value, path) {
            return Some(found);
        }
    }
    None
}

fn descend<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn first_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(serde_json::Value::as_u64)
}

fn first_string<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(serde_json::Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventMessage;
    use serde_json::json;

    #[test]
    fn extracts_verified_assistant_message_usage_shape() {
        let value = json!({
            "type": "assistant/message",
            "data": {
                "usage": {"inputTokens": 100, "outputTokens": 25, "cacheReadTokens": 4, "totalTokens": 129}
            },
            "sessionId": "s-1",
            "createdAtUnixMs": 1_700_000_000_000i64
        });
        let sample = extract_sample(&value).expect("sample");
        assert_eq!(sample.session_id.as_deref(), Some("s-1"));
        assert_eq!(sample.input_tokens, 100);
        assert_eq!(sample.output_tokens, 25);
        assert_eq!(sample.cache_read_tokens, 4);
        assert_eq!(sample.at_unix_ms, Some(1_700_000_000_000));
    }

    #[test]
    fn extracts_verified_usage_chunk_shape() {
        let value = json!({
            "type": "usage",
            "usage": {"inputTokens": 7, "outputTokens": 3},
            "sessionId": "s-2"
        });
        let sample = extract_sample(&value).expect("sample");
        assert_eq!(sample.input_tokens, 7);
        assert_eq!(sample.output_tokens, 3);
        assert_eq!(sample.session_id.as_deref(), Some("s-2"));
    }

    #[test]
    fn tolerates_snake_case_aliases_and_nested_session() {
        let value = json!({
            "session": {"id": "s-3"},
            "data": {"usage": {"input_tokens": 11, "output_tokens": 12}},
            "ts": 123
        });
        let sample = extract_sample(&value).expect("sample");
        assert_eq!(sample.session_id.as_deref(), Some("s-3"));
        assert_eq!(sample.input_tokens, 11);
        assert_eq!(sample.output_tokens, 12);
        assert_eq!(sample.at_unix_ms, Some(123));
    }

    #[test]
    fn fails_closed_on_missing_or_malformed_usage() {
        assert!(extract_sample(&json!({"type": "assistant/message", "data": {}})).is_none());
        assert!(extract_sample(&json!({"usage": {"inputTokens": 1}})).is_none());
        assert!(extract_sample(&json!({"usage": "nope"})).is_none());
        assert!(
            extract_sample(&json!({"data": {"usage": {"inputTokens": "x", "outputTokens": 1}}}))
                .is_none()
        );
    }

    #[test]
    fn aggregates_per_session_in_first_seen_order() {
        let mut aggregator = UsageAggregator::new();
        aggregator.ingest_value(
            Some("s-1"),
            &json!({"usage": {"inputTokens": 10, "outputTokens": 2}}),
        );
        aggregator.ingest_value(
            Some("s-2"),
            &json!({"usage": {"inputTokens": 30, "outputTokens": 5}}),
        );
        aggregator.ingest_value(
            Some("s-1"),
            &json!({"usage": {"inputTokens": 5, "outputTokens": 1}}),
        );
        aggregator.ingest_value(
            Some("s-3"),
            &json!({"type": "assistant/message", "data": {}}),
        );

        assert_eq!(aggregator.session_count(), 2);
        assert_eq!(aggregator.usage_events, 3);
        assert_eq!(aggregator.skipped_events, 1);

        let records = aggregator.snapshot(1_700_000_000_000);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].source, "dsh");
        assert!(records[0].is_estimate);
        assert_eq!(records[0].input_tokens, 15);
        assert_eq!(records[0].output_tokens, 3);
        assert_eq!(records[1].input_tokens, 30);
        assert_eq!(records[1].output_tokens, 5);

        let totals = aggregator.totals();
        assert_eq!(totals.input_tokens, 45);
        assert_eq!(totals.output_tokens, 8);
        assert_eq!(totals.estimate_count, 2);
    }

    #[test]
    fn ingest_message_uses_event_payload_and_session_id() {
        let mut aggregator = UsageAggregator::new();
        let message = EventMessage::Emit {
            event: "api-session/status".to_string(),
            args: json!([{"sessionId": "s-9", "usage": {"inputTokens": 4, "outputTokens": 6}}]),
        };
        aggregator.ingest_message(&message);
        assert_eq!(aggregator.usage_events, 1);
        let records = aggregator.snapshot(0);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].input_tokens, 4);
    }

    #[test]
    fn unix_ms_to_rfc3339_is_correct() {
        assert_eq!(unix_ms_to_rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            unix_ms_to_rfc3339(1_788_048_000_000),
            "2026-08-30T00:00:00.000Z"
        );
        assert_eq!(
            unix_ms_to_rfc3339(1_456_704_000_000),
            "2016-02-29T00:00:00.000Z"
        );
        assert_eq!(
            unix_ms_to_rfc3339(86_400_000 + 3_661_000),
            "1970-01-02T01:01:01.000Z"
        );
    }

    #[test]
    fn usage_record_serializes_camel_case() {
        let record = UsageRecord {
            schema_version: 1,
            source: "dsh".to_string(),
            period: UsagePeriod {
                start: "2026-08-30T00:00:00.000Z".to_string(),
                end: "2026-08-30T00:00:01.000Z".to_string(),
            },
            input_tokens: 1,
            output_tokens: 2,
            cache_read_tokens: Some(3),
            cost: None,
            currency: None,
            is_estimate: true,
            recorded_at_unix_ms: 9,
        };
        let value = serde_json::to_value(&record).expect("serialize");
        let object = value.as_object().expect("object");
        assert_eq!(
            object
                .get("inputTokens")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            object
                .get("outputTokens")
                .and_then(serde_json::Value::as_u64),
            Some(2)
        );
        assert_eq!(
            object
                .get("cacheReadTokens")
                .and_then(serde_json::Value::as_u64),
            Some(3)
        );
        assert_eq!(
            object
                .get("isEstimate")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(object.get("cost").is_none(), "None cost must be skipped");
    }
}

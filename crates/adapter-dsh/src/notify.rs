//! Event -> desktop notification mapping (M5-C, ADR-0018 decision 6).
//!
//! The adapter does NOT depend on tauri. It produces AdapterNotification
//! values shaped after the desktop NotificationService request contract
//! (specs/notification/notification-request.schema.json); the tauri wiring
//! converts them into NotificationRequest deliveries.
//!
//! Mapping table (fail-closed: anything unmapped yields None):
//!   approval/request (waterfall)      -> ApprovalRequired, title from toolName
//!   user-questions/request (waterfall)-> QuestionRequired, title from first question
//!   api-session/removed               -> TurnCompleted (inferred: session ended)
//!   settings/document-updated         -> ConfigChanged, dedupe config-changed
//!   cordis/dynamic-package            -> ConfigChanged, dedupe config-changed
//!   cordis/dynamic-retract            -> ConfigChanged, dedupe config-changed
//!
//! Restart hints are downgraded to event inference per ADR-0018 decision 6:
//! there is no native DSH restart-hint surface, so dynamic-package /
//! dynamic-retract / document-updated events produce a generic "config
//! changed" hint. All notifications use TitleOnly policy (no body crossing
//! the adapter boundary; approval/question bodies belong to the desktop UI).

use serde::{Deserialize, Serialize};

use crate::events::{AllowedEvent, EventMessage, allowlist, payload_of};

/// Mirrors the desktop NotificationService event enum (superset: the
/// desktop enum has no config_changed variant yet - the tauri wiring maps
/// it onto the closest local notification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEventKind {
    TurnCompleted,
    ApprovalRequired,
    QuestionRequired,
    RuntimeChanged,
    ScheduleResult,
    ConfigChanged,
}

/// Mirrors the desktop ContentPolicy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentPolicy {
    TitleOnly,
    RedactedSummary,
    ExplicitBody,
}

/// One notification to hand to the desktop layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterNotification {
    pub schema_version: u8,
    pub event: NotificationEventKind,
    pub title: String,
    pub body: Option<String>,
    pub content_policy: ContentPolicy,
    pub dedupe_key: Option<String>,
}

/// Schema version of the frozen notification-request contract.
pub const NOTIFICATION_SCHEMA_VERSION: u8 = 1;
/// Title bound from specs/notification (maxLength 128).
const MAX_TITLE_CHARS: usize = 128;
/// Dedupe key bound from specs/notification (maxLength 128).
const MAX_DEDUPE_KEY_CHARS: usize = 128;
/// Dedupe key folding config-change hints (desktop-side TTL window).
const CONFIG_CHANGED_DEDUPE_KEY: &str = "config-changed";

/// Map one decoded event to a notification; None when unmapped.
pub fn map(message: &EventMessage) -> Option<AdapterNotification> {
    let allowed = allowlist(message)?;
    let payload = payload_of(message);
    match allowed {
        AllowedEvent::ApprovalRequest => {
            let tool_name = get_str(&payload, &["toolName", "tool_name"]);
            let title = tool_name
                .map(|name| format!("审批请求：{name}"))
                .unwrap_or_else(|| "审批请求".to_string());
            Some(notification(
                NotificationEventKind::ApprovalRequired,
                title,
                None,
            ))
        }
        AllowedEvent::UserQuestionRequest => {
            let question = payload
                .get("questions")
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .and_then(|item| get_str(item, &["question", "text", "title"]));
            let title = question.unwrap_or_else(|| "收到提问".to_string());
            Some(notification(
                NotificationEventKind::QuestionRequired,
                title,
                None,
            ))
        }
        AllowedEvent::ApiSession(kind) if kind == "removed" => Some(notification(
            NotificationEventKind::TurnCompleted,
            "DSH 会话已结束".to_string(),
            None,
        )),
        AllowedEvent::ApiSession(_) => None,
        AllowedEvent::SettingsDocumentUpdated => Some(notification(
            NotificationEventKind::ConfigChanged,
            "配置已变化".to_string(),
            Some(CONFIG_CHANGED_DEDUPE_KEY.to_string()),
        )),
        AllowedEvent::CordisDynamicPackage => Some(notification(
            NotificationEventKind::ConfigChanged,
            "配置已变化（动态插件更新）".to_string(),
            Some(CONFIG_CHANGED_DEDUPE_KEY.to_string()),
        )),
        AllowedEvent::CordisDynamicRetract => Some(notification(
            NotificationEventKind::ConfigChanged,
            "配置已变化（插件已撤回）".to_string(),
            Some(CONFIG_CHANGED_DEDUPE_KEY.to_string()),
        )),
    }
}

fn notification(
    event: NotificationEventKind,
    title: String,
    dedupe_key: Option<String>,
) -> AdapterNotification {
    AdapterNotification {
        schema_version: NOTIFICATION_SCHEMA_VERSION,
        event,
        title: truncate_chars(&title, MAX_TITLE_CHARS),
        body: None,
        content_policy: ContentPolicy::TitleOnly,
        dedupe_key: dedupe_key
            .map(|key| truncate_chars(&key, MAX_DEDUPE_KEY_CHARS))
            .filter(|key| !key.is_empty()),
    }
}

pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if out.chars().count() < text.chars().count() {
        out.push('…');
    }
    out
}

fn get_str(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AdapterError;
    use crate::events::EventMessage;

    fn emit(event: &str, args: serde_json::Value) -> EventMessage {
        EventMessage::Emit {
            event: event.to_string(),
            args,
        }
    }

    fn waterfall(event: &str, request: serde_json::Value) -> EventMessage {
        EventMessage::Waterfall {
            event: event.to_string(),
            event_id: Some("e1".to_string()),
            agent_id: None,
            request,
        }
    }

    #[test]
    fn approval_waterfall_maps_with_tool_name() {
        let message = waterfall(
            "approval/request",
            serde_json::json!({"toolName": "browser_navigate", "callId": "c1"}),
        );
        let notification = map(&message).expect("mapped");
        assert_eq!(notification.event, NotificationEventKind::ApprovalRequired);
        assert_eq!(notification.title, "审批请求：browser_navigate");
        assert_eq!(notification.content_policy, ContentPolicy::TitleOnly);
        assert_eq!(notification.body, None);
    }

    #[test]
    fn approval_without_tool_name_uses_default_title() {
        let message = waterfall("approval/request", serde_json::json!({}));
        let notification = map(&message).expect("mapped");
        assert_eq!(notification.title, "审批请求");
    }

    #[test]
    fn user_question_maps_with_first_question_text() {
        let message = waterfall(
            "user-questions/request",
            serde_json::json!({"questions": [{"id": "q1", "question": "允许继续吗？", "header": "确认"}],}),
        );
        let notification = map(&message).expect("mapped");
        assert_eq!(notification.event, NotificationEventKind::QuestionRequired);
        assert_eq!(notification.title, "允许继续吗？");
    }

    #[test]
    fn user_question_without_questions_uses_default_title() {
        let message = waterfall("user-questions/request", serde_json::json!({}));
        let notification = map(&message).expect("mapped");
        assert_eq!(notification.title, "收到提问");
    }

    #[test]
    fn api_session_removed_maps_to_turn_completed_others_do_not() {
        let removed = emit(
            "api-session/removed",
            serde_json::json!([{"sessionId": "s1"}]),
        );
        assert_eq!(
            map(&removed).expect("mapped").event,
            NotificationEventKind::TurnCompleted
        );
        for kind in ["added", "status", "activity", "error"] {
            let message = emit(&format!("api-session/{kind}"), serde_json::json!([]));
            assert!(
                map(&message).is_none(),
                "api-session/{kind} must not notify"
            );
        }
    }

    #[test]
    fn config_change_hints_fold_under_one_dedupe_key() {
        let document = emit(
            "settings/document-updated",
            serde_json::json!(["settings", 3]),
        );
        let package = emit(
            "cordis/dynamic-package",
            serde_json::json!([{"pluginId": "p1", "packageId": "pk", "pluginRunId": "r", "name": "x"}]),
        );
        let retract = emit(
            "cordis/dynamic-retract",
            serde_json::json!([{"pluginId": "p1", "packageId": "pk", "pluginRunId": "r"}]),
        );
        for message in [&document, &package, &retract] {
            let notification = map(message).expect("mapped");
            assert_eq!(notification.event, NotificationEventKind::ConfigChanged);
            assert_eq!(notification.dedupe_key.as_deref(), Some("config-changed"));
            assert_eq!(notification.content_policy, ContentPolicy::TitleOnly);
        }
    }

    #[test]
    fn unmapped_events_and_ready_yield_none() {
        assert!(map(&emit("commands/change", serde_json::json!([]))).is_none());
        assert!(map(&emit("some/other", serde_json::json!([]))).is_none());
        assert!(
            map(&EventMessage::Ready(crate::events::ClientIdentity {
                client_id: "c".to_string(),
                host_home: None,
            }))
            .is_none()
        );
    }

    #[test]
    fn titles_are_truncated_to_contract_bound() {
        let message = waterfall(
            "approval/request",
            serde_json::json!({"toolName": "x".repeat(200)}),
        );
        let notification = map(&message).expect("mapped");
        assert!(notification.title.chars().count() <= MAX_TITLE_CHARS + 1);
        assert!(notification.title.ends_with('…'));
    }

    #[test]
    fn truncate_chars_handles_multibyte() {
        let text = "你好世界".repeat(100);
        let truncated = truncate_chars(&text, 10);
        assert_eq!(truncated.chars().count(), 11);
        assert!(truncated.ends_with('…'));
        assert_eq!(truncate_chars("short", 10), "short");
    }

    #[test]
    fn adapter_notification_serializes_camel_case() {
        let notification = notification(
            NotificationEventKind::ApprovalRequired,
            "审批请求".to_string(),
            None,
        );
        let value = serde_json::to_value(&notification).expect("serialize");
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["event"], "approval_required");
        assert_eq!(value["contentPolicy"], "title_only");
        assert!(value.get("dedupeKey").is_some());
    }

    #[test]
    fn error_is_constructible() {
        let error: AdapterError = AdapterError::Auth("x".to_string());
        assert!(!error.to_string().is_empty());
    }
}

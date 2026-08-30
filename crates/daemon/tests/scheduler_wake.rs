//! Wire-level tests for IF-SCHEDULE-WAKE (ADR-0019 decision 6, M6-D):
//! scheduler.wake / scheduler.cancel over the real envelope server and
//! broker chain, with daemon.status exposing the fired-wake counters.
//!
//! Time budgets: the shortest wake delay used is 100 ms and every wait
//! polls daemon.status with a generous deadline, so the suite is robust
//! against slow CI machines.

mod common;

use common::TestClient;
use dsh_daemon::capabilities::{DAEMON_API_VERSION, DAEMON_KIND, DAEMON_STATUS_METHOD};
use dsh_daemon::envelope::{ErrorCode, ProtocolCoordinate, now_timestamp, now_timestamp_like};
use dsh_daemon::scheduler::{
    SCHEDULER_API_VERSION, SCHEDULER_CANCEL_METHOD, SCHEDULER_KIND, SCHEDULER_WAKE_METHOD,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

fn scheduler() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: SCHEDULER_API_VERSION.into(),
        kind: SCHEDULER_KIND.into(),
    }
}

fn daemon() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: DAEMON_API_VERSION.into(),
        kind: DAEMON_KIND.into(),
    }
}

/// Build schema-conformant wake params.
fn wake_params(wake_id: &str, deadline_in: Option<Duration>, reason: &str) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert("wakeId".into(), serde_json::json!(wake_id));
    params.insert("requestedAt".into(), serde_json::json!(now_timestamp()));
    if let Some(delay) = deadline_in {
        let fire = SystemTime::now() + delay;
        params.insert(
            "deadline".into(),
            serde_json::json!(now_timestamp_like(fire)),
        );
    }
    params.insert("reason".into(), serde_json::json!(reason));
    serde_json::Value::Object(params)
}

/// Poll daemon.status until `predicate` holds; returns the last snapshot.
fn wait_status(
    client: &mut TestClient,
    timeout: Duration,
    predicate: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = Instant::now() + timeout;
    let mut last = serde_json::json!({});
    loop {
        last = client
            .invoke(daemon(), DAEMON_STATUS_METHOD, serde_json::json!({}))
            .expect("daemon.status succeeds");
        if predicate(&last) {
            return last;
        }
        if Instant::now() >= deadline {
            return last;
        }
        thread::sleep(Duration::from_millis(20));
    }
}
/// 1) The scheduler capability is grantable (catalog + broker chain).
#[test]
fn scheduler_capability_is_granted_in_negotiation() {
    let (addr, credential, _server) = common::spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    let agreement = client.negotiate(vec![scheduler()]);
    assert_eq!(agreement.granted, vec![scheduler()]);
    assert!(agreement.unavailable.is_empty());
}

/// 2) A 100 ms wake fires and the daemon.status scheduler counters move
///    (fire counter + lastFired record).
#[test]
fn short_delay_wake_fires_and_daemon_status_counts() {
    let (addr, credential, _server) = common::spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![scheduler(), daemon()]);

    let result = client
        .invoke(
            scheduler(),
            SCHEDULER_WAKE_METHOD,
            wake_params(
                "w-int-000001",
                Some(Duration::from_millis(100)),
                "scheduled_due",
            ),
        )
        .expect("wake accepted");
    assert_eq!(result["wakeId"], "w-int-000001");
    assert!(
        result["scheduledFor"]
            .as_str()
            .is_some_and(|t| t.contains('T'))
    );
    assert_eq!(result["pending"], 1);

    let status = wait_status(&mut client, Duration::from_secs(5), |s| {
        s["scheduler"]["fired"].as_u64() == Some(1)
    });
    assert_eq!(status["scheduler"]["registered"], 1);
    assert_eq!(status["scheduler"]["fired"], 1);
    assert_eq!(status["scheduler"]["pending"], 0);
    assert_eq!(status["scheduler"]["lastFired"]["wakeId"], "w-int-000001");
    assert_eq!(status["scheduler"]["lastFired"]["reason"], "scheduled_due");
    assert!(
        status["scheduler"]["lastFired"]["firedAt"]
            .as_str()
            .is_some()
    );
}

/// 3) A periodic wake fires its full count and then completes (removed).
#[test]
fn periodic_wake_fires_count_times_then_completes() {
    let (addr, credential, _server) = common::spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![scheduler(), daemon()]);

    let mut params = wake_params(
        "w-int-per-01",
        Some(Duration::from_millis(60)),
        "recovery_retry",
    );
    params["repeat"] = serde_json::json!({ "intervalMs": 60, "count": 3 });
    let result = client
        .invoke(scheduler(), SCHEDULER_WAKE_METHOD, params)
        .expect("periodic wake accepted");
    assert_eq!(result["repeat"]["intervalMs"], 60);
    assert_eq!(result["repeat"]["count"], 3);

    let status = wait_status(&mut client, Duration::from_secs(5), |s| {
        s["scheduler"]["fired"].as_u64() == Some(3)
    });
    assert_eq!(status["scheduler"]["fired"], 3);
    assert_eq!(status["scheduler"]["registered"], 1);
    assert_eq!(
        status["scheduler"]["pending"], 0,
        "bounded repeat completes"
    );
    assert_eq!(status["scheduler"]["lastFired"]["reason"], "recovery_retry");
}

/// 4) Cancelling a pending wake prevents the fire.
#[test]
fn cancel_prevents_a_pending_fire() {
    let (addr, credential, _server) = common::spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![scheduler(), daemon()]);

    client
        .invoke(
            scheduler(),
            SCHEDULER_WAKE_METHOD,
            wake_params(
                "w-int-can-01",
                Some(Duration::from_millis(300)),
                "user_requested",
            ),
        )
        .expect("wake accepted");

    let cancel = client
        .invoke(
            scheduler(),
            SCHEDULER_CANCEL_METHOD,
            serde_json::json!({ "wakeId": "w-int-can-01" }),
        )
        .expect("cancel accepted");
    assert_eq!(cancel["cancelled"], true);
    assert_eq!(cancel["pending"], 0);

    // Well past the original deadline: nothing fired.
    let status = wait_status(&mut client, Duration::from_millis(500), |s| {
        s["scheduler"]["cancelled"].as_u64() == Some(1)
    });
    assert_eq!(status["scheduler"]["fired"], 0);
    assert_eq!(status["scheduler"]["cancelled"], 1);
    assert!(
        status["scheduler"]["lastFired"].is_null(),
        "no fire recorded"
    );
}

/// 5) Cancelling an unknown wakeId is an idempotent success.
#[test]
fn cancel_unknown_wake_is_idempotent() {
    let (addr, credential, _server) = common::spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![scheduler()]);

    let cancel = client
        .invoke(
            scheduler(),
            SCHEDULER_CANCEL_METHOD,
            serde_json::json!({ "wakeId": "w-ghost-0001" }),
        )
        .expect("cancel accepted");
    assert_eq!(cancel["cancelled"], false);
    assert_eq!(cancel["pending"], 0);
}

/// 6) Wake params that violate the contract -> MALFORMED_MESSAGE.
#[test]
fn invalid_wake_request_is_malformed_message() {
    let (addr, credential, _server) = common::spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![scheduler()]);

    // wakeId below minLength.
    let error = client
        .invoke(
            scheduler(),
            SCHEDULER_WAKE_METHOD,
            serde_json::json!({
                "wakeId": "short",
                "requestedAt": now_timestamp(),
                "reason": "scheduled_due",
            }),
        )
        .expect_err("wakeId too short");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Unknown reason enum value.
    let error = client
        .invoke(
            scheduler(),
            SCHEDULER_WAKE_METHOD,
            serde_json::json!({
                "wakeId": "w-int-bad-01",
                "requestedAt": now_timestamp(),
                "reason": "nonsense",
            }),
        )
        .expect_err("unknown reason");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Unknown payload field (additionalProperties: false).
    let mut params = wake_params("w-int-bad-02", None, "scheduled_due");
    params["sneaky"] = serde_json::json!(1);
    let error = client
        .invoke(scheduler(), SCHEDULER_WAKE_METHOD, params)
        .expect_err("unknown field");
    assert_eq!(error.code, ErrorCode::MalformedMessage);
}

/// 7) Duplicate wakeId -> CONFLICT.
#[test]
fn duplicate_wake_id_is_conflict() {
    let (addr, credential, _server) = common::spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![scheduler(), daemon()]);

    client
        .invoke(
            scheduler(),
            SCHEDULER_WAKE_METHOD,
            wake_params(
                "w-int-dup-01",
                Some(Duration::from_secs(30)),
                "scheduled_due",
            ),
        )
        .expect("first registration");

    let error = client
        .invoke(
            scheduler(),
            SCHEDULER_WAKE_METHOD,
            wake_params(
                "w-int-dup-01",
                Some(Duration::from_secs(30)),
                "scheduled_due",
            ),
        )
        .expect_err("duplicate wakeId");
    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(error.message.contains("already scheduled"));

    // The original wake is untouched.
    let cancel = client
        .invoke(
            scheduler(),
            SCHEDULER_CANCEL_METHOD,
            serde_json::json!({ "wakeId": "w-int-dup-01" }),
        )
        .expect("cancel");
    assert_eq!(cancel["cancelled"], true);
}

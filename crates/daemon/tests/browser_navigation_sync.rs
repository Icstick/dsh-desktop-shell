//! M6-C4 (0.2.1): render-side navigation state reporting over the wire.
//! The render host reports every completed top-level navigation
//! (browser.navigate) and every failed load (browser.load-failed) so the
//! daemon session registry - the state authority - records the document
//! the user actually sees (final URL / error). Pure logic, no WebView2.

mod common;

use common::{TestClient, spawn_daemon};
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_CREATE_METHOD, BROWSER_KIND, BROWSER_LOAD_FAILED_METHOD,
    BROWSER_NAVIGATE_METHOD, BROWSER_STATUS_METHOD,
};
use dsh_daemon::envelope::{ErrorCode, ProtocolCoordinate};

fn browser() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: BROWSER_API_VERSION.into(),
        kind: BROWSER_KIND.into(),
    }
}

fn create_request() -> serde_json::Value {
    serde_json::json!({ "schemaVersion": 1, "mode": "human_surface" })
}

fn navigate_request(session_id: &str, url: &str) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "sessionId": session_id,
        "url": url,
    })
}

fn load_failed_request(session_id: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "sessionId": session_id,
        "message": message,
    })
}

/// A reported navigation updates the daemon authority: ready state with
/// the final url (including navigations the Shell never commanded - link
/// clicks, redirects, history).
#[test]
fn reported_navigation_updates_the_session_authority() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![browser()]);

    let created = client
        .invoke(browser(), BROWSER_CREATE_METHOD, create_request())
        .expect("browser.create succeeds");
    let session_id = created["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();

    let reported = client
        .invoke(
            browser(),
            BROWSER_NAVIGATE_METHOD,
            navigate_request(&session_id, "https://example.com/landed"),
        )
        .expect("browser.navigate succeeds");
    assert_eq!(reported["state"], "ready");
    assert_eq!(reported["currentUrl"], "https://example.com/landed");
    assert!(reported["error"].is_null());

    let status = client
        .invoke(browser(), BROWSER_STATUS_METHOD, serde_json::json!({}))
        .expect("browser.status succeeds");
    assert_eq!(status["sessions"][0]["state"], "ready");
    assert_eq!(
        status["sessions"][0]["currentUrl"],
        "https://example.com/landed"
    );

    // A second navigation replaces the url (history entry etc.).
    let again = client
        .invoke(
            browser(),
            BROWSER_NAVIGATE_METHOD,
            navigate_request(&session_id, "https://example.com/next"),
        )
        .expect("second navigation succeeds");
    assert_eq!(again["currentUrl"], "https://example.com/next");
}

/// A reported failed load moves the authority to the error state while
/// keeping the failed url.
#[test]
fn reported_load_failure_moves_session_to_error() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![browser()]);

    let created = client
        .invoke(browser(), BROWSER_CREATE_METHOD, create_request())
        .expect("browser.create succeeds");
    let session_id = created["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    client
        .invoke(
            browser(),
            BROWSER_NAVIGATE_METHOD,
            navigate_request(&session_id, "https://example.com/landed"),
        )
        .expect("browser.navigate succeeds");

    let failed = client
        .invoke(
            browser(),
            BROWSER_LOAD_FAILED_METHOD,
            load_failed_request(&session_id, "connection reset"),
        )
        .expect("browser.load-failed succeeds");
    assert_eq!(failed["state"], "error");
    assert_eq!(failed["error"], "connection reset");
    assert_eq!(failed["currentUrl"], "https://example.com/landed");
}

/// Policy rejections: unknown sessions, sessions owned by another
/// connection, invalid urls and malformed requests stay fail-closed.
#[test]
fn navigation_report_validation_stays_fail_closed() {
    let (addr, credential, _server) = spawn_daemon();
    let mut owner = TestClient::connect(addr, &credential);
    owner.negotiate(vec![browser()]);
    let created = owner
        .invoke(browser(), BROWSER_CREATE_METHOD, create_request())
        .expect("browser.create succeeds");
    let session_id = created["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();

    // Unknown session.
    let err = owner
        .invoke(
            browser(),
            BROWSER_NAVIGATE_METHOD,
            navigate_request("brw-000-0", "https://example.com"),
        )
        .expect_err("unknown session rejected");
    assert_eq!(err.code, ErrorCode::Unavailable);

    // URL outside the navigation policy (ftp).
    let err = owner
        .invoke(
            browser(),
            BROWSER_NAVIGATE_METHOD,
            navigate_request(&session_id, "ftp://nope.example"),
        )
        .expect_err("invalid url rejected");
    assert_eq!(err.code, ErrorCode::MalformedMessage);

    // Malformed requests: missing schemaVersion, empty message.
    let err = owner
        .invoke(
            browser(),
            BROWSER_NAVIGATE_METHOD,
            serde_json::json!({ "sessionId": session_id, "url": "https://example.com" }),
        )
        .expect_err("missing schemaVersion rejected");
    assert_eq!(err.code, ErrorCode::MalformedMessage);
    let err = owner
        .invoke(
            browser(),
            BROWSER_LOAD_FAILED_METHOD,
            load_failed_request(&session_id, "   "),
        )
        .expect_err("empty message rejected");
    assert_eq!(err.code, ErrorCode::MalformedMessage);

    // A second connection may not report on an owned session.
    let credential2 = _server.issue_credential(std::time::Duration::from_secs(300));
    let mut stranger = TestClient::connect_as(addr, &credential2, "other", "conn");
    stranger.negotiate(vec![browser()]);
    // A conflicting non-Shell participant is fail-closed at the broker
    // gate (no grant) before the owner check even runs.
    let err = stranger
        .invoke(
            browser(),
            BROWSER_NAVIGATE_METHOD,
            navigate_request(&session_id, "https://example.com/evil"),
        )
        .expect_err("stranger rejected");
    assert_eq!(err.code, ErrorCode::Unauthorized);
}

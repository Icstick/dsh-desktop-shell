//! Wire-level tests for the real Managed runtime capability (M6-C2):
//! the daemon **owns the DSH process tree** (ADR-0019 decision 3) and
//! exposes it over the envelope (runtime.start / runtime.status /
//! runtime.stop / runtime.restart), resolving environmentId against the
//! persisted catalog the Shell writes (M6-C2 decision: daemon reads
//! environment-catalog-v1.json from the daemon data directory; tests
//! isolate the catalog in a temp directory).
//!
//! The DSH child is a **real node process** (like the diagnostics
//! AC-LOG-001 fixture): it prints the `dsh web:` endpoint marker and
//! answers the HTTP readiness probe, so the full publication gate runs
//! against a real spawned tree — no re-exec trick, the daemon is a
//! separate process boundary.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use common::{TestClient, spawn_daemon_with_catalog};
use dsh_daemon::capabilities::{
    DAEMON_API_VERSION, DAEMON_KIND, DAEMON_STATUS_METHOD, RUNTIME_API_VERSION, RUNTIME_KIND,
    RUNTIME_RESTART_METHOD, RUNTIME_START_METHOD, RUNTIME_STATUS_METHOD, RUNTIME_STOP_METHOD,
};
use dsh_daemon::envelope::{ErrorCode, ProtocolCoordinate};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One temp catalog directory per test (removed on drop).
struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "dsh-runtime-it-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("create test directory");
        Self(dir)
    }

    fn catalog_path(&self) -> PathBuf {
        self.0.join("environment-catalog-v1.json")
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Resolve node.exe from PATH (the diagnostics fixture pattern).
fn node_executable() -> PathBuf {
    let path_var = std::env::var_os("PATH").expect("PATH");
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(if cfg!(windows) { "node.exe" } else { "node" });
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("node must be resolvable on PATH for the runtime integration test");
}

/// The fake-DSH child: a real node process that prints the endpoint
/// marker and answers the HTTP-level readiness probe (FM-1).
fn write_fake_dsh(dir: &Path) -> PathBuf {
    let script = dir.join("fake-dsh.js");
    fs::write(
        &script,
        r#"const net = require('net');
const server = net.createServer((socket) => {
  // The readiness probe drops its TCP stream with unread response bytes;
  // Windows may then RST the connection. Swallow the socket error so the
  // child never dies from the probe (uncaught ECONNRESET exits node).
  socket.on('error', () => {});
  socket.once('data', () => {
    socket.end('HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n');
  });
});
server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  console.log('dsh web: http://127.0.0.1:' + port + '/');
});
setInterval(() => {}, 1000);"#,
    )
    .expect("write fake dsh script");
    script
}

/// A managed repository environment referencing the fake-DSH script.
fn managed_environment(dir: &Path, id: &str, node: &Path) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "id": id,
        "label": format!("Managed {id}"),
        "harness": {
            "mode": "repository",
            "path": dir.join("fake-dsh.js"),
            "cwd": dir,
        },
        "dshHome": "C:/Users/example/.dsh",
        "profile": "default",
        "nodePath": node,
        "endpoint": { "host": "127.0.0.1", "port": "auto" },
        "ownership": "managed",
        "policy": { "autoRestartOnCrash": false },
    })
}

/// Persist the catalog the daemon resolves environments from.
fn write_catalog(dir: &TestDirectory, environments: Vec<serde_json::Value>) {
    let catalog = serde_json::json!({
        "schemaVersion": 1,
        "revision": 1,
        "activeEnvironmentId": environments[0]["id"],
        "environments": environments,
    });
    fs::write(
        dir.catalog_path(),
        serde_json::to_vec_pretty(&catalog).expect("catalog serializes"),
    )
    .expect("write catalog");
}

fn runtime() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: RUNTIME_API_VERSION.into(),
        kind: RUNTIME_KIND.into(),
    }
}

fn daemon() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: DAEMON_API_VERSION.into(),
        kind: DAEMON_KIND.into(),
    }
}

fn start_request(environment_id: &str) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "environmentId": environment_id,
    })
}

fn stop_request(environment_id: &str, generation: u64) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "environmentId": environment_id,
        "expectedGeneration": generation,
    })
}

/// 1) Full lifecycle over the wire with a real node child:
///    start → healthy → daemon.status counts it → restart (new
///    generation) → stop → stopped, endpoint released.
#[test]
fn managed_runtime_full_lifecycle_over_envelope() {
    let dir = TestDirectory::new();
    let node = node_executable();
    let script = write_fake_dsh(&dir.0);
    assert!(script.is_file());
    write_catalog(
        &dir,
        vec![managed_environment(&dir.0, "managed-local", &node)],
    );

    let (addr, credential, _server) = spawn_daemon_with_catalog(dir.catalog_path());
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![runtime(), daemon()]);

    // --- start ---
    let started = client
        .invoke(
            runtime(),
            RUNTIME_START_METHOD,
            start_request("managed-local"),
        )
        .expect("runtime.start succeeds");
    assert_eq!(started["schemaVersion"], 1);
    assert_eq!(started["environmentId"], "managed-local");
    assert_eq!(started["state"], "healthy");
    assert_eq!(started["ownership"], "managed");
    assert_eq!(started["processOwnership"], "owned");
    assert_eq!(started["readiness"], "verified");
    assert_eq!(started["lifecycleMutation"], "allowed");
    assert_eq!(started["stopDisposition"], "not_requested");
    let generation = started["generation"].as_u64().expect("generation");
    assert!(generation >= 1);
    let instance_id = started["instanceId"].as_str().expect("instanceId");
    assert!(instance_id.starts_with("managed-"));
    let port = started["endpoint"]["port"].as_u64().expect("endpoint port");
    assert!(port >= 1024);
    assert_eq!(started["endpoint"]["host"], "127.0.0.1");
    assert_eq!(started["endpoint"]["source"], "managed_process_output");
    // The public report never carries the bootstrap token.
    let serialized = serde_json::to_string(&started).expect("serialize");
    assert!(
        !serialized.contains("token="),
        "report must not leak the token"
    );
    assert!(!serialized.contains("?"));

    // --- status ---
    let status = client
        .invoke(
            runtime(),
            RUNTIME_STATUS_METHOD,
            start_request("managed-local"),
        )
        .expect("runtime.status succeeds");
    assert_eq!(
        status["state"],
        "healthy",
        "status report: {}",
        serde_json::to_string_pretty(&status).expect("serialize")
    );
    assert_eq!(status["generation"], generation);

    // --- daemon.status resource count ---
    let daemon_status = client
        .invoke(daemon(), DAEMON_STATUS_METHOD, serde_json::json!({}))
        .expect("daemon.status succeeds");
    assert_eq!(daemon_status["resources"]["managedRuntimes"], 1);

    // --- restart (new generation, new instance) ---
    let restarted = client
        .invoke(
            runtime(),
            RUNTIME_RESTART_METHOD,
            stop_request("managed-local", generation),
        )
        .expect("runtime.restart succeeds");
    assert_eq!(restarted["state"], "healthy");
    let restarted_generation = restarted["generation"].as_u64().expect("generation");
    assert!(restarted_generation > generation);
    assert_ne!(
        restarted["instanceId"].as_str(),
        Some(instance_id),
        "restart must create a new instance"
    );

    // --- stop (exact current generation) ---
    let stopped = client
        .invoke(
            runtime(),
            RUNTIME_STOP_METHOD,
            stop_request("managed-local", restarted_generation),
        )
        .expect("runtime.stop succeeds");
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["processOwnership"], "none");
    assert_eq!(stopped["readiness"], "not_started");
    assert!(stopped["endpoint"].is_null());
    assert!(stopped["instanceId"].is_null());

    // --- stopped status + resource count ---
    let status = client
        .invoke(
            runtime(),
            RUNTIME_STATUS_METHOD,
            start_request("managed-local"),
        )
        .expect("runtime.status after stop");
    assert_eq!(status["state"], "stopped");
    let daemon_status = client
        .invoke(daemon(), DAEMON_STATUS_METHOD, serde_json::json!({}))
        .expect("daemon.status after stop");
    assert_eq!(daemon_status["resources"]["managedRuntimes"], 0);
}

/// 2) Fail-closed authorization and validation: a connection without
///    the runtime grant cannot use it at all; malformed payloads are
///    MALFORMED_MESSAGE; unknown environments are UNAVAILABLE; stale
///    generations are STALE_GENERATION.
#[test]
fn runtime_authorization_and_validation_matrix() {
    let dir = TestDirectory::new();
    let node = node_executable();
    write_fake_dsh(&dir.0);
    // Two environments: managed-other exists in the catalog so the
    // supervisor conflict path (FM-4) is exercised, not the
    // environment-not-found path.
    write_catalog(
        &dir,
        vec![
            managed_environment(&dir.0, "managed-local", &node),
            managed_environment(&dir.0, "managed-other", &node),
        ],
    );

    let (addr, credential, server) = spawn_daemon_with_catalog(dir.catalog_path());

    // 2a) No runtime grant in the Agreement → UNAUTHORIZED (the server
    //     gate, before any handler runs). Local-transport credentials are
    //     single-use (AC-IPC-001), so the second client gets its own.
    let mut unauthenticated = TestClient::connect(addr, &credential);
    unauthenticated.negotiate(vec![]);
    let error = unauthenticated
        .invoke(
            runtime(),
            RUNTIME_START_METHOD,
            start_request("managed-local"),
        )
        .expect_err("runtime not granted");
    assert_eq!(error.code, ErrorCode::Unauthorized);

    let second_credential = server.issue_credential(Duration::from_secs(300));
    let mut client = TestClient::connect(addr, &second_credential);
    client.negotiate(vec![runtime()]);

    // 2b) Malformed payloads fail closed.
    let error = client
        .invoke(runtime(), RUNTIME_STATUS_METHOD, serde_json::json!({}))
        .expect_err("empty payload");
    assert_eq!(error.code, ErrorCode::MalformedMessage);
    let error = client
        .invoke(
            runtime(),
            RUNTIME_START_METHOD,
            serde_json::json!({ "schemaVersion": 2, "environmentId": "managed-local" }),
        )
        .expect_err("schema version 2");
    assert_eq!(error.code, ErrorCode::MalformedMessage);
    let error = client
        .invoke(
            runtime(),
            RUNTIME_START_METHOD,
            serde_json::json!({ "schemaVersion": 1, "environmentId": "Not Valid!" }),
        )
        .expect_err("bad environment id");
    assert_eq!(error.code, ErrorCode::MalformedMessage);
    let error = client
        .invoke(
            runtime(),
            RUNTIME_STOP_METHOD,
            serde_json::json!({
                "schemaVersion": 1,
                "environmentId": "managed-local",
                "expectedGeneration": 0,
            }),
        )
        .expect_err("generation zero");
    assert_eq!(error.code, ErrorCode::MalformedMessage);
    let error = client
        .invoke(runtime(), "runtime.shutdown", serde_json::json!({}))
        .expect_err("unknown method");
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(error.message.contains("not implemented"));

    // 2c) Well-formed request for an environment not in the catalog.
    let error = client
        .invoke(runtime(), RUNTIME_STATUS_METHOD, start_request("ghost-env"))
        .expect_err("unknown environment");
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(!error.retryable);

    // 2d) Stale generation stop on a running runtime.
    let started = client
        .invoke(
            runtime(),
            RUNTIME_START_METHOD,
            start_request("managed-local"),
        )
        .expect("start for stale test");
    let generation = started["generation"].as_u64().expect("generation");
    let error = client
        .invoke(
            runtime(),
            RUNTIME_STOP_METHOD,
            stop_request("managed-local", generation + 1),
        )
        .expect_err("stale generation stop");
    assert_eq!(error.code, ErrorCode::StaleGeneration);

    // 2e) Cross-environment conflict while one runtime is held.
    let error = client
        .invoke(
            runtime(),
            RUNTIME_STATUS_METHOD,
            start_request("managed-other"),
        )
        .expect_err("foreign environment status");
    assert_eq!(error.code, ErrorCode::Conflict);

    // Cleanup: exact-generation stop.
    client
        .invoke(
            runtime(),
            RUNTIME_STOP_METHOD,
            stop_request("managed-local", generation),
        )
        .expect("cleanup stop");
}

/// 3) A corrupt catalog fails closed: nothing can start from it.
#[test]
fn corrupt_catalog_fails_closed() {
    let dir = TestDirectory::new();
    fs::write(dir.catalog_path(), b"not a catalog").expect("corrupt catalog");

    let (addr, credential, _server) = spawn_daemon_with_catalog(dir.catalog_path());
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![runtime()]);

    let error = client
        .invoke(
            runtime(),
            RUNTIME_START_METHOD,
            start_request("managed-local"),
        )
        .expect_err("corrupt catalog must not launch");
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(!error.retryable);
    assert!(error.message.contains("catalog"));
}

/// 4) A missing catalog is an empty catalog: environments resolve to
///    UNAVAILABLE (never a silent launch from nothing).
#[test]
fn missing_catalog_is_empty_and_never_launches() {
    let dir = TestDirectory::new();
    let (addr, credential, _server) = spawn_daemon_with_catalog(dir.catalog_path());
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![runtime()]);

    let error = client
        .invoke(
            runtime(),
            RUNTIME_START_METHOD,
            start_request("managed-local"),
        )
        .expect_err("no catalog -> no environment");
    assert_eq!(error.code, ErrorCode::Unavailable);
}

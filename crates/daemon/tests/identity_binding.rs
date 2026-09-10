//! ADR-0021 (proposed) 的协议不变量测试：Shell–Daemon 身份绑定。
//!
//! 审计 H-2 的两处缺陷：`handle_hello` 的「请求什么授予什么」，以及
//! broker-relaxed human 路径由自报的 component/facet 字符串决定。
//! 本文件守护 option C 落地的两条不变量：
//!
//! 1. 一条连接只能建立一种 authority 类别（混用即 UNAUTHORIZED）；
//! 2. Shell 身份在**独立连接**上仍然拿到 broker-relaxed human 路径
//!    （Shell 重启场景不能被误伤）。
//!
//! 明确不守护的（ADR-0021 第 5 条）：持有 token 的同用户进程仍可自称
//! Shell。本测试不能、也不声称证明 H-2 已关闭。

mod common;

use common::{TestClient, spawn_daemon};
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_KIND, TERMINAL_API_VERSION, TERMINAL_KIND, TERMINAL_STATUS_METHOD,
};
use dsh_daemon::envelope::{EnvelopeKind, ErrorCode, ProtocolCoordinate};
use std::time::Duration;

fn terminal() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: TERMINAL_API_VERSION.into(),
        kind: TERMINAL_KIND.into(),
    }
}

fn browser() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: BROWSER_API_VERSION.into(),
        kind: BROWSER_KIND.into(),
    }
}

fn catalog() -> Vec<ProtocolCoordinate> {
    vec![terminal(), browser()]
}

/// ADR-0021 decision 2：非 Shell 身份不能借同一条连接取得 Shell 类别。
///
/// 反向顺序（先非 Shell 再自称 Shell）同样拒绝——这是「先以低权限连接、
/// 再升级为控制面」的路径，必须 fail closed。
#[test]
fn one_connection_cannot_mix_identity_classes() {
    let (addr, credential, _server) = spawn_daemon();

    // 1) A non-Shell participant establishes the connection's class.
    let mut agent = TestClient::connect_as(addr, &credential, "evil-agent", "automation");
    let agreement = agent.negotiate(catalog());
    assert_eq!(
        agreement.granted.len(),
        2,
        "a non-Shell participant is served"
    );

    // 2) The same connection now claims the Shell identity: rejected.
    agent.set_identity("dsh-desktop-shell", "shell");
    let (hello_id, reply) = agent.send_hello(catalog());
    assert_ne!(
        reply.kind,
        EnvelopeKind::Agreement,
        "a connection may not upgrade itself to the control plane"
    );
    let error = reply.error.as_ref().expect("rejection carries an error");
    assert_eq!(error.code, ErrorCode::Unauthorized);
    assert_eq!(error.correlation_id, hello_id, "correlation must match");
}

/// ADR-0021 decision 2/3：以 Shell 身份建立后，同一连接改用其它身份
/// 一律拒绝，且原 activation 不受影响。
#[test]
fn switching_identity_on_one_connection_is_unauthorized() {
    let (addr, credential, _server) = spawn_daemon();
    let mut shell = TestClient::connect_as(addr, &credential, "dsh-desktop-shell", "shell");

    let first = shell.negotiate(catalog());
    assert_eq!(first.granted.len(), 2);

    // The same connection now claims a different identity.
    shell.set_identity("evil-agent", "automation");
    let (hello_id, reply) = shell.send_hello(catalog());
    assert_ne!(
        reply.kind,
        EnvelopeKind::Agreement,
        "a mixed-identity Hello must not be agreed"
    );
    let error = reply.error.as_ref().expect("rejection carries an error");
    assert_eq!(error.code, ErrorCode::Unauthorized);
    assert_eq!(error.correlation_id, hello_id, "correlation must match");

    // The original activation still works (no collateral damage).
    shell.restore_identity();
    let status = shell
        .invoke(terminal(), TERMINAL_STATUS_METHOD, serde_json::json!({}))
        .expect("the first activation survives the rejected Hello");
    assert_eq!(status["count"], 0);
}

/// ADR-0021：Shell 重启（新的独立连接、新的凭据）必须仍然拿到
/// broker-relaxed human 路径——收紧身份绑定不能把正常重启误伤成故障。
#[test]
fn shell_restart_on_a_new_connection_keeps_the_human_path() {
    let (addr, credential, server) = spawn_daemon();

    let mut first = TestClient::connect_as(addr, &credential, "dsh-desktop-shell", "shell");
    assert_eq!(first.negotiate(catalog()).granted.len(), 2);

    // Shell restart: fresh credential, new connection, same identity.
    let restarted_credential = server.issue_credential(Duration::from_secs(300));
    let mut restarted =
        TestClient::connect_as(addr, &restarted_credential, "dsh-desktop-shell", "shell");
    let agreement = restarted.negotiate(catalog());
    assert_eq!(
        agreement.granted.len(),
        2,
        "a restarted Shell keeps the broker-relaxed human path"
    );
    let status = restarted
        .invoke(terminal(), TERMINAL_STATUS_METHOD, serde_json::json!({}))
        .expect("the restarted Shell dispatches");
    assert_eq!(status["count"], 0);
}

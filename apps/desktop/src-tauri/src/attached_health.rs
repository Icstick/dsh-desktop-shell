use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::commands::DshEnvironment;

pub(crate) const PROBE_TIMEOUT_MS: u64 = 750;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachedHealthRequest {
    schema_version: u8,
    environment_id: String,
}

impl AttachedHealthRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachedHealthReport {
    schema_version: u8,
    environment_id: String,
    ownership: &'static str,
    state: AttachedState,
    reachability: Reachability,
    identity: &'static str,
    process_ownership: &'static str,
    lifecycle_mutation: &'static str,
    endpoint: AttachedEndpoint,
    timeout_ms: u64,
    latency_ms: Option<u64>,
    observed_at_unix_ms: u64,
    evidence: Vec<HealthEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum AttachedState {
    Attached,
    Detached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Reachability {
    Reachable,
    Refused,
    Timeout,
    IoError,
}

#[derive(Debug, Clone, Serialize)]
struct AttachedEndpoint {
    host: &'static str,
    port: u16,
}

#[derive(Debug, Clone, Serialize)]
struct HealthEvidence {
    code: &'static str,
    severity: &'static str,
    message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachedHealthError {
    NotAttached,
    FixedPortRequired,
    ClockUnavailable,
}

pub(crate) fn probe_attached_environment(
    environment: &DshEnvironment,
) -> Result<AttachedHealthReport, AttachedHealthError> {
    probe_with_connector(environment, |address, timeout| {
        TcpStream::connect_timeout(&address, timeout).map(drop)
    })
}

fn probe_with_connector<Connector>(
    environment: &DshEnvironment,
    connector: Connector,
) -> Result<AttachedHealthReport, AttachedHealthError>
where
    Connector: FnOnce(SocketAddr, Duration) -> io::Result<()>,
{
    if !environment.is_attached() {
        return Err(AttachedHealthError::NotAttached);
    }
    let port = environment
        .fixed_loopback_port()
        .ok_or(AttachedHealthError::FixedPortRequired)?;
    let observed_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AttachedHealthError::ClockUnavailable)?
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    let timeout = Duration::from_millis(PROBE_TIMEOUT_MS);
    let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    let started = Instant::now();

    let (state, reachability, latency_ms, evidence) = match connector(address, timeout) {
        Ok(()) => (
            AttachedState::Attached,
            Reachability::Reachable,
            Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
            HealthEvidence {
                code: "TCP_REACHABLE_IDENTITY_UNVERIFIED",
                severity: "warning",
                message: "The loopback endpoint accepted a TCP connection; DSH identity and process ownership remain unverified.",
            },
        ),
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => (
            AttachedState::Detached,
            Reachability::Refused,
            None,
            HealthEvidence {
                code: "TCP_CONNECTION_REFUSED",
                severity: "error",
                message: "The persisted loopback endpoint refused the bounded TCP connection.",
            },
        ),
        Err(error) if error.kind() == io::ErrorKind::TimedOut => (
            AttachedState::Detached,
            Reachability::Timeout,
            None,
            HealthEvidence {
                code: "TCP_CONNECT_TIMEOUT",
                severity: "error",
                message: "The persisted loopback endpoint did not respond before the probe deadline.",
            },
        ),
        Err(_) => (
            AttachedState::Detached,
            Reachability::IoError,
            None,
            HealthEvidence {
                code: "TCP_CONNECT_IO_ERROR",
                severity: "error",
                message: "The persisted loopback endpoint could not be reached.",
            },
        ),
    };

    Ok(AttachedHealthReport {
        schema_version: 1,
        environment_id: environment.id().to_string(),
        ownership: "attached",
        state,
        reachability,
        identity: "unverified",
        process_ownership: "external",
        lifecycle_mutation: "denied",
        endpoint: AttachedEndpoint {
            host: "127.0.0.1",
            port,
        },
        timeout_ms: PROBE_TIMEOUT_MS,
        latency_ms,
        observed_at_unix_ms,
        evidence: vec![evidence],
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn environment(ownership: &str, port: serde_json::Value) -> DshEnvironment {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "attached-local",
            "label": "Attached DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": port },
            "ownership": ownership
        }))
        .expect("environment fixture")
    }

    #[test]
    fn reachable_endpoint_never_elevates_identity_or_ownership() {
        let report = probe_with_connector(
            &environment("attached", serde_json::json!(4317)),
            |address, timeout| {
                assert_eq!(address, "127.0.0.1:4317".parse().expect("socket address"));
                assert_eq!(timeout, Duration::from_millis(PROBE_TIMEOUT_MS));
                Ok(())
            },
        )
        .expect("probe report");

        assert_eq!(report.state, AttachedState::Attached);
        assert_eq!(report.reachability, Reachability::Reachable);
        assert_eq!(report.identity, "unverified");
        assert_eq!(report.process_ownership, "external");
        assert_eq!(report.lifecycle_mutation, "denied");
        assert!(report.latency_ms.is_some());
    }

    #[test]
    fn refusal_is_detached_without_raw_os_error() {
        let report =
            probe_with_connector(&environment("attached", serde_json::json!(4317)), |_, _| {
                Err(io::Error::from(io::ErrorKind::ConnectionRefused))
            })
            .expect("probe report");

        assert_eq!(report.state, AttachedState::Detached);
        assert_eq!(report.reachability, Reachability::Refused);
        assert!(report.latency_ms.is_none());
        assert_eq!(report.evidence[0].code, "TCP_CONNECTION_REFUSED");
    }

    #[test]
    fn timeout_is_bounded_and_detached() {
        let report = probe_with_connector(
            &environment("attached", serde_json::json!(4317)),
            |_, timeout| {
                assert_eq!(timeout, Duration::from_millis(750));
                Err(io::Error::from(io::ErrorKind::TimedOut))
            },
        )
        .expect("probe report");

        assert_eq!(report.state, AttachedState::Detached);
        assert_eq!(report.reachability, Reachability::Timeout);
    }

    #[test]
    fn managed_environment_is_rejected_before_connect() {
        let called = Cell::new(false);
        let result =
            probe_with_connector(&environment("managed", serde_json::json!(4317)), |_, _| {
                called.set(true);
                Ok(())
            });
        assert!(matches!(result, Err(AttachedHealthError::NotAttached)));
        assert!(!called.get());
    }

    #[test]
    fn auto_port_is_rejected_without_scanning() {
        let called = Cell::new(false);
        let result = probe_with_connector(
            &environment("attached", serde_json::json!("auto")),
            |_, _| {
                called.set(true);
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(AttachedHealthError::FixedPortRequired)
        ));
        assert!(!called.get());
    }
}

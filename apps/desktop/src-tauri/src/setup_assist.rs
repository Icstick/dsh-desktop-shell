//! Setup-wizard assistance facts (M7-A, WI-M7-SETUP-WIZARD): profile
//! scanning and port probing.
//!
//! The wizard guides the user through Managed/Attached environment setup;
//! these helpers supply the discoverable facts: which DSH profiles exist
//! under a `dshHome` and whether a candidate port is already in use.
//!
//! Profile layout: DSH keeps profiles in `$DSH_HOME/profiles/<name>/`
//! (each profile directory carries a `cordis.yml` root config; the home
//! level has its own `cordis.patch.yml`). A profile entry is reported for
//! every directory under `profiles/`, with a flag whether the root config
//! file exists (a directory without it may be a partial/stale profile).
//!
//! Port probing: a bounded loopback TCP connect against 127.0.0.1 (a
//! listening daemon answers SYN with SYN-ACK; nothing listening refuses).

use std::fs;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Schema version of the request/report shapes.
pub const SCHEMA_VERSION: u8 = 1;
/// Sub-directory of `dshHome` holding the profile directories.
pub const PROFILE_DIR_NAME: &str = "profiles";
/// Root config filename inside a profile directory (dsh profile-boot).
pub const PROFILE_ROOT_FILE: &str = "cordis.yml";
/// Upper bound on reported profiles (a malicious/broken tree cannot blow
/// up the report).
pub const MAX_PROFILES: usize = 64;
/// Upper bound on a dshHome path length.
pub const MAX_HOME_LENGTH: usize = 4096;
/// Bounded connect attempt for the port probe.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// `discover_profiles` request (dshHome to scan).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoverProfilesRequest {
    pub schema_version: u8,
    pub dsh_home: String,
}

/// One discovered profile directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEntry {
    pub name: String,
    pub path: String,
    /// Whether `cordis.yml` exists in the profile directory.
    pub has_root_config: bool,
}

/// `discover_profiles` report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverProfilesReport {
    pub schema_version: u8,
    pub dsh_home: String,
    pub profiles: Vec<ProfileEntry>,
}

/// `probe_port` request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbePortRequest {
    pub schema_version: u8,
    pub port: u16,
}

/// `probe_port` report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbePortReport {
    pub schema_version: u8,
    pub port: u16,
    pub in_use: bool,
}

/// Failures of the setup-assist helpers (mirrors DiscoveryError style).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupAssistError {
    MalformedRequest,
    /// The dshHome directory does not exist.
    HomeMissing,
    /// The dshHome path is not a directory.
    HomeNotDirectory,
    /// Reading the profiles directory failed.
    Io(String),
}

impl std::fmt::Display for SetupAssistError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedRequest => write!(formatter, "malformed setup-assist request"),
            Self::HomeMissing => write!(formatter, "the dshHome directory does not exist"),
            Self::HomeNotDirectory => {
                write!(formatter, "the dshHome path is not a directory")
            }
            Self::Io(message) => write!(formatter, "cannot read the profiles directory: {message}"),
        }
    }
}

impl std::error::Error for SetupAssistError {}

impl From<io::Error> for SetupAssistError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Validate a profile entry name (directory basename): non-empty, no path
/// separators, bounded length, no dots-only names.
fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.chars().count() <= 128
        && !name.contains('/')
        && !name.contains('\\')
}

/// Scan `<dsh_home>/profiles/` and report every directory entry.
pub fn discover_profiles(
    request: &DiscoverProfilesRequest,
) -> Result<DiscoverProfilesReport, SetupAssistError> {
    if request.schema_version != SCHEMA_VERSION
        || request.dsh_home.trim().is_empty()
        || request.dsh_home.chars().count() > MAX_HOME_LENGTH
    {
        return Err(SetupAssistError::MalformedRequest);
    }
    let home = Path::new(request.dsh_home.trim());
    if !home.exists() {
        return Err(SetupAssistError::HomeMissing);
    }
    if !home.is_dir() {
        return Err(SetupAssistError::HomeNotDirectory);
    }
    let profiles_dir = home.join(PROFILE_DIR_NAME);
    let mut profiles: Vec<ProfileEntry> = Vec::new();
    if profiles_dir.is_dir() {
        for entry in fs::read_dir(&profiles_dir)? {
            if profiles.len() >= MAX_PROFILES {
                break;
            }
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !is_valid_profile_name(name) {
                continue;
            }
            profiles.push(ProfileEntry {
                name: name.to_string(),
                path: path_string(&path),
                has_root_config: path.join(PROFILE_ROOT_FILE).is_file(),
            });
        }
        profiles.sort_by(|left, right| left.name.cmp(&right.name));
    }
    Ok(DiscoverProfilesReport {
        schema_version: SCHEMA_VERSION,
        dsh_home: request.dsh_home.trim().to_string(),
        profiles,
    })
}

/// Probe whether a loopback port is in use (bounded connect; a refused
/// connection means the port is free).
pub fn probe_port(request: &ProbePortRequest) -> Result<ProbePortReport, SetupAssistError> {
    if request.schema_version != SCHEMA_VERSION {
        return Err(SetupAssistError::MalformedRequest);
    }
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, request.port));
    let in_use = TcpStream::connect_timeout(&address, PROBE_TIMEOUT).is_ok();
    Ok(ProbePortReport {
        schema_version: SCHEMA_VERSION,
        port: request.port,
        in_use,
    })
}

/// Lossless path string for the report (falls back to the lossy form).
fn path_string(path: &Path) -> String {
    path.to_str()
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("dsh-setup-assist-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp home");
        (dir.clone(), dir.join(PROFILE_DIR_NAME))
    }

    #[test]
    fn discovers_profiles_with_root_config_flag() {
        let (home, profiles) = temp_home("discover");
        fs::create_dir_all(profiles.join("default")).expect("profile dir");
        fs::write(
            profiles.join("default").join(PROFILE_ROOT_FILE),
            "plugins: []",
        )
        .expect("root config");
        fs::create_dir_all(profiles.join("work")).expect("profile dir");
        // work has no cordis.yml → partial profile
        fs::write(home.join("not-a-profile"), "file").expect("ignored file");

        let report = discover_profiles(&DiscoverProfilesRequest {
            schema_version: 1,
            dsh_home: home.to_string_lossy().into_owned(),
        })
        .expect("discover");

        assert_eq!(report.profiles.len(), 2);
        let default = report
            .profiles
            .iter()
            .find(|p| p.name == "default")
            .expect("default");
        assert!(default.has_root_config);
        assert!(
            default.path.ends_with("default") && default.path.contains("profiles"),
            "path must point at <home>/profiles/default: {}",
            default.path
        );
        let work = report
            .profiles
            .iter()
            .find(|p| p.name == "work")
            .expect("work");
        assert!(!work.has_root_config);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn missing_home_is_reported_not_fatal() {
        let error = discover_profiles(&DiscoverProfilesRequest {
            schema_version: 1,
            dsh_home: "C:\\no-such-dsh-home-xyz".into(),
        })
        .expect_err("missing home");
        assert_eq!(error, SetupAssistError::HomeMissing);
    }

    #[test]
    fn malformed_request_rejected() {
        assert_eq!(
            discover_profiles(&DiscoverProfilesRequest {
                schema_version: 2,
                dsh_home: "C:\\dsh".into(),
            })
            .expect_err("schema version"),
            SetupAssistError::MalformedRequest
        );
        assert_eq!(
            discover_profiles(&DiscoverProfilesRequest {
                schema_version: 1,
                dsh_home: "   ".into(),
            })
            .expect_err("empty home"),
            SetupAssistError::MalformedRequest
        );
    }

    #[test]
    fn no_profiles_dir_yields_empty_report() {
        let (home, _) = temp_home("empty");
        let report = discover_profiles(&DiscoverProfilesRequest {
            schema_version: 1,
            dsh_home: home.to_string_lossy().into_owned(),
        })
        .expect("discover");
        assert!(report.profiles.is_empty());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn probe_port_detects_listener_and_free_port() {
        // Bind a listener on an ephemeral port → probe says in use.
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let report = probe_port(&ProbePortRequest {
            schema_version: 1,
            port,
        })
        .expect("probe");
        assert!(report.in_use, "bound port must be reported in use");

        // A port from the ephemeral range with no listener is free (the
        // bind above just released nothing; pick a fresh one).
        let free = {
            let probe = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
            let port = probe.local_addr().expect("addr").port();
            drop(probe);
            port
        };
        let report = probe_port(&ProbePortRequest {
            schema_version: 1,
            port: free,
        })
        .expect("probe");
        assert!(!report.in_use, "unbound port must be reported free");
    }

    #[test]
    fn probe_port_rejects_bad_schema() {
        assert_eq!(
            probe_port(&ProbePortRequest {
                schema_version: 2,
                port: 8080,
            })
            .expect_err("schema"),
            SetupAssistError::MalformedRequest
        );
    }
}

//! Example capability catalog shared by the client (Hello.supports) and the
//! server (grant policy + dispatch).
//!
//! Coordinates follow the fixture convention
//! (`<domain>.dsh-desktop.local/v1alpha1` + PascalCase kind, see
//! `specs/protocol/fixtures/envelope.hello.valid.json`).

use crate::envelope::ProtocolCoordinate;

/// Capability `system` (method `ping`) — the reference health check.
pub const SYSTEM_API_VERSION: &str = "system.dsh-desktop.local/v1alpha1";
pub const SYSTEM_KIND: &str = "System";
pub const SYSTEM_PING_METHOD: &str = "ping";

/// Capability `browser` (method `list_browsers`) — static catalog demo.
pub const BROWSER_API_VERSION: &str = "browser.dsh-desktop.local/v1alpha1";
pub const BROWSER_KIND: &str = "Browser";
pub const BROWSER_LIST_METHOD: &str = "list_browsers";

/// The `system` capability coordinate.
pub fn system() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: SYSTEM_API_VERSION.into(),
        kind: SYSTEM_KIND.into(),
    }
}

/// The `browser` capability coordinate.
pub fn browser() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: BROWSER_API_VERSION.into(),
        kind: BROWSER_KIND.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_coordinates_are_stable() {
        assert_eq!(system().api_version, "system.dsh-desktop.local/v1alpha1");
        assert_eq!(system().kind, "System");
        assert_eq!(browser().api_version, "browser.dsh-desktop.local/v1alpha1");
        assert_eq!(browser().kind, "Browser");
    }
}

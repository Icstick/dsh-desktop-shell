//! Minimal dsh-std facet model.
//!
//! INFERENCE NOTE (M5-D): the exact dsh-std facet semantics could not be
//! verified in this session - the pinned README (dsh-std bb194ad... blob
//! d9db9823...) and the npm registry are not reachable from this build
//! sandbox, and dsh-std still marks everything as early drafts. The minimal
//! set below is inferred from this repository's frozen documents:
//! docs/compatibility/LADDER.md (L2 = "negotiation/facets/conformance") and
//! docs/compatibility/DSH_STD_POLICY.md (meta-protocol, independently
//! versioned packages, requires/supports, adapter, activation ownership).
//! Re-verify against the pinned README before relying on facet
//! compatibility for real peers.

/// Facet kinds this adapter models on the Desktop side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FacetKind {
    /// Hello/Agreement exchange with per-activation ownership.
    Negotiation,
    /// Pinned-coordinate declaration (see `conformance`).
    Conformance,
    /// Invocation/Result/Event over granted capabilities.
    Invocation,
}

impl FacetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Negotiation => "negotiation",
            Self::Conformance => "conformance",
            Self::Invocation => "invocation",
        }
    }

    /// Unknown facet strings fail closed: they are never auto-granted.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "negotiation" => Some(Self::Negotiation),
            "conformance" => Some(Self::Conformance),
            "invocation" => Some(Self::Invocation),
            _ => None,
        }
    }
}

/// One facet of a component, with its own (independently versioned)
/// apiVersion, mirroring dsh-std's independent package versioning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facet {
    pub kind: FacetKind,
    pub api_version: String,
}

impl Facet {
    pub fn new(kind: FacetKind, api_version: impl Into<String>) -> Self {
        Self {
            kind,
            api_version: api_version.into(),
        }
    }

    /// Stable identifier: `<kind>/<apiVersion>`.
    pub fn id(&self) -> String {
        format!("{}/{}", self.kind.as_str(), self.api_version)
    }

    /// Compatibility = same kind + same apiVersion (fail closed otherwise).
    pub fn is_compatible_with(&self, other: &Facet) -> bool {
        self.kind == other.kind && self.api_version == other.api_version
    }
}

/// The local (Desktop-side) facet catalog of this adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FacetCatalog {
    facets: Vec<Facet>,
}

impl FacetCatalog {
    /// The minimal desktop-side catalog. The apiVersion namespace
    /// (`dsh-std.dsh-desktop.local/v1alpha1`) is inferred: the envelope
    /// protocol itself is `interop.dsh-desktop.local/v1alpha1`; a dsh-std
    /// facet surface would be declared on top of it once the wire is stable.
    pub fn local_default() -> Self {
        let api = "dsh-std.dsh-desktop.local/v1alpha1";
        Self {
            facets: vec![
                Facet::new(FacetKind::Negotiation, api),
                Facet::new(FacetKind::Conformance, api),
                Facet::new(FacetKind::Invocation, api),
            ],
        }
    }

    pub fn facets(&self) -> &[Facet] {
        &self.facets
    }

    /// Whether a peer facet is supported (kind + exact apiVersion).
    pub fn supports(&self, facet: &Facet) -> bool {
        self.facets.iter().any(|f| f.is_compatible_with(facet))
    }

    /// The subset of peer facets this catalog supports.
    pub fn intersect(&self, peer: &[Facet]) -> Vec<Facet> {
        peer.iter().filter(|f| self.supports(f)).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const API: &str = "dsh-std.dsh-desktop.local/v1alpha1";

    #[test]
    fn unknown_facet_kind_fails_closed() {
        assert_eq!(FacetKind::parse("telepathy"), None);
        assert_eq!(FacetKind::parse(""), None);
    }

    #[test]
    fn known_facet_kinds_roundtrip() {
        for kind in [
            FacetKind::Negotiation,
            FacetKind::Conformance,
            FacetKind::Invocation,
        ] {
            assert_eq!(FacetKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn local_catalog_supports_its_own_facets() {
        let catalog = FacetCatalog::local_default();
        assert_eq!(catalog.facets().len(), 3);
        for facet in catalog.facets() {
            assert!(catalog.supports(facet));
        }
    }

    #[test]
    fn version_mismatch_is_incompatible() {
        let catalog = FacetCatalog::local_default();
        let older = Facet::new(FacetKind::Negotiation, "dsh-std.dsh-desktop.local/v0alpha1");
        assert!(!catalog.supports(&older));
    }

    #[test]
    fn intersect_grants_only_supported_facets() {
        let catalog = FacetCatalog::local_default();
        let peer = vec![
            Facet::new(FacetKind::Negotiation, API),
            Facet::new(FacetKind::Conformance, API),
            Facet::new(FacetKind::Invocation, "dsh-std.dsh-desktop.local/v0alpha1"),
            Facet::new(FacetKind::Negotiation, "other.local/v1"),
        ];
        let common = catalog.intersect(&peer);
        assert_eq!(common.len(), 2);
        assert!(common.iter().all(|f| catalog.supports(f)));
        assert!(common.iter().all(|f| f.api_version == API));
    }
}

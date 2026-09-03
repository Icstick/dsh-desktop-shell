//! # dsh-managed-runtime
//!
//! The Managed DSH runtime core (MOD-PROCESS-MANAGER), extracted from
//! the tauri Shell into a standalone, tauri-free crate in M6-C2
//! (ADR-0019 decision 3: the DSH process tree migrates into the daemon;
//! ADR-0008 through-crate evolution).
//!
//! - supervisor — the Supervisor state machine: one DSH process tree
//!   at a time, generation-guarded lifecycle, readiness publication
//!   gates, bounded crash recovery, Windows Job Object / unix process
//!   group ownership.
//! - environment — the persisted environment catalog
//!   (environment-catalog-v1.json, M1 contract): the daemon resolves
//!   environmentId against it, the Shell remains the writer.
//!
//! The crate has no tauri dependency; it is shared by the daemon
//! (resource owner since M6-C2) and the Shell (thin wrapper until M6-C4).

pub mod environment;
pub mod supervisor;

pub use environment::{
    CATALOG_FILE_NAME, CatalogError, EnvironmentCatalog, HarnessMode, ManagedEnvironment,
    is_valid_id, load_catalog,
};
pub use supervisor::{
    LaunchSpec, ManagedRuntimeError, ManagedRuntimeBindingRequest, ManagedRuntimeReport, ManagedRuntimeRestartRequest,
    ManagedRuntimeStartRequest, ManagedRuntimeState, ManagedRuntimeStatusRequest,
    ManagedRuntimeStopRequest, VerifiedSurfaceBinding, get_managed_runtime_status,
    restart_managed_environment, start_managed_environment, start_with_spec,
    stop_managed_environment, verified_surface_binding,
};

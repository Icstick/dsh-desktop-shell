const COMMANDS: &[&str] = &[
    "discover_harnesses",
    "evaluate_dsh_surface_navigation",
    "get_dsh_surface_status",
    "get_dsh_surface_policy",
    "get_environment_catalog",
    "get_managed_runtime_status",
    "get_shell_snapshot",
    "mount_dsh_surface",
    "probe_attached_environment",
    "reload_dsh_surface",
    "save_environment",
    "start_managed_environment",
    "stop_managed_environment",
    "unmount_dsh_surface",
    "update_dsh_surface_layout",
    "validate_environment",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}

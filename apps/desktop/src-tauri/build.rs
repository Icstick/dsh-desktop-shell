const COMMANDS: &[&str] = &[
    "close_terminal",
    "create_terminal",
    "dismiss_notification",
    "discover_harnesses",
    "evaluate_dsh_surface_navigation",
    "get_dsh_surface_status",
    "get_dsh_surface_policy",
    "get_environment_catalog",
    "get_diagnostics",
    "get_managed_runtime_status",
    "get_shell_snapshot",
    "get_usage_snapshot",
    "mount_dsh_surface",
    "probe_attached_environment",
    "reload_dsh_surface",
    "save_environment",
    "list_terminals",
    "list_notifications",
    "notify_application",
    "resize_terminal",
    "start_managed_environment",
    "restart_managed_environment",
    "status_terminal",
    "stop_managed_environment",
    "write_terminal",
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

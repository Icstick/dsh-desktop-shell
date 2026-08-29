mod attached_health;
mod commands;
mod diagnostics;
mod discovery;
mod dsh_surface;
mod dsh_surface_policy;
mod environment_store;
mod managed_runtime;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(managed_runtime::ManagedRuntimeState::default())
        .manage(dsh_surface::DshSurfaceState::default())
        .invoke_handler(tauri::generate_handler![
            commands::discover_harnesses,
            commands::evaluate_dsh_surface_navigation,
            commands::get_dsh_surface_status,
            commands::get_dsh_surface_policy,
            commands::get_environment_catalog,
            commands::get_diagnostics,
            commands::get_managed_runtime_status,
            commands::get_shell_snapshot,
            commands::mount_dsh_surface,
            commands::probe_attached_environment,
            commands::reload_dsh_surface,
            commands::save_environment,
            commands::restart_managed_environment,
            commands::start_managed_environment,
            commands::stop_managed_environment,
            commands::unmount_dsh_surface,
            commands::update_dsh_surface_layout,
            commands::validate_environment
        ])
        .run(tauri::generate_context!())
        .expect("error while running DSH Desktop Shell");
}

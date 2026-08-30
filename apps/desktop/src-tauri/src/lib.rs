use tauri::Manager;

mod attached_health;
mod browser;
mod commands;
mod diagnostics;
mod discovery;
mod dsh_surface;
mod dsh_surface_policy;
mod environment_store;
mod managed_runtime;
mod notification;
mod terminal;
mod usage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(managed_runtime::ManagedRuntimeState::default())
        .manage(dsh_surface::DshSurfaceState::default())
        .manage(terminal::TerminalState::default())
        .manage(browser::BrowserState::default())
        .manage(notification::NotificationService::default())
        .manage(usage::UsageService::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<terminal::TerminalState>();
            terminal::start_event_drain(handle.clone(), state.inner().clone());
            let browser_state = app.state::<browser::BrowserState>();
            browser::start_event_drain(handle, browser_state.inner().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::discover_harnesses,
            commands::evaluate_dsh_surface_navigation,
            commands::get_dsh_surface_status,
            commands::get_dsh_surface_policy,
            commands::get_environment_catalog,
            commands::get_diagnostics,
            commands::get_managed_runtime_status,
            commands::get_shell_snapshot,
            commands::get_usage_snapshot,
            commands::mount_dsh_surface,
            commands::probe_attached_environment,
            commands::close_browser,
            commands::close_terminal,
            commands::create_browser,
            commands::create_terminal,
            commands::list_browsers,
            commands::list_terminals,
            commands::navigate_browser,
            commands::snapshot_browser,
            commands::dismiss_notification,
            commands::list_notifications,
            commands::notify_application,
            commands::reload_dsh_surface,
            commands::resize_terminal,
            commands::save_environment,
            commands::status_terminal,
            commands::write_terminal,
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

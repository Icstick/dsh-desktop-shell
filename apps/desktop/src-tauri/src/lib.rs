use tauri::Manager;

mod agent_broker;
mod attached_health;
mod browser;
mod commands;
mod daemon_client;
mod diagnostics;
mod discovery;
mod dsh_surface;
mod dsh_surface_policy;
mod environment_store;
mod managed_runtime;
mod notification;
mod setup_assist;
mod terminal;
mod usage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // One capability broker shared by every agent_automation surface
    // (ADR-0018 decision 7); the terminal agent wiring (TerminalState::new
    // with a registered provider) lands with M5-E2 in the same branch.
    let broker_state = agent_broker::BrokerState::default();
    let broker_inner = broker_state.inner();
    let browser_state = browser::BrowserState::new(broker_inner);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(broker_state)
        .manage(daemon_client::DaemonClientState::new())
        .manage(dsh_surface::DshSurfaceState::default())
        .manage(browser_state)
        .manage(notification::NotificationService::default())
        .manage(usage::UsageService::default())
        .setup(|app| {
            // M6-C4: the PTY/DSH resources live in the daemon; the Shell
            // connects in the background (probe -> spawn -> credential ->
            // envelope) and bridges daemon events onto the frontend events.
            // The Shell never disconnects the daemon on close (ADR-0008).
            let handle = app.handle().clone();
            let browser_state = app.state::<browser::BrowserState>();
            browser::start_event_drain(handle.clone(), browser_state.inner().clone());
            let daemon_state = app.state::<daemon_client::DaemonClientState>();
            daemon_client::start_background(
                handle,
                daemon_state.inner().clone(),
                daemon_client::StartupOptions::default(),
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::discover_harnesses,
            commands::discover_profiles,
            commands::set_active_environment,
            commands::probe_port,
            commands::evaluate_dsh_surface_navigation,
            commands::get_dsh_surface_status,
            commands::get_dsh_surface_policy,
            commands::get_environment_catalog,
            commands::get_diagnostics,
            commands::get_managed_runtime_status,
            commands::get_shell_snapshot,
            commands::get_usage_snapshot,
            commands::mount_dsh_surface,
            commands::pick_directory,
            commands::probe_attached_environment,
            commands::close_browser,
            commands::close_terminal,
            commands::create_browser,
            commands::create_terminal,
            commands::interact_browser,
            commands::take_over_browser,
            commands::list_browsers,
            commands::list_terminals,
            commands::navigate_browser,
            commands::snapshot_browser,
            commands::dismiss_notification,
            commands::list_notifications,
            commands::notify_application,
            commands::reload_dsh_surface,
            commands::resize_terminal,
            commands::remove_environment,
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

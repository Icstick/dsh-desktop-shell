// GUI application: never attach a console window, in dev or release.
// (Rust defaults to the console subsystem; tauri apps must opt into the
// Windows GUI subsystem. Dev logging can go through a log file instead.)
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    dsh_desktop_shell_lib::run();
}

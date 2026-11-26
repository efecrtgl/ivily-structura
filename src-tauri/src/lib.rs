//! Ivily Structura backend.
//!
//! See `docs/ARCHITECTURE.md` for the module layout. Analysis modules (`scan`, `lang`,
//! `providers`, `project`, ...) are plain Rust and do not depend on Tauri.

pub mod core;
pub mod error;
pub mod iep;
pub mod lang;
pub mod project;
pub mod providers;
pub mod scan;

pub const APP_NAME: &str = "Ivily Structura";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("failed to start Ivily Structura");
}

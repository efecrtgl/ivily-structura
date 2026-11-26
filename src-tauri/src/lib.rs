//! Ivily Structura backend.
//!
//! See `docs/ARCHITECTURE.md` for the module layout. Analysis modules (`scan`, `lang`,
//! `providers`, `project`, `deps`, `graph`, `health`, ...) are plain Rust and do not depend on
//! Tauri; `commands` is the only IPC surface.

pub mod analysis;
pub mod commands;
pub mod compare;
pub mod core;
pub mod db;
pub mod deps;
pub mod error;
pub mod graph;
pub mod health;
pub mod iep;
pub mod lang;
pub mod project;
pub mod providers;
pub mod report;
pub mod scan;
pub mod state;
pub mod store;

pub const APP_NAME: &str = "Ivily Structura";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// First command-line argument that is an existing directory (ivily-structura.exe C:\code\app).
pub fn startup_path<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    args.into_iter().skip(1).filter(|a| !a.starts_with('-')).find(|a| std::path::Path::new(a).is_dir())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            use tauri::{Emitter, Manager};
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            // A second launch with a folder argument opens that project in the running window.
            if let Some(p) = startup_path(args) {
                let _ = app.emit("open-path", p);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state::AppState::init())
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::settings_get,
            commands::app::settings_save,
            commands::app::db_status,
            commands::app::db_backup,
            commands::app::db_restore,
            commands::app::profile_list,
            commands::app::profile_save,
            commands::app::profile_delete,
            commands::app::history_list,
            commands::app::history_clear,
            commands::app::update_check,
            commands::app::update_install,
            commands::analysis::project_open,
            commands::analysis::project_list,
            commands::analysis::project_get,
            commands::analysis::project_update,
            commands::analysis::project_delete,
            commands::analysis::analysis_start,
            commands::analysis::analysis_cancel,
            commands::analysis::explorer_list,
            commands::views::analysis_overview,
            commands::views::analysis_files,
            commands::views::analysis_findings,
            commands::views::analysis_tree,
            commands::views::file_detail,
            commands::views::graph_view,
            commands::views::dependency_view,
            commands::data::snapshot_list,
            commands::data::snapshot_create,
            commands::data::snapshot_update,
            commands::data::snapshot_delete,
            commands::data::snapshot_open,
            commands::data::snapshot_export,
            commands::data::snapshot_import,
            commands::data::compare_analyses,
            commands::data::mapping_get,
            commands::data::annotation_set,
            commands::data::group_save,
            commands::data::group_delete,
            commands::data::layers_set,
            commands::data::relation_add,
            commands::data::relation_delete,
            commands::data::note_save,
            commands::data::note_delete,
            commands::data::mapping_export,
            commands::data::mapping_import,
            commands::data::report_preview,
            commands::data::report_export,
            commands::data::file_export,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Ivily Structura");
}

//! App-level commands: info, settings, database, profiles, history, updates.

use super::user_file;
use crate::analysis::profile::AnalysisProfile;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::{history, profiles, settings};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
    engine_version: u32,
    data_dir: String,
    degraded: Option<String>,
    languages: Vec<crate::lang::LanguageInfo>,
    rules: Vec<crate::health::RuleInfo>,
    report_sections: &'static [&'static str],
    csv_tables: &'static [&'static str],
    iep_version: &'static str,
}

#[tauri::command]
pub fn app_info(state: State<AppState>) -> AppInfo {
    AppInfo {
        name: crate::APP_NAME,
        version: crate::APP_VERSION,
        engine_version: crate::analysis::model::ENGINE_VERSION,
        data_dir: crate::core::data_dir().to_string_lossy().into_owned(),
        degraded: state.degraded.clone(),
        languages: crate::lang::registry(),
        rules: crate::health::registry(),
        report_sections: crate::report::ALL_SECTIONS,
        csv_tables: crate::report::CSV_TABLES,
        iep_version: crate::iep::PROTOCOL_VERSION,
    }
}

#[tauri::command]
pub fn settings_get(state: State<AppState>) -> AppResult<settings::Settings> {
    settings::get(&state.db)
}

#[tauri::command]
pub fn settings_save(state: State<AppState>, settings: settings::Settings) -> AppResult<settings::Settings> {
    let before = settings::get(&state.db)?;
    let saved = settings::save(&state.db, &settings)?;
    if before.history_retention_days != saved.history_retention_days {
        history::prune(&state.db, saved.history_retention_days)?;
    }
    let _ = history::record(
        &state.db,
        history::NewEntry {
            kind: "config.settings",
            outcome: "success",
            project_id: None,
            profile_id: None,
            duration_ms: None,
            title: "Settings changed".into(),
            detail: serde_json::json!({}),
        },
    );
    Ok(saved)
}

#[tauri::command]
pub fn db_status(state: State<AppState>) -> AppResult<crate::db::DbStatus> {
    state.db.status()
}

#[tauri::command]
pub fn db_backup(state: State<AppState>, dest: String) -> AppResult<()> {
    let p = user_file(&dest, &["db", "sqlite", "bak"], false)?;
    let r = state.db.backup_to(&p);
    let _ = history::record(
        &state.db,
        history::NewEntry {
            kind: "data.backup",
            outcome: if r.is_ok() { "success" } else { "failure" },
            project_id: None,
            profile_id: None,
            duration_ms: None,
            title: "Database backup".into(),
            detail: serde_json::json!({}),
        },
    );
    r
}

#[tauri::command]
pub fn db_restore(state: State<AppState>, src: String) -> AppResult<()> {
    let p = user_file(&src, &["db", "sqlite", "bak"], true)?;
    // Safety copy of the current data before replacing it.
    let safety = crate::core::data_dir().join(format!("before-restore-{}.db", crate::core::now_ts()));
    state.db.backup_to(&safety)?;
    state.db.restore_from(&p)?;
    let _ = history::record(
        &state.db,
        history::NewEntry {
            kind: "data.restore",
            outcome: "success",
            project_id: None,
            profile_id: None,
            duration_ms: None,
            title: "Database restored from backup".into(),
            detail: serde_json::json!({}),
        },
    );
    Ok(())
}

#[tauri::command]
pub fn profile_list(state: State<AppState>) -> AppResult<Vec<AnalysisProfile>> {
    profiles::list(&state.db)
}

#[tauri::command]
pub fn profile_save(state: State<AppState>, profile: AnalysisProfile) -> AppResult<AnalysisProfile> {
    let p = profiles::save(&state.db, &profile)?;
    let _ = history::record(
        &state.db,
        history::NewEntry {
            kind: "config.profile",
            outcome: "success",
            project_id: None,
            profile_id: Some(&p.id),
            duration_ms: None,
            title: format!("Profile \"{}\" saved", p.name),
            detail: serde_json::json!({}),
        },
    );
    Ok(p)
}

#[tauri::command]
pub fn profile_delete(state: State<AppState>, id: String) -> AppResult<()> {
    profiles::delete(&state.db, &id)?;
    let _ = history::record(
        &state.db,
        history::NewEntry {
            kind: "config.profile",
            outcome: "success",
            project_id: None,
            profile_id: Some(&id),
            duration_ms: None,
            title: "Profile deleted".into(),
            detail: serde_json::json!({}),
        },
    );
    Ok(())
}

#[tauri::command]
pub fn history_list(state: State<AppState>, query: history::Query) -> AppResult<Vec<history::Entry>> {
    history::list(&state.db, &query)
}

#[tauri::command]
pub fn history_clear(state: State<AppState>) -> AppResult<usize> {
    history::clear(&state.db)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    available: bool,
    current_version: String,
    version: Option<String>,
    notes: Option<String>,
    date: Option<String>,
}

/// Check GitHub Releases for a signed update. The only network request Structura makes.
#[tauri::command]
pub async fn update_check(app: tauri::AppHandle, state: State<'_, AppState>) -> AppResult<UpdateInfo> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| AppError::Internal(e.to_string()))?;
    let res = updater.check().await.map_err(|e| AppError::Io(format!("update check failed: {e}")))?;
    let info = match res {
        Some(u) => UpdateInfo {
            available: true,
            current_version: u.current_version.clone(),
            version: Some(u.version.clone()),
            notes: u.body.clone(),
            date: u.date.map(|d| d.to_string()),
        },
        None => UpdateInfo {
            available: false,
            current_version: crate::APP_VERSION.into(),
            version: None,
            notes: None,
            date: None,
        },
    };
    let latest = info.version.clone().unwrap_or_else(|| crate::APP_VERSION.to_string());
    let _ = settings::kv_set(
        &state.db,
        "update_last_check",
        &serde_json::json!({ "latestVersion": latest, "checkedAt": crate::core::now_ts() }).to_string(),
    );
    Ok(info)
}

/// Download, verify (minisign signature) and install the update. The frontend restarts the app.
#[tauri::command]
pub async fn update_install(app: tauri::AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| AppError::Internal(e.to_string()))?;
    let Some(update) = updater.check().await.map_err(|e| AppError::Io(e.to_string()))? else {
        return Err(AppError::NotFound("no update available".into()));
    };
    // Back up the database before the installer replaces the app (data stays in %LOCALAPPDATA%).
    let safety = crate::core::data_dir().join(format!("before-update-{}.db", crate::core::now_ts()));
    let _ = state.db.backup_to(&safety);
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| AppError::Io(format!("update failed: {e}")))?;
    let _ = history::record(
        &state.db,
        history::NewEntry {
            kind: "app.update",
            outcome: "success",
            project_id: None,
            profile_id: None,
            duration_ms: None,
            title: format!("Updated to {}", update.version),
            detail: serde_json::json!({}),
        },
    );
    Ok(())
}

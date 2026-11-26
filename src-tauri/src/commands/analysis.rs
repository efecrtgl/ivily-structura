//! Projects, analysis jobs (background + progress events + cancellation) and the explorer.

use crate::analysis::{self, model::Analysis};
use crate::core::{validate_root, CancelToken};
use crate::error::{AppError, AppResult};
use crate::scan::explorer::{self, DirItem, ListOptions};
use crate::state::{AppState, Job};
use crate::store::{history, mapping, profiles, projects, settings};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisHandle {
    pub id: String,
    pub project_id: i64,
    pub meta: analysis::model::Meta,
    pub summary: projects::Summary,
    /// `live` (just analysed), `cache` (last analysis from disk) or `snapshot`
    pub source: String,
    pub snapshot_id: Option<i64>,
}

pub fn handle(project_id: i64, a: &Analysis, source: &str, snapshot_id: Option<i64>) -> AnalysisHandle {
    AnalysisHandle {
        id: a.meta.id.clone(),
        project_id,
        meta: a.meta.clone(),
        summary: projects::summary_of(a),
        source: source.into(),
        snapshot_id,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    project: projects::Project,
    analysis: Option<AnalysisHandle>,
}

/// Register/open a project directory and load its last analysis (if any) from the cache.
#[tauri::command]
pub fn project_open(state: State<AppState>, path: String) -> AppResult<OpenedProject> {
    let root = validate_root(&path)?;
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let project = projects::upsert(&state.db, &root.to_string_lossy(), &name)?;
    let analysis = load_cached(&state, project.id)?;
    Ok(OpenedProject { project, analysis })
}

fn load_cached(state: &AppState, project_id: i64) -> AppResult<Option<AnalysisHandle>> {
    Ok(projects::load_cached_analysis(&state.db, project_id)?.map(|a| {
        let h = handle(project_id, &a, "cache", None);
        state.put_analysis(project_id, a);
        h
    }))
}

#[tauri::command]
pub fn project_list(state: State<AppState>) -> AppResult<Vec<projects::Project>> {
    projects::list(&state.db)
}

#[tauri::command]
pub fn project_get(state: State<AppState>, id: i64) -> AppResult<OpenedProject> {
    let project = projects::get(&state.db, id)?;
    let analysis = load_cached(&state, id)?;
    Ok(OpenedProject { project, analysis })
}

#[tauri::command]
pub fn project_update(
    state: State<AppState>,
    id: i64,
    update: projects::ProjectUpdate,
) -> AppResult<projects::Project> {
    let p = projects::update(&state.db, id, &update)?;
    if update.config.is_some() || update.profile_id.is_some() {
        let _ = history::record(
            &state.db,
            history::NewEntry {
                kind: "config.project",
                outcome: "success",
                project_id: Some(id),
                profile_id: p.profile_id.as_deref(),
                duration_ms: None,
                title: "Project configuration changed".into(),
                detail: serde_json::json!({ "extraExcludes": p.config.extra_excludes.len() }),
            },
        );
    }
    Ok(p)
}

/// Forget a project (Structura's data only — the project folder is never touched).
#[tauri::command]
pub fn project_delete(state: State<AppState>, id: i64) -> AppResult<()> {
    projects::delete(&state.db, id)?;
    state.drop_project(id);
    Ok(())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    job_id: String,
    project_id: i64,
    phase: &'static str,
    done: usize,
    total: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    job_id: String,
    project_id: i64,
    analysis: Option<AnalysisHandle>,
    error: Option<serde_json::Value>,
    cancelled: bool,
}

/// Start an analysis in the background. Progress: `analysis-progress`, result: `analysis-done`.
#[tauri::command]
pub fn analysis_start(
    app: AppHandle,
    state: State<AppState>,
    project_id: i64,
    profile_id: Option<String>,
) -> AppResult<String> {
    let project = projects::get(&state.db, project_id)?;
    let root = validate_root(&project.root)?;
    {
        let jobs = state.jobs.lock();
        if jobs.iter().any(|j| j.project_id == project_id) {
            return Err(AppError::Conflict("an analysis of this project is already running".into()));
        }
    }
    let pid = profile_id
        .or(project.profile_id.clone())
        .unwrap_or_else(|| settings::get(&state.db).map(|s| s.default_profile).unwrap_or_else(|_| "full".into()));
    let mut profile = profiles::get(&state.db, &pid)?;
    profile.scan.exclude.extend(project.config.extra_excludes.iter().cloned());
    let m = mapping::get(&state.db, project_id)?;
    let (layers, entries) = mapping::health_inputs(&m);
    profile.health_config.layers = layers;
    profile.health_config.extra_entry_points = entries;

    let cancel = CancelToken::new();
    let job_id = format!("job-{}-{}", project_id, crate::core::now_ts());
    state.jobs.lock().push(Job { id: job_id.clone(), project_id, cancel: cancel.clone() });

    let jid = job_id.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let emitter = app.clone();
        let last = parking_lot::Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(1));
        let progress = |p: analysis::Progress| {
            // Throttle UI events to ~15/s.
            let mut l = last.lock();
            if l.elapsed().as_millis() < 66 && p.phase != "done" {
                return;
            }
            *l = std::time::Instant::now();
            let _ = emitter.emit(
                "analysis-progress",
                ProgressEvent { job_id: jid.clone(), project_id, phase: p.phase, done: p.done, total: p.total },
            );
        };
        let result = analysis::run(&root, &profile, &cancel, &progress);
        state.jobs.lock().retain(|j| j.id != jid);
        let ev = match result {
            Ok(a) => {
                let _ = projects::record_analysis(&state.db, project_id, &a);
                let _ = history::record(
                    &state.db,
                    history::NewEntry {
                        kind: "analysis",
                        outcome: "success",
                        project_id: Some(project_id),
                        profile_id: Some(&profile.id),
                        duration_ms: Some(a.meta.duration_ms),
                        title: format!("Analysed with \"{}\"", profile.name),
                        detail: serde_json::json!({ "files": a.metrics.total_files, "findings": a.findings.len(), "cycles": a.metrics.cycles, "truncated": a.meta.truncated }),
                    },
                );
                let h = handle(project_id, &a, "live", None);
                state.put_analysis(project_id, a);
                DoneEvent { job_id: jid.clone(), project_id, analysis: Some(h), error: None, cancelled: false }
            }
            Err(e) => {
                let cancelled = matches!(e, AppError::Cancelled);
                let _ = history::record(
                    &state.db,
                    history::NewEntry {
                        kind: "analysis",
                        outcome: if cancelled { "cancelled" } else { "failure" },
                        project_id: Some(project_id),
                        profile_id: Some(&profile.id),
                        duration_ms: None,
                        title: if cancelled { "Analysis cancelled".into() } else { "Analysis failed".into() },
                        detail: serde_json::json!({ "error": e.code() }),
                    },
                );
                DoneEvent {
                    job_id: jid.clone(),
                    project_id,
                    analysis: None,
                    error: (!cancelled).then(|| serde_json::to_value(&e).unwrap_or_default()),
                    cancelled,
                }
            }
        };
        let _ = emitter.emit("analysis-done", ev);
    });
    Ok(job_id)
}

#[tauri::command]
pub fn analysis_cancel(state: State<AppState>, job_id: String) -> AppResult<()> {
    let jobs = state.jobs.lock();
    let j = jobs.iter().find(|j| j.id == job_id).ok_or_else(|| AppError::NotFound("job".into()))?;
    j.cancel.cancel();
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerItem {
    #[serde(flatten)]
    item: DirItem,
    lines: Option<u32>,
    fan_in: Option<u32>,
    fan_out: Option<u32>,
    findings: u32,
    annotated: bool,
    label: Option<String>,
    important: bool,
}

/// Live listing of one directory (lazy tree), overlaid with analysis results when available.
#[tauri::command]
pub fn explorer_list(
    state: State<AppState>,
    project_id: i64,
    analysis_id: Option<String>,
    path: String,
    show_hidden: bool,
    show_ignored: bool,
) -> AppResult<Vec<ExplorerItem>> {
    let project = projects::get(&state.db, project_id)?;
    let root = validate_root(&project.root)?;
    let mut excludes = profiles::get(&state.db, project.profile_id.as_deref().unwrap_or("full"))
        .map(|p| p.scan.exclude)
        .unwrap_or_default();
    excludes.extend(project.config.extra_excludes.iter().cloned());
    let items = explorer::list_dir(&root, &path, &excludes, ListOptions { show_hidden, show_ignored })?;
    let a: Option<Arc<Analysis>> = analysis_id.and_then(|id| state.analysis(&id).ok().map(|x| x.1));
    let m = mapping::get(&state.db, project_id)?;
    Ok(items
        .into_iter()
        .map(|item| {
            let rec = a.as_ref().and_then(|a| a.file_index(&item.path).map(|i| &a.files[i]));
            let dir_findings = |a: &Analysis| {
                a.findings
                    .iter()
                    .filter(|f| f.files.iter().any(|x| x == &item.path || x.starts_with(&format!("{}/", item.path))))
                    .count() as u32
            };
            let ann = m.annotations.iter().find(|x| x.path == item.path);
            ExplorerItem {
                lines: rec.and_then(|r| r.lines.map(|l| l.total)).or_else(|| {
                    a.as_ref()
                        .and_then(|a| a.dirs.iter().find(|d| d.path == item.path))
                        .map(|d| d.total_lines.min(u32::MAX as u64) as u32)
                }),
                fan_in: rec.map(|r| r.fan_in),
                fan_out: rec.map(|r| r.fan_out),
                findings: a.as_ref().map(|a| dir_findings(a)).unwrap_or(0),
                annotated: ann.is_some(),
                label: ann.map(|x| x.label.clone()).filter(|l| !l.is_empty()),
                important: ann.is_some_and(|x| x.important),
                item,
            }
        })
        .collect())
}

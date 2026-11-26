//! Snapshots, comparison, mapping and reports.

use super::analysis::{handle, AnalysisHandle};
use super::user_file;
use crate::analysis::model::Analysis;
use crate::compare::{self, Comparison};
use crate::error::{AppError, AppResult};
use crate::report::{self, ReportInput, ReportOptions};
use crate::state::AppState;
use crate::store::{history, mapping, snapshots};
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

fn log(state: &AppState, kind: &str, outcome: &str, project_id: Option<i64>, title: String, detail: serde_json::Value) {
    let _ = history::record(
        &state.db,
        history::NewEntry { kind, outcome, project_id, profile_id: None, duration_ms: None, title, detail },
    );
}

#[tauri::command]
pub fn snapshot_list(state: State<AppState>, project_id: i64) -> AppResult<Vec<snapshots::SnapshotMeta>> {
    snapshots::list(&state.db, project_id)
}

#[tauri::command]
pub fn snapshot_create(
    state: State<AppState>,
    analysis_id: String,
    name: String,
    note: String,
) -> AppResult<snapshots::SnapshotMeta> {
    let (project_id, a) = state.analysis(&analysis_id)?;
    let m = snapshots::create(&state.db, project_id, &name, &note, &a)?;
    log(
        &state,
        "snapshot.create",
        "success",
        Some(project_id),
        format!("Snapshot \"{}\" created", m.name),
        serde_json::json!({ "files": a.metrics.total_files }),
    );
    Ok(m)
}

#[tauri::command]
pub fn snapshot_update(
    state: State<AppState>,
    id: i64,
    name: String,
    note: String,
) -> AppResult<snapshots::SnapshotMeta> {
    snapshots::update(&state.db, id, &name, &note)
}

#[tauri::command]
pub fn snapshot_delete(state: State<AppState>, id: i64) -> AppResult<()> {
    let m = snapshots::get_meta(&state.db, id)?;
    snapshots::delete(&state.db, id)?;
    log(
        &state,
        "snapshot.delete",
        "success",
        Some(m.project_id),
        format!("Snapshot \"{}\" deleted", m.name),
        serde_json::json!({}),
    );
    Ok(())
}

/// Restore a snapshot's analysis into memory (read-only view; the project is not touched).
#[tauri::command]
pub fn snapshot_open(state: State<AppState>, id: i64) -> AppResult<AnalysisHandle> {
    let m = snapshots::get_meta(&state.db, id)?;
    let mut a = snapshots::load(&state.db, id)?;
    // Unique in-memory id so the snapshot does not replace the live analysis.
    a.meta.id = format!("snap-{id}-{}", a.meta.id);
    let h = handle(m.project_id, &a, "snapshot", Some(id));
    state.put_analysis(m.project_id, a);
    Ok(h)
}

#[tauri::command]
pub fn snapshot_export(state: State<AppState>, id: i64, dest: String) -> AppResult<()> {
    let p = user_file(&dest, &["json"], false)?;
    let m = snapshots::get_meta(&state.db, id)?;
    snapshots::export(&state.db, id, &p)?;
    log(
        &state,
        "snapshot.export",
        "success",
        Some(m.project_id),
        format!("Snapshot \"{}\" exported", m.name),
        serde_json::json!({}),
    );
    Ok(())
}

#[tauri::command]
pub fn snapshot_import(state: State<AppState>, project_id: i64, src: String) -> AppResult<snapshots::SnapshotMeta> {
    let p = user_file(&src, &["json"], true)?;
    let r = snapshots::import(&state.db, project_id, &p);
    log(
        &state,
        "snapshot.import",
        if r.is_ok() { "success" } else { "failure" },
        Some(project_id),
        "Snapshot imported".into(),
        serde_json::json!({}),
    );
    r
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AnalysisRef {
    /// In-memory analysis id
    Analysis {
        id: String,
    },
    Snapshot {
        id: i64,
    },
}

fn resolve_ref(state: &AppState, r: &AnalysisRef) -> AppResult<(Arc<Analysis>, String, Option<i64>)> {
    match r {
        AnalysisRef::Analysis { id } => {
            let (p, a) = state.analysis(id)?;
            Ok((a.clone(), format!("{} (analysis)", a.meta.name), Some(p)))
        }
        AnalysisRef::Snapshot { id } => {
            let m = snapshots::get_meta(&state.db, *id)?;
            Ok((Arc::new(snapshots::load(&state.db, *id)?), m.name, Some(m.project_id)))
        }
    }
}

#[tauri::command]
pub fn compare_analyses(state: State<AppState>, before: AnalysisRef, after: AnalysisRef) -> AppResult<Comparison> {
    let (a, la, pa) = resolve_ref(&state, &before)?;
    let (b, lb, _) = resolve_ref(&state, &after)?;
    let c = compare::compare(&a, &b, &la, &lb);
    log(
        &state,
        "snapshot.compare",
        "success",
        pa,
        format!("Compared \"{la}\" with \"{lb}\""),
        serde_json::json!({ "filesAdded": c.files.added_count, "filesRemoved": c.files.removed_count }),
    );
    Ok(c)
}

// ---------------------------------------------------------------------------------------------
// Mapping

#[tauri::command]
pub fn mapping_get(state: State<AppState>, project_id: i64) -> AppResult<mapping::Mapping> {
    mapping::get(&state.db, project_id)
}

#[tauri::command]
pub fn annotation_set(
    state: State<AppState>,
    project_id: i64,
    annotation: mapping::Annotation,
) -> AppResult<Option<mapping::Annotation>> {
    mapping::set_annotation(&state.db, project_id, &annotation)
}

#[tauri::command]
pub fn group_save(state: State<AppState>, project_id: i64, group: mapping::Group) -> AppResult<mapping::Group> {
    mapping::save_group(&state.db, project_id, &group)
}

#[tauri::command]
pub fn group_delete(state: State<AppState>, project_id: i64, id: i64) -> AppResult<()> {
    mapping::delete_group(&state.db, project_id, id)
}

#[tauri::command]
pub fn layers_set(
    state: State<AppState>,
    project_id: i64,
    layers: Vec<mapping::Layer>,
) -> AppResult<Vec<mapping::Layer>> {
    let r = mapping::set_layers(&state.db, project_id, &layers)?;
    log(
        &state,
        "config.layers",
        "success",
        Some(project_id),
        format!("{} architecture layers defined", r.len()),
        serde_json::json!({}),
    );
    Ok(r)
}

#[tauri::command]
pub fn relation_add(
    state: State<AppState>,
    project_id: i64,
    relation: mapping::Relation,
) -> AppResult<mapping::Relation> {
    mapping::add_relation(&state.db, project_id, &relation)
}

#[tauri::command]
pub fn relation_delete(state: State<AppState>, project_id: i64, id: i64) -> AppResult<()> {
    mapping::delete_relation(&state.db, project_id, id)
}

#[tauri::command]
pub fn note_save(state: State<AppState>, project_id: i64, note: mapping::Note) -> AppResult<mapping::Note> {
    mapping::save_note(&state.db, project_id, &note)
}

#[tauri::command]
pub fn note_delete(state: State<AppState>, project_id: i64, id: i64) -> AppResult<()> {
    mapping::delete_note(&state.db, project_id, id)
}

#[tauri::command]
pub fn mapping_export(state: State<AppState>, project_id: i64, dest: String) -> AppResult<()> {
    let p = user_file(&dest, &["json"], false)?;
    let m = mapping::get(&state.db, project_id)?;
    let doc = serde_json::json!({ "format": "ivily-structura-mapping", "version": 1, "mapping": m });
    std::fs::write(p, serde_json::to_vec_pretty(&doc)?)?;
    Ok(())
}

#[tauri::command]
pub fn mapping_import(state: State<AppState>, project_id: i64, src: String) -> AppResult<mapping::Mapping> {
    let p = user_file(&src, &["json"], true)?;
    if std::fs::metadata(&p)?.len() > 50 * 1024 * 1024 {
        return Err(AppError::validation("import_too_large", "mapping file is too large"));
    }
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p)?)?;
    if v["format"] != "ivily-structura-mapping" || v["version"].as_u64().unwrap_or(0) > 1 {
        return Err(AppError::validation("import_invalid", "not a supported Ivily Structura mapping file"));
    }
    let m: mapping::Mapping = serde_json::from_value(v["mapping"].clone())?;
    let r = mapping::replace_all(&state.db, project_id, &m)?;
    log(
        &state,
        "config.mapping",
        "success",
        Some(project_id),
        "Project mapping imported".into(),
        serde_json::json!({ "annotations": r.annotations.len() }),
    );
    Ok(r)
}

// ---------------------------------------------------------------------------------------------
// Reports

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    analysis_id: String,
    options: ReportOptions,
    compare_with: Option<AnalysisRef>,
}

fn build_report(state: &AppState, req: &ReportRequest) -> AppResult<String> {
    let (project_id, a) = state.analysis(&req.analysis_id)?;
    let m = mapping::get(&state.db, project_id)?;
    let cmp = match &req.compare_with {
        Some(r) => {
            let (b, lb, _) = resolve_ref(state, r)?;
            Some(compare::compare(&b, &a, &lb, &a.meta.name))
        }
        None => None,
    };
    report::generate(&ReportInput { analysis: &a, mapping: Some(&m), comparison: cmp.as_ref() }, &req.options)
}

#[tauri::command]
pub fn report_preview(state: State<AppState>, request: ReportRequest) -> AppResult<String> {
    let mut s = build_report(&state, &request)?;
    const MAX: usize = 400_000;
    if s.len() > MAX {
        let cut = s.char_indices().take_while(|(i, _)| *i < MAX).last().map(|(i, _)| i).unwrap_or(0);
        s.truncate(cut);
        s.push_str("\n\n… preview truncated — export to see the full report.\n");
    }
    Ok(s)
}

#[tauri::command]
pub fn report_export(state: State<AppState>, request: ReportRequest, dest: String) -> AppResult<()> {
    let ext: &[&str] = match request.options.format.as_str() {
        "markdown" => &["md", "markdown"],
        "json" => &["json"],
        "csv" => &["csv"],
        _ => return Err(AppError::validation("unknown_format", "unknown report format")),
    };
    let p = user_file(&dest, ext, false)?;
    let s = build_report(&state, &request)?;
    // CSV with BOM so spreadsheet apps detect UTF-8.
    let bytes = if request.options.format == "csv" {
        [b"\xEF\xBB\xBF".as_slice(), s.as_bytes()].concat()
    } else {
        s.into_bytes()
    };
    std::fs::write(&p, bytes)?;
    let (project_id, _) = state.analysis(&request.analysis_id)?;
    log(
        &state,
        "report.export",
        "success",
        Some(project_id),
        format!("{} report exported", request.options.format),
        serde_json::json!({ "sections": request.options.sections.len() }),
    );
    Ok(())
}

/// Save a file produced by the UI (graph image/data). Only image/data extensions are allowed.
#[tauri::command]
pub fn file_export(dest: String, content: String, base64: bool) -> AppResult<()> {
    let p = user_file(&dest, &["png", "svg", "json"], false)?;
    if content.len() > 64 * 1024 * 1024 {
        return Err(AppError::validation("too_large", "export is too large"));
    }
    let bytes = if base64 {
        decode_base64(&content).ok_or_else(|| AppError::validation("invalid_data", "invalid base64 data"))?
    } else {
        content.into_bytes()
    };
    std::fs::write(p, bytes)?;
    Ok(())
}

fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let s = s.split_once(',').map(|x| x.1).unwrap_or(s).trim();
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|b| **b == b'=').count();
        let mut n = 0u32;
        for (i, b) in chunk.iter().enumerate() {
            n |= if *b == b'=' { 0 } else { val(*b)? } << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_decoding() {
        assert_eq!(super::decode_base64("data:image/png;base64,aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::decode_base64("aGk=").unwrap(), b"hi");
        assert_eq!(super::decode_base64("YWJj").unwrap(), b"abc");
        assert!(super::decode_base64("***").is_none());
    }
}

//! Known projects, their per-project configuration and the cached latest analysis.

use crate::analysis::model::Analysis;
use crate::core::now_ts;
use crate::db::{pack, unpack, Db};
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectConfig {
    /// Additional exclusion globs on top of the profile's.
    pub extra_excludes: Vec<String>,
}

/// Compact summary of an analysis (dashboard cards, project list, IEP).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Summary {
    pub analyzed_at: i64,
    pub profile_id: String,
    pub duration_ms: u64,
    pub truncated: bool,
    pub files: u32,
    pub dirs: u32,
    pub bytes: u64,
    pub source_code_lines: u64,
    pub languages: Vec<(String, u32)>,
    pub ecosystems: Vec<String>,
    pub frameworks: Vec<String>,
    pub internal_edges: u32,
    pub external_packages: u32,
    pub cycles: u32,
    /// Findings per severity: [info, low, medium, high]
    pub findings: [u32; 4],
}

pub fn summary_of(a: &Analysis) -> Summary {
    let mut frameworks: Vec<String> =
        a.ecosystems.iter().flat_map(|e| e.frameworks.iter().map(|f| f.name.clone())).collect();
    frameworks.sort();
    frameworks.dedup();
    let mut ecosystems: Vec<String> = a.ecosystems.iter().map(|e| e.name.clone()).collect();
    ecosystems.sort();
    ecosystems.dedup();
    Summary {
        analyzed_at: a.meta.started_at,
        profile_id: a.meta.profile_id.clone(),
        duration_ms: a.meta.duration_ms,
        truncated: a.meta.truncated,
        files: a.metrics.total_files,
        dirs: a.metrics.total_dirs,
        bytes: a.metrics.total_bytes,
        source_code_lines: a.metrics.source_code_lines,
        languages: a.languages.iter().take(8).map(|l| (l.name.clone(), l.files)).collect(),
        ecosystems,
        frameworks,
        internal_edges: a.metrics.internal_edges,
        external_packages: a.metrics.external_packages,
        cycles: a.metrics.cycles,
        findings: crate::health::count_by_severity(&a.findings),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: i64,
    pub root: String,
    pub name: String,
    pub profile_id: Option<String>,
    pub config: ProjectConfig,
    pub pinned: bool,
    pub created_at: i64,
    pub last_opened_at: Option<i64>,
    pub last_analyzed_at: Option<i64>,
    pub last_summary: Option<Summary>,
    /// Whether the root still exists on disk (computed, not stored).
    pub exists: bool,
}

const COLS: &str =
    "id, root, name, profile_id, config, pinned, created_at, last_opened_at, last_analyzed_at, last_summary";

fn from_row(r: &Row) -> rusqlite::Result<Project> {
    let config: String = r.get(4)?;
    let summary: Option<String> = r.get(9)?;
    let root: String = r.get(1)?;
    Ok(Project {
        id: r.get(0)?,
        exists: std::path::Path::new(&root).is_dir(),
        root,
        name: r.get(2)?,
        profile_id: r.get(3)?,
        config: serde_json::from_str(&config).unwrap_or_default(),
        pinned: r.get::<_, i64>(5)? != 0,
        created_at: r.get(6)?,
        last_opened_at: r.get(7)?,
        last_analyzed_at: r.get(8)?,
        last_summary: summary.and_then(|s| serde_json::from_str(&s).ok()),
    })
}

/// Register a project root (validated canonical path) or return the existing entry.
pub fn upsert(db: &Db, root: &str, name: &str) -> AppResult<Project> {
    let now = now_ts();
    db.with(|c| {
        c.execute(
            "INSERT INTO projects(root, name, created_at, last_opened_at) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(root) DO UPDATE SET last_opened_at = excluded.last_opened_at",
            params![root, name, now],
        )
    })?;
    db.with(|c| c.query_row(&format!("SELECT {COLS} FROM projects WHERE root = ?1"), [root], from_row))
}

pub fn list(db: &Db) -> AppResult<Vec<Project>> {
    db.with(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {COLS} FROM projects ORDER BY pinned DESC, COALESCE(last_opened_at, created_at) DESC"
        ))?;
        let rows = st.query_map([], from_row)?;
        rows.collect()
    })
}

pub fn get(db: &Db, id: i64) -> AppResult<Project> {
    db.with(|c| c.query_row(&format!("SELECT {COLS} FROM projects WHERE id = ?1"), [id], from_row).optional())?
        .ok_or_else(|| AppError::NotFound(format!("project {id}")))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub name: Option<String>,
    pub profile_id: Option<String>,
    pub config: Option<ProjectConfig>,
    pub pinned: Option<bool>,
}

pub fn update(db: &Db, id: i64, u: &ProjectUpdate) -> AppResult<Project> {
    let mut p = get(db, id)?;
    if let Some(n) = &u.name {
        let n = n.trim();
        if n.is_empty() || n.chars().count() > 120 {
            return Err(AppError::validation("name_invalid", "name must be 1–120 characters"));
        }
        p.name = n.to_string();
    }
    if let Some(pid) = &u.profile_id {
        p.profile_id = Some(pid.clone());
    }
    if let Some(cfg) = &u.config {
        crate::scan::Excluder::new(&cfg.extra_excludes)?;
        if cfg.extra_excludes.len() > 500 {
            return Err(AppError::validation("too_many_excludes", "at most 500 exclusion patterns"));
        }
        p.config = cfg.clone();
    }
    if let Some(pin) = u.pinned {
        p.pinned = pin;
    }
    db.with(|c| {
        c.execute(
            "UPDATE projects SET name = ?2, profile_id = ?3, config = ?4, pinned = ?5 WHERE id = ?1",
            params![id, p.name, p.profile_id, serde_json::to_string(&p.config).unwrap_or_default(), p.pinned as i64],
        )
    })?;
    get(db, id)
}

/// Forget a project: removes its snapshots, mapping and cache (never touches the project files).
pub fn delete(db: &Db, id: i64) -> AppResult<()> {
    let n = db.with(|c| c.execute("DELETE FROM projects WHERE id = ?1", [id]))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("project {id}")));
    }
    Ok(())
}

pub fn record_analysis(db: &Db, id: i64, a: &Analysis) -> AppResult<()> {
    let summary = serde_json::to_string(&summary_of(a))?;
    let json = serde_json::to_vec(a)?;
    let (data, hash) = pack(&json)?;
    db.with_tx(|tx| {
        tx.execute(
            "UPDATE projects SET last_analyzed_at = ?2, last_summary = ?3 WHERE id = ?1",
            params![id, a.meta.started_at, summary],
        )?;
        tx.execute(
            "INSERT INTO analysis_cache(project_id, created_at, data, data_sha256) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project_id) DO UPDATE SET created_at = excluded.created_at, data = excluded.data, data_sha256 = excluded.data_sha256",
            params![id, a.meta.started_at, data, hash],
        )?;
        Ok(())
    })
}

pub fn load_cached_analysis(db: &Db, id: i64) -> AppResult<Option<Analysis>> {
    let row: Option<(Vec<u8>, String)> = db.with(|c| {
        c.query_row("SELECT data, data_sha256 FROM analysis_cache WHERE project_id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
    })?;
    match row {
        None => Ok(None),
        Some((data, hash)) => match unpack(&data, &hash).and_then(|j| Ok(serde_json::from_slice::<Analysis>(&j)?)) {
            Ok(a) => Ok(Some(a)),
            // A corrupt cache is not fatal: drop it, the user can re-analyse.
            Err(_) => {
                db.with(|c| c.execute("DELETE FROM analysis_cache WHERE project_id = ?1", [id]))?;
                Ok(None)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_update_delete() {
        let db = Db::open_in_memory().unwrap();
        let p = upsert(&db, "C:\\Proj\\A", "A").unwrap();
        let again = upsert(&db, "c:\\proj\\a", "ignored").unwrap();
        assert_eq!(p.id, again.id, "roots are case-insensitive on Windows");
        let u = update(
            &db,
            p.id,
            &ProjectUpdate {
                name: Some(" Renamed ".into()),
                profile_id: Some("quick".into()),
                config: Some(ProjectConfig { extra_excludes: vec!["fixtures".into()] }),
                pinned: Some(true),
            },
        )
        .unwrap();
        assert_eq!(u.name, "Renamed");
        assert!(u.pinned);
        assert_eq!(u.config.extra_excludes, vec!["fixtures"]);
        assert!(update(
            &db,
            p.id,
            &ProjectUpdate { name: Some("".into()), profile_id: None, config: None, pinned: None }
        )
        .is_err());
        assert!(update(
            &db,
            p.id,
            &ProjectUpdate {
                name: None,
                profile_id: None,
                config: Some(ProjectConfig { extra_excludes: vec!["a[".into()] }),
                pinned: None
            }
        )
        .is_err());
        assert_eq!(list(&db).unwrap().len(), 1);
        delete(&db, p.id).unwrap();
        assert!(get(&db, p.id).is_err());
    }

    #[test]
    fn analysis_cache_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let p = upsert(&db, "C:\\X", "X").unwrap();
        let mut a = Analysis::default();
        a.meta.started_at = 42;
        a.metrics.total_files = 7;
        record_analysis(&db, p.id, &a).unwrap();
        let back = load_cached_analysis(&db, p.id).unwrap().unwrap();
        assert_eq!(back.metrics.total_files, 7);
        let pr = get(&db, p.id).unwrap();
        assert_eq!(pr.last_analyzed_at, Some(42));
        assert_eq!(pr.last_summary.unwrap().files, 7);
        // Corrupt cache is discarded, not fatal.
        db.with(|c| c.execute("UPDATE analysis_cache SET data_sha256 = 'x'", [])).unwrap();
        assert!(load_cached_analysis(&db, p.id).unwrap().is_none());
    }
}

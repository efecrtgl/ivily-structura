//! Structure snapshots: named, immutable copies of an analysis (compressed + hashed).
//! Snapshots store structure, metrics, graph and findings — never file contents.

use super::projects::{summary_of, Summary};
use crate::analysis::model::{Analysis, ENGINE_VERSION};
use crate::core::now_ts;
use crate::db::{pack, unpack, Db};
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EXPORT_FORMAT: &str = "ivily-structura-snapshot";
pub const EXPORT_VERSION: u32 = 1;
const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub note: String,
    pub created_at: i64,
    pub engine_version: u32,
    pub size_bytes: i64,
    pub summary: Summary,
}

fn validate_text(name: &str, note: &str) -> AppResult<()> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 120 {
        return Err(AppError::validation("snapshot_name_invalid", "snapshot name must be 1–120 characters"));
    }
    if note.chars().count() > 20_000 {
        return Err(AppError::validation("note_too_long", "note must be at most 20,000 characters"));
    }
    Ok(())
}

pub fn create(db: &Db, project_id: i64, name: &str, note: &str, a: &Analysis) -> AppResult<SnapshotMeta> {
    validate_text(name, note)?;
    let summary = summary_of(a);
    let json = serde_json::to_vec(a)?;
    let (data, hash) = pack(&json)?;
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO snapshots(project_id, name, note, created_at, engine_version, summary, data, data_sha256, size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![project_id, name.trim(), note, now_ts(), a.meta.engine_version, serde_json::to_string(&summary).unwrap_or_default(), data, hash, data.len() as i64],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    get_meta(db, id)
}

const META_COLS: &str = "id, project_id, name, note, created_at, engine_version, size_bytes, summary";

fn meta_row(r: &rusqlite::Row) -> rusqlite::Result<SnapshotMeta> {
    let s: String = r.get(7)?;
    Ok(SnapshotMeta {
        id: r.get(0)?,
        project_id: r.get(1)?,
        name: r.get(2)?,
        note: r.get(3)?,
        created_at: r.get(4)?,
        engine_version: r.get(5)?,
        size_bytes: r.get(6)?,
        summary: serde_json::from_str(&s).unwrap_or_default(),
    })
}

pub fn list(db: &Db, project_id: i64) -> AppResult<Vec<SnapshotMeta>> {
    db.with(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {META_COLS} FROM snapshots WHERE project_id = ?1 ORDER BY created_at DESC, id DESC"
        ))?;
        let rows = st.query_map([project_id], meta_row)?;
        rows.collect()
    })
}

pub fn get_meta(db: &Db, id: i64) -> AppResult<SnapshotMeta> {
    db.with(|c| c.query_row(&format!("SELECT {META_COLS} FROM snapshots WHERE id = ?1"), [id], meta_row).optional())?
        .ok_or_else(|| AppError::NotFound(format!("snapshot {id}")))
}

pub fn load(db: &Db, id: i64) -> AppResult<Analysis> {
    let (data, hash): (Vec<u8>, String) = db
        .with(|c| {
            c.query_row("SELECT data, data_sha256 FROM snapshots WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
        })?
        .ok_or_else(|| AppError::NotFound(format!("snapshot {id}")))?;
    let json = unpack(&data, &hash)?;
    Ok(serde_json::from_slice(&json)?)
}

pub fn update(db: &Db, id: i64, name: &str, note: &str) -> AppResult<SnapshotMeta> {
    validate_text(name, note)?;
    let n = db.with(|c| {
        c.execute("UPDATE snapshots SET name = ?2, note = ?3 WHERE id = ?1", params![id, name.trim(), note])
    })?;
    if n == 0 {
        return Err(AppError::NotFound(format!("snapshot {id}")));
    }
    get_meta(db, id)
}

pub fn delete(db: &Db, id: i64) -> AppResult<()> {
    let n = db.with(|c| c.execute("DELETE FROM snapshots WHERE id = ?1", [id]))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("snapshot {id}")));
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportFile {
    format: String,
    version: u32,
    exported_at: i64,
    name: String,
    note: String,
    created_at: i64,
    analysis: Analysis,
}

/// Export a snapshot as a standalone JSON file.
pub fn export(db: &Db, id: i64, dest: &Path) -> AppResult<()> {
    let m = get_meta(db, id)?;
    let a = load(db, id)?;
    let f = ExportFile {
        format: EXPORT_FORMAT.into(),
        version: EXPORT_VERSION,
        exported_at: now_ts(),
        name: m.name,
        note: m.note,
        created_at: m.created_at,
        analysis: a,
    };
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&f)?)?;
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// Import a snapshot file into a project. The file is validated (format, version, size).
pub fn import(db: &Db, project_id: i64, src: &Path) -> AppResult<SnapshotMeta> {
    let meta = std::fs::metadata(src)?;
    if meta.len() > MAX_IMPORT_BYTES {
        return Err(AppError::validation("import_too_large", "snapshot file is too large"));
    }
    let bytes = std::fs::read(src)?;
    let f: ExportFile = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::validation("import_invalid", "not an Ivily Structura snapshot file"))?;
    if f.format != EXPORT_FORMAT {
        return Err(AppError::validation("import_invalid", "not an Ivily Structura snapshot file"));
    }
    if f.version > EXPORT_VERSION || f.analysis.meta.engine_version > ENGINE_VERSION {
        return Err(AppError::validation(
            "import_newer",
            "the snapshot was created by a newer version of Ivily Structura",
        ));
    }
    let name = if f.name.trim().is_empty() {
        "Imported snapshot".to_string()
    } else {
        f.name.chars().take(110).collect::<String>() + " (imported)"
    };
    let note: String = f.note.chars().take(20_000).collect();
    create(db, project_id, &name, &note, &f.analysis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::projects;

    #[test]
    fn snapshot_lifecycle_export_import() {
        let db = Db::open_in_memory().unwrap();
        let p = projects::upsert(&db, "C:\\P", "P").unwrap();
        let mut a = Analysis::default();
        a.meta.engine_version = ENGINE_VERSION;
        a.metrics.total_files = 12;
        let s = create(&db, p.id, " v1 ", "first", &a).unwrap();
        assert_eq!(s.name, "v1");
        assert_eq!(s.summary.files, 12);
        assert_eq!(load(&db, s.id).unwrap().metrics.total_files, 12);
        let u = update(&db, s.id, "v1.0", "renamed").unwrap();
        assert_eq!((u.name.as_str(), u.note.as_str()), ("v1.0", "renamed"));
        assert!(create(&db, p.id, "", "", &a).is_err());

        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("snap.json");
        export(&db, s.id, &file).unwrap();
        let imported = import(&db, p.id, &file).unwrap();
        assert!(imported.name.ends_with("(imported)"));
        assert_eq!(list(&db, p.id).unwrap().len(), 2);

        // Invalid files are rejected.
        std::fs::write(&file, r#"{"format":"other","version":1}"#).unwrap();
        assert!(import(&db, p.id, &file).is_err());
        let mut newer = serde_json::to_value(ExportFile {
            format: EXPORT_FORMAT.into(),
            version: 99,
            exported_at: 0,
            name: "x".into(),
            note: String::new(),
            created_at: 0,
            analysis: a,
        })
        .unwrap();
        newer["version"] = 99.into();
        std::fs::write(&file, newer.to_string()).unwrap();
        assert!(import(&db, p.id, &file).is_err());

        delete(&db, s.id).unwrap();
        assert!(load(&db, s.id).is_err());
        // Deleting the project cascades to its snapshots.
        projects::delete(&db, p.id).unwrap();
        assert!(list(&db, p.id).unwrap().is_empty());
    }
}

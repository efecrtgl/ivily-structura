//! Project Mapping workspace: user annotations, module groups, layers, relations and notes.
//!
//! Mapping data is keyed by project-relative path and stored independently of analysis
//! results, so it survives re-indexing. Paths that disappear are reported as stale, never
//! deleted automatically.

use crate::core::{normalize_rel, now_ts};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::health::Layer as HealthLayer;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Annotation {
    pub id: i64,
    pub path: String,
    pub label: String,
    pub note: String,
    pub important: bool,
    pub entry_point: bool,
    pub group_id: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub description: String,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Layer {
    pub id: i64,
    pub name: String,
    pub rank: i64,
    pub description: String,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Relation {
    pub id: i64,
    pub from_path: String,
    pub to_path: String,
    pub kind: String,
    pub note: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Mapping {
    pub annotations: Vec<Annotation>,
    pub groups: Vec<Group>,
    pub layers: Vec<Layer>,
    pub relations: Vec<Relation>,
    pub notes: Vec<Note>,
}

fn check_len(field: &'static str, s: &str, max: usize) -> AppResult<()> {
    if s.chars().count() > max {
        return Err(AppError::validation(field, format!("must be at most {max} characters")));
    }
    Ok(())
}

fn check_patterns(p: &[String]) -> AppResult<()> {
    if p.len() > 100 {
        return Err(AppError::validation("too_many_patterns", "at most 100 patterns"));
    }
    for x in p {
        globset::Glob::new(x.trim())
            .map_err(|_| AppError::validation("invalid_pattern", format!("invalid pattern: {x}")))?;
    }
    Ok(())
}

const RELATION_KINDS: &[&str] = &["depends-on", "calls", "owns", "replaces", "related", "deploys-with"];

pub fn get(db: &Db, project_id: i64) -> AppResult<Mapping> {
    db.with(|c| {
        let mut m = Mapping::default();
        let mut st = c.prepare("SELECT id, path, label, note, important, entry_point, group_id, updated_at FROM annotations WHERE project_id = ?1 ORDER BY path")?;
        m.annotations = st
            .query_map([project_id], |r| {
                Ok(Annotation {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    label: r.get(2)?,
                    note: r.get(3)?,
                    important: r.get::<_, i64>(4)? != 0,
                    entry_point: r.get::<_, i64>(5)? != 0,
                    group_id: r.get(6)?,
                    updated_at: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        let mut st = c.prepare("SELECT id, name, color, description, patterns FROM module_groups WHERE project_id = ?1 ORDER BY name COLLATE NOCASE")?;
        m.groups = st
            .query_map([project_id], |r| {
                let p: String = r.get(4)?;
                Ok(Group { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, description: r.get(3)?, patterns: serde_json::from_str(&p).unwrap_or_default() })
            })?
            .collect::<Result<_, _>>()?;
        let mut st = c.prepare("SELECT id, name, rank, description, patterns FROM layers WHERE project_id = ?1 ORDER BY rank, id")?;
        m.layers = st
            .query_map([project_id], |r| {
                let p: String = r.get(4)?;
                Ok(Layer { id: r.get(0)?, name: r.get(1)?, rank: r.get(2)?, description: r.get(3)?, patterns: serde_json::from_str(&p).unwrap_or_default() })
            })?
            .collect::<Result<_, _>>()?;
        let mut st = c.prepare("SELECT id, from_path, to_path, kind, note, created_at FROM relations WHERE project_id = ?1 ORDER BY id")?;
        m.relations = st
            .query_map([project_id], |r| Ok(Relation { id: r.get(0)?, from_path: r.get(1)?, to_path: r.get(2)?, kind: r.get(3)?, note: r.get(4)?, created_at: r.get(5)? }))?
            .collect::<Result<_, _>>()?;
        let mut st = c.prepare("SELECT id, title, body, created_at, updated_at FROM notes WHERE project_id = ?1 ORDER BY updated_at DESC")?;
        m.notes = st
            .query_map([project_id], |r| Ok(Note { id: r.get(0)?, title: r.get(1)?, body: r.get(2)?, created_at: r.get(3)?, updated_at: r.get(4)? }))?
            .collect::<Result<_, _>>()?;
        Ok(m)
    })
}

/// Create/replace the annotation for a path. An annotation with no content is removed.
pub fn set_annotation(db: &Db, project_id: i64, a: &Annotation) -> AppResult<Option<Annotation>> {
    let path = normalize_rel(&a.path)?;
    check_len("label_too_long", &a.label, 200)?;
    check_len("note_too_long", &a.note, 20_000)?;
    let empty =
        a.label.trim().is_empty() && a.note.trim().is_empty() && !a.important && !a.entry_point && a.group_id.is_none();
    if empty {
        db.with(|c| {
            c.execute("DELETE FROM annotations WHERE project_id = ?1 AND path = ?2", params![project_id, path])
        })?;
        return Ok(None);
    }
    if let Some(g) = a.group_id {
        let ok: Option<i64> = db.with(|c| {
            c.query_row("SELECT id FROM module_groups WHERE id = ?1 AND project_id = ?2", params![g, project_id], |r| {
                r.get(0)
            })
            .optional()
        })?;
        if ok.is_none() {
            return Err(AppError::validation("group_not_found", "group does not belong to this project"));
        }
    }
    db.with(|c| {
        c.execute(
            "INSERT INTO annotations(project_id, path, label, note, important, entry_point, group_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(project_id, path) DO UPDATE SET label = excluded.label, note = excluded.note, important = excluded.important,
               entry_point = excluded.entry_point, group_id = excluded.group_id, updated_at = excluded.updated_at",
            params![project_id, path, a.label.trim(), a.note, a.important as i64, a.entry_point as i64, a.group_id, now_ts()],
        )
    })?;
    Ok(get(db, project_id)?.annotations.into_iter().find(|x| x.path == path))
}

pub fn save_group(db: &Db, project_id: i64, g: &Group) -> AppResult<Group> {
    let name = g.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::validation("group_name_invalid", "group name must be 1–80 characters"));
    }
    check_len("description_too_long", &g.description, 2000)?;
    check_len("color_invalid", &g.color, 32)?;
    check_patterns(&g.patterns)?;
    let patterns = serde_json::to_string(&g.patterns)?;
    let id = db.with(|c| {
        if g.id > 0 {
            c.execute(
                "UPDATE module_groups SET name = ?3, color = ?4, description = ?5, patterns = ?6 WHERE id = ?1 AND project_id = ?2",
                params![g.id, project_id, name, g.color, g.description, patterns],
            )?;
            Ok(g.id)
        } else {
            c.execute(
                "INSERT INTO module_groups(project_id, name, color, description, patterns) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![project_id, name, g.color, g.description, patterns],
            )?;
            Ok(c.last_insert_rowid())
        }
    })
    .map_err(|e| match e {
        AppError::Database(m) if m.contains("UNIQUE") => AppError::Conflict("a group with this name already exists".into()),
        other => other,
    })?;
    get(db, project_id)?.groups.into_iter().find(|x| x.id == id).ok_or_else(|| AppError::NotFound("group".into()))
}

pub fn delete_group(db: &Db, project_id: i64, id: i64) -> AppResult<()> {
    db.with(|c| c.execute("DELETE FROM module_groups WHERE id = ?1 AND project_id = ?2", params![id, project_id]))?;
    Ok(())
}

/// Replace the full ordered layer list (order = rank, top layer first).
pub fn set_layers(db: &Db, project_id: i64, layers: &[Layer]) -> AppResult<Vec<Layer>> {
    if layers.len() > 20 {
        return Err(AppError::validation("too_many_layers", "at most 20 layers"));
    }
    let mut names = std::collections::HashSet::new();
    for l in layers {
        let n = l.name.trim();
        if n.is_empty() || n.chars().count() > 80 {
            return Err(AppError::validation("layer_name_invalid", "layer name must be 1–80 characters"));
        }
        if !names.insert(n.to_lowercase()) {
            return Err(AppError::validation("layer_duplicate", format!("duplicate layer name {n}")));
        }
        check_patterns(&l.patterns)?;
        check_len("description_too_long", &l.description, 2000)?;
    }
    db.with_tx(|tx| {
        tx.execute("DELETE FROM layers WHERE project_id = ?1", [project_id])?;
        for (i, l) in layers.iter().enumerate() {
            tx.execute(
                "INSERT INTO layers(project_id, name, rank, description, patterns) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![project_id, l.name.trim(), i as i64, l.description, serde_json::to_string(&l.patterns)?],
            )?;
        }
        Ok(())
    })?;
    Ok(get(db, project_id)?.layers)
}

pub fn add_relation(db: &Db, project_id: i64, r: &Relation) -> AppResult<Relation> {
    let from = normalize_rel(&r.from_path)?;
    let to = normalize_rel(&r.to_path)?;
    let kind = if r.kind.is_empty() { "depends-on" } else { r.kind.as_str() };
    if !RELATION_KINDS.contains(&kind) {
        return Err(AppError::validation("relation_kind_invalid", "unknown relation kind"));
    }
    if from == to {
        return Err(AppError::validation("relation_self", "a relation needs two different paths"));
    }
    check_len("note_too_long", &r.note, 2000)?;
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO relations(project_id, from_path, to_path, kind, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![project_id, from, to, kind, r.note, now_ts()],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    get(db, project_id)?.relations.into_iter().find(|x| x.id == id).ok_or_else(|| AppError::NotFound("relation".into()))
}

pub fn delete_relation(db: &Db, project_id: i64, id: i64) -> AppResult<()> {
    db.with(|c| c.execute("DELETE FROM relations WHERE id = ?1 AND project_id = ?2", params![id, project_id]))?;
    Ok(())
}

pub fn save_note(db: &Db, project_id: i64, n: &Note) -> AppResult<Note> {
    let title = n.title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(AppError::validation("note_title_invalid", "note title must be 1–200 characters"));
    }
    check_len("note_too_long", &n.body, 100_000)?;
    let now = now_ts();
    let id = db.with(|c| {
        if n.id > 0 {
            c.execute(
                "UPDATE notes SET title = ?3, body = ?4, updated_at = ?5 WHERE id = ?1 AND project_id = ?2",
                params![n.id, project_id, title, n.body, now],
            )?;
            Ok(n.id)
        } else {
            c.execute(
                "INSERT INTO notes(project_id, title, body, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![project_id, title, n.body, now],
            )?;
            Ok(c.last_insert_rowid())
        }
    })?;
    get(db, project_id)?.notes.into_iter().find(|x| x.id == id).ok_or_else(|| AppError::NotFound("note".into()))
}

pub fn delete_note(db: &Db, project_id: i64, id: i64) -> AppResult<()> {
    db.with(|c| c.execute("DELETE FROM notes WHERE id = ?1 AND project_id = ?2", params![id, project_id]))?;
    Ok(())
}

/// Mapping-derived inputs for health rules (layers and user-declared entry points).
pub fn health_inputs(m: &Mapping) -> (Vec<HealthLayer>, Vec<String>) {
    let layers = m.layers.iter().map(|l| HealthLayer { name: l.name.clone(), patterns: l.patterns.clone() }).collect();
    let entries = m.annotations.iter().filter(|a| a.entry_point).map(|a| a.path.clone()).collect();
    (layers, entries)
}

/// Replace a project's entire mapping (import). Runs in one transaction.
pub fn replace_all(db: &Db, project_id: i64, m: &Mapping) -> AppResult<Mapping> {
    for a in &m.annotations {
        normalize_rel(&a.path)?;
        check_len("label_too_long", &a.label, 200)?;
        check_len("note_too_long", &a.note, 20_000)?;
    }
    for g in &m.groups {
        check_patterns(&g.patterns)?;
    }
    db.with_tx(|tx| {
        for t in ["annotations", "relations", "notes", "layers", "module_groups"] {
            tx.execute(&format!("DELETE FROM {t} WHERE project_id = ?1"), [project_id])?;
        }
        let mut group_ids = std::collections::HashMap::new();
        for g in &m.groups {
            tx.execute(
                "INSERT INTO module_groups(project_id, name, color, description, patterns) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![project_id, g.name.trim(), g.color, g.description, serde_json::to_string(&g.patterns)?],
            )?;
            group_ids.insert(g.id, tx.last_insert_rowid());
        }
        for (i, l) in m.layers.iter().enumerate() {
            tx.execute(
                "INSERT INTO layers(project_id, name, rank, description, patterns) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![project_id, l.name.trim(), i as i64, l.description, serde_json::to_string(&l.patterns)?],
            )?;
        }
        let now = now_ts();
        for a in &m.annotations {
            tx.execute(
                "INSERT INTO annotations(project_id, path, label, note, important, entry_point, group_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![project_id, normalize_rel(&a.path)?, a.label, a.note, a.important as i64, a.entry_point as i64, a.group_id.and_then(|g| group_ids.get(&g).copied()), now],
            )?;
        }
        for r in &m.relations {
            tx.execute(
                "INSERT INTO relations(project_id, from_path, to_path, kind, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![project_id, normalize_rel(&r.from_path)?, normalize_rel(&r.to_path)?, r.kind, r.note, now],
            )?;
        }
        for n in &m.notes {
            tx.execute("INSERT INTO notes(project_id, title, body, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)", params![project_id, n.title, n.body, now])?;
        }
        Ok(())
    })?;
    get(db, project_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::projects;

    #[test]
    fn mapping_crud_and_health_inputs() {
        let db = Db::open_in_memory().unwrap();
        let p = projects::upsert(&db, "C:\\M", "M").unwrap().id;
        let g = save_group(
            &db,
            p,
            &Group { name: "Core".into(), patterns: vec!["src/core/**".into()], ..Default::default() },
        )
        .unwrap();
        assert!(matches!(
            save_group(&db, p, &Group { name: "Core".into(), ..Default::default() }),
            Err(AppError::Conflict(_))
        ));
        let a = set_annotation(
            &db,
            p,
            &Annotation {
                path: "src\\plugins".into(),
                label: "Plugins".into(),
                entry_point: true,
                group_id: Some(g.id),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(a.path, "src/plugins");
        assert!(set_annotation(&db, p, &Annotation { path: "../x".into(), label: "x".into(), ..Default::default() })
            .is_err());
        set_layers(
            &db,
            p,
            &[
                Layer { name: "UI".into(), patterns: vec!["src/ui".into()], ..Default::default() },
                Layer { name: "Core".into(), patterns: vec!["src/core".into()], ..Default::default() },
            ],
        )
        .unwrap();
        assert!(set_layers(
            &db,
            p,
            &[Layer { name: "A".into(), ..Default::default() }, Layer { name: "a".into(), ..Default::default() }]
        )
        .is_err());
        add_relation(
            &db,
            p,
            &Relation {
                from_path: "src/ui".into(),
                to_path: "src/api".into(),
                kind: "calls".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(add_relation(&db, p, &Relation { from_path: "a".into(), to_path: "a".into(), ..Default::default() })
            .is_err());
        save_note(&db, p, &Note { title: "Why".into(), body: "Because".into(), ..Default::default() }).unwrap();

        let m = get(&db, p).unwrap();
        assert_eq!(
            (m.annotations.len(), m.groups.len(), m.layers.len(), m.relations.len(), m.notes.len()),
            (1, 1, 2, 1, 1)
        );
        assert_eq!(m.layers[0].name, "UI");
        let (layers, entries) = health_inputs(&m);
        assert_eq!(layers.len(), 2);
        assert_eq!(entries, vec!["src/plugins"]);

        // Deleting a group keeps the annotation but clears its group.
        delete_group(&db, p, g.id).unwrap();
        assert_eq!(get(&db, p).unwrap().annotations[0].group_id, None);
        // Clearing all fields removes the annotation.
        assert!(set_annotation(&db, p, &Annotation { path: "src/plugins".into(), ..Default::default() })
            .unwrap()
            .is_none());

        // Replace-all (import) round trip
        let copy = replace_all(&db, p, &m).unwrap();
        assert_eq!(copy.layers.len(), 2);
        assert_eq!(copy.annotations[0].group_id, Some(copy.groups[0].id));
    }
}

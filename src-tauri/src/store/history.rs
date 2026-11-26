//! Analysis and activity history with configurable retention.
//! Entries hold identifiers and counts only — never file contents.

use crate::core::now_ts;
use crate::db::Db;
use crate::error::AppResult;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub ts: i64,
    pub kind: String,
    pub outcome: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub profile_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub title: String,
    pub detail: serde_json::Value,
}

pub struct NewEntry<'a> {
    pub kind: &'a str,
    pub outcome: &'a str,
    pub project_id: Option<i64>,
    pub profile_id: Option<&'a str>,
    pub duration_ms: Option<u64>,
    pub title: String,
    pub detail: serde_json::Value,
}

pub fn record(db: &Db, e: NewEntry) -> AppResult<()> {
    db.with(|c| {
        c.execute(
            "INSERT INTO history(ts, kind, outcome, project_id, profile_id, duration_ms, title, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![now_ts(), e.kind, e.outcome, e.project_id, e.profile_id, e.duration_ms.map(|d| d as i64), e.title, e.detail.to_string()],
        )
    })?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    pub project_id: Option<i64>,
    pub kind: Option<String>,
    pub outcome: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub fn list(db: &Db, q: &Query) -> AppResult<Vec<Entry>> {
    let limit = q.limit.unwrap_or(200).min(2000) as i64;
    let offset = q.offset.unwrap_or(0) as i64;
    db.with(|c| {
        let mut st = c.prepare(
            "SELECT h.id, h.ts, h.kind, h.outcome, h.project_id, p.name, h.profile_id, h.duration_ms, h.title, h.detail
             FROM history h LEFT JOIN projects p ON p.id = h.project_id
             WHERE (?1 IS NULL OR h.project_id = ?1) AND (?2 IS NULL OR h.kind LIKE ?2 || '%') AND (?3 IS NULL OR h.outcome = ?3)
             ORDER BY h.ts DESC, h.id DESC LIMIT ?4 OFFSET ?5",
        )?;
        let rows = st.query_map(params![q.project_id, q.kind, q.outcome, limit, offset], |r| {
            let detail: String = r.get(9)?;
            Ok(Entry {
                id: r.get(0)?,
                ts: r.get(1)?,
                kind: r.get(2)?,
                outcome: r.get(3)?,
                project_id: r.get(4)?,
                project_name: r.get(5)?,
                profile_id: r.get(6)?,
                duration_ms: r.get(7)?,
                title: r.get(8)?,
                detail: serde_json::from_str(&detail).unwrap_or(serde_json::Value::Null),
            })
        })?;
        rows.collect()
    })
}

/// Delete entries older than `days` (0 = keep forever). Returns deleted count.
pub fn prune(db: &Db, days: u32) -> AppResult<usize> {
    if days == 0 {
        return Ok(0);
    }
    let cutoff = now_ts() - days as i64 * 86_400;
    db.with(|c| c.execute("DELETE FROM history WHERE ts < ?1", [cutoff]))
}

pub fn clear(db: &Db) -> AppResult<usize> {
    db.with(|c| c.execute("DELETE FROM history", []))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_list_filter_prune() {
        let db = Db::open_in_memory().unwrap();
        for (k, o) in [("analysis", "success"), ("analysis", "failure"), ("snapshot.create", "success")] {
            record(
                &db,
                NewEntry {
                    kind: k,
                    outcome: o,
                    project_id: None,
                    profile_id: Some("full"),
                    duration_ms: Some(5),
                    title: k.into(),
                    detail: serde_json::json!({"files": 3}),
                },
            )
            .unwrap();
        }
        assert_eq!(list(&db, &Query::default()).unwrap().len(), 3);
        assert_eq!(list(&db, &Query { kind: Some("snapshot".into()), ..Default::default() }).unwrap().len(), 1);
        assert_eq!(list(&db, &Query { outcome: Some("failure".into()), ..Default::default() }).unwrap().len(), 1);
        db.with(|c| c.execute("UPDATE history SET ts = 0 WHERE kind = 'snapshot.create'", [])).unwrap();
        assert_eq!(prune(&db, 30).unwrap(), 1);
        assert_eq!(prune(&db, 0).unwrap(), 0);
        assert_eq!(clear(&db).unwrap(), 2);
    }
}

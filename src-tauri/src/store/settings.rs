//! Application settings (single JSON document in `kv`). Unknown/missing fields fall back to
//! defaults so older and newer versions can share a database.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `system`, `dark`, `light`
    pub theme: String,
    pub reduce_motion: bool,
    pub default_profile: String,
    /// Days to keep history entries (0 = forever).
    pub history_retention_days: u32,
    pub check_updates: bool,
    pub explorer_show_hidden: bool,
    pub explorer_show_ignored: bool,
    pub explorer_max_depth: u32,
    pub graph_max_nodes: u32,
    pub graph_layout: String,
    pub confirm_destructive: bool,
    /// Reopen the most recent project on start.
    pub reopen_last_project: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            reduce_motion: false,
            default_profile: "full".into(),
            history_retention_days: 90,
            check_updates: true,
            explorer_show_hidden: false,
            explorer_show_ignored: false,
            explorer_max_depth: 32,
            graph_max_nodes: 300,
            graph_layout: "cose".into(),
            confirm_destructive: true,
            reopen_last_project: true,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> AppResult<()> {
        if !matches!(self.theme.as_str(), "system" | "dark" | "light") {
            return Err(AppError::validation("invalid_theme", "theme must be system, dark or light"));
        }
        if !matches!(self.graph_layout.as_str(), "cose" | "breadthfirst" | "concentric" | "circle" | "grid") {
            return Err(AppError::validation("invalid_layout", "unknown graph layout"));
        }
        if !(20..=3000).contains(&self.graph_max_nodes) {
            return Err(AppError::validation("invalid_graph_max", "graph node limit must be 20–3000"));
        }
        if self.history_retention_days > 3650 {
            return Err(AppError::validation("invalid_retention", "retention must be at most 3650 days"));
        }
        if !(1..=256).contains(&self.explorer_max_depth) {
            return Err(AppError::validation("invalid_depth", "explorer depth must be 1–256"));
        }
        Ok(())
    }
}

pub fn get(db: &Db) -> AppResult<Settings> {
    let raw: Option<String> =
        db.with(|c| c.query_row("SELECT value FROM kv WHERE key = 'settings'", [], |r| r.get(0)).optional())?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

pub fn save(db: &Db, s: &Settings) -> AppResult<Settings> {
    s.validate()?;
    let json = serde_json::to_string(s)?;
    db.with(|c| {
        c.execute(
            "INSERT INTO kv(key, value) VALUES ('settings', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [json],
        )
    })?;
    get(db)
}

pub fn kv_get(db: &Db, key: &str) -> AppResult<Option<String>> {
    db.with(|c| c.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0)).optional())
}

pub fn kv_set(db: &Db, key: &str, value: &str) -> AppResult<()> {
    db.with(|c| {
        c.execute(
            "INSERT INTO kv(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_save_and_validation() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(get(&db).unwrap(), Settings::default());
        let s = Settings { theme: "dark".into(), history_retention_days: 30, ..Default::default() };
        assert_eq!(save(&db, &s).unwrap().theme, "dark");
        assert!(save(&db, &Settings { theme: "neon".into(), ..Default::default() }).is_err());
        assert!(save(&db, &Settings { graph_max_nodes: 5, ..Default::default() }).is_err());
        // Partial/old documents fill in defaults.
        kv_set(&db, "settings", r#"{"theme":"light"}"#).unwrap();
        let g = get(&db).unwrap();
        assert_eq!(g.theme, "light");
        assert_eq!(g.default_profile, "full");
    }
}

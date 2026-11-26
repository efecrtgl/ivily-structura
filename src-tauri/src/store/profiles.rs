//! Analysis profiles: built-in (read-only) plus user-defined custom profiles.

use crate::analysis::profile::{builtin, AnalysisProfile};
use crate::core::now_ts;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use rusqlite::params;

pub fn list(db: &Db) -> AppResult<Vec<AnalysisProfile>> {
    let mut out = builtin();
    let custom: Vec<String> = db.with(|c| {
        let mut st = c.prepare("SELECT config FROM profiles ORDER BY name COLLATE NOCASE")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect()
    })?;
    for json in custom {
        if let Ok(mut p) = serde_json::from_str::<AnalysisProfile>(&json) {
            p.builtin = false;
            out.push(p);
        }
    }
    Ok(out)
}

pub fn get(db: &Db, id: &str) -> AppResult<AnalysisProfile> {
    list(db)?.into_iter().find(|p| p.id == id).ok_or_else(|| AppError::NotFound(format!("profile {id}")))
}

pub fn validate(p: &AnalysisProfile) -> AppResult<()> {
    let id_ok =
        !p.id.is_empty() && p.id.len() <= 64 && p.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !id_ok {
        return Err(AppError::validation(
            "profile_id_invalid",
            "profile id must be 1–64 characters: letters, digits, '-' or '_'",
        ));
    }
    let name = p.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::validation("profile_name_invalid", "profile name must be 1–80 characters"));
    }
    if p.scan.exclude.len() > 500 {
        return Err(AppError::validation("too_many_excludes", "at most 500 exclusion patterns"));
    }
    crate::scan::Excluder::new(&p.scan.exclude)?;
    if !(1_000..=5_000_000).contains(&p.scan.max_entries) {
        return Err(AppError::validation("max_entries_invalid", "entry limit must be between 1,000 and 5,000,000"));
    }
    if !(1..=65_536).contains(&p.max_file_kb) {
        return Err(AppError::validation("max_file_invalid", "max file size must be 1 KB – 64 MB"));
    }
    if p.dependencies && !p.content {
        return Err(AppError::validation("deps_need_content", "dependency analysis requires content analysis"));
    }
    let known: Vec<&str> = crate::health::registry().iter().map(|r| r.id).collect();
    if let Some(bad) = p.health_config.disabled_rules.iter().find(|r| !known.contains(&r.as_str())) {
        return Err(AppError::validation("unknown_rule", format!("unknown rule `{bad}`")));
    }
    Ok(())
}

/// Create or update a custom profile. Built-in ids cannot be overwritten.
pub fn save(db: &Db, p: &AnalysisProfile) -> AppResult<AnalysisProfile> {
    if builtin().iter().any(|b| b.id == p.id) {
        return Err(AppError::validation(
            "profile_builtin",
            "built-in profiles cannot be modified; duplicate it first",
        ));
    }
    validate(p)?;
    let mut p = p.clone();
    p.builtin = false;
    p.name = p.name.trim().to_string();
    let json = serde_json::to_string(&p)?;
    db.with(|c| {
        c.execute(
            "INSERT INTO profiles(id, name, config, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, config = excluded.config, updated_at = excluded.updated_at",
            params![p.id, p.name, json, now_ts()],
        )
    })?;
    Ok(p)
}

pub fn delete(db: &Db, id: &str) -> AppResult<()> {
    if builtin().iter().any(|b| b.id == id) {
        return Err(AppError::validation("profile_builtin", "built-in profiles cannot be deleted"));
    }
    let n = db.with(|c| c.execute("DELETE FROM profiles WHERE id = ?1", [id]))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("profile {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_profiles_lifecycle() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(list(&db).unwrap().len(), builtin().len());
        let mut p = get(&db, "full").unwrap();
        assert!(save(&db, &p).is_err(), "cannot overwrite built-in");
        p.id = "my-profile".into();
        p.name = "Mine".into();
        p.scan.exclude.push("fixtures".into());
        save(&db, &p).unwrap();
        let back = get(&db, "my-profile").unwrap();
        assert!(!back.builtin);
        assert!(back.scan.exclude.contains(&"fixtures".to_string()));
        p.health_config.disabled_rules = vec!["nope".into()];
        assert!(save(&db, &p).is_err());
        p.health_config.disabled_rules = vec![];
        p.id = "bad id!".into();
        assert!(save(&db, &p).is_err());
        assert!(delete(&db, "full").is_err());
        delete(&db, "my-profile").unwrap();
        assert!(get(&db, "my-profile").is_err());
    }
}

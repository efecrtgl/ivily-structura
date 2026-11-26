//! SQLite database: connection, migrations, integrity, backup/restore and compressed blobs.

pub mod migrations;

use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct Db {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStatus {
    pub path: Option<String>,
    pub schema_version: i64,
    pub latest_version: i64,
    pub integrity_ok: bool,
    pub size_bytes: u64,
    pub projects: i64,
    pub snapshots: i64,
    pub history_entries: i64,
}

impl Db {
    /// Open (or create) the database file and apply pending migrations.
    ///
    /// Recovery: if the file is corrupt, it is moved aside as `*.corrupt-<ts>` and a fresh
    /// database is created, so the app stays usable. The broken file is kept for manual rescue.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        match Self::open_inner(path) {
            Ok(db) => Ok(db),
            Err(AppError::Database(msg)) if path.exists() && !msg.contains("newer") => {
                let aside = path.with_extension(format!("corrupt-{}", crate::core::now_ts()));
                std::fs::rename(path, &aside)?;
                for ext in ["db-wal", "db-shm"] {
                    let _ = std::fs::remove_file(path.with_extension(ext));
                }
                Self::open_inner(path)
            }
            Err(e) => Err(e),
        }
    }

    fn open_inner(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        let db = Db { conn: Mutex::new(conn), path: Some(path.to_path_buf()) };
        db.configure(true)?;
        db.check_integrity_quick()?;
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let db = Db { conn: Mutex::new(Connection::open_in_memory()?), path: None };
        db.configure(false)?;
        db.migrate()?;
        Ok(db)
    }

    fn configure(&self, file: bool) -> AppResult<()> {
        let c = self.conn.lock();
        c.pragma_update(None, "foreign_keys", "ON")?;
        c.busy_timeout(std::time::Duration::from_secs(5))?;
        if file {
            c.pragma_update(None, "journal_mode", "WAL")?;
            c.pragma_update(None, "synchronous", "NORMAL")?;
        }
        Ok(())
    }

    fn check_integrity_quick(&self) -> AppResult<()> {
        let ok: String = self.conn.lock().query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if ok != "ok" {
            return Err(AppError::Database(format!("integrity check failed: {ok}")));
        }
        Ok(())
    }

    fn migrate(&self) -> AppResult<()> {
        let mut c = self.conn.lock();
        let current: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let latest = migrations::latest_version();
        if current > latest {
            return Err(AppError::Database(format!(
                "database schema ({current}) is newer than this app supports ({latest})"
            )));
        }
        for (i, sql) in migrations::MIGRATIONS.iter().enumerate().skip(current as usize) {
            let tx = c.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> AppResult<T> {
        Ok(f(&self.conn.lock())?)
    }

    pub fn with_tx<T>(&self, f: impl FnOnce(&rusqlite::Transaction) -> AppResult<T>) -> AppResult<T> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn status(&self) -> AppResult<DbStatus> {
        let c = self.conn.lock();
        let schema_version: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let integrity: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        let count = |t: &str| c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0));
        Ok(DbStatus {
            path: self.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            schema_version,
            latest_version: migrations::latest_version(),
            integrity_ok: integrity == "ok",
            size_bytes: self.path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0),
            projects: count("projects")?,
            snapshots: count("snapshots")?,
            history_entries: count("history")?,
        })
    }

    /// Consistent online backup to `dest` (SQLite backup API).
    pub fn backup_to(&self, dest: &Path) -> AppResult<()> {
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        let c = self.conn.lock();
        let mut out = Connection::open(dest)?;
        let b = rusqlite::backup::Backup::new(&c, &mut out)?;
        b.run_to_completion(256, std::time::Duration::from_millis(0), None)?;
        Ok(())
    }

    /// Validate a backup file and replace the live database with it.
    pub fn restore_from(&self, src: &Path) -> AppResult<()> {
        let backup = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| AppError::validation("backup_invalid", "the file is not a readable database"))?;
        let check: String = backup
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|_| AppError::validation("backup_invalid", "the file is not an Ivily Structura database"))?;
        if check != "ok" {
            return Err(AppError::validation("backup_corrupt", "the backup failed its integrity check"));
        }
        let v: i64 = backup.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if v < 1 || v > migrations::latest_version() {
            return Err(AppError::validation("backup_version", format!("unsupported backup schema version {v}")));
        }
        let has_tables: i64 = backup.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('projects','snapshots','history')",
            [],
            |r| r.get(0),
        )?;
        if has_tables != 3 {
            return Err(AppError::validation("backup_invalid", "the file is not an Ivily Structura database"));
        }
        {
            let mut c = self.conn.lock();
            let b = rusqlite::backup::Backup::new(&backup, &mut c)?;
            b.run_to_completion(256, std::time::Duration::from_millis(0), None)?;
        }
        self.configure(self.path.is_some())?;
        self.migrate()
    }
}

/// Compress (deflate) and hash a blob for storage.
pub fn pack(json: &[u8]) -> AppResult<(Vec<u8>, String)> {
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(json)?;
    let data = enc.finish()?;
    let hash = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
    Ok((data, hash))
}

/// Verify the hash and decompress. Refuses tampered/corrupt blobs.
pub fn unpack(data: &[u8], expected_sha256: &str) -> AppResult<Vec<u8>> {
    let hash: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
    if hash != expected_sha256 {
        return Err(AppError::validation("data_corrupt", "stored data failed its integrity check"));
    }
    let mut out = Vec::new();
    // Bound decompression to prevent decompression bombs from imported files.
    flate2::read::DeflateDecoder::new(data)
        .take(512 * 1024 * 1024)
        .read_to_end(&mut out)
        .map_err(|_| AppError::validation("data_corrupt", "stored data could not be decompressed"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_in_memory_and_reports_status() {
        let db = Db::open_in_memory().unwrap();
        let s = db.status().unwrap();
        assert_eq!(s.schema_version, migrations::latest_version());
        assert!(s.integrity_ok);
    }

    #[test]
    fn file_db_backup_and_restore() {
        let t = tempfile::tempdir().unwrap();
        let db = Db::open(&t.path().join("a.db")).unwrap();
        db.with(|c| c.execute("INSERT INTO kv(key, value) VALUES ('k', 'v1')", [])).unwrap();
        let bak = t.path().join("backup.db");
        db.backup_to(&bak).unwrap();
        db.with(|c| c.execute("UPDATE kv SET value = 'v2' WHERE key = 'k'", [])).unwrap();
        db.restore_from(&bak).unwrap();
        let v: String = db.with(|c| c.query_row("SELECT value FROM kv WHERE key='k'", [], |r| r.get(0))).unwrap();
        assert_eq!(v, "v1");
        // Reject non-database files
        let junk = t.path().join("junk.db");
        std::fs::write(&junk, b"not a database at all").unwrap();
        assert!(db.restore_from(&junk).is_err());
    }

    #[test]
    fn refuses_newer_schema_without_touching_it() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("n.db");
        {
            let c = Connection::open(&p).unwrap();
            c.pragma_update(None, "user_version", 999).unwrap();
        }
        assert!(Db::open(&p).is_err());
        assert!(p.exists(), "newer database must not be moved aside");
    }

    #[test]
    fn corrupt_database_is_moved_aside() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("c.db");
        std::fs::write(&p, vec![0x42u8; 8192]).unwrap();
        let db = Db::open(&p).unwrap();
        assert!(db.status().unwrap().integrity_ok);
        assert!(std::fs::read_dir(t.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("corrupt-")));
    }

    #[test]
    fn pack_unpack_detects_tampering() {
        let (data, hash) = pack(b"{\"a\":1}").unwrap();
        assert_eq!(unpack(&data, &hash).unwrap(), b"{\"a\":1}");
        let mut bad = data.clone();
        bad[0] ^= 0xFF;
        assert!(unpack(&bad, &hash).is_err());
    }
}

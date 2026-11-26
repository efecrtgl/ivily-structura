//! Schema migrations. Append-only: never edit a released migration, add a new one.
//! The schema version is stored in `PRAGMA user_version`.

pub const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r#"
    CREATE TABLE kv (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE projects (
        id               INTEGER PRIMARY KEY,
        root             TEXT NOT NULL UNIQUE COLLATE NOCASE,
        name             TEXT NOT NULL,
        profile_id       TEXT,
        config           TEXT NOT NULL DEFAULT '{}',
        pinned           INTEGER NOT NULL DEFAULT 0,
        created_at       INTEGER NOT NULL,
        last_opened_at   INTEGER,
        last_analyzed_at INTEGER,
        last_summary     TEXT
    );

    -- Latest analysis per project, so results survive restarts without re-scanning.
    CREATE TABLE analysis_cache (
        project_id  INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
        created_at  INTEGER NOT NULL,
        data        BLOB NOT NULL,
        data_sha256 TEXT NOT NULL
    );

    CREATE TABLE profiles (
        id         TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        config     TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );

    CREATE TABLE snapshots (
        id             INTEGER PRIMARY KEY,
        project_id     INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name           TEXT NOT NULL,
        note           TEXT NOT NULL DEFAULT '',
        created_at     INTEGER NOT NULL,
        engine_version INTEGER NOT NULL,
        summary        TEXT NOT NULL,
        data           BLOB NOT NULL,
        data_sha256    TEXT NOT NULL,
        size_bytes     INTEGER NOT NULL
    );
    CREATE INDEX idx_snapshots_project ON snapshots(project_id, created_at DESC);

    CREATE TABLE module_groups (
        id          INTEGER PRIMARY KEY,
        project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name        TEXT NOT NULL,
        color       TEXT NOT NULL DEFAULT '',
        description TEXT NOT NULL DEFAULT '',
        patterns    TEXT NOT NULL DEFAULT '[]',
        UNIQUE (project_id, name)
    );

    CREATE TABLE layers (
        id          INTEGER PRIMARY KEY,
        project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name        TEXT NOT NULL,
        rank        INTEGER NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        patterns    TEXT NOT NULL DEFAULT '[]',
        UNIQUE (project_id, name)
    );

    CREATE TABLE annotations (
        id          INTEGER PRIMARY KEY,
        project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        path        TEXT NOT NULL,
        label       TEXT NOT NULL DEFAULT '',
        note        TEXT NOT NULL DEFAULT '',
        important   INTEGER NOT NULL DEFAULT 0,
        entry_point INTEGER NOT NULL DEFAULT 0,
        group_id    INTEGER REFERENCES module_groups(id) ON DELETE SET NULL,
        updated_at  INTEGER NOT NULL,
        UNIQUE (project_id, path)
    );

    CREATE TABLE relations (
        id         INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        from_path  TEXT NOT NULL,
        to_path    TEXT NOT NULL,
        kind       TEXT NOT NULL DEFAULT 'depends-on',
        note       TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL
    );

    CREATE TABLE notes (
        id         INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        title      TEXT NOT NULL,
        body       TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );

    -- Activity/analysis history. `detail` holds counts and identifiers only, never file contents.
    CREATE TABLE history (
        id          INTEGER PRIMARY KEY,
        ts          INTEGER NOT NULL,
        kind        TEXT NOT NULL,
        outcome     TEXT NOT NULL,
        project_id  INTEGER REFERENCES projects(id) ON DELETE SET NULL,
        profile_id  TEXT,
        duration_ms INTEGER,
        title       TEXT NOT NULL DEFAULT '',
        detail      TEXT NOT NULL DEFAULT '{}'
    );
    CREATE INDEX idx_history_ts ON history(ts DESC);
    CREATE INDEX idx_history_project ON history(project_id, ts DESC);
    "#,
];

pub fn latest_version() -> i64 {
    MIGRATIONS.len() as i64
}

//! Ivily Ecosystem Protocol (IEP) 0.1 — optional, local integration surface for Ivily Center.
//!
//! Wire-compatible with the IEP 0.1 used by other Ivily apps (see `docs/IEP.md`):
//! - **Transport:** CLI. `ivily-structura.exe --iep describe` prints one JSON descriptor;
//!   `--iep request` reads one request (≤ 64 KB) from stdin and prints one response. No window,
//!   no listening socket, no background service.
//! - **Read-only:** identity, capabilities, health, update metadata, activity counts, known
//!   projects and their latest analysis summaries. Nothing can be modified through IEP and no
//!   analysis is started.
//! - **Strict:** unknown fields, methods and versions are rejected with explicit error codes.
//!
//! Structura works fully without Ivily Center; this module is never required.

use crate::db::migrations;
use crate::store::projects::Summary;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

pub const PROTOCOL: &str = "iep";
pub const PROTOCOL_VERSION: &str = "0.1.0";
pub const APP_ID: &str = "com.efecrtgl.ivilystructura";
pub const PRODUCT: &str = "ivily-structura";
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub const ACTIVITY_WINDOW_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub min_compatible: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub id: &'static str,
    pub product: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub platform: &'static str,
    pub repository: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportInfo {
    pub kind: &'static str,
    pub describe: [&'static str; 2],
    pub request: [&'static str; 2],
    pub background_service: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Read,
    UserApproved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    Available,
    Planned,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub id: &'static str,
    pub version: &'static str,
    pub access: Access,
    pub status: Availability,
    pub description: &'static str,
}

pub fn capabilities() -> Vec<Capability> {
    use Access::*;
    use Availability::*;
    let c = |id, access, status, description| Capability { id, version: "1", access, status, description };
    vec![
        c("app.identity", Read, Available, "Application identity, version and protocol version."),
        c("app.capabilities", Read, Available, "List of supported capabilities."),
        c("app.health", Read, Available, "Data directory and database state."),
        c("update.metadata", Read, Available, "Result of the last update check. No network request is made."),
        c(
            "activity.summary",
            Read,
            Available,
            "Counts of history entries in the last 7 days by kind and outcome. No titles or paths.",
        ),
        c(
            "projects.list",
            Read,
            Available,
            "Projects known to Structura: name, root path, last analysis time and summary counts.",
        ),
        c(
            "analysis.summary",
            Read,
            Available,
            "Latest analysis summary (counts only) for one project root. Does not start an analysis.",
        ),
        c(
            "analysis.request",
            UserApproved,
            Planned,
            "Center proposes analysing a project; Structura asks the user before scanning.",
        ),
        c("project.open", UserApproved, Planned, "Center asks Structura to open a project view."),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthStatus {
    Ok,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DatabaseState {
    Missing,
    Ok,
    NeedsMigration,
    NewerSchema,
    Unreadable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub status: HealthStatus,
    pub data_dir_exists: bool,
    pub database: DatabaseState,
    pub schema_version: Option<i64>,
    pub expected_schema_version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    pub current_version: &'static str,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub checked_at: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySummary {
    pub window_days: i64,
    pub total: u64,
    pub by_kind: BTreeMap<String, u64>,
    pub by_outcome: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntry {
    pub name: String,
    pub root: String,
    pub last_analyzed_at: Option<i64>,
    pub summary: Option<Summary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub protocol: ProtocolInfo,
    pub app: AppInfo,
    pub transport: TransportInfo,
    pub capabilities: Vec<Capability>,
    pub health: Health,
    pub update: UpdateMetadata,
    pub activity: Option<ActivitySummary>,
    pub project_count: Option<u64>,
    pub generated_at: i64,
}

/// Open an existing database without creating or modifying it.
fn open_existing_read_only(path: &Path) -> Result<rusqlite::Connection, DatabaseState> {
    use rusqlite::OpenFlags;
    if !path.is_file() {
        return Err(DatabaseState::Missing);
    }
    let conn = rusqlite::Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| DatabaseState::Unreadable)?;
    conn.pragma_update(None, "query_only", true).map_err(|_| DatabaseState::Unreadable)?;
    conn.busy_timeout(std::time::Duration::from_secs(2)).map_err(|_| DatabaseState::Unreadable)?;
    Ok(conn)
}

fn read_activity(conn: &rusqlite::Connection, now: i64) -> Option<ActivitySummary> {
    let since = now - ACTIVITY_WINDOW_DAYS * 86_400;
    let mut st =
        conn.prepare("SELECT kind, outcome, COUNT(*) FROM history WHERE ts >= ?1 GROUP BY kind, outcome").ok()?;
    let rows =
        st.query_map([since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))).ok()?;
    let mut s = ActivitySummary { window_days: ACTIVITY_WINDOW_DAYS, ..Default::default() };
    for (kind, outcome, n) in rows.flatten() {
        let n = n.max(0) as u64;
        s.total += n;
        *s.by_kind.entry(kind).or_default() += n;
        *s.by_outcome.entry(outcome).or_default() += n;
    }
    Some(s)
}

fn read_update(conn: &rusqlite::Connection) -> UpdateMetadata {
    let cached: Option<serde_json::Value> = conn
        .query_row("SELECT value FROM kv WHERE key = 'update_last_check'", [], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let latest = cached.as_ref().and_then(|v| v.get("latestVersion")).and_then(|v| v.as_str()).map(String::from);
    let checked_at = cached.as_ref().and_then(|v| v.get("checkedAt")).and_then(|v| v.as_i64());
    let update_available = latest
        .as_deref()
        .and_then(|l| semver::Version::parse(l.trim_start_matches('v')).ok())
        .zip(semver::Version::parse(crate::APP_VERSION).ok())
        .is_some_and(|(l, c)| l > c);
    UpdateMetadata { current_version: crate::APP_VERSION, latest_version: latest, update_available, checked_at }
}

fn read_projects(conn: &rusqlite::Connection) -> Vec<ProjectEntry> {
    let Ok(mut st) = conn.prepare(
        "SELECT name, root, last_analyzed_at, last_summary FROM projects ORDER BY name COLLATE NOCASE LIMIT 500",
    ) else {
        return vec![];
    };
    st.query_map([], |r| {
        let s: Option<String> = r.get(3)?;
        Ok(ProjectEntry {
            name: r.get(0)?,
            root: r.get(1)?,
            last_analyzed_at: r.get(2)?,
            summary: s.and_then(|x| serde_json::from_str(&x).ok()),
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

pub struct Context {
    pub descriptor: Descriptor,
    pub projects: Vec<ProjectEntry>,
}

pub fn context_at(data_dir: &Path, db_path: &Path) -> Context {
    let now = chrono::Utc::now().timestamp();
    let latest = migrations::latest_version();
    let mut schema_version = None;
    let mut activity = None;
    let mut projects = Vec::new();
    let mut update = UpdateMetadata {
        current_version: crate::APP_VERSION,
        latest_version: None,
        update_available: false,
        checked_at: None,
    };
    let database = match open_existing_read_only(db_path) {
        Err(state) => state,
        Ok(conn) => match conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)) {
            Err(_) => DatabaseState::Unreadable,
            Ok(v) => {
                schema_version = Some(v);
                if v > latest {
                    DatabaseState::NewerSchema
                } else if v < 1 {
                    DatabaseState::NeedsMigration
                } else {
                    activity = read_activity(&conn, now);
                    update = read_update(&conn);
                    projects = read_projects(&conn);
                    if v < latest {
                        DatabaseState::NeedsMigration
                    } else {
                        DatabaseState::Ok
                    }
                }
            }
        },
    };
    let healthy = matches!(database, DatabaseState::Ok | DatabaseState::Missing | DatabaseState::NeedsMigration);
    let descriptor = Descriptor {
        protocol: ProtocolInfo { name: PROTOCOL, version: PROTOCOL_VERSION, min_compatible: PROTOCOL_VERSION },
        app: AppInfo {
            id: APP_ID,
            product: PRODUCT,
            name: crate::APP_NAME,
            version: crate::APP_VERSION,
            platform: "windows",
            repository: "https://github.com/efecrtgl/ivily-structura",
        },
        transport: TransportInfo {
            kind: "cli",
            describe: ["--iep", "describe"],
            request: ["--iep", "request"],
            background_service: false,
        },
        capabilities: capabilities(),
        health: Health {
            status: if healthy { HealthStatus::Ok } else { HealthStatus::Degraded },
            data_dir_exists: data_dir.is_dir(),
            database,
            schema_version,
            expected_schema_version: latest,
        },
        update,
        activity,
        project_count: (database == DatabaseState::Ok || database == DatabaseState::NeedsMigration)
            .then_some(projects.len() as u64),
        generated_at: now,
    };
    Context { descriptor, projects }
}

pub fn context() -> Context {
    context_at(&crate::core::data_dir(), &crate::core::db_path())
}

// ---------------------------------------------------------------------------------------------
// Requests

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub protocol: String,
    pub version: String,
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    Describe,
    Health,
    Capabilities,
    Projects,
    AnalysisSummary { root: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtocolError {
    pub code: &'static str,
    pub message: String,
}

fn perr(code: &'static str, message: impl Into<String>) -> ProtocolError {
    ProtocolError { code, message: message.into() }
}

/// Semver: same major; while 0.x, the minor must match too.
pub fn is_compatible(client: &str) -> bool {
    let (Ok(c), Ok(ours)) = (semver::Version::parse(client), semver::Version::parse(PROTOCOL_VERSION)) else {
        return false;
    };
    c.major == ours.major && (ours.major != 0 || c.minor == ours.minor)
}

fn no_params(p: &Option<serde_json::Value>) -> Result<(), ProtocolError> {
    match p {
        None | Some(serde_json::Value::Null) => Ok(()),
        Some(serde_json::Value::Object(m)) if m.is_empty() => Ok(()),
        Some(_) => Err(perr("invalid_params", "this method takes no parameters")),
    }
}

pub fn parse_request(text: &str) -> Result<(Request, Method), ProtocolError> {
    if text.len() > MAX_REQUEST_BYTES {
        return Err(perr("too_large", format!("request must be at most {MAX_REQUEST_BYTES} bytes")));
    }
    let req: Request = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| perr("invalid_request", "malformed request"))?;
    if req.protocol != PROTOCOL {
        return Err(perr("unsupported_protocol", format!("expected protocol '{PROTOCOL}'")));
    }
    if !is_compatible(&req.version) {
        return Err(perr(
            "incompatible_version",
            format!("client version {} is not compatible with IEP {PROTOCOL_VERSION}", req.version),
        ));
    }
    if req.id.is_empty() || req.id.len() > 64 || !req.id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return Err(perr("invalid_id", "id must be 1–64 characters of letters, digits, '-', '_' or '.'"));
    }
    let method = match req.method.as_str() {
        "describe" => no_params(&req.params).map(|_| Method::Describe),
        "health" => no_params(&req.params).map(|_| Method::Health),
        "capabilities" => no_params(&req.params).map(|_| Method::Capabilities),
        "projects" => no_params(&req.params).map(|_| Method::Projects),
        "analysis.summary" => {
            let root = req
                .params
                .as_ref()
                .and_then(|p| p.as_object())
                .filter(|m| m.len() == 1)
                .and_then(|m| m.get("root"))
                .and_then(|r| r.as_str())
                .filter(|r| !r.is_empty() && r.len() <= 1024 && !r.contains('\0'))
                .map(String::from)
                .ok_or_else(|| perr("invalid_params", "expected params {\"root\": \"<absolute project path>\"}"))?;
            Ok(Method::AnalysisSummary { root })
        }
        _ => Err(perr("unknown_method", "unknown method")),
    }?;
    Ok((req, method))
}

fn norm_root(s: &str) -> String {
    s.trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase()
}

pub fn handle_request(text: &str, ctx: impl FnOnce() -> Context) -> serde_json::Value {
    let envelope =
        |id: Option<&str>| serde_json::json!({ "protocol": PROTOCOL, "version": PROTOCOL_VERSION, "id": id });
    let fail = |id: Option<&str>, e: ProtocolError| {
        let mut v = envelope(id);
        v["ok"] = false.into();
        v["error"] = serde_json::to_value(e).unwrap_or_default();
        v
    };
    match parse_request(text) {
        Err(e) => fail(None, e),
        Ok((req, method)) => {
            let c = ctx();
            let result = match method {
                Method::Describe => serde_json::to_value(&c.descriptor),
                Method::Health => serde_json::to_value(&c.descriptor.health),
                Method::Capabilities => serde_json::to_value(&c.descriptor.capabilities),
                Method::Projects => serde_json::to_value(&c.projects),
                Method::AnalysisSummary { root } => {
                    let key = norm_root(&root);
                    match c.projects.iter().find(|p| norm_root(&p.root) == key) {
                        Some(p) => serde_json::to_value(p),
                        None => {
                            return fail(Some(&req.id), perr("not_found", "project is not known to Ivily Structura"))
                        }
                    }
                }
            }
            .unwrap_or_default();
            let mut v = envelope(Some(&req.id));
            v["ok"] = true.into();
            v["result"] = result;
            v
        }
    }
}

/// Handle `--iep` arguments. Returns `None` when the process is not an IEP call (normal start).
pub fn run_cli<I: IntoIterator<Item = String>>(
    args: I,
    stdin: &mut dyn std::io::Read,
    stdout: &mut dyn std::io::Write,
) -> Option<i32> {
    let args: Vec<String> = args.into_iter().collect();
    let pos = args.iter().position(|a| a == "--iep")?;
    match args.get(pos + 1).map(String::as_str) {
        Some("describe") => {
            let _ = writeln!(stdout, "{}", serde_json::to_value(context().descriptor).unwrap_or_default());
            Some(0)
        }
        Some("request") => {
            let mut buf = Vec::new();
            let _ = Read::take(&mut *stdin, MAX_REQUEST_BYTES as u64 + 1).read_to_end(&mut buf);
            let v = handle_request(&String::from_utf8_lossy(&buf), context);
            let ok = v["ok"].as_bool().unwrap_or(false);
            let _ = writeln!(stdout, "{v}");
            Some(if ok { 0 } else { 1 })
        }
        _ => {
            let _ = writeln!(
                stdout,
                "{}",
                serde_json::json!({ "protocol": PROTOCOL, "version": PROTOCOL_VERSION, "ok": false,
                    "error": { "code": "usage", "message": "usage: --iep describe | --iep request (request is read from stdin)" } })
            );
            Some(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn req(over: serde_json::Value) -> String {
        let mut v = serde_json::json!({ "protocol": "iep", "version": "0.1.0", "id": "c-1", "method": "describe" });
        for (k, val) in over.as_object().unwrap() {
            v[k] = val.clone();
        }
        v.to_string()
    }

    #[test]
    fn descriptor_for_missing_database_does_not_create_it() {
        let t = tempfile::tempdir().unwrap();
        let db = t.path().join("structura.db");
        let c = context_at(t.path(), &db);
        assert!(!db.exists());
        assert_eq!(c.descriptor.health.database, DatabaseState::Missing);
        assert_eq!(c.descriptor.health.status, HealthStatus::Ok);
        let v = serde_json::to_value(&c.descriptor).unwrap();
        assert_eq!(v["app"]["id"], APP_ID);
        assert_eq!(v["transport"]["backgroundService"], false);
    }

    #[test]
    fn reads_projects_activity_and_update_without_writing() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("structura.db");
        {
            let db = Db::open(&path).unwrap();
            let p = crate::store::projects::upsert(&db, "C:\\Work\\App", "App").unwrap();
            let mut a = crate::analysis::model::Analysis::default();
            a.metrics.total_files = 9;
            crate::store::projects::record_analysis(&db, p.id, &a).unwrap();
            crate::store::history::record(
                &db,
                crate::store::history::NewEntry {
                    kind: "analysis",
                    outcome: "success",
                    project_id: Some(p.id),
                    profile_id: None,
                    duration_ms: None,
                    title: "secret title".into(),
                    detail: serde_json::json!({}),
                },
            )
            .unwrap();
            crate::store::settings::kv_set(
                &db,
                "update_last_check",
                r#"{"latestVersion":"99.0.0","checkedAt":1700000000}"#,
            )
            .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let c = context_at(t.path(), &path);
        assert_eq!(c.descriptor.health.database, DatabaseState::Ok);
        assert_eq!(c.descriptor.project_count, Some(1));
        assert_eq!(c.descriptor.activity.as_ref().unwrap().total, 1);
        assert!(c.descriptor.update.update_available);
        let s = serde_json::to_string(&c.descriptor).unwrap();
        assert!(!s.contains("secret title"), "activity exposes counts only");
        let ok = handle_request(
            &req(serde_json::json!({ "method": "analysis.summary", "params": { "root": "c:/work/app/" } })),
            || context_at(t.path(), &path),
        );
        assert_eq!(ok["ok"], true, "{ok}");
        assert_eq!(ok["result"]["summary"]["files"], 9);
        let nf = handle_request(
            &req(serde_json::json!({ "method": "analysis.summary", "params": { "root": "C:\\Other" } })),
            || context_at(t.path(), &path),
        );
        assert_eq!(nf["error"]["code"], "not_found");
        assert_eq!(std::fs::read(&path).unwrap(), before, "database must not change");
    }

    #[test]
    fn compatibility_and_validation() {
        assert!(is_compatible("0.1.0") && is_compatible("0.1.9"));
        assert!(!is_compatible("0.2.0") && !is_compatible("1.0.0") && !is_compatible("x"));
        let code = |s: &str| parse_request(s).unwrap_err().code;
        assert_eq!(code("{bad"), "invalid_request");
        assert_eq!(code(&req(serde_json::json!({ "extra": 1 }))), "invalid_request");
        assert_eq!(code(&req(serde_json::json!({ "protocol": "http" }))), "unsupported_protocol");
        assert_eq!(code(&req(serde_json::json!({ "version": "0.2.0" }))), "incompatible_version");
        assert_eq!(code(&req(serde_json::json!({ "id": "a b" }))), "invalid_id");
        for m in ["analyze", "scan", "delete", "exec", "snapshot.create", ""] {
            assert_eq!(code(&req(serde_json::json!({ "method": m }))), "unknown_method", "{m}");
        }
        assert_eq!(code(&req(serde_json::json!({ "params": { "path": "C:\\" } }))), "invalid_params");
        assert_eq!(code(&req(serde_json::json!({ "method": "analysis.summary" }))), "invalid_params");
        assert_eq!(
            code(&req(serde_json::json!({ "method": "analysis.summary", "params": { "root": "C:\\", "x": 1 } }))),
            "invalid_params"
        );
        assert_eq!(code(&" ".repeat(MAX_REQUEST_BYTES + 1)), "too_large");
        assert!(parse_request(&req(serde_json::json!({ "method": "projects", "params": null }))).is_ok());
    }

    #[test]
    fn cli_only_handles_iep_arguments() {
        let mut out = Vec::new();
        let mut empty: &[u8] = b"";
        assert_eq!(run_cli(["--other".to_string()], &mut empty, &mut out), None);
        assert!(out.is_empty());
        assert_eq!(run_cli(["--iep".to_string()], &mut empty, &mut out), Some(2));
        let mut out = Vec::new();
        let mut input: &[u8] = br#"{"protocol":"iep","version":"0.1.0","id":"x","method":"nope"}"#;
        assert_eq!(run_cli(["--iep".to_string(), "request".to_string()], &mut input, &mut out), Some(1));
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["error"]["code"], "unknown_method");
    }
}

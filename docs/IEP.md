# Ivily Ecosystem Protocol (IEP) — Structura 0.1

IEP lets **Ivily Center** discover Ivily apps and read their status locally. It is the same protocol
version (0.1) used by Ivily Optimizer; this document describes Structura's capabilities.
Implementation: `src-tauri/src/iep.rs`.

## Principles

1. **Independence.** Structura works fully without Ivily Center. IEP is optional.
2. **Local only.** No network, no listening port, no background service.
3. **Read-only.** No IEP method changes data or starts an analysis.
4. **User approval.** Future action requests (`analysis.request`, `project.open`) will only be
   *proposals* that Structura shows to the user.
5. **Narrow surface.** Fixed method allow-list; no shell, arbitrary paths or Tauri commands.
6. **Versioned.** Semver; incompatible clients receive an explicit error.

## Transport (0.1): command line

| Call | Behaviour |
|---|---|
| `ivily-structura.exe --iep describe` | Prints the descriptor as one JSON line; exit code `0`. |
| `ivily-structura.exe --iep request` | Reads one request (≤ 64 KB) from stdin, prints one JSON response. `0` on success, `1` on protocol error. |
| invalid `--iep` usage | Usage error JSON, exit code `2`. |

No window is opened and the single-instance lock is not taken, so these calls are safe while the app
is running. The database is **never created or modified**: an existing file is opened with
`query_only`, no migrations run. The release build is a GUI-subsystem executable, so read stdout through
a pipe (e.g. Rust `Command::output()`, .NET `RedirectStandardOutput`).

Installed location: `%LOCALAPPDATA%\Ivily Structura\ivily-structura.exe` (per-user NSIS install).

## Descriptor

```json
{
  "protocol": { "name": "iep", "version": "0.1.0", "minCompatible": "0.1.0" },
  "app": { "id": "com.efecrtgl.ivilystructura", "product": "ivily-structura", "name": "Ivily Structura",
           "version": "0.1.0", "platform": "windows", "repository": "https://github.com/efecrtgl/ivily-structura" },
  "transport": { "kind": "cli", "describe": ["--iep", "describe"], "request": ["--iep", "request"], "backgroundService": false },
  "capabilities": [ { "id": "app.identity", "version": "1", "access": "read", "status": "available", "description": "…" } ],
  "health": { "status": "ok", "dataDirExists": true, "database": "ok", "schemaVersion": 1, "expectedSchemaVersion": 1 },
  "update": { "currentVersion": "0.1.0", "latestVersion": "0.1.0", "updateAvailable": false, "checkedAt": 1795700000 },
  "activity": { "windowDays": 7, "total": 4, "byKind": { "analysis": 3, "snapshot.create": 1 }, "byOutcome": { "success": 4 } },
  "projectCount": 2,
  "generatedAt": 1795721642
}
```

`health.database`: `missing` (never started), `ok`, `needs-migration`, `newer-schema`, `unreadable`.
`health.status` is `degraded` for `newer-schema`/`unreadable`.

## Capabilities

| Id | Access | Status | Description |
|---|---|---|---|
| `app.identity` | read | available | Identity, version, protocol version |
| `app.capabilities` | read | available | Capability list |
| `app.health` | read | available | Data directory and database state |
| `update.metadata` | read | available | Last update check (no network request) |
| `activity.summary` | read | available | History counts for 7 days by kind/outcome — no titles or paths |
| `projects.list` | read | available | Known projects: name, root path, last analysis time, summary counts |
| `analysis.summary` | read | available | Latest analysis summary (counts) for one project root |
| `analysis.request` | user-approved | planned | Center proposes an analysis; Structura asks the user |
| `project.open` | user-approved | planned | Center asks Structura to show a project |

Clients must rely only on `available` capabilities.

## Requests

```json
{ "protocol": "iep", "version": "0.1.0", "id": "center-42", "method": "analysis.summary", "params": { "root": "C:\\code\\app" } }
```

| Method | Params |
|---|---|
| `describe`, `health`, `capabilities`, `projects` | none (`null` or `{}`) |
| `analysis.summary` | `{ "root": "<absolute path>" }` (case- and slash-insensitive match) |

Success: `{ "protocol": "iep", "version": "0.1.0", "id": "center-42", "ok": true, "result": { … } }`
Error: `{ …, "id": null, "ok": false, "error": { "code": "unknown_method", "message": "…" } }`

| Error code | Meaning |
|---|---|
| `too_large` | Request over 64 KB |
| `invalid_request` | Invalid JSON, missing or unknown field |
| `unsupported_protocol` | `protocol` is not `"iep"` |
| `incompatible_version` | Version not compatible |
| `invalid_id` | `id` not 1–64 chars of `[A-Za-z0-9-_.]` |
| `unknown_method` | Method not in the allow-list |
| `invalid_params` | Unexpected or malformed params |
| `not_found` | `analysis.summary` for a root Structura does not know |
| `usage` | Wrong command-line usage |

## Compatibility

- Same major version required; while 0.x the minor version must match too.
- Patch versions (0.1.x) are backwards-compatible fixes.
- New descriptor fields may be added; clients must ignore unknown fields. Requests with unknown fields are rejected.

## Privacy

Project names and root paths of the same Windows user are exposed through `projects.list` /
`analysis.summary` because Center needs them to associate projects. No file contents, file lists,
findings text or history titles are exposed. IEP never requests elevation.

## Future transport (draft)

A per-user named pipe (`\\.\pipe\ivily-iep-<SID>`, `PIPE_REJECT_REMOTE_CLIENTS`) open only while the app
runs, using the same envelope, to support user-approved action requests.

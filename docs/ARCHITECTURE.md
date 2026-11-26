# Architecture

Ivily Structura is a Tauri 2 desktop app: a Rust core does all filesystem access and analysis; a
React (plain JavaScript) UI renders results. The UI has **no** filesystem, shell or HTTP plugin —
every capability goes through validated Rust commands.

```
┌──────────────────────────── UI (src/) ────────────────────────────┐
│ pages/ (Dashboard, Explorer, Graph, Health, Dependencies, ...)    │
│ components/ · lib/api.js (single IPC gateway) · app/store.jsx     │
└───────────────▲───────────────────────────────────────────────────┘
                │ invoke / events (analysis-progress, analysis-done)
┌───────────────┴──────────── commands/ (IPC) ──────────────────────┐
│ validate input → call domain modules → serialize                  │
└───────────────▲───────────────────────────────────────────────────┘
                │
┌───────────────┴──────────── domain (plain Rust, no Tauri) ────────┐
│ scan → lang → providers → project → deps → graph → health         │
│        └────────────── analysis (pipeline, model, metrics) ───────│
│ compare · report · store (projects, profiles, snapshots, mapping, │
│ history, settings) · db (SQLite) · iep (ecosystem protocol)       │
└───────────────────────────────────────────────────────────────────┘
```

## Modules (`src-tauri/src/`)

| Module | Responsibility |
|---|---|
| `core` | App paths, cancellation token, path validation (`validate_root`, `resolve_inside`, `normalize_rel`). |
| `scan` | Directory walking (ripgrep's `ignore` engine): `.gitignore`/`.ignore`, exclusion globs, depth/entry limits, symlinks never followed. `scan::explorer` lists one directory lazily with hidden/ignored/excluded/sensitive flags. |
| `lang` | Language registry (60+), detection by name/extension/shebang, declared **support level**. `lang::mask` masks comments/strings with byte-aligned output and computes line statistics. |
| `providers` | Lexical extractors per language: imports, exports, symbols, complexity indicators, inline tests. |
| `project` | File roles (`roles`), sensitive-file policy, ecosystem/framework/manifest detection (`ecosystem`), structure overview and pattern hints (`structure`). |
| `deps` | Import resolution (`resolve`): relative paths, tsconfig paths, bundler aliases, workspaces, Rust module tree, Python packages, Go packages, C/C++ include roots, JVM/C#/PHP namespaces. Every resolution records its method. |
| `graph` | Iterative Tarjan SCC, shortest concrete cycle, filtered/aggregated graph views for the UI. |
| `health` | Rule engine (`rules.rs`) with thresholds, categories, layers and user entry points. |
| `analysis` | Pipeline orchestration (`run`), result model (`model`), profiles, metrics. Parallel content pass with rayon. |
| `compare` | Differences between two analyses (snapshots over time or two projects). |
| `report` | Markdown/JSON/CSV reports, documentation intelligence (`docs`). |
| `db` | SQLite (WAL), append-only migrations, integrity check, corrupt-file recovery, backup/restore, compressed+hashed blobs. |
| `store` | Persistence for projects (+ cached latest analysis), profiles, settings, snapshots, mapping, history. |
| `iep` | Ivily Ecosystem Protocol CLI surface (`--iep describe|request`). |
| `state`, `commands` | Tauri state (bounded in-memory analysis cache, jobs) and IPC commands. |

## Analysis pipeline

1. **Scan** — entries with size, mtime, hidden flag (dot-name or Windows attribute), symlink flag.
2. **Classify** — language and role per file (path-based, no content).
3. **Ecosystems** — only manifests/markers are read (≤ 2 MB, never sensitive files).
4. **Content pass** (parallel) — files ≤ the profile's size limit and not sensitive/symlinked are
   read once: content hash (duplicates), binary detection, encoding (UTF-8/BOM/UTF-16), shebang
   fallback, generated-file markers, then the language provider.
5. **Resolve** — imports become `Internal` (with method), `External`, `Builtin`, `Local` (dropped) or
   `Unresolved`. Edges are either **uses** (dependencies) or **contains** (Rust `mod`, parent→child
   re-exports); only *uses* edges participate in cycle detection.
6. **Metrics, structure, health** — computed from the in-memory model.

Results are cached per project (compressed, SHA-256 verified) so reopening a project does not require
a rescan. Snapshots are immutable copies of the same model.

## Data storage

`%LOCALAPPDATA%\IvilyStructura\structura.db` — see `db/migrations.rs` for the schema. Analysed projects
are never written to.

## Extending

- **Language**: register in `lang::LANGUAGES`, add an extractor in `providers/extract.rs`, resolution in
  `deps/resolve.rs`, tests with false-positive cases.
- **Health rule**: add a function and a `rule!` entry in `health/rules.rs`; thresholds in
  `health::Thresholds` (serde defaults keep old profiles valid).
- **Ecosystem**: add a parser in `project/ecosystem.rs` and a branch in `detect`.
- **Graph feature**: extend `graph::GraphQuery` (all fields have defaults).
- **Report section**: add to `report::ALL_SECTIONS` and the generators.

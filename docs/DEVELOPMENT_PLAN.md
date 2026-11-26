# Ivily Structura — Development Plan

## 1. Repository audit (2026-11-27)

| Item | Finding |
|---|---|
| Local working copy | Empty except `.claude/` (local tool settings, not committed). No git repository. |
| GitHub `efecrtgl/ivily-structura` | Exists, no commits, no branches. |
| Toolchain | Rust 1.100, Cargo 1.100, Node 24, npm 11. `gh` CLI not installed. |
| Sibling apps | `ivily-envora` and `ivily-optimizer` use Tauri 2 + JS + rusqlite. `ivily-optimizer/docs/IEP.md` defines **IEP 0.1** (CLI transport, read-only, semver). |

Decision: start a fresh repository on `main`, follow the sibling conventions (module layout,
error type, migration pattern, design tokens, CI/release shape) and implement IEP 0.1 so that
Ivily Center can talk to every Ivily app the same way.

## 2. Guiding principles

1. **Read-only on projects.** Structura never writes inside an analyzed project. All state lives in
   the app data directory (`%LOCALAPPDATA%\IvilyStructura`).
2. **Honest analysis.** Every language has a declared *support level*; every finding carries a
   *confidence*; every number is labelled *exact* or *estimated*. No universal "architecture score".
3. **Local-first.** No telemetry, no uploads. The only network call is the optional update check.
4. **Pure core, thin shell.** Analysis lives in plain Rust modules that do not depend on Tauri, so
   it is unit-testable and reusable from the CLI/IEP surface.
5. **Extensible providers.** Languages, ecosystems and health rules are registered in tables /
   trait objects; adding one does not touch the pipeline.

## 3. Architecture

```
src-tauri/src/
  core/        shared types: paths, cancel token, progress, limits
  scan/        directory walking, .gitignore, exclusions, symlink policy, lazy explorer listing
  lang/        language registry + detection (extension, filename, shebang)
  providers/   per-language analyzers (imports, exports, symbols) behind `LanguageProvider`
  project/     ecosystem/framework detection, file role classification, entry points
  deps/        import resolution -> dependency graph, external packages
  graph/       SCC/cycle detection, aggregation (file/dir), focused neighbourhoods
  metrics/     project statistics
  health/      rule engine (configurable thresholds, severity, confidence, evidence)
  analysis/    pipeline orchestration (scan -> detect -> parse -> resolve -> metrics -> health)
  compare/     analysis/snapshot diff
  report/      Markdown / JSON / CSV generation
  db/          SQLite connection, migrations, backup/restore
  store/       projects, profiles, snapshots, annotations (mapping), history, settings
  iep/         Ivily Ecosystem Protocol 0.1
  commands/    Tauri IPC commands (validation + delegation only)
src/           React (plain JS) UI
```

## 4. Phases

| # | Phase | Deliverables |
|---|---|---|
| 0 | Init | Repo, plan, scaffold (Tauri 2 + React + Vite), licence, community files, CI |
| 1 | Scanning & detection | Walker, ignore rules, exclusions, symlink safety, language & ecosystem detection, roles, metrics |
| 2 | Dependencies | Providers (JS/TS, CSS, HTML, Rust, Python, Go, C/C++, C#, Java/Kotlin, PHP, Ruby, Lua, Zig, shell), resolution, graph, cycles |
| 3 | Health | Rule engine + rules, configurable thresholds |
| 4 | Persistence | SQLite migrations, projects, profiles, history, settings, snapshots, compare, mapping/annotations, backup/restore |
| 5 | Reports & IEP | Markdown/JSON/CSV reports, IEP CLI surface |
| 6 | UI | Shell + custom frame, dashboard, explorer, graph workspace, file intelligence, health, snapshots/compare, mapping, reports, history, settings |
| 7 | Release | Icons, NSIS installer, updater, release workflow with SHA-256 sums, docs |

Each phase: implement → test → review → commit → push → document (CHANGELOG / docs).

## 5. Analysis levels (honesty contract)

| Level | Meaning |
|---|---|
| `detected` | Recognised by name/extension only; counted in metrics. |
| `lexical` | Imports/exports/symbols extracted by a comment- and string-aware scanner. Results are reliable for conventional code but can miss dynamic constructs. |
| `semantic` | Parser-based (planned: Tree-sitter for JS/TS, Rust, Python). |

Tree-sitter is deferred deliberately: the lexical providers cover dependency extraction for the
listed languages without native grammar builds; the provider trait allows swapping in a parser per
language later without changing the pipeline.

## 6. Status (2026-11-27)

| Phase | Status | Verification |
|---|---|---|
| 0 Init | done | repo pushed, CI workflow |
| 1 Scanning & detection | done | unit tests (scanner, explorer, languages, masker, roles) |
| 2 Dependencies | done | resolver/provider tests; real projects: 0 unresolved imports on two Tauri apps |
| 3 Health | done | rule trigger/non-trigger tests, layer and entry-point tests |
| 4 Persistence | done | migrations, backup/restore, corrupt-DB recovery, snapshot import/export tests |
| 5 Reports & IEP | done | report format tests, IEP validation and read-only tests |
| 6 UI | done | Vite build, Vitest; app launched and analysis verified end-to-end on a real project |
| 7 Release | done locally | release workflow; installer built locally with signed updater artifacts. First CI release requires the signing secrets (docs/RELEASING.md). |

### Next steps

- Tree-sitter semantic providers for JS/TS, Rust and Python (symbol references, call graph).
- IEP named-pipe transport for user-approved `analysis.request` / `project.open`.
- Localisation (Turkish/English) consistent with other Ivily apps.
- UI component tests with a mocked IPC layer.

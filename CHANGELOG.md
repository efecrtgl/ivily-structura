# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Repository scaffold: Tauri 2 + React (JavaScript) + Vite, MIT licence, community files, CI.
- Development plan and repository audit (`docs/DEVELOPMENT_PLAN.md`).
- Scanner honouring `.gitignore`/`.ignore`, configurable exclusions, depth/entry limits, symlink
  safety and cancellation; lazy explorer listing with ignored/hidden/sensitive flags.
- Language registry (60+ languages) with declared support levels; lexical providers for imports,
  exports, symbols and complexity across 20+ languages.
- Ecosystem and framework detection (npm/pnpm/yarn/bun, Cargo, Go modules, Python, Composer,
  Bundler, Maven/Gradle, .NET, SwiftPM, Pub, CMake, Docker, Terraform).
- Import resolution (relative, tsconfig paths, bundler aliases, workspaces, Rust module paths,
  Python packages, Go packages, C/C++ include roots, JVM/C#/PHP namespaces) with recorded method.
- Dependency graph with containment vs. use edges, cycle detection (iterative Tarjan) and
  concrete cycle paths; aggregated, filterable graph views.
- Architecture health engine with 25 configurable rules, severity, confidence, evidence and
  suggestions — no aggregate score.
- Analysis profiles: quick, full, dependencies, security, performance, documentation.
- SQLite persistence: migrations, integrity checks, corrupt-file recovery, backup/restore; projects
  with cached latest analysis; custom profiles; settings; history with retention.
- Snapshots (compressed, SHA-256 verified, export/import) and comparison of analyses/projects.
- Project mapping: layers, module groups, annotations, relations, notes, visual map.
- Markdown/JSON/CSV reports and documentation intelligence.
- Ivily Ecosystem Protocol 0.1 (read-only CLI surface).
- Desktop UI: dashboard, explorer, architecture graph, health, dependencies, snapshots, mapping,
  reports, history, settings; custom window frame; dark/light themes.
- Command-line opening (`ivily-structura.exe <folder> [--analyze]`).
- Release workflow: NSIS installer, signed updater, portable zip, SHA-256 checksums.

<p align="center"><img src="assets/logo.svg" width="96" alt="Ivily Structura"></p>

<h1 align="center">Ivily Structura</h1>

<p align="center"><b>Understand how a codebase is organised — locally, honestly, fast.</b></p>

Ivily Structura is an open-source, lightweight desktop application (Windows-first) that analyses,
visualises and documents software project structure: directory layout, languages and ecosystems,
file roles, internal and external dependencies, circular dependencies and architecture health. It
helps you get into unfamiliar repositories and keep an understandable overview of your own.

## Features

- **Project explorer** — lazy tree for large repos; `.gitignore`-aware; hidden, ignored, excluded,
  symlinked and sensitive entries flagged; search, filter and sort by size, lines, complexity and
  coupling.
- **Structure analysis** — entry points, source/test/doc roots, file roles, manifests, generated
  files, architectural pattern hints with evidence.
- **60+ languages recognised**; imports, exports and symbols for 30 (JS/TS/JSX/TSX, Vue, Svelte,
  HTML, CSS/SCSS/Less, Rust, C/C++, C#, Go, Java, Kotlin, Swift, Python, PHP, Ruby, Lua, Zig, Shell,
  PowerShell, SQL, GraphQL, Terraform, Docker) — each with a declared support level.
- **Ecosystems & frameworks** — npm/pnpm/yarn/bun, Cargo, Go modules, Python (pip/Poetry/uv/Pipenv),
  Composer, Bundler, Maven/Gradle, .NET, SwiftPM, Pub, CMake, Docker, Terraform; React, Vue, Svelte,
  Next, Tauri, Axum, Django, FastAPI, Laravel, Spring Boot, ASP.NET and more.
- **Dependency graph** — import resolution with a recorded method (relative, tsconfig paths, aliases,
  workspaces, module paths, namespaces…), external packages (declared/undeclared/unused), cycle
  detection with concrete paths, fan-in/fan-out.
- **Interactive architecture graph** — files or folders, focus + depth, filters, layouts, cycles,
  external packages, PNG/JSON export; bounded so large graphs stay readable.
- **Architecture health** — 25 configurable rules (cycles, bottlenecks, coupling, layer violations,
  unused modules, large/complex files, nesting, duplicates, missing docs/tests, conflicting
  lockfiles, risky dependency specs, sensitive files…) with severity, confidence, evidence and
  suggestions. **No opaque score.**
- **Snapshots & comparison** — over time or between projects.
- **Project mapping** — layers, module groups, annotations, relations and notes that survive
  re-analysis, with a visual layer map.
- **Reports** — Markdown, JSON, CSV; configurable sections.
- **Analysis profiles**, persistent history, backup/restore, signed auto-updates.
- **Ivily Ecosystem Protocol** — optional, read-only integration with Ivily Center.

## Principles

- **Read-only.** Projects are never modified.
- **Local-first.** No telemetry, no uploads. Secrets-bearing files (`.env`, keys, credentials) are
  never read.
- **Honest.** Exact vs. estimated values are labelled; every finding states its confidence.

## Documentation

| | |
|---|---|
| [Installation](docs/INSTALLATION.md) | Installer, portable build, building from source |
| [Usage](docs/USAGE.md) | Walkthrough of every view |
| [Analysis](docs/ANALYSIS.md) | What is measured, how reliably, and how rules work |
| [Architecture](docs/ARCHITECTURE.md) | Module layout and extension points |
| [IEP](docs/IEP.md) | Ivily Ecosystem Protocol |
| [Releasing](docs/RELEASING.md) | Versioning strategy and release process |
| [Development plan](docs/DEVELOPMENT_PLAN.md) | Audit and phases |
| [Security](SECURITY.md) · [Contributing](CONTRIBUTING.md) · [Changelog](CHANGELOG.md) | |

## Tech stack

Rust · Tauri 2 · React 19 (plain JavaScript) · SQLite · Cytoscape.js

## Development

```powershell
npm ci
npm run tauri dev        # run the app (dev server on port 1450)
npm test                 # UI tests
cd src-tauri; cargo test # Rust tests
cargo run --release --example analyze -- C:\path\to\project   # CLI analysis summary
```

## Licence

[MIT](LICENSE) © 2026 efecrtgl

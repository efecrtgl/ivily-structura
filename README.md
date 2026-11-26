# Ivily Structura

**Understand how a codebase is organised — locally, honestly, fast.**

Ivily Structura is an open-source, lightweight desktop application (Windows-first) that analyzes,
visualizes and documents software project structure: directory layout, languages and ecosystems,
file roles, internal and external dependencies, circular dependencies and architecture health.

> Status: early development (0.1.x). See [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md).

## Principles

- **Read-only.** Structura never modifies the projects it analyzes.
- **Local-first.** No telemetry, no uploads. Your code never leaves your machine.
- **Honest.** Every language has a declared support level, every finding a confidence and evidence,
  every number is labelled exact or estimated. There is no opaque "architecture score".
- **Independent.** Works on its own; optionally exposes the Ivily Ecosystem Protocol for
  [Ivily Center](https://github.com/efecrtgl).

## Tech stack

Rust · Tauri 2 · React (plain JavaScript) · SQLite

## Development

```powershell
npm ci
npm run tauri dev      # run the app
npm test               # UI tests
cd src-tauri; cargo test
```

## Licence

[MIT](LICENSE)

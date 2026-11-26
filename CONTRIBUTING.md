# Contributing to Ivily Structura

Thanks for your interest! Structura is a local-first, read-only analysis tool. Contributions are
welcome as long as they keep three promises:

1. **Never modify an analyzed project.** All writes go to the app data directory.
2. **Never overclaim.** New analysis must declare its support level (`detected` / `lexical` /
   `semantic`), and findings must carry a confidence and evidence.
3. **Never leak source code.** No telemetry, no uploads, no file contents in logs or errors.

## Development setup

Requirements: Windows 10/11, Rust (stable, MSVC), Node.js 22+, WebView2 runtime.

```powershell
npm ci
npm run tauri dev
```

## Checks (run before opening a PR)

```powershell
npm test                 # UI unit tests (Vitest)
npm run build            # UI build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

## Code layout

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). In short: analysis code lives in plain Rust
modules under `src-tauri/src/` (no Tauri types), IPC commands in `src-tauri/src/commands/` only
validate input and delegate, and the UI (plain JavaScript + React, no TypeScript) lives in `src/`.

### Adding a language provider

1. Register the language in `src-tauri/src/lang/mod.rs` (extensions, comment syntax, support level).
2. Add an extractor in `src-tauri/src/providers/extract.rs` and map it in `provider_for`.
3. Add resolution rules in `src-tauri/src/deps/resolve.rs` if the language has local imports.
4. Add tests in `src-tauri/src/providers/tests.rs` with *false-positive* cases (comments, strings).
5. Document it in [docs/ANALYSIS.md](docs/ANALYSIS.md).

### Adding a health rule

Rules live in `src-tauri/src/health/rules.rs`. Each rule needs an id, default severity, a
configurable threshold (if numeric), a confidence, evidence and a suggestion. Add a test that both
triggers and *does not* trigger the rule.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) in English:
`feat:`, `fix:`, `perf:`, `docs:`, `test:`, `ci:`, `chore:`, `refactor:`.

## Reporting security issues

See [SECURITY.md](SECURITY.md). Please do not open public issues for vulnerabilities.

# Security Policy

## Supported versions

Only the latest released minor version receives security fixes.

## Reporting a vulnerability

Please report vulnerabilities **privately** via
[GitHub security advisories](https://github.com/efecrtgl/ivily-structura/security/advisories/new).
Do not open a public issue. You can expect an initial response within 7 days.

## Security model

Ivily Structura analyzes source code on the local machine.

| Area | Guarantee |
|---|---|
| Project files | Opened **read-only**. Structura never writes, renames or deletes anything inside an analyzed project. |
| Network | No telemetry. No source code is uploaded. The only network request is the optional update check against GitHub Releases (can be disabled). |
| Sensitive files | Files that typically hold secrets (`.env*`, private keys, keystores, credential files, Terraform state, ...) are listed by name and size only; their contents are never read. |
| Symlinks | Symbolic links and junctions are never followed during scans, so a link cannot redirect a scan outside the chosen project. File inspection verifies that the resolved path stays inside the project root. |
| IPC | Every Tauri command validates its input (absolute roots, project-relative paths without `..`, size limits). The frontend has no filesystem, shell or HTTP plugin. A strict CSP is applied. |
| Storage | Analysis results, snapshots and annotations are stored in a local SQLite database under `%LOCALAPPDATA%\IvilyStructura`. Snapshots store structure and metrics, not file contents. |
| Logs/errors | Error messages contain paths and codes, never file contents. |
| IEP | The Ivily Ecosystem Protocol surface is read-only, local (CLI), size-limited and allow-listed. See [docs/IEP.md](docs/IEP.md). |
| Updates | Update packages are signed (minisign); the app refuses unsigned or mismatched packages. Release assets are published with SHA-256 checksums. |

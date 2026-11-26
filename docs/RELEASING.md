# Releasing & versioning

## Versioning strategy

Ivily Structura follows [Semantic Versioning](https://semver.org/):

| Change | Bump |
|---|---|
| Incompatible change to stored data that cannot be migrated, or to the IEP major version | **major** |
| New features, new languages/rules, new database migrations (always backwards-compatible) | **minor** |
| Fixes, analysis accuracy improvements without new features | **patch** |

Related version numbers, each independent of the app version:

- **Database schema** (`PRAGMA user_version`, `src-tauri/src/db/migrations.rs`): append-only migrations.
  A newer app always upgrades older databases; an older app refuses a newer database instead of corrupting it.
- **Analysis engine** (`ENGINE_VERSION` in `src-tauri/src/analysis/model.rs`): bumped when stored
  analyses/snapshots change shape incompatibly. Comparisons across engine versions show a notice.
- **Snapshot export format** (`EXPORT_VERSION`) and **IEP** (`docs/IEP.md`).

The app version lives in `package.json` and `src-tauri/Cargo.toml` (kept equal; CI checks this with
`scripts/check-version.ps1`). `tauri.conf.json` reads it from `package.json`.

## One-time setup: updater signing key

Updates are verified with a minisign key. The public key is in `src-tauri/tauri.conf.json`
(`plugins.updater.pubkey`). The private key is **never** committed.

1. Locate the private key generated for this project (`%USERPROFILE%\.tauri\ivily-structura.key` and
   `ivily-structura.key.password.txt`), or generate a new pair with
   `npx tauri signer generate -w %USERPROFILE%\.tauri\ivily-structura.key` and update `pubkey`.
2. In GitHub → Settings → Secrets and variables → Actions, add:
   - `TAURI_SIGNING_PRIVATE_KEY` — the contents of the `.key` file
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password

Losing the private key means existing installations can no longer auto-update (users must download
the next version manually).

## Release steps

1. Update `CHANGELOG.md` (move items from *Unreleased* to the new version).
2. Bump the version in `package.json` and `src-tauri/Cargo.toml`; run `npm install` and
   `cargo check` so lockfiles update.
3. Commit (`chore: release vX.Y.Z`), tag and push:
   ```powershell
   git tag vX.Y.Z
   git push origin main vX.Y.Z
   ```
4. The **Release** workflow builds and verifies the NSIS installer, signs the updater package, and
   creates a **draft** release with `latest.json`, a portable zip and `SHA256SUMS.txt`.
5. Review the draft (download, verify checksum, install over the previous version), paste the
   changelog section into the notes, and publish. Publishing makes `latest.json` visible to the
   in-app updater.

## Safe upgrades

- The installer (`currentUser` NSIS mode) replaces program files only. User data in
  `%LOCALAPPDATA%\IvilyStructura` (database, snapshots, mappings, settings) is kept on upgrade
  and on uninstall.
- Before an in-app update installs, Structura writes a database backup
  (`before-update-<timestamp>.db`) into the data folder.
- Database migrations run in transactions on first start of the new version.

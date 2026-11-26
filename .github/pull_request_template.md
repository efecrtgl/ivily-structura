## Summary

<!-- What does this change do and why? Related issue: #... -->

## Type

- [ ] Bug fix (`fix:`)
- [ ] Feature (`feat:`)
- [ ] Performance (`perf:`)
- [ ] Documentation (`docs:`)
- [ ] Tests / CI / chore

## Safety checklist

- [ ] Does not write to analyzed projects (all writes go to the app data directory)
- [ ] Does not read contents of sensitive files, does not log file contents
- [ ] New analysis declares its support level; new findings carry confidence + evidence
- [ ] IPC input is validated (paths stay inside the project, sizes bounded)

## Verification

- [ ] `npm test` and `npm run build`
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (in `src-tauri/`)
- [ ] `CHANGELOG.md` / `docs/` updated where relevant

## Screenshots

<!-- For UI changes: dark and light theme. -->

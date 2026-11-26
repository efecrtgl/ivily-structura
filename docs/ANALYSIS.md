# How Structura analyses a project

This document explains what each number and finding means and how reliable it is.

## Support levels

| Level | Languages | What is extracted |
|---|---|---|
| **Lexical** | JavaScript, JSX, TypeScript, TSX, Vue, Svelte, HTML, CSS, SCSS/Sass, Less, Rust, C, C++, C#, Go, Java, Kotlin, Swift, Python, PHP, Ruby, Lua, Zig, Shell, PowerShell, SQL, GraphQL, Terraform/HCL, Dockerfile | imports, exports, symbols (names + lines), decision points, nesting depth, line statistics |
| **Detected** | Markdown, reStructuredText, AsciiDoc, YAML, TOML, XML, JSON, INI, Makefile, CMake, Gradle, Protobuf, Assembly, Batch, Dart, Scala, Elixir, Erlang, Haskell, R, Perl, Objective-C, F#, Visual Basic, Solidity, Nix, shaders, CSV, plain text | file counts, sizes, line statistics |

"Lexical" means a comment- and string-aware scanner with per-language patterns, not a full parser.
It is reliable for conventional code. It **does not** see: computed module paths (`require(x)`),
macro-generated modules, reflection/DI containers, runtime plugin loading, aliases defined only in
code it cannot read. Such imports are reported as *unresolved* — never guessed.

Tree-sitter-based *semantic* providers are planned; the provider interface allows replacing a
lexical provider per language without touching the pipeline.

## Exact vs. estimated

| Measure | Kind |
|---|---|
| File/directory counts, sizes, depth, total and blank lines | exact |
| Code lines, comment lines | estimated (lexical masking) |
| Decision points ("complexity"), nesting depth | estimated (keyword/brace counting, not cyclomatic complexity) |
| Internal edges, fan-in/out, cycles | exact over *statically resolved* imports |
| External packages, declared/undeclared | exact for Node/Rust; Python import names may differ from distribution names |

## Import resolution methods

Shown in the Dependencies view so you can see how much relies on heuristics:

`relative`, `root-relative` (HTML/Vite `/src/...`), `tsconfig-paths`, `tsconfig-baseUrl`,
`bundler-alias` (only literal aliases found in vite/webpack/... configs), `workspace-package`,
`mod-declaration`, `mod-path-attribute`, `module-path`, `child-module`, `go-package`,
`include-root`, `unique-suffix` (a header path that matches exactly one file),
`fully-qualified-name`, `package`, `namespace` (C# — one using can map to many files), `psr-4`,
`load-path`, `module-dir`.

## Containment vs. dependency

Rust `mod x;` declarations and parent modules re-exporting their children are **containment**, not
dependencies. They are drawn dashed in the graph and excluded from cycle detection, so the normal
`mod.rs` ↔ child pattern does not appear as a cycle. Lazy `import()` cycles are reported with low
severity because they cannot cause initialisation-order problems.

## File roles

Path-based: `source`, `test`, `config`, `documentation`, `build`, `manifest`, `lockfile`, `script`,
`asset`, `generated` (also detected from `@generated`/`DO NOT EDIT` headers), `data`, `other`.

## Sensitive files

`.env*` (except `.env.example`/`.sample`/`.template`), private keys and certificates, keystores,
`id_rsa*`, `.npmrc`, `.pypirc`, `.netrc`, credential/secret JSON/YAML, Terraform state are listed by name
and size only. Their contents are never read, hashed or parsed.

## Health rules

See *Health → Rules* in the app for the live list. Each finding carries:

- **Severity** — info, low, medium, high
- **Confidence** — how reliable the underlying analysis is for this case
- **Evidence** — e.g. the concrete import lines forming a cycle
- **Measured value and threshold** for numeric rules
- **Suggestion**

There is intentionally **no aggregate architecture score**: a single number hides context and invites
optimising the metric instead of the code. Thresholds, enabled categories and individual rules are
configurable per analysis profile. Layer rules use the layers defined in *Project mapping*; entry
points marked there silence the "potentially unused module" rule.

Notable safeguards against false positives:

- *Potentially unused modules* is skipped entirely when more than 25 % of local imports are
  unresolved, and only evaluated for languages with path-exact resolution. Framework convention
  folders (`pages/`, `routes/`, `app/`, ...), entry files and configs are excluded.
- *Missing tests* counts inline Rust `#[cfg(test)]`/`#[test]` and Zig `test` blocks.

## Performance

Measured on a Windows 11 laptop (release build, warm file cache):

| Project | Files | Time (full profile) |
|---|---|---|
| Tauri + React app (`ivily-envora`) | 175 | ≈ 50–170 ms |
| Tauri + Svelte app (`ivily-optimizer`) | 165 | ≈ 90 ms |
| `node_modules` tree (JS/TS) | 2,263 (74 MB) | ≈ 1.3 s |

Scanning is sequential; reading and parsing run in parallel. The UI receives throttled progress
events and can cancel at any time. Graph views are bounded (configurable node limit) and can be
aggregated to folder level.

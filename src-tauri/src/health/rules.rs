//! Health rules. Each rule documents what it measures and how confident it can be.

use super::{HealthConfig, Rule, RuleInfo};
use crate::analysis::model::{
    Analysis, Confidence, EdgeKind, FileRecord, Finding, ImportTarget, MetricValue, Severity,
};
use crate::project::roles::Role;
use crate::providers::ImportKind;
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_FILES_LISTED: usize = 100;

macro_rules! rule {
    ($id:expr, $cat:expr, $title:expr, $desc:expr, $deps:expr, $f:expr) => {
        Rule {
            info: RuleInfo { id: $id, category: $cat, title: $title, description: $desc, requires_dependencies: $deps },
            run: $f,
        }
    };
}

pub fn all() -> Vec<Rule> {
    vec![
        rule!("circular-dependency", "dependencies", "Circular dependencies", "Files that (transitively) import each other. Cycles make modules impossible to understand, test or change in isolation.", true, circular),
        rule!("architectural-bottleneck", "dependencies", "Architectural bottlenecks", "Heavily used files that are also part of a dependency cycle.", true, bottleneck),
        rule!("hub-module", "dependencies", "Highly depended-upon modules", "Files imported by many others. Not a defect by itself, but changes to them have a wide blast radius.", true, hub),
        rule!("high-fan-out", "dependencies", "Excessive dependency coupling", "Files that import many other project files.", true, fan_out),
        rule!("layer-violation", "dependencies", "Layer violations", "Imports that go against the layer order defined in Project Mapping.", true, layer_violation),
        rule!("unreferenced-module", "dependencies", "Potentially unused modules", "Source files no other project file imports and that are not entry points. Only evaluated for languages with reliable import resolution.", true, unreferenced),
        rule!("disconnected-cluster", "dependencies", "Disconnected module clusters", "Groups of files that import each other but are not connected to the rest of the project or an entry point.", true, disconnected),
        rule!("undeclared-dependency", "dependencies", "Undeclared external packages", "Packages imported in code but missing from the nearest manifest.", true, undeclared),
        rule!("unused-declared-dependency", "dependencies", "Declared but never imported", "Runtime dependencies listed in package.json that no source file imports.", true, unused_declared),
        rule!("risky-dependency-spec", "dependencies", "High-risk dependency specifications", "Dependencies pinned to wildcards, `latest`, git URLs or remote archives.", false, risky_specs),
        rule!("large-file", "size", "Large files", "Source files with many lines.", false, large_files),
        rule!("complex-file", "size", "Unusually complex files", "Files with many decision points or deeply nested blocks (lexical estimate).", false, complex_files),
        rule!("deep-nesting", "structure", "Deeply nested directories", "Directory chains deeper than the configured limit.", false, deep_dirs),
        rule!("wide-directory", "structure", "Overcrowded directories", "Directories directly containing very many files.", false, wide_dirs),
        rule!("unnecessary-nesting", "structure", "Unnecessary directory nesting", "Chains of folders that each contain only one sub-folder and no files.", false, single_child_chains),
        rule!("duplicate-files", "structure", "Duplicate files", "Files with byte-identical content.", false, duplicates),
        rule!("similar-structure", "structure", "Suspiciously similar directories", "Directories containing exactly the same set of file names.", false, similar_dirs),
        rule!("mixed-concerns", "structure", "Unclear separation of concerns", "Directories that directly mix source, tests, configuration, documentation and assets.", false, mixed_concerns),
        rule!("misplaced-source", "structure", "Unusually placed source files", "Source code inside documentation or asset folders.", false, misplaced),
        rule!("missing-readme", "documentation", "Missing README", "The project (or a package inside it) has no README.", false, missing_readme),
        rule!("missing-docs", "documentation", "Little documentation", "Larger codebases without a documentation folder or Markdown files.", false, missing_docs),
        rule!("missing-tests", "testing", "Test structure", "No or very few test files compared to source files.", false, missing_tests),
        rule!("conflicting-lockfiles", "configuration", "Conflicting package managers", "More than one lockfile for the same package manifest.", false, lockfiles),
        rule!("duplicate-tool-config", "configuration", "Duplicate tool configuration", "Several configuration files for the same tool in one directory.", false, duplicate_config),
        rule!("sensitive-files", "security", "Sensitive files in the project tree", "Files that usually contain secrets and are not excluded from version control.", false, sensitive),
    ]
}

fn sev_for(value: f64, medium: f64, high: f64) -> Severity {
    if value >= high {
        Severity::High
    } else if value >= medium {
        Severity::Medium
    } else {
        Severity::Low
    }
}

fn list(files: impl IntoIterator<Item = String>) -> Vec<String> {
    files.into_iter().take(MAX_FILES_LISTED).collect()
}

fn is_entry(a: &Analysis, cfg: &HealthConfig, path: &str) -> bool {
    a.structure.entry_points.iter().any(|e| e.path == path)
        || cfg
            .extra_entry_points
            .iter()
            .any(|p| path == p || path.starts_with(&format!("{}/", p.trim_end_matches('/'))))
}

/// Languages whose local imports Structura resolves to exact files (not namespaces/packages).
const RELIABLE_RESOLUTION: &[&str] =
    &["javascript", "jsx", "typescript", "tsx", "vue", "svelte", "python", "rust", "css", "scss", "less"];

/// Evidence line for an edge, preferring real imports over `mod` declarations.
fn import_line(f: &FileRecord, to: u32) -> Option<String> {
    let mut hits: Vec<&crate::analysis::model::FileImport> = f
        .imports
        .iter()
        .filter(|i| matches!(&i.target, ImportTarget::Internal { files, .. } if files.contains(&to)))
        .collect();
    hits.sort_by_key(|i| (i.kind == ImportKind::Mod, i.kind == ImportKind::Dynamic));
    hits.first().map(|i| {
        let how = if i.kind == ImportKind::Dynamic { " (dynamic import)" } else { "" };
        format!("{}:{} imports '{}'{how}", f.path, i.line, i.spec)
    })
}

/// True if every import from `from` to `to` is a dynamic `import()`.
fn only_dynamic(f: &FileRecord, to: u32) -> bool {
    let mut any = false;
    for i in &f.imports {
        if let ImportTarget::Internal { files, .. } = &i.target {
            if files.contains(&to) {
                if i.kind != ImportKind::Dynamic {
                    return false;
                }
                any = true;
            }
        }
    }
    any
}

// ------------------------------------------------------------------------------------------
// Dependencies

fn circular(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    for c in &a.graph.cycles {
        let size = c.files.len();
        let reliable = c
            .files
            .iter()
            .all(|i| a.files[*i as usize].language.as_deref().is_some_and(|l| RELIABLE_RESOLUTION.contains(&l)));
        let mut evidence = Vec::new();
        let p = &c.path;
        let chain: Vec<&str> = p.iter().chain(p.first()).map(|i| a.files[*i as usize].path.as_str()).collect();
        evidence.push(format!("Cycle: {}", chain.join(" → ")));
        for w in p.windows(2).chain(std::iter::once(&[*p.last().unwrap(), p[0]][..])) {
            if let Some(l) = import_line(&a.files[w[0] as usize], w[1]) {
                evidence.push(l);
            }
        }
        let loop_edges: Vec<(u32, u32)> =
            p.windows(2).map(|w| (w[0], w[1])).chain(std::iter::once((*p.last().unwrap(), p[0]))).collect();
        let lazy = loop_edges.iter().any(|(x, y)| only_dynamic(&a.files[*x as usize], *y));
        if lazy {
            evidence.push("The loop includes a dynamic import(): modules load lazily, so this cycle cannot cause initialisation-order problems, but the files are still coupled.".into());
        }
        out.push(Finding {
            rule: "circular-dependency".into(),
            category: "dependencies".into(),
            severity: if lazy {
                Severity::Low
            } else if size as u32 >= cfg.thresholds.large_cycle {
                Severity::High
            } else {
                Severity::Medium
            },
            confidence: if reliable { Confidence::High } else { Confidence::Medium },
            title: format!("Circular dependency between {size} files"),
            explanation: "These files depend on each other in a loop. None of them can be loaded, tested or refactored without the others, and changes ripple around the cycle.".into(),
            files: list(c.files.iter().map(|i| a.files[*i as usize].path.clone())),
            evidence,
            suggestion: "Break the loop: move the shared code into a lower-level module both sides import, invert one dependency (callbacks, events, interfaces), or merge files that are really one unit.".into(),
            metric: Some(MetricValue { name: "files in cycle".into(), value: size as f64, threshold: cfg.thresholds.large_cycle as f64 }),
        });
    }
}

fn bottleneck(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let in_cycle: HashSet<u32> = a.graph.cycles.iter().flat_map(|c| c.files.iter().copied()).collect();
    let limit = (cfg.thresholds.high_fan_in / 2).max(5);
    for (i, f) in a.files.iter().enumerate() {
        if in_cycle.contains(&(i as u32)) && f.fan_in >= limit {
            out.push(Finding {
                rule: "architectural-bottleneck".into(),
                category: "dependencies".into(),
                severity: Severity::High,
                confidence: Confidence::Medium,
                title: format!("Bottleneck: {}", f.path),
                explanation: format!("{} is imported by {} files and is part of a dependency cycle, so a change here propagates to a large part of the codebase.", f.path, f.fan_in),
                files: vec![f.path.clone()],
                evidence: vec![format!("fan-in {} (limit {limit}), member of a cycle", f.fan_in)],
                suggestion: "Stabilise this module: remove its dependencies on higher-level code (breaking the cycle) and keep its public surface small.".into(),
                metric: Some(MetricValue { name: "fan-in".into(), value: f.fan_in as f64, threshold: limit as f64 }),
            });
        }
    }
}

fn hub(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let modules = a.metrics.modules_in_graph.max(1) as f64;
    let mut hubs: Vec<&FileRecord> = a.files.iter().filter(|f| f.fan_in >= cfg.thresholds.high_fan_in).collect();
    hubs.sort_by_key(|f| std::cmp::Reverse(f.fan_in));
    for f in hubs.into_iter().take(20) {
        let share = f.fan_in as f64 / modules;
        out.push(Finding {
            rule: "hub-module".into(),
            category: "dependencies".into(),
            severity: if share >= 0.3 && modules >= 20.0 { Severity::Medium } else { Severity::Low },
            confidence: Confidence::High,
            title: format!("{} is used by {} files", f.path, f.fan_in),
            explanation: format!("{:.0}% of the modules in the dependency graph import this file directly. Central modules are normal (utilities, types), but they concentrate risk.", share * 100.0),
            files: vec![f.path.clone()],
            evidence: vec![format!("fan-in {}, fan-out {}", f.fan_in, f.fan_out)],
            suggestion: "Keep it stable and well tested. If it mixes unrelated responsibilities, split it so dependents only pull in what they need.".into(),
            metric: Some(MetricValue { name: "fan-in".into(), value: f.fan_in as f64, threshold: cfg.thresholds.high_fan_in as f64 }),
        });
    }
}

fn fan_out(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let mut v: Vec<&FileRecord> = a.files.iter().filter(|f| f.fan_out >= cfg.thresholds.high_fan_out).collect();
    v.sort_by_key(|f| std::cmp::Reverse(f.fan_out));
    for f in v.into_iter().take(20) {
        out.push(Finding {
            rule: "high-fan-out".into(),
            category: "dependencies".into(),
            severity: sev_for(f.fan_out as f64, cfg.thresholds.high_fan_out as f64, cfg.thresholds.high_fan_out as f64 * 2.0),
            confidence: Confidence::High,
            title: format!("{} imports {} project files", f.path, f.fan_out),
            explanation: "Files that depend on many others are fragile: any of those dependencies changing can break them, and they are hard to test in isolation.".into(),
            files: vec![f.path.clone()],
            evidence: vec![format!("fan-out {} (threshold {})", f.fan_out, cfg.thresholds.high_fan_out)],
            suggestion: "Check whether the file has several responsibilities that could be split, or whether it is a composition root (then this is expected).".into(),
            metric: Some(MetricValue { name: "fan-out".into(), value: f.fan_out as f64, threshold: cfg.thresholds.high_fan_out as f64 }),
        });
    }
}

fn layer_of(cfg: &HealthConfig, sets: &[globset::GlobSet], path: &str) -> Option<usize> {
    let _ = cfg;
    sets.iter().position(|s| s.is_match(path))
}

fn layer_violation(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    if cfg.layers.len() < 2 {
        return;
    }
    let sets: Vec<globset::GlobSet> = cfg
        .layers
        .iter()
        .map(|l| {
            let mut b = globset::GlobSetBuilder::new();
            for p in &l.patterns {
                let p = p.trim().trim_end_matches('/');
                for g in [p.to_string(), format!("{p}/**")] {
                    if let Ok(g) = globset::Glob::new(&g) {
                        b.add(g);
                    }
                }
            }
            b.build().unwrap_or_else(|_| globset::GlobSet::empty())
        })
        .collect();
    let layers: Vec<Option<usize>> = a.files.iter().map(|f| layer_of(cfg, &sets, &f.path)).collect();
    let mut groups: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
    for e in a.graph.edges.iter().filter(|e| e.kind == EdgeKind::Uses) {
        if let (Some(lf), Some(lt)) = (layers[e.from as usize], layers[e.to as usize]) {
            if lt < lf {
                let from = &a.files[e.from as usize];
                let ev = import_line(from, e.to)
                    .unwrap_or_else(|| format!("{} → {}", from.path, a.files[e.to as usize].path));
                groups.entry((lf, lt)).or_default().push(ev);
            }
        }
    }
    for ((lf, lt), ev) in groups {
        let n = ev.len();
        out.push(Finding {
            rule: "layer-violation".into(),
            category: "dependencies".into(),
            severity: Severity::Medium,
            confidence: Confidence::High,
            title: format!("Layer \"{}\" depends on higher layer \"{}\" ({n} imports)", cfg.layers[lf].name, cfg.layers[lt].name),
            explanation: "Your project mapping declares an order of layers where lower layers must not depend on higher ones.".into(),
            files: list(ev.iter().map(|e| e.split(':').next().unwrap_or("").to_string()).collect::<std::collections::BTreeSet<_>>()),
            evidence: ev.into_iter().take(50).collect(),
            suggestion: "Move the needed code down into the lower layer or invert the dependency (interfaces, callbacks, events).".into(),
            metric: None,
        });
    }
}

const ROUTE_DIRS: &[&str] = &[
    "pages",
    "routes",
    "app",
    "api",
    "migrations",
    "bin",
    "scripts",
    "public",
    "static",
    "stories",
    "examples",
    "example",
    "demo",
    "e2e",
    "fixtures",
    "benches",
    "benchmarks",
    "plugins",
    "commands",
    "layouts",
    "middleware",
    "views",
];
const ENTRY_STEMS: &[&str] = &[
    "index",
    "main",
    "lib",
    "mod",
    "app",
    "__init__",
    "__main__",
    "setup",
    "conftest",
    "server",
    "cli",
    "build",
    "+page",
    "+layout",
    "+server",
    "_app",
    "_document",
    "page",
    "layout",
    "route",
    "middleware",
    "manage",
    "wsgi",
    "asgi",
    "worker",
    "sw",
    "service-worker",
    "vite-env",
];

fn unreferenced(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let g = &a.graph;
    // Skip when too many imports are unresolved: absence of evidence is not evidence.
    let local_attempts = g.imports_internal + g.imports_unresolved;
    if local_attempts == 0 || g.imports_unresolved as f64 / local_attempts as f64 > 0.25 {
        return;
    }
    let mut incoming = vec![false; a.files.len()];
    for e in &g.edges {
        incoming[e.to as usize] = true;
    }
    let mut hits = Vec::new();
    for (i, f) in a.files.iter().enumerate() {
        if incoming[i] || f.role() != Role::Source || is_entry(a, cfg, &f.path) {
            continue;
        }
        let Some(lang) = f.language.as_deref() else { continue };
        if !RELIABLE_RESOLUTION.contains(&lang) || f.path.ends_with(".d.ts") {
            continue;
        }
        let lower = f.path.to_ascii_lowercase();
        let stem = f.name().split('.').next().unwrap_or("").to_ascii_lowercase();
        if ENTRY_STEMS.contains(&stem.as_str()) || lower.contains(".config.") || lower.contains(".stories.") {
            continue;
        }
        if lower.split('/').any(|seg| ROUTE_DIRS.contains(&seg)) {
            continue;
        }
        // Rust files are reachable through `mod` declarations (Contains edges).
        hits.push(f.path.clone());
    }
    if hits.is_empty() {
        return;
    }
    let n = hits.len();
    out.push(Finding {
        rule: "unreferenced-module".into(),
        category: "dependencies".into(),
        severity: Severity::Low,
        confidence: if g.imports_unresolved == 0 { Confidence::Medium } else { Confidence::Low },
        title: format!("{n} source file(s) are not imported by any project file"),
        explanation: "No static import of these files was found and they are not recognised entry points. They may be unused — or loaded dynamically, by a framework convention, a build tool or tests outside the scan.".into(),
        files: list(hits),
        evidence: vec![
            format!("{} of {} local imports resolved", g.imports_internal, local_attempts),
            "Only files in languages with path-exact import resolution are evaluated.".into(),
        ],
        suggestion: "Verify each file before deleting it. Mark intentional entry points in Project Mapping to silence this finding.".into(),
        metric: None,
    });
}

fn disconnected(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let n = a.files.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let nx = p[c];
            p[c] = r;
            c = nx;
        }
        r
    }
    let mut in_graph = vec![false; n];
    for e in &a.graph.edges {
        in_graph[e.from as usize] = true;
        in_graph[e.to as usize] = true;
        let (x, y) = (find(&mut parent, e.from as usize), find(&mut parent, e.to as usize));
        if x != y {
            parent[x] = y;
        }
    }
    let mut comps: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in (0..n).filter(|i| in_graph[*i]) {
        let r = find(&mut parent, i);
        comps.entry(r).or_default().push(i);
    }
    if comps.len() < 2 {
        return;
    }
    let largest = comps.values().map(|v| v.len()).max().unwrap_or(0);
    let mut isolated: Vec<Vec<usize>> = comps
        .into_values()
        .filter(|c| c.len() >= 3 && c.len() < largest)
        .filter(|c| !c.iter().any(|i| is_entry(a, cfg, &a.files[*i].path) || a.files[*i].role() == Role::Test))
        .collect();
    isolated.sort_by_key(|c| std::cmp::Reverse(c.len()));
    for c in isolated.into_iter().take(10) {
        let files: Vec<String> = c.iter().map(|i| a.files[*i].path.clone()).collect();
        out.push(Finding {
            rule: "disconnected-cluster".into(),
            category: "dependencies".into(),
            severity: Severity::Info,
            confidence: Confidence::Low,
            title: format!("Cluster of {} files not connected to the main graph", files.len()),
            explanation: "These files import each other but neither import nor are imported by the rest of the project, and contain no recognised entry point.".into(),
            evidence: vec![format!("component size {} vs. main component {largest}", files.len())],
            files: list(files),
            suggestion: "This may be a separate tool, a plugin loaded at runtime, or leftover code. Mark its entry point in Project Mapping if intentional.".into(),
            metric: None,
        });
    }
}

const TOOLING_DEPS: &[&str] = &[
    "typescript",
    "vite",
    "webpack",
    "rollup",
    "esbuild",
    "eslint",
    "prettier",
    "tailwindcss",
    "postcss",
    "autoprefixer",
    "nodemon",
    "concurrently",
    "cross-env",
    "husky",
    "lint-staged",
    "vitest",
    "jest",
    "mocha",
    "turbo",
    "nx",
    "rimraf",
    "dotenv-cli",
    "ts-node",
    "tsx",
    "sass",
    "less",
    "core-js",
    "regenerator-runtime",
    "tslib",
];

fn manifest_for<'a>(a: &'a Analysis, file: &str, eco: &str) -> Option<&'a crate::project::ecosystem::Ecosystem> {
    a.ecosystems
        .iter()
        .filter(|e| e.id == eco && (e.root.is_empty() || file.starts_with(&format!("{}/", e.root))))
        .max_by_key(|e| e.root.len())
}

fn undeclared(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let mut missing: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for p in &a.graph.packages {
        if p.declared || !matches!(p.ecosystem.as_str(), "node" | "python" | "rust") {
            continue;
        }
        for u in p.users.iter().take(20) {
            let user = &a.files[*u as usize];
            if user.role() == Role::Test || user.role() == Role::Script || user.role() == Role::Build {
                continue;
            }
            if let Some(m) = manifest_for(a, &user.path, &p.ecosystem) {
                missing.entry((p.name.clone(), m.manifest.clone())).or_default().push(user.path.clone());
            }
        }
    }
    let by_eco: Vec<_> = missing.into_iter().collect();
    if by_eco.is_empty() {
        return;
    }
    let python =
        by_eco.iter().all(|((n, _), _)| a.graph.packages.iter().any(|p| p.name == *n && p.ecosystem == "python"));
    let n = by_eco.len();
    let files: Vec<String> = by_eco
        .iter()
        .flat_map(|(_, v)| v.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    out.push(Finding {
        rule: "undeclared-dependency".into(),
        category: "dependencies".into(),
        severity: Severity::Medium,
        confidence: if python { Confidence::Low } else { Confidence::Medium },
        title: format!("{n} imported package(s) not declared in a manifest"),
        explanation: "Code imports these packages but the nearest manifest does not list them. Builds may rely on transitive or globally installed packages and break unexpectedly. (Python import names can differ from distribution names, e.g. `yaml` vs `PyYAML`.)".into(),
        files: list(files),
        evidence: by_eco.iter().take(50).map(|((pkg, manifest), users)| format!("`{pkg}` used in {} (first: {}) — not in {manifest}", users.len(), users[0])).collect(),
        suggestion: "Declare each package explicitly in the manifest with a version range.".into(),
        metric: None,
    });
}

fn unused_declared(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    for e in a.ecosystems.iter().filter(|e| e.id == "node") {
        let used: HashSet<&str> = a
            .graph
            .packages
            .iter()
            .filter(|p| p.ecosystem == "node" && p.use_count > 0)
            .map(|p| p.name.as_str())
            .collect();
        // Only evaluate if this package has analysed source files.
        let prefix = if e.root.is_empty() { String::new() } else { format!("{}/", e.root) };
        if !a.files.iter().any(|f| f.path.starts_with(&prefix) && f.role() == Role::Source && !f.imports.is_empty()) {
            continue;
        }
        let unused: Vec<String> = e
            .dependencies
            .iter()
            .filter(|d| d.kind == crate::project::ecosystem::DepKind::Runtime && d.source == "registry")
            .filter(|d| {
                !used.contains(d.name.as_str())
                    && !d.name.starts_with("@types/")
                    && !TOOLING_DEPS.contains(&d.name.as_str())
            })
            .map(|d| d.name.clone())
            .collect();
        if unused.is_empty() {
            continue;
        }
        out.push(Finding {
            rule: "unused-declared-dependency".into(),
            category: "dependencies".into(),
            severity: Severity::Info,
            confidence: Confidence::Low,
            title: format!("{} dependencies in {} are never imported", unused.len(), e.manifest),
            explanation: "No analysed file imports these runtime dependencies. They may be used by configuration files, CLIs, CSS, or loaded dynamically.".into(),
            files: vec![e.manifest.clone()],
            evidence: unused.iter().take(50).map(|n| format!("`{n}`")).collect(),
            suggestion: "Confirm each one is really unused before removing it; move build-only tools to devDependencies.".into(),
            metric: None,
        });
    }
}

fn risky_specs(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let mut ev = Vec::new();
    let mut files = std::collections::BTreeSet::new();
    for e in &a.ecosystems {
        for d in &e.dependencies {
            let v = d.version.as_deref().unwrap_or("").trim();
            let risky = match d.source.as_str() {
                "git" => Some("git source (unpinned unless a commit is specified)"),
                "url" => Some("remote archive URL"),
                "registry" if matches!(v, "*" | "latest" | "x" | ">=0") && e.id != "go" => Some("unbounded version"),
                "registry" if v.is_empty() && matches!(e.id.as_str(), "node" | "rust" | "php") => {
                    Some("no version constraint")
                }
                _ => None,
            };
            if let Some(why) = risky {
                ev.push(format!(
                    "{}: `{}` {} — {why}",
                    e.manifest,
                    d.name,
                    if v.is_empty() { String::new() } else { format!("\"{v}\"") }
                ));
                files.insert(e.manifest.clone());
            }
        }
    }
    if ev.is_empty() {
        return;
    }
    out.push(Finding {
        rule: "risky-dependency-spec".into(),
        category: "dependencies".into(),
        severity: Severity::Medium,
        confidence: Confidence::High,
        title: format!("{} dependency specification(s) are not reproducible", ev.len()),
        explanation: "Wildcard versions, `latest`, git branches and remote archives can change underneath you, breaking builds or pulling in unreviewed code.".into(),
        files: files.into_iter().collect(),
        evidence: ev.into_iter().take(50).collect(),
        suggestion: "Pin to a version range (and commit lockfiles), or pin git dependencies to a specific commit/tag.".into(),
        metric: None,
    });
}

// ------------------------------------------------------------------------------------------
// Size & complexity

fn large_files(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let t = &cfg.thresholds;
    let mut large = Vec::new();
    for f in &a.files {
        if !matches!(f.role(), Role::Source | Role::Test) {
            continue;
        }
        let Some(lines) = f.lines else { continue };
        if lines.total >= t.huge_file_lines {
            out.push(Finding {
                rule: "large-file".into(),
                category: "size".into(),
                severity: Severity::High,
                confidence: Confidence::High,
                title: format!("{} has {} lines", f.path, lines.total),
                explanation: "Very large files usually hold several responsibilities, are hard to review and cause merge conflicts.".into(),
                files: vec![f.path.clone()],
                evidence: vec![format!("{} lines ({} estimated code lines), {} symbols", lines.total, lines.code, f.symbol_count)],
                suggestion: "Split it along its responsibilities; extract cohesive groups of functions/types into their own modules.".into(),
                metric: Some(MetricValue { name: "lines".into(), value: lines.total as f64, threshold: t.huge_file_lines as f64 }),
            });
        } else if lines.total >= t.large_file_lines {
            large.push((f.path.clone(), lines.total));
        }
    }
    if !large.is_empty() {
        large.sort_by_key(|x| std::cmp::Reverse(x.1));
        out.push(Finding {
            rule: "large-file".into(),
            category: "size".into(),
            severity: Severity::Low,
            confidence: Confidence::High,
            title: format!("{} file(s) exceed {} lines", large.len(), t.large_file_lines),
            explanation: "These files are getting large. Not a problem by itself, but worth watching.".into(),
            evidence: large.iter().take(50).map(|(p, l)| format!("{p} — {l} lines")).collect(),
            files: list(large.into_iter().map(|x| x.0)),
            suggestion: "Consider splitting when you next change them.".into(),
            metric: Some(MetricValue {
                name: "lines".into(),
                value: t.large_file_lines as f64,
                threshold: t.large_file_lines as f64,
            }),
        });
    }
}

fn complex_files(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let t = &cfg.thresholds;
    let mut v: Vec<&FileRecord> = a
        .files
        .iter()
        .filter(|f| f.role() == Role::Source)
        .filter(|f| f.complexity.unwrap_or(0) >= t.complex_file || f.max_nesting.unwrap_or(0) >= t.deep_block_nesting)
        .collect();
    v.sort_by_key(|f| std::cmp::Reverse(f.complexity.unwrap_or(0)));
    for f in v.into_iter().take(30) {
        let c = f.complexity.unwrap_or(0);
        let nest = f.max_nesting.unwrap_or(0);
        let lines = f.lines.map(|l| l.code.max(1)).unwrap_or(1);
        out.push(Finding {
            rule: "complex-file".into(),
            category: "size".into(),
            severity: if c >= t.complex_file * 2 || nest >= t.deep_block_nesting + 2 { Severity::Medium } else { Severity::Low },
            confidence: Confidence::Medium,
            title: format!("{}: {} decision points, nesting depth {}", f.path, c, nest),
            explanation: "Many branches and deep nesting make code hard to follow and test. Values are lexical estimates (branch keywords and block depth), not exact cyclomatic complexity.".into(),
            files: vec![f.path.clone()],
            evidence: vec![format!("{c} decision points over {lines} code lines ({:.1} per 100 lines); max nesting {nest}", c as f64 * 100.0 / lines as f64)],
            suggestion: "Extract deeply nested blocks into well-named functions; replace long if/else chains with lookups or polymorphism.".into(),
            metric: Some(MetricValue { name: "decision points".into(), value: c as f64, threshold: t.complex_file as f64 }),
        });
    }
}

// ------------------------------------------------------------------------------------------
// Structure

fn deep_dirs(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let limit = cfg.thresholds.deep_directory;
    let jvm = |p: &str| p.contains("src/main/java") || p.contains("src/test/java") || p.contains("src/main/kotlin");
    let mut deep: Vec<(&str, u32)> = a
        .dirs
        .iter()
        .filter(|d| d.depth > limit && !jvm(&d.path) && d.direct_files > 0)
        .map(|d| (d.path.as_str(), d.depth))
        .collect();
    if deep.is_empty() {
        return;
    }
    deep.sort_by_key(|x| std::cmp::Reverse(x.1));
    let max = deep[0].1;
    out.push(Finding {
        rule: "deep-nesting".into(),
        category: "structure".into(),
        severity: if max > limit + 4 { Severity::Medium } else { Severity::Low },
        confidence: Confidence::High,
        title: format!("{} directories nested deeper than {limit} levels", deep.len()),
        explanation: "Deep hierarchies make files hard to find and import paths long.".into(),
        evidence: deep.iter().take(20).map(|(p, d)| format!("{p} (depth {d})")).collect(),
        files: list(deep.iter().map(|x| x.0.to_string())),
        suggestion:
            "Flatten levels that only group a handful of files; prefer feature folders over deep technical nesting."
                .into(),
        metric: Some(MetricValue { name: "max depth".into(), value: max as f64, threshold: limit as f64 }),
    });
}

fn wide_dirs(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let limit = cfg.thresholds.wide_directory;
    let mut v: Vec<_> = a.dirs.iter().filter(|d| d.direct_files >= limit).collect();
    v.sort_by_key(|d| std::cmp::Reverse(d.direct_files));
    for d in v.into_iter().take(15) {
        out.push(Finding {
            rule: "wide-directory".into(),
            category: "structure".into(),
            severity: sev_for(d.direct_files as f64, limit as f64 * 2.0, limit as f64 * 4.0),
            confidence: Confidence::High,
            title: format!("{} contains {} files directly", d.path, d.direct_files),
            explanation: "Very large flat directories hide structure and make related files hard to find.".into(),
            files: vec![d.path.clone()],
            evidence: vec![format!("{} files, {} sub-directories", d.direct_files, d.direct_dirs)],
            suggestion: "Group files by feature or responsibility into sub-folders (unless the folder is generated data or assets).".into(),
            metric: Some(MetricValue { name: "files".into(), value: d.direct_files as f64, threshold: limit as f64 }),
        });
    }
}

fn single_child_chains(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let jvm = |p: &str| p.contains("java/") || p.contains("kotlin/") || p.contains("scala/");
    let by_path: HashMap<&str, &crate::analysis::model::DirRecord> =
        a.dirs.iter().map(|d| (d.path.as_str(), d)).collect();
    let mut chains = Vec::new();
    for d in &a.dirs {
        // chain start: single-child, empty dir whose parent is not itself such a dir
        let is_link = |x: &crate::analysis::model::DirRecord| x.direct_files == 0 && x.direct_dirs == 1;
        if !is_link(d) || jvm(&d.path) {
            continue;
        }
        let parent = d.path.rfind('/').map(|i| &d.path[..i]);
        if parent.and_then(|p| by_path.get(p)).is_some_and(|p| is_link(p)) {
            continue;
        }
        let mut len = 1;
        let mut cur = d;
        while let Some(child) =
            a.dirs.iter().find(|x| x.path.starts_with(&format!("{}/", cur.path)) && x.depth == cur.depth + 1)
        {
            if is_link(child) {
                len += 1;
                cur = child;
            } else {
                break;
            }
        }
        if len >= cfg.thresholds.single_child_chain {
            chains.push(format!("{} → {} ({len} empty levels)", d.path, cur.path));
        }
    }
    if chains.is_empty() {
        return;
    }
    out.push(Finding {
        rule: "unnecessary-nesting".into(),
        category: "structure".into(),
        severity: Severity::Info,
        confidence: Confidence::High,
        title: format!("{} chain(s) of single-folder directories", chains.len()),
        explanation: "Folders that contain only one sub-folder add path length without adding structure.".into(),
        files: list(chains.iter().map(|c| c.split(' ').next().unwrap_or("").to_string())),
        evidence: chains.into_iter().take(30).collect(),
        suggestion: "Collapse the intermediate folders unless a tool requires the layout.".into(),
        metric: None,
    });
}

fn duplicates(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let mut groups: HashMap<&str, Vec<&FileRecord>> = HashMap::new();
    for f in &a.files {
        if f.size >= cfg.thresholds.duplicate_min_bytes && !matches!(f.role(), Role::Lockfile | Role::Generated) {
            if let Some(h) = f.hash.as_deref() {
                groups.entry(h).or_default().push(f);
            }
        }
    }
    let mut dups: Vec<Vec<&FileRecord>> = groups.into_values().filter(|g| g.len() > 1).collect();
    dups.sort_by_key(|g| std::cmp::Reverse(g[0].size * g.len() as u64));
    for g in dups.into_iter().take(30) {
        let wasted = g[0].size * (g.len() as u64 - 1);
        let code = g.iter().any(|f| f.role() == Role::Source);
        out.push(Finding {
            rule: "duplicate-files".into(),
            category: "structure".into(),
            severity: if code { Severity::Medium } else { Severity::Low },
            confidence: Confidence::High,
            title: format!("{} identical copies of {}", g.len(), g[0].name()),
            explanation: "These files have byte-identical content (SHA-256). Copied code drifts apart over time and fixes get applied to only one copy.".into(),
            files: g.iter().map(|f| f.path.clone()).collect(),
            evidence: vec![format!("{} bytes each, {wasted} bytes duplicated", g[0].size)],
            suggestion: if code { "Keep one copy and import it from the others (or a shared package).".into() } else { "Keep a single copy if the duplicates are not required by tooling.".into() },
            metric: None,
        });
    }
}

fn similar_dirs(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let mut by_dir: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for f in &a.files {
        by_dir.entry(f.dir()).or_default().push(f.name());
    }
    let mut sig: HashMap<String, Vec<&str>> = HashMap::new();
    for (d, names) in by_dir {
        if names.len() < 3 || d.is_empty() {
            continue;
        }
        let mut n = names.clone();
        n.sort_unstable();
        sig.entry(n.join("|")).or_default().push(d);
    }
    let mut groups: Vec<(String, Vec<&str>)> = sig.into_iter().filter(|(_, v)| v.len() > 1).collect();
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    for (names, dirs) in groups.into_iter().take(10) {
        let count = names.split('|').count();
        out.push(Finding {
            rule: "similar-structure".into(),
            category: "structure".into(),
            severity: Severity::Info,
            confidence: Confidence::Low,
            title: format!("{} directories share the same {count} file names", dirs.len()),
            explanation: "Identical file layouts can indicate copy-pasted modules — or a deliberate convention (e.g. one folder per component).".into(),
            files: dirs.iter().map(|d| d.to_string()).collect(),
            evidence: vec![format!("files: {}", names.replace('|', ", "))],
            suggestion: "If the contents are also similar, extract the shared part; if it is a convention, ignore this finding.".into(),
            metric: None,
        });
    }
}

fn mixed_concerns(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let mut by_dir: BTreeMap<&str, HashMap<Role, u32>> = BTreeMap::new();
    for f in &a.files {
        *by_dir.entry(f.dir()).or_default().entry(f.role()).or_default() += 1;
    }
    let mut hits = Vec::new();
    for (d, roles) in by_dir {
        if d.is_empty() {
            continue;
        }
        let total: u32 = roles.values().sum();
        let concerns = [Role::Source, Role::Test, Role::Config, Role::Documentation, Role::Asset, Role::Data]
            .iter()
            .filter(|r| roles.get(r).copied().unwrap_or(0) >= 2)
            .count();
        if total >= 8 && concerns >= 3 {
            let mut parts: Vec<String> =
                roles.iter().filter(|(_, n)| **n >= 2).map(|(r, n)| format!("{n} {}", r.as_str())).collect();
            parts.sort();
            hits.push(format!("{d}: {}", parts.join(", ")));
        }
    }
    if hits.is_empty() {
        return;
    }
    out.push(Finding {
        rule: "mixed-concerns".into(),
        category: "structure".into(),
        severity: Severity::Info,
        confidence: Confidence::Low,
        title: format!("{} directories mix several kinds of files", hits.len()),
        explanation: "Folders that directly combine source code, tests, configuration, documentation and assets make responsibilities harder to see. Co-locating tests with code is a valid convention.".into(),
        files: list(hits.iter().map(|h| h.split(':').next().unwrap_or("").to_string())),
        evidence: hits.into_iter().take(30).collect(),
        suggestion: "Group non-code files into dedicated sub-folders where it helps navigation.".into(),
        metric: None,
    });
}

fn misplaced(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let roots: Vec<&String> = a.structure.doc_roots.iter().chain(a.structure.asset_roots.iter()).collect();
    if roots.is_empty() {
        return;
    }
    let hits: Vec<String> = a
        .files
        .iter()
        .filter(|f| {
            f.role() == Role::Source && !matches!(f.language.as_deref(), Some("css" | "scss" | "less" | "html"))
        })
        .filter(|f| roots.iter().any(|r| f.path.starts_with(&format!("{r}/"))))
        .map(|f| f.path.clone())
        .collect();
    if hits.is_empty() {
        return;
    }
    out.push(Finding {
        rule: "misplaced-source".into(),
        category: "structure".into(),
        severity: Severity::Info,
        confidence: Confidence::Low,
        title: format!("{} source file(s) inside documentation or asset folders", hits.len()),
        explanation: "Code inside docs/ or assets/ is easy to miss in reviews and builds. It may be intentional (examples, embedded scripts).".into(),
        files: list(hits),
        evidence: vec![format!("documentation/asset roots: {}", roots.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))],
        suggestion: "Move real source code next to the rest of the code; keep examples clearly labelled.".into(),
        metric: None,
    });
}

// ------------------------------------------------------------------------------------------
// Documentation, testing, configuration, security

fn missing_readme(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let has = |dir: &str| a.files.iter().any(|f| f.dir() == dir && f.name().to_ascii_lowercase().starts_with("readme"));
    if !has("") {
        out.push(Finding {
            rule: "missing-readme".into(),
            category: "documentation".into(),
            severity: Severity::Medium,
            confidence: Confidence::High,
            title: "No README at the project root".into(),
            explanation: "A README is the first thing contributors and tools look for: purpose, setup, usage.".into(),
            files: vec![],
            evidence: vec!["no README* file in the project root".into()],
            suggestion:
                "Add a README.md describing what the project does, how to build/run it and where to start reading."
                    .into(),
            metric: None,
        });
    }
    let pkgs: Vec<String> = a
        .ecosystems
        .iter()
        .filter(|e| !e.root.is_empty() && !matches!(e.id.as_str(), "docker" | "terraform"))
        .filter(|e| !has(&e.root))
        .map(|e| e.manifest.clone())
        .collect();
    if pkgs.len() >= 2 {
        out.push(Finding {
            rule: "missing-readme".into(),
            category: "documentation".into(),
            severity: Severity::Info,
            confidence: Confidence::High,
            title: format!("{} packages without their own README", pkgs.len()),
            explanation: "In multi-package repositories, a short README per package explains its role.".into(),
            files: list(pkgs.clone()),
            evidence: pkgs.into_iter().take(30).map(|m| format!("{m} has no README next to it")).collect(),
            suggestion: "Add a brief README to each package.".into(),
            metric: None,
        });
    }
}

fn missing_docs(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let source = a.metrics.by_role.get("source").copied().unwrap_or(0);
    let docs = a
        .files
        .iter()
        .filter(|f| f.language.as_deref().is_some_and(|l| matches!(l, "markdown" | "rst" | "asciidoc")))
        .count();
    if source >= 150 && a.structure.doc_roots.is_empty() && docs < 3 {
        out.push(Finding {
            rule: "missing-docs".into(),
            category: "documentation".into(),
            severity: Severity::Low,
            confidence: Confidence::Medium,
            title: format!("{source} source files but only {docs} documentation file(s)"),
            explanation: "Larger codebases benefit from architecture and setup documentation beyond a README.".into(),
            files: vec![],
            evidence: vec![format!("{source} source files, {docs} Markdown/RST/AsciiDoc files, no docs/ folder")],
            suggestion: "Add a docs/ folder with an architecture overview (Structura's report export is a good start)."
                .into(),
            metric: None,
        });
    }
}

fn missing_tests(a: &Analysis, cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let source = a.metrics.by_role.get("source").copied().unwrap_or(0);
    let tests = a.metrics.by_role.get("test").copied().unwrap_or(0);
    if source < cfg.thresholds.min_source_files_for_tests {
        return;
    }
    // Rust and others keep unit tests inline; count files containing test modules as tests.
    let inline = a
        .files
        .iter()
        .filter(|f| {
            f.role() == Role::Source
                && (f.inline_tests || f.imports.iter().any(|i| i.spec == "tests" && i.kind == ImportKind::Mod))
        })
        .count() as u32;
    let total_tests = tests + inline;
    let ratio = total_tests as f64 / source as f64;
    if total_tests == 0 {
        out.push(Finding {
            rule: "missing-tests".into(),
            category: "testing".into(),
            severity: Severity::Medium,
            confidence: Confidence::Medium,
            title: "No test files found".into(),
            explanation: "No files matched common test conventions (test/, tests/, *.test.*, *_test.*, test_*.py, inline Rust test modules). Tests may live elsewhere or be excluded by the profile.".into(),
            files: vec![],
            evidence: vec![format!("{source} source files, 0 test files")],
            suggestion: "Add automated tests for the most central modules first (see the hub modules in the dependency view).".into(),
            metric: Some(MetricValue { name: "test files per source file".into(), value: 0.0, threshold: 0.1 }),
        });
    } else if ratio < 0.1 {
        out.push(Finding {
            rule: "missing-tests".into(),
            category: "testing".into(),
            severity: Severity::Info,
            confidence: Confidence::Low,
            title: format!("{total_tests} test files for {source} source files"),
            explanation: "The test structure is thin compared to the amount of source code. File counts are only a structural indicator, not coverage.".into(),
            files: vec![],
            evidence: vec![format!("{tests} test files + {inline} files with inline test modules; ratio {ratio:.2}")],
            suggestion: "Measure real coverage with your test runner and prioritise central, complex modules.".into(),
            metric: Some(MetricValue { name: "test files per source file".into(), value: ratio, threshold: 0.1 }),
        });
    }
}

fn lockfiles(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let node_locks = ["package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb", "bun.lock"];
    let mut by_dir: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for f in &a.files {
        if node_locks.contains(&f.name()) {
            by_dir.entry(f.dir()).or_default().push(f.name());
        }
    }
    for (d, locks) in by_dir.into_iter().filter(|(_, v)| v.len() > 1) {
        out.push(Finding {
            rule: "conflicting-lockfiles".into(),
            category: "configuration".into(),
            severity: Severity::Medium,
            confidence: Confidence::High,
            title: format!("{} lockfiles in {}", locks.len(), if d.is_empty() { "the project root" } else { d }),
            explanation: "Different package managers resolve different dependency trees; contributors and CI may install different versions.".into(),
            files: locks.iter().map(|l| if d.is_empty() { l.to_string() } else { format!("{d}/{l}") }).collect(),
            evidence: vec![format!("lockfiles: {}", locks.join(", "))],
            suggestion: "Pick one package manager, delete the other lockfiles and set `packageManager` in package.json.".into(),
            metric: None,
        });
    }
}

fn duplicate_config(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let tools: &[(&str, &[&str])] = &[
        ("ESLint", &[".eslintrc", "eslint.config."]),
        ("Prettier", &[".prettierrc", "prettier.config."]),
        ("Babel", &[".babelrc", "babel.config."]),
        ("Jest", &["jest.config."]),
        ("Stylelint", &[".stylelintrc", "stylelint.config."]),
        ("TS/JS project", &["tsconfig.json", "jsconfig.json"]),
        ("PostCSS", &["postcss.config.", ".postcssrc"]),
    ];
    let mut by_dir: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for f in &a.files {
        let n = f.name().to_ascii_lowercase();
        for (tool, prefixes) in tools {
            if prefixes.iter().any(|p| n.starts_with(p)) && !(n.starts_with("tsconfig.") && n != "tsconfig.json") {
                by_dir.entry((f.dir(), tool)).or_default().push(f.name());
            }
        }
    }
    for ((d, tool), files) in by_dir.into_iter().filter(|(_, v)| v.len() > 1) {
        out.push(Finding {
            rule: "duplicate-tool-config".into(),
            category: "configuration".into(),
            severity: Severity::Low,
            confidence: Confidence::Medium,
            title: format!("{} {tool} configuration files in {}", files.len(), if d.is_empty() { "the project root" } else { d }),
            explanation: "When several config files exist for one tool, only one usually wins — the others are silently ignored or merged unexpectedly.".into(),
            files: files.iter().map(|n| if d.is_empty() { n.to_string() } else { format!("{d}/{n}") }).collect(),
            evidence: vec![files.join(", ")],
            suggestion: "Keep one configuration file per tool and directory.".into(),
            metric: None,
        });
    }
}

fn sensitive(a: &Analysis, _cfg: &HealthConfig, out: &mut Vec<Finding>) {
    let hits: Vec<String> = a.files.iter().filter(|f| f.sensitive).map(|f| f.path.clone()).collect();
    if hits.is_empty() {
        return;
    }
    let gi = a.meta.options.respect_gitignore;
    out.push(Finding {
        rule: "sensitive-files".into(),
        category: "security".into(),
        severity: Severity::High,
        confidence: if gi { Confidence::High } else { Confidence::Medium },
        title: format!("{} file(s) that commonly contain secrets", hits.len()),
        explanation: if gi {
            "These files are not excluded by .gitignore, so they may be committed and shared. Structura did not read their contents.".into()
        } else {
            "These files are present in the project tree (the profile does not apply .gitignore, so they may already be ignored). Structura did not read their contents.".into()
        },
        files: list(hits.clone()),
        evidence: hits.iter().take(30).map(|h| format!("{h} — name matches a credential/key pattern")).collect(),
        suggestion: "Add them to .gitignore, rotate any secret that was ever committed, and provide `.env.example`-style templates instead.".into(),
        metric: None,
    });
}

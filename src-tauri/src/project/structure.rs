//! Structure overview: entry points, source/test/doc roots and architectural pattern hints.
//!
//! Patterns are *hints*, never verdicts: each carries evidence (the directories that triggered
//! it) and a confidence level. No universal layout is assumed.

use crate::analysis::model::{Confidence, EntryPoint, FileRecord, Pattern, Structure};
use crate::project::ecosystem::Ecosystem;
use crate::project::roles::Role;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const CONVENTIONAL_ENTRIES: &[(&str, &str)] = &[
    ("index.html", "HTML entry page"),
    ("src/main.js", "conventional app entry"),
    ("src/main.jsx", "conventional app entry"),
    ("src/main.ts", "conventional app entry"),
    ("src/main.tsx", "conventional app entry"),
    ("src/index.js", "conventional app entry"),
    ("src/index.jsx", "conventional app entry"),
    ("src/index.ts", "conventional app entry"),
    ("src/index.tsx", "conventional app entry"),
    ("index.js", "conventional entry"),
    ("index.ts", "conventional entry"),
    ("server.js", "conventional server entry"),
    ("app.js", "conventional entry"),
    ("main.py", "conventional Python entry"),
    ("app.py", "conventional Python entry"),
    ("manage.py", "Django management entry"),
    ("__main__.py", "Python package entry"),
    ("main.go", "Go main package"),
    ("Program.cs", ".NET entry point"),
    ("main.c", "C entry point"),
    ("main.cpp", "C++ entry point"),
    ("main.lua", "Lua entry point"),
    ("main.zig", "Zig entry point"),
    ("src/main.zig", "Zig entry point"),
    ("main.swift", "Swift entry point"),
    ("Main.java", "Java entry point"),
    ("App.java", "Java entry point"),
];

fn parent(p: &str) -> &str {
    p.rfind('/').map(|i| &p[..i]).unwrap_or("")
}

pub fn analyze(files: &[FileRecord], ecosystems: &[Ecosystem], dirs: &[String]) -> Structure {
    let mut s = Structure::default();
    let set: HashMap<&str, usize> = files.iter().enumerate().map(|(i, f)| (f.path.as_str(), i)).collect();

    // Entry points: manifests first, then conventions relative to each ecosystem root and the project root.
    for e in ecosystems {
        for ep in &e.entry_points {
            if !s.entry_points.iter().any(|x| x.path == *ep) {
                s.entry_points.push(EntryPoint { path: ep.clone(), reason: format!("declared by {}", e.manifest) });
            }
        }
    }
    let mut roots: BTreeSet<String> = ecosystems.iter().map(|e| e.root.clone()).collect();
    roots.insert(String::new());
    for root in &roots {
        for (rel, reason) in CONVENTIONAL_ENTRIES {
            let p = if root.is_empty() { rel.to_string() } else { format!("{root}/{rel}") };
            if set.contains_key(p.as_str()) && !s.entry_points.iter().any(|x| x.path == p) {
                s.entry_points.push(EntryPoint { path: p, reason: reason.to_string() });
            }
        }
    }
    // Go: any `main.go` under cmd/
    for f in files {
        if f.path.contains("cmd/") && f.path.ends_with("/main.go") && !s.entry_points.iter().any(|x| x.path == f.path) {
            s.entry_points.push(EntryPoint { path: f.path.clone(), reason: "Go command (cmd/*/main.go)".into() });
        }
    }

    // Role counts and roots.
    let mut by_dir_role: BTreeMap<String, HashMap<Role, u32>> = BTreeMap::new();
    for f in files {
        *s.roles.entry(f.role().as_str().to_string()).or_default() += 1;
        // top two levels as candidate roots
        let parts: Vec<&str> = f.path.split('/').collect();
        for depth in 1..=2.min(parts.len().saturating_sub(1)) {
            let d = parts[..depth].join("/");
            *by_dir_role.entry(d).or_default().entry(f.role()).or_default() += 1;
        }
    }
    let named =
        |names: &[&str], d: &str| names.contains(&d.rsplit('/').next().unwrap_or(d).to_ascii_lowercase().as_str());
    for (d, counts) in &by_dir_role {
        let total: u32 = counts.values().sum();
        if total == 0 {
            continue;
        }
        let share = |r: Role| counts.get(&r).copied().unwrap_or(0) as f64 / total as f64;
        let is_nested_in = |list: &Vec<String>| list.iter().any(|x| d.starts_with(&format!("{x}/")));
        if ((named(
            &[
                "src", "lib", "app", "source", "sources", "pkg", "internal", "cmd", "crates", "packages", "apps",
                "server", "client", "core",
            ],
            d,
        ) && share(Role::Source) >= 0.3)
            || share(Role::Source) >= 0.6 && total >= 3)
            && !is_nested_in(&s.source_roots)
        {
            s.source_roots.push(d.clone());
        }
        if share(Role::Test) >= 0.6 && !is_nested_in(&s.test_roots) {
            s.test_roots.push(d.clone());
        }
        if share(Role::Documentation) >= 0.6 && !is_nested_in(&s.doc_roots) {
            s.doc_roots.push(d.clone());
        }
        if share(Role::Asset) >= 0.6 && !is_nested_in(&s.asset_roots) {
            s.asset_roots.push(d.clone());
        }
    }
    s.readme = files
        .iter()
        .filter(|f| !f.path.contains('/') && f.name().to_ascii_lowercase().starts_with("readme"))
        .map(|f| f.path.clone())
        .next();

    s.patterns = patterns(files, ecosystems, dirs);
    s
}

fn patterns(files: &[FileRecord], ecosystems: &[Ecosystem], dirs: &[String]) -> Vec<Pattern> {
    let dir_names: BTreeMap<String, Vec<&String>> = dirs.iter().fold(BTreeMap::new(), |mut m, d| {
        m.entry(d.rsplit('/').next().unwrap_or(d).to_ascii_lowercase()).or_insert_with(Vec::new).push(d);
        m
    });
    let has = |n: &str| dir_names.get(n).map(|v| v.len()).unwrap_or(0) > 0;
    let ev = |names: &[&str]| -> Vec<String> {
        names
            .iter()
            .filter_map(|n| dir_names.get(*n).and_then(|v| v.first()).map(|d| format!("directory {d}/")))
            .collect()
    };
    let mut out = Vec::new();
    let mut push = |id: &str, name: &str, conf: Confidence, evidence: Vec<String>| {
        if !evidence.is_empty() {
            out.push(Pattern { id: id.into(), name: name.into(), confidence: conf, evidence });
        }
    };

    let manifests_by_kind =
        ecosystems.iter().filter(|e| !matches!(e.id.as_str(), "docker" | "terraform" | "make")).count();
    if manifests_by_kind >= 3 || ecosystems.iter().any(|e| !e.workspace_members.is_empty()) {
        let mut evd: Vec<String> = ecosystems
            .iter()
            .filter(|e| !e.workspace_members.is_empty())
            .map(|e| format!("workspaces declared in {}", e.manifest))
            .collect();
        if manifests_by_kind >= 3 {
            evd.push(format!("{manifests_by_kind} package manifests"));
        }
        push(
            "monorepo",
            "Monorepo / multi-package",
            if evd.len() > 1 { Confidence::High } else { Confidence::Medium },
            evd,
        );
    }
    if has("controllers") && (has("models") || has("views")) {
        push("mvc", "Model-View-Controller", Confidence::Medium, ev(&["controllers", "models", "views"]));
    }
    let layered =
        ["domain", "application", "infrastructure", "presentation", "usecases", "entities", "adapters", "ports"];
    let found: Vec<&str> = layered.iter().copied().filter(|n| has(n)).collect();
    if found.len() >= 2 {
        push(
            "layered",
            "Layered / Clean / Hexagonal architecture",
            if found.len() >= 3 { Confidence::Medium } else { Confidence::Low },
            ev(&found),
        );
    }
    if has("components") && (has("pages") || has("views") || has("routes") || has("screens")) {
        push(
            "component-ui",
            "Component-based UI",
            Confidence::Medium,
            ev(&["components", "pages", "views", "routes", "screens"]),
        );
    }
    if has("cmd") && (has("internal") || has("pkg")) && ecosystems.iter().any(|e| e.id == "go") {
        push("go-standard-layout", "Go standard project layout", Confidence::High, ev(&["cmd", "internal", "pkg"]));
    }
    if has("features") || has("modules") {
        push("feature-modules", "Feature / module oriented", Confidence::Low, ev(&["features", "modules"]));
    }
    if has("services") && (has("handlers") || has("routes") || has("api") || has("middleware")) {
        push(
            "service-layer",
            "Service layer with request handlers",
            Confidence::Low,
            ev(&["services", "handlers", "routes", "api", "middleware"]),
        );
    }
    if has("hooks") || has("store") || has("stores") || has("state") {
        push(
            "client-state",
            "Client-side state / hooks modules",
            Confidence::Low,
            ev(&["hooks", "store", "stores", "state"]),
        );
    }
    if dirs.iter().any(|d| d == "src-tauri") {
        push(
            "tauri-app",
            "Tauri desktop app (web UI + Rust core)",
            Confidence::High,
            vec!["directory src-tauri/".into()],
        );
    }
    if has("migrations") {
        push("db-migrations", "Versioned database migrations", Confidence::Medium, ev(&["migrations"]));
    }
    if files.iter().any(|f| f.path.starts_with(".github/workflows/")) {
        push(
            "ci",
            "Continuous integration (GitHub Actions)",
            Confidence::High,
            vec!["directory .github/workflows/".into()],
        );
    }
    if ecosystems.iter().any(|e| e.id == "docker") {
        let evd = ecosystems.iter().filter(|e| e.id == "docker").map(|e| e.manifest.clone()).collect();
        push("containerized", "Containerized (Docker)", Confidence::High, evd);
    }
    let _ = parent;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang;
    use crate::project::roles;

    fn recs(paths: &[&str]) -> Vec<FileRecord> {
        let mut v: Vec<FileRecord> = paths
            .iter()
            .map(|p| FileRecord {
                path: p.to_string(),
                role: Some(roles::classify(p, lang::detect(p.rsplit('/').next().unwrap()))),
                ..Default::default()
            })
            .collect();
        v.sort_by(|a, b| a.path.cmp(&b.path));
        v
    }

    fn dirs_of(files: &[FileRecord]) -> Vec<String> {
        let mut d = BTreeSet::new();
        for f in files {
            let mut p = parent(&f.path);
            while !p.is_empty() {
                d.insert(p.to_string());
                p = parent(p);
            }
        }
        d.into_iter().collect()
    }

    #[test]
    fn entries_roots_and_patterns() {
        let files = recs(&[
            "README.md",
            "index.html",
            "src/main.jsx",
            "src/components/Button.jsx",
            "src/pages/Home.jsx",
            "src/lib/api.js",
            "src-tauri/src/main.rs",
            "src-tauri/src/lib.rs",
            "tests/app.test.js",
            "tests/util.test.js",
            "docs/a.md",
            "docs/b.md",
            ".github/workflows/ci.yml",
        ]);
        let dirs = dirs_of(&files);
        let eco = vec![Ecosystem {
            id: "rust".into(),
            root: "src-tauri".into(),
            manifest: "src-tauri/Cargo.toml".into(),
            entry_points: vec!["src-tauri/src/main.rs".into()],
            ..Default::default()
        }];
        let s = analyze(&files, &eco, &dirs);
        let eps: Vec<&str> = s.entry_points.iter().map(|e| e.path.as_str()).collect();
        assert!(eps.contains(&"src-tauri/src/main.rs"));
        assert!(eps.contains(&"index.html"));
        assert!(eps.contains(&"src/main.jsx"));
        assert!(s.source_roots.contains(&"src".to_string()));
        assert!(s.test_roots.contains(&"tests".to_string()));
        assert!(s.doc_roots.contains(&"docs".to_string()));
        assert_eq!(s.readme.as_deref(), Some("README.md"));
        let ids: Vec<&str> = s.patterns.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"component-ui"));
        assert!(ids.contains(&"tauri-app"));
        assert!(ids.contains(&"ci"));
        assert!(!ids.contains(&"mvc"));
        assert!(s.patterns.iter().all(|p| !p.evidence.is_empty()));
    }
}

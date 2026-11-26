//! End-to-end pipeline tests on generated sample projects.

use super::*;
use crate::analysis::profile::builtin;
use std::fs;

pub fn write_tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    for (p, c) in files {
        let full = t.path().join(p);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, c).unwrap();
    }
    t
}

pub fn analyze_tree(t: &tempfile::TempDir, profile_id: &str) -> Analysis {
    let root = crate::core::simplify(t.path().canonicalize().unwrap());
    let p = builtin().into_iter().find(|p| p.id == profile_id).unwrap();
    run(&root, &p, &CancelToken::new(), &|_| {}).unwrap()
}

pub fn web_project() -> tempfile::TempDir {
    write_tree(&[
        ("package.json", r#"{"name":"web","dependencies":{"react":"^19","left-pad":"*"},"devDependencies":{"vite":"^8"}}"#),
        ("package-lock.json", "{}"),
        ("yarn.lock", ""),
        ("index.html", "<script type=\"module\" src=\"/src/main.jsx\"></script>"),
        ("src/main.jsx", "import React from 'react';\nimport App from './App';\nimport './style.css';\n"),
        ("src/App.jsx", "import { a } from './lib/a';\nimport { helper } from '@/lib/helper';\nimport lodash from 'lodash';\nexport default function App() { if (a) { return helper(); } }\n"),
        ("src/lib/a.js", "import { b } from './b';\nexport const a = 1;\n"),
        ("src/lib/b.js", "import { a } from './a';\nexport const b = 2;\n"),
        ("src/lib/helper.js", "export function helper() {}\n"),
        ("src/lib/orphan.js", "export function unused() {}\n"),
        ("src/style.css", "@import './base.css';\nbody{}\n"),
        ("src/base.css", "html{}\n"),
        ("src/dup1.js", &"// same content\n".repeat(60)),
        ("src/copy/dup2.js", &"// same content\n".repeat(60)),
        ("vite.config.js", "export default { resolve: { alias: { '@': '/src' } } }"),
        (".env", "SECRET=1"),
        ("docs/guide.md", "# Guide\n"),
    ])
}

#[test]
fn full_pipeline_on_web_project() {
    let t = web_project();
    let a = analyze_tree(&t, "full");

    // Files & roles
    let f = |p: &str| a.files.iter().find(|f| f.path == p).unwrap_or_else(|| panic!("missing {p}"));
    assert_eq!(f("src/App.jsx").role(), roles::Role::Source);
    assert_eq!(f("package-lock.json").role(), roles::Role::Lockfile);
    assert!(a.files.iter().all(|f| f.path != ".env"), "hidden files excluded by default");

    // Ecosystem
    assert_eq!(a.ecosystems.len(), 1);
    assert!(a.ecosystems[0].frameworks.iter().any(|x| x.id == "react"));

    // Lines and symbols
    let app = f("src/App.jsx");
    assert_eq!(app.lines.unwrap().total, 4);
    assert_eq!(app.complexity, Some(1));
    assert!(app.exports.contains(&"default".to_string()));

    // Dependencies: relative, alias, css, html root-relative
    let idx = |p: &str| a.file_index(p).unwrap() as u32;
    let has_edge = |from: &str, to: &str| a.graph.edges.iter().any(|e| e.from == idx(from) && e.to == idx(to));
    assert!(has_edge("src/main.jsx", "src/App.jsx"));
    assert!(has_edge("src/App.jsx", "src/lib/helper.js"), "bundler alias '@'");
    assert!(has_edge("src/style.css", "src/base.css"));
    assert!(has_edge("index.html", "src/main.jsx"));
    assert!(has_edge("src/main.jsx", "src/style.css"));

    // Cycle a <-> b
    assert_eq!(a.graph.cycles.len(), 1);
    assert_eq!(a.graph.cycles[0].files.len(), 2);

    // Packages: react declared+used, lodash undeclared, left-pad declared-unused
    let pkg = |n: &str| a.graph.packages.iter().find(|p| p.name == n).unwrap_or_else(|| panic!("pkg {n}"));
    assert!(pkg("react").declared && pkg("react").use_count == 1);
    assert!(!pkg("lodash").declared);
    assert_eq!(pkg("left-pad").use_count, 0);

    // Fan-in / fan-out
    assert_eq!(f("src/lib/a.js").fan_in, 2);
    assert_eq!(f("src/App.jsx").fan_out, 2);

    // Entry points
    assert!(a.structure.entry_points.iter().any(|e| e.path == "src/main.jsx"));
    assert!(a.structure.entry_points.iter().any(|e| e.path == "index.html"));

    // Metrics
    assert_eq!(a.metrics.total_files as usize, a.files.len());
    assert_eq!(a.metrics.cycles, 1);
    assert!(a.metrics.source_code_lines > 0);
    assert!(a.languages.iter().any(|l| l.id == "jsx"));

    // Health findings
    let rules: Vec<&str> = a.findings.iter().map(|f| f.rule.as_str()).collect();
    for r in [
        "circular-dependency",
        "duplicate-files",
        "conflicting-lockfiles",
        "undeclared-dependency",
        "unreferenced-module",
        "risky-dependency-spec",
        "unused-declared-dependency",
    ] {
        assert!(rules.contains(&r), "expected rule {r}, got {rules:?}");
    }
    let orphan = a.findings.iter().find(|f| f.rule == "unreferenced-module").unwrap();
    assert!(orphan.files.contains(&"src/lib/orphan.js".to_string()));
    assert!(!orphan.files.contains(&"src/main.jsx".to_string()), "entry points are never unreferenced");
    let cyc = a.findings.iter().find(|f| f.rule == "circular-dependency").unwrap();
    assert!(cyc.evidence.iter().any(|e| e.contains("src/lib/a.js:1 imports './b'")));
    assert!(a.findings.iter().all(|f| !f.explanation.is_empty() && !f.suggestion.is_empty()));
}

#[test]
fn quick_profile_reads_no_content() {
    let t = web_project();
    let a = analyze_tree(&t, "quick");
    assert!(a.files.iter().all(|f| f.lines.is_none() && f.imports.is_empty() && f.hash.is_none()));
    assert!(a.graph.edges.is_empty());
    assert!(!a.meta.dependencies_analyzed);
    // Rules that need the graph are skipped rather than reporting false "unused" files.
    assert!(a.findings.iter().all(|f| f.category != "dependencies"));
}

#[test]
fn security_profile_includes_hidden_but_never_reads_secrets() {
    let t = web_project();
    let a = analyze_tree(&t, "security");
    let env = a.files.iter().find(|f| f.path == ".env").expect(".env listed");
    assert!(env.sensitive);
    assert_eq!(env.skipped.as_deref(), Some("sensitive"));
    assert!(env.lines.is_none() && env.hash.is_none());
    assert!(a.findings.iter().any(|f| f.rule == "sensitive-files" && f.files.contains(&".env".to_string())));
    let json = serde_json::to_string(&a).unwrap();
    assert!(!json.contains("SECRET=1"), "secret content must never appear in results");
}

#[test]
fn rust_crate_mod_tree_is_not_a_cycle() {
    let t = write_tree(&[
        ("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n[dependencies]\nserde = \"1\"\n"),
        ("src/main.rs", "mod a;\nmod b;\nuse crate::a::run;\nfn main() { run(); }\n#[cfg(test)]\nmod tests {}\n"),
        ("src/a.rs", "use crate::b::helper;\nuse serde::Serialize;\npub fn run() { helper(); }\n"),
        ("src/b.rs", "use super::a;\npub fn helper() {}\n"),
    ]);
    let a = analyze_tree(&t, "full");
    // main -> a (contains), a -> b (uses), b -> a (uses: super::a) => one real cycle a<->b only
    assert_eq!(a.graph.cycles.len(), 1);
    let names: Vec<&str> = a.graph.cycles[0].files.iter().map(|i| a.files[*i as usize].path.as_str()).collect();
    assert_eq!(names, vec!["src/a.rs", "src/b.rs"]);
    assert!(a.graph.packages.iter().any(|p| p.name == "serde" && p.declared && p.use_count == 1));
    assert!(a.graph.edges.iter().any(|e| e.kind == EdgeKind::Contains));
}

#[test]
fn rust_reexports_test_modules_and_enum_globs_are_not_cycles() {
    let t = write_tree(&[
        ("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n"),
        ("src/lib.rs", "pub mod env;\n"),
        ("src/env/mod.rs", "pub mod provider;\npub use provider::Provider;\npub struct Shared;\n"),
        ("src/env/provider.rs", "use super::Shared;\npub enum Kind { A }\nuse Kind::*;\npub struct Provider;\n#[cfg(test)]\nmod tests {\n    use super::*;\n    use crate::env::Shared;\n}\n"),
    ]);
    let a = analyze_tree(&t, "full");
    assert!(a.graph.cycles.is_empty(), "cycles: {:?}", a.graph.cycles);
    assert!(a.graph.packages.iter().all(|p| p.name != "kind"), "enum glob is not a crate");
    let idx = |p: &str| a.file_index(p).unwrap() as u32;
    // provider -> mod (uses Shared) is a real dependency; mod -> provider is containment.
    assert!(a
        .graph
        .edges
        .iter()
        .any(|e| e.from == idx("src/env/provider.rs") && e.to == idx("src/env/mod.rs") && e.kind == EdgeKind::Uses));
    assert!(a.graph.edges.iter().all(|e| !(e.from == idx("src/env/mod.rs") && e.kind == EdgeKind::Uses)));
}

#[test]
fn lazy_import_cycles_are_low_severity() {
    let t = write_tree(&[
        ("README.md", "x"),
        ("src/nav.js", "export const pages = { s: () => import('./Settings.js') };\n"),
        ("src/Settings.js", "import { pages } from './nav.js';\n"),
    ]);
    let a = analyze_tree(&t, "full");
    let c = a.findings.iter().find(|f| f.rule == "circular-dependency").unwrap();
    assert_eq!(c.severity, Severity::Low);
    assert!(c.evidence.iter().any(|e| e.contains("(dynamic import)")));
}

#[test]
fn python_project() {
    let t = write_tree(&[
        ("pyproject.toml", "[project]\nname = \"svc\"\ndependencies = [\"requests\"]\n"),
        ("svc/__init__.py", ""),
        ("svc/api.py", "import requests\nimport yaml\nfrom .models import User\nfrom svc.util import helper\n"),
        ("svc/models.py", "class User:\n    pass\n"),
        ("svc/util.py", "def helper():\n    pass\n"),
        ("tests/test_api.py", "from svc.api import *\n"),
    ]);
    let a = analyze_tree(&t, "full");
    let idx = |p: &str| a.file_index(p).unwrap() as u32;
    assert!(a.graph.edges.iter().any(|e| e.from == idx("svc/api.py") && e.to == idx("svc/models.py")));
    assert!(a.graph.edges.iter().any(|e| e.from == idx("svc/api.py") && e.to == idx("svc/util.py")));
    assert!(a.graph.packages.iter().any(|p| p.name == "requests" && p.declared));
    assert!(a.graph.packages.iter().any(|p| p.name == "yaml" && !p.declared));
    let und = a.findings.iter().find(|f| f.rule == "undeclared-dependency").unwrap();
    assert_eq!(und.confidence, Confidence::Low, "python import names may differ from dist names");
}

#[test]
fn cancellation_is_respected() {
    let t = web_project();
    let root = crate::core::simplify(t.path().canonicalize().unwrap());
    let c = CancelToken::new();
    c.cancel();
    let r = run(&root, &AnalysisProfile::default(), &c, &|_| {});
    assert!(matches!(r, Err(crate::error::AppError::Cancelled)));
}

#[test]
fn analysis_round_trips_through_json() {
    let t = web_project();
    let a = analyze_tree(&t, "full");
    let s = serde_json::to_string(&a).unwrap();
    let b: Analysis = serde_json::from_str(&s).unwrap();
    assert_eq!(b.files.len(), a.files.len());
    assert_eq!(b.graph.edges, a.graph.edges);
    assert_eq!(b.findings, a.findings);
}

#[test]
fn decode_handles_bom_utf16_and_binary() {
    assert_eq!(decode(b"\xEF\xBB\xBFabc").unwrap(), "abc");
    assert_eq!(decode(&[0xFF, 0xFE, b'h', 0, b'i', 0]).unwrap(), "hi");
    assert!(decode(&[0x89, b'P', b'N', b'G', 0, 0]).is_none());
}

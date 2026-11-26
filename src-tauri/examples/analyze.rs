//! Developer tool: run an analysis from the command line and print a summary.
//!
//! cargo run --release --example analyze -- <project-dir> [profile-id] [--json out.json]

use structura_lib::analysis::{self, profile};
use structura_lib::core::{validate_root, CancelToken};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first() else {
        eprintln!("usage: analyze <project-dir> [profile-id] [--json out.json]");
        std::process::exit(2);
    };
    let profile_id = args.get(1).filter(|a| !a.starts_with("--")).map(String::as_str).unwrap_or("full");
    let root = validate_root(dir).expect("valid project directory");
    let p = profile::builtin().into_iter().find(|p| p.id == profile_id).expect("known profile");
    let a = analysis::run(&root, &p, &CancelToken::new(), &|_| {}).expect("analysis");
    let m = &a.metrics;
    println!(
        "{} — {} files, {} dirs, {:.1} MB, {} ms",
        a.meta.name,
        m.total_files,
        m.total_dirs,
        m.total_bytes as f64 / 1e6,
        a.meta.duration_ms
    );
    println!("lines {} (code≈{}), source code≈{}", m.total_lines, m.code_lines, m.source_code_lines);
    println!(
        "languages: {}",
        a.languages.iter().take(8).map(|l| format!("{} {}", l.name, l.files)).collect::<Vec<_>>().join(", ")
    );
    for e in &a.ecosystems {
        println!(
            "ecosystem {} @ '{}' — {} deps, frameworks: {}",
            e.id,
            e.root,
            e.dependencies.len(),
            e.frameworks.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    let g = &a.graph;
    println!(
        "imports {} = internal {} + external {} + builtin {} + unresolved {}; edges {}, cycles {}",
        g.imports_total,
        g.imports_internal,
        g.imports_external,
        g.imports_builtin,
        g.imports_unresolved,
        g.edges.len(),
        g.cycles.len()
    );
    println!("resolution: {:?}", g.resolution_methods);
    println!("entry points: {:?}", a.structure.entry_points.iter().map(|e| &e.path).collect::<Vec<_>>());
    println!("patterns: {:?}", a.structure.patterns.iter().map(|p| &p.name).collect::<Vec<_>>());
    println!("findings ({}):", a.findings.len());
    for f in &a.findings {
        println!("  [{:?}/{:?}] {} — {}", f.severity, f.confidence, f.rule, f.title);
    }
    let unresolved: Vec<String> = a
        .files
        .iter()
        .flat_map(|f| {
            f.imports
                .iter()
                .filter(|i| i.target == analysis::model::ImportTarget::Unresolved)
                .map(move |i| format!("{}: {}", f.path, i.spec))
        })
        .take(25)
        .collect();
    if !unresolved.is_empty() {
        println!("sample unresolved:\n  {}", unresolved.join("\n  "));
    }
    if let Some(pos) = args.iter().position(|a| a == "--json") {
        let out = args.get(pos + 1).expect("output path");
        std::fs::write(out, serde_json::to_vec(&a).unwrap()).unwrap();
    }
}

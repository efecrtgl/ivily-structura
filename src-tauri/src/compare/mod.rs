//! Comparison of two analyses: snapshots of the same project over time, or two projects.

use crate::analysis::model::{Analysis, EdgeKind, Finding, Severity};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const LIST_CAP: usize = 500;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Delta {
    pub name: String,
    pub before: f64,
    pub after: f64,
    pub change: f64,
    /// `exact` or `estimated`
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub added_count: usize,
    pub removed_count: usize,
}

fn list_diff(before: &BTreeSet<String>, after: &BTreeSet<String>) -> ListDiff {
    let added: Vec<String> = after.difference(before).cloned().collect();
    let removed: Vec<String> = before.difference(after).cloned().collect();
    ListDiff {
        added_count: added.len(),
        removed_count: removed.len(),
        added: added.into_iter().take(LIST_CAP).collect(),
        removed: removed.into_iter().take(LIST_CAP).collect(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub lines_before: Option<u32>,
    pub lines_after: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageDelta {
    pub id: String,
    pub name: String,
    pub files_before: u32,
    pub files_after: u32,
    pub lines_before: u64,
    pub lines_after: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub before_label: String,
    pub after_label: String,
    pub same_project: bool,
    pub metrics: Vec<Delta>,
    pub languages: Vec<LanguageDelta>,
    pub files: ListDiff,
    pub changed_files: Vec<ChangedFile>,
    pub changed_files_count: usize,
    pub directories: ListDiff,
    /// Top-level "modules": directories (depth ≤ 2) that contain source files.
    pub modules: ListDiff,
    pub packages: ListDiff,
    pub ecosystems: ListDiff,
    pub frameworks: ListDiff,
    pub dependency_edges: ListDiff,
    pub cycles_new: Vec<Vec<String>>,
    pub cycles_resolved: Vec<Vec<String>>,
    pub findings_new: Vec<FindingRef>,
    pub findings_resolved: Vec<FindingRef>,
    pub engine_versions: (u32, u32),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FindingRef {
    pub rule: String,
    pub severity: Severity,
    pub title: String,
}

fn finding_key(f: &Finding) -> String {
    let mut files = f.files.clone();
    files.sort();
    format!("{}|{}", f.rule, files.join(","))
}

fn modules(a: &Analysis) -> BTreeSet<String> {
    a.files
        .iter()
        .filter(|f| f.role() == crate::project::roles::Role::Source)
        .filter_map(|f| {
            let parts: Vec<&str> = f.path.split('/').collect();
            (parts.len() > 1).then(|| parts[..(parts.len() - 1).min(2)].join("/"))
        })
        .collect()
}

pub fn compare(before: &Analysis, after: &Analysis, before_label: &str, after_label: &str) -> Comparison {
    let (mb, ma) = (&before.metrics, &after.metrics);
    let d = |name: &str, b: f64, a: f64, kind: &'static str| Delta {
        name: name.into(),
        before: b,
        after: a,
        change: a - b,
        kind,
    };
    let metrics = vec![
        d("Files", mb.total_files as f64, ma.total_files as f64, "exact"),
        d("Directories", mb.total_dirs as f64, ma.total_dirs as f64, "exact"),
        d("Size (bytes)", mb.total_bytes as f64, ma.total_bytes as f64, "exact"),
        d("Total lines", mb.total_lines as f64, ma.total_lines as f64, "exact"),
        d("Source code lines", mb.source_code_lines as f64, ma.source_code_lines as f64, "estimated"),
        d(
            "Source files",
            *mb.by_role.get("source").unwrap_or(&0) as f64,
            *ma.by_role.get("source").unwrap_or(&0) as f64,
            "exact",
        ),
        d(
            "Test files",
            *mb.by_role.get("test").unwrap_or(&0) as f64,
            *ma.by_role.get("test").unwrap_or(&0) as f64,
            "exact",
        ),
        d("Internal dependencies", mb.internal_edges as f64, ma.internal_edges as f64, "exact"),
        d("External packages", mb.external_packages as f64, ma.external_packages as f64, "exact"),
        d("Circular dependencies", mb.cycles as f64, ma.cycles as f64, "exact"),
        d("Files in cycles", mb.files_in_cycles as f64, ma.files_in_cycles as f64, "exact"),
        d("Max fan-in", mb.max_fan_in as f64, ma.max_fan_in as f64, "exact"),
        d("Average complexity", mb.avg_complexity, ma.avg_complexity, "estimated"),
        d("Max directory depth", mb.max_depth as f64, ma.max_depth as f64, "exact"),
        d("Findings", before.findings.len() as f64, after.findings.len() as f64, "exact"),
        d(
            "High-severity findings",
            before.findings.iter().filter(|f| f.severity == Severity::High).count() as f64,
            after.findings.iter().filter(|f| f.severity == Severity::High).count() as f64,
            "exact",
        ),
    ];

    let mut langs: BTreeMap<String, LanguageDelta> = BTreeMap::new();
    for l in &before.languages {
        langs.insert(
            l.id.clone(),
            LanguageDelta {
                id: l.id.clone(),
                name: l.name.clone(),
                files_before: l.files,
                files_after: 0,
                lines_before: l.lines,
                lines_after: 0,
            },
        );
    }
    for l in &after.languages {
        let e = langs.entry(l.id.clone()).or_insert_with(|| LanguageDelta {
            id: l.id.clone(),
            name: l.name.clone(),
            files_before: 0,
            files_after: 0,
            lines_before: 0,
            lines_after: 0,
        });
        e.files_after = l.files;
        e.lines_after = l.lines;
    }
    let mut languages: Vec<LanguageDelta> = langs.into_values().collect();
    languages.sort_by_key(|l| std::cmp::Reverse(l.files_after.max(l.files_before)));

    let files_b: BTreeSet<String> = before.files.iter().map(|f| f.path.clone()).collect();
    let files_a: BTreeSet<String> = after.files.iter().map(|f| f.path.clone()).collect();
    let by_path: HashMap<&str, &crate::analysis::model::FileRecord> =
        before.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut changed = Vec::new();
    for f in &after.files {
        if let Some(b) = by_path.get(f.path.as_str()) {
            let content_changed = match (&b.hash, &f.hash) {
                (Some(x), Some(y)) => x != y,
                _ => b.size != f.size || b.lines.map(|l| l.total) != f.lines.map(|l| l.total),
            };
            if content_changed {
                changed.push(ChangedFile {
                    path: f.path.clone(),
                    bytes_before: b.size,
                    bytes_after: f.size,
                    lines_before: b.lines.map(|l| l.total),
                    lines_after: f.lines.map(|l| l.total),
                });
            }
        }
    }
    let changed_count = changed.len();
    changed.truncate(LIST_CAP);

    let dirs = |a: &Analysis| a.dirs.iter().map(|d| d.path.clone()).collect::<BTreeSet<_>>();
    let pkgs = |a: &Analysis| {
        a.graph
            .packages
            .iter()
            .filter(|p| p.use_count > 0 || p.declared)
            .map(|p| format!("{}:{}", p.ecosystem, p.name))
            .collect::<BTreeSet<_>>()
    };
    let ecos = |a: &Analysis| {
        a.ecosystems
            .iter()
            .map(|e| format!("{} ({})", e.name, if e.root.is_empty() { "." } else { &e.root }))
            .collect::<BTreeSet<_>>()
    };
    let fws = |a: &Analysis| {
        a.ecosystems.iter().flat_map(|e| e.frameworks.iter().map(|f| f.name.clone())).collect::<BTreeSet<_>>()
    };
    let edges = |a: &Analysis| {
        a.graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Uses)
            .map(|e| format!("{} → {}", a.files[e.from as usize].path, a.files[e.to as usize].path))
            .collect::<BTreeSet<_>>()
    };
    let cycles = |a: &Analysis| {
        a.graph
            .cycles
            .iter()
            .map(|c| {
                let mut v: Vec<String> = c.files.iter().map(|i| a.files[*i as usize].path.clone()).collect();
                v.sort();
                v
            })
            .collect::<BTreeSet<_>>()
    };
    let (cb, ca) = (cycles(before), cycles(after));
    let fb: BTreeMap<String, &Finding> = before.findings.iter().map(|f| (finding_key(f), f)).collect();
    let fa: BTreeMap<String, &Finding> = after.findings.iter().map(|f| (finding_key(f), f)).collect();
    let fref = |f: &Finding| FindingRef { rule: f.rule.clone(), severity: f.severity, title: f.title.clone() };

    Comparison {
        before_label: before_label.into(),
        after_label: after_label.into(),
        same_project: before.meta.root.eq_ignore_ascii_case(&after.meta.root),
        metrics,
        languages,
        files: list_diff(&files_b, &files_a),
        changed_files: changed,
        changed_files_count: changed_count,
        directories: list_diff(&dirs(before), &dirs(after)),
        modules: list_diff(&modules(before), &modules(after)),
        packages: list_diff(&pkgs(before), &pkgs(after)),
        ecosystems: list_diff(&ecos(before), &ecos(after)),
        frameworks: list_diff(&fws(before), &fws(after)),
        dependency_edges: list_diff(&edges(before), &edges(after)),
        cycles_new: ca.difference(&cb).take(100).cloned().collect(),
        cycles_resolved: cb.difference(&ca).take(100).cloned().collect(),
        findings_new: fa.iter().filter(|(k, _)| !fb.contains_key(*k)).map(|(_, f)| fref(f)).take(LIST_CAP).collect(),
        findings_resolved: fb
            .iter()
            .filter(|(k, _)| !fa.contains_key(*k))
            .map(|(_, f)| fref(f))
            .take(LIST_CAP)
            .collect(),
        engine_versions: (before.meta.engine_version, after.meta.engine_version),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{analyze_tree, write_tree};

    #[test]
    fn compares_two_versions_of_a_project() {
        let v1 = write_tree(&[
            ("README.md", "x"),
            ("src/a.js", "import './b';\n"),
            ("src/b.js", "import './a';\n"),
            ("src/old.js", "export const x = 1;\n"),
        ]);
        let v2 = write_tree(&[
            ("README.md", "x"),
            ("src/a.js", "import './b';\nimport react from 'react';\n"),
            ("src/b.js", "export const b = 1;\n"),
            ("lib/new.js", "export const y = 1;\n"),
        ]);
        let a1 = analyze_tree(&v1, "full");
        let a2 = analyze_tree(&v2, "full");
        let c = compare(&a1, &a2, "v1", "v2");
        assert_eq!(c.files.added, vec!["lib/new.js"]);
        assert_eq!(c.files.removed, vec!["src/old.js"]);
        assert_eq!(c.changed_files_count, 2);
        assert_eq!(c.cycles_resolved.len(), 1);
        assert!(c.cycles_new.is_empty());
        assert!(c.modules.added.contains(&"lib".to_string()));
        assert!(c.packages.added.contains(&"node:react".to_string()));
        assert!(c.dependency_edges.removed.contains(&"src/b.js → src/a.js".to_string()));
        assert!(c.findings_resolved.iter().any(|f| f.rule == "circular-dependency"));
        let cyc = c.metrics.iter().find(|m| m.name == "Circular dependencies").unwrap();
        assert_eq!((cyc.before, cyc.after, cyc.change), (1.0, 0.0, -1.0));
        assert!(!c.same_project);
    }
}

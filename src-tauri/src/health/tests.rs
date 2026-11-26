use super::*;
use crate::analysis::model::*;
use crate::analysis::tests::{analyze_tree, write_tree};

fn rules_of(a: &Analysis) -> Vec<&str> {
    a.findings.iter().map(|f| f.rule.as_str()).collect()
}

#[test]
fn clean_project_has_no_significant_findings() {
    let t = write_tree(&[
        ("README.md", "# Demo\n"),
        ("package.json", r#"{"name":"demo","main":"src/index.js","dependencies":{"react":"^19"}}"#),
        ("package-lock.json", "{}"),
        ("src/index.js", "import { util } from './util';\nimport React from 'react';\nutil();\n"),
        ("src/util.js", "export function util() {}\n"),
        ("src/util.test.js", "import { util } from './util';\n"),
    ]);
    let a = analyze_tree(&t, "full");
    let significant: Vec<_> = a.findings.iter().filter(|f| f.severity >= Severity::Medium).collect();
    assert!(significant.is_empty(), "unexpected findings: {significant:#?}");
    assert!(!rules_of(&a).contains(&"unreferenced-module"));
    assert!(!rules_of(&a).contains(&"missing-readme"));
}

#[test]
fn thresholds_and_disabling_are_respected() {
    let big = "let x = 1;\n".repeat(700);
    let t = write_tree(&[("README.md", "x"), ("src/big.js", &big), ("src/index.js", "import './big';\n")]);
    let a = analyze_tree(&t, "full");
    assert!(rules_of(&a).contains(&"large-file"));
    let lf = a.findings.iter().find(|f| f.rule == "large-file").unwrap();
    assert_eq!(lf.severity, Severity::Low);

    let mut cfg = HealthConfig::default();
    cfg.thresholds.large_file_lines = 2000;
    cfg.thresholds.huge_file_lines = 5000;
    assert!(!analyze(&a, &cfg).iter().any(|f| f.rule == "large-file"));

    cfg = HealthConfig { disabled_rules: vec!["large-file".into()], ..Default::default() };
    assert!(!analyze(&a, &cfg).iter().any(|f| f.rule == "large-file"));

    cfg = HealthConfig::default();
    cfg.thresholds.huge_file_lines = 600;
    let f = analyze(&a, &cfg);
    let lf = f.iter().find(|f| f.rule == "large-file").unwrap();
    assert_eq!(lf.severity, Severity::High);
    assert_eq!(lf.metric.as_ref().unwrap().threshold, 600.0);
}

#[test]
fn layer_rules_flag_upward_imports() {
    let t = write_tree(&[
        ("README.md", "x"),
        ("src/ui/view.js", "import { svc } from '../service/svc';\n"),
        ("src/service/svc.js", "import { db } from '../data/db';\nexport const svc = 1;\n"),
        ("src/data/db.js", "import { view } from '../ui/view';\nexport const db = 1;\n"),
    ]);
    let a = analyze_tree(&t, "full");
    let cfg = HealthConfig {
        layers: vec![
            Layer { name: "UI".into(), patterns: vec!["src/ui".into()] },
            Layer { name: "Service".into(), patterns: vec!["src/service".into()] },
            Layer { name: "Data".into(), patterns: vec!["src/data".into()] },
        ],
        ..Default::default()
    };
    let f = analyze(&a, &cfg);
    let v: Vec<_> = f.iter().filter(|f| f.rule == "layer-violation").collect();
    assert_eq!(v.len(), 1);
    assert!(v[0].title.contains("\"Data\" depends on higher layer \"UI\""));
    assert!(v[0].evidence[0].contains("src/data/db.js:1"));
}

#[test]
fn mapping_entry_points_silence_unreferenced() {
    let t = write_tree(&[
        ("README.md", "x"),
        ("src/index.js", "import './a';\n"),
        ("src/a.js", ""),
        ("src/plugin/loader.js", ""),
    ]);
    let a = analyze_tree(&t, "full");
    assert!(a
        .findings
        .iter()
        .any(|f| f.rule == "unreferenced-module" && f.files.contains(&"src/plugin/loader.js".to_string())));
    let cfg = HealthConfig { extra_entry_points: vec!["src/plugin".into()], ..Default::default() };
    assert!(!analyze(&a, &cfg).iter().any(|f| f.rule == "unreferenced-module"));
}

#[test]
fn structure_rules() {
    let mut files: Vec<(String, String)> = (0..65).map(|i| (format!("flat/f{i}.txt"), format!("{i}"))).collect();
    files.push(("a/b/c/d/e/f/g/h/i/deep.txt".into(), "x".into()));
    files.push(("x/y/z/w/leaf.js".into(), "x".into()));
    files.push(("m1/a.js".into(), "1".into()));
    files.push(("m1/b.js".into(), "2".into()));
    files.push(("m1/c.js".into(), "3".into()));
    files.push(("m2/a.js".into(), "4".into()));
    files.push(("m2/b.js".into(), "5".into()));
    files.push(("m2/c.js".into(), "6".into()));
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let t = write_tree(&refs);
    let a = analyze_tree(&t, "full");
    let r = rules_of(&a);
    for want in ["wide-directory", "deep-nesting", "unnecessary-nesting", "similar-structure", "missing-readme"] {
        assert!(r.contains(&want), "missing {want}: {r:?}");
    }
}

#[test]
fn registry_is_consistent() {
    let reg = registry();
    let mut ids = std::collections::HashSet::new();
    for r in &reg {
        assert!(ids.insert(r.id), "duplicate rule id {}", r.id);
        assert!(!r.description.is_empty());
    }
    assert!(reg.len() >= 20);
}

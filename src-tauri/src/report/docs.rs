//! Documentation intelligence: what documentation exists and where it is missing.

use crate::analysis::model::Analysis;
use crate::project::roles::Role;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocArea {
    pub path: String,
    pub source_files: u32,
    pub doc_files: u32,
    pub has_readme: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DocOverview {
    pub readme: Option<String>,
    pub readmes: Vec<String>,
    pub doc_roots: Vec<String>,
    pub markdown_files: u32,
    pub doc_lines: u64,
    pub community: BTreeMap<String, Option<String>>,
    pub api_docs: Vec<String>,
    pub config_docs: Vec<String>,
    /// Per top-level area (depth 1-2 directories containing source code).
    pub areas: Vec<DocArea>,
    /// Areas with source code but no README/Markdown inside.
    pub undocumented_areas: Vec<String>,
}

pub fn overview(a: &Analysis) -> DocOverview {
    let mut o = DocOverview {
        readme: a.structure.readme.clone(),
        doc_roots: a.structure.doc_roots.clone(),
        ..Default::default()
    };
    for key in ["license", "contributing", "changelog", "code_of_conduct", "security"] {
        let found = a
            .files
            .iter()
            .find(|f| !f.path.contains('/') && f.name().to_ascii_lowercase().starts_with(key))
            .map(|f| f.path.clone());
        o.community.insert(key.into(), found);
    }
    let mut areas: BTreeMap<String, DocArea> = BTreeMap::new();
    for f in &a.files {
        let lower = f.path.to_ascii_lowercase();
        let name = f.name().to_ascii_lowercase();
        let is_doc = f.role() == Role::Documentation
            || f.language.as_deref().is_some_and(|l| matches!(l, "markdown" | "rst" | "asciidoc"));
        if name.starts_with("readme") {
            o.readmes.push(f.path.clone());
        }
        if f.language.as_deref() == Some("markdown") {
            o.markdown_files += 1;
        }
        if is_doc {
            o.doc_lines += f.lines.map(|l| l.total as u64).unwrap_or(0);
        }
        if name.starts_with("openapi.")
            || name.starts_with("swagger.")
            || name == "typedoc.json"
            || name.ends_with(".apib")
            || lower.contains("docs/api/")
            || lower.contains("/apidocs/")
            || name == "mkdocs.yml"
            || name == "docusaurus.config.js"
            || name == "book.toml"
            || name == "conf.py" && lower.contains("doc")
        {
            o.api_docs.push(f.path.clone());
        }
        if (name.contains("config")
            || name.contains("settings")
            || name.contains("environment")
            || name.contains("configuration"))
            && is_doc
        {
            o.config_docs.push(f.path.clone());
        }
        let parts: Vec<&str> = f.path.split('/').collect();
        if parts.len() < 2 {
            continue;
        }
        for depth in 1..=2.min(parts.len() - 1) {
            let key = parts[..depth].join("/");
            let e = areas.entry(key.clone()).or_insert(DocArea {
                path: key,
                source_files: 0,
                doc_files: 0,
                has_readme: false,
            });
            if f.role() == Role::Source {
                e.source_files += 1;
            }
            if is_doc {
                e.doc_files += 1;
            }
            if name.starts_with("readme") && f.dir() == e.path {
                e.has_readme = true;
            }
        }
    }
    o.areas = areas.into_values().filter(|x| x.source_files > 0).collect();
    o.undocumented_areas =
        o.areas.iter().filter(|x| x.source_files >= 10 && x.doc_files == 0).map(|x| x.path.clone()).collect();
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{analyze_tree, write_tree};

    #[test]
    fn documentation_overview() {
        let mut files: Vec<(String, String)> = (0..12).map(|i| (format!("core/m{i}.js"), "x".into())).collect();
        files.extend([
            ("README.md".to_string(), "# r".to_string()),
            ("LICENSE".into(), "MIT".into()),
            ("docs/api/openapi.yaml".into(), "x".into()),
            ("web/README.md".into(), "# w".into()),
            ("web/app.js".into(), "x".into()),
        ]);
        let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let a = analyze_tree(&write_tree(&refs), "full");
        let o = overview(&a);
        assert_eq!(o.readme.as_deref(), Some("README.md"));
        assert_eq!(o.readmes.len(), 2);
        assert_eq!(o.community["license"].as_deref(), Some("LICENSE"));
        assert!(o.community["contributing"].is_none());
        assert_eq!(o.api_docs, vec!["docs/api/openapi.yaml"]);
        assert_eq!(o.undocumented_areas, vec!["core"]);
        assert!(o.areas.iter().find(|x| x.path == "web").unwrap().has_readme);
    }
}

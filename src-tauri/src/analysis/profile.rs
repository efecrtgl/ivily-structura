//! Analysis profiles: reusable configurations of scope, analyzers and rules.

use crate::health::HealthConfig;
use crate::scan::ScanOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalysisProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
    pub scan: ScanOptions,
    /// Read text files: line counts, symbols, complexity.
    pub content: bool,
    /// Resolve imports and build the dependency graph (requires `content`).
    pub dependencies: bool,
    /// Hash files to find exact duplicates.
    pub duplicates: bool,
    pub health: bool,
    /// Files larger than this are listed but not read.
    pub max_file_kb: u64,
    pub health_config: HealthConfig,
    /// Report sections enabled by default when exporting.
    pub report_sections: Vec<String>,
}

impl Default for AnalysisProfile {
    fn default() -> Self {
        Self {
            id: "full".into(),
            name: "Full architecture analysis".into(),
            description: String::new(),
            builtin: false,
            scan: ScanOptions::default(),
            content: true,
            dependencies: true,
            duplicates: true,
            health: true,
            max_file_kb: 1024,
            health_config: HealthConfig::default(),
            report_sections: crate::report::ALL_SECTIONS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

pub fn builtin() -> Vec<AnalysisProfile> {
    let base = AnalysisProfile { builtin: true, ..Default::default() };
    let cats =
        |c: &[&str]| HealthConfig { categories: c.iter().map(|s| s.to_string()).collect(), ..Default::default() };
    vec![
        AnalysisProfile {
            id: "quick".into(),
            name: "Quick scan".into(),
            description: "Structure, languages and sizes from file metadata only. No file contents are read.".into(),
            content: false,
            dependencies: false,
            duplicates: false,
            health_config: cats(&["structure", "documentation", "testing", "configuration", "security"]),
            report_sections: vec!["overview".into(), "structure".into(), "languages".into(), "metrics".into()],
            ..base.clone()
        },
        AnalysisProfile {
            id: "full".into(),
            name: "Full architecture analysis".into(),
            description: "Everything: structure, languages, line counts, symbols, dependency graph, duplicates and all health rules.".into(),
            ..base.clone()
        },
        AnalysisProfile {
            id: "dependencies".into(),
            name: "Dependency analysis".into(),
            description: "Imports, internal and external dependencies, cycles and coupling.".into(),
            duplicates: false,
            health_config: cats(&["dependencies"]),
            report_sections: vec!["overview".into(), "dependencies".into(), "cycles".into(), "health".into()],
            ..base.clone()
        },
        AnalysisProfile {
            id: "security".into(),
            name: "Security-oriented structure review".into(),
            description: "Sensitive files, risky dependency specifications, undeclared packages and configuration conflicts. Hidden files are included.".into(),
            scan: ScanOptions { include_hidden: true, ..ScanOptions::default() },
            duplicates: false,
            health_config: cats(&["security", "configuration", "dependencies"]),
            report_sections: vec!["overview".into(), "health".into(), "dependencies".into()],
            ..base.clone()
        },
        AnalysisProfile {
            id: "performance".into(),
            name: "Performance-oriented structure review".into(),
            description: "Large and complex files, hubs, fan-out, cycles and duplicated code — structural hotspots that slow builds and changes.".into(),
            health_config: cats(&["size", "dependencies", "structure"]),
            report_sections: vec!["overview".into(), "metrics".into(), "health".into(), "cycles".into()],
            ..base.clone()
        },
        AnalysisProfile {
            id: "documentation".into(),
            name: "Documentation analysis".into(),
            description: "README files, documentation folders and their distribution across the project.".into(),
            dependencies: false,
            duplicates: false,
            health_config: cats(&["documentation", "testing"]),
            report_sections: vec!["overview".into(), "documentation".into(), "structure".into(), "health".into()],
            ..base
        },
    ]
}

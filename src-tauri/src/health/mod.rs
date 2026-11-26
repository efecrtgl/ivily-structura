//! Architecture health analysis.
//!
//! Rules are independent functions over the [`Analysis`]. Each finding states severity,
//! confidence, affected files, evidence, a suggestion and — for numeric rules — the measured
//! value and the configured threshold. There is deliberately **no aggregate score**.

mod rules;

use crate::analysis::model::{Analysis, Finding, Severity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Thresholds {
    pub large_file_lines: u32,
    pub huge_file_lines: u32,
    pub complex_file: u32,
    pub deep_block_nesting: u32,
    pub high_fan_in: u32,
    pub high_fan_out: u32,
    pub deep_directory: u32,
    pub wide_directory: u32,
    pub duplicate_min_bytes: u64,
    pub single_child_chain: u32,
    pub min_source_files_for_tests: u32,
    pub large_cycle: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            large_file_lines: 600,
            huge_file_lines: 1500,
            complex_file: 100,
            deep_block_nesting: 7,
            high_fan_in: 25,
            high_fan_out: 20,
            deep_directory: 8,
            wide_directory: 60,
            duplicate_min_bytes: 512,
            single_child_chain: 3,
            min_source_files_for_tests: 10,
            large_cycle: 5,
        }
    }
}

/// User-defined architecture layers (from Project Mapping). Listed top (e.g. UI) to bottom
/// (e.g. core). A layer may depend on layers *below* it only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub name: String,
    /// Glob patterns over project-relative paths.
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct HealthConfig {
    pub disabled_rules: Vec<String>,
    /// Only run rules of these categories (empty = all).
    pub categories: Vec<String>,
    pub thresholds: Thresholds,
    pub layers: Vec<Layer>,
    /// Paths (prefixes) the user marked as entry points in the mapping.
    pub extra_entry_points: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: &'static str,
    pub category: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub requires_dependencies: bool,
}

pub type RuleFn = fn(&Analysis, &HealthConfig, &mut Vec<Finding>);

pub struct Rule {
    pub info: RuleInfo,
    pub run: RuleFn,
}

pub fn registry() -> Vec<RuleInfo> {
    rules::all().into_iter().map(|r| r.info).collect()
}

pub fn analyze(a: &Analysis, cfg: &HealthConfig) -> Vec<Finding> {
    let mut out = Vec::new();
    for r in rules::all() {
        if cfg.disabled_rules.iter().any(|d| d == r.info.id) {
            continue;
        }
        if !cfg.categories.is_empty() && !cfg.categories.iter().any(|c| c == r.info.category) {
            continue;
        }
        if r.info.requires_dependencies && !a.meta.dependencies_analyzed {
            continue;
        }
        (r.run)(a, cfg, &mut out);
    }
    out.sort_by(|x, y| y.severity.cmp(&x.severity).then(y.confidence.cmp(&x.confidence)).then(x.rule.cmp(&y.rule)));
    out
}

pub fn count_by_severity(f: &[Finding]) -> [u32; 4] {
    let mut c = [0u32; 4];
    for x in f {
        c[match x.severity {
            Severity::Info => 0,
            Severity::Low => 1,
            Severity::Medium => 2,
            Severity::High => 3,
        }] += 1;
    }
    c
}

#[cfg(test)]
mod tests;

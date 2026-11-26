//! The analysis result model. Serialised as-is into snapshots (compressed JSON), so changes
//! must stay backwards compatible (`#[serde(default)]`) or bump [`ENGINE_VERSION`].

use crate::lang::mask::LineStats;
use crate::lang::{Category, Support};
use crate::project::ecosystem::Ecosystem;
use crate::project::roles::Role;
use crate::providers::ImportKind;
use crate::scan::{ScanOptions, ScanWarning};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ENGINE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Meta {
    pub id: String,
    pub root: String,
    pub name: String,
    pub started_at: i64,
    pub duration_ms: u64,
    pub profile_id: String,
    pub profile_name: String,
    pub engine_version: u32,
    pub app_version: String,
    pub truncated: bool,
    pub options: ScanOptions,
    pub content_analyzed: bool,
    pub dependencies_analyzed: bool,
    pub health_analyzed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ImportTarget {
    Internal { files: Vec<u32>, via: String },
    External { name: String, ecosystem: String },
    Builtin { name: String },
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileImport {
    pub spec: String,
    pub line: u32,
    pub kind: ImportKind,
    pub target: ImportTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FileRecord {
    pub path: String,
    pub size: u64,
    pub modified: Option<i64>,
    pub hidden: bool,
    pub symlink: bool,
    pub sensitive: bool,
    pub binary: bool,
    pub language: Option<String>,
    pub role: Option<Role>,
    pub lines: Option<LineStats>,
    pub complexity: Option<u32>,
    pub max_nesting: Option<u32>,
    pub symbol_count: u32,
    pub inline_tests: bool,
    pub exports: Vec<String>,
    pub namespace: Option<String>,
    pub imports: Vec<FileImport>,
    /// Why content was not analysed (`sensitive`, `too-large`, `binary`, `unreadable`, `disabled`).
    pub skipped: Option<String>,
    /// Short content hash (duplicate detection). Never the content itself.
    pub hash: Option<String>,
    pub fan_in: u32,
    pub fan_out: u32,
    pub external_deps: u32,
}

impl FileRecord {
    pub fn role(&self) -> Role {
        self.role.unwrap_or(Role::Other)
    }
    pub fn dir(&self) -> &str {
        self.path.rfind('/').map(|i| &self.path[..i]).unwrap_or("")
    }
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DirRecord {
    pub path: String,
    pub depth: u32,
    pub direct_files: u32,
    pub direct_dirs: u32,
    pub total_files: u32,
    pub total_bytes: u64,
    pub total_lines: u64,
    pub hidden: bool,
    pub symlink: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageStat {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub support: Support,
    pub files: u32,
    pub bytes: u64,
    pub lines: u64,
    pub code_lines: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntryPoint {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pattern {
    pub id: String,
    pub name: String,
    pub confidence: Confidence,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Structure {
    pub entry_points: Vec<EntryPoint>,
    pub source_roots: Vec<String>,
    pub test_roots: Vec<String>,
    pub doc_roots: Vec<String>,
    pub asset_roots: Vec<String>,
    pub patterns: Vec<Pattern>,
    pub roles: BTreeMap<String, u32>,
    pub readme: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    /// Import/use/include: a real dependency.
    Uses,
    /// Structural containment (Rust `mod x;`). Excluded from cycle detection.
    Contains,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    pub kind: EdgeKind,
    pub count: u32,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExternalPackage {
    pub name: String,
    pub ecosystem: String,
    pub declared: bool,
    pub declared_kind: Option<String>,
    pub version: Option<String>,
    pub manifest: Option<String>,
    pub use_count: u32,
    pub users: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Cycle {
    /// All files of the strongly connected component.
    pub files: Vec<u32>,
    /// One concrete loop through the component (first file repeated implicitly).
    pub path: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DepGraph {
    pub edges: Vec<Edge>,
    pub packages: Vec<ExternalPackage>,
    pub cycles: Vec<Cycle>,
    pub imports_total: u32,
    pub imports_internal: u32,
    pub imports_external: u32,
    pub imports_builtin: u32,
    pub imports_unresolved: u32,
    /// Resolution method -> count (relative, tsconfig-paths, module-path, ...).
    pub resolution_methods: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule: String,
    pub category: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub title: String,
    pub explanation: String,
    pub files: Vec<String>,
    pub evidence: Vec<String>,
    pub suggestion: String,
    /// Measured value and threshold for numeric rules (transparency).
    pub metric: Option<MetricValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricValue {
    pub name: String,
    pub value: f64,
    pub threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SizeItem {
    pub path: String,
    pub bytes: u64,
    pub lines: u64,
    pub files: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Metrics {
    // Exact (from filesystem metadata)
    pub total_files: u32,
    pub total_dirs: u32,
    pub total_bytes: u64,
    pub max_depth: u32,
    pub avg_depth: f64,
    pub hidden_files: u32,
    pub symlinks: u32,
    pub by_role: BTreeMap<String, u32>,
    // Exact for analysed text files
    pub text_files_analyzed: u32,
    pub total_lines: u64,
    pub blank_lines: u64,
    // Estimated (lexical)
    pub code_lines: u64,
    pub comment_lines: u64,
    pub source_code_lines: u64,
    pub avg_complexity: f64,
    pub max_complexity: u32,
    // Size rankings
    pub largest_files: Vec<SizeItem>,
    pub longest_files: Vec<SizeItem>,
    pub largest_dirs: Vec<SizeItem>,
    // Dependencies
    pub internal_edges: u32,
    pub modules_in_graph: u32,
    pub external_packages: u32,
    pub declared_dependencies: u32,
    pub undeclared_packages: u32,
    pub avg_fan_out: f64,
    pub max_fan_in: u32,
    pub max_fan_out: u32,
    pub cycles: u32,
    pub files_in_cycles: u32,
    // Skips
    pub skipped_sensitive: u32,
    pub skipped_large: u32,
    pub binary_files: u32,
    pub generated_files: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Analysis {
    pub meta: Meta,
    pub files: Vec<FileRecord>,
    pub dirs: Vec<DirRecord>,
    pub languages: Vec<LanguageStat>,
    pub ecosystems: Vec<Ecosystem>,
    pub structure: Structure,
    pub graph: DepGraph,
    pub metrics: Metrics,
    pub findings: Vec<Finding>,
    pub warnings: Vec<ScanWarning>,
}

impl Analysis {
    pub fn file_index(&self, path: &str) -> Option<usize> {
        self.files.binary_search_by(|f| f.path.as_str().cmp(path)).ok()
    }
}

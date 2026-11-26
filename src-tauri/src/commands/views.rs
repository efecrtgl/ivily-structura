//! Read-only views over an in-memory analysis.

use crate::analysis::model::*;
use crate::core::{normalize_rel, resolve_inside, validate_root};
use crate::error::{AppError, AppResult};
use crate::graph::{self, GraphQuery, GraphView};
use crate::project::roles::Role;
use crate::providers::{self, Symbol};
use crate::state::AppState;
use crate::store::mapping;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    meta: Meta,
    metrics: Metrics,
    languages: Vec<LanguageStat>,
    ecosystems: Vec<crate::project::ecosystem::Ecosystem>,
    structure: Structure,
    findings_by_severity: [u32; 4],
    findings_by_category: std::collections::BTreeMap<String, u32>,
    top_findings: Vec<Finding>,
    warnings: Vec<crate::scan::ScanWarning>,
    warning_count: usize,
    hubs: Vec<FileRow>,
    top_dirs: Vec<DirRecord>,
    documentation: crate::report::docs::DocOverview,
    resolution: ResolutionStats,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolutionStats {
    total: u32,
    internal: u32,
    external: u32,
    builtin: u32,
    unresolved: u32,
    methods: std::collections::BTreeMap<String, u32>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    path: String,
    name: String,
    role: Role,
    language: Option<String>,
    size: u64,
    modified: Option<i64>,
    lines: Option<u32>,
    code_lines: Option<u32>,
    complexity: Option<u32>,
    fan_in: u32,
    fan_out: u32,
    hidden: bool,
    sensitive: bool,
    skipped: Option<String>,
}

fn row(f: &FileRecord) -> FileRow {
    FileRow {
        path: f.path.clone(),
        name: f.name().to_string(),
        role: f.role(),
        language: f.language.clone(),
        size: f.size,
        modified: f.modified,
        lines: f.lines.map(|l| l.total),
        code_lines: f.lines.map(|l| l.code),
        complexity: f.complexity,
        fan_in: f.fan_in,
        fan_out: f.fan_out,
        hidden: f.hidden,
        sensitive: f.sensitive,
        skipped: f.skipped.clone(),
    }
}

#[tauri::command]
pub fn analysis_overview(state: State<AppState>, id: String) -> AppResult<Overview> {
    let (_, a) = state.analysis(&id)?;
    let mut by_cat = std::collections::BTreeMap::new();
    for f in &a.findings {
        *by_cat.entry(f.category.clone()).or_insert(0) += 1;
    }
    let mut hubs: Vec<&FileRecord> = a.files.iter().filter(|f| f.fan_in > 0).collect();
    hubs.sort_by_key(|f| std::cmp::Reverse(f.fan_in));
    let mut top_dirs: Vec<DirRecord> = a.dirs.iter().filter(|d| d.depth == 1).cloned().collect();
    top_dirs.sort_by_key(|d| std::cmp::Reverse(d.total_bytes));
    let g = &a.graph;
    Ok(Overview {
        meta: a.meta.clone(),
        metrics: a.metrics.clone(),
        languages: a.languages.clone(),
        ecosystems: a.ecosystems.clone(),
        structure: a.structure.clone(),
        findings_by_severity: crate::health::count_by_severity(&a.findings),
        findings_by_category: by_cat,
        top_findings: a.findings.iter().take(8).cloned().collect(),
        warning_count: a.warnings.len(),
        warnings: a.warnings.iter().take(200).cloned().collect(),
        hubs: hubs.into_iter().take(10).map(row).collect(),
        top_dirs: top_dirs.into_iter().take(12).collect(),
        documentation: crate::report::docs::overview(&a),
        resolution: ResolutionStats {
            total: g.imports_total,
            internal: g.imports_internal,
            external: g.imports_external,
            builtin: g.imports_builtin,
            unresolved: g.imports_unresolved,
            methods: g.resolution_methods.clone(),
        },
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FileQuery {
    search: Option<String>,
    roles: Vec<String>,
    languages: Vec<String>,
    extension: Option<String>,
    dir: Option<String>,
    /// `path`, `size`, `lines`, `complexity`, `fanIn`, `fanOut`, `modified`
    sort: Option<String>,
    desc: bool,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePage {
    total: usize,
    rows: Vec<FileRow>,
}

/// Search, filter and sort the analysed files (fast, in memory).
#[tauri::command]
pub fn analysis_files(state: State<AppState>, id: String, query: FileQuery) -> AppResult<FilePage> {
    let (_, a) = state.analysis(&id)?;
    let s = query.search.as_deref().map(str::to_lowercase).filter(|s| !s.is_empty());
    let ext = query.extension.as_deref().map(|e| e.trim_start_matches('.').to_lowercase()).filter(|e| !e.is_empty());
    let dir = query.dir.as_deref().map(|d| d.trim_end_matches('/').to_string()).filter(|d| !d.is_empty());
    let mut v: Vec<&FileRecord> = a
        .files
        .iter()
        .filter(|f| s.as_ref().is_none_or(|s| f.path.to_lowercase().contains(s.as_str())))
        .filter(|f| query.roles.is_empty() || query.roles.iter().any(|r| r == f.role().as_str()))
        .filter(|f| {
            query.languages.is_empty() || f.language.as_deref().is_some_and(|l| query.languages.iter().any(|x| x == l))
        })
        .filter(|f| ext.as_ref().is_none_or(|e| crate::lang::extension(f.name()).as_deref() == Some(e.as_str())))
        .filter(|f| dir.as_ref().is_none_or(|d| f.path.starts_with(&format!("{d}/"))))
        .collect();
    match query.sort.as_deref().unwrap_or("path") {
        "size" => v.sort_by_key(|f| f.size),
        "lines" => v.sort_by_key(|f| f.lines.map(|l| l.total).unwrap_or(0)),
        "complexity" => v.sort_by_key(|f| f.complexity.unwrap_or(0)),
        "fanIn" => v.sort_by_key(|f| f.fan_in),
        "fanOut" => v.sort_by_key(|f| f.fan_out),
        "modified" => v.sort_by_key(|f| f.modified.unwrap_or(0)),
        _ => {}
    }
    if query.desc {
        v.reverse();
    }
    let total = v.len();
    let off = query.offset.unwrap_or(0);
    let lim = query.limit.unwrap_or(200).min(5000);
    Ok(FilePage { total, rows: v.into_iter().skip(off).take(lim).map(row).collect() })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    path: String,
    line: u32,
    spec: String,
    kind: crate::providers::ImportKind,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    spec: String,
    line: u32,
    kind: crate::providers::ImportKind,
    /// `internal`, `external`, `builtin`, `unresolved`
    target_kind: String,
    targets: Vec<String>,
    via: Option<String>,
    package: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommit {
    hash: String,
    author: String,
    timestamp: i64,
    subject: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDetail {
    file: FileRecord,
    language_name: Option<String>,
    support: Option<crate::lang::Support>,
    imports: Vec<ImportRow>,
    referenced_by: Vec<Reference>,
    symbols: Vec<Symbol>,
    /// Why symbols are not available (sensitive/binary/unsupported/changed).
    symbols_note: Option<String>,
    findings: Vec<Finding>,
    in_cycle: bool,
    annotation: Option<mapping::Annotation>,
    git: Vec<GitCommit>,
    git_note: Option<String>,
}

fn git_history(root: &std::path::Path, rel: &str) -> (Vec<GitCommit>, Option<String>) {
    if !root.join(".git").exists() {
        return (vec![], Some("not a git repository".into()));
    }
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(root).args(["log", "--follow", "-n", "15", "--format=%H%x1f%an%x1f%at%x1f%s", "--", rel]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    match cmd.output() {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let commits = s
                .lines()
                .filter_map(|l| {
                    let p: Vec<&str> = l.split('\u{1f}').collect();
                    (p.len() == 4).then(|| GitCommit {
                        hash: p[0].chars().take(12).collect(),
                        author: p[1].into(),
                        timestamp: p[2].parse().unwrap_or(0),
                        subject: p[3].chars().take(200).collect(),
                    })
                })
                .collect::<Vec<_>>();
            let note = commits.is_empty().then(|| "no commits touch this file".to_string());
            (commits, note)
        }
        Ok(_) => (vec![], Some("git could not read the history".into())),
        Err(_) => (vec![], Some("git is not installed or not on PATH".into())),
    }
}

/// Everything known about one file. Symbols are re-extracted on demand (never stored).
#[tauri::command]
pub fn file_detail(state: State<AppState>, id: String, path: String) -> AppResult<FileDetail> {
    let (project_id, a) = state.analysis(&id)?;
    let path = normalize_rel(&path)?;
    let idx = a.file_index(&path).ok_or_else(|| AppError::NotFound(path.clone()))?;
    let f = &a.files[idx];
    let lang = f.language.as_deref().and_then(crate::lang::by_id);
    let imports = f
        .imports
        .iter()
        .map(|i| {
            let (target_kind, targets, via, package) = match &i.target {
                ImportTarget::Internal { files, via } => (
                    "internal",
                    files.iter().map(|x| a.files[*x as usize].path.clone()).collect(),
                    Some(via.clone()),
                    None,
                ),
                ImportTarget::External { name, ecosystem } => {
                    ("external", vec![], None, Some(format!("{ecosystem}:{name}")))
                }
                ImportTarget::Builtin { name } => ("builtin", vec![], None, Some(name.clone())),
                ImportTarget::Unresolved => ("unresolved", vec![], None, None),
            };
            ImportRow {
                spec: i.spec.clone(),
                line: i.line,
                kind: i.kind,
                target_kind: target_kind.into(),
                targets,
                via,
                package,
            }
        })
        .collect();
    let mut referenced_by = Vec::new();
    for e in a.graph.edges.iter().filter(|e| e.to as usize == idx) {
        let src = &a.files[e.from as usize];
        for i in src
            .imports
            .iter()
            .filter(|i| matches!(&i.target, ImportTarget::Internal { files, .. } if files.contains(&(idx as u32))))
        {
            referenced_by.push(Reference { path: src.path.clone(), line: i.line, spec: i.spec.clone(), kind: i.kind });
        }
    }
    // Symbols: re-read the file only if it is safe, inside the project and unchanged in size.
    let project = crate::store::projects::get(&state.db, project_id)?;
    let (mut symbols, mut symbols_note) = (vec![], None);
    if f.sensitive {
        symbols_note = Some("sensitive file — contents are never read".into());
    } else if f.binary {
        symbols_note = Some("binary file".into());
    } else if lang.is_none_or(|l| l.support < crate::lang::Support::Lexical) {
        symbols_note = Some("symbol extraction is not available for this language (detection only)".into());
    } else if let Ok(root) = validate_root(&project.root) {
        match resolve_inside(&root, &path) {
            Ok(p) => match std::fs::metadata(&p) {
                Ok(m) if m.len() > 4 * 1024 * 1024 => {
                    symbols_note = Some("file too large for symbol extraction".into())
                }
                Ok(_) => match std::fs::read(&p) {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        symbols = providers::analyze(lang.unwrap().id, &text).symbols;
                        if bytes.len() as u64 != f.size {
                            symbols_note =
                                Some("the file changed since the analysis; symbols reflect the current file".into());
                        }
                    }
                    Err(_) => symbols_note = Some("file could not be read".into()),
                },
                Err(_) => symbols_note = Some("file no longer exists".into()),
            },
            Err(_) => symbols_note = Some("file no longer exists or is outside the project".into()),
        }
    } else {
        symbols_note = Some("project folder is not available".into());
    }
    let (git, git_note) = match validate_root(&project.root) {
        Ok(root) => git_history(&root, &path),
        Err(_) => (vec![], Some("project folder is not available".into())),
    };
    let annotation = mapping::get(&state.db, project_id)?.annotations.into_iter().find(|x| x.path == path);
    Ok(FileDetail {
        language_name: lang.map(|l| l.name.to_string()),
        support: lang.map(|l| l.support),
        imports,
        referenced_by,
        symbols,
        symbols_note,
        findings: a.findings.iter().filter(|x| x.files.contains(&path)).cloned().collect(),
        in_cycle: a.graph.cycles.iter().any(|c| c.files.contains(&(idx as u32))),
        annotation,
        git,
        git_note,
        file: f.clone(),
    })
}

#[tauri::command]
pub fn graph_view(state: State<AppState>, id: String, query: GraphQuery) -> AppResult<GraphView> {
    let (_, a) = state.analysis(&id)?;
    Ok(graph::view(&a, &query))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyView {
    ecosystems: Vec<crate::project::ecosystem::Ecosystem>,
    packages: Vec<ExternalPackage>,
    unresolved: Vec<Reference>,
    unresolved_total: u32,
    resolution_methods: std::collections::BTreeMap<String, u32>,
    cycles: Vec<CycleView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleView {
    files: Vec<String>,
    path: Vec<String>,
}

#[tauri::command]
pub fn dependency_view(state: State<AppState>, id: String) -> AppResult<DependencyView> {
    let (_, a) = state.analysis(&id)?;
    let mut unresolved = Vec::new();
    for f in &a.files {
        for i in f.imports.iter().filter(|i| i.target == ImportTarget::Unresolved) {
            if unresolved.len() < 1000 {
                unresolved.push(Reference { path: f.path.clone(), line: i.line, spec: i.spec.clone(), kind: i.kind });
            }
        }
    }
    let p = |i: &u32| a.files[*i as usize].path.clone();
    Ok(DependencyView {
        ecosystems: a.ecosystems.clone(),
        packages: a.graph.packages.clone(),
        unresolved,
        unresolved_total: a.graph.imports_unresolved,
        resolution_methods: a.graph.resolution_methods.clone(),
        cycles: a
            .graph
            .cycles
            .iter()
            .map(|c| CycleView { files: c.files.iter().map(p).collect(), path: c.path.iter().map(p).collect() })
            .collect(),
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FindingQuery {
    severities: Vec<Severity>,
    categories: Vec<String>,
    search: Option<String>,
    path: Option<String>,
}

#[tauri::command]
pub fn analysis_findings(state: State<AppState>, id: String, query: FindingQuery) -> AppResult<Vec<Finding>> {
    let (_, a) = state.analysis(&id)?;
    let s = query.search.as_deref().map(str::to_lowercase).filter(|s| !s.is_empty());
    Ok(a.findings
        .iter()
        .filter(|f| query.severities.is_empty() || query.severities.contains(&f.severity))
        .filter(|f| query.categories.is_empty() || query.categories.contains(&f.category))
        .filter(|f| {
            s.as_ref().is_none_or(|s| {
                f.title.to_lowercase().contains(s.as_str())
                    || f.rule.contains(s.as_str())
                    || f.files.iter().any(|p| p.to_lowercase().contains(s.as_str()))
            })
        })
        .filter(|f| {
            query.path.as_ref().is_none_or(|p| f.files.iter().any(|x| x == p || x.starts_with(&format!("{p}/"))))
        })
        .cloned()
        .collect())
}

/// Directory children from the analysis (used by the structure tree and maps).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    path: String,
    name: String,
    is_dir: bool,
    files: u32,
    bytes: u64,
    lines: u64,
    role: Option<Role>,
    language: Option<String>,
}

#[tauri::command]
pub fn analysis_tree(state: State<AppState>, id: String, path: String) -> AppResult<Vec<TreeNode>> {
    let (_, a) = state.analysis(&id)?;
    let dir = normalize_rel(&path)?;
    let depth = if dir.is_empty() { 1 } else { dir.matches('/').count() as u32 + 2 };
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let mut out: Vec<TreeNode> = a
        .dirs
        .iter()
        .filter(|d| d.depth == depth && d.path.starts_with(&prefix))
        .map(|d| TreeNode {
            name: d.path.rsplit('/').next().unwrap_or(&d.path).into(),
            path: d.path.clone(),
            is_dir: true,
            files: d.total_files,
            bytes: d.total_bytes,
            lines: d.total_lines,
            role: None,
            language: None,
        })
        .collect();
    out.extend(a.files.iter().filter(|f| f.dir() == dir).map(|f| TreeNode {
        name: f.name().into(),
        path: f.path.clone(),
        is_dir: false,
        files: 1,
        bytes: f.size,
        lines: f.lines.map(|l| l.total as u64).unwrap_or(0),
        role: Some(f.role()),
        language: f.language.clone(),
    }));
    Ok(out)
}

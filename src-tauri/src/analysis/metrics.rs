//! Directory aggregation, language statistics and project metrics.

use super::model::{Analysis, DirRecord, LanguageStat, Metrics, SizeItem};
use crate::lang;
use crate::project::roles::Role;
use crate::scan::RawEntry;
use std::collections::{BTreeMap, HashMap};

pub fn build_dirs(entries: &[RawEntry], a: &Analysis) -> Vec<DirRecord> {
    let mut map: BTreeMap<String, DirRecord> = BTreeMap::new();
    for e in entries.iter().filter(|e| e.is_dir) {
        map.insert(
            e.path.clone(),
            DirRecord {
                path: e.path.clone(),
                depth: e.depth as u32,
                hidden: e.hidden,
                symlink: e.symlink,
                ..Default::default()
            },
        );
    }
    for e in entries.iter().filter(|e| e.is_dir) {
        if let Some(p) = e.path.rfind('/').map(|i| &e.path[..i]) {
            if let Some(d) = map.get_mut(p) {
                d.direct_dirs += 1;
            }
        }
    }
    for f in &a.files {
        let lines = f.lines.map(|l| l.total as u64).unwrap_or(0);
        let mut dir = f.dir();
        let mut direct = true;
        while !dir.is_empty() {
            if let Some(d) = map.get_mut(dir) {
                if direct {
                    d.direct_files += 1;
                }
                d.total_files += 1;
                d.total_bytes += f.size;
                d.total_lines += lines;
            }
            direct = false;
            dir = dir.rfind('/').map(|i| &dir[..i]).unwrap_or("");
        }
    }
    map.into_values().collect()
}

pub fn build_languages(a: &Analysis) -> Vec<LanguageStat> {
    let mut m: HashMap<&str, LanguageStat> = HashMap::new();
    for f in &a.files {
        let Some(id) = f.language.as_deref() else { continue };
        let Some(l) = lang::by_id(id) else { continue };
        let s = m.entry(l.id).or_insert_with(|| LanguageStat {
            id: l.id.into(),
            name: l.name.into(),
            category: l.category,
            support: l.support,
            files: 0,
            bytes: 0,
            lines: 0,
            code_lines: 0,
        });
        s.files += 1;
        s.bytes += f.size;
        if let Some(ls) = f.lines {
            s.lines += ls.total as u64;
            s.code_lines += ls.code as u64;
        }
    }
    let mut v: Vec<LanguageStat> = m.into_values().collect();
    v.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.id.cmp(&b.id)));
    v
}

pub fn build_metrics(a: &Analysis) -> Metrics {
    let mut m = Metrics { total_files: a.files.len() as u32, total_dirs: a.dirs.len() as u32, ..Default::default() };
    let mut depth_sum = 0u64;
    let mut complexity_sum = 0u64;
    let mut complexity_n = 0u64;
    for f in &a.files {
        m.total_bytes += f.size;
        let depth = f.path.matches('/').count() as u32 + 1;
        depth_sum += depth as u64;
        m.max_depth = m.max_depth.max(depth);
        if f.hidden {
            m.hidden_files += 1;
        }
        if f.symlink {
            m.symlinks += 1;
        }
        *m.by_role.entry(f.role().as_str().to_string()).or_default() += 1;
        if f.role() == Role::Generated {
            m.generated_files += 1;
        }
        if f.binary {
            m.binary_files += 1;
        }
        match f.skipped.as_deref() {
            Some("sensitive") => m.skipped_sensitive += 1,
            Some("too-large") => m.skipped_large += 1,
            _ => {}
        }
        if let Some(l) = f.lines {
            m.text_files_analyzed += 1;
            m.total_lines += l.total as u64;
            m.blank_lines += l.blank as u64;
            m.code_lines += l.code as u64;
            m.comment_lines += l.comment as u64;
            if f.role() == Role::Source {
                m.source_code_lines += l.code as u64;
            }
        }
        if let Some(c) = f.complexity {
            if f.role() == Role::Source {
                complexity_sum += c as u64;
                complexity_n += 1;
                m.max_complexity = m.max_complexity.max(c);
            }
        }
        m.max_fan_in = m.max_fan_in.max(f.fan_in);
        m.max_fan_out = m.max_fan_out.max(f.fan_out);
    }
    if !a.files.is_empty() {
        m.avg_depth = depth_sum as f64 / a.files.len() as f64;
    }
    if complexity_n > 0 {
        m.avg_complexity = complexity_sum as f64 / complexity_n as f64;
    }
    let mut by_size: Vec<_> = a.files.iter().collect();
    by_size.sort_by_key(|f| std::cmp::Reverse(f.size));
    m.largest_files = by_size
        .iter()
        .take(20)
        .map(|f| SizeItem {
            path: f.path.clone(),
            bytes: f.size,
            lines: f.lines.map(|l| l.total as u64).unwrap_or(0),
            files: 1,
        })
        .collect();
    let mut by_lines: Vec<_> = a
        .files
        .iter()
        .filter(|f| f.lines.is_some() && f.role() != Role::Lockfile && f.role() != Role::Generated)
        .collect();
    by_lines.sort_by_key(|f| std::cmp::Reverse(f.lines.map(|l| l.total).unwrap_or(0)));
    m.longest_files = by_lines
        .iter()
        .take(20)
        .map(|f| SizeItem {
            path: f.path.clone(),
            bytes: f.size,
            lines: f.lines.map(|l| l.total as u64).unwrap_or(0),
            files: 1,
        })
        .collect();
    // Largest top-level-ish directories (depth <= 2) to avoid listing every ancestor.
    let mut dirs: Vec<_> = a.dirs.iter().filter(|d| d.depth <= 2).collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.total_bytes));
    m.largest_dirs = dirs
        .iter()
        .take(20)
        .map(|d| SizeItem { path: d.path.clone(), bytes: d.total_bytes, lines: d.total_lines, files: d.total_files })
        .collect();

    let g = &a.graph;
    m.internal_edges = g.edges.iter().filter(|e| e.kind == super::model::EdgeKind::Uses).count() as u32;
    let mut in_graph = vec![false; a.files.len()];
    for e in &g.edges {
        in_graph[e.from as usize] = true;
        in_graph[e.to as usize] = true;
    }
    m.modules_in_graph = in_graph.iter().filter(|x| **x).count() as u32;
    m.external_packages = g.packages.iter().filter(|p| p.use_count > 0).count() as u32;
    m.declared_dependencies = a.ecosystems.iter().map(|e| e.dependencies.len() as u32).sum();
    m.undeclared_packages = g.packages.iter().filter(|p| p.use_count > 0 && !p.declared).count() as u32;
    let with_out: Vec<u32> = a.files.iter().filter(|f| f.fan_out > 0).map(|f| f.fan_out).collect();
    if !with_out.is_empty() {
        m.avg_fan_out = with_out.iter().map(|x| *x as f64).sum::<f64>() / with_out.len() as f64;
    }
    m.cycles = g.cycles.len() as u32;
    m.files_in_cycles = g.cycles.iter().map(|c| c.files.len() as u32).sum();
    m
}

//! Graph algorithms (cycles) and UI graph views (aggregation, focus, limits).

use crate::analysis::model::{Analysis, Cycle, Edge, EdgeKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// Strongly connected components with more than one node (iterative Tarjan).
pub fn strongly_connected(n: usize, adj: &[Vec<u32>]) -> Vec<Vec<u32>> {
    let mut index = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<u32> = Vec::new();
    let mut next = 0u32;
    let mut out = Vec::new();
    for start in 0..n {
        if index[start] != u32::MAX {
            continue;
        }
        // (node, next child position)
        let mut call: Vec<(u32, usize)> = vec![(start as u32, 0)];
        index[start] = next;
        low[start] = next;
        next += 1;
        stack.push(start as u32);
        on_stack[start] = true;
        while let Some(&mut (v, ref mut pos)) = call.last_mut() {
            let vu = v as usize;
            if *pos < adj[vu].len() {
                let w = adj[vu][*pos] as usize;
                *pos += 1;
                if index[w] == u32::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w as u32);
                    on_stack[w] = true;
                    call.push((w as u32, 0));
                } else if on_stack[w] {
                    low[vu] = low[vu].min(index[w]);
                }
            } else {
                call.pop();
                if let Some(&(p, _)) = call.last() {
                    low[p as usize] = low[p as usize].min(low[vu]);
                }
                if low[vu] == index[vu] {
                    let mut comp = Vec::new();
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack[w as usize] = false;
                        comp.push(w);
                        if w as usize == vu {
                            break;
                        }
                    }
                    if comp.len() > 1 {
                        comp.sort_unstable();
                        out.push(comp);
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    out
}

/// Shortest loop through `start` that stays inside `members` (BFS).
pub fn shortest_cycle(start: u32, members: &HashSet<u32>, adj: &[Vec<u32>]) -> Vec<u32> {
    let mut prev: HashMap<u32, u32> = HashMap::new();
    let mut q = VecDeque::new();
    for &w in &adj[start as usize] {
        if members.contains(&w) && !prev.contains_key(&w) {
            prev.insert(w, start);
            q.push_back(w);
        }
    }
    while let Some(v) = q.pop_front() {
        if v == start {
            let mut path = vec![start];
            let mut cur = prev[&start];
            while cur != start {
                path.push(cur);
                cur = prev[&cur];
            }
            path[1..].reverse();
            return path;
        }
        for &w in &adj[v as usize] {
            if members.contains(&w) && !prev.contains_key(&w) {
                prev.insert(w, v);
                q.push_back(w);
            }
        }
    }
    vec![start]
}

pub fn find_cycles(n: usize, edges: &[Edge]) -> Vec<Cycle> {
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for e in edges.iter().filter(|e| e.kind == EdgeKind::Uses && e.from != e.to) {
        adj[e.from as usize].push(e.to);
    }
    for a in adj.iter_mut() {
        a.sort_unstable();
        a.dedup();
    }
    strongly_connected(n, &adj)
        .into_iter()
        .map(|files| {
            let set: HashSet<u32> = files.iter().copied().collect();
            let path = shortest_cycle(files[0], &set, &adj);
            Cycle { files, path }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Views

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GraphQuery {
    /// `file` or `directory`
    pub level: String,
    /// Directory aggregation depth (1 = top-level folders).
    pub dir_depth: usize,
    pub include_external: bool,
    pub include_contains: bool,
    pub include_tests: bool,
    pub focus: Option<String>,
    pub focus_depth: u32,
    /// `both`, `in` (who uses the focus) or `out` (what the focus uses)
    pub direction: String,
    pub search: Option<String>,
    pub only_cycles: bool,
    pub roles: Vec<String>,
    pub languages: Vec<String>,
    pub path_prefix: Option<String>,
    pub max_nodes: usize,
}

impl Default for GraphQuery {
    fn default() -> Self {
        Self {
            level: "file".into(),
            dir_depth: 2,
            include_external: false,
            include_contains: true,
            include_tests: true,
            focus: None,
            focus_depth: 1,
            direction: "both".into(),
            search: None,
            only_cycles: false,
            roles: vec![],
            languages: vec![],
            path_prefix: None,
            max_nodes: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    /// `file`, `directory`, `package`
    pub kind: String,
    pub path: String,
    pub group: String,
    pub role: Option<String>,
    pub language: Option<String>,
    pub size: u64,
    pub files: u32,
    pub fan_in: u32,
    pub fan_out: u32,
    pub in_cycle: bool,
    pub entry: bool,
    pub focus: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub weight: u32,
    pub kind: String,
    pub cyclic: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphView {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub total_nodes: usize,
    pub truncated: bool,
    pub hint: Option<String>,
}

fn dir_key(path: &str, depth: usize) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let dirs = &parts[..parts.len().saturating_sub(1)];
    if dirs.is_empty() {
        return "(root)".into();
    }
    dirs[..depth.min(dirs.len()).max(1)].join("/")
}

pub fn view(a: &Analysis, q: &GraphQuery) -> GraphView {
    let n = a.files.len();
    let in_cycle: HashSet<u32> = a.graph.cycles.iter().flat_map(|c| c.files.iter().copied()).collect();
    let cyc_comp: HashMap<u32, usize> =
        a.graph.cycles.iter().enumerate().flat_map(|(ci, c)| c.files.iter().map(move |f| (*f, ci))).collect();
    let entries: HashSet<&str> = a.structure.entry_points.iter().map(|e| e.path.as_str()).collect();

    // 1. File filter
    let search = q.search.as_deref().map(|s| s.to_lowercase()).filter(|s| !s.is_empty());
    let keep = |i: usize| -> bool {
        let f = &a.files[i];
        if !q.include_tests && f.role() == crate::project::roles::Role::Test {
            return false;
        }
        if !q.roles.is_empty() && !q.roles.iter().any(|r| r == f.role().as_str()) {
            return false;
        }
        if !q.languages.is_empty() && !f.language.as_deref().is_some_and(|l| q.languages.iter().any(|x| x == l)) {
            return false;
        }
        if let Some(p) = q.path_prefix.as_deref().filter(|p| !p.is_empty()) {
            if !(f.path == p || f.path.starts_with(&format!("{}/", p.trim_end_matches('/')))) {
                return false;
            }
        }
        !(q.only_cycles && !in_cycle.contains(&(i as u32)))
    };
    let edges: Vec<&Edge> = a
        .graph
        .edges
        .iter()
        .filter(|e| q.include_contains || e.kind == EdgeKind::Uses)
        .filter(|e| keep(e.from as usize) && keep(e.to as usize))
        .collect();

    // Only files that take part in the graph (or match search/focus) become nodes.
    let mut active: HashSet<u32> = HashSet::new();
    for e in &edges {
        active.insert(e.from);
        active.insert(e.to);
    }
    if q.include_external {
        for p in &a.graph.packages {
            for u in &p.users {
                if keep(*u as usize) {
                    active.insert(*u);
                }
            }
        }
    }

    // 2. Focus neighbourhood
    let focus_idx = q.focus.as_deref().and_then(|p| a.file_index(p)).map(|i| i as u32);
    let focus_dir = q.focus.as_deref().filter(|_| focus_idx.is_none()).map(|s| s.trim_end_matches('/').to_string());
    if let Some(fi) = focus_idx {
        let mut out_adj: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut in_adj: HashMap<u32, Vec<u32>> = HashMap::new();
        for e in &edges {
            out_adj.entry(e.from).or_default().push(e.to);
            in_adj.entry(e.to).or_default().push(e.from);
        }
        let mut seen: HashSet<u32> = HashSet::from([fi]);
        let mut frontier = vec![fi];
        for _ in 0..q.focus_depth.max(1) {
            let mut nextf = Vec::new();
            for v in frontier {
                if q.direction != "in" {
                    for w in out_adj.get(&v).into_iter().flatten() {
                        if seen.insert(*w) {
                            nextf.push(*w);
                        }
                    }
                }
                if q.direction != "out" {
                    for w in in_adj.get(&v).into_iter().flatten() {
                        if seen.insert(*w) {
                            nextf.push(*w);
                        }
                    }
                }
            }
            frontier = nextf;
        }
        active = seen;
    }

    if let Some(s) = &search {
        let matches: HashSet<u32> = (0..n as u32)
            .filter(|i| keep(*i as usize) && a.files[*i as usize].path.to_lowercase().contains(s))
            .collect();
        if focus_idx.is_none() {
            // Matches plus their direct neighbours.
            let mut keep_set = matches.clone();
            for e in &edges {
                if matches.contains(&e.from) {
                    keep_set.insert(e.to);
                }
                if matches.contains(&e.to) {
                    keep_set.insert(e.from);
                }
            }
            active = keep_set;
        }
    }

    let level_dir = q.level == "directory";
    let node_key = |i: u32| -> String {
        let p = &a.files[i as usize].path;
        if level_dir {
            format!("d:{}", dir_key(p, q.dir_depth.max(1)))
        } else {
            format!("f:{p}")
        }
    };

    // 3. Build nodes
    let mut nodes: BTreeMap<String, GraphNode> = BTreeMap::new();
    for &i in &active {
        let f = &a.files[i as usize];
        let key = node_key(i);
        let entry = nodes.entry(key.clone()).or_insert_with(|| {
            if level_dir {
                let d = key[2..].to_string();
                GraphNode {
                    id: key.clone(),
                    label: d.rsplit('/').next().unwrap_or(&d).to_string(),
                    kind: "directory".into(),
                    path: d.clone(),
                    group: d.split('/').next().unwrap_or("").to_string(),
                    role: None,
                    language: None,
                    size: 0,
                    files: 0,
                    fan_in: 0,
                    fan_out: 0,
                    in_cycle: false,
                    entry: false,
                    focus: false,
                }
            } else {
                GraphNode {
                    id: key.clone(),
                    label: f.name().to_string(),
                    kind: "file".into(),
                    path: f.path.clone(),
                    group: dir_key(&f.path, 1),
                    role: Some(f.role().as_str().to_string()),
                    language: f.language.clone(),
                    size: f.lines.map(|l| l.total as u64).unwrap_or(0),
                    files: 1,
                    fan_in: f.fan_in,
                    fan_out: f.fan_out,
                    in_cycle: in_cycle.contains(&i),
                    entry: entries.contains(f.path.as_str()),
                    focus: Some(i) == focus_idx,
                }
            }
        });
        if level_dir {
            entry.files += 1;
            entry.size += f.lines.map(|l| l.total as u64).unwrap_or(0);
            entry.in_cycle |= in_cycle.contains(&i);
            entry.entry |= entries.contains(f.path.as_str());
            if let Some(fd) = &focus_dir {
                entry.focus |= entry.path == *fd;
            }
        }
    }

    // 4. Edges (aggregated)
    let mut agg: HashMap<(String, String, EdgeKind), (u32, bool)> = HashMap::new();
    for e in &edges {
        if !active.contains(&e.from) || !active.contains(&e.to) {
            continue;
        }
        let (s, t) = (node_key(e.from), node_key(e.to));
        if s == t {
            continue;
        }
        let cyclic = e.kind == EdgeKind::Uses && cyc_comp.get(&e.from).is_some_and(|c| cyc_comp.get(&e.to) == Some(c));
        let v = agg.entry((s, t, e.kind)).or_insert((0, false));
        v.0 += e.count.max(1);
        v.1 |= cyclic;
    }
    if q.include_external {
        for p in &a.graph.packages {
            let pid = format!("p:{}:{}", p.ecosystem, p.name);
            let mut used = false;
            for u in &p.users {
                if active.contains(u) {
                    used = true;
                    *agg.entry((node_key(*u), pid.clone(), EdgeKind::Uses)).or_insert((0, false)) = (1, false);
                }
            }
            if used {
                nodes.insert(
                    pid.clone(),
                    GraphNode {
                        id: pid,
                        label: p.name.clone(),
                        kind: "package".into(),
                        path: p.name.clone(),
                        group: format!("external:{}", p.ecosystem),
                        role: None,
                        language: None,
                        size: 0,
                        files: 0,
                        fan_in: p.users.len() as u32,
                        fan_out: 0,
                        in_cycle: false,
                        entry: false,
                        focus: false,
                    },
                );
            }
        }
    }
    let mut gedges: Vec<GraphEdge> = agg
        .into_iter()
        .map(|((s, t, k), (w, c))| GraphEdge {
            source: s,
            target: t,
            weight: w,
            kind: if k == EdgeKind::Uses { "uses".into() } else { "contains".into() },
            cyclic: c,
        })
        .collect();
    gedges.sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));

    if level_dir {
        for e in &gedges {
            if let Some(n) = nodes.get_mut(&e.source) {
                n.fan_out += 1;
            }
            if let Some(n) = nodes.get_mut(&e.target) {
                n.fan_in += 1;
            }
        }
    }

    // 5. Limit: keep focus + highest-degree nodes.
    let total = nodes.len();
    let max = q.max_nodes.clamp(10, 3000);
    let mut truncated = false;
    let mut hint = None;
    if total > max {
        truncated = true;
        let mut degree: HashMap<&str, u32> = HashMap::new();
        for e in &gedges {
            *degree.entry(e.source.as_str()).or_default() += e.weight;
            *degree.entry(e.target.as_str()).or_default() += e.weight;
        }
        let mut ranked: Vec<(&String, &GraphNode)> = nodes.iter().collect();
        ranked.sort_by_key(|(id, nd)| {
            (std::cmp::Reverse(nd.focus), std::cmp::Reverse(degree.get(id.as_str()).copied().unwrap_or(0)))
        });
        let keep_ids: HashSet<String> = ranked.into_iter().take(max).map(|(id, _)| id.clone()).collect();
        nodes.retain(|id, _| keep_ids.contains(id));
        gedges.retain(|e| keep_ids.contains(&e.source) && keep_ids.contains(&e.target));
        hint = Some(if level_dir {
            "Graph limited to the most connected folders. Narrow the path filter or reduce folder depth.".into()
        } else {
            "Graph limited to the most connected files. Switch to folder level, focus a file, or filter by path.".into()
        });
    }
    GraphView { nodes: nodes.into_values().collect(), edges: gedges, total_nodes: total, truncated, hint }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(from: u32, to: u32) -> Edge {
        Edge { from, to, kind: EdgeKind::Uses, count: 1, line: 1 }
    }

    #[test]
    fn finds_cycles_and_concrete_paths() {
        // 0 -> 1 -> 2 -> 0 ; 3 -> 4 -> 3 ; 5 isolated ; 2 -> 5
        let edges = vec![edge(0, 1), edge(1, 2), edge(2, 0), edge(3, 4), edge(4, 3), edge(2, 5)];
        let c = find_cycles(6, &edges);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].files, vec![0, 1, 2]);
        assert_eq!(c[0].path, vec![0, 1, 2]);
        assert_eq!(c[1].files, vec![3, 4]);
    }

    #[test]
    fn contains_edges_do_not_form_cycles() {
        let edges = vec![Edge { from: 0, to: 1, kind: EdgeKind::Contains, count: 1, line: 1 }, edge(1, 0)];
        assert!(find_cycles(2, &edges).is_empty());
    }

    #[test]
    fn deep_chains_do_not_overflow() {
        let n = 200_000;
        let mut edges: Vec<Edge> = (0..n - 1).map(|i| edge(i, i + 1)).collect();
        edges.push(edge(n - 1, 0));
        let c = find_cycles(n as usize, &edges);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].files.len(), n as usize);
    }

    #[test]
    fn dir_keys() {
        assert_eq!(dir_key("src/app/x.js", 1), "src");
        assert_eq!(dir_key("src/app/x.js", 2), "src/app");
        assert_eq!(dir_key("src/app/x.js", 5), "src/app");
        assert_eq!(dir_key("x.js", 2), "(root)");
    }
}

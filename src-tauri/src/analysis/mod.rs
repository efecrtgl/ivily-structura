//! Analysis pipeline: scan -> classify -> ecosystems -> content -> dependencies -> metrics -> health.

pub mod metrics;
pub mod model;
pub mod profile;

use crate::core::CancelToken;
use crate::deps::resolve::{self, FileNs, Resolver, Target};
use crate::error::AppResult;
use crate::lang;
use crate::project::{ecosystem, roles, structure};
use crate::providers::{self, ImportKind, ImportRef};
use crate::scan;
use model::*;
use profile::AnalysisProfile;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
}

/// Manifests and configs are read only when they are small.
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

fn new_id() -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}", nanos, COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Read a small, non-sensitive project file as text.
fn read_small(root: &Path, rel: &str) -> Option<String> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if roles::is_sensitive(name) {
        return None;
    }
    let p = root.join(rel);
    let meta = p.symlink_metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    let bytes = std::fs::read(&p).ok()?;
    Some(decode(&bytes)?.into_owned())
}

/// Decode text (UTF-8 with/without BOM, UTF-16 with BOM). `None` for binary data.
fn decode(bytes: &[u8]) -> Option<std::borrow::Cow<'_, str>> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Some(String::from_utf8_lossy(rest));
    }
    if bytes.len() >= 2 && (bytes[..2] == [0xFF, 0xFE] || bytes[..2] == [0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        return Some(std::borrow::Cow::Owned(String::from_utf16_lossy(&units)));
    }
    let probe = &bytes[..bytes.len().min(8000)];
    if probe.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(bytes))
}

pub fn run(
    root: &Path,
    profile: &AnalysisProfile,
    cancel: &CancelToken,
    progress: &(dyn Fn(Progress) + Sync),
) -> AppResult<Analysis> {
    let started = std::time::Instant::now();
    let mut a = Analysis {
        meta: Meta {
            id: new_id(),
            root: root.to_string_lossy().into_owned(),
            name: root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned()),
            started_at: crate::core::now_ts(),
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            engine_version: ENGINE_VERSION,
            app_version: crate::APP_VERSION.into(),
            options: profile.scan.clone(),
            content_analyzed: profile.content,
            dependencies_analyzed: profile.content && profile.dependencies,
            health_analyzed: profile.health,
            ..Default::default()
        },
        ..Default::default()
    };

    // 1. Scan
    progress(Progress { phase: "scan", done: 0, total: 0 });
    let scanned =
        scan::scan(root, &profile.scan, cancel, &mut |n| progress(Progress { phase: "scan", done: n, total: 0 }))?;
    a.meta.truncated = scanned.truncated;
    a.warnings = scanned.warnings;

    // 2. Classify
    for e in scanned.entries.iter().filter(|e| !e.is_dir) {
        let name = e.path.rsplit('/').next().unwrap_or(&e.path);
        let language = lang::detect(name);
        a.files.push(FileRecord {
            path: e.path.clone(),
            size: e.size,
            modified: e.modified,
            hidden: e.hidden,
            symlink: e.symlink,
            sensitive: roles::is_sensitive(name),
            language: language.map(|l| l.id.to_string()),
            role: Some(roles::classify(&e.path, language)),
            ..Default::default()
        });
    }
    a.files.sort_by(|x, y| x.path.cmp(&y.path));
    let file_paths: Vec<String> = a.files.iter().map(|f| f.path.clone()).collect();
    cancel.check()?;

    // 3. Ecosystems (manifest files only)
    progress(Progress { phase: "ecosystems", done: 0, total: 0 });
    a.ecosystems = ecosystem::detect(&file_paths, &|rel| read_small(root, rel));

    // 4. Content pass (parallel)
    let mut raw_imports: Vec<Vec<ImportRef>> = vec![Vec::new(); a.files.len()];
    let read_content = profile.content || profile.duplicates;
    if read_content {
        let total = a.files.len();
        let done = AtomicUsize::new(0);
        let max_bytes = profile.max_file_kb.saturating_mul(1024);
        let want_hash = profile.duplicates;
        let want_facts = profile.content;
        let want_imports = profile.content && profile.dependencies;
        a.files.par_iter_mut().zip(raw_imports.par_iter_mut()).try_for_each(|(f, imports)| -> AppResult<()> {
            let n = done.fetch_add(1, Ordering::Relaxed);
            if n % 256 == 0 {
                cancel.check()?;
                progress(Progress { phase: "read", done: n, total });
            }
            if f.sensitive {
                f.skipped = Some("sensitive".into());
                return Ok(());
            }
            if f.symlink {
                f.skipped = Some("symlink".into());
                return Ok(());
            }
            if f.size > max_bytes {
                f.skipped = Some("too-large".into());
                return Ok(());
            }
            let p = root.join(&f.path);
            let Ok(meta) = p.symlink_metadata() else {
                f.skipped = Some("unreadable".into());
                return Ok(());
            };
            if !meta.is_file() {
                f.skipped = Some("unreadable".into());
                return Ok(());
            }
            let mut bytes = Vec::with_capacity(f.size as usize);
            if std::fs::File::open(&p).and_then(|fh| fh.take(max_bytes + 1).read_to_end(&mut bytes)).is_err() {
                f.skipped = Some("unreadable".into());
                return Ok(());
            }
            if want_hash && !bytes.is_empty() {
                let h = Sha256::digest(&bytes);
                f.hash = Some(h[..16].iter().map(|b| format!("{b:02x}")).collect());
            }
            let Some(text) = decode(&bytes) else {
                f.binary = true;
                return Ok(());
            };
            if f.language.is_none() && !f.name().contains('.') {
                if let Some(l) = text.lines().next().and_then(lang::detect_shebang) {
                    f.language = Some(l.id.to_string());
                    f.role = Some(roles::classify(&f.path, Some(l)));
                }
            }
            let head: String = text.chars().take(1200).collect();
            if roles::has_generated_marker(&head)
                && matches!(f.role(), roles::Role::Source | roles::Role::Other | roles::Role::Data)
            {
                f.role = Some(roles::Role::Generated);
            }
            if !want_facts {
                return Ok(());
            }
            match f.language.clone() {
                Some(lid) => {
                    let facts = providers::analyze(&lid, &text);
                    f.lines = Some(facts.lines);
                    let programming = lang::by_id(&lid)
                        .is_some_and(|l| matches!(l.category, lang::Category::Programming | lang::Category::Shell));
                    if programming && providers::provider_for(&lid).is_some() {
                        f.complexity = Some(facts.complexity);
                        f.max_nesting = Some(facts.max_nesting);
                    }
                    f.symbol_count = facts.symbols.len() as u32;
                    f.inline_tests = facts.inline_tests;
                    f.exports = facts.exports.into_iter().take(200).collect();
                    f.namespace = facts.namespace;
                    if want_imports {
                        *imports = facts.imports;
                    }
                }
                None => {
                    f.lines = Some(providers::line_stats(None, &text));
                }
            }
            Ok(())
        })?;
        progress(Progress { phase: "read", done: total, total });
    }
    cancel.check()?;

    // 5. Dependencies
    if a.meta.dependencies_analyzed {
        progress(Progress { phase: "resolve", done: 0, total: a.files.len() });
        build_dependencies(root, &mut a, raw_imports);
    }
    cancel.check()?;

    // 6. Directories, languages, structure, metrics
    progress(Progress { phase: "metrics", done: 0, total: 0 });
    a.dirs = metrics::build_dirs(&scanned.entries, &a);
    a.languages = metrics::build_languages(&a);
    let dir_paths: Vec<String> = a.dirs.iter().map(|d| d.path.clone()).collect();
    a.structure = structure::analyze(&a.files, &a.ecosystems, &dir_paths);
    a.metrics = metrics::build_metrics(&a);

    // 7. Health
    if profile.health {
        progress(Progress { phase: "health", done: 0, total: 0 });
        a.findings = crate::health::analyze(&a, &profile.health_config);
    }
    a.meta.duration_ms = started.elapsed().as_millis() as u64;
    progress(Progress { phase: "done", done: a.files.len(), total: a.files.len() });
    Ok(a)
}

fn norm_pkg(eco: &str, name: &str) -> String {
    match eco {
        "python" | "rust" => name.to_ascii_lowercase().replace('_', "-"),
        _ => name.to_string(),
    }
}

fn build_dependencies(root: &Path, a: &mut Analysis, raw: Vec<Vec<ImportRef>>) {
    // Resolver inputs: tsconfig/jsconfig paths, bundler aliases.
    let mut ts = Vec::new();
    let mut aliases = Vec::new();
    for f in &a.files {
        let n = f.name().to_ascii_lowercase();
        if (n.starts_with("tsconfig") || n == "jsconfig.json") && n.ends_with(".json") {
            if let Some(t) = read_small(root, &f.path).and_then(|t| resolve::ts_paths_from(&f.path, &t)) {
                ts.push(t);
            }
        } else if [
            "vite.config.",
            "webpack.config.",
            "svelte.config.",
            "nuxt.config.",
            "vue.config.",
            "quasar.config.",
            "astro.config.",
        ]
        .iter()
        .any(|p| n.starts_with(p))
        {
            if let Some(t) = read_small(root, &f.path) {
                aliases.extend(resolve::alias_dirs_from_config(&f.path, &t));
            }
        }
    }
    let file_paths: Vec<String> = a.files.iter().map(|f| f.path.clone()).collect();
    let ns: Vec<FileNs> =
        a.files.iter().map(|f| FileNs { lang: f.language.as_deref(), namespace: f.namespace.as_deref() }).collect();
    let resolver = Resolver::new(&file_paths, &ns, &a.ecosystems, ts, aliases);

    let mut g = DepGraph::default();
    let mut edges: HashMap<(u32, u32, EdgeKind), (u32, u32)> = HashMap::new();
    let mut packages: BTreeMap<(String, String), (HashSet<u32>, u32)> = BTreeMap::new();
    let mut resolved_imports: Vec<Vec<FileImport>> = vec![Vec::new(); a.files.len()];
    for (i, imports) in raw.into_iter().enumerate() {
        if imports.is_empty() {
            continue;
        }
        let lang = a.files[i].language.clone().unwrap_or_default();
        let own_ns = a.files[i].namespace.clone();
        for imp in imports {
            g.imports_total += 1;
            let target = resolver.resolve(i as u32, &lang, own_ns.as_deref(), &imp);
            let t = match target {
                Target::Internal { files, via } => {
                    g.imports_internal += 1;
                    *g.resolution_methods.entry(via.to_string()).or_default() += 1;
                    let kind = if imp.kind == ImportKind::Mod || via == "child-module" {
                        EdgeKind::Contains
                    } else {
                        EdgeKind::Uses
                    };
                    for &to in &files {
                        let e = edges.entry((i as u32, to, kind)).or_insert((0, imp.line));
                        e.0 += 1;
                        e.1 = e.1.min(imp.line);
                    }
                    ImportTarget::Internal { files, via: via.to_string() }
                }
                Target::External { name, ecosystem } => {
                    g.imports_external += 1;
                    let p = packages.entry((ecosystem.to_string(), norm_pkg(ecosystem, &name))).or_default();
                    p.0.insert(i as u32);
                    p.1 += 1;
                    ImportTarget::External { name, ecosystem: ecosystem.to_string() }
                }
                Target::Builtin { name } => {
                    g.imports_builtin += 1;
                    ImportTarget::Builtin { name }
                }
                Target::Unresolved => {
                    g.imports_unresolved += 1;
                    ImportTarget::Unresolved
                }
                Target::Local => {
                    g.imports_total -= 1;
                    continue;
                }
            };
            if resolved_imports[i].len() < 500 {
                resolved_imports[i].push(FileImport { spec: imp.spec, line: imp.line, kind: imp.kind, target: t });
            }
        }
    }
    for (f, imps) in a.files.iter_mut().zip(resolved_imports) {
        f.imports = imps;
    }

    let mut e: Vec<Edge> =
        edges.into_iter().map(|((from, to, kind), (count, line))| Edge { from, to, kind, count, line }).collect();
    e.sort_by_key(|x| (x.from, x.to));
    g.edges = e;

    // Packages: declared status against the nearest manifest of the same ecosystem.
    for ((eco, name), (users, count)) in packages {
        let mut users: Vec<u32> = users.into_iter().collect();
        users.sort_unstable();
        let declared = find_declared(a, &eco, &name, users.first().map(|u| a.files[*u as usize].path.as_str()));
        g.packages.push(ExternalPackage {
            declared: declared.is_some(),
            declared_kind: declared.as_ref().map(|d| format!("{:?}", d.1.kind).to_lowercase()),
            version: declared.as_ref().and_then(|d| d.1.version.clone()),
            manifest: declared.as_ref().map(|d| d.0.clone()),
            name,
            ecosystem: eco,
            use_count: count,
            users: users.into_iter().take(200).collect(),
        });
    }
    // Declared but never imported (still listed, use_count 0).
    for e in &a.ecosystems {
        let eco_key = match e.id.as_str() {
            "node" | "python" | "rust" | "go" | "dotnet" | "php" | "ruby" => e.id.as_str(),
            _ => continue,
        };
        for d in &e.dependencies {
            if d.source == "path" && eco_key == "dotnet" {
                continue;
            }
            let key = norm_pkg(eco_key, &d.name);
            if !g.packages.iter().any(|p| {
                p.ecosystem == eco_key
                    && (p.name == key
                        || (eco_key == "dotnet" && p.manifest.as_deref() == Some(&e.manifest) && p.name == d.name))
            }) {
                g.packages.push(ExternalPackage {
                    name: key,
                    ecosystem: eco_key.into(),
                    declared: true,
                    declared_kind: Some(format!("{:?}", d.kind).to_lowercase()),
                    version: d.version.clone(),
                    manifest: Some(e.manifest.clone()),
                    use_count: 0,
                    users: vec![],
                });
            }
        }
    }
    g.packages.sort_by(|x, y| y.use_count.cmp(&x.use_count).then(x.name.cmp(&y.name)));

    // Fan-in/out on dependency (Uses) edges.
    let mut fan_in: Vec<HashSet<u32>> = vec![HashSet::new(); a.files.len()];
    let mut fan_out: Vec<HashSet<u32>> = vec![HashSet::new(); a.files.len()];
    for e in g.edges.iter().filter(|e| e.kind == EdgeKind::Uses) {
        fan_out[e.from as usize].insert(e.to);
        fan_in[e.to as usize].insert(e.from);
    }
    let mut ext: Vec<u32> = vec![0; a.files.len()];
    for p in &g.packages {
        for u in &p.users {
            ext[*u as usize] += 1;
        }
    }
    for (i, f) in a.files.iter_mut().enumerate() {
        f.fan_in = fan_in[i].len() as u32;
        f.fan_out = fan_out[i].len() as u32;
        f.external_deps = ext[i];
    }
    g.cycles = crate::graph::find_cycles(a.files.len(), &g.edges);
    a.graph = g;
}

fn find_declared<'a>(
    a: &'a Analysis,
    eco: &str,
    name: &str,
    user: Option<&str>,
) -> Option<(String, &'a ecosystem::DeclaredDep)> {
    let eco_id = eco;
    let candidates: Vec<&ecosystem::Ecosystem> = {
        let mut c: Vec<&ecosystem::Ecosystem> = a.ecosystems.iter().filter(|e| e.id == eco_id).collect();
        // nearest enclosing manifest first, then any
        if let Some(u) = user {
            c.sort_by_key(|e| {
                let encloses = e.root.is_empty() || u.starts_with(&format!("{}/", e.root));
                (std::cmp::Reverse(encloses), std::cmp::Reverse(e.root.len()))
            });
        }
        c
    };
    for e in candidates {
        for d in &e.dependencies {
            let matches = match eco {
                "jvm" => d.name.starts_with(name),
                "go" => d.name == name || name.starts_with(&format!("{}/", d.name)),
                _ => norm_pkg(eco, &d.name) == name,
            };
            if matches {
                return Some((e.manifest.clone(), d));
            }
        }
        // Only the nearest manifest counts for declaration checks in node/python/rust.
        if matches!(eco, "node" | "python" | "rust")
            && user.is_some_and(|u| e.root.is_empty() || u.starts_with(&format!("{}/", e.root)))
        {
            // Monorepo: also accept the root manifest (hoisted deps).
            return a
                .ecosystems
                .iter()
                .filter(|x| x.id == eco && x.root.is_empty())
                .flat_map(|x| x.dependencies.iter().map(move |d| (x, d)))
                .find(|(_, d)| norm_pkg(eco, &d.name) == name)
                .map(|(x, d)| (x.manifest.clone(), d));
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod tests;

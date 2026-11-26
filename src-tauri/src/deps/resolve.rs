//! Import resolution: specifier -> project file(s) / external package / builtin / unresolved.
//!
//! Every successful resolution records *how* it was resolved (`via`), so evidence is traceable
//! and heuristics are distinguishable from exact path resolution.

use crate::project::ecosystem::Ecosystem;
use crate::providers::{ImportKind, ImportRef};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Internal {
        files: Vec<u32>,
        via: &'static str,
    },
    External {
        name: String,
        ecosystem: &'static str,
    },
    Builtin {
        name: String,
    },
    /// Refers to an item inside the same file/scope (e.g. `use Enum::*`); not a dependency.
    Local,
    Unresolved,
}

pub struct Resolver<'a> {
    files: &'a [String],
    index: HashMap<&'a str, u32>,
    dir_files: HashMap<String, Vec<u32>>,
    ecosystems: &'a [Ecosystem],
    ts_configs: Vec<TsPaths>,
    js_alias_dirs: Vec<(String, String, String)>, // (package root, alias, target dir)
    py_modules: HashMap<String, u32>,
    jvm_fqn: HashMap<String, u32>,
    jvm_pkg: HashMap<String, Vec<u32>>,
    cs_ns: HashMap<String, Vec<u32>>,
    php_fqn: HashMap<String, u32>,
    include_roots: Vec<String>,
    basename_index: HashMap<String, Vec<u32>>,
}

#[derive(Debug, Clone)]
pub struct TsPaths {
    pub dir: String,
    /// Directory `paths` are relative to (baseUrl, or the config's directory).
    pub base_url: String,
    pub has_base_url: bool,
    pub paths: Vec<(String, Vec<String>)>,
}

/// Facts the resolver needs beyond paths (from the content pass).
pub struct FileNs<'a> {
    pub lang: Option<&'a str>,
    pub namespace: Option<&'a str>,
}

fn parent(p: &str) -> &str {
    p.rfind('/').map(|i| &p[..i]).unwrap_or("")
}
fn file_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}
fn stem(p: &str) -> &str {
    let n = file_name(p);
    n.find('.').map(|i| &n[..i]).filter(|s| !s.is_empty()).unwrap_or(n)
}
fn join(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        dir.to_string()
    } else {
        format!("{dir}/{rel}")
    }
}

/// Normalise `a/./b/../c` (returns None when escaping the project root).
pub fn normalize(path: &str) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            s => out.push(s),
        }
    }
    Some(out.join("/"))
}

const JS_EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "vue", "svelte", "json"];
const CSS_EXTS: &[&str] = &["css", "scss", "sass", "less"];

pub const NODE_BUILTINS: &[&str] = &[
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
    "test",
];

pub const PY_STDLIB: &[&str] = &[
    "__future__",
    "abc",
    "argparse",
    "array",
    "ast",
    "asyncio",
    "base64",
    "bisect",
    "builtins",
    "bz2",
    "calendar",
    "cmath",
    "collections",
    "concurrent",
    "configparser",
    "contextlib",
    "contextvars",
    "copy",
    "csv",
    "ctypes",
    "dataclasses",
    "datetime",
    "decimal",
    "difflib",
    "dis",
    "email",
    "enum",
    "errno",
    "fnmatch",
    "fractions",
    "ftplib",
    "functools",
    "gc",
    "getpass",
    "gettext",
    "glob",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "imaplib",
    "importlib",
    "inspect",
    "io",
    "ipaddress",
    "itertools",
    "json",
    "keyword",
    "linecache",
    "locale",
    "logging",
    "lzma",
    "math",
    "mimetypes",
    "multiprocessing",
    "numbers",
    "operator",
    "os",
    "pathlib",
    "pickle",
    "pkgutil",
    "platform",
    "plistlib",
    "pprint",
    "profile",
    "queue",
    "random",
    "re",
    "reprlib",
    "sched",
    "secrets",
    "select",
    "selectors",
    "shelve",
    "shlex",
    "shutil",
    "signal",
    "site",
    "smtplib",
    "socket",
    "socketserver",
    "sqlite3",
    "ssl",
    "stat",
    "statistics",
    "string",
    "struct",
    "subprocess",
    "sys",
    "sysconfig",
    "tarfile",
    "tempfile",
    "textwrap",
    "threading",
    "time",
    "timeit",
    "tkinter",
    "token",
    "tokenize",
    "tomllib",
    "traceback",
    "types",
    "typing",
    "typing_extensions",
    "unicodedata",
    "unittest",
    "urllib",
    "uuid",
    "venv",
    "warnings",
    "weakref",
    "webbrowser",
    "winreg",
    "wsgiref",
    "xml",
    "xmlrpc",
    "zipfile",
    "zipimport",
    "zlib",
    "zoneinfo",
];

const RUST_BUILTIN: &[&str] = &["std", "core", "alloc", "proc_macro", "test"];

/// Strip `//` and `/* */` comments plus trailing commas so tsconfig/jsconfig parse as JSON.
pub fn parse_jsonc(text: &str) -> Option<serde_json::Value> {
    let m = crate::lang::mask::mask(text, &crate::lang::by_id("json").unwrap().syntax);
    let re = regex::Regex::new(r",(\s*[}\]])").ok()?;
    serde_json::from_str(&re.replace_all(&m.code, "$1")).ok()
}

pub fn ts_paths_from(config_path: &str, text: &str) -> Option<TsPaths> {
    let v = parse_jsonc(text)?;
    let co = v.get("compilerOptions")?;
    let dir = parent(config_path).to_string();
    let base_url = co.get("baseUrl").and_then(|x| x.as_str()).map(|b| normalize(&join(&dir, b)).unwrap_or_default());
    let paths: Vec<(String, Vec<String>)> = co
        .get("paths")
        .and_then(|x| x.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if base_url.is_none() && paths.is_empty() {
        return None;
    }
    Some(TsPaths { has_base_url: base_url.is_some(), base_url: base_url.unwrap_or_else(|| dir.clone()), dir, paths })
}

impl<'a> Resolver<'a> {
    pub fn new(
        files: &'a [String],
        ns: &[FileNs<'_>],
        ecosystems: &'a [Ecosystem],
        ts_configs: Vec<TsPaths>,
        js_alias_dirs: Vec<(String, String, String)>,
    ) -> Self {
        let index: HashMap<&str, u32> = files.iter().enumerate().map(|(i, f)| (f.as_str(), i as u32)).collect();
        let mut dir_files: HashMap<String, Vec<u32>> = HashMap::new();
        let mut basename_index: HashMap<String, Vec<u32>> = HashMap::new();
        for (i, f) in files.iter().enumerate() {
            dir_files.entry(parent(f).to_string()).or_default().push(i as u32);
            basename_index.entry(file_name(f).to_ascii_lowercase()).or_default().push(i as u32);
        }

        // Python module index for plausible source roots.
        let mut py_roots: Vec<String> = vec![String::new(), "src".into(), "lib".into()];
        for e in ecosystems.iter().filter(|e| e.id == "python") {
            py_roots.push(e.root.clone());
            py_roots.push(join(&e.root, "src"));
        }
        py_roots.sort();
        py_roots.dedup();
        let mut py_modules = HashMap::new();
        for (i, f) in files.iter().enumerate() {
            if !(f.ends_with(".py") || f.ends_with(".pyi")) {
                continue;
            }
            for root in &py_roots {
                let rel = if root.is_empty() {
                    Some(f.as_str())
                } else {
                    f.strip_prefix(root.as_str()).and_then(|r| r.strip_prefix('/'))
                };
                if let Some(rel) = rel {
                    let rel = rel.trim_end_matches(".pyi").trim_end_matches(".py");
                    let module = rel.trim_end_matches("/__init__").replace('/', ".");
                    if !module.is_empty() && !module.contains('-') {
                        py_modules.entry(module).or_insert(i as u32);
                    }
                }
            }
        }

        let mut jvm_fqn = HashMap::new();
        let mut jvm_pkg: HashMap<String, Vec<u32>> = HashMap::new();
        let mut cs_ns: HashMap<String, Vec<u32>> = HashMap::new();
        let mut php_fqn = HashMap::new();
        for (i, f) in files.iter().enumerate() {
            let Some(n) = ns.get(i) else { continue };
            match (n.lang, n.namespace) {
                (Some("java" | "kotlin"), Some(pkg)) => {
                    jvm_fqn.insert(format!("{pkg}.{}", stem(f)), i as u32);
                    jvm_pkg.entry(pkg.to_string()).or_default().push(i as u32);
                }
                (Some("csharp"), Some(n)) => cs_ns.entry(n.to_string()).or_default().push(i as u32),
                (Some("php"), Some(n)) => {
                    php_fqn.insert(format!("{n}\\{}", stem(f)), i as u32);
                }
                _ => {}
            }
        }

        let mut include_roots: Vec<String> = vec![String::new()];
        let mut seen = HashSet::new();
        for f in files {
            let mut d = parent(f);
            while !d.is_empty() {
                let last = file_name(d);
                if matches!(last, "include" | "inc" | "src" | "source" | "lib") && seen.insert(d.to_string()) {
                    include_roots.push(d.to_string());
                }
                d = parent(d);
            }
        }
        for e in ecosystems {
            if !include_roots.contains(&e.root) {
                include_roots.push(e.root.clone());
            }
        }

        Self {
            files,
            index,
            dir_files,
            ecosystems,
            ts_configs,
            js_alias_dirs,
            py_modules,
            jvm_fqn,
            jvm_pkg,
            cs_ns,
            php_fqn,
            include_roots,
            basename_index,
        }
    }

    fn file(&self, p: &str) -> Option<u32> {
        self.index.get(p).copied()
    }

    fn try_paths(&self, candidates: impl IntoIterator<Item = String>) -> Option<u32> {
        candidates.into_iter().find_map(|c| normalize(&c).and_then(|n| self.file(&n)))
    }

    /// Nearest enclosing ecosystem of a given kind.
    fn eco_for(&self, file: &str, id: &str) -> Option<&'a Ecosystem> {
        self.ecosystems
            .iter()
            .filter(|e| e.id == id && (e.root.is_empty() || file.starts_with(&format!("{}/", e.root))))
            .max_by_key(|e| e.root.len())
    }

    fn js_candidates(&self, base: &str) -> Vec<String> {
        let mut c = vec![base.to_string()];
        // ESM TypeScript: import './x.js' refers to x.ts
        for (from, tos) in
            [("js", &["ts", "tsx"][..]), ("jsx", &["tsx"][..]), ("mjs", &["mts"][..]), ("cjs", &["cts"][..])]
        {
            if let Some(s) = base.strip_suffix(&format!(".{from}")) {
                for to in tos {
                    c.push(format!("{s}.{to}"));
                }
            }
        }
        for e in JS_EXTS.iter().chain(CSS_EXTS) {
            c.push(format!("{base}.{e}"));
        }
        for e in JS_EXTS {
            c.push(format!("{base}/index.{e}"));
        }
        c
    }

    fn resolve_js(&self, from: &str, spec: &str, kind: ImportKind) -> Target {
        let spec = spec.split(['?', '#']).next().unwrap_or(spec);
        let dir = parent(from);
        if spec.starts_with("./") || spec.starts_with("../") || spec == "." || spec == ".." {
            return match self.try_paths(self.js_candidates(&join(dir, spec))) {
                Some(i) => Target::Internal { files: vec![i], via: "relative" },
                None => Target::Unresolved,
            };
        }
        if let Some(rest) = spec.strip_prefix('/') {
            // HTML/Vite root-relative: relative to the HTML file's directory, then the project root.
            if matches!(kind, ImportKind::Script | ImportKind::Stylesheet) || from.ends_with(".html") {
                if let Some(i) =
                    self.try_paths(self.js_candidates(&join(dir, rest)).into_iter().chain(self.js_candidates(rest)))
                {
                    return Target::Internal { files: vec![i], via: "root-relative" };
                }
            }
            return Target::Unresolved;
        }
        // tsconfig/jsconfig paths (nearest config enclosing the file)
        let mut cfgs: Vec<&TsPaths> =
            self.ts_configs.iter().filter(|c| c.dir.is_empty() || from.starts_with(&format!("{}/", c.dir))).collect();
        cfgs.sort_by_key(|c| std::cmp::Reverse(c.dir.len()));
        for cfg in cfgs {
            for (pat, targets) in &cfg.paths {
                let captured = if let Some(prefix) = pat.strip_suffix('*') {
                    spec.strip_prefix(prefix)
                } else if pat == spec {
                    Some("")
                } else {
                    None
                };
                if let Some(cap) = captured {
                    for t in targets {
                        let t = t.replace('*', cap);
                        if let Some(i) = self.try_paths(self.js_candidates(&join(&cfg.base_url, &t))) {
                            return Target::Internal { files: vec![i], via: "tsconfig-paths" };
                        }
                    }
                }
            }
            if cfg.has_base_url {
                if let Some(i) = self.try_paths(self.js_candidates(&join(&cfg.base_url, spec))) {
                    return Target::Internal { files: vec![i], via: "tsconfig-baseUrl" };
                }
            }
        }
        // Bundler alias evidenced in a build config (e.g. `'@': './src'`).
        for (root, alias, target) in &self.js_alias_dirs {
            if (root.is_empty() || from.starts_with(&format!("{root}/"))) && spec.starts_with(&format!("{alias}/")) {
                let rest = &spec[alias.len() + 1..];
                if let Some(i) = self.try_paths(self.js_candidates(&join(target, rest))) {
                    return Target::Internal { files: vec![i], via: "bundler-alias" };
                }
            }
        }
        if let Some(b) = spec.strip_prefix("node:") {
            return Target::Builtin { name: format!("node:{b}") };
        }
        if spec.starts_with("sass:") {
            return Target::Builtin { name: spec.into() };
        }
        if spec.starts_with("virtual:")
            || spec.starts_with("~icons/")
            || spec.starts_with("$app/")
            || spec.starts_with("$lib")
        {
            return Target::Unresolved;
        }
        let spec = spec.trim_start_matches('~');
        let mut parts = spec.splitn(3, '/');
        let first = parts.next().unwrap_or("");
        let pkg = if first.starts_with('@') {
            match parts.next() {
                Some(second) => format!("{first}/{second}"),
                None => first.to_string(),
            }
        } else {
            first.to_string()
        };
        if NODE_BUILTINS.contains(&pkg.as_str()) {
            return Target::Builtin { name: pkg };
        }
        // Workspace package inside this project?
        if let Some(e) =
            self.ecosystems.iter().find(|e| e.id == "node" && e.package_name.as_deref() == Some(pkg.as_str()))
        {
            let sub = spec[pkg.len()..].trim_start_matches('/');
            let mut cands: Vec<String> = Vec::new();
            if sub.is_empty() {
                cands.extend(e.entry_points.iter().cloned());
                cands.extend(self.js_candidates(&join(&e.root, "src/index")));
                cands.extend(self.js_candidates(&join(&e.root, "index")));
            } else {
                cands.extend(self.js_candidates(&join(&e.root, sub)));
                cands.extend(self.js_candidates(&join(&e.root, &format!("src/{sub}"))));
            }
            if let Some(i) = self.try_paths(cands) {
                return Target::Internal { files: vec![i], via: "workspace-package" };
            }
        }
        if pkg.is_empty() || pkg.contains(' ') {
            return Target::Unresolved;
        }
        Target::External { name: pkg, ecosystem: "node" }
    }

    fn resolve_css(&self, from: &str, spec: &str) -> Target {
        if spec.starts_with("sass:") {
            return Target::Builtin { name: spec.into() };
        }
        let dir = parent(from);
        let base = join(dir, spec);
        let (d, n) = (parent(&base).to_string(), file_name(&base).to_string());
        let mut cands = vec![base.clone()];
        for e in CSS_EXTS {
            cands.push(format!("{base}.{e}"));
            cands.push(format!("{}/_{n}.{e}", d));
            cands.push(format!("{base}/_index.{e}"));
            cands.push(format!("{base}/index.{e}"));
        }
        if let Some(i) = self.try_paths(cands) {
            return Target::Internal { files: vec![i], via: "relative" };
        }
        if spec.starts_with('.') || spec.starts_with('/') {
            return Target::Unresolved;
        }
        self.resolve_js(from, spec, ImportKind::Stylesheet)
    }

    /// Directory holding the module's children for a Rust file.
    fn rust_mod_dir(file: &str) -> String {
        let n = file_name(file);
        if matches!(n, "mod.rs" | "lib.rs" | "main.rs") || parent(file).ends_with("/bin") || file.ends_with("build.rs")
        {
            parent(file).to_string()
        } else {
            join(parent(file), n.trim_end_matches(".rs"))
        }
    }

    fn rust_crate_src(&self, from: &str) -> Option<String> {
        let eco = self.eco_for(from, "rust")?;
        Some(join(&eco.root, "src"))
    }

    fn rust_lookup(&self, base_dir: &str, segs: &[&str]) -> Option<u32> {
        for k in (1..=segs.len()).rev() {
            let p = join(base_dir, &segs[..k].join("/"));
            if let Some(i) = self.try_paths([format!("{p}.rs"), format!("{p}/mod.rs")]) {
                return Some(i);
            }
        }
        None
    }

    fn resolve_rust(&self, from: &str, imp: &ImportRef) -> Target {
        if imp.kind == ImportKind::Mod {
            let dir = Self::rust_mod_dir(from);
            if let Some(p) = imp.names.iter().find_map(|n| n.strip_prefix("path:")) {
                return match self.try_paths([join(parent(from), p)]) {
                    Some(i) => Target::Internal { files: vec![i], via: "mod-path-attribute" },
                    None => Target::Unresolved,
                };
            }
            return match self
                .try_paths([join(&dir, &format!("{}.rs", imp.spec)), join(&dir, &format!("{}/mod.rs", imp.spec))])
            {
                Some(i) => Target::Internal { files: vec![i], via: "mod-declaration" },
                None => Target::Unresolved,
            };
        }
        let mut segs: Vec<&str> = imp.spec.split("::").filter(|s| !s.is_empty() && *s != "*").collect();
        // Inside inline `mod x { }` blocks, the first N `super`s stay within this file.
        let inline_depth: usize =
            imp.names.iter().find_map(|n| n.strip_prefix("inline:")).and_then(|d| d.parse().ok()).unwrap_or(0);
        if inline_depth > 0 {
            let supers = segs.iter().take_while(|s| **s == "super").count();
            if segs.first() == Some(&"self") || (supers > 0 && supers <= inline_depth) {
                return Target::Local; // refers to items of this same file
            }
            if supers > inline_depth {
                segs.drain(..inline_depth);
            }
        }
        let Some(&first) = segs.first() else { return Target::Unresolved };
        if RUST_BUILTIN.contains(&first) {
            return Target::Builtin { name: first.into() };
        }
        // `use Enum::*` / `use Type::Assoc`: a local item, never a crate (crates are lowercase).
        if first.chars().next().is_some_and(|c| c.is_uppercase()) {
            return Target::Local;
        }
        let eco = self.eco_for(from, "rust");
        let own_lib = eco.and_then(|e| e.module_path.as_deref());
        let crate_src = self.rust_crate_src(from);
        let found = match first {
            "crate" => crate_src.as_deref().and_then(|src| {
                self.rust_lookup(src, &segs[1..])
                    .or_else(|| self.try_paths([join(src, "lib.rs"), join(src, "main.rs")]))
            }),
            "self" => self.rust_lookup(&Self::rust_mod_dir(from), &segs[1..]),
            "super" => {
                let mut dir = Self::rust_mod_dir(from);
                let mut rest = &segs[..];
                while rest.first() == Some(&"super") {
                    dir = parent(&dir).to_string();
                    rest = &rest[1..];
                }
                self.rust_lookup(&dir, rest).or_else(|| {
                    // `super::Item` refers to the parent module file itself.
                    self.try_paths([
                        format!("{dir}.rs"),
                        join(&dir, "mod.rs"),
                        join(&dir, "lib.rs"),
                        join(&dir, "main.rs"),
                    ])
                })
            }
            f if Some(f) == own_lib => crate_src
                .as_deref()
                .and_then(|src| self.rust_lookup(src, &segs[1..]).or_else(|| self.try_paths([join(src, "lib.rs")]))),
            _ => None,
        };
        // Path relative to the current module (`use child::X` in mod.rs), or a crate-root module
        // used without `crate::` (2018 edition paths).
        let found = found.or_else(|| {
            if matches!(first, "crate" | "self" | "super") {
                return None;
            }
            let src = crate_src.as_deref()?;
            self.rust_lookup(&Self::rust_mod_dir(from), &segs)
                .filter(|i| self.files[*i as usize] != from)
                .or_else(|| (parent(from) == src).then(|| self.rust_lookup(src, &segs)).flatten())
        });
        if let Some(i) = found {
            let target = &self.files[i as usize];
            if target == from {
                return Target::Local;
            }
            // A parent module referring to its own descendants (`pub use child::X`, `crate::x`
            // from the crate root) is containment, not a dependency that can form harmful cycles.
            let own_dir = Self::rust_mod_dir(from);
            let is_root = matches!(file_name(from), "mod.rs" | "lib.rs" | "main.rs");
            if is_root && target.starts_with(&format!("{own_dir}/")) {
                return Target::Internal { files: vec![i], via: "child-module" };
            }
            return Target::Internal { files: vec![i], via: "module-path" };
        }
        if matches!(first, "crate" | "self" | "super") {
            return Target::Unresolved;
        }
        Target::External { name: first.replace('_', "-"), ecosystem: "rust" }
    }

    fn py_lookup(&self, module: &str) -> Option<u32> {
        let mut m = module;
        loop {
            if let Some(i) = self.py_modules.get(m) {
                return Some(*i);
            }
            m = &m[..m.rfind('.')?];
        }
    }

    fn resolve_python(&self, from: &str, imp: &ImportRef) -> Target {
        let spec = imp.spec.as_str();
        if spec.starts_with('.') {
            let dots = spec.chars().take_while(|c| *c == '.').count();
            let mut dir = parent(from).to_string();
            for _ in 1..dots {
                dir = parent(&dir).to_string();
            }
            let rest = spec[dots..].replace('.', "/");
            let base = join(&dir, &rest);
            let mut targets = Vec::new();
            for n in &imp.names {
                let p = join(&base, n);
                if let Some(i) = self.try_paths([format!("{p}.py"), format!("{p}/__init__.py")]) {
                    targets.push(i);
                }
            }
            if targets.is_empty() {
                if let Some(i) = self.try_paths([format!("{base}.py"), format!("{base}/__init__.py")]) {
                    targets.push(i);
                }
            }
            targets.retain(|i| self.files[*i as usize] != from);
            return if targets.is_empty() {
                Target::Unresolved
            } else {
                Target::Internal { files: targets, via: "relative" }
            };
        }
        let top = spec.split('.').next().unwrap_or(spec);
        // `from pkg import submodule` prefers the submodule file.
        let mut targets: Vec<u32> =
            imp.names.iter().filter_map(|n| self.py_modules.get(&format!("{spec}.{n}")).copied()).collect();
        if targets.is_empty() {
            if let Some(i) = self.py_lookup(spec) {
                // Only accept a prefix match if the top-level package itself is local.
                if self.py_modules.contains_key(top)
                    || self.py_modules.keys().any(|k| k.starts_with(&format!("{top}.")))
                {
                    targets.push(i);
                }
            }
        }
        targets.retain(|i| self.files[*i as usize] != from);
        if !targets.is_empty() {
            return Target::Internal { files: targets, via: "module-path" };
        }
        if PY_STDLIB.contains(&top) {
            return Target::Builtin { name: top.into() };
        }
        Target::External { name: top.to_ascii_lowercase().replace('_', "-"), ecosystem: "python" }
    }

    fn resolve_go(&self, from: &str, spec: &str) -> Target {
        if let Some(e) = self.eco_for(from, "go").or_else(|| self.ecosystems.iter().find(|e| e.id == "go")) {
            if let Some(mp) = e.module_path.as_deref() {
                if spec == mp || spec.starts_with(&format!("{mp}/")) {
                    let rel = spec[mp.len()..].trim_start_matches('/');
                    let dir = join(&e.root, rel);
                    let files: Vec<u32> = self
                        .dir_files
                        .get(&dir)
                        .into_iter()
                        .flatten()
                        .copied()
                        .filter(|i| {
                            let f = &self.files[*i as usize];
                            f.ends_with(".go") && !f.ends_with("_test.go") && f != from
                        })
                        .take(50)
                        .collect();
                    return if files.is_empty() {
                        Target::Unresolved
                    } else {
                        Target::Internal { files, via: "go-package" }
                    };
                }
            }
        }
        let first = spec.split('/').next().unwrap_or(spec);
        if !first.contains('.') {
            return Target::Builtin { name: spec.into() };
        }
        // Prefer the declared module path that prefixes the import.
        let declared = self
            .ecosystems
            .iter()
            .filter(|e| e.id == "go")
            .flat_map(|e| e.dependencies.iter())
            .filter(|d| spec == d.name || spec.starts_with(&format!("{}/", d.name)))
            .max_by_key(|d| d.name.len());
        let name =
            declared.map(|d| d.name.clone()).unwrap_or_else(|| spec.split('/').take(3).collect::<Vec<_>>().join("/"));
        Target::External { name, ecosystem: "go" }
    }

    fn resolve_include(&self, from: &str, spec: &str, system: bool) -> Target {
        let spec_n = spec.replace('\\', "/");
        if !system {
            if let Some(i) = self.try_paths([join(parent(from), &spec_n)]) {
                return Target::Internal { files: vec![i], via: "relative" };
            }
        }
        if let Some(i) = self.try_paths(self.include_roots.iter().map(|r| join(r, &spec_n))) {
            return Target::Internal { files: vec![i], via: "include-root" };
        }
        // Unique suffix match (e.g. include path configured in the build system).
        let name = file_name(&spec_n).to_ascii_lowercase();
        if let Some(c) = self.basename_index.get(&name) {
            let m: Vec<u32> = c
                .iter()
                .copied()
                .filter(|i| {
                    self.files[*i as usize].ends_with(&format!("/{spec_n}")) || self.files[*i as usize] == spec_n
                })
                .collect();
            if m.len() == 1 {
                return Target::Internal { files: m, via: "unique-suffix" };
            }
        }
        if system {
            Target::Builtin { name: spec.into() }
        } else {
            Target::Unresolved
        }
    }

    fn resolve_jvm(&self, from: &str, spec: &str) -> Target {
        if let Some(pkg) = spec.strip_suffix(".*") {
            if let Some(files) = self.jvm_pkg.get(pkg) {
                let f: Vec<u32> = files.iter().copied().filter(|i| self.files[*i as usize] != from).take(50).collect();
                if !f.is_empty() {
                    return Target::Internal { files: f, via: "package" };
                }
            }
        } else {
            // a.b.C or a.b.C.method (static import)
            let mut s = spec;
            loop {
                if let Some(i) = self.jvm_fqn.get(s) {
                    if self.files[*i as usize] != from {
                        return Target::Internal { files: vec![*i], via: "fully-qualified-name" };
                    }
                    return Target::Unresolved;
                }
                match s.rfind('.') {
                    Some(p) => s = &s[..p],
                    None => break,
                }
            }
        }
        let first = spec.split('.').next().unwrap_or("");
        if matches!(first, "java" | "javax" | "kotlin" | "kotlinx" | "android" | "jdk" | "sun") {
            return Target::Builtin { name: spec.split('.').take(2).collect::<Vec<_>>().join(".") };
        }
        Target::External { name: spec.split('.').take(2).collect::<Vec<_>>().join("."), ecosystem: "jvm" }
    }

    fn resolve_csharp(&self, from: &str, own_ns: Option<&str>, spec: &str) -> Target {
        if own_ns == Some(spec) {
            return Target::Unresolved;
        }
        if let Some(files) = self.cs_ns.get(spec) {
            let f: Vec<u32> = files.iter().copied().filter(|i| self.files[*i as usize] != from).take(50).collect();
            if !f.is_empty() {
                return Target::Internal { files: f, via: "namespace" };
            }
        }
        let first = spec.split('.').next().unwrap_or("");
        if matches!(first, "System" | "Microsoft" | "Windows") {
            return Target::Builtin { name: spec.split('.').take(2).collect::<Vec<_>>().join(".") };
        }
        // Match a declared package by prefix.
        let declared = self
            .ecosystems
            .iter()
            .filter(|e| e.id == "dotnet")
            .flat_map(|e| e.dependencies.iter())
            .filter(|d| spec == d.name || spec.starts_with(&format!("{}.", d.name)))
            .max_by_key(|d| d.name.len());
        Target::External {
            name: declared.map(|d| d.name.clone()).unwrap_or_else(|| first.to_string()),
            ecosystem: "dotnet",
        }
    }

    fn resolve_php(&self, from: &str, imp: &ImportRef) -> Target {
        let spec = imp.spec.trim_start_matches('\\');
        if imp.kind == ImportKind::Require {
            let p = spec.trim_start_matches("./");
            if let Some(i) = self.try_paths([join(parent(from), p), p.to_string()]) {
                return Target::Internal { files: vec![i], via: "relative" };
            }
            return Target::Unresolved;
        }
        if let Some(i) = self.php_fqn.get(spec) {
            return Target::Internal { files: vec![*i], via: "fully-qualified-name" };
        }
        for e in self.ecosystems.iter().filter(|e| e.id == "php") {
            for (prefix, dir) in &e.namespace_roots {
                if let Some(rest) = spec.strip_prefix(prefix.as_str()) {
                    let p = join(dir, &format!("{}.php", rest.trim_start_matches('\\').replace('\\', "/")));
                    if let Some(i) = self.try_paths([p]) {
                        return Target::Internal { files: vec![i], via: "psr-4" };
                    }
                }
            }
        }
        let vendor = spec.split('\\').take(2).collect::<Vec<_>>().join("\\");
        Target::External { name: vendor, ecosystem: "php" }
    }

    fn resolve_ruby(&self, from: &str, spec: &str) -> Target {
        let s = spec.trim_end_matches(".rb");
        if s.starts_with('.') {
            return match self.try_paths([format!("{}.rb", join(parent(from), s))]) {
                Some(i) => Target::Internal { files: vec![i], via: "relative" },
                None => Target::Unresolved,
            };
        }
        let mut roots = vec!["lib".to_string(), String::new()];
        for e in self.ecosystems.iter().filter(|e| e.id == "ruby") {
            roots.push(join(&e.root, "lib"));
            roots.push(e.root.clone());
        }
        if let Some(i) = self.try_paths(roots.iter().map(|r| format!("{}.rb", join(r, s)))) {
            return Target::Internal { files: vec![i], via: "load-path" };
        }
        Target::External { name: s.split('/').next().unwrap_or(s).to_string(), ecosystem: "ruby" }
    }

    fn resolve_lua(&self, from: &str, spec: &str) -> Target {
        let p = spec.replace('.', "/");
        let roots = [String::new(), parent(from).to_string(), "src".into(), "lua".into()];
        if let Some(i) = self
            .try_paths(roots.iter().flat_map(|r| [format!("{}.lua", join(r, &p)), join(r, &format!("{p}/init.lua"))]))
        {
            if self.files[i as usize] != from {
                return Target::Internal { files: vec![i], via: "module-path" };
            }
        }
        Target::External { name: spec.split('.').next().unwrap_or(spec).to_string(), ecosystem: "lua" }
    }

    fn resolve_relative_file(&self, from: &str, spec: &str, via: &'static str) -> Target {
        let s = spec.replace('\\', "/");
        let s = s
            .trim_start_matches("$PSScriptRoot/")
            .trim_start_matches("${PSScriptRoot}/")
            .trim_start_matches("$(dirname \"$0\")/")
            .trim_start_matches("$(dirname $0)/")
            .trim_start_matches("$SCRIPT_DIR/");
        if s.starts_with('$') || s.starts_with('~') {
            return Target::Unresolved;
        }
        match self.try_paths([join(parent(from), s), s.to_string()]) {
            Some(i) => Target::Internal { files: vec![i], via },
            None => Target::Unresolved,
        }
    }

    pub fn resolve(&self, from_idx: u32, lang: &str, own_ns: Option<&str>, imp: &ImportRef) -> Target {
        let from = self.files[from_idx as usize].as_str();
        let t = match lang {
            "javascript" | "jsx" | "typescript" | "tsx" | "vue" | "svelte" | "html" => {
                self.resolve_js(from, &imp.spec, imp.kind)
            }
            "css" | "scss" | "less" => self.resolve_css(from, &imp.spec),
            "rust" => self.resolve_rust(from, imp),
            "python" => self.resolve_python(from, imp),
            "go" => self.resolve_go(from, &imp.spec),
            "c" | "cpp" => self.resolve_include(from, &imp.spec, imp.kind == ImportKind::SystemInclude),
            "java" | "kotlin" => self.resolve_jvm(from, &imp.spec),
            "csharp" => self.resolve_csharp(from, own_ns, &imp.spec),
            "php" => self.resolve_php(from, imp),
            "ruby" => self.resolve_ruby(from, &imp.spec),
            "lua" => self.resolve_lua(from, &imp.spec),
            "zig" => {
                if imp.spec.ends_with(".zig") || imp.spec.ends_with(".zon") {
                    self.resolve_relative_file(from, &imp.spec, "relative")
                } else if matches!(imp.spec.as_str(), "std" | "builtin" | "root") {
                    Target::Builtin { name: imp.spec.clone() }
                } else {
                    Target::External { name: imp.spec.clone(), ecosystem: "zig" }
                }
            }
            "swift" => {
                Target::External { name: imp.spec.split('.').next().unwrap_or(&imp.spec).into(), ecosystem: "swift" }
            }
            "bash" | "graphql" => self.resolve_relative_file(from, &imp.spec, "relative"),
            "powershell" => {
                if imp.spec.contains('.')
                    && (imp.spec.contains('/')
                        || imp.spec.contains('\\')
                        || imp.spec.ends_with(".ps1")
                        || imp.spec.ends_with(".psm1"))
                {
                    self.resolve_relative_file(from, &imp.spec, "relative")
                } else {
                    Target::External { name: imp.spec.clone(), ecosystem: "powershell" }
                }
            }
            "terraform" => {
                if imp.kind == ImportKind::External {
                    Target::External { name: imp.spec.clone(), ecosystem: "terraform" }
                } else {
                    let dir = normalize(&join(parent(from), &imp.spec)).unwrap_or_default();
                    let files: Vec<u32> = self
                        .dir_files
                        .get(&dir)
                        .into_iter()
                        .flatten()
                        .copied()
                        .filter(|i| self.files[*i as usize].ends_with(".tf"))
                        .collect();
                    if files.is_empty() {
                        Target::Unresolved
                    } else {
                        Target::Internal { files, via: "module-dir" }
                    }
                }
            }
            "dockerfile" => {
                Target::External { name: imp.spec.split(':').next().unwrap_or(&imp.spec).into(), ecosystem: "docker" }
            }
            _ => Target::Unresolved,
        };
        if let Target::Internal { files, via } = t {
            let files: Vec<u32> = files.into_iter().filter(|i| *i != from_idx).collect();
            if files.is_empty() {
                return Target::Unresolved;
            }
            return Target::Internal { files, via };
        }
        t
    }
}

/// Detect simple bundler aliases (`'@': ... 'src'`) in Vite/webpack/etc. configs. Only
/// literal, single-line alias declarations are recognised; anything else stays unresolved.
pub fn alias_dirs_from_config(config_path: &str, text: &str) -> Vec<(String, String, String)> {
    let root = parent(config_path).to_string();
    let mut out = Vec::new();
    let Ok(re) = regex::Regex::new(
        r#"['"]?([@~$#][\w-]*)['"]?\s*:\s*(?:path\.resolve\(\s*__dirname\s*,\s*|fileURLToPath\(\s*new URL\(\s*|resolve\(\s*__dirname\s*,\s*)?['"]\.?/?([\w./-]+?)/?['"]"#,
    ) else {
        return out;
    };
    for c in re.captures_iter(text) {
        let alias = c[1].to_string();
        let target = normalize(&join(&root, &c[2])).unwrap_or_default();
        if !out.iter().any(|(_, a, _)| *a == alias) {
            out.push((root.clone(), alias, target));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ImportRef;

    fn r(spec: &str, kind: ImportKind) -> ImportRef {
        ImportRef { spec: spec.into(), line: 1, kind, names: vec![] }
    }
    fn names(spec: &str, n: &[&str]) -> ImportRef {
        ImportRef {
            spec: spec.into(),
            line: 1,
            kind: ImportKind::Import,
            names: n.iter().map(|s| s.to_string()).collect(),
        }
    }
    fn files(list: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = list.iter().map(|s| s.to_string()).collect();
        v.sort();
        v
    }
    fn idx(f: &[String], p: &str) -> u32 {
        f.iter().position(|x| x == p).unwrap() as u32
    }
    fn internal(t: &Target) -> Vec<u32> {
        match t {
            Target::Internal { files, .. } => files.clone(),
            other => panic!("expected internal, got {other:?}"),
        }
    }

    #[test]
    fn normalize_paths() {
        assert_eq!(normalize("a/./b/../c").as_deref(), Some("a/c"));
        assert_eq!(normalize("../x"), None);
    }

    #[test]
    fn javascript_resolution() {
        let f = files(&[
            "src/app/App.jsx",
            "src/lib/util.ts",
            "src/lib/index.js",
            "src/main.jsx",
            "index.html",
            "src/style.css",
            "src/components/Button.vue",
        ]);
        let ts = vec![ts_paths_from(
            "tsconfig.json",
            r#"{ // comment
            "compilerOptions": { "baseUrl": ".", "paths": { "@lib/*": ["src/lib/*"], }, }
        }"#,
        )
        .unwrap()];
        let alias = alias_dirs_from_config(
            "vite.config.js",
            "resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } }",
        );
        assert_eq!(alias, vec![(String::new(), "@".into(), "src".into())]);
        let ns: Vec<FileNs> = f.iter().map(|_| FileNs { lang: None, namespace: None }).collect();
        let res = Resolver::new(&f, &ns, &[], ts, alias);
        let app = idx(&f, "src/app/App.jsx");
        assert_eq!(
            internal(&res.resolve(app, "jsx", None, &r("../lib/util", ImportKind::Import))),
            vec![idx(&f, "src/lib/util.ts")]
        );
        assert_eq!(
            internal(&res.resolve(app, "jsx", None, &r("../lib/util.js", ImportKind::Import))),
            vec![idx(&f, "src/lib/util.ts")]
        );
        assert_eq!(
            internal(&res.resolve(app, "jsx", None, &r("../lib", ImportKind::Import))),
            vec![idx(&f, "src/lib/index.js")]
        );
        assert_eq!(
            internal(&res.resolve(app, "jsx", None, &r("@lib/util", ImportKind::Import))),
            vec![idx(&f, "src/lib/util.ts")]
        );
        assert_eq!(
            internal(&res.resolve(app, "jsx", None, &r("@/components/Button.vue", ImportKind::Import))),
            vec![idx(&f, "src/components/Button.vue")]
        );
        assert_eq!(
            res.resolve(app, "jsx", None, &r("react-dom/client", ImportKind::Import)),
            Target::External { name: "react-dom".into(), ecosystem: "node" }
        );
        assert_eq!(
            res.resolve(app, "jsx", None, &r("@tauri-apps/api/core", ImportKind::Import)),
            Target::External { name: "@tauri-apps/api".into(), ecosystem: "node" }
        );
        assert_eq!(
            res.resolve(app, "jsx", None, &r("node:fs", ImportKind::Import)),
            Target::Builtin { name: "node:fs".into() }
        );
        assert_eq!(
            res.resolve(app, "jsx", None, &r("path", ImportKind::Import)),
            Target::Builtin { name: "path".into() }
        );
        assert_eq!(res.resolve(app, "jsx", None, &r("./missing", ImportKind::Import)), Target::Unresolved);
        let html = idx(&f, "index.html");
        assert_eq!(
            internal(&res.resolve(html, "html", None, &r("/src/main.jsx", ImportKind::Script))),
            vec![idx(&f, "src/main.jsx")]
        );
    }

    #[test]
    fn rust_resolution() {
        let f = files(&[
            "src-tauri/src/lib.rs",
            "src-tauri/src/main.rs",
            "src-tauri/src/scan/mod.rs",
            "src-tauri/src/scan/explorer.rs",
            "src-tauri/src/core.rs",
            "src-tauri/src/lang/mod.rs",
            "src-tauri/src/lang/mask.rs",
        ]);
        let eco = vec![Ecosystem {
            id: "rust".into(),
            root: "src-tauri".into(),
            module_path: Some("my_lib".into()),
            ..Default::default()
        }];
        let ns: Vec<FileNs> = f.iter().map(|_| FileNs { lang: Some("rust"), namespace: None }).collect();
        let res = Resolver::new(&f, &ns, &eco, vec![], vec![]);
        let lib = idx(&f, "src-tauri/src/lib.rs");
        let scan = idx(&f, "src-tauri/src/scan/mod.rs");
        let explorer = idx(&f, "src-tauri/src/scan/explorer.rs");
        let main = idx(&f, "src-tauri/src/main.rs");
        assert_eq!(internal(&res.resolve(lib, "rust", None, &r("scan", ImportKind::Mod))), vec![scan]);
        assert_eq!(
            internal(&res.resolve(lib, "rust", None, &r("core", ImportKind::Mod))),
            vec![idx(&f, "src-tauri/src/core.rs")]
        );
        assert_eq!(internal(&res.resolve(scan, "rust", None, &r("explorer", ImportKind::Mod))), vec![explorer]);
        assert_eq!(
            internal(&res.resolve(explorer, "rust", None, &r("crate::lang::mask::LineIndex", ImportKind::Use))),
            vec![idx(&f, "src-tauri/src/lang/mask.rs")]
        );
        assert_eq!(internal(&res.resolve(explorer, "rust", None, &r("super::Excluder", ImportKind::Use))), vec![scan]);
        assert_eq!(internal(&res.resolve(explorer, "rust", None, &r("crate::APP_NAME", ImportKind::Use))), vec![lib]);
        assert_eq!(internal(&res.resolve(main, "rust", None, &r("my_lib::scan::scan", ImportKind::Use))), vec![scan]);
        assert_eq!(
            res.resolve(explorer, "rust", None, &r("std::path::Path", ImportKind::Use)),
            Target::Builtin { name: "std".into() }
        );
        assert_eq!(
            res.resolve(explorer, "rust", None, &r("serde_json::Value", ImportKind::Use)),
            Target::External { name: "serde-json".into(), ecosystem: "rust" }
        );
        let mut with_path = r("platform", ImportKind::Mod);
        with_path.names = vec!["path:lang/mask.rs".into()];
        assert_eq!(internal(&res.resolve(lib, "rust", None, &with_path)), vec![idx(&f, "src-tauri/src/lang/mask.rs")]);
    }

    #[test]
    fn python_resolution() {
        let f = files(&[
            "app/__init__.py",
            "app/models.py",
            "app/views.py",
            "app/core/__init__.py",
            "app/core/base.py",
            "app/api/routes.py",
            "manage.py",
        ]);
        let ns: Vec<FileNs> = f.iter().map(|_| FileNs { lang: Some("python"), namespace: None }).collect();
        let res = Resolver::new(&f, &ns, &[], vec![], vec![]);
        let routes = idx(&f, "app/api/routes.py");
        let views = idx(&f, "app/views.py");
        assert_eq!(
            internal(&res.resolve(views, "python", None, &names(".", &["models"]))),
            vec![idx(&f, "app/models.py")]
        );
        assert_eq!(
            internal(&res.resolve(routes, "python", None, &names("..core.base", &["Base"]))),
            vec![idx(&f, "app/core/base.py")]
        );
        assert_eq!(
            internal(&res.resolve(routes, "python", None, &names("app.core", &["base"]))),
            vec![idx(&f, "app/core/base.py")]
        );
        assert_eq!(
            internal(&res.resolve(routes, "python", None, &names("app.models", &[]))),
            vec![idx(&f, "app/models.py")]
        );
        assert_eq!(res.resolve(routes, "python", None, &names("os.path", &[])), Target::Builtin { name: "os".into() });
        assert_eq!(
            res.resolve(routes, "python", None, &names("Flask_Cors", &[])),
            Target::External { name: "flask-cors".into(), ecosystem: "python" }
        );
    }

    #[test]
    fn go_c_jvm_csharp_php_resolution() {
        let f = files(&[
            "go.mod",
            "cmd/app/main.go",
            "internal/store/store.go",
            "internal/store/store_test.go",
            "include/util/math.h",
            "src/engine.cpp",
            "src/engine.h",
            "java/App.java",
            "java/Engine.java",
            "cs/Core.cs",
            "cs/Web.cs",
            "php/app/Models/User.php",
            "php/app/Http/Kernel.php",
        ]);
        let mut ns: Vec<FileNs> = f.iter().map(|_| FileNs { lang: None, namespace: None }).collect();
        ns[idx(&f, "java/Engine.java") as usize] = FileNs { lang: Some("java"), namespace: Some("com.acme.core") };
        ns[idx(&f, "java/App.java") as usize] = FileNs { lang: Some("java"), namespace: Some("com.acme") };
        ns[idx(&f, "cs/Core.cs") as usize] = FileNs { lang: Some("csharp"), namespace: Some("Acme.Core") };
        ns[idx(&f, "cs/Web.cs") as usize] = FileNs { lang: Some("csharp"), namespace: Some("Acme.Web") };
        let eco = vec![
            Ecosystem {
                id: "go".into(),
                root: String::new(),
                module_path: Some("example.com/app".into()),
                ..Default::default()
            },
            Ecosystem {
                id: "php".into(),
                root: "php".into(),
                namespace_roots: vec![("App".into(), "php/app".into())],
                ..Default::default()
            },
        ];
        let res = Resolver::new(&f, &ns, &eco, vec![], vec![]);
        let main = idx(&f, "cmd/app/main.go");
        assert_eq!(
            internal(&res.resolve(main, "go", None, &r("example.com/app/internal/store", ImportKind::Import))),
            vec![idx(&f, "internal/store/store.go")]
        );
        assert_eq!(
            res.resolve(main, "go", None, &r("net/http", ImportKind::Import)),
            Target::Builtin { name: "net/http".into() }
        );
        assert_eq!(
            res.resolve(main, "go", None, &r("github.com/a/b/c/d", ImportKind::Import)),
            Target::External { name: "github.com/a/b".into(), ecosystem: "go" }
        );

        let cpp = idx(&f, "src/engine.cpp");
        assert_eq!(
            internal(&res.resolve(cpp, "cpp", None, &r("engine.h", ImportKind::Include))),
            vec![idx(&f, "src/engine.h")]
        );
        assert_eq!(
            internal(&res.resolve(cpp, "cpp", None, &r("util/math.h", ImportKind::Include))),
            vec![idx(&f, "include/util/math.h")]
        );
        assert_eq!(
            res.resolve(cpp, "cpp", None, &r("vector", ImportKind::SystemInclude)),
            Target::Builtin { name: "vector".into() }
        );

        let app = idx(&f, "java/App.java");
        assert_eq!(
            internal(&res.resolve(app, "java", None, &r("com.acme.core.Engine", ImportKind::Import))),
            vec![idx(&f, "java/Engine.java")]
        );
        assert_eq!(
            internal(&res.resolve(app, "java", None, &r("com.acme.core.*", ImportKind::Import))),
            vec![idx(&f, "java/Engine.java")]
        );
        assert!(matches!(
            res.resolve(app, "java", None, &r("java.util.List", ImportKind::Import)),
            Target::Builtin { .. }
        ));

        let web = idx(&f, "cs/Web.cs");
        assert_eq!(
            internal(&res.resolve(web, "csharp", Some("Acme.Web"), &r("Acme.Core", ImportKind::Use))),
            vec![idx(&f, "cs/Core.cs")]
        );
        assert!(matches!(
            res.resolve(web, "csharp", Some("Acme.Web"), &r("System.Linq", ImportKind::Use)),
            Target::Builtin { .. }
        ));

        let kernel = idx(&f, "php/app/Http/Kernel.php");
        assert_eq!(
            internal(&res.resolve(kernel, "php", None, &r("App\\Models\\User", ImportKind::Use))),
            vec![idx(&f, "php/app/Models/User.php")]
        );
    }
}

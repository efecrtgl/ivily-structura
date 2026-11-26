//! Ecosystem, framework and declared-dependency detection from manifests.
//!
//! Only manifest/marker files are read (never sensitive files). Detection is evidence-based:
//! every framework carries the evidence that triggered it.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum DepKind {
    Runtime,
    Dev,
    Build,
    Peer,
    Optional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredDep {
    pub name: String,
    pub version: Option<String>,
    pub kind: DepKind,
    /// `registry`, `path`, `git`, `workspace`, `url`
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Framework {
    pub id: String,
    pub name: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ecosystem {
    pub id: String,
    pub name: String,
    /// Manifest path (project-relative).
    pub manifest: String,
    /// Directory containing the manifest ("" = project root).
    pub root: String,
    pub package_name: Option<String>,
    pub version: Option<String>,
    pub package_manager: Option<String>,
    pub frameworks: Vec<Framework>,
    pub dependencies: Vec<DeclaredDep>,
    pub entry_points: Vec<String>,
    pub workspace_members: Vec<String>,
    /// Go module path, Rust crate name (for import resolution).
    pub module_path: Option<String>,
    /// PSR-4 style namespace roots: (prefix, dir) — PHP composer autoload.
    pub namespace_roots: Vec<(String, String)>,
}

fn parent(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[..i]).unwrap_or("")
}

fn join(dir: &str, rel: &str) -> String {
    let rel = rel.trim_start_matches("./").replace('\\', "/");
    if dir.is_empty() {
        rel
    } else {
        format!("{dir}/{rel}")
    }
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// Known frameworks keyed by dependency name, per ecosystem.
const NODE_FRAMEWORKS: &[(&str, &str, &str)] = &[
    ("react", "react", "React"),
    ("preact", "preact", "Preact"),
    ("vue", "vue", "Vue"),
    ("svelte", "svelte", "Svelte"),
    ("@sveltejs/kit", "sveltekit", "SvelteKit"),
    ("next", "nextjs", "Next.js"),
    ("nuxt", "nuxt", "Nuxt"),
    ("@angular/core", "angular", "Angular"),
    ("solid-js", "solid", "SolidJS"),
    ("astro", "astro", "Astro"),
    ("@remix-run/react", "remix", "Remix"),
    ("gatsby", "gatsby", "Gatsby"),
    ("express", "express", "Express"),
    ("fastify", "fastify", "Fastify"),
    ("koa", "koa", "Koa"),
    ("@nestjs/core", "nestjs", "NestJS"),
    ("hono", "hono", "Hono"),
    ("electron", "electron", "Electron"),
    ("@tauri-apps/api", "tauri", "Tauri"),
    ("react-native", "react-native", "React Native"),
    ("expo", "expo", "Expo"),
    ("vite", "vite", "Vite"),
    ("webpack", "webpack", "webpack"),
    ("typescript", "typescript", "TypeScript"),
    ("tailwindcss", "tailwind", "Tailwind CSS"),
    ("vitest", "vitest", "Vitest"),
    ("jest", "jest", "Jest"),
    ("mocha", "mocha", "Mocha"),
    ("@playwright/test", "playwright", "Playwright"),
    ("cypress", "cypress", "Cypress"),
    ("prisma", "prisma", "Prisma"),
    ("three", "threejs", "three.js"),
];
const RUST_FRAMEWORKS: &[(&str, &str, &str)] = &[
    ("tauri", "tauri", "Tauri"),
    ("axum", "axum", "Axum"),
    ("actix-web", "actix", "Actix Web"),
    ("rocket", "rocket", "Rocket"),
    ("warp", "warp", "warp"),
    ("tokio", "tokio", "Tokio"),
    ("bevy", "bevy", "Bevy"),
    ("leptos", "leptos", "Leptos"),
    ("yew", "yew", "Yew"),
    ("dioxus", "dioxus", "Dioxus"),
    ("iced", "iced", "iced"),
    ("eframe", "egui", "egui"),
    ("clap", "clap", "clap (CLI)"),
    ("diesel", "diesel", "Diesel"),
    ("sqlx", "sqlx", "SQLx"),
];
const PY_FRAMEWORKS: &[(&str, &str, &str)] = &[
    ("django", "django", "Django"),
    ("flask", "flask", "Flask"),
    ("fastapi", "fastapi", "FastAPI"),
    ("starlette", "starlette", "Starlette"),
    ("streamlit", "streamlit", "Streamlit"),
    ("celery", "celery", "Celery"),
    ("pytest", "pytest", "pytest"),
    ("torch", "pytorch", "PyTorch"),
    ("tensorflow", "tensorflow", "TensorFlow"),
    ("pandas", "pandas", "pandas"),
    ("sqlalchemy", "sqlalchemy", "SQLAlchemy"),
    ("pydantic", "pydantic", "Pydantic"),
];
const GO_FRAMEWORKS: &[(&str, &str, &str)] = &[
    ("github.com/gin-gonic/gin", "gin", "Gin"),
    ("github.com/labstack/echo", "echo", "Echo"),
    ("github.com/gofiber/fiber", "fiber", "Fiber"),
    ("github.com/go-chi/chi", "chi", "chi"),
    ("github.com/spf13/cobra", "cobra", "Cobra (CLI)"),
    ("google.golang.org/grpc", "grpc", "gRPC"),
    ("gorm.io/gorm", "gorm", "GORM"),
];
const PHP_FRAMEWORKS: &[(&str, &str, &str)] = &[
    ("laravel/framework", "laravel", "Laravel"),
    ("symfony/framework-bundle", "symfony", "Symfony"),
    ("slim/slim", "slim", "Slim"),
    ("phpunit/phpunit", "phpunit", "PHPUnit"),
];
const RUBY_FRAMEWORKS: &[(&str, &str, &str)] =
    &[("rails", "rails", "Ruby on Rails"), ("sinatra", "sinatra", "Sinatra"), ("rspec", "rspec", "RSpec")];

fn frameworks_from(deps: &[DeclaredDep], table: &[(&str, &str, &str)], manifest: &str) -> Vec<Framework> {
    let mut out = Vec::new();
    for (dep, id, name) in table {
        if let Some(d) = deps.iter().find(|d| d.name == *dep || d.name.starts_with(&format!("{dep}/"))) {
            out.push(Framework {
                id: id.to_string(),
                name: name.to_string(),
                evidence: format!("dependency `{}` in {manifest}", d.name),
            });
        }
    }
    out
}

fn dep(name: &str, version: Option<String>, kind: DepKind, source: &str) -> DeclaredDep {
    DeclaredDep { name: name.to_string(), version, kind, source: source.to_string() }
}

// --------------------------------------------------------------------------------------------
// Parsers

fn parse_package_json(manifest: &str, text: &str, files: &HashSet<&str>) -> Option<Ecosystem> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let dir = parent(manifest).to_string();
    let mut e = Ecosystem {
        id: "node".into(),
        name: "Node.js / JavaScript".into(),
        manifest: manifest.into(),
        root: dir.clone(),
        package_name: v.get("name").and_then(|x| x.as_str()).map(String::from),
        version: v.get("version").and_then(|x| x.as_str()).map(String::from),
        ..Default::default()
    };
    for (key, kind) in [
        ("dependencies", DepKind::Runtime),
        ("devDependencies", DepKind::Dev),
        ("peerDependencies", DepKind::Peer),
        ("optionalDependencies", DepKind::Optional),
    ] {
        if let Some(m) = v.get(key).and_then(|x| x.as_object()) {
            for (name, ver) in m {
                let ver = ver.as_str().unwrap_or("").to_string();
                let source = if ver.starts_with("file:") || ver.starts_with("link:") {
                    "path"
                } else if ver.starts_with("workspace:") {
                    "workspace"
                } else if ver.starts_with("git") || ver.contains("github:") || ver.contains(".git") {
                    "git"
                } else if ver.starts_with("http") {
                    "url"
                } else {
                    "registry"
                };
                e.dependencies.push(dep(name, Some(ver), kind, source));
            }
        }
    }
    let mut entries = Vec::new();
    for key in ["main", "module", "browser"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            entries.push(s.to_string());
        }
    }
    match v.get("bin") {
        Some(serde_json::Value::String(s)) => entries.push(s.clone()),
        Some(serde_json::Value::Object(m)) => entries.extend(m.values().filter_map(|x| x.as_str().map(String::from))),
        _ => {}
    }
    fn collect_exports(v: &serde_json::Value, out: &mut Vec<String>, depth: u8) {
        if depth > 4 {
            return;
        }
        match v {
            serde_json::Value::String(s) if s.starts_with("./") => out.push(s.clone()),
            serde_json::Value::Object(m) => m.values().for_each(|x| collect_exports(x, out, depth + 1)),
            _ => {}
        }
    }
    if let Some(x) = v.get("exports") {
        collect_exports(x, &mut entries, 0);
    }
    for en in entries {
        let p = join(&dir, &en);
        if files.contains(p.as_str()) && !e.entry_points.contains(&p) {
            e.entry_points.push(p);
        }
    }
    match v.get("workspaces") {
        Some(serde_json::Value::Array(a)) => {
            e.workspace_members = a.iter().filter_map(|x| x.as_str().map(String::from)).collect()
        }
        Some(serde_json::Value::Object(m)) => {
            if let Some(serde_json::Value::Array(a)) = m.get("packages") {
                e.workspace_members = a.iter().filter_map(|x| x.as_str().map(String::from)).collect();
            }
        }
        _ => {}
    }
    if let Some(pm) = v.get("packageManager").and_then(|x| x.as_str()) {
        e.package_manager = Some(pm.split('@').next().unwrap_or(pm).to_string());
    }
    e.frameworks = frameworks_from(&e.dependencies, NODE_FRAMEWORKS, manifest);
    Some(e)
}

fn toml_dep_source(v: &toml::Value) -> (&'static str, Option<String>) {
    match v {
        toml::Value::String(s) => ("registry", Some(s.clone())),
        toml::Value::Table(t) => {
            let ver = t.get("version").and_then(|x| x.as_str()).map(String::from);
            if t.contains_key("path") {
                ("path", ver)
            } else if t.contains_key("git") {
                ("git", ver)
            } else if t.get("workspace").and_then(|x| x.as_bool()) == Some(true) {
                ("workspace", ver)
            } else {
                ("registry", ver)
            }
        }
        _ => ("registry", None),
    }
}

fn parse_cargo_toml(manifest: &str, text: &str, files: &HashSet<&str>) -> Option<Ecosystem> {
    let t: toml::Table = text.parse().ok()?;
    let dir = parent(manifest).to_string();
    let pkg = t.get("package").and_then(|x| x.as_table());
    let mut e = Ecosystem {
        id: "rust".into(),
        name: "Rust / Cargo".into(),
        manifest: manifest.into(),
        root: dir.clone(),
        package_name: pkg.and_then(|p| p.get("name")).and_then(|x| x.as_str()).map(String::from),
        version: pkg.and_then(|p| p.get("version")).and_then(|x| x.as_str()).map(String::from),
        package_manager: Some("cargo".into()),
        ..Default::default()
    };
    e.module_path = e.package_name.as_ref().map(|n| n.replace('-', "_"));
    if let Some(lib_name) = t.get("lib").and_then(|l| l.get("name")).and_then(|x| x.as_str()) {
        e.module_path = Some(lib_name.to_string());
    }
    for (key, kind) in
        [("dependencies", DepKind::Runtime), ("dev-dependencies", DepKind::Dev), ("build-dependencies", DepKind::Build)]
    {
        if let Some(m) = t.get(key).and_then(|x| x.as_table()) {
            for (name, v) in m {
                let (source, ver) = toml_dep_source(v);
                e.dependencies.push(dep(name, ver, kind, source));
            }
        }
    }
    // target-specific deps
    if let Some(targets) = t.get("target").and_then(|x| x.as_table()) {
        for (_, tt) in targets {
            for (key, kind) in [("dependencies", DepKind::Runtime), ("dev-dependencies", DepKind::Dev)] {
                if let Some(m) = tt.get(key).and_then(|x| x.as_table()) {
                    for (name, v) in m {
                        let (source, ver) = toml_dep_source(v);
                        if !e.dependencies.iter().any(|d| d.name == *name) {
                            e.dependencies.push(dep(name, ver, kind, source));
                        }
                    }
                }
            }
        }
    }
    if let Some(ws) = t.get("workspace").and_then(|x| x.as_table()) {
        if let Some(m) = ws.get("members").and_then(|x| x.as_array()) {
            e.workspace_members = m.iter().filter_map(|x| x.as_str().map(String::from)).collect();
        }
        if pkg.is_none() {
            e.name = "Rust / Cargo workspace".into();
        }
    }
    let mut entries: Vec<String> = Vec::new();
    for p in ["src/main.rs", "src/lib.rs", "build.rs"] {
        entries.push(join(&dir, p));
    }
    if let Some(p) = t.get("lib").and_then(|l| l.get("path")).and_then(|x| x.as_str()) {
        entries.push(join(&dir, p));
    }
    if let Some(bins) = t.get("bin").and_then(|x| x.as_array()) {
        for b in bins {
            if let Some(p) = b.get("path").and_then(|x| x.as_str()) {
                entries.push(join(&dir, p));
            }
        }
    }
    let bin_prefix = join(&dir, "src/bin/");
    for f in files {
        if f.starts_with(&bin_prefix) && f.ends_with(".rs") && !f[bin_prefix.len()..].contains('/') {
            entries.push(f.to_string());
        }
        if f.starts_with(&bin_prefix) && f.ends_with("/main.rs") {
            entries.push(f.to_string());
        }
    }
    for en in entries {
        if files.contains(en.as_str()) && !e.entry_points.contains(&en) {
            e.entry_points.push(en);
        }
    }
    e.frameworks = frameworks_from(&e.dependencies, RUST_FRAMEWORKS, manifest);
    Some(e)
}

fn parse_go_mod(manifest: &str, text: &str, files: &HashSet<&str>) -> Option<Ecosystem> {
    let dir = parent(manifest).to_string();
    let mut e = Ecosystem {
        id: "go".into(),
        name: "Go modules".into(),
        manifest: manifest.into(),
        root: dir.clone(),
        package_manager: Some("go".into()),
        ..Default::default()
    };
    let mut in_require = false;
    for line in text.lines() {
        let l = line.split("//").next().unwrap_or("").trim();
        if let Some(m) = l.strip_prefix("module ") {
            e.module_path = Some(m.trim().trim_matches('"').to_string());
            e.package_name = e.module_path.clone();
        } else if l.starts_with("require (") {
            in_require = true;
        } else if in_require && l == ")" {
            in_require = false;
        } else if let Some(rest) = l.strip_prefix("require ").or(if in_require { Some(l) } else { None }) {
            let mut parts = rest.split_whitespace();
            if let (Some(name), ver) = (parts.next(), parts.next()) {
                let indirect = line.contains("// indirect");
                e.dependencies.push(dep(
                    name,
                    ver.map(String::from),
                    if indirect { DepKind::Optional } else { DepKind::Runtime },
                    "registry",
                ));
            }
        } else if let Some(v) = l.strip_prefix("go ") {
            e.version = Some(format!("go {}", v.trim()));
        }
    }
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    for f in files {
        if let Some(rest) = f.strip_prefix(&prefix) {
            if rest == "main.go" || (rest.starts_with("cmd/") && rest.ends_with("/main.go")) {
                e.entry_points.push(f.to_string());
            }
        }
    }
    e.entry_points.sort();
    e.frameworks = frameworks_from(&e.dependencies, GO_FRAMEWORKS, manifest);
    Some(e)
}

fn pep508_name(s: &str) -> Option<(String, Option<String>)> {
    let s = s.trim();
    if s.is_empty() || s.starts_with('#') || s.starts_with('-') {
        return None;
    }
    let end = s.find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '.')).unwrap_or(s.len());
    let name = s[..end].to_ascii_lowercase().replace('_', "-");
    if name.is_empty() {
        return None;
    }
    let rest = s[end..].trim();
    let rest = rest.split(';').next().unwrap_or("").trim();
    let rest = if rest.starts_with('[') { rest.split_once(']').map(|x| x.1).unwrap_or("").trim() } else { rest };
    Some((name, if rest.is_empty() { None } else { Some(rest.to_string()) }))
}

fn python_eco(manifest: &str, files: &HashSet<&str>) -> Ecosystem {
    let dir = parent(manifest).to_string();
    let mut e = Ecosystem {
        id: "python".into(),
        name: "Python".into(),
        manifest: manifest.into(),
        root: dir.clone(),
        package_manager: Some("pip".into()),
        ..Default::default()
    };
    for p in ["manage.py", "main.py", "app.py", "__main__.py", "wsgi.py", "asgi.py", "src/main.py", "run.py"] {
        let f = join(&dir, p);
        if files.contains(f.as_str()) {
            e.entry_points.push(f);
        }
    }
    e
}

fn parse_pyproject(manifest: &str, text: &str, files: &HashSet<&str>) -> Option<Ecosystem> {
    let t: toml::Table = text.parse().ok()?;
    let mut e = python_eco(manifest, files);
    if let Some(p) = t.get("project").and_then(|x| x.as_table()) {
        e.package_name = p.get("name").and_then(|x| x.as_str()).map(String::from);
        e.version = p.get("version").and_then(|x| x.as_str()).map(String::from);
        if let Some(a) = p.get("dependencies").and_then(|x| x.as_array()) {
            for s in a.iter().filter_map(|x| x.as_str()) {
                if let Some((n, v)) = pep508_name(s) {
                    e.dependencies.push(dep(&n, v, DepKind::Runtime, "registry"));
                }
            }
        }
        if let Some(opt) = p.get("optional-dependencies").and_then(|x| x.as_table()) {
            for (_, a) in opt {
                for s in a.as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
                    if let Some((n, v)) = pep508_name(s) {
                        e.dependencies.push(dep(&n, v, DepKind::Optional, "registry"));
                    }
                }
            }
        }
    }
    if let Some(poetry) = t.get("tool").and_then(|x| x.get("poetry")).and_then(|x| x.as_table()) {
        e.package_manager = Some("poetry".into());
        if e.package_name.is_none() {
            e.package_name = poetry.get("name").and_then(|x| x.as_str()).map(String::from);
            e.version = poetry.get("version").and_then(|x| x.as_str()).map(String::from);
        }
        let mut add = |m: Option<&toml::Table>, kind| {
            for (n, v) in m.into_iter().flatten() {
                if n != "python" {
                    let (source, ver) = toml_dep_source(v);
                    e.dependencies.push(dep(&n.to_ascii_lowercase(), ver, kind, source));
                }
            }
        };
        add(poetry.get("dependencies").and_then(|x| x.as_table()), DepKind::Runtime);
        add(poetry.get("dev-dependencies").and_then(|x| x.as_table()), DepKind::Dev);
        if let Some(groups) = poetry.get("group").and_then(|x| x.as_table()) {
            for (_, g) in groups {
                add(g.get("dependencies").and_then(|x| x.as_table()), DepKind::Dev);
            }
        }
    }
    if t.get("tool").and_then(|x| x.get("uv")).is_some() {
        e.package_manager = Some("uv".into());
    }
    e.frameworks = frameworks_from(&e.dependencies, PY_FRAMEWORKS, manifest);
    Some(e)
}

fn parse_requirements(manifest: &str, text: &str, files: &HashSet<&str>) -> Option<Ecosystem> {
    let mut e = python_eco(manifest, files);
    let kind = if file_name(manifest).contains("dev") || file_name(manifest).contains("test") {
        DepKind::Dev
    } else {
        DepKind::Runtime
    };
    for line in text.lines() {
        if let Some((n, v)) = pep508_name(line) {
            e.dependencies.push(dep(&n, v, kind, "registry"));
        }
    }
    e.frameworks = frameworks_from(&e.dependencies, PY_FRAMEWORKS, manifest);
    Some(e)
}

fn parse_composer(manifest: &str, text: &str, files: &HashSet<&str>) -> Option<Ecosystem> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let dir = parent(manifest).to_string();
    let mut e = Ecosystem {
        id: "php".into(),
        name: "PHP / Composer".into(),
        manifest: manifest.into(),
        root: dir.clone(),
        package_name: v.get("name").and_then(|x| x.as_str()).map(String::from),
        version: v.get("version").and_then(|x| x.as_str()).map(String::from),
        package_manager: Some("composer".into()),
        ..Default::default()
    };
    for (key, kind) in [("require", DepKind::Runtime), ("require-dev", DepKind::Dev)] {
        if let Some(m) = v.get(key).and_then(|x| x.as_object()) {
            for (n, ver) in m {
                if n == "php" || n.starts_with("ext-") {
                    continue;
                }
                e.dependencies.push(dep(n, ver.as_str().map(String::from), kind, "registry"));
            }
        }
    }
    for key in ["autoload", "autoload-dev"] {
        if let Some(m) = v.get(key).and_then(|x| x.get("psr-4")).and_then(|x| x.as_object()) {
            for (prefix, d) in m {
                let dirs: Vec<String> = match d {
                    serde_json::Value::String(s) => vec![s.clone()],
                    serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                    _ => vec![],
                };
                for d in dirs {
                    e.namespace_roots
                        .push((prefix.trim_end_matches('\\').to_string(), join(&dir, d.trim_end_matches('/'))));
                }
            }
        }
    }
    for p in ["public/index.php", "index.php", "artisan", "bin/console"] {
        let f = join(&dir, p);
        if files.contains(f.as_str()) {
            e.entry_points.push(f);
        }
    }
    e.frameworks = frameworks_from(&e.dependencies, PHP_FRAMEWORKS, manifest);
    Some(e)
}

fn parse_gemfile(manifest: &str, text: &str, files: &HashSet<&str>) -> Option<Ecosystem> {
    let dir = parent(manifest).to_string();
    let mut e = Ecosystem {
        id: "ruby".into(),
        name: "Ruby / Bundler".into(),
        manifest: manifest.into(),
        root: dir.clone(),
        package_manager: Some("bundler".into()),
        ..Default::default()
    };
    let re = regex::Regex::new(r#"(?m)^\s*gem\s+["']([^"']+)["'](?:\s*,\s*["']([^"']+)["'])?"#).ok()?;
    let mut group_dev = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("group") && (l.contains(":development") || l.contains(":test")) {
            group_dev = true;
        } else if l == "end" {
            group_dev = false;
        }
        if let Some(c) = re.captures(line) {
            e.dependencies.push(dep(
                &c[1],
                c.get(2).map(|m| m.as_str().to_string()),
                if group_dev { DepKind::Dev } else { DepKind::Runtime },
                "registry",
            ));
        }
    }
    for p in ["config.ru", "bin/rails", "app.rb", "main.rb"] {
        let f = join(&dir, p);
        if files.contains(f.as_str()) {
            e.entry_points.push(f);
        }
    }
    e.frameworks = frameworks_from(&e.dependencies, RUBY_FRAMEWORKS, manifest);
    Some(e)
}

fn parse_maven(manifest: &str, text: &str) -> Option<Ecosystem> {
    let dir = parent(manifest).to_string();
    let mut e = Ecosystem {
        id: "jvm".into(),
        name: "Java / Maven".into(),
        manifest: manifest.into(),
        root: dir,
        package_manager: Some("maven".into()),
        ..Default::default()
    };
    let block = regex::Regex::new(r"(?s)<dependency>(.*?)</dependency>").ok()?;
    let tag = |s: &str, t: &str| {
        regex::Regex::new(&format!(r"<{t}>\s*([^<]+?)\s*</{t}>")).ok()?.captures(s).map(|c| c[1].to_string())
    };
    for c in block.captures_iter(text) {
        let b = &c[1];
        if let (Some(g), Some(a)) = (tag(b, "groupId"), tag(b, "artifactId")) {
            let kind = if tag(b, "scope").as_deref() == Some("test") { DepKind::Dev } else { DepKind::Runtime };
            e.dependencies.push(dep(&format!("{g}:{a}"), tag(b, "version"), kind, "registry"));
        }
    }
    let stripped = block.replace_all(text, "");
    e.package_name = tag(&stripped, "artifactId");
    e.version = tag(&stripped, "version");
    if e.dependencies.iter().any(|d| d.name.starts_with("org.springframework.boot:")) || text.contains("spring-boot") {
        e.frameworks.push(Framework {
            id: "spring-boot".into(),
            name: "Spring Boot".into(),
            evidence: format!("spring-boot in {manifest}"),
        });
    }
    Some(e)
}

fn parse_gradle(manifest: &str, text: &str) -> Option<Ecosystem> {
    let dir = parent(manifest).to_string();
    let kts = manifest.ends_with(".kts");
    let mut e = Ecosystem {
        id: "jvm".into(),
        name: if kts { "Kotlin / Gradle".into() } else { "JVM / Gradle".into() },
        manifest: manifest.into(),
        root: dir,
        package_manager: Some("gradle".into()),
        ..Default::default()
    };
    let re = regex::Regex::new(r#"(?m)^\s*(implementation|api|compileOnly|runtimeOnly|testImplementation|androidTestImplementation|kapt|ksp|annotationProcessor|classpath)\s*\(?\s*["']([^:"']+):([^:"']+)(?::([^"']+))?["']"#).ok()?;
    for c in re.captures_iter(text) {
        let kind = match &c[1] {
            "testImplementation" | "androidTestImplementation" => DepKind::Dev,
            "kapt" | "ksp" | "annotationProcessor" | "classpath" | "compileOnly" => DepKind::Build,
            _ => DepKind::Runtime,
        };
        e.dependencies.push(dep(
            &format!("{}:{}", &c[2], &c[3]),
            c.get(4).map(|m| m.as_str().to_string()),
            kind,
            "registry",
        ));
    }
    if text.contains("com.android.application") || text.contains("com.android.library") {
        e.frameworks.push(Framework {
            id: "android".into(),
            name: "Android".into(),
            evidence: format!("Android Gradle plugin in {manifest}"),
        });
    }
    if text.contains("org.springframework.boot") {
        e.frameworks.push(Framework {
            id: "spring-boot".into(),
            name: "Spring Boot".into(),
            evidence: format!("Spring Boot plugin in {manifest}"),
        });
    }
    if text.contains("io.ktor") {
        e.frameworks.push(Framework {
            id: "ktor".into(),
            name: "Ktor".into(),
            evidence: format!("io.ktor in {manifest}"),
        });
    }
    Some(e)
}

fn parse_csproj(manifest: &str, text: &str, files: &HashSet<&str>) -> Option<Ecosystem> {
    let dir = parent(manifest).to_string();
    let name = file_name(manifest).rsplit_once('.').map(|x| x.0).unwrap_or(manifest).to_string();
    let mut e = Ecosystem {
        id: "dotnet".into(),
        name: ".NET".into(),
        manifest: manifest.into(),
        root: dir.clone(),
        package_name: Some(name),
        package_manager: Some("nuget".into()),
        ..Default::default()
    };
    let pr = regex::Regex::new(r#"<PackageReference\s+Include="([^"]+)"(?:\s+Version="([^"]+)")?"#).ok()?;
    for c in pr.captures_iter(text) {
        e.dependencies.push(dep(&c[1], c.get(2).map(|m| m.as_str().to_string()), DepKind::Runtime, "registry"));
    }
    let projref = regex::Regex::new(r#"<ProjectReference\s+Include="([^"]+)""#).ok()?;
    for c in projref.captures_iter(text) {
        let p = c[1].replace('\\', "/");
        e.dependencies.push(dep(&p, None, DepKind::Runtime, "path"));
    }
    if let Some(v) = regex::Regex::new(r"<Version>([^<]+)</Version>").ok()?.captures(text) {
        e.version = Some(v[1].to_string());
    }
    let fw = |id: &str, name: &str, ev: &str| Framework {
        id: id.into(),
        name: name.into(),
        evidence: format!("{ev} in {manifest}"),
    };
    if text.contains("Microsoft.NET.Sdk.Web") {
        e.frameworks.push(fw("aspnetcore", "ASP.NET Core", "Microsoft.NET.Sdk.Web"));
    }
    if text.contains("<UseWPF>true") {
        e.frameworks.push(fw("wpf", "WPF", "<UseWPF>"));
    }
    if text.contains("<UseWindowsForms>true") {
        e.frameworks.push(fw("winforms", "Windows Forms", "<UseWindowsForms>"));
    }
    if text.contains("<UseMaui>true") {
        e.frameworks.push(fw("maui", ".NET MAUI", "<UseMaui>"));
    }
    if text.contains("Microsoft.NET.Sdk.BlazorWebAssembly") {
        e.frameworks.push(fw("blazor", "Blazor WebAssembly", "Microsoft.NET.Sdk.BlazorWebAssembly"));
    }
    for d in &e.dependencies {
        if d.name.starts_with("xunit") || d.name.starts_with("NUnit") || d.name.starts_with("MSTest") {
            e.frameworks.push(fw("dotnet-test", "xUnit/NUnit/MSTest", &d.name));
            break;
        }
    }
    for p in ["Program.cs", "App.xaml.cs", "Startup.cs"] {
        let f = join(&dir, p);
        if files.contains(f.as_str()) {
            e.entry_points.push(f);
        }
    }
    Some(e)
}

fn simple(id: &str, name: &str, manifest: &str, pm: Option<&str>) -> Ecosystem {
    Ecosystem {
        id: id.into(),
        name: name.into(),
        manifest: manifest.into(),
        root: parent(manifest).into(),
        package_manager: pm.map(String::from),
        ..Default::default()
    }
}

fn parse_cmake(manifest: &str, text: &str) -> Ecosystem {
    let mut e = simple("cmake", "C/C++ / CMake", manifest, Some("cmake"));
    if let Some(c) = regex::Regex::new(r"(?i)project\s*\(\s*([\w.-]+)").ok().and_then(|r| r.captures(text)) {
        e.package_name = Some(c[1].to_string());
    }
    if let Ok(r) = regex::Regex::new(r"(?i)find_package\s*\(\s*([\w.-]+)") {
        for c in r.captures_iter(text) {
            e.dependencies.push(dep(&c[1], None, DepKind::Runtime, "system"));
        }
    }
    e
}

fn parse_pubspec(manifest: &str, text: &str) -> Ecosystem {
    let mut e = simple("dart", "Dart / Flutter", manifest, Some("pub"));
    let mut section: Option<DepKind> = None;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') {
            section = match line.trim_end() {
                "dependencies:" => Some(DepKind::Runtime),
                "dev_dependencies:" => Some(DepKind::Dev),
                _ => None,
            };
            if let Some(n) = line.strip_prefix("name:") {
                e.package_name = Some(n.trim().to_string());
            }
            if let Some(v) = line.strip_prefix("version:") {
                e.version = Some(v.trim().to_string());
            }
            continue;
        }
        if let Some(kind) = section {
            let indent = line.len() - line.trim_start().len();
            if indent == 2 {
                if let Some((n, v)) = line.trim().split_once(':') {
                    let v = v.trim();
                    e.dependencies.push(dep(n.trim(), (!v.is_empty()).then(|| v.to_string()), kind, "registry"));
                }
            }
        }
    }
    if e.dependencies.iter().any(|d| d.name == "flutter") {
        e.frameworks.push(Framework {
            id: "flutter".into(),
            name: "Flutter".into(),
            evidence: format!("flutter in {manifest}"),
        });
    }
    e
}

/// Marker files that reveal frameworks/tools independent of dependency lists.
const MARKERS: &[(&str, &str, &str)] = &[
    ("tauri.conf.json", "tauri", "Tauri"),
    ("angular.json", "angular", "Angular"),
    ("next.config.js", "nextjs", "Next.js"),
    ("next.config.mjs", "nextjs", "Next.js"),
    ("next.config.ts", "nextjs", "Next.js"),
    ("nuxt.config.ts", "nuxt", "Nuxt"),
    ("svelte.config.js", "svelte", "Svelte"),
    ("astro.config.mjs", "astro", "Astro"),
    ("vite.config.js", "vite", "Vite"),
    ("vite.config.ts", "vite", "Vite"),
    ("vite.config.mjs", "vite", "Vite"),
    ("manage.py", "django", "Django"),
    ("artisan", "laravel", "Laravel"),
    ("androidmanifest.xml", "android", "Android"),
    ("electron-builder.yml", "electron", "Electron"),
    ("app.json", "expo", "Expo"),
];

/// Detect all ecosystems. `read` returns a manifest's text (bounded, never sensitive files).
pub fn detect(files: &[String], read: &dyn Fn(&str) -> Option<String>) -> Vec<Ecosystem> {
    let set: HashSet<&str> = files.iter().map(|s| s.as_str()).collect();
    let mut out: Vec<Ecosystem> = Vec::new();
    let mut docker_dirs: BTreeMap<String, Ecosystem> = BTreeMap::new();
    let mut tf_dirs: BTreeMap<String, Ecosystem> = BTreeMap::new();
    for f in files {
        let name = file_name(f).to_ascii_lowercase();
        let ext = crate::lang::extension(&name).unwrap_or_default();
        let parsed = match name.as_str() {
            "package.json" => read(f).and_then(|t| parse_package_json(f, &t, &set)),
            "cargo.toml" => read(f).and_then(|t| parse_cargo_toml(f, &t, &set)),
            "go.mod" => read(f).and_then(|t| parse_go_mod(f, &t, &set)),
            "pyproject.toml" => read(f).and_then(|t| parse_pyproject(f, &t, &set)),
            "setup.py" | "setup.cfg" | "pipfile" => {
                let dir = parent(f);
                // Prefer pyproject.toml when both exist.
                if set.contains(join(dir, "pyproject.toml").as_str()) {
                    None
                } else {
                    let mut e = python_eco(f, &set);
                    if name == "pipfile" {
                        e.package_manager = Some("pipenv".into());
                        if let Some(t) = read(f).and_then(|t| t.parse::<toml::Table>().ok()) {
                            for (key, kind) in [("packages", DepKind::Runtime), ("dev-packages", DepKind::Dev)] {
                                for (n, v) in t.get(key).and_then(|x| x.as_table()).into_iter().flatten() {
                                    let (source, ver) = toml_dep_source(v);
                                    e.dependencies.push(dep(&n.to_ascii_lowercase(), ver, kind, source));
                                }
                            }
                        }
                    }
                    e.frameworks = frameworks_from(&e.dependencies, PY_FRAMEWORKS, f);
                    Some(e)
                }
            }
            n if n.starts_with("requirements") && ext == "txt" => {
                let dir = parent(f);
                if let Some(existing) = out.iter_mut().find(|e| e.id == "python" && e.root == dir) {
                    if let Some(extra) = read(f).and_then(|t| parse_requirements(f, &t, &set)) {
                        for d in extra.dependencies {
                            if !existing.dependencies.iter().any(|x| x.name == d.name) {
                                existing.dependencies.push(d);
                            }
                        }
                        for fw in extra.frameworks {
                            if !existing.frameworks.iter().any(|x| x.id == fw.id) {
                                existing.frameworks.push(fw);
                            }
                        }
                    }
                    None
                } else {
                    read(f).and_then(|t| parse_requirements(f, &t, &set))
                }
            }
            "composer.json" => read(f).and_then(|t| parse_composer(f, &t, &set)),
            "gemfile" => read(f).and_then(|t| parse_gemfile(f, &t, &set)),
            "pom.xml" => read(f).and_then(|t| parse_maven(f, &t)),
            "build.gradle" | "build.gradle.kts" => read(f).and_then(|t| parse_gradle(f, &t)),
            "package.swift" => Some(simple("swift", "Swift Package Manager", f, Some("swiftpm"))),
            "pubspec.yaml" => read(f).map(|t| parse_pubspec(f, &t)),
            "build.zig" => Some(simple("zig", "Zig", f, Some("zig"))),
            "cmakelists.txt" => {
                // Only the top-most CMakeLists of a tree is an ecosystem root.
                let dir = parent(f);
                if out
                    .iter()
                    .any(|e| e.id == "cmake" && (e.root.is_empty() || dir.starts_with(&format!("{}/", e.root))))
                {
                    None
                } else {
                    read(f).map(|t| parse_cmake(f, &t))
                }
            }
            "makefile" | "gnumakefile" if parent(f).is_empty() => Some(simple("make", "Make", f, Some("make"))),
            "deno.json" | "deno.jsonc" => Some(simple("deno", "Deno", f, Some("deno"))),
            "mix.exs" => Some(simple("elixir", "Elixir / Mix", f, Some("mix"))),
            _ => {
                if matches!(ext.as_str(), "csproj" | "fsproj" | "vbproj") {
                    read(f).and_then(|t| parse_csproj(f, &t, &set))
                } else if name.starts_with("dockerfile")
                    || name.starts_with("docker-compose")
                    || name.starts_with("compose.y")
                {
                    let dir = parent(f).to_string();
                    let e = docker_dirs.entry(dir).or_insert_with(|| simple("docker", "Docker", f, None));
                    if name.starts_with("dockerfile") {
                        if let Some(t) = read(f) {
                            let facts = crate::providers::analyze("dockerfile", &t);
                            for i in facts.imports {
                                if !e.dependencies.iter().any(|d| d.name == i.spec) {
                                    let (n, v) = i
                                        .spec
                                        .split_once(':')
                                        .map(|(a, b)| (a.to_string(), Some(b.to_string())))
                                        .unwrap_or((i.spec.clone(), None));
                                    e.dependencies.push(dep(&n, v, DepKind::Runtime, "image"));
                                }
                            }
                        }
                    }
                    None
                } else if ext == "tf" {
                    let dir = parent(f).to_string();
                    tf_dirs.entry(dir).or_insert_with(|| simple("terraform", "Terraform", f, Some("terraform")));
                    None
                } else {
                    None
                }
            }
        };
        if let Some(e) = parsed {
            out.push(e);
        }
    }
    out.extend(docker_dirs.into_values());
    out.extend(tf_dirs.into_values());

    // Lockfile-based package manager detection.
    for e in out.iter_mut() {
        if e.id != "node" || e.package_manager.is_some() {
            continue;
        }
        for (lock, pm) in [
            ("pnpm-lock.yaml", "pnpm"),
            ("yarn.lock", "yarn"),
            ("bun.lockb", "bun"),
            ("bun.lock", "bun"),
            ("package-lock.json", "npm"),
        ] {
            if set.contains(join(&e.root, lock).as_str()) {
                e.package_manager = Some(pm.into());
                break;
            }
        }
    }
    for e in out.iter_mut().filter(|e| e.id == "python") {
        for (lock, pm) in [("poetry.lock", "poetry"), ("uv.lock", "uv"), ("pipfile.lock", "pipenv")] {
            if files.iter().any(|f| parent(f) == e.root && file_name(f).eq_ignore_ascii_case(lock)) {
                e.package_manager = Some(pm.into());
            }
        }
    }

    // Marker files attach frameworks to the nearest enclosing ecosystem.
    for f in files {
        let name = file_name(f).to_ascii_lowercase();
        if let Some((_, id, fname)) = MARKERS.iter().find(|(m, _, _)| *m == name) {
            let dir = parent(f);
            let best = out
                .iter_mut()
                .filter(|e| {
                    e.root.is_empty()
                        || dir == e.root
                        || dir.starts_with(&format!("{}/", e.root))
                        || e.root.starts_with(&format!("{dir}/"))
                })
                .filter(|e| !matches!(e.id.as_str(), "docker" | "terraform" | "make"))
                .max_by_key(|e| if dir.starts_with(&e.root) { e.root.len() as i64 } else { -(e.root.len() as i64) });
            if let Some(e) = best {
                if !e.frameworks.iter().any(|x| x.id == *id) {
                    e.frameworks.push(Framework {
                        id: id.to_string(),
                        name: fname.to_string(),
                        evidence: format!("marker file {f}"),
                    });
                }
            }
        }
    }
    for e in out.iter_mut() {
        e.dependencies.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
        e.frameworks.sort_by(|a, b| a.name.cmp(&b.name));
    }
    out.sort_by(|a, b| a.root.cmp(&b.root).then(a.id.cmp(&b.id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn run(files: &[(&str, &str)]) -> Vec<Ecosystem> {
        let map: HashMap<String, String> = files.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let list: Vec<String> = files.iter().map(|(a, _)| a.to_string()).collect();
        detect(&list, &|p| map.get(p).cloned())
    }

    #[test]
    fn node_project_with_frameworks_entries_and_pm() {
        let e = run(&[
            (
                "package.json",
                r#"{"name":"web","version":"1.2.0","main":"./src/index.js","bin":{"cli":"bin/cli.js"},"dependencies":{"react":"^19","local":"file:../x"},"devDependencies":{"vite":"^8","vitest":"^5"},"workspaces":["packages/*"]}"#,
            ),
            ("pnpm-lock.yaml", ""),
            ("src/index.js", ""),
            ("bin/cli.js", ""),
            ("vite.config.js", ""),
        ]);
        assert_eq!(e.len(), 1);
        let n = &e[0];
        assert_eq!(n.id, "node");
        assert_eq!(n.package_name.as_deref(), Some("web"));
        assert_eq!(n.package_manager.as_deref(), Some("pnpm"));
        assert_eq!(n.entry_points, vec!["src/index.js", "bin/cli.js"]);
        assert_eq!(n.workspace_members, vec!["packages/*"]);
        let ids: Vec<&str> = n.frameworks.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"react") && ids.contains(&"vite") && ids.contains(&"vitest"));
        assert_eq!(n.dependencies.iter().find(|d| d.name == "local").unwrap().source, "path");
        assert_eq!(n.dependencies.iter().find(|d| d.name == "vite").unwrap().kind, DepKind::Dev);
    }

    #[test]
    fn rust_crate_and_tauri_marker() {
        let e = run(&[
            ("package.json", r#"{"name":"app","dependencies":{"@tauri-apps/api":"2"}}"#),
            ("src-tauri/Cargo.toml", "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n[lib]\nname = \"my_app_lib\"\n[dependencies]\ntauri = { version = \"2\" }\nserde = \"1\"\nlocal = { path = \"../local\" }\n[dev-dependencies]\ntempfile = \"3\"\n"),
            ("src-tauri/src/main.rs", ""),
            ("src-tauri/src/lib.rs", ""),
            ("src-tauri/src/bin/tool.rs", ""),
            ("src-tauri/tauri.conf.json", "{}"),
        ]);
        let r = e.iter().find(|x| x.id == "rust").unwrap();
        assert_eq!(r.root, "src-tauri");
        assert_eq!(r.module_path.as_deref(), Some("my_app_lib"));
        assert!(r.entry_points.contains(&"src-tauri/src/main.rs".to_string()));
        assert!(r.entry_points.contains(&"src-tauri/src/bin/tool.rs".to_string()));
        assert_eq!(r.dependencies.iter().find(|d| d.name == "local").unwrap().source, "path");
        assert!(r.frameworks.iter().any(|f| f.id == "tauri" && f.evidence.contains("dependency")));
        let n = e.iter().find(|x| x.id == "node").unwrap();
        assert!(n.frameworks.iter().any(|f| f.id == "tauri"));
    }

    #[test]
    fn go_python_php_dotnet_jvm() {
        let e = run(&[
            ("go.mod", "module example.com/app\n\ngo 1.22\n\nrequire (\n\tgithub.com/gin-gonic/gin v1.9.0\n\tgolang.org/x/sys v0.1.0 // indirect\n)\n"),
            ("cmd/server/main.go", ""),
            ("py/pyproject.toml", "[project]\nname = \"svc\"\ndependencies = [\"fastapi>=0.1\", \"Pydantic[email]==2.0 ; python_version>'3.8'\"]\n"),
            ("py/requirements-dev.txt", "pytest==8\n# comment\n-r base.txt\n"),
            ("py/main.py", ""),
            ("php/composer.json", r#"{"require":{"php":">=8","laravel/framework":"^11"},"autoload":{"psr-4":{"App\\":"app/"}}}"#),
            ("net/Api.csproj", r#"<Project Sdk="Microsoft.NET.Sdk.Web"><ItemGroup><PackageReference Include="Serilog" Version="3.0" /><ProjectReference Include="..\Core\Core.csproj" /></ItemGroup></Project>"#),
            ("net/Program.cs", ""),
            ("java/pom.xml", "<project><artifactId>demo</artifactId><version>1.0</version><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId></dependency><dependency><groupId>junit</groupId><artifactId>junit</artifactId><scope>test</scope></dependency></dependencies></project>"),
        ]);
        let go = e.iter().find(|x| x.id == "go").unwrap();
        assert_eq!(go.module_path.as_deref(), Some("example.com/app"));
        assert_eq!(go.entry_points, vec!["cmd/server/main.go"]);
        assert!(go.frameworks.iter().any(|f| f.id == "gin"));
        assert_eq!(go.dependencies.iter().find(|d| d.name == "golang.org/x/sys").unwrap().kind, DepKind::Optional);

        let py: Vec<_> = e.iter().filter(|x| x.id == "python").collect();
        assert_eq!(py.len(), 1, "requirements merge into pyproject ecosystem");
        let py = py[0];
        assert!(py.dependencies.iter().any(|d| d.name == "pydantic"));
        assert!(py.dependencies.iter().any(|d| d.name == "pytest" && d.kind == DepKind::Dev));
        assert!(py.frameworks.iter().any(|f| f.id == "fastapi"));
        assert_eq!(py.entry_points, vec!["py/main.py"]);

        let php = e.iter().find(|x| x.id == "php").unwrap();
        assert_eq!(php.namespace_roots, vec![("App".to_string(), "php/app".to_string())]);
        assert!(!php.dependencies.iter().any(|d| d.name == "php"));
        assert!(php.frameworks.iter().any(|f| f.id == "laravel"));

        let net = e.iter().find(|x| x.id == "dotnet").unwrap();
        assert!(net.frameworks.iter().any(|f| f.id == "aspnetcore"));
        assert!(net.dependencies.iter().any(|d| d.name == "../Core/Core.csproj" && d.source == "path"));
        assert_eq!(net.entry_points, vec!["net/Program.cs"]);

        let jvm = e.iter().find(|x| x.id == "jvm").unwrap();
        assert_eq!(jvm.package_name.as_deref(), Some("demo"));
        assert!(jvm.dependencies.iter().any(|d| d.name == "junit:junit" && d.kind == DepKind::Dev));
        assert!(jvm.frameworks.iter().any(|f| f.id == "spring-boot"));
    }

    #[test]
    fn malformed_manifests_are_skipped() {
        let e = run(&[("package.json", "{ not json"), ("Cargo.toml", "[[[")]);
        assert!(e.is_empty());
    }

    #[test]
    fn docker_images_become_dependencies() {
        let e = run(&[("Dockerfile", "FROM rust:1.80 AS build\nFROM debian:bookworm-slim\n")]);
        let d = e.iter().find(|x| x.id == "docker").unwrap();
        assert_eq!(d.dependencies.len(), 2);
        assert_eq!(d.dependencies[0].name, "debian");
    }
}

//! File role classification (source, test, config, docs, ...) and sensitive-file policy.
//!
//! Classification is path/name based and therefore language-agnostic; it never reads content.
//! Generated-file detection additionally uses a header marker found during the content pass.

use crate::lang::{self, Category, Language};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Source,
    Test,
    Config,
    Documentation,
    Build,
    Manifest,
    Lockfile,
    Script,
    Asset,
    Generated,
    Data,
    Other,
}

impl Role {
    pub const ALL: [Role; 12] = [
        Role::Source,
        Role::Test,
        Role::Config,
        Role::Documentation,
        Role::Build,
        Role::Manifest,
        Role::Lockfile,
        Role::Script,
        Role::Asset,
        Role::Generated,
        Role::Data,
        Role::Other,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Source => "source",
            Role::Test => "test",
            Role::Config => "config",
            Role::Documentation => "documentation",
            Role::Build => "build",
            Role::Manifest => "manifest",
            Role::Lockfile => "lockfile",
            Role::Script => "script",
            Role::Asset => "asset",
            Role::Generated => "generated",
            Role::Data => "data",
            Role::Other => "other",
        }
    }
}

pub const MANIFESTS: &[&str] = &[
    "package.json",
    "cargo.toml",
    "go.mod",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "pipfile",
    "composer.json",
    "gemfile",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "package.swift",
    "pubspec.yaml",
    "deno.json",
    "deno.jsonc",
    "mix.exs",
    "build.zig.zon",
    "vcpkg.json",
    "conanfile.txt",
    "conanfile.py",
];

pub const LOCKFILES: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    "bun.lock",
    "cargo.lock",
    "go.sum",
    "poetry.lock",
    "pipfile.lock",
    "uv.lock",
    "composer.lock",
    "gemfile.lock",
    "pubspec.lock",
    "packages.lock.json",
    "deno.lock",
    "mix.lock",
];

const BUILD_FILES: &[&str] = &[
    "makefile",
    "gnumakefile",
    "cmakelists.txt",
    "build.rs",
    "build.zig",
    "meson.build",
    "dockerfile",
    "containerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
    "jenkinsfile",
    "justfile",
    "taskfile.yml",
    "rakefile",
    "gruntfile.js",
    "gulpfile.js",
    "build.sh",
    "build.ps1",
    "gradlew",
    "gradlew.bat",
    "mvnw",
    "mvnw.cmd",
    "procfile",
    "vercel.json",
    "netlify.toml",
    "tauri.conf.json",
    "app.json",
    "electron-builder.yml",
];

const DOC_NAMES: &[&str] = &[
    "readme",
    "changelog",
    "changes",
    "history",
    "contributing",
    "code_of_conduct",
    "security",
    "license",
    "licence",
    "authors",
    "notice",
    "roadmap",
    "support",
    "governance",
    "maintainers",
    "copying",
];

const TEST_DIRS: &[&str] = &[
    "test",
    "tests",
    "__tests__",
    "spec",
    "specs",
    "e2e",
    "testing",
    "testdata",
    "test_data",
    "fixtures",
    "__mocks__",
    "cypress",
    "playwright",
];
const DOC_DIRS: &[&str] = &["docs", "doc", "documentation", "wiki", "manual", "guides"];
const ASSET_DIRS: &[&str] =
    &["assets", "static", "public", "images", "img", "media", "fonts", "icons", "resources", "res"];
const SCRIPT_DIRS: &[&str] = &["scripts", "script", "tools", "bin", "ci", "hack"];
const GENERATED_DIRS: &[&str] = &["generated", "__generated__", "gen", "autogen", ".generated"];

fn stem_lower(name: &str) -> String {
    let l = name.to_ascii_lowercase();
    match l.find('.') {
        Some(0) | None => l,
        Some(i) => l[..i].to_string(),
    }
}

pub fn is_test_file(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.contains(".test.")
        || l.contains(".spec.")
        || l.contains(".e2e.")
        || l.ends_with("_test.go")
        || l.ends_with("_test.py")
        || l.ends_with("_test.rs")
        || l.ends_with("_spec.rb")
        || l.ends_with("_test.rb")
        || l.ends_with("_test.dart")
        || (l.starts_with("test_") && l.ends_with(".py"))
        || l == "conftest.py"
        || l.ends_with("test.java")
        || l.ends_with("tests.java")
        || l.ends_with("test.kt")
        || l.ends_with("tests.cs")
        || l.ends_with("test.cs")
        || l.ends_with("tests.swift")
        || l.ends_with("test.php")
}

pub fn is_generated_name(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".min.js")
        || l.ends_with(".min.css")
        || l.ends_with(".map")
        || l.ends_with(".g.cs")
        || l.ends_with(".designer.cs")
        || l.ends_with(".g.dart")
        || l.ends_with(".freezed.dart")
        || l.ends_with("_pb2.py")
        || l.ends_with("_pb2_grpc.py")
        || l.ends_with(".pb.go")
        || l.ends_with(".pb.cc")
        || l.ends_with(".pb.h")
        || l.ends_with(".generated.ts")
        || l.ends_with(".gen.go")
        || l == "bundle.js"
}

/// Header markers that conventionally flag generated code.
pub fn has_generated_marker(head: &str) -> bool {
    let h = head.to_ascii_lowercase();
    h.contains("@generated")
        || h.contains("auto-generated")
        || h.contains("autogenerated")
        || h.contains("do not edit")
        || h.contains("code generated by")
        || h.contains("this file was generated")
}

/// Classify a file by its project-relative path (`a/b/c.ext`).
pub fn classify(rel: &str, language: Option<&Language>) -> Role {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let lower = name.to_ascii_lowercase();
    let dirs: Vec<String> = rel.split('/').rev().skip(1).map(|d| d.to_ascii_lowercase()).collect();
    let in_dir = |set: &[&str]| dirs.iter().any(|d| set.contains(&d.as_str()));
    let ext = lang::extension(&lower).unwrap_or_default();

    if LOCKFILES.contains(&lower.as_str()) {
        return Role::Lockfile;
    }
    if MANIFESTS.contains(&lower.as_str())
        || matches!(ext.as_str(), "csproj" | "fsproj" | "vbproj" | "gemspec" | "cabal" | "nimble")
    {
        return Role::Manifest;
    }
    if lower.starts_with("requirements") && ext == "txt" {
        return Role::Manifest;
    }
    if is_generated_name(&lower) || in_dir(GENERATED_DIRS) {
        return Role::Generated;
    }
    if dirs.iter().any(|d| d == ".github" || d == ".gitlab" || d == ".circleci" || d == ".azure-pipelines")
        && matches!(ext.as_str(), "yml" | "yaml")
    {
        return Role::Build;
    }
    if BUILD_FILES.contains(&lower.as_str())
        || lower.starts_with("dockerfile")
        || lower.starts_with("docker-compose")
        || matches!(ext.as_str(), "sln" | "mk" | "cmake" | "gradle" | "bazel" | "bzl")
        || lower == "build" && dirs.is_empty()
        || lower.starts_with("vite.config.")
        || lower.starts_with("webpack.")
        || lower.starts_with("rollup.config.")
        || lower.starts_with("esbuild.")
        || lower.starts_with("tsup.config.")
        || lower.starts_with("next.config.")
        || lower.starts_with("nuxt.config.")
        || lower.starts_with("svelte.config.")
        || lower.starts_with("astro.config.")
    {
        return Role::Build;
    }
    if is_test_file(&lower) || in_dir(TEST_DIRS) {
        // Docs/assets in test folders are still tests (fixtures).
        return Role::Test;
    }
    let stem = stem_lower(&lower);
    if DOC_NAMES.contains(&stem.as_str()) {
        return Role::Documentation;
    }
    if let Some(l) = language {
        if l.category == Category::Documentation {
            return Role::Documentation;
        }
    }
    if lang::asset_kind(&ext).is_some() {
        return Role::Asset;
    }
    // Configuration: dotfiles, *.config.*, *rc files, tool configs.
    let is_dotfile = lower.starts_with('.');
    if is_dotfile
        || lower.contains(".config.")
        || lower.ends_with("rc")
        || lower.ends_with("rc.json")
        || lower.ends_with("rc.js")
        || lower.ends_with("rc.cjs")
        || lower.ends_with("rc.yml")
        || lower.ends_with("rc.yaml")
        || lower.starts_with("tsconfig")
        || lower.starts_with("jsconfig")
        || matches!(
            lower.as_str(),
            "tox.ini"
                | "pytest.ini"
                | "mypy.ini"
                | "rustfmt.toml"
                | "clippy.toml"
                | "rust-toolchain"
                | "rust-toolchain.toml"
                | "deny.toml"
                | "global.json"
                | "nuget.config"
                | "directory.build.props"
                | "appsettings.json"
                | "launchsettings.json"
                | "renovate.json"
                | "dependabot.yml"
                | "codecov.yml"
                | "biome.json"
                | "turbo.json"
                | "nx.json"
                | "lerna.json"
        )
        || lower.starts_with("appsettings.")
        || dirs.iter().any(|d| d == "config" || d == "configs" || d == ".config" || d == "settings")
    {
        return Role::Config;
    }
    if let Some(l) = language {
        match l.category {
            Category::Shell => return Role::Script,
            Category::Programming | Category::Stylesheet | Category::Markup | Category::Query => {
                if in_dir(SCRIPT_DIRS) && dirs.len() <= 2 && !in_dir(&["src", "lib", "app"]) {
                    return Role::Script;
                }
                if in_dir(DOC_DIRS) {
                    return Role::Documentation;
                }
                return Role::Source;
            }
            Category::Config => return Role::Config,
            Category::Build => return Role::Build,
            Category::Data => {
                // YAML/TOML/JSON at the root or beside code are almost always configuration.
                if matches!(l.id, "yaml" | "toml" | "ini") || dirs.is_empty() {
                    return Role::Config;
                }
                return Role::Data;
            }
            Category::Documentation => return Role::Documentation,
        }
    }
    if in_dir(DOC_DIRS) {
        return Role::Documentation;
    }
    if in_dir(ASSET_DIRS) {
        return Role::Asset;
    }
    Role::Other
}

/// Files whose *content* Structura never reads (credentials, keys, local secrets).
/// They are still listed (name, size) so their presence can be reported.
pub fn is_sensitive(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    if l == ".env"
        || (l.starts_with(".env.")
            && !matches!(l.as_str(), ".env.example" | ".env.sample" | ".env.template" | ".env.dist"))
    {
        return true;
    }
    let ext = lang::extension(&l).unwrap_or_default();
    matches!(
        ext.as_str(),
        "pem" | "key" | "p12" | "pfx" | "jks" | "keystore" | "kdbx" | "ppk" | "asc" | "gpg" | "der" | "crt" | "cer"
    ) || l.starts_with("id_rsa")
        || l.starts_with("id_dsa")
        || l.starts_with("id_ecdsa")
        || l.starts_with("id_ed25519")
        || matches!(
            l.as_str(),
            ".npmrc"
                | ".pypirc"
                | ".netrc"
                | "_netrc"
                | ".git-credentials"
                | ".htpasswd"
                | "credentials"
                | "credentials.json"
                | "secrets.json"
                | "secrets.yml"
                | "secrets.yaml"
                | "service-account.json"
                | "terraform.tfstate"
                | "terraform.tfstate.backup"
                | ".dockercfg"
                | "auth.json"
        )
        || (l.contains("secret") && matches!(ext.as_str(), "json" | "yml" | "yaml" | "toml" | "env" | "txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(p: &str) -> Role {
        classify(p, lang::detect(p.rsplit('/').next().unwrap()))
    }

    #[test]
    fn classifies_common_layouts() {
        assert_eq!(role("src/main.rs"), Role::Source);
        assert_eq!(role("src/app/App.jsx"), Role::Source);
        assert_eq!(role("src/app/App.test.jsx"), Role::Test);
        assert_eq!(role("tests/integration.rs"), Role::Test);
        assert_eq!(role("pkg/server/handler_test.go"), Role::Test);
        assert_eq!(role("tests/test_api.py"), Role::Test);
        assert_eq!(role("README.md"), Role::Documentation);
        assert_eq!(role("docs/guide/intro.md"), Role::Documentation);
        assert_eq!(role("LICENSE"), Role::Documentation);
        assert_eq!(role("package.json"), Role::Manifest);
        assert_eq!(role("src-tauri/Cargo.toml"), Role::Manifest);
        assert_eq!(role("App.csproj"), Role::Manifest);
        assert_eq!(role("requirements-dev.txt"), Role::Manifest);
        assert_eq!(role("package-lock.json"), Role::Lockfile);
        assert_eq!(role("Cargo.lock"), Role::Lockfile);
        assert_eq!(role("Dockerfile"), Role::Build);
        assert_eq!(role(".github/workflows/ci.yml"), Role::Build);
        assert_eq!(role("vite.config.js"), Role::Build);
        assert_eq!(role("tsconfig.json"), Role::Config);
        assert_eq!(role(".eslintrc.json"), Role::Config);
        assert_eq!(role(".gitignore"), Role::Config);
        assert_eq!(role("config/app.yml"), Role::Config);
        assert_eq!(role("assets/logo.svg"), Role::Asset);
        assert_eq!(role("public/fonts/a.woff2"), Role::Asset);
        assert_eq!(role("dist/app.min.js"), Role::Generated);
        assert_eq!(role("src/proto/api_pb2.py"), Role::Generated);
        assert_eq!(role("scripts/release.ps1"), Role::Script);
        assert_eq!(role("scripts/gen.py"), Role::Script);
        assert_eq!(role("data/cities.csv"), Role::Data);
        assert_eq!(role("weird.bin"), Role::Other);
    }

    #[test]
    fn sensitive_files() {
        for s in [
            ".env",
            ".env.local",
            "server.key",
            "cert.pem",
            "id_rsa",
            "id_ed25519.pub",
            ".npmrc",
            "credentials.json",
            "terraform.tfstate",
            "app-secrets.json",
        ] {
            assert!(is_sensitive(s), "{s}");
        }
        for s in [".env.example", "main.rs", "keys.rs", "package.json", "secret_handling.rs"] {
            assert!(!is_sensitive(s), "{s}");
        }
    }

    #[test]
    fn generated_markers() {
        assert!(has_generated_marker("// Code generated by protoc-gen-go. DO NOT EDIT."));
        assert!(has_generated_marker("/* @generated */"));
        assert!(!has_generated_marker("// handwritten"));
    }
}

//! Filesystem scanning.
//!
//! - Uses the `ignore` crate's walker (the ripgrep engine): `.gitignore`, `.ignore`,
//!   `.git/info/exclude` are honoured when enabled, even outside a git repository.
//! - **Symlinks are never followed.** They are reported as entries with `symlink: true` so a
//!   link pointing outside the project (or into a loop) cannot widen the scan.
//! - User exclusions are glob patterns matched against the project-relative path and every
//!   path segment (`node_modules` excludes the folder anywhere).
//! - Errors (permission denied, vanished files) become [`ScanWarning`]s; the scan continues.

pub mod explorer;

use crate::core::{rel_string, CancelToken};
use crate::error::{AppError, AppResult};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::UNIX_EPOCH;

pub const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".parcel-cache",
    ".cache",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".gradle",
    ".idea",
    ".vs",
    "bin/Debug",
    "bin/Release",
    "obj",
    "Pods",
    "DerivedData",
    "vendor/bundle",
    ".terraform",
    "zig-cache",
    ".zig-cache",
    "zig-out",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanOptions {
    pub respect_gitignore: bool,
    pub include_hidden: bool,
    /// Maximum directory depth below the root (`None` = unlimited).
    pub max_depth: Option<usize>,
    /// Stop after this many entries; the result is marked truncated.
    pub max_entries: usize,
    pub exclude: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            include_hidden: false,
            max_depth: None,
            max_entries: 250_000,
            exclude: DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEntry {
    /// Project-relative path with `/` separators.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub hidden: bool,
    pub symlink: bool,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScanWarning {
    pub path: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub entries: Vec<RawEntry>,
    pub warnings: Vec<ScanWarning>,
    pub truncated: bool,
}

/// Compiled exclusion matcher.
pub struct Excluder {
    set: GlobSet,
}

impl Excluder {
    pub fn new(patterns: &[String]) -> AppResult<Self> {
        let mut b = GlobSetBuilder::new();
        for p in patterns {
            let p = p.trim().trim_matches('/').replace('\\', "/");
            if p.is_empty() || p.starts_with('#') {
                continue;
            }
            let add = |b: &mut GlobSetBuilder, g: &str| -> AppResult<()> {
                let glob = Glob::new(g)
                    .map_err(|_| AppError::validation("invalid_exclude", format!("invalid exclude pattern: {g}")))?;
                b.add(glob);
                Ok(())
            };
            if p.contains('/') || p.contains('*') {
                add(&mut b, &p)?;
                if !p.starts_with("**/") {
                    add(&mut b, &format!("**/{p}"))?;
                }
            } else {
                // Bare name: match that segment anywhere.
                add(&mut b, &format!("**/{p}"))?;
                add(&mut b, &p)?;
            }
        }
        let set = b.build().map_err(|e| AppError::validation("invalid_exclude", e.to_string()))?;
        Ok(Self { set })
    }

    pub fn is_excluded(&self, rel: &str) -> bool {
        !rel.is_empty() && self.set.is_match(rel)
    }
}

pub fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

#[cfg(windows)]
fn hidden_attr(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}
#[cfg(not(windows))]
fn hidden_attr(_meta: &std::fs::Metadata) -> bool {
    false
}

pub fn mtime(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

/// Walk `root` (already validated and canonical). `progress` is called periodically with the
/// number of entries seen so far.
pub fn scan(
    root: &Path,
    opts: &ScanOptions,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(usize),
) -> AppResult<ScanResult> {
    let excluder = Excluder::new(&opts.exclude)?;
    let mut wb = ignore::WalkBuilder::new(root);
    wb.standard_filters(false)
        .hidden(!opts.include_hidden)
        .git_ignore(opts.respect_gitignore)
        .git_exclude(opts.respect_gitignore)
        .ignore(opts.respect_gitignore)
        .git_global(false)
        .parents(false)
        .require_git(false)
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b));
    if let Some(d) = opts.max_depth {
        wb.max_depth(Some(d));
    }
    let root_owned = root.to_path_buf();
    let ex = std::sync::Arc::new(excluder);
    let ex2 = ex.clone();
    wb.filter_entry(move |e| {
        if e.depth() == 0 {
            return true;
        }
        if e.file_name() == ".git" {
            return false;
        }
        let rel = rel_string(&root_owned, e.path());
        !ex2.is_excluded(&rel)
    });

    let mut out = ScanResult::default();
    for item in wb.build() {
        if out.entries.len() % 512 == 0 {
            cancel.check()?;
            progress(out.entries.len());
        }
        let entry = match item {
            Ok(e) => e,
            Err(err) => {
                let (path, kind) = match &err {
                    ignore::Error::WithPath { path, err } => (
                        rel_string(root, path),
                        if matches!(**err, ignore::Error::Io(ref io) if io.kind() == std::io::ErrorKind::PermissionDenied)
                        {
                            "permission-denied"
                        } else {
                            "io"
                        },
                    ),
                    ignore::Error::Loop { child, .. } => (rel_string(root, child), "symlink-loop"),
                    _ => (String::new(), "io"),
                };
                // Pattern errors in a .gitignore are not fatal either.
                out.warnings.push(ScanWarning { path, kind: kind.into(), message: short_error(&err) });
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let ft = match entry.file_type() {
            Some(ft) => ft,
            None => continue,
        };
        let rel = rel_string(root, entry.path());
        let name = entry.file_name().to_string_lossy();
        let symlink = ft.is_symlink();
        let meta = if symlink {
            entry.path().symlink_metadata()
        } else {
            entry.metadata().map_err(|e| std::io::Error::other(e.to_string()))
        };
        let meta = match meta {
            Ok(m) => m,
            Err(e) => {
                out.warnings.push(ScanWarning { path: rel, kind: "metadata".into(), message: e.to_string() });
                continue;
            }
        };
        if symlink {
            out.warnings.push(ScanWarning {
                path: rel.clone(),
                kind: "symlink-skipped".into(),
                message: "symbolic link or junction not followed".into(),
            });
        }
        let hidden = is_hidden_name(&name) || hidden_attr(&meta);
        if hidden && !opts.include_hidden && !is_hidden_name(&name) {
            // Windows hidden attribute (the walker only knows dot-files).
            continue;
        }
        out.entries.push(RawEntry {
            path: rel,
            is_dir: ft.is_dir(),
            size: if ft.is_file() { meta.len() } else { 0 },
            modified: mtime(&meta),
            hidden,
            symlink,
            depth: entry.depth(),
        });
        if out.entries.len() >= opts.max_entries {
            out.truncated = true;
            out.warnings.push(ScanWarning {
                path: String::new(),
                kind: "limit".into(),
                message: format!("scan stopped after {} entries (profile limit)", opts.max_entries),
            });
            break;
        }
    }
    progress(out.entries.len());
    // Entries under a skipped hidden directory may have been emitted before we knew the
    // directory was hidden only by attribute; keep the list consistent.
    if !opts.include_hidden {
        let hidden_dirs: Vec<String> =
            out.entries.iter().filter(|e| e.is_dir && e.hidden).map(|e| format!("{}/", e.path)).collect();
        if !hidden_dirs.is_empty() {
            out.entries.retain(|e| !hidden_dirs.iter().any(|d| e.path.starts_with(d.as_str())));
        }
    }
    Ok(out)
}

fn short_error(e: &ignore::Error) -> String {
    let s = e.to_string();
    if s.len() > 200 {
        format!("{}…", &s[..s.char_indices().nth(200).map(|(i, _)| i).unwrap_or(s.len())])
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(files: &[&str]) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        for f in files {
            let p = t.path().join(f);
            if f.ends_with('/') {
                fs::create_dir_all(&p).unwrap();
            } else {
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(&p, format!("content of {f}")).unwrap();
            }
        }
        t
    }

    fn paths(t: &tempfile::TempDir, opts: &ScanOptions) -> Vec<String> {
        let root = crate::core::simplify(t.path().canonicalize().unwrap());
        let r = scan(&root, opts, &CancelToken::new(), &mut |_| {}).unwrap();
        r.entries.into_iter().map(|e| e.path).collect()
    }

    #[test]
    fn respects_gitignore_without_git_repo() {
        let t = tree(&[
            ".gitignore",
            "src/a.rs",
            "secret.log",
            "gen/x.rs",
            "src/nested/.gitignore",
            "src/nested/keep.rs",
            "src/nested/drop.tmp",
        ]);
        fs::write(t.path().join(".gitignore"), "*.log\ngen/\n").unwrap();
        fs::write(t.path().join("src/nested/.gitignore"), "*.tmp\n").unwrap();
        let p = paths(&t, &ScanOptions::default());
        assert!(p.contains(&"src/a.rs".to_string()));
        assert!(p.contains(&"src/nested/keep.rs".to_string()));
        assert!(!p.iter().any(|x| x.ends_with(".log")));
        assert!(!p.iter().any(|x| x.starts_with("gen")));
        assert!(!p.iter().any(|x| x.ends_with(".tmp")));
        // Hidden files are excluded by default.
        assert!(!p.contains(&".gitignore".to_string()));

        let all = paths(&t, &ScanOptions { respect_gitignore: false, include_hidden: true, ..Default::default() });
        assert!(all.contains(&"secret.log".to_string()));
        assert!(all.contains(&"gen/x.rs".to_string()));
        assert!(all.contains(&".gitignore".to_string()));
    }

    #[test]
    fn default_and_custom_exclusions() {
        let t = tree(&[
            "node_modules/pkg/index.js",
            "app/node_modules/x.js",
            "src/main.js",
            "src/fixtures/big.json",
            "docs/a.md",
        ]);
        let p = paths(&t, &ScanOptions::default());
        assert!(!p.iter().any(|x| x.contains("node_modules")));
        assert!(p.contains(&"src/main.js".to_string()));
        let mut o = ScanOptions::default();
        o.exclude.push("src/fixtures".into());
        o.exclude.push("*.md".into());
        let p = paths(&t, &o);
        assert!(!p.iter().any(|x| x.contains("fixtures")));
        assert!(!p.iter().any(|x| x.ends_with(".md")));
    }

    #[test]
    fn never_scans_git_dir_and_honours_depth_and_limits() {
        let t = tree(&[".git/config", "a/b/c/d.txt", "a/x.txt"]);
        let p = paths(&t, &ScanOptions { include_hidden: true, ..Default::default() });
        assert!(!p.iter().any(|x| x.starts_with(".git")));
        let p = paths(&t, &ScanOptions { max_depth: Some(2), ..Default::default() });
        assert!(p.contains(&"a/b".to_string()));
        assert!(!p.contains(&"a/b/c".to_string()));
        let root = crate::core::simplify(t.path().canonicalize().unwrap());
        let r = scan(&root, &ScanOptions { max_entries: 2, ..Default::default() }, &CancelToken::new(), &mut |_| {})
            .unwrap();
        assert!(r.truncated);
        assert_eq!(r.entries.len(), 2);
    }

    #[test]
    fn cancellation_stops_scan() {
        let t = tree(&["a.txt"]);
        let c = CancelToken::new();
        c.cancel();
        let root = crate::core::simplify(t.path().canonicalize().unwrap());
        assert!(matches!(scan(&root, &ScanOptions::default(), &c, &mut |_| {}), Err(AppError::Cancelled)));
    }

    #[test]
    fn invalid_exclude_is_rejected() {
        assert!(Excluder::new(&["a[".to_string()]).is_err());
        let e = Excluder::new(&["vendor".into(), "*.min.js".into(), "docs/generated".into()]).unwrap();
        assert!(e.is_excluded("vendor"));
        assert!(e.is_excluded("x/vendor"));
        assert!(e.is_excluded("web/app.min.js"));
        assert!(e.is_excluded("docs/generated"));
        assert!(!e.is_excluded("src/vendor.rs"));
    }

    #[cfg(windows)]
    #[test]
    fn symlinked_directories_are_not_followed() {
        let t = tree(&["inside/a.txt"]);
        let outside = tree(&["secret/b.txt"]);
        let link = t.path().join("link");
        // Junctions need no privileges, unlike symlinks.
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J", link.to_str().unwrap(), outside.path().join("secret").to_str().unwrap()])
            .output()
            .unwrap();
        if !status.status.success() {
            return; // environment does not allow junctions
        }
        let p = paths(&t, &ScanOptions::default());
        assert!(p.contains(&"link".to_string()));
        assert!(!p.iter().any(|x| x.contains("b.txt")), "{p:?}");
    }
}

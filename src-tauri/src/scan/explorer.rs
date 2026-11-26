//! Lazy, one-directory-at-a-time listing for the Project Explorer.
//!
//! Unlike the analysis scan, the explorer shows *everything* (optionally) and flags each entry
//! as hidden, git-ignored, excluded, symlink or sensitive, so users can see why a file is or is
//! not analysed.

use super::{is_hidden_name, mtime, Excluder};
use crate::core::{normalize_rel, rel_string, resolve_inside};
use crate::error::AppResult;
use crate::lang;
use crate::project::roles::{self, Role};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub hidden: bool,
    pub ignored: bool,
    pub excluded: bool,
    pub symlink: bool,
    pub sensitive: bool,
    pub language: Option<&'static str>,
    pub role: Option<Role>,
    /// Number of direct children for directories (cheap read_dir count, capped).
    pub child_count: Option<u32>,
}

/// Git ignore rules from the root down to a directory (deeper files win).
pub struct IgnoreChain {
    levels: Vec<Gitignore>,
}

impl IgnoreChain {
    pub fn for_dir(root: &Path, dir: &Path) -> Self {
        let mut levels = Vec::new();
        let mut chain: Vec<PathBuf> = Vec::new();
        let mut cur = Some(dir);
        while let Some(d) = cur {
            chain.push(d.to_path_buf());
            if d == root {
                break;
            }
            cur = d.parent();
        }
        chain.reverse();
        for (i, d) in chain.iter().enumerate() {
            let mut b = GitignoreBuilder::new(d);
            let mut any = false;
            if i == 0 {
                let ex = d.join(".git").join("info").join("exclude");
                if ex.is_file() {
                    any |= b.add(ex).is_none();
                }
            }
            for name in [".gitignore", ".ignore"] {
                let f = d.join(name);
                if f.is_file() {
                    any |= b.add(f).is_none();
                }
            }
            if any {
                if let Ok(g) = b.build() {
                    levels.push(g);
                }
            }
        }
        Self { levels }
    }

    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        let mut ignored = false;
        for g in &self.levels {
            let m = g.matched_path_or_any_parents(path, is_dir);
            if m.is_ignore() {
                ignored = true;
            } else if m.is_whitelist() {
                ignored = false;
            }
        }
        ignored
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ListOptions {
    pub show_hidden: bool,
    pub show_ignored: bool,
}

/// List direct children of `rel` inside `root`. Directories first, then by name.
pub fn list_dir(root: &Path, rel: &str, excludes: &[String], opts: ListOptions) -> AppResult<Vec<DirItem>> {
    let rel = normalize_rel(rel)?;
    let dir = resolve_inside(root, &rel)?;
    let chain = IgnoreChain::for_dir(root, &dir);
    let excluder = Excluder::new(excludes)?;
    let mut out = Vec::new();
    for ent in std::fs::read_dir(&dir)? {
        let Ok(ent) = ent else { continue };
        let name = ent.file_name().to_string_lossy().into_owned();
        if name == ".git" && !opts.show_hidden {
            continue;
        }
        let Ok(ft) = ent.file_type() else { continue };
        let symlink = ft.is_symlink();
        let meta = if symlink { ent.path().symlink_metadata() } else { ent.metadata() };
        let Ok(meta) = meta else { continue };
        let is_dir = ft.is_dir() || (symlink && meta.is_dir());
        let path = rel_string(root, &ent.path());
        let hidden = is_hidden_name(&name) || {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                meta.file_attributes() & 0x2 != 0
            }
            #[cfg(not(windows))]
            {
                false
            }
        };
        let ignored = name == ".git" || chain.is_ignored(&ent.path(), is_dir);
        let excluded = excluder.is_excluded(&path);
        if hidden && !opts.show_hidden {
            continue;
        }
        if (ignored || excluded) && !opts.show_ignored {
            continue;
        }
        let language = if is_dir { None } else { lang::detect(&name).map(|l| l.id) };
        let role = if is_dir { None } else { Some(roles::classify(&path, lang::detect(&name))) };
        let child_count = if is_dir && !symlink {
            std::fs::read_dir(ent.path()).ok().map(|it| it.take(10_000).count() as u32)
        } else {
            None
        };
        out.push(DirItem {
            sensitive: !is_dir && roles::is_sensitive(&name),
            name,
            path,
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified: mtime(&meta),
            hidden,
            ignored,
            excluded,
            symlink,
            language,
            role,
            child_count,
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn flags_ignored_hidden_excluded_and_sensitive() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        fs::create_dir_all(r.join("src/sub")).unwrap();
        fs::create_dir_all(r.join("node_modules/x")).unwrap();
        fs::write(r.join(".gitignore"), "*.log\n!keep.log\n").unwrap();
        fs::write(r.join("src/.gitignore"), "local.txt\n").unwrap();
        fs::write(r.join("a.log"), "").unwrap();
        fs::write(r.join("keep.log"), "").unwrap();
        fs::write(r.join(".env"), "SECRET=1").unwrap();
        fs::write(r.join("src/main.rs"), "fn main(){}").unwrap();
        fs::write(r.join("src/local.txt"), "").unwrap();
        let root = crate::core::simplify(r.canonicalize().unwrap());
        let ex = vec!["node_modules".to_string()];
        let all = list_dir(&root, "", &ex, ListOptions { show_hidden: true, show_ignored: true }).unwrap();
        let get = |n: &str| all.iter().find(|i| i.name == n).unwrap().clone();
        assert!(get("a.log").ignored);
        assert!(!get("keep.log").ignored, "negation re-includes");
        assert!(get(".env").hidden && get(".env").sensitive);
        assert!(get("node_modules").excluded && get("node_modules").is_dir);
        assert!(all[0].is_dir, "directories first");
        assert_eq!(get("src").child_count, Some(4));

        let visible = list_dir(&root, "", &ex, ListOptions { show_hidden: false, show_ignored: false }).unwrap();
        assert!(visible.iter().all(|i| !i.ignored && !i.hidden && !i.excluded));

        let sub = list_dir(&root, "src", &ex, ListOptions { show_hidden: true, show_ignored: true }).unwrap();
        assert!(sub.iter().find(|i| i.name == "local.txt").unwrap().ignored, "nested .gitignore applies");
        let main = sub.iter().find(|i| i.name == "main.rs").unwrap();
        assert_eq!(main.language, Some("rust"));
        assert_eq!(main.path, "src/main.rs");
        assert!(list_dir(&root, "../", &ex, ListOptions { show_hidden: true, show_ignored: true }).is_err());
    }
}

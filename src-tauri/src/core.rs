//! Small shared building blocks: app paths, cancellation, path normalisation.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{AppError, AppResult};

pub const APP_DIR_NAME: &str = "IvilyStructura";

/// `%LOCALAPPDATA%\IvilyStructura` (or the platform equivalent).
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join(APP_DIR_NAME)
}

pub fn db_path() -> PathBuf {
    data_dir().join("structura.db")
}

/// Cooperative cancellation flag shared between the UI command and the worker.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> AppResult<()> {
        if self.is_cancelled() {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Remove the Windows verbatim prefix produced by `canonicalize` (`\\?\C:\x` -> `C:\x`).
pub fn simplify(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if !rest.starts_with("UNC\\") {
            return PathBuf::from(rest);
        }
    }
    p
}

/// Validate a user-supplied project root: absolute, existing directory, canonicalised.
pub fn validate_root(input: &str) -> AppResult<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("path_empty", "path is empty"));
    }
    if trimmed.contains('\0') {
        return Err(AppError::validation("path_invalid", "path contains NUL"));
    }
    let p = Path::new(trimmed);
    if !p.is_absolute() {
        return Err(AppError::validation("path_not_absolute", "project path must be absolute"));
    }
    let canon = p.canonicalize().map_err(|_| AppError::NotFound(trimmed.to_string()))?;
    if !canon.is_dir() {
        return Err(AppError::validation("path_not_directory", "project path must be a directory"));
    }
    Ok(simplify(canon))
}

/// Validate a project-relative path (forward or back slashes). Rejects absolute paths,
/// drive prefixes and `..` components. Returns the normalised `a/b/c` form ("" for root).
pub fn normalize_rel(rel: &str) -> AppResult<String> {
    let rel = rel.replace('\\', "/");
    let mut parts = Vec::new();
    for c in Path::new(&rel).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return Err(AppError::validation("path_outside_project", "path must stay inside the project")),
        }
    }
    Ok(parts.join("/"))
}

/// Resolve a relative path inside `root` and verify (after resolving symlinks) that the
/// target is still inside `root`.
pub fn resolve_inside(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let rel = normalize_rel(rel)?;
    let joined = if rel.is_empty() { root.to_path_buf() } else { root.join(&rel) };
    let canon = simplify(joined.canonicalize().map_err(|_| AppError::NotFound(rel.clone()))?);
    let root_canon = simplify(root.canonicalize()?);
    if !canon.starts_with(&root_canon) {
        return Err(AppError::validation("path_outside_project", "path resolves outside the project"));
    }
    Ok(canon)
}

/// `root`-relative path with `/` separators.
pub fn rel_string(root: &Path, p: &Path) -> String {
    let r = p.strip_prefix(root).unwrap_or(p);
    let mut out = String::new();
    for (i, c) in r.components().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&c.as_os_str().to_string_lossy());
    }
    out
}

pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rejects_escapes() {
        assert_eq!(normalize_rel("a\\b/./c").unwrap(), "a/b/c");
        assert_eq!(normalize_rel("").unwrap(), "");
        assert!(normalize_rel("../x").is_err());
        assert!(normalize_rel("a/../../x").is_err());
        assert!(normalize_rel("/etc/passwd").is_err());
        assert!(normalize_rel("C:\\Windows").is_err());
    }

    #[test]
    fn validate_root_requires_existing_absolute_dir() {
        let t = tempfile::tempdir().unwrap();
        assert!(validate_root(t.path().to_str().unwrap()).is_ok());
        assert!(validate_root("relative/dir").is_err());
        assert!(validate_root("").is_err());
        let f = t.path().join("f.txt");
        std::fs::write(&f, "x").unwrap();
        assert!(validate_root(f.to_str().unwrap()).is_err());
        assert!(validate_root(t.path().join("missing").to_str().unwrap()).is_err());
    }

    #[test]
    fn resolve_inside_blocks_traversal() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join("a")).unwrap();
        std::fs::write(t.path().join("a/x.txt"), "x").unwrap();
        let root = simplify(t.path().canonicalize().unwrap());
        assert!(resolve_inside(&root, "a/x.txt").is_ok());
        assert!(resolve_inside(&root, "../").is_err());
        assert!(resolve_inside(&root, "a/missing").is_err());
    }

    #[test]
    fn cancel_token() {
        let c = CancelToken::new();
        assert!(c.check().is_ok());
        c.clone().cancel();
        assert!(matches!(c.check(), Err(AppError::Cancelled)));
    }
}

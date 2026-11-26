//! Tauri IPC commands. Commands validate input and delegate to the domain modules; no analysis
//! logic lives here.

pub mod analysis;
pub mod app;
pub mod data;
pub mod views;

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// Validate a user-chosen output/input file path (from a native dialog).
pub fn user_file(path: &str, allowed_ext: &[&str], must_exist: bool) -> AppResult<PathBuf> {
    let p = Path::new(path.trim());
    if path.contains('\0') || !p.is_absolute() {
        return Err(AppError::validation("path_invalid", "an absolute file path is required"));
    }
    let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !allowed_ext.iter().any(|e| name.ends_with(&format!(".{e}"))) {
        return Err(AppError::validation(
            "extension_invalid",
            format!("file must end with: {}", allowed_ext.join(", ")),
        ));
    }
    if must_exist {
        if !p.is_file() {
            return Err(AppError::NotFound(path.to_string()));
        }
    } else {
        let parent = p.parent().ok_or_else(|| AppError::validation("path_invalid", "invalid path"))?;
        if !parent.is_dir() {
            return Err(AppError::NotFound(parent.to_string_lossy().into_owned()));
        }
        if p.is_dir() {
            return Err(AppError::validation("path_is_directory", "the target is a directory"));
        }
    }
    Ok(p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_file_validation() {
        let t = tempfile::tempdir().unwrap();
        let ok = t.path().join("r.md");
        assert!(user_file(ok.to_str().unwrap(), &["md"], false).is_ok());
        assert!(user_file(ok.to_str().unwrap(), &["md"], true).is_err());
        assert!(user_file("relative.md", &["md"], false).is_err());
        assert!(user_file(t.path().join("r.exe").to_str().unwrap(), &["md"], false).is_err());
        assert!(user_file(t.path().join("missing/r.md").to_str().unwrap(), &["md"], false).is_err());
    }
}

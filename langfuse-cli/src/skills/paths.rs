//! Lexical and physical path helpers (Node's `path.resolve`, `relative` and
//! `realpath` semantics, which the skills lockfile format depends on).

use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use crate::error::{CliError, Result};

/// `path` made absolute against `base`, with `.` and `..` resolved lexically.
pub fn resolve(base: &Path, path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    normalize(&joined)
}

/// Resolve `.` and `..` lexically.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The relative path from `from` to `to` (both absolute and normalized).
pub fn relative(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<Component<'_>> = from.components().collect();
    let to: Vec<Component<'_>> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for component in &to[common..] {
        out.push(component);
    }
    out
}

/// Whether `child` is `parent` or inside it.
pub fn contains(parent: &Path, child: &Path) -> bool {
    child.starts_with(parent)
}

/// `path` with `/` separators (lockfile keys).
pub fn to_slash(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The real path of `path`, or of its nearest existing ancestor followed by
/// the remaining segments.
pub fn physical(path: &Path) -> Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(real) => Ok(real),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            match (path.parent(), path.file_name()) {
                (Some(parent), Some(name)) => Ok(physical(parent)?.join(name)),
                _ => Ok(path.to_path_buf()),
            }
        }
        Err(error) => Err(CliError::local(format!(
            "Cannot check installation directory {}: {error}",
            path.display()
        ))),
    }
}

/// Whether something (even a broken symlink) exists at `path`.
pub fn exists(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_helpers() {
        assert_eq!(resolve(Path::new("/a/b"), "../c/./d"), Path::new("/a/c/d"));
        assert_eq!(resolve(Path::new("/a"), "/x/../y"), Path::new("/y"));
        assert_eq!(
            relative(Path::new("/a/b"), Path::new("/a/c/d")),
            Path::new("../c/d")
        );
        assert_eq!(relative(Path::new("/a"), Path::new("/a")), Path::new(""));
        assert!(contains(Path::new("/a"), Path::new("/a/b")));
        assert!(contains(Path::new("/a"), Path::new("/a")));
        assert!(!contains(Path::new("/a/b"), Path::new("/a/bc")));
        assert_eq!(to_slash(Path::new("x/y/z")), "x/y/z");
    }
}

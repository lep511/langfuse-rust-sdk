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

/// `path` with `/` separators (lockfile keys, Git paths), or `None` when a
/// component is not valid UTF-8: such a path has no faithful text form.
pub fn to_slash(path: &Path) -> Option<String> {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?;
    Some(components.join("/"))
}

/// A hidden sibling of `path` for work in progress:
/// `.<name>.<purpose>-<uuid>`, in the same directory so that a rename onto
/// `path` stays on one filesystem.
pub fn sibling(path: &Path, purpose: &str) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(".{purpose}-{}", uuid::Uuid::new_v4()));
    path.parent().unwrap_or(Path::new("/")).join(name)
}

/// A temporary file or directory, removed when the guard is dropped: after it
/// was renamed into place (when nothing is left to remove), on every error
/// path, on a panic, and when the future holding it is cancelled.
#[derive(Debug)]
pub struct Temporary(PathBuf);

impl Temporary {
    /// Guard `path`, which the caller creates.
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    /// The guarded path.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        match std::fs::symlink_metadata(&self.0) {
            Ok(metadata) if metadata.is_dir() => {
                let _ = std::fs::remove_dir_all(&self.0);
            }
            Ok(_) => {
                let _ = std::fs::remove_file(&self.0);
            }
            Err(_) => {}
        }
    }
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
        assert_eq!(to_slash(Path::new("x/y/z")).as_deref(), Some("x/y/z"));
    }

    #[cfg(unix)]
    #[test]
    fn paths_that_are_not_utf8_have_no_text_form() {
        use std::os::unix::ffi::OsStrExt as _;
        let name = std::ffi::OsStr::from_bytes(b"caf\xe9");
        assert_eq!(to_slash(&Path::new("x").join(name)), None);
        let hidden = sibling(&Path::new("/a").join(name), "install");
        assert_eq!(hidden.parent(), Some(Path::new("/a")));
        assert!(
            hidden
                .file_name()
                .unwrap()
                .as_encoded_bytes()
                .starts_with(b".caf\xe9.install-")
        );
    }

    #[test]
    fn temporaries_are_removed_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let staging = Temporary::new(sibling(&dir.path().join("skill"), "install"));
        std::fs::create_dir(staging.path()).unwrap();
        std::fs::write(staging.path().join("a"), "a").unwrap();
        let file = Temporary::new(dir.path().join("lock.tmp"));
        std::fs::write(file.path(), "x").unwrap();
        // Renamed into place: nothing is left for the guard to remove.
        let renamed = Temporary::new(dir.path().join("renamed.tmp"));
        std::fs::write(renamed.path(), "x").unwrap();
        std::fs::rename(renamed.path(), dir.path().join("kept")).unwrap();
        let (staging_path, file_path) = (staging.path().to_owned(), file.path().to_owned());
        drop((staging, file, renamed));
        assert!(!staging_path.exists() && !file_path.exists());
        assert!(dir.path().join("kept").exists());
    }
}

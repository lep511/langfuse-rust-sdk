//! Local agent skills: discovery and the references Langfuse records on spans.
//!
//! [`discover_skills`] lists the skills installed in a directory (one
//! subdirectory with a `SKILL.md` file per skill), with the versions pinned in
//! a `langfuse-skills.lock` file when there is one. With the `otel` feature,
//! the result goes to `TracingOptions::with_skills` (attached to the spans of
//! scopes that enable `PropagatedAttributes::with_attach_skills`) or directly
//! to `PropagatedAttributes::with_skills_available`; Langfuse shows them as the
//! skills available to the observation.
//!
//! ```rust,no_run
//! use langfuse_rust_sdk::skills::discover_skills;
//!
//! let skills = discover_skills("./skills");
//! for skill in &skills {
//!     println!("{} {:?}", skill.name, skill.version);
//! }
//! ```
//!
//! This is independent of the REST API for skills
//! ([`client.skills`](crate::api::skills::SkillsClient)).

use std::collections::{HashMap, HashSet};
use std::fs::{self, Metadata};
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Name of the lock file written by the Langfuse CLI when it installs skills.
pub const SKILLS_LOCK_FILE: &str = "langfuse-skills.lock";
/// Longest skill name.
const MAX_NAME_CHARS: usize = 128;
/// Largest skill version in a lock file.
const MAX_VERSION: i64 = 2_147_483_647;

/// A skill available to an agent, optionally pinned to a version.
///
/// Serializes as `{"name": ..., "version": ...}` plus any [`extra`](Self::extra)
/// fields, which are kept as they are (for fields newer than this SDK).
///
/// ```
/// use langfuse_rust_sdk::skills::SkillReference;
///
/// let skill = SkillReference::new("customer-support").with_version(3);
/// assert_eq!(
///     serde_json::to_string(&skill).unwrap(),
///     r#"{"name":"customer-support","version":3}"#
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SkillReference {
    /// Skill name.
    pub name: String,
    /// Installed version, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    /// Other fields, serialized next to `name` and `version`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl SkillReference {
    /// A skill without version.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            extra: Map::new(),
        }
    }

    /// Set [`version`](Self::version).
    pub fn with_version(mut self, version: i64) -> Self {
        self.version = Some(version);
        self
    }

    /// Add one [`extra`](Self::extra) field.
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

/// List the skills installed in `dir`, sorted by directory name.
///
/// Every immediate subdirectory holding a regular `SKILL.md` file is one skill
/// (its contents are not read). Hidden directories (`.name`) are skipped and
/// symbolic links are followed. A skill is named after its directory, unless
/// the lock file records it: `<dir>/langfuse-skills.lock` or, when that file
/// does not exist, `./langfuse-skills.lock` (only the first one found is
/// read). The lock is the JSON the Langfuse CLI writes,
/// `{"lockVersion": 1, "skills": [{"path": "...", "name": "...", "version": 2}]}`,
/// with paths relative to the lock file; lock entries do not detect local
/// edits.
///
/// Never fails: a missing directory returns an empty list. Unreadable
/// directories, unreadable or malformed lock files (the directory names are
/// used then), invalid lock entries, names over 128 characters and duplicate
/// names are skipped with a warning logged through `tracing`. Same rules as
/// `discover_skills` of the Python SDK.
pub fn discover_skills(dir: impl AsRef<Path>) -> Vec<SkillReference> {
    let cwd = std::env::current_dir().ok();
    discover(dir.as_ref(), cwd.as_deref())
}

/// [`discover_skills`] with an explicit working directory (for the lock
/// fallback and relative paths).
fn discover(dir: &Path, cwd: Option<&Path>) -> Vec<SkillReference> {
    let root = absolute(dir, cwd);
    let candidates = match list_directory(&root) {
        Ok(Some(candidates)) => candidates,
        Ok(None) => return Vec::new(),
        Err(error) => {
            tracing::warn!(
                directory = %root.display(),
                %error,
                "Could not read skills directory. No skills discovered"
            );
            return Vec::new();
        }
    };

    let locked = match find_lock(&root, cwd).and_then(|lock| read_lock(lock.as_deref())) {
        Ok(locked) => locked,
        Err(error) => {
            tracing::warn!(
                directory = %root.display(),
                %error,
                "Could not read skill lockfile. Using directory names without versions"
            );
            HashMap::new()
        }
    };

    let mut available = Vec::new();
    let mut names = HashSet::new();
    for name in candidates {
        let Some(name) = name.to_str().map(str::to_owned) else {
            tracing::warn!(name = ?name, "Skipping skill directory: name is not valid UTF-8");
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let directory = root.join(&name);
        match is_skill_directory(&directory) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                tracing::warn!(
                    directory = %directory.display(),
                    %error,
                    "Skipping skill directory: could not inspect files"
                );
                continue;
            }
        }
        let skill = locked
            .get(&directory)
            .cloned()
            .unwrap_or_else(|| SkillReference::new(name));
        if skill.name.chars().count() > MAX_NAME_CHARS {
            tracing::warn!(name = %skill.name, "Skipping skill: name exceeds 128 characters");
            continue;
        }
        if !names.insert(skill.name.clone()) {
            tracing::warn!(name = %skill.name, "Skipping duplicate skill name");
            continue;
        }
        available.push(skill);
    }
    available
}

/// The entry names of `root`, sorted; `None` when it does not exist.
fn list_directory(root: &Path) -> io::Result<Option<Vec<std::ffi::OsString>>> {
    if stat_if_exists(root)?.is_none() {
        return Ok(None);
    }
    let mut names = fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    Ok(Some(names))
}

/// Whether `directory` is a directory with a regular `SKILL.md` file
/// (following symbolic links).
fn is_skill_directory(directory: &Path) -> io::Result<bool> {
    let is_dir = stat_if_exists(directory)?.is_some_and(|m| m.is_dir());
    if !is_dir {
        return Ok(false);
    }
    Ok(stat_if_exists(&directory.join("SKILL.md"))?.is_some_and(|m| m.is_file()))
}

/// Metadata of `path` (following symbolic links), `None` when it does not exist.
fn stat_if_exists(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The lock of `root`, else the one of the working directory, if any exists.
fn find_lock(root: &Path, cwd: Option<&Path>) -> Result<Option<PathBuf>, LockError> {
    let candidates = std::iter::once(root.join(SKILLS_LOCK_FILE))
        .chain(cwd.map(|cwd| absolute(&cwd.join(SKILLS_LOCK_FILE), None)));
    for path in candidates {
        if stat_if_exists(&path)?.is_some() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Why a lock file could not be used.
#[derive(Debug)]
enum LockError {
    Io(io::Error),
    Invalid(String),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Io(error) => error.fmt(f),
            LockError::Invalid(message) => f.write_str(message),
        }
    }
}

impl From<io::Error> for LockError {
    fn from(error: io::Error) -> Self {
        LockError::Io(error)
    }
}

/// The skills recorded in `file`, by absolute directory path.
fn read_lock(file: Option<&Path>) -> Result<HashMap<PathBuf, SkillReference>, LockError> {
    let Some(file) = file else {
        return Ok(HashMap::new());
    };
    let Some(metadata) = stat_if_exists(file)? else {
        return Ok(HashMap::new());
    };
    if !metadata.is_file() {
        return Err(LockError::Invalid(format!(
            "Invalid skill lockfile {}.",
            file.display()
        )));
    }
    let unsupported = || {
        LockError::Invalid(format!(
            "Unsupported or invalid skill lockfile {}.",
            file.display()
        ))
    };
    let value: Value = serde_json::from_str(&fs::read_to_string(file)?).map_err(|error| {
        LockError::Invalid(format!("Invalid JSON in {}: {error}", file.display()))
    })?;
    let lock_version_is_int = value
        .get("lockVersion")
        .and_then(Value::as_number)
        .is_some_and(|n| n.is_i64() || n.is_u64());
    let skills = value
        .get("skills")
        .and_then(Value::as_array)
        .filter(|_| lock_version_is_int)
        .ok_or_else(unsupported)?;

    let base = file.parent().unwrap_or(Path::new(""));
    let mut locked = HashMap::new();
    for skill in skills {
        let Some((path, reference)) = locked_skill(skill) else {
            tracing::warn!(file = %file.display(), "Skipping invalid skill entry");
            continue;
        };
        locked.insert(absolute(&base.join(path), None), reference);
    }
    Ok(locked)
}

/// The path and reference of a valid lock entry.
fn locked_skill(skill: &Value) -> Option<(&str, SkillReference)> {
    let path = skill.get("path")?.as_str().filter(|p| !p.is_empty())?;
    let name = skill
        .get("name")?
        .as_str()
        .filter(|n| (1..=MAX_NAME_CHARS).contains(&n.chars().count()))?;
    // `as_i64` is `None` for floats (`1.5`, `2.0`) and booleans.
    let version = skill
        .get("version")?
        .as_i64()
        .filter(|v| (1..=MAX_VERSION).contains(v))?;
    Some((path, SkillReference::new(name).with_version(version)))
}

/// `path` made absolute against `cwd` and normalized lexically (`.` and `..`
/// removed, symbolic links kept), like Python's `os.path.abspath`.
fn absolute(path: &Path, cwd: Option<&Path>) -> PathBuf {
    let joined = match cwd {
        Some(cwd) if !path.is_absolute() => cwd.join(path),
        _ => path.to_path_buf(),
    };
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Compact JSON of a skill list for the span attribute, `None` (with a
/// warning) when it cannot be serialized.
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub(crate) fn serialize_skills(skills: &[SkillReference]) -> Option<String> {
    match serde_json::to_string(skills) {
        Ok(json) => Some(json),
        Err(error) => {
            tracing::warn!(%error, "Failed to serialize available skills. Dropping value");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system temp directory, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("langfuse-skills-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn install(root: &Path, name: &str) -> PathBuf {
        let directory = root.join(name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("SKILL.md"), "# Skill").unwrap();
        directory
    }

    fn write_lock(root: &Path, skills: Value, lock_version: Value) {
        let lock = serde_json::json!({"lockVersion": lock_version, "skills": skills});
        fs::write(root.join(SKILLS_LOCK_FILE), lock.to_string()).unwrap();
    }

    fn names(skills: &[SkillReference]) -> Vec<(String, Option<i64>)> {
        skills.iter().map(|s| (s.name.clone(), s.version)).collect()
    }

    fn unversioned(names: &[&str]) -> Vec<(String, Option<i64>)> {
        names.iter().map(|n| (n.to_string(), None)).collect()
    }

    #[cfg(unix)]
    #[test]
    fn discovery_follows_symlinks_and_only_includes_immediate_skills() {
        use std::os::unix::fs::symlink;
        let tmp = TempDir::new();
        let root = tmp.0.join("skills");
        install(&root, "plain");
        install(&root, ".hidden");
        install(&root, "nested/deeper");
        symlink(root.join("plain"), root.join("directory-link")).unwrap();
        fs::create_dir(root.join("file-link")).unwrap();
        symlink(root.join("plain/SKILL.md"), root.join("file-link/SKILL.md")).unwrap();
        fs::create_dir_all(root.join("directory-marker/SKILL.md")).unwrap();

        let skills = discover(&root, Some(&tmp.0));
        assert_eq!(
            names(&skills),
            unversioned(&["directory-link", "file-link", "plain"])
        );
        assert!(discover(&tmp.0.join("missing"), Some(&tmp.0)).is_empty());
    }

    #[test]
    fn configured_lock_precedes_cwd_and_resolves_paths_from_the_lock() {
        for lock_version in [1, 2] {
            let tmp = TempDir::new();
            let root = tmp.0.join("skills");
            install(&root, "customer");
            write_lock(
                &tmp.0,
                serde_json::json!([{"path": "skills/customer", "name": "cwd", "version": 1}]),
                lock_version.into(),
            );
            write_lock(
                &root,
                serde_json::json!([{"path": "customer", "name": "configured", "version": 2}]),
                lock_version.into(),
            );
            // A relative directory is resolved against the working directory.
            let skills = discover(Path::new("skills"), Some(&tmp.0));
            assert_eq!(names(&skills), [("configured".to_string(), Some(2))]);

            fs::remove_file(root.join(SKILLS_LOCK_FILE)).unwrap();
            let skills = discover(Path::new("./skills/../skills"), Some(&tmp.0));
            assert_eq!(names(&skills), [("cwd".to_string(), Some(1))]);
        }
    }

    #[test]
    fn invalid_entries_duplicates_and_long_names_preserve_other_skills() {
        let tmp = TempDir::new();
        let long = "x".repeat(129);
        for name in ["first", "second", "invalid-version", long.as_str()] {
            install(&tmp.0, name);
        }
        write_lock(
            &tmp.0,
            serde_json::json!([
                {"path": "first", "name": "same", "version": 2_147_483_647_i64},
                {"path": "second", "name": "same", "version": 2},
                {"path": "invalid-version", "name": "bad", "version": true},
                {"path": "first", "name": "bad", "version": 2_147_483_648_i64},
                {"path": "first", "name": "bad", "version": 0},
                {"path": "first", "name": "bad", "version": 1.5},
                {"path": "", "name": "bad", "version": 1},
                {"path": "first", "name": "", "version": 1},
            ]),
            1.into(),
        );
        let skills = discover(&tmp.0, None);
        assert_eq!(
            names(&skills),
            [
                ("same".to_string(), Some(2_147_483_647)),
                ("invalid-version".to_string(), None)
            ]
        );
    }

    #[test]
    fn malformed_lock_falls_back_to_directory_names() {
        for content in [
            "not json",
            r#"{"lockVersion":1,"skills":{}}"#,
            r#"{"lockVersion":true,"skills":[]}"#,
            r#"{"lockVersion":1.5,"skills":[]}"#,
        ] {
            let tmp = TempDir::new();
            let root = tmp.0.join("skills");
            install(&root, "customer");
            write_lock(
                &tmp.0,
                serde_json::json!([{"path": "skills/customer", "name": "cwd", "version": 1}]),
                1.into(),
            );
            fs::write(root.join(SKILLS_LOCK_FILE), content).unwrap();
            assert_eq!(
                names(&discover(&root, Some(&tmp.0))),
                unversioned(&["customer"]),
                "{content}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn discovery_follows_a_lock_symlink() {
        use std::os::unix::fs::symlink;
        let tmp = TempDir::new();
        let root = tmp.0.join("skills");
        install(&root, "customer");
        write_lock(
            &tmp.0,
            serde_json::json!([{"path": "customer", "name": "locked", "version": 2}]),
            1.into(),
        );
        // Paths are relative to the lock file named in the directory, not its target.
        symlink(tmp.0.join(SKILLS_LOCK_FILE), root.join(SKILLS_LOCK_FILE)).unwrap();
        assert_eq!(
            names(&discover(&root, None)),
            [("locked".to_string(), Some(2))]
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_lock_falls_back_to_directory_names() {
        use std::os::unix::fs::symlink;
        for kind in ["directory", "symlink-loop"] {
            let tmp = TempDir::new();
            install(&tmp.0, "customer");
            let lock = tmp.0.join(SKILLS_LOCK_FILE);
            if kind == "directory" {
                fs::create_dir(&lock).unwrap();
            } else {
                symlink(&lock, &lock).unwrap();
            }
            assert_eq!(
                names(&discover(&tmp.0, None)),
                unversioned(&["customer"]),
                "{kind}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn discovery_skips_entries_with_filesystem_errors() {
        use std::os::unix::fs::symlink;
        for broken_path in ["directory", "SKILL.md"] {
            let tmp = TempDir::new();
            install(&tmp.0, "valid");
            let mut broken = tmp.0.join("broken");
            if broken_path == "SKILL.md" {
                fs::create_dir(&broken).unwrap();
                broken = broken.join("SKILL.md");
            }
            symlink(&broken, &broken).unwrap();
            symlink(tmp.0.join("missing"), tmp.0.join("missing-target")).unwrap();
            assert_eq!(
                names(&discover(&tmp.0, None)),
                unversioned(&["valid"]),
                "{broken_path}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_skills_directory_returns_an_empty_list() {
        use std::os::unix::fs::symlink;
        let tmp = TempDir::new();
        let file = tmp.0.join("file");
        fs::write(&file, "not a directory").unwrap();
        assert!(discover(&file, None).is_empty());
        let looped = tmp.0.join("loop");
        symlink(&looped, &looped).unwrap();
        assert!(discover(&looped, None).is_empty());
    }

    #[test]
    fn serialization_keeps_extra_fields() {
        let skills = vec![
            SkillReference::new("customer")
                .with_version(2)
                .with_field("futureField", serde_json::json!({"nested": true})),
        ];
        let json = serialize_skills(&skills).unwrap();
        assert_eq!(
            json,
            r#"[{"name":"customer","version":2,"futureField":{"nested":true}}]"#
        );
        let back: Vec<SkillReference> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, skills);
        assert_eq!(serialize_skills(&[]).unwrap(), "[]");
    }

    #[test]
    fn absolute_normalizes_lexically() {
        assert_eq!(
            absolute(Path::new("a/./b/../c"), Some(Path::new("/w"))),
            PathBuf::from("/w/a/c")
        );
        assert_eq!(
            absolute(Path::new("/x/../../y"), Some(Path::new("/w"))),
            PathBuf::from("/y")
        );
    }
}

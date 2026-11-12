//! `langfuse-skills.lock`: the installed or pushed skills of a project.
//!
//! Strict like the official CLI (unknown keys, a `lockVersion` other than 1,
//! invalid entries and duplicate paths are errors), and readable by
//! `langfuse_rust_sdk::skills::discover_skills`.

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{CliError, Result};
use crate::skills::paths::{Temporary, physical, relative, resolve, sibling, to_slash};
use crate::skills::types::{SkillName, Version};

/// The lockfile name.
pub const LOCK_FILE: &str = "langfuse-skills.lock";

/// What a lock entry follows: `{"label": …}` or `{"version": …}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Selector {
    /// A label (`production`).
    #[serde(deserialize_with = "non_empty")]
    Label(String),
    /// An immutable version.
    Version(Version),
}

/// One locked skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedSkill {
    /// Installation directory, relative to the lockfile, `/`-separated.
    #[serde(deserialize_with = "non_empty")]
    pub path: String,
    /// Skill name.
    pub name: SkillName,
    /// What updates follow.
    pub selector: Selector,
    /// The installed or pushed version.
    pub version: Version,
}

/// The lockfile contents.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillsLock {
    #[serde(rename = "lockVersion")]
    _format: Format,
    /// Locked skills.
    pub skills: Vec<LockedSkill>,
}

/// The lockfile format, `"lockVersion": 1`, the only one there is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Format;

impl Serialize for Format {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> Deserialize<'de> for Format {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        match serde_json::Value::deserialize(deserializer)?.as_i64() {
            Some(1) => Ok(Self),
            _ => Err(D::Error::custom("lockVersion must be 1")),
        }
    }
}

/// A string that is not empty.
fn non_empty<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<String, D::Error> {
    let text = String::deserialize(deserializer)?;
    if text.is_empty() {
        return Err(D::Error::custom("expected a non-empty string"));
    }
    Ok(text)
}

/// What the derived deserialization cannot say: no two entries share a path.
fn validate(skills: &[LockedSkill]) -> std::result::Result<(), String> {
    let mut paths = std::collections::HashSet::new();
    if skills
        .iter()
        .any(|skill| !paths.insert(skill.path.as_str()))
    {
        return Err("skill paths must be unique".to_owned());
    }
    Ok(())
}

fn parse(text: &str) -> std::result::Result<SkillsLock, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    // A derived struct also deserializes from an array of its fields in
    // order; the lockfile and its entries must be objects.
    if !value.is_object() {
        return Err("the lockfile must be a JSON object".to_owned());
    }
    let entries = value.get("skills").and_then(serde_json::Value::as_array);
    if let Some(index) = entries.and_then(|entries| entries.iter().position(|e| !e.is_object())) {
        return Err(format!("skills[{index}] must be an object"));
    }
    let lock: SkillsLock = serde_json::from_value(value).map_err(|error| error.to_string())?;
    validate(&lock.skills)?;
    Ok(lock)
}

/// Read the lockfile; a missing file is an empty lock.
pub fn read(path: &Path) -> Result<SkillsLock> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text)
            .map_err(|error| CliError::local(format!("Cannot read {}: {error}", path.display()))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(SkillsLock::default()),
        Err(error) => Err(CliError::local(format!(
            "Cannot read {}: {error}",
            path.display()
        ))),
    }
}

/// The `path` of `directory` in `lockfile`: relative to the lockfile and
/// `/`-separated. A directory whose path is not valid UTF-8 cannot be locked.
pub fn key(lockfile: &Path, directory: &Path) -> Result<String> {
    let base = lockfile.parent().unwrap_or(Path::new("/"));
    to_slash(&relative(base, directory)).ok_or_else(|| {
        CliError::local(format!(
            "Cannot record {} in the lockfile: its path is not valid UTF-8",
            directory.display()
        ))
    })
}

/// Record `skill`, replacing any entry for the same installation directory.
pub fn record(lock: &mut SkillsLock, lockfile: &Path, skill: LockedSkill) {
    let directory = lockfile.parent().unwrap_or(Path::new("/"));
    let destination = resolve(directory, &skill.path);
    lock.skills
        .retain(|existing| resolve(directory, &existing.path) != destination);
    lock.skills.push(skill);
}

/// The lockfile as it is written: the entries sorted by path.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Document<'a> {
    lock_version: Format,
    skills: Vec<&'a LockedSkill>,
}

/// Write the lockfile atomically (temporary file, then rename), sorted by path.
pub fn write(lock: &SkillsLock, path: &Path) -> Result<()> {
    let fail = |error: &dyn std::fmt::Display| {
        CliError::local(format!("Cannot write {}: {error}", path.display()))
    };
    validate(&lock.skills).map_err(|error| fail(&error))?;
    let mut skills: Vec<&LockedSkill> = lock.skills.iter().collect();
    skills.sort_by(|a, b| a.path.cmp(&b.path));
    let document = Document {
        lock_version: Format,
        skills,
    };
    let mut text = serde_json::to_string_pretty(&document).map_err(|error| fail(&error))?;
    text.push('\n');
    let directory = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(directory).map_err(|error| fail(&error))?;
    // Removed on every path out of this function; after the rename there is
    // nothing left to remove.
    let temporary = Temporary::new(sibling(path, "tmp"));
    let written = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary.path())?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(temporary.path(), path)
    })();
    written.map_err(|error| fail(&error))
}

/// The lockfile to use: `--lockfile`, else `<directory>/langfuse-skills.lock`
/// or `./langfuse-skills.lock` (the first that exists), else a new one in
/// `directory`. `None` with `--no-lockfile`.
pub fn resolve_path(
    cwd: &Path,
    directory: &Path,
    explicit: Option<&Path>,
    no_lockfile: bool,
) -> Result<Option<PathBuf>> {
    if explicit.is_some() && no_lockfile {
        return Err(CliError::usage(
            "Use either --lockfile or --no-lockfile, not both",
        ));
    }
    if no_lockfile {
        return Ok(None);
    }
    if explicit.is_some_and(|path| path.as_os_str().is_empty()) {
        return Err(CliError::usage("--lockfile requires a value"));
    }
    if let Some(explicit) = explicit {
        return canonical(&resolve(cwd, explicit)).map(Some);
    }
    let default = resolve(directory, LOCK_FILE);
    let mut candidates = vec![default.clone()];
    let in_cwd = resolve(cwd, LOCK_FILE);
    if in_cwd != default {
        candidates.push(in_cwd);
    }
    for candidate in candidates {
        match crate::skills::paths::exists(&candidate) {
            Ok(true) => return canonical(&candidate).map(Some),
            Ok(false) => {}
            Err(error) => {
                return Err(CliError::local(format!(
                    "Cannot check {}: {error}",
                    candidate.display()
                )));
            }
        }
    }
    canonical(&default).map(Some)
}

/// The real path of the lockfile's directory plus its name.
fn canonical(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new("/"));
    let name = path.file_name().unwrap_or_default();
    Ok(physical(parent)?.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locked(path: &str, name: &str, selector: Selector, version: i64) -> LockedSkill {
        LockedSkill {
            path: path.into(),
            name: SkillName::new(name).unwrap(),
            selector,
            version: Version::new(version).unwrap(),
        }
    }

    #[test]
    fn round_trip_is_sorted_and_strict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK_FILE);
        let mut lock = SkillsLock::default();
        let pinned = Selector::Version(Version::new(2).unwrap());
        record(&mut lock, &path, locked("z", "z", pinned.clone(), 2));
        let followed = Selector::Label("production".into());
        record(&mut lock, &path, locked("a", "a", followed.clone(), 1));
        write(&lock, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "{\n  \"lockVersion\": 1,\n  \"skills\": [\n    {\n      \"path\": \"a\",\n      \"name\": \"a\",\n      \
             \"selector\": {\n        \"label\": \"production\"\n      },\n      \"version\": 1\n    },\n    {\n      \
             \"path\": \"z\",\n      \"name\": \"z\",\n      \"selector\": {\n        \"version\": 2\n      },\n      \
             \"version\": 2\n    }\n  ]\n}\n"
        );
        assert_eq!(
            read(&path).unwrap().skills,
            [locked("a", "a", followed, 1), locked("z", "z", pinned, 2)]
        );
        let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 1, "the temporary file is gone");

        for (invalid, reason) in [
            ("not json", "expected"),
            ("[]", "the lockfile must be a JSON object"),
            ("[1,[]]", "the lockfile must be a JSON object"),
            (
                r#"{"lockVersion":1,"skills":[["a","a",{"label":"x"},1]]}"#,
                "skills[0] must be an object",
            ),
            (r#"{"lockVersion":2,"skills":[]}"#, "lockVersion must be 1"),
            (
                r#"{"lockVersion":"1","skills":[]}"#,
                "lockVersion must be 1",
            ),
            (r#"{"skills":[]}"#, "lockVersion"),
            (r#"{"lockVersion":1}"#, "skills"),
            (r#"{"lockVersion":1,"skills":{}}"#, "invalid type"),
            (
                r#"{"lockVersion":1,"skills":[],"extra":1}"#,
                "unknown field `extra`",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"label":"x"},"version":1,"extra":1}]}"#,
                "unknown field `extra`",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"","name":"a","selector":{"label":"x"},"version":1}]}"#,
                "non-empty string",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"name":"a","selector":{"label":"x"},"version":1}]}"#,
                "missing field `path`",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"A","selector":{"label":"x"},"version":1}]}"#,
                "expected a skill name",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"label":"x","version":1},"version":1}]}"#,
                "",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"label":""},"version":1}]}"#,
                "non-empty string",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"tag":"x"},"version":1}]}"#,
                "unknown variant `tag`",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"version":0},"version":1}]}"#,
                "positive version number",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"label":"x"},"version":-1}]}"#,
                "positive version number",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"label":"x"},"version":1.5}]}"#,
                "invalid type",
            ),
            (
                r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"label":"x"},"version":1},{"path":"a","name":"b","selector":{"label":"x"},"version":1}]}"#,
                "skill paths must be unique",
            ),
        ] {
            std::fs::write(&path, invalid).unwrap();
            let error = read(&path).unwrap_err();
            assert_eq!(error.exit, crate::error::Exit::Local, "{invalid}");
            let prefix = format!("Cannot read {}: ", path.display());
            assert!(error.message.starts_with(&prefix), "{}", error.message);
            assert!(
                error.message.contains(reason),
                "{invalid}: {}",
                error.message
            );
        }
        assert!(
            read(&dir.path().join("missing.lock"))
                .unwrap()
                .skills
                .is_empty()
        );
    }

    #[test]
    fn lock_path_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = std::fs::canonicalize(dir.path()).unwrap();
        let skills = cwd.join(".agents/skills");
        assert_eq!(
            resolve_path(&cwd, &skills, None, false).unwrap(),
            Some(skills.join(LOCK_FILE))
        );
        std::fs::write(cwd.join(LOCK_FILE), "{}").unwrap();
        assert_eq!(
            resolve_path(&cwd, &skills, None, false).unwrap(),
            Some(cwd.join(LOCK_FILE))
        );
        assert_eq!(resolve_path(&cwd, &skills, None, true).unwrap(), None);
        assert!(resolve_path(&cwd, &skills, Some(Path::new("x")), true).is_err());
        assert!(resolve_path(&cwd, &skills, Some(Path::new("")), false).is_err());
    }
}

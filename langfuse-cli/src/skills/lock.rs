//! `langfuse-skills.lock`: the installed or pushed skills of a project.
//!
//! Strict like the official CLI (unknown keys, a `lockVersion` other than 1,
//! invalid entries and duplicate paths are errors), and readable by
//! `langfuse_rust_sdk::skills::discover_skills`.

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::error::{CliError, Result};
use crate::skills::api::is_skill_name;
use crate::skills::paths::{physical, resolve};

/// The lockfile name.
pub const LOCK_FILE: &str = "langfuse-skills.lock";

/// What a lock entry follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// A label (`production`).
    Label(String),
    /// An immutable version.
    Version(i64),
}

impl Selector {
    fn to_json(&self) -> Value {
        match self {
            Selector::Label(label) => serde_json::json!({ "label": label }),
            Selector::Version(version) => serde_json::json!({ "version": version }),
        }
    }

    fn from_json(value: &Value) -> std::result::Result<Self, String> {
        let object = value.as_object().ok_or("selector must be an object")?;
        match (object.get("label"), object.get("version"), object.len()) {
            (Some(Value::String(label)), None, 1) if !label.is_empty() => Ok(Selector::Label(label.clone())),
            (None, Some(version), 1) => version
                .as_i64()
                .filter(|version| *version > 0)
                .map(Selector::Version)
                .ok_or_else(|| "selector.version must be a positive integer".to_owned()),
            _ => Err("selector must be {\"label\": <non-empty string>} or {\"version\": <positive integer>}".to_owned()),
        }
    }
}

/// One locked skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedSkill {
    /// Installation directory, relative to the lockfile, `/`-separated.
    pub path: String,
    /// Skill name.
    pub name: String,
    /// What updates follow.
    pub selector: Selector,
    /// The installed or pushed version.
    pub version: i64,
}

/// The lockfile contents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillsLock {
    /// Locked skills.
    pub skills: Vec<LockedSkill>,
}

fn parse_entry(index: usize, value: &Value) -> std::result::Result<LockedSkill, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("skills[{index}] must be an object"))?;
    if let Some(key) = object
        .keys()
        .find(|key| !["path", "name", "selector", "version"].contains(&key.as_str()))
    {
        return Err(format!("skills[{index}]: unrecognized key \"{key}\""));
    }
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| format!("skills[{index}].path must be a non-empty string"))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| is_skill_name(name))
        .ok_or_else(|| format!("skills[{index}].name must be a valid skill name"))?;
    let selector = Selector::from_json(object.get("selector").unwrap_or(&Value::Null))
        .map_err(|error| format!("skills[{index}].{error}"))?;
    let version = object
        .get("version")
        .and_then(Value::as_i64)
        .filter(|version| *version > 0)
        .ok_or_else(|| format!("skills[{index}].version must be a positive integer"))?;
    Ok(LockedSkill {
        path: path.to_owned(),
        name: name.to_owned(),
        selector,
        version,
    })
}

fn parse(text: &str) -> std::result::Result<SkillsLock, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let object = value
        .as_object()
        .ok_or("the lockfile must be a JSON object")?;
    if let Some(key) = object
        .keys()
        .find(|key| !["lockVersion", "skills"].contains(&key.as_str()))
    {
        return Err(format!("unrecognized key \"{key}\""));
    }
    if object.get("lockVersion").and_then(Value::as_i64) != Some(1) {
        return Err("lockVersion must be 1".to_owned());
    }
    let skills = object
        .get("skills")
        .and_then(Value::as_array)
        .ok_or("skills must be an array")?
        .iter()
        .enumerate()
        .map(|(index, entry)| parse_entry(index, entry))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    validate(&skills)?;
    Ok(SkillsLock { skills })
}

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

/// Record `skill`, replacing any entry for the same installation directory.
pub fn record(lock: &mut SkillsLock, lockfile: &Path, skill: LockedSkill) {
    let directory = lockfile.parent().unwrap_or(Path::new("/"));
    let destination = resolve(directory, &skill.path);
    lock.skills
        .retain(|existing| resolve(directory, &existing.path) != destination);
    lock.skills.push(skill);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LockFileJson {
    lock_version: u8,
    skills: Vec<Value>,
}

/// Write the lockfile atomically (temporary file, then rename), sorted by path.
pub fn write(lock: &SkillsLock, path: &Path) -> Result<()> {
    let fail = |error: &dyn std::fmt::Display| {
        CliError::local(format!("Cannot write {}: {error}", path.display()))
    };
    validate(&lock.skills).map_err(|error| fail(&error))?;
    let mut skills = lock.skills.clone();
    skills.sort_by(|a, b| a.path.cmp(&b.path));
    let document = LockFileJson {
        lock_version: 1,
        skills: skills
            .iter()
            .map(|skill| {
                serde_json::json!({
                    "path": skill.path,
                    "name": skill.name,
                    "selector": skill.selector.to_json(),
                    "version": skill.version,
                })
            })
            .collect(),
    };
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&document).map_err(|error| fail(&error))?
    );
    let directory = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(directory).map_err(|error| fail(&error))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = directory.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    let _ = std::fs::remove_file(&temporary);
    result.map_err(|error| fail(&error))
}

/// The lockfile to use: `--lockfile`, else `<directory>/langfuse-skills.lock`
/// or `./langfuse-skills.lock` (the first that exists), else a new one in
/// `directory`. `None` with `--no-lockfile`.
pub fn resolve_path(
    cwd: &Path,
    directory: &Path,
    explicit: Option<&str>,
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
    if explicit == Some("") {
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

    #[test]
    fn round_trip_is_sorted_and_strict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK_FILE);
        let mut lock = SkillsLock::default();
        record(
            &mut lock,
            &path,
            LockedSkill {
                path: "z".into(),
                name: "z".into(),
                selector: Selector::Version(2),
                version: 2,
            },
        );
        record(
            &mut lock,
            &path,
            LockedSkill {
                path: "a".into(),
                name: "a".into(),
                selector: Selector::Label("production".into()),
                version: 1,
            },
        );
        write(&lock, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "{\n  \"lockVersion\": 1,\n  \"skills\": [\n    {\n      \"path\": \"a\",\n      \"name\": \"a\",\n      \
             \"selector\": {\n        \"label\": \"production\"\n      },\n      \"version\": 1\n    },\n    {\n      \
             \"path\": \"z\",\n      \"name\": \"z\",\n      \"selector\": {\n        \"version\": 2\n      },\n      \
             \"version\": 2\n    }\n  ]\n}\n"
        );
        let read_back = read(&path).unwrap();
        assert_eq!(read_back.skills.len(), 2);

        for invalid in [
            r#"{"lockVersion":2,"skills":[]}"#,
            r#"{"lockVersion":1,"skills":[],"extra":1}"#,
            r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"label":"x","version":1},"version":1}]}"#,
            r#"{"lockVersion":1,"skills":[{"path":"a","name":"A","selector":{"label":"x"},"version":1}]}"#,
            r#"{"lockVersion":1,"skills":[{"path":"a","name":"a","selector":{"label":"x"},"version":1},{"path":"a","name":"b","selector":{"label":"x"},"version":1}]}"#,
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert_eq!(
                read(&path).unwrap_err().exit,
                crate::error::Exit::Local,
                "{invalid}"
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
        assert!(resolve_path(&cwd, &skills, Some("x"), true).is_err());
    }
}

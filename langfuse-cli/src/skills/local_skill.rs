//! Reading a local skill directory for `skills push`, and discovering skill
//! directories with `--recursive`.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{CliError, Result};
use crate::output::{Io, escape_terminal_text};
use crate::skills::api::{is_skill_name, sha256_base64};
use crate::skills::local::decode_skill_content;
use crate::skills::paths::contains;
use crate::skills::publish_filter::{
    Exclusion, GitSelection, git_excludes, publish_exclusion, read_git_selection,
};

const MAX_FILES: usize = 200;
const MAX_BYTES: u64 = 1_000_000;
const TEXT_EXTENSIONS: &[&str] = &[
    "md",
    "markdown",
    "mdx",
    "txt",
    "rst",
    "adoc",
    "json",
    "jsonc",
    "jsonl",
    "ndjson",
    "yaml",
    "yml",
    "toml",
    "xml",
    "csv",
    "tsv",
    "ini",
    "cfg",
    "conf",
    "properties",
    "log",
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    "js",
    "jsx",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "py",
    "pyi",
    "sh",
    "bash",
    "zsh",
    "fish",
    "ps1",
    "rb",
    "go",
    "rs",
    "java",
    "c",
    "h",
    "cc",
    "cpp",
    "hpp",
    "cs",
    "swift",
    "kt",
    "kts",
    "lua",
    "r",
    "pl",
    "php",
    "sql",
    "graphql",
    "gql",
    "tex",
];

/// One file to publish.
#[derive(Debug, Clone)]
pub struct LocalSkillFile {
    /// `/`-separated path relative to the skill.
    pub path: String,
    /// Text content (BOM stripped).
    pub content: String,
    /// Base64 SHA-256 of the content.
    pub sha256_hash: String,
    /// UTF-8 byte length of the content.
    pub content_length: u64,
}

/// A path left out of the snapshot.
#[derive(Debug, Clone)]
pub struct ExcludedPath {
    /// The path (directories end with `/`).
    pub path: String,
    /// Why.
    pub reason: Exclusion,
}

/// A local skill ready to publish.
#[derive(Debug, Clone)]
pub struct LocalSkill {
    /// Name from the SKILL.md frontmatter.
    pub name: String,
    /// Real path of the skill directory.
    pub directory: PathBuf,
    /// Files, sorted by path.
    pub files: Vec<LocalSkillFile>,
    /// Excluded paths, sorted.
    pub excluded: Vec<ExcludedPath>,
}

/// Why a skill could not be read.
#[derive(Debug)]
pub enum ReadError {
    /// Git could not be asked (never skipped).
    Git(CliError),
    /// The directory is not a valid skill (skipped by `--recursive`).
    Invalid(CliError),
}

impl From<ReadError> for CliError {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::Git(error) | ReadError::Invalid(error) => error,
        }
    }
}

fn canonical(path: &Path) -> std::result::Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn is_directory_within_root(root: &Path, directory: &Path) -> std::result::Result<bool, String> {
    let real = canonical(directory)?;
    let is_dir = std::fs::symlink_metadata(directory)
        .map_err(|error| error.to_string())?
        .is_dir();
    Ok(contains(root, &real) && is_dir)
}

fn notice(io: &mut Io<'_>, text: &str) {
    let _ = writeln!(io.stderr, "{text}");
}

/// The skill directories below `directory` (a directory with a SKILL.md is a
/// skill and is not descended into), sorted.
pub fn discover_skill_directories(
    directory: &Path,
    follow_symlinks: bool,
    io: &mut Io<'_>,
) -> Result<Vec<PathBuf>> {
    let fail = |message: String| {
        CliError::local(format!(
            "Cannot discover skills at {}: {message}",
            directory.display()
        ))
    };
    let git = |error: CliError| error;
    let source = if follow_symlinks {
        canonical(directory).map_err(fail)?
    } else {
        directory.to_path_buf()
    };
    let is_dir = std::fs::symlink_metadata(&source)
        .map(|meta| meta.is_dir())
        .map_err(|error| fail(error.to_string()))?;
    if !is_dir {
        return Err(fail(
            "The skills directory must be a real directory, not a symlink".to_owned(),
        ));
    }
    let root = canonical(&source).map_err(fail)?;
    let mut skills = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut pending: Vec<(PathBuf, Option<GitSelection>)> = vec![(source, None)];
    while let Some((current, parent_selection)) = pending.pop() {
        if !follow_symlinks && !is_directory_within_root(&root, &current).map_err(fail)? {
            return Err(fail(format!(
                "Directory must stay inside the skills root without symlinks: {}",
                current.display()
            )));
        }
        let real = canonical(&current).map_err(fail)?;
        if !visited.insert(real.clone()) {
            continue;
        }
        let mut entries = std::fs::read_dir(&real)
            .and_then(|entries| entries.collect::<std::io::Result<Vec<_>>>())
            .map_err(|error| fail(error.to_string()))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        let selection = read_git_selection(&real, parent_selection.as_ref()).map_err(git)?;

        if entries.iter().any(|entry| entry.file_name() == "SKILL.md") {
            if git_excludes(&selection, &real.join("SKILL.md")) {
                notice(
                    io,
                    &format!(
                        "Skipped {}: SKILL.md is Git-ignored",
                        escape_terminal_text(
                            &Value::from(current.display().to_string()).to_string()
                        )
                    ),
                );
            } else {
                skills.push(if follow_symlinks { real } else { current });
            }
            continue;
        }
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = current.join(&name);
            let kind = entry.file_type().map_err(|error| fail(error.to_string()))?;
            let reason = publish_exclusion(&name, &[]).or_else(|| {
                git_excludes(&selection, &real.join(&name)).then_some(Exclusion::GitIgnore)
            });
            if let Some(reason) = reason {
                if reason == Exclusion::GitIgnore && (kind.is_dir() || kind.is_symlink()) {
                    notice(
                        io,
                        &format!(
                            "Skipped {}: Git-ignored",
                            escape_terminal_text(
                                &Value::from(path.display().to_string()).to_string()
                            )
                        ),
                    );
                }
                continue;
            }
            if kind.is_symlink() {
                if follow_symlinks {
                    match std::fs::canonicalize(&path) {
                        Ok(target) if target.is_dir() => {
                            pending.push((target, Some(selection.clone())))
                        }
                        Ok(_) => notice(
                            io,
                            &format!(
                                "Skipped {}: only directory symlinks are supported",
                                escape_terminal_text(&path.display().to_string())
                            ),
                        ),
                        Err(error) => notice(
                            io,
                            &escape_terminal_text(&format!(
                                "Skipped {}: cannot resolve directory symlink: {error}",
                                path.display()
                            )),
                        ),
                    }
                } else {
                    notice(
                        io,
                        &format!(
                            "Skipped {}: symlinks are not supported",
                            escape_terminal_text(&path.display().to_string())
                        ),
                    );
                }
                continue;
            }
            if kind.is_dir() {
                pending.push((path, Some(selection.clone())));
            }
        }
    }
    skills.sort();
    Ok(skills)
}

fn validate_file_path(path: &str) -> std::result::Result<(), String> {
    let bad_segment = path
        .split('/')
        .any(|segment| segment == "." || segment == "..");
    let bad_char = path
        .chars()
        .any(|c| c == '\\' || (c as u32) < 0x20 || c == '\x7f');
    if path.len() > 512 || bad_char || bad_segment {
        return Err(format!("Invalid skill file path: {path}"));
    }
    Ok(())
}

fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

fn read_skill_file(directory: &Path, path: &str) -> std::result::Result<LocalSkillFile, String> {
    let filename = path.rsplit('/').next().unwrap_or(path);
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    if !TEXT_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!("Unsupported text file extension: {path}"));
    }
    let file = open_no_follow(&directory.join(path)).map_err(|error| format!("{path}: {error}"))?;
    let meta = file
        .metadata()
        .map_err(|error| format!("{path}: {error}"))?;
    if !meta.is_file() || meta.len() > MAX_BYTES + 3 {
        return Err(format!(
            "Skill file exceeds {MAX_BYTES} UTF-8 bytes: {path}"
        ));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_BYTES + 4)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{path}: {error}"))?;
    let content = decode_skill_content(&bytes)
        .ok_or_else(|| format!("{path}: The encoded data was not valid for encoding utf-8"))?;
    if content.contains('\0') {
        return Err(format!("Skill file contains NUL characters: {path}"));
    }
    Ok(LocalSkillFile {
        path: path.to_owned(),
        sha256_hash: sha256_base64(content.as_bytes()),
        content_length: content.len() as u64,
        content,
    })
}

fn read_skill_files(
    directory: &Path,
    exclusions: &[String],
) -> std::result::Result<(Vec<LocalSkillFile>, Vec<ExcludedPath>), ReadError> {
    let invalid = |message: String| ReadError::Invalid(CliError::local(message));
    let is_dir = std::fs::symlink_metadata(directory)
        .map(|meta| meta.is_dir())
        .map_err(|error| invalid(error.to_string()))?;
    if !is_dir {
        return Err(invalid(
            "The skill directory must be a real directory, not a symlink".to_owned(),
        ));
    }
    let root = canonical(directory).map_err(invalid)?;
    let mut pending: Vec<(String, Option<GitSelection>)> = vec![(String::new(), None)];
    let mut files = Vec::new();
    let mut excluded = Vec::new();
    let mut total = 0u64;
    while let Some((parent, parent_selection)) = pending.pop() {
        let disk = if parent.is_empty() {
            directory.to_path_buf()
        } else {
            directory.join(&parent)
        };
        if !is_directory_within_root(&root, &disk).map_err(invalid)? {
            return Err(invalid(format!(
                "Directory must stay inside the skill root without symlinks: {parent}"
            )));
        }
        let real = canonical(&disk).map_err(invalid)?;
        let selection =
            read_git_selection(&real, parent_selection.as_ref()).map_err(ReadError::Git)?;
        let entries = std::fs::read_dir(&disk)
            .and_then(|entries| entries.collect::<std::io::Result<Vec<_>>>())
            .map_err(|error| invalid(error.to_string()))?;
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if parent.is_empty() {
                name
            } else {
                format!("{parent}/{name}")
            };
            let kind = entry
                .file_type()
                .map_err(|error| invalid(error.to_string()))?;
            let reason = publish_exclusion(&path, exclusions).or_else(|| {
                git_excludes(&selection, &root.join(&path)).then_some(Exclusion::GitIgnore)
            });
            if let Some(reason) = reason {
                excluded.push(ExcludedPath {
                    path: if kind.is_dir() {
                        format!("{path}/")
                    } else {
                        path
                    },
                    reason,
                });
                continue;
            }
            validate_file_path(&path).map_err(invalid)?;
            if kind.is_dir() {
                pending.push((path, Some(selection.clone())));
                continue;
            }
            if !kind.is_file() {
                return Err(invalid(format!(
                    "Skill files must be regular files, not symlinks: {path}"
                )));
            }
            if files.len() >= MAX_FILES {
                return Err(invalid(format!(
                    "A skill must not exceed {MAX_FILES} files"
                )));
            }
            let file = read_skill_file(directory, &path).map_err(invalid)?;
            total += file.content_length;
            if total > MAX_BYTES {
                return Err(invalid(format!(
                    "A skill must not exceed {MAX_BYTES} UTF-8 bytes in total"
                )));
            }
            files.push(file);
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    excluded.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((files, excluded))
}

/// The YAML frontmatter of a SKILL.md: `^---\r?\n(…)\r?\n---(\r?\n|$)`.
pub fn frontmatter(content: &str) -> Option<&str> {
    let start = if content.starts_with("---\n") {
        4
    } else if content.starts_with("---\r\n") {
        5
    } else {
        return None;
    };
    let mut from = start;
    while let Some(offset) = content.get(from..)?.find("\n---") {
        let newline = from + offset;
        let after = &content[newline + 4..];
        if after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n") {
            let end = if newline > start && content.as_bytes()[newline - 1] == b'\r' {
                newline - 1
            } else {
                newline
            };
            return Some(&content[start..end.max(start)]);
        }
        from = newline + 1;
    }
    None
}

fn read_skill_name(files: &[LocalSkillFile]) -> std::result::Result<String, String> {
    let skill = files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .ok_or("Every skill must contain a root SKILL.md file")?;
    let yaml = frontmatter(&skill.content)
        .ok_or("SKILL.md must start with YAML frontmatter delimited by ---")?;
    let value: serde_norway::Value = serde_norway::from_str(yaml)
        .map_err(|error| format!("Invalid SKILL.md frontmatter: {error}"))?;
    let mapping = value
        .as_mapping()
        .ok_or("SKILL.md frontmatter must be an object")?;
    let name = mapping
        .get("name")
        .and_then(serde_norway::Value::as_str)
        .map(str::trim);
    let name = name
        .filter(|name| name.chars().count() <= 64 && is_skill_name(name))
        .ok_or("SKILL.md requires a lowercase skill name of at most 64 characters")?;
    let description = mapping
        .get("description")
        .and_then(serde_norway::Value::as_str)
        .map(str::trim);
    if !description
        .is_some_and(|description| !description.is_empty() && description.chars().count() <= 1024)
    {
        return Err("SKILL.md requires a description of 1 to 1024 characters".to_owned());
    }
    Ok(name.to_owned())
}

/// Read the skill at `directory`.
pub fn read_local_skill(
    directory: &Path,
    follow_symlinks: bool,
    exclusions: &[String],
) -> std::result::Result<LocalSkill, ReadError> {
    let wrap = |message: String| {
        ReadError::Invalid(CliError::local(format!(
            "Cannot read skill at {}: {message}",
            directory.display()
        )))
    };
    let source = if follow_symlinks {
        canonical(directory).map_err(wrap)?
    } else {
        directory.to_path_buf()
    };
    let (files, excluded) = match read_skill_files(&source, exclusions) {
        Ok(read) => read,
        Err(ReadError::Git(error)) => return Err(ReadError::Git(error)),
        Err(ReadError::Invalid(error)) => return Err(wrap(error.message)),
    };
    let name = read_skill_name(&files).map_err(wrap)?;
    Ok(LocalSkill {
        name,
        directory: canonical(&source).map_err(wrap)?,
        files,
        excluded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_delimiters() {
        assert_eq!(frontmatter("---\nname: a\n---\nbody"), Some("name: a"));
        assert_eq!(frontmatter("---\r\nname: a\r\n---\r\n"), Some("name: a"));
        assert_eq!(frontmatter("---\nname: a\n---"), Some("name: a"));
        assert_eq!(frontmatter("---\n\n---\n"), Some(""));
        assert_eq!(
            frontmatter("---\nname: a\n----\nx: 1\n---\n"),
            Some("name: a\n----\nx: 1")
        );
        assert_eq!(frontmatter("name: a\n---\n"), None);
        assert_eq!(frontmatter("---\n---\n"), None);
    }

    fn skill(content: &str) -> std::result::Result<String, String> {
        read_skill_name(&[LocalSkillFile {
            path: "SKILL.md".into(),
            content: content.into(),
            sha256_hash: String::new(),
            content_length: 0,
        }])
    }

    #[test]
    fn skill_names_from_frontmatter() {
        assert_eq!(
            skill("---\nname: ' my-skill '\ndescription: Does things\n---\n").unwrap(),
            "my-skill"
        );
        assert!(skill("---\nname: My\ndescription: x\n---\n").is_err());
        assert!(skill("---\nname: ok\n---\n").is_err());
        assert!(skill("---\n- a\n---\n").is_err());
        assert!(skill("no frontmatter").is_err());
    }
}

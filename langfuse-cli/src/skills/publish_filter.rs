//! Which files of a skill directory `skills push` publishes: Git's view of
//! the checkout (tracked plus non-ignored untracked files), `--exclude`, and
//! never Git metadata.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::error::{CliError, Result};
use crate::skills::paths::{contains, relative, to_slash};

const GIT_TIMEOUT: Duration = Duration::from_secs(10);
const GIT_MAX_OUTPUT: usize = 16 * 1024 * 1024;

/// Why a path is not published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    /// Ignored and untracked in Git.
    GitIgnore,
    /// Matched by `--exclude`.
    Exclude,
    /// `.git` or `.gitignore`.
    GitMetadata,
}

impl Exclusion {
    /// The name used in output.
    pub fn as_str(self) -> &'static str {
        match self {
            Exclusion::GitIgnore => "gitignore",
            Exclusion::Exclude => "exclude",
            Exclusion::GitMetadata => "git-metadata",
        }
    }
}

/// The files Git lists below `directory` (and their parent directories), or
/// `None` outside a Git checkout.
#[derive(Debug, Clone)]
pub struct GitSelection {
    directory: PathBuf,
    paths: Option<Rc<HashSet<String>>>,
}

/// A Git failure; never skipped, even by `--recursive`.
pub fn git_error(directory: &Path, error: impl std::fmt::Display) -> CliError {
    CliError::local(format!(
        "Cannot apply Git ignore rules at {}; check that Git is installed and the checkout is readable: {error}",
        directory.display()
    ))
}

fn has_git_metadata(directory: &Path) -> std::io::Result<bool> {
    crate::skills::paths::exists(&directory.join(".git"))
}

/// The selection for `directory`, reusing `parent`'s unless `directory`
/// starts a nested checkout.
pub fn read_git_selection(directory: &Path, parent: Option<&GitSelection>) -> Result<GitSelection> {
    let fail = |error: &dyn std::fmt::Display| git_error(directory, error);
    if let Some(parent) = parent
        && contains(&parent.directory, directory)
        && !has_git_metadata(directory).map_err(|error| fail(&error))?
    {
        return Ok(parent.clone());
    }
    let mut ancestor = directory.to_path_buf();
    while !has_git_metadata(&ancestor).map_err(|error| fail(&error))? {
        match ancestor.parent() {
            Some(next) => ancestor = next.to_path_buf(),
            None => {
                return Ok(GitSelection {
                    directory: directory.to_path_buf(),
                    paths: None,
                });
            }
        }
    }
    let output = git_ls_files(directory).map_err(|error| fail(&error))?;
    let mut paths = HashSet::new();
    for path in output.split('\0').filter(|path| !path.is_empty()) {
        let segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        for length in 1..=segments.len() {
            paths.insert(segments[..length].join("/"));
        }
    }
    Ok(GitSelection {
        directory: directory.to_path_buf(),
        paths: Some(Rc::new(paths)),
    })
}

fn git_ls_files(directory: &Path) -> std::result::Result<String, String> {
    let mut command = Command::new("git");
    command
        .args([
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            ".",
        ])
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Inspect this checkout even when invoked from a Git hook.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command.env("GIT_OPTIONAL_LOCKS", "0");
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let result = (&mut stdout)
            .take(GIT_MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut buffer);
        result.map(|_| buffer)
    });
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => break status,
            None if started.elapsed() > GIT_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "git ls-files timed out after {}s",
                    GIT_TIMEOUT.as_secs()
                ));
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "git output reader panicked".to_owned())?
        .map_err(|error| error.to_string())?;
    let stderr_text = errors.join().unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "git ls-files failed ({status}): {}",
            stderr_text.trim()
        ));
    }
    if bytes.len() > GIT_MAX_OUTPUT {
        return Err("git ls-files output exceeds 16 MiB".to_owned());
    }
    String::from_utf8(bytes).map_err(|_| "git ls-files printed a non-UTF-8 path".to_owned())
}

/// Whether Git ignores `path` (absolute) in `selection`.
pub fn git_excludes(selection: &GitSelection, path: &Path) -> bool {
    selection
        .paths
        .as_ref()
        .is_some_and(|paths| !paths.contains(&to_slash(&relative(&selection.directory, path))))
}

/// Validate `--exclude` values: relative paths, `./` and trailing `/` dropped.
pub fn parse_exclusions(values: &[String]) -> Result<Vec<String>> {
    values
        .iter()
        .map(|value| {
            let mut path = value.as_str();
            while let Some(rest) = path.strip_prefix("./") {
                path = rest;
            }
            let path = path.trim_end_matches('/');
            let drive = path.len() >= 2
                && path.as_bytes()[0].is_ascii_alphabetic()
                && path.as_bytes()[1] == b':';
            let bad_char = path
                .chars()
                .any(|c| c == '\\' || (c as u32) < 0x20 || c == '\x7f');
            if path.is_empty()
                || drive
                || bad_char
                || path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(CliError::usage(
                    "--exclude requires a relative file or directory path within each skill",
                ));
            }
            Ok(path.to_owned())
        })
        .collect()
}

/// Why `path` (relative to the skill, `/`-separated) is never published.
pub fn publish_exclusion(path: &str, exclusions: &[String]) -> Option<Exclusion> {
    if path
        .split('/')
        .any(|part| part.eq_ignore_ascii_case(".git") || part.eq_ignore_ascii_case(".gitignore"))
    {
        return Some(Exclusion::GitMetadata);
    }
    if exclusions
        .iter()
        .any(|excluded| path == excluded || path.starts_with(&format!("{excluded}/")))
    {
        return Some(Exclusion::Exclude);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusions() {
        assert_eq!(
            parse_exclusions(&["./private/".into(), "a.json".into()]).unwrap(),
            ["private", "a.json"]
        );
        for bad in ["", "/abs", "../x", "a//b", "C:x", "a\\b", "./", "a/./b"] {
            assert!(parse_exclusions(&[bad.into()]).is_err(), "{bad:?}");
        }
        assert_eq!(
            publish_exclusion("docs/.GIT/x", &[]),
            Some(Exclusion::GitMetadata)
        );
        assert_eq!(
            publish_exclusion(".gitignore", &[]),
            Some(Exclusion::GitMetadata)
        );
        assert_eq!(
            publish_exclusion("private/key.json", &["private".into()]),
            Some(Exclusion::Exclude)
        );
        assert_eq!(publish_exclusion("privately.md", &["private".into()]), None);
    }
}

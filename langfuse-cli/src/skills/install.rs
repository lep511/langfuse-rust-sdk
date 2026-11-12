//! `langfuse skills install` and `langfuse skills update`.

use std::collections::HashSet;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use futures_util::stream::{self, StreamExt as _, TryStreamExt as _};
use serde::Serialize;
use serde_json::Value;

use crate::config::Settings;
use crate::error::{CliError, Exit, Result};
use crate::output::{Io, escape_terminal_text, json_line};
use crate::skills::api::{REQUEST_CONCURRENCY, Remote, SkillFile, SkillVersion, sha256_base64};
use crate::skills::local::matches_manifest;
use crate::skills::lock::{self, LockedSkill, Selector, SkillsLock};
use crate::skills::paths::{Temporary, exists, physical, resolve, sibling};
use crate::skills::types::{SkillName, Version};
use crate::skills::{InstallArgs, UpdateArgs, directories};

/// Default parent directory of installed skills.
pub const DEFAULT_DIRECTORY: &str = ".agents/skills";

/// Which command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Update,
}

/// Validated options of install and update.
#[derive(Debug)]
struct Options {
    name: Option<SkillName>,
    tag: Option<String>,
    all: bool,
    version: Option<Version>,
    label: Option<String>,
    directory: PathBuf,
    lockfile: Option<PathBuf>,
    force: bool,
}

/// The largest integer a JavaScript number holds exactly (the official CLI's
/// "safe integer" bound on versions).
const MAX_SAFE_INTEGER: i64 = (1 << 53) - 1;

fn parse_options(raw: &InstallArgs, action: Action, cwd: &Path) -> Result<Options> {
    let selectors =
        usize::from(raw.name.is_some()) + usize::from(raw.tag.is_some()) + usize::from(raw.all);
    if selectors > 1 {
        return Err(CliError::usage(
            "Use a skill name, --tag <tag>, or --all, not more than one",
        ));
    }
    let mut name = raw.name.as_deref();
    let mut version = raw.version.as_deref();
    let mut label = raw.label.as_deref();
    if let Some((skill, selector)) = name.and_then(|full| full.split_once('@')) {
        if action == Action::Update {
            return Err(CliError::usage(
                "skills update uses the saved selector; use skills install to select a label or version",
            ));
        }
        if label.is_some() || version.is_some() {
            return Err(CliError::usage(
                "Use either name@selector or --label/--version, not both",
            ));
        }
        if selector.is_empty() {
            return Err(CliError::usage("Provide a label or version after @"));
        }
        if selector.bytes().all(|byte| byte.is_ascii_digit()) {
            version = Some(selector);
        } else {
            label = Some(selector);
        }
        name = Some(skill);
    }
    if raw.no_lockfile && (action == Action::Update || selectors == 0) {
        return Err(CliError::usage(
            "--no-lockfile requires skills install with a name, --tag, or --all; bare install and update \
             require the lockfile",
        ));
    }
    let name = name
        .map(|name| {
            SkillName::new(name).ok_or_else(|| {
                CliError::usage(
                    "Skill names may only contain lowercase letters, numbers, and single hyphens",
                )
            })
        })
        .transpose()?;
    for (option, empty) in [
        ("tag", raw.tag.as_deref() == Some("")),
        ("version", version == Some("")),
        ("label", label == Some("")),
        (
            "directory",
            raw.directory
                .as_deref()
                .is_some_and(|directory| directory.as_os_str().is_empty()),
        ),
    ] {
        if empty {
            return Err(CliError::usage(format!("--{option} requires a value")));
        }
    }
    if selectors == 0 {
        for (option, given) in [("version", version.is_some()), ("label", label.is_some())] {
            if given {
                return Err(CliError::usage(format!(
                    "--{option} requires a skill name, --tag, or --all; bare install uses the lockfile"
                )));
            }
        }
    }
    let version = version
        .map(|text| {
            text.parse::<i64>()
                .ok()
                .filter(|version| *version <= MAX_SAFE_INTEGER)
                .and_then(Version::new)
                .ok_or_else(|| CliError::usage("Skill version must be a positive safe integer"))
        })
        .transpose()?;
    if version.is_some() && label.is_some() {
        return Err(CliError::usage("Use either --version or --label, not both"));
    }
    if version.is_some() && name.is_none() {
        return Err(CliError::usage(
            "--version requires a skill name; use --label with --tag or --all",
        ));
    }
    let directory = resolve(
        cwd,
        raw.directory
            .as_deref()
            .unwrap_or(Path::new(DEFAULT_DIRECTORY)),
    );
    let lockfile = lock::resolve_path(cwd, &directory, raw.lockfile.as_deref(), raw.no_lockfile)?;
    Ok(Options {
        name,
        tag: raw.tag.clone(),
        all: raw.all,
        version,
        label: label.map(str::to_owned),
        directory,
        lockfile,
        force: raw.force,
    })
}

struct Target {
    name: SkillName,
    destination: PathBuf,
    selector: Selector,
    request: Selector,
    previous_version: Option<Version>,
}

/// The skills to install, and the names of those skipped for having no
/// production version.
async fn resolve_targets(
    remote: &Remote,
    options: &Options,
    lock: &SkillsLock,
    action: Action,
) -> Result<(Vec<Target>, Vec<SkillName>)> {
    if action == Action::Update || (options.name.is_none() && options.tag.is_none() && !options.all)
    {
        let lockfile = options
            .lockfile
            .as_deref()
            .ok_or_else(|| CliError::usage("Bare install and update require a lockfile"))?;
        let entries: Vec<&LockedSkill> = lock
            .skills
            .iter()
            .filter(|skill| options.name.as_ref().is_none_or(|name| &skill.name == name))
            .collect();
        if entries.is_empty() {
            return Err(CliError::usage(match &options.name {
                Some(name) => format!("No installation of {name} found in {}", lockfile.display()),
                None => format!(
                    "No skills recorded in {}; install a skill by name, --tag, or --all first",
                    lockfile.display()
                ),
            }));
        }
        let base = lockfile.parent().unwrap_or(Path::new("/"));
        let mut destinations = HashSet::new();
        let mut targets = Vec::new();
        for skill in entries {
            let destination = resolve(base, &skill.path);
            if Path::new(&skill.path).is_absolute() || !destinations.insert(destination.clone()) {
                return Err(CliError::local(format!(
                    "Invalid or duplicate installation directory in lockfile: {}",
                    skill.path
                )));
            }
            targets.push(Target {
                name: skill.name.clone(),
                destination,
                selector: skill.selector.clone(),
                request: if action == Action::Update {
                    skill.selector.clone()
                } else {
                    Selector::Version(skill.version)
                },
                previous_version: Some(skill.version),
            });
        }
        return Ok((targets, Vec::new()));
    }

    let selector = match (options.version, &options.label) {
        (Some(version), _) => Selector::Version(version),
        (None, Some(label)) => Selector::Label(label.clone()),
        (None, None) => Selector::Label("production".to_owned()),
    };
    let mut names = Vec::new();
    let mut skipped = Vec::new();
    if options.all || options.tag.is_some() {
        for skill in remote.fetch_skills(options.tag.as_deref()).await? {
            if options.label.is_none() && skill.production_version.is_none() {
                skipped.push(skill.name);
            } else {
                names.push(skill.name);
            }
        }
    } else if let Some(name) = &options.name {
        names.push(name.clone());
    }
    let targets = names
        .into_iter()
        .map(|name| {
            Ok(Target {
                destination: physical(&options.directory.join(name.as_str()))?,
                name,
                selector: selector.clone(),
                request: selector.clone(),
                previous_version: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((targets, skipped))
}

/// The destination of `file_path` inside `directory`; exit 7 for paths that
/// are empty, contain `.`/`..`/backslashes/NUL, or escape the directory.
fn resolve_file_path(directory: &Path, file_path: &str) -> Result<PathBuf> {
    let unsafe_path = || {
        CliError::response(format!(
            "Skill contains unsafe file path: {}",
            Value::from(file_path)
        ))
    };
    if file_path.contains('\\')
        || file_path.contains('\0')
        || file_path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(unsafe_path());
    }
    let destination = resolve(directory, file_path);
    if destination == directory || !destination.starts_with(directory) {
        return Err(unsafe_path());
    }
    Ok(destination)
}

fn validate_file_paths(files: &[SkillFile], directory: &Path) -> Result<()> {
    if !files.iter().any(|file| file.path == "SKILL.md") {
        return Err(CliError::response(
            "Langfuse skill manifest has no root SKILL.md",
        ));
    }
    let mut paths = HashSet::new();
    for file in files {
        if !paths.insert(resolve_file_path(directory, &file.path)?) {
            return Err(CliError::response(format!(
                "Skill manifest contains duplicate path: {}",
                file.path
            )));
        }
    }
    Ok(())
}

/// A skill ready to install: its manifest resolved and its destination
/// checked, before any installation changes.
struct Installation {
    skill: SkillVersion,
    destination: PathBuf,
    selector: Selector,
    /// The `path` of the lock entry, when there is a lockfile.
    lock_path: Option<String>,
    exists: bool,
    unchanged: bool,
}

async fn plan(
    remote: &Remote,
    target: Target,
    options: &Options,
    action: Action,
) -> Result<Installation> {
    let lock_path = options
        .lockfile
        .as_deref()
        .map(|lockfile| lock::key(lockfile, &target.destination))
        .transpose()?;
    let exists = exists(&target.destination).map_err(|error| {
        CliError::local(format!(
            "Cannot check {}: {error}",
            target.destination.display()
        ))
    })?;
    let skill = remote.fetch_skill(&target.name, &target.request).await?;
    validate_file_paths(&skill.files, &target.destination)?;
    let unchanged = exists && matches_manifest(&target.destination, &skill.files)?;
    if exists && !unchanged && !options.force {
        // Updates may replace a clean copy of the locked version, never local edits.
        let clean_update = match target.previous_version {
            Some(previous) if action == Action::Update && previous != skill.version => {
                let previous = remote
                    .fetch_skill(&target.name, &Selector::Version(previous))
                    .await?;
                matches_manifest(&target.destination, &previous.files)?
            }
            _ => false,
        };
        if !clean_update {
            return Err(CliError::local(format!(
                "{} contains different files; pass --force to replace it",
                target.destination.display()
            )));
        }
    }
    Ok(Installation {
        skill,
        destination: target.destination,
        selector: target.selector,
        lock_path,
        exists,
        unchanged,
    })
}

async fn install_files(
    remote: &Remote,
    files: &[SkillFile],
    destination: &Path,
    replace: bool,
) -> Result<()> {
    let parent = destination.parent().unwrap_or(Path::new("/"));
    std::fs::create_dir_all(parent).map_err(cannot_install)?;
    // Dropped, and so removed, however this function ends; after the swap
    // below it is the destination and there is nothing left to remove.
    let staging = Temporary::new(sibling(destination, "install"));
    std::fs::create_dir(staging.path()).map_err(cannot_install)?;
    download_files(remote, files, staging.path()).await?;
    // Names as stored by the filesystem, directory spelling included.
    if !matches_manifest(staging.path(), files)? {
        return Err(CliError::response(
            "Skill file paths conflict on this filesystem",
        ));
    }
    replace_directory(staging.path(), destination, replace).map_err(cannot_install)
}

fn cannot_install(error: std::io::Error) -> CliError {
    CliError::local(format!("Cannot install skill: {error}"))
}

async fn download_files(remote: &Remote, files: &[SkillFile], directory: &Path) -> Result<()> {
    let mut hashes: Vec<String> = Vec::new();
    for file in files {
        if !hashes.contains(&file.sha256_hash) {
            hashes.push(file.sha256_hash.clone());
        }
    }
    let contents = remote.fetch_all_contents(&hashes).await?;
    for file in files {
        let content = contents.get(&file.sha256_hash).ok_or_else(|| {
            CliError::response("Langfuse returned incomplete skill file contents")
        })?;
        write_verified_file(directory, file, content)?;
    }
    Ok(())
}

fn write_verified_file(directory: &Path, file: &SkillFile, content: &str) -> Result<()> {
    let bytes = content.as_bytes();
    if u64::try_from(bytes.len()).ok() != Some(file.content_length) {
        return Err(CliError::response(format!(
            "Downloaded size mismatch for {}: expected {}, got {}",
            file.path,
            file.content_length,
            bytes.len()
        )));
    }
    if sha256_base64(bytes) != file.sha256_hash {
        return Err(CliError::response(format!(
            "Downloaded checksum mismatch for {}",
            file.path
        )));
    }
    let destination = resolve_file_path(directory, &file.path)?;
    let conflict = || {
        CliError::response(format!(
            "Skill file path conflicts on this filesystem: {}",
            file.path
        ))
    };
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| match error.kind() {
            ErrorKind::AlreadyExists | ErrorKind::NotADirectory => conflict(),
            _ => cannot_install(error),
        })?;
    }
    let mut handle = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| match error.kind() {
            ErrorKind::AlreadyExists | ErrorKind::NotADirectory => conflict(),
            _ => cannot_install(error),
        })?;
    handle.write_all(bytes).map_err(cannot_install)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o644))
            .map_err(cannot_install)?;
    }
    Ok(())
}

/// Swap the staging directory in, keeping a backup until the swap succeeded.
///
/// The backup is deliberately not a drop guard: if the swap fails and the old
/// copy cannot be put back either, the backup is the only copy left and must
/// stay on disk.
fn replace_directory(staging: &Path, destination: &Path, replace: bool) -> std::io::Result<()> {
    if !replace {
        return std::fs::rename(staging, destination);
    }
    let backup = sibling(destination, "backup");
    std::fs::rename(destination, &backup)?;
    if let Err(error) = std::fs::rename(staging, destination) {
        std::fs::rename(&backup, destination)?;
        return Err(error);
    }
    let _ = std::fs::remove_dir_all(&backup);
    Ok(())
}

/// The `status` of a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Installed,
    Unchanged,
    Skipped,
}

/// Why a skill was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Reason {
    NoProductionVersion,
}

/// One result of the `--json` output. The order of the fields is the order
/// of the keys, which is the official CLI's.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Report {
    /// `--tag` or `--all` met a skill without a production version.
    Skipped {
        name: SkillName,
        status: Status,
        reason: Reason,
    },
    /// An installed skill, or one that was already current.
    Installed {
        name: SkillName,
        version: Version,
        labels: Vec<String>,
        directory: String,
        files: usize,
        status: Status,
    },
}

impl Report {
    /// The line of the text output (skipped skills are announced on stderr).
    fn line(&self) -> Option<String> {
        match self {
            Report::Skipped { .. } => None,
            Report::Installed {
                name,
                version,
                directory,
                files,
                status,
                ..
            } => {
                let directory = escape_terminal_text(directory);
                Some(if *status == Status::Unchanged {
                    format!("Already up to date: {name}@{version} at {directory}")
                } else {
                    format!("Installed {name}@{version} to {directory} ({files} files)")
                })
            }
        }
    }
}

/// Run install (`args`) or update (converted to the same options).
pub async fn run(
    settings: Settings,
    args: &InstallArgs,
    action: Action,
    json_output: bool,
    io: &mut Io<'_>,
) -> Result<Exit> {
    let options = parse_options(args, action, &io.cwd)?;
    let remote = Remote::new(settings);
    let mut lock = match &options.lockfile {
        Some(path) => lock::read(path)?,
        None => SkillsLock::default(),
    };
    let (targets, skipped) = resolve_targets(&remote, &options, &lock, action).await?;
    for name in &skipped {
        let _ = writeln!(
            io.stderr,
            "Skipped {}: no production version",
            escape_terminal_text(name.as_str())
        );
    }
    let destinations: Vec<PathBuf> = targets
        .iter()
        .map(|target| target.destination.clone())
        .collect();
    directories::validate(&io.cwd, &destinations, &lock, options.lockfile.as_deref())?;

    // Resolve every manifest and check every destination before changing any
    // installation. The requests overlap; the results keep the targets' order.
    let installations: Vec<Installation> = stream::iter(targets)
        .map(|target| plan(&remote, target, &options, action))
        .buffered(REQUEST_CONCURRENCY)
        .try_collect()
        .await?;

    let mut reports: Vec<Report> = skipped
        .into_iter()
        .map(|name| Report::Skipped {
            name,
            status: Status::Skipped,
            reason: Reason::NoProductionVersion,
        })
        .collect();
    let first_installed = reports.len();
    for installation in installations {
        let Installation {
            skill,
            destination,
            selector,
            lock_path,
            exists,
            unchanged,
        } = installation;
        if !unchanged {
            install_files(&remote, &skill.files, &destination, exists).await?;
        }
        if let (Some(lockfile), Some(path)) = (&options.lockfile, lock_path) {
            lock::record(
                &mut lock,
                lockfile,
                LockedSkill {
                    path,
                    name: skill.name.clone(),
                    selector,
                    version: skill.version,
                },
            );
            lock::write(&lock, lockfile)?;
        }
        let report = Report::Installed {
            name: skill.name,
            version: skill.version,
            labels: skill.labels,
            directory: destination.display().to_string(),
            files: skill.files.len(),
            status: if unchanged {
                Status::Unchanged
            } else {
                Status::Installed
            },
        };
        if !json_output && let Some(line) = report.line() {
            io.status(&line)?;
        }
        reports.push(report);
    }

    if json_output {
        let line = if action == Action::Install && options.name.is_some() {
            // One named skill: its result alone, not a list.
            json_line(&reports.get(first_installed))?
        } else {
            json_line(&reports)?
        };
        io.status(&escape_terminal_text(&line))?;
    } else if reports.is_empty() {
        let message = if options.all {
            "No skills found in this Langfuse project".to_owned()
        } else {
            format!(
                "No skills found with tag {}",
                escape_terminal_text(
                    &Value::from(options.tag.as_deref().unwrap_or_default()).to_string()
                )
            )
        };
        io.status(&message)?;
    }
    Ok(Exit::Success)
}

/// The install options equivalent to `skills update`.
pub fn update_as_install(args: &UpdateArgs) -> InstallArgs {
    InstallArgs {
        name: args.name.clone(),
        directory: args.directory.clone(),
        lockfile: args.lockfile.clone(),
        force: args.force,
        ..InstallArgs::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(name: Option<&str>) -> InstallArgs {
        InstallArgs {
            name: name.map(str::to_owned),
            ..InstallArgs::default()
        }
    }

    #[test]
    fn option_validation_messages() {
        let cwd = std::fs::canonicalize(tempfile::tempdir().unwrap().path())
            .unwrap_or_else(|_| "/tmp".into());
        let error =
            |raw: InstallArgs, action| parse_options(&raw, action, &cwd).unwrap_err().message;
        assert_eq!(
            error(
                InstallArgs {
                    all: true,
                    ..args(Some("a"))
                },
                Action::Install
            ),
            "Use a skill name, --tag <tag>, or --all, not more than one"
        );
        assert_eq!(
            error(args(Some("a@")), Action::Install),
            "Provide a label or version after @"
        );
        assert_eq!(
            error(
                InstallArgs {
                    label: Some("x".into()),
                    ..args(Some("a@3"))
                },
                Action::Install
            ),
            "Use either name@selector or --label/--version, not both"
        );
        assert_eq!(
            error(args(Some("a@3")), Action::Update),
            "skills update uses the saved selector; use skills install to select a label or version"
        );
        assert_eq!(
            error(args(Some("Bad_Name")), Action::Install),
            "Skill names may only contain lowercase letters, numbers, and single hyphens"
        );
        assert_eq!(
            error(
                InstallArgs {
                    no_lockfile: true,
                    ..args(None)
                },
                Action::Install
            ),
            "--no-lockfile requires skills install with a name, --tag, or --all; bare install and update require the lockfile"
        );
        assert_eq!(
            error(
                InstallArgs {
                    label: Some("x".into()),
                    ..args(None)
                },
                Action::Install
            ),
            "--label requires a skill name, --tag, or --all; bare install uses the lockfile"
        );
        assert_eq!(
            error(
                InstallArgs {
                    version: Some("0".into()),
                    ..args(Some("a"))
                },
                Action::Install
            ),
            "Skill version must be a positive safe integer"
        );
        assert_eq!(
            error(
                InstallArgs {
                    all: true,
                    version: Some("2".into()),
                    ..args(None)
                },
                Action::Install
            ),
            "--version requires a skill name; use --label with --tag or --all"
        );
        assert_eq!(
            error(
                InstallArgs {
                    tag: Some(String::new()),
                    ..args(None)
                },
                Action::Install
            ),
            "--tag requires a value"
        );
        let parsed = parse_options(&args(Some("a@staging")), Action::Install, &cwd).unwrap();
        assert_eq!(parsed.label.as_deref(), Some("staging"));
        let parsed = parse_options(&args(Some("a@12")), Action::Install, &cwd).unwrap();
        assert_eq!(parsed.version.map(Version::get), Some(12));
        assert_eq!(parsed.name.unwrap().as_str(), "a");
    }

    #[test]
    fn unsafe_manifest_paths() {
        let dir = Path::new("/x/skill");
        for path in ["../a", "a/../b", "a//b", "./a", "a\\b", "", "/abs"] {
            assert_eq!(
                resolve_file_path(dir, path).unwrap_err().exit,
                Exit::Response,
                "{path:?}"
            );
        }
        assert_eq!(
            resolve_file_path(dir, "docs/a.md").unwrap(),
            Path::new("/x/skill/docs/a.md")
        );
    }
}

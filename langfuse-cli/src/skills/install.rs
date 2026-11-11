//! `langfuse skills install` and `langfuse skills update`.

use std::collections::HashSet;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::config::Settings;
use crate::error::{CliError, Exit, Result};
use crate::output::{Io, escape_terminal_text};
use crate::skills::api::{Remote, SkillFile, SkillVersion, is_skill_name, sha256_base64};
use crate::skills::local::matches_manifest;
use crate::skills::lock::{self, LockedSkill, Selector, SkillsLock};
use crate::skills::paths::{exists, physical, relative, resolve, to_slash};
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
    name: Option<String>,
    tag: Option<String>,
    all: bool,
    version: Option<i64>,
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
    let mut name = raw.name.clone();
    let mut version = raw.version.clone();
    let mut label = raw.label.clone();
    if let Some(full) = name.clone()
        && let Some((skill, selector)) = full.split_once('@')
    {
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
            version = Some(selector.to_owned());
        } else {
            label = Some(selector.to_owned());
        }
        name = Some(skill.to_owned());
    }
    if raw.no_lockfile && (action == Action::Update || selectors == 0) {
        return Err(CliError::usage(
            "--no-lockfile requires skills install with a name, --tag, or --all; bare install and update \
             require the lockfile",
        ));
    }
    if let Some(name) = &name
        && !is_skill_name(name)
    {
        return Err(CliError::usage(
            "Skill names may only contain lowercase letters, numbers, and single hyphens",
        ));
    }
    for (option, value) in [
        ("tag", &raw.tag),
        ("version", &version),
        ("label", &label),
        ("directory", &raw.directory),
    ] {
        if value.as_deref() == Some("") {
            return Err(CliError::usage(format!("--{option} requires a value")));
        }
    }
    if selectors == 0 {
        for (option, value) in [("version", &version), ("label", &label)] {
            if value.is_some() {
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
                .filter(|version| (1..=MAX_SAFE_INTEGER).contains(version))
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
    let directory = resolve(cwd, raw.directory.as_deref().unwrap_or(DEFAULT_DIRECTORY));
    let lockfile = lock::resolve_path(cwd, &directory, raw.lockfile.as_deref(), raw.no_lockfile)?;
    Ok(Options {
        name,
        tag: raw.tag.clone(),
        all: raw.all,
        version,
        label,
        directory,
        lockfile,
        force: raw.force,
    })
}

struct Target {
    name: String,
    destination: PathBuf,
    selector: Selector,
    request: Selector,
    previous_version: Option<i64>,
}

struct Skipped {
    name: String,
}

async fn resolve_targets(
    remote: &mut Remote,
    options: &Options,
    lock: &SkillsLock,
    action: Action,
) -> Result<(Vec<Target>, Vec<Skipped>)> {
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
                skipped.push(Skipped { name: skill.name });
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
                destination: physical(&options.directory.join(&name))?,
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

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

async fn install_files(
    remote: &mut Remote,
    files: &[SkillFile],
    destination: &Path,
    replace: bool,
) -> Result<()> {
    let parent = destination.parent().unwrap_or(Path::new("/")).to_path_buf();
    let staging = parent.join(format!(
        ".{}.install-{}",
        file_name(destination),
        uuid::Uuid::new_v4()
    ));
    let result = async {
        std::fs::create_dir_all(&parent).map_err(cannot_install)?;
        std::fs::create_dir(&staging).map_err(cannot_install)?;
        download_files(remote, files, &staging).await?;
        // Names as stored by the filesystem, directory spelling included.
        if !matches_manifest(&staging, files)? {
            return Err(CliError::response(
                "Skill file paths conflict on this filesystem",
            ));
        }
        replace_directory(&staging, destination, replace).map_err(cannot_install)
    }
    .await;
    let _ = std::fs::remove_dir_all(&staging);
    result
}

fn cannot_install(error: std::io::Error) -> CliError {
    CliError::local(format!("Cannot install skill: {error}"))
}

async fn download_files(remote: &mut Remote, files: &[SkillFile], directory: &Path) -> Result<()> {
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
    if bytes.len() as u64 != file.content_length {
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
fn replace_directory(staging: &Path, destination: &Path, replace: bool) -> std::io::Result<()> {
    let backup = destination.parent().unwrap_or(Path::new("/")).join(format!(
        ".{}.backup-{}",
        file_name(destination),
        uuid::Uuid::new_v4()
    ));
    if replace {
        std::fs::rename(destination, &backup)?;
    }
    if let Err(error) = std::fs::rename(staging, destination) {
        if replace {
            std::fs::rename(&backup, destination)?;
        }
        return Err(error);
    }
    if replace {
        let _ = std::fs::remove_dir_all(&backup);
    }
    Ok(())
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
    let mut remote = Remote::new(settings);
    let mut lock = match &options.lockfile {
        Some(path) => lock::read(path)?,
        None => SkillsLock::default(),
    };
    let (targets, skipped) = resolve_targets(&mut remote, &options, &lock, action).await?;
    for skill in &skipped {
        let _ = writeln!(
            io.stderr,
            "Skipped {}: no production version",
            escape_terminal_text(&skill.name)
        );
    }
    let destinations: Vec<PathBuf> = targets
        .iter()
        .map(|target| target.destination.clone())
        .collect();
    directories::validate(&io.cwd, &destinations, &lock, options.lockfile.as_deref())?;

    // Resolve every manifest and check every destination before changing any installation.
    struct Installation {
        skill: SkillVersion,
        destination: PathBuf,
        selector: Selector,
        exists: bool,
        unchanged: bool,
    }
    let mut installations = Vec::new();
    for target in targets {
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
            let mut clean_update = false;
            if action == Action::Update
                && let Some(previous) = target.previous_version
                && previous != skill.version
            {
                let previous = remote
                    .fetch_skill(&target.name, &Selector::Version(previous))
                    .await?;
                clean_update = matches_manifest(&target.destination, &previous.files)?;
            }
            if !clean_update {
                return Err(CliError::local(format!(
                    "{} contains different files; pass --force to replace it",
                    target.destination.display()
                )));
            }
        }
        installations.push(Installation {
            skill,
            destination: target.destination,
            selector: target.selector,
            exists,
            unchanged,
        });
    }

    let mut results = Vec::new();
    for installation in installations {
        let Installation {
            skill,
            destination,
            selector,
            exists,
            unchanged,
        } = installation;
        if !unchanged {
            install_files(&mut remote, &skill.files, &destination, exists).await?;
        }
        if let Some(lockfile) = &options.lockfile {
            let key = to_slash(&relative(
                lockfile.parent().unwrap_or(Path::new("/")),
                &destination,
            ));
            lock::record(
                &mut lock,
                lockfile,
                LockedSkill {
                    path: key,
                    name: skill.name.clone(),
                    selector,
                    version: skill.version,
                },
            );
            lock::write(&lock, lockfile)?;
        }
        let directory = destination.display().to_string();
        if !json_output {
            let line = if unchanged {
                format!(
                    "Already up to date: {}@{} at {}",
                    skill.name,
                    skill.version,
                    escape_terminal_text(&directory)
                )
            } else {
                format!(
                    "Installed {}@{} to {} ({} files)",
                    skill.name,
                    skill.version,
                    escape_terminal_text(&directory),
                    skill.files.len()
                )
            };
            let _ = writeln!(io.stdout, "{line}");
        }
        results.push(json!({
            "name": skill.name,
            "version": skill.version,
            "labels": skill.labels,
            "directory": directory,
            "files": skill.files.len(),
            "status": if unchanged { "unchanged" } else { "installed" },
        }));
    }

    if json_output {
        let value = if action == Action::Install && options.name.is_some() {
            results.into_iter().next().unwrap_or(Value::Null)
        } else {
            Value::Array(
                skipped
                    .iter()
                    .map(|skill| json!({"name": skill.name, "status": "skipped", "reason": "no-production-version"}))
                    .chain(results)
                    .collect(),
            )
        };
        let _ = writeln!(io.stdout, "{}", escape_terminal_text(&value.to_string()));
    } else if results.is_empty() && skipped.is_empty() {
        let message = if options.all {
            "No skills found in this Langfuse project".to_owned()
        } else {
            format!(
                "No skills found with tag {}",
                escape_terminal_text(
                    &Value::from(options.tag.clone().unwrap_or_default()).to_string()
                )
            )
        };
        let _ = writeln!(io.stdout, "{message}");
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
        assert_eq!(parsed.version, Some(12));
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

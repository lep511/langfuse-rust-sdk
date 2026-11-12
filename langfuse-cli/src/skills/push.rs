//! `langfuse skills push`: publish local skill directories as new versions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures_util::stream::{self, StreamExt as _, TryStreamExt as _};
use serde::Serialize;
use serde_json::Value;

use crate::config::Settings;
use crate::error::{CliError, Exit, Result};
use crate::output::{Io, escape_terminal_text, json_line};
use crate::skills::PushArgs;
use crate::skills::api::{REQUEST_CONCURRENCY, Remote, SkillVersion};
use crate::skills::directories;
use crate::skills::local_skill::{
    ExcludedPath, LocalSkill, LocalSkillFile, ReadError, discover_skill_directories,
    read_local_skill,
};
use crate::skills::lock::{self, LockedSkill, Selector};
use crate::skills::paths::resolve;
use crate::skills::publish_filter::parse_exclusions;
use crate::skills::types::{SkillName, Version};

struct Options {
    directory: PathBuf,
    commit_message: Option<String>,
    label: Option<String>,
    dry_run: bool,
    recursive: bool,
    follow_symlinks: bool,
    update_only: bool,
    no_lockfile: bool,
    lockfile: Option<PathBuf>,
    exclude: Vec<String>,
}

fn parse_options(args: &PushArgs, cwd: &Path) -> Result<Options> {
    if args.directory.as_os_str().is_empty() {
        return Err(CliError::usage("skills push requires one directory"));
    }
    if let Some(message) = &args.commit_message
        && message.chars().count() > 500
    {
        return Err(CliError::usage(
            "--commit-message must be at most 500 characters",
        ));
    }
    if let Some(label) = &args.label {
        let valid = !label.is_empty()
            && label.len() <= 36
            && label != "latest"
            && label.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
            });
        if !valid {
            return Err(CliError::usage(
                "--label must be 1–36 lowercase letters, digits, underscores, dots, or hyphens; latest is \
                 managed automatically",
            ));
        }
    }
    Ok(Options {
        directory: resolve(cwd, &args.directory),
        commit_message: args.commit_message.clone(),
        label: args.label.clone(),
        dry_run: args.dry_run,
        recursive: args.recursive,
        follow_symlinks: args.follow_symlinks,
        update_only: args.update_only,
        no_lockfile: args.no_lockfile,
        lockfile: args.lockfile.clone(),
        exclude: parse_exclusions(&args.exclude)?,
    })
}

/// The paths a push adds, changes and removes.
#[derive(Debug, Clone, Serialize)]
struct Changes {
    added: Vec<String>,
    changed: Vec<String>,
    removed: Vec<String>,
}

impl Changes {
    fn any(&self) -> bool {
        !self.added.is_empty() || !self.changed.is_empty() || !self.removed.is_empty()
    }
}

fn snapshot_changes(files: &[LocalSkillFile], skill: Option<&SkillVersion>) -> Changes {
    let mut remote: HashMap<&str, (&str, u64)> = skill
        .map(|skill| {
            skill
                .files
                .iter()
                .map(|file| {
                    (
                        file.path.as_str(),
                        (file.sha256_hash.as_str(), file.content_length),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let mut changes = Changes {
        added: Vec::new(),
        changed: Vec::new(),
        removed: Vec::new(),
    };
    for file in files {
        match remote.remove(file.path.as_str()) {
            None => changes.added.push(file.path.clone()),
            Some((hash, length)) if hash != file.sha256_hash || length != file.content_length => {
                changes.changed.push(file.path.clone());
            }
            Some(_) => {}
        }
    }
    changes.removed = remote.into_keys().map(str::to_owned).collect();
    changes.removed.sort();
    changes
}

struct Plan {
    local: LocalSkill,
    previous: Option<SkillVersion>,
    changes: Changes,
    unchanged: bool,
    /// The `path` of the lock entry, when there is a lockfile.
    lock_path: Option<String>,
}

/// The `status` of a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Created,
    Unchanged,
    Labeled,
    Skipped,
    WouldCreate,
    WouldLabel,
}

/// Why a skill was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Reason {
    InvalidSkill,
    NotFound,
}

/// The files of a snapshot, listed by `--dry-run`.
#[derive(Debug, Serialize)]
struct Snapshot {
    included: Vec<String>,
    excluded: Vec<ExcludedPath>,
}

/// A local skill that was not published, as its result starts.
#[derive(Debug, Serialize)]
struct Unpublished {
    name: SkillName,
    /// The `latest` version in Langfuse; `null` when the skill is not there.
    version: Option<Version>,
    directory: String,
    files: usize,
    #[serde(flatten)]
    snapshot: Option<Snapshot>,
}

impl Unpublished {
    fn new(options: &Options, plan: &Plan) -> Self {
        Self {
            name: plan.local.name.clone(),
            version: plan.previous.as_ref().map(|previous| previous.version),
            directory: plan.local.directory.display().to_string(),
            files: plan.local.files.len(),
            snapshot: options.dry_run.then(|| Snapshot {
                included: plan
                    .local
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect(),
                excluded: plan.local.excluded.clone(),
            }),
        }
    }
}

fn is_false(flag: &bool) -> bool {
    !flag
}

/// One result of the `--json` output. The order of the fields is the order
/// of the keys, which is the official CLI's.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Report {
    /// `--recursive` met a directory that is not a valid skill.
    #[serde(rename_all = "camelCase")]
    Invalid {
        directory: String,
        status: Status,
        reason: Reason,
        message: String,
        #[serde(skip_serializing_if = "is_false")]
        dry_run: bool,
    },
    /// `--update-only` met a skill that does not exist in Langfuse.
    #[serde(rename_all = "camelCase")]
    NotFound {
        #[serde(flatten)]
        skill: Unpublished,
        status: Status,
        reason: Reason,
        #[serde(skip_serializing_if = "is_false")]
        dry_run: bool,
    },
    /// What `--dry-run` would do.
    #[serde(rename_all = "camelCase")]
    DryRun {
        #[serde(flatten)]
        skill: Unpublished,
        status: Status,
        changes: Changes,
        dry_run: bool,
        expected_version: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// A published, unchanged or labelled skill.
    Completed {
        name: SkillName,
        version: Version,
        directory: String,
        files: usize,
        labels: Vec<String>,
        status: Status,
    },
}

impl Report {
    /// The text output: a summary line, then the snapshot of a dry run.
    fn text(&self) -> String {
        let (summary, snapshot) = match self {
            Report::Invalid { message, .. } => (format!("Skipped invalid skill: {message}"), None),
            Report::NotFound { skill, .. } => (
                format!(
                    "Skipped {}: skill does not exist in Langfuse ({})",
                    skill.name, skill.directory
                ),
                skill.snapshot.as_ref(),
            ),
            Report::DryRun {
                skill,
                changes,
                expected_version,
                label,
                ..
            } => {
                let versions = match skill.version {
                    Some(version) => format!("v{version}->v{expected_version}"),
                    None => format!("->v{expected_version}"),
                };
                let label = label
                    .as_ref()
                    .map(|label| format!("; label: {label}"))
                    .unwrap_or_default();
                (
                    format!(
                        "Dry run: {} {versions} ({} added, {} changed, {} removed{label})",
                        skill.name,
                        changes.added.len(),
                        changes.changed.len(),
                        changes.removed.len()
                    ),
                    skill.snapshot.as_ref(),
                )
            }
            Report::Completed {
                name,
                version,
                files,
                labels,
                status,
                ..
            } => {
                let action = match status {
                    Status::Unchanged => "Already up to date",
                    Status::Labeled => "Labeled",
                    _ => "Published",
                };
                (
                    format!(
                        "{action}: {name}@{version} ({files} files; labels: {})",
                        labels.join(", ")
                    ),
                    None,
                )
            }
        };
        // Paths are printed as JSON strings, like the official CLI.
        let quoted = |path: &str| escape_terminal_text(&Value::from(path).to_string());
        let mut lines = vec![escape_terminal_text(&summary)];
        if let Some(snapshot) = snapshot {
            for path in &snapshot.included {
                lines.push(format!("  Include {}", quoted(path)));
            }
            for excluded in &snapshot.excluded {
                lines.push(format!(
                    "  Exclude {} ({})",
                    quoted(&excluded.path),
                    excluded.reason.as_str()
                ));
            }
        }
        lines.join("\n")
    }
}

/// A finished push of one skill (published, unchanged or labelled).
struct Completed {
    name: SkillName,
    version: Version,
    directory: PathBuf,
    files: usize,
    labels: Vec<String>,
    status: Status,
}

impl From<Completed> for Report {
    fn from(completed: Completed) -> Self {
        Report::Completed {
            name: completed.name,
            version: completed.version,
            directory: completed.directory.display().to_string(),
            files: completed.files,
            labels: completed.labels,
            status: completed.status,
        }
    }
}

enum Outcome {
    Completed(Completed),
    Other(Report),
}

async fn push_skill(remote: &Remote, options: &Options, plan: &Plan) -> Result<Outcome> {
    if options.update_only && plan.previous.is_none() {
        return Ok(Outcome::Other(Report::NotFound {
            skill: Unpublished::new(options, plan),
            status: Status::Skipped,
            reason: Reason::NotFound,
            dry_run: options.dry_run,
        }));
    }
    if options.dry_run {
        let would_label = options.label.as_ref().is_some_and(|label| {
            !plan
                .previous
                .as_ref()
                .is_some_and(|previous| previous.labels.contains(label))
        });
        let status = match (plan.unchanged, would_label) {
            (true, true) => Status::WouldLabel,
            (true, false) => Status::Unchanged,
            (false, _) => Status::WouldCreate,
        };
        let previous_version = plan
            .previous
            .as_ref()
            .map_or(0, |previous| previous.version.get());
        return Ok(Outcome::Other(Report::DryRun {
            skill: Unpublished::new(options, plan),
            status,
            changes: plan.changes.clone(),
            dry_run: true,
            expected_version: if plan.unchanged {
                previous_version
            } else {
                previous_version + 1
            },
            label: options.label.clone(),
        }));
    }
    let skill = match (&plan.previous, plan.unchanged) {
        (Some(previous), true) => previous.clone(),
        _ => publish_skill(remote, plan, options.commit_message.as_deref()).await?,
    };
    Ok(Outcome::Completed(Completed {
        name: skill.name,
        version: skill.version,
        directory: plan.local.directory.clone(),
        files: plan.local.files.len(),
        labels: skill.labels,
        status: if plan.unchanged {
            Status::Unchanged
        } else {
            Status::Created
        },
    }))
}

async fn publish_skill(
    remote: &Remote,
    plan: &Plan,
    commit_message: Option<&str>,
) -> Result<SkillVersion> {
    let files = plan
        .local
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.content.as_str()));
    let skill = remote
        .create_skill_version(&plan.local.name, files, commit_message)
        .await?;
    let mismatch = snapshot_changes(&plan.local.files, Some(&skill)).any();
    let not_advanced = plan
        .previous
        .as_ref()
        .is_some_and(|previous| skill.version <= previous.version);
    if mismatch || not_advanced {
        return Err(CliError::response(
            "Langfuse returned a skill version that does not match the published snapshot",
        ));
    }
    Ok(skill)
}

/// Run `langfuse skills push`.
pub async fn run(
    settings: Settings,
    args: &PushArgs,
    json_output: bool,
    io: &mut Io<'_>,
) -> Result<Exit> {
    let options = parse_options(args, &io.cwd)?;
    let parent = options
        .directory
        .parent()
        .unwrap_or(Path::new("/"))
        .to_path_buf();
    let lock_base = if options.recursive {
        &options.directory
    } else {
        &parent
    };
    let mut lockfile = lock::resolve_path(
        &io.cwd,
        lock_base,
        options.lockfile.as_deref(),
        options.no_lockfile,
    )?;

    // Read the local skills.
    let directories_found = if options.recursive {
        discover_skill_directories(&options.directory, options.follow_symlinks, io).await?
    } else {
        vec![options.directory.clone()]
    };
    let mut skills: Vec<LocalSkill> = Vec::new();
    // The directory and the reason of each skill that could not be read.
    let mut invalid: Vec<(PathBuf, String)> = Vec::new();
    let mut names: HashMap<SkillName, PathBuf> = HashMap::new();
    for directory in directories_found {
        let read = read_local_skill(&directory, options.follow_symlinks, &options.exclude).await;
        let skill = match read {
            Ok(skill) => skill,
            Err(ReadError::Invalid(error)) if options.recursive => {
                invalid.push((directory, error.message));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if let Some(duplicate) = names.get(&skill.name) {
            return Err(CliError::local(format!(
                "Multiple skill directories declare the name {}: {}, {}",
                skill.name,
                duplicate.display(),
                directory.display()
            )));
        }
        names.insert(skill.name.clone(), directory);
        skills.push(skill);
    }

    let source = std::fs::canonicalize(&options.directory).map_err(|error| {
        CliError::local(format!(
            "Cannot read {}: {error}",
            options.directory.display()
        ))
    })?;
    // Recursive discovery can select the root itself; keep the lockfile outside its snapshot.
    let root_selected = skills
        .iter()
        .map(|skill| &skill.directory)
        .chain(invalid.iter().map(|(directory, _)| directory))
        .any(|directory| *directory == source || *directory == options.directory);
    if options.recursive && options.lockfile.is_none() && root_selected {
        lockfile = lock::resolve_path(&io.cwd, &parent, None, options.no_lockfile)?;
    }
    let mut reports: Vec<Report> = invalid
        .into_iter()
        .map(|(directory, message)| Report::Invalid {
            directory: directory.display().to_string(),
            status: Status::Skipped,
            reason: Reason::InvalidSkill,
            message,
            dry_run: options.dry_run,
        })
        .collect();
    if !json_output {
        for report in &reports {
            io.status(&report.text())?;
        }
    }
    let mut lock_state = match &lockfile {
        Some(path) => Some(lock::read(path)?),
        None => None,
    };
    if let Some(lock_state) = &lock_state {
        let targets: Vec<PathBuf> = skills.iter().map(|skill| skill.directory.clone()).collect();
        directories::validate(&io.cwd, &targets, lock_state, lockfile.as_deref())?;
    }
    // The lock entry of every skill, before any request is sent.
    let lock_paths = skills
        .iter()
        .map(|skill| {
            lockfile
                .as_deref()
                .map(|lockfile| lock::key(lockfile, &skill.directory))
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?;

    // Resolve every skill's remote version before publishing anything. The
    // requests overlap; the plans keep the skills' order.
    let remote = Remote::new(settings);
    let plans: Vec<Plan> = stream::iter(skills.into_iter().zip(lock_paths))
        .map(|(local, lock_path)| {
            let remote = &remote;
            async move {
                let previous = remote.fetch_latest_skill(&local.name).await?;
                let changes = snapshot_changes(&local.files, previous.as_ref());
                let unchanged = previous.is_some() && !changes.any();
                Ok::<_, CliError>(Plan {
                    local,
                    previous,
                    changes,
                    unchanged,
                    lock_path,
                })
            }
        })
        .buffered(REQUEST_CONCURRENCY)
        .try_collect()
        .await?;

    // Publish one skill at a time: each one is recorded before the next.
    let mut published: Vec<String> = Vec::new();
    for plan in &plans {
        let outcome = async {
            let mut outcome = push_skill(&remote, &options, plan).await?;
            if let Outcome::Completed(completed) = &mut outcome {
                if completed.status == Status::Created {
                    published.push(format!("{}@{}", completed.name, completed.version));
                }
                if let Some(label) = &options.label
                    && !completed.labels.contains(label)
                {
                    let mut labels: Vec<String> = completed
                        .labels
                        .iter()
                        .filter(|existing| existing.as_str() != "latest")
                        .cloned()
                        .collect();
                    labels.push(label.clone());
                    let skill = remote
                        .set_skill_labels(&completed.name, completed.version, &labels)
                        .await?;
                    completed.labels = skill.labels;
                    if completed.status == Status::Unchanged {
                        completed.status = Status::Labeled;
                    }
                }
                if let (Some(lock_state), Some(lockfile), Some(path)) =
                    (lock_state.as_mut(), lockfile.as_deref(), &plan.lock_path)
                {
                    lock::record(
                        lock_state,
                        lockfile,
                        LockedSkill {
                            path: path.clone(),
                            name: completed.name.clone(),
                            selector: match &options.label {
                                Some(label) => Selector::Label(label.clone()),
                                None => Selector::Version(completed.version),
                            },
                            version: completed.version,
                        },
                    );
                    lock::write(lock_state, lockfile)?;
                }
            }
            Ok::<_, CliError>(outcome)
        }
        .await;
        let outcome = outcome.map_err(|error| {
            let completed = if published.is_empty() {
                String::new()
            } else {
                format!(
                    " Published versions remain available: {}.",
                    published.join(", ")
                )
            };
            CliError::new(
                error.exit,
                format!(
                    "Failed to push {} at {}: {}.{completed}",
                    plan.local.name,
                    plan.local.directory.display(),
                    error.message
                ),
            )
        })?;
        let report = match outcome {
            Outcome::Completed(completed) => Report::from(completed),
            Outcome::Other(report) => report,
        };
        if !json_output {
            io.status(&report.text())?;
        }
        reports.push(report);
    }

    if json_output {
        let line = if options.recursive {
            json_line(&reports)?
        } else {
            // One directory: its result alone, not a list.
            json_line(&reports.first())?
        };
        io.status(&escape_terminal_text(&line))?;
    } else if reports.is_empty() {
        io.status(&format!(
            "No skills found in {}",
            escape_terminal_text(&options.directory.display().to_string())
        ))?;
    }
    Ok(Exit::Success)
}

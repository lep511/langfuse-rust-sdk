//! `langfuse skills push`: publish local skill directories as new versions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::config::Settings;
use crate::error::{CliError, Exit, Result};
use crate::output::{Io, escape_terminal_text};
use crate::skills::PushArgs;
use crate::skills::api::{Remote, SkillVersion};
use crate::skills::directories;
use crate::skills::local_skill::{
    LocalSkill, LocalSkillFile, ReadError, discover_skill_directories, read_local_skill,
};
use crate::skills::lock::{self, LockedSkill, Selector};
use crate::skills::paths::{relative, resolve, to_slash};
use crate::skills::publish_filter::parse_exclusions;

struct Options {
    directory: PathBuf,
    commit_message: Option<String>,
    label: Option<String>,
    dry_run: bool,
    recursive: bool,
    follow_symlinks: bool,
    update_only: bool,
    no_lockfile: bool,
    lockfile: Option<String>,
    exclude: Vec<String>,
}

fn parse_options(args: &PushArgs, cwd: &Path) -> Result<Options> {
    if args.directory.is_empty() {
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

struct Changes {
    added: Vec<String>,
    changed: Vec<String>,
    removed: Vec<String>,
}

impl Changes {
    fn any(&self) -> bool {
        !self.added.is_empty() || !self.changed.is_empty() || !self.removed.is_empty()
    }

    fn json(&self) -> Value {
        json!({"added": self.added, "changed": self.changed, "removed": self.removed})
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
}

/// A finished push of one skill (published, unchanged or labelled).
struct Completed {
    fields: Map<String, Value>,
    name: String,
    version: i64,
    labels: Vec<String>,
    status: &'static str,
    directory: PathBuf,
    files: usize,
}

enum Outcome {
    Completed(Completed),
    Other(Map<String, Value>),
}

fn base_fields(options: &Options, plan: &Plan) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("name".into(), json!(plan.local.name));
    fields.insert(
        "version".into(),
        json!(plan.previous.as_ref().map(|previous| previous.version)),
    );
    fields.insert(
        "directory".into(),
        json!(plan.local.directory.display().to_string()),
    );
    fields.insert("files".into(), json!(plan.local.files.len()));
    if options.dry_run {
        fields.insert(
            "included".into(),
            json!(
                plan.local
                    .files
                    .iter()
                    .map(|file| file.path.as_str())
                    .collect::<Vec<_>>()
            ),
        );
        fields.insert(
            "excluded".into(),
            Value::Array(
                plan.local
                    .excluded
                    .iter()
                    .map(|excluded| json!({"path": excluded.path, "reason": excluded.reason.as_str()}))
                    .collect(),
            ),
        );
    }
    fields
}

async fn push_skill(remote: &mut Remote, options: &Options, plan: &Plan) -> Result<Outcome> {
    let mut fields = base_fields(options, plan);
    if options.update_only && plan.previous.is_none() {
        fields.insert("status".into(), json!("skipped"));
        fields.insert("reason".into(), json!("not-found"));
        if options.dry_run {
            fields.insert("dryRun".into(), json!(true));
        }
        return Ok(Outcome::Other(fields));
    }
    if options.dry_run {
        let would_label = options.label.as_ref().is_some_and(|label| {
            !plan
                .previous
                .as_ref()
                .is_some_and(|previous| previous.labels.contains(label))
        });
        let status = match (plan.unchanged, would_label) {
            (true, true) => "would-label",
            (true, false) => "unchanged",
            (false, _) => "would-create",
        };
        let previous_version = plan.previous.as_ref().map(|previous| previous.version);
        let expected = if plan.unchanged {
            previous_version.unwrap_or(0)
        } else {
            previous_version.unwrap_or(0) + 1
        };
        fields.insert("status".into(), json!(status));
        fields.insert("changes".into(), plan.changes.json());
        fields.insert("dryRun".into(), json!(true));
        fields.insert("expectedVersion".into(), json!(expected));
        if let Some(label) = &options.label {
            fields.insert("label".into(), json!(label));
        }
        return Ok(Outcome::Other(fields));
    }
    let skill = match (&plan.previous, plan.unchanged) {
        (Some(previous), true) => previous.clone(),
        _ => publish_skill(remote, plan, options.commit_message.as_deref()).await?,
    };
    Ok(Outcome::Completed(Completed {
        fields,
        name: skill.name.clone(),
        version: skill.version,
        labels: skill.labels,
        status: if plan.unchanged {
            "unchanged"
        } else {
            "created"
        },
        directory: plan.local.directory.clone(),
        files: plan.local.files.len(),
    }))
}

async fn publish_skill(
    remote: &mut Remote,
    plan: &Plan,
    commit_message: Option<&str>,
) -> Result<SkillVersion> {
    let files: Vec<(String, String)> = plan
        .local
        .files
        .iter()
        .map(|file| (file.path.clone(), file.content.clone()))
        .collect();
    let skill = remote
        .create_skill_version(&plan.local.name, &files, commit_message)
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

impl Completed {
    fn json(&self) -> Value {
        let mut fields = self.fields.clone();
        fields.insert("version".into(), json!(self.version));
        fields.insert("labels".into(), json!(self.labels));
        fields.insert("status".into(), json!(self.status));
        Value::Object(fields)
    }

    fn summary(&self) -> String {
        let action = match self.status {
            "unchanged" => "Already up to date",
            "labeled" => "Labeled",
            _ => "Published",
        };
        format!(
            "{action}: {}@{} ({} files; labels: {})",
            self.name,
            self.version,
            self.files,
            self.labels.join(", ")
        )
    }
}

fn summary(fields: &Map<String, Value>) -> String {
    let text = |key: &str| {
        fields
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match text("reason").as_str() {
        "invalid-skill" => return format!("Skipped invalid skill: {}", text("message")),
        "not-found" => {
            return format!(
                "Skipped {}: skill does not exist in Langfuse ({})",
                text("name"),
                text("directory")
            );
        }
        _ => {}
    }
    let count = |key: &str| {
        fields
            .get("changes")
            .and_then(|changes| changes.get(key))
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    let label = fields
        .get("label")
        .and_then(Value::as_str)
        .map(|label| format!("; label: {label}"))
        .unwrap_or_default();
    let expected = fields
        .get("expectedVersion")
        .cloned()
        .unwrap_or(Value::Null);
    let versions = match fields.get("version").and_then(Value::as_i64) {
        Some(version) => format!("v{version}->v{expected}"),
        None => format!("->v{expected}"),
    };
    format!(
        "Dry run: {} {versions} ({} added, {} changed, {} removed{label})",
        text("name"),
        count("added"),
        count("changed"),
        count("removed")
    )
}

fn format_result(summary: &str, fields: &Map<String, Value>) -> String {
    let mut lines = vec![escape_terminal_text(summary)];
    if let Some(included) = fields.get("included").and_then(Value::as_array) {
        for path in included {
            lines.push(format!(
                "  Include {}",
                escape_terminal_text(&path.to_string())
            ));
        }
        for excluded in fields
            .get("excluded")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            lines.push(format!(
                "  Exclude {} ({})",
                escape_terminal_text(&excluded["path"].to_string()),
                excluded["reason"].as_str().unwrap_or_default()
            ));
        }
    }
    lines.join("\n")
}

fn skipped_invalid(directory: &Path, message: &str, dry_run: bool) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("directory".into(), json!(directory.display().to_string()));
    fields.insert("status".into(), json!("skipped"));
    fields.insert("reason".into(), json!("invalid-skill"));
    fields.insert("message".into(), json!(message));
    if dry_run {
        fields.insert("dryRun".into(), json!(true));
    }
    fields
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
        options.directory.clone()
    } else {
        parent.clone()
    };
    let mut lockfile = lock::resolve_path(
        &io.cwd,
        &lock_base,
        options.lockfile.as_deref(),
        options.no_lockfile,
    )?;

    // Read the local skills.
    let directories_found = if options.recursive {
        discover_skill_directories(&options.directory, options.follow_symlinks, io)?
    } else {
        vec![options.directory.clone()]
    };
    let mut skills: Vec<LocalSkill> = Vec::new();
    let mut skipped: Vec<Map<String, Value>> = Vec::new();
    let mut names: HashMap<String, PathBuf> = HashMap::new();
    for directory in directories_found {
        let skill = match read_local_skill(&directory, options.follow_symlinks, &options.exclude) {
            Ok(skill) => skill,
            Err(ReadError::Invalid(error)) if options.recursive => {
                skipped.push(skipped_invalid(&directory, &error.message, options.dry_run));
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
        .map(|skill| skill.directory.clone())
        .chain(
            skipped
                .iter()
                .filter_map(|fields| fields["directory"].as_str().map(PathBuf::from)),
        )
        .any(|directory| directory == source || directory == options.directory);
    if options.recursive && options.lockfile.is_none() && root_selected {
        lockfile = lock::resolve_path(&io.cwd, &parent, None, options.no_lockfile)?;
    }
    if !json_output {
        for fields in &skipped {
            let _ = writeln!(io.stdout, "{}", format_result(&summary(fields), fields));
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

    // Resolve every skill's remote version before publishing anything.
    let mut remote = Remote::new(settings);
    let mut plans = Vec::new();
    for local in skills {
        let previous = remote.fetch_latest_skill(&local.name).await?;
        let changes = snapshot_changes(&local.files, previous.as_ref());
        let unchanged = previous.is_some() && !changes.any();
        plans.push(Plan {
            local,
            previous,
            changes,
            unchanged,
        });
    }

    let mut results: Vec<Value> = skipped.iter().cloned().map(Value::Object).collect();
    let mut published: Vec<String> = Vec::new();
    for plan in &plans {
        let outcome = async {
            let mut outcome = push_skill(&mut remote, &options, plan).await?;
            if let Outcome::Completed(completed) = &mut outcome {
                if completed.status == "created" {
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
                    if completed.status == "unchanged" {
                        completed.status = "labeled";
                    }
                }
                if let (Some(lock_state), Some(lockfile)) =
                    (lock_state.as_mut(), lockfile.as_deref())
                {
                    let key = to_slash(&relative(
                        lockfile.parent().unwrap_or(Path::new("/")),
                        &completed.directory,
                    ));
                    lock::record(
                        lock_state,
                        lockfile,
                        LockedSkill {
                            path: key,
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
        let (value, text) = match outcome {
            Outcome::Completed(completed) => (
                completed.json(),
                format_result(&completed.summary(), &completed.fields),
            ),
            Outcome::Other(fields) => {
                let text = format_result(&summary(&fields), &fields);
                (Value::Object(fields), text)
            }
        };
        if !json_output {
            let _ = writeln!(io.stdout, "{text}");
        }
        results.push(value);
    }

    if json_output {
        let value = if options.recursive {
            Value::Array(results)
        } else {
            results.into_iter().next().unwrap_or(Value::Null)
        };
        let _ = writeln!(io.stdout, "{}", escape_terminal_text(&value.to_string()));
    } else if results.is_empty() {
        let _ = writeln!(
            io.stdout,
            "No skills found in {}",
            escape_terminal_text(&options.directory.display().to_string())
        );
    }
    Ok(Exit::Success)
}

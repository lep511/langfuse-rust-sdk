//! `langfuse skills install|update|push`: install Langfuse-hosted skills into
//! a project (tracked in `langfuse-skills.lock`) and publish local ones.

pub mod api;
pub mod directories;
pub mod install;
pub mod local;
pub mod local_skill;
pub mod lock;
pub mod paths;
pub mod publish_filter;
pub mod push;

use clap::{Args, Subcommand};

use crate::cli::GlobalArgs;
use crate::config;
use crate::error::{Exit, Result};
use crate::output::Io;

const INSTALL_AFTER_HELP: &str = "\
Examples:
  langfuse skills install support-triage            # production label
  langfuse skills install support-triage@staging    # a label
  langfuse skills install support-triage@3          # an immutable version
  langfuse skills install --tag support
  langfuse skills install --all
  langfuse skills install                           # restore the versions in the lockfile

Without --label, --tag and --all skip skills that have no production version.";

/// `langfuse skills`.
#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct SkillsArgs {
    #[command(subcommand)]
    pub command: SkillsCommand,
}

/// Subcommands of `langfuse skills`.
#[derive(Debug, Subcommand)]
pub enum SkillsCommand {
    /// Install Langfuse-hosted skills by name, tag, or --all (or restore the lockfile)
    #[command(after_help = INSTALL_AFTER_HELP)]
    Install(InstallArgs),
    /// Update locked skills using their saved selectors
    Update(UpdateArgs),
    /// Publish a local skill directory to Langfuse
    Push(PushArgs),
}

/// `langfuse skills install`.
#[derive(Debug, Clone, Default, Args)]
pub struct InstallArgs {
    /// Skill name, optionally with @label or @version
    #[arg(value_name = "NAME[@SELECTOR]")]
    pub name: Option<String>,

    /// Install every skill with this tag
    #[arg(long)]
    pub tag: Option<String>,

    /// Install every skill of the connected Langfuse project
    #[arg(long)]
    pub all: bool,

    /// Install this immutable version (named skill only)
    #[arg(long, value_name = "N")]
    pub version: Option<String>,

    /// Install this label of each selected skill [default: production]
    #[arg(long)]
    pub label: Option<String>,

    /// Parent of installed skills, where the lockfile is looked up [default: .agents/skills]
    #[arg(long, value_name = "PATH")]
    pub directory: Option<String>,

    /// Read and write this lockfile instead of langfuse-skills.lock
    #[arg(long, value_name = "PATH")]
    pub lockfile: Option<String>,

    /// Neither read nor write a lockfile (with a name, --tag or --all)
    #[arg(long)]
    pub no_lockfile: bool,

    /// Replace an existing installation whose contents differ
    #[arg(long)]
    pub force: bool,
}

/// `langfuse skills update`.
#[derive(Debug, Clone, Default, Args)]
pub struct UpdateArgs {
    /// Only this locked skill
    pub name: Option<String>,

    /// Where the lockfile is looked up [default: .agents/skills]
    #[arg(long, value_name = "PATH")]
    pub directory: Option<String>,

    /// Read and write this lockfile instead of langfuse-skills.lock
    #[arg(long, value_name = "PATH")]
    pub lockfile: Option<String>,

    /// Replace installations with local changes
    #[arg(long)]
    pub force: bool,
}

/// `langfuse skills push`.
#[derive(Debug, Clone, Default, Args)]
pub struct PushArgs {
    /// The skill directory (or, with --recursive, a directory of skills)
    #[arg(value_name = "DIRECTORY")]
    pub directory: String,

    /// Record why this version was published (at most 500 characters)
    #[arg(long, value_name = "TEXT")]
    pub commit_message: Option<String>,

    /// Apply a label and follow it in the lockfile [default: pin the version]
    #[arg(long)]
    pub label: Option<String>,

    /// Print the changes and included/excluded paths without publishing
    #[arg(long)]
    pub dry_run: bool,

    /// Discover skills recursively; report and skip invalid ones
    #[arg(long)]
    pub recursive: bool,

    /// Follow symlinked source and discovery directories (links inside a skill stay unsupported)
    #[arg(long)]
    pub follow_symlinks: bool,

    /// Skip skills that do not exist in Langfuse
    #[arg(long)]
    pub update_only: bool,

    /// Neither read nor write a lockfile
    #[arg(long)]
    pub no_lockfile: bool,

    /// Read and write this lockfile instead of langfuse-skills.lock
    #[arg(long, value_name = "PATH")]
    pub lockfile: Option<String>,

    /// Leave out a file or directory, relative to each skill (repeatable; literal, no globs)
    #[arg(long, value_name = "PATH")]
    pub exclude: Vec<String>,
}

/// Run `langfuse skills …`.
pub async fn run(skills: &SkillsArgs, globals: &GlobalArgs, io: &mut Io<'_>) -> Result<Exit> {
    let settings = config::load(globals)?;
    match &skills.command {
        SkillsCommand::Install(args) => {
            install::run(settings, args, install::Action::Install, globals.json, io).await
        }
        SkillsCommand::Update(args) => {
            let args = install::update_as_install(args);
            install::run(settings, &args, install::Action::Update, globals.json, io).await
        }
        SkillsCommand::Push(args) => push::run(settings, args, globals.json, io).await,
    }
}

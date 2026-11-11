//! The command-line grammar: global options and the top-level commands.

use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Args, ColorChoice, Parser, Subcommand};

use crate::api::ApiArgs;
use crate::skills::SkillsArgs;

/// Cargo-like colours for help and error output.
pub fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Cyan.on_default())
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
        .valid(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .invalid(AnsiColor::Yellow.on_default() | Effects::BOLD)
}

const AFTER_HELP: &str = "\
Exit codes:
  0 success · 1 internal · 2 usage · 3 configuration · 4 network · 5 HTTP error · 6 local file · 7 invalid response

Examples:
  langfuse api help
  langfuse api prompts list
  langfuse api prompts create --type text --name my-prompt --prompt 'Hello {{name}}'
  langfuse api observations list --limit 20 --format table
  langfuse skills install support-triage@staging
  langfuse skills push .agents/skills --recursive --update-only";

/// Langfuse CLI — interact with Langfuse from the command line.
///
/// Wraps every operation of the Langfuse public API (`langfuse api`), installs
/// and publishes Langfuse-hosted agent skills (`langfuse skills`), and prints
/// the latest Langfuse agent skill (`langfuse get-skill`).
#[derive(Debug, Parser)]
#[command(
    name = "langfuse",
    version,
    styles = styles(),
    after_help = AFTER_HELP,
    arg_required_else_help = true,
    max_term_width = 120
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

/// Options accepted before or after any command.
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Global options")]
pub struct GlobalArgs {
    /// Langfuse public key [env: LANGFUSE_PUBLIC_KEY]
    #[arg(long, global = true, value_name = "KEY")]
    pub public_key: Option<String>,

    /// Langfuse secret key [env: LANGFUSE_SECRET_KEY]
    #[arg(long, global = true, value_name = "KEY")]
    pub secret_key: Option<String>,

    /// Langfuse host [env: LANGFUSE_BASE_URL, LANGFUSE_HOST] [default: https://cloud.langfuse.com]
    #[arg(long, global = true, value_name = "URL")]
    pub host: Option<String>,

    /// Load LANGFUSE_* variables from a dotenv file (it takes precedence over the environment)
    #[arg(long, global = true, value_name = "PATH")]
    pub env: Option<PathBuf>,

    /// Request timeout in milliseconds [default: 30000]
    #[arg(long, global = true, value_name = "MS", value_parser = clap::value_parser!(u64).range(1..))]
    pub timeout: Option<u64>,

    /// Retries of failed requests (exponential backoff, honours Retry-After; 0 disables) [default: 2]
    #[arg(long, global = true, value_name = "N")]
    pub max_retries: Option<u32>,

    /// Machine-readable output: the {status, headers, body} envelope for API calls, JSON for skills
    #[arg(long, global = true)]
    pub json: bool,

    /// When to use colours
    #[arg(long, global = true, value_name = "WHEN", default_value = "auto")]
    pub color: ColorChoice,
}

/// The top-level commands.
#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Interact with the Langfuse REST API
    Api(ApiArgs),

    /// Install, update and publish Langfuse-hosted skills
    Skills(SkillsArgs),

    /// Print the latest Langfuse agent skill from GitHub
    GetSkill,

    /// Print a shell completion script
    Completions {
        /// The shell to generate completions for
        shell: clap_complete::Shell,
    },
}

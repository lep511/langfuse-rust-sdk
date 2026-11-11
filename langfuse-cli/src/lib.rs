//! `langfuse`: a command-line client for the Langfuse API, built on
//! [`langfuse_rust_sdk`].
//!
//! The binary is a thin wrapper around [`run`], which takes the arguments and
//! the standard streams so the whole CLI is testable in-process.

pub mod api;
pub mod args;
pub mod cli;
pub mod config;
pub mod error;
pub mod execute;
pub mod get_skill;
pub mod operation;
pub mod output;
pub mod paginate;
pub mod schema;
pub mod skills;
pub mod table;

use std::ffi::OsString;
use std::io::IsTerminal;

use clap::{ColorChoice, CommandFactory, FromArgMatches};

use crate::cli::{Cli, Command};
use crate::error::{CliError, Exit};
use crate::output::{Io, escape_terminal_text};

/// The clap command tree, with each API action's help naming its HTTP call.
pub fn command() -> clap::Command {
    let mut command = Cli::command();
    let entries = schema::registry();
    command = command.mut_subcommand("api", |mut api| {
        for entry in entries.values() {
            let spellings = std::iter::once((entry.resource.clone(), entry.action.clone()))
                .chain(entry.aliases.iter().cloned());
            let canonical = format!("{} {}", entry.resource, entry.action);
            for (resource, action) in spellings {
                let is_alias = format!("{resource} {action}") != canonical;
                let call = format!(
                    "Calls {} {} (operationId {}).",
                    entry.meta.method.as_str(),
                    entry.meta.path,
                    entry.meta.operation_id
                );
                let note = if is_alias {
                    format!("{call} Alias of `langfuse api {canonical}`.")
                } else {
                    call
                };
                // Resource aliases (`prompts-v2`) share the canonical command.
                let named = |command: &clap::Command, name: &str| {
                    command.get_subcommands().any(|sub| sub.get_name() == name)
                };
                if named(&api, &resource) {
                    api = api.mut_subcommand(&resource, |res| {
                        if named(&res, &action) {
                            res.mut_subcommand(&action, |cmd| cmd.before_long_help(note.clone()))
                        } else {
                            res
                        }
                    });
                }
            }
        }
        api
    });
    command
}

/// Whether to colour a stream: `--color always|never`, else a terminal
/// without `NO_COLOR` and with a `TERM` other than `dumb`.
pub fn use_color(choice: ColorChoice, terminal: bool) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            terminal
                && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
                && std::env::var_os("TERM").is_none_or(|term| term != "dumb")
        }
    }
}

/// `--color` as given on the command line, before parsing (it decides how
/// clap renders its own errors and help).
fn color_choice(args: &[OsString]) -> ColorChoice {
    let mut choice = ColorChoice::Auto;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let Some(arg) = arg.to_str() else { continue };
        let value = match arg.strip_prefix("--color") {
            Some("") => iter.next().and_then(|value| value.to_str()),
            Some(rest) => rest.strip_prefix('='),
            None => None,
        };
        choice = match value {
            Some("always") => ColorChoice::Always,
            Some("never") => ColorChoice::Never,
            Some("auto") => ColorChoice::Auto,
            _ => choice,
        };
    }
    choice
}

/// `api <resource> <action> help` means `--help`, as in the official CLI.
fn normalize(mut args: Vec<OsString>) -> Vec<OsString> {
    let word = |arg: Option<&OsString>| {
        arg.filter(|arg| !arg.to_string_lossy().starts_with('-'))
            .is_some()
    };
    if let Some(api) = args
        .iter()
        .skip(1)
        .position(|arg| arg == "api")
        .map(|index| index + 1)
        && word(args.get(api + 1))
        && args[api + 1] != "help"
        && word(args.get(api + 2))
        && args.get(api + 3).is_some_and(|arg| arg == "help")
    {
        args[api + 3] = OsString::from("--help");
    }
    args
}

/// Run the CLI with `args` (the program name first) on `io`.
pub async fn run(args: impl IntoIterator<Item = impl Into<OsString>>, io: &mut Io<'_>) -> Exit {
    let args = normalize(args.into_iter().map(Into::into).collect());
    let choice = color_choice(&args);
    let command = command().color(choice);

    let matches = match command.try_get_matches_from(&args) {
        Ok(matches) => matches,
        Err(error) => return clap_error(error, choice, io),
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => return clap_error(error, choice, io),
    };

    let result = match &cli.command {
        Command::Api(api) => api::run(api, &cli.global, io).await,
        Command::Skills(skills) => skills::run(skills, &cli.global, io).await,
        Command::GetSkill => get_skill::run(io).await,
        Command::Completions { shell } => {
            let mut command = command_for_completions();
            clap_complete::generate(*shell, &mut command, "langfuse", io.stdout);
            Ok(Exit::Success)
        }
    };
    match result {
        Ok(exit) => exit,
        Err(error) => report(&error, io),
    }
}

fn command_for_completions() -> clap::Command {
    Cli::command()
}

/// Print a CLI error to stderr and return its exit code.
pub fn report(error: &CliError, io: &mut Io<'_>) -> Exit {
    let _ = writeln!(
        io.stderr,
        "{}{}",
        escape_terminal_text(&error.message),
        error.hint
    );
    error.exit
}

/// Help and version go to stdout with exit 0; usage errors to stderr with 2.
fn clap_error(error: clap::Error, choice: ColorChoice, io: &mut Io<'_>) -> Exit {
    use clap::error::ErrorKind;
    let styled = error.render();
    let to_stdout = matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    if to_stdout {
        let text = if use_color(choice, io.stdout_tty) {
            styled.ansi().to_string()
        } else {
            styled.to_string()
        };
        let _ = write!(io.stdout, "{text}");
        Exit::Success
    } else {
        let text = if use_color(choice, io.stderr_tty) {
            styled.ansi().to_string()
        } else {
            styled.to_string()
        };
        let _ = write!(io.stderr, "{text}");
        Exit::Usage
    }
}

/// Real standard streams.
pub fn stdio_terminal() -> (bool, bool, Option<u16>) {
    let stdout_tty = std::io::stdout().is_terminal();
    let stderr_tty = std::io::stderr().is_terminal();
    let width = terminal_size::terminal_size().map(|(width, _)| width.0);
    (stdout_tty, stderr_tty, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn trailing_help_after_an_action_becomes_the_flag() {
        assert_eq!(
            normalize(os(&["langfuse", "api", "prompts", "create", "help"])),
            os(&["langfuse", "api", "prompts", "create", "--help"])
        );
        assert_eq!(
            normalize(os(&[
                "langfuse", "--json", "api", "prompts", "list", "help"
            ])),
            os(&["langfuse", "--json", "api", "prompts", "list", "--help"])
        );
        // `api help prompts create` is clap's own help subcommand.
        assert_eq!(
            normalize(os(&["langfuse", "api", "help", "prompts", "create"])),
            os(&["langfuse", "api", "help", "prompts", "create"])
        );
    }

    #[test]
    fn color_flag_is_read_before_parsing() {
        assert_eq!(
            color_choice(&os(&["langfuse", "--color", "never"])),
            ColorChoice::Never
        );
        assert_eq!(
            color_choice(&os(&["langfuse", "--color=always"])),
            ColorChoice::Always
        );
        assert_eq!(color_choice(&os(&["langfuse"])), ColorChoice::Auto);
    }
}

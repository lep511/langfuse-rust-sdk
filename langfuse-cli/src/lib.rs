//! `langfuse`: a command-line client for the Langfuse API, built on
//! [`langfuse_rust_sdk`].
//!
//! The binary is a thin wrapper around [`run`], which takes the arguments and
//! the standard streams ([`Io`]) and returns the exit code ([`Exit`]), so the
//! whole CLI is testable in-process. That, and the clap tree ([`command`]),
//! is the public API of this crate: the modules are private.

mod api;
mod args;
mod cli;
mod config;
mod error;
mod execute;
mod get_skill;
mod operation;
mod output;
mod paginate;
mod schema;
mod skills;
mod table;

use std::ffi::OsString;

use clap::{ColorChoice, CommandFactory, FromArgMatches};
use futures_util::FutureExt as _;

use crate::cli::{Cli, Command};
use crate::error::{CliError, Result};
use crate::output::escape_terminal_text;

pub use crate::error::Exit;
pub use crate::output::Io;

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
    let word =
        |arg: Option<&OsString>| arg.is_some_and(|arg| !arg.as_encoded_bytes().starts_with(b"-"));
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
///
/// The exit code is always one of [`Exit`]: a panic, which is a bug, is
/// caught and becomes [`Exit::Internal`] after Rust's own panic message.
pub async fn run(args: impl IntoIterator<Item = impl Into<OsString>>, io: &mut Io<'_>) -> Exit {
    let args = normalize(args.into_iter().map(Into::into).collect());
    let outcome = std::panic::AssertUnwindSafe(dispatch(&args, io))
        .catch_unwind()
        .await;
    let exit = match outcome {
        Ok(Ok(exit)) => exit,
        Ok(Err(error)) => report(&error, io),
        Err(_panic) => Exit::Internal,
    };
    // Output may still sit in the stream's buffer; flushing it is part of
    // writing it, so a failure here is a failed write like any other.
    match io.stdout.flush() {
        Err(error) if exit == Exit::Success => report(&CliError::stdout(error), io),
        _ => exit,
    }
}

/// Parse `args` and run the selected command.
async fn dispatch(args: &[OsString], io: &mut Io<'_>) -> Result<Exit> {
    let choice = color_choice(args);
    let command = command().color(choice);

    let matches = match command.try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(error) => return clap_error(&error, choice, io),
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => return clap_error(&error, choice, io),
    };

    match &cli.command {
        Command::Api(api) => api::run(api, &cli.global, io).await,
        Command::Skills(skills) => skills::run(skills, &cli.global, io).await,
        Command::GetSkill => get_skill::run(io).await,
        Command::Completions { shell } => {
            // `clap_complete` panics when its writer fails, so the script is
            // generated in memory and written like any other output.
            let mut script = Vec::new();
            clap_complete::generate(*shell, &mut Cli::command(), "langfuse", &mut script);
            io.out(&script)?;
            Ok(Exit::Success)
        }
    }
}

/// `langfuse get-skill` reading the skill from `url` instead of GitHub, for
/// tests of this crate.
#[doc(hidden)]
pub async fn get_skill_from(url: &str, io: &mut Io<'_>) -> Exit {
    match get_skill::run_from(url, io).await {
        Ok(exit) => exit,
        Err(error) => report(&error, io),
    }
}

/// Print a CLI error to stderr and return its exit code. The reader closing
/// stdout is not an error: nothing is printed and the exit code is 0.
fn report(error: &CliError, io: &mut Io<'_>) -> Exit {
    if error.is_closed_stdout() {
        return Exit::Success;
    }
    let _ = writeln!(io.stderr, "{}", escape_terminal_text(&error.message));
    error.exit
}

/// Help and version go to stdout with exit 0; usage errors to stderr with 2.
fn clap_error(error: &clap::Error, choice: ColorChoice, io: &mut Io<'_>) -> Result<Exit> {
    use clap::error::ErrorKind;
    let styled = error.render();
    let to_stdout = matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    if to_stdout {
        let text = if io.color_stdout(choice) {
            styled.ansi().to_string()
        } else {
            styled.to_string()
        };
        io.out(text.as_bytes())?;
        Ok(Exit::Success)
    } else {
        let text = if io.color_stderr(choice) {
            styled.ansi().to_string()
        } else {
            styled.to_string()
        };
        let _ = write!(io.stderr, "{text}");
        Ok(Exit::Usage)
    }
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

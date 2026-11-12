//! The commands of the chat: the lines that start with `/`.
//!
//! Such a line is for the chat itself and never for the model. When it is
//! not one of the commands, the chat says so, and names the command it looks
//! like when there is one.

use std::fmt;
use std::mem;

/// What every command starts with.
pub const SIGIL: char = '/';
/// The name of the command that takes an argument: the model to change to.
pub const MODEL: &str = "model";

/// A command, as the chat lists it.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    /// What follows the `/`.
    pub name: &'static str,
    /// What the command does, in a few words.
    pub about: &'static str,
    command: Command<'static>,
}

/// The commands, in the order `/help` lists them.
pub static COMMANDS: [Entry; 5] = [
    Entry {
        name: "clear",
        about: "start a new chat",
        command: Command::Clear,
    },
    Entry {
        name: "usage",
        about: "show the tokens used",
        command: Command::Usage,
    },
    Entry {
        name: MODEL,
        about: "show or change the model",
        command: Command::Model(None),
    },
    Entry {
        name: "exit",
        about: "leave the chat",
        command: Command::Exit,
    },
    Entry {
        name: "help",
        about: "list the commands",
        command: Command::Help,
    },
];

/// What a line typed at the prompt asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command<'a> {
    /// Leave the chat.
    Exit,
    /// Start a new conversation on an empty screen.
    Clear,
    /// Show what the conversation has used.
    Usage,
    /// Change to the model with this name, or without one list the models.
    Model(Option<&'a str>),
    /// List the commands.
    Help,
    /// A line that starts with `/` and is not a command. The chat answers
    /// with what is wrong with it, and the line goes no further.
    Invalid(Invalid<'a>),
    /// Anything else is a message for the model.
    Message(&'a str),
}

impl<'a> Command<'a> {
    /// `None` for a blank line.
    pub fn parse(line: &'a str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let Some((_, command, arguments)) = split(line) else {
            return Some(Self::Message(line));
        };
        let typed = &command[SIGIL.len_utf8()..];
        // The line is trimmed: what follows the command, if anything, has
        // text in it.
        let argument = Some(arguments.trim_start()).filter(|argument| !argument.is_empty());
        let invalid = match COMMANDS.iter().find(|entry| entry.name == typed) {
            Some(entry) => match (entry.command, argument) {
                (Self::Model(_), name) => return Some(Self::Model(name)),
                (command, None) => return Some(command),
                (_, Some(_)) => Invalid::Arguments { name: entry.name },
            },
            None => Invalid::Unknown {
                name: typed,
                closest: closest(typed),
            },
        };
        Some(Self::Invalid(invalid))
    }

    /// The command's entry in the chat's lists, whatever its argument.
    /// `None` for a line that is not a command.
    pub fn entry(self) -> Option<&'static Entry> {
        let kind = mem::discriminant(&self);
        COMMANDS
            .iter()
            .find(|entry| mem::discriminant(&entry.command) == kind)
    }
}

/// Why a line that starts with `/` is not a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid<'a> {
    /// No command has this name. `closest` is the command the name looks
    /// like, when there is one.
    Unknown {
        name: &'a str,
        closest: Option<&'static str>,
    },
    /// The command takes no arguments, and the line has some.
    Arguments { name: &'static str },
}

impl fmt::Display for Invalid<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Unknown {
                name,
                closest: Some(closest),
            } => write!(
                f,
                "{SIGIL}{name} is not a command. Did you mean {SIGIL}{closest}?"
            ),
            Self::Unknown {
                name,
                closest: None,
            } => {
                write!(f, "{SIGIL}{name} is not a command. Commands:")?;
                let mut separator = "";
                for entry in &COMMANDS {
                    write!(f, "{separator} {SIGIL}{}", entry.name)?;
                    separator = ",";
                }
                f.write_str(".")
            }
            Self::Arguments { name } => write!(f, "{SIGIL}{name} takes no arguments."),
        }
    }
}

/// Splits a line that starts with `/` around its command: the white space
/// before it, the `/` with the name that follows, and the rest of the line.
/// `None` for any other line.
pub fn split(line: &str) -> Option<(&str, &str, &str)> {
    let (blank, rest) = line.split_at(line.len() - line.trim_start().len());
    if !rest.starts_with(SIGIL) {
        return None;
    }
    let (command, rest) = rest.split_at(rest.find(char::is_whitespace).unwrap_or(rest.len()));
    Some((blank, command, rest))
}

/// The command that `typed` looks like, when there is one: a command it is
/// the beginning of, or one that it is within one typing slip per three
/// characters of. Of several, the nearest.
fn closest(typed: &str) -> Option<&'static str> {
    let typed = typed.to_lowercase();
    if typed.is_empty() {
        return None;
    }
    let typed_len = typed.chars().count();
    COMMANDS
        .iter()
        .filter_map(|entry| {
            let name = entry.name;
            let distance = edit_distance(&typed, name);
            let allowed = typed_len.max(name.chars().count()) / 3;
            (name.starts_with(&typed) || distance <= allowed).then_some((distance, name))
        })
        .min_by_key(|&(distance, _)| distance)
        .map(|(_, name)| name)
}

/// The typing slips between `a` and `b`: the characters to add, drop or
/// replace and the neighbours to swap that turn one into the other (their
/// optimal string alignment distance).
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // Three rows of the table of distances between the beginnings of `a`
    // and of `b`: for the beginning of `a` two characters back, for the one
    // a character back, and for the current one.
    let mut before_last = vec![0; b.len() + 1];
    let mut last: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        current[0] = i;
        for j in 1..=b.len() {
            let replaced = usize::from(a[i - 1] != b[j - 1]);
            let mut distance = (last[j] + 1)
                .min(current[j - 1] + 1)
                .min(last[j - 1] + replaced);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                distance = distance.min(before_last[j - 2] + 1);
            }
            current[j] = distance;
        }
        std::mem::swap(&mut before_last, &mut last);
        std::mem::swap(&mut last, &mut current);
    }
    last[b.len()]
}

#[cfg(test)]
mod tests {
    use super::{COMMANDS, Command, Invalid, closest, edit_distance, split};

    fn unknown<'a>(name: &'a str, closest: Option<&'static str>) -> Option<Command<'a>> {
        Some(Command::Invalid(Invalid::Unknown { name, closest }))
    }

    #[test]
    fn commands_are_whole_lines() {
        assert_eq!(Command::parse("  /exit \n"), Some(Command::Exit));
        assert_eq!(Command::parse("/clear"), Some(Command::Clear));
        assert_eq!(Command::parse("/usage"), Some(Command::Usage));
        assert_eq!(Command::parse("/help"), Some(Command::Help));
        assert_eq!(Command::parse("/model"), Some(Command::Model(None)));
        assert_eq!(Command::parse("  \t"), None);
    }

    #[test]
    fn the_model_command_takes_the_rest_of_the_line() {
        assert_eq!(
            Command::parse("/model sonnet"),
            Some(Command::Model(Some("sonnet")))
        );
        assert_eq!(
            Command::parse("  /model \t claude-opus-4-6 1m \n"),
            Some(Command::Model(Some("claude-opus-4-6 1m")))
        );
        assert_eq!(Command::parse("/model  "), Some(Command::Model(None)));
        // With or without a name, it is the same command of the lists.
        let entry = Command::Model(None).entry().unwrap();
        assert_eq!(entry.name, "model");
        assert_eq!(Command::Model(Some("sonnet")).entry(), Some(entry));
    }

    #[test]
    fn every_listed_command_is_one() {
        for entry in &COMMANDS {
            let line = format!("/{}", entry.name);
            let command = Command::parse(&line);
            assert_eq!(command, Some(entry.command), "{line}");
            assert_eq!(command.and_then(Command::entry), Some(entry));
        }
        assert_eq!(Command::Message("/model").entry(), None);
        assert_eq!(Command::parse("/nope").and_then(Command::entry), None);
    }

    #[test]
    fn a_line_is_a_message_unless_it_starts_with_a_slash() {
        assert_eq!(Command::parse(" hola "), Some(Command::Message("hola")));
        assert_eq!(
            Command::parse("what does /clear do?"),
            Some(Command::Message("what does /clear do?"))
        );
        assert_eq!(Command::parse("exit"), Some(Command::Message("exit")));
    }

    #[test]
    fn a_line_that_starts_with_a_slash_is_never_a_message() {
        for line in [
            "/",
            "/ hola",
            "//",
            "/clera",
            "  /Exit",
            "/exit/",
            "/usage of what?",
            "/home/me/notes.txt what is this?",
            "/¿qué?",
            "\t/quit\n",
        ] {
            assert!(
                matches!(Command::parse(line), Some(Command::Invalid(_))),
                "{line:?}"
            );
        }
    }

    #[test]
    fn a_command_takes_no_arguments() {
        assert_eq!(
            Command::parse("/usage of what?"),
            Some(Command::Invalid(Invalid::Arguments { name: "usage" }))
        );
        assert_eq!(
            Command::parse("/exit\tnow"),
            Some(Command::Invalid(Invalid::Arguments { name: "exit" }))
        );
    }

    #[test]
    fn an_unknown_command_gets_the_one_it_looks_like() {
        // A typing slip: a swap, a missing, an extra or a wrong character.
        assert_eq!(Command::parse("/clera"), unknown("clera", Some("clear")));
        assert_eq!(Command::parse("/usge"), unknown("usge", Some("usage")));
        assert_eq!(Command::parse("/exitt"), unknown("exitt", Some("exit")));
        assert_eq!(Command::parse("/edit"), unknown("edit", Some("exit")));
        // The beginning of a command, however short.
        assert_eq!(Command::parse("/c"), unknown("c", Some("clear")));
        assert_eq!(Command::parse("/us"), unknown("us", Some("usage")));
        // Another case, and arguments after it.
        assert_eq!(Command::parse("/EXIT"), unknown("EXIT", Some("exit")));
        assert_eq!(
            Command::parse("/Clear all"),
            unknown("Clear", Some("clear"))
        );
        // Two slips in a longer name.
        assert_eq!(
            Command::parse("/cleared"),
            unknown("cleared", Some("clear"))
        );
        assert_eq!(Command::parse("/hlep"), unknown("hlep", Some("help")));
        assert_eq!(Command::parse("/modle x"), unknown("modle", Some("model")));
    }

    #[test]
    fn an_unknown_command_unlike_any_other_gets_none() {
        for name in ["", "quit", "reset", "clr", "x", "user", "home/me/notes.txt"] {
            assert_eq!(closest(name), None, "{name:?}");
        }
        assert_eq!(Command::parse("/reset it"), unknown("reset", None));
        assert_eq!(Command::parse("/"), unknown("", None));
        assert_eq!(Command::parse("/ clear"), unknown("", None));
    }

    #[test]
    fn the_nearest_command_wins() {
        // One slip from `exit`, and more from the others.
        assert_eq!(closest("exlt"), Some("exit"));
        assert_eq!(closest("e"), Some("exit"));
        assert_eq!(closest("usagee"), Some("usage"));
    }

    #[test]
    fn invalid_lines_say_what_is_wrong() {
        let message = |line| match Command::parse(line) {
            Some(Command::Invalid(invalid)) => invalid.to_string(),
            other => panic!("{line:?} parsed as {other:?}"),
        };
        assert_eq!(
            message("/clera"),
            "/clera is not a command. Did you mean /clear?"
        );
        assert_eq!(
            message("/reset"),
            "/reset is not a command. Commands: /clear, /usage, /model, /exit, /help."
        );
        assert_eq!(
            message("/"),
            "/ is not a command. Commands: /clear, /usage, /model, /exit, /help."
        );
        assert_eq!(message("/usage of what?"), "/usage takes no arguments.");
    }

    #[test]
    fn split_finds_the_command_of_a_line() {
        assert_eq!(split("/clear"), Some(("", "/clear", "")));
        assert_eq!(
            split("  /usage of  what? "),
            Some(("  ", "/usage", " of  what? "))
        );
        assert_eq!(split("/"), Some(("", "/", "")));
        assert_eq!(split("/ clear"), Some(("", "/", " clear")));
        assert_eq!(split("hola /clear"), None);
        assert_eq!(split(""), None);
        assert_eq!(split("   "), None);
    }

    #[test]
    fn edit_distance_counts_typing_slips() {
        assert_eq!(edit_distance("clear", "clear"), 0);
        assert_eq!(edit_distance("", "exit"), 4);
        assert_eq!(edit_distance("exit", ""), 4);
        assert_eq!(edit_distance("cler", "clear"), 1);
        assert_eq!(edit_distance("clearr", "clear"), 1);
        assert_eq!(edit_distance("cleat", "clear"), 1);
        // Swapped neighbours are one slip, not two.
        assert_eq!(edit_distance("clera", "clear"), 1);
        assert_eq!(edit_distance("xeit", "exit"), 1);
        assert_eq!(edit_distance("quit", "exit"), 2);
        assert_eq!(edit_distance("año", "ano"), 1);
    }
}

//! The menus under the prompt.
//!
//! A line that is a `/` and the beginning of a command's name has the
//! commands it could become listed under it, the most used first. A command
//! that takes an argument has, once it is typed, a menu of what the argument
//! can be; Enter on the command alone opens that menu on the value in use.
//! Down steps into the list and through it, and Up back; each step puts the
//! row it reaches on the line, and past either end of the list the line is
//! again what was typed.
//!
//! The line editor does the stepping, with what it has for cycling through
//! completions (see [`crate::input`]). This module decides what the keys
//! mean, and keeps the arrows walking the history of earlier lines on a line
//! they brought back from it.

use std::iter;

use crate::command::{self, Entry, SIGIL};

/// Ends a style.
pub const RESET_STYLE: &str = "\x1b[0m";
/// Before the row of the menu that is on the line.
const CHOSEN_MARK: char = '>';

/// The ANSI sequences that start the styles of a line with a command, and of
/// the menu under it.
pub struct Styles {
    /// A command: the one of the line, and what the rows of the menu list.
    pub command: String,
    /// The row of the menu that is on the line.
    pub chosen: String,
    /// What a row is about.
    pub about: String,
}

/// What the argument of a command can be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    /// The text of the argument.
    pub name: String,
    /// What it is, in a few words.
    pub about: String,
    /// Whether it is the one in use: the row a menu opens on.
    pub current: bool,
}

/// What the menus list.
#[derive(Default)]
pub struct Choices {
    /// The commands, in the order to list them.
    pub commands: Vec<&'static Entry>,
    /// What the argument of a command can be, by the command's name and in
    /// the order to list them.
    pub arguments: Vec<(&'static str, Vec<Value>)>,
}

impl Choices {
    /// The rows of the longest menu.
    pub fn rows(&self) -> usize {
        let arguments = self.arguments.iter().map(|(_, values)| values.len());
        arguments.chain([self.commands.len()]).max().unwrap_or(0)
    }
}

/// What a key that steps through the menu has the editor do.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Go on through the lines it is cycling through or, when it is not
    /// cycling, start on those that [`Menu::begin`] gives it.
    Forward,
    /// Go back through the lines it is cycling through.
    Backward,
    /// What it does without a menu: the arrows walk the history of earlier
    /// lines.
    History,
}

/// The menu of one line, from its first key to its last.
#[derive(Default)]
pub struct Menu {
    choices: Choices,
    /// The line whose menu is being stepped through: what was typed, which
    /// the line is again past either end of the list. It lasts as long as
    /// the line is that one or a row of its menu.
    origin: Option<String>,
    /// Whether the editor is cycling through the lines of [`Menu::begin`]:
    /// from the key that starts it to the first that is not a step.
    cycling: bool,
    /// Whether it cycles through them from the last to the first: it was
    /// Up that started it.
    reversed: bool,
    /// An arrow was left to the history, which has not shown its line yet.
    moving: bool,
    /// The line the user left for the history.
    own: Option<String>,
    /// The line the history brought back, while it is the one on the prompt.
    /// It has no menu, so that the arrows go on through the history.
    recalled: Option<String>,
}

impl Menu {
    /// Starts over for a new line, with what its menus list.
    pub fn start(&mut self, choices: Choices) {
        *self = Self {
            choices,
            ..Self::default()
        };
    }

    /// What the menu shows under `line`, if anything.
    pub fn view(&self, line: &str) -> Option<View> {
        if self.recalled.as_deref() == Some(line) {
            return None;
        }
        // While it is stepped through, the menu is the one of the line the
        // steps started from.
        if let Some(origin) = &self.origin {
            let rows = self.rows(origin);
            let chosen = rows.iter().position(|row| row.line == line);
            if chosen.is_some() || line == origin {
                return Some(View { rows, chosen });
            }
        }
        let rows = self.rows(line);
        (!rows.is_empty()).then_some(View { rows, chosen: None })
    }

    /// The rows of the menu of `line`.
    ///
    /// A line that is a `/` and the beginning of a name has the commands the
    /// name could become. One that goes on after a command has what the
    /// command's argument can be, of those with the rest of the line in
    /// them. Both whatever the case of what was typed.
    fn rows(&self, line: &str) -> Vec<Row> {
        let Some((_, command, rest)) = command::split(line) else {
            return Vec::new();
        };
        let name = &command[SIGIL.len_utf8()..];
        if rest.is_empty() {
            let typed = name.to_lowercase();
            let could_be = |entry: &&&Entry| entry.name.starts_with(&typed);
            let row = |entry: &&Entry| {
                let line = format!("{SIGIL}{}", entry.name);
                Row {
                    label: line.clone(),
                    line,
                    about: entry.about.to_owned(),
                }
            };
            let commands = self.choices.commands.iter();
            return commands.filter(could_be).map(row).collect();
        }
        let typed = rest.trim_start().to_lowercase();
        let has = |value: &&Value| value.name.to_lowercase().contains(&typed);
        let row = |value: &Value| Row {
            line: format!("{command} {}", value.name),
            label: value.name.clone(),
            about: value.about.clone(),
        };
        self.values(name).iter().filter(has).map(row).collect()
    }

    /// What the argument of the command called `name` can be.
    fn values(&self, name: &str) -> &[Value] {
        let values = self.choices.arguments.iter().find(|(of, _)| *of == name);
        values.map_or(&[], |(_, values)| values)
    }

    /// Takes note of the line the editor is about to draw.
    pub fn drawn(&mut self, line: &str) {
        if std::mem::take(&mut self.moving) {
            // The history has moved: to an earlier line, or back to the one
            // the user left.
            self.recalled = (self.own.as_deref() != Some(line)).then(|| line.to_owned());
        } else if self
            .recalled
            .as_deref()
            .is_some_and(|recalled| recalled != line)
        {
            // Edited, the line is the user's own.
            self.recalled = None;
        }
        self.keep_origin(line);
    }

    /// Forgets the line the steps started from once `line` is neither that
    /// one nor a row of its menu: it was edited.
    fn keep_origin(&mut self, line: &str) {
        let kept = self.origin.as_ref().is_some_and(|origin| {
            line == origin || self.rows(origin).iter().any(|row| row.line == line)
        });
        if !kept {
            self.origin = None;
        }
    }

    /// A key that steps through the menu was pressed on `line`: Down or a
    /// key like it (`down`), or Up or one like it.
    pub fn step(&mut self, line: &str, down: bool) -> Step {
        self.keep_origin(line);
        let chosen = self.view(line).map(|view| view.chosen);
        // Up steps back through the list. From what was typed, above the
        // list, it is not a step.
        let steps = match chosen {
            Some(chosen) => down || chosen.is_some(),
            None => false,
        };
        if !steps {
            self.cycling = false;
            return Step::History;
        }
        if !self.cycling {
            self.reversed = !down;
            return Step::Forward;
        }
        if down == self.reversed {
            Step::Backward
        } else {
            Step::Forward
        }
    }

    /// An arrow pressed on `line` was left to the editor, for the history.
    pub fn left_to_history(&mut self, line: &str) {
        self.moving = true;
        if self.recalled.is_none() {
            self.own = Some(line.to_owned());
        }
    }

    /// A key that is not a step was pressed: it ends the editor's cycling.
    pub fn other_key(&mut self) {
        self.cycling = false;
        // The history had nowhere to move to.
        self.moving = false;
    }

    /// Enter was pressed on `line`, with the cursor at its end or not.
    ///
    /// On a command that takes an argument and has none yet, Enter opens
    /// the menu of what the argument can be, on the one in use. Returns
    /// what to add to the line for that, in place of entering the line.
    pub fn enter(&mut self, line: &str, at_end: bool) -> Option<String> {
        self.other_key();
        // The space that goes before an argument may be there already.
        let command = line.strip_suffix(' ').unwrap_or(line);
        let space = if command.len() == line.len() { " " } else { "" };
        let name = command.strip_prefix(SIGIL).filter(|_| at_end)?;
        let current = self.values(name).iter().find(|value| value.current)?;
        let added = format!("{space}{}", current.name);
        // The menu is the one of the command and a space, which is what
        // the line is again past either end of the list.
        self.origin = Some(format!("{command} "));
        Some(added)
    }

    /// The editor starts cycling through the menu of `line`. Returns the
    /// lines it cycles through, in order: none when the line has no menu.
    ///
    /// The editor shows each in turn and, after the last, the line it
    /// started from. With what was typed and then each row as a round,
    /// these are the other lines of the round: from the next one on, or
    /// from the one before back when it was Up that started it.
    pub fn begin(&mut self, line: &str) -> Vec<String> {
        self.keep_origin(line);
        self.cycling = false;
        let Some(view) = self.view(line) else {
            return Vec::new();
        };
        let origin = self.origin.get_or_insert_with(|| line.to_owned()).clone();
        let rows = view.rows.into_iter().map(|row| row.line);
        let round: Vec<String> = iter::once(origin).chain(rows).collect();
        let at = view.chosen.map_or(0, |row| row + 1);
        let (lines, reversed) = (round.len(), self.reversed);
        let other = |by: usize| {
            let step = if reversed { lines - by } else { by };
            round[(at + step) % lines].clone()
        };
        self.cycling = true;
        (1..lines).map(other).collect()
    }
}

/// A row of a menu.
#[derive(Debug, PartialEq, Eq)]
struct Row {
    /// The line the row stands for.
    line: String,
    /// What the row shows of the line: the part the menu is about.
    label: String,
    /// What that is, in a few words.
    about: String,
}

/// The menu as it shows: its rows, and the one that is on the line.
#[derive(Debug, PartialEq, Eq)]
pub struct View {
    rows: Vec<Row>,
    chosen: Option<usize>,
}

impl View {
    /// The text of the menu, a line for each row, to draw from the end of
    /// the prompt's line. `styles` add colors to the same text.
    pub fn render(&self, styles: Option<&Styles>) -> String {
        let width = self.rows.iter().map(|row| row.label.chars().count()).max();
        let width = width.unwrap_or(0);
        let mut text = String::new();
        for (index, Row { label, about, .. }) in self.rows.iter().enumerate() {
            let chosen = self.chosen == Some(index);
            let mark = if chosen { CHOSEN_MARK } else { ' ' };
            // Nothing after a label that is all there is to say.
            let gap = if about.is_empty() { "" } else { "  " };
            text += &match styles {
                Some(Styles { chosen: style, .. }) if chosen => {
                    format!("\n{style}{mark} {label:<width$}{gap}{about}{RESET_STYLE}")
                }
                Some(Styles {
                    command: style,
                    about: about_style,
                    ..
                }) => format!(
                    "\n{mark} {style}{label:<width$}{RESET_STYLE}{gap}\
                     {about_style}{about}{RESET_STYLE}"
                ),
                None => format!("\n{mark} {label:<width$}{gap}{about}"),
            };
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::{Choices, Menu, Step, Styles, Value};
    use crate::command::COMMANDS;

    /// A menu of the commands, in the order of `names`, and of three models
    /// for the argument of `/model`, the second one in use.
    fn menu(names: &[&str]) -> Menu {
        let commands = names
            .iter()
            .map(|name| COMMANDS.iter().find(|entry| entry.name == *name).unwrap())
            .collect();
        let value = |name: &str, about: &str| Value {
            name: name.to_owned(),
            about: about.to_owned(),
            current: about.ends_with("current"),
        };
        let models = vec![
            value("claude-opus-4-6 1m", "1M context"),
            value("claude-opus-4-6", "200k context · current"),
            value("claude-sonnet-4-6", ""),
        ];
        let mut menu = Menu::default();
        menu.start(Choices {
            commands,
            arguments: vec![("model", models)],
        });
        menu
    }

    fn usual() -> Menu {
        menu(&["usage", "clear", "model", "exit", "help"])
    }

    /// A menu of four commands, to step through.
    fn short() -> Menu {
        menu(&["usage", "clear", "exit", "help"])
    }

    /// What the menu's rows show under `line`, the one on the line marked
    /// with `>`.
    fn rows(menu: &Menu, line: &str) -> Vec<String> {
        let Some(view) = menu.view(line) else {
            return Vec::new();
        };
        let mark = |index| if view.chosen == Some(index) { ">" } else { "" };
        let row = |(index, label)| format!("{}{label}", mark(index));
        let labels = view.rows.iter().map(|row| &row.label);
        labels.enumerate().map(row).collect()
    }

    /// An arrow pressed on `line`, as the editor's key for it handles it.
    fn arrow(menu: &mut Menu, line: &str, down: bool) -> Step {
        let step = menu.step(line, down);
        if step == Step::History {
            menu.left_to_history(line);
        }
        step
    }

    #[test]
    fn the_longest_menu_is_the_one_with_most_rows() {
        let choices = |commands: usize, models: usize| {
            let model = Value {
                name: "model".to_owned(),
                about: String::new(),
                current: false,
            };
            Choices {
                commands: COMMANDS.iter().take(commands).collect(),
                arguments: vec![("model", vec![model; models]), ("other", Vec::new())],
            }
        };
        assert_eq!(choices(5, 3).rows(), 5);
        assert_eq!(choices(2, 7).rows(), 7);
        assert_eq!(Choices::default().rows(), 0);
    }

    #[test]
    fn a_slash_lists_the_commands_in_the_order_given() {
        assert_eq!(
            rows(&usual(), "/"),
            ["/usage", "/clear", "/model", "/exit", "/help"]
        );
        assert_eq!(
            rows(&menu(&["help", "exit", "clear"]), "/"),
            ["/help", "/exit", "/clear"]
        );
    }

    #[test]
    fn the_beginning_of_a_name_lists_the_commands_it_could_become() {
        let menu = usual();
        assert_eq!(rows(&menu, "/e"), ["/exit"]);
        assert_eq!(rows(&menu, "/us"), ["/usage"]);
        assert_eq!(rows(&menu, "/usage"), ["/usage"]);
        assert_eq!(rows(&menu, "  /CL"), ["/clear"]);
        assert_eq!(rows(&menu, "/model"), ["/model"]);
    }

    #[test]
    fn other_lines_have_no_menu() {
        let menu = usual();
        for line in ["", "hola", "what does /clear do?", "/x", "/usagee"] {
            assert_eq!(menu.view(line), None, "{line:?}");
        }
        // The command is over, and it takes no argument.
        for line in ["/usage of what?", "/clear ", "/ clear", "/ model"] {
            assert_eq!(menu.view(line), None, "{line:?}");
        }
    }

    #[test]
    fn a_command_with_an_argument_lists_what_it_can_be() {
        let menu = usual();
        let all = ["claude-opus-4-6 1m", "claude-opus-4-6", "claude-sonnet-4-6"];
        assert_eq!(rows(&menu, "/model "), all);
        assert_eq!(rows(&menu, "  /model \t"), all);
        // Those with what was typed anywhere in them, in any case.
        assert_eq!(
            rows(&menu, "/model OPUS"),
            ["claude-opus-4-6 1m", "claude-opus-4-6"]
        );
        assert_eq!(rows(&menu, "/model sonnet"), ["claude-sonnet-4-6"]);
        assert_eq!(
            rows(&menu, "/model claude-opus-4-6 "),
            ["claude-opus-4-6 1m"]
        );
        assert_eq!(menu.view("/model gpt"), None);
        // Only the command that has them.
        assert_eq!(menu.view("/MODEL opus"), None);
        assert_eq!(menu.view("/models opus"), None);
    }

    #[test]
    fn down_steps_into_the_menu_and_up_back_out_of_it() {
        let mut menu = short();
        // Up from what was typed is for the history, as without a menu.
        assert_eq!(arrow(&mut menu, "/", false), Step::History);
        menu.other_key();
        menu.drawn("/");

        assert_eq!(arrow(&mut menu, "/", true), Step::Forward);
        assert_eq!(menu.begin("/"), ["/usage", "/clear", "/exit", "/help"]);
        // The editor puts each on the line: the menu stays whole, and
        // marks the one that is there.
        assert_eq!(
            rows(&menu, "/usage"),
            [">/usage", "/clear", "/exit", "/help"]
        );
        assert_eq!(arrow(&mut menu, "/usage", true), Step::Forward);
        assert_eq!(
            rows(&menu, "/clear"),
            ["/usage", ">/clear", "/exit", "/help"]
        );
        assert_eq!(arrow(&mut menu, "/clear", false), Step::Backward);
        assert_eq!(arrow(&mut menu, "/usage", false), Step::Backward);
        // Past the first one, the line is what was typed.
        assert_eq!(rows(&menu, "/"), ["/usage", "/clear", "/exit", "/help"]);
        assert_eq!(arrow(&mut menu, "/", true), Step::Forward);
        // And from there, Up leaves the menu.
        assert_eq!(arrow(&mut menu, "/", false), Step::History);
    }

    #[test]
    fn steps_go_on_from_the_row_on_the_line() {
        let mut menu = short();
        menu.begin("/");
        // A key that leaves the line as it is: the editor is done cycling,
        // and the menu is still the one of what was typed.
        menu.other_key();
        let stepping = ["/usage", ">/clear", "/exit", "/help"];
        assert_eq!(rows(&menu, "/clear"), stepping);
        // Down: the editor starts on the lines after this one, and ends
        // its round on it.
        assert_eq!(arrow(&mut menu, "/clear", true), Step::Forward);
        assert_eq!(menu.begin("/clear"), ["/exit", "/help", "/", "/usage"]);
        assert_eq!(arrow(&mut menu, "/exit", true), Step::Forward);
        assert_eq!(arrow(&mut menu, "/help", false), Step::Backward);
        menu.other_key();
        // Up: on the lines before it, and back through them.
        assert_eq!(arrow(&mut menu, "/clear", false), Step::Forward);
        assert_eq!(menu.begin("/clear"), ["/usage", "/", "/help", "/exit"]);
        assert_eq!(
            rows(&menu, "/usage"),
            [">/usage", "/clear", "/exit", "/help"]
        );
        assert_eq!(arrow(&mut menu, "/usage", true), Step::Backward);
        assert_eq!(arrow(&mut menu, "/clear", false), Step::Forward);
        // What was typed is a line of the round as the rows are: Down goes
        // on from it, and Up leaves the menu.
        assert_eq!(arrow(&mut menu, "/", true), Step::Backward);
        assert_eq!(arrow(&mut menu, "/", false), Step::History);
    }

    #[test]
    fn the_arrows_step_through_the_rows_that_were_listed() {
        let mut menu = usual();
        assert_eq!(menu.begin("/e"), ["/exit"]);
        assert_eq!(rows(&menu, "/exit"), [">/exit"]);
        assert_eq!(rows(&menu, "/e"), ["/exit"]);
        // From the only row, the round goes back to what was typed.
        menu.other_key();
        assert_eq!(menu.begin("/exit"), ["/e"]);
        // A line without a menu has nothing to step through.
        assert!(menu.begin("hola").is_empty());
        assert!(menu.begin("/x").is_empty());
        assert_eq!(arrow(&mut menu, "/x", true), Step::History);
    }

    #[test]
    fn the_arrows_put_a_command_and_its_argument_on_the_line() {
        let mut menu = usual();
        assert_eq!(arrow(&mut menu, "/model op", true), Step::Forward);
        assert_eq!(
            menu.begin("/model op"),
            ["/model claude-opus-4-6 1m", "/model claude-opus-4-6"]
        );
        assert_eq!(
            rows(&menu, "/model claude-opus-4-6"),
            ["claude-opus-4-6 1m", ">claude-opus-4-6"]
        );
        assert_eq!(
            arrow(&mut menu, "/model claude-opus-4-6", false),
            Step::Backward
        );
        assert_eq!(
            rows(&menu, "/model op"),
            ["claude-opus-4-6 1m", "claude-opus-4-6"]
        );
    }

    #[test]
    fn an_edit_ends_the_stepping() {
        let mut menu = short();
        menu.begin("/");
        assert_eq!(
            rows(&menu, "/clear"),
            ["/usage", ">/clear", "/exit", "/help"]
        );
        // Backspace: the menu is the one of the line as it stands.
        menu.other_key();
        menu.drawn("/clea");
        assert_eq!(rows(&menu, "/clea"), ["/clear"]);
        // Even when the line is again one that was stepped through.
        menu.other_key();
        menu.drawn("/clear");
        assert_eq!(rows(&menu, "/clear"), ["/clear"]);
        assert_eq!(arrow(&mut menu, "/clear", false), Step::History);
    }

    #[test]
    fn enter_on_a_command_without_its_argument_opens_the_menu_of_it() {
        let mut menu = usual();
        // What Enter adds puts the value in use on the line.
        assert_eq!(
            menu.enter("/model", true).as_deref(),
            Some(" claude-opus-4-6")
        );
        let line = "/model claude-opus-4-6";
        menu.drawn(line);
        // The menu is whole, on that row.
        assert_eq!(
            rows(&menu, line),
            [
                "claude-opus-4-6 1m",
                ">claude-opus-4-6",
                "claude-sonnet-4-6"
            ]
        );
        // Up and Down step from it, through the command and a space past
        // either end.
        assert_eq!(arrow(&mut menu, line, false), Step::Forward);
        assert_eq!(
            menu.begin(line),
            [
                "/model claude-opus-4-6 1m",
                "/model ",
                "/model claude-sonnet-4-6"
            ]
        );
        menu.other_key();
        assert_eq!(arrow(&mut menu, line, true), Step::Forward);
        assert_eq!(
            menu.begin(line),
            [
                "/model claude-sonnet-4-6",
                "/model ",
                "/model claude-opus-4-6 1m"
            ]
        );
        assert_eq!(
            rows(&menu, "/model "),
            ["claude-opus-4-6 1m", "claude-opus-4-6", "claude-sonnet-4-6"]
        );
        // There, with the space already typed, Enter opens it again.
        assert_eq!(
            menu.enter("/model ", true).as_deref(),
            Some("claude-opus-4-6")
        );
    }

    #[test]
    fn enter_enters_any_other_line() {
        let mut menu = usual();
        let entered = [
            // The argument is there: the row chosen, or what was typed.
            ("/model claude-sonnet-4-6", true),
            ("/model son", true),
            // The cursor is not where the argument goes.
            ("/model", false),
            // Not the command alone.
            ("  /model", true),
            ("/model  ", true),
            ("/MODEL", true),
            // Commands without an argument, and lines without a command.
            ("/usage", true),
            ("/", true),
            ("", true),
            ("hola", true),
        ];
        for (line, at_end) in entered {
            assert_eq!(menu.enter(line, at_end), None, "{line:?}");
        }
        // Nor a command whose argument has no value in use.
        let mut menu = Menu::default();
        let value = Value {
            name: "opus".to_owned(),
            about: String::new(),
            current: false,
        };
        menu.start(Choices {
            commands: Vec::new(),
            arguments: vec![("model", vec![value])],
        });
        assert_eq!(menu.enter("/model", true), None);
    }

    #[test]
    fn a_line_from_the_history_has_no_menu_until_it_is_edited() {
        let mut menu = usual();
        assert_eq!(arrow(&mut menu, "", false), Step::History);
        menu.drawn("/usage");
        assert_eq!(menu.view("/usage"), None);
        // The arrows go on through the history, in both directions.
        assert_eq!(arrow(&mut menu, "/usage", false), Step::History);
        menu.drawn("/model sonnet");
        assert_eq!(menu.view("/model sonnet"), None);
        assert_eq!(arrow(&mut menu, "/model sonnet", true), Step::History);
        menu.drawn("/usage");
        assert_eq!(menu.view("/usage"), None);
        assert!(menu.begin("/usage").is_empty());
        // A key that leaves the line as it is changes nothing.
        menu.other_key();
        assert_eq!(arrow(&mut menu, "/usage", true), Step::History);
        menu.other_key();
        // Edited, it is a line like any other, even as it was.
        menu.drawn("/usag");
        assert_eq!(rows(&menu, "/usag"), ["/usage"]);
        menu.other_key();
        menu.drawn("/usage");
        assert_eq!(rows(&menu, "/usage"), ["/usage"]);
        assert_eq!(arrow(&mut menu, "/usage", true), Step::Forward);
    }

    #[test]
    fn enter_opens_the_menu_of_a_line_from_the_history_too() {
        let mut menu = usual();
        assert_eq!(arrow(&mut menu, "", false), Step::History);
        menu.drawn("/model");
        assert_eq!(menu.view("/model"), None);
        assert_eq!(
            menu.enter("/model", true).as_deref(),
            Some(" claude-opus-4-6")
        );
        menu.drawn("/model claude-opus-4-6");
        assert_eq!(
            rows(&menu, "/model claude-opus-4-6"),
            [
                "claude-opus-4-6 1m",
                ">claude-opus-4-6",
                "claude-sonnet-4-6"
            ]
        );
    }

    #[test]
    fn the_line_left_for_the_history_has_its_menu_back() {
        let mut menu = short();
        menu.drawn("/");
        assert_eq!(arrow(&mut menu, "/", false), Step::History);
        menu.drawn("hola");
        assert_eq!(arrow(&mut menu, "hola", false), Step::History);
        menu.drawn("/clear");
        assert_eq!(menu.view("/clear"), None);
        assert_eq!(arrow(&mut menu, "/clear", true), Step::History);
        menu.drawn("hola");
        assert_eq!(arrow(&mut menu, "hola", true), Step::History);
        menu.drawn("/");
        assert_eq!(rows(&menu, "/"), ["/usage", "/clear", "/exit", "/help"]);
        assert_eq!(arrow(&mut menu, "/", true), Step::Forward);
    }

    #[test]
    fn an_arrow_the_history_does_not_follow_hides_no_menu() {
        let mut menu = menu(&["usage", "exit"]);
        // No earlier line: nothing is drawn.
        assert_eq!(arrow(&mut menu, "", false), Step::History);
        assert_eq!(arrow(&mut menu, "", true), Step::History);
        menu.other_key();
        menu.drawn("/");
        assert_eq!(rows(&menu, "/"), ["/usage", "/exit"]);
    }

    #[test]
    fn a_key_that_steps_without_walking_the_history_hides_no_menu() {
        let mut menu = menu(&["usage", "exit"]);
        // Tab on a line without a menu: not a step, and not for the
        // history either.
        assert_eq!(menu.step("", true), Step::History);
        menu.drawn("/");
        assert_eq!(rows(&menu, "/"), ["/usage", "/exit"]);
    }

    #[test]
    fn a_new_line_starts_over() {
        let mut menu = usual();
        menu.begin("/");
        arrow(&mut menu, "hola", false);
        menu.drawn("/exit");
        menu.start(Choices {
            commands: vec![&COMMANDS[0]],
            arguments: Vec::new(),
        });
        assert_eq!(rows(&menu, "/"), [format!("/{}", COMMANDS[0].name)]);
        assert_eq!(arrow(&mut menu, "/", false), Step::History);
        assert_eq!(menu.view("/model "), None);
    }

    #[test]
    fn the_menu_is_drawn_under_the_line() {
        let mut menu = menu(&["usage", "exit"]);
        assert_eq!(
            menu.view("/").unwrap().render(None),
            "\n  /usage  show the tokens used\n  /exit   leave the chat"
        );
        menu.begin("/");
        assert_eq!(
            menu.view("/exit").unwrap().render(None),
            "\n  /usage  show the tokens used\n> /exit   leave the chat"
        );
        // What an argument can be, without the command.
        menu.other_key();
        menu.begin("/model ");
        assert_eq!(
            menu.view("/model claude-sonnet-4-6").unwrap().render(None),
            "\n  claude-opus-4-6 1m  1M context\
             \n  claude-opus-4-6     200k context · current\
             \n> claude-sonnet-4-6 "
        );
    }

    #[test]
    fn styles_color_the_same_text() {
        let styles = Styles {
            command: "<c>".to_owned(),
            chosen: "<x>".to_owned(),
            about: "<a>".to_owned(),
        };
        let mut menu = menu(&["usage", "exit"]);
        menu.begin("/");
        let view = menu.view("/exit").unwrap();
        let styled = view.render(Some(&styles));
        assert_eq!(
            styled,
            "\n  <c>/usage\x1b[0m  <a>show the tokens used\x1b[0m\
             \n<x>> /exit   leave the chat\x1b[0m"
        );
        let plain = ["<c>", "<x>", "<a>", "\x1b[0m"]
            .iter()
            .fold(styled, |text, style| text.replace(style, ""));
        assert_eq!(plain, view.render(None));
    }
}

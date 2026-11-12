//! Line input for the chat: a `rustyline` editor on a thread of its own.
//!
//! `rustyline` gives the prompt its editing keys: Left/Right (and Home/End)
//! move the cursor, Up/Down walk through the lines already entered, and
//! Ctrl+C or Ctrl+D leave. A line that starts with `/` shows its command in
//! a style of its own as it is typed (see [`crate::command`]), and under it
//! the menu of the commands it could become, or of what the argument of its
//! command can be, which Down and Up step through and Enter opens (see
//! [`crate::menu`]).
//! When stdin is not a terminal it reads plain lines instead, so the chat
//! can also be fed from a pipe.
//!
//! `readline` blocks, and the agent system needs the main thread to keep
//! running its tasks, hence the thread. It is a plain thread rather than
//! `spawn_blocking` because the runtime waits for its blocking tasks when it
//! shuts down, and a `readline` waiting for input would never let it.

use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread;

use rustyline::completion::Completer;
use rustyline::config::BellStyle;
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::{Hint, Hinter};
use rustyline::history::DefaultHistory;
use rustyline::line_buffer::LineBuffer;
use rustyline::validate::Validator;
use rustyline::{
    Changeset, Cmd, ConditionalEventHandler, Config, Context, Editor, Event, EventContext,
    EventHandler, Helper, KeyCode, KeyEvent, Modifiers, Prompt, RepeatCount,
};
use tokio::sync::oneshot;

use crate::command;
use crate::menu::{Choices, Menu, RESET_STYLE, Step, Styles};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// What the prompt returned.
#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    /// A submitted line, as typed.
    Line(String),
    /// Ctrl+C or Ctrl+D at the prompt, or the end of a piped stdin.
    Quit,
}

/// The prompt shown before each line: plain, and with ANSI styling. Both
/// must have the same display width.
pub struct PromptText {
    pub plain: String,
    pub styled: String,
}

impl Prompt for PromptText {
    fn raw(&self) -> &str {
        &self.plain
    }

    fn styled(&self) -> &str {
        &self.styled
    }
}

/// The menu of the line being typed. The editor's helper draws it and its
/// keys step through it, all of them on the editor thread.
#[derive(Clone, Default)]
struct SharedMenu(Arc<Mutex<Menu>>);

impl SharedMenu {
    fn lock(&self) -> MutexGuard<'_, Menu> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The editor's helper: shows the command of a line that starts with `/` in
/// the command style as the line is typed, and the line's menu under it.
struct CommandHelper {
    /// `None` leaves the line and the menu without colors.
    styles: Option<Styles>,
    menu: SharedMenu,
    /// The menu the editor is drawing, without colors and with them.
    colored: RefCell<Option<(String, String)>>,
}

impl CommandHelper {
    fn new(styles: Option<Styles>, menu: SharedMenu) -> Self {
        Self {
            styles,
            menu,
            colored: RefCell::default(),
        }
    }
}

impl Highlighter for CommandHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        match (&self.styles, command::split(line)) {
            (Some(styles), Some((blank, command, rest))) => {
                let style = &styles.command;
                Cow::Owned(format!("{blank}{style}{command}{RESET_STYLE}{rest}"))
            }
            _ => Cow::Borrowed(line),
        }
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        match &*self.colored.borrow() {
            Some((plain, colored)) if plain == hint => Cow::Owned(colored.clone()),
            _ => Cow::Borrowed(hint),
        }
    }

    /// The editor draws a typed character on its own, without a style,
    /// unless this asks for the whole line: a line with a command is drawn
    /// again on every change. So is, by the editor, the line that just lost
    /// its command.
    ///
    /// Not under a menu, which already has the editor draw the line on
    /// every change: asked for here, the line would also be drawn when the
    /// cursor moves, and that time without its menu.
    fn highlight_char(&self, line: &str, _pos: usize, _kind: CmdKind) -> bool {
        self.styles.is_some()
            && command::split(line).is_some()
            && self.menu.lock().view(line).is_none()
    }
}

/// The menu as the editor's hint: text it draws from the end of the line.
struct MenuHint(String);

impl Hint for MenuHint {
    fn display(&self) -> &str {
        &self.0
    }

    /// None: the menu is not text that completes the line.
    fn completion(&self) -> Option<&str> {
        None
    }
}

impl Hinter for CommandHelper {
    type Hint = MenuHint;

    /// Called whenever the line changes, before it is drawn.
    fn hint(&self, line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<MenuHint> {
        let mut menu = self.menu.lock();
        menu.drawn(line);
        let view = menu.view(line)?;
        let plain = view.render(None);
        let colored = self.styles.as_ref().map(|styles| view.render(Some(styles)));
        *self.colored.borrow_mut() = colored.map(|colored| (plain.clone(), colored));
        Some(MenuHint(plain))
    }
}

/// The arrows step through the menu with what the editor has for Tab: it
/// cycles through the completions of the line, each in turn on the line, and
/// then back to the line it started from.
impl Completer for CommandHelper {
    type Candidate = String;

    /// The completions of a line are the other lines of its menu's round.
    fn complete(
        &self,
        line: &str,
        _pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        Ok((0, self.menu.lock().begin(line)))
    }

    /// A row takes the place of the whole line, wherever the cursor is.
    fn update(&self, line: &mut LineBuffer, _start: usize, elected: &str, changes: &mut Changeset) {
        line.replace(0..line.len(), elected, changes);
    }
}

impl Validator for CommandHelper {}

impl Helper for CommandHelper {}

/// A key that steps through the menu of a line that has one: Down or a key
/// like it (`down`), or Up or one like it. On any other line it is left to
/// the editor.
struct StepKey {
    menu: SharedMenu,
    down: bool,
    /// Whether the editor walks the history with the key.
    history: bool,
}

impl ConditionalEventHandler for StepKey {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let mut menu = self.menu.lock();
        match menu.step(ctx.line(), self.down) {
            Step::Forward => Some(Cmd::Complete),
            Step::Backward => Some(Cmd::CompleteBackward),
            Step::History => {
                if self.history {
                    menu.left_to_history(ctx.line());
                }
                None
            }
        }
    }
}

/// Enter: on a command that still lacks its argument, opens the menu of
/// what it can be instead of entering the line.
struct EnterKey {
    menu: SharedMenu,
}

impl ConditionalEventHandler for EnterKey {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let at_end = ctx.pos() == ctx.line().len();
        let added = self.menu.lock().enter(ctx.line(), at_end)?;
        Some(Cmd::Insert(1, added))
    }
}

/// Any other key: left to the editor, once the menu knows of it.
struct OtherKey {
    menu: SharedMenu,
    /// Set by the key that has the editor erase the screen.
    screen_erased: Arc<AtomicBool>,
}

impl ConditionalEventHandler for OtherKey {
    fn handle(&self, event: &Event, _: RepeatCount, _: bool, _: &EventContext) -> Option<Cmd> {
        self.menu.lock().other_key();
        if event.get(0) == Some(&KeyEvent::ctrl('L')) {
            self.screen_erased.store(true, Ordering::Relaxed);
        }
        None
    }
}

/// A `read_line` call, for the editor thread.
struct Request {
    /// What the menus of the line list.
    choices: Choices,
    /// Where the thread answers.
    reply: oneshot::Sender<Result<Input, ReadlineError>>,
}

/// Handle to the editor thread. The thread ends with the process.
pub struct LineReader {
    requests: mpsc::Sender<Request>,
    /// Whether Ctrl+L erased the screen, with what the chat had drawn on
    /// it, since this was last asked.
    screen_erased: Arc<AtomicBool>,
}

impl LineReader {
    /// Starts the editor thread. It reads nothing until `read_line` is called.
    ///
    /// `styles` color the command of a line and its menu, on a terminal that
    /// takes colors.
    pub fn spawn(prompt: PromptText, styles: Option<Styles>) -> Result<Self, BoxError> {
        // Without the bell, which the editor rings when the arrows are back
        // on what was typed, past an end of the menu.
        let config = Config::builder().bell_style(BellStyle::None).build();
        let mut editor = Editor::<CommandHelper, DefaultHistory>::with_config(config)?;
        let menu = SharedMenu::default();
        // `rustyline` only prints the styled prompt when a helper is set.
        editor.set_helper(Some(CommandHelper::new(styles, menu.clone())));
        let key = |code| KeyEvent(code, Modifiers::NONE);
        // The arrows and the keys that walk the history as they do, then
        // the editor's keys for the completions.
        for (key, down, history) in [
            (key(KeyCode::Up), false, true),
            (key(KeyCode::Down), true, true),
            (KeyEvent::ctrl('P'), false, true),
            (KeyEvent::ctrl('N'), true, true),
            (key(KeyCode::BackTab), false, false),
            (key(KeyCode::Tab), true, false),
        ] {
            let menu = menu.clone();
            let step = StepKey {
                menu,
                down,
                history,
            };
            editor.bind_sequence(key, EventHandler::Conditional(Box::new(step)));
        }
        // Enter, and the two keys that enter a line as it does.
        for key in [
            key(KeyCode::Enter),
            KeyEvent::ctrl('J'),
            KeyEvent::ctrl('M'),
        ] {
            let enter = EnterKey { menu: menu.clone() };
            editor.bind_sequence(key, EventHandler::Conditional(Box::new(enter)));
        }
        let screen_erased = Arc::new(AtomicBool::new(false));
        let other_key = OtherKey {
            menu: menu.clone(),
            screen_erased: screen_erased.clone(),
        };
        editor.bind_sequence(Event::Any, EventHandler::Conditional(Box::new(other_key)));

        let (requests, queue) = mpsc::channel::<Request>();
        thread::Builder::new()
            .name("line-editor".to_owned())
            .spawn(move || {
                for Request { choices, reply } in queue {
                    menu.lock().start(choices);
                    let input = match editor.readline(&prompt) {
                        Ok(line) => {
                            if !line.trim().is_empty() {
                                // What Up/Down bring back. A full history
                                // only drops its oldest line, so the result
                                // carries nothing to act on.
                                let _ = editor.add_history_entry(line.as_str());
                            }
                            Ok(Input::Line(line))
                        }
                        Err(ReadlineError::Interrupted | ReadlineError::Eof) => Ok(Input::Quit),
                        Err(error) => Err(error),
                    };
                    if reply.send(input).is_err() {
                        break; // The chat stopped waiting: it is leaving.
                    }
                }
            })?;
        Ok(Self {
            requests,
            screen_erased,
        })
    }

    /// Whether a key at the prompt erased the screen since the last call.
    pub fn screen_erased(&self) -> bool {
        self.screen_erased.swap(false, Ordering::Relaxed)
    }

    /// Shows the prompt and waits for a line. A line that starts with `/`
    /// has its menu from `choices`.
    pub async fn read_line(&self, choices: Choices) -> Result<Input, BoxError> {
        const STOPPED: &str = "the line editor thread stopped";
        let (reply, input) = oneshot::channel();
        let request = Request { choices, reply };
        self.requests.send(request).map_err(|_| STOPPED)?;
        Ok(input.await.map_err(|_| STOPPED)??)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use rustyline::Context;
    use rustyline::completion::Completer;
    use rustyline::highlight::{CmdKind, Highlighter};
    use rustyline::hint::{Hint, Hinter};
    use rustyline::history::DefaultHistory;

    use super::{CommandHelper, SharedMenu};
    use crate::command::COMMANDS;
    use crate::menu::{Choices, Styles, Value};

    /// A helper with the commands and two models in its menus, and with
    /// colors or not.
    fn helper(colors: bool) -> CommandHelper {
        let styles = colors.then(|| Styles {
            command: "\x1b[36m".to_owned(),
            chosen: "\x1b[7m".to_owned(),
            about: "\x1b[2m".to_owned(),
        });
        let model = |name: &str| Value {
            name: name.to_owned(),
            about: "1M context".to_owned(),
            current: name == "sonnet",
        };
        let menu = SharedMenu::default();
        menu.lock().start(Choices {
            commands: COMMANDS.iter().collect(),
            arguments: vec![("model", vec![model("opus"), model("sonnet")])],
        });
        CommandHelper::new(styles, menu)
    }

    /// The menu the helper has the editor draw under `line`.
    fn hint(helper: &CommandHelper, line: &str) -> Option<String> {
        let history = DefaultHistory::new();
        let hint = helper.hint(line, line.len(), &Context::new(&history))?;
        assert_eq!(hint.completion(), None);
        Some(hint.display().to_owned())
    }

    #[test]
    fn the_command_of_a_line_is_styled() {
        let helper = helper(true);
        assert_eq!(helper.highlight("/", 1), "\x1b[36m/\x1b[0m");
        assert_eq!(helper.highlight("/clear", 6), "\x1b[36m/clear\x1b[0m");
        // Known or not, and only the command.
        assert_eq!(
            helper.highlight("  /clera now ", 0),
            "  \x1b[36m/clera\x1b[0m now "
        );
    }

    #[test]
    fn other_lines_are_left_as_typed() {
        let helper = helper(true);
        for line in ["", "hola", "what does /clear do?"] {
            assert!(matches!(helper.highlight(line, 0), Cow::Borrowed(same) if same == line));
            assert!(!helper.highlight_char(line, 0, CmdKind::Other));
            assert_eq!(hint(&helper, line), None);
        }
    }

    #[test]
    fn a_line_with_a_command_and_no_menu_is_drawn_again_on_every_change() {
        let helper = helper(true);
        for kind in [CmdKind::Other, CmdKind::MoveCursor, CmdKind::ForcedRefresh] {
            assert!(helper.highlight_char("/clera", 6, kind));
            assert!(helper.highlight_char(" /usage x", 3, kind));
            // The menu has the editor draw these.
            assert!(!helper.highlight_char("/", 1, kind));
            assert!(!helper.highlight_char("/cl", 3, kind));
        }
    }

    #[test]
    fn without_styles_nothing_is_styled() {
        let helper = helper(false);
        assert!(matches!(
            helper.highlight("/clear", 6),
            Cow::Borrowed("/clear")
        ));
        assert!(!helper.highlight_char("/clera", 6, CmdKind::Other));
        let menu = hint(&helper, "/cl").unwrap();
        assert_eq!(menu, "\n  /clear  start a new chat");
        assert!(matches!(helper.highlight_hint(&menu), Cow::Borrowed(same) if same == menu));
    }

    #[test]
    fn the_menu_is_the_hint_of_a_line_and_is_colored() {
        let helper = helper(true);
        let menu = hint(&helper, "/e").unwrap();
        assert_eq!(menu, "\n  /exit  leave the chat");
        assert_eq!(
            helper.highlight_hint(&menu),
            "\n  \x1b[36m/exit\x1b[0m  \x1b[2mleave the chat\x1b[0m"
        );
        // Only the menu being drawn.
        assert!(matches!(
            helper.highlight_hint("\n  /exit"),
            Cow::Borrowed(_)
        ));
        assert_eq!(hint(&helper, "/exit now"), None);
    }

    #[test]
    fn the_completions_of_a_line_are_the_commands_of_its_menu() {
        let helper = helper(true);
        let history = DefaultHistory::new();
        let complete = |line: &str| {
            let context = Context::new(&history);
            helper.complete(line, line.len(), &context).unwrap()
        };
        let all: Vec<String> = COMMANDS
            .iter()
            .map(|entry| format!("/{}", entry.name))
            .collect();
        assert_eq!(complete("/"), (0, all));
        assert_eq!(complete("/us"), (0, vec!["/usage".to_owned()]));
        assert_eq!(complete("hola"), (0, Vec::new()));
        // The menu marks the one the editor puts on the line.
        complete("/");
        let menu = hint(&helper, "/usage").unwrap();
        assert!(menu.contains("\n> /usage  "), "{menu:?}");
        assert!(helper.highlight_hint(&menu).contains("\x1b[7m> /usage"));
    }

    #[test]
    fn a_command_with_an_argument_has_a_menu_of_what_it_can_be() {
        let helper = helper(true);
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        assert_eq!(
            hint(&helper, "/model ").unwrap(),
            "\n  opus    1M context\n  sonnet  1M context"
        );
        // Under its menu, the line is not drawn for its own sake.
        assert!(!helper.highlight_char("/model so", 9, CmdKind::Other));
        assert!(helper.highlight_char("/model gpt", 10, CmdKind::Other));
        let (start, lines) = helper.complete("/model so", 9, &context).unwrap();
        assert_eq!((start, lines), (0, vec!["/model sonnet".to_owned()]));
        assert_eq!(
            hint(&helper, "/model sonnet").unwrap(),
            "\n> sonnet  1M context"
        );
        // Only the command is in the command style.
        assert_eq!(
            helper.highlight("/model sonnet", 13),
            "\x1b[36m/model\x1b[0m sonnet"
        );
    }
}

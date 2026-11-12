//! An interactive chat with a Bedrock model.
//!
//! Each line typed at the prompt is sent to one agent thread, and the reply
//! is printed as it streams, its Markdown rendered for the terminal (see
//! [`markdown`]). `/clear` starts a new conversation on an empty screen,
//! `/usage` shows the tokens the current one has used, and `/exit`, Ctrl+C
//! or Ctrl+D leave the chat; the
//! prompt's editing keys are described in [`input`]. The model can list the
//! directory the chat was started in (see [`tools`]), and every turn is
//! traced in Langfuse when its credentials are set (see [`langfuse`]).

mod input;
mod langfuse;
mod markdown;
mod tools;
mod ui;
mod usage;

use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::sync::Arc;

use infinity_agent_core::stores::{InMemoryConversationStore, InMemoryStateStore};
use infinity_agent_core::system::local::{LaunchingSystem, ThreadHandle, ThreadLifecycleState};
use infinity_agent_core::system::{AgentEvent, AgentSystemBuilder, NoRapHttp, StaticModel};
use infinity_provider_bedrock::BedrockProvider;
use tokio::task::LocalSet;

use crate::input::{Input, LineReader};
use crate::langfuse::{Langfuse, TurnTrace};
use crate::tools::ListDirectoryTool;
use crate::ui::Ui;
use crate::usage::UsageMeter;

const MODEL_ID: &str = "global.anthropic.claude-opus-5-5";
const HINT: &str =
    "/clear new chat · /usage tokens used · /exit or Ctrl+C quit · ↑/↓ earlier messages";
const CONNECTING: &str = "Connecting…";
const THINKING: &str = "Thinking…";
const SYSTEM_STOPPED: &str = "the agent system stopped";

/// The built-in tool a model calls when it has nothing left to do but wait:
/// its result is the next input.
const WAIT_FOR_INPUT_TOOL: &str = "sleep_until_event_or_input";

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type System = LaunchingSystem<InMemoryConversationStore, InMemoryStateStore, NoRapHttp>;

fn main() -> Result<ExitCode, BoxError> {
    // Before the runtime starts its threads: the environment can only be
    // changed safely while nothing else can be reading it.
    load_dotenv();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    // The agent system runs on local tasks.
    runtime.block_on(LocalSet::new().run_until(chat()))
}

/// Loads the `.env` of the current directory, or of the nearest one above
/// it, into the environment. Variables that are already set keep their value.
fn load_dotenv() {
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        eprintln!("warning: .env was not loaded: {error}");
    }
}

async fn chat() -> Result<ExitCode, BoxError> {
    // Without a terminal on stdin there is no line editing: each line of the
    // pipe or file is one message.
    let interactive = io::stdin().is_terminal();
    let hint = interactive.then_some(HINT);
    let mut ui = Ui::new()?;
    ui.header(MODEL_ID, hint)?;

    ui.start_spinner(CONNECTING);
    let meter = UsageMeter::default();
    let connect = async {
        let langfuse = Langfuse::connect().await;
        let mut provider = meter.wrap(Arc::new(BedrockProvider::from_env()));
        if let Ok(langfuse) = &langfuse {
            provider = langfuse.trace_provider(provider);
        }
        let model = StaticModel::new(provider, MODEL_ID).await?;
        Ok::<_, BoxError>((langfuse, model))
    };
    let (langfuse, model) = tokio::select! {
        connected = connect => connected?,
        () = ctrl_c() => return Ok(ExitCode::SUCCESS),
    };
    // The chat works without Langfuse: it only says why it is not traced.
    let langfuse = match langfuse {
        Ok(langfuse) => {
            ui.note(&format!("Langfuse session: {}", langfuse.session_url()))?;
            Some(langfuse)
        }
        Err(reason) => {
            ui.diagnostic(&format!("Langfuse tracing is off: {reason}"))?;
            None
        }
    };
    let mut system = AgentSystemBuilder::new_local(
        InMemoryConversationStore::new(),
        InMemoryStateStore::new(),
        model,
    )
    .with_tokio_sleep_tools()
    .tool(Box::new(ListDirectoryTool::in_current_dir()?))
    .start();
    let thread = system.thread_builder().launch().await;
    ui.end_output()?;

    let mut chat = Chat {
        ui: &mut ui,
        system: &mut system,
        thread,
        langfuse: langfuse.as_ref(),
        meter: &meter,
        hint,
    };
    let last_turn_ok = chat.converse().await;

    // Also after an error: the thread flushes what it has before it stops,
    // and the traces still queued are sent. A second Ctrl+C does not wait
    // for that.
    let wind_down = async {
        system.shutdown().await;
        if let Some(langfuse) = &langfuse {
            langfuse.shutdown().await;
        }
    };
    tokio::select! {
        () = wind_down => {}
        () = ctrl_c() => {}
    }
    // Like a shell, the chat exits with the status of its last turn.
    Ok(if last_turn_ok? {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// What a line typed at the prompt asks for.
#[derive(Debug, PartialEq, Eq)]
enum Command<'a> {
    /// Leave the chat.
    Exit,
    /// Start a new conversation on an empty screen.
    Clear,
    /// Show what the conversation has used.
    Usage,
    /// Anything else is a message for the model.
    Message(&'a str),
}

impl<'a> Command<'a> {
    /// `None` for a blank line.
    fn parse(line: &'a str) -> Option<Self> {
        match line.trim() {
            "" => None,
            "/exit" => Some(Self::Exit),
            "/clear" => Some(Self::Clear),
            "/usage" => Some(Self::Usage),
            message => Some(Self::Message(message)),
        }
    }
}

/// A chat: the conversation with one thread, and what it takes to run it.
struct Chat<'a> {
    ui: &'a mut Ui,
    system: &'a mut System,
    /// The thread of the current conversation.
    thread: ThreadHandle,
    langfuse: Option<&'a Langfuse>,
    meter: &'a UsageMeter,
    /// The header's line about the commands, when there is someone to read
    /// it.
    hint: Option<&'static str>,
}

impl Chat<'_> {
    /// Reads lines and does what they ask until the user leaves. Returns
    /// whether the last turn went well.
    async fn converse(&mut self) -> Result<bool, BoxError> {
        let reader = LineReader::spawn(self.ui.prompt())?;
        let mut last_turn_ok = true;
        // The current conversation: its turns, and what the meter read when
        // it started.
        let mut turns = 0;
        let mut usage_before = self.meter.totals();
        loop {
            let input = tokio::select! {
                input = reader.read_line() => input?,
                // At a terminal the line editor reads Ctrl+C as a key and
                // returns `Quit`. The signal only comes when it reads stdin
                // as a file.
                () = ctrl_c() => Input::Quit,
            };
            let Input::Line(line) = input else { break };
            let Some(command) = Command::parse(&line) else {
                continue;
            };
            if command == Command::Exit {
                break;
            }
            if self.hint.is_none() {
                // Nobody typed the line on this terminal: show it.
                self.ui.echo_input(line.trim())?;
            }
            match command {
                Command::Exit => unreachable!("left the loop above"),
                Command::Clear => {
                    self.clear().await?;
                    turns = 0;
                    usage_before = self.meter.totals();
                    last_turn_ok = true;
                }
                Command::Usage => {
                    let used = self.meter.totals().since(&usage_before);
                    self.ui.usage_report(turns, &used)?;
                }
                Command::Message(message) => {
                    turns += 1;
                    match self.run_turn(message).await? {
                        TurnEnd::Completed { ok } => last_turn_ok = ok,
                        TurnEnd::Interrupted => break,
                    }
                }
            }
        }
        Ok(last_turn_ok)
    }

    /// Starts a new conversation on an empty screen: a thread that has seen
    /// nothing, and a session of its own in Langfuse. The lines typed so far
    /// stay within reach of Up and Down.
    async fn clear(&mut self) -> Result<(), BoxError> {
        self.thread = self.system.thread_builder().launch().await;
        self.ui.clear_screen()?;
        self.ui.header(MODEL_ID, self.hint)?;
        if let Some(langfuse) = self.langfuse {
            langfuse.new_session();
            let session = format!("Langfuse session: {}", langfuse.session_url());
            self.ui.note(&session)?;
        }
        Ok(())
    }
}

/// How a turn ended.
enum TurnEnd {
    /// The thread has nothing left to do before the next message. `ok` is
    /// false when the turn reported an error.
    Completed { ok: bool },
    /// Ctrl+C while the thread was working.
    Interrupted,
}

impl Chat<'_> {
    /// Sends `message` to the thread and prints what it does with it, until
    /// the thread is ready for the next message.
    async fn run_turn(&mut self, message: &str) -> Result<TurnEnd, BoxError> {
        // The trace starts before the message is sent, so that the model
        // calls the message causes find it. It ends when `turn` is dropped.
        let mut turn = Turn::default();
        if let Some(langfuse) = self.langfuse {
            turn.trace = Some(langfuse.start_turn(message).await);
        }
        let usage_before = self.meter.totals();
        self.thread.send_user_text(message).await?;
        self.ui.start_spinner(THINKING);

        let thread_id = self.thread.thread_id().to_owned();
        let interrupt = ctrl_c();
        tokio::pin!(interrupt);
        loop {
            tokio::select! {
                // In this order: the driver sends its events before it
                // reports itself idle, so no event is left when the idle
                // report is read.
                biased;
                () = &mut interrupt => {
                    self.ui.end_output()?;
                    if let Some(trace) = &mut turn.trace {
                        trace.interrupted();
                    }
                    return Ok(TurnEnd::Interrupted);
                }
                event = self.thread.recv() => {
                    if turn.on_event(self.ui, event.ok_or(SYSTEM_STOPPED)?)? {
                        break;
                    }
                }
                lifecycle = self.system.next_lifecycle_event() => {
                    let lifecycle = lifecycle.ok_or(SYSTEM_STOPPED)?;
                    // A turn spans several rounds when the model calls
                    // tools. It is over when the thread's driver has no work
                    // left and exits. An idle report from before this
                    // message, left by work that finished while the prompt
                    // was waiting, is not it.
                    if lifecycle.thread_id == thread_id
                        && lifecycle.state == ThreadLifecycleState::Idle
                        && turn.started()
                    {
                        break;
                    }
                }
            }
        }
        // Every model call of the turn has ended, and is in the meter.
        let used = self.meter.totals().since(&usage_before);
        self.ui.footer((used.calls > 0).then_some(&used.usage))?;
        Ok(TurnEnd::Completed { ok: !turn.failed })
    }
}

/// What a turn has gone through so far.
#[derive(Default)]
struct Turn {
    /// The thread took the message into its history.
    accepted: bool,
    /// A round reported an error.
    failed: bool,
    /// The last tool called that has no result yet.
    pending_tool: Option<String>,
    /// The turn's trace in Langfuse, when the chat is traced.
    trace: Option<TurnTrace>,
}

impl Turn {
    /// Whether the thread has reacted to the turn's message.
    fn started(&self) -> bool {
        self.accepted || self.failed
    }

    /// Prints `event`. Returns `true` when the thread now waits for the next
    /// message.
    fn on_event(&mut self, ui: &mut Ui, event: AgentEvent) -> Result<bool, BoxError> {
        match event {
            AgentEvent::UserInput { .. } => self.accepted = true,
            AgentEvent::TextChunk { text } => {
                if let Some(trace) = &mut self.trace {
                    trace.reply_chunk(&text);
                }
                ui.reply_chunk(&text)?;
            }
            AgentEvent::ToolCall {
                name, display_as, ..
            } => {
                if let Some(trace) = &mut self.trace {
                    trace.reply_break();
                }
                ui.tool_call(display_as.as_deref().unwrap_or(&name))?;
                ui.start_spinner(THINKING);
                self.pending_tool = Some(name);
            }
            AgentEvent::ToolResult { .. } => self.pending_tool = None,
            // Retries ("Stream error …, retrying…"), warnings and errors.
            // agent-core reports a failed round only as an "Error: …"
            // message, followed by a `CompletionFinished` without usage, so
            // the prefix is the only failure signal.
            AgentEvent::Info { text } => {
                if let Some(error) = text.strip_prefix("Error: ") {
                    self.failed = true;
                    if let Some(trace) = &mut self.trace {
                        trace.error(error);
                    }
                }
                ui.diagnostic(&text)?;
                ui.start_spinner(THINKING);
            }
            // A round that ends on this tool leaves the driver parked, not
            // idle: the thread resumes with the next message.
            AgentEvent::CompletionFinished { .. } => {
                return Ok(self.pending_tool.as_deref() == Some(WAIT_FOR_INPUT_TOOL));
            }
            _ => {}
        }
        Ok(false)
    }
}

/// Resolves on Ctrl+C (SIGINT). The first call replaces the signal's default
/// action for the rest of the process, so from then on something must be
/// waiting on this wherever Ctrl+C should end the chat.
async fn ctrl_c() {
    if tokio::signal::ctrl_c().await.is_err() {
        // No handler was installed: SIGINT still ends the process.
        std::future::pending::<()>().await;
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn commands_are_whole_lines() {
        assert_eq!(Command::parse("  /exit \n"), Some(Command::Exit));
        assert_eq!(Command::parse("/clear"), Some(Command::Clear));
        assert_eq!(Command::parse("/usage"), Some(Command::Usage));
        assert_eq!(Command::parse("  \t"), None);
        // Text that only starts like a command is a message.
        assert_eq!(
            Command::parse("/usage of what?"),
            Some(Command::Message("/usage of what?"))
        );
        assert_eq!(Command::parse(" hola "), Some(Command::Message("hola")));
    }
}

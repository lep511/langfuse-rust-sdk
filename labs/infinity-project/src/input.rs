//! Line input for the chat: a `rustyline` editor on a thread of its own.
//!
//! `rustyline` gives the prompt its editing keys: Left/Right (and Home/End)
//! move the cursor, Up/Down walk through the lines already entered, and
//! Ctrl+C or Ctrl+D leave. When stdin is not a terminal it reads plain lines
//! instead, so the chat can also be fed from a pipe.
//!
//! `readline` blocks, and the agent system needs the main thread to keep
//! running its tasks, hence the thread. It is a plain thread rather than
//! `spawn_blocking` because the runtime waits for its blocking tasks when it
//! shuts down, and a `readline` waiting for input would never let it.

use std::sync::mpsc;
use std::thread;

use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Editor, Prompt};
use tokio::sync::oneshot;

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

/// Where the editor thread answers one `read_line` call.
type Reply = oneshot::Sender<Result<Input, ReadlineError>>;

/// Handle to the editor thread. The thread ends with the process.
pub struct LineReader {
    requests: mpsc::Sender<Reply>,
}

impl LineReader {
    /// Starts the editor thread. It reads nothing until `read_line` is called.
    pub fn spawn(prompt: PromptText) -> Result<Self, BoxError> {
        let mut editor = Editor::<(), DefaultHistory>::new()?;
        // `rustyline` only prints the styled prompt when a helper is set; the
        // unit helper adds nothing else.
        editor.set_helper(Some(()));

        let (requests, queue) = mpsc::channel::<Reply>();
        thread::Builder::new()
            .name("line-editor".to_owned())
            .spawn(move || {
                for reply in queue {
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
        Ok(Self { requests })
    }

    /// Shows the prompt and waits for a line.
    pub async fn read_line(&self) -> Result<Input, BoxError> {
        const STOPPED: &str = "the line editor thread stopped";
        let (reply, input) = oneshot::channel();
        self.requests.send(reply).map_err(|_| STOPPED)?;
        Ok(input.await.map_err(|_| STOPPED)??)
    }
}

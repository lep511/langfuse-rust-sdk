//! Terminal output of the chat: the header, the replies as they stream, tool
//! calls, diagnostics and the usage footer that closes each turn.
//!
//! Replies are Markdown (see [`crate::markdown`]). A reply is printed block
//! by block, as each one is complete; on a terminal, the text still being
//! written shows under them on one dim line that is redrawn as it grows.

use std::io::{self, IsTerminal};

use infinity_provider_protocol::Usage;
use richrs::console::ColorSystem;
use richrs::prelude::{Console, Justify, Rule, Segment, Status, Style, Text};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::input::PromptText;
use crate::markdown::{Line, MarkdownStream};
use crate::usage::{Tokens, Totals};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

const PROMPT_MARK: &str = "❯";
const REPLY_MARK: &str = "●";
const TOOL_MARK: &str = "◇";
/// The reply is indented by the width of its mark and the space after it.
const REPLY_MARGIN: &str = "  ";
/// Columns the preview leaves free at the end of its line. The line must
/// not wrap, or it could no longer be erased: one column keeps the cursor on
/// it, and two more hold the `^C` the terminal echoes on Ctrl+C.
const PREVIEW_SLACK: usize = 3;
/// Moves the cursor to the start of its line and erases the line.
const ERASE_LINE: &str = "\r\x1b[K";

pub struct Ui {
    /// The conversation goes to stdout.
    console: Console,
    /// Diagnostics (retries, warnings, errors) go to stderr.
    err_console: Console,
    colors: bool,
    err_colors: bool,
    /// Whether stdout is a terminal where a line can be redrawn.
    live: bool,
    accent: Style,
    dim: Style,
    warning: Style,
    reply_mark: Style,
    spinner_style: Style,
    /// The spinner on stderr. Dropping it stops the animation and clears its
    /// line. It never runs while the reply has a preview on screen.
    spinner: Option<Status>,
    /// The reply being streamed.
    reply: Option<Reply>,
    /// Whether the last line on screen is the preview of the reply.
    preview: bool,
}

/// A reply as it streams.
#[derive(Default)]
struct Reply {
    markdown: MarkdownStream,
    /// Whether the reply's mark is on screen, before its first line.
    marked: bool,
}

impl Ui {
    pub fn new() -> Result<Self, BoxError> {
        let colors = colors_enabled(io::stdout());
        let err_colors = colors_enabled(io::stderr());
        let mut console = Console::new();
        let mut err_console = Console::stderr();
        // richrs always emits ANSI codes unless told otherwise.
        if !colors {
            console.set_color_system(ColorSystem::None);
        }
        if !err_colors {
            err_console.set_color_system(ColorSystem::None);
        }
        Ok(Self {
            console,
            err_console,
            colors,
            err_colors,
            live: io::stdout().is_terminal()
                && std::env::var_os("TERM").is_some_and(|t| t != "dumb"),
            accent: Style::parse("bold cyan")?,
            dim: Style::parse("dim")?,
            warning: Style::parse("yellow")?,
            reply_mark: Style::parse("bold magenta")?,
            spinner_style: Style::parse("magenta")?,
            spinner: None,
            reply: None,
            preview: false,
        })
    }

    /// A rule with `title`, then the `hint` line.
    pub fn header(&mut self, title: &str, hint: Option<&str>) -> Result<(), BoxError> {
        let rule = Rule::with_title(title)
            .style(self.dim.clone())
            .title_style(self.accent.clone());
        self.console
            .write_segments(&rule.render(self.console.width()))?;
        if let Some(hint) = hint {
            self.console
                .print_text(&Text::styled(hint, self.dim.clone()))?;
        }
        Ok(self.console.flush()?)
    }

    /// Erases the screen and what has scrolled off it, on a terminal.
    pub fn clear_screen(&mut self) -> Result<(), BoxError> {
        self.end_output()?;
        if self.live {
            // Cursor home, erase the display, erase the scrollback.
            self.console.write("\x1b[H\x1b[2J\x1b[3J")?;
            self.console.flush()?;
        }
        Ok(())
    }

    /// What the chat has used so far: the answer to `/usage`.
    pub fn usage_report(&mut self, turns: u32, totals: &Totals) -> Result<(), BoxError> {
        self.end_output()?;
        let count = |n: u64, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
        let mut report = Text::styled("This chat: ", self.accent.clone());
        report.append_plain(&format!(
            "{} · {}",
            count(u64::from(turns), "turn"),
            count(totals.calls, "model call")
        ));
        self.console.print_text(&report)?;
        if totals.calls > 0 {
            let tokens = format!("{REPLY_MARGIN}{}", format_usage(&totals.usage));
            self.console.print_text(&Text::from_str(tokens))?;
        }
        Ok(())
    }

    /// The prompt for the line editor.
    pub fn prompt(&self) -> PromptText {
        let plain = format!("{PROMPT_MARK} ");
        let styled = if self.colors {
            format!("{}{PROMPT_MARK}\x1b[0m ", self.accent.to_ansi())
        } else {
            plain.clone()
        };
        PromptText { plain, styled }
    }

    /// Prints a submitted line after the prompt mark, for input that the
    /// terminal did not echo (stdin is a pipe or a file).
    pub fn echo_input(&mut self, text: &str) -> Result<(), BoxError> {
        let mut line = Text::styled(format!("{PROMPT_MARK} "), self.accent.clone());
        line.append_plain(&sanitize(text));
        Ok(self.console.print_text(&line)?)
    }

    /// Shows a spinner with `label`, when stderr is a terminal, until the
    /// next output.
    pub fn start_spinner(&mut self, label: &str) {
        if self.spinner.is_some() || !io::stderr().is_terminal() {
            return;
        }
        let mut status = Status::new(label);
        if self.err_colors {
            status = status.spinner_style(self.spinner_style.clone());
        }
        status.start();
        self.spinner = Some(status);
    }

    /// Takes a chunk of the reply: prints the blocks it completes, and shows
    /// the rest as a preview.
    pub fn reply_chunk(&mut self, chunk: &str) -> Result<(), BoxError> {
        let width = self.reply_width();
        let reply = self.reply.get_or_insert_default();
        let lines = reply.markdown.push(&sanitize(chunk), width);
        self.print_reply_lines(lines)?;
        self.show_preview()
    }

    /// The columns a reply has: the terminal's, less the reply's margin.
    fn reply_width(&self) -> usize {
        self.console
            .width()
            .saturating_sub(REPLY_MARGIN.len())
            .max(20)
    }

    /// Prints lines of the reply, the first one of all after the reply's
    /// mark and the others under it.
    fn print_reply_lines(&mut self, lines: Vec<Line>) -> Result<(), BoxError> {
        if lines.is_empty() {
            return Ok(());
        }
        self.spinner = None;
        self.clear_preview()?;
        for line in lines {
            if self
                .reply
                .as_mut()
                .is_some_and(|reply| !std::mem::replace(&mut reply.marked, true))
            {
                let mark = Segment::styled(format!("{REPLY_MARK} "), self.reply_mark.clone());
                self.console.write_segment(&mark)?;
            } else if !line.is_empty() {
                self.console.write(REPLY_MARGIN)?;
            }
            for span in line {
                self.console
                    .write_segment(&Segment::styled(span.text, span.style))?;
            }
            self.console.write("\n")?;
        }
        Ok(self.console.flush()?)
    }

    /// Shows the end of the text that is not printed yet on the line under
    /// the reply, so that the reply is seen being written. It is one line
    /// that never wraps, which is what lets it be erased.
    fn show_preview(&mut self) -> Result<(), BoxError> {
        let Some(reply) = self.reply.as_ref().filter(|_| self.live) else {
            return Ok(());
        };
        let room = self
            .console
            .width()
            .saturating_sub(REPLY_MARGIN.len() + PREVIEW_SLACK);
        let text = preview_text(reply.markdown.pending(), room);
        if text.is_empty() {
            return Ok(());
        }
        let lead = if reply.marked {
            Segment::new(REPLY_MARGIN)
        } else {
            Segment::styled(format!("{REPLY_MARK} "), self.reply_mark.clone())
        };
        self.spinner = None;
        self.console.write(ERASE_LINE)?;
        self.console.write_segment(&lead)?;
        self.console
            .write_segment(&Segment::styled(text, self.dim.clone()))?;
        self.preview = true;
        Ok(self.console.flush()?)
    }

    fn clear_preview(&mut self) -> Result<(), BoxError> {
        if std::mem::take(&mut self.preview) {
            self.console.write(ERASE_LINE)?;
            self.console.flush()?;
        }
        Ok(())
    }

    /// A line about the chat itself.
    pub fn note(&mut self, text: &str) -> Result<(), BoxError> {
        self.end_output()?;
        let line = Text::styled(text, self.dim.clone());
        Ok(self.console.print_text(&line)?)
    }

    /// A line for a tool the model called.
    pub fn tool_call(&mut self, label: &str) -> Result<(), BoxError> {
        self.end_output()?;
        let label = sanitize(label).replace(['\n', '\t'], " ");
        let line = Text::styled(format!("{TOOL_MARK} {label}"), self.dim.clone());
        Ok(self.console.print_text(&line)?)
    }

    /// A retry notice, warning or error, on stderr.
    pub fn diagnostic(&mut self, text: &str) -> Result<(), BoxError> {
        self.end_output()?;
        let line = Text::styled(sanitize(text), self.warning.clone());
        Ok(self.err_console.print_text(&line)?)
    }

    /// Stops the spinner and ends the reply being streamed, printing what
    /// was left of it, so that what comes next starts on a line of its own.
    pub fn end_output(&mut self) -> Result<(), BoxError> {
        self.spinner = None;
        self.clear_preview()?;
        let width = self.reply_width();
        if let Some(reply) = &mut self.reply {
            let lines = reply.markdown.finish(width);
            self.print_reply_lines(lines)?;
        }
        self.reply = None;
        Ok(())
    }

    /// Closes a turn with a rule that carries its token usage, when the
    /// provider reported it.
    pub fn footer(&mut self, usage: Option<&Usage>) -> Result<(), BoxError> {
        self.end_output()?;
        let rule = match usage {
            Some(usage) => Rule::with_title(format_usage(usage))
                .title_style(self.dim.clone())
                .align(Justify::Right),
            None => Rule::new(),
        };
        let rule = rule.style(self.dim.clone());
        self.console
            .write_segments(&rule.render(self.console.width()))?;
        Ok(self.console.flush()?)
    }
}

/// Colors only on a terminal, and never when `NO_COLOR` is set
/// (<https://no-color.org>).
fn colors_enabled(stream: impl IsTerminal) -> bool {
    stream.is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

/// Drops the control characters of `text`, except line breaks and tabs. Model
/// output and provider messages are printed as they come, and must not be
/// able to send escape sequences to the terminal.
fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\t')
        .collect()
}

/// The end of `pending` as one line of at most `room` columns: the words
/// being written now, after an ellipsis when there were more before them.
fn preview_text(pending: &str, room: usize) -> String {
    const ELLIPSIS: &str = "… ";
    let room = room.saturating_sub(ELLIPSIS.width());
    // From the end, with every run of whitespace as one space.
    let mut tail = Vec::new();
    let mut width = 0;
    let mut cut = false;
    for c in pending.trim_end().chars().rev() {
        let c = if c.is_whitespace() { ' ' } else { c };
        if c == ' ' && tail.last() == Some(&' ') {
            continue;
        }
        let c_width = c.width().unwrap_or(0);
        if width + c_width > room {
            cut = true;
            break;
        }
        tail.push(c);
        width += c_width;
    }
    let text: String = tail.into_iter().rev().collect();
    let text = text.trim_start();
    if cut {
        format!("{ELLIPSIS}{text}")
    } else {
        text.to_owned()
    }
}

/// Formats token usage so that `in + out = total` (see [`Tokens`]).
fn format_usage(usage: &Usage) -> String {
    let tokens = Tokens::of(usage);
    let breakdown: Vec<String> = [
        (tokens.cache_read, "cache read"),
        (tokens.cache_write, "cache write"),
    ]
    .into_iter()
    .filter(|&(count, _)| count > 0)
    .map(|(count, label)| format!("{count} {label}"))
    .collect();
    let input = tokens.input();
    let input_label = if breakdown.is_empty() {
        input.to_string()
    } else {
        format!("{input} ({})", breakdown.join(", "))
    };
    format!(
        "{input_label} in · {} out · {} total tokens",
        tokens.output,
        tokens.total()
    )
}

#[cfg(test)]
mod tests {
    use super::{Usage, format_usage, preview_text, sanitize};

    fn usage(input: u64, output: u64, total: u64, cache_read: u64) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            cached_input_tokens: cache_read,
        }
    }

    #[test]
    fn usage_shows_cache_writes_hidden_in_the_total() {
        // The first run: Bedrock reported 4 in, 33 out, 2928 total.
        assert_eq!(
            format_usage(&usage(4, 33, 2928, 0)),
            "2895 (2891 cache write) in · 33 out · 2928 total tokens"
        );
    }

    #[test]
    fn usage_shows_cache_reads_and_writes() {
        assert_eq!(
            format_usage(&usage(4, 33, 2928, 2891)),
            "2895 (2891 cache read) in · 33 out · 2928 total tokens"
        );
        assert_eq!(
            format_usage(&usage(10, 20, 330, 200)),
            "310 (200 cache read, 100 cache write) in · 20 out · 330 total tokens"
        );
    }

    #[test]
    fn usage_without_caching_or_total() {
        assert_eq!(
            format_usage(&usage(100, 50, 150, 0)),
            "100 in · 50 out · 150 total tokens"
        );
        assert_eq!(
            format_usage(&usage(100, 50, 0, 0)),
            "100 in · 50 out · 150 total tokens"
        );
    }

    #[test]
    fn preview_is_the_text_on_one_line() {
        assert_eq!(
            preview_text("  Many nodes.\n\n| a |\t| b |\n", 40),
            "Many nodes. | a | | b |"
        );
        assert_eq!(preview_text(" \n", 40), "");
    }

    #[test]
    fn preview_keeps_the_end_of_a_long_text() {
        // "… " and the 10 columns that are left.
        assert_eq!(preview_text("one two three four five", 12), "… four five");
        assert_eq!(preview_text("日本語のテキスト", 9), "… キスト");
        assert_eq!(preview_text("abc", 2), "… ");
    }

    #[test]
    fn sanitize_drops_escape_sequences_but_not_layout() {
        assert_eq!(
            sanitize("a\x1b[31mred\x1b[0m\r\n\tb\u{9b}2J\x07"),
            "a[31mred[0m\n\tb2J"
        );
    }
}

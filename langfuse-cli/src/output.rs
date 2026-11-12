//! Writing API results: the official CLI's formats (pretty JSON, the
//! `--json` envelope, `--output`), plus `--format table|ndjson` and coloured
//! JSON on terminals.

use std::ffi::OsString;
use std::io::{BufWriter, ErrorKind, IsTerminal as _, Read, Write};
use std::path::PathBuf;

use clap::{ColorChoice, ValueEnum};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{CliError, Exit, Result};
use crate::execute::ApiResult;
use crate::table;

/// The process's standard streams, injectable for in-process tests.
pub struct Io<'a> {
    /// Standard output.
    pub stdout: &'a mut dyn Write,
    /// Standard error.
    pub stderr: &'a mut dyn Write,
    /// Standard input (`--body-file -`).
    pub stdin: &'a mut dyn Read,
    /// Whether stdout is a terminal (colours, table width).
    pub stdout_tty: bool,
    /// Whether stderr is a terminal (colours of clap errors).
    pub stderr_tty: bool,
    /// Terminal width for tables.
    pub width: Option<u16>,
    /// Whether the environment allows colours on terminals (`NO_COLOR` is
    /// unset or empty, and `TERM` is not `dumb`).
    pub color_allowed: bool,
    /// Working directory: the project root of `skills` commands and the base
    /// of their relative paths.
    pub cwd: PathBuf,
}

impl std::fmt::Debug for Io<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Io")
            .field("stdout_tty", &self.stdout_tty)
            .field("stderr_tty", &self.stderr_tty)
            .field("width", &self.width)
            .field("color_allowed", &self.color_allowed)
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}

/// Whether an environment allows colours on terminals. `var` looks a
/// variable up, so this is tested without the process environment.
fn color_allowed(var: impl Fn(&str) -> Option<OsString>) -> bool {
    var("NO_COLOR").is_none_or(|value| value.is_empty())
        && var("TERM").is_none_or(|term| term != "dumb")
}

/// `--color always|never`, else whether the stream can show colours.
fn use_color(choice: ColorChoice, capable: bool) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => capable,
    }
}

impl<'a> Io<'a> {
    /// The streams of this process, with its terminal, environment and
    /// working directory.
    pub fn stdio(
        stdout: &'a mut dyn Write,
        stderr: &'a mut dyn Write,
        stdin: &'a mut dyn Read,
    ) -> Self {
        Self {
            stdout,
            stderr,
            stdin,
            stdout_tty: std::io::stdout().is_terminal(),
            stderr_tty: std::io::stderr().is_terminal(),
            width: terminal_size::terminal_size().map(|(width, _)| width.0),
            color_allowed: color_allowed(|name| std::env::var_os(name)),
            cwd: std::env::current_dir().unwrap_or_else(|_| ".".into()),
        }
    }
}

impl Io<'_> {
    /// Whether to colour stdout.
    pub(crate) fn color_stdout(&self, choice: ColorChoice) -> bool {
        use_color(choice, self.stdout_tty && self.color_allowed)
    }

    /// Whether to colour stderr.
    pub(crate) fn color_stderr(&self, choice: ColorChoice) -> bool {
        use_color(choice, self.stderr_tty && self.color_allowed)
    }

    /// Write `bytes` to stdout.
    ///
    /// Every write to stdout goes through this method, [`out_with`](Self::out_with)
    /// or [`status`](Self::status), so a closed pipe and a failed write are
    /// handled in one place ([`CliError::stdout`]).
    pub(crate) fn out(&mut self, bytes: &[u8]) -> Result<()> {
        self.stdout.write_all(bytes).map_err(CliError::stdout)
    }

    /// Let `write` produce output of any size straight into stdout, through a
    /// buffer, instead of building it in memory first.
    pub(crate) fn out_with(
        &mut self,
        write: impl FnOnce(&mut dyn Write) -> std::io::Result<()>,
    ) -> Result<()> {
        let mut out = BufWriter::new(&mut *self.stdout);
        write(&mut out)
            .and_then(|()| out.flush())
            .map_err(CliError::stdout)
    }

    /// Write one status line of a command whose work is its side effects
    /// (`skills`): when the reader closed stdout the work goes on, so only
    /// other write failures are errors.
    pub(crate) fn status(&mut self, line: &str) -> Result<()> {
        match writeln!(self.stdout, "{line}") {
            Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
            result => result.map_err(CliError::stdout),
        }
    }
}

/// Output format of API results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Pretty JSON, as the official CLI (coloured on terminals)
    #[default]
    Json,
    /// A table sized to the terminal: list items as rows, or an object's fields
    Table,
    /// One compact JSON line per list item (streamed page by page with --all)
    Ndjson,
}

/// How to write a result.
#[derive(Debug, Clone, Default)]
pub struct OutputOptions {
    /// `--json`: the `{status, headers, body}` envelope.
    pub envelope: bool,
    /// `--output <file>`.
    pub file: Option<PathBuf>,
    /// `--format`.
    pub format: Format,
    /// `--columns`.
    pub columns: Vec<String>,
    /// Colour JSON output.
    pub color: bool,
    /// Table width.
    pub width: Option<u16>,
}

/// Escape control characters (C0, DEL, C1) of untrusted text as `\uXXXX`,
/// before the output adds its own line breaks.
pub fn escape_terminal_text(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        let code = u32::from(character);
        if code < 0x20 || (0x7f..=0x9f).contains(&code) {
            // Writing to a `String` cannot fail.
            let _ = write!(out, "\\u{code:04x}");
        } else {
            out.push(character);
        }
    }
    out
}

/// `value` as one line of compact JSON (as `JSON.stringify(v)`).
pub fn json_line(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|error| {
        CliError::new(
            Exit::Internal,
            format!("Cannot serialize the output: {error}"),
        )
    })
}

/// Pretty JSON with two-space indentation (as `JSON.stringify(v, null, 2)`).
pub fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// The `--json` envelope, borrowing the result it describes.
#[derive(Serialize)]
struct Envelope<'a> {
    status: u16,
    headers: &'a Map<String, Value>,
    body: Option<&'a Value>,
}

/// Write `result` in the selected format; the exit code is 5 for non-2xx.
pub fn write_result(result: &ApiResult, options: &OutputOptions, io: &mut Io<'_>) -> Result<Exit> {
    if let Some(path) = &options.file {
        let written = match &result.body {
            None => std::fs::write(path, ""),
            Some(Value::String(text)) => std::fs::write(path, text),
            Some(value) => std::fs::write(path, pretty(value)),
        };
        written.map_err(|error| {
            CliError::local(format!(
                "Cannot write --output file {}: {error}",
                path.display()
            ))
        })?;
    } else if options.envelope {
        let envelope = Envelope {
            status: result.status,
            headers: &result.headers,
            body: result.body.as_ref(),
        };
        io.out_with(|out| {
            serde_json::to_writer(&mut *out, &envelope)?;
            out.write_all(b"\n")
        })?;
    } else {
        match (&result.body, options.format) {
            (None, _) => {}
            (Some(Value::String(text)), _) => {
                io.out(text.as_bytes())?;
                if !text.ends_with('\n') {
                    io.out(b"\n")?;
                }
            }
            (Some(body), Format::Table) if result.ok => {
                let rendered = table::render(body, &options.columns, options.width);
                io.out_with(|out| {
                    out.write_all(rendered.as_bytes())?;
                    out.write_all(b"\n")
                })?;
            }
            (Some(body), Format::Ndjson) if result.ok => {
                write_ndjson(body, io)?;
            }
            (Some(body), _) => io.out_with(|out| {
                if options.color {
                    write_colored(body, 0, out)?;
                } else {
                    serde_json::to_writer_pretty(&mut *out, body)?;
                }
                out.write_all(b"\n")
            })?,
        }
    }
    Ok(if result.ok { Exit::Success } else { Exit::Http })
}

/// One compact line per item of a list response (`data`), else one line.
pub fn write_ndjson(body: &Value, io: &mut Io<'_>) -> Result<()> {
    match body.get("data").and_then(Value::as_array) {
        Some(items) => write_ndjson_items(items, io),
        None => write_ndjson_items(std::slice::from_ref(body), io),
    }
}

/// One compact line per item.
pub fn write_ndjson_items(items: &[Value], io: &mut Io<'_>) -> Result<()> {
    io.out_with(|out| {
        for item in items {
            serde_json::to_writer(&mut *out, item)?;
            out.write_all(b"\n")?;
        }
        Ok(())
    })
}

const KEY: &str = "\x1b[1;34m";
const STRING: &str = "\x1b[32m";
const NUMBER: &str = "\x1b[33m";
const LITERAL: &str = "\x1b[35m";
const RESET: &str = "\x1b[0m";

fn indent(out: &mut dyn Write, depth: usize) -> std::io::Result<()> {
    for _ in 0..depth {
        out.write_all(b"  ")?;
    }
    Ok(())
}

/// Pretty JSON with ANSI colours (same layout as [`pretty`]).
fn write_colored(value: &Value, depth: usize, out: &mut dyn Write) -> std::io::Result<()> {
    match value {
        Value::Null => write!(out, "{LITERAL}null{RESET}"),
        Value::Bool(flag) => write!(out, "{LITERAL}{flag}{RESET}"),
        Value::Number(number) => write!(out, "{NUMBER}{number}{RESET}"),
        Value::String(_) => write!(out, "{STRING}{value}{RESET}"),
        Value::Array(items) if items.is_empty() => out.write_all(b"[]"),
        Value::Array(items) => {
            out.write_all(b"[\n")?;
            for (index, item) in items.iter().enumerate() {
                indent(out, depth + 1)?;
                write_colored(item, depth + 1, out)?;
                if index + 1 < items.len() {
                    out.write_all(b",")?;
                }
                out.write_all(b"\n")?;
            }
            indent(out, depth)?;
            out.write_all(b"]")
        }
        Value::Object(fields) if fields.is_empty() => out.write_all(b"{}"),
        Value::Object(fields) => {
            out.write_all(b"{\n")?;
            for (index, (key, item)) in fields.iter().enumerate() {
                indent(out, depth + 1)?;
                out.write_all(KEY.as_bytes())?;
                serde_json::to_writer(&mut *out, key)?;
                write!(out, "{RESET}: ")?;
                write_colored(item, depth + 1, out)?;
                if index + 1 < fields.len() {
                    out.write_all(b",")?;
                }
                out.write_all(b"\n")?;
            }
            indent(out, depth)?;
            out.write_all(b"}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn strip_ansi(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn colours_follow_the_flag_then_the_terminal_and_the_environment() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            }
        };
        assert!(color_allowed(env(&[])));
        assert!(color_allowed(env(&[("NO_COLOR", ""), ("TERM", "xterm")])));
        assert!(!color_allowed(env(&[("NO_COLOR", "1")])));
        assert!(!color_allowed(env(&[("TERM", "dumb")])));

        assert!(use_color(ColorChoice::Always, false));
        assert!(!use_color(ColorChoice::Never, true));
        assert!(use_color(ColorChoice::Auto, true));
        assert!(!use_color(ColorChoice::Auto, false));

        let (mut out, mut err, mut input) = (Vec::new(), Vec::new(), std::io::empty());
        let mut io = Io::stdio(&mut out, &mut err, &mut input);
        io.stdout_tty = true;
        io.stderr_tty = false;
        io.color_allowed = true;
        assert!(io.color_stdout(ColorChoice::Auto));
        assert!(!io.color_stderr(ColorChoice::Auto));
        io.color_allowed = false;
        assert!(!io.color_stdout(ColorChoice::Auto));
        assert!(io.color_stdout(ColorChoice::Always));
        assert!(format!("{io:?}").starts_with("Io { stdout_tty: true"));
    }

    #[test]
    fn escapes_control_characters() {
        assert_eq!(
            escape_terminal_text("a\x1b]52;c;x\x07\r\n\u{9b}b"),
            "a\\u001b]52;c;x\\u0007\\u000d\\u000a\\u009bb"
        );
        assert_eq!(escape_terminal_text("ñandú 🦀"), "ñandú 🦀");
    }

    #[test]
    fn colored_json_has_the_pretty_layout() {
        let value = json!({"a": [1, "x", null, {"b": true}], "c": {}, "d": [], "e": "q\"uote"});
        let mut colored = Vec::new();
        write_colored(&value, 0, &mut colored).unwrap();
        let colored = String::from_utf8(colored).unwrap();
        assert!(colored.contains(KEY) && colored.contains(RESET));
        assert_eq!(strip_ansi(&colored), pretty(&value));
    }

    #[test]
    fn preserves_server_key_order() {
        let value: Value = serde_json::from_str(r#"{"z":1,"a":2}"#).unwrap();
        assert_eq!(value.to_string(), r#"{"z":1,"a":2}"#);
    }
}

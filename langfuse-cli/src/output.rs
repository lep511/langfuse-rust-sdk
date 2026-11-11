//! Writing API results: the official CLI's formats (pretty JSON, the
//! `--json` envelope, `--output`), plus `--format table|ndjson` and coloured
//! JSON on terminals.

use std::io::{Read, Write};
use std::path::PathBuf;

use clap::ValueEnum;
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
    /// Working directory: the project root of `skills` commands and the base
    /// of their relative paths.
    pub cwd: PathBuf,
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
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        let code = character as u32;
        if code < 0x20 || (0x7f..=0x9f).contains(&code) {
            out.push_str(&format!("\\u{code:04x}"));
        } else {
            out.push(character);
        }
    }
    out
}

fn write_out(io: &mut Io<'_>, text: &str) -> Result<()> {
    io.stdout
        .write_all(text.as_bytes())
        .map_err(|error| CliError::new(Exit::Internal, format!("Cannot write to stdout: {error}")))
}

/// Pretty JSON with two-space indentation (as `JSON.stringify(v, null, 2)`).
pub fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Write `result` in the selected format; the exit code is 5 for non-2xx.
pub fn write_result(result: &ApiResult, options: &OutputOptions, io: &mut Io<'_>) -> Result<Exit> {
    if let Some(path) = &options.file {
        let content = match &result.body {
            None => String::new(),
            Some(Value::String(text)) => text.clone(),
            Some(value) => pretty(value),
        };
        std::fs::write(path, content).map_err(|error| {
            CliError::local(format!(
                "Cannot write --output file {}: {error}",
                path.display()
            ))
        })?;
    } else if options.envelope {
        let mut envelope = Map::new();
        envelope.insert("status".into(), Value::from(result.status));
        envelope.insert("headers".into(), Value::Object(result.headers.clone()));
        envelope.insert("body".into(), result.body.clone().unwrap_or(Value::Null));
        write_out(io, &format!("{}\n", Value::Object(envelope)))?;
    } else {
        match (&result.body, options.format) {
            (None, _) => {}
            (Some(Value::String(text)), _) => {
                let newline = if text.ends_with('\n') { "" } else { "\n" };
                write_out(io, &format!("{text}{newline}"))?;
            }
            (Some(body), Format::Table) if result.ok => {
                let rendered = table::render(body, &options.columns, options.width);
                write_out(io, &format!("{rendered}\n"))?;
            }
            (Some(body), Format::Ndjson) if result.ok => {
                write_ndjson(body, io)?;
            }
            (Some(body), _) => {
                let text = if options.color {
                    colored(body)
                } else {
                    pretty(body)
                };
                write_out(io, &format!("{text}\n"))?;
            }
        }
    }
    Ok(if result.ok { Exit::Success } else { Exit::Http })
}

/// One compact line per item of a list response (`data`), else one line.
pub fn write_ndjson(body: &Value, io: &mut Io<'_>) -> Result<()> {
    match body.get("data").and_then(Value::as_array) {
        Some(items) => write_ndjson_items(items, io),
        None => write_out(io, &format!("{body}\n")),
    }
}

/// One compact line per item.
pub fn write_ndjson_items(items: &[Value], io: &mut Io<'_>) -> Result<()> {
    let mut text = String::new();
    for item in items {
        text.push_str(&item.to_string());
        text.push('\n');
    }
    write_out(io, &text)
}

const KEY: &str = "\x1b[1;34m";
const STRING: &str = "\x1b[32m";
const NUMBER: &str = "\x1b[33m";
const LITERAL: &str = "\x1b[35m";
const RESET: &str = "\x1b[0m";

/// Pretty JSON with ANSI colours (same layout as [`pretty`]).
pub fn colored(value: &Value) -> String {
    let mut out = String::new();
    write_colored(value, 0, &mut out);
    out
}

fn write_colored(value: &Value, depth: usize, out: &mut String) {
    let indent = |depth: usize| "  ".repeat(depth);
    match value {
        Value::Null => out.push_str(&format!("{LITERAL}null{RESET}")),
        Value::Bool(flag) => out.push_str(&format!("{LITERAL}{flag}{RESET}")),
        Value::Number(number) => out.push_str(&format!("{NUMBER}{number}{RESET}")),
        Value::String(_) => out.push_str(&format!("{STRING}{value}{RESET}")),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                out.push_str(&indent(depth + 1));
                write_colored(item, depth + 1, out);
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&indent(depth));
            out.push(']');
        }
        Value::Object(fields) if fields.is_empty() => out.push_str("{}"),
        Value::Object(fields) => {
            out.push_str("{\n");
            for (index, (key, item)) in fields.iter().enumerate() {
                out.push_str(&indent(depth + 1));
                out.push_str(&format!("{KEY}{}{RESET}: ", Value::String(key.clone())));
                write_colored(item, depth + 1, out);
                if index + 1 < fields.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&indent(depth));
            out.push('}');
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
        assert_eq!(strip_ansi(&colored(&value)), pretty(&value));
    }

    #[test]
    fn preserves_server_key_order() {
        let value: Value = serde_json::from_str(r#"{"z":1,"a":2}"#).unwrap();
        assert_eq!(value.to_string(), r#"{"z":1,"a":2}"#);
    }
}

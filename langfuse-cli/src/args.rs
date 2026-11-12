//! Value parsers and argument groups shared by the `api` actions.

use std::path::PathBuf;

use clap::Args;
use clap::builder::{PossibleValuesParser, TypedValueParser};
use langfuse_rust_sdk::pagination::{CursorParams, PageParams};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{CliError, Exit, Result};
use crate::operation::Context;

/// Any JSON value.
pub fn json_value(text: &str) -> std::result::Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("invalid JSON: {error}"))
}

/// A JSON object.
pub fn json_object(text: &str) -> std::result::Result<Value, String> {
    match json_value(text)? {
        value @ Value::Object(_) => Ok(value),
        _ => Err("expected a JSON object".to_owned()),
    }
}

/// A string that must be valid JSON (sent as text, e.g. `--filter`).
pub fn json_text(text: &str) -> std::result::Result<String, String> {
    json_value(text).map(|_| text.to_owned())
}

/// An RFC 3339 / ISO 8601 date-time (`2026-11-01T00:00:00Z`), sent unchanged.
pub fn date_time(text: &str) -> std::result::Result<String, String> {
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|_| text.to_owned())
        .map_err(|error| {
            format!("expected an RFC 3339 date-time such as 2026-11-01T00:00:00Z ({error})")
        })
}

/// A `true` / `false` flag whose bare form (`--flag`) means `true`.
///
/// Every boolean API parameter pairs this with a hidden `--no-<flag>`; see
/// [`tri_state`].
pub fn bool_value(text: &str) -> std::result::Result<bool, String> {
    match text {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("expected true or false, got {other}")),
    }
}

/// Combine `--flag[=true|false]` and `--no-flag` into the value to send.
pub fn tri_state(flag: Option<bool>, negated: bool) -> Option<bool> {
    if negated { Some(false) } else { flag }
}

/// The parser of a flag whose values are those of one of the SDK's wire enums
/// (`"TRACE"` → `AnnotationQueueObjectType::Trace`): clap lists and checks
/// `values`, and the flag has the enum's type, so nothing is converted later.
///
/// The SDK's enums read an unknown string as `Unknown`, which cannot be sent;
/// a listed value that is not a variant is therefore rejected here, by
/// checking that it serializes back to itself.
pub fn wire<T>(values: &'static [&'static str]) -> impl TypedValueParser<Value = T>
where
    T: DeserializeOwned + Serialize + Clone + Send + Sync + 'static,
{
    PossibleValuesParser::new(values).try_map(|text: String| {
        let value = Value::String(text);
        let parsed: T = serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        match serde_json::to_value(&parsed) {
            Ok(sent) if sent == value => Ok(parsed),
            _ => Err(format!("{value} is not a value the API accepts")),
        }
    })
}

/// The value of a flag that clap requires unless the body comes from
/// `--body-json` / `--body-file`.
pub fn required<T>(flag: &str, value: Option<T>) -> Result<T> {
    value.ok_or_else(|| {
        CliError::usage(format!(
            "--{flag} is required to build the body from field flags"
        ))
    })
}

/// Expand the values of a repeatable array flag: a value that is a JSON array
/// contributes all its elements, any other value is one item.
pub fn expand_items(values: &[Value]) -> Vec<Value> {
    values
        .iter()
        .flat_map(|value| match value {
            Value::Array(items) => items.clone(),
            other => vec![other.clone()],
        })
        .collect()
}

/// Expand JSON-object items (`--prompt '{"role":…}'`, or one JSON array of
/// them) and check each one is an object.
pub fn expand_objects(flag: &str, values: &[Value]) -> Result<Vec<Value>> {
    let items = expand_items(values);
    for (index, item) in items.iter().enumerate() {
        if !item.is_object() {
            return Err(CliError::usage(format!(
                "--{flag}: array item {index} must be an object"
            )));
        }
    }
    Ok(items)
}

/// Expand string items: a value that parses as a JSON array of strings
/// contributes its elements, any other value is taken literally.
pub fn expand_strings(flag: &str, values: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        match serde_json::from_str::<Value>(value) {
            Ok(Value::Array(items)) => {
                for (index, item) in items.into_iter().enumerate() {
                    match item {
                        Value::String(text) => out.push(text),
                        _ => {
                            return Err(CliError::usage(format!(
                                "--{flag}: array item {index} must be a string"
                            )));
                        }
                    }
                }
            }
            _ => out.push(value.clone()),
        }
    }
    Ok(out)
}

/// `Some(values)` when the repeatable flag was given at least once.
pub fn non_empty<T>(values: Vec<T>) -> Option<Vec<T>> {
    (!values.is_empty()).then_some(values)
}

/// A JSON object body built from field flags, for bodies whose SDK request
/// types hold nested values the CLI cannot construct from JSON (the SDK's
/// request types are serialize-only). Unset fields are left out.
#[derive(Debug, Default)]
pub struct ObjectBody(serde_json::Map<String, Value>);

impl ObjectBody {
    /// An empty object.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set `key` when `value` is `Some`.
    pub fn opt(mut self, key: &str, value: Option<impl Into<Value>>) -> Self {
        if let Some(value) = value {
            self.0.insert(key.to_owned(), value.into());
        }
        self
    }

    /// Set `key` to `items` when there is at least one.
    pub fn items(self, key: &str, items: Vec<Value>) -> Self {
        let value = (!items.is_empty()).then_some(Value::Array(items));
        self.opt(key, value)
    }

    /// The object.
    pub fn build(self) -> Value {
        Value::Object(self.0)
    }
}

/// `--page` / `--limit` of page-based lists.
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Pagination")]
pub struct PageArgs {
    /// Page number, starts at 1
    #[arg(long, value_name = "N")]
    pub page: Option<i64>,

    /// Items per page
    #[arg(long, value_name = "N")]
    pub limit: Option<i64>,
}

impl PageArgs {
    /// The SDK's page parameters.
    pub fn params(&self) -> PageParams {
        PageParams {
            page: self.page,
            limit: self.limit,
        }
    }
}

/// `--cursor` / `--limit` of cursor-based lists.
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Pagination")]
pub struct CursorArgs {
    /// Cursor from the `meta.cursor` of the previous page
    #[arg(long)]
    pub cursor: Option<String>,

    /// Items per page
    #[arg(long, value_name = "N")]
    pub limit: Option<i64>,
}

impl CursorArgs {
    /// The SDK's cursor parameters.
    pub fn params(&self) -> CursorParams {
        CursorParams {
            cursor: self.cursor.clone(),
            limit: self.limit,
        }
    }
}

/// Default `--max-items` of `--all`.
pub const DEFAULT_MAX_ITEMS: u64 = 1000;

/// `--all` / `--max-items` of paginated lists.
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Pagination")]
pub struct AllPages {
    /// Fetch every page (bounded by --max-items and 100 requests)
    #[arg(long)]
    pub all: bool,

    /// Item cap for --all [default: 1000]
    #[arg(long, value_name = "N", requires = "all", value_parser = clap::value_parser!(u64).range(1..))]
    pub max_items: Option<u64>,
}

/// The JSON body channel of operations whose body has field flags: it
/// conflicts with the flags (their group is `body-fields`).
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Request body")]
pub struct BodyChannel {
    /// Lossless JSON request body (instead of the field flags)
    #[arg(
        long = "body-json",
        alias = "json-body",
        value_name = "JSON",
        value_parser = json_value,
        conflicts_with = "body_file"
    )]
    pub body_json: Option<Value>,

    /// Read the JSON request body from a file, or from stdin with -
    #[arg(long = "body-file", value_name = "PATH|-")]
    pub body_file: Option<PathBuf>,
}

impl BodyChannel {
    /// The body given through `--body-json` / `--body-file`, if any.
    pub fn read(&self, ctx: &mut Context<'_>) -> Result<Option<Value>> {
        if let Some(body) = &self.body_json {
            return Ok(Some(body.clone()));
        }
        match &self.body_file {
            Some(path) => read_body_file(path, ctx).map(Some),
            None => Ok(None),
        }
    }
}

/// The required JSON body of operations without field flags.
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Request body")]
#[group(id = "json-body", required = true, multiple = false)]
pub struct JsonBody {
    /// Lossless JSON request body
    #[arg(long = "body-json", alias = "json-body", value_name = "JSON", value_parser = json_value)]
    pub body_json: Option<Value>,

    /// Read the JSON request body from a file, or from stdin with -
    #[arg(long = "body-file", value_name = "PATH|-")]
    pub body_file: Option<PathBuf>,
}

impl JsonBody {
    /// The body (clap guarantees one of the two flags).
    pub fn read(&self, ctx: &mut Context<'_>) -> Result<Value> {
        if let Some(body) = &self.body_json {
            return Ok(body.clone());
        }
        match &self.body_file {
            Some(path) => read_body_file(path, ctx),
            None => Err(CliError::usage(
                "A request body is required: pass --body-json or --body-file",
            )),
        }
    }
}

fn read_body_file(path: &PathBuf, ctx: &mut Context<'_>) -> Result<Value> {
    let stdin = path.as_os_str() == "-";
    let text = if stdin {
        let mut text = String::new();
        ctx.stdin
            .read_to_string(&mut text)
            .map(|_| text)
            .map_err(|error| CliError::local(format!("Cannot read body file -: {error}")))?
    } else {
        std::fs::read_to_string(path).map_err(|error| {
            CliError::local(format!("Cannot read body file {}: {error}", path.display()))
        })?
    };
    serde_json::from_str(&text).map_err(|error| {
        CliError::new(
            Exit::Local,
            format!(
                "Invalid JSON in {}: {error}",
                if stdin {
                    "stdin".to_owned()
                } else {
                    path.display().to_string()
                }
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_parsers() {
        assert_eq!(json_value("[1]").unwrap(), json!([1]));
        assert!(json_value("{").is_err());
        assert!(json_object("[1]").is_err());
        assert_eq!(json_text(r#"[{"a":1}]"#).unwrap(), r#"[{"a":1}]"#);
        assert!(date_time("2026-11-01T00:00:00Z").is_ok());
        assert!(date_time("2026-11-01T00:00:00.123+02:00").is_ok());
        assert!(date_time("yesterday").is_err());
        assert_eq!(tri_state(Some(true), true), Some(false));
        assert_eq!(tri_state(None, false), None);
    }

    #[test]
    fn wire_flags_have_the_type_of_the_sdk_enum() {
        use langfuse_rust_sdk::models::annotation_queues::AnnotationQueueStatus;
        let command = clap::Command::new("test").arg(
            clap::Arg::new("status")
                .long("status")
                .value_parser(wire::<AnnotationQueueStatus>(&["PENDING", "ARCHIVED"])),
        );
        let parse = |value: &str| {
            command
                .clone()
                .try_get_matches_from(["test", "--status", value])
                .map(|matches| matches.get_one::<AnnotationQueueStatus>("status").copied())
        };
        assert_eq!(
            parse("PENDING").unwrap(),
            Some(AnnotationQueueStatus::Pending)
        );
        // Not listed.
        assert!(parse("COMPLETED").is_err());
        // Listed, but not a variant: the SDK would read it as `Unknown`.
        let error = parse("ARCHIVED").unwrap_err().to_string();
        assert!(error.contains("is not a value the API accepts"), "{error}");
        assert_eq!(required("status", Some(1)).unwrap(), 1);
        assert_eq!(
            required::<i32>("status", None).unwrap_err().message,
            "--status is required to build the body from field flags"
        );
    }

    #[test]
    fn array_items_expand_json_arrays() {
        let items = expand_items(&[json!({"a":1}), json!([{"b":2},{"c":3}])]);
        assert_eq!(items, [json!({"a":1}), json!({"b":2}), json!({"c":3})]);
        assert!(expand_objects("prompt", &[json!("text")]).is_err());
        assert_eq!(
            expand_strings("labels", &["a".into(), r#"["b","c"]"#.into()]).unwrap(),
            ["a", "b", "c"]
        );
        assert!(expand_strings("labels", &["[1]".into()]).is_err());
    }

    #[test]
    fn body_file_from_stdin() {
        let mut stdin: &[u8] = br#"{"name":"x"}"#;
        let mut ctx = Context { stdin: &mut stdin };
        let body = JsonBody {
            body_json: None,
            body_file: Some("-".into()),
        };
        assert_eq!(body.read(&mut ctx).unwrap(), json!({"name":"x"}));

        let mut stdin: &[u8] = b"not json";
        let mut ctx = Context { stdin: &mut stdin };
        let error = body.read(&mut ctx).unwrap_err();
        assert_eq!(error.exit, Exit::Local);
        assert!(error.message.starts_with("Invalid JSON in stdin"));
    }
}

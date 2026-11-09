//! Prompt request and response types, plus variable and placeholder compilation.
//!
//! Prompt templates use `{{variable}}` placeholders (whitespace inside the
//! braces is ignored). [`Prompt::compile`] substitutes them; chat prompts can
//! also splice message lists into their placeholders with
//! [`Prompt::compile_with_placeholders`].
//!
//! ```
//! use langfuse_rust_sdk::models::prompts::{CompiledPrompt, Prompt};
//! use serde_json::json;
//!
//! let prompt: Prompt = serde_json::from_value(json!({
//!     "type": "text", "name": "greeting", "version": 1, "config": {},
//!     "labels": ["production"], "tags": [],
//!     "prompt": "Hello {{ name }}, welcome to {{service}}!"
//! }))
//! .unwrap();
//!
//! assert_eq!(prompt.variables(), ["name", "service"]);
//! let compiled = prompt
//!     .compile(&json!({"name": "Ada", "service": "Langfuse"}))
//!     .unwrap();
//! assert_eq!(compiled.as_text(), Some("Hello Ada, welcome to Langfuse!"));
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::error::LangfuseError;

/// Fields shared by text and chat prompts.
///
/// Implements `Default`, so tests and mocks can build one with
/// `BasePrompt { name: "..".into(), ..Default::default() }` without breaking
/// when fields are added.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasePrompt {
    pub name: String,
    pub version: i64,
    pub config: serde_json::Value,
    pub labels: Vec<String>,
    pub tags: Vec<String>,
    pub commit_message: Option<String>,
    pub resolution_graph: Option<serde_json::Value>,
    /// `true` when the prompt is the caller-supplied fallback returned by
    /// [`PromptsClient::get_cached`](crate::api::prompts::PromptsClient::get_cached)
    /// because the prompt could not be fetched. Never sent by the API.
    #[serde(skip)]
    pub is_fallback: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Prompt {
    Chat {
        #[serde(flatten)]
        base: BasePrompt,
        prompt: Vec<ChatPromptMessage>,
    },
    Text {
        #[serde(flatten)]
        base: BasePrompt,
        prompt: String,
    },
}

impl Prompt {
    /// The fields shared by text and chat prompts.
    pub fn base(&self) -> &BasePrompt {
        match self {
            Prompt::Chat { base, .. } | Prompt::Text { base, .. } => base,
        }
    }

    /// The prompt name.
    pub fn name(&self) -> &str {
        &self.base().name
    }

    /// The prompt version.
    pub fn version(&self) -> i64 {
        self.base().version
    }

    /// The prompt config (model parameters and other settings).
    pub fn config(&self) -> &serde_json::Value {
        &self.base().config
    }

    /// Whether this is a fallback prompt; see [`BasePrompt::is_fallback`].
    pub fn is_fallback(&self) -> bool {
        self.base().is_fallback
    }

    /// Names of the `{{variables}}` used by the prompt, in order of appearance
    /// and including repeats. For chat prompts only message contents are
    /// scanned (placeholders hold no variables).
    pub fn variables(&self) -> Vec<String> {
        match self {
            Prompt::Text { prompt, .. } => find_variable_names(prompt),
            Prompt::Chat { prompt, .. } => prompt
                .iter()
                .filter_map(|message| match message {
                    ChatPromptMessage::Message(m) => Some(find_variable_names(&m.content)),
                    ChatPromptMessage::Placeholder(_) => None,
                })
                .flatten()
                .collect(),
        }
    }

    /// Substitute `{{variables}}` with the values of `variables`.
    ///
    /// `variables` is anything that serializes to a JSON object: a
    /// `HashMap`/`BTreeMap`, a `serde_json::json!` object or a struct deriving
    /// `Serialize` (`()` means no variables). String values are inserted as-is,
    /// `null` as an empty string, and other values as compact JSON. Variables
    /// without a value are left untouched (`{{name}}` stays in the output).
    ///
    /// Chat prompts keep their placeholders; use
    /// [`compile_with_placeholders`](Self::compile_with_placeholders) to fill them.
    ///
    /// Returns [`LangfuseError::Serialization`] if `variables` does not
    /// serialize to an object.
    pub fn compile<V: Serialize + ?Sized>(
        &self,
        variables: &V,
    ) -> Result<CompiledPrompt, LangfuseError> {
        self.compile_with_placeholders(variables, std::iter::empty::<(&str, &[ChatMessage])>())
    }

    /// Like [`compile`](Self::compile), and also replace each chat
    /// placeholder named in `placeholders` with its messages.
    ///
    /// Placeholders are resolved first, so their message contents get the
    /// variable substitution too. Placeholders without an entry stay in the
    /// output as [`ChatPromptMessage::Placeholder`]; entries that match no
    /// placeholder are ignored, as are all placeholders of text prompts.
    ///
    /// ```
    /// # use langfuse_rust_sdk::models::prompts::{ChatMessage, ChatPromptMessage, Prompt};
    /// # use serde_json::json;
    /// # let prompt: Prompt = serde_json::from_value(json!({
    /// #     "type": "chat", "name": "support", "version": 1, "config": {},
    /// #     "labels": [], "tags": [],
    /// #     "prompt": [
    /// #         {"role": "system", "content": "You are a {{tone}} assistant."},
    /// #         {"type": "placeholder", "name": "history"},
    /// #         {"role": "user", "content": "{{question}}"}
    /// #     ]
    /// # })).unwrap();
    /// let history = vec![ChatMessage {
    ///     role: "user".into(),
    ///     content: "Hi, I'm {{customer}}".into(),
    /// }];
    /// let messages = prompt
    ///     .compile_with_placeholders(
    ///         &json!({"tone": "friendly", "customer": "Ada", "question": "Where is my order?"}),
    ///         [("history", &history)],
    ///     )?
    ///     .into_chat()
    ///     .unwrap();
    /// assert_eq!(messages.len(), 3);
    /// # Ok::<(), langfuse_rust_sdk::LangfuseError>(())
    /// ```
    pub fn compile_with_placeholders<V, I, K, M>(
        &self,
        variables: &V,
        placeholders: I,
    ) -> Result<CompiledPrompt, LangfuseError>
    where
        V: Serialize + ?Sized,
        I: IntoIterator<Item = (K, M)>,
        K: AsRef<str>,
        M: AsRef<[ChatMessage]>,
    {
        let variables = variables_object(variables)?;
        let compile = |content: &str| compile_template(content, &variables);
        match self {
            Prompt::Text { prompt, .. } => Ok(CompiledPrompt::Text(compile(prompt))),
            Prompt::Chat { prompt, .. } => {
                let placeholders: HashMap<String, M> = placeholders
                    .into_iter()
                    .map(|(name, messages)| (name.as_ref().to_owned(), messages))
                    .collect();
                let mut compiled = Vec::with_capacity(prompt.len());
                for message in prompt {
                    match message {
                        ChatPromptMessage::Message(m) => {
                            compiled.push(ChatPromptMessage::Message(ChatMessage {
                                role: m.role.clone(),
                                content: compile(&m.content),
                            }))
                        }
                        ChatPromptMessage::Placeholder(p) => match placeholders.get(&p.name) {
                            Some(messages) => compiled.extend(messages.as_ref().iter().map(|m| {
                                ChatPromptMessage::Message(ChatMessage {
                                    role: m.role.clone(),
                                    content: compile(&m.content),
                                })
                            })),
                            None => {
                                tracing::debug!(placeholder = %p.name, "prompt placeholder left unresolved");
                                compiled.push(message.clone());
                            }
                        },
                    }
                }
                Ok(CompiledPrompt::Chat(compiled))
            }
        }
    }

    /// Like [`compile_with_placeholders`](Self::compile_with_placeholders),
    /// but each placeholder value can be anything that serializes to an array
    /// of JSON objects (for example the request message types of an LLM
    /// client crate), and every field of those messages is kept: `tool_calls`,
    /// `tool_call_id`, `name`, ...
    ///
    /// The output messages are JSON objects: prompt messages as
    /// `{"role", "content"}`, placeholder messages as given with a string
    /// `content` compiled (other `content` shapes are left as-is), and
    /// unresolved placeholders as `{"type": "placeholder", "name"}`. Convert
    /// them to your client's message type with `serde_json::from_value`.
    ///
    /// ```
    /// # use langfuse_rust_sdk::models::prompts::Prompt;
    /// # use serde_json::json;
    /// # let prompt: Prompt = serde_json::from_value(json!({
    /// #     "type": "chat", "name": "agent", "version": 1, "config": {},
    /// #     "labels": [], "tags": [],
    /// #     "prompt": [
    /// #         {"role": "system", "content": "You are a {{tone}} assistant."},
    /// #         {"type": "placeholder", "name": "history"}
    /// #     ]
    /// # })).unwrap();
    /// let history = json!([
    ///     {"role": "assistant", "content": "", "tool_calls": [
    ///         {"id": "call_1", "type": "function",
    ///          "function": {"name": "get_weather", "arguments": "{}"}}
    ///     ]},
    ///     {"role": "tool", "content": "Sunny", "tool_call_id": "call_1"}
    /// ]);
    /// let messages = prompt
    ///     .compile_with_json_placeholders(&json!({"tone": "concise"}), [("history", &history)])?
    ///     .into_chat()
    ///     .unwrap();
    /// assert_eq!(messages[1]["tool_calls"][0]["id"], "call_1");
    /// assert_eq!(messages[2]["tool_call_id"], "call_1");
    /// # Ok::<(), langfuse_rust_sdk::LangfuseError>(())
    /// ```
    ///
    /// Returns [`LangfuseError::Serialization`] if `variables` is not an
    /// object or a placeholder value is not an array of objects.
    pub fn compile_with_json_placeholders<V, I, K, M>(
        &self,
        variables: &V,
        placeholders: I,
    ) -> Result<CompiledPrompt<Value>, LangfuseError>
    where
        V: Serialize + ?Sized,
        I: IntoIterator<Item = (K, M)>,
        K: AsRef<str>,
        M: Serialize,
    {
        let variables = variables_object(variables)?;
        let compile = |content: &str| compile_template(content, &variables);
        let prompt = match self {
            Prompt::Text { prompt, .. } => return Ok(CompiledPrompt::Text(compile(prompt))),
            Prompt::Chat { prompt, .. } => prompt,
        };

        let mut filled: HashMap<String, Vec<Map<String, Value>>> = HashMap::new();
        for (name, messages) in placeholders {
            let name = name.as_ref();
            filled.insert(name.to_owned(), placeholder_messages(name, &messages)?);
        }

        let mut compiled = Vec::with_capacity(prompt.len());
        for message in prompt {
            match message {
                ChatPromptMessage::Message(m) => {
                    compiled.push(json!({"role": m.role, "content": compile(&m.content)}));
                }
                ChatPromptMessage::Placeholder(p) => match filled.get(&p.name) {
                    Some(messages) => compiled.extend(messages.iter().map(|message| {
                        let mut message = message.clone();
                        if let Some(Value::String(content)) = message.get_mut("content") {
                            let text = compile(content);
                            *content = text;
                        }
                        Value::Object(message)
                    })),
                    None => {
                        tracing::debug!(placeholder = %p.name, "prompt placeholder left unresolved");
                        compiled.push(json!({"type": "placeholder", "name": p.name}));
                    }
                },
            }
        }
        Ok(CompiledPrompt::Chat(compiled))
    }
}

/// The result of compiling a prompt: text, or chat messages with any
/// unresolved placeholders still in place.
///
/// The messages are [`ChatPromptMessage`]s for [`Prompt::compile`] and
/// [`Prompt::compile_with_placeholders`], and JSON objects
/// (`CompiledPrompt<serde_json::Value>`) for
/// [`Prompt::compile_with_json_placeholders`].
#[derive(Debug, Clone)]
pub enum CompiledPrompt<M = ChatPromptMessage> {
    /// A compiled text prompt.
    Text(String),
    /// Compiled chat messages.
    Chat(Vec<M>),
}

impl<M> CompiledPrompt<M> {
    /// The text of a compiled text prompt.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            CompiledPrompt::Text(text) => Some(text),
            CompiledPrompt::Chat(_) => None,
        }
    }

    /// The messages of a compiled chat prompt.
    pub fn as_chat(&self) -> Option<&[M]> {
        match self {
            CompiledPrompt::Chat(messages) => Some(messages),
            CompiledPrompt::Text(_) => None,
        }
    }

    /// The text of a compiled text prompt.
    pub fn into_text(self) -> Option<String> {
        match self {
            CompiledPrompt::Text(text) => Some(text),
            CompiledPrompt::Chat(_) => None,
        }
    }

    /// The messages of a compiled chat prompt.
    pub fn into_chat(self) -> Option<Vec<M>> {
        match self {
            CompiledPrompt::Chat(messages) => Some(messages),
            CompiledPrompt::Text(_) => None,
        }
    }
}

/// A regular chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// A placeholder in a chat prompt, filled with a list of messages at compile time.
/// Serialized as `{"type": "placeholder", "name": ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename = "placeholder")]
pub struct PlaceholderMessage {
    pub name: String,
}

/// One entry of a chat prompt (spec `ChatMessageWithPlaceholders`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatPromptMessage {
    Message(ChatMessage),
    Placeholder(PlaceholderMessage),
}

impl ChatPromptMessage {
    /// A regular message with `role` and `content`.
    pub fn message(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Message(ChatMessage {
            role: role.into(),
            content: content.into(),
        })
    }

    /// A placeholder named `name`.
    pub fn placeholder(name: impl Into<String>) -> Self {
        Self::Placeholder(PlaceholderMessage { name: name.into() })
    }
}

impl From<ChatMessage> for ChatPromptMessage {
    fn from(message: ChatMessage) -> Self {
        Self::Message(message)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CreatePromptRequest {
    #[serde(rename_all = "camelCase")]
    Chat {
        name: String,
        prompt: Vec<ChatPromptMessage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        config: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        labels: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit_message: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Text {
        name: String,
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        config: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        labels: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit_message: Option<String>,
    },
}

// ═══════════════════════════════════════════════════════════════════════
//  Template engine
// ═══════════════════════════════════════════════════════════════════════

const OPENING: &str = "{{";
const CLOSING: &str = "}}";

/// The next `{{name}}` at or after `from`: `(trimmed name, start, end)`, where
/// `end` is just past the closing braces.
fn next_variable(content: &str, from: usize) -> Option<(&str, usize, usize)> {
    let start = from + content[from..].find(OPENING)?;
    let close = start + content[start..].find(CLOSING)?;
    let name = content[start + OPENING.len()..close].trim();
    Some((name, start, close + CLOSING.len()))
}

fn find_variable_names(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut from = 0;
    while let Some((name, _, end)) = next_variable(content, from) {
        names.push(name.to_owned());
        from = end;
    }
    names
}

fn compile_template(content: &str, variables: &Map<String, Value>) -> String {
    let mut out = String::with_capacity(content.len());
    let mut from = 0;
    while let Some((name, start, end)) = next_variable(content, from) {
        out.push_str(&content[from..start]);
        match variables.get(name) {
            Some(Value::String(text)) => out.push_str(text),
            Some(Value::Null) => {}
            Some(other) => out.push_str(&other.to_string()),
            None => out.push_str(&content[start..end]),
        }
        from = end;
    }
    out.push_str(&content[from..]);
    out
}

/// Serialize a placeholder value into its list of message objects.
fn placeholder_messages<M: Serialize + ?Sized>(
    name: &str,
    messages: &M,
) -> Result<Vec<Map<String, Value>>, LangfuseError> {
    let invalid = || {
        LangfuseError::Serialization(<serde_json::Error as serde::ser::Error>::custom(format!(
            "placeholder '{name}' must serialize to an array of message objects"
        )))
    };
    match serde_json::to_value(messages)? {
        Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                Value::Object(message) => Ok(message),
                _ => Err(invalid()),
            })
            .collect(),
        _ => Err(invalid()),
    }
}

/// Serialize `variables` into the JSON object used for substitution.
fn variables_object<V: Serialize + ?Sized>(
    variables: &V,
) -> Result<Map<String, Value>, LangfuseError> {
    match serde_json::to_value(variables)? {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        other => Err(LangfuseError::Serialization(
            <serde_json::Error as serde::ser::Error>::custom(format!(
                "prompt variables must serialize to a JSON object, got {other}"
            )),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{compile_template, find_variable_names};
    use serde_json::{Map, Value, json};

    fn vars(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => unreachable!("test variables are objects"),
        }
    }

    fn compile(template: &str, variables: Value) -> String {
        compile_template(template, &vars(variables))
    }

    #[test]
    fn substitutes_variables() {
        assert_eq!(
            compile("Hello, {{ name }}!", json!({"name": "John"})),
            "Hello, John!"
        );
        assert_eq!(
            compile(
                "{{greeting}}, {{name}}! Your balance is {{balance}}.",
                json!({"greeting": "Hello", "name": "John", "balance": "$100"})
            ),
            "Hello, John! Your balance is $100."
        );
        assert_eq!(
            compile("This is a {{content}}.", json!({"content": "dog"})),
            "This is a dog."
        );
        assert_eq!(
            compile("Symbols: {{symbol}}.", json!({"symbol": "@$%^&*"})),
            "Symbols: @$%^&*."
        );
        assert_eq!(
            compile("Hello, {{    name }}!", json!({"name": "John"})),
            "Hello, John!"
        );
    }

    #[test]
    fn leaves_text_without_variables_untouched() {
        assert_eq!(compile("This is a test.", json!({})), "This is a test.");
        assert_eq!(compile("No variables", json!({})), "No variables");
        assert_eq!(compile("}}", json!({"name": "john"})), "}}");
    }

    #[test]
    fn keeps_missing_variables() {
        assert_eq!(compile("Hello, {{name}}!", json!({})), "Hello, {{name}}!");
        assert_eq!(
            compile("Hello, {{ name }}!", json!({})),
            "Hello, {{ name }}!"
        );
    }

    #[test]
    fn null_and_empty_values_become_empty() {
        assert_eq!(
            compile("Hello, {{name}}!", json!({"name": null})),
            "Hello, !"
        );
        assert_eq!(compile("Hello, {{name}}!", json!({"name": ""})), "Hello, !");
    }

    #[test]
    fn non_string_values_are_compact_json() {
        assert_eq!(
            compile("{{a}} + {{a}} = {{b}}", json!({"a": 1, "b": 2})),
            "1 + 1 = 2"
        );
        assert_eq!(compile("{{a}} + {{a}}", json!({"a": 1, "b": 2})), "1 + 1");
        assert_eq!(
            compile(
                "{{flag}} {{ratio}} {{list}} {{obj}}",
                json!({"flag": true, "ratio": 0.5, "list": [1, "x"], "obj": {"k": "v"}})
            ),
            r#"true 0.5 [1,"x"] {"k":"v"}"#
        );
    }

    #[test]
    fn unmatched_braces_are_literal() {
        assert_eq!(
            compile(
                "Hello, {{name! Your balance is $100.",
                json!({"name": "John"})
            ),
            "Hello, {{name! Your balance is $100."
        );
        assert_eq!(
            compile(
                "Hello, {{name}}! Your balance is $100}}",
                json!({"name": "John"})
            ),
            "Hello, John! Your balance is $100}}"
        );
        assert_eq!(
            compile("{{a}} + {a} = {{b}", json!({"a": 1, "b": 2})),
            "1 + {a} = {{b}"
        );
    }

    #[test]
    fn json_blocks_without_matching_variable_are_kept() {
        let template = "{{a}} + {{\n    \"key1\": \"val1\",\n    \"key2\": \"val2\",\n    }}";
        let expected = "1 + {{\n    \"key1\": \"val1\",\n    \"key2\": \"val2\",\n    }}";
        assert_eq!(compile(template, json!({"a": 1, "b": 2})), expected);
    }

    #[test]
    fn values_are_not_escaped() {
        let some_json = "{\n  \"user\": {\"id\": 12345, \"roles\": [\"user\", \"admin\"]}\n}";
        assert_eq!(
            compile("{{some_json}}", json!({"some_json": some_json})),
            some_json
        );
    }

    #[test]
    fn names_are_case_sensitive() {
        assert_eq!(
            compile(
                "{{Name}} != {{name}}",
                json!({"Name": "John", "name": "john"})
            ),
            "John != john"
        );
    }

    #[test]
    fn handles_multibyte_text() {
        assert_eq!(
            compile("¡Hola {{ nombre }}! ñandú", json!({"nombre": "José"})),
            "¡Hola José! ñandú"
        );
    }

    #[test]
    fn finds_variable_names_in_order_with_repeats() {
        assert_eq!(find_variable_names("{{a}} {{ b }} {{a}}"), ["a", "b", "a"]);
        assert_eq!(find_variable_names("no {{ unclosed"), Vec::<String>::new());
        assert_eq!(find_variable_names("{{}}"), [""]);
    }
}

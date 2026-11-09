//! `Prompt::variables`, `Prompt::compile` and `Prompt::compile_with_placeholders`.

use std::collections::{BTreeMap, HashMap};

use langfuse_rust_sdk::LangfuseError;
use langfuse_rust_sdk::models::prompts::{ChatMessage, ChatPromptMessage, CompiledPrompt, Prompt};
use serde::Serialize;
use serde_json::{Value, json};

fn text_prompt(template: &str) -> Prompt {
    serde_json::from_value(json!({
        "type": "text", "name": "greeting", "version": 4, "config": {"temperature": 0.2},
        "labels": ["production"], "tags": [], "prompt": template
    }))
    .unwrap()
}

/// System message, `examples` placeholder, user message.
fn chat_prompt() -> Prompt {
    serde_json::from_value(json!({
        "type": "chat", "name": "assistant", "version": 1, "config": {},
        "labels": [], "tags": [],
        "prompt": [
            {"role": "system", "content": "You are a {{role}} assistant", "type": "chatmessage"},
            {"type": "placeholder", "name": "examples"},
            {"role": "user", "content": "Help me with {{task}}"}
        ]
    }))
    .unwrap()
}

fn message(role: &str, content: &str) -> ChatMessage {
    ChatMessage {
        role: role.into(),
        content: content.into(),
    }
}

/// Contents of compiled chat messages; `None` for an unresolved placeholder.
fn contents(compiled: CompiledPrompt) -> Vec<Option<String>> {
    compiled
        .into_chat()
        .expect("chat prompt")
        .into_iter()
        .map(|m| match m {
            ChatPromptMessage::Message(m) => Some(m.content),
            ChatPromptMessage::Placeholder(_) => None,
        })
        .collect()
}

fn some(values: &[&str]) -> Vec<Option<String>> {
    values.iter().map(|v| Some(v.to_string())).collect()
}

// ═══════════════════════════════════════════════════════════════════════
//  Text prompts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn text_prompt_compiles_with_json_object() {
    let prompt = text_prompt("Hello {{ name }}, your balance is {{balance}}.");
    let compiled = prompt
        .compile(&json!({"name": "Ada", "balance": 100}))
        .unwrap();
    assert_eq!(compiled.as_text(), Some("Hello Ada, your balance is 100."));
    assert!(compiled.as_chat().is_none());
}

#[test]
fn variables_can_be_maps_or_structs() {
    let prompt = text_prompt("{{city}} in {{month}}");

    let map = HashMap::from([("city", "Lima"), ("month", "May")]);
    assert_eq!(
        prompt.compile(&map).unwrap().into_text().unwrap(),
        "Lima in May"
    );

    let map = BTreeMap::from([("city".to_string(), json!("Oslo"))]);
    assert_eq!(
        prompt.compile(&map).unwrap().into_text().unwrap(),
        "Oslo in {{month}}"
    );

    #[derive(Serialize)]
    struct Vars<'a> {
        city: &'a str,
        month: Option<&'a str>,
    }
    let vars = Vars {
        city: "Rome",
        month: None,
    };
    assert_eq!(
        prompt.compile(&vars).unwrap().into_text().unwrap(),
        "Rome in "
    );
}

#[test]
fn unit_means_no_variables() {
    let prompt = text_prompt("Hello {{name}}");
    assert_eq!(
        prompt.compile(&()).unwrap().into_text().unwrap(),
        "Hello {{name}}"
    );
}

#[test]
fn non_object_variables_are_rejected() {
    let prompt = text_prompt("Hello {{name}}");
    for variables in [json!("Ada"), json!(["Ada"]), json!(3)] {
        match prompt.compile(&variables) {
            Err(LangfuseError::Serialization(err)) => {
                assert!(err.to_string().contains("JSON object"), "{err}");
            }
            other => panic!("expected Serialization error, got {other:?}"),
        }
    }
}

#[test]
fn text_prompt_ignores_placeholders() {
    let prompt = text_prompt("Hi {{name}}");
    let compiled = prompt
        .compile_with_placeholders(
            &json!({"name": "Ada"}),
            [("examples", vec![message("user", "x")])],
        )
        .unwrap();
    assert_eq!(compiled.into_text().unwrap(), "Hi Ada");
}

#[test]
fn text_prompt_variables_in_order_with_repeats() {
    let prompt = text_prompt("{{a}} + {{ b }} = {{a}}{{b");
    assert_eq!(prompt.variables(), ["a", "b", "a"]);
}

#[test]
fn accessors_expose_base_fields() {
    let prompt = text_prompt("x");
    assert_eq!(prompt.name(), "greeting");
    assert_eq!(prompt.version(), 4);
    assert_eq!(prompt.config(), &json!({"temperature": 0.2}));
    assert_eq!(prompt.base().labels, ["production"]);
    assert!(!prompt.is_fallback());
}

// ═══════════════════════════════════════════════════════════════════════
//  Chat prompts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn chat_compile_substitutes_and_keeps_placeholders() {
    let compiled = chat_prompt()
        .compile(&json!({"role": "helpful", "task": "coding"}))
        .unwrap();
    assert_eq!(
        contents(compiled),
        [
            Some("You are a helpful assistant".to_string()),
            None,
            Some("Help me with coding".to_string()),
        ]
    );
}

#[test]
fn chat_compile_without_variables_keeps_templates() {
    assert_eq!(
        contents(chat_prompt().compile(&()).unwrap()),
        [
            Some("You are a {{role}} assistant".to_string()),
            None,
            Some("Help me with {{task}}".to_string()),
        ]
    );
}

#[test]
fn placeholders_are_replaced_by_their_messages() {
    let examples = vec![
        message("user", "Example question"),
        message("assistant", "Example answer"),
    ];
    let compiled = chat_prompt()
        .compile_with_placeholders(&(), [("examples", &examples)])
        .unwrap();
    let messages = compiled.as_chat().unwrap();
    assert_eq!(messages.len(), 4);
    match &messages[2] {
        ChatPromptMessage::Message(m) => {
            assert_eq!(m.role, "assistant");
            assert_eq!(m.content, "Example answer");
        }
        other => panic!("expected message, got {other:?}"),
    }
    assert_eq!(
        contents(compiled),
        some(&[
            "You are a {{role}} assistant",
            "Example question",
            "Example answer",
            "Help me with {{task}}",
        ])
    );
}

#[test]
fn placeholder_messages_get_variables_too() {
    let examples = vec![
        message("user", "Show me {{task}}"),
        message("assistant", "Here's {{task}}"),
    ];
    let compiled = chat_prompt()
        .compile_with_placeholders(
            &json!({"role": "helpful", "task": "coding"}),
            [("examples", examples)],
        )
        .unwrap();
    assert_eq!(
        contents(compiled),
        some(&[
            "You are a helpful assistant",
            "Show me coding",
            "Here's coding",
            "Help me with coding",
        ])
    );
}

#[test]
fn unknown_placeholders_are_ignored() {
    let compiled = chat_prompt()
        .compile_with_placeholders(
            &json!({"role": "helpful", "task": "coding"}),
            [("unused", vec![message("user", "Won't appear")])],
        )
        .unwrap();
    assert_eq!(
        contents(compiled),
        [
            Some("You are a helpful assistant".to_string()),
            None,
            Some("Help me with coding".to_string()),
        ]
    );
}

#[test]
fn empty_placeholder_list_removes_the_placeholder() {
    let compiled = chat_prompt()
        .compile_with_placeholders(&(), [("examples", Vec::<ChatMessage>::new())])
        .unwrap();
    assert_eq!(
        contents(compiled),
        some(&["You are a {{role}} assistant", "Help me with {{task}}"])
    );
}

#[test]
fn placeholders_accept_a_map_by_reference() {
    let placeholders: HashMap<String, Vec<ChatMessage>> =
        HashMap::from([("examples".to_string(), vec![message("user", "Q")])]);
    let compiled = chat_prompt()
        .compile_with_placeholders(&(), &placeholders)
        .unwrap();
    assert_eq!(compiled.as_chat().unwrap().len(), 3);
}

#[test]
fn unresolved_placeholder_keeps_its_name() {
    let compiled = chat_prompt().compile(&()).unwrap();
    match &compiled.as_chat().unwrap()[1] {
        ChatPromptMessage::Placeholder(p) => assert_eq!(p.name, "examples"),
        other => panic!("expected placeholder, got {other:?}"),
    }
    // Serializes back to the API shape.
    assert_eq!(
        serde_json::to_value(&compiled.as_chat().unwrap()[1]).unwrap(),
        json!({"type": "placeholder", "name": "examples"})
    );
}

#[test]
fn chat_variables_skip_placeholders() {
    let prompt: Prompt = serde_json::from_value(json!({
        "type": "chat", "name": "c", "version": 1, "config": {}, "labels": [], "tags": [],
        "prompt": [
            {"role": "system", "content": "{{a}} and {{b}}"},
            {"type": "placeholder", "name": "history"},
            {"role": "user", "content": "{{a}}"}
        ]
    }))
    .unwrap();
    assert_eq!(prompt.variables(), ["a", "b", "a"]);
}

#[test]
fn compiled_values_are_inserted_unescaped() {
    let prompt = text_prompt("Data: {{payload}}");
    let payload: Value = json!({"user": {"id": 1, "tags": ["a", "b"]}});
    assert_eq!(
        prompt
            .compile(&json!({"payload": payload}))
            .unwrap()
            .into_text()
            .unwrap(),
        r#"Data: {"user":{"id":1,"tags":["a","b"]}}"#
    );
}

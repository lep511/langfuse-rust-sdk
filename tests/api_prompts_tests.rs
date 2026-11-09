mod common;

use common::*;
use langfuse_rust_sdk::api::prompt_version::UpdatePromptVersionRequest;
use langfuse_rust_sdk::api::prompts::{DeletePromptParams, GetPromptParams, ListPromptsParams};
use langfuse_rust_sdk::models::enums::PromptType;
use langfuse_rust_sdk::models::prompts::{ChatPromptMessage, CreatePromptRequest, Prompt};
use serde_json::{Value, json};
use wiremock::matchers::{body_json, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

fn text_prompt_json(name: &str) -> Value {
    json!({
        "type": "text",
        "name": name,
        "version": 1,
        "config": {},
        "labels": ["production"],
        "tags": [],
        "commitMessage": null,
        "prompt": "Hello {{name}}"
    })
}

fn chat_prompt_with_placeholder_json() -> Value {
    json!({
        "type": "chat",
        "name": "assistant",
        "version": 3,
        "config": null,
        "labels": [],
        "tags": [],
        "prompt": [
            {"role": "system", "content": "Be brief.", "type": "chatmessage"},
            {"type": "placeholder", "name": "history"},
            {"role": "user", "content": "{{question}}"}
        ]
    })
}

#[tokio::test]
async fn get_prompt_in_folder_encodes_slashes() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/prompts/folder%2Fsub%2Fname"))
        .respond_with(json(200, text_prompt_json("folder/sub/name")))
        .expect(1)
        .mount(&server)
        .await;

    let prompt = client.prompts.get("folder/sub/name").await.unwrap();
    match prompt {
        Prompt::Text { base, prompt } => {
            assert_eq!(base.name, "folder/sub/name");
            assert_eq!(prompt, "Hello {{name}}");
        }
        other => panic!("expected text prompt, got {other:?}"),
    }
}

#[tokio::test]
async fn get_prompt_with_resolve_false() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/prompts/assistant"))
        .and(query_param("resolve", "false"))
        .and(query_param("label", "staging"))
        .and(query_param_is_missing("version"))
        .respond_with(json(200, chat_prompt_with_placeholder_json()))
        .expect(1)
        .mount(&server)
        .await;

    let prompt = client
        .prompts
        .get_with_params(
            "assistant",
            &GetPromptParams {
                label: Some("staging".into()),
                resolve: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    match prompt {
        Prompt::Chat { prompt, .. } => {
            assert_eq!(prompt.len(), 3);
            assert!(matches!(&prompt[0], ChatPromptMessage::Message(m) if m.role == "system"));
            assert!(matches!(&prompt[1], ChatPromptMessage::Placeholder(p) if p.name == "history"));
            assert!(
                matches!(&prompt[2], ChatPromptMessage::Message(m) if m.content == "{{question}}")
            );
        }
        other => panic!("expected chat prompt, got {other:?}"),
    }
}

#[tokio::test]
async fn list_prompts_sends_filter_and_parses_meta() {
    let (client, server) = test_client().await;
    let filter = r#"[{"type":"string","column":"name","operator":"starts with","value":"tools/"}]"#;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/prompts"))
        .and(query_param("filter", filter))
        .and(query_param("limit", "5"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "name": "tools/search",
                    "type": "chat",
                    "versions": [1, 2, 3],
                    "labels": ["production", "latest"],
                    "tags": ["tools"],
                    "lastUpdatedAt": TS,
                    "lastConfig": {"temperature": 0.2}
                }],
                "meta": {"page": 1, "limit": 5, "totalItems": 1, "totalPages": 1}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let list = client
        .prompts
        .list(&ListPromptsParams {
            filter: Some(filter.into()),
            limit: Some(5),
            ..Default::default()
        })
        .await
        .unwrap();
    let meta = &list.data[0];
    assert_eq!(meta.name, "tools/search");
    assert_eq!(meta.r#type, PromptType::Chat);
    assert_eq!(meta.versions, vec![1, 2, 3]);
    assert_eq!(meta.last_config["temperature"], 0.2);
    assert_eq!(
        meta.last_updated_at.to_rfc3339(),
        "2024-06-15T10:30:00+00:00"
    );
}

#[tokio::test]
async fn create_chat_prompt_with_placeholder() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/v2/prompts"))
        .and(body_json(json!({
            "type": "chat",
            "name": "assistant",
            "prompt": [
                {"role": "system", "content": "Be brief."},
                {"type": "placeholder", "name": "history"}
            ]
        })))
        .respond_with(json(200, chat_prompt_with_placeholder_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .prompts
        .create(&CreatePromptRequest::Chat {
            name: "assistant".into(),
            prompt: vec![
                ChatPromptMessage::message("system", "Be brief."),
                ChatPromptMessage::placeholder("history"),
            ],
            config: None,
            labels: None,
            tags: None,
            commit_message: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_prompt_without_params_is_204() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/v2/prompts/old"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client.prompts.delete("old").await.unwrap();
    assert!(single_request(&server).await.url.query().is_none());
}

#[tokio::test]
async fn delete_prompt_with_label_and_version() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/v2/prompts/greeter"))
        .and(query_param("label", "staging"))
        .and(query_param("version", "4"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client
        .prompts
        .delete_with_params(
            "greeter",
            &DeletePromptParams {
                label: Some("staging".into()),
                version: Some(4),
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn update_prompt_version_sends_new_labels() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path("/api/public/v2/prompts/folder%2Fgreeter/versions/2"))
        .and(body_json(json!({"newLabels": ["production"]})))
        .respond_with(json(200, text_prompt_json("folder/greeter")))
        .expect(1)
        .mount(&server)
        .await;

    let prompt = client
        .prompt_versions
        .update(
            "folder/greeter",
            2,
            &UpdatePromptVersionRequest {
                new_labels: vec!["production".into()],
            },
        )
        .await
        .unwrap();
    assert!(matches!(prompt, Prompt::Text { .. }));
}

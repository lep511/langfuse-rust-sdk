//! `PromptsClient::get_cached`: caching, background refresh, fallback and invalidation.
mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use langfuse_rust_sdk::api::prompt_version::UpdatePromptVersionRequest;
use langfuse_rust_sdk::api::prompts::{GetPromptOptions, PromptFallback};
use langfuse_rust_sdk::models::prompts::{ChatPromptMessage, CreatePromptRequest, Prompt};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError, RetryPolicy};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PROMPT_PATH: &str = "/api/public/v2/prompts/greeting";

fn text_prompt_json(version: i64) -> Value {
    json!({
        "type": "text", "name": "greeting", "version": version, "config": {},
        "labels": ["production"], "tags": [], "prompt": format!("v{version}: Hello {{{{name}}}}")
    })
}

/// `GET` of the production prompt answering `version`, at most `times` times (`None`: always).
async fn mount_version(server: &MockServer, version: i64, times: Option<u64>) {
    let mock = Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .and(query_param("label", "production"))
        .respond_with(json(200, text_prompt_json(version)));
    match times {
        Some(n) => mock.up_to_n_times(n).mount(server).await,
        None => mock.mount(server).await,
    }
}

async fn get_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "GET")
        .count()
}

/// Poll `server` until it has received `count` GET requests (panics after 3 s).
async fn wait_for_gets(server: &MockServer, count: usize) {
    for _ in 0..150 {
        if get_requests(server).await >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "expected {count} GET requests, got {}",
        get_requests(server).await
    );
}

fn ttl(ms: u64) -> GetPromptOptions {
    GetPromptOptions::default().with_cache_ttl(Duration::from_millis(ms))
}

fn with_fallback(options: GetPromptOptions, text: &str) -> GetPromptOptions {
    options.with_fallback(PromptFallback::Text(text.into()))
}

// ═══════════════════════════════════════════════════════════════════════
//  Caching
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn fresh_prompt_is_served_from_cache() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, None).await;

    let options = GetPromptOptions::default();
    let first = client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    let second = client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();

    assert_eq!(first.version(), 1);
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(get_requests(&server).await, 1);
}

#[tokio::test]
async fn labels_and_versions_are_cached_separately() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .and(query_param("label", "staging"))
        .respond_with(json(200, text_prompt_json(7)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .and(query_param("version", "2"))
        .respond_with(json(200, text_prompt_json(2)))
        .expect(1)
        .mount(&server)
        .await;

    for _ in 0..2 {
        let staging = GetPromptOptions::default().with_label("staging");
        let version = GetPromptOptions::default().with_version(2);
        assert_eq!(
            client
                .prompts
                .get_cached("greeting", &staging)
                .await
                .unwrap()
                .version(),
            7
        );
        assert_eq!(
            client
                .prompts
                .get_cached("greeting", &version)
                .await
                .unwrap()
                .version(),
            2
        );
    }
}

#[tokio::test]
async fn zero_ttl_bypasses_the_cache() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, None).await;

    let options = GetPromptOptions::default().with_cache_ttl(Duration::ZERO);
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    assert_eq!(get_requests(&server).await, 2);
}

#[tokio::test]
async fn default_ttl_comes_from_the_config() {
    let server = MockServer::start().await;
    mount_version(&server, 1, None).await;
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .prompt_cache_ttl(Duration::ZERO)
        .build()
        .unwrap();
    assert_eq!(config.prompt_cache_ttl(), Duration::ZERO);
    let client = LangfuseClient::new(config).unwrap();

    let options = GetPromptOptions::default();
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    assert_eq!(get_requests(&server).await, 2);

    // A per-call TTL overrides the configured default.
    let cached = ttl(60_000);
    client
        .prompts
        .get_cached("greeting", &cached)
        .await
        .unwrap();
    client
        .prompts
        .get_cached("greeting", &cached)
        .await
        .unwrap();
    assert_eq!(get_requests(&server).await, 3);
}

#[tokio::test]
async fn cache_is_shared_by_clones() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, None).await;

    let options = GetPromptOptions::default();
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    let clone = client.clone();
    clone
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    assert_eq!(get_requests(&server).await, 1);
}

#[tokio::test]
async fn prompt_names_are_path_encoded() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/prompts/folder%2Fmy%20prompt"))
        .respond_with(json(200, text_prompt_json(1)))
        .expect(1)
        .mount(&server)
        .await;

    client
        .prompts
        .get_cached("folder/my prompt", &GetPromptOptions::default())
        .await
        .unwrap();
}

// ═══════════════════════════════════════════════════════════════════════
//  Background refresh
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn expired_prompt_is_served_while_it_refreshes() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, Some(1)).await;
    mount_version(&server, 2, None).await;

    let options = ttl(50);
    assert_eq!(
        client
            .prompts
            .get_cached("greeting", &options)
            .await
            .unwrap()
            .version(),
        1
    );
    tokio::time::sleep(Duration::from_millis(80)).await;

    // Stale: returned at once, refreshed in the background.
    assert_eq!(
        client
            .prompts
            .get_cached("greeting", &options)
            .await
            .unwrap()
            .version(),
        1
    );
    wait_for_gets(&server, 2).await;
    let mut version = 1;
    for _ in 0..100 {
        version = client
            .prompts
            .get_cached("greeting", &options)
            .await
            .unwrap()
            .version();
        if version == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(version, 2);
}

#[tokio::test]
async fn concurrent_stale_reads_trigger_a_single_refresh() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, Some(1)).await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(text_prompt_json(2))
                .set_delay(Duration::from_millis(200)),
        )
        .mount(&server)
        .await;

    let options = ttl(50);
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;

    let reads = (0..5).map(|_| client.prompts.get_cached("greeting", &options));
    for prompt in futures::future::join_all(reads).await {
        assert_eq!(prompt.unwrap().version(), 1);
    }
    wait_for_gets(&server, 2).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(get_requests(&server).await, 2);
}

#[tokio::test]
async fn failed_refresh_keeps_serving_the_stale_prompt() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, Some(1)).await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(json(500, json!({"message": "down"})))
        .mount(&server)
        .await;

    let options = ttl(50);
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;

    assert_eq!(
        client
            .prompts
            .get_cached("greeting", &options)
            .await
            .unwrap()
            .version(),
        1
    );
    wait_for_gets(&server, 2).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        client
            .prompts
            .get_cached("greeting", &options)
            .await
            .unwrap()
            .version(),
        1
    );
}

#[tokio::test]
async fn not_found_on_refresh_evicts_the_prompt() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, Some(1)).await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(json(404, json!({"message": "Prompt not found"})))
        .mount(&server)
        .await;

    let options = ttl(50);
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(
        client
            .prompts
            .get_cached("greeting", &options)
            .await
            .unwrap()
            .version(),
        1
    );
    wait_for_gets(&server, 2).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Evicted: the next call fetches again, gets 404 and uses the fallback.
    let fallback = with_fallback(ttl(50), "fallback text prompt");
    let prompt = client
        .prompts
        .get_cached("greeting", &fallback)
        .await
        .unwrap();
    assert!(prompt.is_fallback());
    assert_eq!(get_requests(&server).await, 3);
}

// ═══════════════════════════════════════════════════════════════════════
//  Fallback, retries and timeouts
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn fallback_is_returned_when_nothing_is_cached_and_the_fetch_fails() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(json(500, json!({"message": "down"})))
        .mount(&server)
        .await;

    let options = with_fallback(GetPromptOptions::default(), "Hi {{name}}");
    let prompt = client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    assert!(prompt.is_fallback());
    assert_eq!(prompt.name(), "greeting");
    assert_eq!(prompt.version(), 0);
    assert_eq!(prompt.base().labels, ["production"]);
    assert_eq!(prompt.config(), &json!({}));
    assert_eq!(
        prompt
            .compile(&json!({"name": "Ada"}))
            .unwrap()
            .into_text()
            .unwrap(),
        "Hi Ada"
    );

    // Fallbacks are not cached: the next call tries the server again.
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    assert_eq!(get_requests(&server).await, 2);
}

#[tokio::test]
async fn chat_fallback_for_a_pinned_version() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(json(503, json!({"message": "maintenance"})))
        .mount(&server)
        .await;

    let options = GetPromptOptions::default()
        .with_version(3)
        .with_fallback(PromptFallback::Chat(vec![
            ChatPromptMessage::message("system", "Be {{tone}}"),
            ChatPromptMessage::placeholder("history"),
        ]));
    let prompt = client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    assert!(prompt.is_fallback());
    assert_eq!(prompt.version(), 3);
    assert!(prompt.base().labels.is_empty());
    match prompt.as_ref() {
        Prompt::Chat { prompt, .. } => assert_eq!(prompt.len(), 2),
        other => panic!("expected chat fallback, got {other:?}"),
    }
}

#[tokio::test]
async fn error_is_returned_without_a_fallback() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(json(500, json!({"message": "down"})))
        .mount(&server)
        .await;

    let err = client
        .prompts
        .get_cached("greeting", &GetPromptOptions::default())
        .await
        .unwrap_err();
    assert!(
        matches!(err, LangfuseError::ServerError { status: 500, .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn max_retries_per_call_overrides_the_client_policy() {
    // `test_client` retries 0 times; this call allows one retry.
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(json(503, json!({"message": "busy"})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_version(&server, 1, None).await;

    let options = GetPromptOptions::default().with_max_retries(1);
    let prompt = client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    assert_eq!(prompt.version(), 1);
    assert_eq!(get_requests(&server).await, 2);
}

#[tokio::test]
async fn zero_retries_per_call_overrides_a_retrying_client() {
    let (client, server) = test_client_with_retries(3).await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(json(503, json!({"message": "busy"})))
        .mount(&server)
        .await;

    let options = GetPromptOptions::default().with_max_retries(0);
    assert!(
        client
            .prompts
            .get_cached("greeting", &options)
            .await
            .is_err()
    );
    assert_eq!(get_requests(&server).await, 1);
}

#[tokio::test]
async fn fetch_timeout_per_call() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(text_prompt_json(1))
                .set_delay(Duration::from_millis(500)),
        )
        .mount(&server)
        .await;

    let options = GetPromptOptions::default().with_fetch_timeout(Duration::from_millis(50));
    let err = client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, LangfuseError::Transport(e) if e.is_timeout()),
        "{err:?}"
    );

    let options = with_fallback(options, "offline");
    assert!(
        client
            .prompts
            .get_cached("greeting", &options)
            .await
            .unwrap()
            .is_fallback()
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Invalidation
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_update_and_delete_invalidate_the_prompt() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, None).await;
    Mock::given(method("POST"))
        .and(path("/api/public/v2/prompts"))
        .respond_with(json(201, text_prompt_json(2)))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/public/v2/prompts/greeting/versions/2"))
        .respond_with(json(200, text_prompt_json(2)))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(PROMPT_PATH))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let options = GetPromptOptions::default();
    let get = || client.prompts.get_cached("greeting", &options);

    get().await.unwrap();
    get().await.unwrap();
    assert_eq!(get_requests(&server).await, 1);

    client
        .prompts
        .create(&CreatePromptRequest::Text {
            name: "greeting".into(),
            prompt: "v2".into(),
            config: None,
            labels: None,
            tags: None,
            commit_message: None,
        })
        .await
        .unwrap();
    get().await.unwrap();
    assert_eq!(get_requests(&server).await, 2);

    client
        .prompt_versions
        .update(
            "greeting",
            2,
            &UpdatePromptVersionRequest {
                new_labels: vec!["production".into()],
            },
        )
        .await
        .unwrap();
    get().await.unwrap();
    assert_eq!(get_requests(&server).await, 3);

    client.prompts.delete("greeting").await.unwrap();
    get().await.unwrap();
    assert_eq!(get_requests(&server).await, 4);
}

#[tokio::test]
async fn clear_cache_forces_a_fetch() {
    let (client, server) = test_client().await;
    mount_version(&server, 1, None).await;

    let options = GetPromptOptions::default();
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    client.prompts.clear_cache();
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    assert_eq!(get_requests(&server).await, 2);
}

#[tokio::test]
async fn a_fetch_overtaken_by_an_invalidation_is_not_cached() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(PROMPT_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(text_prompt_json(1))
                .set_delay(Duration::from_millis(300)),
        )
        .mount(&server)
        .await;

    let task_client = client.clone();
    let slow = tokio::spawn(async move {
        task_client
            .prompts
            .get_cached("greeting", &GetPromptOptions::default())
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    client.prompts.clear_cache();
    // The caller still gets the prompt it fetched...
    assert_eq!(slow.await.unwrap().unwrap().version(), 1);

    // ...but it was not cached, since it may predate the invalidation.
    client
        .prompts
        .get_cached("greeting", &GetPromptOptions::default())
        .await
        .unwrap();
    assert_eq!(get_requests(&server).await, 2);
}

#[tokio::test]
async fn invalidation_keeps_other_prompts() {
    let server = MockServer::start().await;
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .retry_policy(RetryPolicy {
            max_retries: 0,
            ..Default::default()
        })
        .build()
        .unwrap();
    let client = LangfuseClient::new(config).unwrap();
    mount_version(&server, 1, None).await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/prompts/greeting-extra"))
        .respond_with(json(200, text_prompt_json(9)))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(PROMPT_PATH))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let options = GetPromptOptions::default();
    client
        .prompts
        .get_cached("greeting", &options)
        .await
        .unwrap();
    client
        .prompts
        .get_cached("greeting-extra", &options)
        .await
        .unwrap();
    client.prompts.delete("greeting").await.unwrap();

    client
        .prompts
        .get_cached("greeting-extra", &options)
        .await
        .unwrap();
    assert_eq!(
        get_requests(&server).await,
        2,
        "greeting-extra stays cached"
    );
}

//! Langfuse AI Gateway client against a wiremock gateway: routes, headers,
//! streams, errors and retries; with `otel`, the trace context of the
//! current observation; with `openai`, the async-openai configuration.

mod common;

use std::time::Duration;

use common::*;
use futures::StreamExt;
use langfuse_rust_sdk::LangfuseError;
use langfuse_rust_sdk::RetryPolicy;
use langfuse_rust_sdk::gateway::{GatewayClient, GatewayHeaders, ServerSentEvent};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const KEY: &str = "lf-gw-test";

/// A gateway client for a fresh mock server, with `retries` fast retries.
async fn gateway(retries: u32) -> (GatewayClient, MockServer) {
    let server = MockServer::start().await;
    let client = GatewayClient::builder(server.uri(), KEY)
        .retry_policy(RetryPolicy {
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            max_retries: retries,
            jitter_fraction: 0.0,
        })
        .build()
        .unwrap();
    (client, server)
}

/// Response headers the gateway sets on calls that record a generation.
fn recorded(template: ResponseTemplate) -> ResponseTemplate {
    template
        .insert_header("langfuse-request-id", "req-1")
        .insert_header("langfuse-trace-id", "0af7651916cd43dd8448eb211c80319c")
        .insert_header("langfuse-observation-id", "b7ad6b7169203331")
        .insert_header("langfuse-organization-id", "org-1")
        .insert_header("langfuse-project-id", "project-1")
        .insert_header("langfuse-provider", "anthropic")
        .insert_header("langfuse-provider-connection-id", "conn-1")
}

async fn mock(server: &MockServer, verb: &str, route: &str, response: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(response)
        .mount(server)
        .await;
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers.get(name).map(|v| v.to_str().unwrap())
}

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_owned(), "text/event-stream")
}

#[tokio::test]
async fn messages_sends_the_key_and_reads_the_gateway_headers() {
    let (gateway, server) = gateway(0).await;
    let reply = json!({"type": "message", "content": [{"type": "text", "text": "pong"}]});
    mock(
        &server,
        "POST",
        "/anthropic/v1/messages",
        recorded(json(200, reply.clone())),
    )
    .await;

    let body = json!({"model": "anthropic-prod:claude", "max_tokens": 8, "messages": []});
    let response = gateway.messages::<Value, _>(&body).await.unwrap();
    assert_eq!(response.body, reply);
    let info = &response.info;
    assert_eq!(info.request_id.as_deref(), Some("req-1"));
    assert_eq!(
        info.trace_id.as_deref(),
        Some("0af7651916cd43dd8448eb211c80319c")
    );
    assert_eq!(info.observation_id.as_deref(), Some("b7ad6b7169203331"));
    assert_eq!(info.organization_id.as_deref(), Some("org-1"));
    assert_eq!(info.project_id.as_deref(), Some("project-1"));
    assert_eq!(info.provider.as_deref(), Some("anthropic"));
    assert_eq!(info.provider_connection_id.as_deref(), Some("conn-1"));

    let request = single_request(&server).await;
    assert_eq!(header(&request, "authorization"), Some("Bearer lf-gw-test"));
    assert_eq!(header(&request, "anthropic-version"), Some("2023-06-01"));
    assert_eq!(header(&request, "content-type"), Some("application/json"));
    assert!(
        header(&request, "user-agent")
            .unwrap()
            .starts_with("langfuse-rust-sdk/")
    );
    assert_eq!(body_json(&request), body);
    // Outside an observation there is no trace context.
    assert_eq!(header(&request, "traceparent"), None);
}

#[tokio::test]
async fn every_route_has_its_path() {
    let (gateway, server) = gateway(0).await;
    for (verb, route) in [
        ("GET", "/openai/v1/models"),
        ("GET", "/anthropic/v1/models"),
        ("POST", "/openai/v1/responses"),
        ("POST", "/openai/v1/responses/compact"),
        ("POST", "/openai/v1/chat/completions"),
        ("POST", "/anthropic/v1/messages/count_tokens"),
    ] {
        mock(&server, verb, route, json(200, json!({"route": route}))).await;
    }
    let body = json!({"model": "m"});
    let routes: Vec<Value> = vec![
        gateway.openai_models().await.unwrap().body,
        gateway.anthropic_models().await.unwrap().body,
        gateway.responses(&body).await.unwrap().body,
        gateway.responses_compact(&body).await.unwrap().body,
        gateway.chat_completions(&body).await.unwrap().body,
        gateway.count_tokens(&body).await.unwrap().body,
    ];
    assert_eq!(routes[0]["route"], "/openai/v1/models");
    assert_eq!(routes[5]["route"], "/anthropic/v1/messages/count_tokens");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 6);
    assert_eq!(requests[1].url.query(), Some("limit=1000"));
    // Only the Anthropic routes get `anthropic-version`.
    assert_eq!(header(&requests[0], "anthropic-version"), None);
    assert_eq!(header(&requests[4], "anthropic-version"), None);
    assert_eq!(
        header(&requests[1], "anthropic-version"),
        Some("2023-06-01")
    );
    assert_eq!(
        header(&requests[5], "anthropic-version"),
        Some("2023-06-01")
    );
}

#[tokio::test]
async fn request_headers_are_sent_and_extra_headers_win() {
    let server = MockServer::start().await;
    let gateway = GatewayClient::builder(format!("{}/", server.uri()), KEY)
        .headers(GatewayHeaders::default().with_tag("service-a"))
        .header("anthropic-beta", "beta-1")
        .build()
        .unwrap();
    mock(
        &server,
        "POST",
        "/anthropic/v1/messages",
        json(200, json!({})),
    )
    .await;

    gateway
        .with_headers(
            GatewayHeaders::default()
                .with_user_id("svc@example.com")
                .with_session_id("chat 7")
                .with_environment("ci")
                .with_tags(["a,b"])
                .with_metadata("repo", "docs:main")
                .with_trace_name("nightly"),
        )
        .with_header("anthropic-version", "2024-01-01")
        .unwrap()
        .messages::<Value, _>(&json!({}))
        .await
        .unwrap();

    let request = single_request(&server).await;
    assert_eq!(
        header(&request, "langfuse-user-id"),
        Some("svc@example.com")
    );
    assert_eq!(header(&request, "langfuse-session-id"), Some("chat%207"));
    assert_eq!(header(&request, "langfuse-environment"), Some("ci"));
    assert_eq!(header(&request, "langfuse-tags"), Some("service-a,a%2Cb"));
    assert_eq!(
        header(&request, "langfuse-metadata"),
        Some("repo:docs:main")
    );
    assert_eq!(header(&request, "langfuse-trace-name"), Some("nightly"));
    assert_eq!(header(&request, "anthropic-beta"), Some("beta-1"));
    assert_eq!(header(&request, "anthropic-version"), Some("2024-01-01"));
    assert!(gateway.with_header("bad name", "v").is_err());
}

#[tokio::test]
async fn streams_yield_server_sent_events() {
    let (gateway, server) = gateway(0).await;
    let body = "event: message_start\ndata: {\"type\":\"message_start\"}\n\n\
                : ping\n\n\
                event: content_block_delta\ndata: {\"delta\":{\"text\":\"po\"}}\n\n\
                event: content_block_delta\r\ndata: {\"delta\":{\"text\":\"ng\"}}\r\n\r\n\
                event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    mock(
        &server,
        "POST",
        "/anthropic/v1/messages",
        recorded(sse(body)),
    )
    .await;

    let mut stream = gateway
        .messages_stream(&json!({"model": "m", "max_tokens": 8, "messages": []}))
        .await
        .unwrap();
    assert_eq!(stream.info().request_id.as_deref(), Some("req-1"));
    let mut text = String::new();
    let mut names = Vec::new();
    while let Some(event) = stream.next().await {
        let event: ServerSentEvent = event.unwrap();
        names.push(event.event.clone().unwrap());
        if event.event.as_deref() == Some("content_block_delta") {
            let delta: Value = event.json().unwrap();
            text.push_str(delta["delta"]["text"].as_str().unwrap());
        }
    }
    assert_eq!(text, "pong");
    assert_eq!(
        names,
        [
            "message_start",
            "content_block_delta",
            "content_block_delta",
            "message_stop"
        ]
    );

    let request = single_request(&server).await;
    assert_eq!(body_json(&request)["stream"], true);
    assert_eq!(header(&request, "accept"), Some("text/event-stream"));
}

#[tokio::test]
async fn chat_completion_streams_end_with_done() {
    let (gateway, server) = gateway(0).await;
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n\
                data: {\"choices\":[],\"usage\":{\"total_tokens\":3}}\n\n\
                data: [DONE]\n\n";
    mock(&server, "POST", "/openai/v1/chat/completions", sse(body)).await;

    let events: Vec<ServerSentEvent> = gateway
        .chat_completions_stream(&json!({"model": "m", "messages": []}))
        .await
        .unwrap()
        .map(Result::unwrap)
        .collect()
        .await;
    assert_eq!(events.len(), 3);
    let usage: Value = events[1].json().unwrap();
    assert_eq!(usage["choices"], json!([]));
    assert!(events[2].is_done());
    assert!(!events[1].is_done());

    mock(
        &server,
        "POST",
        "/openai/v1/responses",
        sse("event: response.completed\ndata: {}\n\n"),
    )
    .await;
    let mut responses = gateway
        .responses_stream(&json!({"model": "m", "input": "x"}))
        .await
        .unwrap();
    let event = responses.next().await.unwrap().unwrap();
    assert_eq!(event.event.as_deref(), Some("response.completed"));
    assert!(responses.next().await.is_none());
}

#[tokio::test]
async fn errors_keep_the_provider_message_and_post_failures_are_not_retried() {
    let (gateway, server) = gateway(2).await;
    let anthropic_error = json!({
        "type": "error",
        "error": {"type": "authentication_error", "message": "invalid gateway key"},
        "request_id": "req-9"
    });
    mock(
        &server,
        "POST",
        "/anthropic/v1/messages",
        json(401, anthropic_error.clone()),
    )
    .await;
    mock(
        &server,
        "POST",
        "/openai/v1/chat/completions",
        json(502, json!({"error": {"message": "provider failed"}})),
    )
    .await;

    let error = gateway.messages::<Value, _>(&json!({})).await.unwrap_err();
    assert!(
        matches!(error, LangfuseError::Unauthorized { .. }),
        "{error}"
    );
    assert_eq!(error.message(), Some("invalid gateway key"));
    assert_eq!(error.body(), Some(&anthropic_error));

    // 502 after the provider was called: retrying could record and bill it twice.
    let error = gateway
        .chat_completions::<Value, _>(&json!({}))
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(502));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn rate_limits_are_retried_and_models_on_server_errors() {
    let (gateway, server) = gateway(2).await;
    Mock::given(method("POST"))
        .and(path("/anthropic/v1/messages"))
        .respond_with(json(429, json!({"error": {"message": "slow down"}})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mock(
        &server,
        "POST",
        "/anthropic/v1/messages",
        json(200, json!({"ok": true})),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/openai/v1/models"))
        .respond_with(json(500, json!({})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mock(
        &server,
        "GET",
        "/openai/v1/models",
        json(200, json!({"data": []})),
    )
    .await;

    let reply = gateway.messages::<Value, _>(&json!({})).await.unwrap();
    assert_eq!(reply.body["ok"], true);
    let models = gateway.openai_models::<Value>().await.unwrap();
    assert_eq!(models.body["data"], json!([]));
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
}

#[cfg(feature = "otel")]
mod trace_context {
    use super::*;
    use langfuse_rust_sdk::otel::{
        ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes,
    };

    #[tokio::test]
    async fn calls_inside_an_observation_join_its_trace() {
        let (client, _langfuse) = test_client().await;
        let tracer = client
            .tracer(
                TracingOptions::default()
                    .with_flush_interval(Duration::from_secs(3600))
                    .with_environment("staging"),
            )
            .unwrap();
        let (gateway, server) = gateway(0).await;
        mock(
            &server,
            "POST",
            "/anthropic/v1/messages",
            json(200, json!({})),
        )
        .await;

        let span = tracer.start_observation("answer", ObservationType::Span);
        let attributes = PropagatedAttributes::default()
            .with_user_id("user-1")
            .with_session_id("session-1")
            .with_tag("beta")
            .with_metadata("repo", "docs")
            .with_metadata("attempt", &2);
        let call = async {
            gateway.messages::<Value, _>(&json!({})).await.unwrap();
            // Explicit headers win over the propagated ones.
            gateway
                .with_headers(GatewayHeaders::default().with_session_id("explicit"))
                .messages::<Value, _>(&json!({}))
                .await
                .unwrap();
            assert_eq!(
                gateway.gateway_headers().traceparent,
                Some(format!("00-{}-{}-01", span.trace_id(), span.id()))
            );
            gateway
                .with_trace_context_propagation(false)
                .messages::<Value, _>(&json!({}))
                .await
                .unwrap();
        };
        span.in_scope(propagate_attributes(attributes, call)).await;
        span.end();

        let requests = server.received_requests().await.unwrap();
        let first = &requests[0];
        assert_eq!(
            header(first, "traceparent"),
            Some(format!("00-{}-{}-01", span.trace_id(), span.id()).as_str())
        );
        assert_eq!(header(first, "langfuse-user-id"), Some("user-1"));
        assert_eq!(header(first, "langfuse-session-id"), Some("session-1"));
        assert_eq!(header(first, "langfuse-environment"), Some("staging"));
        assert_eq!(header(first, "langfuse-tags"), Some("beta"));
        assert_eq!(
            header(first, "langfuse-metadata"),
            Some("attempt:2,repo:docs")
        );
        assert_eq!(
            header(&requests[1], "langfuse-session-id"),
            Some("explicit")
        );
        assert!(header(&requests[1], "traceparent").is_some());
        assert_eq!(header(&requests[2], "traceparent"), None);
        assert_eq!(header(&requests[2], "langfuse-session-id"), None);
        tracer.shutdown().await;
    }

    #[tokio::test]
    async fn the_propagated_environment_wins_over_the_tracer_default() {
        let (client, _langfuse) = test_client().await;
        let tracer = client
            .tracer(
                TracingOptions::default()
                    .with_flush_interval(Duration::from_secs(3600))
                    .with_environment("staging"),
            )
            .unwrap();
        let span = tracer.start_observation("root", ObservationType::Agent);
        let headers = span
            .in_scope(propagate_attributes(
                PropagatedAttributes::default().with_environment("canary"),
                async { GatewayHeaders::current() },
            ))
            .await;
        assert_eq!(headers.environment.as_deref(), Some("canary"));
        assert_eq!(GatewayHeaders::current(), GatewayHeaders::default());
        let in_child = span
            .in_scope(async {
                let child = tracer.start_observation("child", ObservationType::Tool);
                child.in_scope(async { GatewayHeaders::current() }).await
            })
            .await;
        assert_eq!(in_child.environment.as_deref(), Some("staging"));
        assert_ne!(
            in_child.traceparent,
            Some(format!("00-{}-{}-01", span.trace_id(), span.id()))
        );
        assert!(in_child.traceparent.unwrap().contains(span.trace_id()));
        tracer.shutdown().await;
    }
}

#[cfg(feature = "openai")]
mod openai_config {
    use super::*;
    use async_openai::types::chat::CreateChatCompletionRequest;
    use langfuse_rust_sdk::otel::{ObservationType, TracingOptions};

    #[tokio::test]
    async fn async_openai_requests_carry_the_key_and_the_trace_context() {
        let (client, _langfuse) = test_client().await;
        let tracer = client
            .tracer(TracingOptions::default().with_flush_interval(Duration::from_secs(3600)))
            .unwrap();
        let (gateway, server) = gateway(0).await;
        let completion = json!({
            "id": "chatcmpl-1", "object": "chat.completion", "created": 1, "model": "m",
            "choices": [{"index": 0, "finish_reason": "stop",
                         "message": {"role": "assistant", "content": "hi"}}],
        });
        mock(
            &server,
            "POST",
            "/openai/v1/chat/completions",
            json(200, completion),
        )
        .await;

        let config = gateway
            .with_headers(GatewayHeaders::default().with_tag("openai-sdk"))
            .openai_config();
        let openai = async_openai::Client::with_config(config);
        let request: CreateChatCompletionRequest = serde_json::from_value(json!({
            "model": "openai-prod:gpt-4.1-mini",
            "messages": [{"role": "user", "content": "Say hi"}],
        }))
        .unwrap();
        let span = tracer.start_observation("chat", ObservationType::Span);
        let response = span.in_scope(openai.chat().create(request)).await.unwrap();
        span.end();
        assert_eq!(response.choices[0].message.content.as_deref(), Some("hi"));

        let request = single_request(&server).await;
        assert_eq!(header(&request, "authorization"), Some("Bearer lf-gw-test"));
        assert_eq!(header(&request, "langfuse-tags"), Some("openai-sdk"));
        assert_eq!(
            header(&request, "traceparent"),
            Some(format!("00-{}-{}-01", span.trace_id(), span.id()).as_str())
        );
        assert_eq!(header(&request, "openai-organization"), None);
        tracer.shutdown().await;
    }
}

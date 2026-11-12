//! `langfuse api …` against a mock server: the request sent and the output.

mod common;

use common::{api, api_with, body_json, json, query_pairs, single_request};
use langfuse_rust_cli::Exit;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[tokio::test]
async fn list_sends_the_sdk_query_and_prints_pretty_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/prompts"))
        .and(header(
            "authorization",
            "Basic cGstbGYtdGVzdDpzay1sZi10ZXN0",
        ))
        .and(header("x-langfuse-sdk-name", "rust"))
        .respond_with(json(
            200,
            json!({"data": [{"name": "a"}], "meta": {"page": 1, "totalPages": 1}}),
        ))
        .mount(&server)
        .await;
    let output = api(
        &server,
        &[
            "api",
            "prompts",
            "list",
            "--limit",
            "5",
            "--from-updated-at",
            "2026-11-01T00:00:00Z",
            "--label",
            "production",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(
        output.stdout,
        "{\n  \"data\": [\n    {\n      \"name\": \"a\"\n    }\n  ],\n  \"meta\": {\n    \"page\": 1,\n    \"totalPages\": 1\n  }\n}\n"
    );
    let request = single_request(&server).await;
    assert_eq!(
        query_pairs(&request),
        pairs(&[
            ("label", "production"),
            ("limit", "5"),
            ("fromUpdatedAt", "2026-11-01T00:00:00Z")
        ])
    );
    assert!(
        request
            .headers
            .get("user-agent")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("langfuse-rust-cli/")
    );
}

#[tokio::test]
async fn json_envelope_has_status_and_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            json(200, json!({"status": "OK", "version": "4.78.0"}))
                .insert_header("x-request-id", "r1"),
        )
        .mount(&server)
        .await;
    let output = api(&server, &["--json", "api", "health", "get"]).await;
    let envelope = output.json();
    assert_eq!(envelope["status"], 200);
    assert_eq!(envelope["headers"]["x-request-id"], "r1");
    assert_eq!(envelope["body"]["version"], "4.78.0");
    assert!(output.stdout.ends_with("}\n") && output.stdout.lines().count() == 1);
}

#[tokio::test]
async fn path_parameters_are_encoded_and_booleans_are_tri_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    api(
        &server,
        &[
            "api",
            "prompts",
            "get",
            "folder/my prompt",
            "--no-resolve",
            "--prompt-version",
            "3",
        ],
    )
    .await;
    let request = single_request(&server).await;
    assert_eq!(
        request.url.path(),
        "/api/public/v2/prompts/folder%2Fmy%20prompt"
    );
    assert_eq!(
        query_pairs(&request),
        pairs(&[("version", "3"), ("resolve", "false")])
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    api(&server, &["api", "prompts", "get", "p", "--resolve"]).await;
    assert_eq!(
        query_pairs(&single_request(&server).await),
        pairs(&[("resolve", "true")])
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    api(&server, &["api", "prompts", "get", "p", "--resolve=false"]).await;
    assert_eq!(
        query_pairs(&single_request(&server).await),
        pairs(&[("resolve", "false")])
    );
}

#[tokio::test]
async fn repeated_query_parameters_are_kept() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(200, json!({"data": [], "meta": {}})))
        .mount(&server)
        .await;
    let output = api(
        &server,
        &[
            "api",
            "observations-v2",
            "list",
            "--environment",
            "prod",
            "--environment",
            "dev",
            "--type",
            "GENERATION",
            "--level",
            "ERROR",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let request = single_request(&server).await;
    assert_eq!(request.url.path(), "/api/public/v2/observations");
    assert_eq!(
        query_pairs(&request),
        pairs(&[
            ("type", "GENERATION"),
            ("level", "ERROR"),
            ("environment", "prod"),
            ("environment", "dev")
        ])
    );
}

#[tokio::test]
async fn union_bodies_from_field_flags() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(json(200, json!({"ok": true})))
        .mount(&server)
        .await;
    let output = api(
        &server,
        &[
            "api",
            "prompts",
            "create",
            "--type",
            "text",
            "--name",
            "greeting",
            "--prompt",
            "Hello {{name}}",
            "--labels",
            "production",
            "--tags",
            r#"["a","b"]"#,
            "--config",
            r#"{"temperature":0}"#,
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(
        body_json(&single_request(&server).await),
        json!({"type": "text", "name": "greeting", "prompt": "Hello {{name}}", "config": {"temperature": 0}, "labels": ["production"], "tags": ["a", "b"]})
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    api(
        &server,
        &[
            "api",
            "prompts",
            "create",
            "--type",
            "chat",
            "--name",
            "support",
            "--prompt",
            r#"{"role":"system","content":"be nice"}"#,
            "--prompt",
            r#"[{"role":"user","content":"{{q}}"}]"#,
        ],
    )
    .await;
    assert_eq!(
        body_json(&single_request(&server).await),
        json!({"type": "chat", "name": "support", "prompt": [{"role": "system", "content": "be nice"}, {"role": "user", "content": "{{q}}"}]})
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    api(
        &server,
        &[
            "api",
            "evaluators",
            "create",
            "--type",
            "code",
            "--name",
            "len",
            "--source-code",
            "return 1",
            "--source-code-language",
            "PYTHON",
        ],
    )
    .await;
    assert_eq!(
        body_json(&single_request(&server).await),
        json!({"name": "len", "type": "code", "sourceCode": "return 1", "sourceCodeLanguage": "PYTHON"})
    );
}

#[tokio::test]
async fn union_usage_errors() {
    let server = MockServer::start().await;
    for args in [
        vec!["api", "prompts", "create", "--name", "x", "--prompt", "y"],
        vec![
            "api", "prompts", "create", "--type", "text", "--name", "x", "--prompt", "a",
            "--prompt", "b",
        ],
        vec![
            "api", "prompts", "create", "--type", "chat", "--name", "x", "--prompt", "not json",
        ],
        vec![
            "api",
            "prompts",
            "create",
            "--type",
            "chat",
            "--name",
            "x",
            "--prompt",
            r#"{"no":"role"}"#,
        ],
        vec![
            "api",
            "prompts",
            "create",
            "--name",
            "x",
            "--body-json",
            "{}",
        ],
        vec![
            "api",
            "evaluators",
            "create",
            "--type",
            "code",
            "--name",
            "x",
        ],
        vec!["api", "scores", "create"],
    ] {
        let output = api(&server, &args).await;
        assert_eq!(output.exit, Exit::Usage, "{args:?}: {output:?}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn json_body_channels_are_sent_unchanged() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(json(207, json!({"accepted": 1, "rejected": 1})))
        .mount(&server)
        .await;
    let body = r#"[{"name":"accuracy","value":0.9,"traceId":"t1"},{"name":"x","value":"y"}]"#;
    let output = api(&server, &["api", "scores", "create", "--json-body", body]).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let request = single_request(&server).await;
    assert_eq!(request.url.path(), "/api/public/scores");
    assert_eq!(
        body_json(&request),
        serde_json::from_str::<serde_json::Value>(body).unwrap()
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    let output = api_with(
        &server,
        br#"{"name":"qa","description":"pairs"}"#,
        &["api", "datasets", "create", "--body-file", "-"],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(
        body_json(&single_request(&server).await),
        json!({"name": "qa", "description": "pairs"})
    );

    let output = api_with(
        &server,
        b"{",
        &["api", "datasets", "create", "--body-file", "-"],
    )
    .await;
    assert_eq!(output.exit, Exit::Local);
    let output = api(
        &server,
        &[
            "api",
            "datasets",
            "create",
            "--body-file",
            "/nonexistent/body.json",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Local);
    assert!(
        output
            .stderr
            .starts_with("Cannot read body file /nonexistent/body.json")
    );
}

#[tokio::test]
async fn field_flags_use_the_wire_names() {
    let cases: Vec<(Vec<&str>, &str, &str, serde_json::Value)> = vec![
        (
            vec![
                "api",
                "llm-connections",
                "upsert",
                "--provider",
                "gw",
                "--adapter",
                "openai",
                "--provider-secret-key",
                "sk",
                "--base-url",
                "https://gw",
                "--custom-models",
                "m1",
                "--no-with-default-models",
            ],
            "PUT",
            "/api/public/llm-connections",
            json!({"provider": "gw", "adapter": "openai", "secretKey": "sk", "baseURL": "https://gw", "customModels": ["m1"], "withDefaultModels": false}),
        ),
        (
            vec![
                "api",
                "projects",
                "create-api-key",
                "p1",
                "--predefined-public-key",
                "pk-lf-x",
                "--predefined-secret-key",
                "sk-lf-x",
                "--name",
                "ci",
            ],
            "POST",
            "/api/public/projects/p1/apiKeys",
            json!({"name": "ci", "publicKey": "pk-lf-x", "secretKey": "sk-lf-x"}),
        ),
        (
            vec![
                "api",
                "annotation-queues",
                "create-item",
                "q1",
                "--object-id",
                "t1",
                "--object-type",
                "TRACE",
            ],
            "POST",
            "/api/public/annotation-queues/q1/items",
            json!({"objectId": "t1", "objectType": "TRACE"}),
        ),
        (
            vec![
                "api",
                "comments",
                "create",
                "--project-id",
                "p",
                "--object-type",
                "trace",
                "--object-id",
                "t",
                "--content",
                "hi",
            ],
            "POST",
            "/api/public/comments",
            json!({"projectId": "p", "objectType": "trace", "objectId": "t", "content": "hi"}),
        ),
        (
            vec![
                "api",
                "trace",
                "delete-many",
                "--trace-ids",
                "a",
                "--trace-id",
                "b",
            ],
            "DELETE",
            "/api/public/traces",
            json!({"traceIds": ["a", "b"]}),
        ),
        (
            vec![
                "api",
                "prompt-version",
                "update-version",
                "folder/p",
                "2",
                "--new-labels",
                "production",
            ],
            "PATCH",
            "/api/public/v2/prompts/folder%2Fp/versions/2",
            json!({"newLabels": ["production"]}),
        ),
        (
            vec![
                "api",
                "score-configs",
                "create",
                "--name",
                "q",
                "--data-type",
                "CATEGORICAL",
                "--categories",
                r#"[{"label":"good","value":1}]"#,
            ],
            "POST",
            "/api/public/score-configs",
            json!({"name": "q", "dataType": "CATEGORICAL", "categories": [{"value": 1.0, "label": "good"}]}),
        ),
        (
            vec![
                "api",
                "organizations",
                "update-membership",
                "--user-id",
                "u",
                "--role",
                "VIEWER",
            ],
            "PUT",
            "/api/public/organizations/memberships",
            json!({"userId": "u", "role": "VIEWER"}),
        ),
        (
            vec![
                "api",
                "skills",
                "create",
                "--files",
                r#"{"path":"SKILL.md","content":"x"}"#,
                "--commit-message",
                "m",
            ],
            "POST",
            "/api/public/skills",
            json!({"files": [{"path": "SKILL.md", "content": "x"}], "commitMessage": "m"}),
        ),
        (
            vec![
                "api",
                "evaluation-rules",
                "create",
                "--name",
                "r",
                "--enabled",
                "--evaluator-assignments",
                r#"{"evaluatorId":"e"}"#,
                "--sampling",
                "0.5",
            ],
            "POST",
            "/api/public/v2/evaluation-rules",
            json!({"name": "r", "enabled": true, "sampling": 0.5, "evaluatorAssignments": [{"evaluatorId": "e"}]}),
        ),
    ];
    for (args, expected_method, expected_path, expected_body) in cases {
        let server = MockServer::start().await;
        Mock::given(method(expected_method))
            .respond_with(json(200, json!({})))
            .mount(&server)
            .await;
        let output = api(&server, &args).await;
        assert_eq!(output.exit, Exit::Success, "{args:?}: {output:?}");
        let request = single_request(&server).await;
        assert_eq!(request.url.path(), expected_path, "{args:?}");
        assert_eq!(body_json(&request), expected_body, "{args:?}");
    }
}

#[tokio::test]
async fn no_content_responses_print_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/v2/prompts/p"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let output = api(
        &server,
        &["api", "prompts", "delete", "p", "--label", "staging"],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(output.stdout, "");
    assert_eq!(
        query_pairs(&single_request(&server).await),
        pairs(&[("label", "staging")])
    );
}

#[tokio::test]
async fn http_errors_print_the_body_with_exit_5() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(
            404,
            json!({"message": "Prompt not found", "error": "LangfuseNotFoundError"}),
        ))
        .mount(&server)
        .await;
    let output = api(&server, &["api", "prompts", "get", "missing"]).await;
    assert_eq!(output.exit, Exit::Http);
    assert_eq!(output.json()["message"], "Prompt not found");

    let output = api(&server, &["--json", "api", "prompts", "get", "missing"]).await;
    assert_eq!(output.exit, Exit::Http);
    assert_eq!(output.json()["status"], 404);
    assert_eq!(output.json()["body"]["error"], "LangfuseNotFoundError");
}

#[tokio::test]
async fn invalid_2xx_responses_are_exit_7() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>"))
        .mount(&server)
        .await;
    let output = api(&server, &["api", "health", "get"]).await;
    assert_eq!(output.exit, Exit::Response, "{output:?}");
}

#[tokio::test]
async fn unreachable_hosts_are_exit_4() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let host = format!("http://127.0.0.1:{port}");
    let output = common::run(&[
        "--host",
        &host,
        "--public-key",
        "pk",
        "--secret-key",
        "sk",
        "--max-retries",
        "0",
        "api",
        "prompts",
        "list",
    ])
    .await;
    assert_eq!(output.exit, Exit::Network, "{output:?}");
    assert!(
        output
            .stderr
            .starts_with(&format!("GET {host}/api/public/v2/prompts failed: ")),
        "{}",
        output.stderr
    );
}

#[tokio::test]
async fn rate_limits_are_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(json(200, json!({"data": []})))
        .mount(&server)
        .await;
    let mut args = common::connection(&server);
    args.retain(|arg| arg != "--max-retries" && arg != "0");
    let mut all: Vec<&str> = args.iter().map(String::as_str).collect();
    all.extend(["--max-retries", "1", "api", "models", "list"]);
    let output = common::run(&all).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn otel_export_sends_the_ingestion_version_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/public/otel/v1/traces"))
        .and(header("x-langfuse-ingestion-version", "4"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    let output = api(
        &server,
        &[
            "api",
            "opentelemetry",
            "export-traces",
            "--body-json",
            r#"{"resourceSpans":[]}"#,
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
}

#[tokio::test]
async fn aliases_reach_the_same_operations() {
    for (args, expected_path) in [
        (vec!["api", "scores-v3", "list"], "/api/public/v3/scores"),
        (
            vec!["api", "datasets-v2", "get", "qa"],
            "/api/public/v2/datasets/qa",
        ),
        (
            vec!["api", "llm-connections", "list-llm-connections"],
            "/api/public/organizations/llm-connections",
        ),
        (
            vec!["api", "blob-storage-integrations", "list-blob-storage"],
            "/api/public/integrations/blob-storage",
        ),
        (
            vec![
                "api",
                "metrics-v2",
                "get",
                "--query",
                r#"{"view":"observations"}"#,
            ],
            "/api/public/v2/metrics",
        ),
        (
            vec![
                "api",
                "skills",
                "list-content",
                "--sha256-hashes",
                "a+b/c=,d=",
            ],
            "/api/public/skills/files/content",
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(json(200, json!({})))
            .mount(&server)
            .await;
        let output = api(&server, &args).await;
        assert_eq!(output.exit, Exit::Success, "{args:?}: {output:?}");
        let request = single_request(&server).await;
        assert_eq!(request.url.path(), expected_path, "{args:?}");
        if args[1] == "skills" {
            assert_eq!(
                request.url.query(),
                Some("sha256Hashes=a%2Bb%2Fc%3D%2Cd%3D")
            );
        }
    }
}

#[tokio::test]
async fn output_file_table_and_ndjson() {
    let server = MockServer::start().await;
    let body = json!({"data": [{"id": "1", "name": "alpha", "labels": ["production"]}, {"id": "2", "name": "beta", "labels": []}], "meta": {"page": 1}});
    Mock::given(method("GET"))
        .respond_with(json(200, body.clone()))
        .mount(&server)
        .await;

    let (_dir, root) = common::temp_dir();
    let file = root.join("out.json");
    let output = api(
        &server,
        &["api", "prompts", "list", "--output", file.to_str().unwrap()],
    )
    .await;
    assert_eq!(output.exit, Exit::Success);
    assert_eq!(output.stdout, "");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        serde_json::to_string_pretty(&body).unwrap()
    );

    let output = api(
        &server,
        &[
            "api",
            "prompts",
            "list",
            "--format",
            "table",
            "--columns",
            "name,labels",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success);
    assert!(
        output.stdout.contains("alpha") && output.stdout.contains("production"),
        "{}",
        output.stdout
    );
    assert!(!output.stdout.contains('\x1b'));

    let output = api(&server, &["api", "prompts", "list", "--format", "ndjson"]).await;
    assert_eq!(
        output.stdout,
        "{\"id\":\"1\",\"name\":\"alpha\",\"labels\":[\"production\"]}\n{\"id\":\"2\",\"name\":\"beta\",\"labels\":[]}\n"
    );

    let output = api(&server, &["api", "prompts", "list", "--columns", "name"]).await;
    assert_eq!(output.exit, Exit::Usage);
}

#[tokio::test]
async fn curl_preview_sends_nothing() {
    let server = MockServer::start().await;
    let output = api(
        &server,
        &[
            "api", "prompts", "create", "--type", "text", "--name", "a", "--prompt", "it's",
            "--curl",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success);
    assert!(output.stdout.starts_with(&format!("curl --request POST {}/api/public/v2/prompts --header 'accept: application/json' --header 'authorization: Basic <redacted>'", server.uri())), "{}", output.stdout);
    assert!(
        output
            .stdout
            .trim_end()
            .ends_with(r#"--data '{"type":"text","name":"a","prompt":"it'"'"'s"}'"#),
        "{}",
        output.stdout
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(
        api(&server, &["api", "prompts", "list", "--all", "--curl"])
            .await
            .exit,
        Exit::Usage
    );
}

#[cfg(feature = "unstable")]
#[tokio::test]
async fn unstable_placements() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    let output = api(
        &server,
        &[
            "api",
            "unstable-dashboards",
            "add-placement",
            "d1",
            "--type",
            "widget",
            "--widget-id",
            "w1",
            "--width",
            "12",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let request = single_request(&server).await;
    assert_eq!(
        request.url.path(),
        "/api/public/unstable/dashboards/d1/placements"
    );
    assert_eq!(
        body_json(&request),
        json!({"type": "widget", "widgetId": "w1", "width": 12})
    );
    assert_eq!(
        api(
            &server,
            &[
                "api",
                "unstable-dashboards",
                "add-placement",
                "d1",
                "--type",
                "preset"
            ]
        )
        .await
        .exit,
        Exit::Usage
    );
}

/// Flags whose values are an SDK enum: listed by clap, sent as the wire name.
#[tokio::test]
async fn enum_flags_are_checked_by_clap_and_sent_as_wire_names() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;

    let output = api(
        &server,
        &[
            "api",
            "annotation-queues",
            "create-item",
            "q1",
            "--object-id",
            "t1",
            "--object-type",
            "TRACE",
            "--status",
            "COMPLETED",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let output = api(
        &server,
        &[
            "api",
            "annotation-queues",
            "update-item",
            "q1",
            "i1",
            "--status",
            "PENDING",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let output = api(
        &server,
        &[
            "api",
            "annotation-queues",
            "list-items",
            "q1",
            "--status",
            "PENDING",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let output = api(
        &server,
        &[
            "api",
            "score-configs",
            "create",
            "--name",
            "quality",
            "--data-type",
            "NUMERIC",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let output = api(
        &server,
        &[
            "api",
            "organizations",
            "update-membership",
            "--user-id",
            "u1",
            "--role",
            "ADMIN",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 5);
    assert_eq!(
        body_json(&requests[0]),
        json!({"objectId": "t1", "objectType": "TRACE", "status": "COMPLETED"})
    );
    assert_eq!(body_json(&requests[1]), json!({"status": "PENDING"}));
    assert_eq!(query_pairs(&requests[2]), pairs(&[("status", "PENDING")]));
    assert_eq!(
        body_json(&requests[3]),
        json!({"name": "quality", "dataType": "NUMERIC"})
    );
    assert_eq!(
        body_json(&requests[4]),
        json!({"userId": "u1", "role": "ADMIN"})
    );

    // A value outside the list is a usage error that names the list.
    for args in [
        vec![
            "api",
            "annotation-queues",
            "list-items",
            "q1",
            "--status",
            "DONE",
        ],
        vec![
            "api",
            "organizations",
            "update-membership",
            "--user-id",
            "u1",
            "--role",
            "admin",
        ],
        vec![
            "api",
            "score-configs",
            "create",
            "--name",
            "n",
            "--data-type",
            "CORRECTION",
        ],
    ] {
        let output = api(&server, &args).await;
        assert_eq!(output.exit, Exit::Usage, "{args:?}");
        assert!(
            output.stderr.contains("possible values:"),
            "{}",
            output.stderr
        );
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 5);
}

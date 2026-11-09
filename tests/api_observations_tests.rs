mod common;

use common::*;
use langfuse_sdk::api::observations::ListObservationsParams;
use serde_json::json;
use wiremock::Mock;
use wiremock::matchers::{method, path};

fn empty_page() -> serde_json::Value {
    json!({"data": [], "meta": {"cursor": null}})
}

#[tokio::test]
async fn environment_is_sent_as_repeated_key() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .respond_with(json(200, empty_page()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .observations
        .list(&ListObservationsParams {
            limit: Some(10),
            environment: Some(vec!["production".into(), "staging".into()]),
            parse_io_as_json: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();

    let req = single_request(&server).await;
    let pairs = query_pairs(&req);
    let envs: Vec<&str> = pairs
        .iter()
        .filter(|(k, _)| k == "environment")
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(envs, ["production", "staging"]);
    assert!(
        req.url
            .query()
            .unwrap()
            .contains("environment=production&environment=staging")
    );
    assert_eq!(query_value(&req, "limit"), "10");
    assert_eq!(query_value(&req, "parseIoAsJson"), "true");
}

#[tokio::test]
async fn all_params_use_camel_case_keys() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "id": "obs-1", "traceId": "t-1", "projectId": "p", "type": "GENERATION",
                    "startTime": TS, "level": "DEFAULT"
                }],
                "meta": {"cursor": "next"}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .observations
        .list(&ListObservationsParams {
            limit: Some(1),
            cursor: Some("c".into()),
            name: Some("n".into()),
            user_id: Some("u".into()),
            session_id: Some("s".into()),
            r#type: Some("GENERATION".into()),
            trace_id: Some("t".into()),
            level: Some("ERROR".into()),
            parent_observation_id: Some("po".into()),
            environment: Some(vec!["e".into()]),
            from_start_time: Some(TS.into()),
            to_start_time: Some(TS.into()),
            version: Some("v1".into()),
            fields: Some("core,io".into()),
            expand_metadata: Some("key".into()),
            parse_io_as_json: Some(false),
            is_root_observation: Some(true),
            filter: Some("[]".into()),
        })
        .await
        .unwrap();
    assert_eq!(page.data[0].id, "obs-1");
    assert_eq!(page.meta.cursor.as_deref(), Some("next"));

    let req = single_request(&server).await;
    let expected = [
        ("limit", "1"),
        ("cursor", "c"),
        ("name", "n"),
        ("userId", "u"),
        ("sessionId", "s"),
        ("type", "GENERATION"),
        ("traceId", "t"),
        ("level", "ERROR"),
        ("parentObservationId", "po"),
        ("environment", "e"),
        ("fromStartTime", TS),
        ("toStartTime", TS),
        ("version", "v1"),
        ("fields", "core,io"),
        ("expandMetadata", "key"),
        ("parseIoAsJson", "false"),
        ("isRootObservation", "true"),
        ("filter", "[]"),
    ];
    assert_eq!(query_pairs(&req).len(), expected.len());
    for (key, value) in expected {
        assert_eq!(query_value(&req, key), value, "query param {key}");
    }
}

#[tokio::test]
async fn default_params_send_no_query() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .respond_with(json(200, empty_page()))
        .mount(&server)
        .await;

    client
        .observations
        .list(&ListObservationsParams::default())
        .await
        .unwrap();
    let req = single_request(&server).await;
    assert!(req.url.query().is_none_or(str::is_empty));
}

mod common;

use common::*;
use langfuse_sdk::api::scores::ListScoresParams;
use langfuse_sdk::models::scores::{
    CreateScoreRequest, CreateScoreValue, CreateScoresResponse, ScoreV3,
};
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn list_scores_sends_all_query_params_in_camel_case() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v3/scores"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "id": "s1", "projectId": "p", "name": "acc", "source": "API",
                    "timestamp": TS, "environment": "default",
                    "createdAt": TS, "updatedAt": TS,
                    "dataType": "BOOLEAN", "value": false
                }],
                "meta": {"cursor": "next-page", "limit": 5}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let params = ListScoresParams {
        limit: Some(5),
        cursor: Some("c0".into()),
        name: Some("acc".into()),
        source: Some("API".into()),
        data_type: Some("BOOLEAN".into()),
        environment: Some("prod".into()),
        config_id: Some("cfg".into()),
        trace_id: Some("t".into()),
        observation_id: Some("o".into()),
        from_timestamp: Some(TS.into()),
        to_timestamp: Some(TS.into()),
        id: Some("s1".into()),
        session_id: Some("sess".into()),
        experiment_id: Some("exp".into()),
        queue_id: Some("q".into()),
        author_user_id: Some("u".into()),
        value: Some("1".into()),
        value_min: Some(0.5),
        value_max: Some(1.5),
        fields: Some("details,subject".into()),
    };
    let page = client.scores.list(&params).await.unwrap();
    assert_eq!(page.meta.cursor.as_deref(), Some("next-page"));
    assert!(matches!(
        page.data[0],
        ScoreV3::Boolean { value: false, .. }
    ));

    let req = single_request(&server).await;
    let expected = [
        ("limit", "5"),
        ("cursor", "c0"),
        ("name", "acc"),
        ("source", "API"),
        ("dataType", "BOOLEAN"),
        ("environment", "prod"),
        ("configId", "cfg"),
        ("traceId", "t"),
        ("observationId", "o"),
        ("fromTimestamp", TS),
        ("toTimestamp", TS),
        ("id", "s1"),
        ("sessionId", "sess"),
        ("experimentId", "exp"),
        ("queueId", "q"),
        ("authorUserId", "u"),
        ("value", "1"),
        ("valueMin", "0.5"),
        ("valueMax", "1.5"),
        ("fields", "details,subject"),
    ];
    let pairs = query_pairs(&req);
    assert_eq!(pairs.len(), expected.len());
    for (key, value) in expected {
        assert_eq!(query_value(&req, key), value, "query param {key}");
    }
}

#[tokio::test]
async fn create_scores_posts_array_and_parses_202() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/scores"))
        .and(body_json(json!([
            {"name": "accuracy", "value": 0.95},
            {"name": "sentiment", "value": "positive", "traceId": "t1"}
        ])))
        .respond_with(json(202, json!({"message": "Accepted"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .scores
        .create(&[
            CreateScoreRequest::new("accuracy", CreateScoreValue::Numeric(0.95)),
            CreateScoreRequest {
                trace_id: Some("t1".into()),
                ..CreateScoreRequest::new("sentiment", CreateScoreValue::String("positive".into()))
            },
        ])
        .await
        .unwrap();
    assert!(matches!(resp, CreateScoresResponse::Accepted(ref a) if a.message == "Accepted"));
}

#[tokio::test]
async fn create_scores_parses_207_partial_results() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/scores"))
        .respond_with(json(
            207,
            json!({"accepted": 1, "rejected": 1, "errors": [{"message": "bad value"}]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .scores
        .create(&[
            CreateScoreRequest::new("a", CreateScoreValue::Numeric(1.0)),
            CreateScoreRequest::new("b", CreateScoreValue::Numeric(2.0)),
        ])
        .await
        .unwrap();
    match resp {
        CreateScoresResponse::BatchResults(r) => {
            assert_eq!((r.accepted, r.rejected), (1, 1));
            assert_eq!(r.errors[0].message, "bad value");
        }
        other => panic!("expected batch results, got {other:?}"),
    }
}

#[tokio::test]
async fn delete_score_accepts_204() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/scores/score-1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client.scores.delete("score-1").await.unwrap();
}

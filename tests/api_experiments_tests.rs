mod common;

use common::*;
use langfuse_sdk::api::experiments::{ListExperimentItemsParams, ListExperimentsParams};
use langfuse_sdk::models::enums::ObservationLevel;
use langfuse_sdk::models::scores::ScoreV3;
use serde_json::json;
use wiremock::Mock;
use wiremock::matchers::{method, path};

fn score_json() -> serde_json::Value {
    json!({
        "id": "s-1", "projectId": "p", "name": "accuracy", "source": "EVAL",
        "timestamp": TS, "environment": "default", "createdAt": TS, "updatedAt": TS,
        "dataType": "NUMERIC", "value": 0.8,
        "subject": {"kind": "experiment", "id": "exp-1"}
    })
}

#[tokio::test]
async fn list_experiments_always_sends_from_start_time() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/experiments"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "id": "exp-1", "name": "baseline", "description": null,
                    "startTime": TS, "endTime": TS, "itemCount": 10, "datasetId": "ds-1",
                    "metadata": {"model": "gpt-4o"}, "scores": [score_json()]
                }],
                "meta": {"cursor": "next"}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let params = ListExperimentsParams {
        fields: Some("core,scores".into()),
        limit: Some(5),
        dataset_id: Some("ds-1".into()),
        ..ListExperimentsParams::new(TS)
    };
    let page = client.experiments.list(&params).await.unwrap();
    assert_eq!(page.meta.cursor.as_deref(), Some("next"));
    let experiment = &page.data[0];
    assert_eq!(experiment.item_count, 10);
    assert!(matches!(
        experiment.scores.as_deref().unwrap()[0],
        ScoreV3::Numeric { .. }
    ));

    let req = single_request(&server).await;
    assert_eq!(query_value(&req, "fromStartTime"), TS);
    assert_eq!(query_value(&req, "fields"), "core,scores");
    assert_eq!(query_value(&req, "datasetId"), "ds-1");
    assert_eq!(query_pairs(&req).len(), 4);
}

#[tokio::test]
async fn list_experiments_minimal_params() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/experiments"))
        .respond_with(json(200, json!({"data": [], "meta": {}})))
        .expect(1)
        .mount(&server)
        .await;

    client
        .experiments
        .list(&ListExperimentsParams::new(TS))
        .await
        .unwrap();
    let req = single_request(&server).await;
    assert_eq!(
        query_pairs(&req),
        vec![("fromStartTime".to_string(), TS.to_string())]
    );
}

#[tokio::test]
async fn list_experiment_items() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/experiment-items"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "id": "ei-1", "traceId": "t-1", "startTime": TS, "endTime": null,
                    "level": "DEFAULT", "environment": "default",
                    "experimentId": "exp-1", "experimentName": "baseline", "experimentItemId": "item-1",
                    "input": {"q": "2+2"}, "output": "4", "expectedOutput": "4",
                    "scores": [score_json()]
                }],
                "meta": {"cursor": null}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let params = ListExperimentItemsParams {
        experiment_id: Some("exp-1".into()),
        to_start_time: Some(TS.into()),
        ..ListExperimentItemsParams::new(TS)
    };
    let page = client.experiments.list_items(&params).await.unwrap();
    let item = &page.data[0];
    assert_eq!(item.level, ObservationLevel::Default);
    assert_eq!(item.output.as_ref().unwrap(), "4");
    assert!(item.end_time.is_none());
    assert!(item.experiment_dataset_id.is_none());

    let req = single_request(&server).await;
    assert_eq!(query_value(&req, "fromStartTime"), TS);
    assert_eq!(query_value(&req, "toStartTime"), TS);
    assert_eq!(query_value(&req, "experimentId"), "exp-1");
}

mod common;

use common::*;
use langfuse_sdk::models::enums::ScoreConfigDataType;
use langfuse_sdk::models::score_configs::{
    CreateConfigCategory, CreateScoreConfigRequest, UpdateScoreConfigRequest,
};
use langfuse_sdk::pagination::PageParams;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

const BASE: &str = "/api/public/score-configs";

fn config_json() -> Value {
    json!({
        "id": "cfg-1",
        "name": "sentiment",
        "createdAt": TS,
        "updatedAt": TS,
        "projectId": "p",
        "dataType": "CATEGORICAL",
        "isArchived": false,
        "minValue": null,
        "maxValue": null,
        "categories": [{"value": 0.0, "label": "negative"}, {"value": 1.0, "label": "positive"}],
        "description": null
    })
}

#[tokio::test]
async fn list_score_configs() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(BASE))
        .and(query_param("page", "2"))
        .respond_with(json(
            200,
            json!({"data": [config_json()], "meta": {"page": 2, "limit": 50, "totalItems": 51, "totalPages": 2}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .score_configs
        .list(&PageParams {
            page: Some(2),
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(page.data[0].data_type, ScoreConfigDataType::Categorical);
    assert_eq!(page.data[0].categories.as_ref().unwrap().len(), 2);
}

#[tokio::test]
async fn create_score_config() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(BASE))
        .and(body_json(json!({
            "name": "sentiment",
            "dataType": "CATEGORICAL",
            "categories": [{"value": 0.0, "label": "negative"}, {"value": 1.0, "label": "positive"}]
        })))
        .respond_with(json(200, config_json()))
        .expect(1)
        .mount(&server)
        .await;

    let config = client
        .score_configs
        .create(&CreateScoreConfigRequest {
            name: "sentiment".into(),
            data_type: ScoreConfigDataType::Categorical,
            categories: Some(vec![
                CreateConfigCategory {
                    value: 0.0,
                    label: "negative".into(),
                },
                CreateConfigCategory {
                    value: 1.0,
                    label: "positive".into(),
                },
            ]),
            min_value: None,
            max_value: None,
            description: None,
        })
        .await
        .unwrap();
    assert_eq!(config.id, "cfg-1");
}

#[tokio::test]
async fn get_score_config_by_id() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/cfg-1")))
        .respond_with(json(200, config_json()))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        client.score_configs.get("cfg-1").await.unwrap().name,
        "sentiment"
    );
}

#[tokio::test]
async fn update_score_config_archives() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path(format!("{BASE}/cfg-1")))
        .and(body_json(json!({"isArchived": true})))
        .respond_with(json(200, config_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .score_configs
        .update(
            "cfg-1",
            &UpdateScoreConfigRequest {
                is_archived: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

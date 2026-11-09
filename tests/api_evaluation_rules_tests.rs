mod common;

use common::*;
use langfuse_sdk::LangfuseError;
use langfuse_sdk::models::evaluation_common::{
    PromptVariableMapping, PromptVariableMappingInput, PromptVariableMappingSource, PublicApiError,
    PublicApiErrorCode,
};
use langfuse_sdk::models::evaluation_rules::*;
use langfuse_sdk::pagination::CursorParams;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

const BASE: &str = "/api/public/v2/evaluation-rules";

fn rule_json() -> Value {
    json!({
        "id": "rule-1",
        "name": "Helpfulness on prod",
        "createdBy": {"id": "u-1", "name": null},
        "enabled": true,
        "sampling": 0.5,
        "filter": [
            {"type": "stringOptions", "column": "environment", "operator": "any of", "value": ["production"]},
            {"type": "stringObject", "column": "metadata", "operator": "=", "value": "x", "key": "tier"}
        ],
        "evaluatorAssignments": [{
            "evaluatorId": "ev-1",
            "variableMapping": [
                {"variable": "input", "source": "input", "jsonPath": null},
                {"mappingType": "legacy", "variable": "output", "langfuseObject": "generation",
                 "objectName": "llm-call", "source": "output"}
            ]
        }],
        "createdAt": TS,
        "updatedAt": TS
    })
}

#[tokio::test]
async fn create_rule_with_typed_filters() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(BASE))
        .and(body_json(json!({
            "name": "Helpfulness on prod",
            "enabled": true,
            "sampling": 0.5,
            "filter": [
                {"type": "stringOptions", "column": "environment", "operator": "any of", "value": ["production"]},
                {"type": "null", "column": "parentObservationId", "operator": "is null", "value": ""}
            ],
            "evaluatorAssignments": [{
                "evaluatorId": "ev-1",
                "variableMapping": [{"variable": "input", "source": "input"}]
            }]
        })))
        .respond_with(json(201, rule_json()))
        .expect(1)
        .mount(&server)
        .await;

    let rule = client
        .evaluation_rules
        .create(&CreateEvaluationRuleRequest {
            name: "Helpfulness on prod".into(),
            enabled: true,
            sampling: Some(0.5),
            filter: Some(vec![
                EvaluationRuleFilter::StringOptions {
                    column: "environment".into(),
                    operator: OptionsFilterOperator::AnyOf,
                    value: vec!["production".into()],
                },
                EvaluationRuleFilter::Null {
                    column: "parentObservationId".into(),
                    operator: NullFilterOperator::IsNull,
                    value: EmptyValue,
                },
            ]),
            evaluator_assignments: vec![EvaluationRuleEvaluatorAssignmentInput {
                evaluator_id: "ev-1".into(),
                variable_mapping: Some(vec![PromptVariableMappingInput {
                    variable: "input".into(),
                    source: PromptVariableMappingSource::Input,
                    json_path: None,
                }]),
            }],
        })
        .await
        .unwrap();

    assert_eq!(rule.created_by.as_ref().unwrap().id, "u-1");
    assert_eq!(rule.filter[1].key.as_deref(), Some("tier"));
    assert_eq!(rule.filter[0].value.as_ref().unwrap()[0], "production");
    let mappings = rule.evaluator_assignments[0]
        .variable_mapping
        .as_ref()
        .unwrap();
    assert!(
        matches!(&mappings[0], PromptVariableMapping::Read(m) if m.source.as_deref() == Some("input"))
    );
    assert!(
        matches!(&mappings[1], PromptVariableMapping::Legacy(m) if m.object_name.as_deref() == Some("llm-call"))
    );
}

#[tokio::test]
async fn create_rule_409_parses_public_api_error() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(BASE))
        .respond_with(json(
            409,
            json!({"message": "Evaluator is paused", "code": "conflict", "details": null}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let err = client
        .evaluation_rules
        .create(&CreateEvaluationRuleRequest {
            name: "r".into(),
            enabled: true,
            sampling: None,
            filter: None,
            evaluator_assignments: vec![],
        })
        .await
        .unwrap_err();
    assert!(matches!(err, LangfuseError::Conflict { .. }));
    let api_error = PublicApiError::from_error(&err).unwrap();
    assert_eq!(api_error.code, PublicApiErrorCode::Conflict);
    assert_eq!(api_error.message, "Evaluator is paused");
}

#[tokio::test]
async fn rate_limit_error_details() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/rule-1")))
        .respond_with(json(
            429,
            json!({
                "message": "Too many requests",
                "code": "rate_limited",
                "details": {"retryAfterSeconds": 12, "limit": 100, "remaining": 0, "resetAt": TS}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let err = client.evaluation_rules.get("rule-1").await.unwrap_err();
    assert!(matches!(err, LangfuseError::RateLimited { .. }));
    let details = PublicApiError::from_error(&err).unwrap().details.unwrap();
    assert_eq!(details.retry_after_seconds, Some(12));
    assert_eq!(details.remaining, Some(0));
    assert!(details.reset_at.is_some());
}

#[tokio::test]
async fn list_rules_with_cursor() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(BASE))
        .and(query_param("limit", "2"))
        .and(query_param("cursor", "abc"))
        .respond_with(json(
            200,
            json!({"data": [rule_json()], "meta": {"cursor": "def"}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .evaluation_rules
        .list(&CursorParams {
            cursor: Some("abc".into()),
            limit: Some(2),
        })
        .await
        .unwrap();
    assert_eq!(page.meta.cursor.as_deref(), Some("def"));
    assert!((page.data[0].sampling - 0.5).abs() < f64::EPSILON);
}

#[tokio::test]
async fn update_rule_sends_only_set_fields() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path(format!("{BASE}/rule-1")))
        .and(body_json(json!({"enabled": false})))
        .respond_with(json(200, rule_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .evaluation_rules
        .update(
            "rule-1",
            &UpdateEvaluationRuleRequest {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_rule() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/rule-1")))
        .respond_with(json(200, json!({"id": "rule-1"})))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        client.evaluation_rules.delete("rule-1").await.unwrap().id,
        "rule-1"
    );
}

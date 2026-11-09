mod common;

use common::*;
use langfuse_sdk::LangfuseError;
use langfuse_sdk::models::evaluation_common::{
    PromptVariableMappingInput, PromptVariableMappingSource, PublicApiError, PublicApiErrorCode,
};
use langfuse_sdk::models::evaluators::*;
use langfuse_sdk::pagination::CursorParams;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

const BASE: &str = "/api/public/v2/evaluators";

fn base_fields() -> Value {
    json!({
        "id": "ev-1",
        "name": "Helpfulness",
        "description": null,
        "createdBy": null,
        "status": "active",
        "pausedAt": null,
        "pausedReason": null,
        "pausedMessage": null,
        "evaluationRuleAssignments": [{"evaluationRuleId": "rule-1", "variableMappingOverride": null}],
        "createdAt": TS,
        "updatedAt": TS,
        "versionId": "evv-1",
        "version": 1,
        "versionCreatedAt": TS,
        "versionCreatedBy": {"id": "u-1", "name": "Ada"}
    })
}

fn merge(mut a: Value, b: Value) -> Value {
    a.as_object_mut()
        .unwrap()
        .extend(b.as_object().unwrap().clone());
    a
}

fn llm_evaluator_json() -> Value {
    merge(
        base_fields(),
        json!({
            "type": "llm_as_judge",
            "prompt": [
                {"role": "system", "content": "You are a strict judge."},
                {"role": "user", "content": "Rate {{output}} for {{input}}"}
            ],
            "variables": ["input", "output"],
            "variableMapping": [{"variable": "input", "source": "input", "jsonPath": null}],
            "modelConfig": {"provider": "openai", "model": "gpt-4o"},
            "outputDefinition": {
                "dataType": "CATEGORICAL",
                "categories": ["good", "bad"],
                "shouldAllowMultipleMatches": false,
                "scoreReasoningInstructions": "Explain briefly"
            }
        }),
    )
}

fn code_evaluator_json() -> Value {
    merge(
        base_fields(),
        json!({"type": "code", "sourceCode": "def score(): return 1", "sourceCodeLanguage": "PYTHON"}),
    )
}

#[tokio::test]
async fn create_llm_as_judge_evaluator() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(BASE))
        .and(body_json(json!({
            "type": "llm_as_judge",
            "name": "Helpfulness",
            "prompt": "Rate {{output}} for {{input}}",
            "variableMapping": [{"variable": "input", "source": "input"}],
            "outputDefinition": {"dataType": "NUMERIC", "minValue": 0.0, "maxValue": 1.0}
        })))
        .respond_with(json(201, llm_evaluator_json()))
        .expect(1)
        .mount(&server)
        .await;

    let evaluator = client
        .evaluators
        .create(&CreateEvaluatorRequest::LlmAsJudge {
            name: "Helpfulness".into(),
            description: None,
            prompt: EvaluatorChatPromptInput::Text("Rate {{output}} for {{input}}".into()),
            model_config: None,
            variable_mapping: Some(vec![PromptVariableMappingInput {
                variable: "input".into(),
                source: PromptVariableMappingSource::Input,
                json_path: None,
            }]),
            output_definition: EvaluatorOutputDefinition::Numeric {
                min_value: Some(0.0),
                max_value: Some(1.0),
                score_reasoning_instructions: None,
                score_value_instructions: None,
            },
        })
        .await
        .unwrap();

    assert_eq!(evaluator.base.status, EvaluatorStatus::Active);
    assert_eq!(
        evaluator
            .base
            .version_created_by
            .as_ref()
            .unwrap()
            .name
            .as_deref(),
        Some("Ada")
    );
    match &evaluator.definition {
        EvaluatorDefinition::LlmAsJudge {
            prompt,
            variables,
            model_config,
            output_definition,
            ..
        } => {
            assert_eq!(prompt[0].role, EvaluatorChatMessageRole::System);
            assert_eq!(variables, &["input", "output"]);
            assert_eq!(model_config.as_ref().unwrap().model, "gpt-4o");
            assert!(matches!(
                output_definition,
                EvaluatorOutputDefinition::Categorical { categories, should_allow_multiple_matches: false, .. }
                    if categories.len() == 2
            ));
        }
        other => panic!("expected LLM-as-judge, got {other:?}"),
    }
}

#[tokio::test]
async fn create_code_evaluator_412_is_precondition_failed() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(BASE))
        .and(body_json(json!({
            "type": "code",
            "name": "Exact match",
            "sourceCode": "def score(): return 1",
            "sourceCodeLanguage": "PYTHON"
        })))
        .respond_with(json(
            412,
            json!({"message": "No default evaluation model", "code": "invalid_request"}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let err = client
        .evaluators
        .create(&CreateEvaluatorRequest::Code {
            name: "Exact match".into(),
            description: None,
            source_code: "def score(): return 1".into(),
            source_code_language: CodeEvaluatorSourceCodeLanguage::Python,
        })
        .await
        .unwrap_err();
    assert!(
        matches!(err, LangfuseError::PreconditionFailed { .. }),
        "got {err:?}"
    );
    assert_eq!(
        PublicApiError::from_error(&err).unwrap().code,
        PublicApiErrorCode::InvalidRequest
    );
}

#[tokio::test]
async fn list_evaluators_mixed_types() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(BASE))
        .and(query_param("limit", "2"))
        .respond_with(json(
            200,
            json!({"data": [llm_evaluator_json(), code_evaluator_json()], "meta": {}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .evaluators
        .list(&CursorParams {
            cursor: None,
            limit: Some(2),
        })
        .await
        .unwrap();
    assert!(page.meta.cursor.is_none());
    assert!(matches!(
        page.data[0].definition,
        EvaluatorDefinition::LlmAsJudge { .. }
    ));
    assert!(matches!(
        page.data[1].definition,
        EvaluatorDefinition::Code {
            source_code_language: CodeEvaluatorSourceCodeLanguage::Python,
            ..
        }
    ));
}

#[tokio::test]
async fn get_evaluator() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/ev-1")))
        .respond_with(json(200, code_evaluator_json()))
        .expect(1)
        .mount(&server)
        .await;

    let evaluator = client.evaluators.get("ev-1").await.unwrap();
    assert_eq!(
        evaluator.base.evaluation_rule_assignments[0].evaluation_rule_id,
        "rule-1"
    );
}

#[tokio::test]
async fn update_metadata_can_clear_description() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path(format!("{BASE}/ev-1")))
        .and(body_json(json!({"description": null})))
        .respond_with(json(200, code_evaluator_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .evaluators
        .update(
            "ev-1",
            &UpdateEvaluatorRequest::Metadata(UpdateEvaluatorMetadataRequest {
                name: None,
                description: Some(None),
            }),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn update_definition_409_is_conflict() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path(format!("{BASE}/ev-1")))
        .and(body_json(json!({
            "type": "code",
            "sourceCode": "def score(): return 0",
            "sourceCodeLanguage": "TYPESCRIPT"
        })))
        .respond_with(json(
            409,
            json!({"message": "Type cannot change", "code": "conflict"}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let err = client
        .evaluators
        .update(
            "ev-1",
            &UpdateEvaluatorRequest::Code(UpdateCodeEvaluatorRequest {
                name: None,
                description: None,
                source_code: "def score(): return 0".into(),
                source_code_language: CodeEvaluatorSourceCodeLanguage::Typescript,
            }),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LangfuseError::Conflict { .. }));
}

#[tokio::test]
async fn delete_evaluator() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/ev-1")))
        .respond_with(json(200, json!({"id": "ev-1"})))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(client.evaluators.delete("ev-1").await.unwrap().id, "ev-1");
}

#[tokio::test]
async fn list_versions() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/ev-1/versions")))
        .and(query_param("cursor", "c1"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "id": "evv-2", "version": 2, "createdAt": TS, "createdBy": null,
                    "type": "code", "sourceCode": "x", "sourceCodeLanguage": "TYPESCRIPT"
                }],
                "meta": {"cursor": null}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .evaluators
        .list_versions(
            "ev-1",
            &CursorParams {
                cursor: Some("c1".into()),
                limit: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.data[0].base.version, 2);
    assert!(matches!(
        page.data[0].definition,
        EvaluatorDefinition::Code { .. }
    ));
}

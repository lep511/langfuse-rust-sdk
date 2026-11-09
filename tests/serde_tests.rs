use langfuse_sdk::models::dataset_items::*;
use langfuse_sdk::models::datasets::*;
use langfuse_sdk::models::enums::*;
use langfuse_sdk::models::health::*;
use langfuse_sdk::models::ingestion::*;
use langfuse_sdk::models::media::*;
use langfuse_sdk::models::models_api::*;
use langfuse_sdk::models::observations::*;
use langfuse_sdk::models::projects::*;
use langfuse_sdk::models::prompts::*;
use langfuse_sdk::models::score_configs::*;
use langfuse_sdk::models::scores::*;
use langfuse_sdk::models::skills::*;
use langfuse_sdk::models::traces::*;

// ═══════════════════════════════════════════════════════════════════════
//  Enums — roundtrip for every variant
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn model_usage_unit_all_variants() {
    let cases = [
        (ModelUsageUnit::Characters, r#""CHARACTERS""#),
        (ModelUsageUnit::Tokens, r#""TOKENS""#),
        (ModelUsageUnit::Milliseconds, r#""MILLISECONDS""#),
        (ModelUsageUnit::Seconds, r#""SECONDS""#),
        (ModelUsageUnit::Images, r#""IMAGES""#),
        (ModelUsageUnit::Requests, r#""REQUESTS""#),
    ];
    for (variant, expected_json) in &cases {
        let json = serde_json::to_string(variant).unwrap();
        assert_eq!(&json, expected_json);
        let back: ModelUsageUnit = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, variant);
    }
}

#[test]
fn observation_level_all_variants() {
    let cases = [
        (ObservationLevel::Debug, r#""DEBUG""#),
        (ObservationLevel::Default, r#""DEFAULT""#),
        (ObservationLevel::Warning, r#""WARNING""#),
        (ObservationLevel::Error, r#""ERROR""#),
    ];
    for (variant, expected_json) in &cases {
        let json = serde_json::to_string(variant).unwrap();
        assert_eq!(&json, expected_json);
        let back: ObservationLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, variant);
    }
}

#[test]
fn dataset_status_all_variants() {
    let cases = [
        (DatasetStatus::Active, r#""ACTIVE""#),
        (DatasetStatus::Archived, r#""ARCHIVED""#),
    ];
    for (variant, expected_json) in &cases {
        let json = serde_json::to_string(variant).unwrap();
        assert_eq!(&json, expected_json);
        let back: DatasetStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, variant);
    }
}

#[test]
fn score_source_all_variants() {
    let cases = [
        (ScoreSource::Annotation, r#""ANNOTATION""#),
        (ScoreSource::Api, r#""API""#),
        (ScoreSource::Eval, r#""EVAL""#),
    ];
    for (variant, expected_json) in &cases {
        let json = serde_json::to_string(variant).unwrap();
        assert_eq!(&json, expected_json);
        let back: ScoreSource = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, variant);
    }
}

#[test]
fn score_data_type_all_variants() {
    let cases = [
        (ScoreDataType::Numeric, r#""NUMERIC""#),
        (ScoreDataType::Boolean, r#""BOOLEAN""#),
        (ScoreDataType::Categorical, r#""CATEGORICAL""#),
        (ScoreDataType::Correction, r#""CORRECTION""#),
        (ScoreDataType::Text, r#""TEXT""#),
    ];
    for (variant, expected_json) in &cases {
        let json = serde_json::to_string(variant).unwrap();
        assert_eq!(&json, expected_json);
        let back: ScoreDataType = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, variant);
    }
}

#[test]
fn score_config_data_type_all_variants() {
    let cases = [
        (ScoreConfigDataType::Numeric, r#""NUMERIC""#),
        (ScoreConfigDataType::Boolean, r#""BOOLEAN""#),
        (ScoreConfigDataType::Categorical, r#""CATEGORICAL""#),
        (ScoreConfigDataType::Text, r#""TEXT""#),
    ];
    for (variant, expected_json) in &cases {
        let json = serde_json::to_string(variant).unwrap();
        assert_eq!(&json, expected_json);
        let back: ScoreConfigDataType = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, variant);
    }
}

#[test]
fn prompt_type_all_variants() {
    let cases = [
        (PromptType::Chat, r#""chat""#),
        (PromptType::Text, r#""text""#),
    ];
    for (variant, expected_json) in &cases {
        let json = serde_json::to_string(variant).unwrap();
        assert_eq!(&json, expected_json);
        let back: PromptType = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, variant);
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  ScoreV3 tagged union — all five variants
// ═══════════════════════════════════════════════════════════════════════

fn score_base_json(extra: &str) -> String {
    format!(
        r#"{{
            "id": "score-001",
            "name": "my-score",
            "source": "API",
            "timestamp": "2024-06-15T10:30:00Z",
            "createdAt": "2024-06-15T10:30:00Z",
            "updatedAt": "2024-06-15T10:31:00Z",
            "environment": "production",
            "traceId": "trace-abc",
            "observationId": null,
            "comment": "looks good",
            "metadata": {{"reviewer": "alice"}},
            {extra}
        }}"#
    )
}

#[test]
fn score_v3_numeric() {
    let json = score_base_json(r#""dataType": "NUMERIC", "value": 0.95"#);
    let score: ScoreV3 = serde_json::from_str(&json).unwrap();
    match &score {
        ScoreV3::Numeric { base, value } => {
            assert_eq!(base.id, "score-001");
            assert_eq!(base.name, "my-score");
            assert_eq!(base.source, ScoreSource::Api);
            assert_eq!(base.trace_id.as_deref(), Some("trace-abc"));
            assert!(base.observation_id.is_none());
            assert_eq!(base.comment.as_deref(), Some("looks good"));
            assert!(base.metadata.is_some());
            assert!((value - 0.95).abs() < f64::EPSILON);
        }
        other => panic!("Expected Numeric, got {other:?}"),
    }
}

#[test]
fn score_v3_boolean() {
    let json =
        score_base_json(r#""dataType": "BOOLEAN", "value": 1.0, "stringValue": "true""#);
    let score: ScoreV3 = serde_json::from_str(&json).unwrap();
    match &score {
        ScoreV3::Boolean {
            base,
            value,
            string_value,
        } => {
            assert_eq!(base.id, "score-001");
            assert!((value - 1.0).abs() < f64::EPSILON);
            assert_eq!(string_value, "true");
        }
        other => panic!("Expected Boolean, got {other:?}"),
    }
}

#[test]
fn score_v3_categorical() {
    let json = score_base_json(
        r#""dataType": "CATEGORICAL", "value": 2.0, "stringValue": "positive""#,
    );
    let score: ScoreV3 = serde_json::from_str(&json).unwrap();
    match &score {
        ScoreV3::Categorical {
            value,
            string_value,
            ..
        } => {
            assert!((value.unwrap() - 2.0).abs() < f64::EPSILON);
            assert_eq!(string_value, "positive");
        }
        other => panic!("Expected Categorical, got {other:?}"),
    }
}

#[test]
fn score_v3_categorical_null_value() {
    let json = score_base_json(
        r#""dataType": "CATEGORICAL", "value": null, "stringValue": "neutral""#,
    );
    let score: ScoreV3 = serde_json::from_str(&json).unwrap();
    match &score {
        ScoreV3::Categorical {
            value,
            string_value,
            ..
        } => {
            assert!(value.is_none());
            assert_eq!(string_value, "neutral");
        }
        other => panic!("Expected Categorical, got {other:?}"),
    }
}

#[test]
fn score_v3_correction() {
    let json = score_base_json(
        r#""dataType": "CORRECTION", "value": null, "stringValue": "fixed answer""#,
    );
    let score: ScoreV3 = serde_json::from_str(&json).unwrap();
    match &score {
        ScoreV3::Correction {
            value,
            string_value,
            ..
        } => {
            assert!(value.is_none());
            assert_eq!(string_value, "fixed answer");
        }
        other => panic!("Expected Correction, got {other:?}"),
    }
}

#[test]
fn score_v3_text() {
    let json = score_base_json(r#""dataType": "TEXT", "stringValue": "Great response!""#);
    let score: ScoreV3 = serde_json::from_str(&json).unwrap();
    match &score {
        ScoreV3::Text {
            base, string_value, ..
        } => {
            assert_eq!(base.environment, "production");
            assert_eq!(string_value, "Great response!");
        }
        other => panic!("Expected Text, got {other:?}"),
    }
}

#[test]
fn score_v3_minimal_optional_fields() {
    let json = r#"{
        "id": "s-min",
        "name": "minimal",
        "source": "EVAL",
        "dataType": "NUMERIC",
        "timestamp": "2024-01-01T00:00:00Z",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z",
        "environment": "dev",
        "value": 0.5
    }"#;
    let score: ScoreV3 = serde_json::from_str(json).unwrap();
    match &score {
        ScoreV3::Numeric { base, .. } => {
            assert!(base.trace_id.is_none());
            assert!(base.session_id.is_none());
            assert!(base.observation_id.is_none());
            assert!(base.dataset_run_id.is_none());
            assert!(base.author_user_id.is_none());
            assert!(base.comment.is_none());
            assert!(base.metadata.is_none());
            assert!(base.config_id.is_none());
            assert!(base.queue_id.is_none());
        }
        other => panic!("Expected Numeric, got {other:?}"),
    }
}

#[test]
fn score_v3_unknown_data_type_fails() {
    let json = score_base_json(r#""dataType": "UNKNOWN_TYPE", "value": 1.0"#);
    let result = serde_json::from_str::<ScoreV3>(&json);
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════════════
//  CreateScoreValue — untagged union
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn create_score_value_numeric() {
    let val = CreateScoreValue::Numeric(0.75);
    let json = serde_json::to_value(&val).unwrap();
    assert_eq!(json, 0.75);
}

#[test]
fn create_score_value_string() {
    let val = CreateScoreValue::String("positive".into());
    let json = serde_json::to_value(&val).unwrap();
    assert_eq!(json, "positive");
}

#[test]
fn create_score_value_deserialize_number() {
    let val: CreateScoreValue = serde_json::from_str("42.0").unwrap();
    match val {
        CreateScoreValue::Numeric(n) => assert!((n - 42.0).abs() < f64::EPSILON),
        other => panic!("Expected Numeric, got {other:?}"),
    }
}

#[test]
fn create_score_value_deserialize_string() {
    let val: CreateScoreValue = serde_json::from_str(r#""hello""#).unwrap();
    match val {
        CreateScoreValue::String(s) => assert_eq!(s, "hello"),
        other => panic!("Expected String, got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  CreateScoreRequest serialization
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn create_score_request_minimal() {
    let req = CreateScoreRequest {
        name: "accuracy".into(),
        value: CreateScoreValue::Numeric(0.9),
        id: None,
        trace_id: None,
        session_id: None,
        observation_id: None,
        dataset_run_id: None,
        comment: None,
        metadata: None,
        environment: None,
        queue_id: None,
        data_type: None,
        config_id: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["name"], "accuracy");
    assert_eq!(json["value"], 0.9);
    let obj = json.as_object().unwrap();
    assert!(!obj.contains_key("id"));
    assert!(!obj.contains_key("traceId"));
    assert!(!obj.contains_key("sessionId"));
    assert!(!obj.contains_key("comment"));
    assert!(!obj.contains_key("dataType"));
}

#[test]
fn create_score_request_full() {
    let req = CreateScoreRequest {
        name: "sentiment".into(),
        value: CreateScoreValue::String("positive".into()),
        id: Some("custom-id".into()),
        trace_id: Some("t1".into()),
        session_id: Some("sess-1".into()),
        observation_id: Some("obs-1".into()),
        dataset_run_id: Some("run-1".into()),
        comment: Some("manual review".into()),
        metadata: Some(serde_json::json!({"reviewer": "bob"})),
        environment: Some("staging".into()),
        queue_id: Some("q1".into()),
        data_type: Some(ScoreDataType::Categorical),
        config_id: Some("cfg-1".into()),
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["id"], "custom-id");
    assert_eq!(json["traceId"], "t1");
    assert_eq!(json["sessionId"], "sess-1");
    assert_eq!(json["observationId"], "obs-1");
    assert_eq!(json["datasetRunId"], "run-1");
    assert_eq!(json["comment"], "manual review");
    assert_eq!(json["metadata"]["reviewer"], "bob");
    assert_eq!(json["environment"], "staging");
    assert_eq!(json["queueId"], "q1");
    assert_eq!(json["dataType"], "CATEGORICAL");
    assert_eq!(json["configId"], "cfg-1");
    assert_eq!(json["value"], "positive");
}

// ═══════════════════════════════════════════════════════════════════════
//  Prompts — tagged union on "type"
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn text_prompt_full_fields() {
    let json = r#"{
        "type": "text",
        "name": "greeter",
        "version": 5,
        "config": {"temperature": 0.5, "maxTokens": 200},
        "labels": ["production", "v5"],
        "tags": ["greeting", "en"],
        "commitMessage": "Added greeting template",
        "resolutionGraph": {"deps": []},
        "prompt": "Hello {{name}}, welcome to {{place}}!"
    }"#;
    let prompt: Prompt = serde_json::from_str(json).unwrap();
    match &prompt {
        Prompt::Text { base, prompt } => {
            assert_eq!(base.name, "greeter");
            assert_eq!(base.version, 5);
            assert_eq!(base.labels, vec!["production", "v5"]);
            assert_eq!(base.tags, vec!["greeting", "en"]);
            assert_eq!(base.commit_message.as_deref(), Some("Added greeting template"));
            assert!(base.resolution_graph.is_some());
            assert!(base.config.is_object());
            assert_eq!(prompt, "Hello {{name}}, welcome to {{place}}!");
        }
        _ => panic!("Expected Text"),
    }
}

#[test]
fn chat_prompt_multiple_messages() {
    let json = r#"{
        "type": "chat",
        "name": "multi-turn",
        "version": 1,
        "config": {},
        "labels": [],
        "tags": [],
        "prompt": [
            {"role": "system", "content": "You are a math tutor."},
            {"role": "user", "content": "What is 2+2?"},
            {"role": "assistant", "content": "4"},
            {"role": "user", "content": "And 3+3?"}
        ]
    }"#;
    let prompt: Prompt = serde_json::from_str(json).unwrap();
    match &prompt {
        Prompt::Chat { prompt, .. } => {
            assert_eq!(prompt.len(), 4);
            assert_eq!(prompt[0].role, "system");
            assert_eq!(prompt[1].role, "user");
            assert_eq!(prompt[2].role, "assistant");
            assert_eq!(prompt[2].content, "4");
            assert_eq!(prompt[3].content, "And 3+3?");
        }
        _ => panic!("Expected Chat"),
    }
}

#[test]
fn chat_prompt_empty_messages() {
    let json = r#"{
        "type": "chat",
        "name": "empty",
        "version": 1,
        "config": {},
        "labels": [],
        "tags": [],
        "prompt": []
    }"#;
    let prompt: Prompt = serde_json::from_str(json).unwrap();
    match &prompt {
        Prompt::Chat { prompt, .. } => assert!(prompt.is_empty()),
        _ => panic!("Expected Chat"),
    }
}

#[test]
fn serialize_create_chat_prompt() {
    let req = CreatePromptRequest::Chat {
        name: "bot".into(),
        prompt: vec![
            ChatMessage {
                role: "system".into(),
                content: "Be helpful".into(),
            },
        ],
        config: Some(serde_json::json!({"temperature": 0.7})),
        labels: None,
        tags: Some(vec!["v1".into()]),
        commit_message: Some("initial".into()),
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["type"], "chat");
    assert_eq!(json["name"], "bot");
    assert_eq!(json["prompt"][0]["role"], "system");
    assert_eq!(json["config"]["temperature"], 0.7);
    assert!(!json.as_object().unwrap().contains_key("labels"));
    assert_eq!(json["tags"][0], "v1");
    assert_eq!(json["commitMessage"], "initial");
}

#[test]
fn serialize_create_text_prompt_minimal() {
    let req = CreatePromptRequest::Text {
        name: "simple".into(),
        prompt: "Say hello".into(),
        config: None,
        labels: None,
        tags: None,
        commit_message: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["type"], "text");
    assert_eq!(json["name"], "simple");
    assert_eq!(json["prompt"], "Say hello");
    let obj = json.as_object().unwrap();
    assert!(!obj.contains_key("config"));
    assert!(!obj.contains_key("labels"));
    assert!(!obj.contains_key("tags"));
    assert!(!obj.contains_key("commitMessage"));
}

// ═══════════════════════════════════════════════════════════════════════
//  Health
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn health_response() {
    let json = r#"{"status": "OK", "version": "2.0.1"}"#;
    let h: HealthResponse = serde_json::from_str(json).unwrap();
    assert_eq!(h.status, "OK");
    assert_eq!(h.version, "2.0.1");
}

#[test]
fn health_response_ignores_unknown_fields() {
    let json = r#"{"status": "OK", "version": "2.0.1", "extra": true}"#;
    let h: HealthResponse = serde_json::from_str(json).unwrap();
    assert_eq!(h.status, "OK");
}

// ═══════════════════════════════════════════════════════════════════════
//  Observations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn observation_v2_full() {
    let json = r#"{
        "id": "obs-1",
        "traceId": "trace-1",
        "projectId": "proj-1",
        "type": "GENERATION",
        "startTime": "2024-06-15T10:00:00Z",
        "endTime": "2024-06-15T10:00:05Z",
        "parentObservationId": "obs-parent",
        "isRootObservation": false,
        "name": "llm-call",
        "level": "DEFAULT",
        "statusMessage": null,
        "version": "1.0",
        "environment": "production",
        "bookmarked": true,
        "public": false,
        "userId": "user-1",
        "sessionId": "sess-1",
        "completionStartTime": "2024-06-15T10:00:01Z",
        "createdAt": "2024-06-15T10:00:00Z",
        "updatedAt": "2024-06-15T10:00:05Z",
        "input": {"role": "user", "content": "hi"},
        "output": {"role": "assistant", "content": "hello"},
        "metadata": {"custom": "value"},
        "model": "claude-3-sonnet",
        "modelId": "model-123",
        "internalModelId": "internal-456",
        "modelParameters": {"temperature": 0.7},
        "usageDetails": {"input": 10, "output": 20},
        "costDetails": {"input": 0.001, "output": 0.002},
        "totalCost": 0.003,
        "usagePricingTierName": null,
        "inputPrice": "0.001",
        "outputPrice": "0.002",
        "totalPrice": "0.003",
        "promptId": "prompt-1",
        "promptName": "my-prompt",
        "promptVersion": 3,
        "latency": 5.0,
        "timeToFirstToken": 1.2,
        "traceName": "my-trace",
        "tags": ["demo", "test"],
        "release": "v1.0.0"
    }"#;
    let obs: ObservationV2 = serde_json::from_str(json).unwrap();
    assert_eq!(obs.id, "obs-1");
    assert_eq!(obs.trace_id.as_deref(), Some("trace-1"));
    assert_eq!(obs.project_id, "proj-1");
    assert_eq!(obs.r#type, "GENERATION");
    assert!(!obs.is_root_observation.unwrap());
    assert_eq!(obs.name.as_deref(), Some("llm-call"));
    assert_eq!(obs.level, Some(ObservationLevel::Default));
    assert!(obs.status_message.is_none());
    assert!(obs.bookmarked.unwrap());
    assert_eq!(obs.model.as_deref(), Some("claude-3-sonnet"));
    assert!((obs.total_cost.unwrap() - 0.003).abs() < f64::EPSILON);
    assert_eq!(obs.prompt_version, Some(3));
    assert!((obs.latency.unwrap() - 5.0).abs() < f64::EPSILON);
    assert!((obs.time_to_first_token.unwrap() - 1.2).abs() < f64::EPSILON);
    assert_eq!(obs.tags.as_ref().unwrap().len(), 2);
    assert_eq!(obs.release.as_deref(), Some("v1.0.0"));
}

#[test]
fn observation_v2_minimal() {
    let json = r#"{
        "id": "obs-min",
        "projectId": "proj-1",
        "type": "SPAN",
        "startTime": "2024-01-01T00:00:00Z"
    }"#;
    let obs: ObservationV2 = serde_json::from_str(json).unwrap();
    assert_eq!(obs.id, "obs-min");
    assert_eq!(obs.r#type, "SPAN");
    assert!(obs.trace_id.is_none());
    assert!(obs.end_time.is_none());
    assert!(obs.name.is_none());
    assert!(obs.level.is_none());
    assert!(obs.model.is_none());
    assert!(obs.total_cost.is_none());
    assert!(obs.tags.is_none());
}

// ═══════════════════════════════════════════════════════════════════════
//  Datasets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn dataset_deserialize() {
    let json = r#"{
        "id": "ds-1",
        "name": "eval-set",
        "description": "Evaluation dataset",
        "metadata": {"version": 2},
        "inputSchema": {"type": "object"},
        "expectedOutputSchema": null,
        "projectId": "proj-1",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-06-01T00:00:00Z"
    }"#;
    let ds: Dataset = serde_json::from_str(json).unwrap();
    assert_eq!(ds.id, "ds-1");
    assert_eq!(ds.name, "eval-set");
    assert_eq!(ds.description.as_deref(), Some("Evaluation dataset"));
    assert!(ds.metadata.is_some());
    assert!(ds.input_schema.is_some());
    assert!(ds.expected_output_schema.is_none());
}

#[test]
fn dataset_minimal() {
    let json = r#"{
        "id": "ds-2",
        "name": "bare",
        "projectId": "proj-1",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z"
    }"#;
    let ds: Dataset = serde_json::from_str(json).unwrap();
    assert_eq!(ds.name, "bare");
    assert!(ds.description.is_none());
    assert!(ds.metadata.is_none());
}

#[test]
fn serialize_create_dataset_request_minimal() {
    let req = CreateDatasetRequest {
        name: "my-dataset".into(),
        description: None,
        metadata: None,
        input_schema: None,
        expected_output_schema: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json, serde_json::json!({"name": "my-dataset"}));
}

#[test]
fn serialize_create_dataset_request_full() {
    let req = CreateDatasetRequest {
        name: "full-ds".into(),
        description: Some("A dataset".into()),
        metadata: Some(serde_json::json!({"key": "val"})),
        input_schema: Some(serde_json::json!({"type": "string"})),
        expected_output_schema: Some(serde_json::json!({"type": "number"})),
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["name"], "full-ds");
    assert_eq!(json["description"], "A dataset");
    assert_eq!(json["inputSchema"]["type"], "string");
    assert_eq!(json["expectedOutputSchema"]["type"], "number");
}

// ═══════════════════════════════════════════════════════════════════════
//  Dataset Items
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn dataset_item_deserialize() {
    let json = r#"{
        "id": "di-1",
        "status": "ACTIVE",
        "input": {"question": "What is 2+2?"},
        "expectedOutput": {"answer": "4"},
        "metadata": null,
        "sourceTraceId": "trace-src",
        "sourceObservationId": null,
        "datasetId": "ds-1",
        "datasetName": "eval-set",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z"
    }"#;
    let item: DatasetItem = serde_json::from_str(json).unwrap();
    assert_eq!(item.id, "di-1");
    assert_eq!(item.status, DatasetStatus::Active);
    assert!(item.input.is_some());
    assert!(item.expected_output.is_some());
    assert!(item.metadata.is_none());
    assert_eq!(item.source_trace_id.as_deref(), Some("trace-src"));
    assert!(item.source_observation_id.is_none());
    assert_eq!(item.dataset_name, "eval-set");
}

#[test]
fn dataset_item_archived_status() {
    let json = r#"{
        "id": "di-2",
        "status": "ARCHIVED",
        "datasetId": "ds-1",
        "datasetName": "eval-set",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z"
    }"#;
    let item: DatasetItem = serde_json::from_str(json).unwrap();
    assert_eq!(item.status, DatasetStatus::Archived);
}

#[test]
fn serialize_create_dataset_item_minimal() {
    let req = CreateDatasetItemRequest {
        dataset_name: "my-ds".into(),
        input: None,
        expected_output: None,
        metadata: None,
        source_trace_id: None,
        source_observation_id: None,
        id: None,
        status: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json, serde_json::json!({"datasetName": "my-ds"}));
}

#[test]
fn serialize_create_dataset_item_full() {
    let req = CreateDatasetItemRequest {
        dataset_name: "ds".into(),
        input: Some(serde_json::json!("hello")),
        expected_output: Some(serde_json::json!("world")),
        metadata: Some(serde_json::json!({"source": "manual"})),
        source_trace_id: Some("t1".into()),
        source_observation_id: Some("o1".into()),
        id: Some("custom-id".into()),
        status: Some(DatasetStatus::Active),
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["datasetName"], "ds");
    assert_eq!(json["input"], "hello");
    assert_eq!(json["expectedOutput"], "world");
    assert_eq!(json["sourceTraceId"], "t1");
    assert_eq!(json["sourceObservationId"], "o1");
    assert_eq!(json["id"], "custom-id");
    assert_eq!(json["status"], "ACTIVE");
}

// ═══════════════════════════════════════════════════════════════════════
//  Score Configs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn score_config_deserialize() {
    let json = r#"{
        "id": "cfg-1",
        "name": "sentiment",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z",
        "projectId": "proj-1",
        "dataType": "CATEGORICAL",
        "isArchived": false,
        "minValue": null,
        "maxValue": null,
        "categories": [
            {"value": 1.0, "label": "positive"},
            {"value": 0.0, "label": "negative"},
            {"value": 0.5, "label": "neutral"}
        ],
        "description": "Sentiment scoring"
    }"#;
    let cfg: ScoreConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.id, "cfg-1");
    assert_eq!(cfg.name, "sentiment");
    assert_eq!(cfg.data_type, ScoreConfigDataType::Categorical);
    assert!(!cfg.is_archived);
    assert!(cfg.min_value.is_none());
    let cats = cfg.categories.unwrap();
    assert_eq!(cats.len(), 3);
    assert_eq!(cats[0].label, "positive");
    assert!((cats[0].value - 1.0).abs() < f64::EPSILON);
    assert_eq!(cfg.description.as_deref(), Some("Sentiment scoring"));
}

#[test]
fn score_config_numeric_with_range() {
    let json = r#"{
        "id": "cfg-2",
        "name": "accuracy",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z",
        "projectId": "proj-1",
        "dataType": "NUMERIC",
        "isArchived": true,
        "minValue": 0.0,
        "maxValue": 1.0
    }"#;
    let cfg: ScoreConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.data_type, ScoreConfigDataType::Numeric);
    assert!(cfg.is_archived);
    assert!((cfg.min_value.unwrap() - 0.0).abs() < f64::EPSILON);
    assert!((cfg.max_value.unwrap() - 1.0).abs() < f64::EPSILON);
    assert!(cfg.categories.is_none());
}

#[test]
fn serialize_create_score_config() {
    let req = CreateScoreConfigRequest {
        name: "quality".into(),
        data_type: ScoreConfigDataType::Numeric,
        categories: None,
        min_value: Some(0.0),
        max_value: Some(10.0),
        description: Some("Quality score".into()),
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["name"], "quality");
    assert_eq!(json["dataType"], "NUMERIC");
    assert!(!json.as_object().unwrap().contains_key("categories"));
    assert_eq!(json["minValue"], 0.0);
    assert_eq!(json["maxValue"], 10.0);
}

// ═══════════════════════════════════════════════════════════════════════
//  Models
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn model_deserialize() {
    let json = r#"{
        "id": "model-1",
        "modelName": "gpt-4o",
        "matchPattern": "(?i)^(gpt-4o)$",
        "startDate": "2024-05-01",
        "unit": "TOKENS",
        "inputPrice": 0.005,
        "outputPrice": 0.015,
        "totalPrice": null
    }"#;
    let m: Model = serde_json::from_str(json).unwrap();
    assert_eq!(m.id, "model-1");
    assert_eq!(m.model_name, "gpt-4o");
    assert_eq!(m.match_pattern, "(?i)^(gpt-4o)$");
    assert_eq!(m.start_date.unwrap().to_string(), "2024-05-01");
    assert_eq!(m.unit, Some(ModelUsageUnit::Tokens));
    assert!((m.input_price.unwrap() - 0.005).abs() < f64::EPSILON);
    assert!((m.output_price.unwrap() - 0.015).abs() < f64::EPSILON);
    assert!(m.total_price.is_none());
}

#[test]
fn model_minimal() {
    let json = r#"{
        "id": "model-2",
        "modelName": "custom-llm",
        "matchPattern": "custom-.*"
    }"#;
    let m: Model = serde_json::from_str(json).unwrap();
    assert!(m.start_date.is_none());
    assert!(m.unit.is_none());
    assert!(m.input_price.is_none());
}

#[test]
fn serialize_create_model_request() {
    let req = CreateModelRequest {
        model_name: "my-model".into(),
        match_pattern: "my-model-.*".into(),
        unit: ModelUsageUnit::Characters,
        start_date: None,
        input_price: Some(0.01),
        output_price: Some(0.02),
        total_price: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["modelName"], "my-model");
    assert_eq!(json["matchPattern"], "my-model-.*");
    assert_eq!(json["unit"], "CHARACTERS");
    assert!(!json.as_object().unwrap().contains_key("startDate"));
    assert_eq!(json["inputPrice"], 0.01);
    assert!(!json.as_object().unwrap().contains_key("totalPrice"));
}

// ═══════════════════════════════════════════════════════════════════════
//  Skills
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn skill_version_deserialize() {
    let json = r#"{
        "id": "sv-1",
        "createdAt": "2024-06-01T00:00:00Z",
        "updatedAt": "2024-06-01T00:00:00Z",
        "projectId": "proj-1",
        "createdBy": "user-1",
        "name": "pragmatic-coding",
        "description": "Coding best practices",
        "frontmatter": {"category": "engineering"},
        "version": 3,
        "tags": ["coding", "best-practices"],
        "labels": ["latest", "production"],
        "commitMessage": "Updated habits section",
        "files": [
            {
                "id": "f-1",
                "path": "SKILL.md",
                "blobId": "blob-1",
                "sha256Hash": "abc123",
                "contentType": "text/plain",
                "contentLength": 7365,
                "content": "Pragmatic Coding guide..."
            },
            {
                "id": "f-2",
                "path": "references/examples.txt",
                "blobId": "blob-2",
                "sha256Hash": "def456",
                "contentType": "text/plain",
                "contentLength": 13644,
                "content": null
            }
        ]
    }"#;
    let sv: SkillVersion = serde_json::from_str(json).unwrap();
    assert_eq!(sv.id, "sv-1");
    assert_eq!(sv.name, "pragmatic-coding");
    assert_eq!(sv.description.as_deref(), Some("Coding best practices"));
    assert!(sv.frontmatter.is_some());
    assert_eq!(sv.version, 3);
    assert_eq!(sv.tags, vec!["coding", "best-practices"]);
    assert_eq!(sv.labels, vec!["latest", "production"]);
    assert_eq!(sv.commit_message.as_deref(), Some("Updated habits section"));
    assert_eq!(sv.files.len(), 2);

    let f1 = &sv.files[0];
    assert_eq!(f1.path, "SKILL.md");
    assert_eq!(f1.content_type, "text/plain");
    assert_eq!(f1.content_length, 7365);
    assert!(f1.content.is_some());

    let f2 = &sv.files[1];
    assert_eq!(f2.path, "references/examples.txt");
    assert!(f2.content.is_none());
}

#[test]
fn skill_version_no_optional_fields() {
    let json = r#"{
        "id": "sv-2",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z",
        "projectId": "proj-1",
        "createdBy": "user-1",
        "name": "bare-skill",
        "version": 1,
        "tags": [],
        "labels": ["latest"],
        "files": []
    }"#;
    let sv: SkillVersion = serde_json::from_str(json).unwrap();
    assert_eq!(sv.name, "bare-skill");
    assert!(sv.description.is_none());
    assert!(sv.frontmatter.is_none());
    assert!(sv.commit_message.is_none());
    assert!(sv.files.is_empty());
}

#[test]
fn serialize_create_skill_version_request() {
    let req = CreateSkillVersionRequest {
        files: vec![
            SkillVersionFileInput {
                path: "SKILL.md".into(),
                content: "# My Skill\nDo things well.".into(),
            },
            SkillVersionFileInput {
                path: "refs/guide.md".into(),
                content: "Guide content here".into(),
            },
        ],
        commit_message: Some("Initial version".into()),
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["files"].as_array().unwrap().len(), 2);
    assert_eq!(json["files"][0]["path"], "SKILL.md");
    assert_eq!(json["files"][1]["path"], "refs/guide.md");
    assert_eq!(json["commitMessage"], "Initial version");
}

#[test]
fn serialize_create_skill_version_no_commit_message() {
    let req = CreateSkillVersionRequest {
        files: vec![SkillVersionFileInput {
            path: "SKILL.md".into(),
            content: "content".into(),
        }],
        commit_message: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert!(!json.as_object().unwrap().contains_key("commitMessage"));
}

// ═══════════════════════════════════════════════════════════════════════
//  Projects
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn project_deserialize() {
    let json = r#"{
        "id": "proj-1",
        "name": "My Project",
        "organization": {"id": "org-1", "name": "My Org"},
        "metadata": {"tier": "pro"},
        "retentionDays": 90
    }"#;
    let p: Project = serde_json::from_str(json).unwrap();
    assert_eq!(p.id, "proj-1");
    assert_eq!(p.name, "My Project");
    assert_eq!(p.organization.id, "org-1");
    assert_eq!(p.organization.name, "My Org");
    assert!(p.metadata.is_some());
    assert_eq!(p.retention_days, Some(90));
}

#[test]
fn project_minimal() {
    let json = r#"{
        "id": "proj-2",
        "name": "Bare",
        "organization": {"id": "org-1", "name": "Org"}
    }"#;
    let p: Project = serde_json::from_str(json).unwrap();
    assert!(p.metadata.is_none());
    assert!(p.retention_days.is_none());
}

// ═══════════════════════════════════════════════════════════════════════
//  Traces
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn delete_trace_response_deserialize() {
    let json = r#"{"message": "Trace deleted successfully"}"#;
    let r: DeleteTraceResponse = serde_json::from_str(json).unwrap();
    assert_eq!(r.message, "Trace deleted successfully");
}

#[test]
fn serialize_delete_traces_request() {
    let req = DeleteTracesRequest {
        trace_ids: vec!["t1".into(), "t2".into(), "t3".into()],
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["traceIds"], serde_json::json!(["t1", "t2", "t3"]));
}

// ═══════════════════════════════════════════════════════════════════════
//  Media
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn media_response_deserialize() {
    let json = r#"{
        "mediaId": "media-1",
        "contentType": "image/png",
        "contentLength": 102400,
        "uploadedAt": "2024-06-15T12:00:00Z",
        "url": "https://storage.example.com/media-1.png",
        "urlExpiry": "2024-06-15T13:00:00Z"
    }"#;
    let m: GetMediaResponse = serde_json::from_str(json).unwrap();
    assert_eq!(m.media_id, "media-1");
    assert_eq!(m.content_type, "image/png");
    assert_eq!(m.content_length, 102400);
    assert_eq!(m.url, "https://storage.example.com/media-1.png");
}

// ═══════════════════════════════════════════════════════════════════════
//  Ingestion (OTel) — serialization
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn serialize_ingest_traces_request() {
    let req = IngestTracesRequest {
        resource_spans: vec![OtelResourceSpan {
            resource: Some(OtelResource {
                attributes: Some(vec![OtelKeyValue {
                    key: "service.name".into(),
                    value: OtelAnyValue {
                        string_value: Some("my-service".into()),
                        int_value: None,
                        double_value: None,
                        bool_value: None,
                    },
                }]),
            }),
            scope_spans: vec![OtelScopeSpan {
                scope: Some(OtelInstrumentationScope {
                    name: Some("langfuse-sdk".into()),
                    version: Some("0.1.0".into()),
                }),
                spans: vec![OtelSpan {
                    trace_id: "abc123".into(),
                    span_id: "span-1".into(),
                    parent_span_id: None,
                    name: "llm-call".into(),
                    start_time_unix_nano: "1718445600000000000".into(),
                    end_time_unix_nano: "1718445605000000000".into(),
                    attributes: Some(vec![
                        OtelKeyValue {
                            key: "gen_ai.request.model".into(),
                            value: OtelAnyValue {
                                string_value: Some("claude-3".into()),
                                int_value: None,
                                double_value: None,
                                bool_value: None,
                            },
                        },
                        OtelKeyValue {
                            key: "gen_ai.usage.input_tokens".into(),
                            value: OtelAnyValue {
                                string_value: None,
                                int_value: Some(150),
                                double_value: None,
                                bool_value: None,
                            },
                        },
                    ]),
                    status: Some(OtelStatus {
                        code: Some(1),
                        message: None,
                    }),
                }],
            }],
        }],
    };
    let json = serde_json::to_value(&req).unwrap();
    let span = &json["resourceSpans"][0]["scopeSpans"][0]["spans"][0];
    assert_eq!(span["traceId"], "abc123");
    assert_eq!(span["name"], "llm-call");
    assert_eq!(span["attributes"][0]["key"], "gen_ai.request.model");
    assert_eq!(span["attributes"][0]["value"]["stringValue"], "claude-3");
    assert_eq!(span["attributes"][1]["value"]["intValue"], 150);
    assert_eq!(span["status"]["code"], 1);
    assert!(span["status"]["message"].is_null());

    let resource = &json["resourceSpans"][0]["resource"];
    assert_eq!(resource["attributes"][0]["key"], "service.name");

    let scope = &json["resourceSpans"][0]["scopeSpans"][0]["scope"];
    assert_eq!(scope["name"], "langfuse-sdk");
}

#[test]
fn serialize_otel_any_value_only_populated_fields() {
    let val = OtelAnyValue {
        string_value: None,
        int_value: None,
        double_value: Some(3.14),
        bool_value: None,
    };
    let json = serde_json::to_value(&val).unwrap();
    let obj = json.as_object().unwrap();
    assert_eq!(obj.len(), 1);
    assert!((obj["doubleValue"].as_f64().unwrap() - 3.14).abs() < f64::EPSILON);
}

#[test]
fn serialize_otel_any_value_bool() {
    let val = OtelAnyValue {
        string_value: None,
        int_value: None,
        double_value: None,
        bool_value: Some(true),
    };
    let json = serde_json::to_value(&val).unwrap();
    assert_eq!(json, serde_json::json!({"boolValue": true}));
}

// ═══════════════════════════════════════════════════════════════════════
//  Pagination
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn paginated_response_with_health() {
    let json = r#"{
        "data": [
            {"status": "OK", "version": "2.0.0"},
            {"status": "OK", "version": "2.0.1"}
        ],
        "meta": {
            "page": 1,
            "limit": 10,
            "totalItems": 2,
            "totalPages": 1
        }
    }"#;
    let page: langfuse_sdk::Paginated<HealthResponse> = serde_json::from_str(json).unwrap();
    assert_eq!(page.data.len(), 2);
    assert_eq!(page.data[0].version, "2.0.0");
    assert_eq!(page.meta.page, 1);
    assert_eq!(page.meta.limit, 10);
    assert_eq!(page.meta.total_items, 2);
    assert_eq!(page.meta.total_pages, 1);
}

#[test]
fn paginated_empty_data() {
    let json = r#"{
        "data": [],
        "meta": {"page": 1, "limit": 50, "totalItems": 0, "totalPages": 0}
    }"#;
    let page: langfuse_sdk::Paginated<HealthResponse> = serde_json::from_str(json).unwrap();
    assert!(page.data.is_empty());
    assert_eq!(page.meta.total_items, 0);
}

#[test]
fn cursor_paginated_with_next_cursor() {
    let json = r#"{
        "data": [{"status": "OK", "version": "1.0"}],
        "meta": {"cursor": "eyJpZCI6ImFiYyJ9"}
    }"#;
    let page: langfuse_sdk::CursorPaginated<HealthResponse> =
        serde_json::from_str(json).unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.meta.cursor.as_deref(), Some("eyJpZCI6ImFiYyJ9"));
}

#[test]
fn cursor_paginated_no_next_page() {
    let json = r#"{
        "data": [{"status": "OK", "version": "1.0"}],
        "meta": {"cursor": null}
    }"#;
    let page: langfuse_sdk::CursorPaginated<HealthResponse> =
        serde_json::from_str(json).unwrap();
    assert!(page.meta.cursor.is_none());
}

#[test]
fn cursor_paginated_missing_cursor_field() {
    let json = r#"{
        "data": [],
        "meta": {}
    }"#;
    let page: langfuse_sdk::CursorPaginated<HealthResponse> =
        serde_json::from_str(json).unwrap();
    assert!(page.meta.cursor.is_none());
}

// ═══════════════════════════════════════════════════════════════════════
//  Query params serialization (skip_serializing_if behavior)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn page_params_default_serializes_empty() {
    let params = langfuse_sdk::pagination::PageParams::default();
    let json = serde_json::to_value(&params).unwrap();
    assert_eq!(json, serde_json::json!({}));
}

#[test]
fn page_params_with_values() {
    let params = langfuse_sdk::pagination::PageParams {
        page: Some(2),
        limit: Some(25),
    };
    let json = serde_json::to_value(&params).unwrap();
    assert_eq!(json["page"], 2);
    assert_eq!(json["limit"], 25);
}

#[test]
fn cursor_params_default_serializes_empty() {
    let params = langfuse_sdk::pagination::CursorParams::default();
    let json = serde_json::to_value(&params).unwrap();
    assert_eq!(json, serde_json::json!({}));
}

#[test]
fn list_scores_params_default_serializes_empty() {
    let params = langfuse_sdk::api::scores::ListScoresParams::default();
    let json = serde_json::to_value(&params).unwrap();
    assert_eq!(json, serde_json::json!({}));
}

#[test]
fn list_scores_params_partial() {
    let params = langfuse_sdk::api::scores::ListScoresParams {
        limit: Some(100),
        trace_id: Some("t1".into()),
        ..Default::default()
    };
    let json = serde_json::to_value(&params).unwrap();
    let obj = json.as_object().unwrap();
    assert_eq!(obj.len(), 2);
    assert_eq!(json["limit"], 100);
    assert_eq!(json["traceId"], "t1");
}

#[test]
fn list_skills_params_serialization() {
    let params = langfuse_sdk::api::skills::ListSkillsParams {
        name: Some("pragmatic-coding".into()),
        page: Some(1),
        limit: Some(10),
        ..Default::default()
    };
    let json = serde_json::to_value(&params).unwrap();
    assert_eq!(json["name"], "pragmatic-coding");
    assert_eq!(json["page"], 1);
    assert_eq!(json["limit"], 10);
    assert!(!json.as_object().unwrap().contains_key("search"));
    assert!(!json.as_object().unwrap().contains_key("tag"));
}

#[test]
fn get_skill_params_serialization() {
    let params = langfuse_sdk::api::skills::GetSkillParams {
        label: Some("latest".into()),
        include_content: Some(true),
    };
    let json = serde_json::to_value(&params).unwrap();
    assert_eq!(json["label"], "latest");
    assert_eq!(json["includeContent"], true);
}

// ═══════════════════════════════════════════════════════════════════════
//  Edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn unknown_fields_in_response_are_ignored() {
    let json = r#"{
        "id": "ds-1",
        "name": "test",
        "projectId": "p",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z",
        "unknownField": "should be ignored",
        "anotherNew": 42
    }"#;
    let ds: Dataset = serde_json::from_str(json).unwrap();
    assert_eq!(ds.name, "test");
}

#[test]
fn null_vs_missing_optional_field() {
    let with_null = r#"{
        "id": "ds-1", "name": "a", "projectId": "p",
        "description": null,
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z"
    }"#;
    let without_field = r#"{
        "id": "ds-2", "name": "b", "projectId": "p",
        "createdAt": "2024-01-01T00:00:00Z",
        "updatedAt": "2024-01-01T00:00:00Z"
    }"#;
    let a: Dataset = serde_json::from_str(with_null).unwrap();
    let b: Dataset = serde_json::from_str(without_field).unwrap();
    assert!(a.description.is_none());
    assert!(b.description.is_none());
}

#[test]
fn timestamp_with_fractional_seconds() {
    let json = r#"{
        "id": "obs-tz",
        "projectId": "p",
        "type": "SPAN",
        "startTime": "2024-06-15T10:30:00.123456Z"
    }"#;
    let obs: ObservationV2 = serde_json::from_str(json).unwrap();
    assert_eq!(obs.id, "obs-tz");
}

#[test]
fn timestamp_with_timezone_offset() {
    let json = r#"{
        "id": "obs-tz2",
        "projectId": "p",
        "type": "SPAN",
        "startTime": "2024-06-15T10:30:00+05:30"
    }"#;
    let obs: ObservationV2 = serde_json::from_str(json).unwrap();
    assert_eq!(obs.id, "obs-tz2");
}

#[test]
fn large_json_value_metadata() {
    let json = r#"{
        "id": "obs-meta",
        "projectId": "p",
        "type": "SPAN",
        "startTime": "2024-01-01T00:00:00Z",
        "metadata": {
            "nested": {
                "deep": {
                    "array": [1, 2, {"key": "val"}]
                }
            },
            "list": [true, false, null, "string", 42]
        }
    }"#;
    let obs: ObservationV2 = serde_json::from_str(json).unwrap();
    let meta = obs.metadata.unwrap();
    assert!(meta["nested"]["deep"]["array"].is_array());
    assert_eq!(meta["list"][4], 42);
}

#[test]
fn create_score_response_deserialize() {
    let json = r#"{"id": "new-score-123"}"#;
    let r: CreateScoreResponse = serde_json::from_str(json).unwrap();
    assert_eq!(r.id, "new-score-123");
}

#[test]
fn chat_message_roundtrip() {
    let msg = ChatMessage {
        role: "user".into(),
        content: "Hello!".into(),
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["role"], "user");
    assert_eq!(json["content"], "Hello!");

    let back: ChatMessage = serde_json::from_value(json).unwrap();
    assert_eq!(back.role, "user");
    assert_eq!(back.content, "Hello!");
}

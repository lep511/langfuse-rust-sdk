//! Serde tests for the resources added for full OpenAPI conformance:
//! every enum variant (plus the `Unknown` fallback), tagged-union variants and
//! fields that need explicit renames.

use langfuse_rust_sdk::api::feedback::FeedbackTargetType;
use langfuse_rust_sdk::models::comments::CommentObjectType;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Round-trips every `(variant, wire value)` pair and checks the `Unknown` fallback.
fn assert_enum<T>(cases: &[(T, &str)], unknown: T)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    for (variant, wire) in cases {
        assert_eq!(serde_json::to_value(variant).unwrap(), *wire, "{variant:?}");
        let back: T = serde_json::from_value((*wire).into()).unwrap();
        assert_eq!(&back, variant);
    }
    let parsed: T = serde_json::from_str(r#""SOME_FUTURE_VALUE""#).unwrap();
    assert_eq!(parsed, unknown);
    assert!(serde_json::to_value(&unknown).is_err());
}

/// Checks serialization of a request-only enum.
fn assert_serialized<T: Serialize + std::fmt::Debug>(cases: &[(T, &str)]) {
    for (variant, wire) in cases {
        assert_eq!(serde_json::to_value(variant).unwrap(), *wire, "{variant:?}");
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Comments & Feedback
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn comment_object_type_values() {
    assert_enum(
        &[
            (CommentObjectType::Trace, "TRACE"),
            (CommentObjectType::Observation, "OBSERVATION"),
            (CommentObjectType::Session, "SESSION"),
            (CommentObjectType::Prompt, "PROMPT"),
        ],
        CommentObjectType::Unknown,
    );
}

#[test]
fn feedback_target_type_values() {
    assert_serialized(&[
        (FeedbackTargetType::Skill, "skill"),
        (FeedbackTargetType::McpTool, "mcp-tool"),
        (FeedbackTargetType::Cli, "cli"),
        (FeedbackTargetType::Docs, "docs"),
        (FeedbackTargetType::PublicApi, "public-api"),
        (FeedbackTargetType::Other, "other"),
    ]);
}

// ═══════════════════════════════════════════════════════════════════════
//  Annotation queues
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn annotation_queue_enums() {
    use langfuse_rust_sdk::models::annotation_queues::*;
    assert_enum(
        &[
            (AnnotationQueueStatus::Pending, "PENDING"),
            (AnnotationQueueStatus::Completed, "COMPLETED"),
        ],
        AnnotationQueueStatus::Unknown,
    );
    assert_enum(
        &[
            (AnnotationQueueObjectType::Trace, "TRACE"),
            (AnnotationQueueObjectType::Observation, "OBSERVATION"),
            (AnnotationQueueObjectType::Session, "SESSION"),
        ],
        AnnotationQueueObjectType::Unknown,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Blob storage integrations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn blob_storage_enums() {
    use langfuse_rust_sdk::models::blob_storage_integrations::*;
    assert_enum(
        &[
            (BlobStorageIntegrationType::S3, "S3"),
            (BlobStorageIntegrationType::S3Compatible, "S3_COMPATIBLE"),
            (
                BlobStorageIntegrationType::AzureBlobStorage,
                "AZURE_BLOB_STORAGE",
            ),
            (
                BlobStorageIntegrationType::GoogleCloudStorage,
                "GOOGLE_CLOUD_STORAGE",
            ),
        ],
        BlobStorageIntegrationType::Unknown,
    );
    assert_enum(
        &[
            (BlobStorageIntegrationAuthType::AccessKeys, "ACCESS_KEYS"),
            (BlobStorageIntegrationAuthType::RoleArn, "ROLE_ARN"),
        ],
        BlobStorageIntegrationAuthType::Unknown,
    );
    assert_enum(
        &[
            (BlobStorageIntegrationFileType::Json, "JSON"),
            (BlobStorageIntegrationFileType::Csv, "CSV"),
            (BlobStorageIntegrationFileType::Jsonl, "JSONL"),
            (BlobStorageIntegrationFileType::Parquet, "PARQUET"),
        ],
        BlobStorageIntegrationFileType::Unknown,
    );
    assert_enum(
        &[
            (BlobStorageExportMode::FullHistory, "FULL_HISTORY"),
            (BlobStorageExportMode::FromToday, "FROM_TODAY"),
            (BlobStorageExportMode::FromCustomDate, "FROM_CUSTOM_DATE"),
        ],
        BlobStorageExportMode::Unknown,
    );
    assert_enum(
        &[
            (
                BlobStorageExportFrequency::Every20Minutes,
                "every_20_minutes",
            ),
            (BlobStorageExportFrequency::Hourly, "hourly"),
            (BlobStorageExportFrequency::Daily, "daily"),
            (BlobStorageExportFrequency::Weekly, "weekly"),
        ],
        BlobStorageExportFrequency::Unknown,
    );
    assert_enum(
        &[
            (
                BlobStorageExportSource::LegacyTracesObservations,
                "LEGACY_TRACES_OBSERVATIONS",
            ),
            (BlobStorageExportSource::ObservationsV2, "OBSERVATIONS_V2"),
            (
                BlobStorageExportSource::LegacyTracesAndEnrichedObservations,
                "LEGACY_TRACES_AND_ENRICHED_OBSERVATIONS",
            ),
        ],
        BlobStorageExportSource::Unknown,
    );
    assert_enum(
        &[
            (BlobStorageExportFieldGroup::Core, "core"),
            (BlobStorageExportFieldGroup::Basic, "basic"),
            (BlobStorageExportFieldGroup::Time, "time"),
            (BlobStorageExportFieldGroup::Io, "io"),
            (BlobStorageExportFieldGroup::Metadata, "metadata"),
            (BlobStorageExportFieldGroup::Model, "model"),
            (BlobStorageExportFieldGroup::Usage, "usage"),
            (BlobStorageExportFieldGroup::Prompt, "prompt"),
            (BlobStorageExportFieldGroup::Metrics, "metrics"),
            (BlobStorageExportFieldGroup::Tools, "tools"),
            (BlobStorageExportFieldGroup::TraceContext, "trace_context"),
            (BlobStorageExportFieldGroup::ScoreMetadata, "score_metadata"),
            (
                BlobStorageExportFieldGroup::ScoreProvenance,
                "score_provenance",
            ),
        ],
        BlobStorageExportFieldGroup::Unknown,
    );
    assert_enum(
        &[
            (BlobStorageSyncStatus::Idle, "idle"),
            (BlobStorageSyncStatus::Running, "running"),
            (BlobStorageSyncStatus::Queued, "queued"),
            (BlobStorageSyncStatus::UpToDate, "up_to_date"),
            (BlobStorageSyncStatus::Disabled, "disabled"),
            (BlobStorageSyncStatus::Error, "error"),
        ],
        BlobStorageSyncStatus::Unknown,
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Evaluation rules & shared evaluation types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn evaluation_rule_filter_all_variants() {
    use langfuse_rust_sdk::models::evaluation_rules::*;
    use serde_json::json;
    let cases = [
        (
            EvaluationRuleFilter::DateTime {
                column: "startTime".into(),
                operator: NumberFilterOperator::GreaterThanOrEqual,
                value: "2024-06-15T10:30:00Z".parse().unwrap(),
            },
            json!({"type": "datetime", "column": "startTime", "operator": ">=", "value": "2024-06-15T10:30:00Z"}),
        ),
        (
            EvaluationRuleFilter::String {
                column: "userId".into(),
                operator: StringFilterOperator::DoesNotContain,
                value: "bot".into(),
            },
            json!({"type": "string", "column": "userId", "operator": "does not contain", "value": "bot"}),
        ),
        (
            EvaluationRuleFilter::Number {
                column: "toolCalls".into(),
                operator: NumberFilterOperator::GreaterThan,
                value: 2.0,
            },
            json!({"type": "number", "column": "toolCalls", "operator": ">", "value": 2.0}),
        ),
        (
            EvaluationRuleFilter::StringOptions {
                column: "type".into(),
                operator: OptionsFilterOperator::NoneOf,
                value: vec!["SPAN".into()],
            },
            json!({"type": "stringOptions", "column": "type", "operator": "none of", "value": ["SPAN"]}),
        ),
        (
            EvaluationRuleFilter::CategoryOptions {
                column: "scores".into(),
                key: "quality".into(),
                operator: OptionsFilterOperator::AnyOf,
                value: vec!["good".into()],
            },
            json!({"type": "categoryOptions", "column": "scores", "key": "quality", "operator": "any of", "value": ["good"]}),
        ),
        (
            EvaluationRuleFilter::ArrayOptions {
                column: "tags".into(),
                operator: ArrayOptionsFilterOperator::AllOf,
                value: vec!["a".into(), "b".into()],
            },
            json!({"type": "arrayOptions", "column": "tags", "operator": "all of", "value": ["a", "b"]}),
        ),
        (
            EvaluationRuleFilter::StringObject {
                column: "metadata".into(),
                key: "tier".into(),
                operator: StringObjectFilterOperator::IsSet,
                value: String::new(),
            },
            json!({"type": "stringObject", "column": "metadata", "key": "tier", "operator": "is set", "value": ""}),
        ),
        (
            EvaluationRuleFilter::NumberObject {
                column: "metadata".into(),
                key: "cost".into(),
                operator: NumberFilterOperator::LessThanOrEqual,
                value: 1.5,
            },
            json!({"type": "numberObject", "column": "metadata", "key": "cost", "operator": "<=", "value": 1.5}),
        ),
        (
            EvaluationRuleFilter::Boolean {
                column: "isRootObservation".into(),
                operator: BooleanFilterOperator::NotEquals,
                value: true,
            },
            json!({"type": "boolean", "column": "isRootObservation", "operator": "<>", "value": true}),
        ),
        (
            EvaluationRuleFilter::Null {
                column: "parentObservationId".into(),
                operator: NullFilterOperator::IsNotNull,
                value: EmptyValue,
            },
            json!({"type": "null", "column": "parentObservationId", "operator": "is not null", "value": ""}),
        ),
    ];
    for (filter, expected) in cases {
        assert_eq!(serde_json::to_value(&filter).unwrap(), expected);
    }
}

#[test]
fn evaluation_rule_operator_values() {
    use langfuse_rust_sdk::models::evaluation_rules::*;
    assert_serialized(&[
        (StringFilterOperator::Equals, "="),
        (StringFilterOperator::Contains, "contains"),
        (StringFilterOperator::DoesNotContain, "does not contain"),
        (StringFilterOperator::StartsWith, "starts with"),
        (StringFilterOperator::EndsWith, "ends with"),
    ]);
    assert_serialized(&[
        (StringObjectFilterOperator::Equals, "="),
        (StringObjectFilterOperator::Contains, "contains"),
        (
            StringObjectFilterOperator::DoesNotContain,
            "does not contain",
        ),
        (StringObjectFilterOperator::StartsWith, "starts with"),
        (StringObjectFilterOperator::EndsWith, "ends with"),
        (StringObjectFilterOperator::IsSet, "is set"),
        (StringObjectFilterOperator::IsNotSet, "is not set"),
    ]);
    assert_serialized(&[
        (NumberFilterOperator::Equals, "="),
        (NumberFilterOperator::GreaterThan, ">"),
        (NumberFilterOperator::LessThan, "<"),
        (NumberFilterOperator::GreaterThanOrEqual, ">="),
        (NumberFilterOperator::LessThanOrEqual, "<="),
    ]);
    assert_serialized(&[
        (OptionsFilterOperator::AnyOf, "any of"),
        (OptionsFilterOperator::NoneOf, "none of"),
    ]);
    assert_serialized(&[
        (ArrayOptionsFilterOperator::AnyOf, "any of"),
        (ArrayOptionsFilterOperator::NoneOf, "none of"),
        (ArrayOptionsFilterOperator::AllOf, "all of"),
    ]);
    assert_serialized(&[
        (BooleanFilterOperator::Equals, "="),
        (BooleanFilterOperator::NotEquals, "<>"),
    ]);
    assert_serialized(&[
        (NullFilterOperator::IsNull, "is null"),
        (NullFilterOperator::IsNotNull, "is not null"),
    ]);
}

#[test]
fn evaluation_common_enums() {
    use langfuse_rust_sdk::models::evaluation_common::*;
    assert_serialized(&[
        (PromptVariableMappingSource::Input, "input"),
        (PromptVariableMappingSource::Output, "output"),
        (PromptVariableMappingSource::Metadata, "metadata"),
        (PromptVariableMappingSource::ToolCalls, "tool_calls"),
        (
            PromptVariableMappingSource::ExpectedOutput,
            "expected_output",
        ),
        (
            PromptVariableMappingSource::ExperimentItemMetadata,
            "experiment_item_metadata",
        ),
    ]);
    assert_enum(
        &[
            (LegacyEvaluationObject::Trace, "trace"),
            (LegacyEvaluationObject::Span, "span"),
            (LegacyEvaluationObject::Generation, "generation"),
            (LegacyEvaluationObject::Event, "event"),
            (LegacyEvaluationObject::Agent, "agent"),
            (LegacyEvaluationObject::Tool, "tool"),
            (LegacyEvaluationObject::Chain, "chain"),
            (LegacyEvaluationObject::Retriever, "retriever"),
            (LegacyEvaluationObject::Evaluator, "evaluator"),
            (LegacyEvaluationObject::Embedding, "embedding"),
            (LegacyEvaluationObject::Guardrail, "guardrail"),
            (LegacyEvaluationObject::DatasetItem, "dataset_item"),
        ],
        LegacyEvaluationObject::Unknown,
    );
    assert_enum(
        &[
            (
                PublicApiErrorCode::AuthenticationFailed,
                "authentication_failed",
            ),
            (PublicApiErrorCode::AccessDenied, "access_denied"),
            (PublicApiErrorCode::InvalidRequest, "invalid_request"),
            (PublicApiErrorCode::InvalidQuery, "invalid_query"),
            (PublicApiErrorCode::InvalidBody, "invalid_body"),
            (PublicApiErrorCode::ResourceNotFound, "resource_not_found"),
            (PublicApiErrorCode::Conflict, "conflict"),
            (PublicApiErrorCode::RateLimited, "rate_limited"),
            (PublicApiErrorCode::MethodNotAllowed, "method_not_allowed"),
            (PublicApiErrorCode::InternalError, "internal_error"),
        ],
        PublicApiErrorCode::Unknown,
    );
}

#[test]
fn public_api_error_with_validation_issues() {
    use langfuse_rust_sdk::models::evaluation_common::*;
    let err: PublicApiError = serde_json::from_str(
        r#"{"message":"Invalid body","code":"invalid_body",
            "details":{"issues":[{"code":"invalid_type","message":"Expected string","path":["filter",0,"value"]}]}}"#,
    )
    .unwrap();
    let issue = &err.details.unwrap().issues.unwrap()[0];
    assert_eq!(
        issue.path,
        vec![serde_json::json!("filter"), 0.into(), "value".into()]
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Evaluators
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn evaluator_enums() {
    use langfuse_rust_sdk::models::evaluators::*;
    assert_enum(
        &[
            (EvaluatorStatus::Active, "active"),
            (EvaluatorStatus::Paused, "paused"),
        ],
        EvaluatorStatus::Unknown,
    );
    assert_enum(
        &[
            (CodeEvaluatorSourceCodeLanguage::Python, "PYTHON"),
            (CodeEvaluatorSourceCodeLanguage::Typescript, "TYPESCRIPT"),
        ],
        CodeEvaluatorSourceCodeLanguage::Unknown,
    );
    assert_enum(
        &[
            (EvaluatorChatMessageRole::System, "system"),
            (EvaluatorChatMessageRole::User, "user"),
            (EvaluatorChatMessageRole::Assistant, "assistant"),
        ],
        EvaluatorChatMessageRole::Unknown,
    );
}

#[test]
fn evaluator_output_definition_variants() {
    use langfuse_rust_sdk::models::evaluators::*;
    use serde_json::json;
    let cases = [
        (json!({"dataType": "NUMERIC", "minValue": 0.0}), "NUMERIC"),
        (
            json!({"dataType": "BOOLEAN", "scoreValueInstructions": "true if helpful"}),
            "BOOLEAN",
        ),
        (
            json!({"dataType": "CATEGORICAL", "categories": ["a", "b"], "shouldAllowMultipleMatches": true}),
            "CATEGORICAL",
        ),
    ];
    for (wire, data_type) in cases {
        let parsed: EvaluatorOutputDefinition = serde_json::from_value(wire.clone()).unwrap();
        let back = serde_json::to_value(&parsed).unwrap();
        assert_eq!(back, wire, "{data_type}");
    }
}

#[test]
fn update_evaluator_description_null_vs_omitted() {
    use langfuse_rust_sdk::models::evaluators::*;
    use serde_json::json;
    let clear = UpdateEvaluatorRequest::Metadata(UpdateEvaluatorMetadataRequest {
        name: Some("New".into()),
        description: Some(None),
    });
    assert_eq!(
        serde_json::to_value(&clear).unwrap(),
        json!({"name": "New", "description": null})
    );

    let keep = UpdateEvaluatorRequest::Metadata(UpdateEvaluatorMetadataRequest {
        name: Some("New".into()),
        description: None,
    });
    assert_eq!(serde_json::to_value(&keep).unwrap(), json!({"name": "New"}));

    let set = UpdateEvaluatorRequest::Metadata(UpdateEvaluatorMetadataRequest {
        name: None,
        description: Some(Some("d".into())),
    });
    assert_eq!(
        serde_json::to_value(&set).unwrap(),
        json!({"description": "d"})
    );
}

#[test]
fn update_llm_as_judge_request_has_type_tag() {
    use langfuse_rust_sdk::models::evaluators::*;
    use serde_json::json;
    let req = UpdateEvaluatorRequest::LlmAsJudge(UpdateLlmAsJudgeEvaluatorRequest {
        name: None,
        description: None,
        prompt: EvaluatorChatPromptInput::Messages(vec![EvaluatorChatMessage {
            role: EvaluatorChatMessageRole::User,
            content: "Judge {{output}}".into(),
        }]),
        model_config: Some(EvaluatorModelConfig {
            provider: "anthropic".into(),
            model: "claude-sonnet".into(),
        }),
        variable_mapping: None,
        output_definition: EvaluatorOutputDefinition::Boolean {
            score_reasoning_instructions: None,
            score_value_instructions: None,
        },
    });
    assert_eq!(
        serde_json::to_value(&req).unwrap(),
        json!({
            "type": "llm_as_judge",
            "prompt": [{"role": "user", "content": "Judge {{output}}"}],
            "modelConfig": {"provider": "anthropic", "model": "claude-sonnet"},
            "outputDefinition": {"dataType": "BOOLEAN"}
        })
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  LLM connections
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn llm_adapter_values() {
    use langfuse_rust_sdk::models::llm_connections::LlmAdapter;
    assert_serialized(&[
        (LlmAdapter::Anthropic, "anthropic"),
        (LlmAdapter::Openai, "openai"),
        (LlmAdapter::Azure, "azure"),
        (LlmAdapter::Bedrock, "bedrock"),
        (LlmAdapter::GoogleVertexAi, "google-vertex-ai"),
        (LlmAdapter::GoogleAiStudio, "google-ai-studio"),
    ]);
}

#[test]
fn upsert_llm_connection_minimal_body() {
    use langfuse_rust_sdk::models::llm_connections::*;
    let req = UpsertLlmConnectionRequest::new("bedrock", LlmAdapter::Bedrock, "key");
    assert_eq!(
        serde_json::to_value(&req).unwrap(),
        serde_json::json!({"provider": "bedrock", "adapter": "bedrock", "secretKey": "key"})
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Metrics
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn metrics_enums() {
    use langfuse_rust_sdk::models::metrics::*;
    assert_serialized(&[
        (MetricsView::Observations, "observations"),
        (MetricsView::ScoresNumeric, "scores-numeric"),
        (MetricsView::ScoresBoolean, "scores-boolean"),
        (MetricsView::ScoresCategorical, "scores-categorical"),
    ]);
    assert_serialized(&[
        (MetricsAggregation::Sum, "sum"),
        (MetricsAggregation::Avg, "avg"),
        (MetricsAggregation::Count, "count"),
        (MetricsAggregation::Max, "max"),
        (MetricsAggregation::Min, "min"),
        (MetricsAggregation::P50, "p50"),
        (MetricsAggregation::P75, "p75"),
        (MetricsAggregation::P90, "p90"),
        (MetricsAggregation::P95, "p95"),
        (MetricsAggregation::P99, "p99"),
        (MetricsAggregation::Histogram, "histogram"),
    ]);
    assert_serialized(&[
        (MetricsGranularity::Auto, "auto"),
        (MetricsGranularity::Minute, "minute"),
        (MetricsGranularity::Hour, "hour"),
        (MetricsGranularity::Day, "day"),
        (MetricsGranularity::Week, "week"),
        (MetricsGranularity::Month, "month"),
    ]);
    assert_serialized(&[
        (MetricsSortDirection::Asc, "asc"),
        (MetricsSortDirection::Desc, "desc"),
    ]);
}

// ═══════════════════════════════════════════════════════════════════════
//  Organizations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn membership_role_values() {
    use langfuse_rust_sdk::models::organizations::MembershipRole;
    assert_enum(
        &[
            (MembershipRole::Owner, "OWNER"),
            (MembershipRole::Admin, "ADMIN"),
            (MembershipRole::Member, "MEMBER"),
            (MembershipRole::Viewer, "VIEWER"),
        ],
        MembershipRole::Unknown,
    );
}

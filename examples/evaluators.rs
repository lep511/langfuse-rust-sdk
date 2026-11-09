//! Set up LLM-as-a-judge evaluation: create an evaluator, attach it to a
//! **disabled** evaluation rule for production generations, inspect both, and
//! delete them again.
//!
//! **Writes data**, then cleans up. The rule is created with `enabled: false`,
//! so no evaluation runs and no LLM cost is incurred.
//!
//! The evaluator uses the project's default evaluation model. If the project has
//! none (and no LLM connection), the API answers 412 and the example says so.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example evaluators
//! ```

use langfuse_rust_sdk::models::evaluation_common::{
    PromptVariableMappingInput, PromptVariableMappingSource, PublicApiError,
};
use langfuse_rust_sdk::models::evaluation_rules::{
    CreateEvaluationRuleRequest, EvaluationRuleEvaluatorAssignmentInput, EvaluationRuleFilter,
    OptionsFilterOperator,
};
use langfuse_rust_sdk::models::evaluators::{
    CreateEvaluatorRequest, EvaluatorChatMessage, EvaluatorChatMessageRole,
    EvaluatorChatPromptInput, EvaluatorDefinition, EvaluatorOutputDefinition,
    UpdateEvaluatorMetadataRequest, UpdateEvaluatorRequest,
};
use langfuse_rust_sdk::pagination::CursorParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};

type BoxError = Box<dyn std::error::Error>;

fn build_client() -> Result<LangfuseClient, BoxError> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

fn message(role: EvaluatorChatMessageRole, content: &str) -> EvaluatorChatMessage {
    EvaluatorChatMessage {
        role,
        content: content.into(),
    }
}

fn mapping(variable: &str) -> Option<PromptVariableMappingInput> {
    let source = match variable {
        "input" => PromptVariableMappingSource::Input,
        "output" => PromptVariableMappingSource::Output,
        _ => return None,
    };
    Some(PromptVariableMappingInput {
        variable: variable.into(),
        source,
        json_path: None,
    })
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // 1. Evaluator: how to score. Numeric score between 0 and 1.
    let created = client
        .evaluators
        .create(&CreateEvaluatorRequest::LlmAsJudge {
            name: "rust-sdk-example-helpfulness".into(),
            description: Some("Created by the Rust SDK evaluators example".into()),
            prompt: EvaluatorChatPromptInput::Messages(vec![
                message(
                    EvaluatorChatMessageRole::System,
                    "You are a strict reviewer of assistant answers.",
                ),
                message(
                    EvaluatorChatMessageRole::User,
                    "Question:\n{{input}}\n\nAnswer:\n{{output}}\n\nHow helpful is the answer?",
                ),
            ]),
            model_config: None,
            variable_mapping: None,
            output_definition: EvaluatorOutputDefinition::Numeric {
                min_value: Some(0.0),
                max_value: Some(1.0),
                score_reasoning_instructions: Some("Explain the score in one sentence.".into()),
                score_value_instructions: Some(
                    "1 = fully answers the question, 0 = useless.".into(),
                ),
            },
        })
        .await;
    let evaluator = match created {
        Ok(evaluator) => evaluator,
        Err(err @ LangfuseError::PreconditionFailed { .. }) => {
            let reason = PublicApiError::from_error(&err)
                .map(|e| e.message)
                .unwrap_or_else(|| err.to_string());
            println!("Cannot create an LLM-as-a-judge evaluator: {reason}");
            println!("Add an LLM connection and a default evaluation model in Langfuse first.");
            return Ok(());
        }
        Err(err) => return Err(err.into()),
    };
    println!(
        "Created evaluator {} v{} ({:?})",
        evaluator.base.name, evaluator.base.version, evaluator.base.status
    );
    let variables = match &evaluator.definition {
        EvaluatorDefinition::LlmAsJudge { variables, .. } => variables.clone(),
        EvaluatorDefinition::Code { .. } => Vec::new(),
    };
    println!("  prompt variables: {}", variables.join(", "));

    // Run the rest, then always clean up what was created.
    let mut rule_id: Option<String> = None;
    let outcome = async {
        // 2. Evaluation rule: what to score. Disabled, so nothing runs.
        let rule = client
            .evaluation_rules
            .create(&CreateEvaluationRuleRequest {
                name: "rust-sdk-example production generations".into(),
                enabled: false,
                sampling: Some(0.1),
                filter: Some(vec![
                    EvaluationRuleFilter::StringOptions {
                        column: "type".into(),
                        operator: OptionsFilterOperator::AnyOf,
                        value: vec!["GENERATION".into()],
                    },
                    EvaluationRuleFilter::StringOptions {
                        column: "environment".into(),
                        operator: OptionsFilterOperator::AnyOf,
                        value: vec!["production".into()],
                    },
                ]),
                evaluator_assignments: vec![EvaluationRuleEvaluatorAssignmentInput {
                    evaluator_id: evaluator.base.id.clone(),
                    variable_mapping: Some(variables.iter().filter_map(|v| mapping(v)).collect()),
                }],
            })
            .await?;
        rule_id = Some(rule.id.clone());
        println!(
            "Created rule '{}' (enabled: {}, sampling: {})",
            rule.name, rule.enabled, rule.sampling
        );
        for f in &rule.filter {
            println!(
                "  filter: {} {} {}",
                f.column,
                f.operator,
                f.value.clone().unwrap_or_default()
            );
        }

        // 3. Metadata-only update: does not create a new evaluator version.
        let updated = client
            .evaluators
            .update(
                &evaluator.base.id,
                &UpdateEvaluatorRequest::Metadata(UpdateEvaluatorMetadataRequest {
                    name: None,
                    description: Some(Some("Judges answer helpfulness (Rust SDK example)".into())),
                }),
            )
            .await?;
        println!(
            "Updated description: {}",
            updated.base.description.as_deref().unwrap_or("-")
        );

        let versions = client
            .evaluators
            .list_versions(&evaluator.base.id, &CursorParams::default())
            .await?;
        println!("Evaluator versions: {}", versions.data.len());

        let rules = client
            .evaluation_rules
            .list(&CursorParams {
                cursor: None,
                limit: Some(10),
            })
            .await?;
        println!(
            "Evaluation rules in the project (first page): {}",
            rules.data.len()
        );
        Ok::<(), BoxError>(())
    }
    .await;

    // 4. Clean up: the rule first, then the evaluator.
    if let Some(id) = rule_id {
        client.evaluation_rules.delete(&id).await?;
        println!("Deleted rule {id}");
    }
    client.evaluators.delete(&evaluator.base.id).await?;
    println!("Deleted evaluator {}", evaluator.base.id);

    outcome
}

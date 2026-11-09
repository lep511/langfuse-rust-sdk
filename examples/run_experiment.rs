//! Run an experiment on local data: a toy lookup "model" answers three
//! capital-city questions, two evaluators and a composite evaluator score each
//! answer, and a run evaluator computes the accuracy. Prints the report,
//! waits until Langfuse lists the experiment, then deletes its traces and
//! scores and checks whether the experiment is still listed.
//!
//! Writes one trace per item plus their scores, all deleted at the end. Set
//! `KEEP_EXPERIMENT=1` to keep them and open the printed URL instead.
//!
//! On a Langfuse dataset, fetch it with `client.get_dataset(..)` and call
//! `dataset.run_experiment(&tracer, task, options)`: runs with the same run
//! name on the same dataset then share one experiment.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example run_experiment --features otel
//! ```

use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use langfuse_rust_sdk::api::experiments::ListExperimentsParams;
use langfuse_rust_sdk::api::scores::ListScoresParams;
use langfuse_rust_sdk::experiment::{
    CompositeEvaluatorInput, Evaluation, EvaluatorInput, ExperimentItem, ExperimentOptions,
    ExperimentResult, LocalExperimentItem, RunEvaluatorInput, composite_evaluator, evaluator,
    run_evaluator,
};
use langfuse_rust_sdk::models::traces::DeleteTracesRequest;
use langfuse_rust_sdk::otel::{ObservationType, TracingOptions};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};
use serde_json::json;

/// The "model" under test: it gets one capital of three wrong.
fn lookup(country: &str) -> Result<String, String> {
    match country {
        "France" => Ok("Paris".into()),
        "Italy" => Ok("Rome".into()),
        "Spain" => Ok("Barcelona".into()),
        other => Err(format!("unknown country {other}")),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;
    let tracer = client.tracer(TracingOptions::from_env()?)?;

    let data = vec![
        LocalExperimentItem::new(json!({"country": "France"}))
            .with_expected_output("Paris")
            .with_metadata(json!({"region": "west"})),
        LocalExperimentItem::new(json!({"country": "Italy"})).with_expected_output("Rome"),
        LocalExperimentItem::new(json!({"country": "Spain"})).with_expected_output("Madrid"),
    ];

    // The task runs in the scope of the item's task span: the generation
    // below becomes its child.
    let task = |item: ExperimentItem| {
        let tracer = tracer.clone();
        async move {
            let country = item
                .input()
                .and_then(|input| input["country"].as_str())
                .ok_or("the input has no country")?
                .to_owned();
            let generation = tracer.start_observation("lookup-model", ObservationType::Generation);
            generation.set_model("lookup-table").set_input(&country);
            let answer = lookup(&country);
            match &answer {
                Ok(capital) => generation.set_output(capital),
                Err(err) => generation.set_status_message(err),
            };
            generation.end();
            answer
        }
    };

    let exact_match = evaluator("exact_match", |args: EvaluatorInput| async move {
        let expected = args.expected_output.as_ref().and_then(|v| v.as_str());
        let matches = expected.is_some_and(|e| Some(e) == args.output.as_str());
        Evaluation::boolean("exact_match", matches).with_comment(if matches {
            "Correct capital"
        } else {
            "Wrong capital"
        })
    });
    let brevity = evaluator("brevity", |args: EvaluatorInput| async move {
        let words = args
            .output
            .as_str()
            .unwrap_or_default()
            .split_whitespace()
            .count();
        Evaluation::numeric("brevity", if words <= 2 { 1.0 } else { 0.0 })
    });
    let overall = composite_evaluator("overall", |args: CompositeEvaluatorInput| async move {
        let score = |name: &str| {
            args.evaluations
                .iter()
                .find(|e| e.name == name)
                .and_then(|e| e.value.as_f64())
                .unwrap_or(0.0)
        };
        Evaluation::numeric(
            "overall",
            0.8 * score("exact_match") + 0.2 * score("brevity"),
        )
    });
    let accuracy = run_evaluator("accuracy", |run: RunEvaluatorInput| async move {
        let total = run.item_results.len();
        let correct = run
            .item_results
            .iter()
            .flat_map(|r| &r.evaluations)
            .filter(|e| e.name == "exact_match" && e.value.as_f64() == Some(1.0))
            .count();
        (total > 0).then(|| {
            Evaluation::numeric("accuracy", correct as f64 / total as f64)
                .with_comment(format!("{correct} of {total} correct"))
        })
    });

    let options = ExperimentOptions::new("capital-cities")
        .with_description("Toy lookup model on three capitals")
        .with_metadata("model", "lookup-table")
        .with_evaluator(exact_match)
        .with_evaluator(brevity)
        .with_composite_evaluator(overall)
        .with_run_evaluator(accuracy);
    let result = tracer.run_experiment(data, task, options).await?;
    println!("{}", result.format(true));

    // Langfuse builds the experiment from the traces once they are ingested.
    let listed = wait_for_experiment(&client, &result.experiment_id, true).await?;
    println!(
        "\nLangfuse lists the experiment: {}",
        if listed { "yes" } else { "not yet (timed out)" }
    );

    if std::env::var("KEEP_EXPERIMENT").is_ok_and(|v| !v.is_empty()) {
        println!("KEEP_EXPERIMENT is set: traces and scores are kept.");
    } else {
        cleanup(&client, &result).await?;
        let still_listed = wait_for_experiment(&client, &result.experiment_id, false).await?;
        println!(
            "Experiment still listed after deleting its traces: {}",
            if still_listed { "yes" } else { "no" }
        );
    }
    tracer.shutdown().await;
    client.shutdown().await;
    Ok(())
}

/// Poll `experiments.list` for up to a minute until the experiment is listed
/// (`present`) or no longer listed (`!present`); returns whether it is listed.
async fn wait_for_experiment(
    client: &LangfuseClient,
    experiment_id: &str,
    present: bool,
) -> Result<bool, LangfuseError> {
    let since =
        (Utc::now() - chrono::Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut listed = false;
    for _ in 0..20 {
        let mut params = ListExperimentsParams::new(since.clone());
        params.id = Some(experiment_id.to_owned());
        let page = client.experiments.list(&params).await?;
        listed = page.data.iter().any(|e| e.id == experiment_id);
        if listed == present {
            if let Some(experiment) = page.data.first() {
                println!(
                    "  {} — {} items, dataset {:?}",
                    experiment.name, experiment.item_count, experiment.dataset_id
                );
            }
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    Ok(listed)
}

/// Delete the scores of the run (item scores by trace, run scores by
/// experiment), then its traces.
async fn cleanup(client: &LangfuseClient, result: &ExperimentResult) -> Result<(), LangfuseError> {
    let trace_ids: Vec<String> = result
        .item_results
        .iter()
        .filter_map(|r| r.trace_id.clone())
        .collect();
    let mut queries: Vec<ListScoresParams> = trace_ids
        .iter()
        .map(|trace_id| ListScoresParams {
            trace_id: Some(trace_id.clone()),
            ..Default::default()
        })
        .collect();
    queries.push(ListScoresParams {
        experiment_id: Some(result.experiment_id.clone()),
        ..Default::default()
    });
    let mut deleted = 0;
    for params in &queries {
        for score in client.scores.list(params).await?.data {
            client.scores.delete(&score.base().id).await?;
            deleted += 1;
        }
    }
    client
        .traces
        .delete_batch(&DeleteTracesRequest {
            trace_ids: trace_ids.clone(),
        })
        .await?;
    println!("\nDeleted {deleted} scores and {} traces.", trace_ids.len());
    Ok(())
}

//! List the experiments of the last 30 days with their scores (cursor
//! pagination), then show the items of the first one listed: input, output,
//! expected output and scores.
//!
//! **Read-only:** writes no data. The Experiments API has no endpoints to
//! create or delete experiments: they come into being when experiment traces
//! are ingested. If this example finds nothing, run `run_experiment` with
//! `KEEP_EXPERIMENT=1` first (without it that example deletes its traces, and
//! with them the experiment).
//!
//! Experiments replace the former dataset runs: `GET /api/public/experiments`
//! and `GET /api/public/experiment-items`.
//!
//! **Docs:** <https://langfuse.com/docs/evaluation/experiments/data-model>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example experiments
//! ```

use chrono::{Duration, SecondsFormat, Utc};
use langfuse_rust_sdk::api::experiments::{ListExperimentItemsParams, ListExperimentsParams};
use langfuse_rust_sdk::models::experiments::Experiment;
use langfuse_rust_sdk::models::scores::ScoreV3;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

/// Stop after this many pages so the example stays short on large projects.
const MAX_PAGES: usize = 3;

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

/// `name=value` of a score, whatever its data type.
fn score_text(score: &ScoreV3) -> String {
    let value = match score {
        ScoreV3::Numeric { value, .. } => format!("{value:.3}"),
        ScoreV3::Boolean { value, .. } => value.to_string(),
        ScoreV3::Categorical { value, .. }
        | ScoreV3::Text { value, .. }
        | ScoreV3::Correction { value, .. } => value.clone(),
    };
    format!("{}={value}", score.base().name)
}

/// The first 60 characters of a JSON value.
fn preview(value: &Option<serde_json::Value>) -> String {
    match value {
        Some(v) => v.to_string().chars().take(60).collect(),
        None => "-".into(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // `fromStartTime` is required by both endpoints, so the params have a
    // constructor taking it.
    let from = (Utc::now() - Duration::days(30)).to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut experiments: Vec<Experiment> = Vec::new();
    let mut cursor: Option<String> = None;
    // Manual cursor pagination: pass `meta.cursor` of one page as `cursor`
    // of the next request until it is absent (`experiments.list_pages` does
    // this for you). `fields` adds the `metadata` and `scores` groups to the
    // default `core`; `score_limit` caps the scores returned per experiment.
    for _ in 0..MAX_PAGES {
        let page = client
            .experiments
            .list(&ListExperimentsParams {
                fields: Some("core,metadata,scores".into()),
                limit: Some(10),
                score_limit: Some(5),
                cursor: cursor.clone(),
                ..ListExperimentsParams::new(from.clone())
            })
            .await?;
        experiments.extend(page.data);
        match page.meta.cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    println!("Experiments since {from}: {}", experiments.len());
    for exp in &experiments {
        let scores: Vec<String> = exp.scores.iter().flatten().map(score_text).collect();
        println!(
            "  - {} ({} items, dataset {}) {}",
            exp.name,
            exp.item_count,
            exp.dataset_id.as_deref().unwrap_or("-"),
            if scores.is_empty() {
                String::new()
            } else {
                format!("[{}]", scores.join(", "))
            }
        );
    }

    let Some(latest) = experiments.first() else {
        println!("\nNo experiments in the window. Run a dataset experiment first.");
        return Ok(());
    };

    // One item per test case of the experiment, each with the trace of its
    // run. Input and output need the `io` field group.
    let items = client
        .experiments
        .list_items(&ListExperimentItemsParams {
            experiment_id: Some(latest.id.clone()),
            fields: Some("core,io,scores".into()),
            limit: Some(5),
            ..ListExperimentItemsParams::new(from)
        })
        .await?;
    println!("\nItems of '{}' (first {}):", latest.name, items.data.len());
    for item in &items.data {
        let scores: Vec<String> = item.scores.iter().flatten().map(score_text).collect();
        println!("  trace {}", item.trace_id);
        println!("    input:    {}", preview(&item.input));
        println!("    output:   {}", preview(&item.output));
        println!("    expected: {}", preview(&item.expected_output));
        if !scores.is_empty() {
            println!("    scores:   {}", scores.join(", "));
        }
    }

    Ok(())
}

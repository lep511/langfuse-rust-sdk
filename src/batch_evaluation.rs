//! Batch evaluation: score observations already stored in Langfuse with the
//! evaluators of [`experiment`](crate::experiment).
//!
//! [`LangfuseClient::run_batched_evaluation`] reads observations page by page
//! from `GET /api/public/v2/observations` (newest first, filtered by
//! [`filter`](BatchEvaluationOptions::filter)), turns each one into
//! [`EvaluatorInput`]s with a [`Mapper`], runs the evaluators and queues one
//! score per evaluation on the observation. Nothing is traced. A run that
//! stops early returns a [`BatchEvaluationResumeToken`] to continue it.
//!
//! ```rust,no_run
//! use langfuse_rust_sdk::batch_evaluation::BatchEvaluationOptions;
//! use langfuse_rust_sdk::experiment::{Evaluation, EvaluatorInput, evaluator};
//! use langfuse_rust_sdk::models::observations::ObservationV2;
//!
//! # async fn example(client: &langfuse_rust_sdk::LangfuseClient) -> Result<(), langfuse_rust_sdk::LangfuseError> {
//! let mapper = |observation: ObservationV2| async move {
//!     EvaluatorInput::new(
//!         observation.input.unwrap_or_default(),
//!         observation.output.unwrap_or_default(),
//!     )
//! };
//! let not_empty = evaluator("not_empty", |args: EvaluatorInput| async move {
//!     Evaluation::boolean("not_empty", !args.output.is_null())
//! });
//! let options = BatchEvaluationOptions::new(mapper)
//!     .with_evaluator(not_empty)
//!     .with_filter(r#"[{"type": "string", "column": "type", "operator": "=", "value": "GENERATION"}]"#)
//!     .with_max_items(100);
//! let result = client.run_batched_evaluation(options).await?;
//! println!("{result}");
//! # Ok(())
//! # }
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Display};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use futures_util::future::BoxFuture;
use futures_util::{FutureExt, StreamExt, stream};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::LangfuseClient;
use crate::api::observations::ListObservationsParams;
use crate::error::LangfuseError;
use crate::experiment::{
    CompositeEvaluator, CompositeEvaluatorInput, Evaluation, Evaluator, EvaluatorInput,
};
use crate::http::RequestOptions;
use crate::models::observations::ObservationV2;
use crate::models::scores::CreateScoreRequest;

/// Field groups fetched by default: `core,basic,io,metadata` (`io` has the
/// raw input and output strings). `time` is always added.
pub const DEFAULT_FIELDS: &str = "core,basic,io,metadata";

/// The observations API scans for up to 45 s before returning a page.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// Turns one observation into the inputs of the evaluators. Implemented for
/// async closures `Fn(ObservationV2) -> impl Future` returning an
/// [`EvaluatorInput`] or a `Result` of one with a [`Display`] error (the item
/// then counts as failed).
pub trait Mapper: Send + Sync {
    /// Map `observation`; an error marks the item as failed.
    fn map(&self, observation: ObservationV2) -> BoxFuture<'_, Result<EvaluatorInput, String>>;
}

/// What a mapper closure may return: an [`EvaluatorInput`], or a `Result` of
/// one whose error is any [`Display`] type.
pub trait IntoEvaluatorInput {
    /// The evaluator inputs, or the error message.
    fn into_evaluator_input(self) -> Result<EvaluatorInput, String>;
}

impl IntoEvaluatorInput for EvaluatorInput {
    fn into_evaluator_input(self) -> Result<EvaluatorInput, String> {
        Ok(self)
    }
}

impl<E: Display> IntoEvaluatorInput for Result<EvaluatorInput, E> {
    fn into_evaluator_input(self) -> Result<EvaluatorInput, String> {
        self.map_err(|err| err.to_string())
    }
}

impl<F, Fut> Mapper for F
where
    F: Fn(ObservationV2) -> Fut + Send + Sync,
    Fut: Future + Send + 'static,
    Fut::Output: IntoEvaluatorInput,
{
    fn map(&self, observation: ObservationV2) -> BoxFuture<'_, Result<EvaluatorInput, String>> {
        let future = self(observation);
        async move { future.await.into_evaluator_input() }.boxed()
    }
}

/// Options of [`LangfuseClient::run_batched_evaluation`]; only the mapper is
/// required.
#[derive(Clone)]
#[non_exhaustive]
pub struct BatchEvaluationOptions {
    /// Turns each observation into evaluator inputs.
    pub mapper: Arc<dyn Mapper>,
    /// Evaluators run in order on every observation.
    pub evaluators: Vec<Arc<dyn Evaluator>>,
    /// Evaluator combining the evaluations of each observation; runs when
    /// there is at least one.
    pub composite_evaluator: Option<Arc<dyn CompositeEvaluator>>,
    /// Filter of the observations: a JSON array of conditions of the v2
    /// observations API, e.g.
    /// `[{"type": "string", "column": "name", "operator": "=", "value": "chat"}]`.
    pub filter: Option<String>,
    /// Observations requested per page (default: 50, at most 1000).
    pub fetch_batch_size: usize,
    /// Comma-separated field groups to fetch (default: [`DEFAULT_FIELDS`]).
    /// `time` is always added; `None` or empty fetches `core,basic,time`.
    pub fields: Option<String>,
    /// Stop after reading this many observations (default: all).
    pub max_items: Option<usize>,
    /// Observations evaluated at the same time (default: 5).
    pub max_concurrency: usize,
    /// Metadata added to every score; it wins over the evaluation's.
    pub metadata: Map<String, Value>,
    /// Retries of each page request (default: 3), each with a 60 s timeout.
    pub max_retries: u32,
    /// Log progress with `tracing::info!` (default: `false`).
    pub verbose: bool,
    /// Continue a run that stopped early.
    pub resume_from: Option<BatchEvaluationResumeToken>,
}

impl fmt::Debug for BatchEvaluationOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchEvaluationOptions")
            .field(
                "evaluators",
                &self.evaluators.iter().map(|e| e.name()).collect::<Vec<_>>(),
            )
            .field(
                "composite_evaluator",
                &self.composite_evaluator.as_ref().map(|e| e.name()),
            )
            .field("filter", &self.filter)
            .field("fetch_batch_size", &self.fetch_batch_size)
            .field("fields", &self.fields)
            .field("max_items", &self.max_items)
            .field("max_concurrency", &self.max_concurrency)
            .field("metadata", &self.metadata)
            .field("max_retries", &self.max_retries)
            .field("verbose", &self.verbose)
            .field("resume_from", &self.resume_from)
            .finish_non_exhaustive()
    }
}

impl BatchEvaluationOptions {
    /// Options with `mapper` and the defaults (no evaluators yet).
    pub fn new(mapper: impl Mapper + 'static) -> Self {
        Self {
            mapper: Arc::new(mapper),
            evaluators: Vec::new(),
            composite_evaluator: None,
            filter: None,
            fetch_batch_size: 50,
            fields: Some(DEFAULT_FIELDS.to_owned()),
            max_items: None,
            max_concurrency: 5,
            metadata: Map::new(),
            max_retries: 3,
            verbose: false,
            resume_from: None,
        }
    }

    /// Set [`mapper`](Self::mapper).
    pub fn with_mapper(mut self, mapper: impl Mapper + 'static) -> Self {
        self.mapper = Arc::new(mapper);
        self
    }

    /// Replace [`evaluators`](Self::evaluators).
    pub fn with_evaluators(mut self, evaluators: Vec<Arc<dyn Evaluator>>) -> Self {
        self.evaluators = evaluators;
        self
    }

    /// Add one evaluator.
    pub fn with_evaluator(mut self, evaluator: impl Evaluator + 'static) -> Self {
        self.evaluators.push(Arc::new(evaluator));
        self
    }

    /// Set [`composite_evaluator`](Self::composite_evaluator).
    pub fn with_composite_evaluator(
        mut self,
        evaluator: impl CompositeEvaluator + 'static,
    ) -> Self {
        self.composite_evaluator = Some(Arc::new(evaluator));
        self
    }

    /// Set [`filter`](Self::filter).
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Set [`fetch_batch_size`](Self::fetch_batch_size).
    pub fn with_fetch_batch_size(mut self, fetch_batch_size: usize) -> Self {
        self.fetch_batch_size = fetch_batch_size;
        self
    }

    /// Set [`fields`](Self::fields).
    pub fn with_fields(mut self, fields: impl Into<String>) -> Self {
        self.fields = Some(fields.into());
        self
    }

    /// Set [`max_items`](Self::max_items).
    pub fn with_max_items(mut self, max_items: usize) -> Self {
        self.max_items = Some(max_items);
        self
    }

    /// Set [`max_concurrency`](Self::max_concurrency).
    pub fn with_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = max_concurrency;
        self
    }

    /// Set `metadata[key]` to `value` (skipped with a warning if it cannot be
    /// serialized).
    pub fn with_metadata<T: Serialize + ?Sized>(
        mut self,
        key: impl Into<String>,
        value: &T,
    ) -> Self {
        let key = key.into();
        match serde_json::to_value(value) {
            Ok(value) => {
                self.metadata.insert(key, value);
            }
            Err(err) => {
                tracing::warn!(key = %key, error = %err, "batch evaluation metadata cannot be serialized; skipped");
            }
        }
        self
    }

    /// Set [`max_retries`](Self::max_retries).
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set [`verbose`](Self::verbose).
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Set [`resume_from`](Self::resume_from).
    pub fn with_resume_from(mut self, token: BatchEvaluationResumeToken) -> Self {
        self.resume_from = Some(token);
        self
    }
}

/// Where a batch evaluation stopped; pass it as
/// [`resume_from`](BatchEvaluationOptions::resume_from) to continue. It
/// serializes, so it can be stored between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BatchEvaluationResumeToken {
    /// The filter of the run; resuming with another filter is an error.
    pub filter: Option<String>,
    /// Cursor of the next page to read.
    pub cursor: Option<String>,
    /// Start time (ISO 8601) of the last observation read. Without a cursor,
    /// the run resumes from observations that started at or before it.
    pub last_processed_timestamp: String,
    /// ID of the last observation read.
    pub last_processed_id: String,
    /// Observations evaluated so far, across resumed runs.
    pub items_processed: usize,
    /// IDs of the observations of the last page read, skipped when resuming.
    #[serde(default)]
    pub last_page_ids: Vec<String>,
}

/// Statistics of one evaluator in a batch evaluation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EvaluatorStats {
    /// Evaluator name.
    pub name: String,
    /// Times it ran.
    pub total_runs: usize,
    /// Runs that succeeded.
    pub successful_runs: usize,
    /// Runs that failed.
    pub failed_runs: usize,
    /// Scores created from its evaluations.
    pub total_scores_created: usize,
}

/// Result of [`LangfuseClient::run_batched_evaluation`]. `Display` prints a
/// summary.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BatchEvaluationResult {
    /// Observations read (duplicates included).
    pub total_items_fetched: usize,
    /// Observations evaluated.
    pub total_items_processed: usize,
    /// Observations that could not be evaluated (no trace ID, or the mapper failed).
    pub total_items_failed: usize,
    /// Scores from the item evaluators.
    pub total_scores_created: usize,
    /// Scores from the composite evaluator.
    pub total_composite_scores_created: usize,
    /// Failed evaluator runs.
    pub total_evaluations_failed: usize,
    /// Statistics per evaluator name, in evaluator order (without the composite evaluator).
    pub evaluator_stats: Vec<EvaluatorStats>,
    /// Token to continue the run: set when it stopped at `max_items` or on a
    /// failed page request.
    pub resume_token: Option<BatchEvaluationResumeToken>,
    /// `false` when a page request failed.
    pub completed: bool,
    /// Duration of the run.
    pub duration: Duration,
    /// IDs of the failed observations.
    pub failed_item_ids: Vec<String>,
    /// Number of failed observations per error kind (`MissingTraceId`, `MapperError`).
    pub error_summary: BTreeMap<String, usize>,
    /// Whether observations may remain after `max_items`.
    pub has_more_items: bool,
    /// Evaluations per observation ID.
    pub item_evaluations: BTreeMap<String, Vec<Evaluation>>,
}

impl Display for BatchEvaluationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rule = "=".repeat(60);
        let mut lines = vec![
            rule.clone(),
            "Batch Evaluation Results".into(),
            rule.clone(),
        ];
        lines.push(format!(
            "\nStatus: {}",
            if self.completed {
                "Completed"
            } else {
                "Incomplete"
            }
        ));
        let seconds = self.duration.as_secs_f64();
        lines.push(format!("Duration: {seconds:.2}s"));
        lines.push(format!("\nItems fetched: {}", self.total_items_fetched));
        lines.push(format!("Items processed: {}", self.total_items_processed));
        if self.total_items_failed > 0 {
            lines.push(format!("Items failed: {}", self.total_items_failed));
        }
        if self.total_items_fetched > 0 {
            let rate = self.total_items_processed as f64 / self.total_items_fetched as f64 * 100.0;
            lines.push(format!("Success rate: {rate:.1}%"));
        }
        lines.push(format!("\nScores created: {}", self.total_scores_created));
        if self.total_composite_scores_created > 0 {
            lines.push(format!(
                "Composite scores: {}",
                self.total_composite_scores_created
            ));
        }
        lines.push(format!(
            "Total scores: {}",
            self.total_scores_created + self.total_composite_scores_created
        ));
        if !self.evaluator_stats.is_empty() {
            lines.push("\nEvaluator Performance:".into());
            for stats in &self.evaluator_stats {
                lines.push(format!("  {}:", stats.name));
                if stats.total_runs > 0 {
                    let rate = stats.successful_runs as f64 / stats.total_runs as f64 * 100.0;
                    lines.push(format!(
                        "    Runs: {}/{} ({rate:.1}% success)",
                        stats.successful_runs, stats.total_runs
                    ));
                    lines.push(format!(
                        "    Scores created: {}",
                        stats.total_scores_created
                    ));
                    if stats.failed_runs > 0 {
                        lines.push(format!("    Failed runs: {}", stats.failed_runs));
                    }
                }
            }
        }
        if self.total_items_processed > 0 && seconds > 0.0 {
            lines.push("\nPerformance:".into());
            lines.push(format!(
                "  Throughput: {:.2} items/second",
                self.total_items_processed as f64 / seconds
            ));
            if self.total_scores_created > 0 {
                lines.push(format!(
                    "  Avg scores per item: {:.2}",
                    self.total_scores_created as f64 / self.total_items_processed as f64
                ));
            }
        }
        if !self.error_summary.is_empty() {
            lines.push("\nErrors encountered:".into());
            for (kind, count) in &self.error_summary {
                lines.push(format!("  {kind}: {count}"));
            }
        }
        if !self.completed {
            lines.push("\nWarning: Evaluation incomplete".into());
            if let Some(token) = &self.resume_token {
                lines.push(format!(
                    "  Last processed: {}",
                    token.last_processed_timestamp
                ));
                lines.push(format!("  Items processed: {}", token.items_processed));
                lines.push("  Use resume_from to continue".into());
            }
        }
        if self.has_more_items {
            lines.push("\nNote: More items may be available beyond max_items limit".into());
        }
        lines.push(rule);
        f.write_str(&lines.join("\n"))
    }
}

impl LangfuseClient {
    /// Evaluate observations already in Langfuse and score them; see
    /// [`batch_evaluation`](crate::batch_evaluation).
    ///
    /// Pages are read until the API returns no cursor (empty pages do not end
    /// the run) or [`max_items`](BatchEvaluationOptions::max_items) is
    /// reached. Each page is deduplicated by observation ID (keeping the most
    /// recently updated row) and evaluated with up to
    /// [`max_concurrency`](BatchEvaluationOptions::max_concurrency)
    /// observations at a time. Every evaluation becomes a score on the
    /// observation (with its trace and environment, and the merged
    /// metadata); the scores are flushed before returning.
    ///
    /// An observation without trace ID, or whose mapper fails, counts as
    /// failed; a failing evaluator is counted and the others still run. A
    /// failed page request ends the run with `completed: false` and a resume
    /// token.
    ///
    /// Returns [`LangfuseError::Config`] when the filter is not a JSON array,
    /// when `resume_from` was created for another filter, or for a
    /// `fetch_batch_size` or `max_concurrency` of 0.
    pub async fn run_batched_evaluation(
        &self,
        options: BatchEvaluationOptions,
    ) -> Result<BatchEvaluationResult, LangfuseError> {
        let started = Instant::now();
        let BatchEvaluationOptions {
            mapper,
            evaluators,
            composite_evaluator,
            mut filter,
            fetch_batch_size,
            fields,
            max_items,
            max_concurrency,
            metadata,
            max_retries,
            verbose,
            resume_from,
        } = options;
        if fetch_batch_size == 0 || max_concurrency == 0 {
            return Err(invalid(
                "fetch_batch_size and max_concurrency must be greater than 0",
            ));
        }
        // The token's cursor only continues the query that produced it.
        if let Some(token) = &resume_from {
            match &filter {
                None => filter = token.filter.clone(),
                Some(given) if token.filter.as_deref() != Some(given.as_str()) => {
                    return Err(invalid(
                        "resume token was created for a different filter; pass the same filter or none",
                    ));
                }
                Some(_) => {}
            }
        }
        let effective_filter = build_filter(filter.as_deref(), resume_from.as_ref())?;
        let fields = with_time_fields(fields.as_deref());
        let evaluation = ItemEvaluation {
            client: self,
            mapper: mapper.as_ref(),
            evaluators: &evaluators,
            composite_evaluator: composite_evaluator.as_deref(),
            metadata: &metadata,
        };

        let mut run = RunState::new(&evaluators, resume_from.as_ref(), filter);
        if verbose {
            tracing::info!(fields = %fields, "starting batch evaluation on observations");
            if let Some(token) = &resume_from {
                tracing::info!(
                    after = %token.last_processed_timestamp,
                    items_processed = token.items_processed,
                    "resuming batch evaluation"
                );
            }
        }

        let mut has_more = true;
        let mut batch_number = 0usize;
        loop {
            if max_items.is_some_and(|max| run.fetched >= max) {
                if verbose {
                    tracing::info!(max_items, "reached max_items");
                }
                break;
            }
            // Clamping the page keeps the cursor right after the last
            // observation read, so resuming after max_items skips nothing.
            let limit = match max_items {
                Some(max) => fetch_batch_size.min(max - run.fetched),
                None => fetch_batch_size,
            };
            let params = ListObservationsParams {
                limit: Some(i64::try_from(limit).unwrap_or(i64::MAX)),
                cursor: run.cursor.clone(),
                fields: Some(fields.clone()),
                filter: effective_filter.clone(),
                ..Default::default()
            };
            let request_options = RequestOptions {
                max_retries: Some(max_retries),
                timeout: Some(FETCH_TIMEOUT),
                ..RequestOptions::default()
            };
            let page = match self
                .observations
                .list_with_options(&params, request_options)
                .await
            {
                Ok(page) => page,
                Err(err) => {
                    tracing::error!(error = %err, "failed to fetch observations for batch evaluation");
                    return Ok(run.finish(started, false, true, false));
                }
            };
            batch_number += 1;
            let items = page.data;
            run.fetched += items.len();
            if verbose {
                tracing::info!(batch = batch_number, items = items.len(), "fetched batch");
            }

            let mut skip = run.previous_page_ids.clone();
            if let Some(id) = &run.resumed_item_id {
                skip.insert(id.clone());
            }
            let to_process = deduplicate(&items, &skip);
            let outcomes: Vec<Result<ItemOutcome, ItemFailure>> = stream::iter(to_process)
                .map(|observation| evaluation.evaluate(observation))
                .buffered(max_concurrency)
                .collect()
                .await;
            for outcome in outcomes {
                run.record(outcome);
            }

            if let Some(last) = items.last() {
                run.previous_page_ids = items.iter().map(|o| o.id.clone()).collect();
                run.last_timestamp = last.start_time.to_rfc3339_opts(SecondsFormat::AutoSi, true);
                run.last_id = last.id.clone();
            }
            if verbose {
                match max_items.filter(|max| *max > 0) {
                    Some(max) => tracing::info!(
                        processed = run.processed,
                        max_items = max,
                        percent = format!("{:.1}", run.processed as f64 / max as f64 * 100.0),
                        scores = run.scores,
                        "batch evaluation progress"
                    ),
                    None => tracing::info!(
                        processed = run.processed,
                        scores = run.scores,
                        "batch evaluation progress"
                    ),
                }
            }
            // The API can return short or empty pages before the end; only a
            // missing cursor marks it.
            run.cursor = page.meta.cursor.filter(|c| !c.is_empty());
            if run.cursor.is_none() {
                has_more = false;
                break;
            }
        }

        if verbose {
            tracing::info!("flushing scores to Langfuse");
        }
        self.flush().await;
        if verbose {
            tracing::info!(
                processed = run.processed,
                seconds = format!("{:.2}", started.elapsed().as_secs_f64()),
                "batch evaluation complete"
            );
        }
        Ok(run.finish(started, true, has_more, has_more))
    }
}

fn invalid(message: &str) -> LangfuseError {
    LangfuseError::Config(format!("Invalid batch evaluation options: {message}"))
}

/// Add the `time` group, whose `updatedAt` picks the newest duplicate row.
fn with_time_fields(fields: Option<&str>) -> String {
    let mut groups: Vec<&str> = fields
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .collect();
    if groups.is_empty() {
        // The API default when no groups are requested.
        groups = vec!["core", "basic"];
    }
    if !groups.contains(&"time") {
        groups.push("time");
    }
    groups.join(",")
}

/// The user filter plus, for a token without cursor, the start-time bound
/// (inclusive, so observations with the same start time are not lost).
///
/// Conditions go into the JSON filter rather than query parameters: the API
/// drops a query parameter when the filter has a condition on the same column.
fn build_filter(
    filter: Option<&str>,
    resume_from: Option<&BatchEvaluationResumeToken>,
) -> Result<Option<String>, LangfuseError> {
    let mut conditions = Vec::new();
    if let Some(filter) = filter.filter(|f| !f.is_empty()) {
        let parsed: Value = serde_json::from_str(filter)
            .map_err(|err| invalid(&format!("filter must be a JSON array: {err}")))?;
        match parsed {
            Value::Array(items) => conditions.extend(items),
            other => {
                let kind = match other {
                    Value::Object(_) => "an object",
                    Value::String(_) => "a string",
                    Value::Number(_) => "a number",
                    Value::Bool(_) => "a boolean",
                    _ => "null",
                };
                return Err(invalid(&format!(
                    "filter must be a JSON array of conditions, got {kind}"
                )));
            }
        }
    }
    if let Some(token) = resume_from
        && token.cursor.is_none()
        && !token.last_processed_timestamp.is_empty()
    {
        conditions.push(json!({
            "type": "datetime",
            "column": "startTime",
            "operator": "<=",
            "value": token.last_processed_timestamp,
        }));
    }
    Ok((!conditions.is_empty()).then(|| Value::Array(conditions).to_string()))
}

/// One row per observation ID, keeping the most recently updated one; rows
/// whose ID is in `skip` are dropped. Until ClickHouse merges them, a page
/// (or two consecutive pages) can hold several rows of one observation.
fn deduplicate(items: &[ObservationV2], skip: &HashSet<String>) -> Vec<ObservationV2> {
    fn freshness(observation: &ObservationV2) -> DateTime<Utc> {
        observation.updated_at.unwrap_or(observation.start_time)
    }
    let mut out: Vec<ObservationV2> = Vec::new();
    let mut positions: HashMap<&str, usize> = HashMap::new();
    for item in items {
        if skip.contains(&item.id) {
            continue;
        }
        match positions.get(item.id.as_str()) {
            Some(&position) => {
                if freshness(item) > freshness(&out[position]) {
                    out[position] = item.clone();
                }
            }
            None => {
                positions.insert(&item.id, out.len());
                out.push(item.clone());
            }
        }
    }
    out
}

/// Evaluates one observation.
struct ItemEvaluation<'a> {
    client: &'a LangfuseClient,
    mapper: &'a dyn Mapper,
    evaluators: &'a [Arc<dyn Evaluator>],
    composite_evaluator: Option<&'a dyn CompositeEvaluator>,
    metadata: &'a Map<String, Value>,
}

struct ItemOutcome {
    id: String,
    /// Scores created per evaluator, `None` for a failed run.
    runs: Vec<Option<usize>>,
    composite_scores: usize,
    evaluations: Vec<Evaluation>,
}

struct ItemFailure {
    id: String,
    kind: &'static str,
    message: String,
}

impl ItemEvaluation<'_> {
    async fn evaluate(&self, observation: ObservationV2) -> Result<ItemOutcome, ItemFailure> {
        let id = observation.id.clone();
        let failure = |kind, message| ItemFailure {
            id: id.clone(),
            kind,
            message,
        };
        let Some(trace_id) = observation.trace_id.clone().filter(|t| !t.is_empty()) else {
            return Err(failure(
                "MissingTraceId",
                format!("Observation {id} has no trace_id"),
            ));
        };
        let environment = observation.environment.clone();
        let input = self
            .mapper
            .map(observation)
            .await
            .map_err(|err| failure("MapperError", err))?;

        let mut evaluations = Vec::new();
        let mut runs = Vec::with_capacity(self.evaluators.len());
        for evaluator in self.evaluators {
            match evaluator.evaluate(input.clone()).await {
                Ok(results) => {
                    runs.push(Some(results.len()));
                    evaluations.extend(results);
                }
                Err(err) => {
                    runs.push(None);
                    tracing::warn!(evaluator = evaluator.name(), observation = %id, error = %err, "evaluator failed");
                }
            }
        }
        for evaluation in &evaluations {
            self.score(&id, &trace_id, environment.as_deref(), evaluation);
        }
        let mut composite_scores = 0;
        if let Some(composite) = self.composite_evaluator
            && !evaluations.is_empty()
        {
            match composite
                .evaluate(CompositeEvaluatorInput::new(input, evaluations.clone()))
                .await
            {
                Ok(results) => {
                    for evaluation in results {
                        self.score(&id, &trace_id, environment.as_deref(), &evaluation);
                        composite_scores += 1;
                        evaluations.push(evaluation);
                    }
                }
                Err(err) => {
                    tracing::warn!(evaluator = composite.name(), observation = %id, error = %err, "composite evaluator failed");
                }
            }
        }
        Ok(ItemOutcome {
            id,
            runs,
            composite_scores,
            evaluations,
        })
    }

    fn score(
        &self,
        observation_id: &str,
        trace_id: &str,
        environment: Option<&str>,
        evaluation: &Evaluation,
    ) {
        let request = evaluation.to_score_request();
        self.client.score(CreateScoreRequest {
            observation_id: Some(observation_id.to_owned()),
            trace_id: Some(trace_id.to_owned()),
            environment: environment.map(str::to_owned),
            metadata: merge_metadata(request.metadata.clone(), self.metadata),
            ..request
        });
    }
}

/// The evaluation's metadata with `extra` merged in (`extra` wins); metadata
/// that is not an object is replaced when there is `extra`.
fn merge_metadata(metadata: Option<Value>, extra: &Map<String, Value>) -> Option<Value> {
    if extra.is_empty() {
        return metadata;
    }
    let mut merged = match metadata {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    merged.extend(extra.clone());
    Some(Value::Object(merged))
}

/// Counters and position of a run.
struct RunState {
    filter: Option<String>,
    fetched: usize,
    processed: usize,
    failed: usize,
    scores: usize,
    composite_scores: usize,
    evaluations_failed: usize,
    stats: Vec<EvaluatorStats>,
    /// Index into `stats` of each evaluator.
    stat_index: Vec<usize>,
    failed_item_ids: Vec<String>,
    error_summary: BTreeMap<String, usize>,
    item_evaluations: BTreeMap<String, Vec<Evaluation>>,
    previously_processed: usize,
    cursor: Option<String>,
    last_timestamp: String,
    last_id: String,
    /// Skipped on every page: the start-time bound of a token without
    /// cursor is inclusive.
    resumed_item_id: Option<String>,
    previous_page_ids: HashSet<String>,
}

impl RunState {
    fn new(
        evaluators: &[Arc<dyn Evaluator>],
        resume_from: Option<&BatchEvaluationResumeToken>,
        filter: Option<String>,
    ) -> Self {
        let mut stats: Vec<EvaluatorStats> = Vec::new();
        let stat_index = evaluators
            .iter()
            .map(
                |evaluator| match stats.iter().position(|s| s.name == evaluator.name()) {
                    Some(index) => index,
                    None => {
                        stats.push(EvaluatorStats {
                            name: evaluator.name().to_owned(),
                            ..EvaluatorStats::default()
                        });
                        stats.len() - 1
                    }
                },
            )
            .collect();
        Self {
            filter,
            fetched: 0,
            processed: 0,
            failed: 0,
            scores: 0,
            composite_scores: 0,
            evaluations_failed: 0,
            stats,
            stat_index,
            failed_item_ids: Vec::new(),
            error_summary: BTreeMap::new(),
            item_evaluations: BTreeMap::new(),
            previously_processed: resume_from.map_or(0, |t| t.items_processed),
            cursor: resume_from.and_then(|t| t.cursor.clone()),
            last_timestamp: resume_from
                .map(|t| t.last_processed_timestamp.clone())
                .unwrap_or_default(),
            last_id: resume_from
                .map(|t| t.last_processed_id.clone())
                .unwrap_or_default(),
            resumed_item_id: resume_from
                .filter(|t| t.cursor.is_none() && !t.last_processed_timestamp.is_empty())
                .map(|t| t.last_processed_id.clone()),
            previous_page_ids: resume_from
                .map(|t| t.last_page_ids.iter().cloned().collect())
                .unwrap_or_default(),
        }
    }

    fn record(&mut self, outcome: Result<ItemOutcome, ItemFailure>) {
        match outcome {
            Ok(item) => {
                self.processed += 1;
                for (index, run) in item.runs.iter().enumerate() {
                    let stats = &mut self.stats[self.stat_index[index]];
                    stats.total_runs += 1;
                    match run {
                        Some(scores) => {
                            stats.successful_runs += 1;
                            stats.total_scores_created += scores;
                            self.scores += scores;
                        }
                        None => {
                            stats.failed_runs += 1;
                            self.evaluations_failed += 1;
                        }
                    }
                }
                self.composite_scores += item.composite_scores;
                self.item_evaluations.insert(item.id, item.evaluations);
            }
            Err(failure) => {
                self.failed += 1;
                *self
                    .error_summary
                    .entry(failure.kind.to_owned())
                    .or_default() += 1;
                tracing::warn!(observation = %failure.id, error = %failure.message, "batch evaluation item failed");
                self.failed_item_ids.push(failure.id);
            }
        }
    }

    fn token(&self) -> BatchEvaluationResumeToken {
        let mut last_page_ids: Vec<String> = self.previous_page_ids.iter().cloned().collect();
        last_page_ids.sort();
        BatchEvaluationResumeToken {
            filter: self.filter.clone(),
            cursor: self.cursor.clone(),
            last_processed_timestamp: self.last_timestamp.clone(),
            last_processed_id: self.last_id.clone(),
            items_processed: self.previously_processed + self.processed,
            last_page_ids,
        }
    }

    fn finish(
        self,
        started: Instant,
        completed: bool,
        with_token: bool,
        has_more_items: bool,
    ) -> BatchEvaluationResult {
        BatchEvaluationResult {
            resume_token: with_token.then(|| self.token()),
            total_items_fetched: self.fetched,
            total_items_processed: self.processed,
            total_items_failed: self.failed,
            total_scores_created: self.scores,
            total_composite_scores_created: self.composite_scores,
            total_evaluations_failed: self.evaluations_failed,
            evaluator_stats: self.stats,
            completed,
            duration: started.elapsed(),
            failed_item_ids: self.failed_item_ids,
            error_summary: self.error_summary,
            has_more_items,
            item_evaluations: self.item_evaluations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_is_always_fetched() {
        assert_eq!(
            with_time_fields(Some(DEFAULT_FIELDS)),
            "core,basic,io,metadata,time"
        );
        assert_eq!(with_time_fields(Some(" io , time ")), "io,time");
        assert_eq!(with_time_fields(Some("")), "core,basic,time");
        assert_eq!(with_time_fields(None), "core,basic,time");
    }

    fn token(cursor: Option<&str>) -> BatchEvaluationResumeToken {
        BatchEvaluationResumeToken {
            filter: None,
            cursor: cursor.map(str::to_owned),
            last_processed_timestamp: "2026-11-09T10:00:00Z".into(),
            last_processed_id: "obs-1".into(),
            items_processed: 3,
            last_page_ids: Vec::new(),
        }
    }

    #[test]
    fn filters_must_be_json_arrays() {
        assert_eq!(build_filter(None, None).unwrap(), None);
        assert_eq!(build_filter(Some(""), None).unwrap(), None);
        let filter = r#"[{"type":"string","column":"name","operator":"=","value":"chat"}]"#;
        let built = build_filter(Some(filter), None).unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&built).unwrap(),
            serde_json::from_str::<Value>(filter).unwrap()
        );
        for bad in ["{}", "not json", "\"x\""] {
            assert!(
                matches!(build_filter(Some(bad), None), Err(LangfuseError::Config(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn tokens_without_cursor_bound_the_start_time() {
        let filter = build_filter(Some("[]"), Some(&token(None)))
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&filter).unwrap(),
            json!([{"type": "datetime", "column": "startTime", "operator": "<=", "value": "2026-11-09T10:00:00Z"}])
        );
        assert_eq!(build_filter(None, Some(&token(Some("c")))).unwrap(), None);
    }

    #[test]
    fn metadata_merges_with_the_run_metadata_winning() {
        let extra: Map<String, Value> = [("run".to_string(), json!("nightly"))]
            .into_iter()
            .collect();
        assert_eq!(
            merge_metadata(Some(json!({"a": 1, "run": "x"})), &extra),
            Some(json!({"a": 1, "run": "nightly"}))
        );
        assert_eq!(
            merge_metadata(Some(json!([1])), &Map::new()),
            Some(json!([1]))
        );
        assert_eq!(merge_metadata(None, &Map::new()), None);
    }

    #[test]
    fn tokens_round_trip_through_json() {
        let token = token(Some("c"));
        let text = serde_json::to_string(&token).unwrap();
        assert_eq!(
            serde_json::from_str::<BatchEvaluationResumeToken>(&text).unwrap(),
            token
        );
    }
}

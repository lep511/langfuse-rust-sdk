//! Experiments and evaluations: run a task over dataset items or local data,
//! score the outputs with evaluators and compare runs in Langfuse.
//!
//! - [`Evaluation`] is one score produced by an evaluator.
//! - [`Evaluator`], [`CompositeEvaluator`] and [`RunEvaluator`] score one
//!   item, combine the evaluations of one item, or score a whole run. Build
//!   them from async closures with [`evaluator`], [`composite_evaluator`] and
//!   [`run_evaluator`], or implement the traits on your own types.
//! - [`Task`] is the code under test; any async closure taking an
//!   [`ExperimentItem`] and returning `Result<impl Serialize, impl Display>`
//!   is one.
//! - With the `otel` feature, `LangfuseTracer::run_experiment` (and
//!   `DatasetWithItems::run_experiment`) runs the task on every item, traces
//!   each run, scores it and returns an [`ExperimentResult`]. Langfuse builds
//!   the experiment from the traces: no dataset run is created through the API.
//! - [`LangfuseClient::run_batched_evaluation`](crate::LangfuseClient::run_batched_evaluation)
//!   reuses the same evaluators to score observations already in Langfuse
//!   (see [`batch_evaluation`](crate::batch_evaluation)).
//!
//! ```
//! use langfuse_rust_sdk::experiment::{Evaluation, EvaluatorInput, evaluator};
//! use serde_json::json;
//!
//! // Scores 1 when the output matches the expected output.
//! let exact_match = evaluator("exact_match", |args: EvaluatorInput| async move {
//!     let matches = args.expected_output.as_ref() == Some(&args.output);
//!     Evaluation::boolean("exact_match", matches)
//! });
//!
//! // Evaluators that can fail return a `Result` (the error is recorded on the span).
//! let length = evaluator("length", |args: EvaluatorInput| async move {
//!     let text = args.output.as_str().ok_or("output is not a string")?;
//!     Ok::<_, &str>(Evaluation::numeric("length", text.len() as f64))
//! });
//! # let _ = (exact_match, length, json!(null));
//! ```

use std::collections::BTreeSet;
use std::fmt::{self, Display};
use std::future::Future;
use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::models::dataset_items::DatasetItem;
use crate::models::enums::ScoreDataType;
use crate::models::scores::{CreateScoreRequest, CreateScoreValue};

/// Value of an [`Evaluation`]; it determines the data type of the score.
#[derive(Debug, Clone, PartialEq)]
pub enum EvaluationValue {
    /// A number (`NUMERIC` score).
    Numeric(f64),
    /// A pass/fail result (`BOOLEAN` score, sent as `1` or `0`).
    Boolean(bool),
    /// A label (`CATEGORICAL` score).
    Categorical(String),
}

impl EvaluationValue {
    /// The score data type of this value.
    pub fn data_type(&self) -> ScoreDataType {
        match self {
            EvaluationValue::Numeric(_) => ScoreDataType::Numeric,
            EvaluationValue::Boolean(_) => ScoreDataType::Boolean,
            EvaluationValue::Categorical(_) => ScoreDataType::Categorical,
        }
    }

    /// The value as a number: numeric values as-is, booleans as `1.0` or
    /// `0.0`; `None` for categorical values.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            EvaluationValue::Numeric(value) => Some(*value),
            EvaluationValue::Boolean(value) => Some(if *value { 1.0 } else { 0.0 }),
            EvaluationValue::Categorical(_) => None,
        }
    }
}

impl Serialize for EvaluationValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            EvaluationValue::Numeric(value) => serializer.serialize_f64(*value),
            EvaluationValue::Boolean(value) => serializer.serialize_bool(*value),
            EvaluationValue::Categorical(value) => serializer.serialize_str(value),
        }
    }
}

macro_rules! numeric_from {
    ($($ty:ty),*) => {$(
        impl From<$ty> for EvaluationValue {
            fn from(value: $ty) -> Self {
                EvaluationValue::Numeric(value as f64)
            }
        }
    )*};
}
numeric_from!(f64, f32, i64, i32, u64, u32, usize);

impl From<bool> for EvaluationValue {
    fn from(value: bool) -> Self {
        EvaluationValue::Boolean(value)
    }
}

impl From<String> for EvaluationValue {
    fn from(value: String) -> Self {
        EvaluationValue::Categorical(value)
    }
}

impl From<&str> for EvaluationValue {
    fn from(value: &str) -> Self {
        EvaluationValue::Categorical(value.to_owned())
    }
}

/// One result of an evaluator, stored in Langfuse as a score.
///
/// Build it with [`Evaluation::new`] (or [`numeric`](Self::numeric),
/// [`boolean`](Self::boolean), [`categorical`](Self::categorical)) and the
/// `with_*` methods:
///
/// ```
/// use langfuse_rust_sdk::experiment::Evaluation;
/// use serde_json::json;
///
/// let evaluation = Evaluation::numeric("relevance", 0.8)
///     .with_comment("Answers the question, misses one detail")
///     .with_metadata(&json!({"judge": "gpt-4o"}));
/// assert_eq!(evaluation.value.as_f64(), Some(0.8));
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Evaluation {
    /// Score name, e.g. `accuracy`; keep it stable across runs to compare them.
    pub name: String,
    /// Score value.
    pub value: EvaluationValue,
    /// Explanation shown next to the score.
    pub comment: Option<String>,
    /// Metadata of the score (any JSON value; objects merge with run metadata
    /// in batch evaluations).
    pub metadata: Option<Value>,
    /// Langfuse score config the value must match.
    pub config_id: Option<String>,
}

impl Evaluation {
    /// An evaluation named `name` with `value` (`f64`, integers, `bool` or a
    /// string label).
    pub fn new(name: impl Into<String>, value: impl Into<EvaluationValue>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            comment: None,
            metadata: None,
            config_id: None,
        }
    }

    /// A numeric evaluation.
    pub fn numeric(name: impl Into<String>, value: f64) -> Self {
        Self::new(name, EvaluationValue::Numeric(value))
    }

    /// A boolean evaluation.
    pub fn boolean(name: impl Into<String>, value: bool) -> Self {
        Self::new(name, EvaluationValue::Boolean(value))
    }

    /// A categorical evaluation.
    pub fn categorical(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self::new(name, EvaluationValue::Categorical(value.into()))
    }

    /// Set [`comment`](Self::comment).
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Set [`metadata`](Self::metadata) to anything serializable (skipped
    /// with a warning if it cannot be serialized).
    pub fn with_metadata<T: Serialize + ?Sized>(mut self, metadata: &T) -> Self {
        match serde_json::to_value(metadata) {
            Ok(value) => self.metadata = Some(value),
            Err(err) => {
                tracing::warn!(evaluation = %self.name, error = %err, "evaluation metadata cannot be serialized; skipped");
            }
        }
        self
    }

    /// Set [`config_id`](Self::config_id).
    pub fn with_config_id(mut self, config_id: impl Into<String>) -> Self {
        self.config_id = Some(config_id.into());
        self
    }

    /// The score data type, given by the value.
    pub fn data_type(&self) -> ScoreDataType {
        self.value.data_type()
    }

    /// The score request for this evaluation; the caller sets its subject.
    pub(crate) fn to_score_request(&self) -> CreateScoreRequest {
        let value = match &self.value {
            EvaluationValue::Numeric(value) => CreateScoreValue::Numeric(*value),
            EvaluationValue::Boolean(value) => {
                CreateScoreValue::Numeric(if *value { 1.0 } else { 0.0 })
            }
            EvaluationValue::Categorical(value) => CreateScoreValue::String(value.clone()),
        };
        let name = if self.name.is_empty() {
            "<unknown>"
        } else {
            &self.name
        };
        CreateScoreRequest {
            comment: self.comment.clone(),
            metadata: self.metadata.clone(),
            config_id: self.config_id.clone(),
            data_type: Some(self.data_type()),
            ..CreateScoreRequest::new(name, value)
        }
    }
}

/// Serializes as `{"name", "value", "comment"?, "metadata"?, "data_type", "config_id"?}`,
/// the form recorded as the output of evaluator spans.
impl Serialize for Evaluation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("value", &self.value)?;
        if let Some(comment) = &self.comment {
            map.serialize_entry("comment", comment)?;
        }
        if let Some(metadata) = &self.metadata {
            map.serialize_entry("metadata", metadata)?;
        }
        map.serialize_entry("data_type", &self.data_type())?;
        if let Some(config_id) = &self.config_id {
            map.serialize_entry("config_id", config_id)?;
        }
        map.end()
    }
}

/// What an evaluator closure may return: an [`Evaluation`], a `Vec` of them,
/// an `Option` (none for nothing to report), or a `Result` of any of these
/// whose error is any [`Display`] type.
pub trait IntoEvaluations {
    /// The evaluations, or the error message.
    fn into_evaluations(self) -> Result<Vec<Evaluation>, String>;
}

impl IntoEvaluations for Evaluation {
    fn into_evaluations(self) -> Result<Vec<Evaluation>, String> {
        Ok(vec![self])
    }
}

impl IntoEvaluations for Vec<Evaluation> {
    fn into_evaluations(self) -> Result<Vec<Evaluation>, String> {
        Ok(self)
    }
}

impl IntoEvaluations for Option<Evaluation> {
    fn into_evaluations(self) -> Result<Vec<Evaluation>, String> {
        Ok(self.into_iter().collect())
    }
}

impl<T: IntoEvaluations, E: Display> IntoEvaluations for Result<T, E> {
    fn into_evaluations(self) -> Result<Vec<Evaluation>, String> {
        self.map_err(|err| err.to_string())?.into_evaluations()
    }
}

/// What an evaluator receives: the input of the item, the output of the
/// task, and the expected output and metadata of the item, if any.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct EvaluatorInput {
    /// Input of the item (or observation).
    pub input: Value,
    /// Output to evaluate.
    pub output: Value,
    /// Expected output of the item.
    pub expected_output: Option<Value>,
    /// Metadata of the item.
    pub metadata: Option<Value>,
}

impl EvaluatorInput {
    /// Inputs without expected output and metadata.
    pub fn new(input: impl Into<Value>, output: impl Into<Value>) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
            expected_output: None,
            metadata: None,
        }
    }

    /// Set [`expected_output`](Self::expected_output).
    pub fn with_expected_output(mut self, expected_output: impl Into<Value>) -> Self {
        self.expected_output = Some(expected_output.into());
        self
    }

    /// Set [`metadata`](Self::metadata).
    pub fn with_metadata(mut self, metadata: impl Into<Value>) -> Self {
        self.metadata = Some(metadata.into());
        self
    }
}

/// What a composite evaluator receives: the evaluator inputs of the item plus
/// every evaluation the item evaluators produced for it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct CompositeEvaluatorInput {
    /// Input of the item.
    pub input: Value,
    /// Output of the task.
    pub output: Value,
    /// Expected output of the item.
    pub expected_output: Option<Value>,
    /// Metadata of the item.
    pub metadata: Option<Value>,
    /// Evaluations of the item, in evaluator order.
    pub evaluations: Vec<Evaluation>,
}

impl CompositeEvaluatorInput {
    /// The inputs of an item evaluator plus `evaluations`.
    pub fn new(input: EvaluatorInput, evaluations: Vec<Evaluation>) -> Self {
        Self {
            input: input.input,
            output: input.output,
            expected_output: input.expected_output,
            metadata: input.metadata,
            evaluations,
        }
    }
}

/// What a run evaluator receives: the results of every item that succeeded.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RunEvaluatorInput {
    /// Results of the items whose task succeeded, in data order.
    pub item_results: Arc<Vec<ExperimentItemResult>>,
}

impl RunEvaluatorInput {
    /// Inputs for `item_results`.
    pub fn new(item_results: Vec<ExperimentItemResult>) -> Self {
        Self {
            item_results: Arc::new(item_results),
        }
    }
}

/// Scores the output of one item. See [`evaluator`] to build one from a closure.
pub trait Evaluator: Send + Sync {
    /// Name of the evaluator (the span name in experiments, and the key of
    /// its statistics in batch evaluations).
    fn name(&self) -> &str;

    /// Evaluate one item; an error is recorded and the other evaluators still run.
    fn evaluate(&self, input: EvaluatorInput) -> BoxFuture<'_, Result<Vec<Evaluation>, String>>;
}

/// Combines the evaluations of one item into more (e.g. a weighted average or
/// a pass/fail decision). Runs only when the item evaluators produced at
/// least one evaluation. See [`composite_evaluator`].
pub trait CompositeEvaluator: Send + Sync {
    /// Name of the evaluator.
    fn name(&self) -> &str;

    /// Evaluate one item from its evaluations.
    fn evaluate(
        &self,
        input: CompositeEvaluatorInput,
    ) -> BoxFuture<'_, Result<Vec<Evaluation>, String>>;
}

/// Scores a whole experiment run (e.g. average accuracy). Its evaluations
/// are attached to the experiment, not to a trace. See [`run_evaluator`].
pub trait RunEvaluator: Send + Sync {
    /// Name of the evaluator.
    fn name(&self) -> &str;

    /// Evaluate the run.
    fn evaluate(&self, input: RunEvaluatorInput) -> BoxFuture<'_, Result<Vec<Evaluation>, String>>;
}

/// An evaluator built from a named closure by [`evaluator`],
/// [`composite_evaluator`] or [`run_evaluator`].
#[derive(Clone)]
pub struct FnEvaluator<F> {
    name: String,
    f: F,
}

impl<F> fmt::Debug for FnEvaluator<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FnEvaluator")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// An [`Evaluator`] named `name` running `f`. The closure returns what
/// [`IntoEvaluations`] accepts: an [`Evaluation`], a `Vec`, an `Option`, or a
/// `Result` of these.
///
/// A closure has no name, so it must be given one: Python takes the function
/// name.
pub fn evaluator<F, Fut>(name: impl Into<String>, f: F) -> FnEvaluator<F>
where
    F: Fn(EvaluatorInput) -> Fut + Send + Sync,
    Fut: Future + Send + 'static,
    Fut::Output: IntoEvaluations,
{
    FnEvaluator {
        name: name.into(),
        f,
    }
}

/// A [`CompositeEvaluator`] named `name` running `f`; see [`evaluator`].
pub fn composite_evaluator<F, Fut>(name: impl Into<String>, f: F) -> FnEvaluator<F>
where
    F: Fn(CompositeEvaluatorInput) -> Fut + Send + Sync,
    Fut: Future + Send + 'static,
    Fut::Output: IntoEvaluations,
{
    FnEvaluator {
        name: name.into(),
        f,
    }
}

/// A [`RunEvaluator`] named `name` running `f`; see [`evaluator`].
///
/// ```
/// use langfuse_rust_sdk::experiment::{Evaluation, RunEvaluatorInput, run_evaluator};
///
/// let average_accuracy = run_evaluator("avg_accuracy", |run: RunEvaluatorInput| async move {
///     let values: Vec<f64> = run
///         .item_results
///         .iter()
///         .flat_map(|item| &item.evaluations)
///         .filter(|e| e.name == "accuracy")
///         .filter_map(|e| e.value.as_f64())
///         .collect();
///     (!values.is_empty()).then(|| {
///         Evaluation::numeric("avg_accuracy", values.iter().sum::<f64>() / values.len() as f64)
///     })
/// });
/// # let _ = average_accuracy;
/// ```
pub fn run_evaluator<F, Fut>(name: impl Into<String>, f: F) -> FnEvaluator<F>
where
    F: Fn(RunEvaluatorInput) -> Fut + Send + Sync,
    Fut: Future + Send + 'static,
    Fut::Output: IntoEvaluations,
{
    FnEvaluator {
        name: name.into(),
        f,
    }
}

impl<F, Fut> Evaluator for FnEvaluator<F>
where
    F: Fn(EvaluatorInput) -> Fut + Send + Sync,
    Fut: Future + Send + 'static,
    Fut::Output: IntoEvaluations,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(&self, input: EvaluatorInput) -> BoxFuture<'_, Result<Vec<Evaluation>, String>> {
        let future = (self.f)(input);
        async move { future.await.into_evaluations() }.boxed()
    }
}

impl<F, Fut> CompositeEvaluator for FnEvaluator<F>
where
    F: Fn(CompositeEvaluatorInput) -> Fut + Send + Sync,
    Fut: Future + Send + 'static,
    Fut::Output: IntoEvaluations,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(
        &self,
        input: CompositeEvaluatorInput,
    ) -> BoxFuture<'_, Result<Vec<Evaluation>, String>> {
        let future = (self.f)(input);
        async move { future.await.into_evaluations() }.boxed()
    }
}

impl<F, Fut> RunEvaluator for FnEvaluator<F>
where
    F: Fn(RunEvaluatorInput) -> Fut + Send + Sync,
    Fut: Future + Send + 'static,
    Fut::Output: IntoEvaluations,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(&self, input: RunEvaluatorInput) -> BoxFuture<'_, Result<Vec<Evaluation>, String>> {
        let future = (self.f)(input);
        async move { future.await.into_evaluations() }.boxed()
    }
}

/// The code under test in an experiment: produces the output of one item.
///
/// Implemented for async closures `Fn(ExperimentItem) -> impl Future<Output =
/// Result<T, E>>` with `T: Serialize` (the output) and `E: Display` (a failed
/// item: its spans are marked as errors and it is left out of the results).
/// The closure may borrow from its surroundings (e.g. `&client`).
/// Observations started inside are children of the item's task span.
pub trait Task: Send + Sync {
    /// Run the task on `item`; returns its output as JSON, or the error message.
    fn run(&self, item: ExperimentItem) -> impl Future<Output = Result<Value, String>> + Send;
}

impl<F, Fut, T, E> Task for F
where
    F: Fn(ExperimentItem) -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, E>> + Send,
    T: Serialize,
    E: Display,
{
    fn run(&self, item: ExperimentItem) -> impl Future<Output = Result<Value, String>> + Send {
        let future = self(item);
        async move {
            let output = future.await.map_err(|err| err.to_string())?;
            serde_json::to_value(output)
                .map_err(|err| format!("task output cannot be serialized: {err}"))
        }
    }
}

/// One local data item of an experiment (not stored in Langfuse).
///
/// ```
/// use langfuse_rust_sdk::experiment::LocalExperimentItem;
/// use serde_json::json;
///
/// let item = LocalExperimentItem::new("What is the capital of France?")
///     .with_expected_output("Paris")
///     .with_metadata(json!({"difficulty": "easy"}));
/// # let _ = item;
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct LocalExperimentItem {
    /// Input given to the task; an item with a `null` input is skipped.
    pub input: Value,
    /// Expected output, for evaluators.
    pub expected_output: Option<Value>,
    /// Metadata of the item (an object is recorded on the item's spans).
    pub metadata: Option<Value>,
}

impl LocalExperimentItem {
    /// An item with `input`.
    pub fn new(input: impl Into<Value>) -> Self {
        Self {
            input: input.into(),
            expected_output: None,
            metadata: None,
        }
    }

    /// Set [`expected_output`](Self::expected_output).
    pub fn with_expected_output(mut self, expected_output: impl Into<Value>) -> Self {
        self.expected_output = Some(expected_output.into());
        self
    }

    /// Set [`metadata`](Self::metadata).
    pub fn with_metadata(mut self, metadata: impl Into<Value>) -> Self {
        self.metadata = Some(metadata.into());
        self
    }
}

/// One item of an experiment: local data or a Langfuse dataset item.
#[derive(Debug, Clone)]
pub enum ExperimentItem {
    /// Local data.
    Local(LocalExperimentItem),
    /// An item of a Langfuse dataset; the experiment is linked to its dataset.
    Dataset(Box<DatasetItem>),
}

impl ExperimentItem {
    /// The input, `None` when missing or `null`.
    pub fn input(&self) -> Option<&Value> {
        match self {
            ExperimentItem::Local(item) => Some(&item.input),
            ExperimentItem::Dataset(item) => item.input.as_ref(),
        }
        .filter(|v| !v.is_null())
    }

    /// The expected output, `None` when missing or `null`.
    pub fn expected_output(&self) -> Option<&Value> {
        match self {
            ExperimentItem::Local(item) => item.expected_output.as_ref(),
            ExperimentItem::Dataset(item) => item.expected_output.as_ref(),
        }
        .filter(|v| !v.is_null())
    }

    /// The metadata, `None` when missing or `null`.
    pub fn metadata(&self) -> Option<&Value> {
        match self {
            ExperimentItem::Local(item) => item.metadata.as_ref(),
            ExperimentItem::Dataset(item) => item.metadata.as_ref(),
        }
        .filter(|v| !v.is_null())
    }

    /// The dataset item, for dataset experiments.
    pub fn dataset_item(&self) -> Option<&DatasetItem> {
        match self {
            ExperimentItem::Local(_) => None,
            ExperimentItem::Dataset(item) => Some(item),
        }
    }
}

impl From<LocalExperimentItem> for ExperimentItem {
    fn from(item: LocalExperimentItem) -> Self {
        ExperimentItem::Local(item)
    }
}

impl From<DatasetItem> for ExperimentItem {
    fn from(item: DatasetItem) -> Self {
        ExperimentItem::Dataset(Box::new(item))
    }
}

/// Options of an experiment run; only the name is required.
///
/// ```
/// use langfuse_rust_sdk::experiment::{Evaluation, EvaluatorInput, ExperimentOptions, evaluator};
///
/// let options = ExperimentOptions::new("capital-cities")
///     .with_run_name("gpt-4o baseline")
///     .with_description("Country to capital")
///     .with_max_concurrency(10)
///     .with_evaluator(evaluator("exact_match", |args: EvaluatorInput| async move {
///         Evaluation::boolean("exact_match", args.expected_output == Some(args.output))
///     }));
/// assert_eq!(options.evaluators.len(), 1);
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct ExperimentOptions {
    /// Experiment name.
    pub name: String,
    /// Name of this run, shown as the experiment name in Langfuse (default:
    /// `"{name} - {start time, ISO 8601 UTC}"`). Runs of dataset items with
    /// the same run name on the same dataset share one experiment ID.
    pub run_name: Option<String>,
    /// Description of the run.
    pub description: Option<String>,
    /// Metadata of the run, recorded on every item's spans and shown as the
    /// experiment metadata.
    pub metadata: serde_json::Map<String, Value>,
    /// Items processed at the same time (default: 50).
    pub max_concurrency: usize,
    /// Record the run in this experiment instead of deriving its ID; calls
    /// with the same ID (e.g. one per worker) add to one experiment. Use
    /// [`create_trace_id`](crate::ids::create_trace_id) for a new one.
    pub experiment_id: Option<String>,
    /// Item evaluators, run in order on every item whose task succeeded.
    pub evaluators: Vec<Arc<dyn Evaluator>>,
    /// Evaluator combining the evaluations of each item.
    pub composite_evaluator: Option<Arc<dyn CompositeEvaluator>>,
    /// Evaluators of the whole run, run in order after every item.
    pub run_evaluators: Vec<Arc<dyn RunEvaluator>>,
}

impl fmt::Debug for ExperimentOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExperimentOptions")
            .field("name", &self.name)
            .field("run_name", &self.run_name)
            .field("description", &self.description)
            .field("metadata", &self.metadata)
            .field("max_concurrency", &self.max_concurrency)
            .field("experiment_id", &self.experiment_id)
            .field(
                "evaluators",
                &self.evaluators.iter().map(|e| e.name()).collect::<Vec<_>>(),
            )
            .field(
                "composite_evaluator",
                &self.composite_evaluator.as_ref().map(|e| e.name()),
            )
            .field(
                "run_evaluators",
                &self
                    .run_evaluators
                    .iter()
                    .map(|e| e.name())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ExperimentOptions {
    /// Options for an experiment named `name`, with the defaults.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            run_name: None,
            description: None,
            metadata: serde_json::Map::new(),
            max_concurrency: 50,
            experiment_id: None,
            evaluators: Vec::new(),
            composite_evaluator: None,
            run_evaluators: Vec::new(),
        }
    }

    /// Set [`name`](Self::name).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set [`run_name`](Self::run_name).
    pub fn with_run_name(mut self, run_name: impl Into<String>) -> Self {
        self.run_name = Some(run_name.into());
        self
    }

    /// Set [`description`](Self::description).
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
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
                tracing::warn!(key = %key, error = %err, "experiment metadata cannot be serialized; skipped");
            }
        }
        self
    }

    /// Set [`max_concurrency`](Self::max_concurrency).
    pub fn with_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = max_concurrency;
        self
    }

    /// Set [`experiment_id`](Self::experiment_id).
    pub fn with_experiment_id(mut self, experiment_id: impl Into<String>) -> Self {
        self.experiment_id = Some(experiment_id.into());
        self
    }

    /// Replace [`evaluators`](Self::evaluators).
    pub fn with_evaluators(mut self, evaluators: Vec<Arc<dyn Evaluator>>) -> Self {
        self.evaluators = evaluators;
        self
    }

    /// Add one item evaluator.
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

    /// Replace [`run_evaluators`](Self::run_evaluators).
    pub fn with_run_evaluators(mut self, evaluators: Vec<Arc<dyn RunEvaluator>>) -> Self {
        self.run_evaluators = evaluators;
        self
    }

    /// Add one run evaluator.
    pub fn with_run_evaluator(mut self, evaluator: impl RunEvaluator + 'static) -> Self {
        self.run_evaluators.push(Arc::new(evaluator));
        self
    }
}

/// Result of one item whose task succeeded.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExperimentItemResult {
    /// The item.
    pub item: ExperimentItem,
    /// Output of the task.
    pub output: Value,
    /// Evaluations of the item evaluators, then of the composite evaluator.
    pub evaluations: Vec<Evaluation>,
    /// Trace of the item's run (`None` when it was not recorded: tracing
    /// disabled or sampled out).
    pub trace_id: Option<String>,
    /// Experiment of the run.
    pub experiment_id: String,
}

/// Result of an experiment run.
///
/// [`format`](Self::format) (or `Display`; `{:#}` includes the items) prints
/// a summary:
///
/// ```text
/// ──────────────────────────────────────────────────
/// 🧪 Experiment: capital-cities
/// 📋 Run name: capital-cities - 2026-11-09T10:00:00Z
/// 3 items
/// Evaluations:
///   • exact_match
///
/// Average Scores:
///   • exact_match: 0.667
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExperimentResult {
    /// Experiment name.
    pub name: String,
    /// Run name.
    pub run_name: String,
    /// Run description.
    pub description: Option<String>,
    /// Results of the items whose task succeeded, in data order. Failed
    /// items are logged and left out.
    pub item_results: Vec<ExperimentItemResult>,
    /// Evaluations of the run evaluators.
    pub run_evaluations: Vec<Evaluation>,
    /// Experiment ID in Langfuse.
    pub experiment_id: String,
    /// URL of the experiment results in the Langfuse UI, when the project is known.
    pub experiment_url: Option<String>,
}

impl ExperimentResult {
    /// Human-readable report: run overview, evaluation names, average scores
    /// (numeric and boolean evaluations), run evaluations and the URL; with
    /// `include_item_results`, also the input, expected and actual output,
    /// scores and trace ID of every item.
    ///
    /// The text is the same as the Python SDK's, except that evaluation names
    /// are sorted and non-string values are printed as JSON.
    pub fn format(&self, include_item_results: bool) -> String {
        if self.item_results.is_empty() {
            return "No experiment results to display.".to_owned();
        }
        let mut out = String::new();
        if include_item_results {
            for (i, result) in self.item_results.iter().enumerate() {
                out += &format!("\n{}. Item {}:\n", i + 1, i + 1);
                if let Some(input) = result.item.input() {
                    out += &format!("   Input:    {}\n", format_value(input));
                }
                if let Some(expected) = result.item.expected_output() {
                    out += &format!("   Expected: {}\n", format_value(expected));
                }
                out += &format!("   Actual:   {}\n", format_value(&result.output));
                if !result.evaluations.is_empty() {
                    out += "   Scores:\n";
                    for evaluation in &result.evaluations {
                        out += &format!(
                            "     • {}: {}",
                            evaluation.name,
                            format_score(&evaluation.value)
                        );
                        if let Some(comment) =
                            evaluation.comment.as_deref().filter(|c| !c.is_empty())
                        {
                            out += &format!("\n       💭 {comment}");
                        }
                        out += "\n";
                    }
                }
                if let Some(trace_id) = &result.trace_id {
                    out += &format!("\n   Trace ID: {trace_id}\n");
                }
            }
        } else {
            out += &format!(
                "Individual Results: Hidden ({} items)\n",
                self.item_results.len()
            );
            out += "💡 Call format(true) to view them\n";
        }

        out += &format!("\n{}\n", "─".repeat(50));
        out += &format!("🧪 Experiment: {}", self.name);
        out += &format!("\n📋 Run name: {}", self.run_name);
        if let Some(description) = self.description.as_deref().filter(|d| !d.is_empty()) {
            out += &format!(" - {description}");
        }
        out += &format!("\n{} items", self.item_results.len());

        let names: BTreeSet<&str> = self
            .item_results
            .iter()
            .flat_map(|r| &r.evaluations)
            .map(|e| e.name.as_str())
            .collect();
        if !names.is_empty() {
            out += "\nEvaluations:";
            for name in &names {
                out += &format!("\n  • {name}");
            }
            out += "\n";
            out += "\nAverage Scores:";
            for name in &names {
                if let Some(average) = self.average(name) {
                    out += &format!("\n  • {name}: {average:.3}");
                }
            }
            out += "\n";
        }

        if !self.run_evaluations.is_empty() {
            out += "\nRun Evaluations:";
            for evaluation in &self.run_evaluations {
                out += &format!(
                    "\n  • {}: {}",
                    evaluation.name,
                    format_score(&evaluation.value)
                );
                if let Some(comment) = evaluation.comment.as_deref().filter(|c| !c.is_empty()) {
                    out += &format!("\n    💭 {comment}");
                }
            }
            out += "\n";
        }

        if let Some(url) = &self.experiment_url {
            out += &format!("\n🔗 Experiment:\n   {url}");
        }
        out
    }

    /// Average of the item evaluations named `name` (numeric values, and
    /// booleans as `1`/`0`); `None` without such evaluations.
    pub fn average(&self, name: &str) -> Option<f64> {
        let values: Vec<f64> = self
            .item_results
            .iter()
            .flat_map(|r| &r.evaluations)
            .filter(|e| e.name == name)
            .filter_map(|e| e.value.as_f64())
            .collect();
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    }
}

/// `{}` prints [`format(false)`](ExperimentResult::format), `{:#}` [`format(true)`](ExperimentResult::format).
impl Display for ExperimentResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format(f.alternate()))
    }
}

/// Strings cut to 50 characters (plus `...`); other values as compact JSON.
fn format_value(value: &Value) -> String {
    match value {
        Value::String(text) if text.chars().count() > 50 => {
            format!("{}...", text.chars().take(50).collect::<String>())
        }
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Numbers and booleans with three decimals, labels as-is.
fn format_score(value: &EvaluationValue) -> String {
    match value {
        EvaluationValue::Categorical(label) => label.clone(),
        other => format!("{:.3}", other.as_f64().unwrap_or_default()),
    }
}

/// Raised by your code to fail a CI run when an experiment regresses; the SDK
/// never returns it.
///
/// ```
/// # use langfuse_rust_sdk::experiment::{ExperimentResult, RegressionError};
/// fn gate(result: ExperimentResult) -> Result<ExperimentResult, RegressionError> {
///     let accuracy = result.average("accuracy").unwrap_or(0.0);
///     if accuracy < 0.9 {
///         return Err(RegressionError::new(result).with_metric("accuracy", accuracy, Some(0.9)));
///     }
///     Ok(result)
/// }
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RegressionError {
    /// The experiment that regressed.
    pub result: Box<ExperimentResult>,
    /// Metric that regressed.
    pub metric: Option<String>,
    /// Its value.
    pub value: Option<f64>,
    /// The threshold it missed.
    pub threshold: Option<f64>,
    /// Message, replacing the default one.
    pub message: Option<String>,
}

impl RegressionError {
    /// A regression of `result`, with the message `Experiment regression detected`.
    pub fn new(result: ExperimentResult) -> Self {
        Self {
            result: Box::new(result),
            metric: None,
            value: None,
            threshold: None,
            message: None,
        }
    }

    /// Set the metric, its value and the threshold; the message becomes
    /// ``Regression on `{metric}`: {value} (threshold {threshold})``.
    pub fn with_metric(
        mut self,
        metric: impl Into<String>,
        value: f64,
        threshold: Option<f64>,
    ) -> Self {
        self.metric = Some(metric.into());
        self.value = Some(value);
        self.threshold = threshold;
        self
    }

    /// Set [`message`](Self::message).
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

impl Display for RegressionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.message, &self.metric, self.value) {
            (Some(message), _, _) => f.write_str(message),
            (None, Some(metric), Some(value)) => {
                let threshold = self
                    .threshold
                    .map_or_else(|| "None".to_owned(), python_float_repr);
                write!(
                    f,
                    "Regression on `{metric}`: {} (threshold {threshold})",
                    python_float_repr(value)
                )
            }
            _ => f.write_str("Experiment regression detected"),
        }
    }
}

impl std::error::Error for RegressionError {}

/// ISO 8601 in UTC with `Z`, with microseconds only when not zero: Python's
/// `isoformat()` of a UTC datetime as the Python SDK sends it.
pub(crate) fn python_isoformat(time: DateTime<Utc>) -> String {
    let format = if time.timestamp_subsec_micros() == 0 {
        SecondsFormat::Secs
    } else {
        SecondsFormat::Micros
    };
    time.to_rfc3339_opts(format, true)
}

/// `repr()` of a Python float: the shortest round-trip digits, in scientific
/// notation below `1e-4` and from `1e16` (`1e-05`, `1e+16`), else with a
/// decimal point (`1.0`, `0.0001`).
pub(crate) fn python_float_repr(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "nan".to_owned()
        } else if value > 0.0 {
            "inf".to_owned()
        } else {
            "-inf".to_owned()
        };
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("LowerExp output has an exponent");
    let exponent: i32 = exponent.parse().expect("LowerExp exponent is an integer");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let point = exponent + 1;
    let body = if !(-4 < point && point <= 16) {
        let mut out = digits[..1].to_owned();
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{out}e{sign}{:02}", exponent.unsigned_abs())
    } else if point <= 0 {
        format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
    } else {
        let (int, frac) = digits.split_at(point as usize);
        format!("{int}.{frac}")
    };
    if value < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item_result(
        input: &str,
        output: Value,
        evaluations: Vec<Evaluation>,
    ) -> ExperimentItemResult {
        ExperimentItemResult {
            item: LocalExperimentItem::new(input)
                .with_expected_output("Paris")
                .into(),
            output,
            evaluations,
            trace_id: Some("abc".into()),
            experiment_id: "exp".into(),
        }
    }

    fn result(items: Vec<ExperimentItemResult>) -> ExperimentResult {
        ExperimentResult {
            name: "capitals".into(),
            run_name: "run-1".into(),
            description: Some("desc".into()),
            item_results: items,
            run_evaluations: vec![Evaluation::numeric("avg", 0.5).with_comment("half")],
            experiment_id: "exp".into(),
            experiment_url: Some("https://x/project/p/experiments/results?baseline=exp".into()),
        }
    }

    #[test]
    fn evaluations_serialize_with_their_data_type() {
        let evaluation = Evaluation::boolean("ok", true)
            .with_comment("c")
            .with_config_id("cfg");
        assert_eq!(
            serde_json::to_value(&evaluation).unwrap(),
            json!({"name": "ok", "value": true, "comment": "c", "data_type": "BOOLEAN", "config_id": "cfg"})
        );
        assert_eq!(
            serde_json::to_value(Evaluation::new("label", "good")).unwrap(),
            json!({"name": "label", "value": "good", "data_type": "CATEGORICAL"})
        );
        assert_eq!(Evaluation::new("n", 3).value, EvaluationValue::Numeric(3.0));
    }

    #[test]
    fn score_requests_follow_the_value() {
        let request = Evaluation::boolean("ok", false).to_score_request();
        assert!(matches!(request.value, CreateScoreValue::Numeric(v) if v == 0.0));
        assert_eq!(request.data_type, Some(ScoreDataType::Boolean));
        let request = Evaluation::categorical("", "x").to_score_request();
        assert_eq!(request.name, "<unknown>");
        assert!(matches!(request.value, CreateScoreValue::String(ref s) if s == "x"));
    }

    #[test]
    fn closures_may_return_several_shapes() {
        assert_eq!(
            Evaluation::numeric("a", 1.0)
                .into_evaluations()
                .unwrap()
                .len(),
            1
        );
        assert!(None::<Evaluation>.into_evaluations().unwrap().is_empty());
        assert_eq!(
            Err::<Evaluation, _>("boom").into_evaluations().unwrap_err(),
            "boom"
        );
        assert_eq!(
            Ok::<_, String>(vec![Evaluation::numeric("a", 1.0); 2])
                .into_evaluations()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn format_summarizes_the_run() {
        let r = result(vec![
            item_result(
                "France?",
                json!("Paris"),
                vec![
                    Evaluation::boolean("exact", true),
                    Evaluation::numeric("len", 5.0).with_comment("short"),
                ],
            ),
            item_result(
                "Italy?",
                json!({"answer": "Rome"}),
                vec![
                    Evaluation::boolean("exact", false),
                    Evaluation::categorical("tone", "ok"),
                ],
            ),
        ]);
        let expected = "Individual Results: Hidden (2 items)\n\
💡 Call format(true) to view them\n\
\n──────────────────────────────────────────────────\n\
🧪 Experiment: capitals\n\
📋 Run name: run-1 - desc\n\
2 items\n\
Evaluations:\n  • exact\n  • len\n  • tone\n\
\nAverage Scores:\n  • exact: 0.500\n  • len: 5.000\n\
\nRun Evaluations:\n  • avg: 0.500\n    💭 half\n\
\n🔗 Experiment:\n   https://x/project/p/experiments/results?baseline=exp";
        assert_eq!(r.format(false), expected);
        assert_eq!(r.to_string(), expected);
        assert_eq!(r.average("exact"), Some(0.5));
        assert_eq!(r.average("tone"), None);
    }

    #[test]
    fn format_with_items_shows_each_item() {
        let long = "x".repeat(60);
        let r = result(vec![item_result(
            &long,
            json!({"answer": "Rome"}),
            vec![Evaluation::numeric("len", 5.0).with_comment("short")],
        )]);
        let text = format!("{r:#}");
        let expected_start = format!(
            "\n1. Item 1:\n   Input:    {}...\n   Expected: Paris\n   Actual:   {{\"answer\":\"Rome\"}}\n   Scores:\n     • len: 5.000\n       💭 short\n\n   Trace ID: abc\n\n──",
            "x".repeat(50)
        );
        assert!(text.starts_with(&expected_start), "{text}");
        assert_eq!(
            result(Vec::new()).format(true),
            "No experiment results to display."
        );
    }

    #[test]
    fn regression_messages() {
        let r = result(Vec::new());
        assert_eq!(
            RegressionError::new(r.clone()).to_string(),
            "Experiment regression detected"
        );
        assert_eq!(
            RegressionError::new(r.clone())
                .with_metric("acc", 0.7, Some(0.9))
                .to_string(),
            "Regression on `acc`: 0.7 (threshold 0.9)"
        );
        assert_eq!(
            RegressionError::new(r.clone())
                .with_metric("acc", 1.0, None)
                .to_string(),
            "Regression on `acc`: 1.0 (threshold None)"
        );
        assert_eq!(
            RegressionError::new(r)
                .with_metric("acc", 0.7, Some(0.9))
                .with_message("too low")
                .to_string(),
            "too low"
        );
    }

    #[test]
    fn datetimes_print_like_python() {
        let time = DateTime::parse_from_rfc3339("2026-11-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(python_isoformat(time), "2026-11-09T10:00:00Z");
        let time = DateTime::parse_from_rfc3339("2026-11-09T10:00:00.120000500Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(python_isoformat(time), "2026-11-09T10:00:00.120000Z");
    }

    #[test]
    fn floats_print_like_python() {
        for (value, expected) in [
            (1.0, "1.0"),
            (0.5, "0.5"),
            (-2.25, "-2.25"),
            (123.456, "123.456"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1.2345e20, "1.2345e+20"),
            (0.1 + 0.2, "0.30000000000000004"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1e300, "1e+300"),
        ] {
            assert_eq!(python_float_repr(value), expected, "{value:e}");
        }
    }
}

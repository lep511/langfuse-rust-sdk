//! Runtime support of the `#[observe]` attribute (feature `macros`).
//!
//! Not a public API: the items are used by the code the macro generates and
//! may change in any release.

use std::any::type_name;
use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context as TaskContext, Poll};

use futures_util::stream::Stream;
use opentelemetry::Context;
use opentelemetry::context::FutureExt as _;
use serde::Serialize;
pub use serde_json::{Map, Value};

use super::observation::{Level, Observation, ObservationType};
use super::tracer::LangfuseTracer;

/// Environment variable that turns off input and output capture of
/// `#[observe]` when `false` or `0` (unless the attribute sets
/// `capture_input` / `capture_output`).
pub const ENV_OBSERVE_IO_CAPTURE_ENABLED: &str = "LANGFUSE_OBSERVE_DECORATOR_IO_CAPTURE_ENABLED";

/// [`ENV_OBSERVE_IO_CAPTURE_ENABLED`], read once.
fn io_capture_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        capture_enabled_from(
            std::env::var(ENV_OBSERVE_IO_CAPTURE_ENABLED)
                .ok()
                .as_deref(),
        )
    })
}

/// Capture is on unless the variable is `false` or `0` (any case).
fn capture_enabled_from(value: Option<&str>) -> bool {
    value.is_none_or(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "0"))
}

/// Holds a reference to a value for autoref-based dispatch on its traits.
///
/// The traits below take `&self`, so a call needs one more `&` than the
/// `Self` of the preferred impl: `(&&Wrap(&v)).__lf_capture()` and
/// `(&&&&Wrap(&v)).__lf_outcome(..)`. Method lookup then tries the impls from
/// the most to the least specific.
pub struct Wrap<T>(pub T);

/// What a function call (or one stream item) produced.
pub enum Outcome {
    /// A value, serialized (or a `"<type>"` marker), when output is captured.
    Value(Option<Value>),
    /// An `Err`, with the error's type name and `Display`.
    Error(&'static str, String),
}

fn to_value<T: Serialize + ?Sized>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or_else(|_| marker::<T>())
}

/// `"<type name>"`, recorded for values that are not `Serialize`.
fn marker<T: ?Sized>() -> Value {
    Value::String(format!("<{}>", type_name::<T>()))
}

/// Input capture of `Serialize` values.
pub trait CaptureSerialize {
    /// The value as JSON.
    fn __lf_capture(&self) -> Value;
}

impl<T: Serialize + ?Sized> CaptureSerialize for &Wrap<&T> {
    fn __lf_capture(&self) -> Value {
        to_value(self.0)
    }
}

/// Input capture fallback: the type name.
pub trait CaptureAny {
    /// A `"<type name>"` marker.
    fn __lf_capture(&self) -> Value;
}

impl<T: ?Sized> CaptureAny for Wrap<&T> {
    fn __lf_capture(&self) -> Value {
        marker::<T>()
    }
}

/// Outcome of a `Result` with a `Serialize` value and a `Display` error.
pub trait OutcomeResultSerialize {
    /// The outcome; the value is only serialized when `capture` is set.
    fn __lf_outcome(&self, capture: bool) -> Outcome;
}

impl<T: Serialize, E: Display> OutcomeResultSerialize for &&&Wrap<&Result<T, E>> {
    fn __lf_outcome(&self, capture: bool) -> Outcome {
        match self.0 {
            Ok(value) => Outcome::Value(capture.then(|| to_value(value))),
            Err(error) => Outcome::Error(type_name::<E>(), error.to_string()),
        }
    }
}

/// Outcome of a `Result` with a `Display` error.
pub trait OutcomeResult {
    /// The outcome; the value is recorded as a type marker.
    fn __lf_outcome(&self, capture: bool) -> Outcome;
}

impl<T, E: Display> OutcomeResult for &&Wrap<&Result<T, E>> {
    fn __lf_outcome(&self, capture: bool) -> Outcome {
        match self.0 {
            Ok(_) => Outcome::Value(capture.then(marker::<T>)),
            Err(error) => Outcome::Error(type_name::<E>(), error.to_string()),
        }
    }
}

/// Outcome of a `Serialize` value.
pub trait OutcomeSerialize {
    /// The outcome; the value is only serialized when `capture` is set.
    fn __lf_outcome(&self, capture: bool) -> Outcome;
}

impl<T: Serialize + ?Sized> OutcomeSerialize for &Wrap<&T> {
    fn __lf_outcome(&self, capture: bool) -> Outcome {
        Outcome::Value(capture.then(|| to_value(self.0)))
    }
}

/// Outcome of any other value.
pub trait OutcomeAny {
    /// The outcome; the value is recorded as a type marker.
    fn __lf_outcome(&self, capture: bool) -> Outcome;
}

impl<T: ?Sized> OutcomeAny for Wrap<&T> {
    fn __lf_outcome(&self, capture: bool) -> Outcome {
        Outcome::Value(capture.then(marker::<T>))
    }
}

/// The observation of one call of an observed function. Without a tracer
/// (no current observation, no global tracer) every method is a no-op.
pub struct Observer {
    observation: Option<Observation>,
    capture_output: bool,
    finished: bool,
}

impl Observer {
    /// Start the observation of a call: a child of the current observation
    /// with its tracer, else a new one with the global tracer, else nothing.
    /// `input` is only called when the observation records its input.
    pub fn start(
        name: &str,
        kind: ObservationType,
        capture_input: Option<bool>,
        capture_output: Option<bool>,
        input: impl FnOnce() -> Option<Value>,
    ) -> Self {
        let parent = Context::current();
        let observation = match Observation::current() {
            Some(current) => Some(Observation::start(
                current.shared(),
                name.to_owned(),
                kind,
                &parent,
                false,
            )),
            None => LangfuseTracer::global().map(|tracer| {
                Observation::start(tracer.shared(), name.to_owned(), kind, &parent, false)
            }),
        };
        if let Some(observation) = &observation
            && observation.is_recording()
            && capture_input.unwrap_or_else(io_capture_enabled)
            && let Some(input) = input()
        {
            observation.set_input(&input);
        }
        Self {
            observation,
            capture_output: capture_output.unwrap_or_else(io_capture_enabled),
            finished: false,
        }
    }

    /// Whether the result is recorded as output.
    pub fn captures_output(&self) -> bool {
        self.capture_output
    }

    /// Run `future` with the observation current.
    pub async fn scope<F: Future>(&self, future: F) -> F::Output {
        match &self.observation {
            Some(observation) => future.with_context(observation.context()).await,
            None => future.await,
        }
    }

    /// Make the observation current until the guard is dropped (sync functions).
    pub fn enter(&self) -> Option<opentelemetry::ContextGuard> {
        self.observation.as_ref().map(|o| o.context().attach())
    }

    /// Record the outcome of the call (computed only when it is recorded) and
    /// end the observation.
    pub fn finish(mut self, outcome: impl FnOnce(bool) -> Outcome) {
        self.finished = true;
        let Some(observation) = self.observation.take() else {
            return;
        };
        if observation.is_recording() {
            record(&observation, outcome(self.capture_output));
        }
        observation.end();
    }

    /// Keep the observation open until `stream` ends or is dropped, recording
    /// its items (see [`ObservedStream`]).
    pub fn wrap_stream<S, C>(
        mut self,
        stream: S,
        capture: C,
        transform: Option<fn(&[Value]) -> String>,
    ) -> ObservedStream<S, C>
    where
        S: Stream,
        C: Fn(&S::Item, bool) -> Outcome,
    {
        self.finished = true;
        ObservedStream {
            stream: Box::pin(stream),
            items: Items::new(self.observation.take(), self.capture_output, transform),
            capture,
        }
    }

    /// Keep the observation open until `iter` ends or is dropped, recording
    /// its items (see [`ObservedIter`]).
    pub fn wrap_iter<I, C>(
        mut self,
        iter: I,
        capture: C,
        transform: Option<fn(&[Value]) -> String>,
    ) -> ObservedIter<I::IntoIter, C>
    where
        I: IntoIterator,
        C: Fn(&I::Item, bool) -> Outcome,
    {
        self.finished = true;
        ObservedIter {
            iter: iter.into_iter(),
            items: Items::new(self.observation.take(), self.capture_output, transform),
            capture,
        }
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Some(observation) = self.observation.take() {
            // The call did not return: it panicked, or its future was dropped.
            let message = if std::thread::panicking() {
                "panicked"
            } else {
                "cancelled"
            };
            observation
                .set_level(Level::Error)
                .set_status_message(message);
            observation.end();
        }
    }
}

fn record(observation: &Observation, outcome: Outcome) {
    match outcome {
        Outcome::Value(Some(value)) if !value.is_null() => {
            observation.set_output(&value);
        }
        Outcome::Value(_) => {}
        Outcome::Error(type_name, message) => {
            observation.record_error_parts(type_name, message);
        }
    }
}

/// Items of an observed stream or iterator, and its observation.
struct Items {
    observation: Option<Observation>,
    capture: bool,
    transform: Option<fn(&[Value]) -> String>,
    values: Vec<Value>,
}

impl Items {
    fn new(
        observation: Option<Observation>,
        capture: bool,
        transform: Option<fn(&[Value]) -> String>,
    ) -> Self {
        let capture = capture && observation.as_ref().is_some_and(Observation::is_recording);
        Self {
            observation,
            capture,
            transform,
            values: Vec::new(),
        }
    }

    fn push(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Value(Some(value)) => self.values.push(value),
            Outcome::Value(None) => {}
            Outcome::Error(type_name, message) => {
                if let Some(observation) = &self.observation {
                    observation.record_error_parts(type_name, message);
                }
            }
        }
    }

    /// Record the output and end the observation: the transform's result, the
    /// concatenation when every item is a string, else the list.
    fn finish(&mut self) {
        let Some(observation) = self.observation.take() else {
            return;
        };
        if self.capture {
            let values = std::mem::take(&mut self.values);
            let output = match self.transform {
                Some(transform) => Value::String(transform(&values)),
                None if values.iter().all(Value::is_string) => {
                    Value::String(values.iter().filter_map(Value::as_str).collect::<String>())
                }
                None => Value::Array(values),
            };
            observation.set_output(&output);
        }
        observation.end();
    }
}

impl Drop for Items {
    fn drop(&mut self) {
        // Dropped before the end: record what was produced, like closing a
        // Python generator.
        self.finish();
    }
}

/// A stream returned by an observed function; the observation is current
/// while the stream is polled and ends with it.
pub struct ObservedStream<S, C> {
    stream: Pin<Box<S>>,
    items: Items,
    capture: C,
}

// The stream is boxed and nothing else is pinned, so moving is fine.
impl<S, C> Unpin for ObservedStream<S, C> {}

impl<S, C> Stream for ObservedStream<S, C>
where
    S: Stream,
    C: Fn(&S::Item, bool) -> Outcome,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<S::Item>> {
        let this = self.get_mut();
        let polled = {
            let _guard = this
                .items
                .observation
                .as_ref()
                .map(|o| o.context().attach());
            this.stream.as_mut().poll_next(cx)
        };
        match &polled {
            Poll::Ready(Some(item)) => {
                if this.items.observation.is_some() {
                    let outcome = (this.capture)(item, this.items.capture);
                    this.items.push(outcome);
                }
            }
            Poll::Ready(None) => this.items.finish(),
            Poll::Pending => {}
        }
        polled
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

/// An iterator returned by an observed function; the observation is current
/// while it produces items and ends with it.
pub struct ObservedIter<I, C> {
    iter: I,
    items: Items,
    capture: C,
}

impl<I, C> Iterator for ObservedIter<I, C>
where
    I: Iterator,
    C: Fn(&I::Item, bool) -> Outcome,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        let next = {
            let _guard = self
                .items
                .observation
                .as_ref()
                .map(|o| o.context().attach());
            self.iter.next()
        };
        match &next {
            Some(item) => {
                if self.items.observation.is_some() {
                    let outcome = (self.capture)(item, self.items.capture);
                    self.items.push(outcome);
                }
            }
            None => self.items.finish(),
        }
        next
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_capture_is_on_unless_false_or_zero() {
        for (value, expected) in [
            (None, true),
            (Some(""), true),
            (Some("true"), true),
            (Some("1"), true),
            (Some("False"), false),
            (Some(" 0 "), false),
        ] {
            assert_eq!(capture_enabled_from(value), expected, "{value:?}");
        }
    }

    #[test]
    // The extra borrows select the impl (autoref dispatch).
    #[allow(clippy::needless_borrow)]
    fn values_dispatch_on_their_traits() {
        struct Opaque;
        let number = 3_u32;
        assert_eq!((&&Wrap(&number)).__lf_capture(), serde_json::json!(3));
        let marker = (&&Wrap(&Opaque)).__lf_capture();
        assert!(marker.as_str().unwrap().ends_with("Opaque>"), "{marker}");

        let ok: Result<&str, std::fmt::Error> = Ok("fine");
        match (&&&&Wrap(&ok)).__lf_outcome(true) {
            Outcome::Value(Some(value)) => assert_eq!(value, "fine"),
            _ => panic!("expected a value"),
        }
        let err: Result<Opaque, std::fmt::Error> = Err(std::fmt::Error);
        match (&&&&Wrap(&err)).__lf_outcome(true) {
            Outcome::Error(type_name, message) => {
                assert_eq!(type_name, "core::fmt::Error");
                assert_eq!(message, "an error occurred when formatting an argument");
            }
            _ => panic!("expected an error"),
        }
        // Without capture, values are not serialized.
        assert!(matches!(
            (&&&&Wrap(&number)).__lf_outcome(false),
            Outcome::Value(None)
        ));
    }
}

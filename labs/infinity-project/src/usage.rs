//! Token accounting: what the model calls of the chat cost, for the footer of
//! each turn, the `/usage` command and the Langfuse traces.

use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, ready};

use async_trait::async_trait;
use futures_core::Stream;
use infinity_provider_protocol::{
    CompletionError, CompletionRequest, ModelEntry, ModelProvider, ModelStream, StreamChunk, Usage,
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The tokens of one or more completions, split the way they are billed.
///
/// `Usage::input_tokens` counts only the uncached part of the prompt: Bedrock
/// reports cache reads and cache writes separately and includes both in
/// `total_tokens`. The provider keeps cache reads (`cached_input_tokens`) but
/// drops cache writes, so they are recovered here as what the total leaves
/// unexplained. The provider puts a cache point on every request, so a first
/// run usually writes the whole prompt to the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tokens {
    /// Input that was neither read from nor written to the cache.
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
}

impl Tokens {
    pub fn of(usage: &Usage) -> Self {
        let cache_read = usage.cached_input_tokens;
        // `max` keeps the numbers consistent if a provider reports no total.
        let input = (usage.input_tokens + cache_read)
            .max(usage.total_tokens.saturating_sub(usage.output_tokens));
        Self {
            uncached_input: usage.input_tokens,
            cache_read,
            cache_write: input - usage.input_tokens - cache_read,
            output: usage.output_tokens,
        }
    }

    /// The whole prompt: uncached, read from the cache and written to it.
    pub fn input(&self) -> u64 {
        self.uncached_input + self.cache_read + self.cache_write
    }

    pub fn total(&self) -> u64 {
        self.input() + self.output
    }
}

/// What a number of model calls add up to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    /// The calls that reported their usage. One that fails or is cancelled
    /// reports none.
    pub calls: u64,
    pub usage: Usage,
}

impl Totals {
    /// What was added since `earlier`.
    pub fn since(&self, earlier: &Totals) -> Totals {
        Totals {
            calls: self.calls - earlier.calls,
            usage: Usage {
                input_tokens: self.usage.input_tokens - earlier.usage.input_tokens,
                output_tokens: self.usage.output_tokens - earlier.usage.output_tokens,
                total_tokens: self.usage.total_tokens - earlier.usage.total_tokens,
                cached_input_tokens: self.usage.cached_input_tokens
                    - earlier.usage.cached_input_tokens,
            },
        }
    }
}

/// Counts the usage of every call to a model, where the calls are made.
///
/// The agent runtime also reports usage, with the events of a thread, but
/// only that of the last call of a round: a round that runs a tool and calls
/// the model again with its result would count once.
#[derive(Clone, Default)]
pub struct UsageMeter(Arc<Mutex<Totals>>);

impl UsageMeter {
    /// What all the calls so far add up to.
    pub fn totals(&self) -> Totals {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wraps `provider` so that the usage of its calls is counted here.
    pub fn wrap(&self, provider: Arc<dyn ModelProvider>) -> Arc<dyn ModelProvider> {
        Arc::new(MeteredProvider {
            inner: provider,
            meter: self.clone(),
        })
    }

    fn record(&self, usage: &Usage) {
        let mut totals = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        totals.calls += 1;
        totals.usage.input_tokens += usage.input_tokens;
        totals.usage.output_tokens += usage.output_tokens;
        totals.usage.total_tokens += usage.total_tokens;
        totals.usage.cached_input_tokens += usage.cached_input_tokens;
    }
}

struct MeteredProvider {
    inner: Arc<dyn ModelProvider>,
    meter: UsageMeter,
}

#[async_trait]
impl ModelProvider for MeteredProvider {
    async fn list_models(&self) -> Result<Vec<ModelEntry>, BoxError> {
        self.inner.list_models().await
    }

    async fn invoke_model(
        &self,
        model_id: &str,
        request: CompletionRequest,
    ) -> Result<ModelStream, CompletionError> {
        let stream = self.inner.invoke_model(model_id, request).await?;
        Ok(Box::pin(MeteredStream {
            stream,
            meter: self.meter.clone(),
        }))
    }
}

/// A completion stream that reports the usage in its final chunk.
struct MeteredStream {
    stream: ModelStream,
    meter: UsageMeter,
}

impl Stream for MeteredStream {
    type Item = Result<StreamChunk, CompletionError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let item = ready!(this.stream.as_mut().poll_next(cx));
        if let Some(Ok(StreamChunk::Final(response))) = &item
            && let Some(usage) = &response.usage
        {
            this.meter.record(usage);
        }
        Poll::Ready(item)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll};

    use async_trait::async_trait;
    use futures_core::Stream;
    use infinity_provider_protocol::{
        CompletionError, CompletionRequest, ErrorClass, FinalResponse, ModelEntry, ModelProvider,
        ModelStream, StreamChunk, Usage,
    };

    use super::{BoxError, Tokens, Totals, UsageMeter};

    fn usage(input: u64, output: u64, total: u64, cache_read: u64) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            cached_input_tokens: cache_read,
        }
    }

    /// A provider whose calls stream the chunks they are given, one call
    /// after the other.
    struct Scripted(std::sync::Mutex<VecDeque<Vec<Result<StreamChunk, CompletionError>>>>);

    struct Chunks(VecDeque<Result<StreamChunk, CompletionError>>);

    impl Stream for Chunks {
        type Item = Result<StreamChunk, CompletionError>;

        fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    #[async_trait]
    impl ModelProvider for Scripted {
        async fn list_models(&self) -> Result<Vec<ModelEntry>, BoxError> {
            Ok(Vec::new())
        }

        async fn invoke_model(
            &self,
            _model_id: &str,
            _request: CompletionRequest,
        ) -> Result<ModelStream, CompletionError> {
            let chunks = self.0.lock().unwrap().pop_front().unwrap();
            Ok(Box::pin(Chunks(chunks.into())))
        }
    }

    /// Calls the model once and reads the stream to its end.
    async fn call(provider: &dyn ModelProvider) {
        let mut stream = provider
            .invoke_model("model", CompletionRequest::default())
            .await
            .unwrap();
        while std::future::poll_fn(|cx| stream.as_mut().poll_next(cx))
            .await
            .is_some()
        {}
    }

    #[tokio::test]
    async fn the_meter_counts_every_call_that_reports_usage() {
        let last = |usage| StreamChunk::Final(FinalResponse { usage });
        let script = vec![
            vec![
                Ok(StreamChunk::Text("Hi".to_owned())),
                Ok(last(Some(usage(4, 33, 2928, 2891)))),
            ],
            // A call that fails, and one whose provider reports no usage.
            vec![Err(CompletionError::provider(
                ErrorClass::Transient,
                "dropped",
            ))],
            vec![Ok(last(None))],
            vec![Ok(last(Some(usage(10, 20, 330, 200))))],
        ];
        let meter = UsageMeter::default();
        let provider = meter.wrap(Arc::new(Scripted(std::sync::Mutex::new(script.into()))));

        call(provider.as_ref()).await;
        let first = meter.totals();
        assert_eq!(
            first,
            Totals {
                calls: 1,
                usage: usage(4, 33, 2928, 2891),
            }
        );

        for _ in 0..3 {
            call(provider.as_ref()).await;
        }
        assert_eq!(
            meter.totals(),
            Totals {
                calls: 2,
                usage: usage(14, 53, 3258, 3091),
            }
        );
        assert_eq!(
            meter.totals().since(&first),
            Totals {
                calls: 1,
                usage: usage(10, 20, 330, 200),
            }
        );
    }

    #[test]
    fn cache_writes_are_what_the_total_leaves_unexplained() {
        let tokens = Tokens::of(&usage(10, 20, 330, 200));
        assert_eq!(
            tokens,
            Tokens {
                uncached_input: 10,
                cache_read: 200,
                cache_write: 100,
                output: 20,
            }
        );
        assert_eq!((tokens.input(), tokens.total()), (310, 330));
    }
}

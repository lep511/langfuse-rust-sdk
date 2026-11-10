//! Background work of [`LangfuseClient`](crate::LangfuseClient): batched score
//! sending and media uploads.
//!
//! Each worker is a Tokio task started on first use, fed through a bounded
//! channel; enqueuing never blocks and never fails the caller (problems are
//! logged). Workers hold only the HTTP client, so they do not keep the
//! `LangfuseClient` alive.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::api::media::MediaClient;
use crate::config::ScoreQueueOptions;
use crate::http::{CallOptions, HttpClient};
use crate::media::{LangfuseMedia, MediaTarget};
use crate::models::scores::{CreateScoreRequest, CreateScoresResponse};

/// Queue capacity of media uploads.
const MEDIA_QUEUE_SIZE: usize = 1_000;

enum ScoreCommand {
    Score(Box<CreateScoreRequest>),
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
}

enum MediaCommand {
    Upload(Box<(LangfuseMedia, MediaTarget)>),
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
}

pub(crate) struct Background {
    http: Arc<HttpClient>,
    options: ScoreQueueOptions,
    scores: Mutex<Option<mpsc::Sender<ScoreCommand>>>,
    media: Mutex<Option<mpsc::Sender<MediaCommand>>>,
    shut_down: AtomicBool,
}

impl Background {
    pub(crate) fn new(http: Arc<HttpClient>, options: ScoreQueueOptions) -> Self {
        Self {
            http,
            options,
            scores: Mutex::new(None),
            media: Mutex::new(None),
            shut_down: AtomicBool::new(false),
        }
    }

    pub(crate) fn enqueue_score(&self, score: CreateScoreRequest) {
        if self.shut_down.load(Ordering::Acquire) {
            tracing::warn!(score = %score.name, "Langfuse client is shut down; score dropped");
            return;
        }
        let http = Arc::clone(&self.http);
        let options = self.options.clone();
        let Some(sender) = start(&self.scores, self.options.max_queue_size, |rx| {
            run_scores(http, options, rx)
        }) else {
            tracing::error!(score = %score.name, "scores are sent by a Tokio task: queue them inside a Tokio runtime; score dropped");
            return;
        };
        match sender.try_send(ScoreCommand::Score(Box::new(score))) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    "score queue is full; score dropped (send less often or raise max_queue_size)"
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("score queue is closed; score dropped");
            }
        }
    }

    pub(crate) fn enqueue_media(&self, media: LangfuseMedia, target: MediaTarget) {
        if self.shut_down.load(Ordering::Acquire) {
            tracing::warn!(
                media = media.media_id(),
                "Langfuse client is shut down; media upload dropped"
            );
            return;
        }
        let client = MediaClient {
            http: Arc::clone(&self.http),
        };
        let Some(sender) = start(&self.media, MEDIA_QUEUE_SIZE, |rx| run_media(client, rx)) else {
            tracing::error!(
                media = media.media_id(),
                "media is uploaded by a Tokio task outside of any runtime; upload dropped"
            );
            return;
        };
        if sender
            .try_send(MediaCommand::Upload(Box::new((media, target))))
            .is_err()
        {
            tracing::warn!("media upload queue is full or closed; upload dropped");
        }
    }

    /// Send every queued score and finish every queued upload.
    pub(crate) async fn flush(&self) {
        if let Some(sender) = current(&self.scores) {
            let (ack, done) = oneshot::channel();
            if sender.send(ScoreCommand::Flush(ack)).await.is_ok() {
                let _ = done.await;
            }
        }
        if let Some(sender) = current(&self.media) {
            let (ack, done) = oneshot::channel();
            if sender.send(MediaCommand::Flush(ack)).await.is_ok() {
                let _ = done.await;
            }
        }
    }

    /// Flush, then stop both workers; later scores and uploads are dropped.
    pub(crate) async fn shutdown(&self) {
        self.shut_down.store(true, Ordering::Release);
        if let Some(sender) = current(&self.scores) {
            let (ack, done) = oneshot::channel();
            if sender.send(ScoreCommand::Shutdown(ack)).await.is_ok() {
                let _ = done.await;
            }
        }
        if let Some(sender) = current(&self.media) {
            let (ack, done) = oneshot::channel();
            if sender.send(MediaCommand::Shutdown(ack)).await.is_ok() {
                let _ = done.await;
            }
        }
    }
}

/// The sender of a running worker, if it was started.
fn current<C>(slot: &Mutex<Option<mpsc::Sender<C>>>) -> Option<mpsc::Sender<C>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// The sender of the worker in `slot`, spawning it first if needed. `None`
/// outside a Tokio runtime.
fn start<C, F, Fut>(
    slot: &Mutex<Option<mpsc::Sender<C>>>,
    capacity: usize,
    worker: F,
) -> Option<mpsc::Sender<C>>
where
    F: FnOnce(mpsc::Receiver<C>) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(sender) = slot.as_ref() {
        return Some(sender.clone());
    }
    let runtime = tokio::runtime::Handle::try_current().ok()?;
    let (sender, receiver) = mpsc::channel(capacity);
    runtime.spawn(worker(receiver));
    *slot = Some(sender.clone());
    Some(sender)
}

async fn run_scores(
    http: Arc<HttpClient>,
    options: ScoreQueueOptions,
    mut commands: mpsc::Receiver<ScoreCommand>,
) {
    let mut batch = ScoreBatch::new(options.max_batch_bytes);
    let mut ticker = tokio::time::interval(options.flush_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // the first tick completes immediately
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(ScoreCommand::Score(score)) => {
                    if let Some(full) = batch.push(&score) {
                        send_scores(&http, full).await;
                    }
                    if batch.len() >= options.flush_at {
                        send_scores(&http, batch.take()).await;
                    }
                }
                Some(ScoreCommand::Flush(ack)) => {
                    send_scores(&http, batch.take()).await;
                    let _ = ack.send(());
                }
                Some(ScoreCommand::Shutdown(ack)) => {
                    while let Ok(ScoreCommand::Score(score)) = commands.try_recv() {
                        if let Some(full) = batch.push(&score) {
                            send_scores(&http, full).await;
                        }
                    }
                    send_scores(&http, batch.take()).await;
                    commands.close();
                    let _ = ack.send(());
                    return;
                }
                None => {
                    send_scores(&http, batch.take()).await;
                    return;
                }
            },
            _ = ticker.tick() => {
                if batch.len() > 0 {
                    send_scores(&http, batch.take()).await;
                }
            }
        }
    }
}

/// Scores serialized for one request, kept under a body size limit.
struct ScoreBatch {
    max_bytes: usize,
    items: Vec<Value>,
    /// JSON size of the array body: brackets, items and separating commas.
    bytes: usize,
}

impl ScoreBatch {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            items: Vec::new(),
            bytes: 2,
        }
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn take(&mut self) -> Vec<Value> {
        self.bytes = 2;
        std::mem::take(&mut self.items)
    }

    /// Add a score; returns the previous items when they have to be sent
    /// first to stay under the limit. Oversized scores are dropped.
    fn push(&mut self, score: &CreateScoreRequest) -> Option<Vec<Value>> {
        let value = match serde_json::to_value(score) {
            Ok(value) => value,
            Err(err) => {
                tracing::error!(score = %score.name, error = %err, "score cannot be serialized; dropped");
                return None;
            }
        };
        let size = value.to_string().len();
        if size + 2 > self.max_bytes {
            tracing::error!(score = %score.name, bytes = size, limit = self.max_bytes, "score exceeds the request size limit; dropped");
            return None;
        }
        let full =
            (!self.items.is_empty() && self.bytes + size + 1 > self.max_bytes).then(|| self.take());
        self.bytes += size + usize::from(!self.items.is_empty());
        self.items.push(value);
        full
    }
}

async fn send_scores(http: &HttpClient, batch: Vec<Value>) {
    if batch.is_empty() {
        return;
    }
    // Every queued score has an ID and the endpoint upserts by ID, so a
    // repeated batch does not duplicate scores: retry it like an idempotent
    // request.
    let options = CallOptions {
        idempotent: Some(true),
        ..CallOptions::default()
    };
    match http
        .post_with_options::<CreateScoresResponse, _>("/api/public/scores", &batch, options)
        .await
    {
        Ok(CreateScoresResponse::Accepted(_)) => {
            tracing::debug!(scores = batch.len(), "sent scores to Langfuse");
        }
        Ok(CreateScoresResponse::BatchResults(results)) => {
            let errors: Vec<&str> = results.errors.iter().map(|e| e.message.as_str()).collect();
            tracing::error!(
                accepted = results.accepted,
                rejected = results.rejected,
                errors = %errors.join("; "),
                "Langfuse rejected some scores; rejected scores are not retried"
            );
        }
        Err(err) => {
            tracing::error!(scores = batch.len(), error = %err, "failed to send scores to Langfuse; scores dropped");
        }
    }
}

async fn run_media(client: MediaClient, mut commands: mpsc::Receiver<MediaCommand>) {
    while let Some(command) = commands.recv().await {
        match command {
            MediaCommand::Upload(job) => {
                let (media, target) = *job;
                if let Err(err) = media.upload_with(&client, &target).await {
                    tracing::warn!(media = media.media_id(), error = %err, "media upload failed");
                }
            }
            MediaCommand::Flush(ack) => {
                let _ = ack.send(());
            }
            MediaCommand::Shutdown(ack) => {
                while let Ok(MediaCommand::Upload(job)) = commands.try_recv() {
                    let (media, target) = *job;
                    if let Err(err) = media.upload_with(&client, &target).await {
                        tracing::warn!(media = media.media_id(), error = %err, "media upload failed");
                    }
                }
                commands.close();
                let _ = ack.send(());
                return;
            }
        }
    }
}

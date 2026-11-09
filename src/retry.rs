//! Retry policy with exponential backoff and jitter.

use std::time::Duration;

use rand::Rng;
use reqwest::StatusCode;

const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(60);
const DEFAULT_MAX_RETRIES: u32 = 2;
const DEFAULT_JITTER_FRACTION: f64 = 0.2;

/// Configures how failed requests are retried.
///
/// Retryable HTTP statuses: 408 (Request Timeout), 429 (Too Many Requests),
/// and 5xx. Status 409 (Conflict) is **not** retried because it signals a
/// real conflict that a retry cannot fix.
///
/// The default policy retries up to 2 times with 1 s initial backoff, 60 s
/// maximum backoff and 20 % jitter.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Base delay before the first retry (default: 1 s).
    pub initial_backoff: Duration,
    /// Upper bound on the computed delay (default: 60 s).
    pub max_backoff: Duration,
    /// Maximum number of retry attempts (default: 2).
    pub max_retries: u32,
    /// Fraction of the delay added/subtracted as random jitter (default: 0.2).
    pub jitter_fraction: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            max_retries: DEFAULT_MAX_RETRIES,
            jitter_fraction: DEFAULT_JITTER_FRACTION,
        }
    }
}

impl RetryPolicy {
    /// Whether a failed request with `status` should be retried on attempt `attempt`.
    ///
    /// Retryable statuses: 408, 429 and 5xx. 409 is not retried because it signals
    /// a real conflict (duplicate id, resource still referenced) that a retry cannot fix.
    pub fn should_retry(&self, status: StatusCode, attempt: u32) -> bool {
        if attempt >= self.max_retries {
            return false;
        }
        matches!(status.as_u16(), 408 | 429 | 500..=599)
    }

    /// Compute the delay before the next retry attempt.
    ///
    /// If `retry_after` is `Some`, the server's suggestion is used (clamped
    /// to [`max_backoff`](Self::max_backoff)).  Otherwise the delay is
    /// `initial_backoff * 2^attempt` with random jitter.
    pub fn delay(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        if let Some(server_delay) = retry_after {
            return server_delay.min(self.max_backoff);
        }

        let base = self.initial_backoff.as_secs_f64() * 2_f64.powi(attempt as i32);
        let clamped = base.min(self.max_backoff.as_secs_f64());

        let jitter_range = clamped * self.jitter_fraction;
        let jitter = rand::rng().random_range(-jitter_range..=jitter_range);
        let final_secs = (clamped + jitter).max(0.0);

        Duration::from_secs_f64(final_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_statuses() {
        let policy = RetryPolicy::default();
        assert!(policy.should_retry(StatusCode::TOO_MANY_REQUESTS, 0));
        assert!(policy.should_retry(StatusCode::REQUEST_TIMEOUT, 0));
        assert!(policy.should_retry(StatusCode::INTERNAL_SERVER_ERROR, 0));
        assert!(policy.should_retry(StatusCode::BAD_GATEWAY, 0));
        assert!(policy.should_retry(StatusCode::SERVICE_UNAVAILABLE, 0));
    }

    #[test]
    fn non_retryable_statuses() {
        let policy = RetryPolicy::default();
        assert!(!policy.should_retry(StatusCode::BAD_REQUEST, 0));
        assert!(!policy.should_retry(StatusCode::UNAUTHORIZED, 0));
        assert!(!policy.should_retry(StatusCode::FORBIDDEN, 0));
        assert!(!policy.should_retry(StatusCode::NOT_FOUND, 0));
        assert!(!policy.should_retry(StatusCode::CONFLICT, 0));
        assert!(!policy.should_retry(StatusCode::PRECONDITION_FAILED, 0));
    }

    #[test]
    fn max_retries_respected() {
        let policy = RetryPolicy::default();
        assert!(policy.should_retry(StatusCode::INTERNAL_SERVER_ERROR, 0));
        assert!(policy.should_retry(StatusCode::INTERNAL_SERVER_ERROR, 1));
        assert!(!policy.should_retry(StatusCode::INTERNAL_SERVER_ERROR, 2));
    }

    #[test]
    fn delay_increases_with_attempts() {
        let policy = RetryPolicy {
            jitter_fraction: 0.0,
            ..Default::default()
        };
        let d0 = policy.delay(0, None);
        let d1 = policy.delay(1, None);
        assert!(d1 > d0);
    }

    #[test]
    fn retry_after_takes_precedence() {
        let policy = RetryPolicy::default();
        let server_delay = Duration::from_secs(5);
        let delay = policy.delay(0, Some(server_delay));
        assert_eq!(delay, server_delay);
    }

    #[test]
    fn delay_clamped_to_max() {
        let policy = RetryPolicy {
            max_backoff: Duration::from_secs(10),
            jitter_fraction: 0.0,
            ..Default::default()
        };
        let delay = policy.delay(20, None);
        assert!(delay <= Duration::from_secs(10));
    }
}

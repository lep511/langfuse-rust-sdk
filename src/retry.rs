use std::time::Duration;

use rand::Rng;
use reqwest::StatusCode;

const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(60);
const DEFAULT_MAX_RETRIES: u32 = 2;
const DEFAULT_JITTER_FRACTION: f64 = 0.2;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_retries: u32,
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
    pub fn should_retry(&self, status: StatusCode, attempt: u32) -> bool {
        if attempt >= self.max_retries {
            return false;
        }
        matches!(
            status.as_u16(),
            408 | 409 | 429 | 500..=599
        )
    }

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
        assert!(policy.should_retry(StatusCode::CONFLICT, 0));
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

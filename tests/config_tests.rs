use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError, RetryPolicy};
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════
//  Config builder
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn default_base_url_is_cloud_langfuse() {
    let config = ClientConfig::builder("pk", "sk").build().unwrap();
    assert_eq!(config.base_url().as_str(), "https://cloud.langfuse.com/");
}

#[test]
fn default_timeout_is_30_seconds() {
    let config = ClientConfig::builder("pk", "sk").build().unwrap();
    assert_eq!(config.timeout(), Duration::from_secs(30));
}

#[test]
fn custom_base_url() {
    let config = ClientConfig::builder("pk", "sk")
        .base_url("https://langfuse.example.com")
        .build()
        .unwrap();
    assert_eq!(config.base_url().as_str(), "https://langfuse.example.com/");
}

#[test]
fn custom_base_url_with_trailing_slash() {
    let config = ClientConfig::builder("pk", "sk")
        .base_url("https://langfuse.example.com/")
        .build()
        .unwrap();
    assert_eq!(config.base_url().as_str(), "https://langfuse.example.com/");
}

#[test]
fn custom_base_url_with_path() {
    let config = ClientConfig::builder("pk", "sk")
        .base_url("https://langfuse.example.com/v2/")
        .build()
        .unwrap();
    assert_eq!(
        config.base_url().as_str(),
        "https://langfuse.example.com/v2/"
    );
}

#[test]
fn custom_timeout() {
    let config = ClientConfig::builder("pk", "sk")
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap();
    assert_eq!(config.timeout(), Duration::from_secs(120));
}

#[test]
fn invalid_url_returns_config_error() {
    let result = ClientConfig::builder("pk", "sk")
        .base_url("not a url")
        .build();
    assert!(result.is_err());
    let err = result.unwrap_err();
    match &err {
        LangfuseError::Config(msg) => {
            assert!(msg.contains("Invalid base URL"));
            assert!(msg.contains("not a url"));
        }
        other => panic!("Expected Config error, got {other:?}"),
    }
}

#[test]
fn empty_base_url_is_error() {
    let result = ClientConfig::builder("pk", "sk").base_url("").build();
    assert!(result.is_err());
}

#[test]
fn custom_retry_policy() {
    let policy = RetryPolicy {
        initial_backoff: Duration::from_millis(500),
        max_backoff: Duration::from_secs(30),
        max_retries: 5,
        jitter_fraction: 0.1,
    };
    let config = ClientConfig::builder("pk", "sk")
        .retry_policy(policy)
        .build()
        .unwrap();
    // Config builds successfully with custom policy — used internally by HttpClient
    assert_eq!(config.timeout(), Duration::from_secs(30));
}

#[test]
fn custom_headers() {
    let config = ClientConfig::builder("pk", "sk")
        .header("x-custom-header", "custom-value")
        .header("x-another", "another-value")
        .build()
        .unwrap();
    // Config builds successfully with custom headers
    assert_eq!(config.base_url().as_str(), "https://cloud.langfuse.com/");
}

#[test]
fn builder_chaining() {
    let config = ClientConfig::builder("pk-lf-abc", "sk-lf-xyz")
        .base_url("https://self-hosted.example.com")
        .timeout(Duration::from_secs(60))
        .retry_policy(RetryPolicy {
            max_retries: 3,
            ..Default::default()
        })
        .header("x-custom", "val")
        .build()
        .unwrap();
    assert_eq!(
        config.base_url().as_str(),
        "https://self-hosted.example.com/"
    );
    assert_eq!(config.timeout(), Duration::from_secs(60));
}

// ═══════════════════════════════════════════════════════════════════════
//  LangfuseClient construction
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn client_construction_succeeds() {
    let config = ClientConfig::builder("pk-test", "sk-test").build().unwrap();
    let client = LangfuseClient::new(config);
    assert!(client.is_ok());
}

#[test]
fn client_with_custom_config() {
    let config = ClientConfig::builder("pk-test", "sk-test")
        .base_url("https://custom.langfuse.com")
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let client = LangfuseClient::new(config).unwrap();
    // Verify all sub-clients are accessible
    let _ = &client.health;
    let _ = &client.scores;
    let _ = &client.score_configs;
    let _ = &client.observations;
    let _ = &client.datasets;
    let _ = &client.dataset_items;
    let _ = &client.prompts;
    let _ = &client.prompt_versions;
    let _ = &client.models;
    let _ = &client.skills;
    let _ = &client.projects;
    let _ = &client.traces;
    let _ = &client.ingestion;
    let _ = &client.media;
    let _ = &client.comments;
    let _ = &client.feedback;
    let _ = &client.annotation_queues;
    let _ = &client.blob_storage_integrations;
    let _ = &client.evaluation_rules;
    let _ = &client.evaluators;
    let _ = &client.experiments;
    let _ = &client.llm_connections;
    let _ = &client.metrics;
    let _ = &client.organizations;
    let _ = &client.scim;
}

// ═══════════════════════════════════════════════════════════════════════
//  Error display
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn error_display_config() {
    let err = LangfuseError::Config("bad setting".into());
    assert_eq!(err.to_string(), "Invalid configuration: bad setting");
}

#[test]
fn error_display_not_found() {
    let err = LangfuseError::NotFound {
        message: "Skill not found".into(),
        body: None,
    };
    assert_eq!(err.to_string(), "Not found (404): Skill not found");
}

#[test]
fn error_display_bad_request() {
    let err = LangfuseError::BadRequest {
        message: "Invalid input".into(),
        body: Some(serde_json::json!({"message": "Invalid input", "details": "name required"})),
    };
    assert_eq!(err.to_string(), "Bad request (400): Invalid input");
}

#[test]
fn error_display_unauthorized() {
    let err = LangfuseError::Unauthorized {
        message: "Bad credentials".into(),
        body: None,
    };
    assert_eq!(err.to_string(), "Unauthorized (401): Bad credentials");
}

#[test]
fn error_display_access_denied() {
    let err = LangfuseError::AccessDenied {
        message: "No access".into(),
        body: None,
    };
    assert_eq!(err.to_string(), "Access denied (403): No access");
}

#[test]
fn error_display_method_not_allowed() {
    let err = LangfuseError::MethodNotAllowed {
        message: "Use POST".into(),
        body: None,
    };
    assert_eq!(err.to_string(), "Method not allowed (405): Use POST");
}

#[test]
fn error_display_rate_limited() {
    let err = LangfuseError::RateLimited {
        retry_after: Some(Duration::from_secs(30)),
        body: None,
    };
    let display = err.to_string();
    assert!(display.contains("429"));
    assert!(display.contains("30"));
}

#[test]
fn error_display_rate_limited_no_retry_after() {
    let err = LangfuseError::RateLimited {
        retry_after: None,
        body: None,
    };
    let display = err.to_string();
    assert!(display.contains("429"));
    assert!(display.contains("None"));
}

#[test]
fn error_display_server_error() {
    let err = LangfuseError::ServerError {
        status: 503,
        message: "Service unavailable".into(),
        body: None,
    };
    assert_eq!(err.to_string(), "Server error (503): Service unavailable");
}

#[test]
fn error_display_serialization() {
    let err: Result<serde_json::Value, _> = serde_json::from_str("not json");
    let serde_err = err.unwrap_err();
    let langfuse_err = LangfuseError::Serialization(serde_err);
    let display = langfuse_err.to_string();
    assert!(display.starts_with("JSON serialization error:"));
}

#[test]
fn error_display_url_parse() {
    let url_err = url::Url::parse("://bad").unwrap_err();
    let err = LangfuseError::UrlParse(url_err);
    let display = err.to_string();
    assert!(display.starts_with("URL parse error:"));
}

// ═══════════════════════════════════════════════════════════════════════
//  RetryPolicy (integration-level from public API)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn retry_policy_default_values() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.initial_backoff, Duration::from_secs(1));
    assert_eq!(policy.max_backoff, Duration::from_secs(60));
    assert_eq!(policy.max_retries, 2);
    assert!((policy.jitter_fraction - 0.2).abs() < f64::EPSILON);
}

#[test]
fn retry_policy_custom_values() {
    let policy = RetryPolicy {
        initial_backoff: Duration::from_millis(200),
        max_backoff: Duration::from_secs(10),
        max_retries: 5,
        jitter_fraction: 0.0,
    };
    assert_eq!(policy.max_retries, 5);
    assert_eq!(policy.initial_backoff, Duration::from_millis(200));

    // With 0 jitter, delay should be exactly 200ms * 2^0 = 200ms for attempt 0
    let d0 = policy.delay(0, None);
    assert_eq!(d0, Duration::from_millis(200));
    // attempt 1: 200ms * 2^1 = 400ms
    let d1 = policy.delay(1, None);
    assert_eq!(d1, Duration::from_millis(400));
    // attempt 2: 200ms * 2^2 = 800ms
    let d2 = policy.delay(2, None);
    assert_eq!(d2, Duration::from_millis(800));
}

#[test]
fn retry_policy_zero_retries() {
    let policy = RetryPolicy {
        max_retries: 0,
        ..Default::default()
    };
    use reqwest::StatusCode;
    assert!(!policy.should_retry(StatusCode::INTERNAL_SERVER_ERROR, 0));
    assert!(!policy.should_retry(StatusCode::TOO_MANY_REQUESTS, 0));
}

#[test]
fn retry_policy_retry_after_clamped_to_max_backoff() {
    let policy = RetryPolicy {
        max_backoff: Duration::from_secs(5),
        ..Default::default()
    };
    let delay = policy.delay(0, Some(Duration::from_secs(100)));
    assert_eq!(delay, Duration::from_secs(5));
}

#[test]
fn retry_policy_jitter_stays_in_bounds() {
    let policy = RetryPolicy {
        jitter_fraction: 0.2,
        initial_backoff: Duration::from_secs(1),
        max_backoff: Duration::from_secs(60),
        max_retries: 10,
    };
    for attempt in 0..5 {
        for _ in 0..20 {
            let delay = policy.delay(attempt, None);
            let base = 1.0_f64 * 2_f64.powi(attempt as i32);
            let clamped = base.min(60.0);
            let lower = clamped * 0.8; // -20% jitter
            let upper = clamped * 1.2; // +20% jitter
            assert!(
                delay.as_secs_f64() >= lower - 0.001 && delay.as_secs_f64() <= upper + 0.001,
                "attempt {attempt}: delay {:?} out of range [{lower}, {upper}]",
                delay
            );
        }
    }
}

#[cfg(feature = "unstable")]
#[test]
fn unstable_sub_clients_exist_with_feature() {
    let config = ClientConfig::builder("pk-test", "sk-test").build().unwrap();
    let client = LangfuseClient::new(config).unwrap();
    let _ = &client.unstable_dashboard_widgets;
    let _ = &client.unstable_dashboards;
    let _ = &client.unstable_topics;
}

//! Credentials, host and HTTP settings, and the SDK clients built from them.
//!
//! Precedence (as in the official CLI): flag > `--env` file > process
//! environment > default. [`resolve`] takes a lookup function so it is tested
//! without touching the process environment.

use std::collections::HashMap;
use std::time::Duration;

use langfuse_rust_sdk::http::HttpClient;
use langfuse_rust_sdk::{ClientConfig, ConfigBuilder, LangfuseClient, RetryPolicy};

use crate::cli::GlobalArgs;
use crate::error::{CliError, Result};

/// Host used when neither `--host` nor the environment names one.
pub const DEFAULT_HOST: &str = "https://cloud.langfuse.com";
/// Request timeout used when `--timeout` is not given.
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// Retries used when `--max-retries` is not given (the SDK's default policy).
pub const DEFAULT_MAX_RETRIES: u32 = 2;
/// `user-agent` of every request.
pub const USER_AGENT: &str = concat!("langfuse-rust-cli/", env!("CARGO_PKG_VERSION"));

/// Resolved connection settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Public key, if any source provided a non-empty one.
    pub public_key: Option<String>,
    /// Secret key, if any source provided a non-empty one.
    pub secret_key: Option<String>,
    /// Host without trailing slashes.
    pub host: String,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Retries of failed requests.
    pub max_retries: u32,
}

/// Resolve the settings from the flags and a variable lookup (the `--env`
/// file layered over the process environment).
pub fn resolve(globals: &GlobalArgs, lookup: impl Fn(&str) -> Option<String>) -> Settings {
    let non_empty = |value: Option<String>| value.filter(|value| !value.is_empty());
    let var = |name: &str| non_empty(lookup(name));
    let host = non_empty(globals.host.clone())
        .or_else(|| var("LANGFUSE_BASE_URL"))
        .or_else(|| var("LANGFUSE_HOST"))
        .unwrap_or_else(|| DEFAULT_HOST.to_owned());
    Settings {
        public_key: non_empty(globals.public_key.clone()).or_else(|| var("LANGFUSE_PUBLIC_KEY")),
        secret_key: non_empty(globals.secret_key.clone()).or_else(|| var("LANGFUSE_SECRET_KEY")),
        host: host.trim_end_matches('/').to_owned(),
        timeout: Duration::from_millis(globals.timeout.unwrap_or(DEFAULT_TIMEOUT_MS)),
        max_retries: globals.max_retries.unwrap_or(DEFAULT_MAX_RETRIES),
    }
}

/// Read the `--env` file (if any) and resolve the settings against it and the
/// process environment.
pub fn load(globals: &GlobalArgs) -> Result<Settings> {
    let mut file_vars = HashMap::new();
    if let Some(path) = &globals.env {
        let entries = dotenvy::from_path_iter(path).map_err(|error| {
            CliError::config(format!(
                "Cannot read --env file {}: {}",
                path.display(),
                env_error(&error)
            ))
        })?;
        for entry in entries {
            let (key, value) = entry.map_err(|error| {
                CliError::config(format!(
                    "Cannot read --env file {}: {}",
                    path.display(),
                    env_error(&error)
                ))
            })?;
            file_vars.insert(key, value);
        }
    }
    Ok(resolve(globals, |name| {
        file_vars
            .get(name)
            .cloned()
            .or_else(|| std::env::var(name).ok())
    }))
}

fn env_error(error: &dotenvy::Error) -> String {
    match error {
        dotenvy::Error::Io(io) => io.to_string(),
        other => other.to_string(),
    }
}

impl Settings {
    /// Fail with exit 3 unless both keys are set.
    pub fn require_credentials(&self) -> Result<()> {
        if self.public_key.is_some() && self.secret_key.is_some() {
            Ok(())
        } else {
            Err(CliError::config(
                "This operation requires LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY \
                 (or --public-key and --secret-key)",
            ))
        }
    }

    /// The SDK configuration builder for these settings. Empty keys are
    /// allowed: unauthenticated operations (`health get`) still work.
    pub fn builder(&self) -> ConfigBuilder {
        ClientConfig::builder(
            self.public_key.clone().unwrap_or_default(),
            self.secret_key.clone().unwrap_or_default(),
        )
        .base_url(self.host.clone())
        .timeout(self.timeout)
        .retry_policy(RetryPolicy {
            max_retries: self.max_retries,
            ..RetryPolicy::default()
        })
        .header("user-agent", USER_AGENT)
        .header("accept", "application/json")
    }

    /// The SDK's low-level HTTP client (auth, retries, errors) used by `api`.
    pub fn http_client(&self) -> Result<HttpClient> {
        Ok(HttpClient::new(self.builder().build()?)?)
    }

    /// The SDK's typed client, used by `skills` and `api versions detect`.
    pub fn client(&self) -> Result<LangfuseClient> {
        Ok(LangfuseClient::new(self.builder().build()?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| vars.get(name).cloned()
    }

    #[test]
    fn defaults_without_any_source() {
        let settings = resolve(&GlobalArgs::default(), lookup(&[]));
        assert_eq!(settings.host, DEFAULT_HOST);
        assert_eq!(settings.public_key, None);
        assert_eq!(settings.timeout, Duration::from_millis(DEFAULT_TIMEOUT_MS));
        assert_eq!(settings.max_retries, DEFAULT_MAX_RETRIES);
        assert!(settings.require_credentials().is_err());
    }

    #[test]
    fn flags_win_over_variables() {
        let globals = GlobalArgs {
            public_key: Some("pk-flag".into()),
            host: Some("https://flag.example/".into()),
            timeout: Some(1500),
            max_retries: Some(0),
            ..GlobalArgs::default()
        };
        let settings = resolve(
            &globals,
            lookup(&[
                ("LANGFUSE_PUBLIC_KEY", "pk-env"),
                ("LANGFUSE_SECRET_KEY", "sk-env"),
                ("LANGFUSE_BASE_URL", "https://env.example"),
            ]),
        );
        assert_eq!(settings.public_key.as_deref(), Some("pk-flag"));
        assert_eq!(settings.secret_key.as_deref(), Some("sk-env"));
        assert_eq!(settings.host, "https://flag.example");
        assert_eq!(settings.timeout, Duration::from_millis(1500));
        assert_eq!(settings.max_retries, 0);
        assert!(settings.require_credentials().is_ok());
    }

    #[test]
    fn base_url_wins_over_host_and_empty_values_are_unset() {
        let settings = resolve(
            &GlobalArgs::default(),
            lookup(&[
                ("LANGFUSE_BASE_URL", ""),
                ("LANGFUSE_HOST", "https://host.example//"),
                ("LANGFUSE_PUBLIC_KEY", ""),
            ]),
        );
        assert_eq!(settings.host, "https://host.example");
        assert_eq!(settings.public_key, None);

        let settings = resolve(
            &GlobalArgs::default(),
            lookup(&[
                ("LANGFUSE_BASE_URL", "https://base.example"),
                ("LANGFUSE_HOST", "https://host.example"),
            ]),
        );
        assert_eq!(settings.host, "https://base.example");
    }

    #[test]
    fn env_file_layers_over_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(
            &path,
            "# comment\nLANGFUSE_PUBLIC_KEY=pk-file\nLANGFUSE_SECRET_KEY='sk-file'\n",
        )
        .unwrap();
        let globals = GlobalArgs {
            env: Some(path),
            ..GlobalArgs::default()
        };
        let settings = load(&globals).unwrap();
        assert_eq!(settings.public_key.as_deref(), Some("pk-file"));
        assert_eq!(settings.secret_key.as_deref(), Some("sk-file"));
    }

    #[test]
    fn unreadable_env_file_is_a_configuration_error() {
        let globals = GlobalArgs {
            env: Some("/nonexistent/.env".into()),
            ..GlobalArgs::default()
        };
        let error = load(&globals).unwrap_err();
        assert_eq!(error.exit, crate::error::Exit::Config);
        assert!(
            error
                .message
                .starts_with("Cannot read --env file /nonexistent/.env")
        );
    }
}

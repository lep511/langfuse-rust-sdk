//! The CLI error type and its exit codes.
//!
//! The exit codes are a stable, machine-readable contract shared with the
//! official TypeScript CLI: agents tell failures apart without parsing stderr.

use std::fmt;

use langfuse_rust_sdk::LangfuseError;

/// Process exit status of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// Successful API response or local command.
    Success = 0,
    /// Unexpected internal failure.
    Internal = 1,
    /// Invalid command or input; no request was sent.
    Usage = 2,
    /// Missing or invalid configuration or credentials; no request was sent.
    Config = 3,
    /// Network, DNS, TLS or timeout failure reaching the host.
    Network = 4,
    /// The API responded with a non-success HTTP status.
    Http = 5,
    /// Local file failure.
    Local = 6,
    /// Invalid or inconsistent API response.
    Response = 7,
}

impl From<Exit> for std::process::ExitCode {
    fn from(exit: Exit) -> Self {
        std::process::ExitCode::from(exit as u8)
    }
}

/// An error that ends a command with a message on stderr and an exit code.
#[derive(Debug)]
pub struct CliError {
    /// One-line summary, written to stderr (escaped for the terminal).
    pub message: String,
    /// CLI-generated text appended to the message (already escaped).
    pub hint: String,
    /// The process exit status.
    pub exit: Exit,
}

impl CliError {
    /// An error with an explicit exit code.
    pub fn new(exit: Exit, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: String::new(),
            exit,
        }
    }

    /// Invalid command or input (exit 2).
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(Exit::Usage, message)
    }

    /// Missing or invalid configuration (exit 3).
    pub fn config(message: impl Into<String>) -> Self {
        Self::new(Exit::Config, message)
    }

    /// Local file failure (exit 6).
    pub fn local(message: impl Into<String>) -> Self {
        Self::new(Exit::Local, message)
    }

    /// Invalid or inconsistent API response (exit 7).
    pub fn response(message: impl Into<String>) -> Self {
        Self::new(Exit::Response, message)
    }

    /// Append a CLI-generated hint to the message.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.message, self.hint)
    }
}

impl std::error::Error for CliError {}

/// The exit code of an SDK error that is not an HTTP status.
///
/// HTTP errors are printed as API results (exit 5) by the callers that have
/// the response at hand; this mapping is for the remaining variants.
pub fn exit_for(error: &LangfuseError) -> Exit {
    match error {
        LangfuseError::Transport(_) => Exit::Network,
        LangfuseError::Serialization(_) => Exit::Response,
        LangfuseError::Config(_) | LangfuseError::UrlParse(_) => Exit::Config,
        _ if error.status().is_some() => Exit::Http,
        _ => Exit::Internal,
    }
}

/// The message of an error with its chain of sources, for transport errors
/// whose `Display` alone ("error sending request") hides the cause.
pub fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

impl From<LangfuseError> for CliError {
    fn from(error: LangfuseError) -> Self {
        let exit = exit_for(&error);
        let message = match &error {
            LangfuseError::Transport(inner) => error_chain(inner),
            LangfuseError::Serialization(inner) => {
                format!("Invalid API response: {inner}")
            }
            other => other.to_string(),
        };
        CliError::new(exit, message)
    }
}

/// Result alias of the CLI.
pub type Result<T, E = CliError> = std::result::Result<T, E>;

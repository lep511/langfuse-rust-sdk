//! Shared helpers: the CLI run in-process against wiremock servers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use langfuse_rust_cli::error::Exit;
use langfuse_rust_cli::output::Io;
use serde_json::Value;
use wiremock::{MockServer, Request, ResponseTemplate};

/// What one CLI run printed and returned.
#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub exit: Exit,
}

impl Output {
    /// stdout parsed as JSON.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {self:?}"))
    }
}

/// Run `langfuse <args>` with `stdin`, in `cwd`.
pub async fn run_with(cwd: &Path, stdin: &[u8], args: &[&str]) -> Output {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut input = stdin;
    let mut io = Io {
        stdout: &mut stdout,
        stderr: &mut stderr,
        stdin: &mut input,
        stdout_tty: false,
        stderr_tty: false,
        width: Some(100),
        cwd: cwd.to_path_buf(),
    };
    let exit = langfuse_rust_cli::run(
        std::iter::once("langfuse").chain(args.iter().copied()),
        &mut io,
    )
    .await;
    Output {
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
        exit,
    }
}

/// Run `langfuse <args>` in the current directory without stdin.
pub async fn run(args: &[&str]) -> Output {
    run_with(&std::env::current_dir().unwrap(), b"", args).await
}

/// The connection flags for `server` (no retries).
pub fn connection(server: &MockServer) -> Vec<String> {
    vec![
        "--host".into(),
        server.uri(),
        "--public-key".into(),
        "pk-lf-test".into(),
        "--secret-key".into(),
        "sk-lf-test".into(),
        "--max-retries".into(),
        "0".into(),
    ]
}

/// Run `langfuse <connection> <args>` against `server`.
pub async fn api(server: &MockServer, args: &[&str]) -> Output {
    api_with(server, b"", args).await
}

/// [`api`] with stdin.
pub async fn api_with(server: &MockServer, stdin: &[u8], args: &[&str]) -> Output {
    let connection = connection(server);
    let mut all: Vec<&str> = connection.iter().map(String::as_str).collect();
    all.extend_from_slice(args);
    run_with(&std::env::current_dir().unwrap(), stdin, &all).await
}

/// A JSON response.
pub fn json(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

/// The only request the server received.
pub async fn single_request(server: &MockServer) -> Request {
    let mut requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected one request, got {requests:?}");
    requests.remove(0)
}

/// The JSON body of a request.
pub fn body_json(request: &Request) -> Value {
    serde_json::from_slice(&request.body).unwrap()
}

/// The query pairs of a request, in order.
pub fn query_pairs(request: &Request) -> Vec<(String, String)> {
    request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// A canonical temporary directory (macOS `/var` → `/private/var`).
pub fn temp_dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(dir.path()).unwrap();
    (dir, path)
}

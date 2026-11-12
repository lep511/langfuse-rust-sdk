//! The command grammar, help, discovery and exit codes.

mod common;

use std::io::ErrorKind;

use common::{FailingStdout, run, run_on};
use langfuse_rust_cli::Exit;

#[test]
fn command_tree_is_consistent() {
    langfuse_rust_cli::command().debug_assert();
}

#[tokio::test]
async fn help_is_printed_on_stdout_with_exit_0() {
    for args in [
        vec![],
        vec!["--help"],
        vec!["api"],
        vec!["api", "help"],
        vec!["api", "prompts"],
        vec!["api", "prompts", "help"],
        vec!["api", "help", "prompts", "create"],
        vec!["api", "prompts", "create", "help"],
        vec!["api", "prompts", "create", "--help"],
        vec!["skills"],
        vec!["skills", "install", "--help"],
    ] {
        let output = run(&args).await;
        assert_eq!(output.exit, Exit::Success, "{args:?}: {output:?}");
        assert!(
            output.stdout.contains("Usage: langfuse"),
            "{args:?}: {output:?}"
        );
    }
}

#[tokio::test]
async fn action_help_names_the_http_call() {
    let output = run(&["api", "prompts", "create", "help"]).await;
    assert!(
        output
            .stdout
            .contains("Calls POST /api/public/v2/prompts (operationId prompts_create).")
    );
    let output = run(&["api", "scores-v3", "list", "--help"]).await;
    assert!(
        output
            .stdout
            .contains("Alias of `langfuse api scores list`."),
        "{}",
        output.stdout
    );
}

#[tokio::test]
async fn traces_help_points_to_observations() {
    for args in [
        vec!["api", "trace", "--help"],
        vec!["api", "traces", "help"],
        vec!["api", "traces", "-h"],
    ] {
        let output = run(&args).await;
        assert_eq!(output.exit, Exit::Success, "{args:?}: {output:?}");
        assert!(
            output.stdout.contains("deprecated")
                && output
                    .stdout
                    .contains("langfuse api observations list --trace-id")
                && output
                    .stdout
                    .contains("https://langfuse.com/faq/all/deprecated-api-migration#traces"),
            "{args:?}: {}",
            output.stdout
        );
    }
    let output = run(&["api", "help"]).await;
    assert!(
        output
            .stdout
            .contains("reading traces is deprecated: use `observations`"),
        "{}",
        output.stdout
    );
}

#[tokio::test]
async fn usage_errors_are_exit_2_with_suggestions() {
    let output = run(&["api", "promts", "list"]).await;
    assert_eq!(output.exit, Exit::Usage);
    assert!(output.stderr.contains("'prompts'"), "{}", output.stderr);

    let output = run(&["api", "comments", "list", "--object-type", "TRACE"]).await;
    assert_eq!(output.exit, Exit::Usage);
    assert!(
        output
            .stderr
            .contains("possible values: trace, observation, session, prompt")
    );

    // The official CLI turns JSON output on for `--json=false`; here it is an error.
    assert_eq!(
        run(&["--json=false", "api", "health", "get"]).await.exit,
        Exit::Usage
    );
    // `--all` exists on paginated lists only.
    assert_eq!(
        run(&["api", "prompts", "get", "x", "--all"]).await.exit,
        Exit::Usage
    );
    assert_eq!(
        run(&["api", "prompts", "list", "--max-items", "5"])
            .await
            .exit,
        Exit::Usage
    );
    assert_eq!(
        run(&["api", "prompts", "list", "--from-updated-at", "yesterday"])
            .await
            .exit,
        Exit::Usage
    );
    assert_eq!(
        run(&["api", "prompts", "list", "--filter", "{"]).await.exit,
        Exit::Usage
    );
    // API-only options are rejected by skills commands.
    assert_eq!(
        run(&["skills", "install", "x", "--curl"]).await.exit,
        Exit::Usage
    );
}

#[tokio::test]
async fn versions_and_schema() {
    let output = run(&["api", "versions"]).await;
    assert_eq!(output.exit, Exit::Success);
    assert!(output.stdout.contains("OpenAPI spec of 2026-11-11"));

    let output = run(&["--json", "api", "schema"]).await;
    assert_eq!(output.exit, Exit::Success);
    let schema = output.json();
    assert_eq!(schema["schemaVersion"], 1);
    let get = schema["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["name"] == "prompts")
        .unwrap()["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["name"] == "get")
        .unwrap()
        .clone();
    assert_eq!(get["method"], "GET");
    assert_eq!(get["pathParameterOrder"], serde_json::json!(["promptName"]));
    let parameters = get["parameters"].as_array().unwrap();
    let version = parameters
        .iter()
        .find(|p| p["cliName"] == "version")
        .unwrap();
    assert_eq!(version["location"], "query");
    assert_eq!(version["kind"], "integer");
    assert_eq!(version["cliAliases"], serde_json::json!(["prompt-version"]));
    let path = parameters.iter().find(|p| p["location"] == "path").unwrap();
    assert_eq!(path["name"], "promptName");

    let legacy = run(&["--json", "api", "__schema"]).await;
    assert_eq!(legacy.stdout, output.stdout);
}

#[tokio::test]
async fn completions_are_generated() {
    let output = run(&["completions", "bash"]).await;
    assert_eq!(output.exit, Exit::Success);
    assert!(output.stdout.contains("langfuse"));
}

/// Commands that only print: the arguments of each.
fn printing_commands() -> Vec<Vec<&'static str>> {
    vec![
        vec!["--help"],
        vec!["--version"],
        vec!["completions", "bash"],
        vec!["--json", "api", "schema"],
        vec!["api", "schema"],
        vec!["api", "versions"],
        vec![
            "api",
            "prompts",
            "list",
            "--curl",
            "--public-key",
            "pk",
            "--secret-key",
            "sk",
        ],
    ]
}

#[tokio::test]
async fn a_closed_stdout_ends_the_command_quietly() {
    let cwd = std::env::current_dir().unwrap();
    for args in printing_commands() {
        let mut stdout = FailingStdout(ErrorKind::BrokenPipe);
        let (exit, stderr) = run_on(&cwd, &mut stdout, &args).await;
        assert_eq!(exit, Exit::Success, "{args:?}: {stderr}");
        assert_eq!(stderr, "", "{args:?}");
    }
}

#[tokio::test]
async fn a_failed_write_to_stdout_is_exit_1() {
    let cwd = std::env::current_dir().unwrap();
    for args in printing_commands() {
        let mut stdout = FailingStdout(ErrorKind::Other);
        let (exit, stderr) = run_on(&cwd, &mut stdout, &args).await;
        assert_eq!(exit, Exit::Internal, "{args:?}: {stderr}");
        assert_eq!(
            stderr, "Cannot write to stdout: stdout failed\n",
            "{args:?}"
        );
    }
}

#[tokio::test]
async fn a_panic_is_exit_1() {
    struct Panicking;
    impl std::io::Write for Panicking {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            panic!("a bug (expected by this test)");
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let cwd = std::env::current_dir().unwrap();
    let (exit, _) = run_on(&cwd, &mut Panicking, &["completions", "bash"]).await;
    assert_eq!(exit, Exit::Internal);
}

#[tokio::test]
async fn schema_text_has_no_colours_unless_asked() {
    let output = run(&["api", "schema"]).await;
    assert_eq!(output.exit, Exit::Success);
    assert!(!output.stdout.contains('\x1b'), "{}", output.stdout);
    assert!(
        output
            .stdout
            .ends_with("Pass --json for the machine-readable schema.\n")
    );
    let output = run(&["--color", "always", "api", "schema"]).await;
    assert!(output.stdout.contains('\x1b'));
}

/// A stdout that takes every write and fails when flushed, like a buffered
/// stream whose device is full.
struct FailingFlush(ErrorKind);

impl std::io::Write for FailingFlush {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::new(self.0, "flush failed"))
    }
}

#[tokio::test]
async fn a_failed_flush_of_stdout_is_a_failed_write() {
    let cwd = std::env::current_dir().unwrap();
    let mut stdout = FailingFlush(ErrorKind::Other);
    let (exit, stderr) = run_on(&cwd, &mut stdout, &["api", "versions"]).await;
    assert_eq!(exit, Exit::Internal);
    assert_eq!(stderr, "Cannot write to stdout: flush failed\n");

    let mut stdout = FailingFlush(ErrorKind::BrokenPipe);
    let (exit, stderr) = run_on(&cwd, &mut stdout, &["api", "versions"]).await;
    assert_eq!(exit, Exit::Success);
    assert_eq!(stderr, "");

    // An earlier failure keeps its own exit code and message.
    let mut stdout = FailingFlush(ErrorKind::Other);
    let (exit, stderr) = run_on(&cwd, &mut stdout, &["api", "nope"]).await;
    assert_eq!(exit, Exit::Usage);
    assert!(!stderr.contains("Cannot write to stdout"), "{stderr}");
}

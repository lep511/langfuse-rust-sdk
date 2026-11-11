//! The command grammar, help, discovery and exit codes.

mod common;

use common::run;
use langfuse_rust_cli::error::Exit;

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

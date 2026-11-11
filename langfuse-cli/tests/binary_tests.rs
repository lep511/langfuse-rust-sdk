//! The real `langfuse` binary: version, exit codes, environment.

use assert_cmd::Command;

fn langfuse() -> Command {
    let mut command = Command::cargo_bin("langfuse").unwrap();
    for name in [
        "LANGFUSE_PUBLIC_KEY",
        "LANGFUSE_SECRET_KEY",
        "LANGFUSE_BASE_URL",
        "LANGFUSE_HOST",
    ] {
        command.env_remove(name);
    }
    command.env("NO_COLOR", "1");
    command
}

#[test]
fn version_and_help() {
    let output = langfuse().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("langfuse {}\n", env!("CARGO_PKG_VERSION"))
    );
    let output = langfuse().output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Usage: langfuse")
    );
}

#[test]
fn missing_credentials_are_exit_3_before_any_request() {
    let output = langfuse()
        .args(["api", "prompts", "list"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "This operation requires LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY (or --public-key and --secret-key)\n"
    );
    // Keys from the environment are enough.
    let output = langfuse()
        .env("LANGFUSE_PUBLIC_KEY", "pk")
        .env("LANGFUSE_SECRET_KEY", "sk")
        .args(["api", "prompts", "list", "--curl"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn env_files_are_read_and_their_errors_are_exit_3() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".env");
    std::fs::write(&file, "LANGFUSE_PUBLIC_KEY=pk-file\nLANGFUSE_SECRET_KEY=sk-file\nLANGFUSE_BASE_URL=https://example.test/base/\n").unwrap();
    let output = langfuse()
        .args([
            "--env",
            file.to_str().unwrap(),
            "api",
            "health",
            "get",
            "--curl",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("curl --request GET https://example.test/base/api/public/health "),
        "{stdout}"
    );
    assert!(
        !stdout.contains("authorization"),
        "health needs no credentials: {stdout}"
    );

    let output = langfuse()
        .args(["--env", "/nonexistent/.env", "api", "health", "get"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn usage_errors_are_exit_2() {
    let output = langfuse()
        .args(["api", "prompts", "frobnicate"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unrecognized subcommand")
    );
}

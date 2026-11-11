//! The `langfuse` binary.

use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let (stdout_tty, stderr_tty, width) = langfuse_rust_cli::stdio_terminal();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr();
    let mut stdin = std::io::stdin();
    let mut io = langfuse_rust_cli::output::Io {
        stdout: &mut stdout,
        stderr: &mut stderr,
        stdin: &mut stdin,
        stdout_tty,
        stderr_tty,
        width,
        cwd: std::env::current_dir().unwrap_or_else(|_| ".".into()),
    };
    let exit = langfuse_rust_cli::run(std::env::args_os(), &mut io).await;
    let _ = std::io::Write::flush(&mut stdout);
    exit.into()
}

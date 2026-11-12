//! The `langfuse` binary.

use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr();
    let mut stdin = std::io::stdin();
    let mut io = langfuse_rust_cli::Io::stdio(&mut stdout, &mut stderr, &mut stdin);
    // `run` flushes stdout and reports a failure to do so.
    langfuse_rust_cli::run(std::env::args_os(), &mut io)
        .await
        .into()
}

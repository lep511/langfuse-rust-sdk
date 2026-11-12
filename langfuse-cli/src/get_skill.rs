//! `langfuse get-skill`: print the latest Langfuse agent skill from GitHub.

use std::time::Duration;

use crate::error::{Exit, Result, error_chain};
use crate::output::Io;

/// Where the latest Langfuse skill is published.
pub const LANGFUSE_SKILL_URL: &str =
    "https://raw.githubusercontent.com/langfuse/skills/main/skills/langfuse/SKILL.md";
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

async fn fetch(url: &str) -> std::result::Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent(crate::config::USER_AGENT)
        .build()
        .map_err(|error| error_chain(&error))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error_chain(&error))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Failed to fetch skill from {url}: {} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or_default()
        ));
    }
    response.text().await.map_err(|error| error_chain(&error))
}

/// Print the skill verbatim; on failure explain how to get it manually (exit 4).
pub async fn run(io: &mut Io<'_>) -> Result<Exit> {
    run_from(LANGFUSE_SKILL_URL, io).await
}

/// [`run`] with another URL (tests).
pub async fn run_from(url: &str, io: &mut Io<'_>) -> Result<Exit> {
    match fetch(url).await {
        Ok(text) => {
            io.out(text.as_bytes())?;
            Ok(Exit::Success)
        }
        Err(reason) => {
            let _ = write!(
                io.stderr,
                "Failed to fetch the latest Langfuse skill from GitHub.\n\
                 This environment may block direct GitHub access.\n\n\
                 Download the skill manually from:\n  {url}\n\n\
                 Then add the downloaded SKILL.md to your agent context manually.\n\n\
                 Original error: {reason}\n"
            );
            Ok(Exit::Network)
        }
    }
}

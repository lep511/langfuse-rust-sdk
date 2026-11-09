//! List observations with filters and display details.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example observations
//! ```

use langfuse_sdk::api::observations::ListObservationsParams;
use langfuse_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    let config = ClientConfig::builder(
        std::env::var("LANGFUSE_PUBLIC_KEY")?,
        std::env::var("LANGFUSE_SECRET_KEY")?,
    )
    .base_url(
        std::env::var("LANGFUSE_BASE_URL")
            .unwrap_or_else(|_| "https://cloud.langfuse.com".into()),
    )
    .build()?;
    Ok(LangfuseClient::new(config)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // Fetch the most recent observations
    let page = client
        .observations
        .list(&ListObservationsParams {
            limit: Some(10),
            ..Default::default()
        })
        .await?;

    println!("Recent observations ({} returned):", page.data.len());
    println!(
        "{:<12} {:<14} {:<25} {:<20} {:>8}",
        "TYPE", "LEVEL", "NAME", "MODEL", "LATENCY"
    );
    println!("{}", "-".repeat(85));

    for obs in &page.data {
        let name = obs.name.as_deref().unwrap_or("-");
        let model = obs.model.as_deref().unwrap_or("-");
        let level = obs
            .level
            .as_ref()
            .map(|l| format!("{:?}", l))
            .unwrap_or_else(|| "-".into());
        let latency = obs
            .latency
            .map(|l| format!("{:.1}s", l))
            .unwrap_or_else(|| "-".into());

        println!(
            "{:<12} {:<14} {:<25} {:<20} {:>8}",
            obs.r#type,
            level,
            &name[..name.len().min(24)],
            &model[..model.len().min(19)],
            latency,
        );
    }

    if let Some(cursor) = &page.meta.cursor {
        println!("\nNext cursor: {}", &cursor[..cursor.len().min(40)]);
    } else {
        println!("\nNo more pages.");
    }

    Ok(())
}

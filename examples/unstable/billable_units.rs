//! Query organization-level billable units, the numbers behind the
//! Usage settings tab in Langfuse.
//!
//! **Read-only:** writes no data.
//!
//! **Needs:** the `unstable` feature and an **organization-scoped** API key.
//! Project-scoped keys return 403.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example unstable_billable_units --features unstable
//! ```

use langfuse_rust_sdk::api::unstable_billable_units::GetBillableUnitsParams;
use langfuse_rust_sdk::models::unstable_billable_units::BillableUnitsGranularity;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};

type BoxError = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    // The last 7 days, bucketed by day.
    let now = chrono::Utc::now();
    let from = now - chrono::Duration::days(7);
    let params = GetBillableUnitsParams {
        from_timestamp: from.to_rfc3339(),
        to_timestamp: now.to_rfc3339(),
        granularity: Some(BillableUnitsGranularity::Day),
    };

    println!("Querying billable units for the last 7 days (daily)...\n");
    let resp = match client.unstable_billable_units.get(&params).await {
        Ok(r) => r,
        Err(LangfuseError::AccessDenied { message, .. }) => {
            println!("Access denied: {message}");
            println!(
                "\nThis endpoint requires an organization-scoped API key with \
                 the \"organizationUsage:read\" permission. Project-scoped keys \
                 cannot access it."
            );
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    if let Some(recorded_from) = resp.usage_recorded_from {
        println!("Usage recorded from: {recorded_from}");
    } else {
        println!("No usage has been recorded yet on this deployment.");
    }

    println!(
        "\n{:<24} {:<38} {:>7} {:>7} {:>7} {:>7} {:>7}",
        "Bucket", "Project", "Total", "Traces", "Obs", "Scores", "Topics"
    );
    println!("{}", "-".repeat(105));
    for row in &resp.data {
        println!(
            "{:<24} {:<38} {:>7} {:>7} {:>7} {:>7} {:>7}",
            row.bucket.format("%Y-%m-%d %H:%M UTC"),
            row.project_id,
            row.count,
            row.traces,
            row.observations,
            row.scores,
            row.topic_facet_summaries,
        );
    }
    if resp.data.is_empty() {
        println!("  (no billable units in this range)");
    }
    println!("\n{} rows returned.", resp.data.len());

    // Also try the other granularities to show they work.
    for granularity in [
        BillableUnitsGranularity::Hour,
        BillableUnitsGranularity::Week,
        BillableUnitsGranularity::Month,
    ] {
        let params = GetBillableUnitsParams {
            from_timestamp: from.to_rfc3339(),
            to_timestamp: now.to_rfc3339(),
            granularity: Some(granularity),
        };
        let resp = client.unstable_billable_units.get(&params).await?;
        println!("  {granularity:?} granularity: {} rows", resp.data.len());
    }

    // Without granularity (server default: day).
    let params = GetBillableUnitsParams::new(from.to_rfc3339(), now.to_rfc3339());
    let resp = client.unstable_billable_units.get(&params).await?;
    println!("  Default granularity:  {} rows", resp.data.len());

    Ok(())
}

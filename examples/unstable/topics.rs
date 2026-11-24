//! Explore the unstable Topics API: list facets, topics, rules,
//! summaries and assignments.
//!
//! **Read-only:** writes no data. Topics must be enabled on the project;
//! when it is not, the example prints a message and exits cleanly.
//!
//! **Needs:** the `unstable` feature.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example unstable_topics --features unstable
//! ```

use langfuse_rust_sdk::api::unstable_topics::{
    GetTopicParams, ListTopicAssignmentsParams, ListTopicFacetsParams, ListTopicSummariesParams,
    ListTopicsParams,
};
use langfuse_rust_sdk::models::unstable_topics::TopicsOrderBy;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};

type BoxError = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    // --- LIST FACETS ---
    println!("Listing topic facets...");
    let facets = match client
        .unstable_topics
        .list_facets(&ListTopicFacetsParams {
            limit: Some(10),
            ..Default::default()
        })
        .await
    {
        Ok(f) => f,
        Err(LangfuseError::NotFound { message, .. }) => {
            println!("  Topics is not available for this project: {message}");
            println!("  Enable Topics in your Langfuse project settings to use this example.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    println!(
        "  {} facets (page 1 of {})",
        facets.data.len(),
        facets.meta.total_pages
    );
    for f in &facets.data {
        println!(
            "    - {} (id: {}, active: {}, built-in: {})",
            f.name, f.id, f.active, f.is_built_in
        );
    }

    // --- GET FACET ---
    if let Some(first) = facets.data.first() {
        println!("\nFetching facet '{}'...", first.name);
        let facet = client.unstable_topics.get_facet(&first.id).await?;
        println!("  Name:        {}", facet.name);
        println!("  Description: {}", facet.description);
        println!("  Version:     {}", facet.version);
        if let Some(run) = &facet.published_run {
            println!("  Published run: {} ({} topics)", run.id, run.topic_count);
        } else {
            println!("  No published run yet.");
        }
    }

    // --- LIST RULES ---
    println!("\nListing topic rules...");
    let rules = client
        .unstable_topics
        .list_rules(&ListTopicFacetsParams {
            limit: Some(10),
            ..Default::default()
        })
        .await?;
    println!("  {} rules", rules.data.len());
    for r in &rules.data {
        println!(
            "    - {} (id: {}, sampling: {}, facets: {})",
            r.name,
            r.id,
            r.sampling,
            r.facet_ids.len()
        );
    }

    // --- GET RULE ---
    if let Some(first_rule) = rules.data.first() {
        println!("\nFetching rule '{}'...", first_rule.name);
        let rule = client.unstable_topics.get_rule(&first_rule.id).await?;
        println!("  Name:     {}", rule.name);
        println!("  Sampling: {}", rule.sampling);
        println!("  Filters:  {}", rule.filter.len());
        println!("  Facets:   {:?}", rule.facet_ids);
    }

    // --- LIST TOPICS ---
    println!("\nListing topics (last 7 days, by trace count)...");
    let now = chrono::Utc::now();
    let from = now - chrono::Duration::days(7);
    let topics = client
        .unstable_topics
        .list(&ListTopicsParams {
            from_timestamp: Some(from.to_rfc3339()),
            to_timestamp: Some(now.to_rfc3339()),
            order_by: Some(TopicsOrderBy::TraceCountDesc),
            limit: Some(10),
            ..Default::default()
        })
        .await?;
    println!(
        "  {} topics, window {} .. {}",
        topics.data.len(),
        topics.window.from.format("%Y-%m-%d"),
        topics.window.to.format("%Y-%m-%d")
    );
    for t in &topics.data {
        println!(
            "    - {} (traces: {}, facet: {})",
            t.name, t.trace_count, t.facet.name
        );
    }

    // --- GET TOPIC ---
    if let Some(first_topic) = topics.data.first() {
        println!("\nFetching topic '{}'...", first_topic.name);
        let topic = client
            .unstable_topics
            .get(
                &first_topic.id,
                &GetTopicParams {
                    from_timestamp: Some(from.to_rfc3339()),
                    to_timestamp: Some(now.to_rfc3339()),
                },
            )
            .await?;
        println!("  Name:        {}", topic.topic.name);
        println!("  Description: {}", topic.topic.description);
        println!("  Traces:      {}", topic.topic.trace_count);
        println!("  In map:      {}", topic.in_current_map);
    }

    // --- LIST SUMMARIES ---
    println!("\nListing topic summaries (first page)...");
    let summaries = client
        .unstable_topics
        .list_summaries(&ListTopicSummariesParams {
            from_timestamp: Some(from.to_rfc3339()),
            to_timestamp: Some(now.to_rfc3339()),
            limit: Some(5),
            ..Default::default()
        })
        .await?;
    println!("  {} summaries", summaries.data.len());
    for s in &summaries.data {
        println!(
            "    - trace {} ({:?}): {}",
            &s.trace_id[..8.min(s.trace_id.len())],
            s.state,
            s.summary.as_deref().unwrap_or("(no summary)")
        );
    }
    if summaries.meta.cursor.is_some() {
        println!("  (more pages available)");
    }

    // --- LIST ASSIGNMENTS ---
    println!("\nListing topic assignments (first page)...");
    let assignments = client
        .unstable_topics
        .list_assignments(&ListTopicAssignmentsParams {
            from_timestamp: Some(from.to_rfc3339()),
            to_timestamp: Some(now.to_rfc3339()),
            limit: Some(5),
            ..Default::default()
        })
        .await?;
    println!("  {} assignments", assignments.data.len());
    for a in &assignments.data {
        let topic_name = a
            .topic
            .as_ref()
            .map(|t| t.name.as_str())
            .unwrap_or("(outlier)");
        println!(
            "    - trace {} -> {:?} ({})",
            &a.trace_id[..8.min(a.trace_id.len())],
            a.status,
            topic_name
        );
    }

    println!("\nDone!");
    Ok(())
}

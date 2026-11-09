//! API client modules — one struct per Langfuse resource group.
//!
//! Each client holds a shared [`HttpClient`](crate::http::HttpClient) and
//! exposes async methods that map 1-to-1 to the Langfuse REST API operations.
#![allow(missing_docs)]

pub mod annotation_queues;
pub mod blob_storage_integrations;
pub mod comments;
pub mod dataset_items;
pub mod datasets;
pub mod evaluation_rules;
pub mod evaluators;
pub mod experiments;
pub mod feedback;
pub mod health;
pub mod ingestion;
pub mod llm_connections;
pub mod media;
pub mod metrics;
pub mod models;
pub mod observations;
pub mod organizations;
pub mod projects;
pub mod prompt_version;
pub mod prompts;
pub mod scim;
pub mod score_configs;
pub mod scores;
pub mod skills;
pub mod traces;
#[cfg(feature = "unstable")]
pub mod unstable_dashboard_widgets;
#[cfg(feature = "unstable")]
pub mod unstable_dashboards;
#[cfg(feature = "unstable")]
pub mod unstable_topics;

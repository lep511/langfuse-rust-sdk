//! `langfuse api unstable-topic-*` and `unstable-topics` (feature `unstable`;
//! Topics is in closed beta).

use clap::{Args, ValueEnum};
use langfuse_rust_sdk::api::unstable_topics::{
    GetTopicParams, ListTopicAssignmentsParams, ListTopicFacetsParams, ListTopicSummariesParams,
    ListTopicsParams,
};
use langfuse_rust_sdk::models::unstable_topics::{TopicResultsOrderBy, TopicsOrderBy};

use crate::args::{AllPages, CursorArgs, PageArgs, bool_value, date_time, json_text, tri_state};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

/// `--filter` of the facet and rule lists.
#[derive(Debug, Args)]
#[command(next_help_heading = "Query")]
struct FilterQuery {
    /// JSON array of filter conditions
    #[arg(long, value_parser = json_text, value_name = "JSON")]
    filter: Option<String>,
}

/// The trace time window.
#[derive(Debug, Args)]
#[command(next_help_heading = "Query")]
struct Window {
    /// Start of the window, inclusive (RFC 3339) [default: 7 days before --to-timestamp]
    #[arg(long, value_parser = date_time)]
    from_timestamp: Option<String>,
    /// End of the window, exclusive (RFC 3339) [default: now]
    #[arg(long, value_parser = date_time)]
    to_timestamp: Option<String>,
}

#[derive(Debug, Args)]
pub struct ListFacets {
    #[command(flatten)]
    query: FilterQuery,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for ListFacets {
    const META: OperationMeta = OperationMeta::new(
        "unstable-topic-facets",
        "list",
        "unstable_topics_listFacets",
        HttpMethod::Get,
        "/api/public/unstable/topic-facets",
    )
    .page()
    .columns(&["id", "name", "description", "active", "updatedAt"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListTopicFacetsParams {
            filter: self.query.filter.clone(),
            page: self.page.page,
            limit: self.page.limit,
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

path_op! {
    GetFacet {
        /// The facet ID
        facet_id => "facetId",
    }
    meta = OperationMeta::new(
        "unstable-topic-facets", "get", "unstable_topics_getFacet",
        HttpMethod::Get, "/api/public/unstable/topic-facets/{facetId}",
    );
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TopicsOrder {
    #[value(name = "traceCount.desc")]
    TraceCountDesc,
    #[value(name = "traceCount.asc")]
    TraceCountAsc,
    #[value(name = "name.asc")]
    NameAsc,
    #[value(name = "name.desc")]
    NameDesc,
}

impl From<TopicsOrder> for TopicsOrderBy {
    fn from(order: TopicsOrder) -> Self {
        match order {
            TopicsOrder::TraceCountDesc => TopicsOrderBy::TraceCountDesc,
            TopicsOrder::TraceCountAsc => TopicsOrderBy::TraceCountAsc,
            TopicsOrder::NameAsc => TopicsOrderBy::NameAsc,
            TopicsOrder::NameDesc => TopicsOrderBy::NameDesc,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ResultsOrder {
    #[value(name = "timestamp.desc")]
    TimestampDesc,
    #[value(name = "timestamp.asc")]
    TimestampAsc,
}

impl From<ResultsOrder> for TopicResultsOrderBy {
    fn from(order: ResultsOrder) -> Self {
        match order {
            ResultsOrder::TimestampDesc => TopicResultsOrderBy::TimestampDesc,
            ResultsOrder::TimestampAsc => TopicResultsOrderBy::TimestampAsc,
        }
    }
}

#[derive(Debug, Args)]
pub struct List {
    #[command(flatten)]
    window: Window,
    #[command(flatten)]
    query: FilterQuery,
    /// Sort order [server default: traceCount.desc]
    #[arg(long, value_enum, help_heading = "Query")]
    order_by: Option<TopicsOrder>,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for List {
    const META: OperationMeta = OperationMeta::new(
        "unstable-topics",
        "list",
        "unstable_topics_list",
        HttpMethod::Get,
        "/api/public/unstable/topics",
    )
    .page()
    .columns(&["id", "name", "facetName", "traceCount"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListTopicsParams {
            from_timestamp: self.window.from_timestamp.clone(),
            to_timestamp: self.window.to_timestamp.clone(),
            filter: self.query.filter.clone(),
            order_by: self.order_by.map(Into::into),
            page: self.page.page,
            limit: self.page.limit,
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

#[derive(Debug, Args)]
pub struct Get {
    /// The topic ID
    topic_id: String,
    #[command(flatten)]
    window: Window,
}

impl Operation for Get {
    const META: OperationMeta = OperationMeta::new(
        "unstable-topics",
        "get",
        "unstable_topics_get",
        HttpMethod::Get,
        "/api/public/unstable/topics/{topicId}",
    );

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META)
            .path_param("topicId", &self.topic_id)
            .query(&GetTopicParams {
                from_timestamp: self.window.from_timestamp.clone(),
                to_timestamp: self.window.to_timestamp.clone(),
            })
    }
}

#[derive(Debug, Args)]
pub struct ListRules {
    #[command(flatten)]
    query: FilterQuery,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for ListRules {
    const META: OperationMeta = OperationMeta::new(
        "unstable-topic-rules",
        "list",
        "unstable_topics_listRules",
        HttpMethod::Get,
        "/api/public/unstable/topic-rules",
    )
    .page()
    .columns(&["id", "name", "sampling", "facetIds", "updatedAt"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListTopicFacetsParams {
            filter: self.query.filter.clone(),
            page: self.page.page,
            limit: self.page.limit,
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

path_op! {
    GetRule {
        /// The rule ID
        rule_id => "ruleId",
    }
    meta = OperationMeta::new(
        "unstable-topic-rules", "get", "unstable_topics_getRule",
        HttpMethod::Get, "/api/public/unstable/topic-rules/{ruleId}",
    );
}

#[derive(Debug, Args)]
pub struct ListSummaries {
    #[command(flatten)]
    window: Window,
    #[command(flatten)]
    query: FilterQuery,
    /// Truncate summaries to 500 characters [server default: false]
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_truncate_summaries",
        help_heading = "Query"
    )]
    truncate_summaries: Option<bool>,
    #[arg(long, hide = true, overrides_with = "truncate_summaries")]
    no_truncate_summaries: bool,
    /// Sort order [server default: timestamp.desc]
    #[arg(long, value_enum, help_heading = "Query")]
    order_by: Option<ResultsOrder>,
    #[command(flatten)]
    cursor: CursorArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for ListSummaries {
    const META: OperationMeta = OperationMeta::new(
        "unstable-topic-summaries",
        "list",
        "unstable_topics_listSummaries",
        HttpMethod::Get,
        "/api/public/unstable/topic-summaries",
    )
    .cursor()
    .columns(&["traceId", "facetName", "state", "summary", "timestamp"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListTopicSummariesParams {
            from_timestamp: self.window.from_timestamp.clone(),
            to_timestamp: self.window.to_timestamp.clone(),
            filter: self.query.filter.clone(),
            truncate_summaries: tri_state(self.truncate_summaries, self.no_truncate_summaries),
            order_by: self.order_by.map(Into::into),
            limit: self.cursor.limit,
            cursor: self.cursor.cursor.clone(),
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

#[derive(Debug, Args)]
pub struct ListAssignments {
    #[command(flatten)]
    window: Window,
    #[command(flatten)]
    query: FilterQuery,
    /// Sort order [server default: timestamp.desc]
    #[arg(long, value_enum, help_heading = "Query")]
    order_by: Option<ResultsOrder>,
    #[command(flatten)]
    cursor: CursorArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for ListAssignments {
    const META: OperationMeta = OperationMeta::new(
        "unstable-topic-assignments",
        "list",
        "unstable_topics_listAssignments",
        HttpMethod::Get,
        "/api/public/unstable/topic-assignments",
    )
    .cursor()
    .columns(&[
        "traceId",
        "facetName",
        "ruleId",
        "status",
        "topicName",
        "timestamp",
    ]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListTopicAssignmentsParams {
            from_timestamp: self.window.from_timestamp.clone(),
            to_timestamp: self.window.to_timestamp.clone(),
            filter: self.query.filter.clone(),
            order_by: self.order_by.map(Into::into),
            limit: self.cursor.limit,
            cursor: self.cursor.cursor.clone(),
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

resource! {
    pub enum FacetsCommand {
        /// List the active Topics facets (closed beta)
        List(ListFacets),
        /// Get a Topics facet, inactive ones included (closed beta)
        Get(GetFacet),
    }
}

resource! {
    pub enum TopicsCommand {
        /// List the topics of each active facet's live map with trace counts (closed beta)
        List(List),
        /// Get one topic of a facet's current map (closed beta)
        Get(Get),
    }
}

resource! {
    pub enum RulesCommand {
        /// List the Topics rules (closed beta)
        List(ListRules),
        /// Get a Topics rule (closed beta)
        Get(GetRule),
    }
}

resource! {
    pub enum SummariesCommand {
        /// List the current Topics summary per trace and facet (closed beta, cursor pagination)
        List(ListSummaries),
    }
}

resource! {
    pub enum AssignmentsCommand {
        /// List the current topic per trace, facet and rule (closed beta, cursor pagination)
        List(ListAssignments),
    }
}

//! `langfuse api`: every operation of the Langfuse public API, one typed clap
//! command per operation (`api <resource> <action>`), plus the `schema` and
//! `versions` built-ins.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::cli::GlobalArgs;
use crate::error::{CliError, Exit, Result};
use crate::operation::{ApiOperation, Context, OperationMeta};
use crate::output::{Format, Io, OutputOptions, write_result};
use crate::{config, execute, paginate, schema};

/// Declare a resource's subcommand enum and its operation list (in
/// declaration order, which is also clap's subcommand order).
macro_rules! resource {
    (
        $(#[$attr:meta])*
        pub enum $name:ident {
            $( $(#[$vattr:meta])* $variant:ident($ty:ty), )+
        }
    ) => {
        $(#[$attr])*
        #[derive(Debug, clap::Subcommand)]
        // Parsed once per run; boxing the actions would buy nothing.
        #[allow(clippy::large_enum_variant)]
        pub enum $name {
            $( $(#[$vattr])* $variant($ty), )+
        }

        impl $name {
            /// The selected action.
            pub fn operation(&self) -> &dyn $crate::operation::ApiOperation {
                match self {
                    $( Self::$variant(operation) => operation, )+
                }
            }

            /// Every action's operation, in declaration order.
            pub fn metas() -> Vec<&'static $crate::operation::OperationMeta> {
                vec![ $( &<$ty as $crate::operation::Operation>::META, )+ ]
            }
        }
    };
}

/// An operation whose inputs are string path parameters only.
macro_rules! path_op {
    (
        $name:ident { $( $(#[$fattr:meta])* $field:ident => $wire:literal ),* $(,)? }
        meta = $meta:expr;
    ) => {
        #[derive(Debug, clap::Args)]
        pub struct $name {
            $( $(#[$fattr])* $field: String, )*
        }

        impl $crate::operation::Operation for $name {
            const META: $crate::operation::OperationMeta = $meta;

            fn prepare(
                &self,
                _ctx: &mut $crate::operation::Context<'_>,
            ) -> $crate::error::Result<$crate::operation::PreparedRequest> {
                Ok($crate::operation::PreparedRequest::new(&Self::META)
                    $( .path_param($wire, &self.$field) )*)
            }
        }
    };
}

/// An operation with string path parameters and a body given only through
/// `--body-json` / `--body-file`.
macro_rules! json_op {
    (
        $name:ident { $( $(#[$fattr:meta])* $field:ident => $wire:literal ),* $(,)? }
        meta = $meta:expr;
    ) => {
        #[derive(Debug, clap::Args)]
        pub struct $name {
            $( $(#[$fattr])* $field: String, )*
            #[command(flatten)]
            body: $crate::args::JsonBody,
        }

        impl $crate::operation::Operation for $name {
            const META: $crate::operation::OperationMeta = $meta.json_only();

            fn prepare(
                &self,
                ctx: &mut $crate::operation::Context<'_>,
            ) -> $crate::error::Result<$crate::operation::PreparedRequest> {
                Ok($crate::operation::PreparedRequest::new(&Self::META)
                    $( .path_param($wire, &self.$field) )*
                    .body(self.body.read(ctx)?))
            }
        }
    };
}

/// A list with string path parameters and `--page` / `--limit` only.
macro_rules! page_list_op {
    (
        $name:ident { $( $(#[$fattr:meta])* $field:ident => $wire:literal ),* $(,)? }
        meta = $meta:expr;
    ) => {
        #[derive(Debug, clap::Args)]
        pub struct $name {
            $( $(#[$fattr])* $field: String, )*
            #[command(flatten)]
            page: $crate::args::PageArgs,
            #[command(flatten)]
            all: $crate::args::AllPages,
        }

        impl $crate::operation::Operation for $name {
            const META: $crate::operation::OperationMeta = $meta.page();

            fn prepare(
                &self,
                _ctx: &mut $crate::operation::Context<'_>,
            ) -> $crate::error::Result<$crate::operation::PreparedRequest> {
                $crate::operation::PreparedRequest::new(&Self::META)
                    $( .path_param($wire, &self.$field) )*
                    .query(&self.page.params())
            }

            fn all_pages(&self) -> Option<&$crate::args::AllPages> {
                Some(&self.all)
            }
        }
    };
}

/// A list with string path parameters and `--cursor` / `--limit` only.
macro_rules! cursor_list_op {
    (
        $name:ident { $( $(#[$fattr:meta])* $field:ident => $wire:literal ),* $(,)? }
        meta = $meta:expr;
    ) => {
        #[derive(Debug, clap::Args)]
        pub struct $name {
            $( $(#[$fattr])* $field: String, )*
            #[command(flatten)]
            cursor: $crate::args::CursorArgs,
            #[command(flatten)]
            all: $crate::args::AllPages,
        }

        impl $crate::operation::Operation for $name {
            const META: $crate::operation::OperationMeta = $meta.cursor();

            fn prepare(
                &self,
                _ctx: &mut $crate::operation::Context<'_>,
            ) -> $crate::error::Result<$crate::operation::PreparedRequest> {
                $crate::operation::PreparedRequest::new(&Self::META)
                    $( .path_param($wire, &self.$field) )*
                    .query(&self.cursor.params())
            }

            fn all_pages(&self) -> Option<&$crate::args::AllPages> {
                Some(&self.all)
            }
        }
    };
}

/// Declare the `Resource` enum: the stable resources, then the unstable ones
/// (behind the `unstable` feature).
macro_rules! resources {
    (
        stable { $( $(#[$attr:meta])* $variant:ident($ty:ty), )+ }
        unstable { $( $(#[$uattr:meta])* $uvariant:ident($uty:ty), )* }
    ) => {
        /// A resource of the API.
        #[derive(Debug, Subcommand)]
        #[allow(clippy::large_enum_variant)]
        pub enum Resource {
            $(
                $(#[$attr])*
                #[command(subcommand, arg_required_else_help = true)]
                $variant($ty),
            )+
            $(
                #[cfg(feature = "unstable")]
                $(#[$uattr])*
                #[command(subcommand, arg_required_else_help = true)]
                $uvariant($uty),
            )*
        }

        impl Resource {
            /// The selected action.
            pub fn operation(&self) -> &dyn ApiOperation {
                match self {
                    $( Self::$variant(command) => command.operation(), )+
                    $( #[cfg(feature = "unstable")] Self::$uvariant(command) => command.operation(), )*
                }
            }

            /// The operations of every resource, in declaration order.
            pub fn metas() -> Vec<Vec<&'static OperationMeta>> {
                #[allow(unused_mut)]
                let mut out = vec![ $( <$ty>::metas(), )+ ];
                $( #[cfg(feature = "unstable")] out.push(<$uty>::metas()); )*
                out
            }
        }
    };
}

pub mod annotation_queues;
pub mod comments;
pub mod dataset_items;
pub mod datasets;
pub mod evaluation_rules;
pub mod evaluators;
pub mod experiments;
pub mod feedback;
pub mod health;
pub mod integrations;
pub mod llm_connections;
pub mod media;
pub mod metrics;
pub mod models;
pub mod observations;
pub mod organizations;
pub mod otel;
pub mod projects;
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

resources! {
    stable {
        /// Annotation queues, their items and assignments
        AnnotationQueues(annotation_queues::Command),
        /// Comments on traces, observations, sessions and prompts
        Comments(comments::Command),
        /// Items of datasets
        DatasetItems(dataset_items::Command),
        /// Datasets
        #[command(alias = "datasets-v2")]
        Datasets(datasets::Command),
        /// Evaluation rules: which observations evaluators score
        #[command(alias = "evaluation-rules-v2")]
        EvaluationRules(evaluation_rules::Command),
        /// Evaluators: LLM-as-a-judge prompts and code that produce scores
        #[command(alias = "evaluators-v2")]
        Evaluators(evaluators::Command),
        /// Items of experiments (dataset runs)
        ExperimentItems(experiments::ItemsCommand),
        /// Experiments (dataset runs)
        Experiments(experiments::Command),
        /// Feedback about Langfuse itself
        Feedback(feedback::Command),
        /// Check the health of the API and its database (no credentials needed)
        Health(health::Command),
        /// Blob storage integrations (organization-scoped key)
        #[command(alias = "blob-storage-integrations")]
        Integrations(integrations::Command),
        /// LLM connections used by the playground and evaluators
        LlmConnections(llm_connections::Command),
        /// Media records (images, audio, files) and their uploads
        Media(media::Command),
        /// Metrics queries (v2)
        #[command(alias = "metrics-v2")]
        Metrics(metrics::Command),
        /// Model definitions and prices
        Models(models::Command),
        /// Observations: spans, generations, events and the other observation types
        #[command(alias = "observations-v2")]
        Observations(observations::Command),
        /// The organization of an organization-scoped key: memberships, projects, API keys, LLM
        /// connections
        Organizations(organizations::Command),
        /// OpenTelemetry (OTLP/HTTP) trace ingestion
        #[command(alias = "otel-v1", alias = "opentelemetry")]
        Otel(otel::Command),
        /// Projects, their API keys and memberships
        Projects(projects::Command),
        /// Prompts and their versions
        #[command(alias = "prompts-v2")]
        Prompts(prompts::Command),
        /// Alias of `prompts update-version`
        #[command(name = "prompt-version", hide = true)]
        PromptVersion(prompts::VersionAlias),
        /// SCIM user provisioning (organization-scoped key)
        Scim(scim::Command),
        /// Score configs: the schemas of scores
        ScoreConfigs(score_configs::Command),
        /// Scores of traces, observations, sessions and experiments
        Scores(scores::Command),
        /// Alias of `scores list`
        #[command(name = "scores-v3", hide = true)]
        ScoresV3(scores::V3Alias),
        /// Agent skills: versions, labels, tags and files (see also `langfuse skills`)
        Skills(skills::Command),
        /// Traces (deletion; read traces through observations)
        #[command(alias = "trace")]
        Traces(traces::Command),
    }
    unstable {
        /// Dashboard widgets: standalone chart definitions (unstable API)
        UnstableDashboardWidgets(unstable_dashboard_widgets::Command),
        /// Dashboards and their placements (unstable API)
        UnstableDashboards(unstable_dashboards::Command),
        /// Topics: assignments of traces to topics (unstable API, closed beta)
        UnstableTopicAssignments(unstable_topics::AssignmentsCommand),
        /// Topics: facets (unstable API, closed beta)
        UnstableTopicFacets(unstable_topics::FacetsCommand),
        /// Topics: rules (unstable API, closed beta)
        UnstableTopicRules(unstable_topics::RulesCommand),
        /// Topics: per-trace summaries (unstable API, closed beta)
        UnstableTopicSummaries(unstable_topics::SummariesCommand),
        /// Topics: the topics of each facet's map (unstable API, closed beta)
        UnstableTopics(unstable_topics::TopicsCommand),
    }
}

const API_AFTER_HELP: &str = "\
Discovery:
  langfuse api help [resource] [action]   Help of a resource or action (also: api <resource> <action> help)
  langfuse api schema --json              Machine-readable command schema
  langfuse api versions                   The API spec this CLI targets; `versions detect` asks the server

OpenAPI tags and route versions also work as resource names (scores-v3 list, prompts-v2 get, …).";

/// `langfuse api`.
#[derive(Debug, Args)]
#[command(
    arg_required_else_help = true,
    subcommand_value_name = "RESOURCE",
    subcommand_help_heading = "Resources",
    after_help = API_AFTER_HELP
)]
pub struct ApiArgs {
    #[command(flatten)]
    pub output: ApiOutputArgs,

    #[command(subcommand)]
    pub command: ApiCommand,
}

/// Output options of `langfuse api`, accepted after any of its subcommands.
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Output options")]
pub struct ApiOutputArgs {
    /// Print the request as a curl command instead of sending it
    #[arg(long, global = true)]
    pub curl: bool,

    /// Show the credentials in --curl output
    #[arg(long, global = true)]
    pub show_secrets: bool,

    /// Write the response body to a file instead of stdout
    #[arg(long, global = true, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Output format of the response
    #[arg(long, global = true, value_enum, default_value_t)]
    pub format: Format,

    /// Table columns, comma-separated dotted paths (usage.total), with --format table
    #[arg(long, global = true, value_delimiter = ',', value_name = "COLUMNS")]
    pub columns: Vec<String>,
}

/// Subcommands of `langfuse api`.
#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum ApiCommand {
    #[command(flatten)]
    Resource(Resource),

    /// Machine-readable schema of every command (with --json)
    #[command(alias = "__schema", alias = "__spec")]
    Schema,

    /// The API version this CLI targets
    Versions {
        #[command(subcommand)]
        action: Option<VersionsAction>,
    },
}

/// `langfuse api versions …`.
#[derive(Debug, Subcommand)]
pub enum VersionsAction {
    /// Print the API spec this CLI targets (the default)
    List,
    /// Ask the server for its version (GET /api/public/health)
    Detect,
}

/// The API spec this CLI implements.
pub const SPEC: &str =
    "OpenAPI spec of 2026-11-11 (Langfuse 4.71.0 and later), via langfuse-rust-sdk";

/// Run `langfuse api …`.
pub async fn run(api: &ApiArgs, globals: &GlobalArgs, io: &mut Io<'_>) -> Result<Exit> {
    match &api.command {
        ApiCommand::Schema => {
            if globals.json {
                let schema = schema::schema();
                writeln!(io.stdout, "{schema}").map_err(write_error)?;
            } else {
                let help = crate::command()
                    .find_subcommand_mut("api")
                    .map(|api| api.render_help());
                if let Some(help) = help {
                    write!(io.stdout, "{}", help.ansi()).map_err(write_error)?;
                }
                writeln!(io.stdout, "\nPass --json for the machine-readable schema.")
                    .map_err(write_error)?;
            }
            Ok(Exit::Success)
        }
        ApiCommand::Versions { action } => match action {
            None | Some(VersionsAction::List) => {
                writeln!(io.stdout, "{SPEC} {}", execute::SDK_VERSION).map_err(write_error)?;
                Ok(Exit::Success)
            }
            Some(VersionsAction::Detect) => {
                let settings = config::load(globals)?;
                let client = settings.client()?;
                let health = client.health.check().await.map_err(CliError::from)?;
                writeln!(io.stdout, "{}", health.version).map_err(write_error)?;
                Ok(Exit::Success)
            }
        },
        ApiCommand::Resource(resource) => call(resource.operation(), api, globals, io).await,
    }
}

fn write_error(error: std::io::Error) -> CliError {
    CliError::new(Exit::Internal, format!("Cannot write to stdout: {error}"))
}

async fn call(
    operation: &dyn ApiOperation,
    api: &ApiArgs,
    globals: &GlobalArgs,
    io: &mut Io<'_>,
) -> Result<Exit> {
    let meta = operation.meta();
    let output = &api.output;
    let all = operation.all_pages().filter(|all| all.all);
    if all.is_some() && output.curl {
        return Err(CliError::usage("--all cannot be combined with --curl"));
    }
    if !output.columns.is_empty() && output.format != Format::Table {
        return Err(CliError::usage("--columns requires --format table"));
    }

    let request = {
        let mut ctx = Context {
            stdin: &mut *io.stdin,
        };
        operation.prepare(&mut ctx)?
    };
    let settings = config::load(globals)?;
    if meta.auth {
        settings.require_credentials()?;
    }
    if output.curl {
        let curl = execute::render_curl(&settings, &request, output.show_secrets)?;
        writeln!(io.stdout, "{curl}").map_err(write_error)?;
        return Ok(Exit::Success);
    }

    let http = settings.http_client()?;
    let stream = output.format == Format::Ndjson && output.output.is_none() && !globals.json;
    let result = match (all, meta.pagination) {
        (Some(all), Some(pagination)) => {
            let max_items = all.max_items.unwrap_or(crate::args::DEFAULT_MAX_ITEMS);
            paginate::all_pages(&http, &settings, request, pagination, max_items, stream, io)
                .await?
        }
        _ => execute::execute(&http, &settings, &request).await?,
    };
    let columns = if output.columns.is_empty() {
        meta.columns
            .iter()
            .map(|column| (*column).to_owned())
            .collect()
    } else {
        output.columns.clone()
    };
    let options = OutputOptions {
        envelope: globals.json,
        file: output.output.clone(),
        format: output.format,
        columns,
        color: crate::use_color(globals.color, io.stdout_tty),
        width: io.width,
    };
    write_result(&result, &options, io)
}

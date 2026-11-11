//! `langfuse api integrations` (alias `blob-storage-integrations`).

use clap::Args;
use serde_json::Value;

use crate::args::{BodyChannel, ObjectBody, bool_value, date_time, expand_strings, tri_state};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

path_op! {
    ListBlobStorage {}
    meta = OperationMeta::new(
        "integrations", "list-blob-storage", "blobStorageIntegrations_getBlobStorageIntegrations",
        HttpMethod::Get, "/api/public/integrations/blob-storage",
    )
    .columns(&["id", "projectId", "type", "bucketName", "enabled", "lastSyncAt"]);
}

#[derive(Debug, Args)]
pub struct UpsertBlobStorage {
    #[command(flatten)]
    fields: UpsertFields,
    #[command(flatten)]
    body: BodyChannel,
}

const REQUIRED: [&str; 2] = ["body_json", "body_file"];

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct UpsertFields {
    /// Project to configure
    #[arg(long, required_unless_present_any = REQUIRED)]
    project_id: Option<String>,
    /// Storage type
    #[arg(
        long = "type",
        value_name = "TYPE",
        value_parser = ["S3", "S3_COMPATIBLE", "AZURE_BLOB_STORAGE", "GOOGLE_CLOUD_STORAGE"],
        required_unless_present_any = REQUIRED
    )]
    kind: Option<String>,
    /// Bucket (or Azure container) name
    #[arg(long, required_unless_present_any = REQUIRED)]
    bucket_name: Option<String>,
    /// Custom endpoint URL (required for S3_COMPATIBLE)
    #[arg(long)]
    endpoint: Option<String>,
    /// Storage region (AWS, GCS, R2, MinIO, Azure location IDs)
    #[arg(long, required_unless_present_any = REQUIRED)]
    region: Option<String>,
    /// How Langfuse authenticates to the bucket
    #[arg(long, value_parser = ["ACCESS_KEYS", "ROLE_ARN"])]
    auth_type: Option<String>,
    /// IAM role Langfuse assumes with --auth-type ROLE_ARN
    #[arg(long)]
    role_arn: Option<String>,
    /// Access key ID (not used for GOOGLE_CLOUD_STORAGE or ROLE_ARN)
    #[arg(long)]
    access_key_id: Option<String>,
    /// Secret access key (stored encrypted; omitted on update keeps the stored one)
    #[arg(long)]
    secret_access_key: Option<String>,
    /// Path prefix of exported files (ends with /)
    #[arg(long)]
    prefix: Option<String>,
    /// Export frequency
    #[arg(
        long,
        value_parser = ["every_20_minutes", "hourly", "daily", "weekly"],
        required_unless_present_any = REQUIRED
    )]
    export_frequency: Option<String>,
    /// Whether the integration is active
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_enabled",
        required_unless_present_any = ["body_json", "body_file", "no_enabled"]
    )]
    enabled: Option<bool>,
    #[arg(long, hide = true, overrides_with = "enabled")]
    no_enabled: bool,
    /// Use path-style URLs for S3 requests
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_force_path_style",
        required_unless_present_any = ["body_json", "body_file", "no_force_path_style"]
    )]
    force_path_style: Option<bool>,
    #[arg(long, hide = true, overrides_with = "force_path_style")]
    no_force_path_style: bool,
    /// File format of exported data
    #[arg(long, value_parser = ["JSON", "CSV", "JSONL", "PARQUET"], required_unless_present_any = REQUIRED)]
    file_type: Option<String>,
    /// What to export first
    #[arg(
        long,
        value_parser = ["FULL_HISTORY", "FROM_TODAY", "FROM_CUSTOM_DATE"],
        required_unless_present_any = REQUIRED
    )]
    export_mode: Option<String>,
    /// Start of the export with --export-mode FROM_CUSTOM_DATE (RFC 3339)
    #[arg(long, value_parser = date_time)]
    export_start_date: Option<String>,
    /// Gzip exported files [server default: true]
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_compressed"
    )]
    compressed: Option<bool>,
    #[arg(long, hide = true, overrides_with = "compressed")]
    no_compressed: bool,
    /// What data the integration exports
    #[arg(
        long,
        value_parser = ["LEGACY_TRACES_OBSERVATIONS", "OBSERVATIONS_V2", "LEGACY_TRACES_AND_ENRICHED_OBSERVATIONS"]
    )]
    export_source: Option<String>,
    /// Field group of exported rows (repeatable, or one JSON array): core, basic, time, io,
    /// metadata, model, usage, prompt, metrics, tools, trace_context, score_metadata,
    /// score_provenance
    #[arg(long, value_name = "GROUP")]
    export_field_groups: Vec<String>,
}

impl UpsertFields {
    fn body(&self) -> Result<Value> {
        let groups = expand_strings("export-field-groups", &self.export_field_groups)?;
        Ok(ObjectBody::new()
            .opt("projectId", self.project_id.clone())
            .opt("type", self.kind.clone())
            .opt("bucketName", self.bucket_name.clone())
            .opt("endpoint", self.endpoint.clone())
            .opt("region", self.region.clone())
            .opt("authType", self.auth_type.clone())
            .opt("roleArn", self.role_arn.clone())
            .opt("accessKeyId", self.access_key_id.clone())
            .opt("secretAccessKey", self.secret_access_key.clone())
            .opt("prefix", self.prefix.clone())
            .opt("exportFrequency", self.export_frequency.clone())
            .opt("enabled", tri_state(self.enabled, self.no_enabled))
            .opt(
                "forcePathStyle",
                tri_state(self.force_path_style, self.no_force_path_style),
            )
            .opt("fileType", self.file_type.clone())
            .opt("exportMode", self.export_mode.clone())
            .opt("exportStartDate", self.export_start_date.clone())
            .opt("compressed", tri_state(self.compressed, self.no_compressed))
            .opt("exportSource", self.export_source.clone())
            .items(
                "exportFieldGroups",
                groups.into_iter().map(Value::String).collect(),
            )
            .build())
    }
}

impl Operation for UpsertBlobStorage {
    const META: OperationMeta = OperationMeta::new(
        "integrations",
        "upsert-blob-storage",
        "blobStorageIntegrations_upsertBlobStorageIntegration",
        HttpMethod::Put,
        "/api/public/integrations/blob-storage",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => Ok(request.body(self.fields.body()?)),
        }
    }
}

path_op! {
    GetBlobStorageIntegrationStatus {
        /// The integration ID
        id => "id",
    }
    meta = OperationMeta::new(
        "integrations", "get-blob-storage-integration-status",
        "blobStorageIntegrations_getBlobStorageIntegrationStatus",
        HttpMethod::Get, "/api/public/integrations/blob-storage/{id}",
    );
}

path_op! {
    DeleteBlobStorage {
        /// The integration ID
        id => "id",
    }
    meta = OperationMeta::new(
        "integrations", "delete-blob-storage", "blobStorageIntegrations_deleteBlobStorageIntegration",
        HttpMethod::Delete, "/api/public/integrations/blob-storage/{id}",
    );
}

resource! {
    pub enum Command {
        /// Get the organization's blob storage integrations (organization-scoped key)
        ListBlobStorage(ListBlobStorage),
        /// Create or update a project's blob storage integration (organization-scoped key)
        ///
        /// The configuration is validated with a test upload to the bucket.
        UpsertBlobStorage(UpsertBlobStorage),
        /// Get the sync status of a blob storage integration (organization-scoped key)
        GetBlobStorageIntegrationStatus(GetBlobStorageIntegrationStatus),
        /// Delete a blob storage integration (organization-scoped key)
        DeleteBlobStorage(DeleteBlobStorage),
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

wire_enum!(BlobStorageIntegrationType, "SCREAMING_SNAKE_CASE", {
    S3,
    S3Compatible,
    AzureBlobStorage,
    GoogleCloudStorage,
});

wire_enum!(
    /// `ROLE_ARN` (Amazon S3 only) makes Langfuse assume an IAM role in your account.
    BlobStorageIntegrationAuthType, "SCREAMING_SNAKE_CASE", {
    AccessKeys,
    RoleArn,
});

wire_enum!(BlobStorageIntegrationFileType, "SCREAMING_SNAKE_CASE", {
    Json,
    Csv,
    Jsonl,
    Parquet,
});

wire_enum!(BlobStorageExportMode, "SCREAMING_SNAKE_CASE", {
    FullHistory,
    FromToday,
    FromCustomDate,
});

wire_enum!(BlobStorageExportFrequency, "snake_case", {
    #[serde(rename = "every_20_minutes")]
    Every20Minutes,
    Hourly,
    Daily,
    Weekly,
});

wire_enum!(
    /// Data model exported by the integration.
    BlobStorageExportSource, "SCREAMING_SNAKE_CASE", {
    LegacyTracesObservations,
    ObservationsV2,
    LegacyTracesAndEnrichedObservations,
});

wire_enum!(
    /// Column group included in each exported row.
    BlobStorageExportFieldGroup, "snake_case", {
    Core,
    Basic,
    Time,
    Io,
    Metadata,
    Model,
    Usage,
    Prompt,
    Metrics,
    Tools,
    TraceContext,
    ScoreMetadata,
    ScoreProvenance,
});

wire_enum!(BlobStorageSyncStatus, "snake_case", {
    Idle,
    Running,
    Queued,
    UpToDate,
    Disabled,
    Error,
});

/// Body of `PUT /api/public/integrations/blob-storage` (create or update).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBlobStorageIntegrationRequest {
    pub project_id: String,
    pub r#type: BlobStorageIntegrationType,
    pub bucket_name: String,
    /// Required for `S3_COMPATIBLE`; ignored for `GOOGLE_CLOUD_STORAGE`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Use `auto` for `GOOGLE_CLOUD_STORAGE`.
    pub region: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<BlobStorageIntegrationAuthType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_arn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_key_id: Option<String>,
    /// Omit on update to keep the stored secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_access_key: Option<String>,
    /// Must end with `/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub export_frequency: BlobStorageExportFrequency,
    pub enabled: bool,
    pub force_path_style: bool,
    pub file_type: BlobStorageIntegrationFileType,
    pub export_mode: BlobStorageExportMode,
    /// Required when `export_mode` is `FROM_CUSTOM_DATE`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_start_date: Option<DateTime<Utc>>,
    /// Server default `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed: Option<bool>,
    /// Required when `export_field_groups` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_source: Option<BlobStorageExportSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_field_groups: Option<Vec<BlobStorageExportFieldGroup>>,
}

impl CreateBlobStorageIntegrationRequest {
    /// A request with only the required fields set.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_id: impl Into<String>,
        r#type: BlobStorageIntegrationType,
        bucket_name: impl Into<String>,
        region: impl Into<String>,
        export_frequency: BlobStorageExportFrequency,
        enabled: bool,
        force_path_style: bool,
        file_type: BlobStorageIntegrationFileType,
        export_mode: BlobStorageExportMode,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            r#type,
            bucket_name: bucket_name.into(),
            endpoint: None,
            region: region.into(),
            auth_type: None,
            role_arn: None,
            access_key_id: None,
            secret_access_key: None,
            prefix: None,
            export_frequency,
            enabled,
            force_path_style,
            file_type,
            export_mode,
            export_start_date: None,
            compressed: None,
            export_source: None,
            export_field_groups: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobStorageIntegrationResponse {
    pub id: String,
    pub project_id: String,
    pub r#type: BlobStorageIntegrationType,
    pub bucket_name: String,
    pub endpoint: Option<String>,
    pub region: String,
    pub auth_type: BlobStorageIntegrationAuthType,
    pub access_key_id: Option<String>,
    pub role_arn: Option<String>,
    /// External ID to require in the role's trust policy; set once `ROLE_ARN` is used.
    pub external_id: Option<String>,
    /// Langfuse role that `role_arn` must trust.
    pub cross_account_role_arn: Option<String>,
    pub prefix: String,
    pub export_frequency: BlobStorageExportFrequency,
    pub enabled: bool,
    pub force_path_style: bool,
    pub file_type: BlobStorageIntegrationFileType,
    pub export_mode: BlobStorageExportMode,
    pub export_start_date: Option<DateTime<Utc>>,
    pub compressed: bool,
    pub export_source: BlobStorageExportSource,
    pub export_field_groups: Option<Vec<BlobStorageExportFieldGroup>>,
    pub next_sync_at: Option<DateTime<Utc>>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_error_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobStorageIntegrationsResponse {
    pub data: Vec<BlobStorageIntegrationResponse>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobStorageIntegrationStatusResponse {
    pub id: String,
    pub project_id: String,
    pub sync_status: BlobStorageSyncStatus,
    pub enabled: bool,
    /// End of the last exported time window (exports lag about 20 minutes).
    pub last_sync_at: Option<DateTime<Utc>>,
    pub next_sync_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_error_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobStorageIntegrationDeletionResponse {
    pub message: String,
}

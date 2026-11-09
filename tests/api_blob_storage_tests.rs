mod common;

use common::*;
use langfuse_rust_sdk::models::blob_storage_integrations::*;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path};

const BASE: &str = "/api/public/integrations/blob-storage";

fn integration_json() -> Value {
    json!({
        "id": "bs-1",
        "projectId": "p",
        "type": "S3",
        "bucketName": "exports",
        "endpoint": null,
        "region": "us-east-1",
        "authType": "ROLE_ARN",
        "accessKeyId": null,
        "roleArn": "arn:aws:iam::123456789012:role/langfuse-export",
        "externalId": "ext-1",
        "crossAccountRoleArn": "arn:aws:iam::999:role/langfuse",
        "prefix": "langfuse/",
        "exportFrequency": "every_20_minutes",
        "enabled": false,
        "forcePathStyle": false,
        "fileType": "PARQUET",
        "exportMode": "FROM_CUSTOM_DATE",
        "exportStartDate": TS,
        "compressed": true,
        "exportSource": "OBSERVATIONS_V2",
        "exportFieldGroups": ["core", "trace_context", "score_provenance"],
        "nextSyncAt": null,
        "lastSyncAt": null,
        "lastError": null,
        "lastErrorAt": null,
        "createdAt": TS,
        "updatedAt": TS
    })
}

#[tokio::test]
async fn list_integrations() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(BASE))
        .respond_with(json(200, json!({"data": [integration_json()]})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.blob_storage_integrations.list().await.unwrap();
    let integration = &resp.data[0];
    assert_eq!(integration.r#type, BlobStorageIntegrationType::S3);
    assert_eq!(
        integration.auth_type,
        BlobStorageIntegrationAuthType::RoleArn
    );
    assert_eq!(
        integration.export_frequency,
        BlobStorageExportFrequency::Every20Minutes
    );
    assert_eq!(
        integration.file_type,
        BlobStorageIntegrationFileType::Parquet
    );
    assert_eq!(
        integration.export_source,
        BlobStorageExportSource::ObservationsV2
    );
    assert_eq!(
        integration.export_field_groups.as_deref(),
        Some(
            &[
                BlobStorageExportFieldGroup::Core,
                BlobStorageExportFieldGroup::TraceContext,
                BlobStorageExportFieldGroup::ScoreProvenance
            ][..]
        )
    );
    assert!(integration.last_sync_at.is_none());
}

#[tokio::test]
async fn upsert_integration_uses_put() {
    let (client, server) = test_client().await;
    Mock::given(method("PUT"))
        .and(path(BASE))
        .and(body_json(json!({
            "projectId": "p",
            "type": "S3",
            "bucketName": "exports",
            "region": "us-east-1",
            "exportFrequency": "every_20_minutes",
            "enabled": true,
            "forcePathStyle": false,
            "fileType": "JSONL",
            "exportMode": "FULL_HISTORY",
            "authType": "ACCESS_KEYS",
            "accessKeyId": "AKIA",
            "secretAccessKey": "secret",
            "exportSource": "OBSERVATIONS_V2",
            "exportFieldGroups": ["core", "io"]
        })))
        .respond_with(json(200, integration_json()))
        .expect(1)
        .mount(&server)
        .await;

    let req = CreateBlobStorageIntegrationRequest {
        auth_type: Some(BlobStorageIntegrationAuthType::AccessKeys),
        access_key_id: Some("AKIA".into()),
        secret_access_key: Some("secret".into()),
        export_source: Some(BlobStorageExportSource::ObservationsV2),
        export_field_groups: Some(vec![
            BlobStorageExportFieldGroup::Core,
            BlobStorageExportFieldGroup::Io,
        ]),
        ..CreateBlobStorageIntegrationRequest::new(
            "p",
            BlobStorageIntegrationType::S3,
            "exports",
            "us-east-1",
            BlobStorageExportFrequency::Every20Minutes,
            true,
            false,
            BlobStorageIntegrationFileType::Jsonl,
            BlobStorageExportMode::FullHistory,
        )
    };
    client.blob_storage_integrations.upsert(&req).await.unwrap();
}

#[tokio::test]
async fn get_status() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/bs-1")))
        .respond_with(json(
            200,
            json!({
                "id": "bs-1", "projectId": "p", "syncStatus": "up_to_date", "enabled": true,
                "lastSyncAt": TS, "nextSyncAt": TS, "lastError": null, "lastErrorAt": null
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let status = client
        .blob_storage_integrations
        .get_status("bs-1")
        .await
        .unwrap();
    assert_eq!(status.sync_status, BlobStorageSyncStatus::UpToDate);
    assert!(status.last_sync_at.is_some());
}

#[tokio::test]
async fn delete_integration() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/bs-1")))
        .respond_with(json(200, json!({"message": "deleted"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .blob_storage_integrations
        .delete("bs-1")
        .await
        .unwrap();
    assert_eq!(resp.message, "deleted");
}

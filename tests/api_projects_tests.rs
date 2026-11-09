mod common;

use common::*;
use langfuse_sdk::api::projects::{
    CreateApiKeyRequest, CreateProjectRequest, UpdateProjectRequest,
};
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path};

fn project_json() -> Value {
    json!({
        "id": "proj-1",
        "name": "My Project",
        "organization": {"id": "org-1", "name": "Org"},
        "metadata": {},
        "retentionDays": 30
    })
}

#[tokio::test]
async fn list_projects() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/projects"))
        .respond_with(json(200, json!({"data": [project_json()]})))
        .expect(1)
        .mount(&server)
        .await;

    let projects = client.projects.list().await.unwrap();
    assert_eq!(projects.data[0].retention_days, Some(30));
}

#[tokio::test]
async fn create_project_sends_retention() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/projects"))
        .and(body_json(json!({"name": "My Project", "retention": 30})))
        .respond_with(json(200, project_json()))
        .expect(1)
        .mount(&server)
        .await;

    let project = client
        .projects
        .create(&CreateProjectRequest {
            name: "My Project".into(),
            metadata: None,
            retention: 30,
        })
        .await
        .unwrap();
    assert_eq!(project.id, "proj-1");
}

#[tokio::test]
async fn update_project_omits_retention_when_unset() {
    let (client, server) = test_client().await;
    Mock::given(method("PUT"))
        .and(path("/api/public/projects/proj-1"))
        .and(body_json(
            json!({"name": "Renamed", "metadata": {"team": "ml"}}),
        ))
        .respond_with(json(200, project_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .projects
        .update(
            "proj-1",
            &UpdateProjectRequest {
                name: "Renamed".into(),
                metadata: Some(json!({"team": "ml"})),
                retention: None,
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn update_project_sends_retention_when_set() {
    let (client, server) = test_client().await;
    Mock::given(method("PUT"))
        .and(path("/api/public/projects/proj-1"))
        .and(body_json(json!({"name": "P", "retention": 0})))
        .respond_with(json(200, project_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .projects
        .update(
            "proj-1",
            &UpdateProjectRequest {
                name: "P".into(),
                metadata: None,
                retention: Some(0),
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_project_parses_202_response() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/projects/proj-1"))
        .respond_with(json(
            202,
            json!({"success": true, "message": "Deletion queued"}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.projects.delete("proj-1").await.unwrap();
    assert!(resp.success);
    assert_eq!(resp.message, "Deletion queued");
}

#[tokio::test]
async fn get_api_keys() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/projects/proj-1/apiKeys"))
        .respond_with(json(
            200,
            json!({"apiKeys": [{
                "id": "key-1",
                "createdAt": TS,
                "expiresAt": null,
                "lastUsedAt": TS,
                "name": "ci",
                "note": "ci",
                "publicKey": "pk-lf-1",
                "displaySecretKey": "sk-lf-...abcd"
            }, {
                "id": "key-2",
                "createdAt": TS,
                "publicKey": "pk-lf-2",
                "displaySecretKey": "sk-lf-...efgh"
            }]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let keys = client.projects.get_api_keys("proj-1").await.unwrap();
    assert_eq!(keys.api_keys.len(), 2);
    assert_eq!(keys.api_keys[0].name.as_deref(), Some("ci"));
    assert!(keys.api_keys[0].last_used_at.is_some());
    assert!(keys.api_keys[1].expires_at.is_none());
}

#[tokio::test]
async fn create_api_key() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/projects/proj-1/apiKeys"))
        .and(body_json(
            json!({"name": "ci", "expiresAt": "2030-01-01T00:00:00Z"}),
        ))
        .respond_with(json(
            200,
            json!({
                "id": "key-3",
                "createdAt": TS,
                "publicKey": "pk-lf-3",
                "secretKey": "sk-lf-3",
                "displaySecretKey": "sk-lf-...3",
                "expiresAt": "2030-01-01T00:00:00Z",
                "name": "ci",
                "note": null
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let key = client
        .projects
        .create_api_key(
            "proj-1",
            &CreateApiKeyRequest {
                name: Some("ci".into()),
                expires_at: Some("2030-01-01T00:00:00Z".parse().unwrap()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(key.secret_key, "sk-lf-3");
    assert!(key.expires_at.is_some());
}

#[tokio::test]
async fn delete_api_key() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/projects/proj-1/apiKeys/key-1"))
        .respond_with(json(200, json!({"success": true})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .projects
        .delete_api_key("proj-1", "key-1")
        .await
        .unwrap();
    assert!(resp.success);
}

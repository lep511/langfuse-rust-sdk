mod common;

use common::*;
use langfuse_sdk::api::skills::{
    GetSkillParams, ListSkillsParams, SetSkillVersionLabelsRequest, UpdateSkillRequest,
};
use langfuse_sdk::models::skills::{CreateSkillVersionRequest, SkillVersionFileCreateInput};
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

fn skill_version_json() -> Value {
    json!({
        "id": "sv-1",
        "createdAt": TS,
        "updatedAt": TS,
        "projectId": "p",
        "createdBy": "u",
        "name": "review",
        "description": "Code review",
        "frontmatter": {},
        "version": 2,
        "tags": ["eng"],
        "labels": ["production"],
        "commitMessage": null,
        "files": [{
            "id": "f-1", "path": "SKILL.md", "blobId": "b-1", "sha256Hash": "h+/=",
            "contentType": "text/markdown", "contentLength": 12
        }]
    })
}

#[tokio::test]
async fn list_skills_parses_pagination_with_has_next_page() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/skills"))
        .and(query_param("search", "review"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "name": "review",
                    "description": "Code review",
                    "tags": ["eng"],
                    "latestVersion": 3,
                    "latestVersionCreatedAt": TS,
                    "latestVersionLastUpdatedAt": TS,
                    "productionVersion": null
                }],
                "meta": {"page": 1, "limit": 10, "totalItems": 11, "totalPages": 2, "hasNextPage": true}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .skills
        .list(&ListSkillsParams {
            search: Some("review".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(page.meta.has_next_page);
    assert_eq!(page.meta.total_items, 11);
    let skill = &page.data[0];
    assert_eq!(skill.latest_version, 3);
    assert!(skill.production_version.is_none());
}

#[tokio::test]
async fn create_version_with_content_and_reference() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/skills"))
        .and(body_json(json!({
            "files": [
                {"path": "SKILL.md", "content": "# Review"},
                {"path": "refs/a.md", "sha256Hash": "h+/="}
            ]
        })))
        .respond_with(json(201, skill_version_json()))
        .expect(1)
        .mount(&server)
        .await;

    let sv = client
        .skills
        .create_version(&CreateSkillVersionRequest {
            files: vec![
                SkillVersionFileCreateInput::content("SKILL.md", "# Review"),
                SkillVersionFileCreateInput::reference("refs/a.md", "h+/="),
            ],
            commit_message: None,
        })
        .await
        .unwrap();
    assert_eq!(sv.version, 2);
}

#[tokio::test]
async fn get_skill_by_version() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/skills/review"))
        .and(query_param("version", "2"))
        .and(query_param("includeContent", "true"))
        .respond_with(json(200, skill_version_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .skills
        .get(
            "review",
            &GetSkillParams {
                version: Some(2),
                include_content: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn update_skill_sends_required_tags() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path("/api/public/skills/review"))
        .and(body_json(json!({"tags": []})))
        .respond_with(json(200, skill_version_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .skills
        .update("review", &UpdateSkillRequest { tags: vec![] })
        .await
        .unwrap();
}

#[tokio::test]
async fn get_file_contents_joins_and_encodes_hashes() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/skills/files/content"))
        .respond_with(json(
            200,
            json!({"data": [
                {"sha256Hash": "a+b/c=", "content": "one"},
                {"sha256Hash": "d=", "content": "two"}
            ]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .skills
        .get_file_contents(&["a+b/c=", "d="])
        .await
        .unwrap();
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[1].content, "two");

    let req = single_request(&server).await;
    assert_eq!(req.url.query(), Some("sha256Hashes=a%2Bb%2Fc%3D%2Cd%3D"));
    assert_eq!(query_value(&req, "sha256Hashes"), "a+b/c=,d=");
}

#[tokio::test]
async fn set_version_labels() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path("/api/public/skills/review/versions/2"))
        .and(body_json(json!({"labels": ["production"]})))
        .respond_with(json(200, skill_version_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .skills
        .set_version_labels(
            "review",
            2,
            &SetSkillVersionLabelsRequest {
                labels: vec!["production".into()],
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_version_returns_deleted_flag() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/skills/review/versions/2"))
        .respond_with(json(200, json!({"deleted": true})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.skills.delete_version("review", 2).await.unwrap();
    assert!(resp.deleted);
}

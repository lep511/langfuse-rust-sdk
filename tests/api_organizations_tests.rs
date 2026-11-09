mod common;

use common::*;
use langfuse_rust_sdk::models::organizations::{MembershipRequest, MembershipRole};
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path};

const ORG_MEMBERSHIPS: &str = "/api/public/organizations/memberships";
const PROJECT_MEMBERSHIPS: &str = "/api/public/projects/proj-1/memberships";

fn membership_json(role: &str) -> Value {
    json!({"userId": "u-1", "role": role, "email": "ada@example.com", "name": "Ada"})
}

#[tokio::test]
async fn get_organization_memberships() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(ORG_MEMBERSHIPS))
        .respond_with(json(
            200,
            json!({"memberships": [membership_json("OWNER")]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.organizations.get_memberships().await.unwrap();
    assert_eq!(resp.memberships[0].role, MembershipRole::Owner);
}

#[tokio::test]
async fn update_organization_membership() {
    let (client, server) = test_client().await;
    Mock::given(method("PUT"))
        .and(path(ORG_MEMBERSHIPS))
        .and(body_json(json!({"userId": "u-1", "role": "ADMIN"})))
        .respond_with(json(200, membership_json("ADMIN")))
        .expect(1)
        .mount(&server)
        .await;

    let membership = client
        .organizations
        .update_membership(&MembershipRequest {
            user_id: "u-1".into(),
            role: MembershipRole::Admin,
        })
        .await
        .unwrap();
    assert_eq!(membership.email, "ada@example.com");
}

#[tokio::test]
async fn delete_organization_membership_sends_body() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(ORG_MEMBERSHIPS))
        .and(body_json(json!({"userId": "u-1"})))
        .respond_with(json(200, json!({"message": "removed", "userId": "u-1"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.organizations.delete_membership("u-1").await.unwrap();
    assert_eq!(resp.user_id, "u-1");
}

#[tokio::test]
async fn get_project_memberships() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(PROJECT_MEMBERSHIPS))
        .respond_with(json(
            200,
            json!({"memberships": [membership_json("VIEWER")]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .organizations
        .get_project_memberships("proj-1")
        .await
        .unwrap();
    assert_eq!(resp.memberships[0].role, MembershipRole::Viewer);
}

#[tokio::test]
async fn update_project_membership() {
    let (client, server) = test_client().await;
    Mock::given(method("PUT"))
        .and(path(PROJECT_MEMBERSHIPS))
        .and(body_json(json!({"userId": "u-1", "role": "MEMBER"})))
        .respond_with(json(200, membership_json("MEMBER")))
        .expect(1)
        .mount(&server)
        .await;

    client
        .organizations
        .update_project_membership(
            "proj-1",
            &MembershipRequest {
                user_id: "u-1".into(),
                role: MembershipRole::Member,
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_project_membership_sends_body() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(PROJECT_MEMBERSHIPS))
        .and(body_json(json!({"userId": "u-1"})))
        .respond_with(json(200, json!({"message": "removed", "userId": "u-1"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .organizations
        .delete_project_membership("proj-1", "u-1")
        .await
        .unwrap();
    assert_eq!(resp.message, "removed");
}

#[tokio::test]
async fn get_organization_projects() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/organizations/projects"))
        .respond_with(json(
            200,
            json!({"projects": [{"id": "proj-1", "name": "P", "metadata": null, "createdAt": TS, "updatedAt": TS}]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.organizations.get_projects().await.unwrap();
    assert_eq!(resp.projects[0].id, "proj-1");
}

#[tokio::test]
async fn get_organization_api_keys() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/organizations/apiKeys"))
        .respond_with(json(
            200,
            json!({"apiKeys": [{
                "id": "k-1", "createdAt": TS, "expiresAt": null, "lastUsedAt": null,
                "name": "org-admin", "note": "org-admin",
                "publicKey": "pk-lf-org", "displaySecretKey": "sk-lf-...xyz"
            }]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.organizations.get_api_keys().await.unwrap();
    assert_eq!(resp.api_keys[0].public_key, "pk-lf-org");
}

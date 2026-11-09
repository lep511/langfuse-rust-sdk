mod common;

use common::*;
use langfuse_rust_sdk::api::scim::ListScimUsersParams;
use langfuse_rust_sdk::models::scim::{CreateScimUserRequest, ScimEmail, ScimName};
use serde_json::{Value, json};
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const BASE: &str = "/api/public/scim";

fn user_json() -> Value {
    json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
        "id": "u-1",
        "userName": "ada@example.com",
        "name": {"formatted": "Ada Lovelace"},
        "emails": [{"primary": true, "value": "ada@example.com", "type": "work"}],
        "meta": {"resourceType": "User", "created": "2024-06-15T10:30:00Z", "lastModified": null}
    })
}

#[tokio::test]
async fn get_service_provider_config() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/ServiceProviderConfig")))
        .respond_with(json(
            200,
            json!({
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
                "documentationUri": "https://langfuse.com/docs",
                "patch": {"supported": false},
                "bulk": {"supported": false, "maxOperations": 0, "maxPayloadSize": 0},
                "filter": {"supported": true, "maxResults": 100},
                "changePassword": {"supported": false},
                "sort": {"supported": false},
                "etag": {"supported": false},
                "authenticationSchemes": [{
                    "name": "HTTP Basic", "description": "Basic auth", "specUri": "https://tools.ietf.org/html/rfc7617",
                    "type": "httpbasic", "primary": true
                }],
                "meta": {"resourceType": "ServiceProviderConfig", "location": "/ServiceProviderConfig"}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let config = client.scim.get_service_provider_config().await.unwrap();
    assert!(config.filter.supported);
    assert_eq!(config.filter.max_results, 100);
    assert_eq!(config.authentication_schemes[0].r#type, "httpbasic");
}

#[tokio::test]
async fn get_resource_types_reads_capitalized_resources() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/ResourceTypes")))
        .respond_with(json(
            200,
            json!({
                "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
                "totalResults": 1,
                "Resources": [{
                    "id": "User", "name": "User", "endpoint": "/Users", "description": "User Account",
                    "schema": "urn:ietf:params:scim:schemas:core:2.0:User",
                    "schemaExtensions": [{"schema": "urn:ext", "required": false}],
                    "meta": {"resourceType": "ResourceType", "location": "/ResourceTypes/User"}
                }]
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.scim.get_resource_types().await.unwrap();
    assert_eq!(resp.total_results, 1);
    assert_eq!(resp.resources[0].endpoint, "/Users");
    assert!(!resp.resources[0].schema_extensions[0].required);
}

#[tokio::test]
async fn get_schemas() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/Schemas")))
        .respond_with(json(
            200,
            json!({
                "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
                "totalResults": 1,
                "Resources": [{
                    "id": "urn:ietf:params:scim:schemas:core:2.0:User", "name": "User",
                    "description": "User Account", "attributes": [{"name": "userName"}],
                    "meta": {"resourceType": "Schema", "location": "/Schemas/User"}
                }]
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.scim.get_schemas().await.unwrap();
    assert_eq!(resp.resources[0].attributes[0]["name"], "userName");
}

#[tokio::test]
async fn list_users_with_scim_pagination() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/Users")))
        .and(query_param("filter", r#"userName eq "ada@example.com""#))
        .and(query_param("startIndex", "1"))
        .and(query_param("count", "10"))
        .respond_with(json(
            200,
            json!({
                "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
                "totalResults": 1, "startIndex": 1, "itemsPerPage": 10,
                "Resources": [user_json()]
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .scim
        .list_users(&ListScimUsersParams {
            filter: Some(r#"userName eq "ada@example.com""#.into()),
            start_index: Some(1),
            count: Some(10),
        })
        .await
        .unwrap();
    assert_eq!(resp.items_per_page, 10);
    assert_eq!(resp.resources[0].emails[0].r#type, "work");
}

#[tokio::test]
async fn create_user() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/Users")))
        .and(body_json(json!({
            "userName": "ada@example.com",
            "name": {"formatted": "Ada Lovelace"},
            "emails": [{"primary": true, "value": "ada@example.com", "type": "work"}],
            "active": true
        })))
        .respond_with(json(200, user_json()))
        .expect(1)
        .mount(&server)
        .await;

    let user = client
        .scim
        .create_user(&CreateScimUserRequest {
            user_name: "ada@example.com".into(),
            name: ScimName {
                formatted: Some("Ada Lovelace".into()),
            },
            emails: Some(vec![ScimEmail {
                primary: true,
                value: "ada@example.com".into(),
                r#type: "work".into(),
            }]),
            active: Some(true),
            password: None,
        })
        .await
        .unwrap();
    assert_eq!(user.meta.resource_type, "User");
    assert!(user.meta.last_modified.is_none());
}

#[tokio::test]
async fn get_user() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/Users/u-1")))
        .respond_with(json(200, user_json()))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        client.scim.get_user("u-1").await.unwrap().user_name,
        "ada@example.com"
    );
}

#[tokio::test]
async fn delete_user_accepts_empty_object_and_204() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/Users/u-1")))
        .respond_with(json(200, json!({})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/Users/u-2")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client.scim.delete_user("u-1").await.unwrap();
    client.scim.delete_user("u-2").await.unwrap();
}

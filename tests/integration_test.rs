use std::time::Duration;

use http::StatusCode;
use keycloak::{
    types::{
        ClientRepresentation, CredentialRepresentation, RealmRepresentation, RoleRepresentation,
        RolesRepresentation, UserRepresentation,
    },
    KeycloakAdmin,
};
use reqwest::Client;
use assertr::prelude::*;

use keycloak_container::KeycloakContainer;
use serde::Deserialize;

mod backend;
mod common;
mod keycloak_container;

#[tokio::test]
async fn test_integration() {
    common::tracing::init_subscriber();

    let keycloak_container = KeycloakContainer::start().await;

    let admin_client = keycloak_container.admin_client().await;

    configure_keycloak(&admin_client).await;

    let be_jh =
        backend::start_axum_backend(keycloak_container.url.clone(), "test-realm".to_owned()).await;

    let access_token = keycloak_container
        .perform_password_login(
            "test-user-mail@foo.bar",
            "password",
            "test-realm",
            "test-client",
        )
        .await;

    let response = Client::new()
        .get("http://127.0.0.1:9999/who-am-i")
        .bearer_auth(access_token)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();

    #[derive(Debug, Deserialize)]
    struct WhoAmIResponse {
        name: String,
        keycloak_uuid: String,
        token_valid_for_whole_seconds: i32,
    }

    tracing::info!(?response);
    let status = response.status();
    let data = response.json::<WhoAmIResponse>().await.unwrap();
    tracing::info!(?status, ?data);

    assert_that(status).is_equal_to(StatusCode::OK);
    assert_that(data.name.as_str()).is_equal_to("test-user-mail@foo.bar");
    assert_that(data.keycloak_uuid.as_str()).is_equal_to("a7060488-c80b-40c5-83e2-d7000bf9738e");
    assert_that(data.token_valid_for_whole_seconds).is_greater_than(0);

    be_jh.abort();
}

async fn configure_keycloak(admin_client: &KeycloakAdmin) {
    tracing::info!("Configuring Keycloak...");

    admin_client
        .post(RealmRepresentation {
            enabled: Some(true),
            realm: Some("test-realm".to_owned()),
            display_name: Some("test-realm".to_owned()),
            registration_email_as_username: Some(true),
            clients: Some(vec![
                // Being public allows and accepting direct-access-grants allows us to login with grant type "password".
                ClientRepresentation {
                    enabled: Some(true),
                    public_client: Some(true),
                    direct_access_grants_enabled: Some(true),
                    id: Some("test-client".to_owned()),
                    ..Default::default()
                },
            ]),
            roles: Some(RolesRepresentation {
                realm: Some(vec![RoleRepresentation {
                    name: Some("developer".to_owned()),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            users: Some(vec![
                // The user should be "fully set up" to allow logins!
                // No unverified mail, all required fields set (including names), no temporary password, no required pw reset action!
                UserRepresentation {
                    id: Some("a7060488-c80b-40c5-83e2-d7000bf9738e".to_owned()),
                    enabled: Some(true),
                    username: Some("test-user-mail@foo.bar".to_owned()),
                    email: Some("test-user-mail@foo.bar".to_owned()),
                    email_verified: Some(true),
                    first_name: Some("firstName".to_owned()),
                    last_name: Some("lastName".to_owned()),
                    realm_roles: Some(vec!["developer".to_owned()]),
                    credentials: Some(vec![CredentialRepresentation {
                        type_: Some("password".to_owned()),
                        value: Some("password".to_owned()),
                        temporary: Some(false),
                        ..Default::default()
                    }]),
                    required_actions: Some(vec![]),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        })
        .await
        .unwrap();
}

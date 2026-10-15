use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use base64::Engine;
use serde_json::{Value, json};

use super::{Client, MAX_BODY, Metadata, OAuthError, TokenReply, Wire};

#[path = "mcp_oauth_wire_fixture.rs"]
mod fixture;
use fixture::{Fixture, Response, good};

fn metadata(base: &str) -> Metadata {
    Metadata {
        issuer: format!("{base}/tenant"),
        authorization_endpoint: format!("{base}/authorize"),
        token_endpoint: format!("{base}/token"),
        revocation_endpoint: Some(format!("{base}/revoke")),
        authorization_response_iss_parameter_supported: true,
        token_endpoint_auth_methods_supported: vec!["none".to_owned()],
    }
}

fn document(base: &str) -> Value {
    let mut value = good(serde_json::to_value(metadata(base)));
    value["code_challenge_methods_supported"] = json!(["S256"]);
    value["response_types_supported"] = json!(["code"]);
    value["grant_types_supported"] = json!(["authorization_code", "refresh_token"]);
    value["scopes_supported"] = json!(["read", "everything"]);
    value
}

fn prm(base: &str) -> Value {
    json!({"resource": format!("{base}/tenant/mcp"), "authorization_servers": [format!("{base}/tenant")]})
}

fn public_client() -> Client {
    Client {
        client_id: "registered public client".to_owned(),
        client_secret: None,
    }
}

fn fields() -> Vec<(String, String)> {
    [
        ("grant_type", "authorization_code"),
        ("code", "synthetic-code"),
        ("redirect_uri", "http://127.0.0.1:12345/callback"),
        ("code_verifier", "synthetic-verifier"),
        ("resource", "https://mcp.example/resource"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect()
}

fn token_document() -> Value {
    json!({"access_token": "synthetic-access", "token_type": "Bearer", "expires_in": 3600,
        "refresh_token": "synthetic-refresh", "refresh_token_expires_in": 7200, "scope": "read"})
}

fn body_pairs(body: &str) -> Vec<(String, String)> {
    good(reqwest::Url::parse(&format!(
        "http://fixture.invalid/?{body}"
    )))
    .query_pairs()
    .into_owned()
    .collect()
}

async fn discover(fixture: &Fixture) -> Result<Metadata, OAuthError> {
    good(Wire::loopback())
        .discover(
            &format!("{}/tenant/mcp", fixture.base),
            &format!("{}/tenant", fixture.base),
        )
        .await
}

fn default_prm(fixture: &Fixture) {
    fixture.route(
        "/.well-known/oauth-protected-resource/tenant/mcp",
        Response::json(&prm(&fixture.base)),
    );
}

#[tokio::test]
async fn bearer_challenge_wins_and_handles_quoting_and_multiple_challenges() {
    let fixture = Fixture::new().await;
    let challenge = format!(
        "Basic realm=\"ignored, resource_metadata=other\", bEaReR realm=\"a, b \\\"quoted\\\"\", ReSoUrCe_MeTaDaTa = \"{}/chosen,metadata\", scope=\"read write\", Digest realm=\"other\"",
        fixture.base,
    );
    fixture.route(
        "/tenant/mcp",
        Response::text(401, "").header("WWW-Authenticate", &challenge),
    );
    fixture.route("/chosen,metadata", Response::json(&prm(&fixture.base)));
    fixture.route(
        "/.well-known/oauth-authorization-server/tenant",
        Response::json(&document(&fixture.base)),
    );
    let found = good(discover(&fixture).await);
    assert!(found == metadata(&fixture.base));
    assert_eq!(
        fixture.paths(),
        [
            "/tenant/mcp",
            "/chosen,metadata",
            "/.well-known/oauth-authorization-server/tenant"
        ]
    );
    assert!(
        fixture
            .requests()
            .iter()
            .all(|request| request.method == "GET"
                && request.body.is_empty()
                && !request.headers.contains_key("authorization"))
    );
}

#[tokio::test]
async fn separate_authenticate_headers_and_non_bearer_parameters_are_distinguished() {
    let fixture = Fixture::new().await;
    fixture.route(
        "/tenant/mcp",
        Response::text(401, "")
            .header(
                "WWW-Authenticate",
                "Basic resource_metadata=\"https://ignored.invalid/metadata\"",
            )
            .header(
                "WWW-Authenticate",
                &format!("Bearer resource_metadata=\"{}/advertised\"", fixture.base),
            ),
    );
    fixture.route("/advertised", Response::json(&prm(&fixture.base)));
    fixture.route(
        "/.well-known/oauth-authorization-server/tenant",
        Response::json(&document(&fixture.base)),
    );
    assert!(discover(&fixture).await.is_ok());
    assert_eq!(fixture.paths()[1], "/advertised");
}

#[tokio::test]
async fn ambiguous_or_malformed_challenges_fail_instead_of_selecting_a_destination() {
    for challenge in [
        "Bearer resource_metadata=\"https://first.invalid\", resource_metadata=\"https://second.invalid\"",
        "Bearer resource_metadata=\"unterminated",
        "Bearer resource_metadata=\"https://first.invalid\"trailing",
        "Bearer resource_metadata=https://unquoted.invalid",
    ] {
        let fixture = Fixture::new().await;
        fixture.route(
            "/tenant/mcp",
            Response::text(401, "").header("WWW-Authenticate", challenge),
        );
        assert!(matches!(
            discover(&fixture).await,
            Err(OAuthError::Discovery)
        ));
        assert_eq!(fixture.paths(), ["/tenant/mcp"]);
    }
}

#[tokio::test]
async fn duplicate_bearer_metadata_across_headers_is_refused() {
    let fixture = Fixture::new().await;
    let challenge = format!("Bearer resource_metadata=\"{}/metadata\"", fixture.base);
    fixture.route(
        "/tenant/mcp",
        Response::text(401, "")
            .header("WWW-Authenticate", &challenge)
            .header("WWW-Authenticate", &challenge),
    );
    assert!(matches!(
        discover(&fixture).await,
        Err(OAuthError::Discovery)
    ));
    assert_eq!(fixture.paths(), ["/tenant/mcp"]);
}

#[tokio::test]
async fn prm_path_then_root_and_oauth_then_oidc_insertion_then_appending() {
    let fixture = Fixture::new().await;
    fixture.route(
        "/.well-known/oauth-protected-resource",
        Response::json(&prm(&fixture.base)),
    );
    fixture.route(
        "/tenant/.well-known/openid-configuration",
        Response::json(&document(&fixture.base)),
    );
    assert!(discover(&fixture).await.is_ok());
    assert_eq!(
        fixture.paths(),
        [
            "/tenant/mcp",
            "/.well-known/oauth-protected-resource/tenant/mcp",
            "/.well-known/oauth-protected-resource",
            "/.well-known/oauth-authorization-server/tenant",
            "/.well-known/openid-configuration/tenant",
            "/tenant/.well-known/openid-configuration",
        ]
    );
}

#[tokio::test]
async fn oidc_insertion_stops_before_appending() {
    let fixture = Fixture::new().await;
    default_prm(&fixture);
    fixture.route(
        "/.well-known/openid-configuration/tenant",
        Response::json(&document(&fixture.base)),
    );
    assert!(discover(&fixture).await.is_ok());
    assert_eq!(
        fixture.paths(),
        [
            "/tenant/mcp",
            "/.well-known/oauth-protected-resource/tenant/mcp",
            "/.well-known/oauth-authorization-server/tenant",
            "/.well-known/openid-configuration/tenant"
        ]
    );
}

#[tokio::test]
async fn root_resource_and_issuer_have_no_duplicate_probes() {
    let fixture = Fixture::new().await;
    fixture.route(
        "/.well-known/oauth-protected-resource",
        Response::json(&json!({
            "resource": fixture.base, "authorization_servers": [fixture.base],
        })),
    );
    let mut doc = document(&fixture.base);
    doc["issuer"] = json!(fixture.base);
    fixture.route("/.well-known/openid-configuration", Response::json(&doc));
    assert!(
        good(Wire::loopback())
            .discover(&fixture.base, &fixture.base)
            .await
            .is_ok()
    );
    assert_eq!(
        fixture.paths(),
        [
            "/",
            "/.well-known/oauth-protected-resource",
            "/.well-known/oauth-authorization-server",
            "/.well-known/openid-configuration"
        ]
    );
}

#[tokio::test]
async fn resource_and_issuer_must_match_exactly_without_normalization() {
    for (key, replacement) in [
        ("resource", "https://other.invalid"),
        ("resource", "trailing-slash"),
        ("authorization_servers", "https://other.invalid"),
    ] {
        let fixture = Fixture::new().await;
        let mut protected = prm(&fixture.base);
        protected[key] = if key == "authorization_servers" {
            json!([replacement])
        } else if replacement == "trailing-slash" {
            json!(format!("{}/tenant/mcp/", fixture.base))
        } else {
            json!(replacement)
        };
        fixture.route(
            "/.well-known/oauth-protected-resource/tenant/mcp",
            Response::json(&protected),
        );
        assert!(matches!(discover(&fixture).await, Err(OAuthError::Issuer)));
        assert_eq!(fixture.requests().len(), 2);
    }
}

#[tokio::test]
async fn expected_issuer_is_selected_even_when_not_first() {
    let fixture = Fixture::new().await;
    let mut protected = prm(&fixture.base);
    protected["authorization_servers"] =
        json!(["https://other.invalid", format!("{}/tenant", fixture.base)]);
    fixture.route(
        "/.well-known/oauth-protected-resource/tenant/mcp",
        Response::json(&protected),
    );
    fixture.route(
        "/.well-known/oauth-authorization-server/tenant",
        Response::json(&document(&fixture.base)),
    );
    assert!(discover(&fixture).await.is_ok());
    assert_eq!(fixture.requests().len(), 3);
}

#[tokio::test]
async fn issuer_mismatch_never_uses_an_oidc_fallback() {
    let fixture = Fixture::new().await;
    default_prm(&fixture);
    let mut doc = document(&fixture.base);
    doc["issuer"] = json!(format!("{}/tenant/", fixture.base));
    fixture.route(
        "/.well-known/oauth-authorization-server/tenant",
        Response::json(&doc),
    );
    assert!(matches!(discover(&fixture).await, Err(OAuthError::Issuer)));
    assert_eq!(fixture.requests().len(), 3);
}

#[tokio::test]
async fn s256_advertisement_is_required_for_oauth_and_oidc() {
    for path in [
        "/.well-known/oauth-authorization-server/tenant",
        "/.well-known/openid-configuration/tenant",
    ] {
        for methods in [
            None,
            Some(json!(["plain"])),
            Some(json!(["s256"])),
            Some(json!([])),
        ] {
            let fixture = Fixture::new().await;
            default_prm(&fixture);
            let mut doc = document(&fixture.base);
            if let Some(methods) = methods {
                doc["code_challenge_methods_supported"] = methods;
            } else {
                good(doc.as_object_mut().ok_or("object"))
                    .remove("code_challenge_methods_supported");
            }
            fixture.route(path, Response::json(&doc));
            assert!(matches!(discover(&fixture).await, Err(OAuthError::Pkce)));
        }
    }
}

#[tokio::test]
async fn declared_grants_responses_and_endpoint_shapes_are_checked() {
    for (key, value) in [
        ("response_types_supported", json!(["token"])),
        ("grant_types_supported", json!(["client_credentials"])),
        (
            "authorization_endpoint",
            json!("https://auth.invalid/authorize?injected=value"),
        ),
        (
            "token_endpoint",
            json!("https://user:secret@auth.invalid/token"),
        ),
        ("token_endpoint", json!(null)),
        (
            "revocation_endpoint",
            json!("https://auth.invalid/revoke#fragment"),
        ),
    ] {
        let fixture = Fixture::new().await;
        default_prm(&fixture);
        let mut doc = document(&fixture.base);
        doc[key] = value;
        fixture.route(
            "/.well-known/oauth-authorization-server/tenant",
            Response::json(&doc),
        );
        assert!(matches!(
            discover(&fixture).await,
            Err(OAuthError::Discovery)
        ));
    }
}

#[tokio::test]
async fn omitted_optional_metadata_uses_rfc_auth_default_but_not_catalog_scopes() {
    let fixture = Fixture::new().await;
    default_prm(&fixture);
    let mut doc = document(&fixture.base);
    for key in [
        "token_endpoint_auth_methods_supported",
        "response_types_supported",
        "grant_types_supported",
        "revocation_endpoint",
        "authorization_response_iss_parameter_supported",
    ] {
        good(doc.as_object_mut().ok_or("object")).remove(key);
    }
    fixture.route(
        "/.well-known/oauth-authorization-server/tenant",
        Response::json(&doc),
    );
    let found = good(discover(&fixture).await);
    assert_eq!(
        found.token_endpoint_auth_methods_supported,
        ["client_secret_basic"]
    );
    assert!(found.revocation_endpoint.is_none());
    assert!(!found.authorization_response_iss_parameter_supported);
    let serialized = good(serde_json::to_value(found));
    assert!(serialized.get("scopes_supported").is_none());
}

#[tokio::test]
async fn public_client_posts_exact_parent_fields_and_no_authorization_header() {
    let fixture = Fixture::new().await;
    fixture.route("/token", Response::json(&token_document()));
    let reply = good(
        good(Wire::loopback())
            .token(&metadata(&fixture.base), &public_client(), &fields())
            .await,
    );
    assert_eq!(reply.access_token, "synthetic-access");
    assert_eq!(reply.token_type, "Bearer");
    assert_eq!(reply.expires_in, Some(3600));
    assert_eq!(reply.refresh_token.as_deref(), Some("synthetic-refresh"));
    assert_eq!(reply.refresh_token_expires_in, Some(7200));
    assert_eq!(reply.scope.as_deref(), Some("read"));
    let requests = fixture.requests();
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert!(!request.headers.contains_key("authorization"));
    assert_eq!(
        request.headers.get("content-type").map(String::as_str),
        Some("application/x-www-form-urlencoded")
    );
    let mut expected = fields();
    expected.push(("client_id".to_owned(), public_client().client_id));
    assert_eq!(body_pairs(&request.body), expected);
}

#[tokio::test]
async fn confidential_client_prefers_basic_and_encodes_credentials_before_base64() {
    let fixture = Fixture::new().await;
    fixture.route("/token", Response::json(&token_document()));
    let mut metadata = metadata(&fixture.base);
    metadata.token_endpoint_auth_methods_supported = vec![
        "client_secret_post".to_owned(),
        "client_secret_basic".to_owned(),
    ];
    let client = Client {
        client_id: "id :+é".to_owned(),
        client_secret: Some("secret :+/%~".to_owned()),
    };
    assert!(
        good(Wire::loopback())
            .token(&metadata, &client, &fields())
            .await
            .is_ok()
    );
    let requests = fixture.requests();
    let encoded = good(
        requests[0]
            .headers
            .get("authorization")
            .ok_or("authorization"),
    );
    let basic = good(encoded.strip_prefix("Basic ").ok_or("basic"));
    let decoded = good(base64::engine::general_purpose::STANDARD.decode(basic));
    assert_eq!(
        good(String::from_utf8(decoded)),
        "id+%3A%2B%C3%A9:secret+%3A%2B%2F%25%7E"
    );
    assert_eq!(body_pairs(&requests[0].body), fields());
}

#[tokio::test]
async fn client_secret_post_and_refresh_use_one_auth_method_and_frozen_destination() {
    let fixture = Fixture::new().await;
    fixture.route("/token", Response::json(&token_document()));
    let mut metadata = metadata(&fixture.base);
    metadata.token_endpoint_auth_methods_supported = vec!["client_secret_post".to_owned()];
    let client = Client {
        client_id: "registered id".to_owned(),
        client_secret: Some("synthetic secret&+".to_owned()),
    };
    let fields = vec![
        ("grant_type".to_owned(), "refresh_token".to_owned()),
        ("refresh_token".to_owned(), "synthetic refresh".to_owned()),
        (
            "resource".to_owned(),
            "https://mcp.example/resource".to_owned(),
        ),
    ];
    assert!(
        good(Wire::loopback())
            .token(&metadata, &client, &fields)
            .await
            .is_ok()
    );
    let requests = fixture.requests();
    assert_eq!(requests[0].target, "/token");
    assert!(!requests[0].headers.contains_key("authorization"));
    let mut expected = fields;
    expected.push(("client_id".to_owned(), client.client_id));
    expected.push((
        "client_secret".to_owned(),
        good(client.client_secret.ok_or("secret")),
    ));
    assert_eq!(body_pairs(&requests[0].body), expected);
}

#[tokio::test]
async fn unsupported_auth_and_auth_field_override_send_nothing() {
    let fixture = Fixture::new().await;
    let mut metadata = metadata(&fixture.base);
    metadata.token_endpoint_auth_methods_supported = vec!["private_key_jwt".to_owned()];
    let wire = good(Wire::loopback());
    let confidential = Client {
        client_id: "id".to_owned(),
        client_secret: Some("secret".to_owned()),
    };
    for client in [&public_client(), &confidential] {
        assert!(matches!(
            wire.token(&metadata, client, &fields()).await,
            Err(OAuthError::InvalidSetup)
        ));
    }
    metadata.token_endpoint_auth_methods_supported = vec!["none".to_owned()];
    for extra in ["client_id", "client_secret", "grant_type"] {
        let mut request_fields = fields();
        request_fields.push((extra.to_owned(), "override".to_owned()));
        assert!(matches!(
            wire.token(&metadata, &public_client(), &request_fields)
                .await,
            Err(OAuthError::InvalidSetup)
        ));
    }
    assert!(fixture.requests().is_empty());
}

#[tokio::test]
async fn oauth_errors_are_refused_even_in_a_successful_token_body() {
    for error in [
        json!("provider-private-error"),
        json!(null),
        json!({"private": "provider"}),
    ] {
        let fixture = Fixture::new().await;
        let mut body = token_document();
        body["error"] = error;
        body["error_description"] = json!("provider-private-description");
        assert!(serde_json::from_value::<TokenReply>(body.clone()).is_err());
        fixture.route("/token", Response::json(&body));
        let result = good(Wire::loopback())
            .token(&metadata(&fixture.base), &public_client(), &fields())
            .await;
        assert!(matches!(result, Err(OAuthError::Token)));
    }
}

#[tokio::test]
async fn malformed_and_unsuccessful_token_responses_are_value_free() {
    for response in [
        Response::text(400, "provider-private-description"),
        Response::text(200, "not json"),
        Response::json(&json!({"error": "provider-private-error"})),
        Response::json(
            &json!({"access_token": "secret", "token_type": "Bearer", "expires_in": -1}),
        ),
    ] {
        let fixture = Fixture::new().await;
        fixture.route("/token", response);
        let result = good(Wire::loopback())
            .token(&metadata(&fixture.base), &public_client(), &fields())
            .await;
        assert!(matches!(result, Err(OAuthError::Token)));
    }
}

#[tokio::test]
async fn revocation_uses_only_advertised_endpoint_and_form_auth() {
    let fixture = Fixture::new().await;
    let mut metadata = metadata(&fixture.base);
    let wire = good(Wire::loopback());
    metadata.revocation_endpoint = None;
    assert_eq!(
        wire.revoke(&metadata, &public_client(), "synthetic-access")
            .await,
        Err(OAuthError::Revocation)
    );
    assert!(fixture.requests().is_empty());
    metadata.revocation_endpoint = Some(format!("{}/revoke", fixture.base));
    for status in [200, 201, 400] {
        fixture.route(
            "/revoke",
            Response::text(status, "provider-private-response"),
        );
        assert_eq!(
            wire.revoke(&metadata, &public_client(), "synthetic-access")
                .await
                .is_ok(),
            status == 200
        );
    }
    for request in fixture.requests() {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/revoke");
        assert_eq!(
            body_pairs(&request.body),
            [
                ("token".to_owned(), "synthetic-access".to_owned()),
                ("client_id".to_owned(), public_client().client_id)
            ]
        );
    }
}

#[tokio::test]
async fn redirects_never_receive_discovery_or_credential_requests() {
    let fixture = Fixture::new().await;
    fixture.route(
        "/token",
        Response::text(307, "").header("Location", &format!("{}/stolen", fixture.base)),
    );
    fixture.route(
        "/revoke",
        Response::text(302, "").header("Location", &format!("{}/stolen", fixture.base)),
    );
    let wire = good(Wire::loopback());
    assert!(matches!(
        wire.token(&metadata(&fixture.base), &public_client(), &fields())
            .await,
        Err(OAuthError::Token)
    ));
    assert_eq!(
        wire.revoke(
            &metadata(&fixture.base),
            &public_client(),
            "synthetic-access"
        )
        .await,
        Err(OAuthError::Revocation)
    );
    fixture.route(
        "/.well-known/oauth-protected-resource/tenant/mcp",
        Response::text(302, "").header("Location", &format!("{}/stolen", fixture.base)),
    );
    assert!(matches!(
        discover(&fixture).await,
        Err(OAuthError::Discovery)
    ));
    assert!(!fixture.paths().iter().any(|path| path == "/stolen"));
}

#[tokio::test]
async fn response_limit_checks_content_length_and_streamed_bytes() {
    for chunked in [false, true] {
        let fixture = Fixture::new().await;
        let response = Response::text(200, &"x".repeat(MAX_BODY + 1));
        fixture.route(
            "/token",
            if chunked {
                response.chunked()
            } else {
                response
            },
        );
        assert!(matches!(
            good(Wire::loopback())
                .token(&metadata(&fixture.base), &public_client(), &fields())
                .await,
            Err(OAuthError::Unreachable)
        ));
        assert_eq!(fixture.requests().len(), 1);
    }
}

#[tokio::test]
async fn exact_response_byte_limit_is_inclusive() {
    let fixture = Fixture::new().await;
    let mut body = token_document().to_string();
    body.push_str(&" ".repeat(MAX_BODY - body.len()));
    fixture.route("/token", Response::text(200, &body));
    assert!(
        good(Wire::loopback())
            .token(&metadata(&fixture.base), &public_client(), &fields())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn advertised_metadata_failure_never_falls_back_to_another_resource_document() {
    let fixture = Fixture::new().await;
    fixture.route(
        "/tenant/mcp",
        Response::text(401, "").header(
            "WWW-Authenticate",
            &format!("Bearer resource_metadata=\"{}/missing\"", fixture.base),
        ),
    );
    default_prm(&fixture);
    assert!(matches!(
        discover(&fixture).await,
        Err(OAuthError::Discovery)
    ));
    assert_eq!(fixture.paths(), ["/tenant/mcp", "/missing"]);
}

#[tokio::test]
async fn successful_but_malformed_metadata_never_falls_back() {
    let fixture = Fixture::new().await;
    fixture.route(
        "/.well-known/oauth-protected-resource/tenant/mcp",
        Response::text(200, "<html>provider-private-response</html>"),
    );
    fixture.route(
        "/.well-known/oauth-protected-resource",
        Response::json(&prm(&fixture.base)),
    );
    assert!(matches!(
        discover(&fixture).await,
        Err(OAuthError::Discovery)
    ));
    assert_eq!(fixture.requests().len(), 2);
}

#[tokio::test]
async fn oversized_discovery_body_and_unsafe_challenge_are_not_used() {
    let fixture = Fixture::new().await;
    fixture.route(
        "/.well-known/oauth-protected-resource/tenant/mcp",
        Response::text(200, &"x".repeat(MAX_BODY + 1)),
    );
    assert!(matches!(
        discover(&fixture).await,
        Err(OAuthError::Unreachable)
    ));
    assert_eq!(fixture.requests().len(), 2);
    let fixture = Fixture::new().await;
    fixture.route(
        "/tenant/mcp",
        Response::text(401, "").header(
            "WWW-Authenticate",
            "Bearer resource_metadata=\"http://10.0.0.1/private\"",
        ),
    );
    assert!(discover(&fixture).await.is_err());
    assert_eq!(fixture.requests().len(), 1);
}

#[test]
fn syntax_validation_is_offline_and_production_cannot_enable_loopback() {
    let production = good(Wire::new());
    let test = good(Wire::loopback());
    for url in [
        "https://oauth.example/tenant",
        "https://does-not-exist.invalid/issuer",
    ] {
        assert_eq!(production.validate_url(url), Ok(()));
    }
    for url in [
        "http://oauth.example",
        "https://localhost",
        "https://LOCALHOST.",
        "https://x.localhost/",
        "https://user:secret@oauth.example",
        "https://@oauth.example",
        "https://oauth.example?q=secret",
        "https://oauth.example#secret",
        "https://oauth.example/?",
        "http://127.0.0.1:12345",
        "http://[::1]:12345",
        "https://oauth.example/ white",
        "https://oauth.example:0/",
        "https://oauth.example\\private",
        "https:oauth.example",
        "https://127.0.0.1:12345",
        "https://10.0.0.1/private",
        "https://[::ffff:127.0.0.1]/private",
    ] {
        assert!(production.validate_url(url).is_err(), "must reject {url}");
    }
    assert!(test.validate_url("http://127.0.0.1:12345").is_ok());
    assert!(test.validate_url("http://[::1]:12345").is_ok());
    assert!(test.validate_url("http://localhost:12345").is_err());
    assert!(test.validate_url("http://10.0.0.1:12345").is_err());
}

#[test]
fn any_private_dns_answer_is_refused_and_test_exception_requires_a_literal() {
    let wire = good(Wire::new());
    let public = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 443);
    for address in [
        "127.0.0.1:443",
        "10.0.0.1:443",
        "169.254.169.254:443",
        "[::1]:443",
        "[::ffff:127.0.0.1]:443",
    ] {
        let private = good(address.parse());
        assert_eq!(
            wire.check_addresses("provider.example", &[public, private]),
            Err(OAuthError::Refused)
        );
        assert_eq!(
            good(Wire::loopback()).check_addresses("provider.example", &[private]),
            Err(OAuthError::Refused)
        );
    }
    assert_eq!(wire.check_addresses("provider.example", &[public]), Ok(()));
    assert!(wire.check_addresses("provider.example", &[]).is_err());
}

#[tokio::test]
async fn production_network_guard_refuses_literal_and_resolved_loopback_before_http() {
    let fixture = Fixture::new().await;
    let wire = good(Wire::new());
    let mut metadata = metadata(&fixture.base);
    metadata.token_endpoint = fixture.base.replacen("http:", "https:", 1) + "/token";
    assert!(matches!(
        wire.token(&metadata, &public_client(), &fields()).await,
        Err(OAuthError::Refused)
    ));
    let domain_loopback = good(reqwest::Url::parse(&metadata.token_endpoint));
    let port = good(domain_loopback.port().ok_or("port"));
    // The fixture cannot be contacted: a DNS name resolving to loopback gets no test exception.
    metadata.token_endpoint = format!("https://localhost.localdomain:{port}/token");
    assert!(
        wire.token(&metadata, &public_client(), &fields())
            .await
            .is_err()
    );
    assert!(fixture.requests().is_empty());
}

#[test]
fn errors_are_fixed_copyable_and_have_no_provider_source() {
    use std::error::Error;
    for error in [
        OAuthError::InvalidSetup,
        OAuthError::Discovery,
        OAuthError::Issuer,
        OAuthError::Pkce,
        OAuthError::Refused,
        OAuthError::Token,
        OAuthError::Scopes,
        OAuthError::Storage,
        OAuthError::Unreachable,
        OAuthError::NotConnected,
        OAuthError::Expired,
        OAuthError::Revocation,
    ] {
        let copy = error;
        assert_eq!(error, copy);
        assert!(error.source().is_none());
        let message = error.to_string();
        assert!(!message.contains("https://"));
        assert!(!message.contains("provider-private"));
    }
}

use reqwest::{Method, Url, header::WWW_AUTHENTICATE};
use serde::Deserialize;

use super::{Metadata, OAuthError, Reply, Wire, decode};

#[path = "mcp_oauth_wire_challenge.rs"]
mod challenge;

#[derive(Deserialize)]
struct ProtectedResource {
    resource: String,
    authorization_servers: Vec<String>,
}

#[derive(Deserialize)]
struct ServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    #[serde(default)]
    authorization_response_iss_parameter_supported: bool,
    #[serde(default = "default_auth_methods")]
    token_endpoint_auth_methods_supported: Vec<String>,
    code_challenge_methods_supported: Option<Vec<String>>,
    response_types_supported: Option<Vec<String>>,
    grant_types_supported: Option<Vec<String>>,
}

fn default_auth_methods() -> Vec<String> {
    vec!["client_secret_basic".to_owned()]
}

pub(super) async fn discover(
    wire: &Wire,
    resource: &str,
    expected_issuer: &str,
) -> Result<Metadata, OAuthError> {
    let resource_url = wire.parse_url(resource)?;
    let issuer_url = wire.parse_url(expected_issuer)?;
    let response = wire.exchange(resource, Method::GET, &[], None).await?;
    let advertised = advertised_metadata(&response)?;
    let locations = advertised.map_or_else(
        || protected_locations(&resource_url),
        |location| vec![location],
    );
    let protected: ProtectedResource = fetch_first(wire, locations).await?;
    if protected.resource != resource
        || !protected
            .authorization_servers
            .iter()
            .any(|issuer| issuer == expected_issuer)
    {
        return Err(OAuthError::Issuer);
    }
    let raw: ServerMetadata = fetch_first(wire, issuer_locations(&issuer_url)).await?;
    validate_metadata(wire, raw, expected_issuer)
}

fn advertised_metadata(response: &Reply) -> Result<Option<String>, OAuthError> {
    let mut location = None;
    for header in response.headers.get_all(WWW_AUTHENTICATE) {
        let text = header.to_str().map_err(|_| OAuthError::Discovery)?;
        if let Some(found) = challenge::resource_metadata(text)?
            && location.replace(found).is_some()
        {
            return Err(OAuthError::Discovery);
        }
    }
    Ok(location)
}

async fn fetch_first<T: serde::de::DeserializeOwned>(
    wire: &Wire,
    locations: Vec<String>,
) -> Result<T, OAuthError> {
    for location in locations {
        let response = wire.exchange(&location, Method::GET, &[], None).await?;
        if response.status.is_redirection() {
            return Err(OAuthError::Discovery);
        }
        if response.status.is_success() {
            if response.status != reqwest::StatusCode::OK {
                return Err(OAuthError::Discovery);
            }
            return decode(&response, OAuthError::Discovery);
        }
    }
    Err(OAuthError::Discovery)
}

fn validate_metadata(
    wire: &Wire,
    raw: ServerMetadata,
    expected_issuer: &str,
) -> Result<Metadata, OAuthError> {
    if raw.issuer != expected_issuer {
        return Err(OAuthError::Issuer);
    }
    if !raw
        .code_challenge_methods_supported
        .as_ref()
        .is_some_and(|methods| methods.iter().any(|item| item == "S256"))
    {
        return Err(OAuthError::Pkce);
    }
    if raw
        .response_types_supported
        .as_ref()
        .is_some_and(|types| !types.iter().any(|item| item == "code"))
        || raw
            .grant_types_supported
            .as_ref()
            .is_some_and(|types| !types.iter().any(|item| item == "authorization_code"))
    {
        return Err(OAuthError::Discovery);
    }
    wire.validate_url(&raw.authorization_endpoint)
        .map_err(|_| OAuthError::Discovery)?;
    wire.validate_url(&raw.token_endpoint)
        .map_err(|_| OAuthError::Discovery)?;
    if let Some(endpoint) = &raw.revocation_endpoint {
        wire.validate_url(endpoint)
            .map_err(|_| OAuthError::Discovery)?;
    }
    Ok(Metadata {
        issuer: raw.issuer,
        authorization_endpoint: raw.authorization_endpoint,
        token_endpoint: raw.token_endpoint,
        revocation_endpoint: raw.revocation_endpoint,
        authorization_response_iss_parameter_supported: raw
            .authorization_response_iss_parameter_supported,
        token_endpoint_auth_methods_supported: raw.token_endpoint_auth_methods_supported,
    })
}

fn protected_locations(resource: &Url) -> Vec<String> {
    let path = resource.path().trim_end_matches('/');
    let root = at_path(resource, "/.well-known/oauth-protected-resource");
    if path.is_empty() {
        vec![root]
    } else {
        vec![
            at_path(
                resource,
                &format!("/.well-known/oauth-protected-resource{path}"),
            ),
            root,
        ]
    }
}

fn issuer_locations(issuer: &Url) -> Vec<String> {
    let path = issuer.path().trim_end_matches('/');
    let mut locations = vec![
        at_path(
            issuer,
            &format!("/.well-known/oauth-authorization-server{path}"),
        ),
        at_path(issuer, &format!("/.well-known/openid-configuration{path}")),
    ];
    if !path.is_empty() {
        locations.push(at_path(
            issuer,
            &format!("{path}/.well-known/openid-configuration"),
        ));
    }
    locations
}

fn at_path(base: &Url, path: &str) -> String {
    let mut url = base.clone();
    url.set_path(path);
    url.into()
}

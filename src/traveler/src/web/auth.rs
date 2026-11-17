//! Sign-in (design section 5.1): the OIDC authorization-code flow with PKCE against Erawan's auth add-on, the
//! handshake and the member session in signed cookies, the development sign-in, and logout.

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        clippy::exit
    )
)]

use std::collections::HashMap;
use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::{SignedDuration, Timestamp};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rzlib_error::{ErrorInfo, Outcome, errorinfo};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use super::cookies::{Kind, clear_cookie, cookie_value, is_secure, set_cookie};
use super::errors::Failure;
use super::extract::MemberWrite;
use super::{AppState, Served};
use crate::config::Oidc;
use crate::errors::Code;
use crate::facade::random_token;
use crate::model::{Identity, MemberSession, OidcHandshake, User};
use crate::panic::caught;

/// The bound on each call to the identity provider (bylaw 2).
pub const PROVIDER_TIMEOUT: Duration = Duration::from_secs(5);

/// The providers the login page offers, passed to Erawan auth as `idp_hint`.
pub const PROVIDERS: [&str; 3] = ["google", "line", "email"];

#[derive(Debug, Clone, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

/// The claims of a verified id_token this service reads. The provider advertises `sub` alone; `email` and `name`
/// are taken when present.
#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// The identity provider: discovery and JWKS fetched once per process (the JWKS again on an unknown `kid`).
pub struct OidcClient {
    settings: Oidc,
    http: reqwest::Client,
    discovery: RwLock<Option<Discovery>>,
    keys: RwLock<HashMap<String, DecodingKey>>,
}

fn provider_error(message: impl Into<String>) -> ErrorInfo {
    errorinfo!(Code::SERVER_ERROR, message, "identity provider")
}

impl OidcClient {
    pub fn new(settings: Oidc) -> Outcome<Self> {
        let http = reqwest::Client::builder()
            .timeout(PROVIDER_TIMEOUT)
            .build()
            .map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("building the HTTP client: {e}")))?;
        Ok(Self {
            settings,
            http,
            discovery: RwLock::new(None),
            keys: RwLock::new(HashMap::new()),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str, what: &str) -> Outcome<T> {
        let response = caught(self.http.get(url).send())
            .await
            .map_err(|panic| provider_error(format!("fetching {what} panicked: {panic}")))?
            .map_err(|e| provider_error(format!("fetching {what}: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            return Err(provider_error(format!("{what} answered {status}")));
        }
        caught(response.json::<T>())
            .await
            .map_err(|panic| provider_error(format!("reading {what} panicked: {panic}")))?
            .map_err(|e| provider_error(format!("reading {what}: {e}")))
    }

    async fn discovery(&self) -> Outcome<Discovery> {
        if let Some(found) = self.discovery.read().await.clone() {
            return Ok(found);
        }
        let mut slot = self.discovery.write().await;
        if let Some(found) = slot.clone() {
            return Ok(found);
        }
        let discovery: Discovery = self.get_json(&self.settings.well_known_url, "the discovery document").await?;
        *slot = Some(discovery.clone());
        Ok(discovery)
    }

    async fn refresh_keys(&self, jwks_uri: &str) -> Outcome<()> {
        let set: JwkSet = self.get_json(jwks_uri, "the JWKS").await?;
        let mut keys = HashMap::new();
        for jwk in &set.keys {
            if let (Some(kid), Ok(key)) = (jwk.common.key_id.clone(), DecodingKey::from_jwk(jwk)) {
                keys.insert(kid, key);
            }
        }
        *self.keys.write().await = keys;
        Ok(())
    }

    /// The key `kid` names, from the cache, else after one fetch of the JWKS.
    async fn key(&self, kid: &str, jwks_uri: &str) -> Outcome<Option<DecodingKey>> {
        if let Some(key) = self.keys.read().await.get(kid).cloned() {
            return Ok(Some(key));
        }
        self.refresh_keys(jwks_uri).await?;
        Ok(self.keys.read().await.get(kid).cloned())
    }

    /// Where to send the browser, with the handshake's values.
    pub async fn authorization_url(&self, redirect_uri: &str, handshake: &OidcHandshake, provider: Option<&str>) -> Outcome<String> {
        let discovery = self.discovery().await?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(handshake.verifier.as_bytes()));
        let mut url = url::Url::parse(&discovery.authorization_endpoint).map_err(|e| provider_error(format!("authorization_endpoint: {e}")))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", &self.settings.client_id)
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("scope", "openid email profile")
                .append_pair("state", &handshake.state)
                .append_pair("nonce", &handshake.nonce)
                .append_pair("code_challenge", &challenge)
                .append_pair("code_challenge_method", "S256");
            if let Some(provider) = provider.filter(|p| PROVIDERS.contains(p)) {
                query.append_pair("idp_hint", provider);
            }
        }
        Ok(url.to_string())
    }

    /// Exchanges the code and verifies the id_token: RS256 against the JWKS, issuer, audience, expiry, nonce.
    pub async fn complete(&self, code: &str, handshake: &OidcHandshake, redirect_uri: &str) -> Outcome<Claims> {
        let discovery = self.discovery().await?;
        #[derive(Deserialize)]
        struct Tokens {
            id_token: String,
        }
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", self.settings.client_id.as_str()),
            ("client_secret", self.settings.client_secret.expose()),
            ("code_verifier", handshake.verifier.as_str()),
        ];
        let body = serde_urlencoded::to_string(form).map_err(|e| provider_error(format!("encoding the token request: {e}")))?;
        let request = self
            .http
            .post(&discovery.token_endpoint)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body);
        let response = caught(request.send())
            .await
            .map_err(|panic| provider_error(format!("the token endpoint panicked: {panic}")))?
            .map_err(|e| provider_error(format!("the token endpoint: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            return Err(provider_error(format!("the token endpoint answered {status}")));
        }
        let tokens: Tokens = caught(response.json())
            .await
            .map_err(|panic| provider_error(format!("reading the tokens panicked: {panic}")))?
            .map_err(|e| provider_error(format!("reading the tokens: {e}")))?;

        let header = jsonwebtoken::decode_header(&tokens.id_token).map_err(|e| provider_error(format!("the id_token header: {e}")))?;
        let kid = header.kid.ok_or_else(|| provider_error("the id_token names no key"))?;
        let jwks_uri = self.settings.jwks_url.clone().unwrap_or(discovery.jwks_uri.clone());
        let key = self
            .key(&kid, &jwks_uri)
            .await?
            .ok_or_else(|| provider_error("the id_token's key is not in the JWKS"))?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[discovery.issuer.as_str(), self.settings.issuer.as_str()]);
        validation.set_audience(&[self.settings.client_id.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let claims = jsonwebtoken::decode::<Claims>(&tokens.id_token, &key, &validation)
            .map_err(|e| provider_error(format!("the id_token failed verification: {e}")))?
            .claims;
        if claims.nonce.as_deref() != Some(handshake.nonce.as_str()) {
            return Err(provider_error("the id_token's nonce does not match the handshake"));
        }
        Ok(claims)
    }

    pub fn issuer(&self) -> &str {
        &self.settings.issuer
    }
}

// ---- handlers --------------------------------------------------------------------------------------------

fn found(location: &str, cookies: &[String]) -> Response {
    let mut response = StatusCode::FOUND.into_response();
    if let Ok(value) = HeaderValue::from_str(location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    for cookie in cookies {
        if let Ok(value) = HeaderValue::from_str(cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

/// `auth.public-url` + `/auth/callback`, else the request's scheme and host.
fn redirect_uri(public_url: Option<&str>, headers: &HeaderMap) -> String {
    match public_url {
        Some(base) => format!("{base}/auth/callback"),
        None => {
            let scheme = if is_secure(headers) { "https" } else { "http" };
            let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("localhost");
            format!("{scheme}://{host}/auth/callback")
        }
    }
}

fn member_cookie<F>(state: &AppState<F>, user: &User, secure: bool) -> String {
    let days = i64::try_from(state.web.member_days).unwrap_or(30);
    let session = MemberSession {
        user: user.id,
        csrf: random_token(24),
        expires: Timestamp::now()
            .checked_add(SignedDuration::from_hours(days.saturating_mul(24)))
            .unwrap_or(Timestamp::MAX),
    };
    set_cookie(
        Kind::Member,
        &state.cookies.encode_member(&session),
        state.web.member_days.saturating_mul(86_400),
        secure,
    )
}

pub async fn login<F: Served>(State(state): State<AppState<F>>, headers: HeaderMap, Query(query): Query<HashMap<String, String>>) -> Result<Response, Failure> {
    let Some(oidc) = state.oidc.as_ref() else {
        return Err(errorinfo!(Code::NOT_CONFIGURED, "sign-in is not configured").into());
    };
    let secure = is_secure(&headers);
    let minutes = i64::try_from(state.web.oidc_minutes).unwrap_or(15);
    let handshake = OidcHandshake {
        state: random_token(32),
        verifier: random_token(32),
        nonce: random_token(16),
        expires: Timestamp::now().checked_add(SignedDuration::from_mins(minutes)).unwrap_or(Timestamp::MAX),
    };
    let url = oidc
        .authorization_url(
            &redirect_uri(state.web.public_url.as_deref(), &headers),
            &handshake,
            query.get("provider").map(String::as_str),
        )
        .await?;
    let cookie = set_cookie(
        Kind::Oidc,
        &state.cookies.encode_oidc(&handshake),
        state.web.oidc_minutes.saturating_mul(60),
        secure,
    );
    Ok(found(&url, &[cookie]))
}

pub async fn callback<F: Served>(State(state): State<AppState<F>>, headers: HeaderMap, Query(query): Query<HashMap<String, String>>) -> Response {
    let secure = is_secure(&headers);
    let fail = |why: &str, code: &str| {
        tracing::warn!("sign-in failed: {why}");
        found(&format!("/?login_error={code}"), &[clear_cookie(Kind::Oidc, secure)])
    };
    let Some(oidc) = state.oidc.as_ref() else {
        return fail("not configured", "1");
    };
    let (Some(code), Some(state_param)) = (query.get("code"), query.get("state")) else {
        return fail("missing code or state", "1");
    };
    let Some(handshake) = cookie_value(&headers, Kind::Oidc.name(secure)).and_then(|v| state.cookies.decode_oidc(&v, Timestamp::now())) else {
        return fail("no valid handshake cookie", "1");
    };
    if handshake.state != *state_param {
        return fail("state mismatch", "1");
    }
    let claims = match oidc.complete(code, &handshake, &redirect_uri(state.web.public_url.as_deref(), &headers)).await {
        Ok(claims) => claims,
        Err(error) => return fail(&error.message, "1"),
    };
    let identity = Identity {
        issuer: oidc.issuer().to_owned(),
        subject: claims.sub,
    };
    match state.facade.sign_in(&identity, claims.email.as_deref(), claims.name.as_deref()).await {
        Ok(user) => found("/", &[member_cookie(&state, &user, secure), clear_cookie(Kind::Oidc, secure)]),
        Err(error) if error.code == Code::DISABLED => fail("account disabled", "disabled"),
        Err(error) => fail(&error.message, "1"),
    }
}

pub async fn dev_login<F: Served>(State(state): State<AppState<F>>, headers: HeaderMap) -> Result<Response, Failure> {
    if !state.web.dev_login {
        return Err(errorinfo!(Code::NOT_FOUND, "the development sign-in is off").into());
    }
    let user = state.facade.sign_in(&Identity::dev(), Some("dev@localhost"), Some("Dev User")).await?;
    let mut response = Json(json!({ "ok": true })).into_response();
    if let Ok(value) = HeaderValue::from_str(&member_cookie(&state, &user, is_secure(&headers))) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    Ok(response)
}

pub async fn logout<F: Served>(MemberWrite(member): MemberWrite) -> Response {
    let mut response = Json(json!({ "ok": true })).into_response();
    if let Ok(value) = HeaderValue::from_str(&clear_cookie(Kind::Member, member.secure)) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

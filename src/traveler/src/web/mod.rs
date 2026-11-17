//! The HTTP surface over axum 0.8 (design section 3): the router, the headers every response carries, the
//! 404/405 fallbacks and the static files. Each handler validates its request into domain values, calls one facade
//! method and answers the Node version's JSON.

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

pub mod admin;
pub mod api;
pub mod assets;
pub mod auth;
pub mod cookies;
pub mod errors;
pub mod extract;
pub mod keys;

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{HeaderValue, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use jiff::Timestamp;
use serde_json::json;
use tower_http::trace::TraceLayer;

use crate::facade::AppFacade;
use auth::OidcClient;
use cookies::CookieCodec;
use errors::Failure;

/// Request bodies: 16 KiB, as `express.json({ limit: '16kb' })`.
pub const BODY_LIMIT: usize = 16 * 1024;

/// The CSP `server.js` set, verbatim.
pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

/// What every handler reaches.
pub struct AppState<F> {
    pub facade: Arc<F>,
    pub cookies: Arc<CookieCodec>,
    /// The identity provider, when configured.
    pub oidc: Option<Arc<OidcClient>>,
    pub web: Arc<WebSettings>,
}

impl<F> Clone for AppState<F> {
    fn clone(&self) -> Self {
        Self {
            facade: self.facade.clone(),
            cookies: self.cookies.clone(),
            oidc: self.oidc.clone(),
            web: self.web.clone(),
        }
    }
}

/// The settings the web layer reads.
#[derive(Debug, Clone)]
pub struct WebSettings {
    pub dev_login: bool,
    /// `auth.public-url`, without a trailing slash.
    pub public_url: Option<String>,
    pub member_days: u64,
    pub admin_hours: u64,
    pub oidc_minutes: u64,
}

/// The bound every handler needs, named once.
pub trait Served: AppFacade + Send + Sync + 'static {}

impl<F: AppFacade + Send + Sync + 'static> Served for F {}

pub fn router<F: Served>(state: AppState<F>) -> Router {
    let api = Router::new()
        .route("/config", get(api::config::<F>))
        .route("/me", get(api::me::<F>))
        .route("/state", get(api::state::<F>))
        .route("/sessions", get(api::sessions::<F>))
        .route("/sessions/start", post(api::start::<F>))
        .route("/sessions/stop", post(api::stop::<F>))
        .route("/sessions/{id}", patch(api::relabel::<F>).delete(api::delete_session::<F>))
        .route("/activities", post(api::create_activity::<F>))
        .route("/activities/{key}", delete(api::delete_activity::<F>))
        .route("/account", delete(api::delete_account::<F>));
    let auth = Router::new()
        .route("/login", get(auth::login::<F>))
        .route("/callback", get(auth::callback::<F>))
        .route("/dev-login", post(auth::dev_login::<F>))
        .route("/logout", post(auth::logout::<F>));
    let admin = Router::new()
        .route("/status", get(admin::status::<F>))
        .route("/setup", post(admin::setup::<F>))
        .route("/login", post(admin::login::<F>))
        .route("/logout", post(admin::logout::<F>))
        .route("/users", get(admin::users::<F>))
        .route("/users/{id}/disable", post(admin::disable::<F>))
        .route("/users/{id}/enable", post(admin::enable::<F>))
        .route("/users/{id}", delete(admin::delete_user::<F>));

    Router::new()
        .route("/healthz", get(healthz::<F>))
        .nest("/api", api)
        .nest("/auth", auth)
        .nest("/admin/api", admin)
        .fallback(assets::serve)
        .method_not_allowed_fallback(wrong_method)
        .with_state(state)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
}

/// The three headers `server.js` set on every response.
async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    response
}

async fn healthz<F: Served>(axum::extract::State(state): axum::extract::State<AppState<F>>) -> Response {
    use axum::response::IntoResponse;
    match state.facade.ping().await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(error) => {
            tracing::error!(code = %error.code, "healthz: {}", error.message);
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "ok": false }))).into_response()
        }
    }
}

async fn wrong_method() -> Failure {
    rzlib_error::errorinfo!(crate::errors::Code::METHOD_NOT_ALLOWED, "this path does not serve this method").into()
}

/// A timestamp as `Date.prototype.toISOString` wrote it: UTC, three fractional digits, `Z`.
pub fn iso(at: Timestamp) -> String {
    let millis = at.subsec_millisecond().rem_euclid(1000);
    format!("{}.{millis:03}Z", at.strftime("%Y-%m-%dT%H:%M:%S"))
}

pub fn now_iso() -> String {
    iso(Timestamp::now())
}

#[cfg(test)]
pub(crate) mod tests;

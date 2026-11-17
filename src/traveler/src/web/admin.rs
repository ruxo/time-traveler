//! The admin routes under `/admin/api`, answering `src/admin.js`'s JSON.

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

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use jiff::{SignedDuration, Timestamp};
use rzlib_error::errorinfo;
use serde::Deserialize;
use serde_json::{Value, json};

use super::cookies::{Kind, clear_cookie, cookie_value, is_secure, set_cookie};
use super::errors::Failure;
use super::extract::{AdminAuth, AdminWrite, Body, PathUuid};
use super::{AppState, Served, iso};
use crate::errors::Code;
use crate::facade::{Login, UserSummary};
use crate::model::{Admin, AdminSession, normalize_username, password_acceptable};

pub fn user_view(summary: &UserSummary) -> Value {
    let user = &summary.user;
    json!({
        "id": user.id.to_string(),
        "name": user.name,
        "email": user.email,
        "createdAt": iso(user.created),
        "disabled": user.disabled.is_some(),
        "disabledAt": user.disabled.map(iso),
        "lastActive": summary.last_active.map(iso),
        "sessionCount": summary.session_count,
    })
}

fn admin_cookie<F>(state: &AppState<F>, admin: &Admin, secure: bool) -> String {
    let hours = i64::try_from(state.web.admin_hours).unwrap_or(12);
    let session = AdminSession {
        admin: admin.id,
        csrf: crate::facade::random_token(24),
        expires: Timestamp::now().checked_add(SignedDuration::from_hours(hours)).unwrap_or(Timestamp::MAX),
    };
    set_cookie(
        Kind::Admin,
        &state.cookies.encode_admin(&session),
        state.web.admin_hours.saturating_mul(3600),
        secure,
    )
}

fn with_cookie(mut response: Response, cookie: &str) -> Response {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// `status` answers for an anonymous admin page too, so the cookie is read by hand rather than required.
pub async fn status<F: Served>(State(state): State<AppState<F>>, headers: HeaderMap) -> Result<Json<Value>, Failure> {
    let status = state.facade.admin_status().await?;
    let secure = is_secure(&headers);
    let session = cookie_value(&headers, Kind::Admin.name(secure)).and_then(|v| state.cookies.decode_admin(&v, Timestamp::now()));
    let signed_in = match session {
        Some(session) => state.facade.admin(session.admin).await.ok().map(|admin| (admin, session.csrf)),
        None => None,
    };
    Ok(Json(json!({
        "needsSetup": status.needs_setup,
        "signedIn": signed_in.is_some(),
        "username": signed_in.as_ref().map(|(admin, _)| admin.username.clone()),
        "csrf": signed_in.as_ref().map(|(_, csrf)| csrf.clone()),
    })))
}

#[derive(Deserialize)]
pub struct Credentials {
    #[serde(default)]
    token: Option<Value>,
    #[serde(default)]
    username: Option<Value>,
    #[serde(default)]
    password: Option<Value>,
}

impl Credentials {
    /// Node's `credentials()`: a non-string is the empty string.
    fn parts(&self) -> (String, String, String) {
        let text = |v: &Option<Value>| v.as_ref().and_then(Value::as_str).unwrap_or("").to_owned();
        (text(&self.token), text(&self.username), text(&self.password))
    }
}

pub async fn setup<F: Served>(State(state): State<AppState<F>>, headers: HeaderMap, Body(body): Body<Credentials>) -> Result<Response, Failure> {
    let (token, username, password) = body.parts();
    let username = normalize_username(&username).ok_or_else(|| Failure(errorinfo!(Code::BAD_USERNAME, "username is [a-z0-9._-]{3,32}")))?;
    if !password_acceptable(&password) {
        return Err(errorinfo!(Code::BAD_PASSWORD, "password is 10..=200 characters").into());
    }
    let admin = state.facade.setup(&token, username, &password).await?;
    let response = (StatusCode::CREATED, Json(json!({ "ok": true }))).into_response();
    Ok(with_cookie(response, &admin_cookie(&state, &admin, is_secure(&headers))))
}

pub async fn login<F: Served>(State(state): State<AppState<F>>, headers: HeaderMap, Body(body): Body<Credentials>) -> Result<Response, Failure> {
    let (_, username, password) = body.parts();
    let username = username.trim().to_lowercase();
    match state.facade.login(username, &password).await? {
        Login::SignedIn(admin) => Ok(with_cookie(
            Json(json!({ "ok": true })).into_response(),
            &admin_cookie(&state, &admin, is_secure(&headers)),
        )),
        Login::Locked { retry_after_seconds } => Ok((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": Code::LOCKED, "retryAfterSeconds": retry_after_seconds })),
        )
            .into_response()),
    }
}

pub async fn logout<F: Served>(AdminWrite(admin): AdminWrite) -> Response {
    with_cookie(Json(json!({ "ok": true })).into_response(), &clear_cookie(Kind::Admin, admin.secure))
}

pub async fn users<F: Served>(State(state): State<AppState<F>>, _admin: AdminAuth) -> Result<Json<Value>, Failure> {
    let users = state.facade.users().await?;
    Ok(Json(json!({ "users": users.iter().map(user_view).collect::<Vec<_>>() })))
}

pub async fn disable<F: Served>(State(state): State<AppState<F>>, _admin: AdminWrite, PathUuid(id): PathUuid) -> Result<Json<Value>, Failure> {
    let user = state.facade.disable_user(id).await?;
    Ok(Json(json!({ "user": user_view(&user) })))
}

pub async fn enable<F: Served>(State(state): State<AppState<F>>, _admin: AdminWrite, PathUuid(id): PathUuid) -> Result<Json<Value>, Failure> {
    let user = state.facade.enable_user(id).await?;
    Ok(Json(json!({ "user": user_view(&user) })))
}

pub async fn delete_user<F: Served>(State(state): State<AppState<F>>, _admin: AdminWrite, PathUuid(id): PathUuid) -> Result<Json<Value>, Failure> {
    state.facade.delete_user(id).await?;
    Ok(Json(json!({ "ok": true })))
}

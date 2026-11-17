//! The member routes under `/api`, answering `src/api.js`'s JSON.

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

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use jiff::{SignedDuration, Timestamp};
use rzlib_error::errorinfo;
use serde::Deserialize;
use serde_json::{Value, json};

use super::cookies::{Kind, clear_cookie};
use super::errors::Failure;
use super::extract::{Body, Member, MemberWrite, PathUuid};
use super::keys::{activity_from_key, custom_from_key, key_of};
use super::{AppState, Served, iso, now_iso};
use crate::errors::Code;
use crate::model::{OwnedActivity, Running, Session, TimeSession, normalize_name, target_seconds_from_minutes};

/// The widest `GET /api/sessions` range.
const MAX_RANGE: SignedDuration = SignedDuration::from_hours(72);

pub fn running_view(running: &Running) -> Value {
    json!({
        "id": running.id.to_string(),
        "activity": key_of(running.activity),
        "startedAt": iso(running.started_at),
        "endedAt": Value::Null,
        "targetSeconds": running.target_seconds,
    })
}

pub fn ended_view(session: &TimeSession) -> Value {
    json!({
        "id": session.id.to_string(),
        "activity": key_of(session.activity),
        "startedAt": iso(session.started_at),
        "endedAt": iso(session.ended_at),
        "targetSeconds": session.target_seconds,
    })
}

pub fn session_view(session: &Session) -> Value {
    match session {
        Session::Running(running) => running_view(running),
        Session::Ended(ended) => ended_view(ended),
    }
}

pub fn activity_view(owned: &OwnedActivity) -> Value {
    json!({
        "key": key_of(owned.activity.id),
        "name": owned.activity.name,
        "color": owned.activity.color,
        "icon": owned.activity.icon,
        "deleted": owned.ownership.deleted.is_some(),
    })
}

pub async fn config<F: Served>(State(state): State<AppState<F>>) -> Json<Value> {
    let providers: Vec<&str> = if state.oidc.is_some() { vec!["google", "line", "email"] } else { vec![] };
    Json(json!({ "providers": providers, "devLogin": state.web.dev_login }))
}

pub async fn me<F: Served>(member: Member) -> Json<Value> {
    Json(json!({ "name": member.user.name, "email": member.user.email, "csrf": member.csrf }))
}

pub async fn state<F: Served>(State(state): State<AppState<F>>, member: Member) -> Result<Json<Value>, Failure> {
    let current = state.facade.state(member.user.id).await?;
    Ok(Json(json!({
        "serverNow": now_iso(),
        "custom": current.custom.iter().map(activity_view).collect::<Vec<_>>(),
        "current": current.current.as_ref().map(running_view),
    })))
}

pub async fn sessions<F: Served>(
    State(state): State<AppState<F>>,
    member: Member,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, Failure> {
    let parse = |name: &str| query.get(name).and_then(|v| v.parse::<Timestamp>().ok());
    let (from, to) = match (parse("from"), parse("to")) {
        (Some(from), Some(to)) if to > from && to.duration_since(from) <= MAX_RANGE => (from, to),
        _ => return Err(errorinfo!(Code::BAD_RANGE, "from and to are timestamps, from < to, at most 3 days apart").into()),
    };
    let sessions = state.facade.sessions(member.user.id, from, to).await?;
    Ok(Json(
        json!({ "serverNow": now_iso(), "sessions": sessions.iter().map(session_view).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartBody {
    #[serde(default)]
    activity: Option<Value>,
    #[serde(default)]
    target_minutes: Option<Value>,
}

/// Node's `targetMinutes` rule: absent, `null` or `0` → none; a JSON integer in `1..=1440` → seconds; else `bad_target`.
fn target_from(value: Option<&Value>) -> Result<Option<i32>, Failure> {
    let minutes = match value {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) if n.is_i64() => n.as_i64(),
        Some(Value::Number(n)) if n.as_f64().is_some_and(|f| f.fract() == 0.0 && f.abs() < 1e15) => n.as_f64().map(|f| f as i64),
        Some(_) => return Err(errorinfo!(Code::BAD_TARGET, "targetMinutes is not an integer").into()),
    };
    Ok(target_seconds_from_minutes(minutes)?)
}

fn activity_from(value: Option<&Value>) -> Result<uuid::Uuid, Failure> {
    value
        .and_then(Value::as_str)
        .and_then(activity_from_key)
        .ok_or_else(|| errorinfo!(Code::BAD_ACTIVITY, "activity is a predefined name or c:<uuid>").into())
}

pub async fn start<F: Served>(State(state): State<AppState<F>>, MemberWrite(member): MemberWrite, Body(body): Body<StartBody>) -> Result<Response, Failure> {
    let target = target_from(body.target_minutes.as_ref())?;
    let activity = activity_from(body.activity.as_ref())?;
    let running = state.facade.start(member.user.id, activity, target).await?;
    Ok((StatusCode::CREATED, Json(json!({ "current": running_view(&running) }))).into_response())
}

pub async fn stop<F: Served>(State(state): State<AppState<F>>, MemberWrite(member): MemberWrite) -> Result<Json<Value>, Failure> {
    let stopped = state.facade.stop(member.user.id).await?;
    Ok(Json(json!({ "stopped": stopped.as_ref().map(ended_view) })))
}

#[derive(Deserialize)]
pub struct RelabelBody {
    #[serde(default)]
    activity: Option<Value>,
}

pub async fn relabel<F: Served>(
    State(state): State<AppState<F>>,
    MemberWrite(member): MemberWrite,
    PathUuid(id): PathUuid,
    Body(body): Body<RelabelBody>,
) -> Result<Json<Value>, Failure> {
    let activity = activity_from(body.activity.as_ref())?;
    let session = state.facade.relabel(member.user.id, id, activity).await?;
    Ok(Json(json!({ "session": session_view(&session) })))
}

pub async fn delete_session<F: Served>(
    State(state): State<AppState<F>>,
    MemberWrite(member): MemberWrite,
    PathUuid(id): PathUuid,
) -> Result<Json<Value>, Failure> {
    state.facade.delete_session(member.user.id, id).await?;
    Ok(Json(json!({ "deleted": id.to_string() })))
}

#[derive(Deserialize)]
pub struct ActivityBody {
    #[serde(default)]
    name: Option<Value>,
    #[serde(default)]
    color: Option<Value>,
    #[serde(default)]
    icon: Option<Value>,
}

pub async fn create_activity<F: Served>(
    State(state): State<AppState<F>>,
    MemberWrite(member): MemberWrite,
    Body(body): Body<ActivityBody>,
) -> Result<Response, Failure> {
    let name = body
        .name
        .as_ref()
        .and_then(Value::as_str)
        .and_then(normalize_name)
        .ok_or_else(|| Failure(errorinfo!(Code::BAD_NAME, "name is 1..=40 characters")))?;
    let color = body.color.as_ref().and_then(Value::as_str).map(str::to_owned);
    let icon = body.icon.as_ref().and_then(Value::as_str).map(str::to_owned);
    let created = state.facade.create_activity(member.user.id, name, color, icon).await?;
    Ok((StatusCode::CREATED, Json(json!({ "activity": activity_view(&created) }))).into_response())
}

pub async fn delete_activity<F: Served>(
    State(state): State<AppState<F>>,
    MemberWrite(member): MemberWrite,
    axum::extract::Path(key): axum::extract::Path<String>,
) -> Result<Json<Value>, Failure> {
    let id = custom_from_key(&key).ok_or_else(|| Failure(errorinfo!(Code::NOT_FOUND, "not a custom activity key")))?;
    state.facade.delete_activity(member.user.id, id).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_account<F: Served>(State(state): State<AppState<F>>, MemberWrite(member): MemberWrite) -> Result<Response, Failure> {
    state.facade.delete_account(member.user.id).await?;
    let mut response = Json(json!({ "ok": true })).into_response();
    if let Ok(value) = HeaderValue::from_str(&clear_cookie(Kind::Member, member.secure)) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    Ok(response)
}

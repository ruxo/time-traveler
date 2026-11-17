//! Extractors whose rejection is the error envelope: the signed-in member or admin (with the CSRF check for a
//! write), a JSON body, a UUID path segment.

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
use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use jiff::Timestamp;
use rzlib_error::errorinfo;
use serde::de::DeserializeOwned;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::cookies::{Kind, cookie_value, is_secure};
use super::errors::Failure;
use super::keys::parse_uuid;
use super::{AppState, Served};
use crate::errors::Code;
use crate::model::{Admin, User};

/// The member the request's cookie names, loaded and not disabled.
pub struct Member {
    pub user: User,
    pub csrf: String,
    pub secure: bool,
}

impl<F: Served> FromRequestParts<AppState<F>> for Member {
    type Rejection = Failure;

    async fn from_request_parts(parts: &mut Parts, state: &AppState<F>) -> Result<Self, Failure> {
        let secure = is_secure(&parts.headers);
        let value = cookie_value(&parts.headers, Kind::Member.name(secure)).ok_or_else(|| Failure(errorinfo!(Code::NOT_SIGNED_IN, "no session cookie")))?;
        let session = state
            .cookies
            .decode_member(&value, Timestamp::now())
            .ok_or_else(|| Failure(errorinfo!(Code::NOT_SIGNED_IN, "the session cookie does not verify or has expired")))?;
        let user = state.facade.member(session.user).await?;
        Ok(Self {
            user,
            csrf: session.csrf,
            secure,
        })
    }
}

/// A member on a state-changing request: the cookie, and `X-CSRF-Token` equal to its `csrf`.
pub struct MemberWrite(pub Member);

impl<F: Served> FromRequestParts<AppState<F>> for MemberWrite {
    type Rejection = Failure;

    async fn from_request_parts(parts: &mut Parts, state: &AppState<F>) -> Result<Self, Failure> {
        let member = Member::from_request_parts(parts, state).await?;
        check_csrf(&parts.headers, &member.csrf)?;
        Ok(Self(member))
    }
}

/// The admin the request's cookie names.
pub struct AdminAuth {
    pub admin: Admin,
    pub csrf: String,
    pub secure: bool,
}

impl<F: Served> FromRequestParts<AppState<F>> for AdminAuth {
    type Rejection = Failure;

    async fn from_request_parts(parts: &mut Parts, state: &AppState<F>) -> Result<Self, Failure> {
        let secure = is_secure(&parts.headers);
        let value = cookie_value(&parts.headers, Kind::Admin.name(secure)).ok_or_else(|| Failure(errorinfo!(Code::NOT_SIGNED_IN, "no admin cookie")))?;
        let session = state
            .cookies
            .decode_admin(&value, Timestamp::now())
            .ok_or_else(|| Failure(errorinfo!(Code::NOT_SIGNED_IN, "the admin cookie does not verify or has expired")))?;
        let admin = state.facade.admin(session.admin).await?;
        Ok(Self {
            admin,
            csrf: session.csrf,
            secure,
        })
    }
}

/// An admin on a state-changing request.
pub struct AdminWrite(pub AdminAuth);

impl<F: Served> FromRequestParts<AppState<F>> for AdminWrite {
    type Rejection = Failure;

    async fn from_request_parts(parts: &mut Parts, state: &AppState<F>) -> Result<Self, Failure> {
        let admin = AdminAuth::from_request_parts(parts, state).await?;
        check_csrf(&parts.headers, &admin.csrf)?;
        Ok(Self(admin))
    }
}

/// `X-CSRF-Token` equals `expected`, compared in constant time.
pub fn check_csrf(headers: &HeaderMap, expected: &str) -> Result<(), Failure> {
    let sent = headers.get("x-csrf-token").and_then(|v| v.to_str().ok()).unwrap_or("");
    if !expected.is_empty() && sent.len() == expected.len() && bool::from(sent.as_bytes().ct_eq(expected.as_bytes())) {
        Ok(())
    } else {
        Err(Failure(errorinfo!(Code::BAD_CSRF, "X-CSRF-Token does not match the session")))
    }
}

/// A JSON body of the route's shape; anything else is `bad_body`.
pub struct Body<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = Failure;

    async fn from_request(request: Request, state: &S) -> Result<Self, Failure> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Body(value))
            .map_err(|rejection| Failure(errorinfo!(Code::BAD_BODY, format!("the body: {}", rejection.body_text()))))
    }
}

/// A `{id}` path segment in Node's UUID form; anything else is `not_found`.
pub struct PathUuid(pub Uuid);

impl<S: Send + Sync> FromRequestParts<S> for PathUuid {
    type Rejection = Failure;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Failure> {
        let Path(text) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| Failure(errorinfo!(Code::NOT_FOUND, "the path has no id")))?;
        parse_uuid(&text)
            .map(PathUuid)
            .ok_or_else(|| Failure(errorinfo!(Code::NOT_FOUND, "the id is not a UUID")))
    }
}

/// Whether the request is https (section 3.3).
pub struct Secure(pub bool);

impl<S: Send + Sync> FromRequestParts<S> for Secure {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(is_secure(&parts.headers)))
    }
}

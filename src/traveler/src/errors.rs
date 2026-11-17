//! The error model: every refusal inside the service is an `rzlib_error::ErrorInfo` whose `code` is the wire code
//! of design section 3.2, and [`status_of`] is the one map from code to HTTP status. The web layer logs the
//! location trail and answers `{"error": <code>}`; the trail never reaches the wire.
//!
//! Two inner codes never reach the wire as themselves: [`Code::DATABASE`] and [`Code::CORRUPT_DOCUMENT`] are logged
//! and answered as `server_error`, as Node answered a failed query.

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

use http::StatusCode;
use rzlib_error::{ErrorInfo, Outcome, errorinfo};

/// The wire codes, spelt as `src/api.js`, `src/auth.js` and `src/admin.js` spelt them, plus the three the design adds.
pub struct Code;

impl Code {
    pub const NOT_SIGNED_IN: &'static str = "not_signed_in";
    pub const BAD_CSRF: &'static str = "bad_csrf";
    pub const BAD_RANGE: &'static str = "bad_range";
    pub const BAD_TARGET: &'static str = "bad_target";
    pub const BAD_ACTIVITY: &'static str = "bad_activity";
    pub const BAD_NAME: &'static str = "bad_name";
    pub const BAD_USERNAME: &'static str = "bad_username";
    pub const BAD_PASSWORD: &'static str = "bad_password";
    /// A body that is not JSON or not the route's shape. New: Node answered its 500 here.
    pub const BAD_BODY: &'static str = "bad_body";
    pub const NOT_FOUND: &'static str = "not_found";
    /// Sign-in asked while `auth` is not configured. New: Node answered a text 404.
    pub const NOT_CONFIGURED: &'static str = "not_configured";
    /// A known path with another method. New: Node answered Express's HTML page.
    pub const METHOD_NOT_ALLOWED: &'static str = "method_not_allowed";
    pub const RUNNING: &'static str = "running";
    pub const TOO_MANY: &'static str = "too_many";
    pub const ALREADY_SET_UP: &'static str = "already_set_up";
    pub const BAD_TOKEN: &'static str = "bad_token";
    pub const DISABLED: &'static str = "disabled";
    pub const BAD_CREDENTIALS: &'static str = "bad_credentials";
    pub const LOCKED: &'static str = "locked";
    pub const SERVER_ERROR: &'static str = "server_error";

    /// A driver error or a timed-out Mongo call. Logged; the wire sees `server_error`.
    pub const DATABASE: &'static str = "database";
    /// A stored document that converts to no valid domain value (article 10). Logged; the wire sees `server_error`.
    pub const CORRUPT_DOCUMENT: &'static str = "corrupt_document";
}

/// The status map, explicit: anything not listed is 500.
pub fn status_of(code: &str) -> StatusCode {
    match code {
        Code::BAD_RANGE | Code::BAD_TARGET | Code::BAD_ACTIVITY | Code::BAD_NAME | Code::BAD_USERNAME | Code::BAD_PASSWORD | Code::BAD_BODY => {
            StatusCode::BAD_REQUEST
        }
        Code::NOT_SIGNED_IN | Code::BAD_CREDENTIALS => StatusCode::UNAUTHORIZED,
        Code::BAD_CSRF | Code::BAD_TOKEN | Code::DISABLED => StatusCode::FORBIDDEN,
        Code::NOT_FOUND | Code::NOT_CONFIGURED => StatusCode::NOT_FOUND,
        Code::METHOD_NOT_ALLOWED => StatusCode::METHOD_NOT_ALLOWED,
        Code::RUNNING | Code::TOO_MANY | Code::ALREADY_SET_UP => StatusCode::CONFLICT,
        Code::LOCKED => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// The code the wire carries for `code`: the inner codes and anything unmapped become `server_error`.
pub fn wire_code(code: &str) -> &str {
    if status_of(code) == StatusCode::INTERNAL_SERVER_ERROR {
        Code::SERVER_ERROR
    } else {
        code
    }
}

/// A refusal with `code` and no further detail than its message.
pub fn refuse<T>(code: &'static str, message: impl Into<String>) -> Outcome<T> {
    Err(errorinfo!(code, message))
}

/// An `ErrorInfo` with `code`; the trail starts at the caller's line when built through `errorinfo!` there, so this
/// helper is for the places where the message is all that matters.
pub fn error(code: &'static str, message: impl Into<String>) -> ErrorInfo {
    errorinfo!(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_of_maps_every_listed_code_and_nothing_else() {
        for code in [
            Code::BAD_RANGE,
            Code::BAD_TARGET,
            Code::BAD_ACTIVITY,
            Code::BAD_NAME,
            Code::BAD_USERNAME,
            Code::BAD_PASSWORD,
            Code::BAD_BODY,
        ] {
            assert_eq!(status_of(code), StatusCode::BAD_REQUEST, "{code}");
        }
        assert_eq!(status_of(Code::NOT_SIGNED_IN), StatusCode::UNAUTHORIZED);
        assert_eq!(status_of(Code::BAD_CREDENTIALS), StatusCode::UNAUTHORIZED);
        assert_eq!(status_of(Code::BAD_CSRF), StatusCode::FORBIDDEN);
        assert_eq!(status_of(Code::BAD_TOKEN), StatusCode::FORBIDDEN);
        assert_eq!(status_of(Code::DISABLED), StatusCode::FORBIDDEN);
        assert_eq!(status_of(Code::NOT_FOUND), StatusCode::NOT_FOUND);
        assert_eq!(status_of(Code::NOT_CONFIGURED), StatusCode::NOT_FOUND);
        assert_eq!(status_of(Code::METHOD_NOT_ALLOWED), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(status_of(Code::RUNNING), StatusCode::CONFLICT);
        assert_eq!(status_of(Code::TOO_MANY), StatusCode::CONFLICT);
        assert_eq!(status_of(Code::ALREADY_SET_UP), StatusCode::CONFLICT);
        assert_eq!(status_of(Code::LOCKED), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(status_of(Code::SERVER_ERROR), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(status_of(Code::DATABASE), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(status_of(Code::CORRUPT_DOCUMENT), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(status_of("anything-else"), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn inner_codes_reach_the_wire_as_server_error() {
        assert_eq!(wire_code(Code::DATABASE), Code::SERVER_ERROR);
        assert_eq!(wire_code(Code::CORRUPT_DOCUMENT), Code::SERVER_ERROR);
        assert_eq!(wire_code(Code::BAD_RANGE), Code::BAD_RANGE);
        assert_eq!(wire_code(Code::LOCKED), Code::LOCKED);
    }
}

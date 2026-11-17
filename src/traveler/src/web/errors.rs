//! An error on its way to the wire: `{"error": <code>}` under the mapped status, logged once with its trail.

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
use axum::response::{IntoResponse, Response};
use rzlib_error::ErrorInfo;
use serde_json::json;

use crate::errors::{status_of, wire_code};

pub struct Failure(pub ErrorInfo);

impl From<ErrorInfo> for Failure {
    fn from(info: ErrorInfo) -> Self {
        Self(info)
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let info = self.0;
        let status = status_of(&info.code);
        if status.is_server_error() {
            tracing::error!(code = %info.code, status = status.as_u16(), locations = ?info.locations, "request failed: {}", info.message);
        } else {
            tracing::info!(code = %info.code, status = status.as_u16(), "request refused: {}", info.message);
        }
        (status, Json(json!({ "error": wire_code(&info.code) }))).into_response()
    }
}

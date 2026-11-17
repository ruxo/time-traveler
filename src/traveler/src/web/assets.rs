//! `public/`, embedded (design section 3.4): `/` is `index.html`, `/admin/` is `admin/index.html`, `/admin` redirects
//! to `/admin/`; every file answers an `ETag`, `Cache-Control: public, max-age=0` and 304 on a match.

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

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use rzlib_error::errorinfo;

use super::errors::Failure;
use crate::errors::Code;

#[derive(RustEmbed)]
#[folder = "../../public/"]
struct Public;

/// The embedded path a request path names, or a redirect.
enum Target {
    File(String),
    Redirect(&'static str),
}

fn target(path: &str) -> Target {
    match path {
        "/" | "" => Target::File("index.html".to_owned()),
        "/admin" => Target::Redirect("/admin/"),
        "/admin/" => Target::File("admin/index.html".to_owned()),
        other => Target::File(other.trim_start_matches('/').to_owned()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn serve(method: Method, uri: Uri, headers: HeaderMap) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return Failure(errorinfo!(Code::NOT_FOUND, "no route has this path")).into_response();
    }
    let path = match target(uri.path()) {
        Target::Redirect(to) => {
            let mut response = StatusCode::MOVED_PERMANENTLY.into_response();
            response.headers_mut().insert(header::LOCATION, HeaderValue::from_static(to));
            return response;
        }
        Target::File(path) => path,
    };
    let Some(file) = Public::get(&path) else {
        return Failure(errorinfo!(Code::NOT_FOUND, format!("no file {path}"))).into_response();
    };
    let etag = format!("\"{}\"", hex(&file.metadata.sha256_hash()));
    let matched = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| value.split(',').any(|tag| tag.trim().trim_start_matches("W/") == etag));
    let mut response = if matched {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        let body = if method == Method::HEAD {
            Body::empty()
        } else {
            Body::from(file.data.into_owned())
        };
        let mut response = Response::new(body);
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
        response
    };
    if let Ok(value) = HeaderValue::from_str(&etag) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=0"));
    response
}

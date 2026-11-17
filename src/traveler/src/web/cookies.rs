//! The signed cookies (design section 4): `base64url(payload) "." base64url(HMAC-SHA256(key, payload))`, the payload
//! a kind letter and `|`-separated fields. Reading one: verify the tag in constant time, parse, check the kind and
//! `expires > now`; any failure makes the request anonymous.

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

use axum::http::header::{COOKIE, HeaderMap};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use jiff::Timestamp;
use sha2::Sha256;
use uuid::Uuid;

use crate::model::{AdminSession, MemberSession, OidcHandshake};

type HmacSha256 = Hmac<Sha256>;

/// The three cookies, named as Node named them; `__Host-` on https.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Member,
    Admin,
    Oidc,
}

impl Kind {
    pub fn name(self, secure: bool) -> &'static str {
        match (self, secure) {
            (Self::Member, true) => "__Host-tt_sid",
            (Self::Member, false) => "tt_sid",
            (Self::Admin, true) => "__Host-tt_admin",
            (Self::Admin, false) => "tt_admin",
            (Self::Oidc, true) => "__Host-tt_oidc",
            (Self::Oidc, false) => "tt_oidc",
        }
    }

    fn letter(self) -> &'static str {
        match self {
            Self::Member => "m",
            Self::Admin => "a",
            Self::Oidc => "o",
        }
    }
}

/// Signs and verifies with the one `session.key`.
pub struct CookieCodec {
    key: Vec<u8>,
}

impl CookieCodec {
    pub fn new(key: &[u8]) -> Self {
        Self { key: key.to_vec() }
    }

    fn sign(&self, payload: &str) -> String {
        // `new_from_slice` accepts a key of any length.
        let tag = HmacSha256::new_from_slice(&self.key).map(|mut mac| {
            mac.update(payload.as_bytes());
            mac.finalize().into_bytes()
        });
        match tag {
            Ok(tag) => format!("{}.{}", URL_SAFE_NO_PAD.encode(payload), URL_SAFE_NO_PAD.encode(tag)),
            Err(_) => String::new(),
        }
    }

    /// The payload of `value` when its tag verifies and it is of `kind`.
    fn open(&self, value: &str, kind: Kind) -> Option<Vec<String>> {
        let (payload, tag) = value.rsplit_once('.')?;
        let payload = URL_SAFE_NO_PAD.decode(payload).ok()?;
        let tag = URL_SAFE_NO_PAD.decode(tag).ok()?;
        let mut mac = HmacSha256::new_from_slice(&self.key).ok()?;
        mac.update(&payload);
        mac.verify_slice(&tag).ok()?;
        let payload = String::from_utf8(payload).ok()?;
        let mut fields = payload.split('|');
        if fields.next()? != kind.letter() {
            return None;
        }
        Some(fields.map(str::to_owned).collect())
    }

    pub fn encode_member(&self, session: &MemberSession) -> String {
        self.sign(&format!("m|{}|{}|{}", session.user, session.expires.as_second(), session.csrf))
    }

    pub fn decode_member(&self, value: &str, now: Timestamp) -> Option<MemberSession> {
        let fields = self.open(value, Kind::Member)?;
        let [user, expires, csrf] = <[String; 3]>::try_from(fields).ok()?;
        let expires = Timestamp::from_second(expires.parse().ok()?).ok()?;
        if expires <= now {
            return None;
        }
        Some(MemberSession {
            user: Uuid::parse_str(&user).ok()?,
            csrf,
            expires,
        })
    }

    pub fn encode_admin(&self, session: &AdminSession) -> String {
        self.sign(&format!("a|{}|{}|{}", session.admin, session.expires.as_second(), session.csrf))
    }

    pub fn decode_admin(&self, value: &str, now: Timestamp) -> Option<AdminSession> {
        let fields = self.open(value, Kind::Admin)?;
        let [admin, expires, csrf] = <[String; 3]>::try_from(fields).ok()?;
        let expires = Timestamp::from_second(expires.parse().ok()?).ok()?;
        if expires <= now {
            return None;
        }
        Some(AdminSession {
            admin: Uuid::parse_str(&admin).ok()?,
            csrf,
            expires,
        })
    }

    pub fn encode_oidc(&self, handshake: &OidcHandshake) -> String {
        self.sign(&format!(
            "o|{}|{}|{}|{}",
            handshake.state,
            handshake.verifier,
            handshake.nonce,
            handshake.expires.as_second()
        ))
    }

    pub fn decode_oidc(&self, value: &str, now: Timestamp) -> Option<OidcHandshake> {
        let fields = self.open(value, Kind::Oidc)?;
        let [state, verifier, nonce, expires] = <[String; 4]>::try_from(fields).ok()?;
        let expires = Timestamp::from_second(expires.parse().ok()?).ok()?;
        if expires <= now {
            return None;
        }
        Some(OidcHandshake {
            state,
            verifier,
            nonce,
            expires,
        })
    }
}

/// Whether the request came over https: Erawan terminates TLS and forwards the scheme.
pub fn is_secure(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').next().is_some_and(|first| first.trim().eq_ignore_ascii_case("https")))
}

/// The value of the cookie `name` in the request, if any.
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .find_map(|part| {
            let (k, v) = part.split_once('=')?;
            (k.trim() == name).then(|| v.trim().to_owned())
        })
}

/// A `Set-Cookie` value with Node's attributes: `HttpOnly; SameSite=Lax; Path=/`, `Secure` on https, `Max-Age`.
pub fn set_cookie(kind: Kind, value: &str, max_age_seconds: u64, secure: bool) -> String {
    let mut cookie = format!("{}={value}; Max-Age={max_age_seconds}; Path=/; HttpOnly; SameSite=Lax", kind.name(secure));
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// A `Set-Cookie` value that removes the cookie.
pub fn clear_cookie(kind: Kind, secure: bool) -> String {
    set_cookie(kind, "", 0, secure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::SignedDuration;

    fn codec() -> CookieCodec {
        CookieCodec::new(b"0123456789abcdef0123456789abcdef")
    }

    #[test]
    fn a_member_cookie_round_trips_and_is_refused_when_tampered_expired_or_rekinded() {
        let c = codec();
        let now = Timestamp::now();
        let session = MemberSession {
            user: Uuid::new_v4(),
            csrf: "abc".to_owned(),
            expires: now.checked_add(SignedDuration::from_secs(60)).unwrap(),
        };
        let value = c.encode_member(&session);
        let back = c.decode_member(&value, now).expect("verifies");
        assert_eq!(back.user, session.user);
        assert_eq!(back.csrf, session.csrf);
        assert_eq!(back.expires.as_second(), session.expires.as_second());

        let mut tampered = value.clone();
        tampered.replace_range(0..1, if value.starts_with('A') { "B" } else { "A" });
        assert!(c.decode_member(&tampered, now).is_none());
        assert!(c.decode_member(&value, session.expires).is_none(), "expired at exactly expires");
        assert!(c.decode_admin(&value, now).is_none(), "a member cookie is not an admin cookie");
        assert!(CookieCodec::new(b"another-key-another-key-another-key").decode_member(&value, now).is_none());
    }

    #[test]
    fn oidc_and_admin_cookies_round_trip() {
        let c = codec();
        let now = Timestamp::now();
        let expires = now.checked_add(SignedDuration::from_secs(900)).unwrap();
        let handshake = OidcHandshake {
            state: "s".to_owned(),
            verifier: "v".to_owned(),
            nonce: "n".to_owned(),
            expires,
        };
        assert_eq!(c.decode_oidc(&c.encode_oidc(&handshake), now).unwrap().verifier, "v");
        let admin = AdminSession {
            admin: Uuid::new_v4(),
            csrf: "x".to_owned(),
            expires,
        };
        assert_eq!(c.decode_admin(&c.encode_admin(&admin), now).unwrap().admin, admin.admin);
    }

    #[test]
    fn cookie_headers_follow_nodes_attributes() {
        assert_eq!(
            set_cookie(Kind::Member, "v", 10, true),
            "__Host-tt_sid=v; Max-Age=10; Path=/; HttpOnly; SameSite=Lax; Secure"
        );
        assert_eq!(set_cookie(Kind::Member, "v", 10, false), "tt_sid=v; Max-Age=10; Path=/; HttpOnly; SameSite=Lax");
        assert_eq!(clear_cookie(Kind::Oidc, false), "tt_oidc=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax");
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, "a=1; tt_sid=xyz; b=2".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        assert_eq!(cookie_value(&headers, "tt_sid").as_deref(), Some("xyz"));
        assert_eq!(cookie_value(&headers, "nope"), None);
        assert!(is_secure(&headers));
    }
}

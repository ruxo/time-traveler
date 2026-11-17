//! The domain model (design section 2.1) and the application state beside it (section 2.2): plain types with the
//! invariants in their constructors, no serde. Storage and the wire convert into these at their boundaries.

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

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use jiff::{SignedDuration, Timestamp};
use rzlib_error::{Outcome, errorinfo};
use scrypt::Params;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::activities;
use crate::errors::Code;

/// The longest activity name, in Unicode scalar values.
pub const MAX_NAME: usize = 40;
/// A target in seconds: one minute to one day.
pub const MIN_TARGET_SECONDS: i32 = 60;
pub const MAX_TARGET_SECONDS: i32 = 86_400;
/// A username: `^[a-z0-9._-]{3,32}$`.
pub const MIN_USERNAME: usize = 3;
pub const MAX_USERNAME: usize = 32;
/// A password, in UTF-16 code units as JavaScript counted them.
pub const MIN_PASSWORD: usize = 10;
pub const MAX_PASSWORD: usize = 200;
/// Admin login: the failures that lock a username, and for how long.
pub const MAX_LOGIN_FAILURES: u32 = 5;
pub const LOCK_SECONDS: i64 = 15 * 60;

// ---- domain ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub email: Option<String>,
    pub name: Option<String>,
    pub disabled: Option<Timestamp>,
    pub created: Timestamp,
    pub updated: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    /// The key of the drawing the client shows: one of `activities::ICONS` for a stored activity, the activity's
    /// own name for a predefined one.
    pub icon: String,
}

impl Activity {
    /// A stored activity: `name` already normalised (see [`normalize_name`]), 1..=40 scalar values, `color` in the
    /// palette, `icon` one of the icons.
    pub fn new(id: Uuid, name: String, color: String, icon: String) -> Outcome<Self> {
        let count = name.chars().count();
        if count == 0 || count > MAX_NAME || name != normalize_name(&name).unwrap_or_default() {
            return Err(errorinfo!(
                Code::BAD_NAME,
                format!("an activity name is 1..={MAX_NAME} characters, trimmed, single-spaced")
            ));
        }
        if !activities::in_palette(&color) {
            return Err(errorinfo!(Code::BAD_ACTIVITY, format!("{color:?} is not a palette colour")));
        }
        if !activities::is_icon(&icon) {
            return Err(errorinfo!(Code::BAD_ACTIVITY, format!("{icon:?} is not an icon")));
        }
        Ok(Self { id, name, color, icon })
    }

    /// A predefined activity: static data, its colour and its drawing the frontend's, outside the palette and
    /// icon rules. The client draws it by its name.
    pub fn predefined(id: Uuid, name: &str, color: &str) -> Self {
        Self {
            id,
            name: name.to_owned(),
            color: color.to_owned(),
            icon: name.to_owned(),
        }
    }
}

/// An ended session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSession {
    pub id: Uuid,
    pub activity: Uuid,
    pub started_at: Timestamp,
    pub ended_at: Timestamp,
    pub target_seconds: Option<i32>,
}

impl TimeSession {
    pub fn new(id: Uuid, activity: Uuid, started_at: Timestamp, ended_at: Timestamp, target_seconds: Option<i32>) -> Outcome<Self> {
        if ended_at < started_at {
            return Err(errorinfo!(
                Code::CORRUPT_DOCUMENT,
                format!("session {id} ends at {ended_at}, before it started at {started_at}")
            ));
        }
        check_target(target_seconds)?;
        Ok(Self {
            id,
            activity,
            started_at,
            ended_at,
            target_seconds,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admin {
    pub id: Uuid,
    pub username: String,
    pub password: PasswordHash,
    pub created: Timestamp,
    pub updated: Timestamp,
}

// ---- application state ----------------------------------------------------------------------------------

/// Who a user is to the identity provider; `("dev", "local")` for the development sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identity {
    pub issuer: String,
    pub subject: String,
}

impl Identity {
    pub fn dev() -> Self {
        Self {
            issuer: "dev".to_owned(),
            subject: "local".to_owned(),
        }
    }
}

/// The session running now; at most one per user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub id: Uuid,
    pub activity: Uuid,
    pub started_at: Timestamp,
    pub target_seconds: Option<i32>,
}

impl Running {
    pub fn new(id: Uuid, activity: Uuid, started_at: Timestamp, target_seconds: Option<i32>) -> Outcome<Self> {
        check_target(target_seconds)?;
        Ok(Self {
            id,
            activity,
            started_at,
            target_seconds,
        })
    }
}

/// A session as stored: ended, or the running one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    Ended(TimeSession),
    Running(Running),
}

impl Session {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Ended(s) => s.id,
            Self::Running(r) => r.id,
        }
    }
}

/// A session with its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedSession {
    pub user: Uuid,
    pub session: Session,
}

/// What ties a stored activity to its user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ownership {
    pub created: Timestamp,
    pub deleted: Option<Timestamp>,
}

/// A stored activity with its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedActivity {
    pub user: Uuid,
    pub activity: Activity,
    pub ownership: Ownership,
}

/// The first admin's setup token, in memory only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupToken {
    pub token_hash: [u8; 32],
    pub expires: Timestamp,
}

/// The admin login throttle, in memory only: per username, failures so far and the instant a lock ends.
#[derive(Debug, Default)]
pub struct LoginThrottle {
    failures: HashMap<String, (u32, Option<Timestamp>)>,
}

impl LoginThrottle {
    /// Seconds left on `username`'s lock, rounded up; 0 when it is not locked. A lapsed lock is forgotten.
    pub fn locked_for(&mut self, username: &str, now: Timestamp) -> u64 {
        let Some((count, until)) = self.failures.get(username) else {
            return 0;
        };
        if *count < MAX_LOGIN_FAILURES {
            return 0;
        }
        match until {
            Some(until) if *until > now => {
                let left = until.duration_since(now);
                let seconds = u64::try_from(left.as_secs()).unwrap_or(0);
                if left.subsec_nanos() > 0 {
                    seconds.saturating_add(1).max(1)
                } else {
                    seconds.max(1)
                }
            }
            _ => {
                self.failures.remove(username);
                0
            }
        }
    }

    pub fn record_failure(&mut self, username: &str, now: Timestamp) {
        let entry = self.failures.entry(username.to_owned()).or_insert((0, None));
        entry.0 = entry.0.saturating_add(1);
        if entry.0 >= MAX_LOGIN_FAILURES {
            entry.1 = now.checked_add(SignedDuration::from_secs(LOCK_SECONDS)).ok();
        }
    }

    pub fn clear(&mut self, username: &str) {
        self.failures.remove(username);
    }
}

/// The three payloads the signed cookies carry (design section 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberSession {
    pub user: Uuid,
    pub csrf: String,
    pub expires: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminSession {
    pub admin: Uuid,
    pub csrf: String,
    pub expires: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcHandshake {
    pub state: String,
    pub verifier: String,
    pub nonce: String,
    pub expires: Timestamp,
}

// ---- password hashes -------------------------------------------------------------------------------------

/// Node's `scrypt$N$r$p$<salt base64>$<key base64>` (design section 2.5).
#[derive(Clone, PartialEq, Eq)]
pub struct PasswordHash {
    log_n: u8,
    r: u32,
    p: u32,
    salt: Vec<u8>,
    key: Vec<u8>,
}

impl std::fmt::Debug for PasswordHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PasswordHash(<redacted>)")
    }
}

impl PasswordHash {
    /// Node's parameters: N 16384 (`log_n` 14), r 8, p 1, a 16-byte salt and a 64-byte key.
    pub const LOG_N: u8 = 14;
    pub const R: u32 = 8;
    pub const P: u32 = 1;
    pub const SALT_LEN: usize = 16;
    pub const KEY_LEN: usize = 64;

    /// Hashes `password` with Node's parameters and `salt`, which the caller draws at random.
    pub fn hash(password: &str, salt: [u8; Self::SALT_LEN]) -> Outcome<Self> {
        let mut key = vec![0u8; Self::KEY_LEN];
        derive(password.as_bytes(), &salt, Self::LOG_N, Self::R, Self::P, &mut key)?;
        Ok(Self {
            log_n: Self::LOG_N,
            r: Self::R,
            p: Self::P,
            salt: salt.to_vec(),
            key,
        })
    }

    /// Parses the stored string; refuses another kind, an `N` that is not a power of two, or unreadable base64.
    pub fn parse(stored: &str) -> Outcome<Self> {
        let refuse = |why: &str| errorinfo!(Code::CORRUPT_DOCUMENT, format!("a password hash {why}"));
        let mut parts = stored.split('$');
        if parts.next() != Some("scrypt") {
            return Err(refuse("is not scrypt"));
        }
        let n: u64 = parts.next().and_then(|s| s.parse().ok()).ok_or_else(|| refuse("has no N"))?;
        let r: u32 = parts.next().and_then(|s| s.parse().ok()).ok_or_else(|| refuse("has no r"))?;
        let p: u32 = parts.next().and_then(|s| s.parse().ok()).ok_or_else(|| refuse("has no p"))?;
        let salt = parts
            .next()
            .and_then(|s| STANDARD.decode(s).ok())
            .ok_or_else(|| refuse("has an unreadable salt"))?;
        let key = parts
            .next()
            .and_then(|s| STANDARD.decode(s).ok())
            .ok_or_else(|| refuse("has an unreadable key"))?;
        if parts.next().is_some() {
            return Err(refuse("has too many parts"));
        }
        if !n.is_power_of_two() || n < 2 {
            return Err(refuse("has an N that is not a power of two"));
        }
        let log_n = u8::try_from(n.trailing_zeros()).map_err(|_| refuse("has an N out of range"))?;
        if key.is_empty() || salt.is_empty() {
            return Err(refuse("has an empty salt or key"));
        }
        Params::new(log_n, r, p).map_err(|e| refuse(&format!("has parameters scrypt refuses: {e}")))?;
        Ok(Self { log_n, r, p, salt, key })
    }

    /// Whether `password` produces this hash, compared in constant time.
    pub fn verify(&self, password: &str) -> Outcome<bool> {
        let mut out = vec![0u8; self.key.len()];
        derive(password.as_bytes(), &self.salt, self.log_n, self.r, self.p, &mut out)?;
        Ok(out.ct_eq(&self.key).into())
    }
}

impl std::fmt::Display for PasswordHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n: u64 = 1u64 << self.log_n;
        write!(
            f,
            "scrypt${n}${}${}${}${}",
            self.r,
            self.p,
            STANDARD.encode(&self.salt),
            STANDARD.encode(&self.key)
        )
    }
}

fn derive(password: &[u8], salt: &[u8], log_n: u8, r: u32, p: u32, out: &mut [u8]) -> Outcome<()> {
    let params = Params::new(log_n, r, p).map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("scrypt parameters: {e}")))?;
    scrypt::scrypt(password, salt, &params, out).map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("scrypt output length: {e}")))
}

// ---- validation helpers ----------------------------------------------------------------------------------

/// Node's name normalisation: trimmed, runs of whitespace collapsed to one space. `None` when nothing is left
/// or more than [`MAX_NAME`] scalar values remain.
pub fn normalize_name(raw: &str) -> Option<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = name.chars().count();
    if count == 0 || count > MAX_NAME { None } else { Some(name) }
}

/// Node's username normalisation: trimmed and lowercased, then `^[a-z0-9._-]{3,32}$`.
pub fn normalize_username(raw: &str) -> Option<String> {
    let username = raw.trim().to_lowercase();
    let len = username.chars().count();
    let allowed = username
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if (MIN_USERNAME..=MAX_USERNAME).contains(&len) && allowed {
        Some(username)
    } else {
        None
    }
}

/// JavaScript's `.length`: UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Whether a password may be set: 10..=200 UTF-16 code units.
pub fn password_acceptable(password: &str) -> bool {
    (MIN_PASSWORD..=MAX_PASSWORD).contains(&utf16_len(password))
}

/// `targetMinutes` as the body carries it → seconds, or none. `None` for 0 and absent; `Err` outside `1..=1440`.
pub fn target_seconds_from_minutes(minutes: Option<i64>) -> Outcome<Option<i32>> {
    match minutes {
        None | Some(0) => Ok(None),
        Some(m) if (1..=1440).contains(&m) => i32::try_from(m.checked_mul(60).unwrap_or(i64::MAX)).map(Some).map_err(|_| bad_target()),
        Some(_) => Err(bad_target()),
    }
}

fn bad_target() -> rzlib_error::ErrorInfo {
    errorinfo!(Code::BAD_TARGET, "targetMinutes is an integer in 1..=1440, or absent")
}

fn check_target(target_seconds: Option<i32>) -> Outcome<()> {
    match target_seconds {
        Some(t) if !(MIN_TARGET_SECONDS..=MAX_TARGET_SECONDS).contains(&t) => Err(errorinfo!(
            Code::CORRUPT_DOCUMENT,
            format!("a target of {t} seconds is outside {MIN_TARGET_SECONDS}..={MAX_TARGET_SECONDS}")
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).unwrap()
    }

    #[test]
    fn names_are_trimmed_collapsed_and_bounded() {
        assert_eq!(normalize_name("  read   a  book "), Some("read a book".to_owned()));
        assert_eq!(normalize_name("   "), None);
        assert_eq!(normalize_name(&"ก".repeat(40)), Some("ก".repeat(40)));
        assert_eq!(normalize_name(&"ก".repeat(41)), None);
    }

    #[test]
    fn usernames_follow_nodes_pattern() {
        assert_eq!(normalize_username("  Admin.1 "), Some("admin.1".to_owned()));
        assert_eq!(normalize_username("ab"), None);
        assert_eq!(normalize_username("has space"), None);
        assert_eq!(normalize_username(&"a".repeat(33)), None);
    }

    #[test]
    fn passwords_are_counted_in_utf16_units() {
        assert_eq!(utf16_len("😀"), 2);
        assert!(password_acceptable("0123456789"));
        assert!(!password_acceptable("012345678"));
        assert!(password_acceptable(&"😀".repeat(5)), "5 emoji are 10 units");
        assert!(!password_acceptable(&"a".repeat(201)));
    }

    #[test]
    fn target_minutes_convert_as_node_did() {
        assert_eq!(target_seconds_from_minutes(None).unwrap(), None);
        assert_eq!(target_seconds_from_minutes(Some(0)).unwrap(), None);
        assert_eq!(target_seconds_from_minutes(Some(1)).unwrap(), Some(60));
        assert_eq!(target_seconds_from_minutes(Some(1440)).unwrap(), Some(86_400));
        assert_eq!(target_seconds_from_minutes(Some(1441)).unwrap_err().code, Code::BAD_TARGET);
        assert_eq!(target_seconds_from_minutes(Some(-1)).unwrap_err().code, Code::BAD_TARGET);
    }

    #[test]
    fn a_session_cannot_end_before_it_starts() {
        let id = Uuid::new_v4();
        assert!(TimeSession::new(id, id, at(10), at(5), None).is_err());
        assert!(TimeSession::new(id, id, at(10), at(10), Some(60)).is_ok());
        assert!(TimeSession::new(id, id, at(10), at(20), Some(59)).is_err());
        assert!(Running::new(id, id, at(10), Some(86_401)).is_err());
    }

    /// A hash Node's `admin.js` produced for the password `correct horse battery` with N 16384, r 8, p 1.
    #[test]
    fn hashes_round_trip_and_verify() {
        let hash = PasswordHash::hash("correct horse battery", [7u8; 16]).expect("hashes");
        let text = hash.to_string();
        assert!(text.starts_with("scrypt$16384$8$1$"), "{text}");
        let parsed = PasswordHash::parse(&text).expect("parses");
        assert_eq!(parsed, hash);
        assert!(parsed.verify("correct horse battery").unwrap());
        assert!(!parsed.verify("wrong").unwrap());
    }

    #[test]
    fn malformed_hashes_are_refused() {
        for bad in [
            "bcrypt$1$2$3$AA==$AA==",
            "scrypt$1000$8$1$AA==$AA==",
            "scrypt$16384$8$1$not base64!$AA==",
            "scrypt$16384$8$1$AA==",
        ] {
            assert_eq!(PasswordHash::parse(bad).unwrap_err().code, Code::CORRUPT_DOCUMENT, "{bad}");
        }
    }

    #[test]
    fn five_failures_lock_for_fifteen_minutes() {
        let mut throttle = LoginThrottle::default();
        let now = at(1_000);
        for _ in 0..4 {
            throttle.record_failure("root", now);
        }
        assert_eq!(throttle.locked_for("root", now), 0);
        throttle.record_failure("root", now);
        assert_eq!(throttle.locked_for("root", now), 900);
        assert_eq!(throttle.locked_for("root", at(1_000 + 899)), 1);
        assert_eq!(throttle.locked_for("root", at(1_000 + 900)), 0);
        assert_eq!(throttle.locked_for("root", at(1_000)), 0, "a lapsed lock is forgotten");
        throttle.record_failure("other", now);
        throttle.clear("other");
        assert_eq!(throttle.locked_for("other", now), 0);
    }
}

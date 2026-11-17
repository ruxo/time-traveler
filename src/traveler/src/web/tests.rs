//! The router over a fake facade: every route's status and JSON against design section 3.1, the cookie and CSRF
//! rules, the headers, the fallbacks and the static files.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use jiff::{SignedDuration, Timestamp};
use rzlib_error::{Outcome, errorinfo};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

use super::cookies::{CookieCodec, Kind};
use super::{AppState, WebSettings, router};
use crate::activities;
use crate::errors::Code;
use crate::facade::{AdminStatus, AppFacade, Login, State, UserSummary};
use crate::model::{Activity, Admin, AdminSession, Identity, MemberSession, OwnedActivity, Ownership, PasswordHash, Running, Session, TimeSession, User};

const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(second).unwrap()
}

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Start(Uuid, Uuid, Option<i32>),
    Stop(Uuid),
    Relabel(Uuid, Uuid, Uuid),
    DeleteSession(Uuid, Uuid),
    CreateActivity(Uuid, String, Option<String>, Option<String>),
    DeleteActivity(Uuid, Uuid),
    DeleteAccount(Uuid),
    Sessions(Uuid, Timestamp, Timestamp),
    Disable(Uuid),
    Enable(Uuid),
    DeleteUser(Uuid),
    Setup(String, String),
    Login(String),
}

#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<Call>>,
    user: Mutex<Option<User>>,
    admin: Mutex<Option<Admin>>,
    failure: Mutex<Option<String>>,
}

impl Fake {
    fn record(&self, call: Call) -> Outcome<()> {
        self.calls.lock().unwrap().push(call);
        match self.failure.lock().unwrap().as_deref() {
            Some(code) => Err(errorinfo!(code.to_owned().leak(), "scripted")),
            None => Ok(()),
        }
    }
}

fn user() -> User {
    User {
        id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        email: Some("a@b.c".to_owned()),
        name: None,
        disabled: None,
        created: at(1_700_000_000),
        updated: at(1_700_000_000),
    }
}

fn admin() -> Admin {
    Admin {
        id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
        username: "root".to_owned(),
        password: PasswordHash::hash("a long password", [1u8; 16]).unwrap(),
        created: at(1),
        updated: at(1),
    }
}

fn running() -> Running {
    Running::new(
        Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        activities::predefined_id("sleep"),
        at(1_700_000_000),
        Some(600),
    )
    .unwrap()
}

impl AppFacade for Fake {
    async fn ping(&self) -> Outcome<()> {
        Ok(())
    }
    async fn member(&self, id: Uuid) -> Outcome<User> {
        match self.user.lock().unwrap().clone() {
            Some(user) if user.id == id && user.disabled.is_none() => Ok(user),
            _ => Err(errorinfo!(Code::NOT_SIGNED_IN, "no")),
        }
    }
    async fn sign_in(&self, _identity: &Identity, _email: Option<&str>, _name: Option<&str>) -> Outcome<User> {
        Ok(user())
    }
    async fn state(&self, _user: Uuid) -> Outcome<State> {
        let custom = OwnedActivity {
            user: user().id,
            activity: Activity::new(
                Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
                "read".to_owned(),
                activities::PALETTE[0].to_owned(),
                "book".to_owned(),
            )
            .unwrap(),
            ownership: Ownership {
                created: at(1),
                deleted: Some(at(2)),
            },
        };
        Ok(State {
            custom: vec![custom],
            current: Some(running()),
        })
    }
    async fn sessions(&self, user: Uuid, from: Timestamp, to: Timestamp) -> Outcome<Vec<Session>> {
        self.record(Call::Sessions(user, from, to))?;
        let ended = TimeSession::new(running().id, running().activity, at(1_700_000_000), at(1_700_000_600), None).unwrap();
        Ok(vec![Session::Ended(ended), Session::Running(running())])
    }
    async fn start(&self, user: Uuid, activity: Uuid, target_seconds: Option<i32>) -> Outcome<Running> {
        self.record(Call::Start(user, activity, target_seconds))?;
        Ok(running())
    }
    async fn stop(&self, user: Uuid) -> Outcome<Option<TimeSession>> {
        self.record(Call::Stop(user))?;
        Ok(None)
    }
    async fn relabel(&self, user: Uuid, id: Uuid, activity: Uuid) -> Outcome<Session> {
        self.record(Call::Relabel(user, id, activity))?;
        Ok(Session::Running(running()))
    }
    async fn delete_session(&self, user: Uuid, id: Uuid) -> Outcome<()> {
        self.record(Call::DeleteSession(user, id))
    }
    async fn create_activity(&self, user: Uuid, name: String, color: Option<String>, icon: Option<String>) -> Outcome<OwnedActivity> {
        self.record(Call::CreateActivity(user, name.clone(), color, icon))?;
        Ok(OwnedActivity {
            user,
            activity: Activity::new(Uuid::new_v4(), name, activities::PALETTE[2].to_owned(), "music".to_owned()).unwrap(),
            ownership: Ownership { created: at(1), deleted: None },
        })
    }
    async fn delete_activity(&self, user: Uuid, id: Uuid) -> Outcome<()> {
        self.record(Call::DeleteActivity(user, id))
    }
    async fn delete_account(&self, user: Uuid) -> Outcome<()> {
        self.record(Call::DeleteAccount(user))
    }
    async fn admin(&self, id: Uuid) -> Outcome<Admin> {
        match self.admin.lock().unwrap().clone() {
            Some(admin) if admin.id == id => Ok(admin),
            _ => Err(errorinfo!(Code::NOT_SIGNED_IN, "no")),
        }
    }
    async fn admin_status(&self) -> Outcome<AdminStatus> {
        Ok(AdminStatus {
            needs_setup: self.admin.lock().unwrap().is_none(),
        })
    }
    async fn setup(&self, token: &str, username: String, _password: &str) -> Outcome<Admin> {
        self.record(Call::Setup(token.to_owned(), username))?;
        Ok(admin())
    }
    async fn login(&self, username: String, password: &str) -> Outcome<Login> {
        self.record(Call::Login(username.clone()))?;
        if username == "locked" {
            return Ok(Login::Locked { retry_after_seconds: 42 });
        }
        if password == "a long password" {
            Ok(Login::SignedIn(admin()))
        } else {
            Err(errorinfo!(Code::BAD_CREDENTIALS, "no"))
        }
    }
    async fn users(&self) -> Outcome<Vec<UserSummary>> {
        Ok(vec![UserSummary {
            user: user(),
            last_active: Some(at(1_700_000_000)),
            session_count: 3,
        }])
    }
    async fn disable_user(&self, id: Uuid) -> Outcome<UserSummary> {
        self.record(Call::Disable(id))?;
        let mut u = user();
        u.disabled = Some(at(1_700_000_100));
        Ok(UserSummary {
            user: u,
            last_active: None,
            session_count: 0,
        })
    }
    async fn enable_user(&self, id: Uuid) -> Outcome<UserSummary> {
        self.record(Call::Enable(id))?;
        Ok(UserSummary {
            user: user(),
            last_active: None,
            session_count: 0,
        })
    }
    async fn delete_user(&self, id: Uuid) -> Outcome<()> {
        self.record(Call::DeleteUser(id))
    }
}

struct Harness {
    router: axum::Router,
    fake: Arc<Fake>,
    codec: CookieCodec,
}

fn harness() -> Harness {
    let fake = Arc::new(Fake::default());
    let state = AppState {
        facade: fake.clone(),
        cookies: Arc::new(CookieCodec::new(KEY)),
        oidc: None,
        web: Arc::new(WebSettings {
            dev_login: true,
            public_url: None,
            member_days: 30,
            admin_hours: 12,
            oidc_minutes: 15,
        }),
    };
    Harness {
        router: router(state),
        fake,
        codec: CookieCodec::new(KEY),
    }
}

struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Value,
}

impl Harness {
    fn member_cookie(&self) -> String {
        *self.fake.user.lock().unwrap() = Some(user());
        let session = MemberSession {
            user: user().id,
            csrf: "csrf-token".to_owned(),
            expires: Timestamp::now().checked_add(SignedDuration::from_hours(1)).unwrap(),
        };
        format!("tt_sid={}", self.codec.encode_member(&session))
    }

    fn admin_cookie(&self) -> String {
        *self.fake.admin.lock().unwrap() = Some(admin());
        let session = AdminSession {
            admin: admin().id,
            csrf: "admin-csrf".to_owned(),
            expires: Timestamp::now().checked_add(SignedDuration::from_hours(1)).unwrap(),
        };
        format!("tt_admin={}", self.codec.encode_admin(&session))
    }

    async fn send(&self, method: Method, uri: &str, cookie: Option<&str>, csrf: Option<&str>, body: Option<&str>) -> Answer {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        if let Some(csrf) = csrf {
            builder = builder.header("x-csrf-token", csrf);
        }
        let body = match body {
            Some(json) => {
                builder = builder.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_owned())
            }
            None => Body::empty(),
        };
        let response = self.router.clone().oneshot(builder.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        Answer { status, headers, body }
    }

    fn calls(&self) -> Vec<Call> {
        std::mem::take(&mut *self.fake.calls.lock().unwrap())
    }
}

#[tokio::test]
async fn config_answers_providers_and_dev_login_without_a_cookie() {
    let h = harness();
    let a = h.send(Method::GET, "/api/config", None, None, None).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.body, json!({ "providers": [], "devLogin": true }));
    assert_eq!(a.headers[header::CONTENT_SECURITY_POLICY], super::CSP);
    assert_eq!(a.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(a.headers[header::REFERRER_POLICY], "same-origin");
}

#[tokio::test]
async fn member_routes_need_a_verified_cookie() {
    let h = harness();
    let a = h.send(Method::GET, "/api/me", None, None, None).await;
    assert_eq!(a.status, StatusCode::UNAUTHORIZED);
    assert_eq!(a.body, json!({ "error": "not_signed_in" }));

    let a = h.send(Method::GET, "/api/me", Some("tt_sid=forged.forged"), None, None).await;
    assert_eq!(a.status, StatusCode::UNAUTHORIZED);

    let cookie = h.member_cookie();
    let a = h.send(Method::GET, "/api/me", Some(&cookie), None, None).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.body, json!({ "name": null, "email": "a@b.c", "csrf": "csrf-token" }));
}

#[tokio::test]
async fn state_and_sessions_answer_nodes_shapes() {
    let h = harness();
    let cookie = h.member_cookie();
    let a = h.send(Method::GET, "/api/state", Some(&cookie), None, None).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(
        a.body["custom"],
        json!([{ "key": "c:44444444-4444-4444-8444-444444444444", "name": "read", "color": activities::PALETTE[0], "icon": "book", "deleted": true }])
    );
    assert_eq!(
        a.body["current"],
        json!({ "id": "33333333-3333-4333-8333-333333333333", "activity": "sleep", "startedAt": "2023-11-14T22:13:20.000Z", "endedAt": null, "targetSeconds": 600 })
    );
    assert!(a.body["serverNow"].as_str().unwrap().ends_with('Z'));
    assert!(a.body.get("hiddenBuiltins").is_none());

    let a = h
        .send(
            Method::GET,
            "/api/sessions?from=2023-11-14T00:00:00.000Z&to=2023-11-15T00:00:00.000Z",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    assert_eq!(a.body["sessions"][0]["endedAt"], "2023-11-14T22:23:20.000Z");
    assert_eq!(a.body["sessions"][1]["endedAt"], Value::Null);
    assert_eq!(h.calls(), vec![Call::Sessions(user().id, at(1_699_920_000), at(1_700_006_400))]);

    for bad in [
        "",
        "?from=x&to=y",
        "?from=2023-11-15T00:00:00Z&to=2023-11-14T00:00:00Z",
        "?from=2023-11-10T00:00:00Z&to=2023-11-14T00:00:00Z",
    ] {
        let a = h.send(Method::GET, &format!("/api/sessions{bad}"), Some(&cookie), None, None).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(a.body, json!({ "error": "bad_range" }), "{bad}");
    }
}

#[tokio::test]
async fn writes_need_the_csrf_header_and_validate_their_bodies() {
    let h = harness();
    let cookie = h.member_cookie();
    let a = h
        .send(Method::POST, "/api/sessions/start", Some(&cookie), None, Some(r#"{"activity":"sleep"}"#))
        .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert_eq!(a.body, json!({ "error": "bad_csrf" }));
    assert!(h.calls().is_empty());

    let a = h
        .send(
            Method::POST,
            "/api/sessions/start",
            Some(&cookie),
            Some("csrf-token"),
            Some(r#"{"activity":"sleep","targetMinutes":10}"#),
        )
        .await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    assert_eq!(a.body["current"]["activity"], "sleep");
    assert_eq!(h.calls(), vec![Call::Start(user().id, activities::predefined_id("sleep"), Some(600))]);

    for (body, code) in [
        (r#"{"activity":"sleep","targetMinutes":1441}"#, "bad_target"),
        (r#"{"activity":"sleep","targetMinutes":1.5}"#, "bad_target"),
        (r#"{"activity":"sleep","targetMinutes":"10"}"#, "bad_target"),
        (r#"{"activity":"hidden"}"#, "bad_activity"),
        (r#"{"activity":"c:nope"}"#, "bad_activity"),
        (r#"{}"#, "bad_activity"),
        ("not json", "bad_body"),
    ] {
        let a = h.send(Method::POST, "/api/sessions/start", Some(&cookie), Some("csrf-token"), Some(body)).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{body}: {}", a.body);
        assert_eq!(a.body, json!({ "error": code }), "{body}");
        assert!(h.calls().is_empty(), "{body}");
    }

    let a = h
        .send(
            Method::POST,
            "/api/sessions/start",
            Some(&cookie),
            Some("csrf-token"),
            Some(r#"{"activity":"sleep","targetMinutes":0}"#),
        )
        .await;
    assert_eq!(a.status, StatusCode::CREATED);
    assert_eq!(h.calls(), vec![Call::Start(user().id, activities::predefined_id("sleep"), None)]);
}

#[tokio::test]
async fn ids_in_paths_are_uuids_or_not_found_and_facade_codes_map_to_statuses() {
    let h = harness();
    let cookie = h.member_cookie();
    let a = h
        .send(Method::DELETE, "/api/sessions/not-a-uuid", Some(&cookie), Some("csrf-token"), None)
        .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert_eq!(a.body, json!({ "error": "not_found" }));

    let id = running().id;
    let a = h
        .send(Method::DELETE, &format!("/api/sessions/{id}"), Some(&cookie), Some("csrf-token"), None)
        .await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.body, json!({ "deleted": id.to_string() }));

    *h.fake.failure.lock().unwrap() = Some(Code::RUNNING.to_owned());
    let a = h
        .send(Method::DELETE, &format!("/api/sessions/{id}"), Some(&cookie), Some("csrf-token"), None)
        .await;
    assert_eq!(a.status, StatusCode::CONFLICT);
    assert_eq!(a.body, json!({ "error": "running" }));

    *h.fake.failure.lock().unwrap() = Some(Code::DATABASE.to_owned());
    let a = h.send(Method::POST, "/api/sessions/stop", Some(&cookie), Some("csrf-token"), None).await;
    assert_eq!(a.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(a.body, json!({ "error": "server_error" }), "the inner code stays inside");
    *h.fake.failure.lock().unwrap() = None;

    let a = h.send(Method::DELETE, "/api/activities/sleep", Some(&cookie), Some("csrf-token"), None).await;
    assert_eq!(a.status, StatusCode::NOT_FOUND, "a predefined key is not a custom activity");
    h.calls();
    let a = h
        .send(
            Method::POST,
            "/api/activities",
            Some(&cookie),
            Some("csrf-token"),
            Some(r##"{"name":"  read   books ","color":"#000","icon":"book"}"##),
        )
        .await;
    assert_eq!(a.status, StatusCode::CREATED);
    assert_eq!(a.body["activity"]["deleted"], false);
    assert_eq!(a.body["activity"]["icon"], "music", "the icon the facade answered");
    assert_eq!(
        h.calls(),
        vec![Call::CreateActivity(
            user().id,
            "read books".to_owned(),
            Some("#000".to_owned()),
            Some("book".to_owned())
        )]
    );
    let a = h
        .send(Method::POST, "/api/activities", Some(&cookie), Some("csrf-token"), Some(r#"{"name":"   "}"#))
        .await;
    assert_eq!(a.body, json!({ "error": "bad_name" }));

    let a = h.send(Method::DELETE, "/api/account", Some(&cookie), Some("csrf-token"), None).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.headers[header::SET_COOKIE].to_str().unwrap().starts_with("tt_sid=; Max-Age=0"));
}

#[tokio::test]
async fn unknown_paths_wrong_methods_and_static_files() {
    let h = harness();
    let a = h.send(Method::GET, "/api/nothing", None, None, None).await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert_eq!(a.body, json!({ "error": "not_found" }));

    let a = h.send(Method::PUT, "/api/config", None, None, None).await;
    assert_eq!(a.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(a.body, json!({ "error": "method_not_allowed" }));

    let a = h.send(Method::PUT, "/api/builtins/sleep", None, None, Some(r#"{"hidden":true}"#)).await;
    assert_eq!(a.status, StatusCode::NOT_FOUND, "the hide route is gone");

    let a = h.send(Method::GET, "/", None, None, None).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"));
    assert_eq!(a.headers[header::CACHE_CONTROL], "public, max-age=0");
    let etag = a.headers[header::ETAG].to_str().unwrap().to_owned();
    assert!(a.body.as_str().unwrap().contains("Time Traveler"));

    let request = Request::builder().uri("/").header(header::IF_NONE_MATCH, &etag).body(Body::empty()).unwrap();
    let response = h.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    let a = h.send(Method::GET, "/admin", None, None, None).await;
    assert_eq!(a.status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(a.headers[header::LOCATION], "/admin/");
    let a = h.send(Method::GET, "/admin/", None, None, None).await;
    assert_eq!(a.status, StatusCode::OK);
    let a = h.send(Method::GET, "/admin/admin.js", None, None, None).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.headers[header::CONTENT_TYPE].to_str().unwrap().contains("javascript"));
}

#[tokio::test]
async fn admin_routes_follow_nodes_contract() {
    let h = harness();
    let a = h.send(Method::GET, "/admin/api/status", None, None, None).await;
    assert_eq!(a.body, json!({ "needsSetup": true, "signedIn": false, "username": null, "csrf": null }));

    let a = h
        .send(
            Method::POST,
            "/admin/api/setup",
            None,
            None,
            Some(r#"{"token":"t","username":"Root","password":"a long password"}"#),
        )
        .await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    assert!(a.headers[header::SET_COOKIE].to_str().unwrap().starts_with("tt_admin="));
    assert_eq!(h.calls(), vec![Call::Setup("t".to_owned(), "root".to_owned())]);
    let a = h
        .send(
            Method::POST,
            "/admin/api/setup",
            None,
            None,
            Some(r#"{"token":"t","username":"ab","password":"a long password"}"#),
        )
        .await;
    assert_eq!(a.body, json!({ "error": "bad_username" }));
    let a = h
        .send(
            Method::POST,
            "/admin/api/setup",
            None,
            None,
            Some(r#"{"token":"t","username":"root","password":"short"}"#),
        )
        .await;
    assert_eq!(a.body, json!({ "error": "bad_password" }));

    let a = h
        .send(Method::POST, "/admin/api/login", None, None, Some(r#"{"username":"locked","password":"x"}"#))
        .await;
    assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(a.body, json!({ "error": "locked", "retryAfterSeconds": 42 }));
    let a = h
        .send(Method::POST, "/admin/api/login", None, None, Some(r#"{"username":"root","password":"wrong"}"#))
        .await;
    assert_eq!(a.status, StatusCode::UNAUTHORIZED);
    assert_eq!(a.body, json!({ "error": "bad_credentials" }));
    let a = h
        .send(
            Method::POST,
            "/admin/api/login",
            None,
            None,
            Some(r#"{"username":" ROOT ","password":"a long password"}"#),
        )
        .await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.body, json!({ "ok": true }));

    let cookie = h.admin_cookie();
    let a = h.send(Method::GET, "/admin/api/status", Some(&cookie), None, None).await;
    assert_eq!(
        a.body,
        json!({ "needsSetup": false, "signedIn": true, "username": "root", "csrf": "admin-csrf" })
    );
    let a = h.send(Method::GET, "/admin/api/users", Some(&cookie), None, None).await;
    assert_eq!(
        a.body["users"][0],
        json!({ "id": user().id.to_string(), "name": null, "email": "a@b.c", "createdAt": "2023-11-14T22:13:20.000Z", "disabled": false, "disabledAt": null, "lastActive": "2023-11-14T22:13:20.000Z", "sessionCount": 3 })
    );
    let a = h
        .send(Method::POST, &format!("/admin/api/users/{}/disable", user().id), Some(&cookie), None, None)
        .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    let a = h
        .send(
            Method::POST,
            &format!("/admin/api/users/{}/disable", user().id),
            Some(&cookie),
            Some("admin-csrf"),
            None,
        )
        .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    assert_eq!(a.body["user"]["disabled"], true);
    assert_eq!(a.body["user"]["disabledAt"], "2023-11-14T22:15:00.000Z");
    let a = h
        .send(
            Method::DELETE,
            &format!("/admin/api/users/{}", user().id),
            Some(&cookie),
            Some("admin-csrf"),
            None,
        )
        .await;
    assert_eq!(a.body, json!({ "ok": true }));
    let a = h.send(Method::GET, "/admin/api/users", None, None, None).await;
    assert_eq!(a.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sign_in_routes_without_a_provider() {
    let h = harness();
    let a = h.send(Method::GET, "/auth/login?provider=google", None, None, None).await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert_eq!(a.body, json!({ "error": "not_configured" }));

    let a = h.send(Method::GET, "/auth/callback?code=c&state=s", None, None, None).await;
    assert_eq!(a.status, StatusCode::FOUND);
    assert_eq!(a.headers[header::LOCATION], "/?login_error=1");

    let a = h.send(Method::POST, "/auth/dev-login", None, None, None).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.body, json!({ "ok": true }));
    let cookie = a.headers[header::SET_COOKIE].to_str().unwrap().to_owned();
    assert!(cookie.starts_with("tt_sid=") && cookie.contains("Max-Age=2592000") && cookie.contains("HttpOnly; SameSite=Lax"));
    assert!(!cookie.contains("Secure"), "plain http in tests");

    let cookie = h.member_cookie();
    let a = h.send(Method::POST, "/auth/logout", Some(&cookie), Some("csrf-token"), None).await;
    assert_eq!(a.body, json!({ "ok": true }));
    assert!(a.headers[header::SET_COOKIE].to_str().unwrap().starts_with("tt_sid=; Max-Age=0"));
}

#[tokio::test]
async fn https_requests_use_the_host_prefixed_cookie_names() {
    let h = harness();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/auth/dev-login")
        .header("x-forwarded-proto", "https")
        .body(Body::empty())
        .unwrap();
    let response = h.router.clone().oneshot(request).await.unwrap();
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap().to_owned();
    assert!(cookie.starts_with("__Host-tt_sid=") && cookie.ends_with("; Secure"), "{cookie}");
    assert_eq!(Kind::Member.name(true), "__Host-tt_sid");
}

#[test]
fn iso_formats_like_to_iso_string() {
    assert_eq!(super::iso(at(1_700_000_000)), "2023-11-14T22:13:20.000Z");
    assert_eq!(super::iso(Timestamp::from_millisecond(1_700_000_000_123).unwrap()), "2023-11-14T22:13:20.123Z");
}

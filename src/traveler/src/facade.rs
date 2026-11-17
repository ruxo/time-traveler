//! Every business decision (design sections 3 and 5): usability of an activity, the 50 cap and the default colour,
//! the start sequence and its retry, the setup token, the login throttle, disabled users. The web layer validates
//! shapes and calls exactly one method here per request; the store does what it is told.

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
use std::sync::Mutex;

use jiff::{SignedDuration, Timestamp};
use rzlib_error::{Outcome, errorinfo};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::activities;
use crate::errors::Code;
use crate::model::{Activity, Admin, Identity, LoginThrottle, OwnedActivity, Ownership, PasswordHash, Running, Session, SetupToken, TimeSession, User};
use crate::storage::{ATTEMPTS, Inserted, Store, stored_instant};

/// A user's view of the admin list: the user and what the sessions say about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSummary {
    pub user: User,
    pub last_active: Option<Timestamp>,
    pub session_count: i64,
}

/// What `GET /api/state` answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub custom: Vec<OwnedActivity>,
    pub current: Option<Running>,
}

/// What the admin page asks first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminStatus {
    pub needs_setup: bool,
}

/// An admin login that was not refused outright: signed in, or locked for a while yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Login {
    SignedIn(Admin),
    Locked { retry_after_seconds: u64 },
}

/// Random bytes as base64url without padding, the form every token takes.
pub fn random_token(bytes: usize) -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut buffer = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buffer);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

pub fn sha256(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

/// The service's one business layer, generic over the store so tests drive it with a fake.
pub struct Facade<S> {
    store: S,
    setup_token: Mutex<Option<SetupToken>>,
    throttle: Mutex<LoginThrottle>,
    setup_hours: u64,
}

/// What the web layer may ask. `trait-variant` makes every future `Send`.
#[trait_variant::make(Send)]
pub trait AppFacade {
    async fn ping(&self) -> Outcome<()>;

    // ---- members ----
    /// The signed-in user, refused when missing or disabled.
    async fn member(&self, id: Uuid) -> Outcome<User>;
    async fn sign_in(&self, identity: &Identity, email: Option<&str>, name: Option<&str>) -> Outcome<User>;
    async fn state(&self, user: Uuid) -> Outcome<State>;
    async fn sessions(&self, user: Uuid, from: Timestamp, to: Timestamp) -> Outcome<Vec<Session>>;
    async fn start(&self, user: Uuid, activity: Uuid, target_seconds: Option<i32>) -> Outcome<Running>;
    async fn stop(&self, user: Uuid) -> Outcome<Option<TimeSession>>;
    async fn relabel(&self, user: Uuid, id: Uuid, activity: Uuid) -> Outcome<Session>;
    async fn delete_session(&self, user: Uuid, id: Uuid) -> Outcome<()>;
    /// A colour off the palette, or an icon off the list, takes the default as if none had been named.
    async fn create_activity(&self, user: Uuid, name: String, color: Option<String>, icon: Option<String>) -> Outcome<OwnedActivity>;
    async fn delete_activity(&self, user: Uuid, id: Uuid) -> Outcome<()>;
    async fn delete_account(&self, user: Uuid) -> Outcome<()>;

    // ---- admins ----
    async fn admin(&self, id: Uuid) -> Outcome<Admin>;
    async fn admin_status(&self) -> Outcome<AdminStatus>;
    async fn setup(&self, token: &str, username: String, password: &str) -> Outcome<Admin>;
    /// `bad_credentials` for a wrong username or password; [`Login::Locked`] while the name is locked.
    async fn login(&self, username: String, password: &str) -> Outcome<Login>;
    async fn users(&self) -> Outcome<Vec<UserSummary>>;
    async fn disable_user(&self, id: Uuid) -> Outcome<UserSummary>;
    async fn enable_user(&self, id: Uuid) -> Outcome<UserSummary>;
    async fn delete_user(&self, id: Uuid) -> Outcome<()>;
}

impl<S: Store + Sync> Facade<S> {
    pub fn new(store: S, setup_hours: u64) -> Self {
        Self {
            store,
            setup_token: Mutex::new(None),
            throttle: Mutex::new(LoginThrottle::default()),
            setup_hours,
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    /// At startup: while no admin exists, a fresh setup token, returned once for the log. `None` when an admin exists.
    pub async fn issue_setup_token(&self) -> Outcome<Option<String>> {
        if self.store.admin_count().await? > 0 {
            return Ok(None);
        }
        let token = random_token(32);
        let hours = i64::try_from(self.setup_hours).unwrap_or(24);
        let expires = Timestamp::now().checked_add(SignedDuration::from_hours(hours)).unwrap_or(Timestamp::MAX);
        self.set_setup_token(Some(SetupToken {
            token_hash: sha256(&token),
            expires,
        }));
        Ok(Some(token))
    }

    fn set_setup_token(&self, token: Option<SetupToken>) {
        match self.setup_token.lock() {
            Ok(mut slot) => *slot = token,
            Err(poisoned) => *poisoned.into_inner() = token,
        }
    }

    /// Takes the setup token when `token` is it and it has not expired.
    fn take_setup_token(&self, token: &str, now: Timestamp) -> Option<SetupToken> {
        let mut slot = match self.setup_token.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        let matches = slot
            .as_ref()
            .is_some_and(|held| held.expires > now && bool::from(held.token_hash.ct_eq(&sha256(token))));
        if matches { slot.take() } else { None }
    }

    fn with_throttle<T>(&self, f: impl FnOnce(&mut LoginThrottle) -> T) -> T {
        match self.throttle.lock() {
            Ok(mut throttle) => f(&mut throttle),
            Err(poisoned) => f(&mut poisoned.into_inner()),
        }
    }

    /// Whether `activity` may be started or relabelled to by `user`: predefined, or the user's live activity.
    async fn usable(&self, user: Uuid, activity: Uuid) -> Outcome<()> {
        if activities::predefined_by_id(activity).is_some() || self.store.live_activity(user, activity).await?.is_some() {
            Ok(())
        } else {
            Err(errorinfo!(
                Code::BAD_ACTIVITY,
                "the activity is not predefined and not a live activity of this user"
            ))
        }
    }

    async fn summaries(&self, users: Vec<User>) -> Outcome<Vec<UserSummary>> {
        let ids: Vec<Uuid> = users.iter().map(|u| u.id).collect();
        let mut stats: HashMap<Uuid, (Option<Timestamp>, i64)> = self.store.session_stats(&ids).await?;
        Ok(users
            .into_iter()
            .map(|user| {
                let (last_active, session_count) = stats.remove(&user.id).unwrap_or((None, 0));
                UserSummary {
                    user,
                    last_active,
                    session_count,
                }
            })
            .collect())
    }

    async fn summary(&self, user: User) -> Outcome<UserSummary> {
        self.summaries(vec![user])
            .await?
            .pop()
            .ok_or_else(|| errorinfo!(Code::SERVER_ERROR, "a summary of one user is one summary"))
    }
}

fn not_found(what: &str) -> rzlib_error::ErrorInfo {
    errorinfo!(Code::NOT_FOUND, format!("{what} not found"))
}

impl<S: Store + Sync> AppFacade for Facade<S> {
    async fn ping(&self) -> Outcome<()> {
        self.store.ping().await
    }

    async fn member(&self, id: Uuid) -> Outcome<User> {
        match self.store.user(id).await? {
            Some(user) if user.disabled.is_none() => Ok(user),
            Some(_) => Err(errorinfo!(Code::NOT_SIGNED_IN, "the user is disabled")),
            None => Err(errorinfo!(Code::NOT_SIGNED_IN, "no such user")),
        }
    }

    async fn sign_in(&self, identity: &Identity, email: Option<&str>, name: Option<&str>) -> Outcome<User> {
        let user = self.store.sign_in(identity, email, name, stored_instant(Timestamp::now())).await?;
        if user.disabled.is_some() {
            return Err(errorinfo!(Code::DISABLED, "the account is disabled"));
        }
        Ok(user)
    }

    async fn state(&self, user: Uuid) -> Outcome<State> {
        let (custom, current) = tokio::join!(self.store.activities(user), self.store.running(user));
        Ok(State {
            custom: custom?,
            current: current?,
        })
    }

    async fn sessions(&self, user: Uuid, from: Timestamp, to: Timestamp) -> Outcome<Vec<Session>> {
        self.store.sessions_between(user, from, to).await
    }

    /// Design section 5.2: end whatever runs and start the new one at that instant; a lost race is tried again
    /// with a fresh `now`, up to [`ATTEMPTS`] times. When the activity is the one already running, the new session
    /// would begin exactly where that one ends, so the running one goes on instead and no second session is made.
    async fn start(&self, user: Uuid, activity: Uuid, target_seconds: Option<i32>) -> Outcome<Running> {
        self.usable(user, activity).await?;
        let now = stored_instant(Timestamp::now());
        if let Some(running) = self.store.continue_running(user, activity, target_seconds, now).await? {
            return Ok(running);
        }
        for attempt in 1..=ATTEMPTS {
            let now = stored_instant(Timestamp::now());
            let ended = self.store.end_running(user, now).await?;
            let started_at = ended.map_or(now, |ended| ended.ended_at);
            let running = Running::new(Uuid::new_v4(), activity, started_at, target_seconds)?;
            match self.store.insert_running(user, &running).await? {
                Inserted::Ok => return Ok(running),
                Inserted::AlreadyRunning => tracing::warn!(attempt, %user, "a start lost the race to another device"),
            }
        }
        Err(errorinfo!(Code::DATABASE, format!("a start lost the race {ATTEMPTS} times")))
    }

    async fn stop(&self, user: Uuid) -> Outcome<Option<TimeSession>> {
        self.store.end_running(user, stored_instant(Timestamp::now())).await
    }

    async fn relabel(&self, user: Uuid, id: Uuid, activity: Uuid) -> Outcome<Session> {
        self.usable(user, activity).await?;
        self.store.relabel(user, id, activity).await?.ok_or_else(|| not_found("the session"))
    }

    async fn delete_session(&self, user: Uuid, id: Uuid) -> Outcome<()> {
        if self.store.delete_ended(user, id).await? {
            return Ok(());
        }
        match self.store.session(user, id).await? {
            Some(Session::Running(_)) => Err(errorinfo!(Code::RUNNING, "stop the running session before deleting it")),
            Some(Session::Ended(_)) => Err(errorinfo!(Code::DATABASE, "the session was not deleted although it exists ended")),
            None => Err(not_found("the session")),
        }
    }

    async fn create_activity(&self, user: Uuid, name: String, color: Option<String>, icon: Option<String>) -> Outcome<OwnedActivity> {
        let (live, all) = self.store.activity_counts(user).await?;
        if live >= activities::MAX_CUSTOM as u64 {
            return Err(errorinfo!(
                Code::TOO_MANY,
                format!("a user holds at most {} live activities", activities::MAX_CUSTOM)
            ));
        }
        let color = match color {
            Some(c) if activities::in_palette(&c) => c,
            _ => activities::default_color(all).to_owned(),
        };
        let icon = match icon {
            Some(i) if activities::is_icon(&i) => i,
            _ => activities::DEFAULT_ICON.to_owned(),
        };
        let activity = OwnedActivity {
            user,
            activity: Activity::new(Uuid::new_v4(), name, color, icon)?,
            ownership: Ownership {
                created: stored_instant(Timestamp::now()),
                deleted: None,
            },
        };
        self.store.insert_activity(&activity).await?;
        Ok(activity)
    }

    async fn delete_activity(&self, user: Uuid, id: Uuid) -> Outcome<()> {
        if self.store.delete_activity(user, id, stored_instant(Timestamp::now())).await? {
            Ok(())
        } else {
            Err(not_found("the activity"))
        }
    }

    async fn delete_account(&self, user: Uuid) -> Outcome<()> {
        self.store.delete_user(user).await.map(|_| ())
    }

    async fn admin(&self, id: Uuid) -> Outcome<Admin> {
        self.store.admin(id).await?.ok_or_else(|| errorinfo!(Code::NOT_SIGNED_IN, "no such admin"))
    }

    async fn admin_status(&self) -> Outcome<AdminStatus> {
        Ok(AdminStatus {
            needs_setup: self.store.admin_count().await? == 0,
        })
    }

    /// Design section 5.3: hash first, refuse when an admin exists, take the token, insert; a failed insert puts the
    /// token back so the link still works.
    async fn setup(&self, token: &str, username: String, password: &str) -> Outcome<Admin> {
        let mut salt = [0u8; PasswordHash::SALT_LEN];
        rand::RngCore::fill_bytes(&mut rand::rng(), &mut salt);
        let hash = PasswordHash::hash(password, salt)?;
        if self.store.admin_count().await? > 0 {
            return Err(errorinfo!(Code::ALREADY_SET_UP, "an admin exists"));
        }
        let now = stored_instant(Timestamp::now());
        let Some(taken) = self.take_setup_token(token, now) else {
            return Err(errorinfo!(Code::BAD_TOKEN, "the setup token is unknown, used or expired"));
        };
        let admin = Admin {
            id: Uuid::new_v4(),
            username,
            password: hash,
            created: now,
            updated: now,
        };
        if let Err(error) = self.store.insert_admin(&admin).await {
            self.set_setup_token(Some(taken));
            return Err(error);
        }
        Ok(admin)
    }

    async fn login(&self, username: String, password: &str) -> Outcome<Login> {
        let now = Timestamp::now();
        let wait = self.with_throttle(|t| t.locked_for(&username, now));
        if wait > 0 {
            return Ok(Login::Locked { retry_after_seconds: wait });
        }
        let found = self.store.admin_by_username(&username).await?;
        let ok = match &found {
            Some(admin) => crate::model::utf16_len(password) <= crate::model::MAX_PASSWORD && admin.password.verify(password)?,
            None => {
                // Hash anyway, so the time taken does not reveal which names exist.
                let _ = PasswordHash::hash("timing-filler", [0u8; PasswordHash::SALT_LEN])?;
                false
            }
        };
        match (ok, found) {
            (true, Some(admin)) => {
                self.with_throttle(|t| t.clear(&username));
                Ok(Login::SignedIn(admin))
            }
            _ => {
                self.with_throttle(|t| t.record_failure(&username, now));
                Err(errorinfo!(Code::BAD_CREDENTIALS, "wrong username or password"))
            }
        }
    }

    async fn users(&self) -> Outcome<Vec<UserSummary>> {
        let users = self.store.users().await?;
        self.summaries(users).await
    }

    /// Design section 5.3: set `DisabledAt`, then end the running session; two writes, this order.
    async fn disable_user(&self, id: Uuid) -> Outcome<UserSummary> {
        let now = stored_instant(Timestamp::now());
        let user = self.store.disable_user(id, now).await?.ok_or_else(|| not_found("the user"))?;
        self.store.end_running(id, now).await?;
        self.summary(user).await
    }

    async fn enable_user(&self, id: Uuid) -> Outcome<UserSummary> {
        let user = self
            .store
            .enable_user(id, stored_instant(Timestamp::now()))
            .await?
            .ok_or_else(|| not_found("the user"))?;
        self.summary(user).await
    }

    async fn delete_user(&self, id: Uuid) -> Outcome<()> {
        if self.store.delete_user(id).await? {
            Ok(())
        } else {
            Err(not_found("the user"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Ownership;
    use crate::storage::Inserted;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).unwrap()
    }

    /// A store that answers what a test scripts: the live activity, the running session, and whether the next
    /// running insert collides.
    #[derive(Default)]
    struct FakeStore {
        live: Mutex<Vec<OwnedActivity>>,
        running: Mutex<Option<Running>>,
        collisions: AtomicU32,
        inserted: Mutex<Vec<Running>>,
        admins: Mutex<Vec<Admin>>,
        counts: Mutex<(u64, u64)>,
    }

    impl Store for FakeStore {
        async fn ping(&self) -> Outcome<()> {
            Ok(())
        }
        async fn user(&self, _id: Uuid) -> Outcome<Option<User>> {
            Ok(None)
        }
        async fn sign_in(&self, _identity: &Identity, _email: Option<&str>, _name: Option<&str>, _now: Timestamp) -> Outcome<User> {
            unimplemented!()
        }
        async fn users(&self) -> Outcome<Vec<User>> {
            Ok(vec![])
        }
        async fn session_stats(&self, _users: &[Uuid]) -> Outcome<HashMap<Uuid, (Option<Timestamp>, i64)>> {
            Ok(HashMap::new())
        }
        async fn disable_user(&self, _id: Uuid, _now: Timestamp) -> Outcome<Option<User>> {
            Ok(None)
        }
        async fn enable_user(&self, _id: Uuid, _now: Timestamp) -> Outcome<Option<User>> {
            Ok(None)
        }
        async fn delete_user(&self, _id: Uuid) -> Outcome<bool> {
            Ok(false)
        }
        async fn activities(&self, _user: Uuid) -> Outcome<Vec<OwnedActivity>> {
            Ok(self.live.lock().unwrap().clone())
        }
        async fn live_activity(&self, user: Uuid, id: Uuid) -> Outcome<Option<OwnedActivity>> {
            Ok(self
                .live
                .lock()
                .unwrap()
                .iter()
                .find(|a| a.user == user && a.activity.id == id && a.ownership.deleted.is_none())
                .cloned())
        }
        async fn activity_counts(&self, _user: Uuid) -> Outcome<(u64, u64)> {
            Ok(*self.counts.lock().unwrap())
        }
        async fn insert_activity(&self, activity: &OwnedActivity) -> Outcome<()> {
            self.live.lock().unwrap().push(activity.clone());
            Ok(())
        }
        async fn delete_activity(&self, _user: Uuid, _id: Uuid, _now: Timestamp) -> Outcome<bool> {
            Ok(false)
        }
        async fn running(&self, _user: Uuid) -> Outcome<Option<Running>> {
            Ok(self.running.lock().unwrap().clone())
        }
        async fn sessions_between(&self, _user: Uuid, _from: Timestamp, _to: Timestamp) -> Outcome<Vec<Session>> {
            Ok(vec![])
        }
        async fn end_running(&self, _user: Uuid, now: Timestamp) -> Outcome<Option<TimeSession>> {
            let running = self.running.lock().unwrap().take();
            Ok(running.map(|r| TimeSession::new(r.id, r.activity, r.started_at, now.max(r.started_at), r.target_seconds).unwrap()))
        }
        async fn continue_running(&self, _user: Uuid, activity: Uuid, target_seconds: Option<i32>, now: Timestamp) -> Outcome<Option<Running>> {
            let mut running = self.running.lock().unwrap();
            let Some(current) = running.as_mut().filter(|r| r.activity == activity) else {
                return Ok(None);
            };
            let run_seconds = i32::try_from(now.duration_since(current.started_at).as_secs()).unwrap();
            current.target_seconds = target_seconds.map(|t| run_seconds + t);
            Ok(Some(current.clone()))
        }
        async fn insert_running(&self, _user: Uuid, running: &Running) -> Outcome<Inserted> {
            if self.collisions.load(Ordering::SeqCst) > 0 {
                self.collisions.fetch_sub(1, Ordering::SeqCst);
                return Ok(Inserted::AlreadyRunning);
            }
            self.inserted.lock().unwrap().push(running.clone());
            *self.running.lock().unwrap() = Some(running.clone());
            Ok(Inserted::Ok)
        }
        async fn session(&self, _user: Uuid, _id: Uuid) -> Outcome<Option<Session>> {
            Ok(self.running.lock().unwrap().clone().map(Session::Running))
        }
        async fn relabel(&self, _user: Uuid, _id: Uuid, _activity: Uuid) -> Outcome<Option<Session>> {
            Ok(None)
        }
        async fn delete_ended(&self, _user: Uuid, _id: Uuid) -> Outcome<bool> {
            Ok(false)
        }
        async fn admin_count(&self) -> Outcome<u64> {
            Ok(self.admins.lock().unwrap().len() as u64)
        }
        async fn admin(&self, _id: Uuid) -> Outcome<Option<Admin>> {
            Ok(None)
        }
        async fn admin_by_username(&self, username: &str) -> Outcome<Option<Admin>> {
            Ok(self.admins.lock().unwrap().iter().find(|a| a.username == username).cloned())
        }
        async fn insert_admin(&self, admin: &Admin) -> Outcome<()> {
            self.admins.lock().unwrap().push(admin.clone());
            Ok(())
        }
    }

    fn facade() -> Facade<FakeStore> {
        Facade::new(FakeStore::default(), 24)
    }

    #[tokio::test]
    async fn a_start_ends_the_running_session_at_the_instant_the_new_one_begins() {
        let f = facade();
        let user = Uuid::new_v4();
        let sleep = activities::predefined_id("sleep");
        let work = activities::predefined_id("work");
        *f.store.running.lock().unwrap() = Some(Running::new(Uuid::new_v4(), sleep, at(0), None).unwrap());

        let started = f.start(user, work, Some(600)).await.expect("starts");

        let inserted = f.store.inserted.lock().unwrap().clone();
        assert_eq!(inserted, vec![started.clone()]);
        assert_eq!(started.activity, work);
        assert_eq!(started.target_seconds, Some(600));
        assert!(started.started_at >= at(0));
    }

    #[tokio::test]
    async fn a_start_of_the_running_activity_keeps_that_session_going() {
        let f = facade();
        let user = Uuid::new_v4();
        let sleep = activities::predefined_id("sleep");
        let began = Timestamp::now().checked_sub(jiff::SignedDuration::from_secs(100)).unwrap();
        let running = Running::new(Uuid::new_v4(), sleep, stored_instant(began), Some(60)).unwrap();
        *f.store.running.lock().unwrap() = Some(running.clone());

        let kept = f.start(user, sleep, Some(600)).await.expect("starts");

        assert!(f.store.inserted.lock().unwrap().is_empty(), "no second session");
        assert_eq!((kept.id, kept.started_at), (running.id, running.started_at));
        let target = kept.target_seconds.expect("a target");
        assert!(
            (700..=702).contains(&target),
            "600 s from the press, counted from the start 100 s ago: {target}"
        );

        let cleared = f.start(user, sleep, None).await.expect("starts");
        assert_eq!((cleared.id, cleared.target_seconds), (running.id, None));
        assert!(f.store.inserted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_start_that_loses_the_race_twice_still_succeeds_and_three_times_fails() {
        let f = facade();
        f.store.collisions.store(2, Ordering::SeqCst);
        f.start(Uuid::new_v4(), activities::predefined_id("eat"), None)
            .await
            .expect("third attempt wins");

        let f = facade();
        f.store.collisions.store(3, Ordering::SeqCst);
        let error = f.start(Uuid::new_v4(), activities::predefined_id("eat"), None).await.expect_err("gives up");
        assert_eq!(error.code, Code::DATABASE);
    }

    #[tokio::test]
    async fn an_unknown_or_deleted_activity_is_bad_activity() {
        let f = facade();
        let user = Uuid::new_v4();
        let deleted = OwnedActivity {
            user,
            activity: Activity::new(Uuid::new_v4(), "old".to_owned(), activities::PALETTE[0].to_owned(), "star".to_owned()).unwrap(),
            ownership: Ownership {
                created: at(0),
                deleted: Some(at(1)),
            },
        };
        let id = deleted.activity.id;
        f.store.live.lock().unwrap().push(deleted);

        assert_eq!(f.start(user, id, None).await.unwrap_err().code, Code::BAD_ACTIVITY);
        assert_eq!(f.start(user, Uuid::new_v4(), None).await.unwrap_err().code, Code::BAD_ACTIVITY);
        assert_eq!(f.relabel(user, Uuid::new_v4(), id).await.unwrap_err().code, Code::BAD_ACTIVITY);
    }

    #[tokio::test]
    async fn the_cap_and_the_default_colour_follow_the_counts() {
        let f = facade();
        let user = Uuid::new_v4();
        *f.store.counts.lock().unwrap() = (3, 7);
        let created = f
            .create_activity(user, "read".to_owned(), Some("#123456".to_owned()), Some("rocket".to_owned()))
            .await
            .expect("creates");
        assert_eq!(
            created.activity.color,
            activities::PALETTE[1],
            "7 mod 6, the body's colour being off the palette"
        );
        assert_eq!(created.activity.icon, activities::DEFAULT_ICON, "the body's icon being off the list");

        let chosen = f
            .create_activity(user, "write".to_owned(), Some(activities::PALETTE[4].to_owned()), Some("book".to_owned()))
            .await
            .expect("creates");
        assert_eq!(chosen.activity.color, activities::PALETTE[4]);
        assert_eq!(chosen.activity.icon, "book");
        assert_eq!(
            f.create_activity(user, "plain".to_owned(), None, None).await.expect("creates").activity.icon,
            activities::DEFAULT_ICON
        );

        *f.store.counts.lock().unwrap() = (50, 60);
        assert_eq!(f.create_activity(user, "more".to_owned(), None, None).await.unwrap_err().code, Code::TOO_MANY);
    }

    #[tokio::test]
    async fn deleting_the_running_session_is_running_and_an_unknown_one_not_found() {
        let f = facade();
        let user = Uuid::new_v4();
        let running = Running::new(Uuid::new_v4(), activities::predefined_id("nap"), at(0), None).unwrap();
        *f.store.running.lock().unwrap() = Some(running.clone());
        assert_eq!(f.delete_session(user, running.id).await.unwrap_err().code, Code::RUNNING);
        *f.store.running.lock().unwrap() = None;
        assert_eq!(f.delete_session(user, running.id).await.unwrap_err().code, Code::NOT_FOUND);
    }

    #[tokio::test]
    async fn setup_takes_the_token_once_and_refuses_after_an_admin_exists() {
        let f = facade();
        let token = f.issue_setup_token().await.expect("issues").expect("no admin yet");

        assert_eq!(f.setup("wrong", "root".to_owned(), "a long password").await.unwrap_err().code, Code::BAD_TOKEN);
        let admin = f.setup(&token, "root".to_owned(), "a long password").await.expect("sets up");
        assert_eq!(admin.username, "root");
        assert_eq!(
            f.setup(&token, "other".to_owned(), "a long password").await.unwrap_err().code,
            Code::ALREADY_SET_UP
        );
        assert_eq!(f.issue_setup_token().await.unwrap(), None);
    }

    #[tokio::test]
    async fn login_verifies_and_locks_after_five_failures() {
        let f = facade();
        let token = f.issue_setup_token().await.unwrap().unwrap();
        f.setup(&token, "root".to_owned(), "a long password").await.unwrap();

        assert!(matches!(f.login("root".to_owned(), "a long password").await.unwrap(), Login::SignedIn(_)));
        for _ in 0..5 {
            assert_eq!(f.login("root".to_owned(), "nope").await.unwrap_err().code, Code::BAD_CREDENTIALS);
        }
        // The lock started when the fifth attempt began; the hashing since then may have taken a second or two.
        match f.login("root".to_owned(), "a long password").await.unwrap() {
            Login::Locked { retry_after_seconds } => assert!((890..=900).contains(&retry_after_seconds), "{retry_after_seconds}"),
            other => panic!("expected a lock, got {other:?}"),
        }
        assert_eq!(f.login("ghost".to_owned(), "whatever").await.unwrap_err().code, Code::BAD_CREDENTIALS);
    }
}

//! The storage model (design section 2.3): one serde record per collection, PascalCase fields, UUIDs as BSON binary
//! subtype 4, instants as BSON `Date`; the conversions to and from the domain (section 2.4), refusing a document
//! that converts to no valid value; the [`Store`] trait the facade calls; [`MongoStore`]; and the index catalogue
//! both the migrate tool and the startup check read.
//!
//! Every driver call runs inside [`caught`] and a 5 s timeout. A driver error or a timeout is [`Code::DATABASE`]; a
//! document that cannot be read as its record is [`Code::CORRUPT_DOCUMENT`]. Both answer `server_error` on the wire.

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
use std::future::IntoFuture;
use std::time::Duration;

use bson::{Bson, Document, doc};
use jiff::{SignedDuration, Timestamp};
use mongodb::error::{ErrorKind, TRANSIENT_TRANSACTION_ERROR, UNKNOWN_TRANSACTION_COMMIT_RESULT, WriteFailure};
use mongodb::options::{IndexOptions, ReturnDocument};
use mongodb::{Client, ClientSession, Collection, Database, IndexModel};
use rzlib_error::{ErrorInfo, Outcome, errorinfo};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::Code;
use crate::model::{Activity, Admin, Identity, MAX_TARGET_SECONDS, OwnedActivity, OwnedSession, Ownership, PasswordHash, Running, Session, TimeSession, User};
use crate::panic::caught;

pub const USER_COLLECTION: &str = "User";
pub const ACTIVITY_COLLECTION: &str = "Activity";
pub const TIME_SESSION_COLLECTION: &str = "TimeSession";
pub const ADMIN_COLLECTION: &str = "Admin";

/// The bound on one driver call (bylaw 2).
pub const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// How many times a start whose insert lost the race, or a transaction the server calls transient, is tried.
pub const ATTEMPTS: u32 = 3;

// ---- serde bridges ---------------------------------------------------------------------------------------

/// `jiff::Timestamp` in memory, BSON `DateTime` in the document (millisecond precision).
pub mod timestamp_bson {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

    pub fn serialize<S: Serializer>(value: &Timestamp, s: S) -> Result<S::Ok, S::Error> {
        bson::DateTime::from_millis(value.as_millisecond()).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Timestamp, D::Error> {
        let raw = bson::DateTime::deserialize(d)?;
        Timestamp::from_millisecond(raw.timestamp_millis()).map_err(D::Error::custom)
    }
}

/// `Option<Timestamp>` as an optional BSON `DateTime`, absent when `None`.
pub mod timestamp_bson_opt {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

    pub fn serialize<S: Serializer>(value: &Option<Timestamp>, s: S) -> Result<S::Ok, S::Error> {
        value.map(|t| bson::DateTime::from_millis(t.as_millisecond())).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Timestamp>, D::Error> {
        let raw = Option::<bson::DateTime>::deserialize(d)?;
        raw.map(|r| Timestamp::from_millisecond(r.timestamp_millis()).map_err(D::Error::custom))
            .transpose()
    }
}

/// A `Uuid` as the BSON value a filter compares with: binary subtype 4.
pub fn uuid_bson(id: Uuid) -> Bson {
    Bson::Binary(bson::Binary::from_uuid(bson::Uuid::from(id)))
}

/// A `Timestamp` as the BSON value a filter compares with.
pub fn date_bson(at: Timestamp) -> Bson {
    Bson::DateTime(bson::DateTime::from_millis(at.as_millisecond()))
}

/// The instant as BSON keeps it, in whole milliseconds, so what a write returns equals what a read finds.
pub fn stored_instant(at: Timestamp) -> Timestamp {
    Timestamp::from_millisecond(at.as_millisecond()).unwrap_or(at)
}

// ---- records ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct IdentityRecord {
    pub issuer: String,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserRecord {
    #[serde(rename = "_id", with = "bson::serde_helpers::uuid_1_as_binary")]
    pub id: Uuid,
    pub identities: Vec<IdentityRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "timestamp_bson_opt")]
    pub disabled_at: Option<Timestamp>,
    #[serde(with = "timestamp_bson")]
    pub created_at: Timestamp,
    #[serde(with = "timestamp_bson")]
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityRecord {
    #[serde(rename = "_id", with = "bson::serde_helpers::uuid_1_as_binary")]
    pub id: Uuid,
    #[serde(with = "bson::serde_helpers::uuid_1_as_binary")]
    pub user_id: Uuid,
    pub name: String,
    pub color: String,
    /// Absent on a document written before icons could be chosen; it reads as the default icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(with = "timestamp_bson")]
    pub created_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "timestamp_bson_opt")]
    pub deleted_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TimeSessionRecord {
    #[serde(rename = "_id", with = "bson::serde_helpers::uuid_1_as_binary")]
    pub id: Uuid,
    #[serde(with = "bson::serde_helpers::uuid_1_as_binary")]
    pub user_id: Uuid,
    #[serde(with = "bson::serde_helpers::uuid_1_as_binary")]
    pub activity_id: Uuid,
    #[serde(with = "timestamp_bson")]
    pub started_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "timestamp_bson_opt")]
    pub ended_at: Option<Timestamp>,
    /// `true` while running, absent otherwise: the field the `OneRunning` partial index filters on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_seconds: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AdminRecord {
    #[serde(rename = "_id", with = "bson::serde_helpers::uuid_1_as_binary")]
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    #[serde(with = "timestamp_bson")]
    pub created_at: Timestamp,
    #[serde(with = "timestamp_bson")]
    pub updated_at: Timestamp,
}

// ---- conversions -----------------------------------------------------------------------------------------

fn corrupt(collection: &str, id: Uuid, why: impl std::fmt::Display) -> ErrorInfo {
    errorinfo!(Code::CORRUPT_DOCUMENT, format!("{collection} {id}: {why}"))
}

impl From<&Identity> for IdentityRecord {
    fn from(identity: &Identity) -> Self {
        Self {
            issuer: identity.issuer.clone(),
            subject: identity.subject.clone(),
        }
    }
}

impl TryFrom<UserRecord> for User {
    type Error = ErrorInfo;

    fn try_from(r: UserRecord) -> Outcome<Self> {
        if r.identities.is_empty() {
            return Err(corrupt(USER_COLLECTION, r.id, "no identity"));
        }
        Ok(Self {
            id: r.id,
            email: r.email,
            name: r.name,
            disabled: r.disabled_at,
            created: r.created_at,
            updated: r.updated_at,
        })
    }
}

impl From<&OwnedActivity> for ActivityRecord {
    fn from(a: &OwnedActivity) -> Self {
        Self {
            id: a.activity.id,
            user_id: a.user,
            name: a.activity.name.clone(),
            color: a.activity.color.clone(),
            icon: Some(a.activity.icon.clone()),
            created_at: a.ownership.created,
            deleted_at: a.ownership.deleted,
        }
    }
}

impl TryFrom<ActivityRecord> for OwnedActivity {
    type Error = ErrorInfo;

    fn try_from(r: ActivityRecord) -> Outcome<Self> {
        let icon = r.icon.unwrap_or_else(|| crate::activities::DEFAULT_ICON.to_owned());
        let activity = Activity::new(r.id, r.name, r.color, icon).map_err(|e| corrupt(ACTIVITY_COLLECTION, r.id, e.message))?;
        Ok(Self {
            user: r.user_id,
            activity,
            ownership: Ownership {
                created: r.created_at,
                deleted: r.deleted_at,
            },
        })
    }
}

impl TimeSessionRecord {
    pub fn running(user: Uuid, running: &Running) -> Self {
        Self {
            id: running.id,
            user_id: user,
            activity_id: running.activity,
            started_at: running.started_at,
            ended_at: None,
            running: Some(true),
            target_seconds: running.target_seconds,
        }
    }

    pub fn ended(user: Uuid, session: &TimeSession) -> Self {
        Self {
            id: session.id,
            user_id: user,
            activity_id: session.activity,
            started_at: session.started_at,
            ended_at: Some(session.ended_at),
            running: None,
            target_seconds: session.target_seconds,
        }
    }
}

impl From<&OwnedSession> for TimeSessionRecord {
    fn from(s: &OwnedSession) -> Self {
        match &s.session {
            Session::Ended(ended) => Self::ended(s.user, ended),
            Session::Running(running) => Self::running(s.user, running),
        }
    }
}

impl TryFrom<TimeSessionRecord> for OwnedSession {
    type Error = ErrorInfo;

    fn try_from(r: TimeSessionRecord) -> Outcome<Self> {
        let id = r.id;
        let session = match (r.ended_at, r.running) {
            (Some(ended_at), None) => Session::Ended(
                TimeSession::new(id, r.activity_id, r.started_at, ended_at, r.target_seconds).map_err(|e| corrupt(TIME_SESSION_COLLECTION, id, e.message))?,
            ),
            (None, Some(true)) => {
                Session::Running(Running::new(id, r.activity_id, r.started_at, r.target_seconds).map_err(|e| corrupt(TIME_SESSION_COLLECTION, id, e.message))?)
            }
            (ended, running) => {
                return Err(corrupt(
                    TIME_SESSION_COLLECTION,
                    id,
                    format!("EndedAt {ended:?} with Running {running:?} is neither ended nor running"),
                ));
            }
        };
        Ok(Self { user: r.user_id, session })
    }
}

impl From<&Admin> for AdminRecord {
    fn from(a: &Admin) -> Self {
        Self {
            id: a.id,
            username: a.username.clone(),
            password_hash: a.password.to_string(),
            created_at: a.created,
            updated_at: a.updated,
        }
    }
}

impl TryFrom<AdminRecord> for Admin {
    type Error = ErrorInfo;

    fn try_from(r: AdminRecord) -> Outcome<Self> {
        let password = PasswordHash::parse(&r.password_hash).map_err(|e| corrupt(ADMIN_COLLECTION, r.id, e.message))?;
        let username = crate::model::normalize_username(&r.username).ok_or_else(|| corrupt(ADMIN_COLLECTION, r.id, "the username fails the pattern"))?;
        if username != r.username {
            return Err(corrupt(ADMIN_COLLECTION, r.id, "the username is not normalised"));
        }
        Ok(Self {
            id: r.id,
            username,
            password,
            created: r.created_at,
            updated: r.updated_at,
        })
    }
}

// ---- the index catalogue ---------------------------------------------------------------------------------

/// One index as the design names it.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexSpec {
    pub collection: &'static str,
    pub name: &'static str,
    pub keys: Document,
    pub unique: bool,
    pub partial: Option<Document>,
}

impl IndexSpec {
    pub fn describe(&self) -> String {
        let keys: Vec<String> = self.keys.iter().map(|(k, v)| format!("{k}: {v}")).collect();
        let mut text = format!("{}.{} {{{}}}", self.collection, self.name, keys.join(", "));
        if self.unique {
            text.push_str(" unique");
        }
        if let Some(partial) = &self.partial {
            text.push_str(&format!(" partial {partial}"));
        }
        text
    }

    pub fn model(&self) -> IndexModel {
        let mut options = IndexOptions::builder().name(self.name.to_owned()).unique(self.unique).build();
        options.partial_filter_expression = self.partial.clone();
        IndexModel::builder().keys(self.keys.clone()).options(options).build()
    }
}

/// Every index of design section 2.3.
pub fn indexes() -> Vec<IndexSpec> {
    vec![
        IndexSpec {
            collection: USER_COLLECTION,
            name: "Identity",
            keys: doc! { "Identities.Issuer": 1, "Identities.Subject": 1 },
            unique: true,
            partial: None,
        },
        IndexSpec {
            collection: ACTIVITY_COLLECTION,
            name: "UserCreated",
            keys: doc! { "UserId": 1, "CreatedAt": 1 },
            unique: false,
            partial: None,
        },
        IndexSpec {
            collection: TIME_SESSION_COLLECTION,
            name: "UserStarted",
            keys: doc! { "UserId": 1, "StartedAt": 1 },
            unique: false,
            partial: None,
        },
        IndexSpec {
            collection: TIME_SESSION_COLLECTION,
            name: "OneRunning",
            keys: doc! { "UserId": 1 },
            unique: true,
            partial: Some(doc! { "Running": true }),
        },
        IndexSpec {
            collection: ADMIN_COLLECTION,
            name: "Username",
            keys: doc! { "Username": 1 },
            unique: true,
            partial: None,
        },
    ]
}

/// The indexes not on the server, described. A collection that does not exist yet has none.
pub async fn missing_indexes(db: &Database) -> Outcome<Vec<String>> {
    let mut missing = Vec::new();
    for spec in indexes() {
        let present = index_names(&db.collection::<Document>(spec.collection)).await?;
        if !present.iter().any(|(name, keys)| *name == spec.name && *keys == spec.keys) {
            missing.push(spec.describe());
        }
    }
    Ok(missing)
}

/// `(name, keys)` of every index on `collection`.
pub async fn index_names(collection: &Collection<Document>) -> Outcome<Vec<(String, Document)>> {
    const NAMESPACE_NOT_FOUND: i32 = 26;
    let mut cursor = match caught(collection.list_indexes().into_future()).await {
        Ok(Ok(cursor)) => cursor,
        Ok(Err(error)) if matches!(error.kind.as_ref(), ErrorKind::Command(c) if c.code == NAMESPACE_NOT_FOUND) => return Ok(Vec::new()),
        Ok(Err(error)) => return Err(database(&error, "list_indexes")),
        Err(detail) => return Err(driver_panicked(detail, "list_indexes")),
    };
    let mut found = Vec::new();
    while run("list_indexes", cursor.advance()).await? {
        let index = cursor.deserialize_current().map_err(|e| database(&e, "list_indexes"))?;
        let name = index.options.and_then(|o| o.name).unwrap_or_default();
        found.push((name, index.keys));
    }
    Ok(found)
}

/// Creates every index, which MongoDB answers as done when the same index exists under the same name.
pub async fn create_indexes(db: &Database) -> Outcome<Vec<String>> {
    let mut created = Vec::new();
    for spec in indexes() {
        let collection = db.collection::<Document>(spec.collection);
        run("create_index", collection.create_index(spec.model()).into_future()).await?;
        created.push(spec.describe());
    }
    Ok(created)
}

// ---- the store -------------------------------------------------------------------------------------------

/// What a running insert came back with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inserted {
    Ok,
    /// Another running session exists: the `OneRunning` index refused the document.
    AlreadyRunning,
}

/// Where Time Traveler's documents live.
#[trait_variant::make(Send)]
pub trait Store {
    async fn ping(&self) -> Outcome<()>;

    // ---- users ----
    async fn user(&self, id: Uuid) -> Outcome<Option<User>>;
    /// Finds the user with `identity` or creates one; refreshes `email` and `name` when given.
    async fn sign_in(&self, identity: &Identity, email: Option<&str>, name: Option<&str>, now: Timestamp) -> Outcome<User>;
    /// Every user, newest first.
    async fn users(&self) -> Outcome<Vec<User>>;
    /// Per user id: the greatest `StartedAt` and the count of the user's sessions. Users with none are absent.
    async fn session_stats(&self, users: &[Uuid]) -> Outcome<HashMap<Uuid, (Option<Timestamp>, i64)>>;
    /// Sets `DisabledAt` when absent (an earlier one stays); the user after, none when it does not exist.
    async fn disable_user(&self, id: Uuid, now: Timestamp) -> Outcome<Option<User>>;
    async fn enable_user(&self, id: Uuid, now: Timestamp) -> Outcome<Option<User>>;
    /// Deletes the user with every session and activity, in one transaction. `false` when no such user.
    async fn delete_user(&self, id: Uuid) -> Outcome<bool>;

    // ---- activities ----
    /// The user's activities in creation order, deleted ones included.
    async fn activities(&self, user: Uuid) -> Outcome<Vec<OwnedActivity>>;
    /// The activity when it is the user's and not deleted.
    async fn live_activity(&self, user: Uuid, id: Uuid) -> Outcome<Option<OwnedActivity>>;
    /// `(live, all)` counts of the user's activities.
    async fn activity_counts(&self, user: Uuid) -> Outcome<(u64, u64)>;
    async fn insert_activity(&self, activity: &OwnedActivity) -> Outcome<()>;
    /// Marks a live activity deleted; `false` when it is not the user's live activity.
    async fn delete_activity(&self, user: Uuid, id: Uuid, now: Timestamp) -> Outcome<bool>;

    // ---- sessions ----
    async fn running(&self, user: Uuid) -> Outcome<Option<Running>>;
    /// Every session with `StartedAt < to` and (running or `EndedAt > from`), by `StartedAt`.
    async fn sessions_between(&self, user: Uuid, from: Timestamp, to: Timestamp) -> Outcome<Vec<Session>>;
    /// Ends the running session at `now`, returning it ended; none when nothing was running.
    async fn end_running(&self, user: Uuid, now: Timestamp) -> Outcome<Option<TimeSession>>;
    /// Keeps the running session going when it already is `activity`: its id and start stay, and its target becomes
    /// `target_seconds` counted from `now` (none clears it). None when nothing runs, when something else runs, or
    /// when that target, counted from the session's start as targets are, would pass [`MAX_TARGET_SECONDS`].
    async fn continue_running(&self, user: Uuid, activity: Uuid, target_seconds: Option<i32>, now: Timestamp) -> Outcome<Option<Running>>;
    async fn insert_running(&self, user: Uuid, running: &Running) -> Outcome<Inserted>;
    async fn session(&self, user: Uuid, id: Uuid) -> Outcome<Option<Session>>;
    /// Sets the activity of the user's session `id`; the session after, none when it is not the user's.
    async fn relabel(&self, user: Uuid, id: Uuid, activity: Uuid) -> Outcome<Option<Session>>;
    /// Deletes an ended session; `false` when there is no ended session `id` of this user.
    async fn delete_ended(&self, user: Uuid, id: Uuid) -> Outcome<bool>;

    // ---- admins ----
    async fn admin_count(&self) -> Outcome<u64>;
    async fn admin(&self, id: Uuid) -> Outcome<Option<Admin>>;
    async fn admin_by_username(&self, username: &str) -> Outcome<Option<Admin>>;
    async fn insert_admin(&self, admin: &Admin) -> Outcome<()>;
}

/// [`Store`] over the `traveler` database.
pub struct MongoStore {
    client: Client,
    db: Database,
    users: Collection<UserRecord>,
    activities: Collection<ActivityRecord>,
    sessions: Collection<TimeSessionRecord>,
    admins: Collection<AdminRecord>,
}

impl MongoStore {
    pub fn new(client: Client, db: Database) -> Self {
        Self {
            users: db.collection(USER_COLLECTION),
            activities: db.collection(ACTIVITY_COLLECTION),
            sessions: db.collection(TIME_SESSION_COLLECTION),
            admins: db.collection(ADMIN_COLLECTION),
            client,
            db,
        }
    }

    pub fn database(&self) -> &Database {
        &self.db
    }
}

fn user_filter(user: Uuid) -> Document {
    doc! { "UserId": uuid_bson(user) }
}

fn running_filter(user: Uuid) -> Document {
    doc! { "UserId": uuid_bson(user), "Running": true }
}

fn end_update(now: Timestamp) -> Document {
    doc! { "$set": { "EndedAt": date_bson(now) }, "$unset": { "Running": "" } }
}

impl Store for MongoStore {
    async fn ping(&self) -> Outcome<()> {
        run("ping", self.db.run_command(doc! { "ping": 1 }).into_future()).await.map(|_| ())
    }

    async fn user(&self, id: Uuid) -> Outcome<Option<User>> {
        find_one(&self.users, doc! { "_id": uuid_bson(id) }, "user").await
    }

    async fn sign_in(&self, identity: &Identity, email: Option<&str>, name: Option<&str>, now: Timestamp) -> Outcome<User> {
        const OPERATION: &str = "sign_in";
        let filter = doc! { "Identities": { "$elemMatch": { "Issuer": &identity.issuer, "Subject": &identity.subject } } };
        let mut set = Document::new();
        if let Some(email) = email {
            set.insert("Email", email);
        }
        if let Some(name) = name {
            set.insert("Name", name);
        }
        if !set.is_empty() {
            set.insert("UpdatedAt", date_bson(now));
        }
        let mut on_insert = doc! {
            "_id": uuid_bson(Uuid::new_v4()),
            "Identities": [{ "Issuer": &identity.issuer, "Subject": &identity.subject }],
            "CreatedAt": date_bson(now),
        };
        if set.is_empty() {
            on_insert.insert("UpdatedAt", date_bson(now));
        }
        let mut update = doc! { "$setOnInsert": on_insert };
        if !set.is_empty() {
            update.insert("$set", set);
        }
        // Two first sign-ins of one identity at once: the loser's upsert hits the Identity index; the next attempt finds.
        let mut last = None;
        for _ in 0..2 {
            let call = self
                .users
                .find_one_and_update(filter.clone(), update.clone())
                .upsert(true)
                .return_document(ReturnDocument::After);
            match caught(tokio::time::timeout(OPERATION_TIMEOUT, call.into_future())).await {
                Ok(Ok(Ok(Some(record)))) => return User::try_from(record),
                Ok(Ok(Ok(None))) => return Err(errorinfo!(Code::DATABASE, "the upsert returned no document", OPERATION)),
                Ok(Ok(Err(error))) if is_duplicate_key(&error) => last = Some(database(&error, OPERATION)),
                Ok(Ok(Err(error))) => return Err(database(&error, OPERATION)),
                Ok(Err(_)) => return Err(timed_out(OPERATION)),
                Err(detail) => return Err(driver_panicked(detail, OPERATION)),
            }
        }
        Err(last.unwrap_or_else(|| errorinfo!(Code::DATABASE, "the sign-in upsert kept colliding", OPERATION)))
    }

    async fn users(&self) -> Outcome<Vec<User>> {
        find_all(&self.users, doc! {}, Some(doc! { "CreatedAt": -1 }), "users").await
    }

    async fn session_stats(&self, users: &[Uuid]) -> Outcome<HashMap<Uuid, (Option<Timestamp>, i64)>> {
        const OPERATION: &str = "session_stats";
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Row {
            #[serde(rename = "_id", with = "bson::serde_helpers::uuid_1_as_binary")]
            user: Uuid,
            #[serde(default, with = "timestamp_bson_opt")]
            last: Option<Timestamp>,
            count: i64,
        }
        if users.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<Bson> = users.iter().copied().map(uuid_bson).collect();
        let pipeline = vec![
            doc! { "$match": { "UserId": { "$in": ids } } },
            doc! { "$group": { "_id": "$UserId", "Last": { "$max": "$StartedAt" }, "Count": { "$sum": 1 } } },
        ];
        let mut cursor = run(OPERATION, self.sessions.aggregate(pipeline).into_future()).await?;
        let mut stats = HashMap::new();
        while run(OPERATION, cursor.advance()).await? {
            let row: Row = bson::from_document(cursor.deserialize_current().map_err(|e| database(&e, OPERATION))?)
                .map_err(|e| errorinfo!(Code::CORRUPT_DOCUMENT, format!("a session_stats row cannot be read: {e}"), OPERATION))?;
            stats.insert(row.user, (row.last, row.count));
        }
        Ok(stats)
    }

    async fn disable_user(&self, id: Uuid, now: Timestamp) -> Outcome<Option<User>> {
        let update = vec![doc! { "$set": { "DisabledAt": { "$ifNull": ["$DisabledAt", date_bson(now)] }, "UpdatedAt": date_bson(now) } }];
        find_one_and_update(&self.users, doc! { "_id": uuid_bson(id) }, update, "disable_user").await
    }

    async fn enable_user(&self, id: Uuid, now: Timestamp) -> Outcome<Option<User>> {
        let update = doc! { "$unset": { "DisabledAt": "" }, "$set": { "UpdatedAt": date_bson(now) } };
        find_one_and_update(&self.users, doc! { "_id": uuid_bson(id) }, update, "enable_user").await
    }

    async fn delete_user(&self, id: Uuid) -> Outcome<bool> {
        const OPERATION: &str = "delete_user";
        let mut session = run(OPERATION, self.client.start_session().into_future()).await?;
        let mut attempts = 0u32;
        loop {
            attempts = attempts.saturating_add(1);
            match self.delete_user_in(&mut session, id).await {
                Ok(deleted) => return Ok(deleted),
                Err((error, transient)) if transient && attempts < ATTEMPTS => {
                    tracing::warn!(attempt = attempts, "the delete transaction was transient: {}", error.message);
                }
                Err((error, _)) => {
                    let _ = caught(session.abort_transaction().into_future()).await;
                    return Err(error);
                }
            }
        }
    }

    async fn activities(&self, user: Uuid) -> Outcome<Vec<OwnedActivity>> {
        find_all(&self.activities, user_filter(user), Some(doc! { "CreatedAt": 1 }), "activities").await
    }

    async fn live_activity(&self, user: Uuid, id: Uuid) -> Outcome<Option<OwnedActivity>> {
        let filter = doc! { "_id": uuid_bson(id), "UserId": uuid_bson(user), "DeletedAt": { "$exists": false } };
        find_one(&self.activities, filter, "live_activity").await
    }

    async fn activity_counts(&self, user: Uuid) -> Outcome<(u64, u64)> {
        let live = doc! { "UserId": uuid_bson(user), "DeletedAt": { "$exists": false } };
        let live = run("activity_counts", self.activities.count_documents(live).into_future()).await?;
        let all = run("activity_counts", self.activities.count_documents(user_filter(user)).into_future()).await?;
        Ok((live, all))
    }

    async fn insert_activity(&self, activity: &OwnedActivity) -> Outcome<()> {
        run("insert_activity", self.activities.insert_one(ActivityRecord::from(activity)).into_future())
            .await
            .map(|_| ())
    }

    async fn delete_activity(&self, user: Uuid, id: Uuid, now: Timestamp) -> Outcome<bool> {
        let filter = doc! { "_id": uuid_bson(id), "UserId": uuid_bson(user), "DeletedAt": { "$exists": false } };
        let update = doc! { "$set": { "DeletedAt": date_bson(now) } };
        let result = run("delete_activity", self.activities.update_one(filter, update).into_future()).await?;
        Ok(result.matched_count == 1)
    }

    async fn running(&self, user: Uuid) -> Outcome<Option<Running>> {
        let found: Option<OwnedSession> = find_one(&self.sessions, running_filter(user), "running").await?;
        match found.map(|s| s.session) {
            Some(Session::Running(running)) => Ok(Some(running)),
            Some(Session::Ended(ended)) => Err(corrupt(TIME_SESSION_COLLECTION, ended.id, "matched Running: true but has EndedAt")),
            None => Ok(None),
        }
    }

    async fn sessions_between(&self, user: Uuid, from: Timestamp, to: Timestamp) -> Outcome<Vec<Session>> {
        let filter = doc! {
            "UserId": uuid_bson(user),
            "StartedAt": { "$lt": date_bson(to) },
            "$or": [ { "Running": true }, { "EndedAt": { "$gt": date_bson(from) } } ],
        };
        let found: Vec<OwnedSession> = find_all(&self.sessions, filter, Some(doc! { "StartedAt": 1 }), "sessions_between").await?;
        Ok(found.into_iter().map(|s| s.session).collect())
    }

    async fn end_running(&self, user: Uuid, now: Timestamp) -> Outcome<Option<TimeSession>> {
        let ended: Option<OwnedSession> = find_one_and_update(&self.sessions, running_filter(user), end_update(now), "end_running").await?;
        match ended.map(|s| s.session) {
            Some(Session::Ended(session)) => Ok(Some(session)),
            Some(Session::Running(running)) => Err(corrupt(TIME_SESSION_COLLECTION, running.id, "still running after the end update")),
            None => Ok(None),
        }
    }

    async fn continue_running(&self, user: Uuid, activity: Uuid, target_seconds: Option<i32>, now: Timestamp) -> Outcome<Option<Running>> {
        const OPERATION: &str = "continue_running";
        let mut filter = running_filter(user);
        filter.insert("ActivityId", uuid_bson(activity));
        let found: Option<OwnedSession> = match target_seconds {
            None => find_one_and_update(&self.sessions, filter, doc! { "$unset": { "TargetSeconds": "" } }, OPERATION).await?,
            Some(target) => {
                // A target is counted from StartedAt, so one set now is the seconds already run plus `target`. The
                // filter admits only a session young enough for that sum to stay within the limit a read enforces.
                let room = SignedDuration::from_secs(i64::from(MAX_TARGET_SECONDS.saturating_sub(target)));
                let earliest = now
                    .checked_sub(room)
                    .map_err(|e| errorinfo!(Code::DATABASE, format!("no instant {room:?} before {now}: {e}"), OPERATION))?;
                filter.insert("StartedAt", doc! { "$gte": date_bson(earliest) });
                let run_seconds = doc! { "$toInt": { "$trunc": { "$divide": [ { "$subtract": [date_bson(now), "$StartedAt"] }, 1000 ] } } };
                let update = vec![doc! { "$set": { "TargetSeconds": { "$add": [ { "$max": [0, run_seconds] }, target ] } } }];
                find_one_and_update(&self.sessions, filter, update, OPERATION).await?
            }
        };
        match found.map(|s| s.session) {
            Some(Session::Running(running)) => Ok(Some(running)),
            Some(Session::Ended(ended)) => Err(corrupt(TIME_SESSION_COLLECTION, ended.id, "matched Running: true but has EndedAt")),
            None => Ok(None),
        }
    }

    async fn insert_running(&self, user: Uuid, running: &Running) -> Outcome<Inserted> {
        const OPERATION: &str = "insert_running";
        let record = TimeSessionRecord::running(user, running);
        match caught(tokio::time::timeout(OPERATION_TIMEOUT, self.sessions.insert_one(record).into_future())).await {
            Ok(Ok(Ok(_))) => Ok(Inserted::Ok),
            Ok(Ok(Err(error))) if is_duplicate_key(&error) => Ok(Inserted::AlreadyRunning),
            Ok(Ok(Err(error))) => Err(database(&error, OPERATION)),
            Ok(Err(_)) => Err(timed_out(OPERATION)),
            Err(detail) => Err(driver_panicked(detail, OPERATION)),
        }
    }

    async fn session(&self, user: Uuid, id: Uuid) -> Outcome<Option<Session>> {
        let found: Option<OwnedSession> = find_one(&self.sessions, doc! { "_id": uuid_bson(id), "UserId": uuid_bson(user) }, "session").await?;
        Ok(found.map(|s| s.session))
    }

    async fn relabel(&self, user: Uuid, id: Uuid, activity: Uuid) -> Outcome<Option<Session>> {
        let filter = doc! { "_id": uuid_bson(id), "UserId": uuid_bson(user) };
        let update = doc! { "$set": { "ActivityId": uuid_bson(activity) } };
        let found: Option<OwnedSession> = find_one_and_update(&self.sessions, filter, update, "relabel").await?;
        Ok(found.map(|s| s.session))
    }

    async fn delete_ended(&self, user: Uuid, id: Uuid) -> Outcome<bool> {
        let filter = doc! { "_id": uuid_bson(id), "UserId": uuid_bson(user), "EndedAt": { "$exists": true } };
        let result = run("delete_ended", self.sessions.delete_one(filter).into_future()).await?;
        Ok(result.deleted_count == 1)
    }

    async fn admin_count(&self) -> Outcome<u64> {
        run("admin_count", self.admins.count_documents(doc! {}).into_future()).await
    }

    async fn admin(&self, id: Uuid) -> Outcome<Option<Admin>> {
        find_one(&self.admins, doc! { "_id": uuid_bson(id) }, "admin").await
    }

    async fn admin_by_username(&self, username: &str) -> Outcome<Option<Admin>> {
        find_one(&self.admins, doc! { "Username": username }, "admin_by_username").await
    }

    async fn insert_admin(&self, admin: &Admin) -> Outcome<()> {
        run("insert_admin", self.admins.insert_one(AdminRecord::from(admin)).into_future())
            .await
            .map(|_| ())
    }
}

impl MongoStore {
    /// One attempt at the delete transaction. `Err((error, transient))`: `transient` when the whole transaction may be
    /// tried again (the server's `TransientTransactionError` label).
    async fn delete_user_in(&self, session: &mut ClientSession, id: Uuid) -> Result<bool, (ErrorInfo, bool)> {
        tx_step(session.start_transaction().into_future()).await?;
        tx_step(self.sessions.delete_many(user_filter(id)).session(&mut *session).into_future()).await?;
        tx_step(self.activities.delete_many(user_filter(id)).session(&mut *session).into_future()).await?;
        let deleted = tx_step(self.users.delete_one(doc! { "_id": uuid_bson(id) }).session(&mut *session).into_future())
            .await?
            .deleted_count
            == 1;
        // The commit alone may be retried while the server cannot say whether it took.
        let mut commits = 0u32;
        loop {
            commits = commits.saturating_add(1);
            match tx_step(session.commit_transaction().into_future()).await {
                Ok(()) => return Ok(deleted),
                Err((error, _)) if error.message.contains(UNKNOWN_TRANSACTION_COMMIT_RESULT) && commits < ATTEMPTS => continue,
                Err(failure) => return Err(failure),
            }
        }
    }
}

/// One step of a transaction: the call inside the panic boundary and the timeout; a failure with whether the
/// whole transaction may be tried again (the server's `TransientTransactionError` label).
async fn tx_step<T>(call: impl Future<Output = mongodb::error::Result<T>>) -> Result<T, (ErrorInfo, bool)> {
    const OPERATION: &str = "delete_user";
    match caught(tokio::time::timeout(OPERATION_TIMEOUT, call)).await {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(error))) => {
            let transient = error.contains_label(TRANSIENT_TRANSACTION_ERROR);
            let mut info = database(&error, OPERATION);
            if error.contains_label(UNKNOWN_TRANSACTION_COMMIT_RESULT) {
                info.message.push_str(&format!(" [{UNKNOWN_TRANSACTION_COMMIT_RESULT}]"));
            }
            Err((info, transient))
        }
        Ok(Err(_)) => Err((timed_out(OPERATION), false)),
        Err(detail) => Err((driver_panicked(detail, OPERATION), false)),
    }
}

// ---- driver helpers --------------------------------------------------------------------------------------

const DUPLICATE_KEY: i32 = 11000;

/// Awaits one driver call inside the panic boundary and the operation timeout.
pub async fn run<T>(operation: &'static str, call: impl Future<Output = mongodb::error::Result<T>>) -> Outcome<T> {
    match caught(tokio::time::timeout(OPERATION_TIMEOUT, call)).await {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(error))) => Err(database(&error, operation)),
        Ok(Err(_)) => Err(timed_out(operation)),
        Err(detail) => Err(driver_panicked(detail, operation)),
    }
}

pub fn database(error: &mongodb::error::Error, operation: &'static str) -> ErrorInfo {
    match error.kind.as_ref() {
        ErrorKind::BsonDeserialization(_) => errorinfo!(
            Code::CORRUPT_DOCUMENT,
            format!("a stored document cannot be read as its type: {error}"),
            operation
        ),
        _ => errorinfo!(Code::DATABASE, format!("MongoDB: {error}"), operation),
    }
}

pub fn timed_out(operation: &'static str) -> ErrorInfo {
    errorinfo!(Code::DATABASE, format!("MongoDB did not answer within {OPERATION_TIMEOUT:?}"), operation)
}

pub fn driver_panicked(detail: String, operation: &'static str) -> ErrorInfo {
    errorinfo!(Code::SERVER_ERROR, format!("the MongoDB driver panicked: {detail}"), operation)
}

fn is_duplicate_key(error: &mongodb::error::Error) -> bool {
    match error.kind.as_ref() {
        ErrorKind::Write(WriteFailure::WriteError(write)) => write.code == DUPLICATE_KEY,
        ErrorKind::Command(command) => command.code == DUPLICATE_KEY,
        _ => false,
    }
}

/// `findOne`, converted into the domain type; a record that converts to no valid value is refused.
async fn find_one<R, T>(collection: &Collection<R>, filter: Document, operation: &'static str) -> Outcome<Option<T>>
where
    R: DeserializeOwned + Send + Sync,
    T: TryFrom<R, Error = ErrorInfo>,
{
    run(operation, collection.find_one(filter).into_future()).await?.map(T::try_from).transpose()
}

async fn find_all<R, T>(collection: &Collection<R>, filter: Document, sort: Option<Document>, operation: &'static str) -> Outcome<Vec<T>>
where
    R: DeserializeOwned + Send + Sync,
    T: TryFrom<R, Error = ErrorInfo>,
{
    let mut find = collection.find(filter);
    if let Some(sort) = sort {
        find = find.sort(sort);
    }
    let mut cursor = run(operation, find.into_future()).await?;
    let mut found = Vec::new();
    while run(operation, cursor.advance()).await? {
        let record = cursor.deserialize_current().map_err(|e| database(&e, operation))?;
        found.push(T::try_from(record)?);
    }
    Ok(found)
}

/// `findOneAndUpdate` returning the document after the update, converted.
async fn find_one_and_update<R, T, U>(collection: &Collection<R>, filter: Document, update: U, operation: &'static str) -> Outcome<Option<T>>
where
    R: DeserializeOwned + Send + Sync,
    T: TryFrom<R, Error = ErrorInfo>,
    U: Into<mongodb::options::UpdateModifications>,
{
    let call = collection.find_one_and_update(filter, update).return_document(ReturnDocument::After);
    run(operation, call.into_future()).await?.map(T::try_from).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).unwrap()
    }

    #[test]
    fn a_running_record_converts_to_running_and_an_ended_one_to_a_session() {
        let user = Uuid::new_v4();
        let running = Running::new(Uuid::new_v4(), crate::activities::predefined_id("sleep"), at(10), Some(600)).unwrap();
        let record = TimeSessionRecord::running(user, &running);
        assert_eq!(record.running, Some(true));
        assert_eq!(record.ended_at, None);
        let back = OwnedSession::try_from(record).unwrap();
        assert_eq!(back.session, Session::Running(running.clone()));
        assert_eq!(back.user, user);

        let ended = TimeSession::new(running.id, running.activity, at(10), at(20), None).unwrap();
        let record = TimeSessionRecord::ended(user, &ended);
        assert_eq!(record.running, None);
        assert_eq!(OwnedSession::try_from(record).unwrap().session, Session::Ended(ended));
    }

    #[test]
    fn a_session_record_in_any_other_combination_is_refused() {
        let base = TimeSessionRecord {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            activity_id: Uuid::new_v4(),
            started_at: at(10),
            ended_at: None,
            running: None,
            target_seconds: None,
        };
        let neither = base.clone();
        let both = TimeSessionRecord {
            ended_at: Some(at(20)),
            running: Some(true),
            ..base.clone()
        };
        let false_flag = TimeSessionRecord {
            running: Some(false),
            ..base.clone()
        };
        let backwards = TimeSessionRecord {
            ended_at: Some(at(5)),
            ..base.clone()
        };
        let bad_target = TimeSessionRecord {
            ended_at: Some(at(20)),
            target_seconds: Some(30),
            ..base
        };
        for record in [neither, both, false_flag, backwards, bad_target] {
            assert_eq!(OwnedSession::try_from(record).unwrap_err().code, Code::CORRUPT_DOCUMENT);
        }
    }

    #[test]
    fn records_write_pascal_case_with_absent_optionals() {
        let user = Uuid::new_v4();
        let record = TimeSessionRecord::running(user, &Running::new(Uuid::new_v4(), Uuid::new_v4(), at(10), None).unwrap());
        let document = bson::to_document(&record).unwrap();
        assert!(document.contains_key("UserId") && document.contains_key("StartedAt") && document.contains_key("Running"));
        assert!(!document.contains_key("EndedAt") && !document.contains_key("TargetSeconds"));
        assert_eq!(document.get("Running"), Some(&Bson::Boolean(true)));
        assert!(matches!(document.get("_id"), Some(Bson::Binary(b)) if b.subtype == bson::spec::BinarySubtype::Uuid));
        assert!(matches!(document.get("StartedAt"), Some(Bson::DateTime(_))));
    }

    #[test]
    fn a_user_without_identities_and_an_activity_off_the_palette_are_refused() {
        let user = UserRecord {
            id: Uuid::new_v4(),
            identities: vec![],
            email: None,
            name: None,
            disabled_at: None,
            created_at: at(1),
            updated_at: at(1),
        };
        assert_eq!(User::try_from(user).unwrap_err().code, Code::CORRUPT_DOCUMENT);
        let activity = ActivityRecord {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "read".to_owned(),
            color: "#000000".to_owned(),
            icon: None,
            created_at: at(1),
            deleted_at: None,
        };
        assert_eq!(OwnedActivity::try_from(activity).unwrap_err().code, Code::CORRUPT_DOCUMENT);
    }

    #[test]
    fn an_activity_without_an_icon_reads_as_the_default_and_an_unknown_icon_is_refused() {
        let record = |icon: Option<&str>| ActivityRecord {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "read".to_owned(),
            color: crate::activities::PALETTE[0].to_owned(),
            icon: icon.map(str::to_owned),
            created_at: at(1),
            deleted_at: None,
        };
        let old = OwnedActivity::try_from(record(None)).unwrap();
        assert_eq!(old.activity.icon, crate::activities::DEFAULT_ICON);
        assert_eq!(ActivityRecord::from(&old).icon.as_deref(), Some("star"));
        assert_eq!(OwnedActivity::try_from(record(Some("book"))).unwrap().activity.icon, "book");
        assert_eq!(OwnedActivity::try_from(record(Some("rocket"))).unwrap_err().code, Code::CORRUPT_DOCUMENT);
    }

    #[test]
    fn the_catalogue_names_the_five_indexes_of_the_design() {
        let names: Vec<&str> = indexes().iter().map(|i| i.name).collect();
        assert_eq!(names, ["Identity", "UserCreated", "UserStarted", "OneRunning", "Username"]);
        let one_running = indexes().into_iter().find(|i| i.name == "OneRunning").unwrap();
        assert!(one_running.unique);
        assert_eq!(one_running.partial, Some(doc! { "Running": true }));
    }
}

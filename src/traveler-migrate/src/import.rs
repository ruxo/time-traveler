//! `import <dump.json>`: the document the Node export printed at the cutover (see `Dump`), converted row by row through the records into the
//! domain and back (so a row that converts to no valid value is refused), inserted only when every row converts and
//! the database holds no `User` or `Admin`.

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

use std::path::Path;

use bson::doc;
use jiff::Timestamp;
use mongodb::Database;
use rzlib_error::{ErrorInfo, Outcome, errorinfo};
use serde::Deserialize;
use traveler::activities;
use traveler::errors::Code;
use traveler::model::{Admin, OwnedActivity, OwnedSession, User};
use traveler::storage::{
    ACTIVITY_COLLECTION, ADMIN_COLLECTION, ActivityRecord, AdminRecord, IdentityRecord, TIME_SESSION_COLLECTION, TimeSessionRecord, USER_COLLECTION, UserRecord,
};
use traveler::web::keys::activity_from_key;
use uuid::Uuid;

/// What the Node export printed at the cutover of 2026-11-09. The script, run inside the deployed Node app with
/// `erawan run time-traveler -- node -e`, was:
///
/// ```js
/// const pg = require('pg');
/// (async () => {
///   const p = new pg.Pool({ connectionString: process.env.DATABASE_URL, max: 1 });
///   const q = async (s) => (await p.query(s)).rows;
///   const d = {
///     issuer: (process.env.AUTH_ISSUER_URL || '').replace(/\/$/, ''),
///     users: await q('select id, oidc_sub, email, name, disabled_at, created_at from users order by created_at'),
///     activities: await q('select id, user_id, name, color, created_at, deleted_at from activities order by created_at'),
///     time_sessions: await q('select id, user_id, activity, started_at, ended_at, target_seconds from time_sessions order by started_at'),
///     admins: await q('select id, username, password_hash, created_at from admins order by created_at'),
///   };
///   await p.end();
///   process.stdout.write(JSON.stringify(d));
/// })();
/// ```
#[derive(Deserialize)]
pub struct Dump {
    pub issuer: String,
    pub users: Vec<UserRow>,
    pub activities: Vec<ActivityRow>,
    pub time_sessions: Vec<SessionRow>,
    pub admins: Vec<AdminRow>,
}

#[derive(Deserialize)]
pub struct UserRow {
    pub id: Uuid,
    pub oidc_sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub disabled_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

#[derive(Deserialize)]
pub struct ActivityRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub color: String,
    pub created_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

#[derive(Deserialize)]
pub struct SessionRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub activity: String,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    pub target_seconds: Option<i32>,
}

#[derive(Deserialize)]
pub struct AdminRow {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub created_at: Timestamp,
}

/// Every record of the dump, each proven to convert into the domain.
#[derive(Debug)]
pub struct Converted {
    pub users: Vec<UserRecord>,
    pub activities: Vec<ActivityRecord>,
    pub sessions: Vec<TimeSessionRecord>,
    pub admins: Vec<AdminRecord>,
}

fn refused(table: &str, id: Uuid, why: impl std::fmt::Display) -> String {
    format!("{table} {id}: {why}")
}

/// Converts the dump; `Err` lists every refused row.
pub fn convert(dump: &Dump) -> Result<Converted, Vec<String>> {
    let mut refusals = Vec::new();
    let mut converted = Converted {
        users: Vec::new(),
        activities: Vec::new(),
        sessions: Vec::new(),
        admins: Vec::new(),
    };
    for row in &dump.users {
        let identity = if row.oidc_sub == "dev:local" {
            IdentityRecord {
                issuer: "dev".to_owned(),
                subject: "local".to_owned(),
            }
        } else {
            IdentityRecord {
                issuer: dump.issuer.clone(),
                subject: row.oidc_sub.clone(),
            }
        };
        let record = UserRecord {
            id: row.id,
            identities: vec![identity],
            email: row.email.clone(),
            name: row.name.clone(),
            disabled_at: row.disabled_at,
            created_at: row.created_at,
            updated_at: row.created_at,
        };
        match User::try_from(record.clone()) {
            Ok(_) => converted.users.push(record),
            Err(e) => refusals.push(refused("users", row.id, e.message)),
        }
    }
    for row in &dump.activities {
        let record = ActivityRecord {
            id: row.id,
            user_id: row.user_id,
            name: row.name.clone(),
            color: row.color.clone(),
            icon: None,
            created_at: row.created_at,
            deleted_at: row.deleted_at,
        };
        match OwnedActivity::try_from(record.clone()) {
            Ok(_) => converted.activities.push(record),
            Err(e) => refusals.push(refused("activities", row.id, e.message)),
        }
    }
    for row in &dump.time_sessions {
        let Some(activity_id) = activity_from_key(&row.activity) else {
            refusals.push(refused(
                "time_sessions",
                row.id,
                format!("activity key {:?} is neither predefined nor c:<uuid>", row.activity),
            ));
            continue;
        };
        let record = TimeSessionRecord {
            id: row.id,
            user_id: row.user_id,
            activity_id,
            started_at: row.started_at,
            ended_at: row.ended_at,
            running: if row.ended_at.is_none() { Some(true) } else { None },
            target_seconds: row.target_seconds,
        };
        match OwnedSession::try_from(record.clone()) {
            Ok(_) => converted.sessions.push(record),
            Err(e) => refusals.push(refused("time_sessions", row.id, e.message)),
        }
    }
    for row in &dump.admins {
        let record = AdminRecord {
            id: row.id,
            username: row.username.clone(),
            password_hash: row.password_hash.clone(),
            created_at: row.created_at,
            updated_at: row.created_at,
        };
        match Admin::try_from(record.clone()) {
            Ok(_) => converted.admins.push(record),
            Err(e) => refusals.push(refused("admins", row.id, e.message)),
        }
    }
    if refusals.is_empty() { Ok(converted) } else { Err(refusals) }
}

pub async fn import(db: &Database, path: &Path) -> Outcome<()> {
    let text = std::fs::read_to_string(path).map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("reading {}: {e}", path.display())))?;
    let dump: Dump = serde_json::from_str(&text).map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("the dump is not the export's shape: {e}")))?;
    let converted = convert(&dump).map_err(|refusals| {
        for refusal in &refusals {
            eprintln!("refused: {refusal}");
        }
        errorinfo!(
            Code::SERVER_ERROR,
            format!("{} rows convert to no valid document; nothing was written", refusals.len())
        )
    })?;

    let fail = |what: &str, e: mongodb::error::Error| -> ErrorInfo { errorinfo!(Code::DATABASE, format!("{what}: {e}")) };
    let users = db.collection::<UserRecord>(USER_COLLECTION);
    let admins = db.collection::<AdminRecord>(ADMIN_COLLECTION);
    if users.count_documents(doc! {}).await.map_err(|e| fail("counting users", e))? > 0
        || admins.count_documents(doc! {}).await.map_err(|e| fail("counting admins", e))? > 0
    {
        return Err(errorinfo!(
            Code::SERVER_ERROR,
            "the database already holds a User or an Admin; import only into an empty one"
        ));
    }
    if !converted.users.is_empty() {
        users.insert_many(&converted.users).await.map_err(|e| fail("inserting users", e))?;
    }
    if !converted.activities.is_empty() {
        db.collection::<ActivityRecord>(ACTIVITY_COLLECTION)
            .insert_many(&converted.activities)
            .await
            .map_err(|e| fail("inserting activities", e))?;
    }
    if !converted.sessions.is_empty() {
        db.collection::<TimeSessionRecord>(TIME_SESSION_COLLECTION)
            .insert_many(&converted.sessions)
            .await
            .map_err(|e| fail("inserting sessions", e))?;
    }
    if !converted.admins.is_empty() {
        admins.insert_many(&converted.admins).await.map_err(|e| fail("inserting admins", e))?;
    }
    println!(
        "imported {} users, {} activities, {} sessions ({} running), {} admins",
        converted.users.len(),
        converted.activities.len(),
        converted.sessions.len(),
        converted.sessions.iter().filter(|s| s.running.is_some()).count(),
        converted.admins.len()
    );
    let _ = activities::PREDEFINED.len();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dump_converts_and_a_bad_row_refuses_the_whole() {
        let user = Uuid::new_v4();
        let dump: Dump = serde_json::from_value(serde_json::json!({
            "issuer": "https://auth.erawan.cloud",
            "users": [{ "id": user, "oidc_sub": "abc", "email": null, "name": "A", "disabled_at": null, "created_at": "2026-01-01T00:00:00.000Z" }],
            "activities": [{ "id": Uuid::new_v4(), "user_id": user, "name": "read", "color": "#3F7C85", "created_at": "2026-01-01T00:00:00.000Z", "deleted_at": null }],
            "time_sessions": [
                { "id": Uuid::new_v4(), "user_id": user, "activity": "sleep", "started_at": "2026-01-01T00:00:00.000Z", "ended_at": "2026-01-01T01:00:00.000Z", "target_seconds": 600 },
                { "id": Uuid::new_v4(), "user_id": user, "activity": "work", "started_at": "2026-01-02T00:00:00.000Z", "ended_at": null, "target_seconds": null }
            ],
            "admins": [{ "id": Uuid::new_v4(), "username": "root", "password_hash": "scrypt$16384$8$1$AAAAAAAAAAAAAAAAAAAAAA==$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "created_at": "2026-01-01T00:00:00.000Z" }]
        }))
        .unwrap();
        let converted = convert(&dump).expect("converts");
        assert_eq!(converted.sessions.iter().filter(|s| s.running == Some(true)).count(), 1);
        assert_eq!(converted.users[0].identities[0].issuer, "https://auth.erawan.cloud");

        let mut bad = dump;
        bad.time_sessions[0].activity = "hidden".to_owned();
        let refusals = convert(&bad).expect_err("refuses");
        assert_eq!(refusals.len(), 1);
        assert!(refusals[0].contains("hidden"));
    }
}

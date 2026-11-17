//! The hand-run tool (design section 9): `indexes` creates the catalogue, `check` proves the cluster runs a
//! transaction and the partial unique index, `import` loads the Node version's dump. Reads the same settings as the
//! service from the working directory; the Mongo URI never appears in its output.

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

mod import;

use std::future::IntoFuture;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bson::doc;
use clap::{Parser, Subcommand};
use rzlib_error::{Outcome, errorinfo};
use traveler::config::{Settings, environment};
use traveler::errors::Code;
use traveler::storage::{self, MongoStore, Store, TIME_SESSION_COLLECTION, TimeSessionRecord, create_indexes, stored_instant, uuid_bson};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "traveler-migrate", about = "Time Traveler's hand-run database tool")]
struct Cli {
    /// The directory holding appsettings.toml (default: the working directory).
    #[arg(long, default_value = ".")]
    settings: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create every index of the catalogue; idempotent.
    Indexes,
    /// Run one transaction, exercise the OneRunning index and the continue-running update against the real cluster.
    Check,
    /// Load the Node version's JSON dump into an empty database.
    Import { dump: PathBuf },
}

fn main() -> ExitCode {
    let _ = rustls::crypto::CryptoProvider::install_default(rustls::crypto::aws_lc_rs::default_provider());
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("traveler-migrate: cannot start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("traveler-migrate: {} ({})", error.message, error.code);
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Outcome<()> {
    let settings = Settings::load(&cli.settings, &environment())?;
    let (client, db) = traveler::service::mongo(&settings).await?;
    match cli.command {
        Command::Indexes => {
            for index in create_indexes(&db).await? {
                println!("index in place: {index}");
            }
            Ok(())
        }
        Command::Check => check(&client, &db).await,
        Command::Import { dump } => import::import(&db, Path::new(&dump)).await,
    }
}

/// One transaction (start, a read inside it, commit), two running inserts for one user, of which the second must
/// fail on `OneRunning`, and the continue-running update; every document is removed afterwards.
async fn check(client: &mongodb::Client, db: &mongodb::Database) -> Outcome<()> {
    check_cluster(client, db).await?;
    check_continue(client, db).await
}

/// `continue_running` computes the new target inside the server, in an update pipeline no in-process test runs:
/// a session started 100 s ago and continued with a 600 s target must come back with its id and start and a target
/// near 700 s; another activity must not match; no target must clear it.
async fn check_continue(client: &mongodb::Client, db: &mongodb::Database) -> Outcome<()> {
    let fail = |what: &str| errorinfo!(Code::DATABASE, format!("continue_running: {what}"));
    let store = MongoStore::new(client.clone(), db.clone());
    let user = Uuid::new_v4();
    let sleep = traveler::activities::predefined_id("sleep");
    let began = jiff::Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_secs(100))
        .map_err(|e| fail(&e.to_string()))?;
    let running = traveler::model::Running::new(Uuid::new_v4(), sleep, stored_instant(began), Some(60))?;
    store.insert_running(user, &running).await?;

    let now = stored_instant(jiff::Timestamp::now());
    let kept = store.continue_running(user, sleep, Some(600), now).await;
    let other = store.continue_running(user, traveler::activities::predefined_id("work"), None, now).await;
    let too_old = store.continue_running(user, sleep, Some(86_400), now).await;
    let cleared = store.continue_running(user, sleep, None, now).await;
    let cleanup = db
        .collection::<TimeSessionRecord>(TIME_SESSION_COLLECTION)
        .delete_many(doc! { "UserId": uuid_bson(user) })
        .await
        .map_err(|e| fail(&format!("the cleanup: {e}")))?;

    let kept = kept?.ok_or_else(|| fail("the running session of the same activity did not match"))?;
    if (kept.id, kept.started_at) != (running.id, running.started_at) {
        return Err(fail("the id or the start changed"));
    }
    match kept.target_seconds {
        Some(target) if (700..=702).contains(&target) => {}
        other => return Err(fail(&format!("the target is {other:?}, expected about 700"))),
    }
    if other?.is_some() {
        return Err(fail("a session of another activity matched"));
    }
    if too_old?.is_some() {
        return Err(fail("a target past the limit was accepted"));
    }
    match cleared? {
        Some(cleared) if cleared.target_seconds.is_none() => {}
        other => return Err(fail(&format!("no target did not clear it: {other:?}"))),
    }
    println!(
        "continue_running: keeps the session, counts the target from now, clears it ({} documents cleaned up)",
        cleanup.deleted_count
    );
    Ok(())
}

async fn check_cluster(client: &mongodb::Client, db: &mongodb::Database) -> Outcome<()> {
    let fail = |what: &str, e: mongodb::error::Error| errorinfo!(Code::DATABASE, format!("{what}: {e}"));
    let sessions = db.collection::<TimeSessionRecord>(TIME_SESSION_COLLECTION);

    let mut session = client.start_session().await.map_err(|e| fail("startSession", e))?;
    session.start_transaction().await.map_err(|e| fail("startTransaction", e))?;
    let _ = sessions
        .find_one(doc! {})
        .session(&mut session)
        .await
        .map_err(|e| fail("a read inside the transaction", e))?;
    session.commit_transaction().await.map_err(|e| fail("commitTransaction", e))?;
    println!("transaction: ok");

    let user = Uuid::new_v4();
    let first = traveler::model::Running::new(Uuid::new_v4(), traveler::activities::predefined_id("sleep"), jiff::Timestamp::now(), None)?;
    let second = traveler::model::Running::new(Uuid::new_v4(), traveler::activities::predefined_id("work"), jiff::Timestamp::now(), None)?;
    sessions
        .insert_one(TimeSessionRecord::running(user, &first))
        .await
        .map_err(|e| fail("the first running insert", e))?;
    let outcome = sessions.insert_one(TimeSessionRecord::running(user, &second)).into_future().await;
    let cleanup = sessions
        .delete_many(doc! { "UserId": uuid_bson(user) })
        .await
        .map_err(|e| fail("the cleanup", e))?;
    match outcome {
        Ok(_) => Err(errorinfo!(
            Code::DATABASE,
            format!(
                "OneRunning did not refuse a second running session; run `indexes` first ({} documents cleaned up)",
                cleanup.deleted_count
            )
        )),
        Err(e) if e.to_string().contains("E11000") || e.to_string().contains("duplicate key") => {
            println!("OneRunning: refuses a second running session ({} documents cleaned up)", cleanup.deleted_count);
            Ok(())
        }
        Err(e) => Err(fail("the second running insert failed for another reason", e)),
    }
}

#[allow(dead_code)]
fn unused(_: &storage::IndexSpec) {}

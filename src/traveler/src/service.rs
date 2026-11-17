//! The process lifecycle, in the library so `main.rs` stays thin: settings → Mongo client and ping → facade → the
//! setup link → the listener → the background index check → graceful shutdown on the signal.

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

use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;

use mongodb::options::ClientOptions;
use mongodb::{Client, Database};
use rzlib_error::{Outcome, errorinfo};
use tokio::net::TcpListener;

use crate::config::Settings;
use crate::errors::Code;
use crate::facade::Facade;
use crate::panic::caught;
use crate::storage::{MongoStore, Store, missing_indexes};
use crate::web::auth::OidcClient;
use crate::web::cookies::CookieCodec;
use crate::web::{AppState, WebSettings};

/// The driver's wait for a server, unless the URI sets one (bylaw 2).
pub const SERVER_SELECTION_TIMEOUT: Duration = Duration::from_secs(10);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The Mongo client and database for `settings`, with the timeouts of bylaw 2. Nothing is contacted yet.
pub async fn mongo(settings: &Settings) -> Outcome<(Client, Database)> {
    let uri = settings.connection_string();
    let mut options = ClientOptions::parse(uri).await.map_err(|e| {
        errorinfo!(
            Code::SERVER_ERROR,
            format!("mongo.connection-string is not a usable MongoDB URI: {e}"),
            "connecting"
        )
    })?;
    options.server_selection_timeout.get_or_insert(SERVER_SELECTION_TIMEOUT);
    options.connect_timeout.get_or_insert(CONNECT_TIMEOUT);
    let client = Client::with_options(options).map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("the MongoDB client: {e}"), "connecting"))?;
    let db = client.database(&settings.mongo.database);
    Ok((client, db))
}

/// Serves until `shutdown` completes, then drains.
pub async fn run(settings: Settings, shutdown: impl Future<Output = ()> + Send + 'static) -> Outcome<()> {
    let (client, db) = mongo(&settings).await?;
    let store = MongoStore::new(client, db.clone());
    store
        .ping()
        .await
        .map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("MongoDB does not answer a ping: {}", e.message), "starting"))?;
    tracing::info!(database = %settings.mongo.database, "MongoDB answers");

    let facade = Arc::new(Facade::new(store, settings.session.setup_hours));
    if let Some(token) = facade.issue_setup_token().await? {
        let base = settings
            .public_url()
            .unwrap_or_else(|| format!("http://localhost:{}", settings.listen_address().port()));
        tracing::info!(
            "[admin] One-time setup link (valid {}h): {base}/admin/#setup={token}",
            settings.session.setup_hours
        );
    }

    let oidc = match settings.oidc() {
        Some(oidc) => Some(Arc::new(OidcClient::new(oidc)?)),
        None => {
            tracing::warn!("auth is not configured: no sign-in but the development one, when enabled");
            None
        }
    };
    let state = AppState {
        facade,
        cookies: Arc::new(CookieCodec::new(settings.session_key())),
        oidc,
        web: Arc::new(WebSettings {
            dev_login: settings.auth.dev_login,
            public_url: settings.public_url(),
            member_days: settings.session.member_days,
            admin_hours: settings.session.admin_hours,
            oidc_minutes: settings.session.oidc_minutes,
        }),
    };
    let router = crate::web::router(state);

    let address = settings.listen_address();
    let listener = TcpListener::bind(address)
        .await
        .map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("cannot listen on {address}: {e}"), "starting"))?;
    tracing::info!(%address, version = env!("CARGO_PKG_VERSION"), "Time Traveler listening");

    tokio::spawn(report_indexes(db));

    caught(axum::serve(listener, router).with_graceful_shutdown(shutdown).into_future())
        .await
        .map_err(|panic| errorinfo!(Code::SERVER_ERROR, format!("the server panicked: {panic}"), "serving"))?
        .map_err(|e| errorinfo!(Code::SERVER_ERROR, format!("the server failed: {e}"), "serving"))?;
    tracing::info!("stopped");
    Ok(())
}

/// Once after startup: an Error per index not in place, naming the tool that creates it. Nothing is created here.
async fn report_indexes(db: Database) {
    match missing_indexes(&db).await {
        Ok(missing) if missing.is_empty() => tracing::info!("every index is in place"),
        Ok(missing) => {
            for index in missing {
                tracing::error!("{index} is absent: run `traveler-migrate indexes`");
            }
        }
        Err(error) => tracing::error!(code = %error.code, "the index check failed: {}", error.message),
    }
}

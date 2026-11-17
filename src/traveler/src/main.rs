//! The Time Traveler process: the crypto provider, the settings, the subscriber, the runtime, the service.
//! Settings are read from the working directory: `/app` in the image, `src/traveler` for a local run.

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
use std::process::ExitCode;

use traveler::config::{Settings, environment};
use traveler::service;

fn main() -> ExitCode {
    if rustls::crypto::CryptoProvider::install_default(rustls::crypto::aws_lc_rs::default_provider()).is_err()
        && rustls::crypto::CryptoProvider::get_default().is_none()
    {
        eprintln!("traveler: cannot install the TLS crypto provider");
        return ExitCode::FAILURE;
    }
    let settings = match Settings::load(Path::new("."), &environment()) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("traveler: {}", error.message);
            return ExitCode::FAILURE;
        }
    };
    let filter = tracing_subscriber::EnvFilter::try_new(&settings.logging.level).unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().json().with_env_filter(filter).init();

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("traveler: cannot start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(service::run(settings, shutdown_signal())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(code = %error.code, locations = ?error.locations, "startup failed: {}", error.message);
            ExitCode::FAILURE
        }
    }
}

/// Ctrl-C or, on Unix, SIGTERM: how Erawan stops the container.
async fn shutdown_signal() {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = terminate() => {}
    }
    tracing::info!("shutdown signal received");
}

#[cfg(unix)]
async fn terminate() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut signal) => {
            signal.recv().await;
        }
        Err(error) => {
            tracing::warn!(%error, "cannot listen for SIGTERM; Ctrl-C only");
            std::future::pending().await
        }
    }
}

#[cfg(not(unix))]
async fn terminate() {
    std::future::pending().await
}

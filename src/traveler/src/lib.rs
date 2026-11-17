//! Time Traveler's service as a library, so `traveler-migrate` shares its records and index catalogue and the
//! process shell in `main.rs` stays thin. Design: `docs/design/design.adoc`.

pub mod activities;
pub mod config;
pub mod errors;
pub mod facade;
pub mod model;
pub mod panic;
pub mod service;
pub mod storage;
pub mod web;

//! Courrier core library.
//!
//! This crate owns all the durable logic: database schema, IMAP fetcher,
//! mail parsing, search index, and analytics. It is consumed by both the
//! axum HTTP server and the Tauri desktop wrapper.

pub mod analytics;
pub mod database;
pub mod encryption;
pub mod fetcher;
pub mod mail;
pub mod providers;
pub mod search;
pub mod settings;
pub mod sync;

pub use database::Database;
pub use encryption::Encryptor;
pub use settings::Settings;

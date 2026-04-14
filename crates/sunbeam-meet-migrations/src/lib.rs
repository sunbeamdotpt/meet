//! `SQLx` migrations for sunbeam-meet.
//!
//! Embedded at compile time via [`sqlx::migrate!`]. Run with
//! [`MIGRATOR.run(&pool)`](sqlx::migrate::Migrator::run).

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

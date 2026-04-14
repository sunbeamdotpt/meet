//! sunbeam-meet service — library entry point.
//!
//! The binary target in `main.rs` wires these modules into an axum server.
//! Integration and unit tests import this crate to exercise individual
//! modules against real shared services.
//!
//! See [`crate::config`] for configuration, [`crate::handlers`] for RPC
//! implementations, [`crate::stream::join_room`] for the bidi fan-out hub.

pub mod authz;
pub mod cache;
pub mod clients;
pub mod config;
pub mod domain;
pub mod error;
pub mod events;
pub mod handlers;
pub mod metrics;
pub mod middleware;
pub mod state;
pub mod storage;
pub mod stream;
pub mod telemetry;
pub mod webhooks;

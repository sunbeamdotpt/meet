//! Postgres helpers: ID generation and small query utilities.
//!
//! Most CRUD is inlined in handlers to keep the surface readable. The
//! dedicated `rooms` submodule exposes a typed CRUD API used by the
//! integration test — production handlers still use inline sqlx where
//! convenient.

use uuid::Uuid;

pub mod rooms;

/// Generate a sortable UUID v7 with the current timestamp.
pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

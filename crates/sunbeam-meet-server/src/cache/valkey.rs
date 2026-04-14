//! Valkey (Redis-protocol) client for presence, session, rate-limit state.

use redis::aio::ConnectionManager;
use redis::AsyncCommands;

/// Thin wrapper over a shared, multiplexed Valkey connection.
#[derive(Clone)]
pub struct ValkeyClient {
    conn: ConnectionManager,
}

impl ValkeyClient {
    /// Connect to the given Valkey URL.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(Self { conn })
    }

    /// Set a participant's presence timestamp inside `presence:{room_id}`.
    pub async fn set_presence(
        &self,
        room_id: &str,
        identity: &str,
        ts_ms: i64,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        let key = format!("presence:{room_id}");
        let _: () = conn.hset(key, identity, ts_ms).await?;
        Ok(())
    }

    /// Remove a participant from `presence:{room_id}`.
    pub async fn clear_presence(&self, room_id: &str, identity: &str) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        let key = format!("presence:{room_id}");
        let _: () = conn.hdel(key, identity).await?;
        Ok(())
    }

    /// Sliding-window rate-limit counter. Returns the current count.
    pub async fn rate_incr(&self, identity: &str, rpc: &str, ttl_secs: u64) -> anyhow::Result<u64> {
        let mut conn = self.conn.clone();
        let key = format!("rate:{identity}:{rpc}");
        let count: u64 = conn.incr(&key, 1).await?;
        if count == 1 {
            let _: () = conn.expire(&key, ttl_secs as i64).await?;
        }
        Ok(count)
    }
}

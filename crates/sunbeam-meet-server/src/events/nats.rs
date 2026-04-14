//! NATS publisher/subscriber used for cross-instance fan-out.

use async_nats::Client;
use bytes::Bytes;
use futures::StreamExt;
use prost::Message;
use sunbeam_meet_proto::meet::v1::MeetServerMessage;

/// Shared NATS client.
#[derive(Clone)]
pub struct NatsPublisher {
    /// Underlying async-nats client.
    pub client: Client,
}

impl NatsPublisher {
    /// Connect to the given NATS URL.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let client = async_nats::connect(url).await?;
        Ok(Self { client })
    }

    /// Publish a raw byte payload to a subject.
    pub async fn publish(&self, subject: impl Into<String>, payload: Bytes) -> anyhow::Result<()> {
        self.client
            .publish(subject.into(), payload)
            .await
            .map_err(|e| anyhow::anyhow!("nats publish: {e}"))?;
        Ok(())
    }

    /// Subscribe to a subject and return the message stream.
    pub async fn subscribe(
        &self,
        subject: impl Into<String>,
    ) -> anyhow::Result<async_nats::Subscriber> {
        let sub = self
            .client
            .subscribe(subject.into())
            .await
            .map_err(|e| anyhow::anyhow!("nats subscribe: {e}"))?;
        Ok(sub)
    }

    /// Subject for a given room's fan-out bus.
    pub fn room_subject(room_id: &str) -> String {
        format!("meet.room.{room_id}")
    }
}

/// Test-facing publisher that encodes a `MeetServerMessage` before sending.
/// Wraps [`NatsPublisher`]; kept as a separate type so test and production
/// code paths stay distinguishable.
#[derive(Clone)]
pub struct Publisher {
    inner: NatsPublisher,
}

impl Publisher {
    /// Connect.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        Ok(Self {
            inner: NatsPublisher::connect(url).await?,
        })
    }

    /// Encode + publish.
    pub async fn publish(&self, subject: &str, msg: &MeetServerMessage) -> anyhow::Result<()> {
        self.inner
            .publish(subject.to_owned(), msg.encode_to_vec().into())
            .await
    }
}

/// Test-facing subscriber. Like [`Publisher`], but reading.
#[derive(Clone)]
pub struct Subscriber {
    inner: NatsPublisher,
}

impl Subscriber {
    /// Connect.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        Ok(Self {
            inner: NatsPublisher::connect(url).await?,
        })
    }

    /// Subscribe. The returned stream yields decoded `MeetServerMessage`s.
    pub async fn subscribe(&self, subject: &str) -> anyhow::Result<MessageStream> {
        Ok(MessageStream {
            sub: self.inner.subscribe(subject.to_owned()).await?,
        })
    }
}

/// Stream of decoded `MeetServerMessage` frames.
pub struct MessageStream {
    sub: async_nats::Subscriber,
}

impl MessageStream {
    /// Await the next decoded message. `None` indicates the subscription
    /// closed. Each decode error is surfaced on its own poll.
    pub async fn next(&mut self) -> Option<anyhow::Result<MeetServerMessage>> {
        let msg = self.sub.next().await?;
        Some(
            MeetServerMessage::decode(msg.payload.as_ref())
                .map_err(|e| anyhow::anyhow!("decode: {e}")),
        )
    }
}

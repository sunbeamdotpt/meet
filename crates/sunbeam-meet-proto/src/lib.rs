//! Generated protobuf + tonic bindings for the sunbeam-meet service surface.
//!
//! - [`meet::v1`] — `sunbeam.meet.v1.MeetService` (client-facing RPCs).
//! - [`agent::v1`] — `sunbeam.agent.v1.AgentWorker` (called by sunbeam-meet,
//!   implemented by agent workers) and `sunbeam.agent.v1.AgentCallback`
//!   (implemented by sunbeam-meet, called by workers).

#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]

pub mod meet {
    pub mod v1 {
        tonic::include_proto!("sunbeam.meet.v1");
    }
}

pub mod agent {
    pub mod v1 {
        tonic::include_proto!("sunbeam.agent.v1");
    }
}

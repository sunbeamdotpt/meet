//! Unit test — OpenAPI-style error payload shape.
//!
//! The service renders a JSON error body for REST / Connect-JSON clients.
//! Pin the shape with insta so any field rename or code-string drift is
//! caught at review time, not by a frontend consumer.

mod common;

use sunbeam_meet_server::error::{ErrorBody, MeetError};

fn bodies() -> Vec<(&'static str, ErrorBody)> {
    vec![
        (
            "not_found",
            ErrorBody::from(&MeetError::NotFound("room 'x'".into())),
        ),
        (
            "permission_denied",
            ErrorBody::from(&MeetError::PermissionDenied("not owner".into())),
        ),
        (
            "invalid_argument",
            ErrorBody::from(&MeetError::InvalidArgument("bad slug".into())),
        ),
        ("backpressure", ErrorBody::from(&MeetError::Backpressure)),
        (
            "unauthenticated",
            ErrorBody::from(&MeetError::Unauthenticated),
        ),
    ]
}

#[test]
fn snapshot_error_payload_shape() {
    let rendered: Vec<(String, serde_json::Value)> = bodies()
        .into_iter()
        .map(|(name, body)| {
            (
                name.to_string(),
                serde_json::to_value(body).expect("serialize"),
            )
        })
        .collect();
    insta::assert_json_snapshot!("meet_error_json_payloads", rendered);
}

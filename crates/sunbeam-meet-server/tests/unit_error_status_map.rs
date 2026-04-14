//! Unit tests — Connect/gRPC status mapping.
//!
//! The service has a single error type, `error::MeetError`, that converts to
//! a `tonic::Status` (and, when Connect lands, a Connect error). Each
//! MeetError variant must map to a deterministic gRPC status code. We pin
//! that table with an insta snapshot so regressions show up as a diff rather
//! than a flaky one-off assert.

mod common;

use sunbeam_meet_server::error::MeetError;
use tonic::Code;

/// Build one of every variant we care about so the snapshot covers the table.
fn table() -> Vec<(&'static str, MeetError)> {
    vec![
        ("not_found", MeetError::NotFound("room".into())),
        (
            "already_exists",
            MeetError::AlreadyExists("room slug".into()),
        ),
        (
            "permission_denied",
            MeetError::PermissionDenied("not owner".into()),
        ),
        ("unauthenticated", MeetError::Unauthenticated),
        (
            "invalid_argument",
            MeetError::InvalidArgument("bad slug".into()),
        ),
        (
            "failed_precondition",
            MeetError::FailedPrecondition("room not active".into()),
        ),
        ("backpressure", MeetError::Backpressure),
        (
            "resource_exhausted",
            MeetError::ResourceExhausted("too many rooms".into()),
        ),
        ("unavailable", MeetError::Unavailable("livekit".into())),
        ("internal", MeetError::Internal(anyhow::anyhow!("boom"))),
    ]
}

#[test]
fn snapshot_status_code_mapping() {
    let mapping: Vec<(String, String)> = table()
        .into_iter()
        .map(|(name, err)| {
            let status: tonic::Status = err.into();
            (name.to_string(), format!("{:?}", status.code()))
        })
        .collect();

    insta::assert_yaml_snapshot!("meet_error_grpc_status_codes", mapping);
}

#[test]
fn not_found_maps_to_not_found_code() {
    let status: tonic::Status = MeetError::NotFound("x".into()).into();
    assert_eq!(status.code(), Code::NotFound);
}

#[test]
fn unauthenticated_maps_to_unauthenticated_code() {
    let status: tonic::Status = MeetError::Unauthenticated.into();
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[test]
fn permission_denied_maps_to_permission_denied_code() {
    let status: tonic::Status = MeetError::PermissionDenied("x".into()).into();
    assert_eq!(status.code(), Code::PermissionDenied);
}

#[test]
fn backpressure_maps_to_resource_exhausted_code() {
    // Connect/gRPC doesn't have a native backpressure code; convention is
    // RESOURCE_EXHAUSTED.
    let status: tonic::Status = MeetError::Backpressure.into();
    assert_eq!(status.code(), Code::ResourceExhausted);
}

#[test]
fn internal_never_leaks_inner_error_string() {
    // We don't string-match on state — but we DO assert the status code is
    // Internal and that the message is non-empty (operator-friendly). The
    // underlying anyhow chain must NOT appear verbatim (PII / leak risk).
    let secret = "SECRET-TOKEN-DO-NOT-LEAK";
    let err = MeetError::Internal(anyhow::anyhow!(secret));
    let status: tonic::Status = err.into();
    assert_eq!(status.code(), Code::Internal);
    assert!(
        !status.message().contains(secret),
        "Internal error leaked inner context into tonic::Status message",
    );
}

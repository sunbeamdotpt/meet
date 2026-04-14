//! Unit tests — LiveKit webhook JWT verification.
//!
//! Mock-free. Builds JWTs with the `jsonwebtoken` crate that the production
//! code uses, then feeds them to the verifier and asserts on the structured
//! result variant — never on the error message text.
//!
//! Implementer exposes `webhooks::livekit::verify_webhook_jwt(token, secret)
//! -> Result<WebhookClaims, WebhookAuthError>`.

mod common;

use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use sunbeam_meet_server::webhooks::livekit as hook;

#[derive(Serialize, Deserialize, Clone)]
struct TestClaims {
    iss: String,
    iat: u64,
    exp: u64,
    // LiveKit embeds the event payload hash as `sha256`; the verifier checks
    // the header signature, not the hash itself (hash is checked elsewhere
    // against the body).
    sha256: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn sign(secret: &str, claims: &TestClaims) -> String {
    encode(
        &Header::new(jsonwebtoken::Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("signing test JWT")
}

#[test]
fn verifier_accepts_fresh_valid_token() {
    let secret = "test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let claims = TestClaims {
        iss: "APIkey".into(),
        iat: now(),
        exp: now() + 60,
        sha256: "deadbeef".into(),
    };
    let token = sign(secret, &claims);
    let got = hook::verify_webhook_jwt(&token, secret).expect("valid JWT must verify");
    assert_eq!(got.iss, "APIkey");
    assert_eq!(got.sha256, "deadbeef");
}

#[test]
fn verifier_rejects_bad_signature() {
    let claims = TestClaims {
        iss: "APIkey".into(),
        iat: now(),
        exp: now() + 60,
        sha256: "deadbeef".into(),
    };
    let token = sign("wrong-secret-aaaaaaaaaaaaaaaaaaaaaaaaa", &claims);
    let err = hook::verify_webhook_jwt(&token, "real-secret-aaaaaaaaaaaaaaaaaaaaaaaa")
        .expect_err("tampered-signature token must reject");
    assert!(
        matches!(err, hook::WebhookAuthError::BadSignature),
        "got {err:?}",
    );
}

#[test]
fn verifier_rejects_stale_iat() {
    let secret = "test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaa";
    // iat 10 minutes in the past — LiveKit webhooks should arrive within
    // seconds; a 10-minute-old iat is a replay attack signal.
    let claims = TestClaims {
        iss: "APIkey".into(),
        iat: now() - 600,
        exp: now() + 60,
        sha256: "deadbeef".into(),
    };
    let token = sign(secret, &claims);
    let err = hook::verify_webhook_jwt(&token, secret).expect_err("stale iat must reject");
    assert!(
        matches!(err, hook::WebhookAuthError::StaleIat { .. }),
        "got {err:?}"
    );
}

#[test]
fn verifier_rejects_expired() {
    let secret = "test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let claims = TestClaims {
        iss: "APIkey".into(),
        iat: now() - 120,
        exp: now() - 60,
        sha256: "deadbeef".into(),
    };
    let token = sign(secret, &claims);
    let err = hook::verify_webhook_jwt(&token, secret).expect_err("expired token must reject");
    assert!(
        matches!(err, hook::WebhookAuthError::Expired { .. }),
        "got {err:?}"
    );
}

#[test]
fn verifier_rejects_tampered_payload() {
    let secret = "test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let claims = TestClaims {
        iss: "APIkey".into(),
        iat: now(),
        exp: now() + 60,
        sha256: "deadbeef".into(),
    };
    let token = sign(secret, &claims);
    // Flip a char in the payload section (middle segment of the JWT).
    let first_dot = token.find('.').unwrap();
    let second_dot = token[first_dot + 1..].find('.').unwrap() + first_dot + 1;
    let mid = (first_dot + second_dot) / 2;
    // Rebuild the token via byte vec to avoid `unsafe { as_bytes_mut }`.
    // JWT segments are base64url ASCII, so byte-level edits stay valid UTF-8.
    let mut bytes = token.into_bytes();
    bytes[mid] = if bytes[mid] == b'A' { b'B' } else { b'A' };
    let token = String::from_utf8(bytes).expect("base64url stays valid utf-8");

    let err = hook::verify_webhook_jwt(&token, secret).expect_err("payload tamper must reject");
    // Tampering the payload invalidates the HMAC → BadSignature or Malformed.
    assert!(
        matches!(
            err,
            hook::WebhookAuthError::BadSignature | hook::WebhookAuthError::Malformed(_),
        ),
        "got {err:?}",
    );
}

// ── Proptest: verifier never panics on garbage input ───────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn verifier_never_panics_on_garbage(input in ".{0,512}") {
        let _ = hook::verify_webhook_jwt(&input, "some-secret-aaaaaaaaaaaaaaaaaa");
    }
}

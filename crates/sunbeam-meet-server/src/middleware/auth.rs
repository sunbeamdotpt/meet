//! Kratos-backed identity resolution for incoming RPCs.

use crate::clients::kratos::Identity;
use crate::error::{Error, Result};
use crate::state::SharedState;

/// Extract the `Authorization` or `Cookie` header from a tonic request.
pub fn extract_auth<T>(req: &tonic::Request<T>) -> Result<String> {
    let md = req.metadata();
    md.get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| {
            md.get("cookie")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        })
        .ok_or(Error::Unauthenticated)
}

/// Resolve a header to an `Identity`.
pub async fn identity_from_header(state: &SharedState, header: &str) -> Result<Identity> {
    state.kratos.whoami(header).await
}

/// Convenience: extract header from request and resolve identity.
pub async fn identity_from_request<T>(
    state: &SharedState,
    req: &tonic::Request<T>,
) -> Result<Identity> {
    let header = extract_auth(req)?;
    identity_from_header(state, &header).await
}

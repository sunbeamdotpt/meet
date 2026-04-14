//! Authorization helper — wraps Keto relation-tuple checks.

use crate::clients::kratos::Identity;
use crate::error::{Error, Result};
use crate::state::SharedState;

/// Check whether `identity` has `relation` on `(namespace, object)`.
///
/// Returns `Err(PermissionDenied)` if the check says no.
pub async fn can(
    state: &SharedState,
    identity: &Identity,
    namespace: &str,
    object: &str,
    relation: &str,
) -> Result<()> {
    let allowed = state
        .keto
        .check(namespace, object, relation, &identity.id)
        .await?;
    if !allowed {
        return Err(Error::PermissionDenied(format!(
            "{}:{}#{} for {}",
            namespace, object, relation, identity.id
        )));
    }
    Ok(())
}

/// Grant a relation tuple.
pub async fn grant(
    state: &SharedState,
    namespace: &str,
    object: &str,
    relation: &str,
    subject_id: &str,
) -> Result<()> {
    state
        .keto
        .grant(namespace, object, relation, subject_id)
        .await
}

//! Participant entity (history row).

use sunbeam_meet_proto::meet::v1::ParticipantRole;

/// Participant role stored as lowercase string.
pub const ROLE_VIEWER: &str = "viewer";
/// Member role.
pub const ROLE_MEMBER: &str = "member";
/// Admin role.
pub const ROLE_ADMIN: &str = "admin";
/// Owner role.
pub const ROLE_OWNER: &str = "owner";

/// Numeric rank: higher rank = more privilege.
fn rank(r: ParticipantRole) -> u8 {
    match r {
        ParticipantRole::Unspecified | ParticipantRole::Viewer => 1,
        ParticipantRole::Member => 2,
        ParticipantRole::Admin => 3,
        ParticipantRole::Owner => 4,
    }
}

/// Can an actor of role `from` be transitioned to role `to`?
///
/// DESIGN §6: a role may never promote at or above its own rank — only demote
/// or hold. Owner is absolute and may set anything.
#[must_use]
pub fn can_transition(from: ParticipantRole, to: ParticipantRole) -> bool {
    if matches!(from, ParticipantRole::Owner) {
        return true;
    }
    // A non-owner `from` may only produce `to` strictly below its own rank.
    rank(to) < rank(from)
}

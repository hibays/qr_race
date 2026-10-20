//! Who may do what inside a room.
//!
//! The room role is not the server role: an operator (admin) is not a room
//! participant, and an upload privilege is not a room ability.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomRole {
    Host,
    CoHost,
    Member,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoomAbility {
    /// Scan and publish a relay into the room. Members only receive relays.
    PublishRelay,
    /// Grant or revoke co-host status.
    ManageCoHosts,
    /// See who is in the room.
    SeeMembers,
}

impl RoomRole {
    pub const fn allows(self, ability: RoomAbility) -> bool {
        use RoomAbility::*;
        match self {
            RoomRole::Host => matches!(ability, PublishRelay | ManageCoHosts | SeeMembers),
            RoomRole::CoHost => matches!(ability, PublishRelay | SeeMembers),
            RoomRole::Member => matches!(ability, SeeMembers),
        }
    }
}

/// Everything a caller may do, across the room and the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Privileges {
    pub room: Option<RoomRole>,
    pub admin: bool,
}

impl Privileges {
    pub const fn member() -> Self {
        Self {
            room: Some(RoomRole::Member),
            admin: false,
        }
    }

    pub const fn host() -> Self {
        Self {
            room: Some(RoomRole::Host),
            admin: false,
        }
    }

    pub const fn co_host() -> Self {
        Self {
            room: Some(RoomRole::CoHost),
            admin: false,
        }
    }

    pub const fn admin() -> Self {
        Self {
            room: None,
            admin: true,
        }
    }

    pub fn can(&self, ability: RoomAbility) -> bool {
        self.room.is_some_and(|role| role.allows(ability))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_host_and_co_hosts_publish_relays() {
        assert!(RoomRole::Host.allows(RoomAbility::PublishRelay));
        assert!(RoomRole::CoHost.allows(RoomAbility::PublishRelay));
        assert!(!RoomRole::Member.allows(RoomAbility::PublishRelay));
    }

    #[test]
    fn only_the_host_manages_co_hosts() {
        assert!(RoomRole::Host.allows(RoomAbility::ManageCoHosts));
        assert!(!RoomRole::CoHost.allows(RoomAbility::ManageCoHosts));
        assert!(!RoomRole::Member.allows(RoomAbility::ManageCoHosts));
    }

    #[test]
    fn everyone_in_the_room_sees_members() {
        for role in [RoomRole::Host, RoomRole::CoHost, RoomRole::Member] {
            assert!(role.allows(RoomAbility::SeeMembers));
        }
    }

    #[test]
    fn admin_is_separate_from_room_roles() {
        assert!(!Privileges::admin().can(RoomAbility::PublishRelay));
    }
}

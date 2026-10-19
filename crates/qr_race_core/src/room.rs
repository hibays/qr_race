//! Room state and membership transitions.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::address::RoomAddress;
use crate::error::CoreError;
use crate::relay::UserId;
use crate::role::RoomRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomState {
    /// In the fixed inventory, not visible in the lobby.
    Dark,
    /// Activated ("lit"); visible in the lobby.
    Lit,
}

/// An opaque join-secret hash. Hashing and verification belong to the server;
/// the domain model only records that a join password exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretHash(String);

impl SecretHash {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub address: RoomAddress,
    pub host: UserId,
    pub co_hosts: BTreeSet<UserId>,
    pub members: BTreeSet<UserId>,
    pub state: RoomState,
    pub join_secret: Option<SecretHash>,
    pub modules: Vec<String>,
}

impl Room {
    pub fn new(address: RoomAddress, host: UserId) -> Self {
        Self {
            address,
            host,
            co_hosts: BTreeSet::new(),
            members: BTreeSet::new(),
            state: RoomState::Dark,
            join_secret: None,
            modules: Vec::new(),
        }
    }

    /// Lights a dark room. A lit room stays lit; relighting is a programming
    /// error rather than an idempotent no-op.
    pub fn light(&mut self) -> Result<(), CoreError> {
        match self.state {
            RoomState::Lit => Err(CoreError::AlreadyLit),
            RoomState::Dark => {
                self.state = RoomState::Lit;
                Ok(())
            }
        }
    }

    pub fn is_lit(&self) -> bool {
        self.state == RoomState::Lit
    }

    pub fn role_of(&self, user: &UserId) -> Option<RoomRole> {
        if &self.host == user {
            Some(RoomRole::Host)
        } else if self.co_hosts.contains(user) {
            Some(RoomRole::CoHost)
        } else if self.members.contains(user) {
            Some(RoomRole::Member)
        } else {
            None
        }
    }

    /// Joining is idempotent: an existing participant keeps their role.
    pub fn join(&mut self, user: UserId) -> RoomRole {
        if let Some(role) = self.role_of(&user) {
            return role;
        }
        self.members.insert(user);
        RoomRole::Member
    }

    pub fn leave(&mut self, user: &UserId) -> Result<(), CoreError> {
        if &self.host == user {
            return Err(CoreError::HostCannotLeave);
        }
        self.co_hosts.remove(user);
        self.members.remove(user);
        Ok(())
    }

    pub fn promote(&mut self, actor: &UserId, target: &UserId) -> Result<(), CoreError> {
        self.require_host(actor, "promote a co-host")?;
        if target == &self.host {
            return Err(CoreError::AlreadyHost);
        }
        if self.co_hosts.contains(target) {
            return Err(CoreError::AlreadyCoHost);
        }
        if !self.members.remove(target) {
            return Err(CoreError::NotAMember);
        }
        self.co_hosts.insert(target.clone());
        Ok(())
    }

    pub fn demote(&mut self, actor: &UserId, target: &UserId) -> Result<(), CoreError> {
        self.require_host(actor, "revoke a co-host")?;
        if !self.co_hosts.remove(target) {
            return Err(CoreError::NotACoHost);
        }
        self.members.insert(target.clone());
        Ok(())
    }

    pub fn set_join_secret(
        &mut self,
        actor: &UserId,
        secret: Option<SecretHash>,
    ) -> Result<(), CoreError> {
        self.require_host(actor, "change the join password")?;
        self.join_secret = secret;
        Ok(())
    }

    /// Replaces the room's module selection. Only the host configures modules.
    pub fn set_modules(&mut self, actor: &UserId, modules: Vec<String>) -> Result<(), CoreError> {
        self.require_host(actor, "configure room modules")?;
        let mut unique = modules;
        unique.sort();
        unique.dedup();
        self.modules = unique;
        Ok(())
    }

    /// The users allowed to scan and publish relays.
    pub fn scanners(&self) -> impl Iterator<Item = &UserId> {
        std::iter::once(&self.host).chain(self.co_hosts.iter())
    }

    pub fn is_scanner(&self, user: &UserId) -> bool {
        matches!(self.role_of(user), Some(RoomRole::Host | RoomRole::CoHost))
    }

    fn require_host(&self, actor: &UserId, action: &'static str) -> Result<(), CoreError> {
        if &self.host == actor {
            Ok(())
        } else {
            Err(CoreError::NotHost { action })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Capital;

    fn address() -> RoomAddress {
        RoomAddress::new(Capital::Tokyo, "A", "1B", "103").unwrap()
    }

    fn host() -> UserId {
        UserId("host-a".into())
    }

    fn member(name: &str) -> UserId {
        UserId(name.into())
    }

    fn room() -> Room {
        Room::new(address(), host())
    }

    #[test]
    fn rooms_start_dark_and_light_once() {
        let mut room = room();
        assert!(!room.is_lit());
        room.light().unwrap();
        assert!(room.is_lit());
        assert_eq!(room.light(), Err(CoreError::AlreadyLit));
    }

    #[test]
    fn the_host_is_a_participant_without_joining() {
        let room = room();
        assert_eq!(room.role_of(&host()), Some(RoomRole::Host));
        assert_eq!(room.role_of(&member("m-1")), None);
        assert!(room.is_scanner(&host()));
    }

    #[test]
    fn joining_is_idempotent() {
        let mut room = room();
        assert_eq!(room.join(member("m-1")), RoomRole::Member);
        assert_eq!(room.join(member("m-1")), RoomRole::Member);
        assert_eq!(room.members.len(), 1);
    }

    #[test]
    fn only_the_host_promotes_and_demotes() {
        let mut room = room();
        let m1 = member("m-1");
        room.join(m1.clone());

        assert_eq!(
            room.promote(&m1, &member("m-2")),
            Err(CoreError::NotHost {
                action: "promote a co-host"
            })
        );

        room.promote(&host(), &m1).unwrap();
        assert_eq!(room.role_of(&m1), Some(RoomRole::CoHost));
        assert!(room.is_scanner(&m1));

        room.demote(&host(), &m1).unwrap();
        assert_eq!(room.role_of(&m1), Some(RoomRole::Member));
        assert!(!room.is_scanner(&m1));
    }

    #[test]
    fn promoting_a_stranger_or_the_host_fails() {
        let mut room = room();
        let stranger = member("m-2");
        assert_eq!(room.promote(&host(), &stranger), Err(CoreError::NotAMember));
        assert_eq!(room.promote(&host(), &host()), Err(CoreError::AlreadyHost));

        room.join(stranger.clone());
        room.promote(&host(), &stranger).unwrap();
        assert_eq!(
            room.promote(&host(), &stranger),
            Err(CoreError::AlreadyCoHost)
        );
    }

    #[test]
    fn the_host_cannot_leave_but_members_can() {
        let mut room = room();
        let m1 = member("m-1");
        room.join(m1.clone());
        room.promote(&host(), &m1).unwrap();

        assert_eq!(room.leave(&host()), Err(CoreError::HostCannotLeave));
        room.leave(&m1).unwrap();
        assert_eq!(room.role_of(&m1), None);
        assert_eq!(room.co_hosts.len(), 0);
    }

    #[test]
    fn only_the_host_sets_the_join_password() {
        let mut room = room();
        let m1 = member("m-1");
        room.join(m1.clone());

        let denied = room.set_join_secret(&m1, Some(SecretHash::new("h")));
        assert_eq!(
            denied,
            Err(CoreError::NotHost {
                action: "change the join password"
            })
        );

        room.set_join_secret(&host(), Some(SecretHash::new("h")))
            .unwrap();
        assert!(room.join_secret.is_some());
        room.set_join_secret(&host(), None).unwrap();
        assert!(room.join_secret.is_none());
    }

    #[test]
    fn modules_are_host_configured_and_deduplicated() {
        let mut room = room();
        let m1 = member("m-1");
        room.join(m1.clone());

        assert_eq!(
            room.set_modules(&m1, vec!["a".into()]),
            Err(CoreError::NotHost {
                action: "configure room modules"
            })
        );

        room.set_modules(&host(), vec!["b".into(), "a".into(), "a".into()])
            .unwrap();
        assert_eq!(room.modules, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn scanners_are_host_and_co_hosts() {
        let mut room = room();
        let m1 = member("m-1");
        let m2 = member("m-2");
        room.join(m1.clone());
        room.join(m2.clone());
        room.promote(&host(), &m1).unwrap();

        let scanners: Vec<_> = room.scanners().cloned().collect();
        assert_eq!(scanners, vec![host(), m1]);
        assert!(!room.is_scanner(&m2));
    }
}

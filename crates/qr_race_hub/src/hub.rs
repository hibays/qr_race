//! Rooms, relays and the events a transport broadcasts.
//!
//! This layer owns the product rules and nothing else: no sockets, no HTTP, no
//! JSON. Every mutation returns the events it caused so the transport can fan
//! them out to the room without the hub knowing what a connection is.

use std::collections::BTreeMap;
use std::sync::Arc;

use qr_race_core::{
    Compression, CoreError, PayloadKind, RelayEntry, RelayId, Room, RoomAddress, RoomRole, UserId,
    content_hash,
};
use qr_race_store::{Store, StoreError};
use thiserror::Error;

use crate::clock::Clock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomEvent {
    RoomCreated {
        address: RoomAddress,
        host: UserId,
    },
    RoomLit {
        address: RoomAddress,
    },
    MemberJoined {
        address: RoomAddress,
        user: UserId,
        role: RoomRole,
    },
    MemberLeft {
        address: RoomAddress,
        user: UserId,
    },
    RoleChanged {
        address: RoomAddress,
        user: UserId,
        role: RoomRole,
    },
    RelayPublished {
        address: RoomAddress,
        relay: RelayEntry,
    },
    RelaySeen {
        address: RoomAddress,
        relay: RelayId,
        created_at_ms: u64,
    },
}

/// What a publisher uploads. The hub assigns the id and the timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRequest {
    pub kind: PayloadKind,
    pub compression: Compression,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberView {
    pub user: UserId,
    pub role: RoomRole,
    /// The newest relay this member published, if any. This is what the client
    /// highlights so others can tap the avatar and relay onward.
    pub last_relay: Option<RelayId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomView {
    pub address: RoomAddress,
    pub lit: bool,
    pub host: UserId,
    pub members: Vec<MemberView>,
    pub relays: Vec<RelayEntry>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HubError {
    #[error("room {0} already exists")]
    RoomExists(RoomAddress),

    #[error("room {0} does not exist")]
    RoomNotFound(RoomAddress),

    #[error("room {0} is not lit yet")]
    RoomNotLit(RoomAddress),

    #[error("only the host or a co-host may publish a relay")]
    NotAScanner,

    #[error("user is not a participant of this room")]
    NotAParticipant,

    #[error("invalid pairing PIN: {0}")]
    InvalidPin(String),

    #[error("unknown relay hash: {0}")]
    RelayNotFound(String),

    #[error("too many active rooms")]
    TooManyRooms,

    #[error("storage: {0}")]
    Storage(String),

    #[error(transparent)]
    Core(#[from] CoreError),
}

pub struct Hub {
    rooms: BTreeMap<RoomAddress, Room>,
    relays: BTreeMap<RoomAddress, Vec<RelayEntry>>,
    relay_hashes: BTreeMap<RoomAddress, BTreeMap<String, RelayId>>,
    epochs: BTreeMap<RoomAddress, u64>,
    next_epoch: u64,
    clock: Arc<dyn Clock>,
    next_relay: u64,
    store: Option<Arc<Store>>,
    pins: BTreeMap<String, RoomAddress>,
}

/// Relays are kept in memory, so a room keeps its newest entries and drops the
/// oldest beyond this bound. Members re-fetch after a gap (see the WS contract).
pub const MAX_RELAYS_PER_ROOM: usize = 64;
/// Hard cap on the uploaded payload bytes kept for one room.
pub const MAX_RELAY_BYTES_PER_ROOM: usize = 256 * 1024;
/// Hard cap on rooms held in memory. Empty rooms close with their last socket.
pub const MAX_ACTIVE_ROOMS: usize = 512;
/// Relays disappear from the room view and storage after this age.
pub const RELAY_TTL_MS: u64 = 40_000;

impl Hub {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            rooms: BTreeMap::new(),
            relays: BTreeMap::new(),
            relay_hashes: BTreeMap::new(),
            epochs: BTreeMap::new(),
            next_epoch: 0,
            clock,
            next_relay: 0,
            store: None,
            pins: BTreeMap::new(),
        }
    }

    /// Rebuilds persisted rooms, roles and PINs. Active codes are memory-only:
    /// after a restart they arrive again through client heartbeats.
    pub fn with_store(clock: Arc<dyn Clock>, store: Arc<Store>) -> Result<Self, HubError> {
        // Codes are active memory state; purge any rows left by older builds.
        store.clear_relays().map_err(storage_error)?;
        let rooms: BTreeMap<RoomAddress, Room> = store
            .load_rooms()
            .map_err(storage_error)?
            .into_iter()
            .map(|room| (room.address.clone(), room))
            .collect();
        let pins = store
            .room_pins()
            .map_err(storage_error)?
            .into_iter()
            .collect();
        let mut epochs = BTreeMap::new();
        for address in rooms.keys() {
            let epoch = epochs.len() as u64 + 1;
            epochs.insert(address.clone(), epoch);
        }
        let next_epoch = epochs.len() as u64;

        Ok(Self {
            rooms,
            relays: BTreeMap::new(),
            relay_hashes: BTreeMap::new(),
            epochs,
            next_epoch,
            clock,
            next_relay: 0,
            store: Some(store),
            pins,
        })
    }

    fn persist_room(&self, address: &RoomAddress) -> Result<(), HubError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let Some(room) = self.rooms.get(address) else {
            return Ok(());
        };
        store
            .upsert_room(room, self.clock.now_ms())
            .map_err(storage_error)
    }

    /// Creates a dark room in the fixed inventory. Lighting it is separate.
    pub fn create_room(
        &mut self,
        address: RoomAddress,
        host: UserId,
    ) -> Result<Vec<RoomEvent>, HubError> {
        if self.rooms.contains_key(&address) {
            return Err(HubError::RoomExists(address));
        }
        if self.rooms.len() >= MAX_ACTIVE_ROOMS {
            return Err(HubError::TooManyRooms);
        }
        self.rooms
            .insert(address.clone(), Room::new(address.clone(), host.clone()));
        self.next_epoch = self.next_epoch.saturating_add(1);
        self.epochs.insert(address.clone(), self.next_epoch);
        self.persist_room(&address)?;
        Ok(vec![RoomEvent::RoomCreated { address, host }])
    }

    /// Sets the four-digit pairing PIN. Only the host may change it.
    pub fn set_pin(
        &mut self,
        actor: &UserId,
        address: &RoomAddress,
        pin: &str,
    ) -> Result<(), HubError> {
        if pin.len() != 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
            return Err(HubError::InvalidPin("PIN must be four digits".into()));
        }
        {
            let room = self.room_mut(address)?;
            if room.host != *actor {
                return Err(CoreError::NotHost {
                    action: "change the pairing PIN",
                }
                .into());
            }
        }
        if let Some(existing) = self.pins.get(pin)
            && existing != address
        {
            return Err(HubError::InvalidPin("PIN is already in use".into()));
        }
        self.pins.retain(|_, existing| existing != address);
        self.pins.insert(pin.to_string(), address.clone());
        if let Some(store) = &self.store {
            store
                .set_room_pin(pin, address, self.clock.now_ms())
                .map_err(storage_error)?;
        }
        Ok(())
    }

    pub fn resolve_pin(&self, pin: &str) -> Option<RoomAddress> {
        self.pins
            .get(pin)
            .filter(|address| self.rooms.contains_key(*address))
            .cloned()
    }

    pub fn room_exists(&self, address: &RoomAddress) -> bool {
        self.rooms.contains_key(address)
    }

    /// Identifies one incarnation of a room address. Old event sockets carry
    /// the old epoch and must not close a newly recreated room.
    pub fn room_epoch(&self, address: &RoomAddress) -> Option<u64> {
        self.epochs.get(address).copied()
    }

    /// Removes a room and everything tied to its address. Called when the last
    /// live event socket for the room closes.
    pub fn close_room(&mut self, address: &RoomAddress) -> Result<(), HubError> {
        if self.rooms.remove(address).is_none() {
            return Err(HubError::RoomNotFound(address.clone()));
        }
        self.relays.remove(address);
        self.relay_hashes.remove(address);
        self.epochs.remove(address);
        self.pins.retain(|_, existing| existing != address);
        if let Some(store) = &self.store {
            store.delete_room(address).map_err(storage_error)?;
        }
        Ok(())
    }
    pub fn light_room(
        &mut self,
        actor: &UserId,
        address: &RoomAddress,
    ) -> Result<Vec<RoomEvent>, HubError> {
        {
            let room = self.room_mut(address)?;
            if room.host != *actor {
                return Err(CoreError::NotHost {
                    action: "light the room",
                }
                .into());
            }
            room.light()?;
        }
        self.persist_room(address)?;
        Ok(vec![RoomEvent::RoomLit {
            address: address.clone(),
        }])
    }

    /// Joining a dark room is only possible for its host; members need it lit.
    pub fn join(
        &mut self,
        user: UserId,
        address: &RoomAddress,
    ) -> Result<Vec<RoomEvent>, HubError> {
        let role = {
            let room = self.room_mut(address)?;
            let is_host = room.host == user;
            if !is_host && !room.is_lit() {
                return Err(HubError::RoomNotLit(address.clone()));
            }
            if room.role_of(&user).is_some() {
                return Ok(Vec::new());
            }
            room.join(user.clone())
        };
        self.persist_room(address)?;
        Ok(vec![RoomEvent::MemberJoined {
            address: address.clone(),
            user,
            role,
        }])
    }

    pub fn leave(
        &mut self,
        user: &UserId,
        address: &RoomAddress,
    ) -> Result<Vec<RoomEvent>, HubError> {
        {
            let room = self.room_mut(address)?;
            if room.role_of(user).is_none() {
                return Err(HubError::NotAParticipant);
            }
            room.leave(user)?;
        }
        self.persist_room(address)?;
        Ok(vec![RoomEvent::MemberLeft {
            address: address.clone(),
            user: user.clone(),
        }])
    }

    pub fn promote(
        &mut self,
        actor: &UserId,
        target: &UserId,
        address: &RoomAddress,
    ) -> Result<Vec<RoomEvent>, HubError> {
        {
            let room = self.room_mut(address)?;
            room.promote(actor, target)?;
        }
        self.persist_room(address)?;
        Ok(vec![RoomEvent::RoleChanged {
            address: address.clone(),
            user: target.clone(),
            role: RoomRole::CoHost,
        }])
    }

    pub fn demote(
        &mut self,
        actor: &UserId,
        target: &UserId,
        address: &RoomAddress,
    ) -> Result<Vec<RoomEvent>, HubError> {
        {
            let room = self.room_mut(address)?;
            room.demote(actor, target)?;
        }
        self.persist_room(address)?;
        Ok(vec![RoomEvent::RoleChanged {
            address: address.clone(),
            user: target.clone(),
            role: RoomRole::Member,
        }])
    }

    /// Publish is the only way content enters a room, and only scanners may.
    pub fn publish_relay(
        &mut self,
        actor: &UserId,
        address: &RoomAddress,
        request: RelayRequest,
    ) -> Result<Vec<RoomEvent>, HubError> {
        let room = self
            .rooms
            .get(address)
            .ok_or_else(|| HubError::RoomNotFound(address.clone()))?;
        if !room.is_scanner(actor) {
            return Err(HubError::NotAScanner);
        }

        let now = self.clock.now_ms();
        self.prune_relays(address, now)?;
        let hash = content_hash(&request.payload);

        if let Some(relay_id) = self
            .relay_hashes
            .get(address)
            .and_then(|hashes| hashes.get(&hash))
            .cloned()
            && let Some(relays) = self.relays.get_mut(address)
            && let Some(existing) = relays.iter_mut().find(|relay| relay.id == relay_id)
        {
            existing.created_at_ms = now;
            let relay = existing.clone();
            return Ok(vec![RoomEvent::RelayPublished {
                address: address.clone(),
                relay,
            }]);
        }

        self.next_relay += 1;
        let relay = RelayEntry::new(
            format!("r-{}", self.next_relay),
            address.clone(),
            actor.clone(),
            request.kind,
            request.compression,
            request.payload,
            now,
        )?;
        let relay_id = relay.id.clone();
        self.relay_hashes
            .entry(address.clone())
            .or_default()
            .insert(hash, relay_id);
        self.relays
            .entry(address.clone())
            .or_default()
            .push(relay.clone());
        self.trim_room(address);

        Ok(vec![RoomEvent::RelayPublished {
            address: address.clone(),
            relay,
        }])
    }

    /// Refresh an existing content hash without uploading its payload again.
    pub fn heartbeat_relay(
        &mut self,
        actor: &UserId,
        address: &RoomAddress,
        hash: &str,
    ) -> Result<RoomEvent, HubError> {
        let room = self
            .rooms
            .get(address)
            .ok_or_else(|| HubError::RoomNotFound(address.clone()))?;
        if !room.is_scanner(actor) {
            return Err(HubError::NotAScanner);
        }
        let now = self.clock.now_ms();
        self.prune_relays(address, now)?;
        let relay_id = self
            .relay_hashes
            .get(address)
            .and_then(|hashes| hashes.get(hash))
            .cloned()
            .ok_or_else(|| HubError::RelayNotFound(hash.to_string()))?;
        let Some(relays) = self.relays.get_mut(address) else {
            return Err(HubError::RelayNotFound(hash.to_string()));
        };
        let Some(relay) = relays.iter_mut().find(|relay| relay.id == relay_id) else {
            return Err(HubError::RelayNotFound(hash.to_string()));
        };
        relay.created_at_ms = now;
        Ok(RoomEvent::RelaySeen {
            address: address.clone(),
            relay: relay_id,
            created_at_ms: now,
        })
    }

    /// The snapshot a participant's device renders: who is here, what they are,
    /// which of them last relayed, and the relays themselves.
    pub fn view(&mut self, actor: &UserId, address: &RoomAddress) -> Result<RoomView, HubError> {
        let now = self.clock.now_ms();
        self.prune_relays(address, now)?;
        let room = self
            .rooms
            .get(address)
            .ok_or_else(|| HubError::RoomNotFound(address.clone()))?;
        if room.role_of(actor).is_none() {
            return Err(HubError::NotAParticipant);
        }

        let relays: Vec<_> = self
            .relays
            .get(address)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|relay| now.saturating_sub(relay.created_at_ms) <= RELAY_TTL_MS)
            .collect();
        let users = std::iter::once(&room.host)
            .chain(room.co_hosts.iter())
            .chain(room.members.iter());
        let members = users
            .map(|user| MemberView {
                user: user.clone(),
                role: room.role_of(user).expect("every participant has a role"),
                last_relay: relays
                    .iter()
                    .rev()
                    .find(|relay| &relay.publisher == user)
                    .map(|relay| relay.id.clone()),
            })
            .collect();

        Ok(RoomView {
            address: address.clone(),
            lit: room.is_lit(),
            host: room.host.clone(),
            members,
            relays,
        })
    }

    /// Every lit room, for the lobby.
    pub fn lit_rooms(&self) -> Vec<&Room> {
        self.rooms.values().filter(|room| room.is_lit()).collect()
    }

    pub fn room(&self, address: &RoomAddress) -> Option<&Room> {
        self.rooms.get(address)
    }

    /// Drop relays past their TTL. Active codes live only in memory.
    fn prune_relays(&mut self, address: &RoomAddress, now: u64) -> Result<(), HubError> {
        let Some(relays) = self.relays.get_mut(address) else {
            return Ok(());
        };
        let cutoff = now.saturating_sub(RELAY_TTL_MS);
        relays.retain(|relay| relay.created_at_ms >= cutoff);
        self.sync_hashes(address);
        Ok(())
    }

    fn trim_room(&mut self, address: &RoomAddress) {
        while let Some(relays) = self.relays.get(address) {
            let bytes: usize = relays.iter().map(|relay| relay.payload.len()).sum();
            if relays.len() <= MAX_RELAYS_PER_ROOM && bytes <= MAX_RELAY_BYTES_PER_ROOM {
                break;
            }
            let Some(oldest) = relays
                .iter()
                .enumerate()
                .min_by_key(|(_, relay)| relay.created_at_ms)
                .map(|(index, _)| index)
            else {
                break;
            };
            let removed = self
                .relays
                .get_mut(address)
                .expect("relays checked above")
                .remove(oldest);
            if let Some(hashes) = self.relay_hashes.get_mut(address) {
                hashes.retain(|_, relay_id| *relay_id != removed.id);
            }
        }
        if self.relays.get(address).is_some_and(Vec::is_empty) {
            self.relays.remove(address);
            self.relay_hashes.remove(address);
        }
    }

    fn sync_hashes(&mut self, address: &RoomAddress) {
        let Some(relays) = self.relays.get(address) else {
            self.relay_hashes.remove(address);
            return;
        };
        if let Some(hashes) = self.relay_hashes.get_mut(address) {
            hashes.retain(|_, relay_id| relays.iter().any(|relay| relay.id == *relay_id));
        }
    }

    pub fn relays(&self, address: &RoomAddress) -> &[RelayEntry] {
        self.relays
            .get(address)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn room_mut(&mut self, address: &RoomAddress) -> Result<&mut Room, HubError> {
        self.rooms
            .get_mut(address)
            .ok_or_else(|| HubError::RoomNotFound(address.clone()))
    }
}

fn storage_error(error: StoreError) -> HubError {
    HubError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use qr_race_core::Capital;
    use qr_race_store::Store;

    fn address() -> RoomAddress {
        RoomAddress::new(Capital::Tokyo, "A", "1B", "103").unwrap()
    }

    fn host() -> UserId {
        UserId("host-a".into())
    }

    fn member(name: &str) -> UserId {
        UserId(name.into())
    }

    fn hub() -> (Hub, ManualClock) {
        let clock = ManualClock::new(1_700_000_000_000);
        let hub = Hub::new(Arc::new(clock.clone()));
        (hub, clock)
    }

    fn lit_room_with_members() -> (Hub, ManualClock) {
        let (mut hub, clock) = hub();
        hub.create_room(address(), host()).unwrap();
        hub.light_room(&host(), &address()).unwrap();
        hub.join(member("m-1"), &address()).unwrap();
        hub.join(member("m-2"), &address()).unwrap();
        (hub, clock)
    }

    fn request(payload: &[u8]) -> RelayRequest {
        RelayRequest {
            kind: PayloadKind::Text,
            compression: Compression::None,
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn creating_a_room_twice_is_refused() {
        let (mut hub, _clock) = hub();
        let events = hub.create_room(address(), host()).unwrap();
        assert_eq!(
            events,
            vec![RoomEvent::RoomCreated {
                address: address(),
                host: host()
            }]
        );
        assert_eq!(
            hub.create_room(address(), host()),
            Err(HubError::RoomExists(address()))
        );
    }

    #[test]
    fn only_the_host_lights_the_room() {
        let (mut hub, _clock) = hub();
        hub.create_room(address(), host()).unwrap();
        let stranger = member("m-1");
        assert_eq!(
            hub.light_room(&stranger, &address()),
            Err(HubError::Core(CoreError::NotHost {
                action: "light the room"
            }))
        );
        hub.light_room(&host(), &address()).unwrap();
        assert!(hub.room(&address()).unwrap().is_lit());
    }

    #[test]
    fn members_cannot_join_a_dark_room() {
        let (mut hub, _clock) = hub();
        hub.create_room(address(), host()).unwrap();
        assert_eq!(
            hub.join(member("m-1"), &address()),
            Err(HubError::RoomNotLit(address()))
        );
        hub.light_room(&host(), &address()).unwrap();
        let events = hub.join(member("m-1"), &address()).unwrap();
        assert_eq!(
            events,
            vec![RoomEvent::MemberJoined {
                address: address(),
                user: member("m-1"),
                role: RoomRole::Member
            }]
        );
    }

    #[test]
    fn joining_twice_is_silent() {
        let (mut hub, _clock) = lit_room_with_members();
        assert!(hub.join(member("m-1"), &address()).unwrap().is_empty());
    }

    #[test]
    fn only_scanners_publish() {
        let (mut hub, _clock) = lit_room_with_members();
        assert_eq!(
            hub.publish_relay(&member("m-1"), &address(), request(b"x")),
            Err(HubError::NotAScanner)
        );

        let events = hub
            .publish_relay(&host(), &address(), request(b"first"))
            .unwrap();
        let RoomEvent::RelayPublished { relay, .. } = &events[0] else {
            panic!("expected a relay event");
        };
        assert_eq!(relay.id, RelayId("r-1".into()));
        assert_eq!(relay.publisher, host());
        assert_eq!(relay.created_at_ms, 1_700_000_000_000);
    }

    #[test]
    fn a_promoted_member_becomes_a_scanner() {
        let (mut hub, _clock) = lit_room_with_members();
        hub.promote(&host(), &member("m-1"), &address()).unwrap();
        let events = hub
            .publish_relay(&member("m-1"), &address(), request(b"x"))
            .unwrap();
        assert!(matches!(events[0], RoomEvent::RelayPublished { .. }));
    }

    #[test]
    fn the_view_marks_who_last_relayed() {
        let (mut hub, _clock) = lit_room_with_members();
        hub.promote(&host(), &member("m-1"), &address()).unwrap();
        hub.publish_relay(&host(), &address(), request(b"a"))
            .unwrap();
        hub.publish_relay(&member("m-1"), &address(), request(b"b"))
            .unwrap();

        let view = hub.view(&member("m-2"), &address()).unwrap();
        assert_eq!(view.members.len(), 3);
        let host_view = view.members.iter().find(|m| m.user == host()).unwrap();
        let co_host_view = view
            .members
            .iter()
            .find(|m| m.user == member("m-1"))
            .unwrap();
        let plain = view
            .members
            .iter()
            .find(|m| m.user == member("m-2"))
            .unwrap();

        assert_eq!(host_view.role, RoomRole::Host);
        assert_eq!(host_view.last_relay, Some(RelayId("r-1".into())));
        assert_eq!(co_host_view.role, RoomRole::CoHost);
        assert_eq!(co_host_view.last_relay, Some(RelayId("r-2".into())));
        assert_eq!(plain.last_relay, None);
        assert_eq!(view.relays.len(), 2);
    }

    #[test]
    fn non_participants_cannot_view_a_room() {
        let (mut hub, _clock) = lit_room_with_members();
        assert_eq!(
            hub.view(&member("stranger"), &address()),
            Err(HubError::NotAParticipant)
        );
    }

    #[test]
    fn leaving_removes_the_role_and_the_scanner_right() {
        let (mut hub, _clock) = lit_room_with_members();
        hub.promote(&host(), &member("m-1"), &address()).unwrap();
        hub.leave(&member("m-1"), &address()).unwrap();
        assert_eq!(
            hub.publish_relay(&member("m-1"), &address(), request(b"x")),
            Err(HubError::NotAScanner)
        );
    }

    #[test]
    fn the_lobby_lists_only_lit_rooms() {
        let (mut hub, _clock) = hub();
        let dark = RoomAddress::new(Capital::Tokyo, "B", "1B", "104").unwrap();
        hub.create_room(address(), host()).unwrap();
        hub.create_room(dark, host()).unwrap();
        hub.light_room(&host(), &address()).unwrap();
        assert_eq!(hub.lit_rooms().len(), 1);
        assert!(hub.lit_rooms()[0].is_lit());
    }

    #[test]
    fn a_room_keeps_only_the_newest_relays() {
        let (mut hub, _clock) = lit_room_with_members();
        for index in 0..=MAX_RELAYS_PER_ROOM {
            hub.publish_relay(&host(), &address(), request(format!("p{index}").as_bytes()))
                .unwrap();
        }

        let view = hub.view(&host(), &address()).unwrap();
        assert_eq!(view.relays.len(), MAX_RELAYS_PER_ROOM);
        assert_eq!(view.relays.first().unwrap().id, RelayId("r-2".into()));
        assert_eq!(
            view.relays.last().unwrap().id,
            RelayId(format!("r-{}", MAX_RELAYS_PER_ROOM + 1))
        );
    }

    #[test]
    fn pairing_pin_is_host_only_and_survives_restart() {
        let clock = ManualClock::new(1_700_000_000_000);
        let store = Arc::new(Store::in_memory().unwrap());
        let mut hub = Hub::with_store(Arc::new(clock.clone()), store.clone()).unwrap();
        hub.create_room(address(), host()).unwrap();
        assert!(hub.set_pin(&host(), &address(), "1234").is_ok());
        assert!(hub.set_pin(&member("m-1"), &address(), "5678").is_err());
        assert_eq!(hub.resolve_pin("1234"), Some(address()));

        let reloaded = Hub::with_store(Arc::new(clock), store).unwrap();
        assert_eq!(reloaded.resolve_pin("1234"), Some(address()));
    }

    #[test]
    fn closing_a_room_releases_its_pin_for_reuse() {
        let clock = ManualClock::new(1_700_000_000_000);
        let store = Arc::new(Store::in_memory().unwrap());
        let mut hub = Hub::with_store(Arc::new(clock.clone()), store.clone()).unwrap();
        hub.create_room(address(), host()).unwrap();
        hub.set_pin(&host(), &address(), "1234").unwrap();
        hub.close_room(&address()).unwrap();
        assert_eq!(hub.resolve_pin("1234"), None);

        let reloaded = Hub::with_store(Arc::new(clock), store).unwrap();
        assert_eq!(reloaded.resolve_pin("1234"), None);
    }
    #[test]
    fn a_restart_resumes_rooms_and_roles_but_not_codes() {
        let clock = ManualClock::new(1_700_000_000_000);
        let store = Arc::new(Store::in_memory().unwrap());
        let mut hub = Hub::with_store(Arc::new(clock.clone()), store.clone()).unwrap();

        hub.create_room(address(), host()).unwrap();
        hub.light_room(&host(), &address()).unwrap();
        hub.join(member("m-1"), &address()).unwrap();
        hub.promote(&host(), &member("m-1"), &address()).unwrap();
        hub.publish_relay(&host(), &address(), request(b"persisted"))
            .unwrap();

        let mut reloaded = Hub::with_store(Arc::new(clock), store).unwrap();
        let view = reloaded.view(&member("m-1"), &address()).unwrap();
        assert!(view.lit);
        assert!(view.relays.is_empty(), "codes are memory-only");
        let target = member("m-1");
        let promoted = view
            .members
            .iter()
            .find(|entry| entry.user == target)
            .unwrap();
        assert_eq!(promoted.role, RoomRole::CoHost);

        let events = reloaded
            .publish_relay(&host(), &address(), request(b"second"))
            .unwrap();
        match &events[0] {
            RoomEvent::RelayPublished { relay, .. } => {
                assert_eq!(
                    relay.id,
                    RelayId("r-1".into()),
                    "the id counter resets with codes"
                );
            }
            other => panic!("expected a relay event, got {other:?}"),
        }
    }
    #[test]
    fn republishing_same_payload_refreshes_one_relay() {
        let (mut hub, clock) = lit_room_with_members();
        let first = hub
            .publish_relay(&host(), &address(), request(b"same"))
            .unwrap();
        let first_ms = match &first[0] {
            RoomEvent::RelayPublished { relay, .. } => relay.created_at_ms,
            other => panic!("expected relay, got {other:?}"),
        };
        clock.advance(5_000);
        let second = hub
            .publish_relay(&host(), &address(), request(b"same"))
            .unwrap();
        let second_ms = match &second[0] {
            RoomEvent::RelayPublished { relay, .. } => relay.created_at_ms,
            other => panic!("expected relay, got {other:?}"),
        };
        assert_eq!(second_ms, first_ms + 5_000);
        assert_eq!(hub.view(&host(), &address()).unwrap().relays.len(), 1);
    }

    #[test]
    fn another_publisher_refreshes_the_same_content() {
        let (mut hub, clock) = lit_room_with_members();
        hub.promote(&host(), &member("m-1"), &address()).unwrap();
        let first = hub
            .publish_relay(&host(), &address(), request(b"same"))
            .unwrap();
        let first_id = match &first[0] {
            RoomEvent::RelayPublished { relay, .. } => relay.id.clone(),
            other => panic!("expected relay, got {other:?}"),
        };
        clock.advance(5_000);
        let second = hub
            .publish_relay(&member("m-1"), &address(), request(b"same"))
            .unwrap();
        let second_relay = match &second[0] {
            RoomEvent::RelayPublished { relay, .. } => relay,
            other => panic!("expected relay, got {other:?}"),
        };
        assert_eq!(second_relay.id, first_id);
        assert_eq!(second_relay.publisher, host());
        assert_eq!(hub.view(&host(), &address()).unwrap().relays.len(), 1);
    }

    #[test]
    fn a_hash_heartbeat_refreshes_without_the_payload() {
        let (mut hub, clock) = lit_room_with_members();
        let events = hub
            .publish_relay(&host(), &address(), request(b"same"))
            .unwrap();
        let relay_id = match &events[0] {
            RoomEvent::RelayPublished { relay, .. } => relay.id.clone(),
            other => panic!("expected relay, got {other:?}"),
        };
        let hash = content_hash(b"same");
        clock.advance(5_000);
        let event = hub.heartbeat_relay(&host(), &address(), &hash).unwrap();
        match event {
            RoomEvent::RelaySeen {
                relay,
                created_at_ms,
                ..
            } => {
                assert_eq!(relay, relay_id);
                assert_eq!(created_at_ms, 1_700_000_005_000);
            }
            other => panic!("expected seen, got {other:?}"),
        }
        assert_eq!(
            hub.heartbeat_relay(&host(), &address(), "unknown"),
            Err(HubError::RelayNotFound("unknown".into()))
        );
    }

    #[test]
    fn room_byte_cap_evicts_oldest_codes() {
        let (mut hub, _clock) = lit_room_with_members();
        for index in 0..20u8 {
            hub.publish_relay(&host(), &address(), request(&vec![index; 16 * 1024]))
                .unwrap();
        }
        let view = hub.view(&host(), &address()).unwrap();
        let bytes: usize = view.relays.iter().map(|relay| relay.payload.len()).sum();
        assert!(bytes <= MAX_RELAY_BYTES_PER_ROOM);
        assert!(view.relays.len() < 20);
    }

    #[test]
    fn relays_disappear_from_the_view_after_forty_seconds() {
        let (mut hub, clock) = lit_room_with_members();
        hub.publish_relay(&host(), &address(), request(b"old"))
            .unwrap();
        clock.advance(40_001);
        assert!(hub.view(&host(), &address()).unwrap().relays.is_empty());
    }

    #[test]
    fn expired_relays_are_dropped_from_memory() {
        let (mut hub, clock) = lit_room_with_members();
        hub.publish_relay(&host(), &address(), request(b"old"))
            .unwrap();
        clock.advance(40_001);
        assert!(hub.view(&host(), &address()).unwrap().relays.is_empty());
        assert!(hub.relays(&address()).is_empty());
    }
}

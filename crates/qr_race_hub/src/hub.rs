//! Rooms, relays and the events a transport broadcasts.
//!
//! This layer owns the product rules and nothing else: no sockets, no HTTP, no
//! JSON. Every mutation returns the events it caused so the transport can fan
//! them out to the room without the hub knowing what a connection is.

use std::collections::BTreeMap;
use std::sync::Arc;

use qr_race_core::{
    Compression, CoreError, PayloadKind, RelayEntry, RelayId, Room, RoomAddress, RoomRole, UserId,
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

    #[error("storage: {0}")]
    Storage(String),

    #[error(transparent)]
    Core(#[from] CoreError),
}

pub struct Hub {
    rooms: BTreeMap<RoomAddress, Room>,
    relays: BTreeMap<RoomAddress, Vec<RelayEntry>>,
    clock: Arc<dyn Clock>,
    next_relay: u64,
    store: Option<Arc<Store>>,
    pins: BTreeMap<String, RoomAddress>,
}

/// Relays are kept in memory, so a room keeps its newest entries and drops the
/// oldest beyond this bound. Members re-fetch after a gap (see the WS contract).
pub const MAX_RELAYS_PER_ROOM: usize = 64;

impl Hub {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            rooms: BTreeMap::new(),
            relays: BTreeMap::new(),
            clock,
            next_relay: 0,
            store: None,
            pins: BTreeMap::new(),
        }
    }

    /// Rebuilds a hub from the store, so a restart resumes the rooms and relays
    /// users already see. Accounts come from the operator's config, not here.
    pub fn with_store(clock: Arc<dyn Clock>, store: Arc<Store>) -> Result<Self, HubError> {
        let rooms: BTreeMap<RoomAddress, Room> = store
            .load_rooms()
            .map_err(storage_error)?
            .into_iter()
            .map(|room| (room.address.clone(), room))
            .collect();

        let mut relays = BTreeMap::new();
        for address in rooms.keys() {
            relays.insert(
                address.clone(),
                store
                    .load_relays(address, MAX_RELAYS_PER_ROOM)
                    .map_err(storage_error)?,
            );
        }
        let next_relay = store.max_relay_seq().map_err(storage_error)?;
        let pins = store
            .room_pins()
            .map_err(storage_error)?
            .into_iter()
            .collect();

        Ok(Self {
            rooms,
            relays,
            clock,
            next_relay,
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
        self.rooms
            .insert(address.clone(), Room::new(address.clone(), host.clone()));
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
        self.pins.get(pin).cloned()
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

        self.next_relay += 1;
        let relay = RelayEntry::new(
            format!("r-{}", self.next_relay),
            address.clone(),
            actor.clone(),
            request.kind,
            request.compression,
            request.payload,
            self.clock.now_ms(),
        )?;
        let relays = self.relays.entry(address.clone()).or_default();
        relays.push(relay.clone());
        if relays.len() > MAX_RELAYS_PER_ROOM {
            let overflow = relays.len() - MAX_RELAYS_PER_ROOM;
            relays.drain(0..overflow);
        }

        if let Some(store) = &self.store {
            store
                .insert_relay(self.next_relay, &relay)
                .map_err(storage_error)?;
            store
                .trim_relays(address, MAX_RELAYS_PER_ROOM)
                .map_err(storage_error)?;
        }

        Ok(vec![RoomEvent::RelayPublished {
            address: address.clone(),
            relay,
        }])
    }

    /// The snapshot a participant's device renders: who is here, what they are,
    /// which of them last relayed, and the relays themselves.
    pub fn view(&self, actor: &UserId, address: &RoomAddress) -> Result<RoomView, HubError> {
        let room = self
            .rooms
            .get(address)
            .ok_or_else(|| HubError::RoomNotFound(address.clone()))?;
        if room.role_of(actor).is_none() {
            return Err(HubError::NotAParticipant);
        }

        let relays = self.relays.get(address).cloned().unwrap_or_default();
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
        let (hub, _clock) = lit_room_with_members();
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
    fn a_restart_resumes_rooms_roles_and_relays() {
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
        assert_eq!(view.relays.len(), 1);
        assert_eq!(view.relays[0].payload, b"persisted");
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
                assert_eq!(relay.id, RelayId("r-2".into()), "the id counter continues");
            }
            other => panic!("expected a relay event, got {other:?}"),
        }
    }
}

//! Wire types. Bytes travel as base64; room addresses travel as their canonical
//! string plus the structured form, so a client never has to reimplement either.

use data_encoding::BASE64;
use qr_race_core::{
    Compression, PayloadKind, RelayEntry, RelayId, Room, RoomAddress, RoomRole, UserId,
};
use qr_race_hub::{MemberView, RoomEvent, RoomView};
use serde::{Deserialize, Serialize};

pub const fn default_compression() -> Compression {
    Compression::None
}

#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    pub token: String,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub session_id: String,
    pub user_id: UserId,
    pub expires_at_ms: u64,
    pub admin: bool,
}

#[derive(Debug, Serialize)]
pub struct WsTicketResponse {
    pub ticket: String,
    pub expires_at_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomRequest {
    pub address: String,
    #[serde(default)]
    pub pin: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetPinRequest {
    pub address: String,
    pub pin: String,
}

#[derive(Debug, Serialize)]
pub struct PinRoomResponse {
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct PinStartRequest {
    pub pin: String,
}

#[derive(Debug, Serialize)]
pub struct PinStartResponse {
    pub address: String,
    pub created: bool,
}

#[derive(Debug, Deserialize)]
pub struct TargetRequest {
    pub target: UserId,
}

#[derive(Debug, Deserialize)]
pub struct RelayBody {
    pub kind: PayloadKind,
    #[serde(default = "default_compression")]
    pub compression: Compression,
    pub payload_b64: String,
}

#[derive(Debug, Deserialize)]
pub struct RelayHeartbeatBody {
    pub hash: String,
}

#[derive(Debug, Serialize)]
pub struct RelaySeenResponse {
    pub relay: RelayId,
    pub created_at_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct RelayDto {
    pub id: RelayId,
    pub room: RoomAddress,
    pub publisher: UserId,
    pub kind: PayloadKind,
    pub compression: Compression,
    pub created_at_ms: u64,
    pub payload_b64: String,
}

impl From<&RelayEntry> for RelayDto {
    fn from(entry: &RelayEntry) -> Self {
        Self {
            id: entry.id.clone(),
            room: entry.room.clone(),
            publisher: entry.publisher.clone(),
            kind: entry.kind,
            compression: entry.compression,
            created_at_ms: entry.created_at_ms,
            payload_b64: BASE64.encode(&entry.payload),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MemberViewDto {
    pub user: UserId,
    pub role: RoomRole,
    pub last_relay: Option<RelayId>,
}

impl From<&MemberView> for MemberViewDto {
    fn from(member: &MemberView) -> Self {
        Self {
            user: member.user.clone(),
            role: member.role,
            last_relay: member.last_relay.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RoomViewDto {
    pub address: RoomAddress,
    pub label: String,
    pub lit: bool,
    pub host: UserId,
    pub members: Vec<MemberViewDto>,
    pub relays: Vec<RelayDto>,
}

impl From<&RoomView> for RoomViewDto {
    fn from(view: &RoomView) -> Self {
        Self {
            address: view.address.clone(),
            label: view.address.to_string(),
            lit: view.lit,
            host: view.host.clone(),
            members: view.members.iter().map(MemberViewDto::from).collect(),
            relays: view.relays.iter().map(RelayDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LobbyRoomDto {
    pub address: RoomAddress,
    pub label: String,
    pub host: UserId,
    pub member_count: usize,
}

impl From<&Room> for LobbyRoomDto {
    fn from(room: &Room) -> Self {
        Self {
            address: room.address.clone(),
            label: room.address.to_string(),
            host: room.host.clone(),
            member_count: 1 + room.co_hosts.len() + room.members.len(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LobbyDto {
    pub rooms: Vec<LobbyRoomDto>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventDto {
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
        relay: RelayDto,
    },
    RelaySeen {
        address: RoomAddress,
        relay: RelayId,
        created_at_ms: u64,
    },
}

impl From<RoomEvent> for EventDto {
    fn from(event: RoomEvent) -> Self {
        match event {
            RoomEvent::RoomCreated { address, host } => EventDto::RoomCreated { address, host },
            RoomEvent::RoomLit { address } => EventDto::RoomLit { address },
            RoomEvent::MemberJoined {
                address,
                user,
                role,
            } => EventDto::MemberJoined {
                address,
                user,
                role,
            },
            RoomEvent::MemberLeft { address, user } => EventDto::MemberLeft { address, user },
            RoomEvent::RoleChanged {
                address,
                user,
                role,
            } => EventDto::RoleChanged {
                address,
                user,
                role,
            },
            RoomEvent::RelayPublished { address, relay } => EventDto::RelayPublished {
                address,
                relay: RelayDto::from(&relay),
            },
            RoomEvent::RelaySeen {
                address,
                relay,
                created_at_ms,
            } => EventDto::RelaySeen {
                address,
                relay,
                created_at_ms,
            },
        }
    }
}

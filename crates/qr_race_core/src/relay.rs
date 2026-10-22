//! Relays: one scanned payload, published once, carried by the room.
//!
//! The payload is uploaded compressed (the publisher compresses on the device,
//! the server stores and rebroadcasts the bytes). This module does not compress
//! or decompress; it records the codec so a client can.

use std::sync::Arc;

use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};

use crate::address::RoomAddress;
use crate::error::CoreError;

/// Number of BLAKE3 bytes retained for a content id.
pub const CONTENT_HASH_BYTES: usize = 12;

/// Stable content id: BLAKE3 truncated to 96 bits and encoded as lowercase
/// hex (24 chars). Short enough for a compact heartbeat, wide enough to avoid
/// practical collisions inside one room.
pub fn content_hash(payload: &[u8]) -> String {
    let digest = blake3::hash(payload);
    HEXLOWER.encode(&digest.as_bytes()[..CONTENT_HASH_BYTES])
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RelayId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UserId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKind {
    Text,
    Url,
    Json,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    None,
    Zstd,
}

/// Upload cap for one relay, before decompression. The QR symbol a member renders
/// has its own capacity; keeping uploads small keeps the relay re-renderable.
pub const MAX_UPLOAD_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayEntry {
    pub id: RelayId,
    pub room: RoomAddress,
    pub publisher: UserId,
    pub kind: PayloadKind,
    pub compression: Compression,
    pub payload: Arc<[u8]>,
    /// Content id for this payload; computed once when the entry is built.
    pub hash: String,
    pub created_at_ms: u64,
}

impl RelayEntry {
    /// Builds an entry whose payload is already in its uploaded form.
    /// Authorization (only a host or co-host publishes) is the server's call.
    pub fn new(
        id: impl Into<String>,
        room: RoomAddress,
        publisher: UserId,
        kind: PayloadKind,
        compression: Compression,
        payload: Vec<u8>,
        created_at_ms: u64,
    ) -> Result<Self, CoreError> {
        let id = id.into();
        if id.is_empty() {
            return Err(CoreError::Empty("relay id"));
        }
        if payload.is_empty() {
            return Err(CoreError::Empty("relay payload"));
        }
        if payload.len() > MAX_UPLOAD_BYTES {
            return Err(CoreError::RelayTooLarge {
                size: payload.len(),
                max: MAX_UPLOAD_BYTES,
            });
        }
        Ok(Self {
            id: RelayId(id),
            room,
            publisher,
            kind,
            compression,
            hash: content_hash(&payload),
            payload: Arc::from(payload),
            created_at_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Capital;

    fn room() -> RoomAddress {
        RoomAddress::new(Capital::Tokyo, "A", "1B", "103").unwrap()
    }

    #[test]
    fn content_hash_is_blake3_96_bits() {
        let hash = content_hash(b"same content");
        assert_eq!(hash.len(), CONTENT_HASH_BYTES * 2);
        assert!(
            hash.bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
        assert_eq!(hash, content_hash(b"same content"));
        assert_ne!(hash, content_hash(b"different"));
        assert_eq!(
            content_hash(b""),
            "af1349b9f5f9a1a6a0404dea",
            "wire hash changed"
        );
    }

    #[test]
    fn builds_a_relay() {
        let entry = RelayEntry::new(
            "r-1",
            room(),
            UserId("u-a".into()),
            PayloadKind::Url,
            Compression::None,
            b"https://example.test/x".to_vec(),
            1_700_000_000_000,
        )
        .unwrap();
        assert_eq!(entry.id, RelayId("r-1".into()));
        assert_eq!(entry.payload.len(), 22);
    }

    #[test]
    fn rejects_an_empty_payload_or_id() {
        let empty_payload = RelayEntry::new(
            "r-1",
            room(),
            UserId("u-a".into()),
            PayloadKind::Text,
            Compression::None,
            Vec::new(),
            0,
        );
        assert_eq!(empty_payload, Err(CoreError::Empty("relay payload")));

        let empty_id = RelayEntry::new(
            "",
            room(),
            UserId("u-a".into()),
            PayloadKind::Text,
            Compression::None,
            b"x".to_vec(),
            0,
        );
        assert_eq!(empty_id, Err(CoreError::Empty("relay id")));
    }

    #[test]
    fn rejects_an_oversized_payload() {
        let entry = RelayEntry::new(
            "r-1",
            room(),
            UserId("u-a".into()),
            PayloadKind::Binary,
            Compression::None,
            vec![0u8; MAX_UPLOAD_BYTES + 1],
            0,
        );
        assert!(matches!(entry, Err(CoreError::RelayTooLarge { .. })));
    }
}

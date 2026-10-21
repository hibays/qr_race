//! SQLite persistence for the parts of the world that outlive a process:
//! rooms (membership, roles, join secret) and relays.
//!
//! Accounts stay in the operator's config file, not here: provisioning is an
//! operator act and the config is already the source of truth for tokens.

use std::path::Path;
use std::str::FromStr;
use std::sync::Mutex;

use qr_race_core::{
    Compression, PayloadKind, RelayEntry, RelayId, Room, RoomAddress, RoomState, SecretHash, UserId,
};
use rusqlite::{Connection, params};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("corrupt stored row: {0}")]
    Corrupt(String),
}

pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        Self::init(connection)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        Self::init(connection)
    }

    fn init(connection: Connection) -> Result<Self, StoreError> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS rooms (
                address TEXT PRIMARY KEY,
                host TEXT NOT NULL,
                state TEXT NOT NULL,
                co_hosts TEXT NOT NULL,
                members TEXT NOT NULL,
                join_secret TEXT,
                modules TEXT NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS relays (
                seq INTEGER PRIMARY KEY,
                id TEXT NOT NULL,
                room TEXT NOT NULL,
                publisher TEXT NOT NULL,
                kind TEXT NOT NULL,
                compression TEXT NOT NULL,
                payload BLOB NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS relays_room_seq ON relays(room, seq);
            CREATE TABLE IF NOT EXISTS room_pins (
                pin TEXT PRIMARY KEY,
                address TEXT NOT NULL UNIQUE,
                updated_at_ms INTEGER NOT NULL
            );
            ",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn upsert_room(&self, room: &Room, updated_at_ms: u64) -> Result<(), StoreError> {
        let co_hosts =
            serde_json::to_string(&room.co_hosts.iter().map(|id| &id.0).collect::<Vec<_>>())
                .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        let members =
            serde_json::to_string(&room.members.iter().map(|id| &id.0).collect::<Vec<_>>())
                .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        let modules = "[]";
        let state = match room.state {
            RoomState::Dark => "dark",
            RoomState::Lit => "lit",
        };

        self.lock()?.execute(
            "INSERT INTO rooms (address, host, state, co_hosts, members, join_secret, modules, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(address) DO UPDATE SET
               host=excluded.host,
               state=excluded.state,
               co_hosts=excluded.co_hosts,
               members=excluded.members,
               join_secret=excluded.join_secret,
               modules=excluded.modules,
               updated_at_ms=excluded.updated_at_ms",
            params![
                room.address.to_string(),
                room.host.0,
                state,
                co_hosts,
                members,
                room.join_secret.as_ref().map(SecretHash::as_str),
                modules,
                updated_at_ms as i64,
            ],
        )?;
        Ok(())
    }

    pub fn set_room_pin(
        &self,
        pin: &str,
        address: &RoomAddress,
        at_ms: u64,
    ) -> Result<(), StoreError> {
        self.lock()?.execute(
            "INSERT INTO room_pins (pin, address, updated_at_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(pin) DO UPDATE SET address=excluded.address, updated_at_ms=excluded.updated_at_ms",
            params![pin, address.to_string(), at_ms as i64],
        )?;
        Ok(())
    }

    pub fn room_pin(&self, pin: &str) -> Result<Option<RoomAddress>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT address FROM room_pins WHERE pin = ?1")?;
        let mut rows = statement.query(params![pin])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let address: String = row.get(0)?;
        address
            .parse()
            .map(Some)
            .map_err(|error: qr_race_core::CoreError| StoreError::Corrupt(error.to_string()))
    }

    pub fn room_pins(&self) -> Result<Vec<(String, RoomAddress)>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT pin, address FROM room_pins")?;
        let mut rows = statement.query([])?;
        let mut pins = Vec::new();
        while let Some(row) = rows.next()? {
            let pin: String = row.get(0)?;
            let address: String = row.get(1)?;
            let address = address
                .parse()
                .map_err(|error: qr_race_core::CoreError| StoreError::Corrupt(error.to_string()))?;
            pins.push((pin, address));
        }
        Ok(pins)
    }

    pub fn delete_room(&self, address: &RoomAddress) -> Result<(), StoreError> {
        let connection = self.lock()?;
        connection.execute(
            "DELETE FROM relays WHERE room = ?1",
            params![address.to_string()],
        )?;
        connection.execute(
            "DELETE FROM rooms WHERE address = ?1",
            params![address.to_string()],
        )?;
        Ok(())
    }

    pub fn load_rooms(&self) -> Result<Vec<Room>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT address, host, state, co_hosts, members, join_secret FROM rooms")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;

        let mut rooms = Vec::new();
        for row in rows {
            let (address, host, state, co_hosts, members, join_secret) = row?;
            rooms.push(decode_room(
                &address,
                &host,
                &state,
                &co_hosts,
                &members,
                join_secret.as_deref(),
            )?);
        }
        Ok(rooms)
    }

    pub fn insert_relay(&self, seq: u64, relay: &RelayEntry) -> Result<(), StoreError> {
        self.lock()?.execute(
            "INSERT OR REPLACE INTO relays
             (seq, id, room, publisher, kind, compression, payload, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                seq as i64,
                relay.id.0,
                relay.room.to_string(),
                relay.publisher.0,
                kind_to_str(relay.kind),
                compression_to_str(relay.compression),
                relay.payload,
                relay.created_at_ms as i64,
            ],
        )?;
        Ok(())
    }

    /// Newest `limit` relays for a room, oldest first.
    pub fn load_relays(
        &self,
        room: &RoomAddress,
        limit: usize,
    ) -> Result<Vec<RelayEntry>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, room, publisher, kind, compression, payload, created_at_ms
             FROM relays WHERE room = ?1 ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![room.to_string(), limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;

        let mut relays = Vec::new();
        for row in rows {
            let (id, room, publisher, kind, compression, payload, created_at_ms) = row?;
            relays.push(RelayEntry {
                id: RelayId(id),
                room: RoomAddress::from_str(&room)
                    .map_err(|error| StoreError::Corrupt(error.to_string()))?,
                publisher: UserId(publisher),
                kind: kind_from_str(&kind)?,
                compression: compression_from_str(&compression)?,
                payload,
                created_at_ms: created_at_ms as u64,
            });
        }
        relays.reverse();
        Ok(relays)
    }

    pub fn trim_relays(&self, room: &RoomAddress, keep: usize) -> Result<(), StoreError> {
        self.lock()?.execute(
            "DELETE FROM relays WHERE room = ?1 AND seq NOT IN
             (SELECT seq FROM relays WHERE room = ?1 ORDER BY seq DESC LIMIT ?2)",
            params![room.to_string(), keep as i64],
        )?;
        Ok(())
    }

    pub fn max_relay_seq(&self) -> Result<u64, StoreError> {
        let value: Option<i64> =
            self.lock()?
                .query_row("SELECT MAX(seq) FROM relays", [], |row| row.get(0))?;
        Ok(value.unwrap_or(0).max(0) as u64)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StoreError> {
        self.connection
            .lock()
            .map_err(|_| StoreError::Corrupt("store lock poisoned".into()))
    }
}

fn decode_room(
    address: &str,
    host: &str,
    state: &str,
    co_hosts: &str,
    members: &str,
    join_secret: Option<&str>,
) -> Result<Room, StoreError> {
    let address =
        RoomAddress::from_str(address).map_err(|error| StoreError::Corrupt(error.to_string()))?;
    let state = match state {
        "lit" => RoomState::Lit,
        "dark" => RoomState::Dark,
        other => return Err(StoreError::Corrupt(format!("unknown state {other:?}"))),
    };
    let co_hosts: Vec<String> =
        serde_json::from_str(co_hosts).map_err(|error| StoreError::Corrupt(error.to_string()))?;
    let members: Vec<String> =
        serde_json::from_str(members).map_err(|error| StoreError::Corrupt(error.to_string()))?;
    Ok(Room {
        address,
        host: UserId(host.to_string()),
        co_hosts: co_hosts.into_iter().map(UserId).collect(),
        members: members.into_iter().map(UserId).collect(),
        state,
        join_secret: join_secret.map(|secret| SecretHash::new(secret.to_string())),
    })
}

fn kind_to_str(kind: PayloadKind) -> &'static str {
    match kind {
        PayloadKind::Text => "text",
        PayloadKind::Url => "url",
        PayloadKind::Json => "json",
        PayloadKind::Binary => "binary",
    }
}

fn kind_from_str(value: &str) -> Result<PayloadKind, StoreError> {
    match value {
        "text" => Ok(PayloadKind::Text),
        "url" => Ok(PayloadKind::Url),
        "json" => Ok(PayloadKind::Json),
        "binary" => Ok(PayloadKind::Binary),
        other => Err(StoreError::Corrupt(format!(
            "unknown payload kind {other:?}"
        ))),
    }
}

fn compression_to_str(compression: Compression) -> &'static str {
    match compression {
        Compression::None => "none",
        Compression::Zstd => "zstd",
    }
}

fn compression_from_str(value: &str) -> Result<Compression, StoreError> {
    match value {
        "none" => Ok(Compression::None),
        "zstd" => Ok(Compression::Zstd),
        other => Err(StoreError::Corrupt(format!(
            "unknown compression {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qr_race_core::Capital;

    fn address() -> RoomAddress {
        RoomAddress::new(Capital::Tokyo, "A", "1B", "103").unwrap()
    }

    fn room() -> Room {
        let mut room = Room::new(address(), UserId("host-a".into()));
        room.light().unwrap();
        room.join(UserId("m-1".into()));
        room.promote(&UserId("host-a".into()), &UserId("m-1".into()))
            .unwrap();
        room.set_join_secret(&UserId("host-a".into()), Some(SecretHash::new("hash")))
            .unwrap();
        room
    }

    fn relay(id: &str, payload: &[u8], at_ms: u64) -> RelayEntry {
        RelayEntry::new(
            id,
            address(),
            UserId("host-a".into()),
            PayloadKind::Text,
            Compression::None,
            payload.to_vec(),
            at_ms,
        )
        .unwrap()
    }

    #[test]
    fn rooms_round_trip_with_membership() {
        let store = Store::in_memory().unwrap();
        let room = room();
        store.upsert_room(&room, 1).unwrap();

        let loaded = store.load_rooms().unwrap();
        assert_eq!(loaded, vec![room]);
    }

    #[test]
    fn relays_come_back_newest_first_in_storage_and_oldest_first_in_the_view() {
        let store = Store::in_memory().unwrap();
        for (index, payload) in [b"a".as_slice(), b"b", b"c"].into_iter().enumerate() {
            store
                .insert_relay(
                    index as u64 + 1,
                    &relay(&format!("r-{}", index + 1), payload, 10),
                )
                .unwrap();
        }

        let loaded = store.load_relays(&address(), 2).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, RelayId("r-2".into()));
        assert_eq!(loaded[1].id, RelayId("r-3".into()));
        assert_eq!(store.max_relay_seq().unwrap(), 3);
    }

    #[test]
    fn trimming_keeps_the_newest() {
        let store = Store::in_memory().unwrap();
        for index in 1..=3u64 {
            store
                .insert_relay(index, &relay(&format!("r-{index}"), b"x", index))
                .unwrap();
        }
        store.trim_relays(&address(), 1).unwrap();
        let loaded = store.load_relays(&address(), 10).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, RelayId("r-3".into()));
    }

    #[test]
    fn data_survives_reopening_the_file() {
        let path = std::env::temp_dir().join(format!(
            "qr-race-store-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        {
            let store = Store::open(&path).unwrap();
            store.upsert_room(&room(), 1).unwrap();
            store.insert_relay(1, &relay("r-1", b"payload", 5)).unwrap();
        }
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.load_rooms().unwrap().len(), 1);
            assert_eq!(store.load_relays(&address(), 10).unwrap().len(), 1);
            assert_eq!(store.max_relay_seq().unwrap(), 1);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}

//! SQLite persistence for room rows while the process is running.
//!
//! Rooms are live state: they are cleared on startup, so a crashed process
//! cannot leave addresses or PINs reserved. Accounts stay in the operator's
//! config file, not here.

use std::path::Path;
use std::sync::Mutex;

use qr_race_core::{Room, RoomAddress, RoomState, SecretHash};
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

    /// Removes all rooms and pins. Called once at startup.
    pub fn clear_rooms(&self) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM room_pins", [])?;
        transaction.execute("DELETE FROM rooms", [])?;
        transaction.commit()?;
        Ok(())
    }
    pub fn delete_room(&self, address: &RoomAddress) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM rooms WHERE address = ?1",
            params![address.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM room_pins WHERE address = ?1",
            params![address.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StoreError> {
        self.connection
            .lock()
            .map_err(|_| StoreError::Corrupt("store lock poisoned".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qr_race_core::{Capital, UserId};

    fn address() -> RoomAddress {
        RoomAddress::new(Capital::Tokyo, "A", "1B", "103").unwrap()
    }

    fn room() -> Room {
        let mut room = Room::new(address(), UserId("host-a".into()));
        room.light().unwrap();
        room
    }

    #[test]
    fn startup_clear_is_idempotent() {
        let store = Store::in_memory().unwrap();
        store.upsert_room(&room(), 1).unwrap();
        store.set_room_pin("1234", &address(), 1).unwrap();
        store.clear_rooms().unwrap();
        store.clear_rooms().unwrap();
    }
}

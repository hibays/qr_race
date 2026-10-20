//! Durable domain model for qr_race.
//!
//! A room is a fixed, real-venue address. A relay is one scanned payload
//! published by a host or co-host into that room.

pub mod address;
pub mod error;
pub mod relay;
pub mod role;
pub mod room;

pub use address::{Capital, RoomAddress};
pub use error::CoreError;
pub use relay::{Compression, MAX_UPLOAD_BYTES, PayloadKind, RelayEntry, RelayId, UserId};
pub use role::{Privileges, RoomAbility, RoomRole};
pub use room::{Room, RoomState, SecretHash};

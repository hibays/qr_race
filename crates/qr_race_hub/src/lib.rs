//! Server-side coordination: auth, rooms, relays, events.
//!
//! No transport lives here. The binary in `src/` will own HTTP/WebSocket and
//! hand this crate sockets-free commands.

pub mod auth;
pub mod clock;
pub mod hub;
pub mod totp;

pub use auth::{
    AuthError, AuthService, DEFAULT_SESSION_TTL_MS, MIN_TOKEN_LEN, Session, generate_token,
    generate_totp_secret_base32,
};
pub use clock::{Clock, ManualClock, SystemClock};
pub use hub::{
    Hub, HubError, MAX_RELAYS_PER_ROOM, MemberView, RelayRequest, RoomEvent, RoomView, StoredModule,
};
pub use totp::{DEFAULT_DIGITS, STEP_SECONDS};

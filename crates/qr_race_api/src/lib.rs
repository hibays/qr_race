//! HTTP + WebSocket transport. The hub owns every rule; this layer owns bytes,
//! status codes and fan-out.

mod dto;
mod error;
mod routes;
mod ws;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use axum::Router;
use axum::routing::{get, post};
use qr_race_core::{RoomAddress, UserId};
use qr_race_hub::{AuthError, AuthService, Clock, Hub, RoomEvent, Session, generate_token};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

use crate::error::ApiError;

/// A one-shot ticket for the WebSocket upgrade. Browsers cannot set an
/// Authorization header on a WebSocket, and a session id in a URL would leak
/// into logs and referrers, so the URL carries a 60-second single-use ticket.
struct WsTicket {
    user_id: UserId,
    expires_at_ms: u64,
}

struct RoomPresence {
    generation: u64,
    sockets: HashSet<u64>,
}

/// Everything one server process shares between connections.
pub struct AppState {
    auth: Mutex<AuthService>,
    hub: Mutex<Hub>,
    sessions: RwLock<HashMap<String, Session>>,
    tickets: Mutex<HashMap<String, WsTicket>>,
    events: Mutex<HashMap<RoomAddress, broadcast::Sender<RoomEvent>>>,
    room_sockets: Mutex<HashMap<RoomAddress, RoomPresence>>,
    next_socket: AtomicU64,
    clock: Arc<dyn Clock>,
}

impl AppState {
    pub fn new(clock: Arc<dyn Clock>) -> Arc<Self> {
        Self::with_hub(clock.clone(), Hub::new(clock))
    }

    /// Builds state around an already-loaded hub (for persistent servers).
    pub fn with_hub(clock: Arc<dyn Clock>, hub: Hub) -> Arc<Self> {
        Arc::new(Self {
            auth: Mutex::new(AuthService::new(clock.clone())),
            hub: Mutex::new(hub),
            sessions: RwLock::new(HashMap::new()),
            tickets: Mutex::new(HashMap::new()),
            events: Mutex::new(HashMap::new()),
            room_sockets: Mutex::new(HashMap::new()),
            next_socket: AtomicU64::new(1),
            clock,
        })
    }

    pub fn room_socket_opened(&self, address: &RoomAddress, generation: u64) -> u64 {
        let socket_id = self.next_socket.fetch_add(1, Ordering::Relaxed);
        let mut sockets = self.room_sockets.lock().expect("room sockets lock");
        let presence = sockets
            .entry(address.clone())
            .or_insert_with(|| RoomPresence {
                generation,
                sockets: HashSet::new(),
            });
        if presence.generation != generation {
            presence.generation = generation;
            presence.sockets.clear();
        }
        presence.sockets.insert(socket_id);
        socket_id
    }

    /// Drops one socket. A socket from an older incarnation of the same address
    /// must not close a room that has since been recreated.
    pub fn room_socket_closed(&self, address: &RoomAddress, generation: u64, socket_id: u64) {
        let last = {
            let mut sockets = self.room_sockets.lock().expect("room sockets lock");
            let Some(presence) = sockets.get_mut(address) else {
                return;
            };
            if presence.generation != generation {
                return;
            }
            presence.sockets.remove(&socket_id);
            if presence.sockets.is_empty() {
                sockets.remove(address);
                true
            } else {
                false
            }
        };
        if !last {
            return;
        }
        let mut hub = self.hub.lock().expect("hub lock");
        if hub.room_epoch(address) != Some(generation) {
            return;
        }
        let _ = hub.close_room(address);
        self.events.lock().expect("events lock").remove(address);
    }

    pub fn room_epoch(&self, address: &RoomAddress) -> Option<u64> {
        self.hub.lock().expect("hub lock").room_epoch(address)
    }

    pub fn room_is_live(&self, address: &RoomAddress) -> bool {
        self.room_sockets
            .lock()
            .expect("room sockets lock")
            .get(address)
            .is_some_and(|presence| !presence.sockets.is_empty())
    }
    /// Registers a user out of band (admin provisioning).
    pub fn provision(
        &self,
        user_id: UserId,
        token: &str,
        totp_secret_base32: &str,
    ) -> Result<(), AuthError> {
        self.provision_as(user_id, token, totp_secret_base32, false)
    }

    pub fn provision_as(
        &self,
        user_id: UserId,
        token: &str,
        totp_secret_base32: &str,
        admin: bool,
    ) -> Result<(), AuthError> {
        self.auth
            .lock()
            .expect("auth lock")
            .provision_as(user_id, token, totp_secret_base32, admin)
    }

    pub(crate) fn is_admin(&self, user: &UserId) -> bool {
        self.auth.lock().expect("auth lock").is_admin(user)
    }

    pub(crate) fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    /// Resolves a bearer session id to its user, refusing expired sessions.
    pub(crate) fn session_user(&self, session_id: &str) -> Result<UserId, ApiError> {
        let session = {
            let sessions = self.sessions.read().expect("sessions lock");
            sessions.get(session_id).cloned()
        }
        .ok_or_else(|| ApiError::Unauthorized("unknown session".into()))?;

        if session.is_expired(self.now_ms()) {
            return Err(ApiError::Unauthorized("session expired".into()));
        }
        Ok(session.user_id)
    }

    /// Issues a single-use ticket for a WebSocket upgrade.
    pub(crate) fn issue_ticket(&self, user_id: UserId) -> (String, u64) {
        let ticket = generate_token();
        let expires_at_ms = self.now_ms() + 60_000;
        self.tickets.lock().expect("ticket lock").insert(
            ticket.clone(),
            WsTicket {
                user_id,
                expires_at_ms,
            },
        );
        (ticket, expires_at_ms)
    }

    /// Consumes a ticket. Consumption happens before the expiry check, so an
    /// expired ticket cannot be tried twice either.
    pub(crate) fn consume_ticket(&self, ticket: &str) -> Result<UserId, ApiError> {
        let entry = self
            .tickets
            .lock()
            .expect("ticket lock")
            .remove(ticket)
            .ok_or_else(|| ApiError::Unauthorized("unknown or used ticket".into()))?;
        if self.now_ms() >= entry.expires_at_ms {
            return Err(ApiError::Unauthorized("ticket expired".into()));
        }
        Ok(entry.user_id)
    }

    pub(crate) fn subscribe(&self, address: &RoomAddress) -> broadcast::Receiver<RoomEvent> {
        let mut channels = self.events.lock().expect("event lock");
        channels
            .entry(address.clone())
            .or_insert_with(|| broadcast::channel(128).0)
            .subscribe()
    }

    pub(crate) fn publish(&self, address: &RoomAddress, events: &[RoomEvent]) {
        if events.is_empty() {
            return;
        }
        let channels = self.events.lock().expect("event lock");
        if let Some(sender) = channels.get(address) {
            for event in events {
                // No subscribers is not an error: rooms can be empty.
                let _ = sender.send(event.clone());
            }
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/auth", post(routes::auth))
        .route("/v1/ws-ticket", post(routes::ws_ticket))
        .route("/v1/lobby", get(routes::lobby))
        .route("/v1/rooms", post(routes::create_room))
        .route("/v1/rooms/open", post(routes::open_room))
        .route("/v1/rooms/pin", post(routes::set_pin))
        .route("/v1/rooms/start-by-pin", post(routes::start_by_pin))
        .route("/v1/room/by-pin", get(routes::by_pin))
        .route("/v1/room", get(routes::room_view))
        .route("/v1/room/light", post(routes::light))
        .route("/v1/room/join", post(routes::join))
        .route("/v1/room/enter", post(routes::enter))
        .route("/v1/room/guest", post(routes::guest))
        .route("/v1/room/leave", post(routes::leave))
        .route("/v1/room/promote", post(routes::promote))
        .route("/v1/room/demote", post(routes::demote))
        .route("/v1/room/relay", post(routes::relay))
        .route("/v1/room/relay/heartbeat", post(routes::relay_heartbeat))
        .route("/v1/room/events", get(ws::room_events))
        // The Flutter web build is served from a different origin than the API.
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(state: Arc<AppState>, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use qr_race_core::Capital;
    use qr_race_hub::ManualClock;

    fn address() -> RoomAddress {
        RoomAddress::new(Capital::Tokyo, "A", "1B", "103").unwrap()
    }

    #[test]
    fn events_fan_out_to_subscribers() {
        let state = AppState::new(Arc::new(ManualClock::new(0)));
        let address = address();
        let mut receiver = state.subscribe(&address);

        let event = RoomEvent::RoomLit {
            address: address.clone(),
        };
        state.publish(&address, std::slice::from_ref(&event));

        assert_eq!(receiver.try_recv().unwrap(), event);
    }

    #[test]
    fn publishing_without_subscribers_is_harmless() {
        let state = AppState::new(Arc::new(ManualClock::new(0)));
        let address = address();
        state.publish(
            &address,
            &[RoomEvent::RoomLit {
                address: address.clone(),
            }],
        );
    }

    #[test]
    fn an_old_socket_cannot_close_a_recreated_room() {
        let state = AppState::new(Arc::new(ManualClock::new(0)));
        let address = address();
        let host = UserId("u-1".into());
        {
            let mut hub = state.hub.lock().expect("hub");
            hub.create_room(address.clone(), host.clone()).unwrap();
            hub.light_room(&host, &address).unwrap();
        }
        let old_epoch = state.room_epoch(&address).unwrap();
        let old_socket = state.room_socket_opened(&address, old_epoch);

        {
            let mut hub = state.hub.lock().expect("hub");
            hub.close_room(&address).unwrap();
            hub.create_room(address.clone(), host.clone()).unwrap();
            hub.light_room(&host, &address).unwrap();
        }
        let new_epoch = state.room_epoch(&address).unwrap();
        assert_ne!(new_epoch, old_epoch);
        let new_socket = state.room_socket_opened(&address, new_epoch);

        state.room_socket_closed(&address, old_epoch, old_socket);
        assert!(
            state.hub.lock().expect("hub").room_exists(&address),
            "old socket must not close the new room"
        );

        state.room_socket_closed(&address, new_epoch, new_socket);
        assert!(!state.hub.lock().expect("hub").room_exists(&address));
    }

    #[test]
    fn tickets_are_single_use_and_expire() {
        let clock = ManualClock::new(1_000);
        let state = AppState::new(Arc::new(clock.clone()));
        let user = UserId("u-1".into());

        let (ticket, expires_at_ms) = state.issue_ticket(user.clone());
        assert_eq!(expires_at_ms, 61_000);
        assert_eq!(state.consume_ticket(&ticket).unwrap(), user);
        assert!(state.consume_ticket(&ticket).is_err(), "single use");

        let (ticket, _) = state.issue_ticket(user);
        clock.advance(60_000);
        assert!(state.consume_ticket(&ticket).is_err(), "expired");
    }
}

//! Room event stream. One broadcast channel per room; a client that falls
//! behind is told it lagged rather than dropped silently.
//!
//! Browsers cannot set an Authorization header on a WebSocket, so the upgrade
//! takes `?ticket=` from `POST /v1/ws-ticket` (60s, single use) or the usual
//! bearer header.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::http::header;
use axum::response::Response;
use qr_race_core::UserId;
use qr_race_hub::RoomEvent;
use tokio::sync::broadcast;

use crate::AppState;
use crate::dto::EventDto;
use crate::error::ApiError;
use crate::routes::RoomQuery;

pub async fn room_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<RoomQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let address = query.address()?;
    let user = authenticate(&state, &headers, query.ticket.as_deref())?;

    let generation = {
        let mut hub = state.hub.lock().expect("hub lock");
        // Participants only, and this also proves the room exists.
        hub.view(&user, &address)?;
        hub.room_epoch(&address)
            .ok_or_else(|| ApiError::NotFound("room disappeared".into()))?
    };

    let Some(socket_id) = state.room_socket_opened(&address, generation) else {
        return Err(ApiError::NotFound("room incarnation is gone".into()));
    };
    if state.room_epoch(&address) != Some(generation) {
        state.room_socket_closed(&address, generation, socket_id);
        return Err(ApiError::NotFound("room disappeared".into()));
    }
    let receiver = state.subscribe(&address);
    let live_state = state.clone();
    let live_address = address.clone();
    Ok(ws.on_upgrade(move |socket| {
        stream(
            socket,
            receiver,
            live_state,
            live_address,
            generation,
            socket_id,
        )
    }))
}

fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
    ticket: Option<&str>,
) -> Result<UserId, ApiError> {
    if let Some(ticket) = ticket {
        return state.consume_ticket(ticket);
    }
    let header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::Unauthorized("missing ticket or Authorization header".into()))?;
    let session_id = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::Unauthorized("expected 'Bearer <session>'".into()))?;
    state.session_user(session_id)
}

async fn stream(
    mut socket: WebSocket,
    mut receiver: broadcast::Receiver<RoomEvent>,
    state: Arc<AppState>,
    address: qr_race_core::RoomAddress,
    generation: u64,
    socket_id: u64,
) {
    let mut keepalive = tokio::time::interval(Duration::from_secs(20));
    // The first interval tick is immediate; do not inject a ping before the
    // first room event.
    keepalive.tick().await;
    let mut awaiting_pong = false;
    loop {
        tokio::select! {
            _ = keepalive.tick() => {
                if awaiting_pong {
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
                awaiting_pong = true;
            }
            event = receiver.recv() => match event {
                Ok(event) => {
                    let Ok(text) = serde_json::to_string(&EventDto::from(event)) else {
                        break;
                    };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                // A slow client missed events; it must resync with GET /v1/room.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Pong(_))) => {
                    awaiting_pong = false;
                    continue;
                }
                Some(Ok(_)) => continue,
                Some(Err(_)) => break,
            },
        }
    }
    state.room_socket_closed(&address, generation, socket_id);
}


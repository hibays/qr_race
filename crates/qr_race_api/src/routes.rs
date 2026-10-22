//! REST handlers. Each one delegates the rule to the hub and turns the events
//! it returns into a room broadcast.

use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::StatusCode;
use axum::http::header;
use axum::http::request::Parts;
use data_encoding::BASE64;
use qr_race_core::{Capital, CoreError, RelayEntry, RoomAddress, UserId};
use qr_race_hub::{RelayRequest, RoomEvent, generate_token, generate_totp_secret_base32};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::dto::{
    AuthRequest, AuthResponse, CreateRoomRequest, LobbyDto, LobbyRoomDto, PinRoomResponse,
    PinStartRequest, PinStartResponse, RelayBody, RelayDto, RoomViewDto, SetPinRequest,
    TargetRequest, WsTicketResponse,
};
use crate::error::ApiError;

pub async fn health() -> &'static str {
    "ok"
}

pub async fn ws_ticket(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
) -> Json<WsTicketResponse> {
    let (ticket, expires_at_ms) = state.issue_ticket(user);
    Json(WsTicketResponse {
        ticket,
        expires_at_ms,
    })
}

pub async fn auth(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let session = {
        let mut auth = state.auth.lock().expect("auth lock");
        match body
            .code
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
        {
            Some(code) => auth.authenticate(&body.token, code)?,
            None => auth.authenticate_token(&body.token)?,
        }
    };
    state
        .sessions
        .write()
        .expect("sessions lock")
        .insert(session.id.clone(), session.clone());

    let admin = state.is_admin(&session.user_id);
    Ok(Json(AuthResponse {
        session_id: session.id,
        user_id: session.user_id,
        expires_at_ms: session.expires_at_ms,
        admin,
    }))
}

pub async fn lobby(State(state): State<Arc<AppState>>, Authed(_user): Authed) -> Json<LobbyDto> {
    let hub = state.hub.lock().expect("hub lock");
    Json(LobbyDto {
        rooms: hub
            .lit_rooms()
            .into_iter()
            .filter(|room| state.room_is_live(&room.address))
            .map(LobbyRoomDto::from)
            .collect(),
    })
}

pub async fn create_room(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Json(body): Json<CreateRoomRequest>,
) -> Result<(StatusCode, Json<RoomAddress>), ApiError> {
    let address = parse_address(&body.address)?;
    if let Some(pin) = body.pin.as_deref() {
        validate_pin(pin)?;
        let hub = state.hub.lock().expect("hub lock");
        if hub.resolve_pin(pin).is_some() {
            return Err(ApiError::BadRequest("PIN is already in use".into()));
        }
    }
    let events = {
        let mut hub = state.hub.lock().expect("hub lock");
        let events = hub.create_room(address.clone(), user.clone())?;
        if let Some(pin) = body.pin.as_deref() {
            hub.set_pin(&user, &address, pin)?;
        }
        events
    };
    // The channel exists from the moment the room does, so nobody misses the
    // events that follow.
    let _ = state.subscribe(&address);
    state.publish(&address, &events);
    Ok((StatusCode::CREATED, Json(address)))
}

pub async fn start_by_pin(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Json(body): Json<PinStartRequest>,
) -> Result<Json<PinStartResponse>, ApiError> {
    validate_pin(&body.pin)?;
    let mut hub = state.hub.lock().expect("hub lock");
    if let Some(address) = hub.resolve_pin(&body.pin) {
        return Ok(Json(PinStartResponse {
            address: address.to_string(),
            created: false,
        }));
    }
    let address = RoomAddress::new(
        Capital::Antarctica,
        body.pin.clone(),
        "1",
        "1",
    )
    .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    if hub.room_exists(&address) {
        return Err(ApiError::Conflict("PIN room already exists".into()));
    }
    hub.create_room(address.clone(), user.clone())?;
    hub.light_room(&user, &address)?;
    hub.set_pin(&user, &address, &body.pin)?;
    Ok(Json(PinStartResponse {
        address: address.to_string(),
        created: true,
    }))
}

pub async fn set_pin(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Json(body): Json<SetPinRequest>,
) -> Result<StatusCode, ApiError> {
    let address = parse_address(&body.address)?;
    let mut hub = state.hub.lock().expect("hub lock");
    hub.set_pin(&user, &address, &body.pin)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn by_pin(
    State(state): State<Arc<AppState>>,
    Authed(_user): Authed,
    Query(query): Query<PinQuery>,
) -> Result<Json<PinRoomResponse>, ApiError> {
    let pin = query.pin.trim();
    validate_pin(pin)?;
    let hub = state.hub.lock().expect("hub lock");
    let address = hub
        .resolve_pin(pin)
        .ok_or_else(|| ApiError::NotFound("no room for this PIN".into()))?;
    Ok(Json(PinRoomResponse {
        address: address.to_string(),
    }))
}

#[derive(Deserialize)]
pub struct PinQuery {
    pub pin: String,
}

pub async fn room_view(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Query(query): Query<RoomQuery>,
) -> Result<Json<RoomViewDto>, ApiError> {
    let address = query.address()?;
    let view = {
        let hub = state.hub.lock().expect("hub lock");
        hub.view(&user, &address)?
    };
    Ok(Json(RoomViewDto::from(&view)))
}

pub async fn light(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Query(query): Query<RoomQuery>,
) -> Result<StatusCode, ApiError> {
    let address = query.address()?;
    let events = {
        let mut hub = state.hub.lock().expect("hub lock");
        hub.light_room(&user, &address)?
    };
    state.publish(&address, &events);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn join(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Query(query): Query<RoomQuery>,
) -> Result<StatusCode, ApiError> {
    let address = query.address()?;
    let events = {
        let mut hub = state.hub.lock().expect("hub lock");
        hub.join(user, &address)?
    };
    state.publish(&address, &events);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct GuestResponse {
    user_id: String,
    token: String,
}

/// Mints a fresh member identity for one room. The caller must be that room's
/// host; the guest account is not tied to any other room.
pub async fn guest(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Query(query): Query<RoomQuery>,
) -> Result<Json<GuestResponse>, ApiError> {
    let address = query.address()?;
    {
        let hub = state.hub.lock().expect("hub lock");
        let view = hub.view(&user, &address)?;
        if view.host != user {
            return Err(ApiError::Forbidden(
                "only the room host can invite a member".to_string(),
            ));
        }
    }
    let token = generate_token();
    let secret = generate_totp_secret_base32();
    let guest = UserId(format!("guest-{}", &token[..12]));
    state.provision(guest.clone(), &token, &secret)?;
    Ok(Json(GuestResponse {
        user_id: guest.0,
        token,
    }))
}

pub async fn leave(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Query(query): Query<RoomQuery>,
) -> Result<StatusCode, ApiError> {
    let address = query.address()?;
    let events = {
        let mut hub = state.hub.lock().expect("hub lock");
        hub.leave(&user, &address)?
    };
    state.publish(&address, &events);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn promote(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Query(query): Query<RoomQuery>,
    Json(body): Json<TargetRequest>,
) -> Result<StatusCode, ApiError> {
    let address = query.address()?;
    let events = {
        let mut hub = state.hub.lock().expect("hub lock");
        hub.promote(&user, &body.target, &address)?
    };
    state.publish(&address, &events);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn demote(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Query(query): Query<RoomQuery>,
    Json(body): Json<TargetRequest>,
) -> Result<StatusCode, ApiError> {
    let address = query.address()?;
    let events = {
        let mut hub = state.hub.lock().expect("hub lock");
        hub.demote(&user, &body.target, &address)?
    };
    state.publish(&address, &events);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn relay(
    State(state): State<Arc<AppState>>,
    Authed(user): Authed,
    Query(query): Query<RoomQuery>,
    Json(body): Json<RelayBody>,
) -> Result<Json<RelayDto>, ApiError> {
    let address = query.address()?;
    let payload = BASE64
        .decode(body.payload_b64.as_bytes())
        .map_err(|_| ApiError::BadRequest("payload_b64 is not valid base64".into()))?;
    let request = RelayRequest {
        kind: body.kind,
        compression: body.compression,
        payload,
    };

    let events = {
        let mut hub = state.hub.lock().expect("hub lock");
        hub.publish_relay(&user, &address, request)?
    };
    state.publish(&address, &events);

    let relay = events
        .into_iter()
        .find_map(|event| match event {
            RoomEvent::RelayPublished { relay, .. } => Some(relay),
            _ => None,
        })
        .map(|relay: RelayEntry| RelayDto::from(&relay))
        .ok_or_else(|| ApiError::BadRequest("relay was not published".into()))?;
    Ok(Json(relay))
}

fn validate_pin(pin: &str) -> Result<(), ApiError> {
    if pin.len() != 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::BadRequest("PIN must be four digits".into()));
    }
    Ok(())
}

fn parse_address(value: &str) -> Result<RoomAddress, ApiError> {
    value
        .parse()
        .map_err(|error: CoreError| ApiError::BadRequest(error.to_string()))
}

/// A verified caller. The bearer token is the session id issued by `/v1/auth`.
pub struct Authed(pub UserId);

impl FromRequestParts<Arc<AppState>> for Authed {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("missing Authorization header".into()))?;
        let session_id = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthorized("expected 'Bearer <session>'".into()))?;

        Ok(Authed(state.session_user(session_id)?))
    }
}

#[derive(Debug, Deserialize)]
pub struct RoomQuery {
    pub address: String,
    /// Only meaningful on the WebSocket upgrade; REST callers use the header.
    #[serde(default)]
    pub ticket: Option<String>,
}

impl RoomQuery {
    pub(crate) fn address(&self) -> Result<RoomAddress, ApiError> {
        parse_address(&self.address)
    }
}

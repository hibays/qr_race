//! The HTTP surface, exercised through the router with no socket in the way.
//!
//! These tests are the contract a Flutter client codes against: status codes,
//! the bearer session, base64 payloads, and who is allowed to publish.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use qr_race_api::{AppState, router};
use qr_race_core::UserId;
use qr_race_hub::clock::{Clock, ManualClock};
use qr_race_hub::totp;
use serde_json::{Value, json};
use tower::ServiceExt;

const HOST_TOKEN: &str = "host-token-0123456789";
const HOST_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
const MEMBER_TOKEN: &str = "member-token-0123456789";
const MEMBER_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const ADMIN_TOKEN: &str = "admin-token-01234567890";
const ADMIN_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
const ADDRESS: &str = "东京A区1B级103房";

fn fixture() -> (Router, ManualClock, Arc<AppState>) {
    let clock = ManualClock::new(1_700_000_000_000);
    let state = AppState::new(Arc::new(clock.clone()));
    state
        .provision(UserId("host-a".into()), HOST_TOKEN, HOST_SECRET)
        .unwrap();
    state
        .provision(UserId("m-1".into()), MEMBER_TOKEN, MEMBER_SECRET)
        .unwrap();
    state
        .provision_as(UserId("admin".into()), ADMIN_TOKEN, ADMIN_SECRET, true)
        .unwrap();
    (router(state.clone()), clock, state)
}

fn request(method: Method, uri: &str, session: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(session) = session {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {session}"));
    }
    let body = match body {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    builder.body(body).unwrap()
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn room_uri(address: &str, action: &str) -> String {
    format!(
        "/v1/room{action}?address={}",
        utf8_percent_encode(address, NON_ALPHANUMERIC)
    )
}

async fn authenticate(app: &Router, clock: &ManualClock, token: &str, secret: &str) -> String {
    let code = totp::current_code(secret, clock.now_ms(), 6).unwrap();
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/v1/auth",
            None,
            Some(json!({ "token": token, "code": code })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["session_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn cors_preflight_is_allowed_for_the_web_client() {
    let (app, _clock, _state) = fixture();
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/v1/lobby")
        .header(header::ORIGIN, "http://localhost:5173")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "authorization,content-type",
        )
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "*"
    );
}

#[tokio::test]
async fn repeated_bad_codes_lock_the_account_temporarily() {
    let (app, clock, _state) = fixture();

    for _ in 0..qr_race_hub::auth::MAX_FAILED_ATTEMPTS {
        let (status, _) = send(
            &app,
            request(
                Method::POST,
                "/v1/auth",
                None,
                Some(json!({ "token": HOST_TOKEN, "code": "abcdef" })),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    let code = totp::current_code(HOST_SECRET, clock.now_ms(), 6).unwrap();
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/v1/auth",
            None,
            Some(json!({ "token": HOST_TOKEN, "code": code })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");

    clock.advance(qr_race_hub::auth::LOCKOUT_MS);
    let code = totp::current_code(HOST_SECRET, clock.now_ms(), 6).unwrap();
    let (status, _) = send(
        &app,
        request(
            Method::POST,
            "/v1/auth",
            None,
            Some(json!({ "token": HOST_TOKEN, "code": code })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the lock expires");
}

#[tokio::test]
async fn auth_reports_whether_the_account_is_an_admin() {
    let (app, clock, _state) = fixture();

    let admin_code = totp::current_code(ADMIN_SECRET, clock.now_ms(), 6).unwrap();
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/v1/auth",
            None,
            Some(json!({ "token": ADMIN_TOKEN, "code": admin_code })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["admin"], true);

    let member_code = totp::current_code(MEMBER_SECRET, clock.now_ms(), 6).unwrap();
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/v1/auth",
            None,
            Some(json!({ "token": MEMBER_TOKEN, "code": member_code })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["admin"], false);
}

#[tokio::test]
async fn open_room_creates_lights_and_enters_in_one_request() {
    let (app, clock, _state) = fixture();
    let host = authenticate(&app, &clock, HOST_TOKEN, HOST_SECRET).await;

    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/v1/rooms/open",
            Some(&host),
            Some(json!({ "address": ADDRESS })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["room"]["label"], ADDRESS);
    assert_eq!(body["room"]["lit"], true);
    assert!(
        body["ticket"]
            .as_str()
            .is_some_and(|ticket| !ticket.is_empty())
    );

    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/v1/rooms/open",
            Some(&host),
            Some(json!({ "address": ADDRESS })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["room"]["label"], ADDRESS);
}

#[tokio::test]
async fn the_mvp_flow_runs_over_http() {
    let (app, clock, state) = fixture();

    let (status, _) = send(&app, request(Method::GET, "/v1/health", None, None)).await;
    assert_eq!(status, StatusCode::OK);

    let host = authenticate(&app, &clock, HOST_TOKEN, HOST_SECRET).await;
    let member = authenticate(&app, &clock, MEMBER_TOKEN, MEMBER_SECRET).await;

    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/v1/rooms",
            Some(&host),
            Some(json!({ "address": ADDRESS })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, _) = send(
        &app,
        request(
            Method::POST,
            &room_uri(ADDRESS, "/light"),
            Some(&host),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = send(
        &app,
        request(
            Method::POST,
            &room_uri(ADDRESS, "/join"),
            Some(&member),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &room_uri(ADDRESS, "/enter"),
            Some(&member),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["room"]["label"], ADDRESS);
    assert!(
        body["ticket"]
            .as_str()
            .is_some_and(|ticket| !ticket.is_empty()),
        "{body}"
    );

    state.room_socket_opened(&ADDRESS.parse().unwrap());
    let (status, lobby) = send(&app, request(Method::GET, "/v1/lobby", Some(&member), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lobby["rooms"].as_array().unwrap().len(), 1);
    assert_eq!(lobby["rooms"][0]["label"], ADDRESS);

    let payload_b64 = data_encoding::BASE64.encode(b"relay-bytes");
    let (status, relay) = send(
        &app,
        request(
            Method::POST,
            &room_uri(ADDRESS, "/relay"),
            Some(&host),
            Some(json!({ "kind": "text", "payload_b64": payload_b64 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(relay["id"], "r-1");
    assert_eq!(relay["payload_b64"], payload_b64);
    assert_eq!(
        relay["hash"].as_str().expect("hash").len(),
        qr_race_core::CONTENT_HASH_BYTES * 2
    );

    let (status, view) = send(
        &app,
        request(Method::GET, &room_uri(ADDRESS, ""), Some(&member), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["relays"].as_array().unwrap().len(), 1);
    let host_member = view["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["user"] == "host-a")
        .unwrap();
    assert_eq!(host_member["role"], "host");
    assert_eq!(host_member["last_relay"], "r-1");

    // A member receives relays but cannot publish one.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &room_uri(ADDRESS, "/relay"),
            Some(&member),
            Some(json!({ "kind": "text", "payload_b64": payload_b64 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
async fn auth_failures_are_401() {
    let (app, clock, _state) = fixture();
    let code = totp::current_code(HOST_SECRET, clock.now_ms(), 6).unwrap();

    let (status, _) = send(
        &app,
        request(
            Method::POST,
            "/v1/auth",
            None,
            Some(json!({ "token": HOST_TOKEN, "code": "000000" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let host = authenticate(&app, &clock, HOST_TOKEN, HOST_SECRET).await;
    let _ = host;

    // The same step cannot authenticate twice.
    let (status, _) = send(
        &app,
        request(
            Method::POST,
            "/v1/auth",
            None,
            Some(json!({ "token": HOST_TOKEN, "code": code })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(
        &app,
        request(Method::GET, "/v1/lobby", Some("not-a-session"), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(&app, request(Method::GET, "/v1/lobby", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn room_errors_map_to_status_codes() {
    let (app, clock, _state) = fixture();
    let host = authenticate(&app, &clock, HOST_TOKEN, HOST_SECRET).await;
    let member = authenticate(&app, &clock, MEMBER_TOKEN, MEMBER_SECRET).await;

    let (status, _) = send(
        &app,
        request(
            Method::POST,
            "/v1/rooms",
            Some(&host),
            Some(json!({ "address": "东京A区" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "malformed address");

    let (status, _) = send(
        &app,
        request(
            Method::POST,
            "/v1/rooms",
            Some(&host),
            Some(json!({ "address": ADDRESS })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send(
        &app,
        request(
            Method::POST,
            "/v1/rooms",
            Some(&host),
            Some(json!({ "address": ADDRESS })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "duplicate room");

    // Joining before lighting is a conflict, not a crash.
    let (status, _) = send(
        &app,
        request(
            Method::POST,
            &room_uri(ADDRESS, "/join"),
            Some(&member),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Unknown room.
    let unknown = "西京Z区B级999房";
    let (status, _) = send(
        &app,
        request(Method::GET, &room_uri(unknown, ""), Some(&member), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A non-participant cannot read a room.
    let (status, _) = send(
        &app,
        request(
            Method::POST,
            &room_uri(ADDRESS, "/light"),
            Some(&host),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(
        &app,
        request(Method::GET, &room_uri(ADDRESS, ""), Some(&member), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Modifying without the host is forbidden.
    let (status, _) = send(
        &app,
        request(
            Method::POST,
            &room_uri(ADDRESS, "/light"),
            Some(&member),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn pin_start_uses_the_southern_namespace() {
    let (app, clock, _state) = fixture();
    let host = authenticate(&app, &clock, HOST_TOKEN, HOST_SECRET).await;
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/v1/rooms/start-by-pin",
            Some(&host),
            Some(json!({ "pin": "9999" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["address"], "南极9999");
    assert_eq!(body["created"], true);

    let (status, room) = send(
        &app,
        request(Method::GET, &room_uri("南极9999", ""), Some(&host), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(room["label"], "南极9999");
}

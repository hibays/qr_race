//! One real socket test: the WebSocket upgrade, ticket auth, and a relay
//! event crossing the wire. Everything else can be tested in-process; this is
//! the test that would catch a broken upgrade or a missing broadcast.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use qr_race_api::AppState;
use qr_race_core::UserId;
use qr_race_hub::clock::{Clock, ManualClock};
use qr_race_hub::totp;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const HOST_TOKEN: &str = "host-token-0123456789";
const HOST_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
const MEMBER_TOKEN: &str = "member-token-0123456789";
const MEMBER_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const ADDRESS: &str = "东京A区1B级103房";

async fn login(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    secret: &str,
    clock: &ManualClock,
) -> String {
    let code = totp::current_code(secret, clock.now_ms(), 6).unwrap();
    let response = client
        .post(format!("{base}/v1/auth"))
        .json(&json!({ "token": token, "code": code }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = response.json().await.unwrap();
    body["session_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn websocket_delivers_relay_events_over_a_real_socket() {
    let clock = ManualClock::new(1_700_000_000_000);
    let state = AppState::new(Arc::new(clock.clone()));
    state
        .provision(UserId("host-a".into()), HOST_TOKEN, HOST_SECRET)
        .unwrap();
    state
        .provision(UserId("m-1".into()), MEMBER_TOKEN, MEMBER_SECRET)
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { qr_race_api::serve(state, listener).await });

    let http = reqwest::Client::new();
    let base = format!("http://{addr}");
    let host = login(&http, &base, HOST_TOKEN, HOST_SECRET, &clock).await;
    let member = login(&http, &base, MEMBER_TOKEN, MEMBER_SECRET, &clock).await;
    let encoded = utf8_percent_encode(ADDRESS, NON_ALPHANUMERIC).to_string();

    let created = http
        .post(format!("{base}/v1/rooms"))
        .bearer_auth(&host)
        .json(&json!({ "address": ADDRESS }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status().as_u16(), 201);

    for (action, session) in [("light", &host), ("join", &member)] {
        let response = http
            .post(format!("{base}/v1/room/{action}?address={encoded}"))
            .bearer_auth(session)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 204, "{action}");
    }

    // A browser cannot set an Authorization header on a WebSocket, so it asks
    // for a single-use ticket and carries it in the URL.
    let ticket_body: Value = http
        .post(format!("{base}/v1/ws-ticket"))
        .bearer_auth(&member)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ticket = ticket_body["ticket"].as_str().unwrap();

    let url = format!("ws://{addr}/v1/room/events?address={encoded}&ticket={ticket}");
    let (mut socket, _) = connect_async(url).await.unwrap();

    let payload = data_encoding::BASE64.encode(b"hello relay");
    let relay = http
        .post(format!("{base}/v1/room/relay?address={encoded}"))
        .bearer_auth(&host)
        .json(&json!({ "kind": "text", "payload_b64": payload }))
        .send()
        .await
        .unwrap();
    assert_eq!(relay.status().as_u16(), 200);

    let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("the event should arrive within five seconds")
        .expect("the socket should stay open")
        .expect("the frame should decode");
    let Message::Text(text) = frame else {
        panic!("expected a text frame, got {frame:?}");
    };
    let event: Value = serde_json::from_str(text.as_str()).unwrap();
    assert_eq!(event["type"], "relay_published");
    assert_eq!(event["relay"]["id"], "r-1");
    assert_eq!(event["relay"]["payload_b64"], payload);

    server.abort();
}

#[tokio::test]
async fn a_ticket_cannot_be_used_twice() {
    let clock = ManualClock::new(1_700_000_000_000);
    let state = AppState::new(Arc::new(clock.clone()));
    state
        .provision(UserId("host-a".into()), HOST_TOKEN, HOST_SECRET)
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { qr_race_api::serve(state, listener).await });

    let http = reqwest::Client::new();
    let base = format!("http://{addr}");
    let host = login(&http, &base, HOST_TOKEN, HOST_SECRET, &clock).await;
    let encoded = utf8_percent_encode(ADDRESS, NON_ALPHANUMERIC).to_string();

    http.post(format!("{base}/v1/rooms"))
        .bearer_auth(&host)
        .json(&json!({ "address": ADDRESS }))
        .send()
        .await
        .unwrap();
    http.post(format!("{base}/v1/room/light?address={encoded}"))
        .bearer_auth(&host)
        .send()
        .await
        .unwrap();

    let ticket_body: Value = http
        .post(format!("{base}/v1/ws-ticket"))
        .bearer_auth(&host)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ticket = ticket_body["ticket"].as_str().unwrap();

    let url = format!("ws://{addr}/v1/room/events?address={encoded}&ticket={ticket}");
    let (socket, _) = connect_async(&url).await.unwrap();
    drop(socket);

    let second = connect_async(&url).await;
    assert!(second.is_err(), "the ticket was already consumed");

    server.abort();
}

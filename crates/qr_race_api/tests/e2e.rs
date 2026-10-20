//! The product loop, end to end: a real QR image is scanned by the real engine,
//! its bytes are published over HTTP, and another user's WebSocket receives them.
//!
//! This is the test that proves the parts agree with each other: scanner output
//! is byte-exact, the API carries those bytes untouched, and the event stream
//! delivers them to a member.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use qr_race_api::AppState;
use qr_race_core::UserId;
use qr_race_hub::clock::{Clock, ManualClock};
use qr_race_hub::totp;
use qr_race_scan::{GrayFrame, RxingDecoder, ScanEngine};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const HOST_TOKEN: &str = "host-token-0123456789";
const HOST_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
const MEMBER_TOKEN: &str = "member-token-0123456789";
const MEMBER_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const MEMBER2_TOKEN: &str = "member2-token-0123456789";
const MEMBER2_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
const ADDRESS: &str = "东京A区1B级103房";
const PAYLOAD: &str = "https://example.test/relay?n=42";

fn qr_frame(text: &str, scale: u32, quiet: u32) -> GrayFrame {
    let code = qrcode::QrCode::new(text.as_bytes()).expect("payload fits");
    let modules = code.width() as u32;
    let colors = code.to_colors();
    let side = (modules + quiet * 2) * scale;
    let mut data = vec![255u8; (side * side) as usize];

    for y in 0..modules {
        for x in 0..modules {
            if colors[(y * modules + x) as usize] == qrcode::types::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = (x + quiet) * scale + dx;
                        let py = (y + quiet) * scale + dy;
                        data[(py * side + px) as usize] = 0;
                    }
                }
            }
        }
    }
    GrayFrame::new(side, side, data).unwrap()
}

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
async fn a_scanned_code_travels_from_camera_to_member_screen() {
    // 1. "Camera": produce a frame containing one QR code and scan it.
    let frame = qr_frame(PAYLOAD, 4, 2);
    let engine = ScanEngine::new(Box::new(RxingDecoder::new()));
    let outcome = engine.scan(&frame).unwrap();
    assert_eq!(outcome.hits.len(), 1);
    let hit = &outcome.hits[0];
    assert_eq!(hit.raw, PAYLOAD.as_bytes(), "the relay carries exact bytes");

    // 2. Server with two provisioned users.
    let clock = ManualClock::new(1_700_000_000_000);
    let state = AppState::new(Arc::new(clock.clone()));
    state
        .provision(UserId("host-a".into()), HOST_TOKEN, HOST_SECRET)
        .unwrap();
    state
        .provision(UserId("m-1".into()), MEMBER_TOKEN, MEMBER_SECRET)
        .unwrap();
    state
        .provision(UserId("m-2".into()), MEMBER2_TOKEN, MEMBER2_SECRET)
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { qr_race_api::serve(state, listener).await });

    let http = reqwest::Client::new();
    let base = format!("http://{addr}");
    let host = login(&http, &base, HOST_TOKEN, HOST_SECRET, &clock).await;
    let member = login(&http, &base, MEMBER_TOKEN, MEMBER_SECRET, &clock).await;
    let member2 = login(&http, &base, MEMBER2_TOKEN, MEMBER2_SECRET, &clock).await;
    let encoded = utf8_percent_encode(ADDRESS, NON_ALPHANUMERIC).to_string();

    http.post(format!("{base}/v1/rooms"))
        .bearer_auth(&host)
        .json(&json!({ "address": ADDRESS }))
        .send()
        .await
        .unwrap();
    for (action, session) in [("light", &host), ("join", &member), ("join", &member2)] {
        let response = http
            .post(format!("{base}/v1/room/{action}?address={encoded}"))
            .bearer_auth(session)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 204, "{action}");
    }

    // 3. The member subscribes before the scan is published.
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
    let ticket2_body: Value = http
        .post(format!("{base}/v1/ws-ticket"))
        .bearer_auth(&member2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ticket2 = ticket2_body["ticket"].as_str().unwrap();
    let url2 = format!("ws://{addr}/v1/room/events?address={encoded}&ticket={ticket2}");
    let (mut socket2, _) = connect_async(url2).await.unwrap();

    // 4. Publish exactly the bytes the scanner produced.
    let payload_b64 = data_encoding::BASE64.encode(&hit.raw);
    let response = http
        .post(format!("{base}/v1/room/relay?address={encoded}"))
        .bearer_auth(&host)
        .json(&json!({ "kind": "url", "payload_b64": payload_b64 }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);

    // 5. The member receives an event that can rebuild the same QR code.
    let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("the relay should arrive")
        .expect("the socket should stay open")
        .expect("the frame should decode");
    let Message::Text(text) = frame else {
        panic!("expected a text frame, got {frame:?}");
    };
    let event: Value = serde_json::from_str(text.as_str()).unwrap();
    assert_eq!(event["type"], "relay_published");
    let delivered = event["relay"]["payload_b64"].as_str().unwrap();
    assert_eq!(
        data_encoding::BASE64.decode(delivered.as_bytes()).unwrap(),
        hit.raw
    );

    let frame2 = tokio::time::timeout(Duration::from_secs(5), socket2.next())
        .await
        .expect("the second member should receive the relay")
        .expect("the socket should stay open")
        .expect("the frame should decode");
    let Message::Text(text2) = frame2 else {
        panic!("expected a text frame, got {frame2:?}");
    };
    let event2: Value = serde_json::from_str(text2.as_str()).unwrap();
    assert_eq!(event2["type"], "relay_published");
    assert_eq!(
        event2["relay"]["payload_b64"],
        event["relay"]["payload_b64"]
    );

    server.abort();
}

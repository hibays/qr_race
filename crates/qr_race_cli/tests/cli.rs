//! The CLI is a real client: this test runs the compiled binary against a live
//! server, with a QR image generated on disk, and checks the relay arrived.

use std::net::TcpListener as StdTcpListener;
use std::sync::Arc;

use qr_race_api::AppState;
use qr_race_core::UserId;
use qr_race_hub::clock::{Clock, ManualClock};
use qr_race_hub::totp;
use qr_race_tls::{TlsMaterial, TlsSettings};
use serde_json::{Value, json};
use tokio::net::TcpListener;

const HOST_TOKEN: &str = "host-token-0123456789";
const HOST_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
const SCANNER_TOKEN: &str = "scanner-token-0123456789";
const SCANNER_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const ADDRESS: &str = "西京Z区B级103房";
const PAYLOAD: &str = "https://example.test/from-cli";

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

fn write_qr(path: &std::path::Path, text: &str) {
    let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
    let modules = code.width() as u32;
    let colors = code.to_colors();
    let scale = 6u32;
    let quiet = 4u32;
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

    image::GrayImage::from_raw(side, side, data)
        .unwrap()
        .save(path)
        .unwrap();
}

#[tokio::test]
async fn the_cli_trusts_the_built_in_self_signed_certificate() {
    let clock = ManualClock::new(1_700_000_000_000);
    let state = AppState::new(Arc::new(clock.clone()));
    state
        .provision(UserId("host-a".into()), HOST_TOKEN, HOST_SECRET)
        .unwrap();
    state
        .provision(UserId("scanner".into()), SCANNER_TOKEN, SCANNER_SECRET)
        .unwrap();

    let dir = std::env::temp_dir().join(format!("qr-scan-cli-tls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = TlsSettings {
        cert: dir.join("cert.pem"),
        key: dir.join("key.pem"),
        hosts: vec!["127.0.0.1".into()],
    };
    let material = TlsMaterial::load_or_generate(&settings).unwrap();

    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let app = qr_race_api::router(state);
    let server =
        tokio::spawn(async move { qr_race_tls::serve_tls(app, listener, &material).await });

    let cert_pem = std::fs::read(&settings.cert).unwrap();
    let certificate = reqwest::Certificate::from_pem(&cert_pem).unwrap();
    let http = reqwest::Client::builder()
        .add_root_certificate(certificate)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let base = format!("https://127.0.0.1:{}", addr.port());

    // The TLS acceptor needs a moment before the first request lands.
    for _ in 0..50 {
        if http.get(format!("{base}/v1/health")).send().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let host = login(&http, &base, HOST_TOKEN, HOST_SECRET, &clock).await;
    let scanner = login(&http, &base, SCANNER_TOKEN, SCANNER_SECRET, &clock).await;
    let encoded =
        percent_encoding::utf8_percent_encode(ADDRESS, percent_encoding::NON_ALPHANUMERIC)
            .to_string();

    http.post(format!("{base}/v1/rooms"))
        .bearer_auth(&host)
        .json(&json!({ "address": ADDRESS }))
        .send()
        .await
        .unwrap();
    for (action, session) in [("light", &host), ("join", &scanner)] {
        let response = http
            .post(format!("{base}/v1/room/{action}?address={encoded}"))
            .bearer_auth(session)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 204, "{action}");
    }
    http.post(format!("{base}/v1/room/promote?address={encoded}"))
        .bearer_auth(&host)
        .json(&json!({ "target": "scanner" }))
        .send()
        .await
        .unwrap();

    clock.advance(30_000);

    let image = dir.join("code.png");
    write_qr(&image, PAYLOAD);

    let bin = env!("CARGO_BIN_EXE_qr_scan");
    let args: Vec<String> = vec![
        "--server".into(),
        base.clone(),
        "--cacert".into(),
        settings.cert.to_string_lossy().into_owned(),
        "--token".into(),
        SCANNER_TOKEN.into(),
        "--room".into(),
        ADDRESS.into(),
        "--image".into(),
        image.to_string_lossy().into_owned(),
        "--kind".into(),
        "url".into(),
    ];

    let output =
        tokio::task::spawn_blocking(move || std::process::Command::new(bin).args(args).output())
            .await
            .unwrap()
            .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("published relay"), "stdout: {stdout}");

    let view: Value = http
        .get(format!("{base}/v1/room?address={encoded}"))
        .bearer_auth(&host)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let delivered = view["relays"][0]["payload_b64"].as_str().unwrap();
    assert_eq!(
        data_encoding::BASE64.decode(delivered.as_bytes()).unwrap(),
        PAYLOAD.as_bytes()
    );

    server.abort();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_cli_publishes_a_scanned_code() {
    let clock = ManualClock::new(1_700_000_000_000);
    let state = AppState::new(Arc::new(clock.clone()));
    state
        .provision(UserId("host-a".into()), HOST_TOKEN, HOST_SECRET)
        .unwrap();
    state
        .provision(UserId("scanner".into()), SCANNER_TOKEN, SCANNER_SECRET)
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { qr_race_api::serve(state, listener).await });

    let http = reqwest::Client::new();
    let base = format!("http://{addr}");
    let host = login(&http, &base, HOST_TOKEN, HOST_SECRET, &clock).await;
    let scanner = login(&http, &base, SCANNER_TOKEN, SCANNER_SECRET, &clock).await;
    let encoded =
        percent_encoding::utf8_percent_encode(ADDRESS, percent_encoding::NON_ALPHANUMERIC)
            .to_string();

    http.post(format!("{base}/v1/rooms"))
        .bearer_auth(&host)
        .json(&json!({ "address": ADDRESS }))
        .send()
        .await
        .unwrap();
    for (action, session) in [("light", &host), ("join", &scanner)] {
        let response = http
            .post(format!("{base}/v1/room/{action}?address={encoded}"))
            .bearer_auth(session)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 204, "{action}");
    }
    http.post(format!("{base}/v1/room/promote?address={encoded}"))
        .bearer_auth(&host)
        .json(&json!({ "target": "scanner" }))
        .send()
        .await
        .unwrap();

    // Move to a fresh TOTP step: the scanner already spent the previous one
    // joining, and a replayed step is refused by design.
    clock.advance(30_000);

    let dir = std::env::temp_dir().join(format!("qr-scan-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("code.png");
    write_qr(&image, PAYLOAD);

    let bin = env!("CARGO_BIN_EXE_qr_scan");
    let args: Vec<String> = vec![
        "--server".into(),
        base.clone(),
        "--token".into(),
        SCANNER_TOKEN.into(),
        "--room".into(),
        ADDRESS.into(),
        "--image".into(),
        image.to_string_lossy().into_owned(),
        "--kind".into(),
        "url".into(),
    ];

    let output =
        tokio::task::spawn_blocking(move || std::process::Command::new(bin).args(args).output())
            .await
            .unwrap()
            .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("published relay"), "stdout: {stdout}");

    let view: Value = http
        .get(format!("{base}/v1/room?address={encoded}"))
        .bearer_auth(&host)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let delivered = view["relays"][0]["payload_b64"].as_str().unwrap();
    assert_eq!(
        data_encoding::BASE64.decode(delivered.as_bytes()).unwrap(),
        PAYLOAD.as_bytes()
    );

    server.abort();
    let _ = std::fs::remove_dir_all(&dir);
}

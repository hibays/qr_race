//! Proves the generated certificate actually serves a TLS handshake a client can
//! trust: no "danger_accept_invalid_certs", just the certificate file we wrote.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{Router, routing::get};
use qr_race_tls::{TlsMaterial, TlsSettings, install_crypto_provider, serve_tls};

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "qr-race-tls-serve-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_https_with_a_certificate_the_client_can_trust() {
    install_crypto_provider();

    let dir = temp_dir();
    let settings = TlsSettings {
        cert: dir.join("cert.pem"),
        key: dir.join("key.pem"),
        hosts: vec!["localhost".into(), "127.0.0.1".into()],
    };
    let material = Arc::new(TlsMaterial::load_or_generate(&settings).unwrap());
    eprintln!("material ready");

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new().route("/health", get(|| async { "ok" }));

    let server_material = Arc::clone(&material);
    let server = tokio::spawn(async move { serve_tls(app, listener, &server_material).await });
    eprintln!("server spawned on port {port}");

    let certificate = reqwest::Certificate::from_pem(material.cert_pem().as_bytes()).unwrap();
    let client = reqwest::Client::builder()
        .add_root_certificate(certificate)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    eprintln!("client built");
    let url = format!("https://127.0.0.1:{port}/health");

    let mut last_error = None;
    for attempt in 0..30 {
        match tokio::time::timeout(Duration::from_secs(3), client.get(&url).send()).await {
            Ok(Ok(response)) => {
                assert_eq!(response.status(), 200);
                assert_eq!(response.text().await.unwrap(), "ok");
                server.abort();
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
            Ok(Err(error)) => {
                eprintln!("attempt {attempt}: {error}");
                last_error = Some(error.to_string());
            }
            Err(_) => {
                eprintln!("attempt {attempt}: timed out");
                last_error = Some("request timed out".to_string());
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    panic!("TLS server never answered: {last_error:?}");
}

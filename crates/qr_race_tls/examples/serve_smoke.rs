//! Manual TLS smoke server: generates material under `.refs/tls-smoke`, serves
//! `/health` over HTTPS on 127.0.0.1:18443, and runs until killed.
//!
//!   cargo run -p qr_race_tls --example serve_smoke
//!   curl.exe -k https://127.0.0.1:18443/health

use std::net::TcpListener;

use axum::{Router, routing::get};
use qr_race_tls::{TlsMaterial, TlsSettings, serve_tls};

#[tokio::main]
async fn main() {
    let dir = std::path::PathBuf::from(".refs/tls-smoke");
    std::fs::create_dir_all(&dir).unwrap();
    let settings = TlsSettings {
        cert: dir.join("cert.pem"),
        key: dir.join("key.pem"),
        hosts: vec!["localhost".into(), "127.0.0.1".into()],
    };
    let material = TlsMaterial::load_or_generate(&settings).unwrap();
    println!(
        "cert {} (generated: {})",
        settings.cert.display(),
        material.generated()
    );

    let listener = TcpListener::bind("127.0.0.1:18443").unwrap();
    let app = Router::new().route("/health", get(|| async { "ok" }));
    println!("listening on https://127.0.0.1:18443");
    serve_tls(app, listener, &material).await.unwrap();
}

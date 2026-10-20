//! In-process server used by the mobile self-host flow.
//!
//! It binds loopback only, uses the in-memory hub (no SQLite on mobile yet) and
//! can serve plain HTTP or TLS with either a generated certificate or files the
//! user picked. The phone talks to `127.0.0.1`, so TLS here is for parity with
//! the desktop server and for clients that refuse cleartext.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use qr_race_api::{AppState, router};
use qr_race_core::UserId;
use qr_race_hub::{SystemClock, generate_token, generate_totp_secret_base32};
use qr_race_tls::{TlsMaterial, TlsSettings, serve_tls_until};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// TLS choice for an embedded server.
#[derive(Debug, Clone)]
pub enum TlsMode {
    /// Plain HTTP on loopback.
    None,
    /// Generate `cert.pem`/`key.pem` under `directory` on first start, reuse
    /// them afterwards.
    SelfSigned { directory: PathBuf },
    /// Use an existing certificate/key pair, e.g. one picked from the phone.
    Files { cert: PathBuf, key: PathBuf },
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddedInfo {
    pub url: String,
    pub fingerprint: Option<String>,
    pub host_token: String,
    pub host_secret: String,
    pub member_token: String,
    pub member_secret: String,
}

/// Handle that owns the server thread; dropping it without `stop` leaves the
/// thread running, so callers should always call `stop`.
pub struct EmbeddedServer {
    info: EmbeddedInfo,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl EmbeddedServer {
    /// Starts on `127.0.0.1:port` and waits until the listener is bound.
    /// Pass port 0 to let the OS choose.
    pub fn start(port: u16, tls: TlsMode) -> Result<Self, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let join = std::thread::Builder::new()
            .name("qr-race-embedded".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(format!("runtime: {error}")));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let material = match &tls {
                        TlsMode::None => None,
                        TlsMode::SelfSigned { directory } => {
                            if let Err(error) = std::fs::create_dir_all(directory) {
                                let _ = ready_tx.send(Err(format!("tls dir: {error}")));
                                return;
                            }
                            let settings = TlsSettings {
                                cert: directory.join("cert.pem"),
                                key: directory.join("key.pem"),
                                hosts: vec!["localhost".into(), "127.0.0.1".into()],
                            };
                            match TlsMaterial::load_or_generate(&settings) {
                                Ok(material) => Some(material),
                                Err(error) => {
                                    let _ = ready_tx.send(Err(format!("tls: {error}")));
                                    return;
                                }
                            }
                        }
                        TlsMode::Files { cert, key } => {
                            let settings = TlsSettings {
                                cert: cert.clone(),
                                key: key.clone(),
                                hosts: vec!["localhost".into(), "127.0.0.1".into()],
                            };
                            match TlsMaterial::load_or_generate(&settings) {
                                Ok(material) => Some(material),
                                Err(error) => {
                                    let _ = ready_tx.send(Err(format!("tls files: {error}")));
                                    return;
                                }
                            }
                        }
                    };
                    let fingerprint = material
                        .as_ref()
                        .and_then(TlsMaterial::cert_fingerprint_sha256);
                    let scheme = if material.is_some() { "https" } else { "http" };

                    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = ready_tx.send(Err(format!("bind: {error}")));
                            return;
                        }
                    };
                    let actual_port = match listener.local_addr() {
                        Ok(address) => address.port(),
                        Err(error) => {
                            let _ = ready_tx.send(Err(format!("local_addr: {error}")));
                            return;
                        }
                    };

                    let clock = Arc::new(SystemClock);
                    let state = AppState::new(clock);
                    let host_token = generate_token();
                    let host_secret = generate_totp_secret_base32();
                    let member_token = generate_token();
                    let member_secret = generate_totp_secret_base32();
                    if let Err(error) =
                        state.provision(UserId("host".to_string()), &host_token, &host_secret)
                    {
                        let _ = ready_tx.send(Err(format!("provision host: {error}")));
                        return;
                    }
                    if let Err(error) =
                        state.provision(UserId("member".to_string()), &member_token, &member_secret)
                    {
                        let _ = ready_tx.send(Err(format!("provision member: {error}")));
                        return;
                    }

                    let info = EmbeddedInfo {
                        url: format!("{scheme}://127.0.0.1:{actual_port}"),
                        fingerprint,
                        host_token,
                        host_secret,
                        member_token,
                        member_secret,
                    };
                    if ready_tx.send(Ok(info)).is_err() {
                        return;
                    }

                    match material {
                        Some(material) => {
                            let _ =
                                serve_tls_until(router(state), listener, &material, async move {
                                    let _ = shutdown_rx.await;
                                })
                                .await;
                        }
                        None => {
                            let _ = axum::serve(listener, router(state))
                                .with_graceful_shutdown(async move {
                                    let _ = shutdown_rx.await;
                                })
                                .await;
                        }
                    }
                });
            })
            .map_err(|error| format!("thread: {error}"))?;

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(info)) => Ok(Self {
                info,
                shutdown: Some(shutdown_tx),
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(error) => {
                let _ = join.join();
                Err(format!("server did not start: {error}"))
            }
        }
    }

    pub fn info(&self) -> &EmbeddedInfo {
        &self.info
    }

    /// Signals shutdown and waits for the server thread to exit.
    pub fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_on_loopback_and_stops() {
        let server = EmbeddedServer::start(0, TlsMode::None).expect("server starts");
        assert!(server.info().url.starts_with("http://127.0.0.1:"));
        assert_eq!(server.info().host_secret.len(), 32);
        assert!(server.info().fingerprint.is_none());
        server.stop();
    }

    #[test]
    fn self_signed_tls_reports_a_fingerprint() {
        let dir = std::env::temp_dir().join(format!(
            "qr-race-embedded-tls-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let server = EmbeddedServer::start(
            0,
            TlsMode::SelfSigned {
                directory: dir.clone(),
            },
        )
        .expect("tls server starts");
        assert!(server.info().url.starts_with("https://127.0.0.1:"));
        assert_eq!(server.info().fingerprint.as_ref().unwrap().len(), 64);
        server.stop();
        let _ = std::fs::remove_dir_all(dir);
    }
}

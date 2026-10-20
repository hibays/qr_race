//! Self-signed TLS for local deployments: generate a certificate on first run,
//! reuse it afterwards, and serve the API over HTTPS. Clients trust the printed
//! certificate file; nothing here talks to a certificate authority.
//!
//! axum has no TLS of its own, so the accept loop is hyper-util driven:
//! accept → `tokio-rustls` handshake → hyper auto (HTTP/1.1 or HTTP/2) → axum.

use std::future::Future;
use std::net::{IpAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnectionBuilder;
use hyper_util::service::TowerToHyperService;
use rcgen::{CertificateParams, DnType, KeyPair, SanType};
use rustls::ServerConfig;
use thiserror::Error;
use tokio_rustls::TlsAcceptor;

#[derive(Debug, Clone)]
pub struct TlsSettings {
    pub cert: PathBuf,
    pub key: PathBuf,
    /// Names/IPs the certificate is valid for. Empty means localhost + loopback.
    pub hosts: Vec<String>,
}

#[derive(Debug)]
pub struct TlsMaterial {
    cert_pem: String,
    key_pem: String,
    generated: bool,
}

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("tls io: {0}")]
    Io(#[from] std::io::Error),

    #[error("certificate generation: {0}")]
    Certificate(#[from] rcgen::Error),

    #[error("tls config: {0}")]
    Config(String),
}

impl TlsMaterial {
    /// Loads `cert`/`key`, or generates a self-signed pair when neither exists.
    /// A half-present pair is an error: silently replacing one half would break
    /// every client that pinned the old certificate.
    pub fn load_or_generate(settings: &TlsSettings) -> Result<Self, TlsError> {
        let cert_exists = settings.cert.exists();
        let key_exists = settings.key.exists();
        match (cert_exists, key_exists) {
            (true, true) => Ok(Self {
                cert_pem: std::fs::read_to_string(&settings.cert)?,
                key_pem: std::fs::read_to_string(&settings.key)?,
                generated: false,
            }),
            (false, false) => {
                let hosts = if settings.hosts.is_empty() {
                    vec![
                        "localhost".to_string(),
                        "127.0.0.1".to_string(),
                        "::1".to_string(),
                    ]
                } else {
                    settings.hosts.clone()
                };

                let mut params = CertificateParams::default();
                params
                    .distinguished_name
                    .push(DnType::CommonName, "qr_race local server");
                params.subject_alt_names = hosts
                    .iter()
                    .map(|host| match host.parse::<IpAddr>() {
                        Ok(ip) => SanType::IpAddress(ip),
                        Err(_) => SanType::DnsName(
                            host.clone()
                                .try_into()
                                .expect("certificate host is a valid DNS name"),
                        ),
                    })
                    .collect();

                let key_pair = KeyPair::generate()?;
                let certificate = params.self_signed(&key_pair)?;
                let cert_pem = certificate.pem();
                let key_pem = key_pair.serialize_pem();

                if let Some(parent) = settings.cert.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&settings.cert, &cert_pem)?;
                std::fs::write(&settings.key, &key_pem)?;

                Ok(Self {
                    cert_pem,
                    key_pem,
                    generated: true,
                })
            }
            _ => Err(TlsError::Config(format!(
                "{} and {} must both exist or both be absent",
                settings.cert.display(),
                settings.key.display()
            ))),
        }
    }

    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    pub fn key_pem(&self) -> &str {
        &self.key_pem
    }

    /// True when this call wrote a fresh certificate.
    pub fn generated(&self) -> bool {
        self.generated
    }

    /// Lowercase hex SHA-256 of the leaf certificate's DER, for pinning in a
    /// client that cannot install a root CA (e.g. a Flutter build).
    pub fn cert_fingerprint_sha256(&self) -> Option<String> {
        let der = rustls_pemfile::certs(&mut self.cert_pem.as_bytes())
            .next()?
            .ok()?;
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(der.as_ref());
        let mut hex = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
        }
        Some(hex)
    }

    pub fn cert_path(settings: &TlsSettings) -> &Path {
        &settings.cert
    }
}

/// Installs the ring crypto provider once. rustls 0.23 requires this explicitly
/// when the dependency graph does not pin a provider; it is safe to call often.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn server_config(material: &TlsMaterial) -> Result<ServerConfig, TlsError> {
    let certificates = rustls_pemfile::certs(&mut material.cert_pem().as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| TlsError::Config(error.to_string()))?;
    let key = rustls_pemfile::private_key(&mut material.key_pem().as_bytes())
        .map_err(|error| TlsError::Config(error.to_string()))?
        .ok_or_else(|| TlsError::Config("the key file holds no private key".into()))?;

    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|error| TlsError::Config(error.to_string()))
}

/// Like [`serve_tls`], but stops accepting when `shutdown` resolves. Used by
/// the in-process mobile server so the UI can stop it.
pub async fn serve_tls_until<F>(
    router: Router,
    listener: tokio::net::TcpListener,
    material: &TlsMaterial,
    shutdown: F,
) -> Result<(), TlsError>
where
    F: Future<Output = ()>,
{
    install_crypto_provider();

    let acceptor = TlsAcceptor::from(Arc::new(server_config(material)?));
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let acceptor = acceptor.clone();
                let service = TowerToHyperService::new(router.clone());
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(stream).await else {
                        return;
                    };
                    let io = TokioIo::new(tls);
                    let _ = ConnectionBuilder::new(TokioExecutor::new())
                        .serve_connection_with_upgrades(io, service)
                        .await;
                });
            }
        }
    }
    Ok(())
}

/// Serves `router` over TLS on an already-bound listener until the process ends
/// or the listener fails. Each connection gets its own task; TLS handshakes that
/// fail are dropped without touching the other clients.
pub async fn serve_tls(
    router: Router,
    listener: TcpListener,
    material: &TlsMaterial,
) -> Result<(), TlsError> {
    install_crypto_provider();

    let acceptor = TlsAcceptor::from(Arc::new(server_config(material)?));
    listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(listener)?;

    loop {
        let (stream, _) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let service = TowerToHyperService::new(router.clone());
        tokio::spawn(async move {
            let Ok(tls) = acceptor.accept(stream).await else {
                return;
            };
            let io = TokioIo::new(tls);
            let _ = ConnectionBuilder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(io, service)
                .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "qr-race-tls-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn generates_once_then_reuses_the_same_material() {
        let dir = temp_dir();
        let settings = TlsSettings {
            cert: dir.join("cert.pem"),
            key: dir.join("key.pem"),
            hosts: vec!["localhost".into(), "127.0.0.1".into()],
        };

        let first = TlsMaterial::load_or_generate(&settings).unwrap();
        assert!(first.generated());
        assert!(settings.cert.exists() && settings.key.exists());
        assert!(first.cert_pem().contains("BEGIN CERTIFICATE"));

        let second = TlsMaterial::load_or_generate(&settings).unwrap();
        assert!(!second.generated());
        assert_eq!(second.cert_pem(), first.cert_pem());
        assert_eq!(second.key_pem(), first.key_pem());
        assert_eq!(
            second.cert_fingerprint_sha256(),
            first.cert_fingerprint_sha256()
        );
        let fingerprint = first.cert_fingerprint_sha256().unwrap();
        assert_eq!(fingerprint.len(), 64);
        assert!(fingerprint.chars().all(|c| c.is_ascii_hexdigit()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_half_present_pair_is_not_silently_replaced() {
        let dir = temp_dir();
        let settings = TlsSettings {
            cert: dir.join("cert.pem"),
            key: dir.join("key.pem"),
            hosts: Vec::new(),
        };
        std::fs::write(&settings.cert, "cert").unwrap();

        let error = TlsMaterial::load_or_generate(&settings).unwrap_err();
        assert!(matches!(error, TlsError::Config(_)));
        assert!(!settings.key.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

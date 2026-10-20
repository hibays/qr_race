//! The single server binary: HTTP + WebSocket for rooms and relays, plus an
//! operator command that mints a token and TOTP secret for one user.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use qr_race_api::AppState;
use qr_race_core::UserId;
use qr_race_hub::{Clock, Hub, SystemClock, generate_token, generate_totp_secret_base32};
use qr_race_store::Store;
use serde::Deserialize;

#[derive(Parser)]
#[command(
    name = "qr_race_server",
    version,
    about = "qr_race room + relay server"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the HTTP + WebSocket API.
    Serve {
        #[arg(long, default_value = "config.json")]
        config: PathBuf,
    },
    /// Mint a bearer token for one user.
    Provision {
        #[arg(long)]
        user: String,
        /// Mark the account as an admin (can upload modules).
        #[arg(long)]
        admin: bool,
    },
}

#[derive(Debug, Deserialize)]
struct Config {
    listen: String,
    /// SQLite file that keeps rooms and relays across restarts. Omitted means a
    /// purely in-memory server, which is fine for tests but not for production.
    #[serde(default)]
    database: Option<String>,
    /// Serve HTTPS directly with a generated (or provided) certificate.
    #[serde(default)]
    tls: Option<TlsConfig>,
    #[serde(default)]
    accounts: Vec<AccountConfig>,
}

#[derive(Debug, Deserialize)]
struct TlsConfig {
    #[serde(default = "default_cert_path")]
    cert: String,
    #[serde(default = "default_key_path")]
    key: String,
    /// Extra names/IPs for the certificate. Empty means localhost + loopback.
    #[serde(default)]
    hosts: Vec<String>,
}

fn default_cert_path() -> String {
    "cert.pem".to_string()
}

fn default_key_path() -> String {
    "key.pem".to_string()
}

#[derive(Debug, Deserialize)]
struct AccountConfig {
    user_id: String,
    token: String,
    /// Legacy field; clients no longer ask for TOTP. Empty is fine.
    #[serde(default)]
    totp_secret: String,
    /// Admins may upload modules; ordinary accounts cannot.
    #[serde(default)]
    admin: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve {
        config: PathBuf::from("config.json"),
    }) {
        Command::Provision { user, admin } => {
            print_provision(&user, admin);
            ExitCode::SUCCESS
        }
        Command::Serve { config } => match serve(&config).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("qr_race_server: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

fn print_provision(user: &str, admin: bool) {
    let token = generate_token();
    let account = serde_json::json!({
        "user_id": user,
        "token": token,
        "admin": admin,
    });
    println!("{account}");
}

async fn serve(config_path: &Path) -> Result<(), String> {
    let raw = std::fs::read_to_string(config_path)
        .map_err(|error| format!("{}: {error}", config_path.display()))?;
    let config: Config = serde_json::from_str(&raw)
        .map_err(|error| format!("{}: {error}", config_path.display()))?;

    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let state = match &config.database {
        Some(path) => {
            let store =
                Arc::new(Store::open(path).map_err(|error| format!("database {path}: {error}"))?);
            let hub = Hub::with_store(clock.clone(), store)
                .map_err(|error| format!("database {path}: {error}"))?;
            AppState::with_hub(clock, hub)
        }
        None => AppState::new(clock),
    };
    for account in &config.accounts {
        let secret = if account.totp_secret.is_empty() {
            generate_totp_secret_base32()
        } else {
            account.totp_secret.clone()
        };
        state
            .provision_as(
                UserId(account.user_id.clone()),
                &account.token,
                &secret,
                account.admin,
            )
            .map_err(|error| format!("account {}: {error}", account.user_id))?;
    }

    if let Some(tls) = &config.tls {
        let settings = qr_race_tls::TlsSettings {
            cert: PathBuf::from(&tls.cert),
            key: PathBuf::from(&tls.key),
            hosts: tls.hosts.clone(),
        };
        let material = qr_race_tls::TlsMaterial::load_or_generate(&settings)
            .map_err(|error| format!("tls: {error}"))?;
        let listener = std::net::TcpListener::bind(&config.listen)
            .map_err(|error| format!("bind {}: {error}", config.listen))?;
        let local = listener
            .local_addr()
            .map_err(|error| format!("local addr: {error}"))?;
        println!(
            "qr_race_server listening on https://{local} ({} accounts, database: {})",
            config.accounts.len(),
            config.database.as_deref().unwrap_or("memory")
        );
        if material.generated() {
            println!(
                "self-signed certificate written to {} (and key to {}); clients must trust it",
                settings.cert.display(),
                settings.key.display()
            );
        }
        if let Some(fingerprint) = material.cert_fingerprint_sha256() {
            println!("certificate SHA-256: {fingerprint}");
        }
        return qr_race_tls::serve_tls(qr_race_api::router(state), listener, &material)
            .await
            .map_err(|error| format!("serve: {error}"));
    }

    let listener = tokio::net::TcpListener::bind(&config.listen)
        .await
        .map_err(|error| format!("bind {}: {error}", config.listen))?;
    let local = listener
        .local_addr()
        .map_err(|error| format!("local addr: {error}"))?;
    println!(
        "qr_race_server listening on http://{local} ({} accounts, database: {})",
        config.accounts.len(),
        config.database.as_deref().unwrap_or("memory")
    );

    qr_race_api::serve(state, listener)
        .await
        .map_err(|error| format!("serve: {error}"))
}

//! qr_scan: the desktop scanner client in CLI form.
//!
//! It scans a still image or the desktop camera, authenticates to the server,
//! and publishes what it found as a relay. This is the reference path the
//! Flutter client will follow; it exists so the whole chain can be exercised
//! without a GUI.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use data_encoding::BASE64;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use qr_race_capture::{CaptureConfig, CaptureSession};
use qr_race_capture_desktop::{CameraConfig, CameraSource, list_cameras};
use qr_race_core::PayloadKind;
use qr_race_scan::{RxingDecoder, ScanEngine, ScanHit, load_gray_file};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "qr_scan",
    about = "Scan a QR image or camera and publish it as a relay"
)]
struct Cli {
    /// Base URL of the qr_race server, e.g. http://127.0.0.1:8080
    #[arg(long)]
    server: Option<String>,

    /// Trust this PEM certificate (the built-in self-signed HTTPS server).
    #[arg(long, conflicts_with = "insecure")]
    cacert: Option<PathBuf>,

    /// Accept any server certificate. Local testing only.
    #[arg(long)]
    insecure: bool,

    /// Bearer token provisioned by the operator.
    #[arg(long)]
    token: Option<String>,

    /// Room address, e.g. 东京A区1B级103房
    #[arg(long)]
    room: Option<String>,

    /// Image containing one or more QR codes.
    #[arg(long, conflicts_with = "camera")]
    image: Option<PathBuf>,

    /// Camera index; runs live capture instead of reading an image.
    #[arg(long)]
    camera: Option<u32>,

    /// How many camera frames to scan before giving up.
    #[arg(long, default_value_t = 90)]
    frames: u32,

    /// Requested capture width (with --height); default is the device's choice.
    #[arg(long, requires = "height")]
    width: Option<u32>,

    /// Requested capture height (with --width).
    #[arg(long, requires = "width")]
    height: Option<u32>,

    /// Requested capture frame rate when --width/--height are given.
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// text | url | json | binary
    #[arg(long, default_value = "text")]
    kind: String,

    /// Which hit to publish when several were found.
    #[arg(long, default_value_t = 0)]
    index: usize,

    /// Publish every hit instead of just one.
    #[arg(long)]
    all: bool,

    /// Scan and report, but do not publish (no server needed).
    #[arg(long)]
    probe: bool,

    /// Print the cameras the platform reports, then exit.
    #[arg(long)]
    list_cameras: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("qr_scan: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    if cli.list_cameras {
        for camera in list_cameras().map_err(|error| error.to_string())? {
            println!("{camera}");
        }
        return Ok(());
    }

    let kind = parse_kind(&cli.kind)?;
    let hits = match (&cli.image, cli.camera) {
        (Some(path), None) => scan_image(path)?,
        (None, Some(index)) => {
            let requested = match (cli.width, cli.height) {
                (Some(width), Some(height)) => Some(CameraConfig {
                    width,
                    height,
                    fps: cli.fps,
                }),
                _ => None,
            };
            scan_camera(index, cli.frames, requested)?
        }
        (Some(_), Some(_)) => return Err("pass --image or --camera, not both".into()),
        (None, None) => return Err("one of --image or --camera is required".into()),
    };

    if hits.is_empty() {
        return Err("no QR code found".into());
    }
    if cli.probe {
        for hit in &hits {
            println!("hit: {} bytes, {:?}", hit.raw.len(), hit.text);
        }
        return Ok(());
    }

    let server = required(cli.server, "--server")?;
    let token = required(cli.token, "--token")?;
    let room = required(cli.room, "--room")?;

    let selected: Vec<&ScanHit> = if cli.all {
        hits.iter().collect()
    } else {
        let hit = hits
            .get(cli.index)
            .ok_or_else(|| format!("index {} is out of range", cli.index))?;
        vec![hit]
    };

    let base = server.trim_end_matches('/').to_string();
    let http = build_http_client(cli.cacert.as_deref(), cli.insecure)?;
    let session = authenticate(&http, &base, &token).await?;
    let address = utf8_percent_encode(&room, NON_ALPHANUMERIC).to_string();

    for hit in selected {
        let payload_b64 = BASE64.encode(&hit.raw);
        let response = http
            .post(format!("{base}/v1/room/relay?address={address}"))
            .bearer_auth(&session)
            .json(&json!({ "kind": kind_name(kind), "payload_b64": payload_b64 }))
            .send()
            .await
            .map_err(|error| format!("relay request failed: {error}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| format!("relay body failed: {error}"))?;
        if !status.is_success() {
            return Err(format!("relay rejected ({status}): {body}"));
        }
        let relay: Value = serde_json::from_str(&body)
            .map_err(|error| format!("relay response was not JSON: {error}"))?;
        let id = relay["id"].as_str().unwrap_or("?");
        println!(
            "published relay {id}: {} bytes of {:?}",
            hit.raw.len(),
            hit.text
        );
    }
    Ok(())
}

fn scan_image(path: &PathBuf) -> Result<Vec<ScanHit>, String> {
    let frame = load_gray_file(path).map_err(|error| error.to_string())?;
    let engine = ScanEngine::new(Box::new(RxingDecoder::new()));
    let outcome = engine.scan(&frame).map_err(|error| error.to_string())?;
    println!(
        "scanned image {}x{} in {:.2} ms, {} hit(s)",
        frame.width(),
        frame.height(),
        outcome.timings.total.as_secs_f64() * 1000.0,
        outcome.hits.len()
    );
    Ok(outcome.hits)
}

/// Captures up to `frames` frames at 30fps and stops at the first hit.
fn scan_camera(
    index: u32,
    frames: u32,
    requested: Option<CameraConfig>,
) -> Result<Vec<ScanHit>, String> {
    let source = match requested {
        Some(config) => CameraSource::open_with(index, config),
        None => CameraSource::open(index),
    }
    .map_err(|error| error.to_string())?;
    let (width, height) = source.resolution();
    let config = CaptureConfig::new(width, height, 30).map_err(|error| error.to_string())?;
    let mut session = CaptureSession::new(source, config).map_err(|error| error.to_string())?;
    let engine = ScanEngine::new(Box::new(RxingDecoder::new()));
    let started = std::time::Instant::now();

    let mut hits = Vec::new();
    let mut scan_time = std::time::Duration::ZERO;
    let mut scanned = 0u32;
    for _ in 0..frames {
        let Some(frame) = session.next_frame().map_err(|error| error.to_string())? else {
            break;
        };
        let scan_started = std::time::Instant::now();
        let outcome = engine.scan(&frame).map_err(|error| error.to_string())?;
        scan_time += scan_started.elapsed();
        scanned += 1;
        if !outcome.hits.is_empty() {
            hits = outcome.hits;
            break;
        }
    }

    let average_scan_ms = if scanned == 0 {
        0.0
    } else {
        scan_time.as_secs_f64() * 1000.0 / f64::from(scanned)
    };
    println!(
        "captured {}x{}, {:?} in {}ms; scan avg {:.2} ms over {scanned} frame(s)",
        width,
        height,
        session.stats(),
        started.elapsed().as_millis(),
        average_scan_ms
    );
    Ok(hits)
}

async fn authenticate(http: &reqwest::Client, base: &str, token: &str) -> Result<String, String> {
    let response = http
        .post(format!("{base}/v1/auth"))
        .json(&json!({ "token": token }))
        .send()
        .await
        .map_err(|error| format!("auth request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("auth body failed: {error}"))?;
    if !status.is_success() {
        return Err(format!("auth rejected ({status}): {body}"));
    }
    let session: Value = serde_json::from_str(&body)
        .map_err(|error| format!("auth response was not JSON: {error}"))?;
    session["session_id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "auth response has no session_id".to_string())
}

fn required(value: Option<String>, flag: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("{flag} is required to publish"))
}

fn build_http_client(cacert: Option<&Path>, insecure: bool) -> Result<reqwest::Client, String> {
    if insecure {
        return reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|error| format!("could not build the http client: {error}"));
    }

    let mut builder = reqwest::Client::builder();
    if let Some(path) = cacert {
        let pem = std::fs::read(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let certificate = reqwest::Certificate::from_pem(&pem)
            .map_err(|error| format!("{} is not a PEM certificate: {error}", path.display()))?;
        builder = builder.add_root_certificate(certificate);
    }
    builder
        .build()
        .map_err(|error| format!("could not build the http client: {error}"))
}

fn parse_kind(value: &str) -> Result<PayloadKind, String> {
    match value {
        "text" => Ok(PayloadKind::Text),
        "url" => Ok(PayloadKind::Url),
        "json" => Ok(PayloadKind::Json),
        "binary" => Ok(PayloadKind::Binary),
        other => Err(format!(
            "unknown kind {other:?}; expected text, url, json or binary"
        )),
    }
}

fn kind_name(kind: PayloadKind) -> &'static str {
    match kind {
        PayloadKind::Text => "text",
        PayloadKind::Url => "url",
        PayloadKind::Json => "json",
        PayloadKind::Binary => "binary",
    }
}

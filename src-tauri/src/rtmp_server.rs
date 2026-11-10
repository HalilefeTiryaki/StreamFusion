//! Native in-process RTMP ingest server built on `rml_rtmp`.
//!
//! Accepts the RTMP publish from OBS on the configured port (default 1935),
//! performs the handshake and session negotiation in pure Rust, and pushes the
//! received audio/video tags into the [`StreamHub`] for the platform
//! forwarders to consume. No FFmpeg process is involved on the ingest side.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{
    ServerSession, ServerSessionConfig, ServerSessionEvent, ServerSessionResult,
};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

use crate::engine::log;
use crate::hub::{StreamHub, TAG_AUDIO, TAG_VIDEO};
use crate::models::IngestStatusEvent;

pub async fn run_ingest_server(
    app: AppHandle,
    port: u16,
    hub: Arc<StreamHub>,
    cancel: CancellationToken,
) {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            log(
                &app,
                "error",
                "ingest",
                format!("Cannot listen on port {port}: {e}. Is another RTMP server running?"),
            );
            emit_ingest(&app, "offline", None);
            return;
        }
    };

    emit_ingest(&app, "waiting", None);
    log(
        &app,
        "info",
        "ingest",
        format!("RTMP server listening — point OBS to rtmp://127.0.0.1:{port}/live (any stream key)"),
    );

    // Only one publisher may be active at a time.
    let publishing = Arc::new(AtomicBool::new(false));

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            accepted = listener.accept() => {
                let Ok((stream, addr)) = accepted else { continue };
                let app = app.clone();
                let hub = hub.clone();
                let cancel = cancel.clone();
                let publishing = publishing.clone();
                tauri::async_runtime::spawn(async move {
                    let was_publisher = AtomicBool::new(false);
                    let result = handle_connection(
                        &app, &hub, stream, &cancel, &publishing, &was_publisher,
                    )
                    .await;
                    // Clean up if this connection owned the publish session.
                    if was_publisher.load(Ordering::SeqCst) {
                        hub.end_session();
                        publishing.store(false, Ordering::SeqCst);
                        if !cancel.is_cancelled() {
                            emit_ingest(&app, "waiting", None);
                            log(&app, "info", "ingest", "OBS disconnected".to_string());
                        }
                    }
                    if let Err(e) = result {
                        log(&app, "warn", "ingest", format!("Connection from {addr} ended: {e}"));
                    }
                });
            }
        }
    }

    emit_ingest(&app, "offline", None);
}

async fn handle_connection(
    app: &AppHandle,
    hub: &Arc<StreamHub>,
    mut stream: TcpStream,
    cancel: &CancellationToken,
    publishing: &AtomicBool,
    was_publisher: &AtomicBool,
) -> Result<(), String> {
    let _ = stream.set_nodelay(true);
    let mut buf = vec![0u8; 8192];

    // Phase 1: RTMP handshake.
    let mut handshake = Handshake::new(PeerType::Server);
    let remaining = loop {
        let n = read_some(&mut stream, &mut buf, cancel).await?;
        if n == 0 {
            return Ok(());
        }
        match handshake
            .process_bytes(&buf[..n])
            .map_err(|e| format!("handshake failed: {e:?}"))?
        {
            HandshakeProcessResult::InProgress { response_bytes } => {
                write_all(&mut stream, &response_bytes).await?;
            }
            HandshakeProcessResult::Completed {
                response_bytes,
                remaining_bytes,
            } => {
                write_all(&mut stream, &response_bytes).await?;
                break remaining_bytes;
            }
        }
    };

    // Phase 2: RTMP session.
    let (mut session, initial_results) =
        ServerSession::new(ServerSessionConfig::new()).map_err(|e| format!("session: {e:?}"))?;

    let mut media_bytes: u64 = 0;
    let mut last_emit = Instant::now();

    process_results(
        app, hub, &mut session, &mut stream, initial_results,
        publishing, was_publisher, &mut media_bytes,
    )
    .await?;

    if !remaining.is_empty() {
        let results = session
            .handle_input(&remaining)
            .map_err(|e| format!("{e:?}"))?;
        process_results(
            app, hub, &mut session, &mut stream, results,
            publishing, was_publisher, &mut media_bytes,
        )
        .await?;
    }

    loop {
        let n = read_some(&mut stream, &mut buf, cancel).await?;
        if n == 0 {
            return Ok(());
        }
        let results = session
            .handle_input(&buf[..n])
            .map_err(|e| format!("{e:?}"))?;
        process_results(
            app, hub, &mut session, &mut stream, results,
            publishing, was_publisher, &mut media_bytes,
        )
        .await?;

        if was_publisher.load(Ordering::SeqCst) && last_emit.elapsed() >= Duration::from_secs(1) {
            let kbps = (media_bytes as f64 * 8.0 / 1000.0) / last_emit.elapsed().as_secs_f64();
            emit_ingest(app, "receiving", Some(kbps));
            media_bytes = 0;
            last_emit = Instant::now();
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_results(
    app: &AppHandle,
    hub: &Arc<StreamHub>,
    session: &mut ServerSession,
    stream: &mut TcpStream,
    results: Vec<ServerSessionResult>,
    publishing: &AtomicBool,
    was_publisher: &AtomicBool,
    media_bytes: &mut u64,
) -> Result<(), String> {
    let mut queue: VecDeque<ServerSessionResult> = results.into();

    while let Some(result) = queue.pop_front() {
        match result {
            ServerSessionResult::OutboundResponse(packet) => {
                write_all(stream, &packet.bytes).await?;
            }
            ServerSessionResult::RaisedEvent(event) => match event {
                ServerSessionEvent::ConnectionRequested { request_id, app_name } => {
                    log(app, "info", "ingest", format!("RTMP connect (app \"{app_name}\")"));
                    queue.extend(accept(session, request_id)?);
                }
                ServerSessionEvent::ReleaseStreamRequested { request_id, .. } => {
                    queue.extend(accept(session, request_id)?);
                }
                ServerSessionEvent::PublishStreamRequested {
                    request_id,
                    app_name,
                    stream_key,
                    ..
                } => {
                    if publishing.swap(true, Ordering::SeqCst) {
                        return Err("rejected: another source is already publishing".to_string());
                    }
                    was_publisher.store(true, Ordering::SeqCst);
                    hub.begin_session();
                    queue.extend(accept(session, request_id)?);
                    emit_ingest(app, "receiving", Some(0.0));
                    log(
                        app,
                        "info",
                        "ingest",
                        format!(
                            "OBS connected — publishing (app \"{app_name}\", key \"{}\")",
                            mask(&stream_key)
                        ),
                    );
                }
                ServerSessionEvent::PublishStreamFinished { .. } => {
                    if was_publisher.swap(false, Ordering::SeqCst) {
                        hub.end_session();
                        publishing.store(false, Ordering::SeqCst);
                        emit_ingest(app, "waiting", None);
                        log(app, "info", "ingest", "OBS stopped publishing".to_string());
                    }
                }
                ServerSessionEvent::AudioDataReceived { data, timestamp, .. } => {
                    *media_bytes += data.len() as u64;
                    hub.publish_media(TAG_AUDIO, timestamp.value, data.to_vec());
                }
                ServerSessionEvent::VideoDataReceived { data, timestamp, .. } => {
                    *media_bytes += data.len() as u64;
                    hub.publish_media(TAG_VIDEO, timestamp.value, data.to_vec());
                }
                // Metadata is intentionally dropped: FFmpeg derives everything
                // it needs from the AVC/AAC sequence headers and regenerates
                // onMetaData for each platform on the way out.
                _ => {}
            },
            ServerSessionResult::UnhandleableMessageReceived(_) => {}
        }
    }
    Ok(())
}

fn accept(session: &mut ServerSession, request_id: u32) -> Result<Vec<ServerSessionResult>, String> {
    session
        .accept_request(request_id)
        .map_err(|e| format!("accept_request: {e:?}"))
}

async fn read_some(
    stream: &mut TcpStream,
    buf: &mut [u8],
    cancel: &CancellationToken,
) -> Result<usize, String> {
    tokio::select! {
        _ = cancel.cancelled() => Ok(0),
        n = stream.read(buf) => n.map_err(|e| e.to_string()),
    }
}

async fn write_all(stream: &mut TcpStream, bytes: &[u8]) -> Result<(), String> {
    stream.write_all(bytes).await.map_err(|e| e.to_string())
}

fn mask(key: &str) -> String {
    if key.len() <= 4 {
        "***".to_string()
    } else {
        format!("{}***", &key[..2])
    }
}

fn emit_ingest(app: &AppHandle, status: &str, bitrate_kbps: Option<f64>) {
    let _ = app.emit(
        "ingest-status",
        IngestStatusEvent {
            status: status.to_string(),
            bitrate_kbps,
        },
    );
}

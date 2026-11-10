//! Stream relay engine.
//!
//! Architecture (all stream-copy, zero re-encoding):
//!
//! ```text
//! OBS ──rtmp──▶ [native Rust RTMP server (rml_rtmp)]
//!                 └─▶ StreamHub (broadcast of FLV tags)
//!                       ├─▶ [ffmpeg -c copy ◀ stdin] ──▶ Twitch
//!                       ├─▶ [ffmpeg -c copy ◀ stdin] ──▶ YouTube
//!                       ├─▶ [ffmpeg -c copy ◀ stdin] ──▶ Kick
//!                       └─▶ [ffmpeg -c copy ◀ stdin] ──▶ Facebook
//! ```
//!
//! The ingest side is pure Rust: a `rml_rtmp` server accepts the publish from
//! OBS and fans the media tags out through an in-process broadcast channel.
//! Each platform forwarder is an isolated, supervised FFmpeg process fed FLV
//! over stdin: it can be started/stopped independently while the engine keeps
//! running, and it auto-reconnects with exponential backoff if the platform
//! drops the link.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::hub::{HubMessage, SessionCache, StreamHub};
use crate::models::*;
use crate::rtmp_server;

/// A run longer than this resets the reconnect backoff.
const STABLE_RUN: Duration = Duration::from_secs(30);

pub struct Engine {
    app: AppHandle,
    hub: Arc<StreamHub>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    running: bool,
    ingest: Option<TaskHandle>,
    forwarders: HashMap<Platform, TaskHandle>,
}

struct TaskHandle {
    cancel: CancellationToken,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl TaskHandle {
    async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }
}

impl Engine {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            hub: Arc::new(StreamHub::new()),
            inner: Mutex::new(Inner::default()),
        }
    }

    pub async fn is_running(&self) -> bool {
        self.inner.lock().await.running
    }

    pub async fn start(&self, config: &AppConfig) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        if inner.running {
            return Err("Engine is already running".to_string());
        }
        inner.running = true;
        inner.ingest = Some(spawn_ingest(
            self.app.clone(),
            config.ingest_port,
            self.hub.clone(),
        ));

        for platform in Platform::ALL {
            let Some(cfg) = config.platforms.get(&platform) else {
                continue;
            };
            if !cfg.enabled {
                continue;
            }
            if cfg.stream_key.trim().is_empty() {
                log(
                    &self.app,
                    "warn",
                    platform.name(),
                    "Skipped: no stream key configured".to_string(),
                );
                continue;
            }
            inner.forwarders.insert(
                platform,
                spawn_forwarder(self.app.clone(), platform, cfg.clone(), self.hub.clone()),
            );
        }

        log(&self.app, "info", "engine", "Engine started".to_string());
        Ok(())
    }

    pub async fn stop(&self) {
        let (ingest, forwarders) = {
            let mut inner = self.inner.lock().await;
            if !inner.running {
                return;
            }
            inner.running = false;
            (inner.ingest.take(), std::mem::take(&mut inner.forwarders))
        };

        for (_, handle) in forwarders {
            handle.shutdown().await;
        }
        if let Some(handle) = ingest {
            handle.shutdown().await;
        }
        self.hub.end_session();

        let _ = self.app.emit(
            "ingest-status",
            IngestStatusEvent {
                status: "offline".to_string(),
                bitrate_kbps: None,
            },
        );
        log(&self.app, "info", "engine", "Engine stopped".to_string());
    }

    /// Starts or stops one platform's forwarder while the engine is running.
    /// A no-op (config change only) when the engine is stopped.
    pub async fn set_platform_live(
        &self,
        platform: Platform,
        cfg: &PlatformConfig,
        live: bool,
    ) -> Result<(), String> {
        let removed = {
            let mut inner = self.inner.lock().await;
            if !inner.running {
                return Ok(());
            }
            if live {
                if inner.forwarders.contains_key(&platform) {
                    return Ok(());
                }
                if cfg.stream_key.trim().is_empty() {
                    return Err(format!("{}: stream key is empty", platform.name()));
                }
                inner.forwarders.insert(
                    platform,
                    spawn_forwarder(self.app.clone(), platform, cfg.clone(), self.hub.clone()),
                );
                None
            } else {
                inner.forwarders.remove(&platform)
            }
        };
        if let Some(handle) = removed {
            handle.shutdown().await;
        }
        Ok(())
    }
}

fn spawn_ingest(app: AppHandle, port: u16, hub: Arc<StreamHub>) -> TaskHandle {
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tauri::async_runtime::spawn(async move {
        rtmp_server::run_ingest_server(app, port, hub, token).await;
    });
    TaskHandle { cancel, task }
}

// ---------------------------------------------------------------------------
// Forwarders: one supervised FFmpeg process per platform, fed FLV over stdin,
// with auto-reconnect.
// ---------------------------------------------------------------------------

fn spawn_forwarder(
    app: AppHandle,
    platform: Platform,
    cfg: PlatformConfig,
    hub: Arc<StreamHub>,
) -> TaskHandle {
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tauri::async_runtime::spawn(async move {
        forwarder_loop(app, platform, cfg, hub, token).await;
    });
    TaskHandle { cancel, task }
}

async fn forwarder_loop(
    app: AppHandle,
    platform: Platform,
    cfg: PlatformConfig,
    hub: Arc<StreamHub>,
    cancel: CancellationToken,
) {
    let name = platform.name();
    let output_url = format!(
        "{}/{}",
        cfg.server_url.trim().trim_end_matches('/'),
        cfg.stream_key.trim()
    );

    let mut session_rx = hub.session_watch();
    let mut reconnects: u64 = 0;
    let mut consecutive_failures: u32 = 0;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // Wait until OBS is actually publishing before launching FFmpeg.
        if *session_rx.borrow_and_update() == 0 {
            emit_status(
                &app,
                platform,
                PlatformStatus::Connecting,
                Some("Waiting for OBS source…".to_string()),
            );
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = session_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    continue;
                }
            }
        }

        emit_status(
            &app,
            platform,
            if reconnects == 0 {
                PlatformStatus::Connecting
            } else {
                PlatformStatus::Reconnecting
            },
            None,
        );

        let mut cmd = ffmpeg_cmd();
        cmd.stdin(Stdio::piped());
        cmd.args([
            "-hide_banner", "-loglevel", "warning",
            "-progress", "pipe:1", "-stats_period", "1",
            "-f", "flv", "-i", "pipe:0",
            "-c", "copy",
            "-f", "flv", &output_url,
        ]);

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                emit_status(
                    &app,
                    platform,
                    PlatformStatus::Error,
                    Some(format!("Failed to launch FFmpeg: {e}")),
                );
                log(&app, "error", name, format!("Failed to launch FFmpeg: {e}"));
                return;
            }
        };

        // Feed FLV from the hub into FFmpeg's stdin from a separate task.
        let writer_cancel = CancellationToken::new();
        let writer = child.stdin.take().map(|stdin| {
            let hub = hub.clone();
            let token = writer_cancel.clone();
            tauri::async_runtime::spawn(async move {
                feed_ffmpeg(hub, stdin, token).await;
            })
        });

        let started = Instant::now();
        let mut live_since: Option<Instant> = None;

        let reason = run_child(&app, &cancel, child, name, |kv| {
            if live_since.is_none() {
                live_since = Some(Instant::now());
                emit_status(&app, platform, PlatformStatus::Live, None);
                log(&app, "info", name, "Connected — stream is live".to_string());
            }
            let _ = app.emit(
                "platform-metrics",
                PlatformMetricsEvent {
                    platform,
                    bitrate_kbps: parse_bitrate(kv.get("bitrate")),
                    speed: parse_speed(kv.get("speed")),
                    fps: kv
                        .get("fps")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(0.0),
                    uptime_sec: live_since.map(|t| t.elapsed().as_secs()).unwrap_or(0),
                    reconnects,
                },
            );
        })
        .await;

        writer_cancel.cancel();
        if let Some(writer) = writer {
            let _ = writer.await;
        }

        match reason {
            ExitReason::Cancelled => break,
            ExitReason::Exited(code) => {
                if cancel.is_cancelled() {
                    break;
                }
                // If the OBS session ended, this exit is expected: loop back
                // and wait for the next session without burning backoff.
                if *session_rx.borrow_and_update() == 0 {
                    consecutive_failures = 0;
                    continue;
                }
                reconnects += 1;
                if started.elapsed() >= STABLE_RUN {
                    consecutive_failures = 1;
                } else {
                    consecutive_failures += 1;
                }
                let delay =
                    Duration::from_secs((1u64 << consecutive_failures.min(5)).min(30));
                let code = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                emit_status(
                    &app,
                    platform,
                    PlatformStatus::Reconnecting,
                    Some(format!(
                        "Connection lost (exit code {code}) — retrying in {}s",
                        delay.as_secs()
                    )),
                );
                log(
                    &app,
                    "warn",
                    name,
                    format!(
                        "FFmpeg exited (code {code}), reconnecting in {}s (attempt {reconnects})",
                        delay.as_secs()
                    ),
                );
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = sleep(delay) => {}
                }
            }
        }
    }

    emit_status(&app, platform, PlatformStatus::Idle, None);
}

/// Streams FLV (header + cached sequence headers + live tags) into FFmpeg's
/// stdin until the session ends, the receiver lags, or the write fails.
async fn feed_ffmpeg(hub: Arc<StreamHub>, mut stdin: ChildStdin, cancel: CancellationToken) {
    let (mut rx, cache) = hub.subscribe();

    // FLV header (audio + video) and PreviousTagSize0.
    let header: [u8; 13] = [0x46, 0x4c, 0x56, 0x01, 0x05, 0, 0, 0, 9, 0, 0, 0, 0];
    if stdin.write_all(&header).await.is_err() {
        return;
    }

    let SessionCache { video_seq, audio_seq } = cache;
    for tag in [video_seq, audio_seq].into_iter().flatten() {
        if write_tag(&mut stdin, tag.tag_type, 0, &tag.payload).await.is_err() {
            return;
        }
    }

    // Rebase timestamps so every FFmpeg session starts near zero.
    let mut base: Option<u32> = None;

    loop {
        let message = tokio::select! {
            _ = cancel.cancelled() => return,
            message = rx.recv() => message,
        };
        match message {
            Ok(HubMessage::Tag(tag)) => {
                let base = *base.get_or_insert(tag.timestamp);
                let ts = tag.timestamp.wrapping_sub(base);
                if write_tag(&mut stdin, tag.tag_type, ts, &tag.payload).await.is_err() {
                    return;
                }
            }
            // Graceful end: close stdin so FFmpeg flushes and exits cleanly.
            Ok(HubMessage::SessionEnded) => {
                let _ = stdin.shutdown().await;
                return;
            }
            // Fell behind the broadcast: restart the FFmpeg session rather
            // than sending a stream with a gap in it.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => return,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn write_tag(
    stdin: &mut ChildStdin,
    tag_type: u8,
    timestamp: u32,
    payload: &[u8],
) -> std::io::Result<()> {
    let size = payload.len() as u32;
    let mut header = [0u8; 11];
    header[0] = tag_type;
    header[1..4].copy_from_slice(&size.to_be_bytes()[1..]);
    header[4..7].copy_from_slice(&timestamp.to_be_bytes()[1..]);
    header[7] = (timestamp >> 24) as u8;
    // Bytes 8..11: stream id, always 0.
    stdin.write_all(&header).await?;
    stdin.write_all(payload).await?;
    stdin.write_all(&(11 + size).to_be_bytes()).await
}

// ---------------------------------------------------------------------------
// Shared process supervision helpers.
// ---------------------------------------------------------------------------

enum ExitReason {
    Cancelled,
    Exited(Option<i32>),
}

/// Runs a spawned FFmpeg child to completion: streams stderr into the log,
/// parses `-progress` key/value blocks from stdout, and kills the process if
/// the cancellation token fires.
async fn run_child(
    app: &AppHandle,
    cancel: &CancellationToken,
    mut child: Child,
    source: &'static str,
    mut on_progress: impl FnMut(&HashMap<String, String>),
) -> ExitReason {
    if let Some(stderr) = child.stderr.take() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if !line.is_empty() {
                    log(&app, "warn", source, line.to_string());
                }
            }
        });
    }

    let mut stdout_lines = child.stdout.take().map(|s| BufReader::new(s).lines());
    let mut kv: HashMap<String, String> = HashMap::new();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return ExitReason::Cancelled;
            }
            status = child.wait() => {
                return ExitReason::Exited(status.ok().and_then(|s| s.code()));
            }
            line = next_line_or_pending(&mut stdout_lines) => {
                match line {
                    Some(line) => {
                        if let Some((key, value)) = line.split_once('=') {
                            if key.trim() == "progress" {
                                on_progress(&kv);
                                kv.clear();
                            } else {
                                kv.insert(key.trim().to_string(), value.trim().to_string());
                            }
                        }
                    }
                    // stdout closed; keep waiting for the process to exit.
                    None => stdout_lines = None,
                }
            }
        }
    }
}

async fn next_line_or_pending(
    lines: &mut Option<Lines<BufReader<ChildStdout>>>,
) -> Option<String> {
    match lines {
        Some(lines) => lines.next_line().await.ok().flatten(),
        None => std::future::pending().await,
    }
}

pub fn ffmpeg_cmd() -> Command {
    let mut cmd = Command::new("ffmpeg");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

fn parse_bitrate(value: Option<&String>) -> f64 {
    value
        .and_then(|s| s.strip_suffix("kbits/s"))
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.0)
}

fn parse_speed(value: Option<&String>) -> f64 {
    value
        .and_then(|s| s.strip_suffix('x'))
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.0)
}

fn emit_status(app: &AppHandle, platform: Platform, status: PlatformStatus, message: Option<String>) {
    let _ = app.emit(
        "platform-status",
        PlatformStatusEvent {
            platform,
            status,
            message,
        },
    );
}

pub fn log(app: &AppHandle, level: &str, source: &str, message: String) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let _ = app.emit(
        "engine-log",
        LogEvent {
            ts,
            level: level.to_string(),
            source: source.to_string(),
            message,
        },
    );
}

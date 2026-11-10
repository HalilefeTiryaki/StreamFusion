# StreamFusion

High-performance desktop multi-streaming app. Receives **one** RTMP stream from OBS Studio and relays it simultaneously to **Twitch, YouTube, Kick, and Facebook** — with near-zero CPU overhead, because the video is never re-encoded.

Built with **Tauri 2** (Rust backend) + **React / TypeScript / TailwindCSS** frontend.

## How it works

```
OBS ──rtmp──▶ [native Rust RTMP server (rml_rtmp), in-process]
                └─▶ StreamHub (broadcast channel of FLV tags)
                      ├─▶ [ffmpeg -c copy ◀ stdin] ─▶ Twitch
                      ├─▶ [ffmpeg -c copy ◀ stdin] ─▶ YouTube
                      ├─▶ [ffmpeg -c copy ◀ stdin] ─▶ Kick
                      └─▶ [ffmpeg -c copy ◀ stdin] ─▶ Facebook
```

- The **ingest is pure Rust**: an in-process RTMP server built on the `rml_rtmp` crate accepts the publish from OBS (handshake, session negotiation, media demux — no FFmpeg involved). Any app/stream key is accepted; only one publisher at a time.
- Received audio/video tags flow through an in-process **broadcast hub** that caches the AVC/AAC sequence headers, so forwarders that (re)connect mid-stream can prime their decoder configuration instantly.
- Each platform gets its own isolated **forwarder** FFmpeg process, fed FLV over stdin in stream-copy mode. One platform dropping can never affect the others.
- Every forwarder is supervised by an async Rust task with **auto-reconnect and exponential backoff** (1s → 30s, reset after 30s of stable streaming). When OBS stops, forwarders close gracefully and re-arm for the next session without burning backoff.
- Platforms can be toggled on/off live, without interrupting the ingest or the other outputs.
- CPU / RAM / ingest bitrate / per-platform bitrate / mux speed are streamed to the dashboard once per second as Tauri events.

**Design note:** the ingest side is native Rust (`rml_rtmp`) for a robust always-on listener; the outbound legs use supervised FFmpeg processes in `-c copy` mode. For pure stream-copy relaying this is the better trade-off vs. full native bindings: identical performance (nothing is re-encoded either way), per-platform crash isolation, and free `rtmps://` TLS support for Kick/Facebook. All orchestration, supervision, metrics, and routing logic live in Rust tasks.

Because nothing is re-encoded, OBS encodes exactly once and StreamFusion's CPU cost is just packet I/O (typically <1–2 % CPU total). Your **upload bandwidth** must cover `OBS bitrate × number of enabled platforms`.

## Prerequisites

1. **Node.js 18+** (you have it if you can run `npm`)
2. **Rust toolchain** — install from [rustup.rs](https://rustup.rs) (on Windows this also prompts for the *Visual Studio C++ Build Tools*, which Tauri requires)
3. **FFmpeg** on your `PATH` — `winget install Gyan.FFmpeg` or download from [ffmpeg.org](https://ffmpeg.org/download.html). Verify with `ffmpeg -version`.
4. Windows: **WebView2** runtime (preinstalled on Windows 10/11)

## Run

```bash
npm install
npm run tauri dev      # development
npm run tauri build    # production installer (bundle/ output)
```

## Using it

1. Press **● GO LIVE** in StreamFusion. The engine starts and waits for OBS.
2. In OBS: **Settings → Stream**
   - Service: `Custom...`
   - Server: `rtmp://127.0.0.1:1935/live`
   - Stream Key: `streamfusion`
   (both values have copy buttons on the dashboard)
3. Click **Start Streaming** in OBS.
4. Toggle each platform on/off and paste its stream key. Toggles work live, mid-stream.

Platform stream keys are saved in `%APPDATA%/com.streamfusion.app/config.json` (plaintext — treat that file like a password).

### Recommended OBS output settings

Since the same encoded stream goes to every platform, encode to the strictest common denominator:

- Video: H.264 (x264 or NVENC), keyframe interval **2s**, CBR
- Audio: AAC, 160–320 kbps, 44.1/48 kHz
- Bitrate ≤ 6000 kbps if Twitch is enabled (Twitch's ceiling for non-partners)

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| Red banner "FFmpeg was not found" | Install FFmpeg and make sure `ffmpeg -version` works in a new terminal, then restart the app |
| Ingest stuck on "Waiting for OBS" | Check OBS uses Server `rtmp://127.0.0.1:1935/live` (any key works), and that port 1935 isn't used by another app |
| A platform loops "Reconnecting" | Wrong stream key or server URL for that platform (check the Activity Log for the FFmpeg error) — Kick in particular uses a per-account ingest URL from your creator dashboard |
| Facebook/Kick fail instantly | Those require `rtmps://` — make sure your FFmpeg build supports TLS (`ffmpeg -protocols | findstr rtmps`) |

## Project layout

```
streamfusion/
├── src/                     # React dashboard (TypeScript + Tailwind v4)
│   ├── App.tsx              # State, Tauri event wiring, layout
│   └── components/          # IngestCard, PlatformCard, SystemBar, LogPanel
├── src-tauri/
│   ├── src/rtmp_server.rs   # Native Rust RTMP ingest server (rml_rtmp)
│   ├── src/hub.rs           # FLV tag broadcast hub + sequence-header cache
│   ├── src/engine.rs        # Forwarder supervision, auto-reconnect
│   ├── src/commands.rs      # Tauri IPC commands
│   ├── src/metrics.rs       # CPU/RAM monitor (sysinfo)
│   ├── src/config.rs        # JSON config persistence
│   └── src/models.rs        # Shared types / event payloads
└── scripts/generate-icons.mjs  # Regenerates src-tauri/icons (npm run icons)
```

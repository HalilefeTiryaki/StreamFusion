import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppConfig,
  IngestStatus,
  LogEntry,
  Platform,
  PlatformConfig,
  PlatformMetrics,
  PlatformStatus,
  SystemMetrics,
} from "./types";
import { PLATFORMS } from "./types";
import { IngestCard } from "./components/IngestCard";
import { PlatformCard } from "./components/PlatformCard";
import { SystemBar } from "./components/SystemBar";
import { LogPanel } from "./components/LogPanel";

interface PlatformUiState {
  status: PlatformStatus;
  message?: string;
}

const idleStates = (): Record<Platform, PlatformUiState> =>
  Object.fromEntries(PLATFORMS.map((p) => [p, { status: "idle" }])) as Record<
    Platform,
    PlatformUiState
  >;

export default function App() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [running, setRunning] = useState(false);
  const [busy, setBusy] = useState(false);
  const [ffmpegVersion, setFfmpegVersion] = useState<string | null>(null);
  const [ffmpegMissing, setFfmpegMissing] = useState(false);
  const [ingest, setIngest] = useState<IngestStatus>({ status: "offline" });
  const [states, setStates] = useState<Record<Platform, PlatformUiState>>(idleStates());
  const [metrics, setMetrics] = useState<Partial<Record<Platform, PlatformMetrics>>>({});
  const [sys, setSys] = useState<SystemMetrics | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);

  const pushLog = useCallback((entry: LogEntry) => {
    setLogs((prev) => [...prev.slice(-299), entry]);
  }, []);

  const uiError = useCallback(
    (message: string) =>
      pushLog({ ts: Date.now(), level: "error", source: "ui", message }),
    [pushLog],
  );

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    (async () => {
      const [cfg, isRunning] = await Promise.all([
        invoke<AppConfig>("get_config"),
        invoke<boolean>("engine_status"),
      ]);
      if (disposed) return;
      setConfig(cfg);
      setRunning(isRunning);

      invoke<string>("check_ffmpeg")
        .then(setFfmpegVersion)
        .catch(() => setFfmpegMissing(true));

      unlisteners.push(
        await listen<SystemMetrics>("system-metrics", (e) => setSys(e.payload)),
        await listen<IngestStatus>("ingest-status", (e) => setIngest(e.payload)),
        await listen<{ platform: Platform; status: PlatformStatus; message?: string }>(
          "platform-status",
          (e) => {
            const { platform, status, message } = e.payload;
            setStates((prev) => ({ ...prev, [platform]: { status, message } }));
            if (status === "idle") {
              setMetrics((prev) => ({ ...prev, [platform]: undefined }));
            }
          },
        ),
        await listen<PlatformMetrics>("platform-metrics", (e) =>
          setMetrics((prev) => ({ ...prev, [e.payload.platform]: e.payload })),
        ),
        await listen<LogEntry>("engine-log", (e) => pushLog(e.payload)),
      );
      if (disposed) unlisteners.forEach((un) => un());
    })();

    return () => {
      disposed = true;
      unlisteners.forEach((un) => un());
    };
  }, [pushLog]);

  const toggleMaster = async () => {
    setBusy(true);
    try {
      if (running) {
        await invoke("stop_engine");
        setRunning(false);
        setStates(idleStates());
        setMetrics({});
        setIngest({ status: "offline" });
      } else {
        await invoke("start_engine");
        setRunning(true);
      }
    } catch (err) {
      uiError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const updateDraft = (platform: Platform, next: PlatformConfig) => {
    setConfig((prev) =>
      prev ? { ...prev, platforms: { ...prev.platforms, [platform]: next } } : prev,
    );
  };

  const commitConfig = async (platform: Platform, next: PlatformConfig) => {
    updateDraft(platform, next);
    try {
      await invoke("set_platform_config", { platform, config: next });
    } catch (err) {
      uiError(String(err));
    }
  };

  const togglePlatform = async (platform: Platform, live: boolean) => {
    if (!config) return;
    updateDraft(platform, { ...config.platforms[platform], enabled: live });
    try {
      await invoke("set_platform_live", { platform, live });
    } catch (err) {
      updateDraft(platform, { ...config.platforms[platform], enabled: !live });
      uiError(String(err));
    }
  };

  if (!config) {
    return (
      <div className="flex h-screen items-center justify-center text-sm text-slate-500">
        Loading StreamFusion…
      </div>
    );
  }

  const totalKbps = PLATFORMS.reduce(
    (acc, p) =>
      states[p].status === "live" ? acc + (metrics[p]?.bitrateKbps ?? 0) : acc,
    0,
  );
  const liveCount = PLATFORMS.filter((p) => states[p].status === "live").length;

  return (
    <div className="flex h-screen flex-col">
      <header className="flex shrink-0 items-center justify-between border-b border-line bg-panel/80 px-6 py-3.5 backdrop-blur">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-violet-500 to-sky-500 text-xl font-black text-white shadow-lg shadow-violet-500/20">
            S
          </div>
          <div>
            <h1 className="text-base font-bold tracking-tight">StreamFusion</h1>
            <p className="text-xs text-slate-500">One stream in, everywhere out</p>
          </div>
        </div>
        <div className="flex items-center gap-4">
          {ffmpegVersion && (
            <span className="hidden max-w-72 truncate text-xs text-slate-600 lg:block">
              {ffmpegVersion}
            </span>
          )}
          <button
            onClick={toggleMaster}
            disabled={busy || ffmpegMissing}
            className={`rounded-xl px-6 py-2.5 text-sm font-bold tracking-wide text-white shadow-lg transition-all disabled:cursor-not-allowed disabled:opacity-40 ${
              running
                ? "bg-rose-600 shadow-rose-600/25 hover:bg-rose-500"
                : "bg-emerald-600 shadow-emerald-600/25 hover:bg-emerald-500"
            }`}
          >
            {busy ? "WORKING…" : running ? "■ STOP ALL" : "● GO LIVE"}
          </button>
        </div>
      </header>

      {ffmpegMissing && (
        <div className="shrink-0 border-b border-rose-900/50 bg-rose-950/40 px-6 py-2 text-sm text-rose-300">
          FFmpeg was not found on your PATH. Install it (ffmpeg.org) and restart
          StreamFusion — the relay engine depends on it.
        </div>
      )}

      <main className="flex-1 overflow-y-auto px-6 py-5">
        <div className="mx-auto flex max-w-6xl flex-col gap-5">
          <IngestCard ingest={ingest} port={config.ingestPort} running={running} />
          <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
            {PLATFORMS.map((p) => (
              <PlatformCard
                key={p}
                platform={p}
                config={config.platforms[p]}
                status={states[p].status}
                statusMessage={states[p].message}
                metrics={metrics[p]}
                engineRunning={running}
                onToggle={(live) => togglePlatform(p, live)}
                onDraftChange={(next) => updateDraft(p, next)}
                onCommit={(next) => commitConfig(p, next)}
              />
            ))}
          </div>
          <LogPanel logs={logs} />
        </div>
      </main>

      <SystemBar sys={sys} ingest={ingest} totalKbps={totalKbps} liveCount={liveCount} />
    </div>
  );
}

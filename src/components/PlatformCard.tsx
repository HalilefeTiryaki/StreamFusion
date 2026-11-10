import { useState } from "react";
import type {
  Platform,
  PlatformConfig,
  PlatformMetrics,
  PlatformStatus,
} from "../types";
import { PLATFORM_META } from "../platforms";
import { formatKbps, formatUptime } from "../format";
import { Toggle } from "./Toggle";

const STATUS_LABEL: Record<PlatformStatus, string> = {
  idle: "Idle",
  connecting: "Connecting",
  live: "LIVE",
  reconnecting: "Reconnecting",
  error: "Error",
};

const STATUS_CLASS: Record<PlatformStatus, string> = {
  idle: "bg-slate-700/60 text-slate-400",
  connecting: "bg-amber-500/15 text-amber-300 animate-pulse",
  live: "bg-emerald-500/15 text-emerald-300",
  reconnecting: "bg-orange-500/15 text-orange-300 animate-pulse",
  error: "bg-rose-500/15 text-rose-300",
};

export function PlatformCard({
  platform,
  config,
  status,
  statusMessage,
  metrics,
  engineRunning,
  onToggle,
  onDraftChange,
  onCommit,
}: {
  platform: Platform;
  config: PlatformConfig;
  status: PlatformStatus;
  statusMessage?: string;
  metrics?: PlatformMetrics;
  engineRunning: boolean;
  onToggle: (live: boolean) => void;
  onDraftChange: (next: PlatformConfig) => void;
  onCommit: (next: PlatformConfig) => void;
}) {
  const meta = PLATFORM_META[platform];
  const [showKey, setShowKey] = useState(false);

  return (
    <section
      className="rounded-2xl border border-line bg-panel p-5 transition-shadow"
      style={{
        boxShadow:
          status === "live"
            ? `0 0 0 1px ${meta.color}55, 0 0 28px -10px ${meta.color}88`
            : undefined,
      }}
    >
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <span
            className="h-2.5 w-2.5 rounded-full"
            style={{ backgroundColor: meta.color }}
          />
          <h2 className="font-semibold">{meta.label}</h2>
          <span
            className={`rounded-full px-2.5 py-0.5 text-[11px] font-semibold ${STATUS_CLASS[status]}`}
          >
            {STATUS_LABEL[status]}
          </span>
        </div>
        <Toggle checked={config.enabled} onChange={onToggle} />
      </div>

      <div className="flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs text-slate-400">Server URL</span>
          <input
            className="input"
            value={config.serverUrl}
            spellCheck={false}
            onChange={(e) => onDraftChange({ ...config, serverUrl: e.target.value })}
            onBlur={(e) => onCommit({ ...config, serverUrl: e.target.value.trim() })}
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-xs text-slate-400">Stream Key</span>
          <div className="flex gap-2">
            <input
              className="input flex-1"
              type={showKey ? "text" : "password"}
              value={config.streamKey}
              placeholder="Paste your stream key"
              spellCheck={false}
              onChange={(e) => onDraftChange({ ...config, streamKey: e.target.value })}
              onBlur={(e) => onCommit({ ...config, streamKey: e.target.value.trim() })}
            />
            <button
              type="button"
              className="btn-ghost"
              onClick={() => setShowKey((v) => !v)}
            >
              {showKey ? "Hide" : "Show"}
            </button>
          </div>
          <span className="text-[10px] text-slate-600">{meta.keyHint}</span>
        </label>
      </div>

      <div className="mt-4 grid grid-cols-4 gap-2 text-center">
        <Stat label="Bitrate" value={metrics ? formatKbps(metrics.bitrateKbps) : "—"} />
        <Stat label="Speed" value={metrics ? `${metrics.speed.toFixed(2)}x` : "—"} />
        <Stat label="Uptime" value={metrics ? formatUptime(metrics.uptimeSec) : "—"} />
        <Stat label="Retries" value={metrics ? String(metrics.reconnects) : "—"} />
      </div>

      {statusMessage && (
        <p className="mt-3 text-xs text-amber-300/90">{statusMessage}</p>
      )}
      {!engineRunning && config.enabled && (
        <p className="mt-3 text-xs text-slate-500">
          Armed — goes live when the engine starts.
        </p>
      )}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg bg-panel-2 px-2 py-1.5">
      <div className="text-[10px] uppercase tracking-wide text-slate-500">{label}</div>
      <div className="text-sm font-semibold tabular-nums">{value}</div>
    </div>
  );
}

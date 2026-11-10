import { useState } from "react";
import type { IngestStatus } from "../types";
import { copyText, formatKbps } from "../format";

const OBS_STREAM_KEY = "streamfusion";

export function IngestCard({
  ingest,
  port,
  running,
}: {
  ingest: IngestStatus;
  port: number;
  running: boolean;
}) {
  const serverUrl = `rtmp://127.0.0.1:${port}/live`;
  const [copied, setCopied] = useState<string | null>(null);

  const copy = async (text: string, which: string) => {
    await copyText(text);
    setCopied(which);
    setTimeout(() => setCopied(null), 1500);
  };

  const badge = !running
    ? { label: "Engine stopped", cls: "bg-slate-700/60 text-slate-400" }
    : ingest.status === "receiving"
      ? {
          label: `Receiving · ${formatKbps(ingest.bitrateKbps ?? 0)}`,
          cls: "bg-emerald-500/15 text-emerald-300",
        }
      : {
          label: "Waiting for OBS…",
          cls: "bg-amber-500/15 text-amber-300 animate-pulse",
        };

  return (
    <section className="rounded-2xl border border-line bg-panel p-5">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <span
            className={`h-2.5 w-2.5 rounded-full ${
              running && ingest.status === "receiving" ? "bg-emerald-400" : "bg-slate-600"
            }`}
          />
          <h2 className="font-semibold">OBS Source</h2>
        </div>
        <span className={`rounded-full px-2.5 py-0.5 text-[11px] font-semibold ${badge.cls}`}>
          {badge.label}
        </span>
      </div>

      <p className="mb-3 text-xs text-slate-500">
        In OBS: Settings → Stream → Service <span className="text-slate-300">Custom</span>,
        then paste these values and start streaming in OBS after pressing GO LIVE here.
      </p>

      <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <div className="flex items-center gap-2">
          <div className="flex-1">
            <div className="mb-1 text-[10px] uppercase tracking-wide text-slate-500">
              Server
            </div>
            <code className="block truncate rounded-lg border border-line bg-panel-2 px-3 py-2 text-xs text-sky-300">
              {serverUrl}
            </code>
          </div>
          <button className="btn-ghost mt-4" onClick={() => copy(serverUrl, "server")}>
            {copied === "server" ? "Copied!" : "Copy"}
          </button>
        </div>
        <div className="flex items-center gap-2">
          <div className="flex-1">
            <div className="mb-1 text-[10px] uppercase tracking-wide text-slate-500">
              Stream Key
            </div>
            <code className="block truncate rounded-lg border border-line bg-panel-2 px-3 py-2 text-xs text-sky-300">
              {OBS_STREAM_KEY}
            </code>
          </div>
          <button className="btn-ghost mt-4" onClick={() => copy(OBS_STREAM_KEY, "key")}>
            {copied === "key" ? "Copied!" : "Copy"}
          </button>
        </div>
      </div>
    </section>
  );
}

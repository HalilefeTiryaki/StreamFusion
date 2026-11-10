import type { IngestStatus, SystemMetrics } from "../types";
import { formatKbps } from "../format";

export function SystemBar({
  sys,
  ingest,
  totalKbps,
  liveCount,
}: {
  sys: SystemMetrics | null;
  ingest: IngestStatus;
  totalKbps: number;
  liveCount: number;
}) {
  const ingestValue =
    ingest.status === "receiving"
      ? formatKbps(ingest.bitrateKbps ?? 0)
      : ingest.status === "waiting"
        ? "waiting"
        : "offline";

  return (
    <footer className="flex shrink-0 items-center gap-8 border-t border-line bg-panel/80 px-6 py-2.5 backdrop-blur">
      <Metric label="CPU" value={sys ? `${sys.cpuPercent.toFixed(0)}%` : "—"} />
      <Metric
        label="RAM"
        value={
          sys
            ? `${(sys.memUsedMb / 1024).toFixed(1)} / ${(sys.memTotalMb / 1024).toFixed(1)} GB`
            : "—"
        }
      />
      <Metric label="Ingest" value={ingestValue} />
      <Metric label="Total Out" value={formatKbps(totalKbps)} />
      <Metric
        label="Live"
        value={`${liveCount} platform${liveCount === 1 ? "" : "s"}`}
        accent={liveCount > 0}
      />
    </footer>
  );
}

function Metric({
  label,
  value,
  accent,
}: {
  label: string;
  value: string;
  accent?: boolean;
}) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-[10px] uppercase tracking-wider text-slate-500">{label}</span>
      <span
        className={`text-sm font-semibold tabular-nums ${
          accent ? "text-emerald-300" : "text-slate-200"
        }`}
      >
        {value}
      </span>
    </div>
  );
}
